//! The Bible tab: a reader for one module with its navigation (module, book, chapter), its
//! settings (Strong's words), and everything that belongs to the text on screen: selection,
//! margin cards, footnote and dictionary cards. Self-contained: it keeps its own state, and
//! reads and changes the shared notes and marks through [`Shared`].

use crate::history::History;
use crate::live_editor::{live_editor, live_view};
use crate::opening::Spec;
use crate::palette::Palette;
use crate::reader::{self, Link};
use crate::reader_text::{CanonSlice, CardElement, CardSpec, FloatKind, MarkDraw, MarkKind};
use crate::shared::{Shared, TabId};
use iced::widget::operation::{AbsoluteOffset, RelativeOffset, scroll_by, scroll_to, snap_to};
use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, space, text};
use iced::{Color, Element, Font, Length, Task, Theme, font};
use md_editor::{Action as EditAction, Document};
use scripture::{Book, RefRange};
use std::collections::BTreeSet;
use std::fmt;
use study_core::ast::Chapter;
use study_core::margin::{self, Anchoring, Mark, MarkStyle, Slice};
use study_core::markdown::{LinkTarget, find_refs};
use study_core::{LexiconEntry, ModuleId};

/// The tallest a card's text gets; beyond this it scrolls inside the card (about eight lines).
pub const CARD_BODY_MAX: f32 = 136.0;

/// The size of the text in the cards beside the reading text (footnotes, dictionary entries,
/// margin notes): a little smaller than the reading text, so the notes sit quietly beside it.
fn card_size(sh: &Shared) -> f32 {
    sh.settings.bible_size * CARD_TEXT_SCALE
}

const CARD_TEXT_SCALE: f32 = 0.75;

/// How tall a card's text may get before it scrolls: at the default text size, `CARD_BODY_MAX`,
/// and in proportion to the size after that.
fn card_body_max(sh: &Shared) -> f32 {
    CARD_BODY_MAX * sh.settings.bible_size / crate::state::Settings::default().bible_size
}
/// Width of the text column. A reader wider than this plus room for cards shows a margin
/// beside it; a narrower one has no margin and floats a card under a clicked mark instead.
pub const TEXT_COLUMN: f32 = 640.0;

/// What a card opened from the text shows.
pub enum NoteContent {
    Footnote(String),
    /// A Strong's number and its dictionary entry, when an installed dictionary has one.
    Word { key: String, entry: Option<LexiconEntry> },
    /// What a cross reference marker points to.
    References(Vec<RefRange>),
}

/// A footnote or dictionary card, opened by clicking its marker or word. It pops over the
/// text under the click and goes away on a click elsewhere. (With a margin, footnotes are
/// already shown there, so only dictionary entries pop over.)
pub struct NoteCard {
    pub content: NoteContent,
}

/// Keys of the always-shown footnote cards: this plus the footnote's number in the chapter.
/// The kinds of note the reader can show (and the margin can hold).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    Strongs,
    Marks,
    Footnotes,
    Refs,
}

pub const FOOTNOTE_KEY: u64 = 1 << 40;
/// Keys of the dictionary entry cards of the chapter's Strong's words, and the margin's own menu.
pub const WORD_KEY: u64 = 1 << 41;
/// Keys of the cards of the chapter's cross reference markers: this plus the marker's number.
const CROSSREF_KEY: u64 = 1 << 43;
const MARGIN_MENU_KEY: u64 = 1 << 42;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterChoice(pub Book, pub u16);

impl fmt::Display for ChapterChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookChoice(pub Book);

impl fmt::Display for BookChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleChoice(pub ModuleId, pub String);

impl fmt::Display for ModuleChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

#[derive(Debug, Clone)]
pub enum BibleMsg {
    // Reader
    /// A link clicked, where it is, and the marks under the click.
    Link(Link, Option<CanonSlice>, Vec<usize>),
    SelectionChanged(Option<Vec<CanonSlice>>),
    ApplyPen(usize),
    /// Add a note-only mark (no highlight) to the selection and start writing it.
    AddComment,
    /// Put a link to the selected verses on the clipboard, to paste into a note.
    CopyReference,
    /// Put an embedded quote of the selected verses on the clipboard.
    CopyQuote,
    /// The reader reports whether it is wide enough for a margin.
    MarginChanged(bool),
    StartEdit(usize),
    CardAction(EditAction),
    /// Tick or untick a task in a card that is not being edited.
    ToggleCardTask(usize, usize),
    DeleteMarkAt(usize),
    UnlinkMarkAt(usize),
    OpenMarkInWriter(usize),
    /// The notes that link to the verses selected.
    OpenBacklinks,
    /// Read a commentary on this chapter (or the selected verse).
    OpenCommentary,
    /// Search the Bible for the selected words.
    SearchSelection,
    /// Open the search tab.
    OpenSearch,
    /// Mark this chapter as read today, or take the mark back.
    LogChapter,
    /// Show or put away the card with the days the chapter was read.
    ToggleReadCard,
    /// Show the reading log note.
    OpenReadingLog,
    /// Marks under a click in the reader (several when they overlap); empty = dismissed.
    MarksClicked(Vec<usize>),
    AutoScroll(f32),
    /// A press in the text: nothing to do here, but the workspace hears it.
    Pressed,
    CloseNoteCard,
    /// A link in a card's comment, or in a list of references.
    OpenLink(LinkTarget),
    /// Open a Strong's number's definition in a dictionary tab.
    OpenWord(String),
    /// Say the Greek or Hebrew word aloud.
    Speak(String),
    /// A dictionary entry's card was pressed (with the command key it opens in a tab of its own).
    CardPressed(String),
    /// A right click on a link in the text, at this point of the window.
    LinkMenu(Link, iced::Point),
    /// A right click on a link in a card's comment.
    CardLinkMenu(LinkTarget, iced::Point),
    /// A link in the text pulled away, at this point of the window.
    LinkDrag(Link, iced::Point),
    /// A link in a card's comment pulled away.
    CardLinkDrag(LinkTarget, iced::Point),
    // Navigation and settings
    PickModule(ModuleChoice),
    PickBook(BookChoice),
    PickChapter(ChapterChoice),
    /// The chapter before or after this one, in the module's order.
    PrevChapter,
    NextChapter,
    /// Back or forward along this tab's history, or straight to an entry of it.
    Back,
    Forward,
    GoTo(usize),
    /// The list of places this tab has been.
    ToggleHistory,
    CloseHistory,
    /// The reader was scrolled to this far down.
    Scrolled(f32),
    ToggleStrongs(bool),
    ToggleMarks(bool),
    ToggleFootnotes(bool),
    ToggleRefs(bool),
    /// The reader's menu (the hamburger): open or close it.
    ToggleMenu,
    /// The margin's menu, and its choice, for each kind of note, of whether it sits in the margin.
    ToggleMarginMenu,
    SetInMargin(NoteKind, bool),
    CloseMenu,
    /// The tag box of the card being written in, and its changes.
    CardTagDraft(String),
    AddCardTag(String),
    RemoveCardTag(String),
}

/// Something the tab cannot do alone, for the workspace to carry out.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Follow a link, where the keys held (or the menu) say.
    Open(Spec),
    /// Offer where to open a link, at this point of the window.
    Menu(Spec, iced::Point),
    /// A link has been picked up: carry it, from this point of the window.
    Drag(Spec, iced::Point),
    /// Show the note behind mark `index` in a note tab.
    OpenMark(usize),
    /// Show the note at this path in a note tab.
    OpenNote(String),
    /// Show the notes that link to this passage in a tab.
    OpenBacklinks(RefRange),
    /// A commentary at a chapter, and the verse if one is selected.
    OpenCommentary(Book, u16, Option<u16>),
    /// A search for these words.
    Search(String),
    /// The search tab, to type in.
    OpenSearch,
}

/// A book and chapter number.
type ChapterId = (Book, u16);

/// Where a Bible tab has been: enough to come back to the same view of the same text.
#[derive(Clone, Debug)]
pub struct Place {
    pub module: ModuleId,
    pub book: Book,
    pub chapter: u16,
    /// The verses a link had picked out, if one brought it here.
    pub verses: Option<(u16, u16)>,
    /// How far down the chapter it was scrolled when it was left.
    pub scroll: f32,
}

impl Place {
    /// The same place, whatever the scroll.
    pub fn is_same(&self, other: &Place) -> bool {
        self.module == other.module && self.book == other.book && self.chapter == other.chapter && self.verses == other.verses
    }

    fn label(&self, sh: &Shared) -> String {
        let verses = match self.verses {
            Some((a, z)) if a == z => format!(":{a}"),
            Some((a, z)) => format!(":{a}-{z}"),
            None => String::new(),
        };
        format!("{} {}{verses} \u{b7} {}", self.book, self.chapter, sh.module_abbreviation(&self.module))
    }
}

pub struct BibleTab {
    pub id: TabId,
    pub module: ModuleId,
    pub book: Book,
    pub chapter_no: u16,
    pub chapter: Result<Chapter, String>,
    /// Pending "select these verses' text" request for the reader: (token, first, last).
    pub select_request: Option<(u64, u16, u16)>,
    /// Words to select in the reader (from the first to the last), once.
    pub select_words: Option<(u64, CanonSlice)>,
    /// Whether the card with the days the chapter was read is showing.
    pub read_card: bool,
    pub select_token: u64,
    /// Whether the reader is wide enough to show its margin (reported by the reader).
    pub margin_showing: bool,
    /// The mark whose card is being edited in place, and its editor.
    pub editing: Option<usize>,
    pub card_doc: Document,
    /// Marks whose cards float under the last click (a reader with no margin has no room
    /// for the cards, so a click on a mark shows them like a toolbar).
    pub popup: Option<Vec<usize>>,
    /// The reader's current text selection, as canonical verse slices.
    pub text_selection: Option<Vec<CanonSlice>>,
    /// Bumped to make the reader drop its selection.
    pub clear_token: u64,
    /// Pending "scroll this verse into view" request for the reader: (token, verse).
    pub reveal: Option<(u64, u16)>,
    pub reveal_token: u64,
    /// Where this tab has been, and how far down the chapter it is now (kept as it is scrolled).
    pub history: History<Place>,
    pub scroll_y: f32,
    /// The list of places this tab has been is open.
    pub history_open: bool,
    pub show_strongs: bool,
    /// What the reader shows besides the words: mark highlights and cards, footnote markers and
    /// cards, cross reference markers.
    pub show_marks: bool,
    pub show_footnotes: bool,
    pub show_refs: bool,
    /// The reader's menu is open.
    pub menu_open: bool,
    /// Which kinds of note show in the margin, when the reader is wide enough for one; the rest
    /// pop over the text where clicked.
    pub in_margin: crate::state::MarginKinds,
    /// The margin's own menu (the choice just above) is open.
    pub margin_menu_open: bool,
    /// What is typed in the tag box of the card being written in.
    pub card_tag_draft: String,
    pub note_card: Option<NoteCard>,
    /// The chapter's Strong's words with their entries, when they are kept in the margin (looked
    /// up when the chapter loads, not for every drawing).
    pub words: Vec<WordCard>,
}

/// A note's document, with the caret just below its frontmatter (the properties are shown and
/// edited above the text, so the text is where writing starts).
pub fn start_of_note(text: &str) -> Document {
    // The properties are changed with the app's own controls, so the text cannot reach them.
    let mut doc = Document::new(text).lock_frontmatter();
    let at = doc.front_end();
    doc.perform(EditAction::SetCaret { offset: at, extend: false });
    doc
}

pub fn hex_color(hex: &str, alpha: f32) -> Option<Color> {
    let h = hex.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some(Color::from_rgba8(byte(0)?, byte(2)?, byte(4)?, alpha))
}

/// Turn what the reader reports into a tab message.
fn reader_message(event: reader::Event) -> BibleMsg {
    match event {
        reader::Event::Link(link, at, marks) => BibleMsg::Link(link, at, marks),
        reader::Event::Selection(s) => BibleMsg::SelectionChanged(s),
        reader::Event::Scroll(dy) => BibleMsg::AutoScroll(dy),
        reader::Event::Marks(ids) => BibleMsg::MarksClicked(ids),
        reader::Event::Margin(on) => BibleMsg::MarginChanged(on),
        reader::Event::Pressed => BibleMsg::Pressed,
        reader::Event::LinkMenu(link, at) => BibleMsg::LinkMenu(link, at),
        reader::Event::LinkDrag(link, at) => BibleMsg::LinkDrag(link, at),
    }
}

/// The toolbar the reader floats under a text selection. Each pen is its own name written the
/// way it marks: highlighted behind the word, or underlined in its colour. Beside them, the
/// comment button and two icon buttons that copy the selection to the clipboard.
fn pen_toolbar(sh: &Shared, backlinks: bool) -> Element<'static, BibleMsg> {
    use iced::widget::{stack, tooltip};
    let mut bar = row![].spacing(2).align_y(iced::Center);
    for (i, pen) in sh.settings.pens.iter().enumerate() {
        let color = hex_color(&pen.color, 1.0).unwrap_or(Color::WHITE);
        let name = pen.name();
        let label: Element<'static, BibleMsg> = match pen.style {
            // Underlined: the name in the pen's colour with a line of it just under, as wide as
            // the name (the line is a layer over the name, so the name sets the width).
            crate::state::PenStyle::Underline => stack![
                container(text(name.clone()).size(13).color(color))
                    .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 4.0, left: 0.0 }),
                container(container(space::horizontal()).width(Length::Fill).height(2).style(move |_: &Theme| {
                    container::Style { background: Some(color.into()), ..container::Style::default() }
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(iced::alignment::Vertical::Bottom),
            ]
            .into(),
            // Highlighted: the name on a band of its colour, as the text will be marked.
            crate::state::PenStyle::Highlight => container(text(name).size(13))
                .padding([1, 4])
                .style(move |_: &Theme| container::Style {
                    background: Some(Color { a: 0.45, ..color }.into()),
                    border: iced::Border { radius: 3.0.into(), ..iced::Border::default() },
                    ..container::Style::default()
                })
                .into(),
        };
        bar = bar.push(button(label).padding([5, 8]).style(button::text).on_press(BibleMsg::ApplyPen(i)));
    }
    let bar = bar.push(button(text("Add Note").size(13)).padding([5, 8]).style(button::text).on_press(BibleMsg::AddComment));
    // What can be taken to a note: plain text on the clipboard, pasted wherever it is wanted.
    let icon = |picture: crate::icons::Icon, tip: &'static str, msg: BibleMsg| {
        tooltip(
            button(container(picture.view(16.0)).center(Length::Fill))
                .width(30)
                .height(26)
                .padding(0)
                .style(button::secondary)
                .on_press(msg),
            container(text(tip).size(12)).padding(4).style(container::rounded_box),
            tooltip::Position::Bottom,
        )
    };
    let mut copy = row![
        icon(crate::icons::Icon::Link, "Copy reference", BibleMsg::CopyReference),
        icon(crate::icons::Icon::Quote, "Copy quote", BibleMsg::CopyQuote),
        icon(crate::icons::Icon::Search, "Search the Bible for these words", BibleMsg::SearchSelection),
    ]
    .spacing(4);
    // Where the selected verses are green (a note links to them): the notes that do.
    if backlinks {
        copy = copy.push(icon(crate::icons::Icon::Backlinks, "Backlinks: the notes that link here", BibleMsg::OpenBacklinks));
    }
    // The two kinds of action are set apart: marking the text, and copying it away.
    let divider = container(space::horizontal()).width(1).height(18).style(|theme: &Theme| container::Style {
        background: Some(theme.extended_palette().background.strong.color.into()),
        ..container::Style::default()
    });
    container(row![bar, divider, copy].spacing(10).align_y(iced::Center))
        .padding([5, 8])
        .style(container::rounded_box)
        .into()
}

/// The text "Copy reference" and "Copy quote" put on the clipboard: `[[John 3:3-5]]` links, or
/// `![[John 3:3-5@LSB]]` embeds pinned to the module they were taken from.
pub fn copy_text(refs: &[RefRange], module: &str, quote: bool) -> String {
    if quote {
        refs.iter().map(|r| format!("![[{r}@{module}]]")).collect::<Vec<_>>().join("\n\n")
    } else {
        refs.iter().map(|r| format!("[[{r}]]")).collect::<Vec<_>>().join(", ")
    }
}

/// Footnote text, with scripture references in it (`See Mrk 9:29`) as links to the reader.
pub fn footnote_body<'a>(sh: &Shared, note: &str, link: Color) -> Element<'a, BibleMsg> {
    use iced::widget::{rich_text, span};
    // References drawn as the writer draws links, so they can also be right clicked (where to
    // open it) and pulled away to a tab or pane. That means writing the text as markdown, so
    // only text with nothing in it that markdown would take for something else.
    let refs = find_refs(note);
    if !refs.is_empty() && !note.contains(['*', '_', '`', '#', '[', ']', '<', '>', '\\', '\n']) {
        let mut markdown = String::new();
        let mut at = 0;
        for (range, _) in &refs {
            markdown.push_str(&note[at..range.start]);
            markdown.push_str(&format!("[[{}]]", &note[range.clone()]));
            at = range.end;
        }
        markdown.push_str(&note[at..]);
        return link_view(sh, markdown);
    }
    let mut spans = Vec::new();
    let mut at = 0;
    for (range, target) in find_refs(note) {
        if range.start > at {
            spans.push(span(note[at..range.start].to_string()));
        }
        spans.push(span(note[range.clone()].to_string()).color(link).underline(true).link(target));
        at = range.end;
    }
    if at < note.len() {
        spans.push(span(note[at..].to_string()));
    }
    rich_text(spans)
        .size(card_size(sh))
        .font(sh.settings.text_font())
        .on_link_click(|range: RefRange| BibleMsg::OpenLink(LinkTarget::Verse { range, module: None }))
        .into()
}

/// Text with `[[links]]` in it, set like a card's text and with a link's usual behaviour: a click
/// opens it, a right click offers where, and pulling it away takes it to a tab or pane.
fn link_view<'a>(sh: &Shared, markdown: String) -> Element<'a, BibleMsg> {
    live_view(Document::new(&markdown), Palette::for_theme(&sh.theme()))
        .fit_width()
        .size(card_size(sh))
        .font(sh.settings.text_font())
        .padding([2.0, 0.0])
        .on_link(BibleMsg::OpenLink)
        .on_link_menu(BibleMsg::CardLinkMenu)
        .on_link_drag(BibleMsg::CardLinkDrag)
        .into()
}

/// A dictionary entry: the word in its language and its transliteration, then the definition.
fn word_body<'a>(sh: &Shared, e: &study_core::LexiconEntry) -> Element<'a, BibleMsg> {
    let (size, font) = (card_size(sh), sh.settings.text_font());
    let mut head = row![].spacing(8).align_y(iced::Alignment::Center);
    if !e.lemma.is_empty() {
        head = head.push(text(e.lemma.clone()).size(size * 1.2).font(font));
        // Said aloud by the system's voice, where it has one for the language.
        if crate::speech::can_say(&e.lemma) {
            head = head.push(
                button(crate::icons::Icon::Speaker.view(size))
                    .padding([2, 4])
                    .style(button::text)
                    .on_press(BibleMsg::Speak(e.lemma.clone())),
            );
        }
    }
    if !e.transliteration.is_empty() {
        let italic = Font { style: font::Style::Italic, ..font };
        head = head.push(text(e.transliteration.clone()).size(size).font(italic).style(text::secondary));
    }
    column![head, text(e.definition.clone()).size(size).font(font)].spacing(4).into()
}

/// A word with a Strong's number, where it is, and what the dictionary says of it.
pub struct WordCard {
    pub anchor: CanonSlice,
    pub entry: study_core::LexiconEntry,
}

/// Where a footnote's marker sits among the reader's canonical text: the character just
/// before it (or the first one after, at a verse's start). A verse with no text of its own
/// borrows the end of the nearest verse before it that has some.
pub fn footnote_anchor(canon: &margin::CanonicalText, note: &study_core::ast::FootnoteAt) -> Option<CanonSlice> {
    marker_anchor(canon, note.verse, note.offset)
}

/// The same for any marker in the text (a footnote's, a cross reference's), by its verse and
/// the offset in that verse's text where it comes.
pub fn marker_anchor(canon: &margin::CanonicalText, at_verse: u16, offset: usize) -> Option<CanonSlice> {
    let (verse, text) = canon.range(..=at_verse).rev().find(|(_, t)| !t.is_empty())?;
    let end = if *verse == at_verse { offset.min(text.len()) } else { text.len() };
    match text[..end].char_indices().next_back() {
        Some((start, _)) => Some(CanonSlice { verse: *verse, start, end }),
        None => {
            let first = text.chars().next()?.len_utf8();
            Some(CanonSlice { verse: *verse, start: 0, end: first })
        }
    }
}

impl BibleTab {
    /// A tab reading `module`, starting where `near` is reading if the module has that chapter
    /// (so a second translation opens at the same place), else at its own first chapter.
    pub fn started(id: TabId, sh: &Shared, module: ModuleId, near: Option<&BibleTab>) -> Self {
        let here = near.map(|n| (n.book, n.chapter_no)).filter(|at| sh.backend.available_chapters(&module).contains(at));
        let (book, chapter) = here.unwrap_or_else(|| sh.first_chapter(&module));
        let mut tab = BibleTab::blank(id, module);
        if let Some(n) = near {
            (tab.show_strongs, tab.show_marks, tab.show_footnotes, tab.show_refs) =
                (n.show_strongs, n.show_marks, n.show_footnotes, n.show_refs);
            tab.in_margin = n.in_margin;
        }
        tab.open_chapter(sh, book, chapter);
        tab
    }

    /// A tab put back as it was left: this chapter, this far down (the reader is told to scroll
    /// there by whoever builds the tab).
    pub fn restored(id: TabId, sh: &Shared, module: ModuleId, book: Book, chapter: u16, scroll: f32) -> Self {
        let mut tab = BibleTab::blank(id, module);
        tab.scroll_y = scroll;
        tab.open_chapter(sh, book, chapter);
        tab
    }

    fn blank(id: TabId, module: ModuleId) -> Self {
        let first = Place { module: module.clone(), book: Book::JOHN, chapter: 3, verses: None, scroll: 0.0 };
        BibleTab {
            id,
            module,
            book: Book::JOHN,
            chapter_no: 3,
            chapter: Err(String::new()),
            select_request: None,
            select_words: None,
            read_card: false,
            select_token: 0,
            margin_showing: false,
            editing: None,
            card_doc: Document::new(""),
            popup: None,
            text_selection: None,
            clear_token: 0,
            reveal: None,
            reveal_token: 0,
            history: History::new(first),
            scroll_y: 0.0,
            history_open: false,
            show_strongs: false,
            show_marks: true,
            show_footnotes: true,
            show_refs: false,
            menu_open: false,
            in_margin: crate::state::MarginKinds::default(),
            margin_menu_open: false,
            card_tag_draft: String::new(),
            note_card: None,
            words: Vec::new(),
        }
    }

    // Ids of this tab's widgets: two Bible tabs on screen must not answer to each other's
    // scroll and focus requests.
    pub fn reader_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("reader-{}", self.id))
    }

    pub fn card_editor_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("card-editor-{}", self.id))
    }

    pub fn title(&self, sh: &Shared) -> String {
        format!("{} {} · {}", self.book, self.chapter_no, sh.module_abbreviation(&self.module))
    }

    // ---- state changes -------------------------------------------------------------

    /// Read a chapter in a tab that has nothing in progress.
    fn open_chapter(&mut self, sh: &Shared, book: Book, chapter: u16) {
        self.book = book;
        self.chapter_no = chapter;
        self.chapter = sh.backend.chapter(&self.module, book, chapter).map_err(|e| e.to_string());
        self.history = History::new(self.place(None));
        self.refresh_words(sh);
    }

    /// Look up the entries of the chapter's Strong's words, if they are wanted in the margin.
    fn refresh_words(&mut self, sh: &Shared) {
        self.words.clear();
        let Ok(chapter) = &self.chapter else { return };
        if !(self.show_strongs && self.in_margin.strongs) {
            return;
        }
        let mut known: std::collections::HashMap<String, Option<study_core::LexiconEntry>> = std::collections::HashMap::new();
        for at in chapter.strongs_words() {
            let entry = known.entry(at.strongs.clone()).or_insert_with(|| sh.backend.lexicon_entry(&at.strongs));
            if let Some(entry) = entry {
                self.words.push(WordCard {
                    anchor: CanonSlice { verse: at.verse, start: at.start, end: at.end },
                    entry: entry.clone(),
                });
            }
        }
    }

    /// Where this tab is now, as a history entry.
    fn place(&self, verses: Option<(u16, u16)>) -> Place {
        Place { module: self.module.clone(), book: self.book, chapter: self.chapter_no, verses, scroll: self.scroll_y }
    }

    /// Go to a chapter of the module being read, as a move to a new place (Back returns).
    pub fn load(&mut self, sh: &mut Shared, book: Book, chapter: u16) {
        self.history.current_mut().scroll = self.scroll_y;
        self.load_raw(sh, book, chapter);
        self.scroll_y = 0.0;
        self.history.go(self.place(None), Place::is_same);
    }

    /// Show a chapter, with nothing else about the tab changed: the way history puts one back.
    fn load_raw(&mut self, sh: &mut Shared, book: Book, chapter: u16) {
        self.book = book;
        self.chapter_no = chapter;
        self.select_request = None;
        self.select_words = None;
        self.text_selection = None;
        self.read_card = false;
        self.stop_edit(sh);
        self.editing = None;
        self.reveal = None;
        self.note_card = None;
        self.chapter = sh.backend.chapter(&self.module, book, chapter).map_err(|e| e.to_string());
        self.refresh_words(sh);
    }

    /// Show a footnote or dictionary card at `at`. Whatever else was open there is dismissed.
    pub fn open_note_card(&mut self, sh: &mut Shared, at: Option<CanonSlice>, content: NoteContent) {
        // A click with no place in the text (a link we could not locate) opens nothing.
        if at.is_none() {
            return;
        }
        self.finish_editing(sh);
        self.popup = None;
        self.note_card = Some(NoteCard { content });
    }

    /// Show `target` in the reader: switch chapter if needed, mark its verses, scroll to it.
    pub fn show_in_reader(&mut self, sh: &mut Shared, target: RefRange) {
        let available = sh.backend.available_chapters(&self.module);
        if !available.contains(&(target.book, target.chapter)) {
            sh.status = format!("{target} is not in this module");
            return;
        }
        sh.status.clear();
        self.select_words = None;
        self.history.current_mut().scroll = self.scroll_y;
        if (target.book, target.chapter) != (self.book, self.chapter_no) {
            self.load_raw(sh, target.book, target.chapter);
            self.scroll_y = 0.0;
        }
        self.select_token += 1;
        self.select_request = target.verses.map(|(a, z)| (self.select_token, a, z));
        self.reveal_token += 1;
        self.reveal = Some((self.reveal_token, target.verses.map_or(1, |(a, _)| a)));
        self.history.go(self.place(target.verses), Place::is_same);
    }

    /// Return to a place in this tab's history: its module, chapter, verses and how far down it was.
    fn restore(&mut self, sh: &mut Shared, place: Place) -> Task<BibleMsg> {
        self.module = place.module.clone();
        self.load_raw(sh, place.book, place.chapter);
        self.scroll_y = place.scroll;
        if let Some((a, z)) = place.verses {
            self.select_token += 1;
            self.select_request = Some((self.select_token, a, z));
            // Without a known scroll, bring the verses into view.
            if place.scroll <= 0.0 {
                self.reveal_token += 1;
                self.reveal = Some((self.reveal_token, a));
            }
        }
        scroll_to(self.reader_id(), AbsoluteOffset { x: 0.0, y: place.scroll })
    }

    /// One step along the history (`back` or forward), if there is one.
    fn step(&mut self, sh: &mut Shared, back: bool) -> Task<BibleMsg> {
        self.history.current_mut().scroll = self.scroll_y;
        let place = if back { self.history.back() } else { self.history.forward() }.cloned();
        match place {
            Some(place) => self.restore(sh, place),
            None => Task::none(),
        }
    }

    /// Put the selected verses on the clipboard as a reference link or a quote.
    /// Afterwards the selection (and with it the toolbar) is put away, and a toast says what was
    /// copied.
    fn copy_selection(&mut self, sh: &mut Shared, quote: bool) -> Task<BibleMsg> {
        let refs = self.selected_refs();
        if refs.is_empty() {
            return Task::none();
        }
        let text = copy_text(&refs, &sh.module_abbreviation(&self.module), quote);
        sh.toast(if quote { format!("Copied quote of {}", self.selection_label()) } else { format!("Copied {text}") });
        self.text_selection = None;
        self.clear_token += 1;
        iced::clipboard::write(text)
    }

    /// Marks were clicked: write in the first when its card is in the margin (and `edit` says a
    /// click on a mark means to edit it), else float the cards under the click. Empty means
    /// the click-away that ends an edit and any popup.
    /// Whether marks' cards are beside the text (there is a margin, and marks are among what it
    /// holds), rather than floating under a click.
    fn marks_in_margin(&self) -> bool {
        self.margin_showing && self.in_margin.marks
    }

    fn footnotes_in_margin(&self) -> bool {
        self.margin_showing && self.in_margin.footnotes
    }

    fn marks_clicked(&mut self, sh: &mut Shared, ids: Vec<usize>, edit: bool) -> Task<BibleMsg> {
        if ids.is_empty() {
            // The popup closed: that finishes any edit going on in it.
            self.popup = None;
            self.finish_editing(sh);
            Task::none()
        } else if self.marks_in_margin() && edit {
            // The cards are already beside the text: write in the one clicked.
            self.popup = None;
            self.start_edit(sh, ids[0])
        } else {
            let mut ids = ids;
            if let Some(removed) = self.finish_editing(sh) {
                // A discarded empty comment shifts the later marks down by one.
                ids.retain(|&i| i != removed);
                ids.iter_mut().filter(|i| **i > removed).for_each(|i| *i -= 1);
            }
            self.popup = (!ids.is_empty()).then_some(ids);
            Task::none()
        }
    }

    /// Something other than this tab removed a mark: card numbers moved, so drop them.
    pub fn marks_changed(&mut self) {
        self.popup = None;
        self.editing = None;
    }

    // ---- queries -------------------------------------------------------------------

    /// The verses the text selection touches, as contiguous runs of `RefRange`s
    /// ("John 3:3-5", "John 3:7").
    pub fn selected_refs(&self) -> Vec<RefRange> {
        let mut runs: Vec<(u16, u16)> = Vec::new();
        let verses: BTreeSet<u16> = self.text_selection.iter().flatten().map(|s| s.verse).collect();
        for v in verses {
            match runs.last_mut() {
                Some((_, end)) if *end + 1 == v => *end = v,
                _ => runs.push((v, v)),
            }
        }
        runs.into_iter()
            .map(|run| RefRange { book: self.book, chapter: self.chapter_no, verses: Some(run) })
            .collect()
    }

    /// The verses the selection touches, from the first to the last as one passage.
    pub fn selection_span(&self) -> Option<RefRange> {
        let verses: Vec<u16> = self.text_selection.iter().flatten().map(|s| s.verse).collect();
        let (first, last) = (verses.iter().min()?, verses.iter().max()?);
        Some(RefRange { book: self.book, chapter: self.chapter_no, verses: Some((*first, *last)) })
    }

    /// Whether any note links to a verse of the selection: those verses are green in the text.
    pub fn selection_has_backlinks(&self, sh: &Shared) -> bool {
        self.selection_span().is_some_and(|span| !sh.backlinks(&span).is_empty())
    }

    /// "John 3:1-3, 5": the marked verses as one label.
    pub fn selection_label(&self) -> String {
        let refs = self.selected_refs();
        let verses = refs
            .iter()
            .filter_map(|r| r.verses)
            .map(|(a, z)| if a == z { a.to_string() } else { format!("{a}-{z}") })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{} {}:{verses}", self.book, self.chapter_no)
    }

    /// The selection as core slices, plus the open chapter's canonical text.
    fn selection_context(&self) -> Option<(Vec<Slice>, margin::CanonicalText)> {
        let slices = self.text_selection.as_ref()?;
        let canon = self.chapter.as_ref().ok()?.canonical_verses();
        let slices = slices.iter().map(|s| Slice { verse: s.verse, start: s.start, end: s.end });
        Some((slices.collect(), canon))
    }

    /// Marks of the open chapter that show in the displayed module, with their text.
    /// Word-level marks belong to the module they were made in; verse-level ones to the
    /// reference, so they show everywhere.
    pub fn visible_marks(&self, sh: &Shared) -> Vec<(usize, Vec<Slice>)> {
        let Ok(chapter) = &self.chapter else { return Vec::new() };
        if !self.show_marks {
            return Vec::new();
        }
        let canon = chapter.canonical_verses();
        let module = sh.module_abbreviation(&self.module);
        let mut out = Vec::new();
        for (index, mark) in sh.marks.iter().enumerate() {
            let here = mark.anchor.book == self.book && mark.anchor.chapter == self.chapter_no;
            if !here || (!mark.is_verse_level() && mark.module != module) {
                continue;
            }
            let resolved = margin::resolve(mark, &canon);
            if resolved.state == Anchoring::Exact {
                out.push((index, resolved.slices));
            }
        }
        out
    }

    /// Marks of the open chapter, resolved for painting.
    /// Put away what the reader was asked to do once (select, scroll to a verse): it is done when the
    /// reader is shown again, which would otherwise do it again.
    pub fn forget_requests(&mut self) {
        self.select_request = None;
        self.select_words = None;
        self.reveal = None;
        // A card floating under a click goes with the reader's widgets, which know where it was: with
        // them gone it would be there, unseen, taking the place of the selection's toolbar.
        self.note_card = None;
        self.popup = None;
    }

    /// Select these words of `verse` (canonical offsets) in the reader, from the first to the last,
    /// in place of the whole verse.
    pub fn select_found(&mut self, verse: u16, words: &[(usize, usize)]) {
        let (Some(start), Some(end)) = (words.iter().map(|w| w.0).min(), words.iter().map(|w| w.1).max()) else { return };
        self.select_request = None;
        self.select_token += 1;
        self.select_words = Some((self.select_token, CanonSlice { verse, start, end }));
    }

    pub fn mark_draws(&self, sh: &Shared) -> Vec<MarkDraw> {
        let mut out = Vec::new();
        for (index, slices) in self.visible_marks(sh) {
            let mark = &sh.marks[index];
            let (kind, alpha) = match mark.style {
                MarkStyle::Highlight => (MarkKind::Highlight, if sh.is_dark() { 0.38 } else { 0.5 }),
                MarkStyle::Underline => (MarkKind::Underline, 1.0),
                MarkStyle::None => (MarkKind::Note, 0.7),
            };
            let Some(color) = hex_color(mark.shown_color(), alpha) else { continue };
            for s in slices {
                out.push(MarkDraw {
                    mark: index,
                    slice: CanonSlice { verse: s.verse, start: s.start, end: s.end },
                    kind,
                    color,
                });
            }
        }
        out
    }

    // ---- marks -----------------------------------------------------------------------

    /// The passage the current text selection is: its anchor, and the module's abbreviation to keep with
    /// words (none for whole verses). Whole verses (a verse-number click, triple-click, or an exact drag)
    /// tie it to the reference, not to words, so it shows in every module.
    pub fn selection_anchor(&self, sh: &Shared) -> Option<(margin::Anchor, String)> {
        let (slices, canon) = self.selection_context()?;
        let anchor = match margin::whole_verses(&canon, &slices) {
            Some((a, z)) => Some(margin::anchor_for_verses(self.book, self.chapter_no, a, z)),
            None => margin::anchor_from_selection(self.book, self.chapter_no, &canon, &slices),
        }?;
        let module = if anchor.quote.is_empty() { String::new() } else { sh.module_abbreviation(&self.module) };
        Some((anchor, module))
    }

    /// Let go of the selection (it was used).
    pub fn clear_selection(&mut self) {
        self.popup = None;
        self.text_selection = None;
        self.clear_token += 1;
    }

    /// Make a mark of `style` on the current text selection, whole verses becoming
    /// verse-level marks. Returns its index. The selection is spent.
    pub fn make_mark(
        &mut self,
        sh: &mut Shared,
        style: MarkStyle,
        color: &str,
        tags: Vec<String>,
        keep_selection: bool,
    ) -> Option<usize> {
        let (anchor, module) = self.selection_anchor(sh)?;
        let mark = Mark {
            id: sh.new_mark_id(),
            module,
            anchor: anchor.range,
            quote: anchor.quote,
            prefix: anchor.prefix,
            suffix: anchor.suffix,
            style,
            // (A plain mark has no color: it is drawn in a grey.)
            color: if style == MarkStyle::None { String::new() } else { color.to_string() },
            tags,
            body: String::new(),
            created: study_core::frontmatter::now(),
        };
        sh.notes.insert(mark.path(), mark.to_markdown());
        sh.marks.push(mark);
        let new = sh.marks.len() - 1;
        sh.bookmark_in_daily(new);
        self.popup = None;
        if !keep_selection {
            self.text_selection = None;
            self.clear_token += 1;
        }
        Some(sh.marks.len() - 1)
    }

    /// Mark the selected text with pen `pen`, and open the mark's card to write a note in, as
    /// "Add Note" does.
    pub fn apply_pen(&mut self, sh: &mut Shared, pen: usize) -> Task<BibleMsg> {
        let Some(pen) = sh.settings.pens.get(pen).cloned() else { return Task::none() };
        self.stop_edit(sh);
        // With no margin the card appears under the selection, so keep it selected.
        let keep = !self.marks_in_margin();
        let style = match pen.style {
            crate::state::PenStyle::Highlight => MarkStyle::Highlight,
            crate::state::PenStyle::Underline => MarkStyle::Underline,
        };
        match self.make_mark(sh, style, &pen.color, vec![pen.tag.clone()], keep) {
            Some(index) => {
                self.card_doc = Document::new("");
                self.editing = Some(index);
                iced::widget::operation::focus(self.card_editor_id())
            }
            None => Task::none(),
        }
    }

    pub fn delete_mark(&mut self, sh: &mut Shared, index: usize) {
        sh.delete_mark(index, Some(self.id));
        self.marks_changed();
    }

    pub fn unlink_mark(&mut self, sh: &mut Shared, index: usize) {
        sh.unlink_mark(index, Some(self.id));
        self.marks_changed();
    }

    // ---- margin cards ----------------------------------------------------------------

    /// Begin writing in the card of mark `index`.
    pub fn start_edit(&mut self, sh: &mut Shared, index: usize) -> Task<BibleMsg> {
        let mut index = index;
        if let Some(previous) = self.editing.filter(|&p| p != index)
            && let Some(removed) = self.stop_edit(sh)
            && removed < index
        {
            index -= 1;
            let _ = previous;
        }
        let Some(mark) = sh.marks.get(index) else { return Task::none() };
        // The caret goes after the last character, not on the empty line a trailing newline
        // leaves, so typing adds to what is there.
        self.card_tag_draft.clear();
        self.card_doc = Document::new(&mark.body);
        let end = mark.body.trim_end_matches(['\n', '\r']).len();
        self.card_doc.perform(EditAction::SetCaret { offset: end, extend: false });
        self.editing = Some(index);
        iced::widget::operation::focus(self.card_editor_id())
    }

    /// Stop editing. A note-only mark left empty is pointless, so it is removed; returns its
    /// index if so (later indices shift down by one).
    pub fn stop_edit(&mut self, sh: &mut Shared) -> Option<usize> {
        let index = self.editing.take()?;
        let empty_note = sh
            .marks
            .get(index)
            .is_some_and(|m| m.style == MarkStyle::None && m.body.trim().is_empty());
        if empty_note {
            self.delete_mark(sh, index);
            return Some(index);
        }
        None
    }

    /// What closing a card does. A reader with no margin shows the card under the selection
    /// while it is written, so that selection goes too. Returns the index of a mark that was
    /// removed for being empty, if any.
    pub fn finish_editing(&mut self, sh: &mut Shared) -> Option<usize> {
        let removed = self.stop_edit(sh);
        if !self.marks_in_margin() && self.text_selection.is_some() {
            self.text_selection = None;
            self.clear_token += 1;
        }
        removed
    }

    /// Text typed into a card editor goes straight to the mark and its note file.
    fn card_edited(&mut self, sh: &mut Shared) {
        let Some(index) = self.editing else { return };
        sh.set_body(index, self.card_doc.source().to_string());
    }

    // ---- update --------------------------------------------------------------------

    pub fn update(&mut self, sh: &mut Shared, message: BibleMsg) -> (Task<BibleMsg>, Option<Request>) {
        // The reader's menu is a pop-up: anything else done in the reader puts it away.
        if !matches!(
            message,
            BibleMsg::ToggleMenu
                | BibleMsg::ToggleStrongs(_)
                | BibleMsg::ToggleMarks(_)
                | BibleMsg::ToggleFootnotes(_)
                | BibleMsg::ToggleRefs(_)
                | BibleMsg::MarginChanged(_)
                | BibleMsg::AutoScroll(_)
                | BibleMsg::Scrolled(_)
                | BibleMsg::ToggleHistory
                | BibleMsg::SelectionChanged(None)
        ) {
            self.menu_open = false;
            self.history_open = false;
        }
        // So is the margin's menu: it stays for what is done in it, and goes for anything else.
        if !matches!(
            message,
            BibleMsg::ToggleMarginMenu
                | BibleMsg::SetInMargin(..)
                | BibleMsg::MarginChanged(_)
                | BibleMsg::AutoScroll(_)
                | BibleMsg::Scrolled(_)
                | BibleMsg::SelectionChanged(None)
        ) {
            self.margin_menu_open = false;
        }
        let before = (self.book, self.chapter_no);
        let mut request = None;
        let task = match message {
            BibleMsg::AutoScroll(dy) => {
                return (scroll_by(self.reader_id(), AbsoluteOffset { x: 0.0, y: dy }), None);
            }
            BibleMsg::MarksClicked(ids) => {
                self.note_card = None;
                self.marks_clicked(sh, ids, true)
            }
            BibleMsg::MarginChanged(on) => {
                self.margin_showing = on;
                if on {
                    if self.in_margin.marks {
                        self.popup = None;
                    }
                    // The margin shows every footnote itself, so one opened as a popup is moot.
                    if self.in_margin.footnotes && matches!(self.note_card, Some(NoteCard { content: NoteContent::Footnote(_), .. })) {
                        self.note_card = None;
                    }
                }
                Task::none()
            }
            BibleMsg::AddComment => {
                self.stop_edit(sh);
                // With no margin the card appears under the selection, so keep it selected.
                let keep = !self.marks_in_margin();
                match self.make_mark(sh, MarkStyle::None, "#8A9099", Vec::new(), keep) {
                    Some(index) => {
                        self.card_doc = Document::new("");
                        self.editing = Some(index);
                        iced::widget::operation::focus(self.card_editor_id())
                    }
                    None => Task::none(),
                }
            }
            BibleMsg::CopyReference => self.copy_selection(sh, false),
            BibleMsg::CopyQuote => self.copy_selection(sh, true),
            BibleMsg::Pressed => Task::none(),
            BibleMsg::StartEdit(i) => self.start_edit(sh, i),
            BibleMsg::ToggleCardTask(index, at) => {
                if let Some(mark) = sh.marks.get(index) {
                    let mut doc = Document::new(&mark.body);
                    if doc.perform(EditAction::ToggleTask(at)) {
                        let body = doc.source().to_string();
                        sh.set_body(index, body);
                    }
                }
                Task::none()
            }
            BibleMsg::CardAction(action) => {
                if self.card_doc.perform(action) {
                    self.card_edited(sh);
                }
                Task::none()
            }
            BibleMsg::DeleteMarkAt(i) => {
                self.delete_mark(sh, i);
                Task::none()
            }
            BibleMsg::UnlinkMarkAt(i) => {
                // The note stays as a plain note: show it, so it is not lost sight of.
                let kept = sh.marks.get(i).map(|mark| mark.unlinked_note().0);
                self.unlink_mark(sh, i);
                request = kept.map(Request::OpenNote);
                Task::none()
            }
            BibleMsg::LogChapter => {
                let now = chrono::Local::now();
                if sh.read_on(self.book, self.chapter_no, &now) {
                    sh.unlog_reading(self.book, self.chapter_no, &now);
                } else {
                    let _ = sh.log_reading(self.book, self.chapter_no, &now);
                }
                Task::none()
            }
            BibleMsg::OpenReadingLog => {
                request = Some(Request::OpenNote(crate::shared::READING_LOG.to_string()));
                Task::none()
            }
            BibleMsg::ToggleReadCard => {
                self.read_card = !self.read_card;
                Task::none()
            }
            BibleMsg::OpenSearch => {
                request = Some(Request::OpenSearch);
                Task::none()
            }
            BibleMsg::SearchSelection => {
                // The selected words (the first verse's, if it runs over several), as typed in the search box.
                let words = self.selection_context().map(|(slices, canon)| {
                    slices
                        .iter()
                        .filter_map(|s| canon.get(&s.verse).and_then(|t| t.get(s.start..s.end)))
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                });
                request = words.filter(|w| !w.is_empty()).map(|w| Request::Search(if w.contains(' ') { format!("\"{w}\"") } else { w }));
                Task::none()
            }
            BibleMsg::OpenCommentary => {
                self.menu_open = false;
                let verse = self.selection_span().and_then(|r| r.verses).map(|(a, _)| a);
                request = Some(Request::OpenCommentary(self.book, self.chapter_no, verse));
                Task::none()
            }
            BibleMsg::OpenBacklinks => {
                request = self.selection_span().map(Request::OpenBacklinks);
                Task::none()
            }
            BibleMsg::OpenMarkInWriter(i) => {
                self.popup = None;
                self.editing = None;
                request = Some(Request::OpenMark(i));
                Task::none()
            }
            BibleMsg::SelectionChanged(sel) => {
                // A new selection ends an edit; so does losing the selection a no-margin
                // reader was showing the card under (clicking elsewhere, Esc).
                if sel.is_some() || (self.editing.is_some() && !self.marks_in_margin()) {
                    self.stop_edit(sh);
                }
                self.text_selection = sel;
                Task::none()
            }
            BibleMsg::ApplyPen(i) => self.apply_pen(sh, i),
            BibleMsg::Link(Link::Footnote(text), at, marks) => {
                // With a margin the footnotes are already showing beside the text.
                if !self.footnotes_in_margin() {
                    self.open_note_card(sh, at, NoteContent::Footnote(text));
                }
                self.marks_clicked(sh, marks, false)
            }
            BibleMsg::Link(Link::Strongs(key), at, marks) => {
                // With the margin keeping them, every entry is already showing beside the text.
                if !(self.margin_showing && self.in_margin.strongs) {
                    let entry = sh.backend.lexicon_entry(&key);
                    self.open_note_card(sh, at, NoteContent::Word { key, entry });
                }
                // A word inside marked text is both: show its entry and its marks, so either
                // can be picked.
                self.marks_clicked(sh, marks, false)
            }
            BibleMsg::Link(Link::CrossRefs(refs), at, marks) => {
                // With the margin keeping them, the lists are already showing beside the text.
                if !(self.margin_showing && self.in_margin.refs) {
                    self.open_note_card(sh, at, NoteContent::References(refs));
                }
                self.marks_clicked(sh, marks, false)
            }
            BibleMsg::Link(Link::Ref(target), _, _) => {
                request = Some(Request::Open(Spec::Verse { range: target, module: None }));
                Task::none()
            }
            BibleMsg::CloseNoteCard => {
                self.note_card = None;
                Task::none()
            }
            BibleMsg::OpenLink(target) => {
                request = Some(Request::Open(Spec::from_target(&target)));
                Task::none()
            }
            BibleMsg::Speak(word) => {
                if let Err(why) = crate::speech::speak(&word) {
                    sh.toast(why);
                }
                Task::none()
            }
            BibleMsg::CardPressed(_) => Task::none(),
            BibleMsg::OpenWord(key) => {
                // The definition moves to a tab, so the pop-over has done its work.
                self.note_card = None;
                request = Some(Request::Open(Spec::Word { key }));
                Task::none()
            }
            BibleMsg::LinkMenu(link, at) => {
                request = match link {
                    Link::Ref(range) => Some(Request::Menu(Spec::Verse { range, module: None }, at)),
                    Link::Strongs(key) => Some(Request::Menu(Spec::Word { key }, at)),
                    Link::Footnote(_) | Link::CrossRefs(_) => None,
                };
                Task::none()
            }
            BibleMsg::LinkDrag(link, at) => {
                request = match link {
                    Link::Ref(range) => Some(Request::Drag(Spec::Verse { range, module: None }, at)),
                    Link::Strongs(key) => Some(Request::Drag(Spec::Word { key }, at)),
                    Link::Footnote(_) | Link::CrossRefs(_) => None,
                };
                Task::none()
            }
            BibleMsg::CardLinkDrag(target, at) => {
                request = Some(Request::Drag(Spec::from_target(&target), at));
                Task::none()
            }
            BibleMsg::CardLinkMenu(target, at) => {
                request = Some(Request::Menu(Spec::from_target(&target), at));
                Task::none()
            }
            BibleMsg::PickModule(ModuleChoice(id, _)) => {
                self.module = id;
                sh.status.clear();
                let (book, chapter) = if sh.backend.available_chapters(&self.module).contains(&(self.book, self.chapter_no)) {
                    (self.book, self.chapter_no)
                } else {
                    sh.first_chapter(&self.module)
                };
                self.load(sh, book, chapter);
                Task::none()
            }
            BibleMsg::PickBook(BookChoice(book)) => {
                sh.status.clear();
                self.load(sh, book, 1);
                Task::none()
            }
            BibleMsg::PickChapter(ChapterChoice(book, chapter)) => {
                sh.status.clear();
                self.load(sh, book, chapter);
                Task::none()
            }
            BibleMsg::PrevChapter | BibleMsg::NextChapter => {
                let (prev, next) = self.neighbours(sh);
                if let Some((book, chapter)) = if matches!(message, BibleMsg::PrevChapter) { prev } else { next } {
                    sh.status.clear();
                    self.load(sh, book, chapter);
                }
                Task::none()
            }
            BibleMsg::ToggleStrongs(on) => {
                self.show_strongs = on;
                self.refresh_words(sh);
                Task::none()
            }
            BibleMsg::ToggleMarks(on) => {
                self.show_marks = on;
                // What was being done with marks goes with them.
                self.popup = None;
                self.stop_edit(sh);
                Task::none()
            }
            BibleMsg::ToggleFootnotes(on) => {
                self.show_footnotes = on;
                if matches!(self.note_card, Some(NoteCard { content: NoteContent::Footnote(_), .. })) {
                    self.note_card = None;
                }
                Task::none()
            }
            BibleMsg::ToggleRefs(on) => {
                self.show_refs = on;
                if matches!(self.note_card, Some(NoteCard { content: NoteContent::References(_), .. })) {
                    self.note_card = None;
                }
                Task::none()
            }
            BibleMsg::ToggleMarginMenu => {
                self.margin_menu_open = !self.margin_menu_open;
                Task::none()
            }
            BibleMsg::SetInMargin(kind, on) => {
                match kind {
                    NoteKind::Strongs => self.in_margin.strongs = on,
                    NoteKind::Marks => self.in_margin.marks = on,
                    NoteKind::Footnotes => self.in_margin.footnotes = on,
                    NoteKind::Refs => self.in_margin.refs = on,
                }
                self.refresh_words(sh);
                // Whatever was open is where it was; start afresh.
                self.stop_edit(sh);
                self.popup = None;
                self.note_card = None;
                Task::none()
            }
            BibleMsg::ToggleMenu => {
                self.menu_open = !self.menu_open;
                self.history_open = false;
                Task::none()
            }
            BibleMsg::Back => return (self.step(sh, true), None),
            BibleMsg::Forward => return (self.step(sh, false), None),
            BibleMsg::GoTo(index) => {
                self.history.current_mut().scroll = self.scroll_y;
                let place = self.history.jump(index).cloned();
                self.history_open = false;
                return (place.map_or_else(Task::none, |p| self.restore(sh, p)), None);
            }
            BibleMsg::ToggleHistory => {
                self.history_open = !self.history_open;
                self.menu_open = false;
                Task::none()
            }
            BibleMsg::CloseHistory => {
                self.history_open = false;
                Task::none()
            }
            BibleMsg::Scrolled(y) => {
                self.scroll_y = y;
                Task::none()
            }
            BibleMsg::CardTagDraft(draft) => {
                self.card_tag_draft = draft;
                Task::none()
            }
            BibleMsg::AddCardTag(tag) => {
                // Typed and entered (the box), or picked from the suggestions.
                let tag = if tag.is_empty() { std::mem::take(&mut self.card_tag_draft) } else { tag };
                self.card_tag_draft.clear();
                if let Some(index) = self.editing
                    && let Some(mark) = sh.marks.get(index)
                {
                    let mut tags = mark.tags.clone();
                    tags.push(tag);
                    sh.set_mark_tags(index, tags);
                }
                Task::none()
            }
            BibleMsg::RemoveCardTag(tag) => {
                if let Some(index) = self.editing
                    && let Some(mark) = sh.marks.get(index)
                {
                    let tags = mark.tags.iter().filter(|t| **t != tag).cloned().collect();
                    sh.set_mark_tags(index, tags);
                }
                Task::none()
            }
            BibleMsg::CloseMenu => {
                self.menu_open = false;
                Task::none()
            }
        };

        let task = if before == (self.book, self.chapter_no) {
            task
        } else {
            snap_to(self.reader_id(), RelativeOffset::START)
        };
        (task, request)
    }

    // ---- view ----------------------------------------------------------------------

    pub fn view<'a>(&'a self, sh: &'a Shared) -> Element<'a, BibleMsg> {
        let content = column![self.toolbar(sh), iced::widget::rule::horizontal(1), self.reader_pane(sh)];
        // A pop-up (the history list or the reader's menu) hangs from its button; a click anywhere
        // else puts it away. The layers are always in the tree, empty when there is no pop-up, so
        // that opening one does not rebuild the reader beneath (which would lose its scroll).
        let nothing = || -> Element<'a, BibleMsg> { space::horizontal().width(0).height(0).into() };
        let dismiss = |msg: BibleMsg| -> Element<'a, BibleMsg> {
            iced::widget::mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill)).on_press(msg).into()
        };
        let (away, panel): (Element<'a, BibleMsg>, Element<'a, BibleMsg>) = if self.history_open {
            let panel = container(iced::widget::opaque(self.history_panel(sh)))
                .padding(iced::Padding { top: 8.0 + 31.0 + 3.0, right: 0.0, bottom: 0.0, left: 16.0 })
                .width(Length::Fill)
                .height(Length::Fill);
            (dismiss(BibleMsg::CloseHistory), panel.into())
        } else if self.menu_open {
            let panel = container(iced::widget::opaque(self.menu_panel()))
                .padding(iced::Padding { top: 8.0 + 31.0 + 3.0, right: 16.0, bottom: 0.0, left: 0.0 })
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right);
            (dismiss(BibleMsg::CloseMenu), panel.into())
        } else {
            (nothing(), nothing())
        };
        iced::widget::stack![content, away, panel].into()
    }

    /// The places this tab has been, newest first; the one it is at is marked.
    fn history_panel(&self, sh: &Shared) -> Element<'_, BibleMsg> {
        let (entries, at) = self.history.entries();
        let mut list = column![].spacing(2);
        for (i, place) in entries.iter().enumerate().rev() {
            let label = if i == at { format!("\u{2022} {}", place.label(sh)) } else { format!("  {}", place.label(sh)) };
            list = list.push(button(text(label).size(14)).width(Length::Fill).style(button::text).on_press(BibleMsg::GoTo(i)));
        }
        container(scrollable(list).height(Length::Shrink)).padding(8).max_width(300).max_height(360).style(crate::popup_style).into()
    }

    /// What the reader shows: each kind of thing in the text, on or off.
    fn menu_panel(&self) -> Element<'_, BibleMsg> {
        let item = |label: &'static str, on: bool, msg: fn(bool) -> BibleMsg| {
            checkbox(on).label(label).on_toggle(msg).size(16).text_size(14)
        };
        container(
            column![
                item("Strong's numbers", self.show_strongs, BibleMsg::ToggleStrongs),
                item("Marks", self.show_marks, BibleMsg::ToggleMarks),
                item("Footnotes", self.show_footnotes, BibleMsg::ToggleFootnotes),
                item("References", self.show_refs, BibleMsg::ToggleRefs),
                button(text("Commentary\u{2026}").size(14)).padding([2, 0]).style(button::text).on_press(BibleMsg::OpenCommentary),
            ]
            .spacing(10),
        )
        .padding(14)
        .width(230)
        .style(crate::popup_style)
        .into()
    }

    /// The chapters before and after this one in the module.
    fn neighbours(&self, sh: &Shared) -> (Option<ChapterId>, Option<ChapterId>) {
        let all = sh.backend.available_chapters(&self.module);
        let Some(at) = all.iter().position(|&c| c == (self.book, self.chapter_no)) else { return (None, None) };
        (at.checked_sub(1).map(|i| all[i]), all.get(at + 1).copied())
    }

    /// The button at the foot of the text that puts the chapter in the reading log.
    fn log_button<'a>(&'a self, sh: &'a Shared) -> Element<'a, BibleMsg> {
        let size = sh.settings.bible_size * 0.62;
        // Quiet: words in the faint text color with no box, lit when pointed at. A green check mark
        // says the chapter was read today; pressing it again takes the mark back.
        let read = sh.read_on(self.book, self.chapter_no, &chrono::Local::now());
        let label = if read { "\u{2713} Marked as read" } else { "Mark as read" };
        let quiet = move |theme: &Theme, status: button::Status, glow: bool| {
            let p = theme.extended_palette();
            let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let color = if glow { p.success.base.color } else { Color { a: if lit { 0.9 } else { 0.5 }, ..p.background.base.text } };
            button::Style { text_color: color, border: iced::Border { radius: 6.0.into(), ..iced::Border::default() }, ..button::Style::default() }
        };
        let mark = button(text(label).size(size)).padding([4, 10]).style(move |theme, status| quiet(theme, status, read)).on_press(BibleMsg::LogChapter);
        // To its left, a small clock: the days the chapter was read, in a card.
        let history = button(crate::icons::Icon::History.view(size + 2.0))
            .padding([4, 6])
            .style(move |theme, status| quiet(theme, status, false))
            .on_press(BibleMsg::ToggleReadCard);
        let mut out = column![container(row![history, mark].spacing(2).align_y(iced::Center)).width(Length::Fill).center_x(Length::Fill)].spacing(8);
        if self.read_card {
            out = out.push(container(self.read_card_view(sh, size)).width(Length::Fill).center_x(Length::Fill));
        }
        out.into()
    }

    /// The days the chapter was read, and a way to the log itself.
    fn read_card_view<'a>(&'a self, sh: &'a Shared, size: f32) -> Element<'a, BibleMsg> {
        let days = sh.days_read(self.book, self.chapter_no);
        let name = format!("{} {}", self.book, self.chapter_no);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let heading = match days.len() {
            0 => format!("{name} has not been read yet"),
            1 => format!("{name} was read once"),
            n => format!("{name} was read on {n} days"),
        };
        let mut list = column![text(heading).size(size)].spacing(4);
        for day in &days {
            let label = if *day == today { format!("{day}  \u{b7}  today") } else { day.clone() };
            list = list.push(text(label).size(size).style(text::secondary));
        }
        let open = button(text("Open the reading log").size(size)).padding([2, 8]).style(button::secondary).on_press(BibleMsg::OpenReadingLog);
        container(column![list, open].spacing(8))
            .padding(12)
            .width(260)
            .style(crate::popup_style)
            .into()
    }

    /// Previous and next chapter, to put at the top and bottom of the text.
    fn chapter_links<'a>(&'a self, sh: &'a Shared) -> Element<'a, BibleMsg> {
        let (prev, next) = self.neighbours(sh);
        let size = sh.settings.bible_size * 0.7;
        let link = |label: String, msg: BibleMsg, target: Option<ChapterId>| {
            let b = button(text(label).size(size)).padding([2, 6]).style(button::text);
            if target.is_some() { b.on_press(msg) } else { b }
        };
        let prev_label = prev.map_or("\u{2190} Previous".to_string(), |(b, c)| format!("\u{2190} {b} {c}"));
        let next_label = next.map_or("Next \u{2192}".to_string(), |(b, c)| format!("{b} {c} \u{2192}"));
        row![link(prev_label, BibleMsg::PrevChapter, prev), space::horizontal(), link(next_label, BibleMsg::NextChapter, next)]
            .align_y(iced::Center)
            .into()
    }

    fn toolbar<'a>(&'a self, sh: &'a Shared) -> Element<'a, BibleMsg> {
        let available = sh.backend.available_chapters(&self.module);
        let mut books: Vec<BookChoice> = Vec::new();
        for (b, _) in &available {
            if books.last().is_none_or(|l| l.0 != *b) {
                books.push(BookChoice(*b));
            }
        }
        let chapters: Vec<ChapterChoice> = available
            .into_iter()
            .filter(|(b, _)| *b == self.book)
            .map(|(b, c)| ChapterChoice(b, c))
            .collect();
        let current = ChapterChoice(self.book, self.chapter_no);
        let modules: Vec<ModuleChoice> =
            sh.backend.modules().into_iter().map(|m| ModuleChoice(m.id, m.abbreviation)).collect();
        let module = modules.iter().find(|m| m.0 == self.module).cloned();
        let menu = button(container(crate::icons::Icon::Menu.view(18.0)).center(Length::Fill))
            .width(31)
            .height(31)
            .padding(0)
            .style(button::secondary)
            .on_press(BibleMsg::ToggleMenu);
        let search = button(container(crate::icons::Icon::Search.view(18.0)).center(Length::Fill))
            .width(31)
            .height(31)
            .padding(0)
            .style(button::secondary)
            .on_press(BibleMsg::OpenSearch);
        let arrow = |icon: crate::icons::Icon, on: bool, msg: BibleMsg| {
            let b = button(container(icon.view(18.0)).center(Length::Fill)).width(31).height(31).padding(0).style(button::secondary);
            if on { b.on_press(msg) } else { b }
        };
        // Right-clicking either lists the places the tab has been.
        let nav = iced::widget::mouse_area(
            row![
                arrow(crate::icons::Icon::Back, self.history.can_back(), BibleMsg::Back),
                arrow(crate::icons::Icon::Forward, self.history.can_forward(), BibleMsg::Forward),
            ]
            .spacing(4),
        )
        .on_right_press(BibleMsg::ToggleHistory);
        let bar = row![
            nav,
            pick_list(modules, module, BibleMsg::PickModule),
            pick_list(books, Some(BookChoice(self.book)), BibleMsg::PickBook),
            pick_list(chapters, Some(current), BibleMsg::PickChapter),
            space::horizontal(),
            row![search, menu].spacing(4),
        ]
        .spacing(12)
        .align_y(iced::Center);
        container(bar).padding([8, 16]).into()
    }

    pub(crate) fn view_options(&self, sh: &Shared) -> reader::ViewOptions {
        reader::ViewOptions {
            select: self.select_request,
            select_words: self.select_words,
            restore: self.text_selection.clone(),
            annotated: sh.annotated_verses(self.book, self.chapter_no),
            reveal: self.reveal,
            marks: self.mark_draws(sh),
            clear_token: self.clear_token,
            text_width: Some(TEXT_COLUMN),
            show_strongs: self.show_strongs,
            show_footnotes: self.show_footnotes,
            show_refs: self.show_refs,
            pal: Palette::for_theme(&sh.theme()),
            strongs_in_margin: self.margin_showing && self.in_margin.strongs,
            footnotes_in_margin: self.margin_showing && self.in_margin.footnotes,
            refs_in_margin: self.margin_showing && self.in_margin.refs,
            body: sh.settings.bible_size,
            font: sh.settings.text_font(),
            module: sh.module_abbreviation(&self.module),
        }
    }

    /// Cards for the marks in view that have something written, plus the one being written.
    pub fn cards<'a>(&'a self, sh: &'a Shared) -> Vec<CardSpec<'a, BibleMsg>> {
        let mut out = Vec::new();
        // Marks' cards are in the margin only when marks are kept there (else they pop up).
        for (index, slices) in self.visible_marks(sh).into_iter().filter(|_| self.in_margin.marks) {
            let mark = &sh.marks[index];
            let writing = self.editing == Some(index);
            if mark.body.trim().is_empty() && !writing {
                continue;
            }
            let Some(first) = slices.first() else { continue };
            out.push(CardSpec {
                key: index as u64,
                anchor: CanonSlice { verse: first.verse, start: first.start, end: first.end },
                color: hex_color(mark.shown_color(), 1.0).unwrap_or(Color::WHITE),
                pinned: writing,
                follows_link: false,
                header: false,
                element: CardElement::new(move || self.card_view(sh, index, false)),
            });
        }
        // Beside the text every footnote of the chapter is shown, so none can be closed. (Offered
        // whether or not there is a margin yet: the reader only makes one when it has cards, so
        // waiting for it to say it has one would leave a chapter of only footnotes without.)
        if self.show_footnotes
            && self.in_margin.footnotes
            && let Ok(chapter) = &self.chapter
        {
            let canon = chapter.canonical_verses();
            let color = Palette::for_theme(&sh.theme()).link;
            for (i, note) in chapter.footnotes().iter().enumerate() {
                let Some(anchor) = footnote_anchor(&canon, note) else { continue };
                out.push(CardSpec {
                    key: FOOTNOTE_KEY + i as u64,
                    anchor,
                    color,
                    pinned: false,
                    follows_link: true,
                        header: false,
                    element: {
                        let text = note.text.clone();
                        CardElement::new(move || reference_card(sh, "Footnote", footnote_body(sh, &text, color), false, false, None))
                    },
                });
            }
        }
        // Every cross reference marker of the chapter has its list of passages beside the text,
        // like a footnote, when they are kept in the margin.
        if self.show_refs
            && self.in_margin.refs
            && let Ok(chapter) = &self.chapter
        {
            let canon = chapter.canonical_verses();
            let color = Palette::for_theme(&sh.theme()).link;
            for (i, marker) in chapter.cross_references().iter().enumerate() {
                let Some(anchor) = marker_anchor(&canon, marker.verse, marker.offset) else { continue };
                out.push(CardSpec {
                    key: CROSSREF_KEY + i as u64,
                    anchor,
                    color,
                    pinned: false,
                    follows_link: true,
                        header: false,
                    element: {
                        let refs = marker.refs.clone();
                        CardElement::new(move || reference_card(sh, "References", self.reference_list(sh, &refs), false, false, None))
                    },
                });
            }
        }
        // Every word with a Strong's number has its dictionary entry beside the text, when they
        // are kept in the margin: hovering the word brings its entry up.
        if self.show_strongs && self.in_margin.strongs {
            let color = Palette::for_theme(&sh.theme()).link;
            for (i, word) in self.words.iter().enumerate() {
                out.push(CardSpec {
                    key: WORD_KEY + i as u64,
                    anchor: word.anchor,
                    color,
                    pinned: false,
                    follows_link: true,
                    header: false,
                    element: CardElement::new(move || {
                        reference_card(
                            sh,
                            &format!("Strong's {}", word.entry.key),
                            word_body(sh, &word.entry),
                            false,
                            false,
                            Some(("Open in tab", BibleMsg::OpenWord(word.entry.key.clone()))),
                        )
                    }),
                });
            }
        }
        // The margin's own menu, at the top of it, once there is something to put in one.
        {
            out.push(CardSpec {
                key: MARGIN_MENU_KEY,
                anchor: CanonSlice { verse: 1, start: 0, end: 0 },
                color: Color::TRANSPARENT,
                pinned: false,
                follows_link: false,
                header: true,
                element: CardElement::new(move || self.margin_menu()),
            });
        }
        out
    }

    /// The tags of the mark being written in: each can be taken off, and others added, with the
    /// ones in use offered as the name is typed.
    fn card_tags<'a>(&'a self, sh: &'a Shared, index: usize) -> Element<'a, BibleMsg> {
        let meta = sh.settings.bible_size * 0.7;
        let tags = sh.marks.get(index).map(|m| m.tags.clone()).unwrap_or_default();
        let chip = |label: String, color: Option<Color>, msg: BibleMsg| {
            let color = color.unwrap_or(Color::from_rgb(0.62, 0.65, 0.72));
            button(text(label).size(meta))
                .padding([0, 6])
                .style(move |_: &Theme, _| button::Style {
                    text_color: color,
                    border: iced::Border { color, width: 1.0, radius: 8.0.into() },
                    background: Some(Color { a: 0.12, ..color }.into()),
                    ..button::Style::default()
                })
                .on_press(msg)
        };
        let mut chips = row![].spacing(4).align_y(iced::Center);
        for tag in &tags {
            let color = sh.tag_color(tag).and_then(|hex| hex_color(&hex, 1.0));
            chips = chips.push(chip(format!("#{tag} \u{00D7}"), color, BibleMsg::RemoveCardTag(tag.clone())));
        }
        chips = chips.push(
            iced::widget::text_input("Add tag", &self.card_tag_draft)
                .on_input(BibleMsg::CardTagDraft)
                .on_submit(BibleMsg::AddCardTag(String::new()))
                .size(meta)
                .padding([1, 6])
                .width(90),
        );
        let mut out = column![chips].spacing(4);
        let offered = sh.tag_suggestions(&self.card_tag_draft, &tags, 5);
        if !offered.is_empty() {
            let mut row = row![].spacing(4);
            for (tag, _) in offered {
                let color = sh.tag_color(&tag).and_then(|hex| hex_color(&hex, 1.0));
                row = row.push(chip(format!("#{tag}"), color, BibleMsg::AddCardTag(tag)));
            }
            out = out.push(row);
        }
        out.into()
    }

    /// The button at the top of the margin and, when open, its choice.
    fn margin_menu<'a>(&self) -> Element<'a, BibleMsg> {
        let button = button(container(crate::icons::Icon::Menu.view(16.0)).center(Length::Fill))
            .width(26)
            .height(26)
            .padding(0)
            .style(button::secondary)
            .on_press(BibleMsg::ToggleMarginMenu);
        let mut column = column![row![space::horizontal(), button]].spacing(4).width(Length::Fill);
        if self.margin_menu_open {
            column = column.push(
                container(self.where_notes_show()).padding(12).width(Length::Fill).style(crate::popup_style),
            );
        }
        column.into()
    }

    /// The same list as the reader's menu, but of which kinds of note sit in the margin: the
    /// reader's menu says whether a kind shows at all, this says where its cards go.
    fn where_notes_show<'a>(&self) -> Element<'a, BibleMsg> {
        let item = |label: &'static str, on: bool, kind: NoteKind| {
            checkbox(on).label(label).on_toggle(move |on| BibleMsg::SetInMargin(kind, on)).size(16).text_size(14)
        };
        column![
            text("In the margin").size(12).style(text::secondary),
            item("Strong's numbers", self.in_margin.strongs, NoteKind::Strongs),
            item("Marks", self.in_margin.marks, NoteKind::Marks),
            item("Footnotes", self.in_margin.footnotes, NoteKind::Footnotes),
            item("References", self.in_margin.refs, NoteKind::Refs),
        ]
        .spacing(10)
        .into()
    }

    /// The card for a footnote or a dictionary entry opened by a click.
    pub fn note_card_view<'a>(&'a self, sh: &'a Shared, card: &'a NoteCard, fill: bool) -> Element<'a, BibleMsg> {
        let (size, font) = (card_size(sh), sh.settings.text_font());
        let (label, body): (String, Element<'_, BibleMsg>) = match &card.content {
            NoteContent::Footnote(note) => {
                ("Footnote".into(), footnote_body(sh, note, Palette::for_theme(&sh.theme()).link))
            }
            NoteContent::References(refs) => ("References".into(), self.reference_list(sh, refs)),
            NoteContent::Word { key, entry: None } => (
                format!("Strong's {key}"),
                text("No installed dictionary has an entry for this number.").size(size).font(font).style(text::secondary).into(),
            ),
            NoteContent::Word { key, entry: Some(e) } => (format!("Strong's {key}"), word_body(sh, e)),
        };
        // A dictionary entry can be taken to a tab of its own.
        let open = match &card.content {
            NoteContent::Word { key, entry: Some(_) } => Some(("Open in tab", BibleMsg::OpenWord(key.clone()))),
            _ => None,
        };
        reference_card(sh, &label, body, true, fill, open)
    }

    /// The passages a cross reference marker points to, each a link to open it, with the start of
    /// its text so it can be told from the others.
    fn reference_list<'a>(&self, sh: &Shared, refs: &[RefRange]) -> Element<'a, BibleMsg> {
        let mut list = column![].spacing(6);
        for range in refs {
            let preview = sh
                .backend
                .chapter(&self.module, range.book, range.chapter)
                .ok()
                .map(|c| c.plain_text(range))
                .map(|t| {
                    let short: String = t.chars().take(70).collect();
                    if t.chars().count() > 70 { format!("{short}\u{2026}") } else { short }
                })
                .filter(|t| !t.is_empty());
            let mut entry = column![
                link_view(sh, format!("[[{range}]]"))
            ]
            .spacing(1);
            if let Some(preview) = preview {
                entry = entry.push(text(preview).size(card_size(sh) * 0.8).font(sh.settings.text_font()).style(text::secondary));
            }
            list = list.push(entry);
        }
        list.into()
    }

    /// A mark's card. In the floating popup (`popup`) its actions are always offered, since
    /// there is nowhere else to reach them; in the margin only while it is being written.
    pub fn card_view<'a>(&'a self, sh: &'a Shared, index: usize, popup: bool) -> Element<'a, BibleMsg> {
        let mark = &sh.marks[index];
        let color = hex_color(mark.shown_color(), 1.0).unwrap_or(Color::WHITE);
        let writing = self.editing == Some(index);

        let meta = card_size(sh) * 0.7;
        let about = if mark.is_verse_level() {
            mark.anchor.to_string()
        } else {
            let quote: String = mark.quote.chars().take(30).collect();
            let more = if mark.quote.chars().count() > 30 { "…" } else { "" };
            format!("“{quote}{more}”")
        };
        // A card being written, or floating in a popup, takes the width it is given; one resting
        // in the margin is only as wide as its text needs.
        let fill = writing || popup;
        let header = if fill {
            row![
                text(about).size(meta).style(text::secondary),
                space::horizontal(),
                text(mark.tags.join(", ")).size(meta).style(text::secondary),
            ]
        } else {
            row![text(about).size(meta).style(text::secondary), text(mark.tags.join(", ")).size(meta).style(text::secondary)]
        }
        .spacing(12);

        let pal = Palette::for_theme(&sh.theme());
        let module = self.module.clone();
        let full = mark.body.trim().to_string();
        let bare = full.is_empty() && !writing;
        let body: Element<'_, BibleMsg> = if writing {
            live_editor(&self.card_doc, pal)
                .id(self.card_editor_id())
                .on_action(BibleMsg::CardAction)
                .on_link(BibleMsg::OpenLink)
                .on_link_menu(BibleMsg::CardLinkMenu)
                .on_link_drag(BibleMsg::CardLinkDrag)
                .embed(move |target| sh.embed_for(&module, target))
                .size(card_size(sh))
                .font(sh.settings.text_font())
                .padding(4)
                // The editor scrolls itself once its text outgrows this.
                .max_height(card_body_max(sh))
                .into()
        } else if bare {
            button(text("Add a note…").size(card_size(sh)).font(sh.settings.text_font()))
                .padding(0)
                .width(Length::Fill)
                .style(button::text)
                .on_press(BibleMsg::StartEdit(index))
                .into()
        } else {
            // The comment rendered like the writer renders it (links, bold, checkboxes). Long
            // text scrolls inside the card instead of making the card tall.
            live_view(Document::new(&full), pal)
                .fit_width()
                .size(card_size(sh))
                .font(sh.settings.text_font())
                .padding([2.0, 0.0])
                .max_height(card_body_max(sh))
                .on_link(BibleMsg::OpenLink)
                .on_link_menu(BibleMsg::CardLinkMenu)
                .on_link_drag(BibleMsg::CardLinkDrag)
                .embed(move |target| sh.embed_for(&module, target))
                .on_action(move |action| match action {
                    EditAction::ToggleTask(at) => BibleMsg::ToggleCardTask(index, at),
                    _ => BibleMsg::StartEdit(index),
                })
                .on_press(move || BibleMsg::StartEdit(index))
                .into()
        };

        let small = |label: &'static str, msg: BibleMsg| {
            button(text(label).size(meta)).padding([2, 6]).style(button::text).on_press(msg)
        };
        let mut footer = row![].spacing(2);
        if writing || bare || popup {
            footer = footer
                .push(small("Open", BibleMsg::OpenMarkInWriter(index)))
                .push(small("Unlink", BibleMsg::UnlinkMarkAt(index)))
                .push(small("Delete", BibleMsg::DeleteMarkAt(index)));
        }

        let width = if fill { Length::Fill } else { Length::Shrink };
        let mut content = column![header].spacing(6).width(width);
        if writing {
            content = content.push(self.card_tags(sh, index));
        }
        let content = content.push(body).push(footer);
        container(content)
            .padding(10)
            .width(width)
            .style(move |theme: &Theme| {
                let base = container::rounded_box(theme);
                container::Style {
                    border: iced::Border { color, width: 1.0, radius: 6.0.into() },
                    ..base
                }
            })
            .into()
    }

    /// The element the reader floats over its text: the cards of a clicked mark (when the
    /// reader is too narrow for a margin), otherwise the pen toolbar under a text selection.
    pub fn floating<'a>(&'a self, sh: &'a Shared) -> Option<(FloatKind, Element<'a, BibleMsg>)> {
        // A dictionary entry (or a footnote, in a reader with no margin to put it in) pops over
        // the text under the click. Dictionaries never take a place in the margin.
        // A click on a Strong's word inside a mark shows both, one above the other.
        let marks = self.popup.as_ref().filter(|_| !self.marks_in_margin());
        let note_card = self.note_card.as_ref();
        if note_card.is_some() || marks.is_some() {
            let mut cards = column![].spacing(6);
            if let Some(card) = note_card {
                cards = cards.push(self.note_card_view(sh, card, true));
            }
            for &i in marks.into_iter().flatten().filter(|&&i| i < sh.marks.len()) {
                cards = cards.push(self.card_view(sh, i, true));
            }
            let popup = container(cards).padding(6).width(360).style(container::rounded_box);
            return Some((FloatKind::UnderClick, popup.into()));
        }
        // A comment being written in a reader with no margin: its card, under the selection.
        if let Some(index) = self.editing.filter(|&i| i < sh.marks.len())
            && !self.marks_in_margin()
            && self.text_selection.is_some()
        {
            let card = container(self.card_view(sh, index, true)).padding(6).width(360).style(container::rounded_box);
            return Some((FloatKind::UnderSelection, card.into()));
        }
        self.text_selection.is_some().then(|| (FloatKind::UnderSelection, pen_toolbar(sh, self.selection_has_backlinks(sh))))
    }

    fn reader_pane<'a>(&'a self, sh: &'a Shared) -> Element<'a, BibleMsg> {
        let body: Element<'_, BibleMsg> = match &self.chapter {
            Ok(chapter) => {
                // The toolbar is only offered while some text is selected; the reader floats
                // it under that selection.
                reader::chapter_view(chapter, &self.view_options(sh), self.floating(sh), self.cards(sh), reader_message)
            }
            Err(_) if sh.backend.modules().iter().all(|m| m.id != self.module) => text("This module is not installed. Add modules in Modules, or pick another above.").style(text::secondary).into(),
            Err(e) => text(e.clone()).into(),
        };
        let bold = Font { weight: font::Weight::Bold, ..sh.settings.text_font() };
        let title = text(format!("{} {}", self.book, self.chapter_no)).size(sh.settings.bible_size * 1.6).font(bold);

        let scroller = scrollable(container(column![self.chapter_links(sh), title, body, self.chapter_links(sh), self.log_button(sh), space::vertical().height(20)].spacing(16)).padding(32).width(Length::Fill))
            .id(self.reader_id())
            .on_scroll(|v| BibleMsg::Scrolled(v.absolute_offset().y))
            .height(Length::Fill);

        column![scroller].into()
    }
}

/// A card for reference material (a footnote, a dictionary entry): a label, the body, and
/// a close button when it can be closed. A long body scrolls inside the card.
pub fn reference_card<'a>(
    sh: &Shared,
    label: &str,
    body: Element<'a, BibleMsg>,
    closable: bool,
    fill: bool,
    action: Option<(&'static str, BibleMsg)>,
) -> Element<'a, BibleMsg> {
    let color = Palette::for_theme(&sh.theme()).link;
    // A dictionary entry's card, pressed with the command key, is taken to a tab of its own.
    let pressed = match &action {
        Some((_, BibleMsg::OpenWord(key))) => Some(key.clone()),
        _ => None,
    };
    let meta = card_size(sh) * 0.7;
    let mut header = row![text(label.to_string()).size(meta).style(text::secondary)]
        .spacing(16)
        .align_y(iced::Alignment::Center);
    if fill {
        header = header.push(space::horizontal());
    }
    if let Some((label, message)) = action {
        header = header.push(button(text(label).size(meta)).padding([0, 6]).style(button::text).on_press(message));
    }
    if closable {
        header = header
            .push(button(text("✕").size(meta)).padding([0, 4]).style(button::text).on_press(BibleMsg::CloseNoteCard));
    }
    let width = if fill { Length::Fill } else { Length::Shrink };
    let body = scrollable(container(body).padding([0, 8]).width(width)).height(Length::Shrink);
    let card: Element<'a, BibleMsg> = container(column![header, container(body).max_height(card_body_max(sh))].spacing(6).width(width))
        .padding(10)
        .width(width)
        .style(move |theme: &Theme| container::Style {
            border: iced::Border { color, width: 1.0, radius: 6.0.into() },
            ..container::rounded_box(theme)
        })
        .into();
    match pressed {
        Some(key) => iced::widget::mouse_area(card).on_press(BibleMsg::CardPressed(key)).into(),
        None => card,
    }
}
