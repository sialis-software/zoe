//! OSIS verse markup to the shared inline AST (docs/01 §5).
//!
//! SWORD stores one marked-up fragment per verse. Structure (paragraphs, poetry lines,
//! quotations) often opens in one verse and closes in another, so a chapter is converted
//! with a single [`ChapterBuilder`] that carries that state from verse to verse.

use scripture::{Book, RefRange};
use std::collections::HashSet;
use study_core::ast::*;

#[derive(Debug)]
pub(crate) struct Tag {
    pub(crate) name: String,
    pub(crate) closing: bool,
    pub(crate) self_closing: bool,
    attrs: Vec<(String, String)>,
}

impl Tag {
    pub(crate) fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

pub(crate) enum Token {
    Text(String),
    Tag(Tag),
}

pub(crate) fn tokenize(raw: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = raw;
    while !rest.is_empty() {
        match rest.find('<') {
            None => {
                out.push(Token::Text(decode(rest)));
                break;
            }
            Some(0) => match rest.find('>') {
                Some(end) => {
                    if let Some(tag) = parse_tag(&rest[1..end]) {
                        out.push(Token::Tag(tag));
                    }
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push(Token::Text(decode(rest)));
                    break;
                }
            },
            Some(i) => {
                out.push(Token::Text(decode(&rest[..i])));
                rest = &rest[i..];
            }
        }
    }
    out
}

fn parse_tag(body: &str) -> Option<Tag> {
    let body = body.trim();
    if body.starts_with('!') || body.starts_with('?') {
        return None;
    }
    let (closing, body) = match body.strip_prefix('/') {
        Some(b) => (true, b),
        None => (false, body),
    };
    let (self_closing, body) = match body.strip_suffix('/') {
        Some(b) => (true, b),
        None => (false, body),
    };
    let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = body[..name_end].to_string();
    let mut attrs = Vec::new();
    let mut s = body[name_end..].trim_start();
    while let Some(eq) = s.find('=') {
        let key = s[..eq].trim().to_string();
        let after = s[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(close) = after[1..].find(q) else { break };
        attrs.push((key, decode(&after[1..1 + close])));
        s = after[close + 2..].trim_start();
    }
    Some(Tag { name, closing, self_closing, attrs })
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&e| e <= 10).and_then(|e| {
            let ch = match &rest[1..e] {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                n => {
                    let code = match n.strip_prefix("#x").or_else(|| n.strip_prefix("#X")) {
                        Some(h) => u32::from_str_radix(h, 16).ok()?,
                        None => n.strip_prefix('#')?.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((ch, e))
        });
        match decoded {
            Some((ch, e)) => {
                out.push(ch);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `strong:G25`, `strong:H07225` → `G25`, `H7225`. Takes the first Strong's number.
fn strongs_of(tag: &Tag) -> Option<String> {
    let value = tag.attr("lemma").or_else(|| tag.attr("savlm"))?;
    value.split_whitespace().find_map(|part| {
        let n = part.strip_prefix("strong:")?;
        let (kind, digits) = n.split_at(1);
        let digits = digits.trim_start_matches('0');
        let digits = digits.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        (matches!(kind, "G" | "H") && !digits.is_empty()).then(|| format!("{kind}{digits}"))
    })
}

enum Frame {
    /// Anything we have no special treatment for: its content flows into the parent.
    Plain,
    Word(Option<String>),
    Red,
    Emphasis,
    Divine,
    Note { keep: bool },
    Title { keep: bool },
    Reference(Option<RefRange>),
}

struct Open {
    name: String,
    frame: Frame,
    inlines: Vec<Inline>,
}

pub struct ChapterBuilder {
    book: Book,
    chapter: u16,
    blocks: Vec<Block>,
    para: Option<Paragraph>,
    /// Poetry indent of the line we are inside, if any.
    line: Option<u8>,
    verse: u16,
    shown: HashSet<u16>,
    /// A `<q who="Jesus" sID/>` is open.
    red: bool,
    pending: Vec<Inline>,
    stack: Vec<Open>,
    notes: u8,
}

impl ChapterBuilder {
    pub fn new(book: Book, chapter: u16) -> Self {
        ChapterBuilder {
            book,
            chapter,
            blocks: Vec::new(),
            para: None,
            line: None,
            verse: 0,
            shown: HashSet::new(),
            red: false,
            pending: Vec::new(),
            stack: Vec::new(),
            notes: 0,
        }
    }

    /// Feed the next verse's raw markup. Verse 0 is the chapter heading entry: only its
    /// titles are kept.
    pub fn verse(&mut self, verse: u16, raw: &str) {
        self.flush_run();
        self.verse = verse;
        for token in tokenize(raw) {
            match token {
                Token::Text(t) => self.text(&t),
                Token::Tag(tag) => self.tag(tag),
            }
        }
        // Unclosed elements do not leak into the next verse.
        while !self.stack.is_empty() {
            self.close(None);
        }
        self.flush_run();
    }

    pub fn finish(mut self) -> Chapter {
        self.flush_run();
        self.close_paragraph();
        Chapter { book: self.book, chapter: self.chapter, blocks: self.blocks }
    }

    fn out(&mut self) -> &mut Vec<Inline> {
        match self.stack.last_mut() {
            Some(open) => &mut open.inlines,
            None => &mut self.pending,
        }
    }

    fn text(&mut self, t: &str) {
        if t.is_empty() {
            return;
        }
        let inline = Inline::Text(collapse(t));
        let red = self.red && !self.stack.iter().any(|o| matches!(o.frame, Frame::Red));
        let skip = self.stack.iter().any(|o| matches!(o.frame, Frame::Note { keep: false } | Frame::Title { keep: false }));
        if skip {
            return;
        }
        let in_text_frame = self.stack.iter().any(|o| matches!(o.frame, Frame::Note { .. } | Frame::Title { .. }));
        self.out().push(if red && !in_text_frame { Inline::RedLetter(vec![inline]) } else { inline });
    }

    fn tag(&mut self, tag: Tag) {
        let sid = tag.attr("sID").is_some();
        let eid = tag.attr("eID").is_some();
        match tag.name.as_str() {
            "div" | "p" => {
                let ty = tag.attr("type").unwrap_or("");
                if tag.name == "p" || matches!(ty, "x-p" | "paragraph") {
                    self.flush_run();
                    self.close_paragraph();
                }
            }
            "lg" => {
                self.flush_run();
                self.close_paragraph();
                if tag.closing || eid {
                    self.line = None;
                }
            }
            "l" => {
                self.flush_run();
                self.close_paragraph();
                if tag.closing || eid {
                    self.line = None;
                } else {
                    let level: u8 = tag.attr("level").and_then(|l| l.parse().ok()).unwrap_or(1);
                    self.line = Some(level.saturating_sub(1));
                }
            }
            "q" if tag.attr("who") == Some("Jesus") && (sid || eid) => self.red = !eid,
            "lb" | "milestone" | "chapter" | "verse" | "catchWord" | "index" => {}
            _ if tag.closing => self.close(Some(&tag.name)),
            _ if tag.self_closing => {}
            _ => self.open(tag),
        }
    }

    fn open(&mut self, tag: Tag) {
        let in_note = self.stack.iter().any(|o| matches!(o.frame, Frame::Note { .. }));
        let frame = match tag.name.as_str() {
            "w" => Frame::Word(strongs_of(&tag)),
            "q" if tag.attr("who") == Some("Jesus") => Frame::Red,
            "hi" | "transChange" | "emph" | "foreign" | "i" => Frame::Emphasis,
            "divineName" => Frame::Divine,
            "note" => {
                let ty = tag.attr("type").unwrap_or("");
                Frame::Note { keep: !matches!(ty, "crossReference" | "x-strongsMarkup") && !in_note }
            }
            "title" => {
                let ty = tag.attr("type").unwrap_or("");
                Frame::Title { keep: !ty.contains("hapter") && !ty.starts_with("x-") && !in_note }
            }
            "reference" if in_note => Frame::Plain,
            "reference" => Frame::Reference(tag.attr("osisRef").and_then(osis_target)),
            _ => Frame::Plain,
        };
        // The "17:21" label some notes start with repeats where the note is attached.
        let hidden = in_note && tag.name == "reference" && tag.attr("type") == Some("annotateRef");
        let frame = if hidden { Frame::Title { keep: false } } else { frame };
        self.stack.push(Open { name: tag.name, frame, inlines: Vec::new() });
    }

    /// Close the nearest open element called `name` (or the innermost, for `None`).
    fn close(&mut self, name: Option<&str>) {
        let pos = match name {
            Some(n) => self.stack.iter().rposition(|o| o.name == n),
            None => self.stack.len().checked_sub(1),
        };
        let Some(pos) = pos else { return };
        while self.stack.len() > pos {
            let open = self.stack.pop().unwrap();
            self.finish_element(open);
        }
    }

    fn finish_element(&mut self, open: Open) {
        let Open { frame, inlines, .. } = open;
        match frame {
            Frame::Plain => self.out().extend(inlines),
            Frame::Word(Some(strongs)) => {
                let text = plain(&inlines);
                if text.trim().is_empty() {
                    self.out().extend(inlines);
                } else {
                    let (lead, core, trail) = split_space(&text);
                    let red = self.red && !self.stack.iter().any(|o| matches!(o.frame, Frame::Red));
                    let word = Inline::Word { text: core.to_string(), strongs };
                    let out = self.out();
                    if !lead.is_empty() {
                        out.push(Inline::Text(lead.into()));
                    }
                    out.push(if red { Inline::RedLetter(vec![word]) } else { word });
                    if !trail.is_empty() {
                        out.push(Inline::Text(trail.into()));
                    }
                }
            }
            Frame::Word(None) => self.out().extend(inlines),
            Frame::Red => self.out().push(Inline::RedLetter(inlines)),
            Frame::Emphasis => self.out().push(Inline::Emphasis(inlines)),
            Frame::Divine => {
                let t = plain(&inlines);
                let t = if t.chars().any(char::is_lowercase) { t } else { capitalize(&t) };
                self.out().push(Inline::DivineName(t));
            }
            Frame::Note { keep: true } => {
                let text = collapse(plain(&inlines).trim());
                if !text.is_empty() {
                    let id = ((b'a' + self.notes % 26) as char).to_string();
                    self.notes = self.notes.wrapping_add(1);
                    self.out().push(Inline::Footnote { id, text });
                }
            }
            Frame::Note { keep: false } | Frame::Title { keep: false } => {}
            Frame::Title { keep: true } => {
                let text = collapse(plain(&inlines).trim());
                if !text.is_empty() {
                    self.flush_run();
                    self.close_paragraph();
                    self.blocks.push(Block::Heading(text));
                }
            }
            Frame::Reference(Some(target)) => {
                let label = plain(&inlines);
                self.out().push(Inline::Ref { label, target });
            }
            Frame::Reference(None) => self.out().extend(inlines),
        }
    }

    fn close_paragraph(&mut self) {
        if let Some(p) = self.para.take()
            && !p.runs.is_empty()
        {
            self.blocks.push(Block::Paragraph(p));
        }
    }

    /// Turn the inlines gathered so far into a run of the current verse.
    fn flush_run(&mut self) {
        let mut inlines = std::mem::take(&mut self.pending);
        trim_edges(&mut inlines);
        if inlines.is_empty() || self.verse == 0 {
            return;
        }
        let style = match self.line {
            Some(indent) => ParagraphStyle::Poetry { indent },
            None => ParagraphStyle::Prose,
        };
        if self.para.as_ref().is_some_and(|p| p.style != style) {
            self.close_paragraph();
        }
        let show_number = self.shown.insert(self.verse);
        self.para
            .get_or_insert_with(|| Paragraph { style, runs: Vec::new() })
            .runs
            .push(Run { verse: self.verse, show_number, inlines });
    }
}

fn osis_target(osis_ref: &str) -> Option<RefRange> {
    // Only single chapter references are supported for now; the rest stay plain text.
    RefRange::from_osis(osis_ref.split_whitespace().next()?)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Runs of whitespace (including newlines) become one space.
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

fn split_space(s: &str) -> (&str, &str, &str) {
    let core = s.trim();
    let start = s.len() - s.trim_start().len();
    (&s[..start], core, &s[start + core.len()..])
}

fn plain(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) | Inline::DivineName(t) => out.push_str(t),
            Inline::Word { text, .. } => out.push_str(text),
            Inline::Ref { label, .. } => out.push_str(label),
            Inline::RedLetter(inner) | Inline::Emphasis(inner) => out.push_str(&plain(inner)),
            Inline::Footnote { .. } => {}
        }
    }
    out
}

/// Drop blank text at both ends of a run and the whitespace touching the edges.
fn trim_edges(inlines: &mut Vec<Inline>) {
    fn first_text(i: &mut Inline) -> Option<&mut String> {
        match i {
            Inline::Text(t) => Some(t),
            Inline::RedLetter(v) | Inline::Emphasis(v) => v.first_mut().and_then(first_text),
            _ => None,
        }
    }
    fn last_text(i: &mut Inline) -> Option<&mut String> {
        match i {
            Inline::Text(t) => Some(t),
            Inline::RedLetter(v) | Inline::Emphasis(v) => v.last_mut().and_then(last_text),
            _ => None,
        }
    }
    fn blank(i: &Inline) -> bool {
        match i {
            Inline::Text(t) => t.trim().is_empty(),
            Inline::RedLetter(v) | Inline::Emphasis(v) => v.iter().all(blank),
            _ => false,
        }
    }
    while inlines.first().is_some_and(blank) {
        inlines.remove(0);
    }
    while inlines.last().is_some_and(blank) {
        inlines.pop();
    }
    if let Some(t) = inlines.first_mut().and_then(first_text) {
        *t = t.trim_start().to_string();
    }
    if let Some(t) = inlines.last_mut().and_then(last_text) {
        *t = t.trim_end().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(verse: u16, raw: &str) -> Chapter {
        let mut b = ChapterBuilder::new(Book::JOHN, 3);
        b.verse(verse, raw);
        b.finish()
    }

    fn runs(c: &Chapter) -> Vec<&Run> {
        c.blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
            .flat_map(|p| p.runs.iter())
            .collect()
    }

    #[test]
    fn strongs_words_keep_their_number_without_padding() {
        let c = one(16, r#"<w lemma="strong:G2316">God</w> <w lemma="strong:H07225 strong:H1">so</w> plain"#);
        let r = runs(&c)[0];
        assert_eq!(
            r.inlines,
            vec![
                Inline::Word { text: "God".into(), strongs: "G2316".into() },
                Inline::Text(" ".into()),
                Inline::Word { text: "so".into(), strongs: "H7225".into() },
                Inline::Text(" plain".into()),
            ]
        );
    }

    #[test]
    fn a_word_with_no_number_is_just_text() {
        let c = one(1, r#"<w morph="x">Word</w>"#);
        assert_eq!(runs(&c)[0].inlines, vec![Inline::Text("Word".into())]);
    }

    #[test]
    fn words_of_jesus_span_verses_with_milestones() {
        let mut b = ChapterBuilder::new(Book::JOHN, 3);
        b.verse(3, r#"<q who="Jesus" sID="q1"/>Verily, verily"#);
        b.verse(4, r#"I say.<q who="Jesus" eID="q1"/> Nicodemus saith"#);
        let c = b.finish();
        let r = runs(&c);
        assert_eq!(r[0].inlines, vec![Inline::RedLetter(vec![Inline::Text("Verily, verily".into())])]);
        assert_eq!(r[1].inlines[0], Inline::RedLetter(vec![Inline::Text("I say.".into())]));
        assert_eq!(r[1].inlines[1], Inline::Text(" Nicodemus saith".into()));
    }

    #[test]
    fn ordinary_quotes_are_not_red() {
        let c = one(1, r#"<q marker="">“For God</q>"#);
        assert_eq!(runs(&c)[0].inlines, vec![Inline::Text("“For God".into())]);
    }

    #[test]
    fn notes_become_footnotes_and_drop_their_own_label() {
        let c = one(
            2,
            r#"by night<note placement="foot"><reference type="annotateRef">3:2 </reference>Some read <rdg type="alt">dark</rdg>.</note>"#,
        );
        assert_eq!(
            runs(&c)[0].inlines,
            vec![
                Inline::Text("by night".into()),
                Inline::Footnote { id: "a".into(), text: "Some read dark.".into() }
            ]
        );
    }

    #[test]
    fn cross_reference_notes_are_dropped() {
        let c = one(2, r#"text<note type="crossReference"><reference osisRef="Num.21.9">Num 21:9</reference></note>"#);
        assert_eq!(runs(&c)[0].inlines, vec![Inline::Text("text".into())]);
    }

    #[test]
    fn titles_are_headings_before_the_verse() {
        let mut b = ChapterBuilder::new(Book::PSALMS, 23);
        b.verse(1, r#"<title canonical="true" type="psalm">A Psalm of David.</title> <l level="1" sID="a"/>The shepherd<l level="1" eID="a"/>"#);
        let c = b.finish();
        assert_eq!(c.blocks[0], Block::Heading("A Psalm of David.".into()));
        assert!(matches!(&c.blocks[1], Block::Paragraph(p) if p.style == ParagraphStyle::Poetry { indent: 0 }));
    }

    #[test]
    fn poetry_levels_become_indents_and_each_line_its_own_paragraph() {
        let mut b = ChapterBuilder::new(Book::PSALMS, 23);
        b.verse(1, r#"<lg sID="g"/><l level="1">The Lord is my shepherd;</l><l level="2">I shall not want.</l>"#);
        b.verse(2, r#"<l level="1">He maketh me lie down</l><lg eID="g"/>"#);
        let c = b.finish();
        let styles: Vec<_> = c
            .blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some((p.style, p.runs[0].verse, p.runs[0].show_number)) } else { None })
            .collect();
        assert_eq!(
            styles,
            vec![
                (ParagraphStyle::Poetry { indent: 0 }, 1, true),
                (ParagraphStyle::Poetry { indent: 1 }, 1, false),
                (ParagraphStyle::Poetry { indent: 0 }, 2, true),
            ]
        );
    }

    #[test]
    fn prose_paragraphs_break_at_paragraph_milestones() {
        let mut b = ChapterBuilder::new(Book::JOHN, 3);
        b.verse(1, r#"<div sID="p1" type="x-p"/>One.<div eID="p1" type="x-p"/>"#);
        b.verse(2, r#"<div sID="p2" type="x-p"/>Two. "#);
        b.verse(3, "Three.");
        let c = b.finish();
        let ps: Vec<usize> = c
            .blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some(p.runs.len()) } else { None })
            .collect();
        assert_eq!(ps, vec![1, 2]);
    }

    #[test]
    fn divine_name_is_set_in_small_caps_form() {
        let c = one(1, "<divineName>LORD</divineName> is");
        assert_eq!(runs(&c)[0].inlines[0], Inline::DivineName("Lord".into()));
    }

    #[test]
    fn entities_and_whitespace_are_cleaned() {
        let c = one(1, "a &amp; b\n  c &#8217;s &unknown; x");
        assert_eq!(runs(&c)[0].inlines, vec![Inline::Text("a & b c ’s &unknown; x".into())]);
    }

    #[test]
    fn unclosed_elements_do_not_leak_into_the_next_verse() {
        let mut b = ChapterBuilder::new(Book::JOHN, 3);
        b.verse(1, "<hi>never closed");
        b.verse(2, "plain");
        let c = b.finish();
        assert_eq!(runs(&c)[1].inlines, vec![Inline::Text("plain".into())]);
    }

    #[test]
    fn chapter_heading_entry_contributes_only_titles() {
        let mut b = ChapterBuilder::new(Book::JOHN, 3);
        b.verse(0, r#"<title type="main">Nicodemus</title> stray text"#);
        b.verse(1, "There was a man");
        let c = b.finish();
        assert_eq!(c.blocks[0], Block::Heading("Nicodemus".into()));
        assert_eq!(runs(&c).len(), 1);
    }
}
