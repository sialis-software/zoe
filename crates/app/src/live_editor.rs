//! The live-preview markdown editor widget (docs/06). It draws a `md_editor::Document`: each
//! source line is its own block, shaped as a paragraph. The line holding the caret shows its raw
//! markdown with the syntax dimmed; every other line is rendered (markers hidden, bullets and
//! checkboxes drawn, headings enlarged, `![[verse]]` lines shown as the passage). The text is
//! only ever the source string, so there is nothing to round-trip.
//!
//! The widget owns what needs layout: the caret and selection geometry, hit-testing, vertical
//! movement, scrolling, focus, and turning keys, the mouse and input methods into
//! `md_editor::Action`s. It does not edit anything itself; the app applies each action to its
//! `Document` and the next view reflects it.

use crate::fonts;
use crate::palette::Palette;
use iced::advanced::graphics::text::{Paragraph as Para, cosmic_text};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{
    Alignment, Difference, Hit, LineHeight, Paragraph as _, Renderer as _, Shaping, Span, Text, Wrapping,
};
use iced::advanced::widget::{Id, Operation, Tree, operation, tree};
use iced::advanced::{Clipboard, InputMethod, Renderer as _, Shell, Widget, clipboard, input_method, mouse};
use iced::{
    Color, Element, Event, Font, Length, Padding, Pixels, Point, Rectangle, Size, Theme, alignment, font, keyboard,
};
use md_editor::{Action, Deco, Document, Motion, Rendered, Reveal, Role, Wrap, render};
use std::borrow::Cow;
use std::ops::Range;
use std::time::{Duration, Instant};
use study_core::markdown::LinkTarget;

fn laid_deco_is_rule(rendered: &Rendered) -> bool {
    matches!(rendered.deco, Deco::Rule)
}

/// Space for a list marker (bullet, number, checkbox) left of a rendered item's text.
const GUTTER: f32 = 24.0;
/// Space for the quote bar left of a rendered quote's text.
const QUOTE_INDENT: f32 = 16.0;
/// Indent per list nesting level.
const LEVEL_INDENT: f32 = 22.0;
/// Padding inside a code line's background.
const CODE_PAD: f32 = 8.0;
/// A rendered line that is only hidden syntax (a code fence) keeps this much height.
const COLLAPSED_HEIGHT: f32 = 4.0;
const MULTI_CLICK: Duration = Duration::from_millis(400);
/// How long the caret stays on, then off, while the editor is focused and idle.
const BLINK: Duration = Duration::from_millis(500);

/// Whether the caret is showing `since_activity` after the last keypress, click or caret move.
/// It is solid right after activity (so it never disappears while you type) and then blinks.
fn caret_on(since_activity: Duration) -> bool {
    (since_activity.as_millis() / BLINK.as_millis()).is_multiple_of(2)
}

/// How long until the caret next turns on or off.
fn until_blink(since_activity: Duration) -> Duration {
    BLINK - Duration::from_millis((since_activity.as_millis() % BLINK.as_millis()) as u64)
}
/// Line height relative to the text size.
const LINE: f32 = 1.5;

/// What an embed line shows: the passage's reference and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbedContent {
    pub title: String,
    pub body: String,
}

type EmbedFn<'a> = dyn Fn(&LinkTarget) -> Option<EmbedContent> + 'a;

pub struct LiveEditor<'a, Message> {
    doc: Cow<'a, Document>,
    /// A rendered view that cannot be typed into (a margin card at rest): no caret, no focus,
    /// nothing revealed; links open on a plain click, a task's box ticks, and any other click
    /// asks the app (`on_press`) to start editing.
    read_only: bool,
    /// A plain click on a link that is showing as its label follows it, rather than placing
    /// the caret there. (The writer does this; a card being edited does not, since there it is
    /// obvious you are editing.) Cmd/Ctrl-click always follows.
    follow_links: bool,
    on_press: Option<Box<dyn Fn() -> Message + 'a>>,
    pal: Palette,
    size: Pixels,
    font: Font,
    padding: Padding,
    width: Length,
    height: Length,
    max_height: Option<f32>,
    id: Option<Id>,
    placeholder: Option<String>,
    on_action: Option<Box<dyn Fn(Action) -> Message + 'a>>,
    on_link: Option<Box<dyn Fn(LinkTarget) -> Message + 'a>>,
    /// A right click on a link: the link, and where the pointer is (in the window).
    on_link_menu: Option<Box<dyn Fn(LinkTarget, Point) -> Message + 'a>>,
    /// A link pressed and then dragged: the link, and where the pointer is (in the window).
    on_link_drag: Option<Box<dyn Fn(LinkTarget, Point) -> Message + 'a>>,
    embed: Option<Box<EmbedFn<'a>>>,
    /// A text that is read, selected and copied but not changed: nothing is revealed as raw
    /// source and there is no caret to see.
    reading: bool,
    /// What copying makes of the selected source text (the clipboard gets the result).
    copy_as: Option<fn(&str) -> String>,
}

pub fn live_editor<'a, Message>(doc: &'a Document, pal: Palette) -> LiveEditor<'a, Message> {
    editor(Cow::Borrowed(doc), pal, false)
}

/// The same rendering for a document that is only read: see [`LiveEditor::read_only`] fields.
pub fn live_view<'a, Message>(doc: Document, pal: Palette) -> LiveEditor<'a, Message> {
    editor(Cow::Owned(doc), pal, true)
}

fn editor<'a, Message>(doc: Cow<'a, Document>, pal: Palette, read_only: bool) -> LiveEditor<'a, Message> {
    LiveEditor {
        doc,
        read_only,
        follow_links: false,
        on_press: None,
        pal,
        size: Pixels(17.0),
        font: fonts::UI,
        padding: Padding::new(12.0),
        width: Length::Fill,
        height: Length::Fill,
        max_height: None,
        id: None,
        placeholder: None,
        on_action: None,
        on_link: None,
        on_link_menu: None,
        on_link_drag: None,
        embed: None,
        reading: false,
        copy_as: None,
    }
}

impl<'a, Message> LiveEditor<'a, Message> {
    /// Make a plain click follow links that are showing as their label. Option/Alt-click, the
    /// arrow keys, or a click on a link already showing its raw `[[...]]` edit it instead.
    /// Show the text as it is read: selectable and copyable, with no raw source revealed and no
    /// caret. The owner drops the actions that would change the text.
    pub fn reading(mut self, reading: bool) -> Self {
        self.reading = reading;
        self
    }

    /// What Copy puts on the clipboard for the selected source text.
    pub fn copy_as(mut self, f: fn(&str) -> String) -> Self {
        self.copy_as = Some(f);
        self
    }

    pub fn follow_links(mut self, follow: bool) -> Self {
        self.follow_links = follow;
        self
    }

    /// Called when a read-only view is clicked somewhere that is not a link or a checkbox.
    pub fn on_press(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    /// The family the text is set in.
    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Be only as wide as the text needs, up to the width on offer (a short comment in a card
    /// does not stretch the card). Embedded passages and rules still want the full width.
    pub fn fit_width(mut self) -> Self {
        self.width = Length::Shrink;
        self
    }

    /// Shrink to the text, up to this tall; beyond it the editor scrolls inside itself.
    pub fn max_height(mut self, max: f32) -> Self {
        self.height = Length::Shrink;
        self.max_height = Some(max);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    pub fn on_action(mut self, f: impl Fn(Action) -> Message + 'a) -> Self {
        self.on_action = Some(Box::new(f));
        self
    }

    /// Called when a link is followed (Cmd/Ctrl-click).
    pub fn on_link_drag(mut self, f: impl Fn(LinkTarget, Point) -> Message + 'a) -> Self {
        self.on_link_drag = Some(Box::new(f));
        self
    }

    pub fn on_link_menu(mut self, f: impl Fn(LinkTarget, Point) -> Message + 'a) -> Self {
        self.on_link_menu = Some(Box::new(f));
        self
    }

    pub fn on_link(mut self, f: impl Fn(LinkTarget) -> Message + 'a) -> Self {
        self.on_link = Some(Box::new(f));
        self
    }

    /// Supplies the passage shown for an `![[...]]` line.
    pub fn embed(mut self, f: impl Fn(&LinkTarget) -> Option<EmbedContent> + 'a) -> Self {
        self.embed = Some(Box::new(f));
        self
    }

    fn line_px(&self) -> f32 {
        self.size.0 * LINE
    }

    fn emit(&self, shell: &mut Shell<'_, Message>, action: Action) {
        if let Some(f) = &self.on_action {
            shell.publish(f(action));
        }
    }
}

// ---------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------

struct EmbedLaid {
    spans: Vec<Span<'static, ()>>,
    para: Para,
    /// From the top of the block.
    y: f32,
    h: f32,
    missing: bool,
    /// Where the passage comes from. Its title is a link to it.
    target: LinkTarget,
    /// Height of the title line, which is the clickable part.
    header_h: f32,
}

struct Laid {
    spans: Vec<Span<'static, ()>>,
    para: Para,
    rendered: Rendered,
    /// The source line.
    range: Range<usize>,
    x: f32,
    y: f32,
    h: f32,
    embed: Option<EmbedLaid>,
}

impl Laid {
    fn total_h(&self) -> f32 {
        self.embed.as_ref().map_or(self.h, |e| e.y + e.h)
    }
}

enum DragKind {
    Char,
    Word(Range<usize>),
    Line(Range<usize>),
}

struct Drag {
    kind: DragKind,
    last: usize,
    /// Where the pointer was pressed (window coordinates). A drag only begins once the pointer has
    /// actually moved from here: pressing can re-lay-out the line (its raw markdown appears), which
    /// moves text under a pointer that has not moved, and that must not select anything.
    origin: Option<Point>,
    moved: bool,
    /// A press on a link that has not yet become a drag or a click: if the pointer goes up
    /// without having moved, the link is followed; if it moves, this was a drag that happened
    /// to start on a link, and the caret goes where the press was.
    pending_link: Option<(LinkTarget, usize)>,
}

/// How far the pointer must travel before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 3.0;

struct Click {
    at: Instant,
    point: Point,
    count: u8,
}

#[derive(Default)]
struct State {
    focused: bool,
    scroll: f32,
    view_h: f32,
    content_h: f32,
    laid: Vec<Laid>,
    /// Remembered x for repeated Up/Down.
    goal_x: Option<f32>,
    drag: Option<Drag>,
    last_click: Option<Click>,
    modifiers: keyboard::Modifiers,
    /// Where the caret was when we last made sure it was in view.
    last_head: Option<usize>,
    preedit: Option<String>,
    /// The last moment something happened that should show the caret solid.
    activity: Option<Instant>,
}

impl State {
    /// Show the caret solid and restart its blink.
    fn touch(&mut self) {
        self.activity = Some(Instant::now());
    }

    fn caret_visible(&self) -> bool {
        self.focused && self.activity.is_none_or(|at| caret_on(at.elapsed()))
    }
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
        self.touch();
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

// ---------------------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------------------

fn role_color(role: Role, pal: &Palette) -> Option<Color> {
    match role {
        Role::Text => None,
        Role::Marker => Some(Color { a: 0.55, ..pal.muted }),
        Role::Muted => Some(pal.muted),
        Role::Heading => Some(pal.heading),
        Role::Code => Some(pal.code),
        Role::Quote => Some(pal.muted),
        Role::Tag => Some(pal.module_link),
        Role::LinkVerse | Role::LinkWeb => Some(pal.link),
        Role::LinkNote => Some(pal.note_link),
        Role::LinkModule => Some(pal.module_link),
    }
}

fn spans_for(rendered: &Rendered, base: f32, pal: &Palette, text_font: Font) -> Vec<Span<'static, ()>> {
    rendered
        .runs
        .iter()
        .map(|run| {
            let s = &run.style;
            let family = if s.mono { Font::MONOSPACE } else { text_font };
            let font = Font {
                weight: if s.bold { font::Weight::Bold } else { font::Weight::Normal },
                style: if s.italic { font::Style::Italic } else { font::Style::Normal },
                ..family
            };
            let mut span = iced::widget::span(run.text.clone()).size(base * s.scale).font(font);
            if let Some(color) = role_color(s.role, pal) {
                span = span.color(color);
            }
            span
        })
        .collect()
}

/// How far below the baseline underlines go on this line. It follows the line's largest text, not
/// each run's own size, so a smaller run (a module name after a reference) shares the same line
/// as the text beside it instead of riding above it.
fn underline_gap(rendered: &Rendered, base: f32) -> f32 {
    let scale = rendered.runs.iter().map(|r| r.style.scale).fold(0.0, f32::max);
    base * scale * 0.12
}

/// One visual line's slice of a run, for underlines and strike-throughs.
struct LineSegment {
    x: f32,
    w: f32,
    top: f32,
    height: f32,
    baseline: f32,
}

fn run_segments(para: &Para, run: usize) -> Vec<LineSegment> {
    let mut out = Vec::new();
    for line in para.buffer().layout_runs() {
        let mut extent: Option<(f32, f32)> = None;
        for g in line.glyphs.iter().filter(|g| g.metadata == run) {
            let (lo, hi) = extent.unwrap_or((g.x, g.x + g.w));
            extent = Some((lo.min(g.x), hi.max(g.x + g.w)));
        }
        if let Some((lo, hi)) = extent {
            out.push(LineSegment { x: lo, w: hi - lo, top: line.line_top, height: line.line_height, baseline: line.line_y });
        }
    }
    out
}

/// Caret position within a paragraph at `display` (a byte offset in its text): x, the top of
/// its visual line, and that line's height.
fn caret_in(para: &Para, display: usize, fallback_height: f32) -> (f32, f32, f32) {
    let mut last: Option<(f32, f32, f32)> = None;
    for run in para.buffer().layout_runs() {
        let (Some(first), Some(end)) = (run.glyphs.first(), run.glyphs.last()) else {
            last = Some((0.0, run.line_top, run.line_height));
            continue;
        };
        let (start_idx, end_idx) = (first.start.min(end.start), first.end.max(end.end));
        if display >= start_idx && display < end_idx {
            let glyph = run.glyphs.iter().find(|g| display >= g.start && display < g.end).unwrap_or(first);
            let span = (glyph.end - glyph.start).max(1) as f32;
            let into = (display - glyph.start) as f32 / span;
            return (glyph.x + glyph.w * into, run.line_top, run.line_height);
        }
        // Past this line's last glyph: its right edge (until a later line claims the offset).
        let right = run.glyphs.iter().map(|g| g.x + g.w).fold(0.0, f32::max);
        last = Some((right, run.line_top, run.line_height));
    }
    last.unwrap_or((0.0, 0.0, fallback_height))
}

// ---------------------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------------------

impl<Message> LiveEditor<'_, Message> {
    /// Source offset under `p` (in content coordinates: padding and scroll already removed).
    fn offset_at(&self, state: &State, p: Point) -> usize {
        let Some(last) = state.laid.len().checked_sub(1) else { return 0 };
        let b = state.laid.iter().position(|l| p.y < l.y + l.total_h().max(1.0)).unwrap_or(last);
        let l = &state.laid[b];
        let local = Point::new(p.x - l.x, (p.y - l.y).clamp(0.0, (l.h - 1.0).max(0.0)));
        let len = l.rendered.text.len();
        let display = match l.para.hit_test(local) {
            Some(Hit::CharOffset(i)) => i.min(len),
            None if p.x < l.x => 0,
            None => len,
        };
        l.rendered.display_to_source(display).clamp(l.range.start, l.range.end)
    }

    fn block_at(&self, state: &State, p: Point) -> Option<usize> {
        state.laid.iter().position(|l| p.y >= l.y && p.y < l.y + l.total_h())
    }

    /// The link under `p`, if the point is on a link's text, and whether that link is showing
    /// its raw `[[...]]` (the caret is in it) rather than its label.
    fn link_at(&self, state: &State, p: Point) -> Option<(LinkTarget, bool)> {
        let l = &state.laid[self.block_at(state, p)?];
        let local = Point::new(p.x - l.x, p.y - l.y);
        let run = l.para.hit_span(local)?;
        let style = &l.rendered.runs.get(run)?.style;
        Some((style.link.clone()?, style.raw))
    }

    /// The passage whose title line (the link to its source) is under `p`.
    fn embed_header_at(&self, state: &State, p: Point) -> Option<LinkTarget> {
        state.laid.iter().find_map(|l| {
            let e = l.embed.as_ref()?;
            let top = l.y + e.y;
            (p.y >= top && p.y < top + 4.0 + e.header_h && p.x >= l.x).then(|| e.target.clone())
        })
    }

    /// Caret rectangle for a source offset, in content coordinates.
    fn caret_rect(&self, state: &State, offset: usize) -> Option<Rectangle> {
        let b = self.doc.block_index_at(offset);
        let l = state.laid.get(b)?;
        let display = l.rendered.source_to_display(offset.clamp(l.range.start, l.range.end));
        let (x, top, height) = caret_in(&l.para, display, l.h);
        Some(Rectangle::new(Point::new(l.x + x, l.y + top), Size::new(1.5, height)))
    }

    /// Where Up or Down from the caret lands, keeping the horizontal position.
    fn vertical_target(&self, state: &mut State, up: bool) -> Option<usize> {
        let rect = self.caret_rect(state, self.doc.selection().head)?;
        let goal = *state.goal_x.get_or_insert(rect.x);
        let y = if up { rect.y - 2.0 } else { rect.y + rect.height + 2.0 };
        if y < 0.0 {
            return Some(0);
        }
        if y >= state.content_h - self.padding.y() {
            return Some(self.doc.source().len());
        }
        let head = self.doc.selection().head;
        let found = self.offset_at(state, Point::new(goal, y));
        if found != head {
            return Some(found);
        }
        // The step landed in the same block: the space above a heading (or below it) is bigger
        // than the step. Go on to the block before or after that has a line to land on (the
        // properties at the top are blocks with no height), at the end or start of it.
        let at = self.doc.block_index_at(head);
        let beside = |l: &&Laid| l.total_h() > 0.0;
        let target = if up {
            state.laid.get(..at).and_then(|before| before.iter().rev().find(beside)).map(|l| l.y + (l.h - 1.0).max(0.0))
        } else {
            state.laid.get(at + 1..).and_then(|after| after.iter().find(beside)).map(|l| l.y + 1.0)
        };
        match target {
            Some(y) => Some(self.offset_at(state, Point::new(goal, y))),
            None if up => Some(0),
            None => Some(self.doc.source().len()),
        }
    }

    fn content_point(&self, state: &State, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Point> {
        let p = cursor.position_in(bounds)?;
        Some(Point::new(p.x - self.padding.left, p.y - self.padding.top + state.scroll))
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for LiveEditor<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let width = limits.max().width;
        let inner_w = (width - self.padding.x()).max(1.0);
        let font = renderer.default_font();
        let line_px = self.line_px();
        let source = self.doc.source();
        let blocks = self.doc.blocks();
        state.laid.truncate(blocks.len());

        // The frontmatter (and the blank line under it) of a locked document is not shown, and takes
        // no room: the text starts right where the properties end.
        let hidden = self.doc.hidden_blocks();
        let mut y = 0.0;
        // The widest block, for `fit_width`.
        let mut widest = 0.0_f32;
        for (i, block) in blocks.iter().enumerate() {
            let reveal = if self.read_only || self.reading { Reveal::none() } else { self.doc.reveal(i) };
            let rendered = render(source, block, &reveal);
            if hidden == 0 || i > hidden {
                y += rendered.gap_before * line_px;
            }

            let indent = rendered.level as f32 * LEVEL_INDENT
                + match rendered.deco {
                    Deco::Bullet | Deco::Number(_) | Deco::Checkbox(_) => GUTTER,
                    Deco::QuoteBar => QUOTE_INDENT,
                    Deco::CodeBg => CODE_PAD,
                    _ => 0.0,
                };
            let w = (inner_w - indent).max(1.0);
            let spans = spans_for(&rendered, self.size.0, &self.pal, self.font);
            let shape = Text {
                content: (),
                bounds: Size::new(w, f32::INFINITY),
                size: self.size,
                line_height: LineHeight::Relative(LINE),
                font,
                align_x: Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::default(),
            };

            let mut para = state.laid.get_mut(i).map(|l| std::mem::take(&mut l.para)).unwrap_or_default();
            let unchanged = state.laid.get(i).is_some_and(|l| l.spans == spans);
            if spans.is_empty() {
                para = Para::default();
            } else if !unchanged {
                para = Para::with_spans(shape.with_content(spans.as_slice()));
            } else {
                match para.compare(shape) {
                    Difference::None => {}
                    Difference::Bounds => para.resize(shape.bounds),
                    Difference::Shape => para = Para::with_spans(shape.with_content(spans.as_slice())),
                }
            }

            let scale = rendered.runs.first().map_or(1.0, |r| r.style.scale);
            let h = match (&rendered.deco, rendered.text.is_empty()) {
                _ if i < hidden => 0.0,
                (Deco::Collapsed, _) => COLLAPSED_HEIGHT,
                (Deco::Rule, true) => line_px * 0.7,
                (Deco::Embed(_), true) => 0.0,
                (_, true) => line_px * scale.max(1.0),
                _ => para.min_height().max(line_px * 0.5),
            };

            let embed = match &rendered.deco {
                Deco::Embed(target) => Some(self.layout_embed(state.laid.get_mut(i).and_then(|l| l.embed.take()), target, w, h, font)),
                _ => None,
            };

            widest = widest.max(match (&embed, &laid_deco_is_rule(&rendered)) {
                (Some(_), _) | (_, true) => inner_w,
                // Text that wrapped fills the width, so wrapped cards line up.
                _ if para.buffer().layout_runs().count() > 1 => inner_w,
                _ => indent + para.min_width(),
            });
            let laid = Laid { spans, para, rendered, range: block.range.clone(), x: indent, y, h, embed };
            y += laid.total_h();
            match state.laid.get_mut(i) {
                Some(slot) => *slot = laid,
                None => state.laid.push(laid),
            }
        }

        let content_h = y + self.padding.y();
        state.content_h = content_h;
        let view_h = match self.height {
            Length::Fixed(h) => h,
            Length::Shrink => content_h.min(self.max_height.unwrap_or(f32::INFINITY)).min(limits.max().height),
            _ if limits.max().height.is_finite() => limits.max().height,
            _ => content_h,
        };
        state.view_h = view_h;
        state.scroll = state.scroll.clamp(0.0, (content_h - view_h).max(0.0));

        // Keep the caret in view when it moves (typing, arrows), but leave the scroll alone
        // when something else (the wheel) moved the view.
        let head = self.doc.selection().head;
        if !self.read_only && state.last_head != Some(head) {
            state.last_head = Some(head);
            state.touch();
            if let Some(rect) = self.caret_rect(state, head) {
                let (top, bottom) = (rect.y + self.padding.top, rect.y + rect.height + self.padding.top);
                if self.reading {
                    // A text that is read goes to the place asked for with it at the top.
                    state.scroll = (top - self.padding.top).max(0.0);
                } else if top < state.scroll {
                    state.scroll = top;
                } else if bottom > state.scroll + view_h {
                    state.scroll = bottom - view_h;
                }
                state.scroll = state.scroll.clamp(0.0, (content_h - view_h).max(0.0));
            }
        }
        let width = if self.width == Length::Shrink {
            (widest + self.padding.x()).ceil().clamp(1.0, width)
        } else {
            width
        };
        layout::Node::new(Size::new(width, view_h))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        // Only what is inside the visible area is drawn (this may sit in a scrolled, clipped card).
        let Some(clip) = bounds.intersection(viewport) else { return };
        let pal = &self.pal;
        let line_px = self.line_px();
        let origin = Point::new(bounds.x + self.padding.left, bounds.y + self.padding.top - state.scroll);
        let inner_w = (bounds.width - self.padding.x()).max(1.0);
        let sel = self.doc.selection();
        let revealed = self.doc.revealed_block();
        let quad = |renderer: &mut iced::Renderer, r: Rectangle, color: Color, radius: f32| {
            renderer.fill_quad(
                Quad { bounds: r, border: iced::Border { radius: radius.into(), ..Default::default() }, ..Quad::default() },
                color,
            );
        };
        let text_color = style.text_color;

        renderer.with_layer(clip, |renderer| {
            if self.doc.source().is_empty()
                && let Some(placeholder) = &self.placeholder
            {
                renderer.fill_text(
                    iced::advanced::text::Text {
                        content: placeholder.clone(),
                        bounds: Size::new(inner_w, line_px),
                        size: self.size,
                        line_height: LineHeight::Relative(LINE),
                        font: self.font,
                        align_x: Alignment::Default,
                        align_y: alignment::Vertical::Top,
                        shaping: Shaping::Basic,
                        wrapping: Wrapping::default(),
                    },
                    Point::new(origin.x, origin.y),
                    Color { a: 0.5, ..pal.muted },
                    bounds,
                );
            }

            for (i, l) in state.laid.iter().enumerate() {
                let top = origin.y + l.y;
                if top + l.total_h() < bounds.y || top > bounds.y + bounds.height {
                    continue;
                }
                let left = origin.x + l.x;

                // Decorations that sit behind or beside the text.
                match &l.rendered.deco {
                    Deco::CodeBg => quad(renderer, Rectangle::new(Point::new(origin.x, top), Size::new(inner_w, l.h)), pal.block_bg, 3.0),
                    Deco::QuoteBar => quad(renderer, Rectangle::new(Point::new(left - QUOTE_INDENT + 2.0, top), Size::new(3.0, l.h)), pal.quote_bar, 1.5),
                    Deco::Rule => quad(renderer, Rectangle::new(Point::new(origin.x, top + l.h / 2.0), Size::new(inner_w, 1.0)), Color { a: 0.5, ..pal.muted }, 0.0),
                    Deco::Bullet => quad(renderer, Rectangle::new(Point::new(left - GUTTER + 7.0, top + line_px / 2.0 - 3.0), Size::new(6.0, 6.0)), Color { a: 0.8, ..pal.muted }, 3.0),
                    Deco::Checkbox(done) => {
                        let r = Rectangle::new(Point::new(left - GUTTER + 4.0, top + line_px / 2.0 - 7.0), Size::new(14.0, 14.0));
                        if *done {
                            quad(renderer, r, pal.note_link, 3.0);
                            quad(renderer, Rectangle::new(Point::new(r.x + 4.0, r.y + 4.0), Size::new(6.0, 6.0)), Color { a: 0.9, ..Color::WHITE }, 1.5);
                        } else {
                            renderer.fill_quad(
                                Quad {
                                    bounds: r,
                                    border: iced::Border { color: Color { a: 0.8, ..pal.muted }, width: 1.5, radius: 3.0.into() },
                                    ..Quad::default()
                                },
                                Color::TRANSPARENT,
                            );
                        }
                    }
                    Deco::Number(n) => renderer.fill_text(
                        iced::advanced::text::Text {
                            content: n.clone(),
                            bounds: Size::new(GUTTER, line_px),
                            size: self.size,
                            line_height: LineHeight::Relative(LINE),
                            font: self.font,
                            align_x: Alignment::Default,
                            align_y: alignment::Vertical::Top,
                            shaping: Shaping::Basic,
                            wrapping: Wrapping::default(),
                        },
                        Point::new(left - GUTTER + 2.0, top),
                        Color { a: 0.8, ..pal.muted },
                        bounds,
                    ),
                    _ => {}
                }

                // Selection, from the cosmic-text geometry of the display range.
                let s = sel.range();
                let (lo, hi) = (s.start.max(l.range.start), s.end.min(l.range.end));
                if lo < hi {
                    let (d0, d1) = (l.rendered.source_to_display(lo), l.rendered.source_to_display(hi));
                    if d0 < d1 {
                        let (c0, c1) = (cosmic_text::Cursor::new(0, d0), cosmic_text::Cursor::new(0, d1));
                        for run in l.para.buffer().layout_runs() {
                            if let Some((x, w)) = run.highlight(c0, c1) {
                                quad(
                                    renderer,
                                    Rectangle::new(Point::new(left + x, top + run.line_top), Size::new(w, run.line_height)),
                                    pal.text_selection,
                                    0.0,
                                );
                            }
                        }
                    }
                }

                if !l.rendered.text.is_empty() {
                    renderer.fill_paragraph(&l.para, Point::new(left, top), text_color, bounds);
                    // Underlines and strike-throughs, from the runs' real line geometry.
                    let underline_gap = underline_gap(&l.rendered, self.size.0);
                    for (j, run) in l.rendered.runs.iter().enumerate() {
                        if !(run.style.underline || run.style.strike) {
                            continue;
                        }
                        let color = role_color(run.style.role, pal).unwrap_or(text_color);
                        for seg in run_segments(&l.para, j) {
                            if run.style.underline {
                                let at = top + seg.baseline + underline_gap;
                                quad(renderer, Rectangle::new(Point::new(left + seg.x, at), Size::new(seg.w, 1.0)), color, 0.0);
                            }
                            if run.style.strike {
                                let at = top + seg.top + seg.height * 0.55;
                                quad(renderer, Rectangle::new(Point::new(left + seg.x, at), Size::new(seg.w, 1.0)), color, 0.0);
                            }
                        }
                    }
                }

                // An embedded passage, as a quote under (or instead of) the raw line.
                if let Some(e) = &l.embed {
                    let ex = left + 14.0;
                    let ey = top + e.y;
                    quad(renderer, Rectangle::new(Point::new(left, ey), Size::new(inner_w - l.x, e.h)), pal.block_bg, 4.0);
                    quad(renderer, Rectangle::new(Point::new(left, ey), Size::new(3.0, e.h)), if e.missing { pal.muted } else { pal.link }, 1.5);
                    renderer.fill_paragraph(&e.para, Point::new(ex, ey + 4.0), text_color, bounds);
                    for seg in run_segments(&e.para, 0) {
                        let at = ey + 4.0 + seg.baseline + self.size.0 * 0.82 * 0.12;
                        quad(renderer, Rectangle::new(Point::new(ex + seg.x, at), Size::new(seg.w, 1.0)), pal.link, 0.0);
                    }
                }

                let _ = i;
            }

            // The caret, in the one line that is showing its raw source.
            if !self.read_only
                && !self.reading
                && state.caret_visible()
                && revealed.is_some()
                && sel.is_empty()
                && let Some(rect) = self.caret_rect(state, sel.head)
            {
                let at = Point::new(origin.x + rect.x, origin.y + rect.y);
                quad(renderer, Rectangle::new(at, Size::new(1.5, rect.height)), text_color, 0.0);
            }

            // A slim scrollbar when there is more than fits.
            if state.content_h > state.view_h + 0.5 {
                let ratio = state.view_h / state.content_h;
                let bar_h = (state.view_h * ratio).max(20.0);
                let track = state.view_h - bar_h;
                let at = state.scroll / (state.content_h - state.view_h) * track;
                quad(
                    renderer,
                    Rectangle::new(Point::new(bounds.x + bounds.width - 5.0, bounds.y + at), Size::new(3.0, bar_h)),
                    Color { a: 0.35, ..pal.muted },
                    1.5,
                );
            }
        });
        let _ = cursor;
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
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        if let Event::Keyboard(keyboard::Event::ModifiersChanged(m)) = event {
            state.modifiers = *m;
        }

        // Tell the input method where the caret is, so candidate windows sit beside it.
        if !self.read_only
            && let Event::Window(iced::window::Event::RedrawRequested(now)) = event
        {
            // Wake up again when the caret next changes, for as long as we have the keyboard.
            if state.focused {
                let since = state.activity.map_or(Duration::ZERO, |at| now.saturating_duration_since(at));
                shell.request_redraw_at(*now + until_blink(since));
            }
            let method = match (state.focused, self.caret_rect(state, self.doc.selection().head)) {
                (true, Some(rect)) => InputMethod::Enabled {
                    cursor: Rectangle::new(
                        Point::new(
                            bounds.x + self.padding.left + rect.x,
                            bounds.y + self.padding.top + rect.y - state.scroll,
                        ),
                        Size::new(1.0, rect.height),
                    ),
                    purpose: input_method::Purpose::Normal,
                    preedit: state.preedit.as_deref().map(|content| input_method::Preedit {
                        content,
                        selection: None,
                        text_size: Some(self.size),
                    }),
                },
                _ => InputMethod::Disabled,
            };
            shell.request_input_method(&method);
        }

        if shell.is_event_captured() {
            return;
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                // A link, or the title of an embedded passage, offers where to open it.
                let (Some(f), Some(p), Some(at)) =
                    (&self.on_link_menu, self.content_point(state, bounds, cursor), cursor.position())
                else {
                    return;
                };
                let target = self
                    .embed_header_at(state, p)
                    .or_else(|| self.link_at(state, p).filter(|(_, raw)| !raw).map(|(t, _)| t));
                if let Some(target) = target {
                    shell.publish(f(target, at));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if self.read_only => {
                let Some(p) = self.content_point(state, bounds, cursor) else { return };
                shell.capture_event();
                let pressed = self.embed_header_at(state, p).or_else(|| self.link_at(state, p).map(|(t, _)| t));
                if let Some(target) = pressed {
                    if self.on_link_drag.is_some() {
                        // It follows when the button comes up, unless the pointer was dragged away
                        // with the link first.
                        state.drag = Some(Drag {
                            kind: DragKind::Char,
                            last: 0,
                            origin: cursor.position(),
                            moved: false,
                            pending_link: Some((target, 0)),
                        });
                    } else if let Some(f) = &self.on_link {
                        shell.publish(f(target));
                    }
                    return;
                }
                if let Some(b) = self.block_at(state, p) {
                    let l = &state.laid[b];
                    if matches!(l.rendered.deco, Deco::Checkbox(_)) && p.x >= l.x - GUTTER && p.x < l.x {
                        self.emit(shell, Action::ToggleTask(l.range.start));
                        return;
                    }
                }
                if let Some(f) = &self.on_press {
                    shell.publish(f());
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y * self.line_px() * 3.0,
                    mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                let max = (state.content_h - state.view_h).max(0.0);
                let next = (state.scroll - dy).clamp(0.0, max);
                if (next - state.scroll).abs() > f32::EPSILON {
                    state.scroll = next;
                    shell.capture_event();
                    shell.request_redraw();
                }
            }

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if !self.read_only => {
                let Some(p) = self.content_point(state, bounds, cursor) else {
                    state.focused = false;
                    state.drag = None;
                    return;
                };
                state.focused = true;
                state.touch();
                shell.capture_event();
                shell.request_redraw();

                // The title of an embedded passage links to its source; a task's box ticks. Both
                // are buttons, not text: every click on one acts, and none starts a selection
                // (a second quick click on a box must not select the word beside it).
                if let Some(target) = self.embed_header_at(state, p) {
                    state.last_click = None;
                    if let Some(f) = &self.on_link {
                        shell.publish(f(target));
                    }
                    return;
                }
                if let Some(b) = self.block_at(state, p) {
                    let l = &state.laid[b];
                    if matches!(l.rendered.deco, Deco::Checkbox(_)) && p.x >= l.x - GUTTER && p.x < l.x {
                        state.last_click = None;
                        self.emit(shell, Action::ToggleTask(l.range.start));
                        return;
                    }
                }

                let offset = self.offset_at(state, p);
                let count = match &state.last_click {
                    Some(c) if c.at.elapsed() < MULTI_CLICK && distance(c.point, p) < 5.0 => {
                        if c.count >= 3 { 1 } else { c.count + 1 }
                    }
                    _ => 1,
                };
                state.last_click = Some(Click { at: Instant::now(), point: p, count });
                state.goal_x = None;

                // Cmd/Ctrl-click follows a link; a click on a task's box ticks it.
                let link = self.link_at(state, p);
                if count == 1
                    && (state.modifiers.command() || state.modifiers.control())
                    && let Some((target, _)) = link.clone()
                {
                    if let Some(f) = &self.on_link {
                        shell.publish(f(target));
                    }
                    return;
                }
                // A plain click on a link showing as its label follows it, once the button comes
                // up without the pointer having moved (so a drag that starts on a link still
                // selects). Alt, Shift, and links showing their raw source mean editing instead.
                if count == 1
                    && self.follow_links
                    && !state.modifiers.alt()
                    && !state.modifiers.shift()
                    && let Some((target, false)) = link
                {
                    state.drag = Some(Drag {
                        kind: DragKind::Char,
                        last: offset,
                        origin: cursor.position(),
                        moved: false,
                        pending_link: Some((target, offset)),
                    });
                    return;
                }
                match count {
                    2 => {
                        let word = self.doc.word_range(offset);
                        state.drag = Some(Drag { kind: DragKind::Word(word.clone()), last: offset, origin: cursor.position(), moved: false, pending_link: None });
                        self.emit(shell, Action::SelectBetween { anchor: word.start, head: word.end });
                    }
                    3 => {
                        let line = self.doc.line_range(offset);
                        state.drag = Some(Drag { kind: DragKind::Line(line.clone()), last: offset, origin: cursor.position(), moved: false, pending_link: None });
                        self.emit(shell, Action::SelectBetween { anchor: line.start, head: line.end });
                    }
                    _ => {
                        let extend = state.modifiers.shift();
                        state.drag = Some(Drag { kind: DragKind::Char, last: offset, origin: cursor.position(), moved: false, pending_link: None });
                        self.emit(shell, Action::SetCaret { offset, extend });
                    }
                }
            }

            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(drag) = state.drag.as_mut() else { return };
                if !drag.moved {
                    let travelled = match (drag.origin, cursor.position()) {
                        (Some(from), Some(now)) => distance(from, now),
                        _ => f32::INFINITY,
                    };
                    if travelled < DRAG_THRESHOLD {
                        return;
                    }
                    drag.moved = true;
                    if let Some((target, at)) = drag.pending_link.take() {
                        // A link dragged away is a link being carried somewhere; with nowhere to
                        // carry it, it was a drag that starts a selection where the press was.
                        if let (Some(f), Some(pos)) = (&self.on_link_drag, cursor.position()) {
                            state.drag = None;
                            shell.publish(f(target, pos));
                            shell.capture_event();
                            shell.request_redraw();
                            return;
                        }
                        self.emit(shell, Action::SetCaret { offset: at, extend: false });
                    }
                }
                if self.read_only {
                    return;
                }
                let Some(rel) = cursor.position().map(|p| p - bounds.position()) else { return };
                let p = Point::new(rel.x - self.padding.left, rel.y - self.padding.top + state.scroll);
                let offset = self.offset_at(state, p);
                let Some(drag) = state.drag.as_mut().filter(|d| d.last != offset) else { return };
                drag.last = offset;
                let action = match &drag.kind {
                    DragKind::Char => Action::SetCaret { offset, extend: true },
                    DragKind::Word(w) => {
                        let target = self.doc.word_range(offset);
                        if target.start < w.start {
                            Action::SelectBetween { anchor: w.end, head: target.start }
                        } else {
                            Action::SelectBetween { anchor: w.start, head: target.end.max(w.end) }
                        }
                    }
                    DragKind::Line(l) => {
                        let target = self.doc.line_range(offset);
                        if target.start < l.start {
                            Action::SelectBetween { anchor: l.end, head: target.start }
                        } else {
                            Action::SelectBetween { anchor: l.start, head: target.end.max(l.end) }
                        }
                    }
                };
                shell.capture_event();
                self.emit(shell, action);
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(drag) = state.drag.take() {
                    if let Some((target, _)) = drag.pending_link
                        && !drag.moved
                        && let Some(f) = &self.on_link
                    {
                        shell.publish(f(target));
                    }
                    shell.request_redraw();
                }
            }

            Event::InputMethod(input_method::Event::Preedit(content, _)) if state.focused => {
                state.preedit = (!content.is_empty()).then(|| content.clone());
                shell.request_redraw();
            }
            Event::InputMethod(input_method::Event::Commit(text)) if state.focused => {
                state.touch();
                state.preedit = None;
                state.goal_x = None;
                self.emit(shell, Action::Insert(text.clone()));
                shell.capture_event();
            }

            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, text, .. }) if state.focused => {
                if let Some(action) = self.key_action(state, clipboard, key, *modifiers, text.as_deref()) {
                    state.touch();
                    shell.capture_event();
                    self.emit(shell, action);
                } else if matches!(key.as_ref(), keyboard::Key::Named(keyboard::key::Named::Escape)) {
                    state.focused = false;
                    shell.request_redraw();
                }
            }
            _ => {}
        }
        let _ = renderer;
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        if !cursor.is_over(bounds) {
            return mouse::Interaction::None;
        }
        let modifier = state.modifiers.command() || state.modifiers.control();
        match self.content_point(state, bounds, cursor) {
            // A hand over what a click would follow: any link in a read-only view; links showing
            // as their label when plain clicks follow; any link with Cmd/Ctrl held.
            Some(p) if self.embed_header_at(state, p).is_some() => mouse::Interaction::Pointer,
            Some(p) if self.link_at(state, p).is_some_and(|(_, raw)| {
                self.read_only || modifier || (self.follow_links && !raw && !state.modifiers.alt())
            }) => mouse::Interaction::Pointer,
            Some(p) if self.block_at(state, p).is_some_and(|b| {
                let l = &state.laid[b];
                matches!(l.rendered.deco, Deco::Checkbox(_)) && p.x >= l.x - GUTTER && p.x < l.x
            }) => mouse::Interaction::Pointer,
            _ => mouse::Interaction::Text,
        }
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, _renderer: &iced::Renderer, operation: &mut dyn Operation) {
        if self.read_only {
            // Not a place to type: just report the text it shows.
            operation.text(self.id.as_ref(), layout.bounds(), self.doc.source());
            return;
        }
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
    }
}

impl<Message> LiveEditor<'_, Message> {
    /// Lay out the passage for an embed line, reusing the last shaping when nothing changed.
    fn layout_embed(&self, previous: Option<EmbedLaid>, target: &LinkTarget, line_w: f32, line_h: f32, font: Font) -> EmbedLaid {
        let found = self.embed.as_ref().and_then(|f| f(target));
        let missing = found.is_none();
        let content = found.unwrap_or_else(|| EmbedContent {
            title: match target {
                LinkTarget::Verse { range, module: Some(module) } => format!("{range} · {module}"),
                LinkTarget::Verse { range, .. } => range.to_string(),
                LinkTarget::Note(n) => n.clone(),
                LinkTarget::Sword { module, key } => format!("{module}/{key}"),
            },
            body: "Not available in this module.".to_string(),
        });
        let w = (line_w - 14.0 - 10.0).max(1.0);
        let spans = vec![
            // The title is a link to the passage's source, so it looks like one.
            iced::widget::span(content.title).size(self.size.0 * 0.82).font(Font { weight: font::Weight::Bold, ..self.font }).color(self.pal.link),
            iced::widget::span(format!("\n{}", content.body)).size(self.size.0 * 0.95),
        ];
        let shape = Text {
            content: (),
            bounds: Size::new(w, f32::INFINITY),
            size: self.size,
            line_height: LineHeight::Relative(LINE),
            font,
            align_x: Alignment::Default,
            align_y: alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::default(),
        };
        let para = match previous {
            Some(mut p) if p.spans == spans => {
                match p.para.compare(shape) {
                    Difference::None => {}
                    Difference::Bounds => p.para.resize(shape.bounds),
                    Difference::Shape => p.para = Para::with_spans(shape.with_content(spans.as_slice())),
                }
                p.para
            }
            _ => Para::with_spans(shape.with_content(spans.as_slice())),
        };
        let h = para.min_height() + 8.0;
        let header_h = para.buffer().layout_runs().next().map_or(self.line_px(), |r| r.line_height);
        EmbedLaid {
            spans,
            para,
            y: if line_h > 0.0 { line_h + 4.0 } else { 0.0 },
            h,
            missing,
            target: target.clone(),
            header_h,
        }
    }

    /// The action for a key press, if it is one the editor handles. Copy, cut and paste talk
    /// to the clipboard here.
    fn key_action(
        &self,
        state: &mut State,
        clipboard: &mut dyn Clipboard,
        key: &keyboard::Key,
        mods: keyboard::Modifiers,
        text: Option<&str>,
    ) -> Option<Action> {
        use keyboard::Key;
        use keyboard::key::Named;
        let (cmd, alt, shift) = (mods.command(), mods.alt(), mods.shift());
        let vertical_keeps_goal = matches!(key.as_ref(), Key::Named(Named::ArrowUp | Named::ArrowDown | Named::PageUp | Named::PageDown));
        if !vertical_keeps_goal {
            state.goal_x = None;
        }
        let mv = |motion, extend| Some(Action::Move { motion, extend });

        match key.as_ref() {
            Key::Named(Named::ArrowLeft) => mv(if cmd { Motion::LineStart } else if alt { Motion::WordLeft } else { Motion::Left }, shift),
            Key::Named(Named::ArrowRight) => mv(if cmd { Motion::LineEnd } else if alt { Motion::WordRight } else { Motion::Right }, shift),
            Key::Named(Named::Home) => mv(Motion::LineStart, shift),
            Key::Named(Named::End) => mv(Motion::LineEnd, shift),
            Key::Named(Named::ArrowUp) if cmd => mv(Motion::DocStart, shift),
            Key::Named(Named::ArrowDown) if cmd => mv(Motion::DocEnd, shift),
            Key::Named(Named::ArrowUp) => self.vertical_target(state, true).map(|offset| Action::SetCaret { offset, extend: shift }),
            Key::Named(Named::ArrowDown) => self.vertical_target(state, false).map(|offset| Action::SetCaret { offset, extend: shift }),
            Key::Named(Named::PageUp | Named::PageDown) => {
                let up = matches!(key.as_ref(), Key::Named(Named::PageUp));
                let rect = self.caret_rect(state, self.doc.selection().head)?;
                let goal = *state.goal_x.get_or_insert(rect.x);
                let dy = state.view_h * 0.9;
                let y = (if up { rect.y - dy } else { rect.y + dy }).clamp(0.0, state.content_h);
                Some(Action::SetCaret { offset: self.offset_at(state, Point::new(goal, y)), extend: shift })
            }
            Key::Named(Named::Backspace) if alt => Some(Action::DeleteWordBack),
            Key::Named(Named::Backspace) if cmd => Some(Action::DeleteToLineStart),
            Key::Named(Named::Backspace) => Some(Action::Backspace),
            Key::Named(Named::Delete) => Some(Action::Delete),
            Key::Named(Named::Enter) => Some(Action::Newline),
            Key::Named(Named::Tab) => Some(if shift { Action::Outdent } else { Action::Indent }),
            Key::Character(c) if cmd => match (c.to_lowercase().as_str(), shift) {
                ("a", _) => Some(Action::SelectAll),
                ("z", false) => Some(Action::Undo),
                ("z", true) | ("y", _) => Some(Action::Redo),
                ("b", _) => Some(Action::Wrap(Wrap::Bold)),
                ("i", _) => Some(Action::Wrap(Wrap::Italic)),
                ("e", _) => Some(Action::Wrap(Wrap::Code)),
                ("c", _) => {
                    if let Some(selected) = self.doc.selected_text() {
                        clipboard.write(clipboard::Kind::Standard, self.copy_as.map_or(selected.clone(), |f| f(&selected)));
                    }
                    None
                }
                ("x", _) => {
                    let selected = self.doc.selected_text()?;
                    clipboard.write(clipboard::Kind::Standard, selected);
                    Some(Action::Backspace)
                }
                ("v", _) => clipboard.read(clipboard::Kind::Standard).map(Action::Paste),
                _ => None,
            },
            _ if cmd || mods.control() => None,
            _ => text
                .filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
                .map(|t| Action::Insert(t.to_string())),
        }
    }
}

fn distance(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

impl<'a, Message: 'a> From<LiveEditor<'a, Message>> for Element<'a, Message> {
    fn from(editor: LiveEditor<'a, Message>) -> Self {
        Element::new(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_link_wraps_as_one_piece_at_every_width() {
        let line = "aaaaaaaaaaaa [[John 3:16@KJV]] bbbb [[Notes on Nicodemus]] tail";
        let blocks = md_editor::blocks::parse(line);
        let rendered = render(line, &blocks[0], &Reveal::none());
        let spans = spans_for(&rendered, 17.0, &Palette::for_theme(&Theme::Dark), fonts::UI);
        let links = ["John\u{a0}3:16", "Notes\u{a0}on\u{a0}Nicodemus"];
        for width in (140..420).step_by(9) {
            let para = Para::with_spans(Text {
                content: spans.as_slice(),
                bounds: Size::new(width as f32, f32::INFINITY),
                size: Pixels(17.0),
                line_height: LineHeight::Relative(LINE),
                font: fonts::UI,
                align_x: Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::default(),
            });
            let mut lines = 0;
            for run in para.buffer().layout_runs() {
                let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last()) else { continue };
                lines += 1;
                let piece = rendered.text[first.start.min(last.start)..first.end.max(last.end)].trim();
                for link in links {
                    let words: Vec<&str> = link.split('\u{a0}').collect();
                    for k in 1..words.len() {
                        let front = words[..k].join("\u{a0}");
                        let back = words[k..].join("\u{a0}");
                        assert!(
                            !piece.ends_with(&front),
                            "width {width}: a line ends with only the start of {link:?}: {piece:?}"
                        );
                        assert!(
                            !piece.starts_with(&back),
                            "width {width}: a line starts with only the end of {link:?}: {piece:?}"
                        );
                    }
                }
            }
            assert!(lines >= 1);
        }
    }

    #[test]
    fn underlines_on_a_line_share_one_depth_whatever_the_size_of_each_run() {
        let line = "[[John 3:3@KJV]]";
        let blocks = md_editor::blocks::parse(line);
        let r = render(line, &blocks[0], &Reveal::none());
        let small = r.runs.iter().find(|x| x.text == "KJV").unwrap().style.scale;
        assert!(small < 1.0, "the module run is smaller");
        // One depth for the line, set by its biggest text: the same for the reference and module.
        assert!((underline_gap(&r, 17.0) - 17.0 * 0.12).abs() < 1e-6);
        // A heading's underlines sit deeper, in proportion to its size.
        let h = md_editor::blocks::parse("## [[Note]]");
        let hr = render("## [[Note]]", &h[0], &Reveal::none());
        assert!(underline_gap(&hr, 17.0) > underline_gap(&r, 17.0));
    }

    #[test]
    fn the_caret_is_solid_after_activity_and_then_blinks() {
        let ms = Duration::from_millis;
        assert!(caret_on(ms(0)), "solid at once");
        assert!(caret_on(ms(499)));
        assert!(!caret_on(ms(500)), "off for the next half second");
        assert!(!caret_on(ms(999)));
        assert!(caret_on(ms(1000)));
        assert!(!caret_on(ms(1500)));
    }

    #[test]
    fn the_next_wake_up_is_the_next_change() {
        let ms = Duration::from_millis;
        assert_eq!(until_blink(ms(0)), ms(500));
        assert_eq!(until_blink(ms(100)), ms(400));
        assert_eq!(until_blink(ms(499)), ms(1));
        // Exactly on a change, the following one is a full interval away, never zero.
        assert_eq!(until_blink(ms(500)), ms(500));
        assert_eq!(until_blink(ms(1250)), ms(250));
        for t in 0..3000 {
            let wait = until_blink(ms(t));
            assert!(wait > Duration::ZERO && wait <= BLINK);
            // After waiting, the caret has just changed state.
            assert_ne!(caret_on(ms(t)), caret_on(ms(t) + wait), "at {t}ms");
        }
    }

    #[test]
    fn activity_makes_the_caret_solid_again() {
        let mut state = State { focused: true, ..State::default() };
        assert!(state.caret_visible(), "no activity recorded yet: show it");
        state.activity = Some(Instant::now() - Duration::from_millis(700));
        assert!(!state.caret_visible(), "idle for 700ms is in the off half");
        state.touch();
        assert!(state.caret_visible(), "a keypress or click brings it back at once");
        state.focused = false;
        assert!(!state.caret_visible(), "and an unfocused editor has no caret");
    }
}

