//! Turns one block into what is drawn: styled display runs, plus a map between display offsets
//! and source offsets. A block that holds the cursor is *revealed*: shown as raw source with its
//! syntax dimmed. Every other block is *rendered*: markers hidden, bullets and checkboxes
//! drawn as decorations (docs/06 §8).

use crate::blocks::{Block, Kind};
use std::ops::Range;
use study_core::markdown::{LinkTarget, WikiLink, find_links};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Text,
    /// Syntax characters (`**`, `#`, `[[`), shown only in a revealed block.
    Marker,
    Muted,
    Heading,
    Code,
    Quote,
    Tag,
    LinkVerse,
    LinkNote,
    LinkModule,
    LinkWeb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub mono: bool,
    /// Size relative to body text.
    pub scale: f32,
    pub role: Role,
    /// Where a link goes.
    pub link: Option<LinkTarget>,
    /// This link is showing its raw `[[...]]` (the caret is in or at it), so a click there is
    /// editing it, not following it.
    pub raw: bool,
}

impl Style {
    pub fn plain() -> Self {
        Style {
            bold: false,
            italic: false,
            strike: false,
            underline: false,
            mono: false,
            scale: 1.0,
            role: Role::Text,
            link: None,
            raw: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegKind {
    /// Display text is the source text.
    Same,
    /// Source characters that are not shown at all.
    Hidden,
    /// Source characters shown as different text.
    Replaced,
}

/// One stretch of the source and where it appears in the display text.
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub display: Range<usize>,
    pub source: Range<usize>,
    pub kind: SegKind,
}

/// What the widget draws around or instead of text.
#[derive(Clone, Debug, PartialEq)]
pub enum Deco {
    None,
    Bullet,
    /// "1." and so on.
    Number(String),
    Checkbox(bool),
    QuoteBar,
    Rule,
    CodeBg,
    /// The passage an embed line points at, drawn as a quote under (or instead of) the line.
    Embed(LinkTarget),
    /// A line that takes almost no height (a code fence, rendered).
    Collapsed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub runs: Vec<Run>,
    /// The concatenation of the runs.
    pub text: String,
    pub segs: Vec<Seg>,
    pub deco: Deco,
    /// Indent level (list nesting).
    pub level: usize,
    /// Extra space above the block, in body line heights.
    pub gap_before: f32,
}

impl Rendered {
    /// Display offset for a source offset (absolute, within the block).
    pub fn source_to_display(&self, source: usize) -> usize {
        for s in &self.segs {
            if source < s.source.end {
                return match s.kind {
                    SegKind::Same => s.display.start + source.saturating_sub(s.source.start).min(s.display.len()),
                    SegKind::Hidden | SegKind::Replaced => s.display.start,
                };
            }
        }
        self.text.len()
    }

    /// Source offset for a display offset. Positions inside a hidden marker are never returned;
    /// the end of the display text maps to the end of the block.
    pub fn display_to_source(&self, display: usize) -> usize {
        for s in &self.segs {
            if s.kind == SegKind::Hidden || display >= s.display.end {
                continue;
            }
            return match s.kind {
                SegKind::Same => s.source.start + display.saturating_sub(s.display.start),
                _ => s.source.start,
            };
        }
        self.segs.last().map_or(0, |s| s.source.end)
    }
}

/// What of a block shows its raw source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reveal {
    /// The whole line (headings, rules, embeds, code).
    pub line: bool,
    /// Inline elements touching this source range (bold, links...): only those show their
    /// syntax, the rest of the line stays rendered.
    pub selection: Option<Range<usize>>,
}

impl Reveal {
    pub fn none() -> Self {
        Reveal::default()
    }

    pub fn line() -> Self {
        Reveal { line: true, selection: None }
    }

    pub fn inline(selection: Range<usize>) -> Self {
        Reveal { line: false, selection: Some(selection) }
    }

    /// Whether an element covering source `start..end` shows its syntax. Touching counts, so
    /// the caret at either edge of `**bold**` shows its markers.
    fn shows(&self, start: usize, end: usize) -> bool {
        self.line || self.selection.as_ref().is_some_and(|s| s.start <= end && start <= s.end)
    }
}

#[derive(Default)]
struct Out {
    runs: Vec<Run>,
    segs: Vec<Seg>,
    text: String,
}

impl Out {
    fn same(&mut self, text: &str, source: Range<usize>, style: &Style) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.segs.push(Seg { display: start..self.text.len(), source, kind: SegKind::Same });
        self.run(text, style);
    }

    /// Source characters shown as different text (`@` shown as a thin space).
    fn replaced(&mut self, display: &str, source: Range<usize>, style: &Style) {
        if display.is_empty() || source.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(display);
        self.segs.push(Seg { display: start..self.text.len(), source, kind: SegKind::Replaced });
        self.run(display, style);
    }

    fn hidden(&mut self, source: Range<usize>) {
        if source.is_empty() {
            return;
        }
        let at = self.text.len();
        self.segs.push(Seg { display: at..at, source, kind: SegKind::Hidden });
    }

    fn run(&mut self, text: &str, style: &Style) {
        match self.runs.last_mut() {
            Some(last) if last.style == *style => last.text.push_str(text),
            _ => self.runs.push(Run { text: text.to_string(), style: style.clone() }),
        }
    }

    /// A syntax marker: shown dim when revealed, absent otherwise.
    fn marker(&mut self, text: &str, source: Range<usize>, revealed: bool, style: &Style) {
        if revealed {
            self.same(text, source, &Style { role: Role::Marker, link: None, underline: false, ..style.clone() });
        } else {
            self.hidden(source);
        }
    }

    fn finish(self, deco: Deco, level: usize, gap_before: f32) -> Rendered {
        Rendered { runs: self.runs, text: self.text, segs: self.segs, deco, level, gap_before }
    }
}

/// The gap between a reference and its pinned module, and how much smaller the module is. A
/// no-break space: it is a little wider than a thin one, and keeps the module off a line of its own.
const MODULE_GAP: &str = "\u{a0}";
/// Spaces inside a rendered link are these, so a link is never split across two lines.
const NO_BREAK_SPACE: &str = "\u{a0}";
const MODULE_SCALE: f32 = 0.78;

const HEADING_SCALE: [f32; 6] = [1.7, 1.45, 1.25, 1.1, 1.0, 1.0];

/// Render `block` of `source`; `reveal` says what shows its raw syntax (dimmed).
pub fn render(source: &str, block: &Block, reveal: &Reveal) -> Rendered {
    let revealed = reveal.line;
    let line = block.text(source);
    let start = block.range.start;
    let prefix_end = start + block.prefix;
    let mut out = Out::default();
    let plain = Style::plain();

    match &block.kind {
        Kind::Blank => out.finish(Deco::None, 0, 0.0),
        // The properties are shown by the app (and edited there); the raw lines only come out when
        // the caret goes into them.
        Kind::Frontmatter => {
            if revealed {
                let style = Style { mono: true, scale: 0.85, role: Role::Muted, ..plain };
                out.same(line, block.range.clone(), &style);
                out.finish(Deco::None, 0, 0.0)
            } else {
                out.hidden(block.range.clone());
                out.finish(Deco::Collapsed, 0, 0.0)
            }
        }
        Kind::Fence => {
            if revealed {
                let style = Style { mono: true, role: Role::Muted, ..plain };
                out.same(line, block.range.clone(), &style);
                out.finish(Deco::CodeBg, 0, 0.0)
            } else {
                out.hidden(block.range.clone());
                out.finish(Deco::Collapsed, 0, 0.0)
            }
        }
        Kind::Code => {
            let style = Style { mono: true, role: Role::Code, ..plain };
            out.same(line, block.range.clone(), &style);
            out.finish(Deco::CodeBg, 0, 0.0)
        }
        Kind::Rule => {
            if revealed {
                out.same(line, block.range.clone(), &Style { role: Role::Muted, ..plain });
            } else {
                out.hidden(block.range.clone());
            }
            out.finish(Deco::Rule, 0, 0.0)
        }
        Kind::Embed => {
            let target = find_links(line.trim())[0].target.clone();
            if revealed {
                inline(&mut out, line, start, reveal, &plain);
            } else {
                out.hidden(block.range.clone());
            }
            out.finish(Deco::Embed(target), 0, 0.0)
        }
        Kind::Heading(n) => {
            let scale = HEADING_SCALE[usize::from(*n - 1).min(5)];
            let style = Style { bold: true, scale, role: Role::Heading, ..plain };
            out.marker(&line[..block.prefix], start..prefix_end, revealed, &style);
            inline(&mut out, &line[block.prefix..], prefix_end, reveal, &style);
            out.finish(Deco::None, 0, if *n <= 2 { 0.6 } else { 0.3 })
        }
        Kind::Quote => {
            let style = Style { role: Role::Quote, ..plain };
            out.marker(&line[..block.prefix], start..prefix_end, revealed, &style);
            inline(&mut out, &line[block.prefix..], prefix_end, reveal, &style);
            out.finish(Deco::QuoteBar, 0, 0.0)
        }
        Kind::Item { indent, ordered, task } => {
            let level = indent / 2;
            let deco = match (task, ordered) {
                (Some(done), _) => Deco::Checkbox(*done),
                (None, Some(number)) => Deco::Number(number.clone()),
                (None, None) => Deco::Bullet,
            };
            if revealed {
                // The raw prefix, so the line reads as typed.
                out.same(&line[..block.prefix], start..prefix_end, &Style { role: Role::Marker, ..plain.clone() });
                inline(&mut out, &line[block.prefix..], prefix_end, reveal, &plain);
                out.finish(Deco::None, level, 0.0)
            } else {
                out.hidden(start..prefix_end);
                inline(&mut out, &line[block.prefix..], prefix_end, reveal, &plain);
                out.finish(deco, level, 0.0)
            }
        }
        Kind::Paragraph => {
            inline(&mut out, line, start, reveal, &plain);
            out.finish(Deco::None, 0, 0.0)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Inline syntax
// ---------------------------------------------------------------------------------------

/// What an inline delimiter turned out to be, with lengths in bytes.
struct Found {
    /// Everything consumed, delimiters included.
    len: usize,
    open: usize,
    close: usize,
    kind: Delim,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Delim {
    Bold,
    Italic,
    Strike,
    Code,
    /// `[text](url)`: `open` is `[`, `close` is `](url)`.
    Link,
    Tag,
}

fn inline(out: &mut Out, text: &str, base: usize, reveal: &Reveal, style: &Style) {
    let links = find_links(text);
    let (mut i, mut literal) = (0, 0);
    let flush = |out: &mut Out, from: usize, to: usize| {
        if from < to {
            out.same(&text[from..to], base + from..base + to, style);
        }
    };

    while i < text.len() {
        if let Some(link) = links.iter().find(|l| l.span.start == i) {
            flush(out, literal, i);
            emit_link(out, text, base, link, reveal, style);
            i = link.span.end;
            literal = i;
            continue;
        }
        let before = text[..i].chars().next_back();
        match detect(&text[i..], before) {
            Some(found) => {
                flush(out, literal, i);
                emit(out, text, base, i, &found, reveal, style);
                i += found.len;
                literal = i;
            }
            None => i += text[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
    flush(out, literal, text.len());
}

fn detect(rest: &str, before: Option<char>) -> Option<Found> {
    let first = rest.chars().next()?;
    let delimited = |delim: &str, kind: Delim| {
        let after = &rest[delim.len()..];
        let close = after.find(delim).filter(|&k| k > 0)?;
        Some(Found { len: delim.len() * 2 + close, open: delim.len(), close: delim.len(), kind })
    };
    match first {
        '*' if rest.starts_with("**") => delimited("**", Delim::Bold),
        '~' if rest.starts_with("~~") => delimited("~~", Delim::Strike),
        '`' => delimited("`", Delim::Code),
        '*' | '_' => {
            let after = &rest[1..];
            let opens = after.chars().next().is_some_and(|c| !c.is_whitespace() && c != first);
            let at_word = first == '*' || !before.is_some_and(char::is_alphanumeric);
            if !(opens && at_word) {
                return None;
            }
            let close = after.char_indices().find_map(|(k, c)| {
                let prev = after[..k].chars().next_back();
                (c == first && k > 0 && prev.is_some_and(|p| !p.is_whitespace())).then_some(k)
            })?;
            Some(Found { len: 2 + close, open: 1, close: 1, kind: Delim::Italic })
        }
        '[' => {
            let mid = rest.find("](")?;
            let end = rest[mid..].find(')')? + mid;
            (mid > 1).then_some(Found { len: end + 1, open: 1, close: end + 1 - mid, kind: Delim::Link })
        }
        '#' => {
            let ok_before = before.is_none_or(char::is_whitespace);
            let body: usize = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .map(char::len_utf8)
                .sum();
            let starts_ok = rest[1..].chars().next().is_some_and(char::is_alphanumeric);
            (ok_before && starts_ok && body > 0).then_some(Found { len: 1 + body, open: 0, close: 0, kind: Delim::Tag })
        }
        _ => None,
    }
}

fn emit(out: &mut Out, text: &str, base: usize, at: usize, found: &Found, reveal: &Reveal, style: &Style) {
    let abs = |k: usize| base + at + k;
    // Each element decides for itself, so a line can have one bold span open and the rest shut.
    let revealed = reveal.shows(abs(0), abs(found.len));
    let inner_start = at + found.open;
    let inner_end = at + found.len - found.close;
    let inner = &text[inner_start..inner_end];
    out.marker(&text[at..inner_start], abs(0)..abs(found.open), revealed, style);
    match found.kind {
        Delim::Bold => inline(out, inner, base + inner_start, reveal, &Style { bold: true, ..style.clone() }),
        Delim::Italic => inline(out, inner, base + inner_start, reveal, &Style { italic: true, ..style.clone() }),
        Delim::Strike => inline(out, inner, base + inner_start, reveal, &Style { strike: true, ..style.clone() }),
        Delim::Code => out.same(
            inner,
            base + inner_start..base + inner_end,
            &Style { mono: true, role: Role::Code, ..style.clone() },
        ),
        Delim::Link => {
            let link = Style { underline: true, role: Role::LinkWeb, ..style.clone() };
            if revealed {
                out.same(inner, base + inner_start..base + inner_end, &link);
            } else {
                same_unbroken(out, inner, base + inner_start, &link);
            }
        }
        Delim::Tag => out.same(
            &text[at..at + found.len],
            abs(0)..abs(found.len),
            &Style { role: Role::Tag, ..style.clone() },
        ),
    }
    if found.close > 0 {
        out.marker(&text[inner_end..at + found.len], base + inner_end..abs(found.len), revealed, style);
    }
}

/// Show `text` (which starts at source offset `source_start`) with each space as a no-break
/// space, so the text stays in one piece when the line wraps. Each space still maps back to its
/// own source byte.
fn same_unbroken(out: &mut Out, text: &str, source_start: usize, style: &Style) {
    let mut from = 0;
    for (i, c) in text.char_indices() {
        if c == ' ' {
            if from < i {
                out.same(&text[from..i], source_start + from..source_start + i, style);
            }
            out.replaced(NO_BREAK_SPACE, source_start + i..source_start + i + 1, style);
            from = i + 1;
        }
    }
    if from < text.len() {
        out.same(&text[from..], source_start + from..source_start + text.len(), style);
    }
}

fn link_style(target: &LinkTarget, style: &Style) -> Style {
    let role = match target {
        LinkTarget::Verse { .. } => Role::LinkVerse,
        LinkTarget::Note(_) => Role::LinkNote,
        LinkTarget::Sword { .. } => Role::LinkModule,
    };
    Style { underline: true, role, link: Some(target.clone()), ..style.clone() }
}

fn emit_link(out: &mut Out, text: &str, base: usize, link: &WikiLink, reveal: &Reveal, style: &Style) {
    let span = link.span.clone();
    let revealed = reveal.shows(base + span.start, base + span.end);
    let ls = link_style(&link.target, style);
    if revealed {
        out.same(&text[span.clone()], base + span.start..base + span.end, &Style { raw: true, ..ls });
        return;
    }
    // `[[target|alias]]`: show the alias, or the target as typed.
    let open = if link.embed { 3 } else { 2 };
    let inner = &text[span.start + open..span.end - 2];
    let (shown_from, shown_len) = match inner.split_once('|') {
        Some((target, alias)) => (target.len() + 1, alias.len()),
        None => (0, inner.len()),
    };
    let shown_start = span.start + open + shown_from;
    out.hidden(base + span.start..base + shown_start);
    let shown = &text[shown_start..shown_start + shown_len];
    // A verse pinned to a module (`John 3:3@KJV`), with no alias to stand in for it, shows the
    // reference as usual and the module after a thin space, smaller. Still all one link.
    match shown.rsplit_once('@').filter(|_| shown_from == 0 && matches!(link.target, LinkTarget::Verse { .. })) {
        Some((reference, module)) if !reference.is_empty() && !module.is_empty() => {
            let at = shown_start + reference.len();
            same_unbroken(out, reference, base + shown_start, &ls);
            out.replaced(MODULE_GAP, base + at..base + at + 1, &ls);
            out.same(module, base + at + 1..base + at + 1 + module.len(), &Style { scale: ls.scale * MODULE_SCALE, ..ls.clone() });
        }
        _ => same_unbroken(out, shown, base + shown_start, &ls),
    }
    out.hidden(base + shown_start + shown_len..base + span.end);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::parse;

    fn rendered(line: &str, revealed: bool) -> Rendered {
        let blocks = parse(line);
        render(line, &blocks[0], &if revealed { Reveal::line() } else { Reveal::none() })
    }

    fn inline_at(line: &str, from: usize, to: usize) -> Rendered {
        let blocks = parse(line);
        render(line, &blocks[0], &Reveal::inline(from..to))
    }

    fn texts(r: &Rendered) -> Vec<(&str, bool, bool)> {
        r.runs.iter().map(|run| (run.text.as_str(), run.style.bold, run.style.italic)).collect()
    }

    #[test]
    fn a_revealed_block_is_the_raw_source_with_syntax_dimmed() {
        let r = rendered("a **bold** b", true);
        assert_eq!(r.text, "a **bold** b");
        let roles: Vec<(&str, Role)> = r.runs.iter().map(|x| (x.text.as_str(), x.style.role)).collect();
        assert_eq!(
            roles,
            [("a ", Role::Text), ("**", Role::Marker), ("bold", Role::Text), ("**", Role::Marker), (" b", Role::Text)]
        );
        // Raw means identity mapping everywhere.
        for i in 0..=12 {
            assert_eq!(r.source_to_display(i), i);
            assert_eq!(r.display_to_source(i), i);
        }
    }

    #[test]
    fn a_rendered_block_hides_markers_and_styles_what_is_inside() {
        let r = rendered("a **bold** and *it* ~~x~~ `c`", false);
        assert_eq!(r.text, "a bold and it x c");
        assert_eq!(texts(&r)[1], ("bold", true, false));
        assert!(r.runs.iter().any(|x| x.text == "it" && x.style.italic));
        assert!(r.runs.iter().any(|x| x.text == "x" && x.style.strike));
        assert!(r.runs.iter().any(|x| x.text == "c" && x.style.mono));
    }

    #[test]
    fn offsets_map_both_ways_across_hidden_markers() {
        // source: a **bold** b      display: a bold b
        let r = rendered("a **bold** b", false);
        assert_eq!(r.text, "a bold b");
        assert_eq!(r.source_to_display(0), 0);
        assert_eq!(r.source_to_display(2), 2, "just before the opening **");
        assert_eq!(r.source_to_display(4), 2, "after it, on the b of bold");
        assert_eq!(r.source_to_display(8), 6, "after the closing **'s start");
        assert_eq!(r.source_to_display(12), r.text.len());
        assert_eq!(r.display_to_source(2), 4, "the b of bold is at source 4");
        assert_eq!(r.display_to_source(0), 0);
        assert_eq!(r.display_to_source(8), 12, "the end maps to the end of the block");
        // Nested and repeated: map round-trips for every visible character.
        for d in 0..r.text.len() {
            let s = r.display_to_source(d);
            assert_eq!(r.source_to_display(s), d, "display {d} -> source {s}");
        }
    }

    #[test]
    fn headings_hide_hashes_scale_up_and_keep_their_size_when_revealed() {
        let hidden = rendered("## Title", false);
        assert_eq!(hidden.text, "Title");
        assert!(hidden.runs[0].style.bold && (hidden.runs[0].style.scale - 1.45).abs() < 1e-6);
        let shown = rendered("## Title", true);
        assert_eq!(shown.text, "## Title");
        // The marker is as big as the text, so revealing it does not change the line's height.
        assert!(shown.runs.iter().all(|r| (r.style.scale - 1.45).abs() < 1e-6));
        assert_eq!(hidden.display_to_source(0), 3, "click at the left edge lands after the hashes");
    }

    #[test]
    fn wikilinks_show_their_label_and_carry_their_target() {
        let r = rendered("see [[John 3:16]] and [[Nicodemus|him]] now", false);
        assert_eq!(r.text, "see John\u{a0}3:16 and him now");
        let link = r.runs.iter().find(|x| x.text == "John\u{a0}3:16").unwrap();
        assert_eq!(link.style.role, Role::LinkVerse);
        assert!(link.style.underline && matches!(link.style.link, Some(LinkTarget::Verse { .. })));
        let note = r.runs.iter().find(|x| x.text == "him").unwrap();
        assert_eq!(note.style.role, Role::LinkNote);
        assert_eq!(note.style.link, Some(LinkTarget::Note("Nicodemus".into())));
        // Clicking in "him" lands in the alias part of the source.
        let d = r.text.find("him").unwrap();
        let s = r.display_to_source(d);
        assert_eq!(&"see [[John 3:16]] and [[Nicodemus|him]] now"[s..s + 3], "him");
        // Revealed, it is all there.
        assert_eq!(rendered("[[Nicodemus|him]]", true).text, "[[Nicodemus|him]]");
    }

    #[test]
    fn a_link_knows_whether_it_is_showing_its_raw_source() {
        let line = "see [[Nicodemus]] now";
        let rendered = inline_at(line, 0, 0);
        let link = rendered.runs.iter().find(|r| r.style.link.is_some()).unwrap();
        assert!(!link.style.raw, "rendered as its label");
        let editing = inline_at(line, 8, 8);
        let link = editing.runs.iter().find(|r| r.style.link.is_some()).unwrap();
        assert!(link.style.raw && link.text == "[[Nicodemus]]", "the caret is in it: raw");
        // Still the same target either way.
        assert_eq!(link.style.link, rendered.runs.iter().find(|r| r.style.link.is_some()).unwrap().style.link);
    }

    #[test]
    fn a_pinned_module_is_shown_small_after_a_thin_space_when_rendered() {
        let line = "see [[John 3:3@KJV]] now";
        let r = rendered(line, false);
        assert_eq!(r.text, "see John\u{a0}3:3\u{a0}KJV now");
        let module = r.runs.iter().find(|x| x.text == "KJV").expect("its own run");
        assert!((module.style.scale - 0.78).abs() < 1e-6, "smaller");
        assert!(module.style.underline && matches!(module.style.link, Some(LinkTarget::Verse { .. })), "still the link");
        let reference = r.runs.iter().find(|x| x.text.starts_with("John\u{a0}3:3")).unwrap();
        assert!((reference.style.scale - 1.0).abs() < 1e-6);
        assert_eq!(reference.text, "John\u{a0}3:3\u{a0}", "the gap belongs to the reference's run");
        // Revealed, it is the raw `@KJV`.
        assert_eq!(rendered(line, true).text, line);
        // With an alias, the alias is all that shows.
        assert_eq!(rendered("[[John 3:3@KJV|this]]", false).text, "this");
        // A note named with an @ is not a module pin.
        assert_eq!(rendered("[[a@b]]", false).text, "a@b");
    }

    #[test]
    fn offsets_map_through_the_module_suffix() {
        let line = "x [[John 3:3@KJV]] y";
        let r = rendered(line, false);
        for d in 0..r.text.len() {
            if !r.text.is_char_boundary(d) {
                continue;
            }
            let s = r.display_to_source(d);
            assert!(line.is_char_boundary(s), "display {d} -> source {s}");
        }
        let k = r.text.find("KJV").unwrap();
        assert_eq!(&line[r.display_to_source(k)..][..3], "KJV");
        let gap = r.text.rfind('\u{a0}').unwrap();
        assert_eq!(&line[r.display_to_source(gap)..][..1], "@", "the gap stands for the @");
        assert_eq!(r.source_to_display(line.find('K').unwrap()), k);
    }

    #[test]
    fn a_rendered_link_never_has_a_breakable_space_in_it() {
        for line in [
            "[[John 3:16]]",
            "[[Ps 23:1-3@KJV]]",
            "[[Notes on Nicodemus]]",
            "[[sword:StrongsGreek/G5547]]",
            "[[Some long note name|an alias with spaces]]",
            "[a web link with spaces](https://x.org)",
            "![[John 3:16-17]] after",
        ] {
            let r = rendered(line, false);
            for run in r.runs.iter().filter(|run| run.style.link.is_some() || run.style.role == Role::LinkWeb) {
                assert!(!run.text.contains(' '), "{line:?}: a link run has a breakable space: {:?}", run.text);
            }
        }
        // Prose around a link still wraps normally.
        assert_eq!(rendered("see [[A b]] now ok", false).text, "see A\u{a0}b now ok");
        // And what is typed is untouched: each no-break space stands for its source space.
        let r = rendered("[[A b]]", false);
        assert_eq!(&"[[A b]]"[r.display_to_source(r.text.find('b').unwrap())..][..1], "b");
        assert_eq!(&"[[A b]]"[r.display_to_source(1)..][..1], " ", "the gap is the source space");
    }

    #[test]
    fn list_items_task_boxes_and_numbers_become_decorations() {
        let bullet = rendered("- item", false);
        assert_eq!((bullet.text.as_str(), bullet.deco.clone()), ("item", Deco::Bullet));
        let task = rendered("  - [x] done", false);
        assert_eq!((task.text.as_str(), task.deco.clone(), task.level), ("done", Deco::Checkbox(true), 1));
        let open = rendered("- [ ] todo", false);
        assert_eq!(open.deco, Deco::Checkbox(false));
        let num = rendered("2. second", false);
        assert_eq!(num.deco, Deco::Number("2.".into()));
        // Revealed: the raw line, no decoration.
        let raw = rendered("- [ ] todo", true);
        assert_eq!((raw.text.as_str(), raw.deco), ("- [ ] todo", Deco::None));
    }

    #[test]
    fn frontmatter_is_collapsed_until_the_caret_goes_into_it() {
        let doc = "---\ntitle: x\n---\n\nText\n";
        let blocks = parse(doc);
        for b in &blocks[..3] {
            let r = render(doc, b, &Reveal::none());
            assert_eq!((r.deco, r.text.as_str()), (Deco::Collapsed, ""));
        }
        let shown = render(doc, &blocks[1], &Reveal::line());
        assert_eq!(shown.text, "title: x");
    }

    #[test]
    fn quotes_rules_code_fences_and_embeds() {
        let q = rendered("> quoted *text*", false);
        assert_eq!((q.text.as_str(), q.deco.clone()), ("quoted text", Deco::QuoteBar));
        assert!(q.runs.iter().all(|r| r.style.role == Role::Quote));
        let rule = rendered("---\ntext", false);
        assert_eq!((rule.text.as_str(), rule.deco), ("", Deco::Rule));

        let doc = "```\ncode\n```";
        let blocks = parse(doc);
        assert_eq!(render(doc, &blocks[0], &Reveal::none()).deco, Deco::Collapsed);
        assert_eq!(render(doc, &blocks[0], &Reveal::line()).text, "```");
        assert_eq!(render(doc, &blocks[1], &Reveal::none()).deco, Deco::CodeBg);
        assert!(render(doc, &blocks[1], &Reveal::none()).runs[0].style.mono);

        let embed = rendered("![[John 3:16-17]]", false);
        assert_eq!(embed.text, "");
        assert!(matches!(embed.deco, Deco::Embed(LinkTarget::Verse { .. })));
        assert_eq!(rendered("![[John 3:16-17]]", true).text, "![[John 3:16-17]]");
    }

    #[test]
    fn what_is_not_syntax_stays_literal() {
        for line in ["2 * 3 * 4", "snake_case_name", "a * b", "price: $5 ** later", "[not a link]", "#", "# "] {
            let r = rendered(line, false);
            // Nothing hidden except real heading syntax.
            if !line.starts_with('#') || line.len() < 2 {
                assert_eq!(r.text, line, "{line:?}");
            }
        }
        assert_eq!(rendered("#tag here", false).runs[0].style.role, Role::Tag);
        assert_eq!(rendered("a #tag", false).runs.last().unwrap().style.role, Role::Tag);
        assert_eq!(rendered("mail me@x.com#frag", false).text, "mail me@x.com#frag");
    }

    #[test]
    fn markdown_links_show_their_text() {
        let r = rendered("[docs](https://x.org) ok", false);
        assert_eq!(r.text, "docs ok");
        assert_eq!(r.runs[0].style.role, Role::LinkWeb);
        assert_eq!(rendered("[docs](https://x.org) ok", true).text, "[docs](https://x.org) ok");
    }

    #[test]
    fn only_the_element_holding_the_caret_shows_its_syntax() {
        //          0         1         2
        //          0123456789012345678901234567
        let line = "a **bold** b *it* c [[Note]]";
        // Caret inside the bold: its markers show; the italic and the link stay rendered.
        let r = inline_at(line, 6, 6);
        assert_eq!(r.text, "a **bold** b it c Note");
        // Caret in the italic instead.
        let r = inline_at(line, 15, 15);
        assert_eq!(r.text, "a bold b *it* c Note");
        // Caret in the link.
        let r = inline_at(line, 24, 24);
        assert_eq!(r.text, "a bold b it c [[Note]]");
        // Caret in plain text reveals nothing.
        let r = inline_at(line, 11, 11);
        assert_eq!(r.text, "a bold b it c Note");
    }

    #[test]
    fn touching_an_element_counts_so_its_edges_show_the_syntax() {
        let line = "x **b** y";
        assert_eq!(inline_at(line, 2, 2).text, "x **b** y", "caret just before the opening marker");
        assert_eq!(inline_at(line, 7, 7).text, "x **b** y", "caret just after the closing marker");
        assert_eq!(inline_at(line, 1, 1).text, "x b y", "a space away does not");
        assert_eq!(inline_at(line, 8, 8).text, "x b y");
    }

    #[test]
    fn a_selection_reveals_every_element_it_touches() {
        let line = "**a** and *b* and `c`";
        assert_eq!(inline_at(line, 0, 13).text, "**a** and *b* and c");
        assert_eq!(inline_at(line, 0, 21).text, "**a** and *b* and `c`");
    }

    #[test]
    fn nested_elements_each_decide_separately() {
        // Caret inside the italic, which is inside the bold: both contain it, so both show.
        let line = "**bold *it* x**";
        assert_eq!(inline_at(line, 9, 9).text, "**bold *it* x**");
        // Caret in the bold but outside the italic: the bold shows, the italic stays rendered.
        assert_eq!(inline_at(line, 13, 13).text, "**bold it x**");
    }

    #[test]
    fn list_and_quote_syntax_stays_rendered_while_their_text_is_edited() {
        let line = "- item with **bold**";
        let r = inline_at(line, 14, 14);
        assert_eq!(r.text, "item with **bold**", "the bold shows, the bullet marker never does");
        assert_eq!(r.deco, Deco::Bullet);
        let q = inline_at("> quoted *text*", 12, 12);
        assert_eq!(q.text, "quoted *text*");
        assert_eq!(q.deco, Deco::QuoteBar);
        let t = inline_at("- [ ] do *this*", 8, 8);
        assert_eq!((t.text.as_str(), t.deco), ("do this", Deco::Checkbox(false)));
    }

    #[test]
    fn offsets_still_map_with_a_partly_revealed_line() {
        let line = "a **bold** b *it* c";
        let r = inline_at(line, 5, 5);
        assert_eq!(r.text, "a **bold** b it c");
        for d in 0..r.text.len() {
            let s = r.display_to_source(d);
            assert_eq!(r.source_to_display(s), d, "display {d} -> source {s}");
        }
        // The italic's `it` is at display 13; in the source it is after the `*`.
        assert_eq!(&line[r.display_to_source(13)..][..2], "it");
    }

    #[test]
    fn unicode_survives_hiding_and_mapping() {
        let r = rendered("**ἀγαπάω** καί", false);
        assert_eq!(r.text, "ἀγαπάω καί");
        let d = r.text.find("καί").unwrap();
        let s = r.display_to_source(d);
        assert_eq!(&"**ἀγαπάω** καί"[s..], "καί");
    }
}
