//! The reader's text widget: lays out a whole chapter of rich-text blocks and adds what
//! iced's `rich_text` lacks — drag/word/verse selection across paragraphs, copy with a
//! citation, link activation with hover feedback, and the right mouse cursors.
//!
//! One widget per chapter (not one per paragraph) so a single selection can span
//! blocks. Layout and hit-testing reuse iced's cosmic-text paragraphs; selection
//! rectangles come from cosmic-text's `LayoutRun::highlight`.

use iced::advanced::graphics::text::{Paragraph as Para, cosmic_text};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{
    Alignment, Difference, Hit, LineHeight, Paragraph as _, Renderer as _, Shaping, Span,
    Text, Wrapping,
};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, Widget, clipboard, mouse};
use iced::{
    Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Theme, Vector, alignment,
    keyboard, window,
};
use crate::reader_cards::{Slot, assign_lanes, place_column};
use std::collections::{HashMap, BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MULTI_CLICK: Duration = Duration::from_millis(400);
const DRAG_THRESHOLD: f32 = 3.0;

/// What a span means for selection and copying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanMeta {
    /// Included when the selection is copied (verse numbers, footnote markers: no).
    pub copy: bool,
    pub verse: Option<u16>,
    /// A verse number: tinted while hovered, and clicking it selects that verse's text
    /// (Shift-click extends the selection to it).
    pub number: bool,
    /// Underlined with dots, all the time (a Strong's word).
    pub dotted: bool,
}

/// One laid-out paragraph or line of the chapter.
#[derive(Clone)]
pub struct Block<L> {
    pub spans: Vec<Span<'static, L>>,
    /// Parallel to `spans`.
    pub meta: Vec<SpanMeta>,
    pub indent: f32,
    pub gap_before: f32,
    /// When copied, separate from the previous block with one newline instead of a blank line.
    pub soft_break: bool,
}

impl<L> Block<L> {
    fn len(&self) -> usize {
        self.spans.iter().map(|s| s.text.len()).sum()
    }

    fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_ref()).collect()
    }

    /// Byte range of span `index` within the block text.
    fn span_range(&self, index: usize) -> (usize, usize) {
        let start: usize = self.spans[..index].iter().map(|s| s.text.len()).sum();
        (start, start + self.spans[index].text.len())
    }

    /// Index of the span containing `offset` (the last span if `offset` is at the end).
    fn span_at(&self, offset: usize) -> Option<usize> {
        let mut start = 0;
        for (i, s) in self.spans.iter().enumerate() {
            let end = start + s.text.len();
            if offset < end {
                return Some(i);
            }
            start = end;
        }
        self.spans.len().checked_sub(1)
    }
}

/// `— John 3:16-17 (KJV)` is built from these plus the verses actually copied.
#[derive(Clone, Debug)]
pub struct Citation {
    /// "John 3"
    pub chapter: String,
    /// "KJV"
    pub module: String,
}

// ---------------------------------------------------------------------------
// Canonical verse text (what marks anchor to)
// ---------------------------------------------------------------------------

/// A byte range within one verse's canonical text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanonSlice {
    pub verse: u16,
    pub start: usize,
    pub end: usize,
}

/// Reports a clicked link, where it sits in the text, and the marks under the click (a Strong's
/// word inside a highlight is both: the app offers the two).
type LinkHandler<'a, L, Message> = Box<dyn Fn(L, Option<CanonSlice>, Vec<usize>) -> Message + 'a>;

/// A margin card: an element shown in the margin beside its mark. (When the reader is too
/// narrow for a margin the cards are not shown; the app floats one under a clicked mark.)
pub struct CardSpec<'a, Message> {
    /// Identifies the card (the mark's id); its widget state is kept under this key.
    pub key: u64,
    /// Where the mark is; the card is placed level with the first line of this.
    pub anchor: CanonSlice,
    pub color: Color,
    /// Keep level with the mark if at all possible (the card being edited).
    pub pinned: bool,
    /// The card belongs to the link at its anchor (a footnote marker, a Strong's word):
    /// hovering that link emphasises the card, and hovering the card lights up the link.
    pub follows_link: bool,
    pub element: CardElement<'a, Message>,
}

/// A card's element, made only when it is wanted: of a crowd of notes only the one on top of
/// each stack is shown, so the rest are never built, laid out or drawn.
pub struct CardElement<'a, Message> {
    make: Box<dyn Fn() -> Element<'a, Message> + 'a>,
    built: Option<Element<'a, Message>>,
}

impl<'a, Message> CardElement<'a, Message> {
    pub fn new(make: impl Fn() -> Element<'a, Message> + 'a) -> Self {
        CardElement { make: Box::new(make), built: None }
    }

    fn build(&mut self) -> &mut Element<'a, Message> {
        let make = &self.make;
        self.built.get_or_insert_with(make)
    }
}

/// Space between the text column and the card column.
const CARD_GAP: f32 = 28.0;
/// The same with margins on both sides: wider, so the cards do not crowd a text that has no
/// room of its own to spare.
const BOTH_GAP: f32 = 64.0;
/// With margins on both sides, how far the bar at the end of a connector stands from the text.
const HANDLE_OFFSET: f32 = 14.0;
/// Whether the margin is drawn as a faintly different panel (off to see how it looks without).
const SHOW_MARGIN_PANEL: bool = false;
/// Narrowest and widest a card in the column may be.
const CARD_MIN: f32 = 220.0;
const CARD_MAX: f32 = 560.0;
/// The share of the reader's width the margin takes (2 parts in 7), within those limits.
const MARGIN_SHARE: f32 = 2.0 / 7.0;
/// Corner radius of the margin's panel.
const MARGIN_RADIUS: f32 = 10.0;
/// How far below the first note of a stack the marks of the others may be: about three lines of
/// text, whatever the notes' own heights (which would regroup them as other ones came up).
const STACK_REACH: f32 = 96.0;
/// How far what is not pointed at fades toward the background (0 is not at all).
const FADE: f32 = 0.45;
/// How opaque the veil over a card that is not pointed at is.
const VEIL: f32 = 0.3;

/// `color` faded toward `backdrop`, opaque (so connectors that cross do not blend).
fn faded(color: Color, backdrop: Color) -> Color {
    let mix = |a: f32, b: f32| a * (1.0 - FADE) + b * FADE;
    Color::from_rgb(mix(color.r, backdrop.r), mix(color.g, backdrop.g), mix(color.b, backdrop.b))
}

/// Notes on one line are layered into a stack: the one on top shows whole, the others peek out
/// behind it, and a bar of arrows under it cycles through them.
const STACK_BAR: f32 = 26.0;
/// How far each layer behind the top card shows below it, and how many are drawn.
const LAYER_PEEK: f32 = 3.0;
const LAYERS_SHOWN: usize = 2;
/// Space between stacked cards in the margin.
const STACK_GAP: f32 = 8.0;
/// Lanes for the connectors across the gap, and the distance between them. Lane 0 is the middle
/// of the gap, as a lone connector always had; busier ones fan out either side of it.
const LANES: usize = 4;
const LANE_STEP: f32 = 4.0;

/// Horizontal offset of a connector's vertical stretch from the middle of the gap: 0, +1, -1,
/// +2, -2... steps for lanes 0, 1, 2, 3, 4...
fn lane_offset(lane: usize) -> f32 {
    let steps = lane.div_ceil(2) as f32;
    if lane % 2 == 1 { steps * LANE_STEP } else { -steps * LANE_STEP }
}

/// What a floating element is anchored to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatKind {
    /// Under the settled text selection (the pen toolbar).
    UnderSelection,
    /// Under the line of the last click on a mark (the chooser for overlapping marks).
    UnderClick,
}

/// Ids of the marks whose painted region contains the character at `pos`.
fn marks_at(pieces: &BTreeMap<u16, Vec<Piece>>, marks: &[MarkDraw], pos: Pos) -> Vec<usize> {
    let mut out = Vec::new();
    for m in marks {
        let hit = block_ranges(pieces, m.slice)
            .iter()
            .any(|&(b, s, e)| b == pos.block && s <= pos.offset && pos.offset < e);
        if hit && !out.contains(&m.mark) {
            out.push(m.mark);
        }
    }
    out
}

/// The selected text as canonical slices, one per verse touched; `None` if nothing is selected.
pub type TextSelection = Option<Vec<CanonSlice>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Highlight,
    Underline,
    /// An underline of dashes.
    Dashed,
    /// An underline that waves.
    Wavy,
    /// A highlight of diagonal stripes.
    Striped,
    /// A faint line, for a note anchored to text without a highlight.
    Note,
}

/// A mark to paint: where (canonical coordinates), how, and in what colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkDraw {
    /// Caller's identifier for the mark (reported back when it is clicked).
    pub mark: usize,
    pub slice: CanonSlice,
    pub kind: MarkKind,
    pub color: Color,
}

/// Where a stretch of a verse's canonical text sits in the blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Piece {
    block: usize,
    /// Byte offset within the block text.
    start: usize,
    len: usize,
    /// Byte offset within the verse's canonical text.
    canon: usize,
}

/// Locate every verse's canonical text in the blocks. Mirrors `Chapter::canonical_verses`:
/// copyable spans of a run (consecutive spans of one verse) concatenate, and separate
/// runs of the same verse are joined by one space.
fn canonical_pieces<L>(blocks: &[Block<L>]) -> BTreeMap<u16, Vec<Piece>> {
    let mut pieces: BTreeMap<u16, Vec<Piece>> = BTreeMap::new();
    let mut canon_len: BTreeMap<u16, usize> = BTreeMap::new();
    for (b, block) in blocks.iter().enumerate() {
        let mut at = 0;
        let mut group: Option<u16> = None;
        let mut group_has_text = false;
        for (span, meta) in block.spans.iter().zip(&block.meta) {
            if meta.verse != group {
                group = meta.verse;
                group_has_text = false;
            }
            if let (Some(verse), true) = (meta.verse, meta.copy && !span.text.is_empty()) {
                let len = canon_len.entry(verse).or_insert(0);
                if !group_has_text && *len > 0 {
                    *len += 1; // separator between runs
                }
                group_has_text = true;
                pieces.entry(verse).or_default().push(Piece {
                    block: b,
                    start: at,
                    len: span.text.len(),
                    canon: *len,
                });
                *len += span.text.len();
            }
            at += span.text.len();
        }
    }
    pieces
}

/// Block ranges `(block, start, end)` covered by a canonical slice.
fn block_ranges(pieces: &BTreeMap<u16, Vec<Piece>>, slice: CanonSlice) -> Vec<(usize, usize, usize)> {
    let Some(list) = pieces.get(&slice.verse) else { return Vec::new() };
    list.iter()
        .filter_map(|p| {
            let (lo, hi) = (slice.start.max(p.canon), slice.end.min(p.canon + p.len));
            (lo < hi).then(|| (p.block, p.start + (lo - p.canon), p.start + (hi - p.canon)))
        })
        .collect()
}

impl<L, Message> ReaderText<'_, L, Message> {
    /// Where the links are, as places in the canonical text.
    #[cfg(test)]
    pub(crate) fn link_places(&self) -> std::collections::HashSet<CanonSlice> {
        self.link_spans(&canonical_pieces(&self.blocks)).into_keys().collect()
    }

    /// Every link span (block, span) by where it sits in the canonical text (the first, where
    /// several share a place), for cards to find their link without a search each.
    fn link_spans(&self, pieces: &BTreeMap<u16, Vec<Piece>>) -> HashMap<CanonSlice, (usize, usize)> {
        let mut out = HashMap::new();
        for (b, block) in self.blocks.iter().enumerate() {
            for j in 0..block.spans.len() {
                if block.spans[j].link.is_some() && block.meta[j].verse.is_some() && !block.meta[j].number
                    && let Some(slice) = link_slice(pieces, block, b, j)
                {
                    out.entry(slice).or_insert((b, j));
                }
            }
        }
        out
    }

    /// The link span (block, span) sitting at `anchor` in the canonical text, if any.
    #[cfg(test)]
    fn link_span_at(&self, pieces: &BTreeMap<u16, Vec<Piece>>, anchor: CanonSlice) -> Option<(usize, usize)> {
        for (b, block) in self.blocks.iter().enumerate() {
            for j in 0..block.spans.len() {
                if block.spans[j].link.is_some()
                    && block.meta[j].verse == Some(anchor.verse)
                    && !block.meta[j].number
                    && link_slice(pieces, block, b, j) == Some(anchor)
                {
                    return Some((b, j));
                }
            }
        }
        None
    }
}

/// Where link span `j` of `block` sits in its verse's canonical text, so a card can sit level
/// with it. A word is its own text; a marker (a footnote's dagger) has none, so it is the
/// character just before it, or the first one after it when it opens the block.
fn link_slice<L>(
    pieces: &BTreeMap<u16, Vec<Piece>>,
    block: &Block<L>,
    b: usize,
    j: usize,
) -> Option<CanonSlice> {
    let meta = block.meta[j];
    let verse = meta.verse?;
    let list = pieces.get(&verse)?;
    let text: String = block.spans.iter().map(|s| s.text.as_ref()).collect();
    let off: usize = block.spans[..j].iter().map(|s| s.text.len()).sum();
    let end = off + block.spans[j].text.len();
    if meta.copy {
        let p = list.iter().find(|p| p.block == b && p.start <= off && off < p.start + p.len)?;
        let hi = end.min(p.start + p.len);
        return Some(CanonSlice { verse, start: p.canon + (off - p.start), end: p.canon + (hi - p.start) });
    }
    let before = list.iter().filter(|p| p.block == b && p.len > 0 && p.start + p.len <= off).max_by_key(|p| p.start + p.len);
    if let Some(p) = before {
        let last = text[..p.start + p.len].char_indices().next_back()?.0.max(p.start);
        return Some(CanonSlice { verse, start: p.canon + (last - p.start), end: p.canon + p.len });
    }
    let p = list.iter().filter(|p| p.block == b && p.len > 0).min_by_key(|p| p.start)?;
    let first = text[p.start..].chars().next()?.len_utf8();
    Some(CanonSlice { verse, start: p.canon, end: p.canon + first })
}

/// The canonical text of the selection, as one slice per verse touched.
fn selection_slices<L>(
    pieces: &BTreeMap<u16, Vec<Piece>>,
    blocks: &[Block<L>],
    sel: Selection,
) -> Vec<CanonSlice> {
    let mut out: BTreeMap<u16, (usize, usize)> = BTreeMap::new();
    for (&verse, list) in pieces {
        for p in list {
            let Some((s, e)) = range_in_block(sel, p.block, blocks[p.block].len()) else { continue };
            let (lo, hi) = (s.max(p.start), e.min(p.start + p.len));
            if lo < hi {
                let (c0, c1) = (p.canon + (lo - p.start), p.canon + (hi - p.start));
                let entry = out.entry(verse).or_insert((c0, c1));
                *entry = (entry.0.min(c0), entry.1.max(c1));
            }
        }
    }
    out.into_iter().map(|(verse, (start, end))| CanonSlice { verse, start, end }).collect()
}

/// Canonical text rebuilt from the pieces; used to check it matches the core's definition.
#[cfg(test)]
pub(crate) fn canonical_text<L>(blocks: &[Block<L>]) -> BTreeMap<u16, String> {
    let texts: Vec<String> = blocks.iter().map(Block::text).collect();
    canonical_pieces(blocks)
        .into_iter()
        .map(|(verse, list)| {
            let mut out = String::new();
            for p in list {
                if p.canon > out.len() {
                    out.push(' ');
                }
                out.push_str(&texts[p.block][p.start..p.start + p.len]);
            }
            (verse, out)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Selection model (pure; tested below)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    block: usize,
    offset: usize,
}

type Selection = (Pos, Pos);

fn ordered((a, b): Selection) -> Selection {
    if a <= b { (a, b) } else { (b, a) }
}

/// The part of block `b` (of length `len`) covered by the selection.
fn range_in_block(sel: Selection, b: usize, len: usize) -> Option<(usize, usize)> {
    let (start, end) = ordered(sel);
    if b < start.block || b > end.block {
        return None;
    }
    let s = if b == start.block { start.offset } else { 0 };
    let e = if b == end.block { end.offset.min(len) } else { len };
    (s < e).then_some((s, e))
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}'
}

/// Byte range of the word (or single other character) at `offset`.
fn word_bounds(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let at = text[offset..].chars().next();
    let before = text[..offset].chars().next_back();
    // Prefer the character under the pointer; fall back to the one before it.
    let (anchor, c) = match (at, before) {
        (Some(c), _) if is_word_char(c) => (offset, c),
        (_, Some(c)) if is_word_char(c) => (offset - c.len_utf8(), c),
        (Some(c), _) => return (offset, offset + c.len_utf8()),
        _ => return (offset, offset),
    };
    debug_assert!(is_word_char(c));
    let start = text[..anchor]
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_word_char(*ch))
        .last()
        .map_or(anchor, |(i, _)| i);
    let end = text[anchor..]
        .char_indices()
        .find(|(_, ch)| !is_word_char(*ch))
        .map_or(text.len(), |(i, _)| anchor + i);
    (start, end)
}

/// The selection for a drag in progress.
/// - single click: from the press point to the pointer;
/// - double-click: whole words, from the word first clicked to the word under the pointer;
/// - triple-click: whole verses, likewise.
///
/// `anchor_sel` is the selection the multi-click itself made.
fn extended_selection<L>(
    blocks: &[Block<L>],
    count: u8,
    anchor: Pos,
    anchor_sel: Option<Selection>,
    focus: Pos,
) -> Option<Selection> {
    let unit = match count {
        2 => {
            let (s, e) = word_bounds(&blocks[focus.block].text(), focus.offset);
            (s < e).then_some((Pos { block: focus.block, offset: s }, Pos { block: focus.block, offset: e }))
        }
        3 => verse_at(&blocks[focus.block], focus.offset).and_then(|v| verse_selection(blocks, v)),
        _ => return (anchor != focus).then_some((anchor, focus)),
    };
    match (anchor_sel.map(ordered), unit.map(ordered)) {
        (Some((a0, a1)), Some((u0, u1))) => Some((a0.min(u0), a1.max(u1))),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The selection after clicking the number of `verse`: that verse's text; with Shift, the
/// current selection grown to include it; clicking the only selected verse again clears it.
fn verse_number_selection<L>(
    blocks: &[Block<L>],
    verse: u16,
    shift: bool,
    current: Option<Selection>,
) -> Option<Selection> {
    let target = ordered(verse_selection(blocks, verse)?);
    match current.map(ordered) {
        Some(cur) if shift => Some((cur.0.min(target.0), cur.1.max(target.1))),
        Some(cur) if cur == target => None,
        _ => Some(target),
    }
}

/// Selection covering every span of `verse`, across blocks.
fn verse_selection<L>(blocks: &[Block<L>], verse: u16) -> Option<Selection> {
    let mut first = None;
    let mut last = None;
    for (b, block) in blocks.iter().enumerate() {
        for (j, m) in block.meta.iter().enumerate() {
            if m.verse == Some(verse) {
                let (s, e) = block.span_range(j);
                first.get_or_insert(Pos { block: b, offset: s });
                last = Some(Pos { block: b, offset: e });
            }
        }
    }
    Some((first?, last?))
}

/// Verse at `offset` in block `b`, looking at neighbouring spans if that one has none.
fn verse_at<L>(block: &Block<L>, offset: usize) -> Option<u16> {
    let i = block.span_at(offset)?;
    block.meta[i].verse.or_else(|| {
        block.meta[i..].iter().chain(block.meta[..i].iter().rev()).find_map(|m| m.verse)
    })
}

/// Plain text of the selection plus the first/last verse it touches.
fn selected_text<L>(
    blocks: &[Block<L>],
    sel: Selection,
    citation: Option<&Citation>,
) -> Option<String> {
    let mut out = String::new();
    let mut verses: Option<(u16, u16)> = None;
    for (b, block) in blocks.iter().enumerate() {
        let Some((s, e)) = range_in_block(sel, b, block.len()) else { continue };
        let mut piece = String::new();
        let mut start = 0;
        for (span, meta) in block.spans.iter().zip(&block.meta) {
            let end = start + span.text.len();
            let (lo, hi) = (s.max(start), e.min(end));
            if lo < hi
                && meta.copy
                && let Some(t) = span.text.get(lo - start..hi - start)
            {
                piece.push_str(t);
                if let Some(v) = meta.verse {
                    verses = Some(verses.map_or((v, v), |(a, z)| (a.min(v), z.max(v))));
                }
            }
            start = end;
        }
        let piece = piece.split_whitespace().collect::<Vec<_>>().join(" ");
        if piece.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str(if block.soft_break { "\n" } else { "\n\n" });
        }
        out.push_str(&piece);
    }
    if out.is_empty() {
        return None;
    }
    if let (Some(c), Some((a, z))) = (citation, verses) {
        let range = if a == z { format!("{a}") } else { format!("{a}-{z}") };
        out.push_str(&format!("\n— {}:{range} ({})", c.chapter, c.module));
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

pub struct ReaderText<'a, L, Message> {
    /// Identifies the chapter; selection resets when it changes.
    key: u64,
    blocks: Vec<Block<L>>,
    size: Pixels,
    line_height: LineHeight,
    selection_color: Color,
    hover_color: Color,
    citation: Option<Citation>,
    /// Called with the link and where it sits in the verse's canonical text, so the app can
    /// put a card level with it.
    on_link: Option<LinkHandler<'a, L, Message>>,
    on_scroll: Option<Box<dyn Fn(f32) -> Message + 'a>>,
    reveal: Option<(u64, u16)>,
    select: Option<(u64, u16, u16)>,
    select_words: Option<(u64, CanonSlice)>,
    restore: Option<Vec<CanonSlice>>,
    marks: Vec<MarkDraw>,
    on_selection: Option<Box<dyn Fn(TextSelection) -> Message + 'a>>,
    clear_token: u64,
    /// Shown under the selection once it settles; hosted (laid out, drawn, fed events) here
    /// so it can follow the selection and always sees clicks before the text does.
    toolbar: Option<Element<'a, Message>>,
    toolbar_kind: FloatKind,
    cards: Vec<CardSpec<'a, Message>>,
    text_width: Option<f32>,
    both_sides: bool,
    dim_cards: bool,
    on_layout: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    /// A link pressed and then dragged away: the link, and where the pointer is (in the window).
    on_link_drag: Option<Box<dyn Fn(L, Point) -> Message + 'a>>,
    /// A right click on a link: the link, and where the pointer is (in the window).
    on_link_menu: Option<Box<dyn Fn(L, Point) -> Message + 'a>>,
    /// A left press in the text (not on a card): lets the app put away its pop-ups elsewhere.
    on_press: Option<Box<dyn Fn() -> Message + 'a>>,
    margin_colors: (Color, Color),
    on_marks: Option<Box<dyn Fn(Vec<usize>) -> Message + 'a>>,
}

impl<'a, L, Message> ReaderText<'a, L, Message> {
    pub fn new(key: u64, blocks: Vec<Block<L>>) -> Self {
        Self {
            key,
            blocks,
            size: Pixels(19.0),
            line_height: LineHeight::Relative(1.65),
            selection_color: Color::from_rgba(0.3, 0.5, 0.9, 0.35),
            hover_color: Color::from_rgb(0.3, 0.5, 0.9),
            citation: None,
            on_link: None,
            on_scroll: None,
            reveal: None,
            select: None,
            select_words: None,
            restore: None,
            marks: Vec::new(),
            on_selection: None,
            clear_token: 0,
            toolbar: None,
            toolbar_kind: FloatKind::UnderSelection,
            cards: Vec::new(),
            text_width: None,
            both_sides: false,
            dim_cards: false,
            on_layout: None,
            on_press: None,
            on_link_menu: None,
            on_link_drag: None,
            margin_colors: (Color::TRANSPARENT, Color::TRANSPARENT),
            on_marks: None,
        }
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    pub fn selection_color(mut self, color: Color) -> Self {
        self.selection_color = color;
        self
    }

    pub fn hover_color(mut self, color: Color) -> Self {
        self.hover_color = color;
        self
    }

    pub fn citation(mut self, citation: Citation) -> Self {
        self.citation = Some(citation);
        self
    }

    /// Called when a link is clicked.
    pub fn on_link(mut self, f: impl Fn(L, Option<CanonSlice>, Vec<usize>) -> Message + 'a) -> Self {
        self.on_link = Some(Box::new(f));
        self
    }

    /// Called every frame while a drag-selection is held past the top or bottom edge,
    /// with the number of pixels to scroll (negative = up).
    pub fn on_scroll(mut self, f: impl Fn(f32) -> Message + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }

    /// What was selected when the widget was last made (it is made again whenever its tab is shown
    /// again): put back the first time the widget is drawn, if nothing is selected yet.
    pub fn restore_selection(mut self, slices: Option<Vec<CanonSlice>>) -> Self {
        self.restore = slices;
        self
    }

    /// Select from the first to the last of the words in this verse's canonical text (a slice
    /// of it). Each distinct `token` is served once.
    pub fn select_words(mut self, request: Option<(u64, CanonSlice)>) -> Self {
        self.select_words = request;
        self
    }

    /// Select the text of verses `first..=last`. Each distinct `token` is served once.
    pub fn select_verses(mut self, request: Option<(u64, u16, u16)>) -> Self {
        self.select = request;
        self
    }

    /// Marks to paint over the text, in canonical coordinates.
    pub fn marks(mut self, marks: Vec<MarkDraw>) -> Self {
        self.marks = marks;
        self
    }

    /// Called when the text selection settles (not during a drag) with the selected text
    /// as slices of each verse's canonical text, or `None` when nothing is selected.
    pub fn on_selection(
        mut self,
        f: impl Fn(TextSelection) -> Message + 'a,
    ) -> Self {
        self.on_selection = Some(Box::new(f));
        self
    }

    /// An element to float under the text selection (or under a clicked mark), while there
    /// is one and no drag is in progress. Pass `None` to hide it.
    pub fn toolbar(mut self, toolbar: Option<(FloatKind, Element<'a, Message>)>) -> Self {
        match toolbar {
            Some((kind, element)) => {
                self.toolbar = Some(element);
                self.toolbar_kind = kind;
            }
            None => self.toolbar = None,
        }
        self
    }

    /// Margin cards, shown in a margin beside the text when the widget is wide enough for one
    /// (see [`Self::text_width`]); ignored otherwise.
    pub fn cards(mut self, cards: Vec<CardSpec<'a, Message>>) -> Self {
        self.cards = cards;
        self
    }

    /// The width of the text column. When the widget is at least this wide plus room for a
    /// margin, the margin appears to the right of the column; otherwise there is no margin and
    /// the text uses the whole width.
    pub fn text_width(mut self, width: Option<f32>) -> Self {
        self.text_width = width;
        self
    }

    /// Put margins on both sides of the text (with room for them): each card goes to the side its
    /// mark is nearer to, the two sides kept roughly even.
    pub fn both_sides(mut self, both: bool) -> Self {
        self.both_sides = both;
        self
    }

    /// Veil every card, and fade its connector, until it (or its mark) is pointed at.
    pub fn dim_cards(mut self, dim: bool) -> Self {
        self.dim_cards = dim;
        self
    }

    /// Called with whether the margin is showing, whenever that changes (and once at first).
    pub fn on_link_drag(mut self, f: impl Fn(L, Point) -> Message + 'a) -> Self {
        self.on_link_drag = Some(Box::new(f));
        self
    }

    pub fn on_link_menu(mut self, f: impl Fn(L, Point) -> Message + 'a) -> Self {
        self.on_link_menu = Some(Box::new(f));
        self
    }

    pub fn on_press(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub fn on_layout(mut self, f: impl Fn(bool) -> Message + 'a) -> Self {
        self.on_layout = Some(Box::new(f));
        self
    }

    /// Background and edge colours of the margin.
    pub fn margin_colors(mut self, background: Color, rule: Color) -> Self {
        self.margin_colors = (background, rule);
        self
    }

    /// Called when text inside marked regions is single-clicked, with the ids of the marks
    /// there (several when they overlap). An empty list means a chooser was dismissed.
    pub fn on_marks(mut self, f: impl Fn(Vec<usize>) -> Message + 'a) -> Self {
        self.on_marks = Some(Box::new(f));
        self
    }

    /// Clear the selection whenever this token changes from the value last seen.
    pub fn clear_selection(mut self, token: u64) -> Self {
        self.clear_token = token;
        self
    }

    /// Ask for `verse` to be scrolled into view. Each distinct `token` is served once, so
    /// the same verse can be requested again by bumping the token. Needs `on_scroll`.
    pub fn reveal(mut self, request: Option<(u64, u16)>) -> Self {
        self.reveal = request;
        self
    }
}

impl<L: Clone + 'static, Message> ReaderText<'_, L, Message> {
    /// The layers peeking out below the top card of a stack, and its bar of arrows.
    #[allow(clippy::too_many_arguments)]
    fn draw_stack_behind(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        viewport: &Rectangle,
        bounds: Rectangle,
        card: Rectangle,
        stack: &StackBar,
        cursor: mouse::Cursor,
        dim: bool,
    ) {
        use iced::advanced::text::{Renderer as _, Text};
        let weak = theme.extended_palette().background.weak.color;
        let muted = theme.extended_palette().background.base.text;
        let n = stack.keys.len();
        for layer in (1..=(n - 1).min(LAYERS_SHOWN)).rev() {
            let inset = 6.0 * layer as f32;
            let rect = Rectangle::new(
                Point::new(card.x + inset, card.y + LAYER_PEEK * layer as f32),
                Size::new((card.width - 2.0 * inset).max(1.0), card.height),
            );
            let mut color = stack.colors[(stack.at + layer) % n];
            if dim {
                // Stepped back, like the rest of what is not pointed at.
                color = faded(color, theme.extended_palette().background.base.color);
            }
            renderer.fill_quad(
                Quad { bounds: rect, border: iced::Border { color, width: 1.0, radius: 6.0.into() }, ..Quad::default() },
                weak,
            );
        }
        let (prev, next) = stack.zones(bounds);
        let bar = Rectangle::new(Point::new(bounds.x + stack.bar.x, bounds.y + stack.bar.y), stack.bar.size());
        if bar.intersection(viewport).is_none() {
            return;
        }
        let font = renderer.default_font();
        let mut label = |content: String, area: Rectangle, size: f32, lit: bool| {
            renderer.fill_text(
                Text {
                    content,
                    bounds: area.size(),
                    size: Pixels(size),
                    line_height: LineHeight::Relative(1.0),
                    font,
                    align_x: Alignment::Center,
                    align_y: alignment::Vertical::Center,
                    shaping: Shaping::Basic,
                    wrapping: Wrapping::None,
                },
                Point::new(area.center_x(), area.center_y()),
                if lit { self.hover_color } else { Color { a: 0.75, ..muted } },
                *viewport,
            );
        };
        label("\u{2039}".into(), prev, 20.0, cursor.is_over(prev));
        label("\u{203A}".into(), next, 20.0, cursor.is_over(next));
        let middle = Rectangle::new(Point::new(prev.x + prev.width, bar.y), Size::new(next.x - prev.x - prev.width, bar.height));
        label(format!("{} / {}", stack.at + 1, n), middle, 12.0, false);
    }

    /// Offset to lay the toolbar out at, and the rectangle it then covers, when it should be
    /// showing: a toolbar was supplied, a selection exists and no drag is in progress.
    fn toolbar_geometry(
        &self,
        state: &State<L>,
        bounds: Rectangle,
        viewport: &Rectangle,
    ) -> Option<(Vector, Rectangle)> {
        self.toolbar.as_ref()?;
        let node = state.toolbar_node.as_ref()?;
        if state.drag.is_some() {
            return None;
        }
        // What is held under a selection is offered while the reader is the one being used.
        if self.toolbar_kind == FloatKind::UnderSelection && !state.focused {
            return None;
        }
        let (first, last) = match self.toolbar_kind {
            FloatKind::UnderSelection => selection_edges(state, &self.blocks, state.selection?)?,
            FloatKind::UnderClick => {
                let line = state.click_anchor?;
                (line, line)
            }
        };
        let visible = Rectangle::new(
            Point::new(viewport.x - bounds.x, viewport.y - bounds.y),
            viewport.size(),
        );
        let local = place_toolbar(first, last, node.size(), visible, bounds.width);
        let origin = Point::new(bounds.x + local.x, bounds.y + local.y);
        Some((origin - node.bounds().position(), Rectangle::new(origin, node.size())))
    }

    fn publish_selection(&self, state: &mut State<L>, shell: &mut Shell<'_, Message>) {
        let Some(f) = &self.on_selection else { return };
        if state.published == Some(state.selection) {
            return;
        }
        state.published = Some(state.selection);
        let slices = state
            .selection
            .map(|sel| selection_slices(&state.pieces, &self.blocks, sel))
            .filter(|s| !s.is_empty());
        shell.publish(f(slices));
    }
}

/// First and last visual line of the selection, in widget-local coordinates.
fn selection_edges<L>(
    state: &State<L>,
    blocks: &[Block<L>],
    sel: Selection,
) -> Option<(Rectangle, Rectangle)> {
    let (start, end) = ordered(sel);
    let mut first: Option<Rectangle> = None;
    let mut last: Option<Rectangle> = None;
    for b in start.block..=end.block {
        let (Some(laid), Some(block)) = (state.laid.get(b), blocks.get(b)) else { continue };
        let Some((s, e)) = range_in_block(sel, b, block.len()) else { continue };
        let (c0, c1) = (cosmic_text::Cursor::new(0, s), cosmic_text::Cursor::new(0, e));
        for run in laid.para.buffer().layout_runs() {
            if let Some((x, w)) = run.highlight(c0, c1) {
                let rect = Rectangle::new(
                    Point::new(laid.x + x, laid.y + run.line_top),
                    Size::new(w, run.line_height),
                );
                first.get_or_insert(rect);
                last = Some(rect);
            }
        }
    }
    Some((first?, last?))
}

/// Top and bottom (widget-local y) of everything belonging to `verse`.
fn verse_extent<L>(state: &State<L>, blocks: &[Block<L>], verse: u16) -> Option<(f32, f32)> {
    let mut extent: Option<(f32, f32)> = None;
    for (laid, block) in state.laid.iter().zip(blocks) {
        for (j, meta) in block.meta.iter().enumerate() {
            if meta.verse != Some(verse) {
                continue;
            }
            for seg in span_segments(&laid.para, j) {
                let (top, bottom) = (laid.y + seg.top, laid.y + seg.top + seg.height);
                let (lo, hi) = extent.unwrap_or((top, bottom));
                extent = Some((lo.min(top), hi.max(bottom)));
            }
        }
    }
    extent
}

/// Where to put a toolbar of `size` for a selection whose first and last lines are `first`
/// and `last` (widget-local), given the `visible` part of the widget: centred under the last
/// line, or above the first line when there is no room below. Always kept inside `visible`
/// vertically and inside `0..width` horizontally.
fn place_toolbar(first: Rectangle, last: Rectangle, size: Size, visible: Rectangle, width: f32) -> Point {
    const GAP: f32 = 8.0;
    let x = (last.center_x() - size.width / 2.0).clamp(0.0, (width - size.width).max(0.0));
    let below = last.y + last.height + GAP;
    let y = if below + size.height <= visible.y + visible.height {
        below
    } else {
        first.y - size.height - GAP
    };
    let lowest = (visible.y + visible.height - size.height).max(visible.y);
    Point::new(x, y.clamp(visible.y, lowest))
}

/// Pixels to scroll this frame for a pointer at content-space `y`, given the visible
/// `viewport`. Zero in the middle; grows the closer to (or farther past) an edge.
fn edge_scroll(y: f32, viewport: &Rectangle) -> f32 {
    const ZONE: f32 = 40.0;
    const MAX_OVERSHOOT: f32 = 100.0;
    const SPEED: f32 = 0.07;
    const MIN_STEP: f32 = 0.5;
    let top = viewport.y + ZONE;
    let bottom = viewport.y + viewport.height - ZONE;
    if y < top {
        -((top - y).min(MAX_OVERSHOOT) * SPEED).max(MIN_STEP)
    } else if y > bottom {
        ((y - bottom).min(MAX_OVERSHOOT) * SPEED).max(MIN_STEP)
    } else {
        0.0
    }
}

/// Whether there is room for a margin, how wide the text is, and how wide a card is.
/// With room, the margin takes about 2/7 of the width (never narrower or wider than a card
/// should be, and never so wide that the text falls below its base column) and the text takes
/// the rest. Without room there is no margin and the text takes everything; the same when there
/// is nothing to put in it.
fn text_layout(width: f32, text_width: Option<f32>, has_cards: bool) -> (bool, f32, f32) {
    let Some(base) = text_width.filter(|tw| has_cards && width >= tw + CARD_GAP + CARD_MIN) else {
        return (false, width, 0.0);
    };
    let card = (width * MARGIN_SHARE).clamp(CARD_MIN, CARD_MAX).min(width - base - CARD_GAP);
    (true, width - CARD_GAP - card, card)
}

/// Margins on both sides of the text, centred: the width of the left margin (its panel, the gap
/// and the cards in it), of the text, and of a card, when there is room for the text's base column
/// and a card of least width on each side; else none.
pub fn text_layout_both(width: f32, text_width: Option<f32>) -> Option<(f32, f32, f32)> {
    let base = text_width.filter(|tw| width >= tw + 2.0 * (BOTH_GAP + CARD_MIN))?;
    let card = (width * MARGIN_SHARE * 0.7).clamp(CARD_MIN, CARD_MAX).min((width - base) / 2.0 - BOTH_GAP);
    Some((card + BOTH_GAP, width - 2.0 * (card + BOTH_GAP), card))
}

/// Which side of the text each card goes on (true: left), given where its mark's first line is: the
/// side the mark is nearer to, `middle` being the middle of the text. A side is not let to take
/// much more than its share, though: the marks nearest the middle go to the other side instead.
fn sides_by_nearness(lines: &[Option<Rectangle>], middle: f32) -> Vec<bool> {
    // How far left of the middle each mark is (negative: right of it).
    let offset: Vec<Option<f32>> = lines.iter().map(|l| l.map(|r| middle - (r.x + r.width / 2.0))).collect();
    let mut left: Vec<bool> = offset.iter().map(|o| o.is_some_and(|o| o > 0.0)).collect();
    let total = offset.iter().flatten().count();
    let most = (total * 3).div_ceil(5).max(1);
    for side in [true, false] {
        let mut crowd: Vec<usize> = (0..left.len()).filter(|&i| offset[i].is_some() && left[i] == side).collect();
        if crowd.len() <= most {
            continue;
        }
        crowd.sort_by(|&a, &b| offset[a].unwrap_or(0.0).abs().total_cmp(&offset[b].unwrap_or(0.0).abs()));
        for &i in crowd.iter().take(crowd.len() - most) {
            left[i] = !side;
        }
    }
    left
}

/// Where on its card (from the top) the connector arrives.
const CONNECTOR_CARD_OFFSET: f32 = 18.0;

/// A colour with less saturation (halfway to its own grey) and a light touch of opacity: the
/// hover highlight under an underline or note.
/// Paint one line of a mark: `line` is the stretch of the line it marks (as tall as the line), and `under` how far down
/// from its top the underlines go (below the text and the dots of a Strong's word). Every corner is rounded, for a softer,
/// more hand-made look.
fn paint_mark(renderer: &mut iced::Renderer, kind: MarkKind, color: Color, line: Rectangle, under: f32) {
    let (x, top, w, height) = (line.x, line.y, line.width, line.height);
    // A rounded bar `len` long and `thick` thick, `dx` along the stretch and `dy` down from the top of the line.
    let bar = |renderer: &mut iced::Renderer, dx: f32, dy: f32, len: f32, thick: f32| {
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(Point::new(x + dx, top + dy), Size::new(len, thick)),
                border: iced::Border { radius: (thick / 2.0).into(), ..iced::Border::default() },
                ..Quad::default()
            },
            color,
        );
    };
    match kind {
        MarkKind::Highlight => renderer.fill_quad(
            Quad { bounds: line, border: iced::Border { radius: MARK_RADIUS.into(), ..iced::Border::default() }, ..Quad::default() },
            color,
        ),
        MarkKind::Underline => bar(renderer, 0.0, under, w, 2.0),
        MarkKind::Note => bar(renderer, 0.0, under, w, 1.0),
        MarkKind::Dashed => {
            const DASH: f32 = 6.0;
            const GAP: f32 = 3.5;
            let mut at = 0.0;
            while at < w {
                bar(renderer, at, under, DASH.min(w - at), 2.0);
                at += DASH + GAP;
            }
        }
        MarkKind::Wavy => {
            // A wave of round steps, up and down by under two pixels over a period of eight.
            const WAVE: [f32; 8] = [0.0, 0.7, 1.4, 0.7, 0.0, -0.7, -1.4, -0.7];
            let mut at = 0.0;
            let mut step = 0;
            while at < w {
                bar(renderer, at, under - 0.3 + WAVE[step % 8], 2.4f32.min(w - at + 0.4), 1.8);
                at += 2.0;
                step += 1;
            }
        }
        MarkKind::Striped => {
            // Diagonal stripes of the color, each as wide as the gap between them: the stripes are an SVG pattern in a
            // rounded rectangle, one picture for the stretch (the renderer keeps what it has drawn).
            let (pw, ph) = (w.ceil().max(1.0) as u32, height.ceil().max(1.0) as u32);
            let hex = format!("#{:02X}{:02X}{:02X}", (color.r * 255.0).round() as u8, (color.g * 255.0).round() as u8, (color.b * 255.0).round() as u8);
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{pw}\" height=\"{ph}\" viewBox=\"0 0 {pw} {ph}\"><defs>\
                 <pattern id=\"s\" width=\"{STRIPE_PERIOD}\" height=\"{STRIPE_PERIOD}\" patternUnits=\"userSpaceOnUse\" patternTransform=\"rotate(45)\">\
                 <rect width=\"{}\" height=\"{STRIPE_PERIOD}\" fill=\"{hex}\"/></pattern></defs>\
                 <rect width=\"{pw}\" height=\"{ph}\" rx=\"{MARK_RADIUS}\" fill=\"url(#s)\"/></svg>",
                STRIPE_COLORED
            );
            use iced::advanced::svg::Renderer as _;
            // (Opaque stripes drawn at the color's strength: translucent pixels inside the picture come out darker than they should.)
            let picture = iced::advanced::svg::Svg::new(iced::advanced::svg::Handle::from_memory(svg.into_bytes())).opacity(color.a);
            renderer.draw_svg(picture, line, line);
        }
    }
}

/// How far under the baseline (as a part of the text size) a mark's underline is: under the dots of a Strong's word, which
/// are at 0.12, so the two do not run into each other.
const UNDERLINE_DROP: f32 = 0.3;

/// The corner radius of a highlight, in pixels.
const MARK_RADIUS: f32 = 4.0;
/// How far along a striped highlight goes from one stripe to the next, and how much of that is color (the rest is clear).
const STRIPE_PERIOD: f32 = 14.0;
const STRIPE_COLORED: f32 = 9.0;

fn softened(color: Color) -> Color {
    let grey = 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;
    let mix = |c: f32| (c + grey) / 2.0;
    Color { r: mix(color.r), g: mix(color.g), b: mix(color.b), a: 0.3 }
}

/// A card's laid-out size and place (widget-local).
#[derive(Clone)]
struct CardLaid {
    node: layout::Node,
    rect: Rectangle,
    /// The first line of the marked text, in column mode (for the connector).
    anchor_line: Option<Rectangle>,
    /// Which lane the connector's vertical stretch runs in (see `assign_lanes`).
    lane: usize,
    color: Color,
    /// Under the top card of a stack: not shown, nor reachable.
    hidden: bool,
    /// On the top card of a stack: the stack's bar.
    stack: Option<StackBar>,
}

/// The arrows under the top card of a stack of notes.
#[derive(Clone)]
struct StackBar {
    /// Identifies the stack (the lowest key in it), the keys in it in order, and where the top
    /// one is among them.
    id: u64,
    keys: Vec<u64>,
    colors: Vec<Color>,
    at: usize,
    /// Widget-local.
    bar: Rectangle,
}

impl StackBar {
    /// The two places to press, previous and next (absolute, for a widget at `bounds`).
    fn zones(&self, bounds: Rectangle) -> (Rectangle, Rectangle) {
        let bar = Rectangle::new(Point::new(bounds.x + self.bar.x, bounds.y + self.bar.y), self.bar.size());
        // Either side of the counter, close up to it however many digits it has.
        let half = format!("{} / {}", self.at + 1, self.keys.len()).chars().count() as f32 * 3.3;
        let (w, mid) = (26.0, bar.x + bar.width / 2.0);
        (
            Rectangle::new(Point::new(mid - half - 4.0 - w, bar.y), Size::new(w, bar.height)),
            Rectangle::new(Point::new(mid + half + 4.0, bar.y), Size::new(w, bar.height)),
        )
    }

    /// The key of the card a step back or forward goes to.
    fn step(&self, forward: bool) -> u64 {
        let n = self.keys.len();
        self.keys[if forward { (self.at + 1) % n } else { (self.at + n - 1) % n }]
    }
}

impl CardLaid {
    /// Whether the pointer is on this card, or on what it is about (for a stack, on any of its
    /// notes' marks): the card is then drawn lit.
    fn is_hot(&self, key: u64, emphasised: &BTreeSet<usize>) -> bool {
        emphasised.contains(&(key as usize))
            || self.stack.as_ref().is_some_and(|s| s.keys.iter().any(|k| emphasised.contains(&(*k as usize))))
    }

    /// Offset that places the card's node at its rectangle, for a widget at `bounds`.
    fn offset(&self, bounds: Rectangle) -> Vector {
        Point::new(bounds.x + self.rect.x, bounds.y + self.rect.y) - self.node.bounds().position()
    }

    fn absolute(&self, bounds: Rectangle) -> Rectangle {
        Rectangle::new(Point::new(bounds.x + self.rect.x, bounds.y + self.rect.y), self.rect.size())
    }
}

/// The first visual line of `slice`, widget-local.
fn slice_first_line<L>(state: &State<L>, slice: CanonSlice) -> Option<Rectangle> {
    for (b, s, e) in block_ranges(&state.pieces, slice) {
        let laid = state.laid.get(b)?;
        let (c0, c1) = (cosmic_text::Cursor::new(0, s), cosmic_text::Cursor::new(0, e));
        for run in laid.para.buffer().layout_runs() {
            if let Some((x, w)) = run.highlight(c0, c1) {
                return Some(Rectangle::new(
                    Point::new(laid.x + x, laid.y + run.line_top),
                    Size::new(w, run.line_height),
                ));
            }
        }
    }
    None
}

struct Laid<L> {
    /// Spans the paragraph was shaped from (for change detection only).
    spans: Vec<Span<'static, L>>,
    para: Para,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

struct Drag {
    origin: Point,
    anchor: Pos,
    /// What a double/triple-click selected; the drag extends from it.
    anchor_sel: Option<Selection>,
    /// The selection before this press cleared it (a verse-number click may toggle or extend it).
    previous: Option<Selection>,
    count: u8,
    moved: bool,
    pressed_link: Option<(usize, usize)>,
}

struct Click {
    at: Instant,
    point: Point,
    count: u8,
}

struct State<L> {
    key: u64,
    laid: Vec<Laid<L>>,
    selection: Option<Selection>,
    drag: Option<Drag>,
    /// (block, span) of the link under the pointer.
    hovered: Option<(usize, usize)>,
    last_click: Option<Click>,
    modifiers: keyboard::Modifiers,
    /// Fixed end of the selection; shift-click extends from here.
    anchor: Option<Pos>,
    /// Last `reveal` token handled.
    revealed: Option<u64>,
    /// Last `select_verses` token handled.
    selected_token: Option<u64>,
    selected_words: Option<u64>,
    /// Whether the selection of an earlier life of the widget has been put back.
    restored: bool,
    /// Whether a card left open from an earlier life of the widget has been looked for.
    stale_checked: bool,
    /// Whether the last click landed in this widget; keyboard shortcuts apply only then,
    /// so Cmd+C in the writer does not copy the reader's selection.
    focused: bool,
    cleared: u64,
    /// The toolbar's laid-out size (positioned at draw time, under the selection).
    toolbar_node: Option<layout::Node>,
    /// Line (widget-local) under which an `UnderClick` element is shown.
    click_anchor: Option<Rectangle>,
    /// The link (block, span) whose card is floating, so pressing it again leaves the card be.
    click_link: Option<(usize, usize)>,
    /// Widget state of each card element, kept by card key.
    card_trees: BTreeMap<u64, Tree>,
    /// Where each card (in `cards` order) ended up.
    cards: Vec<CardLaid>,
    /// Marks under the pointer (text hover) and the card under it, for emphasis.
    hover_marks: Vec<usize>,
    hover_card: Option<u64>,
    /// The cards (by key) on each line, as of the last layout, and which of a stack is on top
    /// (by the lowest key in it).
    groups: Vec<Vec<u64>>,
    stack_top: BTreeMap<u64, u64>,
    /// The link spans by their place in the text, as of the last layout.
    link_by_slice: HashMap<CanonSlice, (usize, usize)>,
    /// Whether the margin is showing, as of the last layout; and the last value reported.
    column_mode: bool,
    reported_mode: Option<bool>,
    /// Right edge of the text column (where the margin starts, less the gap).
    text_right: f32,
    /// Where the text starts: after the left margin, when there is one.
    text_left: f32,
    /// The space between the text and the cards.
    gap: f32,
    /// Whether there is a margin on the left (cards that follow a link are put there).
    both_sides: bool,
    /// Visible area as of the last event; operations (which get no viewport) use it.
    viewport: Rectangle,
    pieces: BTreeMap<u16, Vec<Piece>>,
    /// Selection last reported through `on_selection`.
    published: Option<Option<Selection>>,
}

impl<L> State<L> {
    fn new(key: u64) -> Self {
        Self {
            key,
            laid: Vec::new(),
            selection: None,
            drag: None,
            hovered: None,
            last_click: None,
            modifiers: keyboard::Modifiers::default(),
            anchor: None,
            revealed: None,
            selected_token: None,
            selected_words: None,
            restored: false,
            stale_checked: false,
            focused: false,
            cleared: 0,
            toolbar_node: None,
            click_anchor: None,
            click_link: None,
            card_trees: BTreeMap::new(),
            cards: Vec::new(),
            hover_marks: Vec::new(),
            hover_card: None,
            groups: Vec::new(),
            stack_top: BTreeMap::new(),
            link_by_slice: HashMap::new(),
            column_mode: false,
            reported_mode: None,
            text_right: 0.0,
            text_left: 0.0,
            gap: CARD_GAP,
            both_sides: false,
            viewport: Rectangle::with_size(Size::ZERO),
            pieces: BTreeMap::new(),
            published: None,
        }
    }

    /// Bring card `key` to the top of its stack; whether that changed anything.
    fn raise(&mut self, key: u64) -> bool {
        let Some(group) = self.groups.iter().find(|g| g.len() > 1 && g.contains(&key)) else { return false };
        let id = group.iter().copied().min().unwrap_or(key);
        self.stack_top.insert(id, key) != Some(key)
    }

    /// Block under (or nearest below) `p`, with the offset hit-tested inside it.
    fn pos_at(&self, blocks_len: impl Fn(usize) -> usize, p: Point) -> Option<Pos> {
        let last = self.laid.len().checked_sub(1)?;
        let b = self.laid.iter().position(|l| p.y < l.y + l.h).unwrap_or(last);
        let l = &self.laid[b];
        let len = blocks_len(b);
        let local = Point::new(p.x - l.x, (p.y - l.y).clamp(0.0, (l.h - 1.0).max(0.0)));
        let offset = match l.para.hit_test(local) {
            Some(Hit::CharOffset(i)) => i.min(len),
            None if p.y >= l.y + l.h => len,
            None => 0,
        };
        Some(Pos { block: b, offset })
    }

    /// Whether `p` is on the text of its visual line, not in the space after the last word of
    /// it (where the paragraph's hit-testing still answers with that last word).
    fn over_text(&self, p: Point) -> bool {
        let Some(l) = self.laid.iter().find(|l| p.y >= l.y && p.y < l.y + l.h) else { return false };
        let (x, y) = (p.x - l.x, p.y - l.y);
        let Some(run) = l.para.buffer().layout_runs().find(|r| y >= r.line_top && y < r.line_top + r.line_height) else { return false };
        let (start, end) = run.glyphs.iter().fold((f32::MAX, f32::MIN), |(lo, hi), g| (lo.min(g.x), hi.max(g.x + g.w)));
        x >= start && x < end
    }

    /// The character under `p` (not the nearest caret position, which can be the next
    /// character along). `None` past the end of a line.
    fn glyph_pos_at(&self, p: Point) -> Option<Pos> {
        let b = self.laid.iter().position(|l| p.y >= l.y && p.y < l.y + l.h)?;
        if !self.over_text(p) {
            return None;
        }
        let l = &self.laid[b];
        let cursor = l.para.buffer().hit(p.x - l.x, p.y - l.y)?;
        let line = l.para.buffer().lines.get(cursor.line)?;
        if cursor.index >= line.text().len() {
            return None;
        }
        let offset = match cursor.affinity {
            cosmic_text::Affinity::Before => cursor.index.saturating_sub(1),
            cosmic_text::Affinity::After => cursor.index,
        };
        Some(Pos { block: b, offset })
    }

    /// A thin rectangle at `p.x` spanning the visual line under `p`.
    fn line_at(&self, p: Point) -> Option<Rectangle> {
        let l = self.laid.iter().find(|l| p.y >= l.y && p.y < l.y + l.h)?;
        let run = l
            .para
            .buffer()
            .layout_runs()
            .find(|r| p.y - l.y >= r.line_top && p.y - l.y < r.line_top + r.line_height)?;
        Some(Rectangle::new(Point::new(p.x, l.y + run.line_top), Size::new(1.0, run.line_height)))
    }

    /// (block, span) of the span under `p` if it has a link.
    fn link_at<M>(&self, widget: &ReaderText<'_, L, M>, p: Point) -> Option<(usize, usize)> {
        let b = self.laid.iter().position(|l| p.y >= l.y && p.y < l.y + l.h)?;
        if !self.over_text(p) {
            return None;
        }
        let l = &self.laid[b];
        let span = l.para.hit_span(Point::new(p.x - l.x, p.y - l.y))?;
        let clickable = widget.blocks[b].spans.get(span)?.link.is_some() || widget.blocks[b].meta[span].number;
        clickable.then_some((b, span))
    }
}

impl<L, Message> Widget<Message, Theme, iced::Renderer> for ReaderText<'_, L, Message>
where
    L: Clone + 'static,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<L>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<L>::new(self.key))
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        self.toolbar.iter().map(|t| Tree::new(t.as_widget())).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(self.toolbar.as_slice());
        let state = tree.state.downcast_mut::<State<L>>();
        // (The cards' own trees are kept up to date as they are laid out: see `layout`.)
        let live: BTreeSet<u64> = self.cards.iter().map(|c| c.key).collect();
        state.card_trees.retain(|key, _| live.contains(key));
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let bounds = layout.bounds();
        {
            let state = tree.state.downcast_mut::<State<L>>();
            for (spec, laid) in self.cards.iter_mut().zip(state.cards.clone()) {
                let Some(card_tree) = state.card_trees.get_mut(&spec.key) else { continue };
                let Some(element) = spec.element.built.as_mut().filter(|_| !laid.hidden) else { continue };
                operation.container(None, laid.absolute(bounds));
                operation.traverse(&mut |operation| {
                    element.as_widget_mut().operate(
                        card_tree,
                        Layout::with_offset(laid.offset(bounds), &laid.node),
                        renderer,
                        operation,
                    );
                });
            }
        }
        let state = tree.state.downcast_ref::<State<L>>();
        let Some((offset, _)) = self.toolbar_geometry(state, layout.bounds(), &state.viewport) else {
            return;
        };
        let Some(node) = state.toolbar_node.clone() else { return };
        if let (Some(toolbar), Some(child)) = (self.toolbar.as_mut(), tree.children.get_mut(0)) {
            operation.container(None, layout.bounds());
            operation.traverse(&mut |operation| {
                toolbar.as_widget_mut().operate(child, Layout::with_offset(offset, &node), renderer, operation);
            });
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<L>>();
        if state.key != self.key {
            *state = State::new(self.key);
        }
        state.laid.truncate(self.blocks.len());
        state.pieces = canonical_pieces(&self.blocks);
        state.link_by_slice = self.link_spans(&state.pieces);

        let width = limits.max().width;
        let font = renderer.default_font();
        let hovered = state.hovered;

        // A margin appears beside the text when there is room for one; otherwise the cards
        // are not laid out at all (the app floats one under a clicked mark instead).
        let has_cards = !self.cards.is_empty();
        let (column_mode, text_left, text_w, card_w) = // (Margins asked for on both sides stay when a chapter has nothing for them: the text does not move.)
        match self.both_sides.then(|| text_layout_both(width, self.text_width)).flatten() {
            Some((left, text, card)) => (true, left, text, card),
            None => {
                let (column, text, card) = text_layout(width, self.text_width, has_cards);
                (column, 0.0, text, card)
            }
        };
        let both_sides = text_left > 0.0;
        let gap = if both_sides { BOTH_GAP } else { CARD_GAP };
        let pad = gap / 2.0;
        state.gap = gap;
        state.column_mode = column_mode;
        state.both_sides = both_sides;
        state.text_left = text_left;
        state.text_right = text_left + text_w;

        // Cards are laid out below, once the text is, and only the ones that show.
        let card_limits = layout::Limits::new(Size::ZERO, Size::new((card_w - pad).max(1.0), f32::INFINITY));
        let mut nodes: Vec<layout::Node> =
            if column_mode { vec![layout::Node::new(Size::ZERO); self.cards.len()] } else { Vec::new() };
        if !column_mode {
            state.card_trees.clear();
        }
        let mut placed: Vec<Option<Rectangle>> = vec![None; nodes.len()];

        let mut y = 0.0;
        for (i, block) in self.blocks.iter().enumerate() {
            if i > 0 {
                y += block.gap_before;
            }
            let w = (text_w - block.indent).max(1.0);

            // The hovered verse number is re-shaped in the hover color.
            let tinted;
            let spans: &[Span<'static, L>] = match hovered {
                Some((hb, hj)) if hb == i && block.meta[hj].number => {
                    tinted = block
                        .spans
                        .iter()
                        .enumerate()
                        .map(|(j, s)| {
                            let mut s = s.clone();
                            if j == hj {
                                s.color = Some(self.hover_color);
                            }
                            s
                        })
                        .collect::<Vec<_>>();
                    &tinted
                }
                _ => &block.spans,
            };

            let shape = Text {
                content: (),
                bounds: Size::new(w, f32::INFINITY),
                size: self.size,
                line_height: self.line_height,
                font,
                align_x: Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::default(),
            };

            match state.laid.get_mut(i) {
                Some(laid) => {
                    if laid.spans.as_slice() != spans {
                        laid.para = Para::with_spans(shape.with_content(spans));
                        laid.spans = spans.to_vec();
                    } else {
                        match laid.para.compare(shape) {
                            Difference::None => {}
                            Difference::Bounds => laid.para.resize(shape.bounds),
                            Difference::Shape => {
                                laid.para = Para::with_spans(shape.with_content(spans));
                            }
                        }
                    }
                }
                None => state.laid.push(Laid {
                    spans: spans.to_vec(),
                    para: Para::with_spans(shape.with_content(spans)),
                    x: 0.0,
                    y: 0.0,
                    w,
                    h: 0.0,
                }),
            }

            let laid = &mut state.laid[i];
            laid.x = text_left + block.indent;
            laid.y = y;
            laid.w = w;
            laid.h = laid.para.min_height();
            y += laid.h;
        }

        // Column mode: level with each mark's first line, stacked where they collide.
        let mut anchors: Vec<Option<Rectangle>> = vec![None; nodes.len()];
        let mut hidden = vec![false; nodes.len()];
        let mut stacks: Vec<Option<StackBar>> = vec![None; nodes.len()];
        if column_mode {
            // A footnote sits where its marker is (which can start a line, while the character
            // before it ends the one above); the rest where their marked text begins.
            let lines: Vec<Option<Rectangle>> = self
                .cards
                .iter()
                .map(|spec| {
                    let marker = spec.follows_link.then(|| state.link_by_slice.get(&spec.anchor).copied()).flatten().and_then(|(b, j)| {
                        let laid = state.laid.get(b)?;
                        let seg = span_segments(&laid.para, j).into_iter().next()?;
                        Some(Rectangle::new(Point::new(laid.x + seg.x, laid.y + seg.top), Size::new(seg.w, seg.height)))
                    });
                    marker.or_else(|| slice_first_line(state, spec.anchor))
                })
                .collect();
            // Notes whose cards would run into each other (marks within `STACK_REACH` of the first
            // of them, a few lines) are layered into a stack: one slot, with the top card shown.
            let at = |ci: usize| lines[ci].map_or(0.0, |l| l.y);
            let on_left: Vec<bool> = if both_sides { sides_by_nearness(&lines, text_left + text_w / 2.0) } else { vec![false; lines.len()] };
            let mut shown: BTreeSet<u64> = BTreeSet::new();
            state.groups.clear();
            for left in [false, true] {
            let mut by_place: Vec<usize> = (0..nodes.len()).filter(|&c| on_left[c] == left).collect();
            by_place.sort_by(|&a, &b| at(a).total_cmp(&at(b)).then(a.cmp(&b)));
            let mut groups: Vec<Vec<usize>> = Vec::new();
            let mut reach = f32::NEG_INFINITY;
            for ci in by_place {
                match groups.last_mut() {
                    Some(group) if at(ci) < reach => group.push(ci),
                    _ => {
                        groups.push(vec![ci]);
                        reach = at(ci) + STACK_REACH;
                    }
                }
            }
            let keys: Vec<u64> = self.cards.iter().map(|c| c.key).collect();
            let id_of = |g: &[usize]| g.iter().map(|&c| keys[c]).min().unwrap_or(0);
            // On top: the card being written, else the one chosen (by pointing at its mark or
            // with the arrows), else, to start with, a note of the user's over a footnote.
            let tops: Vec<usize> = groups
                .iter()
                .map(|g| {
                    g.iter()
                        .copied()
                        .find(|&c| self.cards[c].pinned)
                        .or_else(|| g.iter().copied().find(|&c| state.stack_top.get(&id_of(g)) == Some(&self.cards[c].key)))
                        .or_else(|| g.iter().copied().find(|&c| !self.cards[c].follows_link))
                        .unwrap_or(g[0])
                })
                .collect();
            // Only the card on top of each stack is built and laid out (and the margin's own menu).
            for ci in tops.iter().copied() {
                let spec = &mut self.cards[ci];
                let element = spec.element.build();
                match state.card_trees.get_mut(&spec.key) {
                    Some(card_tree) => card_tree.diff(element.as_widget()),
                    None => {
                        state.card_trees.insert(spec.key, Tree::new(element.as_widget()));
                    }
                }
                if let Some(card_tree) = state.card_trees.get_mut(&spec.key) {
                    nodes[ci] = element.as_widget_mut().layout(card_tree, renderer, &card_limits);
                }
            }
            shown.extend(tops.iter().copied().map(|c| self.cards[c].key));
            let slots: Vec<Slot> = groups
                .iter()
                .zip(&tops)
                .map(|(g, &top)| {
                    let bar = if g.len() > 1 { STACK_BAR } else { 0.0 };
                    let pinned = g.iter().any(|&c| self.cards[c].pinned);
                    Slot {
                        anchor: lines[g[0]].map_or(0.0, |l| l.y),
                        height: nodes[top].size().height + bar,
                        pinned,
                    }
                })
                .collect();
            let mut ys = place_column(&slots, STACK_GAP);
            // Not against the top of the panel.
            let shift = ys.iter().copied().reduce(f32::min).map_or(0.0, |top| (pad - top).max(0.0));
            ys.iter_mut().for_each(|y| *y += shift);
            let card_x = text_left + text_w + gap;
            for (gi, group) in groups.iter().enumerate() {
                let top = tops[gi];
                let shown = nodes[top].size().height;
                // On the left the cards sit against the text, so the connectors have the same length either side.
                let x = if left { text_left - gap - nodes[top].size().width } else { card_x };
                let rect = Rectangle::new(Point::new(x, ys[gi]), Size::new(nodes[top].size().width, shown));
                let stack = (group.len() > 1).then(|| StackBar {
                    id: id_of(group),
                    keys: group.iter().map(|&c| self.cards[c].key).collect(),
                    colors: group.iter().map(|&c| self.cards[c].color).collect(),
                    at: group.iter().position(|&c| c == top).unwrap_or(0),
                    bar: Rectangle::new(Point::new(rect.x, rect.y + shown + LAYER_PEEK * 2.0 + 2.0), Size::new(rect.width, STACK_BAR - 8.0)),
                });
                for &ci in group {
                    placed[ci] = Some(rect);
                    hidden[ci] = ci != top;
                    if ci == top {
                        stacks[ci] = stack.clone();
                    }
                }
            }
            state.groups.extend(groups.iter().map(|g| g.iter().map(|&c| self.cards[c].key).collect::<Vec<_>>()));
            }
            state.card_trees.retain(|key, _| shown.contains(key));
            anchors = lines;
        }

        state.cards = self
            .cards
            .iter()
            .zip(nodes)
            .zip(placed.into_iter().zip(anchors).zip(hidden.into_iter().zip(stacks)))
            .filter_map(|((spec, node), ((rect, anchor_line), (hidden, stack)))| {
                Some(CardLaid { node, rect: rect?, anchor_line, lane: 0, color: spec.color, hidden, stack })
            })
            .collect();
        // Give connectors whose vertical stretches would overlap their own lanes.
        // (A stack has one connector, to the card on top.)
        let stretches: Vec<(f32, f32)> = state
            .cards
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| {
                let mark = c.anchor_line.map_or(c.rect.y, |l| l.y + l.height / 2.0);
                let card = c.rect.y + CONNECTOR_CARD_OFFSET;
                (mark.min(card), mark.max(card))
            })
            .collect();
        for (card, lane) in state.cards.iter_mut().filter(|c| !c.hidden).zip(assign_lanes(&stretches, LANES)) {
            card.lane = lane;
        }
        let cards_bottom =
            state.cards.iter().map(|c| c.rect.y + c.rect.height + if c.stack.is_some() { STACK_BAR } else { 0.0 }).fold(0.0, f32::max);
        // Room below the last card too, so the panel does not end against it.
        let y = if cards_bottom > 0.0 { y.max(cards_bottom + pad) } else { y };

        let width_limit = Size::new(width, f32::INFINITY);
        state.toolbar_node = match (self.toolbar.as_mut(), tree.children.get_mut(0)) {
            (Some(toolbar), Some(child)) => Some(toolbar.as_widget_mut().layout(
                child,
                renderer,
                &layout::Limits::new(Size::ZERO, width_limit),
            )),
            _ => None,
        };
        layout::Node::new(Size::new(width, y))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<L>>();
        let origin = layout.position();
        let mut emphasised: BTreeSet<usize> = state
            .hover_marks
            .iter()
            .copied()
            .chain(state.hover_card.map(|key| key as usize))
            .collect();
        // The pointer on a link emphasises its card; a card that is emphasised (by either end)
        // lights up its link. `link_glow` is each such link span with its card's colour.
        let mut link_glow: Vec<((usize, usize), Color)> = Vec::new();
        for spec in self.cards.iter().filter(|c| c.follows_link) {
            let Some(span) = state.link_by_slice.get(&spec.anchor).copied() else { continue };
            if state.hovered == Some(span) {
                emphasised.insert(spec.key as usize);
            }
            if emphasised.contains(&(spec.key as usize)) {
                link_glow.push((span, spec.color));
            }
        }

        for (b, (laid, block)) in state.laid.iter().zip(&self.blocks).enumerate() {
            let top_left = origin + Vector::new(laid.x, laid.y);
            let area = Rectangle::new(top_left, Size::new(laid.w, laid.h));
            if !area.intersects(viewport) {
                continue;
            }

            // Span backgrounds (e.g. the selected verse), full line height so mixed
            // font sizes (verse numbers next to body text) form one even band.
            for (j, span) in block.spans.iter().enumerate() {
                if let Some(h) = span.highlight {
                    for seg in span_segments(&laid.para, j) {
                        renderer.fill_quad(
                            Quad {
                                bounds: Rectangle::new(
                                    top_left + Vector::new(seg.x, seg.top),
                                    Size::new(seg.w, seg.height),
                                ),
                                ..Quad::default()
                            },
                            h.background,
                        );
                    }
                }
            }

            for &((gb, gj), color) in &link_glow {
                if gb != b {
                    continue;
                }
                for seg in span_segments(&laid.para, gj) {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle::new(top_left + Vector::new(seg.x, seg.top), Size::new(seg.w, seg.height)),
                            border: iced::Border { radius: 3.0.into(), ..iced::Border::default() },
                            ..Quad::default()
                        },
                        Color { a: 0.28, ..color },
                    );
                }
            }

            // Marks (highlights, underlines) from the margin layer. A second pass paints the
            // emphasised ones (hovered, or whose card is hovered) over again, so they deepen.
            for pass in 0..2 {
                for mark in &self.marks {
                    if pass == 1 && !emphasised.contains(&mark.mark) {
                        continue;
                    }
                    // (The words of a verse can be pieces of their own, the Strong's words; a mark is one stretch from the
                    // first of them to the last, drawn whole along each line rather than word by word.)
                    let ranges = block_ranges(&state.pieces, mark.slice).into_iter().filter(|(mb, _, _)| *mb == b);
                    let whole = ranges.fold(None, |acc: Option<(usize, usize)>, (_, s, e)| Some(acc.map_or((s, e), |(lo, hi)| (lo.min(s), hi.max(e)))));
                    if let Some((s, e)) = whole {
                        let (c0, c1) = (cosmic_text::Cursor::new(0, s), cosmic_text::Cursor::new(0, e));
                        for run in laid.para.buffer().layout_runs() {
                            let Some((x, w)) = run.highlight(c0, c1) else { continue };
                            if pass == 1 && mark.kind != MarkKind::Highlight {
                                // Underlines get the hover highlight highlights have: a calmer
                                // version of their colour behind the line.
                                renderer.fill_quad(
                                    Quad {
                                        bounds: Rectangle::new(
                                            top_left + Vector::new(x, run.line_top),
                                            Size::new(w, run.line_height),
                                        ),
                                        ..Quad::default()
                                    },
                                    softened(mark.color),
                                );
                            }
                            paint_mark(
                                renderer,
                                mark.kind,
                                mark.color,
                                Rectangle::new(top_left + Vector::new(x, run.line_top), Size::new(w, run.line_height)),
                                run.line_y - run.line_top + self.size.0 * UNDERLINE_DROP,
                            );
                        }
                    }
                }
            }

            // Text selection.
            if let Some((s, e)) = state.selection.and_then(|sel| range_in_block(sel, b, block.len()))
            {
                let (c0, c1) = (cosmic_text::Cursor::new(0, s), cosmic_text::Cursor::new(0, e));
                for run in laid.para.buffer().layout_runs() {
                    if let Some((x, w)) = run.highlight(c0, c1) {
                        renderer.fill_quad(
                            Quad {
                                bounds: Rectangle::new(
                                    top_left + Vector::new(x, run.line_top),
                                    Size::new(w, run.line_height),
                                ),
                                ..Quad::default()
                            },
                            self.selection_color,
                        );
                    }
                }
            }

            // Underlines: spans that ask for one, and the hovered link. Positioned from the
            // line's real baseline, not the span's own metrics, so small spans sit right.
            for (j, span) in block.spans.iter().enumerate() {
                let hovered = state.hovered == Some((b, j)) && !block.meta[j].number;
                // (A dotted span stays dotted when pointed at; only its color changes.)
                let dotted = block.meta[j].dotted;
                if !(span.underline || hovered || dotted) {
                    continue;
                }
                let size = span.size.unwrap_or(self.size).0;
                let color =
                    if hovered { self.hover_color } else { span.color.unwrap_or(style.text_color) };
                for seg in span_segments(&laid.para, j) {
                    if dotted {
                        // A row of dots, a little fainter than a line would be.
                        let (step, dot) = (3.5, 1.5);
                        let y = top_left.y + seg.baseline + size * 0.12;
                        let mut x = seg.x;
                        while x + dot <= seg.x + seg.w {
                            renderer.fill_quad(
                                Quad { bounds: Rectangle::new(Point::new(top_left.x + x, y), Size::new(dot, dot)), ..Quad::default() },
                                Color { a: 0.85, ..color },
                            );
                            x += step;
                        }
                        continue;
                    }
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle::new(
                                top_left + Vector::new(seg.x, seg.baseline + size * 0.12),
                                Size::new(seg.w, 1.0),
                            ),
                            ..Quad::default()
                        },
                        color,
                    );
                }
            }

            renderer.fill_paragraph(&laid.para, top_left, style.text_color, *viewport);
        }

        // The margin: a faintly different panel, without an edge line, alongside the text from its top to
        // its bottom, so it reads as a place even when it is empty.
        let bounds = layout.bounds();
        if SHOW_MARGIN_PANEL && state.column_mode {
            let edge = bounds.x + state.text_right + state.gap / 2.0;
            let bottom = bounds.y + bounds.height;
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        Point::new(edge, bounds.y),
                        Size::new((bounds.x + bounds.width - edge).max(0.0), bottom - bounds.y),
                    ),
                    border: iced::Border { radius: MARGIN_RADIUS.into(), ..iced::Border::default() },
                    ..Quad::default()
                },
                self.margin_colors.0,
            );
            if state.both_sides {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            Point::new(bounds.x, bounds.y),
                            Size::new((state.text_left - state.gap / 2.0).max(0.0), bottom - bounds.y),
                        ),
                        border: iced::Border { radius: MARGIN_RADIUS.into(), ..iced::Border::default() },
                        ..Quad::default()
                    },
                    self.margin_colors.0,
                );
            }
        }

        // Margin cards, with a connector from each marked line. Connectors use the mark's own
        // colour, opaque, so where they cross they do not blend into other shades; the hovered
        // one is drawn after the rest, so it ends up on top.
        let bounds = layout.bounds();
        // While something is pointed at, everything else steps back: the other connectors fade
        // toward the background (still opaque, so where they cross they do not blend), and the
        // other notes are veiled, so what is pointed at can be followed.
        let lit = self.dim_cards || self.cards.iter().zip(&state.cards).any(|(spec, laid)| !laid.hidden && laid.is_hot(spec.key, &emphasised));
        let backdrop = theme.extended_palette().background.base.color;
        for hovered_pass in [false, true] {
            for (spec, laid) in self.cards.iter().zip(&state.cards) {
                let hot = laid.is_hot(spec.key, &emphasised);
                if laid.hidden || hot != hovered_pass {
                    continue;
                }
                let Some(line) = laid.anchor_line else { continue };
                // Drawn if any of it is in view: one end may be off screen (its mark above or
                // its note below) while the stretch between still crosses what is showing.
                let (top, bottom) = (line.y.min(laid.rect.y), (line.y + line.height).max(laid.rect.y + CONNECTOR_CARD_OFFSET));
                let on_left = state.both_sides && laid.rect.x < state.text_left;
                // The stretch between the card and the text: on the card's left when the card is
                // on the right, else on its right.
                let gap_x = if on_left { bounds.x + laid.rect.x + laid.rect.width } else { bounds.x + laid.rect.x - state.gap };
                let extent = Rectangle::new(Point::new(gap_x, bounds.y + top), Size::new(state.gap + 1.0, (bottom - top).max(1.0)));
                if !extent.intersects(viewport) {
                    continue;
                }
                let weight = if hot && !self.dim_cards { 2.5 } else { 1.5 };
                let gutter = gap_x;
                let (mid, card_mid) =
                    (bounds.y + line.y + line.height / 2.0, bounds.y + laid.rect.y + CONNECTOR_CARD_OFFSET);
                // The elbow sits in the middle of the gap; a connector sharing its stretch with
                // another steps aside to its own lane.
                let elbow = gutter + state.gap / 2.0 + lane_offset(laid.lane);
                let card_x = bounds.x + laid.rect.x;
                // Rounded ends: where two segments meet at a right angle their ends overlap, which
                // rounds the outer corner instead of leaving a stair-stepped square one.
                // (A card being written in is not dimmed with the rest, when they are all dimmed to begin with.)
                let ink = if lit && !hot && !(self.dim_cards && spec.pinned) { faded(laid.color, backdrop) } else { laid.color };
                let mut bar = |x: f32, y: f32, w: f32, h: f32| {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle::new(Point::new(x, y), Size::new(w, h)),
                            border: iced::Border { radius: (w.min(h) / 2.0).into(), ..iced::Border::default() },
                            ..Quad::default()
                        },
                        ink,
                    );
                };
                if on_left {
                    // Mirrored: the mark's bar at the text's left edge, the card to its left.
                    let handle = gutter + state.gap - HANDLE_OFFSET - 3.0;
                    bar(handle, bounds.y + line.y, 3.0, line.height);
                    bar(elbow, mid - weight / 2.0, handle - elbow, weight);
                    bar(elbow - weight / 2.0, mid.min(card_mid), weight, (card_mid - mid).abs());
                    bar(gutter, card_mid - weight / 2.0, elbow - gutter, weight);
                } else {
                    let handle = gutter + if state.both_sides { HANDLE_OFFSET } else { 2.0 };
                    bar(handle, bounds.y + line.y, 3.0, line.height);
                    bar(handle, mid - weight / 2.0, elbow - handle, weight);
                    bar(elbow - weight / 2.0, mid.min(card_mid), weight, (card_mid - mid).abs());
                    bar(elbow, card_mid - weight / 2.0, card_x - elbow, weight);
                }
            }
        }
        for (spec, laid) in self.cards.iter().zip(&state.cards) {
            if laid.hidden {
                continue;
            }
            let abs = laid.absolute(bounds);
            if let Some(stack) = &laid.stack {
                let dim = lit && !laid.is_hot(spec.key, &emphasised) && !(self.dim_cards && spec.pinned);
                self.draw_stack_behind(renderer, theme, viewport, bounds, abs, stack, cursor, dim);
            }
            // Only the part of a card inside what is visible is drawn: the layer's clip is its
            // own, so without this a card past the edge of the pane would spill into the next.
            let Some(visible) = abs.intersection(viewport) else { continue };
            if let (Some(card_tree), Some(element)) = (state.card_trees.get(&spec.key), spec.element.built.as_ref()) {
                renderer.with_layer(visible, |renderer| {
                    element.as_widget().draw(
                        card_tree,
                        renderer,
                        theme,
                        style,
                        Layout::with_offset(laid.offset(bounds), &laid.node),
                        cursor,
                        &visible,
                    );
                });
            }
            // On top of the card, which has a layer of its own: veiled when something else is
            // pointed at, lit when it is what is pointed at.
            let edge = Rectangle::new(Point::new(abs.x - 2.0, abs.y - 2.0), Size::new(abs.width + 4.0, abs.height + 4.0));
            if let Some(clip) = edge.intersection(viewport) {
                renderer.with_layer(clip, |renderer| {
                    if lit && !laid.is_hot(spec.key, &emphasised) && !(self.dim_cards && spec.pinned) {
                        // Something else is pointed at: this note steps back.
                        renderer.fill_quad(
                            Quad { bounds: abs, border: iced::Border { radius: 6.0.into(), ..iced::Border::default() }, ..Quad::default() },
                            Color { a: VEIL, ..backdrop },
                        );
                        // The veil leaves the edge too bright: draw it again, faded like the connectors.
                        renderer.fill_quad(
                            Quad { bounds: abs, border: iced::Border { color: faded(laid.color, backdrop), width: 1.5, radius: 6.0.into() }, ..Quad::default() },
                            Color::TRANSPARENT,
                        );
                    }
                    // (Dimmed cards just come back to normal when pointed at, with no lit edge.)
                    if !self.dim_cards && laid.is_hot(spec.key, &emphasised) {
                        // The pointer is on the card, or on what it is about: its edge lights up.
                        renderer.fill_quad(
                            Quad {
                                bounds: abs,
                                border: iced::Border { color: laid.color, width: 2.5, radius: 6.0.into() },
                                ..Quad::default()
                            },
                            Color { a: 0.12, ..laid.color },
                        );
                    }
                });
            }
        }

        // The toolbar goes in its own layer so it paints over the text beneath it.
        if let Some((offset, _)) = self.toolbar_geometry(state, layout.bounds(), viewport)
            && let (Some(toolbar), Some(node), Some(child)) =
                (self.toolbar.as_ref(), state.toolbar_node.as_ref(), tree.children.first())
        {
            renderer.with_layer(*viewport, |renderer| {
                toolbar.as_widget().draw(
                    child,
                    renderer,
                    theme,
                    style,
                    Layout::with_offset(offset, node),
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // iced hands every event to every widget; one that a widget earlier in the tree
        // already used (a toolbar button, say) is not ours to act on or to treat as a click
        // elsewhere.
        {
            let state = tree.state.downcast_mut::<State<L>>();
            if state.reported_mode != Some(state.column_mode) {
                state.reported_mode = Some(state.column_mode);
                if let Some(f) = &self.on_layout {
                    shell.publish(f(state.column_mode));
                }
            }
        }
        if shell.is_event_captured() {
            return;
        }
        let state = tree.state.downcast_mut::<State<L>>();
        let bounds = layout.bounds();
        state.viewport = *viewport;

        // The toolbar gets every event first. Presses and moves over it never reach the text.
        if let Some((offset, rect)) = self.toolbar_geometry(state, bounds, viewport)
            && let (Some(toolbar), Some(node), Some(child)) =
                (self.toolbar.as_mut(), state.toolbar_node.clone(), tree.children.get_mut(0))
        {
            toolbar.as_widget_mut().update(
                child,
                event,
                Layout::with_offset(offset, &node),
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            if shell.is_event_captured() {
                return;
            }
            if matches!(event, Event::Mouse(e) if !matches!(e, mouse::Event::WheelScrolled { .. }))
                && cursor.is_over(rect)
            {
                if state.hovered.take().is_some() {
                    shell.request_redraw();
                }
                shell.capture_event();
                return;
            }
            // A chooser closes on a press elsewhere, or Escape.
            if self.toolbar_kind == FloatKind::UnderClick {
                let escape = matches!(
                    event,
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::Escape),
                        ..
                    })
                );
                // Pressing the very link the card came from is not a press elsewhere: it stays.
                let on_its_link = matches!(event, Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)))
                    && state.click_link.is_some()
                    && cursor.position().is_some_and(|p| state.link_at(self, Point::new(p.x - bounds.x, p.y - bounds.y)) == state.click_link);
                if !on_its_link && (escape || matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_)))) {
                    state.click_anchor = None;
                    state.click_link = None;
                    if let Some(f) = &self.on_marks {
                        shell.publish(f(Vec::new()));
                    }
                    shell.request_redraw();
                    if escape {
                        shell.capture_event();
                        return;
                    }
                }
            }
        }

        // Margin cards come next; whatever they use never reaches the text.
        if !self.cards.is_empty() {
            // The arrows of a stack of notes.
            if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))) {
                let pressed = state.cards.iter().filter_map(|c| c.stack.as_ref()).find_map(|stack| {
                    let (prev, next) = stack.zones(bounds);
                    let forward = cursor.is_over(next);
                    (cursor.is_over(prev) || forward).then(|| (stack.id, stack.step(forward)))
                });
                if let Some((id, top)) = pressed {
                    state.stack_top.insert(id, top);
                    shell.invalidate_layout();
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            // A press anywhere in the margin counts as a press in the reader (it puts away what is
            // open, like the menus).
            if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)))
                && cursor.is_over(bounds)
                && let Some(f) = &self.on_press
            {
                shell.publish(f());
            }
            let mut over_card = None;
            let mut over_note = None;
            for ci in 0..self.cards.len() {
                let Some(laid) = state.cards.get(ci).cloned() else { continue };
                let spec = &mut self.cards[ci];
                if laid.hidden {
                    continue;
                }
                let abs = laid.absolute(bounds);
                if cursor.is_over(abs) {
                    over_card = Some(spec.key);
                    over_note = Some(spec.key);
                }
                let Some(card_tree) = state.card_trees.get_mut(&spec.key) else { continue };
                let Some(element) = spec.element.built.as_mut() else { continue };
                element.as_widget_mut().update(
                    card_tree,
                    event,
                    Layout::with_offset(laid.offset(bounds), &laid.node),
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
                if shell.is_event_captured() {
                    return;
                }
            }
            if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. })) && state.hover_card != over_note {
                state.hover_card = over_note;
                shell.request_redraw();
            }
            // A scroll wheel over a card scrolls the card if it has more to show (its own
            // scrollable captures that); otherwise it falls through to scroll the page.
            // A press anywhere but on a card ends the edit of the card being written: there is no
            // Done button, clicking away is how you finish. (Pressing another card is left to that
            // card, which moves the edit to itself.)
            if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)))
                && over_card.is_none()
                && self.cards.iter().any(|c| c.pinned)
                && let Some(f) = &self.on_marks
            {
                shell.publish(f(Vec::new()));
            }
            if over_card.is_some()
                && matches!(event, Event::Mouse(e) if !matches!(e, mouse::Event::WheelScrolled { .. }))
            {
                if state.hovered.take().is_some() {
                    shell.request_redraw();
                }
                shell.capture_event();
                return;
            }
        }

        // A "levitating" cursor is outside the scroll area but still has a content position,
        // which is what keeps a drag-selection tracking the pointer past the edges.
        let content_point = match cursor {
            mouse::Cursor::Available(p) | mouse::Cursor::Levitating(p) => Some(p),
            mouse::Cursor::Unavailable => None,
        };
        let local = content_point.map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        let inside = cursor.is_over(bounds);
        let block_len = |b: usize| self.blocks[b].len();
        let tintable = |h: Option<(usize, usize)>| h.is_some_and(|(b, j)| self.blocks[b].meta[j].number);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let (Some(f), Some(p), true, Some(at)) = (&self.on_link_menu, local, inside, cursor.position())
                    && let Some((b, j)) = state.link_at(self, p)
                    && let Some(link) = self.blocks[b].spans[j].link.clone()
                {
                    shell.publish(f(link, at));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if inside && let Some(f) = &self.on_press {
                    shell.publish(f());
                }
                let (Some(p), true) = (local, inside) else {
                    // A press elsewhere leaves the selection where it is, to come back to (its toolbar
                    // is put away until the reader is pressed again): it is the reader's place held.
                    state.focused = false;
                    // (A press can only come with the last one's release long over: a drag still held is a
                    // release that was missed, which would hide the toolbar for good.)
                    state.drag = None;
                    return;
                };
                state.focused = true;
                let Some(pos) = state.pos_at(block_len, p) else { return };

                let count = match &state.last_click {
                    Some(c) if c.at.elapsed() < MULTI_CLICK && distance(c.point, p) < 5.0 => {
                        if c.count >= 3 { 1 } else { c.count + 1 }
                    }
                    _ => 1,
                };
                state.last_click = Some(Click { at: Instant::now(), point: p, count });

                // The second press of a double click (or the third of a triple) is selecting text: the card the first one
                // opened, under the line, goes.
                if count >= 2 && self.toolbar_kind == FloatKind::UnderClick && self.toolbar.is_some() {
                    state.click_anchor = None;
                    state.click_link = None;
                    if let Some(f) = &self.on_marks {
                        shell.publish(f(Vec::new()));
                    }
                }

                let pressed_link = state.link_at(self, p);
                let previous = state.selection;
                let extend = state.modifiers.shift()
                    && count == 1
                    && state.anchor.is_some()
                    && pressed_link.is_none();
                let anchor = if extend { state.anchor.unwrap_or(pos) } else { pos };

                state.selection = None;
                match count {
                    // (Past the end of a line there is no word to select.)
                    2 if !state.over_text(p) => {}
                    2 => {
                        let (s, e) = word_bounds(&self.blocks[pos.block].text(), pos.offset);
                        state.selection = (s < e).then_some((
                            Pos { block: pos.block, offset: s },
                            Pos { block: pos.block, offset: e },
                        ));
                    }
                    3 => {
                        state.selection = verse_at(&self.blocks[pos.block], pos.offset)
                            .and_then(|v| verse_selection(&self.blocks, v));
                    }
                    _ if extend => state.selection = (anchor != pos).then_some((anchor, pos)),
                    _ => {}
                }
                state.anchor = state.selection.map(|(a, _)| a).or(Some(anchor));
                state.drag = Some(Drag {
                    origin: p,
                    anchor,
                    anchor_sel: state.selection,
                    previous,
                    count,
                    // A shift-click is a selection gesture, never a link activation.
                    moved: extend,
                    pressed_link,
                });
                shell.capture_event();
                shell.request_redraw();
            }

            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(drag) = &mut state.drag {
                    let Some(p) = local else { return };
                    if !drag.moved && distance(drag.origin, p) > DRAG_THRESHOLD {
                        drag.moved = true;
                        // Pulled away from a link (not a verse number) after one press: the link is being carried. After a
                        // double or triple click it is the text that is being selected, link or no.
                        let carried = drag
                            .pressed_link
                            .filter(|_| drag.count == 1)
                            .filter(|&(b, j)| !self.blocks[b].meta[j].number)
                            .and_then(|(b, j)| self.blocks[b].spans[j].link.clone());
                        if let (Some(link), Some(f), Some(at)) = (carried, &self.on_link_drag, cursor.position()) {
                            state.drag = None;
                            shell.publish(f(link, at));
                            shell.capture_event();
                            shell.request_redraw();
                            return;
                        }
                    }
                    if drag.moved {
                        let (count, anchor, anchor_sel) = (drag.count, drag.anchor, drag.anchor_sel);
                        if let Some(focus) = state.pos_at(block_len, p) {
                            state.selection =
                                extended_selection(&self.blocks, count, anchor, anchor_sel, focus);
                        }
                    }
                    if tintable(state.hovered.take()) {
                        shell.invalidate_layout();
                    }
                    shell.capture_event();
                    shell.request_redraw();
                } else {
                    let hover_marks = local
                        .filter(|_| inside)
                        .and_then(|p| state.glyph_pos_at(p))
                        .map(|pos| marks_at(&state.pieces, &self.marks, pos))
                        .unwrap_or_default();
                    if hover_marks != state.hover_marks {
                        // Pointing at a mark brings its note to the top of its stack.
                        if let Some(&first) = hover_marks.first()
                            && state.raise(first as u64)
                        {
                            shell.invalidate_layout();
                        }
                        state.hover_marks = hover_marks;
                        shell.request_redraw();
                    }
                    let hovered = local.filter(|_| inside).and_then(|p| state.link_at(self, p));
                    if hovered != state.hovered {
                        if tintable(hovered) || tintable(state.hovered) {
                            shell.invalidate_layout();
                        }
                        // And so does pointing at a footnote's marker.
                        let card = hovered.and_then(|span| {
                            self.cards.iter().find(|c| c.follows_link && state.link_by_slice.get(&c.anchor).copied() == Some(span))
                        });
                        if let Some(card) = card
                            && state.raise(card.key)
                        {
                            shell.invalidate_layout();
                        }
                        state.hovered = hovered;
                        shell.request_redraw();
                    }
                }
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let Some(drag) = state.drag.take() else { return };
                if !drag.moved && drag.count == 1 {
                    let released_on = local.and_then(|p| state.link_at(self, p));
                    if let (Some(pressed), Some(on)) = (drag.pressed_link, released_on)
                        && pressed == on
                    {
                        let meta = self.blocks[on.0].meta[on.1];
                        if meta.number {
                            // A verse number is a shortcut for selecting that verse's text.
                            if let Some(verse) = meta.verse {
                                state.selection = verse_number_selection(
                                    &self.blocks,
                                    verse,
                                    state.modifiers.shift(),
                                    drag.previous,
                                );
                                state.anchor = state.selection.map(|(a, _)| a);
                            }
                        } else if let Some(link) = self.blocks[on.0].spans[on.1].link.clone()
                            && let Some(f) = &self.on_link
                        {
                            // Its card is already floating there: nothing to do.
                            if self.toolbar_kind == FloatKind::UnderClick && self.toolbar.is_some() && state.click_link == Some(on) {
                                shell.request_redraw();
                                return;
                            }
                            // A card for this link floats under the line when there is no margin.
                            state.click_anchor = local.and_then(|p| state.line_at(p));
                            state.click_link = Some(on);
                            let at = link_slice(&state.pieces, &self.blocks[on.0], on.0, on.1);
                            let marks = local
                                .and_then(|p| state.glyph_pos_at(p))
                                .map(|pos| marks_at(&state.pieces, &self.marks, pos))
                                .unwrap_or_default();
                            shell.publish(f(link, at, marks));
                        }
                    }
                }
                // A plain click inside a marked region opens that mark.
                if !drag.moved
                    && drag.count == 1
                    && drag.pressed_link.is_none()
                    && let (Some(f), Some(p)) = (&self.on_marks, local)
                {
                    let hits = state
                        .glyph_pos_at(p)
                        .map(|pos| marks_at(&state.pieces, &self.marks, pos))
                        .unwrap_or_default();
                    if !hits.is_empty() {
                        state.click_link = None;
                        state.click_anchor = state.line_at(p);
                        shell.publish(f(hits));
                    }
                }
                shell.request_redraw();
            }

            Event::Window(window::Event::RedrawRequested(_)) => {
                if state.cleared != self.clear_token {
                    state.cleared = self.clear_token;
                    state.selection = None;
                    state.anchor = None;
                    shell.request_redraw();
                }
                if let Some((token, first, last)) = self.select
                    && state.selected_token != Some(token)
                {
                    state.selected_token = Some(token);
                    state.focused = true;
                    let ends = (verse_selection(&self.blocks, first), verse_selection(&self.blocks, last));
                    if let (Some(a), Some(z)) = ends {
                        let (a, z) = (ordered(a), ordered(z));
                        state.selection = Some((a.0.min(z.0), a.1.max(z.1)));
                        state.anchor = state.selection.map(|(a, _)| a);
                        shell.request_redraw();
                    }
                }
                // A card is floating under a click the widget knows nothing of (it was made again after
                // its tab was away): it is a stale one, which would hide the selection's toolbar for good.
                if !state.stale_checked {
                    state.stale_checked = true;
                    if self.toolbar_kind == FloatKind::UnderClick
                        && self.toolbar.is_some()
                        && state.click_anchor.is_none()
                        && let Some(f) = &self.on_marks
                    {
                        shell.publish(f(Vec::new()));
                    }
                }
                if !state.restored {
                    state.restored = true;
                    if state.selection.is_none()
                        && let Some(slices) = &self.restore
                        && let (Some(first), Some(last)) = (slices.first(), slices.last())
                        && let (Some(&(fb, fs, _)), Some(&(lb, _, le))) = (block_ranges(&state.pieces, *first).first(), block_ranges(&state.pieces, *last).last())
                    {
                        let (a, z) = (Pos { block: fb, offset: fs }, Pos { block: lb, offset: le });
                        state.selection = Some((a, z));
                        state.anchor = Some(a);
                        shell.request_redraw();
                    }
                }
                if let Some((token, slice)) = self.select_words
                    && state.selected_words != Some(token)
                {
                    state.selected_words = Some(token);
                    state.focused = true;
                    let ranges = block_ranges(&state.pieces, slice);
                    if let (Some(&(fb, fs, _)), Some(&(lb, _, le))) = (ranges.first(), ranges.last()) {
                        let (a, z) = (Pos { block: fb, offset: fs }, Pos { block: lb, offset: le });
                        state.selection = Some((a, z));
                        state.anchor = Some(a);
                        shell.request_redraw();
                    }
                }
                if let Some((token, verse)) = self.reveal
                    && state.revealed != Some(token)
                {
                    state.revealed = Some(token);
                    if let Some((top, bottom)) = verse_extent(state, &self.blocks, verse) {
                        let (top, bottom) = (bounds.y + top, bounds.y + bottom);
                        let margin = 24.0;
                        let in_view = top >= viewport.y + margin
                            && bottom <= viewport.y + viewport.height - margin;
                        if !in_view && let Some(f) = &self.on_scroll {
                            // Land the verse a fifth of the way down the pane.
                            shell.publish(f(top - viewport.y - viewport.height * 0.2));
                        }
                    }
                }
                if let (Some(drag), Some(p)) = (&state.drag, content_point)
                    && drag.moved
                {
                    let dy = edge_scroll(p.y, viewport);
                    if dy != 0.0 {
                        let (count, anchor, anchor_sel) = (drag.count, drag.anchor, drag.anchor_sel);
                        if let Some(f) = &self.on_scroll {
                            shell.publish(f(dy));
                        }
                        if let Some(focus) =
                            local.and_then(|lp| state.pos_at(block_len, lp))
                        {
                            state.selection =
                                extended_selection(&self.blocks, count, anchor, anchor_sel, focus);
                        }
                        // Keep frames coming while the pointer sits past the edge.
                        shell.request_redraw();
                    }
                }
            }

            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => state.modifiers = *m,

            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) if state.focused => {
                match key.as_ref() {
                    keyboard::Key::Character(c) if modifiers.command() && c.eq_ignore_ascii_case("c") => {
                        if let Some(text) = state
                            .selection
                            .and_then(|sel| selected_text(&self.blocks, sel, self.citation.as_ref()))
                        {
                            clipboard.write(clipboard::Kind::Standard, text);
                            shell.capture_event();
                        }
                    }
                    keyboard::Key::Character(c) if modifiers.command() && c.eq_ignore_ascii_case("a") => {
                        if let Some(last) = self.blocks.len().checked_sub(1) {
                            state.selection = Some((
                                Pos { block: 0, offset: 0 },
                                Pos { block: last, offset: self.blocks[last].len() },
                            ));
                            shell.capture_event();
                            shell.request_redraw();
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::Escape) => {
                        if state.selection.take().is_some() {
                            shell.capture_event();
                            shell.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        if state.drag.is_none() {
            self.publish_selection(state, shell);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State<L>>();
        for laid in &state.cards {
            if let Some(stack) = laid.stack.as_ref() {
                let (prev, next) = stack.zones(layout.bounds());
                if cursor.is_over(prev) || cursor.is_over(next) {
                    return mouse::Interaction::Pointer;
                }
            }
        }
        for (spec, laid) in self.cards.iter().zip(&state.cards) {
            if !laid.hidden
                && cursor.is_over(laid.absolute(layout.bounds()))
                && let Some(card_tree) = state.card_trees.get(&spec.key)
                && let Some(element) = spec.element.built.as_ref()
            {
                return element.as_widget().mouse_interaction(
                    card_tree,
                    Layout::with_offset(laid.offset(layout.bounds()), &laid.node),
                    cursor,
                    viewport,
                    renderer,
                );
            }
        }
        if let Some((offset, rect)) = self.toolbar_geometry(state, layout.bounds(), viewport)
            && cursor.is_over(rect)
            && let (Some(toolbar), Some(node), Some(child)) =
                (self.toolbar.as_ref(), state.toolbar_node.as_ref(), tree.children.first())
        {
            return toolbar.as_widget().mouse_interaction(
                child,
                Layout::with_offset(offset, node),
                cursor,
                viewport,
                renderer,
            );
        }
        if state.drag.as_ref().is_some_and(|d| d.moved) {
            mouse::Interaction::Text
        } else if state.hovered.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

/// One visual line's worth of a span: horizontal extent plus the line's vertical metrics.
struct LineSegment {
    x: f32,
    w: f32,
    top: f32,
    height: f32,
    baseline: f32,
}

fn span_segments(para: &Para, span: usize) -> Vec<LineSegment> {
    let mut out = Vec::new();
    for run in para.buffer().layout_runs() {
        let mut extent: Option<(f32, f32)> = None;
        for g in run.glyphs.iter().filter(|g| g.metadata == span) {
            let (lo, hi) = extent.unwrap_or((g.x, g.x + g.w));
            extent = Some((lo.min(g.x), hi.max(g.x + g.w)));
        }
        if let Some((lo, hi)) = extent {
            out.push(LineSegment {
                x: lo,
                w: hi - lo,
                top: run.line_top,
                height: run.line_height,
                baseline: run.line_y,
            });
        }
    }
    out
}

fn distance(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

impl<'a, L, Message> From<ReaderText<'a, L, Message>> for Element<'a, Message>
where
    L: Clone + 'static,
    Message: 'a,
{
    fn from(w: ReaderText<'a, L, Message>) -> Self {
        Element::new(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::span;

    fn block(parts: &[(&str, bool, Option<u16>)], soft: bool) -> Block<()> {
        Block {
            spans: parts.iter().map(|(t, _, _)| span(t.to_string())).collect(),
            meta: parts.iter().map(|(_, c, v)| SpanMeta { copy: *c, verse: *v, number: false, dotted: false }).collect(),
            indent: 0.0,
            gap_before: 0.0,
            soft_break: soft,
        }
    }

    fn sample() -> Vec<Block<()>> {
        vec![
            block(&[("Heading", false, None)], false),
            block(
                &[
                    ("1 ", false, Some(1)),
                    ("In the beginning.", true, Some(1)),
                    ("†", false, Some(1)),
                    (" ", true, None),
                    ("2 ", false, Some(2)),
                    ("And the earth.", true, Some(2)),
                ],
                false,
            ),
            block(&[("3 ", false, Some(3)), ("Let there be light.", true, Some(3))], false),
        ]
    }

    /// The sample's block 1 with real link spans on the dagger (a footnote) and a word.
    fn linked() -> Vec<Block<u8>> {
        let mut b = Block {
            spans: vec![
                span("1 ".to_string()),
                span("In the beginning.".to_string()),
                span("†".to_string()).link(1u8),
                span(" ".to_string()),
                span("And the".to_string()).link(2u8),
                span(" earth.".to_string()),
            ],
            meta: [("1 ", false), ("In the beginning.", true), ("†", false), (" ", true), ("And the", true), (" earth.", true)]
                .iter()
                .map(|(_, c)| SpanMeta { copy: *c, verse: Some(1), number: false, dotted: false })
                .collect(),
            indent: 0.0,
            gap_before: 0.0,
            soft_break: false,
        };
        b.meta[0].number = true;
        vec![b]
    }

    #[test]
    fn a_link_knows_where_it_sits_in_the_canonical_text() {
        let blocks = linked();
        let pieces = canonical_pieces(&blocks);
        let slice = |start, end| Some(CanonSlice { verse: 1, start, end });
        // The dagger has no text of its own: it is the last character before it.
        assert_eq!(link_slice(&pieces, &blocks[0], 0, 2), slice(16, 17));
        // A word is itself. (The space between the runs is canonical text too.)
        assert_eq!(link_slice(&pieces, &blocks[0], 0, 4), slice(18, 25));
    }

    #[test]
    fn a_card_finds_the_link_it_belongs_to() {
        let blocks = linked();
        let widget: ReaderText<'_, u8, ()> = ReaderText::new(1, blocks);
        let pieces = canonical_pieces(&widget.blocks);
        let at = |start, end| widget.link_span_at(&pieces, CanonSlice { verse: 1, start, end });
        assert_eq!(at(16, 17), Some((0, 2)), "a footnote card finds its dagger");
        assert_eq!(at(18, 25), Some((0, 4)), "a dictionary card finds its word");
        assert_eq!(at(0, 3), None, "plain text has no link");
        assert_eq!(widget.link_span_at(&pieces, CanonSlice { verse: 9, start: 16, end: 17 }), None);
    }

    const CITE: Citation = Citation { chapter: String::new(), module: String::new() };

    fn cite() -> Citation {
        Citation { chapter: "Gen 1".into(), ..CITE }
    }

    #[test]
    fn word_bounds_cases() {
        let t = "He said, don't go.";
        assert_eq!(&t[word_bounds(t, 1).0..word_bounds(t, 1).1], "He");
        assert_eq!(&t[word_bounds(t, 10).0..word_bounds(t, 10).1], "don't");
        // end of word (pointer just after it) still picks that word
        assert_eq!(&t[word_bounds(t, 2).0..word_bounds(t, 2).1], "He");
        // caret right after a word still picks the word, not the comma that follows
        assert_eq!(&t[word_bounds(t, 7).0..word_bounds(t, 7).1], "said");
        // punctuation with no adjacent word selects itself
        let p = "a -- b";
        assert_eq!(&p[word_bounds(p, 3).0..word_bounds(p, 3).1], "-");
        assert_eq!(word_bounds("", 0), (0, 0));
        let g = "ἀγαπάω καί";
        let (s, e) = word_bounds(g, 3);
        assert_eq!(&g[s..e], "ἀγαπάω");
    }

    #[test]
    fn copy_skips_numbers_markers_and_headings() {
        let blocks = sample();
        let all = (Pos { block: 0, offset: 0 }, Pos { block: 2, offset: blocks[2].len() });
        let text = selected_text(&blocks, all, Some(&cite())).unwrap();
        assert_eq!(
            text,
            "In the beginning. And the earth.\n\nLet there be light.\n— Gen 1:1-3 ()"
        );
    }

    #[test]
    fn partial_selection_and_reversed_anchor() {
        let blocks = sample();
        // "beginning." only (offsets within block 1: "1 " is 2 bytes, then text)
        let s = Pos { block: 1, offset: 2 + 7 };
        let e = Pos { block: 1, offset: 2 + 17 };
        assert_eq!(selected_text(&blocks, (e, s), None).unwrap(), "beginning.");
        assert_eq!(selected_text(&blocks, (s, s), None), None);
    }

    #[test]
    fn poetry_lines_join_with_single_newline() {
        let blocks = vec![
            block(&[("Line one", true, Some(1))], false),
            block(&[("line two", true, Some(1))], true),
        ];
        let all = (Pos { block: 0, offset: 0 }, Pos { block: 1, offset: 8 });
        assert_eq!(selected_text(&blocks, all, None).unwrap(), "Line one\nline two");
    }

    #[test]
    fn verse_selection_spans_blocks() {
        let blocks = vec![
            block(&[("1 ", false, Some(1)), ("first", true, Some(1))], false),
            block(&[("cont", true, Some(1))], true),
            block(&[("2 ", false, Some(2)), ("second", true, Some(2))], false),
        ];
        let (a, z) = verse_selection(&blocks, 1).unwrap();
        assert_eq!((a.block, a.offset, z.block, z.offset), (0, 0, 1, 4));
        assert_eq!(verse_at(&blocks[1], 2), Some(1));
        assert_eq!(verse_selection(&blocks, 9), None);
    }

    #[test]
    fn edge_scroll_zones() {
        let vp = Rectangle::new(Point::new(0.0, 500.0), Size::new(600.0, 400.0));
        assert_eq!(edge_scroll(700.0, &vp), 0.0);
        assert!(edge_scroll(505.0, &vp) < 0.0);
        assert!(edge_scroll(895.0, &vp) > 0.0);
        // farther past the edge scrolls faster, and speed is capped
        assert!(edge_scroll(300.0, &vp) < edge_scroll(480.0, &vp));
        assert_eq!(edge_scroll(-5000.0, &vp), edge_scroll(0.0, &vp));
    }

    #[test]
    fn pieces_follow_the_canonical_text_across_blocks() {
        // Verse 1 is split over two poetry lines; a footnote marker sits mid-run.
        let blocks = vec![
            block(&[("1 ", false, Some(1)), ("The Lord", true, Some(1)), ("†", false, Some(1)), (" is my shepherd;", true, Some(1))], false),
            block(&[("I shall not want.", true, Some(1))], true),
            block(&[("2 ", false, Some(2)), ("He maketh me", true, Some(2))], false),
        ];
        let canon = canonical_text(&blocks);
        assert_eq!(canon[&1], "The Lord is my shepherd; I shall not want.");
        assert_eq!(canon[&2], "He maketh me");

        let pieces = canonical_pieces(&blocks);
        // "shepherd; I": crosses the line break (and the separator) in canonical terms.
        let at = canon[&1].find("shepherd").unwrap();
        let ranges = block_ranges(&pieces, CanonSlice { verse: 1, start: at, end: at + 11 });
        assert_eq!(ranges.len(), 2);
        let (b0, s0, e0) = ranges[0];
        assert_eq!(&blocks[b0].text()[s0..e0], "shepherd;");
        let (b1, s1, e1) = ranges[1];
        assert_eq!(&blocks[b1].text()[s1..e1], "I");
    }

    #[test]
    fn selection_maps_to_canonical_slices() {
        let blocks = sample();
        let pieces = canonical_pieces(&blocks);
        // Everything: heading ignored, verse 1 text, verse 2 text, verse 3 text.
        let all = (Pos { block: 0, offset: 0 }, Pos { block: 2, offset: blocks[2].len() });
        let slices = selection_slices(&pieces, &blocks, all);
        assert_eq!(
            slices,
            [
                CanonSlice { verse: 1, start: 0, end: "In the beginning.".len() },
                CanonSlice { verse: 2, start: 0, end: "And the earth.".len() },
                CanonSlice { verse: 3, start: 0, end: "Let there be light.".len() },
            ]
        );
        // "beginning." only, selected backwards.
        let s = Pos { block: 1, offset: 2 + 7 };
        let e = Pos { block: 1, offset: 2 + 17 };
        let part = selection_slices(&pieces, &blocks, ordered((e, s)));
        assert_eq!(part, [CanonSlice { verse: 1, start: 7, end: 17 }]);
        // Only a verse number selected: nothing.
        let num = (Pos { block: 1, offset: 0 }, Pos { block: 1, offset: 2 });
        assert!(selection_slices(&pieces, &blocks, num).is_empty());
    }

    #[test]
    fn drag_after_double_click_extends_by_whole_words() {
        let blocks = vec![block(&[("Jesus answered and said unto him", true, Some(1))], false)];
        let at = |offset| Pos { block: 0, offset };
        let text = blocks[0].text();
        // Double-click on "answered" (6..14), then drag into "unto" (24..28).
        let anchor_sel = Some((at(6), at(14)));
        let sel = extended_selection(&blocks, 2, at(9), anchor_sel, at(26)).unwrap();
        assert_eq!(&text[sel.0.offset..sel.1.offset], "answered and said unto");
        // Dragging backwards from the same word extends leftwards.
        let back = extended_selection(&blocks, 2, at(9), anchor_sel, at(2)).unwrap();
        assert_eq!(&text[back.0.offset..back.1.offset], "Jesus answered");
        // Staying inside the word keeps just that word.
        let same = extended_selection(&blocks, 2, at(9), anchor_sel, at(10)).unwrap();
        assert_eq!(&text[same.0.offset..same.1.offset], "answered");
    }

    #[test]
    fn drag_after_triple_click_extends_by_whole_verses() {
        let blocks = vec![
            block(&[("1 ", false, Some(1)), ("first verse", true, Some(1))], false),
            block(&[("2 ", false, Some(2)), ("second verse", true, Some(2))], false),
            block(&[("3 ", false, Some(3)), ("third verse", true, Some(3))], false),
        ];
        let at = |block, offset| Pos { block, offset };
        let anchor_sel = verse_selection(&blocks, 1);
        let sel = extended_selection(&blocks, 3, at(0, 5), anchor_sel, at(2, 4)).unwrap();
        assert_eq!(selection_slices(&canonical_pieces(&blocks), &blocks, sel)
            .iter().map(|s| s.verse).collect::<Vec<_>>(), [1, 2, 3]);
        // Dragging only within verse 1 keeps verse 1.
        let one = extended_selection(&blocks, 3, at(0, 5), anchor_sel, at(0, 7)).unwrap();
        assert_eq!(selection_slices(&canonical_pieces(&blocks), &blocks, one)
            .iter().map(|s| s.verse).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn toolbar_sits_under_the_last_line_and_stays_visible() {
        let r = |x, y, w, h| Rectangle::new(Point::new(x, y), Size::new(w, h));
        let size = Size::new(200.0, 40.0);
        let visible = r(0.0, 100.0, 600.0, 400.0);
        // Under the last line, centred on it.
        let p = place_toolbar(r(50.0, 200.0, 100.0, 28.0), r(250.0, 260.0, 100.0, 28.0), size, visible, 600.0);
        assert_eq!(p, Point::new(200.0, 296.0));
        // Near the right edge: kept inside the widget.
        let p = place_toolbar(r(0.0, 200.0, 10.0, 28.0), r(590.0, 260.0, 10.0, 28.0), size, visible, 600.0);
        assert_eq!(p.x, 400.0);
        // No room below the visible area: flips above the first line.
        let p = place_toolbar(r(100.0, 300.0, 50.0, 28.0), r(100.0, 450.0, 50.0, 28.0), size, visible, 600.0);
        assert_eq!(p.y, 300.0 - 40.0 - 8.0);
        // Selection scrolled out of view above: the toolbar is pulled to the visible edge.
        let p = place_toolbar(r(100.0, -500.0, 50.0, 28.0), r(100.0, -400.0, 50.0, 28.0), size, visible, 600.0);
        assert_eq!(p.y, 100.0);
    }

    #[test]
    fn verse_number_click_selects_toggles_and_extends() {
        let blocks = vec![
            block(&[("1 ", false, Some(1)), ("first", true, Some(1))], false),
            block(&[("2 ", false, Some(2)), ("second", true, Some(2))], false),
            block(&[("3 ", false, Some(3)), ("third", true, Some(3))], false),
        ];
        let v = |n, shift, cur| verse_number_selection(&blocks, n, shift, cur);
        let one = v(1, false, None).unwrap();
        assert_eq!(selection_slices(&canonical_pieces(&blocks), &blocks, one)[0].verse, 1);
        // Clicking the same verse again clears; another verse replaces.
        assert_eq!(v(1, false, Some(one)), None);
        let two = v(2, false, Some(one)).unwrap();
        assert_eq!(selection_slices(&canonical_pieces(&blocks), &blocks, two)[0].verse, 2);
        // Shift grows the selection over everything between, in either direction.
        let grown = v(3, true, Some(one)).unwrap();
        let verses: Vec<u16> = selection_slices(&canonical_pieces(&blocks), &blocks, grown).iter().map(|s| s.verse).collect();
        assert_eq!(verses, [1, 2, 3]);
        let back = v(1, true, Some(v(3, false, None).unwrap())).unwrap();
        let verses: Vec<u16> = selection_slices(&canonical_pieces(&blocks), &blocks, back).iter().map(|s| s.verse).collect();
        assert_eq!(verses, [1, 2, 3]);
        assert_eq!(verse_number_selection(&blocks, 9, false, None), None);
    }

    #[test]
    fn marks_at_finds_every_overlapping_mark_under_a_character() {
        let blocks = vec![block(&[("1 ", false, Some(1)), ("so loved the world", true, Some(1))], false)];
        let pieces = canonical_pieces(&blocks);
        let draw = |mark, start, end| MarkDraw {
            mark,
            slice: CanonSlice { verse: 1, start, end },
            kind: MarkKind::Highlight,
            color: Color::WHITE,
        };
        // "so loved" (0..8) and "loved the" (3..12) overlap on "loved".
        let marks = [draw(7, 0, 8), draw(9, 3, 12)];
        let at = |offset: usize| Pos { block: 0, offset: 2 + offset }; // the verse number takes 2 bytes
        assert_eq!(marks_at(&pieces, &marks, at(1)), [7]);
        assert_eq!(marks_at(&pieces, &marks, at(4)), [7, 9]);
        assert_eq!(marks_at(&pieces, &marks, at(10)), [9]);
        assert!(marks_at(&pieces, &marks, at(14)).is_empty());
        // Clicking on the verse number is not a click on the mark.
        assert!(marks_at(&pieces, &marks, Pos { block: 0, offset: 0 }).is_empty());
    }

    #[test]
    fn the_arrows_of_a_stack_go_round_in_both_directions() {
        let stack = StackBar {
            id: 1,
            keys: vec![10, 11, 12],
            colors: vec![Color::WHITE; 3],
            at: 0,
            bar: Rectangle::new(Point::ORIGIN, Size::new(300.0, 18.0)),
        };
        assert_eq!((stack.step(true), stack.step(false)), (11, 12), "forward from the first; back wraps to the last");
        let last = StackBar { at: 2, ..stack.clone() };
        assert_eq!((last.step(true), last.step(false)), (10, 11));
        // The two places to press are either side of the counter, inside the bar.
        let (prev, next) = stack.zones(Rectangle::new(Point::new(100.0, 50.0), Size::new(400.0, 400.0)));
        assert!(prev.x + prev.width < next.x);
        assert!(prev.y == 50.0 && next.y == 50.0 && prev.height == 18.0);
    }

    #[test]
    fn the_text_fills_the_width_when_there_is_no_margin() {
        // A hair too narrow: no margin, and the text expands into the space.
        assert_eq!(text_layout(640.0 + CARD_GAP + CARD_MIN - 1.0, Some(640.0), true), (false, 887.0, 0.0));
        assert_eq!(text_layout(500.0, Some(640.0), true), (false, 500.0, 0.0));
        // A reader that wants no margin at all uses the whole width.
        assert_eq!(text_layout(1000.0, None, true), (false, 1000.0, 0.0));
        // Wide, but nothing to show in a margin: the text takes the whole width.
        assert_eq!(text_layout(3000.0, Some(640.0), false), (false, 3000.0, 0.0));
    }

    #[test]
    fn a_margin_uses_the_room_it_is_given() {
        // Exactly enough for the narrowest useful margin: base text, cards as wide as fit.
        let (margin, text, card) = text_layout(640.0 + CARD_GAP + CARD_MIN, Some(640.0), true);
        assert!(margin && text == 640.0 && card == CARD_MIN);
        // With more room the margin takes 2/7 of the width: here 1400 → 400.
        let (_, text, card) = text_layout(1400.0, Some(640.0), true);
        assert!((card - 400.0).abs() < 0.01 && (text - (1400.0 - CARD_GAP - card)).abs() < 0.01);
        // It stops growing at its widest, and the text takes what is left.
        let (_, text, card) = text_layout(3000.0, Some(640.0), true);
        assert_eq!((text, card), (3000.0 - CARD_GAP - CARD_MAX, CARD_MAX));
        // Whatever the width, text + gap + card never exceed it.
        for w in (900..2500).step_by(37) {
            let (m, t, c) = text_layout(w as f32, Some(640.0), true);
            assert!(m && (t + CARD_GAP + c - w as f32).abs() < 0.01 && t >= 640.0 && c >= CARD_MIN, "{w}: {t} + {c}");
        }
    }

    #[test]
    fn cards_go_to_the_side_their_mark_is_nearer_but_not_all_to_one() {
        let at = |x: f32| Some(Rectangle::new(Point::new(x, 0.0), Size::new(10.0, 10.0)));
        assert_eq!(sides_by_nearness(&[at(10.0), at(500.0), at(20.0), at(480.0)], 255.0), vec![true, false, true, false]);
        // Five marks all left of the middle: the ones nearest the middle cross over.
        let crowded = sides_by_nearness(&[at(10.0), at(50.0), at(90.0), at(130.0), at(170.0)], 255.0);
        assert_eq!(crowded, vec![true, true, true, false, false]);
        assert_eq!(sides_by_nearness(&[None], 255.0), vec![false]);
    }

    #[test]
    fn margins_on_both_sides_share_the_width_and_need_room_for_each() {
        assert_eq!(text_layout_both(640.0 + 2.0 * (BOTH_GAP + CARD_MIN) - 1.0, Some(640.0)), None);
        assert_eq!(text_layout_both(3000.0, None), None);
        for w in [900.0_f32, 1100.0, 1400.0, 2000.0, 3000.0] {
            if let Some((left, text, card)) = text_layout_both(w, Some(640.0)) {
                assert!((left + text + BOTH_GAP + card - w).abs() < 0.01 && text >= 640.0 && card >= CARD_MIN, "{w}");
                assert!((left - (card + BOTH_GAP)).abs() < 0.01, "the same either side");
            }
        }
    }

    #[test]
    fn a_lone_connector_keeps_the_middle_of_the_gap_and_busy_ones_fan_out() {
        assert_eq!(lane_offset(0), 0.0);
        assert_eq!(lane_offset(1), LANE_STEP);
        assert_eq!(lane_offset(2), -LANE_STEP);
        assert_eq!(lane_offset(3), 2.0 * LANE_STEP);
        // Every lane stays inside the gap.
        for lane in 0..LANES {
            assert!(lane_offset(lane).abs() < CARD_GAP / 2.0, "lane {lane}");
        }
    }

    #[test]
    fn softened_colours_are_calmer_and_translucent() {
        let c = Color::from_rgb(0.9, 0.1, 0.1);
        let soft = softened(c);
        assert!((soft.a - 0.3).abs() < 1e-6);
        // Closer together on every channel: less saturated.
        assert!((soft.r - soft.g).abs() < (c.r - c.g).abs());
        assert!((soft.r - soft.b).abs() < (c.r - c.b).abs());
        // Grey stays grey.
        let g = softened(Color::from_rgb(0.5, 0.5, 0.5));
        assert!((g.r - 0.5).abs() < 1e-6 && (g.g - 0.5).abs() < 1e-6);
    }

    #[test]
    fn single_click_drag_is_a_plain_range() {
        let blocks = vec![block(&[("hello world", true, Some(1))], false)];
        let at = |offset| Pos { block: 0, offset };
        assert_eq!(extended_selection(&blocks, 1, at(3), None, at(8)), Some((at(3), at(8))));
        assert_eq!(extended_selection(&blocks, 1, at(3), None, at(3)), None);
    }

    #[test]
    fn range_in_block_clips() {
        let sel = (Pos { block: 1, offset: 4 }, Pos { block: 3, offset: 2 });
        assert_eq!(range_in_block(sel, 0, 10), None);
        assert_eq!(range_in_block(sel, 1, 10), Some((4, 10)));
        assert_eq!(range_in_block(sel, 2, 10), Some((0, 10)));
        assert_eq!(range_in_block(sel, 3, 10), Some((0, 2)));
        assert_eq!(range_in_block(sel, 4, 10), None);
    }
}
