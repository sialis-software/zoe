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
    pub show_strongs: bool,
    /// The kinds of marker in the text, each shown or not (the reader's menu).
    pub show_footnotes: bool,
    pub show_refs: bool,
    pub pal: Palette,
    /// Size of the text, and the family it is set in.
    pub body: f32,
    pub font: Font,
    /// Module abbreviation for citations, e.g. "KJV".
    pub module: String,
}

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
}

impl Builder<'_> {
    fn push(&mut self, span: Span<'static, Link>, copy: bool, verse: Option<u16>) {
        let mut span = span;
        span.font = span.font.or(Some(self.opts.font));
        self.spans.push(span);
        self.meta.push(SpanMeta { copy, verse, number: false });
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
            match item {
                Inline::Text(t) => {
                    let s = self.base(t.clone(), style);
                    self.push(s, true, verse);
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
                    let sp = if self.opts.show_strongs {
                        sp.underline(true).color(pal.link).link(Link::Strongs(strongs.clone()))
                    } else {
                        sp
                    };
                    self.push(sp, true, verse);
                }
                Inline::Footnote { .. } | Inline::CrossRef { .. } if !self.opts.markers_shown(item) => {}
                Inline::Footnote { text, .. } => {
                    // Big enough to hit without shouting over the text.
                    let s = span("†".to_string())
                        .size(self.opts.body * 0.85)
                        .color(pal.link)
                        .link(Link::Footnote(text.clone()));
                    self.push(s, false, verse);
                }
                Inline::CrossRef { refs } => {
                    // A smaller arrow than the dagger: there can be several to a verse.
                    let s = span("\u{2197}".to_string())
                        .size(self.opts.body * 0.85)
                        .color(pal.link)
                        .link(Link::CrossRefs(refs.clone()));
                    self.push(s, false, verse);
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
        .marks(opts.marks.clone())
        .clear_selection(opts.clear_token)
        .toolbar(toolbar)
        .cards(cards)
        .text_width(opts.text_width)
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
                    spans: vec![span(h.clone()).size(15.0).font(bold).color(pal.muted)],
                    meta: vec![SpanMeta { copy: false, verse: None, number: false }],
                    indent: 0.0,
                    gap_before: 20.0,
                    soft_break: false,
                });
                prev = Some(Kind::Heading);
            }
            ast::Block::Paragraph(p) => {
                let mut b = Builder { opts, spans: Vec::new(), meta: Vec::new(), verse: None };
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
            annotated: BTreeSet::new(),
            reveal: None,
            marks: Vec::new(),
            clear_token: 0,
            text_width: None,
            show_strongs,
            show_footnotes: true,
            show_refs: true,
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

    #[test]
    fn divine_name_keeps_canonical_length() {
        let backend = MockBackend;
        let module = backend.modules()[0].id.clone();
        let ps = backend.chapter(&module, scripture::Book::PSALMS, 23).unwrap();
        let shown: String = build_blocks(&ps, &opts(false))[1].spans.iter().map(|s| s.text.as_ref()).collect();
        assert!(shown.contains("LORD"));
        assert!(ps.canonical_verses()[&1].starts_with("The Lord is my shepherd;"));
    }
}
