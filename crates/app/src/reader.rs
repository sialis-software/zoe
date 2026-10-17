//! Turns the core AST into blocks for the custom reader widget. Each interactive
//! piece (verse number, reference, Strong's word, footnote marker) is a link span.

use crate::palette::Palette;
use crate::reader_text::{Block, CanonSlice, CardSpec, Citation, FloatKind, MarkDraw, ReaderText, SpanMeta};
use iced::widget::span;
use iced::widget::text::Span;
use iced::{Color, Element, Font, font};
use scripture::RefRange;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use study_core::ast::{self, Chapter, Inline, ParagraphStyle};

#[derive(Debug, Clone)]
pub enum Link {
    Ref(RefRange),
    Strongs(String),
    Footnote(String),
    /// The passages a cross reference marker points to.
    CrossRefs(Vec<RefRange>),
}


/// What the reader reports back to the app.
#[derive(Debug, Clone)]
pub enum Event {
    /// A link was clicked, and where it sits in the text (for placing a card beside it).
    Link(Link, Option<CanonSlice>, Vec<usize>),
    /// The text selection settled: slices of each verse's canonical text, or none.
    Selection(Option<Vec<CanonSlice>>),
    /// Auto-scroll request (pixels, negative = up) while dragging past an edge.
    Scroll(f32),
    /// Marks under a single click (several when they overlap); empty = popup dismissed.
    Marks(Vec<usize>),
    /// Whether the margin is showing (the reader is wide enough for one).
    Margin(bool),
    /// A left press in the text.
    Pressed,
    /// A right click on a link, at this point of the window.
    LinkMenu(Link, iced::Point),
    /// A link was pulled away from where it is, at this point of the window.
    LinkDrag(Link, iced::Point),
}

impl ViewOptions {
    /// Whether this kind of marker is wanted.
    fn markers_shown(&self, inline: &Inline) -> bool {
        match inline {
            Inline::Footnote { .. } => self.show_footnotes,
            Inline::CrossRef { .. } => self.show_refs,
            _ => true,
        }
    }
}

pub struct ViewOptions {
    /// Select these verses' text: (token, first, last). See `ReaderText::select_verses`.
    pub select: Option<(u64, u16, u16)>,
    /// Select from the first to the last of these words of a verse: (token, a slice of its text).
    pub select_words: Option<(u64, crate::reader_text::CanonSlice)>,
    /// The selection the tab holds, to put back when the reader is made again.
    pub restore: Option<Vec<crate::reader_text::CanonSlice>>,
    /// Verses that some note links to; their numbers are tinted.
    pub annotated: BTreeSet<u16>,
    /// Scroll-into-view request: (token, verse). See `ReaderText::reveal`.
    pub reveal: Option<(u64, u16)>,
    /// Margin marks to paint, already resolved to canonical coordinates.
    pub marks: Vec<MarkDraw>,
    /// Changing this clears the reader's text selection.
    pub clear_token: u64,
    /// Keep the text this wide and leave the remaining width for margin cards (margins mode).
    pub text_width: Option<f32>,
    /// Margins on both sides of the text rather than the right only.
    pub both_sides: bool,
    /// Every margin card is veiled until it (or its mark) is pointed at.
    pub dim_cards: bool,
    pub show_strongs: bool,
    /// The kinds of marker in the text, each shown or not (the reader's menu).
    pub show_footnotes: bool,
    pub show_refs: bool,
    /// Which kinds of note are in the margin just now (not those that pop over the text): their
    /// links in the text are drawn in the link color, the others quietly (grey, or as plain text)
    /// until they are pointed at.
    pub strongs_in_margin: bool,
    pub footnotes_in_margin: bool,
    pub refs_in_margin: bool,
    pub pal: Palette,
    /// Size of the text, and the family it is set in.
    pub body: f32,
    pub font: Font,
    /// Module abbreviation for citations, e.g. "KJV".
    pub module: String,
}

/// U+2060: no line break either side of it, and no width.
const WORD_JOINER: char = '\u{2060}';

/// Inherited formatting while walking nested inlines.
#[derive(Clone, Copy)]
struct Style {
    color: Option<Color>,
    italic: bool,
}

struct Builder<'o> {
    opts: &'o ViewOptions,
    spans: Vec<Span<'static, Link>>,
    meta: Vec<SpanMeta>,
    verse: Option<u16>,
    /// The text so far ends with a space: a marker now comes before a word (as the LSB's footnotes do), not after one.
    after_space: bool,
    /// A marker just put in front of a word: the space after it, if there is one, is not to be the end of a line.
    glue: bool,
}

impl Builder<'_> {
    fn push(&mut self, span: Span<'static, Link>, copy: bool, verse: Option<u16>) {
        let mut span = span;
        span.font = span.font.or(Some(self.opts.font));
        if copy && let Some(last) = span.text.chars().last() {
            self.after_space = last.is_whitespace();
        }
        self.spans.push(span);
        self.meta.push(SpanMeta { copy, verse, number: false, dotted: false });
    }

    fn base(&self, text: String, style: Style) -> Span<'static, Link> {
        let mut sp = span(text).size(self.opts.body);
        if let Some(c) = style.color {
            sp = sp.color(c);
        }
        if style.italic {
            sp = sp.font(Font { style: font::Style::Italic, ..self.opts.font });
        }
        sp
    }

    fn inlines(&mut self, items: &[Inline], style: Style) {
        let pal = self.opts.pal;
        let verse = self.verse;
        for item in items {
            let glue = std::mem::take(&mut self.glue);
            match item {
                Inline::Text(t) => {
                    // After a marker that comes before a word, a space that follows it is held to the word after it: a
                    // word joiner (which is no text) keeps a line from ending on the marker with that space.
                    let lead = if glue { t.len() - t.trim_start().len() } else { 0 };
                    if lead > 0 && lead < t.len() {
                        let first = self.base(t[..lead].to_string(), style);
                        self.push(first, true, verse);
                        self.push(span(WORD_JOINER.to_string()).size(self.opts.body), false, verse);
                        let rest = self.base(t[lead..].to_string(), style);
                        self.push(rest, true, verse);
                    } else {
                        let s = self.base(t.clone(), style);
                        self.push(s, true, verse);
                    }
                }
                Inline::RedLetter(inner) => {
                    self.inlines(inner, Style { color: Some(pal.red), ..style });
                }
                Inline::Emphasis(inner) => self.inlines(inner, Style { italic: true, ..style }),
                Inline::DivineName(name) => {
                    // Small caps are not available in iced; approximate with upper case at
                    // reduced size. Only when the byte length is unchanged, so offsets still
                    // match the canonical text that marks anchor to.
                    let upper = name.to_uppercase();
                    let shown = if upper.len() == name.len() { upper } else { name.clone() };
                    let s = self.base(shown, style).size(self.opts.body * 0.85);
                    self.push(s, true, verse);
                }
                Inline::Word { text, strongs } => {
                    let sp = self.base(text.clone(), style);
                    let sp = if self.opts.show_strongs && self.opts.strongs_in_margin {
                        sp.color(pal.link).link(Link::Strongs(strongs.clone()))
                    } else if self.opts.show_strongs {
                        // A popover's word is plain text until pointed at (when it takes the link color).
                        sp.link(Link::Strongs(strongs.clone()))
                    } else {
                        sp
                    };
                    self.push(sp, true, verse);
                    // With numbers on, the words that have one are dotted underneath.
                    if self.opts.show_strongs
                        && let Some(meta) = self.meta.last_mut()
                    {
                        meta.dotted = true;
                    }
                }
                Inline::Footnote { .. } | Inline::CrossRef { .. } if !self.opts.markers_shown(item) => {}
                Inline::Footnote { text, .. } => {
                    // Big enough to hit without shouting over the text.
                    let s = span("†".to_string())
                        .size(self.opts.body * 0.85)
                        .color(if self.opts.footnotes_in_margin { pal.link } else { pal.muted })
                        .link(Link::Footnote(text.clone()));
                    let before_word = self.after_space;
                    self.push(s, false, verse);
                    self.glue = before_word;
                }
                Inline::CrossRef { refs } => {
                    // A smaller arrow than the dagger: there can be several to a verse.
                    let s = span("\u{2197}".to_string())
                        .size(self.opts.body * 0.85)
                        .color(if self.opts.refs_in_margin { pal.link } else { pal.muted })
                        .link(Link::CrossRefs(refs.clone()));
                    let before_word = self.after_space;
                    self.push(s, false, verse);
                    self.glue = before_word;
                }
                Inline::Ref { label, target } => {
                    let s = self
                        .base(label.clone(), style)
                        .color(pal.link)
                        .underline(true)
                        .link(Link::Ref(*target));
                    self.push(s, true, verse);
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Heading,
    Prose,
    Poetry,
}

/// Build the reader. Events are turned into the app's message type by `to_message` right
/// here, rather than by `Element::map` afterwards: `map` gives the widget a fresh event
/// context, so it could no longer tell that a toolbar button had already taken a click.
pub fn chapter_view<'a, M: 'a>(
    chapter: &Chapter,
    opts: &ViewOptions,
    toolbar: Option<(FloatKind, Element<'a, M>)>,
    cards: Vec<CardSpec<'a, M>>,
    to_message: fn(Event) -> M,
) -> Element<'a, M> {
    let pal = opts.pal;
    let blocks = build_blocks(chapter, opts);

    let mut hasher = DefaultHasher::new();
    (chapter.book.osis(), chapter.chapter).hash(&mut hasher);

    ReaderText::new(hasher.finish(), blocks)
        .size(opts.body)
        .selection_color(pal.text_selection)
        .hover_color(pal.link)
        .citation(Citation {
            chapter: format!("{} {}", chapter.book, chapter.chapter),
            module: opts.module.clone(),
        })
        .on_link(move |link, at, marks| to_message(Event::Link(link, at, marks)))
        .on_scroll(move |dy| to_message(Event::Scroll(dy)))
        .on_selection(move |s| to_message(Event::Selection(s)))
        .on_marks(move |ids| to_message(Event::Marks(ids)))
        .on_layout(move |margin| to_message(Event::Margin(margin)))
        .on_press(move || to_message(Event::Pressed))
        .on_link_menu(move |link, at| to_message(Event::LinkMenu(link, at)))
        .on_link_drag(move |link, at| to_message(Event::LinkDrag(link, at)))
        .margin_colors(pal.margin_bg, pal.margin_rule)
        .reveal(opts.reveal)
        .select_verses(opts.select)
        .select_words(opts.select_words)
        .restore_selection(opts.restore.clone())
        .marks(opts.marks.clone())
        .clear_selection(opts.clear_token)
        .toolbar(toolbar)
        .cards(cards)
        .text_width(opts.text_width)
        .both_sides(opts.both_sides)
        .dim_cards(opts.dim_cards)
        .into()
}

fn build_blocks(chapter: &Chapter, opts: &ViewOptions) -> Vec<Block<Link>> {
    let pal = opts.pal;
    let mut blocks: Vec<Block<Link>> = Vec::new();
    let mut prev: Option<Kind> = None;

    for block in &chapter.blocks {
        match block {
            ast::Block::Heading(h) => {
                let bold = Font { weight: font::Weight::Bold, ..opts.font };
                blocks.push(Block {
                    spans: vec![span(h.clone()).size(opts.body * 0.8).font(bold).color(pal.muted)],
                    meta: vec![SpanMeta { copy: false, verse: None, number: false, dotted: false }],
                    indent: 0.0,
                    gap_before: 20.0,
                    soft_break: false,
                });
                prev = Some(Kind::Heading);
            }
            ast::Block::Parallel(line) => {
                // Quiet and small, close under the heading it belongs to; each passage is a link.
                let size = opts.body * 0.7;
                let mut spans = Vec::new();
                for item in line {
                    spans.push(match item {
                        ast::Inline::Ref { label, target } => {
                            span(label.clone()).size(size).font(opts.font).color(pal.link).link(Link::Ref(*target))
                        }
                        ast::Inline::Text(t) => span(t.clone()).size(size).font(opts.font).color(pal.muted),
                        _ => continue,
                    });
                }
                let meta = spans.iter().map(|_| SpanMeta { copy: false, verse: None, number: false, dotted: false }).collect();
                blocks.push(Block {
                    spans,
                    meta,
                    indent: 0.0,
                    gap_before: 2.0,
                    soft_break: false,
                });
                prev = Some(Kind::Heading);
            }
            ast::Block::Paragraph(p) => {
                let mut b = Builder { opts, spans: Vec::new(), meta: Vec::new(), verse: None, after_space: false, glue: false };
                for (i, run) in p.runs.iter().enumerate() {
                    if i > 0 {
                        b.push(span(" ".to_string()).size(opts.body), true, None);
                    }
                    b.verse = Some(run.verse);
                    if run.show_number {
                        // NBSP keeps the number on the same line as the first word after it.
                        let number_color =
                            if opts.annotated.contains(&run.verse) { pal.note_link } else { pal.muted };
                        let n = span(format!("{}\u{a0}", run.verse))
                            .size(opts.body * 0.65)
                            .color(number_color);
                        b.push(n, false, Some(run.verse));
                        if let Some(m) = b.meta.last_mut() {
                            m.number = true;
                        }
                    }
                    b.inlines(&run.inlines, Style { color: None, italic: false });
                }

                let (kind, indent) = match p.style {
                    ParagraphStyle::Prose => (Kind::Prose, 0.0),
                    ParagraphStyle::Poetry { indent } => (Kind::Poetry, 12.0 + 28.0 * f32::from(indent)),
                };
                let starts_verse = p.runs.first().is_some_and(|r| r.show_number);
                let gap_before = match (prev, kind) {
                    (Some(Kind::Heading), _) => 6.0,
                    (Some(Kind::Poetry), Kind::Poetry) if starts_verse => 8.0,
                    (Some(Kind::Poetry), Kind::Poetry) => 2.0,
                    _ => 12.0,
                };
                blocks.push(Block {
                    spans: b.spans,
                    meta: b.meta,
                    indent,
                    gap_before,
                    soft_break: prev == Some(Kind::Poetry) && kind == Kind::Poetry,
                });
                prev = Some(kind);
            }
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader_text::canonical_text;
    use study_core::{StudyBackend, mock::MockBackend};

    fn opts(show_strongs: bool) -> ViewOptions {
        ViewOptions {
            select: None,
            select_words: None,
            restore: None,
            annotated: BTreeSet::new(),
            reveal: None,
            marks: Vec::new(),
            clear_token: 0,
            text_width: None,
            both_sides: false,
            dim_cards: false,
            show_strongs,
            show_footnotes: true,
            show_refs: true,
            strongs_in_margin: false,
            footnotes_in_margin: false,
            refs_in_margin: false,
            pal: Palette::for_theme(&iced::Theme::Dark),
            body: 19.0,
            font: crate::fonts::UI,
            module: "KJV".into(),
        }
    }

    /// The invariant marks depend on: what the widget thinks a verse's text is must equal
    /// the core's canonical text, whatever the display options.
    #[test]
    fn widget_canonical_text_matches_core_for_every_mock_chapter() {
        let backend = MockBackend;
        let module = backend.modules()[0].id.clone();
        for (book, ch) in backend.available_chapters(&module) {
            let chapter = backend.chapter(&module, book, ch).unwrap();
            let core = chapter.canonical_verses();
            assert!(!core.is_empty());
            for strongs in [false, true] {
                let blocks = build_blocks(&chapter, &opts(strongs));
                let shown = canonical_text(&blocks);
                // The reader shows the divine name upper-cased (no small caps in iced), so
                // text may differ in case only; every byte offset must still line up.
                assert_eq!(shown.len(), core.len(), "{book} {ch}");
                for (verse, text) in &core {
                    let got = &shown[verse];
                    assert_eq!(got.len(), text.len(), "{book} {ch}:{verse} strongs={strongs}");
                    assert_eq!(got.to_lowercase(), text.to_lowercase(), "{book} {ch}:{verse}");
                }
            }
        }
    }

    /// Every dictionary card finds the word it belongs to (so pointing at the word can bring its
    /// card up, and the card can light the word).
    #[test]
    fn every_strongs_word_is_found_by_its_card_anchor() {
        let backend = MockBackend;
        let module = backend.modules()[0].id.clone();
        for (book, ch) in backend.available_chapters(&module) {
            let chapter = backend.chapter(&module, book, ch).unwrap();
            let widget: ReaderText<'_, Link, ()> = ReaderText::new(1, build_blocks(&chapter, &opts(true)));
            let spans = widget.link_places();
            for word in chapter.strongs_words() {
                let anchor = crate::reader_text::CanonSlice { verse: word.verse, start: word.start, end: word.end };
                assert!(spans.contains(&anchor), "{book} {ch}: {word:?} has no link at its place");
            }
        }
    }

    #[test]
    fn divine_name_keeps_canonical_length() {
        let backend = MockBackend;
        let module = backend.modules()[0].id.clone();
        let ps = backend.chapter(&module, scripture::Book::PSALMS, 23).unwrap();
        let shown: String = build_blocks(&ps, &opts(false))[1].spans.iter().map(|s| s.text.as_ref()).collect();
        assert!(shown.contains("LORD"));
        assert!(ps.canonical_verses()[&1].starts_with("The Lord is my shepherd;"));
    }

    /// A marker that comes before a word (the previous text ends with a space) does not end a line with the space that
    /// follows it; one that follows a word is as it was. What is copied is the same either way.
    #[test]
    fn a_marker_before_a_word_is_held_to_it_by_a_word_joiner() {
        let run = |inlines: Vec<ast::Inline>| ast::Run { verse: 1, show_number: false, inlines };
        let text = |t: &str| ast::Inline::Text(t.into());
        let note = ast::Inline::Footnote { id: "a".into(), text: "n".into() };
        let chapter = |inlines| ast::Chapter {
            book: scripture::Book::JOHN,
            chapter: 3,
            blocks: vec![ast::Block::Paragraph(ast::Paragraph { style: ast::ParagraphStyle::Prose, runs: vec![run(inlines)] })],
        };
        let shown = |c: &ast::Chapter| -> Vec<(String, bool)> {
            let block = &build_blocks(c, &opts(false))[0];
            block.spans.iter().zip(&block.meta).map(|(s, m)| (s.text.to_string(), m.copy)).collect()
        };
        // Before a word, with a space after the marker (as "do these † signs").
        let before = chapter(vec![text("do these "), note.clone(), text(" signs")]);
        assert_eq!(
            shown(&before),
            [("do these ".to_string(), true), ("†".to_string(), false), (" ".to_string(), true), ("\u{2060}".to_string(), false), ("signs".to_string(), true)]
        );
        // After a word: left as it is, for the word and the marker to end a line together.
        let after = chapter(vec![text("do these"), note.clone(), text(" signs")]);
        assert_eq!(shown(&after), [("do these".to_string(), true), ("†".to_string(), false), (" signs".to_string(), true)]);
        // Before a word with no space after the marker: nothing to hold.
        let flush = chapter(vec![text("do these "), note, text("signs")]);
        assert_eq!(shown(&flush).len(), 3);
        // The text that is copied does not change.
        for c in [&before, &after, &flush] {
            let core = c.canonical_verses();
            let canon = canonical_text(&build_blocks(c, &opts(false)));
            assert_eq!(canon[&1], core[&1]);
        }
    }

    /// The line breaker really does keep the marker with the word after it: at every width, the dagger is on the same
    /// line as the word it comes before, where without the joiner it can end a line.
    #[test]
    fn at_no_width_does_a_marker_before_a_word_end_a_line() {
        use iced::advanced::graphics::text::Paragraph;
        use iced::advanced::text::{Paragraph as _, Text, Wrapping};
        use iced::{Pixels, Size};
        let note = ast::Inline::Footnote { id: "a".into(), text: "n".into() };
        let c = ast::Chapter {
            book: scripture::Book::JOHN,
            chapter: 3,
            blocks: vec![ast::Block::Paragraph(ast::Paragraph {
                style: ast::ParagraphStyle::Prose,
                runs: vec![ast::Run {
                    verse: 1,
                    show_number: false,
                    inlines: vec![ast::Inline::Text("Truly I say to you, no one can do these ".into()), note, ast::Inline::Text(" signs that You do unless God is with him".into())],
                }],
            })],
        };
        let block = &build_blocks(&c, &opts(false))[0];
        let mut broke = 0;
        for width in (60..400).step_by(3) {
            let text = Text {
                content: block.spans.as_slice(),
                bounds: Size::new(width as f32, f32::INFINITY),
                size: Pixels(19.0),
                line_height: iced::advanced::text::LineHeight::default(),
                font: crate::fonts::UI,
                align_x: iced::advanced::text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: iced::advanced::text::Shaping::Advanced,
                wrapping: Wrapping::default(),
            };
            let p = Paragraph::with_spans(text);
            let (mut dagger_line, mut word_line) = (None, None);
            for (i, run) in p.buffer().layout_runs().enumerate() {
                for glyph in run.glyphs {
                    let t = &run.text[glyph.start..glyph.end];
                    if t == "\u{2020}" {
                        dagger_line = Some(i);
                    }
                    if t == "s" && word_line.is_none() && glyph.start > run.text.find('\u{2020}').unwrap_or(usize::MAX) {
                        word_line = Some(i);
                    }
                }
            }
            if dagger_line != word_line {
                broke += 1;
            }
        }
        assert_eq!(broke, 0, "the marker was left at the end of a line at {broke} widths");
    }
}
