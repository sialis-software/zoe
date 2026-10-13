//! The commentary tab: a commentary's notes on one chapter, verse by verse, with pickers for the
//! commentary, book and chapter. Passages the notes point to are links that show the verse in the
//! reader. Self-contained like the other tab types: its own state, messages, update and view.

use crate::bible_tab::BookChoice;
use crate::live_editor::live_editor;
use crate::palette::Palette;
use crate::shared::{Shared, TabId};
use iced::widget::{button, column, container, pick_list, row, rule};
use iced::{Element, Length};
use md_editor::{Action, Document};
use scripture::{Book, RefRange};
use std::fmt;
use study_core::ModuleId;
use study_core::commentary::Paragraph;
use study_core::markdown::LinkTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentaryChoice(pub ModuleId, pub String);

impl fmt::Display for CommentaryChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

#[derive(Debug, Clone)]
pub enum CommentaryMsg {
    PickModule(CommentaryChoice),
    PickBook(BookChoice),
    PickChapter(u16),
    PrevChapter,
    NextChapter,
    /// Selecting and moving in the text (what would change it is ignored).
    Action(Action),
    /// A link in the text was followed.
    Link(LinkTarget),
}

pub struct CommentaryTab {
    pub id: TabId,
    pub module: ModuleId,
    pub book: Book,
    pub chapter: u16,
    /// The verse asked for, which the text is scrolled to when the tab opens on it.
    pub verse: Option<u16>,
    /// The chapter's notes as markdown, shown by the reader's own text widget so that it can be
    /// selected and copied like any text. It is never changed.
    pub doc: Document,
}

impl CommentaryTab {
    pub fn new(id: TabId, sh: &Shared, module: ModuleId, book: Book, chapter: u16, verse: Option<u16>) -> Self {
        let mut tab = CommentaryTab { id, module, book, chapter: chapter.max(1), verse, doc: Document::new("") };
        tab.rebuild(sh);
        tab
    }

    pub fn editor_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("commentary-{}", self.id))
    }

    pub fn title(&self, sh: &Shared) -> String {
        let name = sh.backend.commentaries().into_iter().find(|c| c.id == self.module).map_or_else(|| self.module.0.clone(), |c| c.abbreviation);
        format!("{name} \u{00B7} {} {}", self.book, self.chapter)
    }

    /// Go to a passage.
    pub fn show(&mut self, sh: &Shared, book: Book, chapter: u16, verse: Option<u16>) {
        (self.book, self.chapter, self.verse) = (book, chapter.max(1), verse);
        self.rebuild(sh);
    }

    pub fn update(&mut self, sh: &mut Shared, message: CommentaryMsg) {
        match message {
            CommentaryMsg::PickModule(CommentaryChoice(module, _)) => {
                self.module = module;
                self.rebuild(sh);
            }
            CommentaryMsg::PickBook(BookChoice(book)) => self.show(sh, book, 1, None),
            CommentaryMsg::PickChapter(chapter) => self.show(sh, self.book, chapter, None),
            CommentaryMsg::PrevChapter => {
                if let Some((book, chapter)) = self.neighbour(false) {
                    self.show(sh, book, chapter, None);
                }
            }
            CommentaryMsg::NextChapter => {
                if let Some((book, chapter)) = self.neighbour(true) {
                    self.show(sh, book, chapter, None);
                }
            }
            // The text is read, not written: only what selects or moves is taken.
            CommentaryMsg::Action(action) => {
                if matches!(
                    action,
                    Action::Move { .. }
                        | Action::SetCaret { .. }
                        | Action::Select(_)
                        | Action::SelectBetween { .. }
                        | Action::SelectAll
                        | Action::SelectWord(_)
                        | Action::SelectLine(_)
                ) {
                    self.doc.perform(action);
                }
            }
            // Showing a verse in the reader is for the workspace to do.
            CommentaryMsg::Link(_) => {}
        }
    }

    /// The chapter before or after this one, across books.
    fn neighbour(&self, forward: bool) -> Option<(Book, u16)> {
        let books: Vec<Book> = Book::all().collect();
        let at = books.iter().position(|&b| b == self.book)?;
        if forward {
            if self.chapter < sword::chapter_count(self.book) {
                Some((self.book, self.chapter + 1))
            } else {
                books.get(at + 1).map(|&b| (b, 1))
            }
        } else if self.chapter > 1 {
            Some((self.book, self.chapter - 1))
        } else {
            at.checked_sub(1).map(|i| (books[i], sword::chapter_count(books[i])))
        }
    }

    /// The notes of the chapter in verse order: each with the first verse it is on. A note that
    /// covers several verses is the same at each, and shows once.
    pub fn notes(&self, sh: &Shared) -> Vec<(u16, Vec<Paragraph>)> {
        let mut out: Vec<(u16, Vec<Paragraph>)> = Vec::new();
        for verse in 1..=sword::verse_count(self.book, self.chapter) {
            let Some(note) = sh.backend.commentary_note(&self.module, self.book, self.chapter, verse) else { continue };
            if out.last().is_none_or(|(_, last)| *last != note) {
                out.push((verse, note));
            }
        }
        out
    }

    /// Make the text of the chapter's notes, with the caret where the tab should be showing (the
    /// verse asked for, else the top).
    fn rebuild(&mut self, sh: &Shared) {
        let before = self.doc.selection().head;
        let mut source = String::new();
        let mut caret = None;
        let notes = self.notes(sh);
        if notes.is_empty() {
            source = format!("This commentary has no notes on {} {}.", self.book, self.chapter);
        }
        for (verse, paragraphs) in &notes {
            let place = RefRange { book: self.book, chapter: self.chapter, verses: Some((*verse, *verse)) };
            if !source.is_empty() {
                source.push_str("\n\n");
            }
            // The verse asked for is the note it falls in.
            if self.verse.is_some_and(|v| notes.iter().rposition(|(n, _)| *n <= v).is_some_and(|at| notes[at].0 == *verse)) {
                caret = Some(source.len());
            }
            source.push_str(&format!("[[{place}]]"));
            for paragraph in paragraphs {
                source.push_str("\n\n");
                source.push_str(&markdown(paragraph));
            }
        }
        let mut doc = Document::new(&source);
        // The widget scrolls to the caret when it moves, so put it somewhere other than where it
        // was: the top of a new chapter, or the verse asked for.
        let at = caret.unwrap_or(if before == 0 { 1.min(source.len()) } else { 0 });
        doc.perform(Action::SetCaret { offset: at, extend: false });
        self.doc = doc;
    }

    pub fn view<'a>(&'a self, sh: &'a Shared) -> Element<'a, CommentaryMsg> {
        let commentaries: Vec<CommentaryChoice> = sh.backend.commentaries().into_iter().map(|c| CommentaryChoice(c.id, c.abbreviation)).collect();
        let current = commentaries.iter().find(|c| c.0 == self.module).cloned();
        let books: Vec<BookChoice> = Book::all().map(BookChoice).collect();
        let chapters: Vec<u16> = (1..=sword::chapter_count(self.book)).collect();
        let arrow = |icon: crate::icons::Icon, on: bool, msg: CommentaryMsg| {
            let b = button(container(icon.view(18.0)).center(Length::Fill)).width(31).height(31).padding(0).style(button::secondary);
            if on { b.on_press(msg) } else { b }
        };
        let bar = row![
            row![
                arrow(crate::icons::Icon::Back, self.neighbour(false).is_some(), CommentaryMsg::PrevChapter),
                arrow(crate::icons::Icon::Forward, self.neighbour(true).is_some(), CommentaryMsg::NextChapter),
            ]
            .spacing(4),
            pick_list(commentaries, current, CommentaryMsg::PickModule),
            pick_list(books, Some(BookChoice(self.book)), CommentaryMsg::PickBook),
            pick_list(chapters, Some(self.chapter), CommentaryMsg::PickChapter),
        ]
        .spacing(16)
        .align_y(iced::Center);

        let text = live_editor(&self.doc, Palette::for_theme(&sh.theme()))
            .id(self.editor_id())
            .reading(true)
            .copy_as(plain_text)
            .follow_links(true)
            .on_action(CommentaryMsg::Action)
            .on_link(CommentaryMsg::Link)
            .size(sh.settings.notes_size)
            .font(sh.settings.text_font())
            .padding(24)
            .height(Length::Fill);
        column![container(bar).padding([8, 16]), rule::horizontal(1), text].into()
    }
}

/// A paragraph as markdown: bold and italic as the module has them, passages as `[[…]]` links
/// that keep the words the note used.
fn markdown(paragraph: &Paragraph) -> String {
    let mut out = String::new();
    for span in paragraph {
        // Spaces at the ends stay outside the marks, which cannot touch them from inside.
        let inner = span.text.trim();
        if inner.is_empty() {
            out.push_str(&span.text);
            continue;
        }
        let lead = &span.text[..span.text.len() - span.text.trim_start().len()];
        let tail = &span.text[span.text.trim_end().len()..];
        let body = match span.passage.as_deref().and_then(passage_range) {
            Some(range) => format!("[[{range}|{}]]", inner.replace(['|', '[', ']'], "")),
            None => escape(inner),
        };
        let marks = match (span.bold, span.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        out.push_str(&format!("{lead}{marks}{body}{marks}{tail}"));
    }
    // A paragraph that would read as a list, a heading or a quote.
    let digits = out.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && out[digits..].starts_with(['.', ')']) {
        out.insert(digits, '\\');
    } else if out.starts_with(['-', '+', '>', '#']) {
        out.insert(0, '\\');
    }
    out
}

/// Text with what markdown would read as marks made literal.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '[' | ']' | '`' | '<' | '>' | '~' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// What copying selected notes puts on the clipboard: the words, without the marks that style
/// them or the brackets of links.
pub fn plain_text(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '*' => {}
            '[' if chars.peek() == Some(&'[') => {
                chars.next();
                let mut inner = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    inner.push(c);
                }
                chars.next_if_eq(&']');
                out.push_str(inner.rsplit_once('|').map_or(inner.as_str(), |(_, shown)| shown));
            }
            _ => out.push(c),
        }
    }
    out
}

/// The first passage a note's reference names: `Joh 2:23,24` is John 2:23; `John.3.16` is that.
pub fn passage_range(passage: &str) -> Option<RefRange> {
    let first = passage.split(';').next()?.trim();
    RefRange::from_osis(first).or_else(|| {
        let first = first.split(',').next()?.trim();
        RefRange::parse(first)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_markdown_that_reads_back_as_the_words() {
        use study_core::commentary::Span;
        let span = |t: &str, bold, italic, passage: Option<&str>| Span { text: t.into(), bold, italic, passage: passage.map(String::from) };
        let paragraph = vec![
            span("1. Not a list, a ", false, false, None),
            span("bold ", true, false, None),
            span("and *odd* [text]", false, true, None),
            span(" see ", false, false, None),
            span("Joh 2:23,24", false, false, Some("Joh 2:23,24")),
        ];
        let md = markdown(&paragraph);
        assert_eq!(md, "1\\. Not a list, a **bold** *and \\*odd\\* \\[text\\]* see [[John 2:23|Joh 2:23,24]]");
        assert_eq!(plain_text(&md), "1. Not a list, a bold and *odd* [text] see Joh 2:23,24");
        assert_eq!(plain_text("[[John 3:16]]\n\nText"), "John 3:16\n\nText");
    }

    #[test]
    fn the_passage_a_reference_names_is_its_first() {
        let john = |c, a, z| Some(RefRange { book: Book::JOHN, chapter: c, verses: Some((a, z)) });
        assert_eq!(passage_range("Joh 2:23,24"), john(2, 23, 23));
        assert_eq!(passage_range("John 3:16-18; Rom 5:8"), john(3, 16, 18));
        assert_eq!(passage_range("John.3.16"), john(3, 16, 16));
        assert_eq!(passage_range("not a reference"), None);
    }
}
