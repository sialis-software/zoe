//! The person's own commentary: a running, ever-changing set of conclusions about each chapter and
//! verse of a book. It is an ordinary note, one for each book, whose only difference is its
//! structure: a heading for each chapter and under it a heading for each verse (or run of verses)
//! written on, in the order of the text.
//!
//! ```text
//! # Commentary on John
//!
//! ## John 3
//!
//! What the chapter as a whole shows.
//!
//! ### John 3:16
//!
//! What this verse shows.
//! ```

use scripture::{Book, RefRange};
use std::ops::Range;

/// Where the commentary on `book` is kept.
pub fn path(book: Book) -> String {
    format!("notes/Commentary on {}.md", book.name())
}

/// The book a commentary note's path is for, if it is one.
pub fn book_of(path: &str) -> Option<Book> {
    let name = path.strip_prefix("notes/Commentary on ")?.strip_suffix(".md")?;
    Book::all().find(|b| b.name() == name)
}

/// A new commentary on `book`: its properties and title.
pub fn new_book(book: Book, created: &str) -> String {
    format!("---\ncreated: {created}\ntype: commentary\ntags: [commentary]\nbook: {}\n---\n# Commentary on {}\n", book.name(), book.name())
}

/// A chapter or verse section of a commentary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub range: RefRange,
    /// Where its heading line starts.
    pub heading: usize,
    /// From the line after the heading to where the next section begins (or the end).
    pub body: Range<usize>,
}

impl Section {
    /// Where writing in the section begins: under its heading and the empty line after it.
    pub fn write_at(&self, text: &str) -> usize {
        let after = self.body.start.min(text.len());
        if text[after..].starts_with('\n') { after + 1 } else { after }
    }

    pub fn is_chapter(&self) -> bool {
        self.range.verses.is_none()
    }
}

/// The sections of `text`, in the order written: a `## John 3` for a chapter and a `### John 3:16`
/// (or `### John 3:16-21`) for verses. A heading that is not one of these is not a section (and is
/// part of the body of the one before it).
pub fn sections(text: &str, book: Book) -> Vec<Section> {
    let mut found: Vec<Section> = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let heading = if let Some(rest) = line.strip_prefix("### ") {
            RefRange::parse(rest.trim()).filter(|r| r.verses.is_some())
        } else if let Some(rest) = line.strip_prefix("## ") {
            RefRange::parse(rest.trim()).filter(|r| r.verses.is_none())
        } else {
            None
        }
        .filter(|r| r.book == book);
        if let Some(range) = heading {
            if let Some(last) = found.last_mut() {
                last.body.end = at;
            }
            found.push(Section { range, heading: at, body: at + line.len()..text.len() });
        } else if line.starts_with("# ") || (line.starts_with("## ") && !found.is_empty()) {
            // Another heading of the same or a higher level ends the section before it.
            if let Some(last) = found.last_mut() {
                last.body.end = last.body.end.min(at);
            }
        }
        at += line.len();
    }
    found
}

fn start_of(range: &RefRange) -> (u16, u16) {
    (range.chapter, range.verses.map_or(0, |v| v.0))
}

/// `text` with a section for `range` in it (and for its chapter, if that has none), put where the
/// order of the text puts it, and the offset in the new text where what is written under it begins.
/// If there is one already, the text is as it was.
pub fn ensure(text: &str, range: RefRange) -> (String, usize) {
    let all = sections(text, range.book);
    let find = |wanted: &RefRange| all.iter().find(|s| s.range == *wanted);
    let mut text = text.to_string();
    // The chapter first.
    let chapter = RefRange::chapter(range.book, range.chapter);
    if find(&chapter).is_none() {
        let before = all.iter().find(|s| s.range.chapter > chapter.chapter).map(|s| s.heading);
        insert_heading(&mut text, before, &format!("## {chapter}"));
    }
    if range.verses.is_none() {
        let all = sections(&text, range.book);
        let at = all.iter().find(|s| s.range == chapter).map_or(text.len(), |s| s.write_at(&text));
        let at = at.min(text.len());
        return (text, at);
    }
    let all = sections(&text, range.book);
    if let Some(existing) = all.iter().find(|s| s.range == range) {
        let at = existing.write_at(&text);
        return (text, at);
    }
    // Before the first section after it in this chapter, else at the end of the chapter's.
    let chapter_end = all.iter().find(|s| s.range == chapter).map(|s| (s.heading, all.iter().skip_while(|x| x.heading <= s.heading).find(|x| x.is_chapter()).map_or(text.len(), |x| x.heading)));
    let (_, end_of_chapter) = chapter_end.unwrap_or((0, text.len()));
    let next = all
        .iter()
        .filter(|s| !s.is_chapter() && s.range.chapter == range.chapter && s.heading < end_of_chapter)
        .find(|s| start_of(&s.range) > start_of(&range))
        .map(|s| s.heading);
    let len = text.len();
    insert_heading(&mut text, Some(next.unwrap_or(end_of_chapter)).filter(|&at| at < len), &format!("### {range}"));
    let all = sections(&text, range.book);
    let at = all.iter().find(|s| s.range == range).map_or(text.len(), |s| s.write_at(&text));
    (text, at)
}

/// Put `heading` (and an empty line after it) into `text` before the byte offset `before`, or at the
/// end, with an empty line before it.
fn insert_heading(text: &mut String, before: Option<usize>, heading: &str) {
    match before {
        Some(at) => {
            let lead = if text[..at].ends_with("\n\n") || at == 0 { "" } else { "\n" };
            text.insert_str(at, &format!("{lead}{heading}\n\n"));
        }
        None => {
            while text.ends_with("\n\n") {
                text.pop();
            }
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("\n{heading}\n\n"));
        }
    }
}

/// The notes on `chapter` as the commentary tab reads them: each section's heading is a link to its
/// passage, with a link `edit(offset)` gives for writing in it. Empty if there is none.
pub fn chapter_source(text: &str, book: Book, chapter: u16, edit: impl Fn(&Section) -> String) -> String {
    let mut out = Vec::new();
    for section in sections(text, book).iter().filter(|s| s.range.chapter == chapter) {
        let body = text[section.body.clone()].trim().to_string();
        let head = format!("[[{}]]  \u{b7}  {}", section.range, edit(section));
        out.push(if body.is_empty() { head } else { format!("{head}\n\n{body}") });
    }
    out.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn john(chapter: u16, verses: Option<(u16, u16)>) -> RefRange {
        RefRange { book: Book::JOHN, chapter, verses }
    }

    fn text_of(sections: &[&str]) -> String {
        let mut text = new_book(Book::JOHN, "2025-10-13T08:00:00");
        for s in sections {
            text.push_str(&format!("\n{s}\n"));
        }
        text
    }

    #[test]
    fn a_book_has_its_own_path_that_names_it_back() {
        assert_eq!(path(Book::JOHN), "notes/Commentary on John.md");
        assert_eq!(book_of("notes/Commentary on John.md"), Some(Book::JOHN));
        assert_eq!(book_of("notes/Nicodemus.md"), None);
        assert_eq!(book_of("notes/Commentary on Narnia.md"), None);
    }

    #[test]
    fn sections_are_the_chapter_and_verse_headings_in_the_order_written() {
        let text = text_of(&["## John 3\n\nThe chapter.\n\n### John 3:16\n\nLove.\n\n### John 3:17-21\n\nSaving.", "## John 4\n\nWell.\n\n## Not a passage\n\nx"]);
        let found = sections(&text, Book::JOHN);
        let ranges: Vec<String> = found.iter().map(|s| s.range.to_string()).collect();
        assert_eq!(ranges, ["John 3", "John 3:16", "John 3:17-21", "John 4"]);
        assert!(text[found[1].body.clone()].contains("Love.") && !text[found[1].body.clone()].contains("Saving"));
        assert!(text[found[0].body.clone()].contains("The chapter.") && !text[found[0].body.clone()].contains("Love"));
        assert!(sections(&text, Book::GENESIS).is_empty(), "another book's headings are not this book's");
    }

    #[test]
    fn a_section_is_put_where_the_order_of_the_text_puts_it_with_its_chapter_when_that_is_new() {
        let (text, at) = ensure(&new_book(Book::JOHN, "x"), john(3, Some((16, 16))));
        assert!(text.ends_with("## John 3\n\n### John 3:16\n\n"), "{text:?}");
        assert_eq!(at, text.len(), "writing begins under it");
        // A later verse goes after, an earlier one before, a different chapter in its place.
        let (text, _) = ensure(&text, john(3, Some((36, 36))));
        let (text, _) = ensure(&text, john(3, Some((1, 2))));
        let (text, _) = ensure(&text, john(1, None));
        let (text, _) = ensure(&text, john(2, Some((5, 5))));
        let order: Vec<String> = sections(&text, Book::JOHN).iter().map(|s| s.range.to_string()).collect();
        assert_eq!(order, ["John 1", "John 2", "John 2:5", "John 3", "John 3:1-2", "John 3:16", "John 3:36"]);
        // What is there is not made again, and the offset is where its body begins.
        let (again, at) = ensure(&text, john(3, Some((16, 16))));
        assert_eq!(again, text);
        assert!(again[..at].ends_with("### John 3:16\n\n") || again[..at].ends_with("### John 3:16\n"), "{:?}", &again[at.saturating_sub(20)..at]);
    }

    #[test]
    fn what_is_written_in_a_section_stays_in_it_as_others_are_made_around_it() {
        let text = text_of(&["## John 3\n\n### John 3:16\n\nLove is the root.\n\n### John 3:36\n\nBelieving."]);
        let (text, _) = ensure(&text, john(3, Some((17, 17))));
        let found = sections(&text, Book::JOHN);
        let body = |r: RefRange| found.iter().find(|s| s.range == r).map(|s| text[s.body.clone()].trim().to_string()).unwrap();
        assert_eq!(body(john(3, Some((16, 16)))), "Love is the root.");
        assert_eq!(body(john(3, Some((17, 17)))), "");
        assert_eq!(body(john(3, Some((36, 36)))), "Believing.");
    }

    #[test]
    fn a_chapter_is_read_as_links_over_what_was_written() {
        let text = text_of(&["## John 3\n\nWhole chapter.\n\n### John 3:16\n\nLove.\n\n## John 4\n\nWell."]);
        let source = chapter_source(&text, Book::JOHN, 3, |s| format!("[[x#{}|Edit]]", s.range));
        assert_eq!(source, "[[John 3]]  \u{b7}  [[x#John 3|Edit]]\n\nWhole chapter.\n\n[[John 3:16]]  \u{b7}  [[x#John 3:16|Edit]]\n\nLove.");
        assert_eq!(chapter_source(&text, Book::JOHN, 5, |_| String::new()), "");
    }
}
