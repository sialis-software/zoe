//! Inline AST that every SWORD markup (OSIS/ThML/GBF/…) is rendered into (docs/01 §5).

use scripture::{Book, RefRange};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub book: Book,
    pub chapter: u16,
    pub blocks: Vec<Block>,
}

/// Text of inlines as scripture text: footnote markers are dropped, everything else kept.
fn flatten(items: &[Inline], out: &mut String) {
    for item in items {
        match item {
            Inline::Text(t) | Inline::DivineName(t) => out.push_str(t),
            Inline::Word { text, .. } => out.push_str(text),
            Inline::Ref { label, .. } => out.push_str(label),
            Inline::RedLetter(inner) | Inline::Emphasis(inner) => flatten(inner, out),
            Inline::Footnote { .. } | Inline::CrossRef { .. } => {}
        }
    }
}

/// Where a footnote sits: its verse and the byte offset in that verse's canonical text
/// where its marker comes (the text is unchanged by the footnote).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootnoteAt {
    pub verse: u16,
    pub offset: usize,
    pub text: String,
}

/// Where a cross reference marker sits (as [`FootnoteAt`]), and the passages it points to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossRefAt {
    pub verse: u16,
    pub offset: usize,
    pub refs: Vec<RefRange>,
}

/// Where a word with a Strong's number sits: its verse and the byte range of the word in that
/// verse's canonical text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordAt {
    pub verse: u16,
    pub start: usize,
    pub end: usize,
    pub strongs: String,
}

/// A marker in the text that has something to say.
enum Marker {
    Footnote(String),
    CrossRef(Vec<RefRange>),
    Word { strongs: String, len: usize },
}

/// Like [`flatten`], also noting where each footnote's and cross reference's marker falls in the
/// output.
fn flatten_noting(items: &[Inline], out: &mut String, notes: &mut Vec<(usize, Marker)>) {
    for item in items {
        match item {
            Inline::Footnote { text, .. } => notes.push((out.len(), Marker::Footnote(text.clone()))),
            Inline::CrossRef { refs } => notes.push((out.len(), Marker::CrossRef(refs.clone()))),
            Inline::Word { text, strongs } => {
                notes.push((out.len(), Marker::Word { strongs: strongs.clone(), len: text.len() }));
                out.push_str(text);
            }
            Inline::RedLetter(inner) | Inline::Emphasis(inner) => flatten_noting(inner, out, notes),
            other => flatten(std::slice::from_ref(other), out),
        }
    }
}

impl Chapter {
    /// Every footnote with its place in the canonical text, in reading order. A footnote in a
    /// verse with no text of its own is placed at offset 0.
    pub fn footnotes(&self) -> Vec<FootnoteAt> {
        self.markers()
            .into_iter()
            .filter_map(|(verse, offset, marker)| match marker {
                Marker::Footnote(text) => Some(FootnoteAt { verse, offset, text }),
                Marker::CrossRef(_) | Marker::Word { .. } => None,
            })
            .collect()
    }

    /// Every cross reference marker with its place, placed as footnotes are.
    pub fn cross_references(&self) -> Vec<CrossRefAt> {
        self.markers()
            .into_iter()
            .filter_map(|(verse, offset, marker)| match marker {
                Marker::CrossRef(refs) => Some(CrossRefAt { verse, offset, refs }),
                Marker::Footnote(_) | Marker::Word { .. } => None,
            })
            .collect()
    }

    /// Every word that carries a Strong's number, with its place, in reading order.
    pub fn strongs_words(&self) -> Vec<WordAt> {
        self.markers()
            .into_iter()
            .filter_map(|(verse, start, marker)| match marker {
                Marker::Word { strongs, len } => Some(WordAt { verse, start, end: start + len, strongs }),
                _ => None,
            })
            .collect()
    }

    fn markers(&self) -> Vec<(u16, usize, Marker)> {
        let mut lens: BTreeMap<u16, usize> = BTreeMap::new();
        let mut out = Vec::new();
        for block in &self.blocks {
            let Block::Paragraph(p) = block else { continue };
            for run in &p.runs {
                let (mut part, mut notes) = (String::new(), Vec::new());
                flatten_noting(&run.inlines, &mut part, &mut notes);
                // Mirrors canonical_verses: a run joins its verse's earlier text after a space.
                let base = match lens.get(&run.verse) {
                    Some(len) if !part.is_empty() => len + 1,
                    Some(len) => *len,
                    None => 0,
                };
                out.extend(notes.into_iter().map(|(at, marker)| {
                    (run.verse, if part.is_empty() { base } else { base + at }, marker)
                }));
                if !part.is_empty() {
                    *lens.entry(run.verse).or_insert(0) = base + part.len();
                }
            }
        }
        out
    }

    /// The canonical text of each verse: what marks anchor to (docs/05 §3). Unchanged by
    /// display options (Strong's, footnotes, red letter). A verse split over several runs
    /// (e.g. poetry lines) has its parts joined by one space.
    pub fn canonical_verses(&self) -> BTreeMap<u16, String> {
        let mut verses: BTreeMap<u16, String> = BTreeMap::new();
        for block in &self.blocks {
            let Block::Paragraph(p) = block else { continue };
            for run in &p.runs {
                let mut part = String::new();
                flatten(&run.inlines, &mut part);
                if part.is_empty() {
                    continue;
                }
                match verses.get_mut(&run.verse) {
                    Some(text) => {
                        text.push(' ');
                        text.push_str(&part);
                    }
                    None => {
                        verses.insert(run.verse, part);
                    }
                }
            }
        }
        verses
    }

    /// Plain text of the verses in `range` (footnotes dropped), for previews and embeds.
    pub fn plain_text(&self, range: &RefRange) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            let Block::Paragraph(p) = block else { continue };
            for run in &p.runs {
                let wanted = range.book == self.book
                    && range.chapter == self.chapter
                    && range.verses.is_none_or(|(a, b)| (a..=b).contains(&run.verse));
                if wanted {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    flatten(&run.inlines, &mut out);
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(String),
    Paragraph(Paragraph),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub style: ParagraphStyle,
    pub runs: Vec<Run>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphStyle {
    Prose,
    /// One line of poetry with an indent level (0, 1, 2…).
    Poetry { indent: u8 },
}

/// A fragment of a single verse. A verse can span several runs (and paragraphs),
/// so `show_number` marks the one that carries the verse number.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub verse: u16,
    pub show_number: bool,
    pub inlines: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    /// Words of Jesus.
    RedLetter(Vec<Inline>),
    /// LORD (YHWH), usually rendered in small caps.
    DivineName(String),
    Emphasis(Vec<Inline>),
    /// A word carrying a Strong's number, e.g. `G25`.
    Word { text: String, strongs: String },
    Footnote { id: String, text: String },
    /// A marker for the passages the module points to from here (its cross references). Like a
    /// footnote it is not part of the verse's text.
    CrossRef { refs: Vec<RefRange> },
    /// A scripture reference with display text.
    Ref { label: String, target: RefRange },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(t: &str) -> Inline {
        Inline::Footnote { id: "a".into(), text: t.into() }
    }

    fn text(t: &str) -> Inline {
        Inline::Text(t.into())
    }

    fn chapter(runs: Vec<Run>) -> Chapter {
        let blocks = vec![Block::Paragraph(Paragraph { style: ParagraphStyle::Prose, runs })];
        Chapter { book: Book::JOHN, chapter: 3, blocks }
    }

    fn run(verse: u16, inlines: Vec<Inline>) -> Run {
        Run { verse, show_number: true, inlines }
    }

    #[test]
    fn footnote_offsets_index_the_canonical_text() {
        let c = chapter(vec![
            run(1, vec![text("By "), note("one"), text("night"), Inline::Emphasis(vec![text(" and day"), note("two")])]),
            run(1, vec![text("again"), note("three")]),
            run(2, vec![note("only a note")]),
            run(3, vec![text("é"), note("four")]),
        ]);
        let canon = c.canonical_verses();
        assert_eq!(canon[&1], "By night and day again");
        let notes = c.footnotes();
        let at = |i: usize| (notes[i].verse, notes[i].offset, notes[i].text.as_str());
        assert_eq!(at(0), (1, 3, "one"));
        assert_eq!(&canon[&1][..notes[0].offset], "By ");
        assert_eq!(at(1), (1, 16, "two"));
        assert_eq!(&canon[&1][..notes[1].offset], "By night and day");
        assert_eq!(at(2), (1, 22, "three"), "after a second run, past the joining space");
        assert_eq!(at(3), (2, 0, "only a note"));
        assert_eq!(at(4), (3, "é".len(), "four"));
        assert!(!canon.contains_key(&2));
    }
}
