//! The search tab: words of the Bible, found in one module. A query box and a module picker,
//! options for how to match, and the verses that fit as a list (with the matched words marked) in
//! canon order or best first, with a count for each book to narrow it. Choosing a verse shows it in
//! the reader with the words that matched pointed out. Self-contained like the other tab types.

use crate::bible_search::{Found, Options, Results};
use crate::bible_tab::{BookChoice, ModuleChoice};
use crate::shared::{Shared, TabId};
use iced::widget::{button, checkbox, column, container, pick_list, row, rule, scrollable, space, text, text_input};
use iced::{Color, Element, Font, Length, font};
use scripture::{Book, RefRange};
use std::collections::BTreeMap;
use study_core::ModuleId;
use study_core::search::Query;

/// How many verses show before "Show more".
const PAGE: usize = 100;

/// How long typing must pause before what is typed is searched.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(180);

#[derive(Debug, Clone)]
pub enum SearchMsg {
    Draft(String),
    /// A pause in typing: search if nothing has been typed since (the number counts the typing).
    Typed(u64),
    /// Search at once for what is typed (Enter).
    Go,
    /// The search itself, a moment after `Go` when the module's index had to be made first.
    Run,
    PickModule(ModuleChoice),
    Case(bool),
    Stem(bool),
    Best(bool),
    /// Only the verses of one book (or all).
    Book(Option<Book>),
    More,
    /// Show this verse in the reader, with these words (byte spans of its text) pointed out.
    Open(RefRange, ModuleId, Vec<(usize, usize)>),
}

/// A search that was made.
pub struct Ran {
    pub text: String,
    pub query: Query,
    pub module: ModuleId,
    pub results: Results,
}

pub struct SearchTab {
    pub id: TabId,
    pub module: ModuleId,
    pub draft: String,
    pub options: Options,
    pub ran: Option<Ran>,
    /// Only this book's verses are listed.
    pub book: Option<Book>,
    /// How many of the verses are listed.
    pub shown: usize,
    /// How many times the box was typed in, to tell the last from the ones before it.
    typed: u64,
    /// What is being waited for (making the index), or why the search failed.
    pub busy: Option<String>,
    pub error: Option<String>,
}

impl SearchTab {
    pub fn new(id: TabId, module: ModuleId) -> Self {
        SearchTab { id, module, draft: String::new(), options: Options::default(), ran: None, book: None, shown: PAGE, typed: 0, busy: None, error: None }
    }

    pub fn input_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("search-{}", self.id))
    }

    pub fn title(&self, sh: &Shared) -> String {
        let name = sh.module_abbreviation(&self.module);
        match &self.ran {
            Some(ran) if !ran.text.trim().is_empty() => format!("{} \u{00B7} {name}", ran.text.trim()),
            _ => format!("Search \u{00B7} {name}"),
        }
    }

    /// Search for `text` (put in the box), now if it can be.
    pub fn search_for(&mut self, sh: &Shared, text: &str) -> iced::Task<SearchMsg> {
        self.draft = text.to_string();
        self.go(sh)
    }

    fn go(&mut self, sh: &Shared) -> iced::Task<SearchMsg> {
        self.error = None;
        if self.draft.trim().is_empty() {
            self.ran = None;
            return iced::Task::none();
        }
        if sh.bible_index_ready(&self.module) {
            return self.run(sh);
        }
        // The index is made the first time: say so, and do it after the screen shows the saying.
        self.busy = Some(format!("Indexing {} for search\u{2026}", sh.module_abbreviation(&self.module)));
        iced::Task::perform(
            async {
                std::thread::sleep(std::time::Duration::from_millis(60));
            },
            |()| SearchMsg::Run,
        )
    }

    /// Make the search of what was typed.
    pub fn run(&mut self, sh: &Shared) -> iced::Task<SearchMsg> {
        self.busy = None;
        let query = Query::parse(&self.draft);
        match sh.search_bible(&self.module, &query, self.options) {
            Ok(results) => {
                self.ran = Some(Ran { text: self.draft.clone(), query, module: self.module.clone(), results });
                self.book = None;
                self.shown = PAGE;
            }
            Err(e) => {
                self.ran = None;
                self.error = Some(e);
            }
        }
        iced::Task::none()
    }

    pub fn update(&mut self, sh: &mut Shared, message: SearchMsg) -> iced::Task<SearchMsg> {
        match message {
            // The results follow the typing: searched when it pauses.
            SearchMsg::Draft(draft) => {
                self.draft = draft;
                self.typed += 1;
                let n = self.typed;
                return iced::Task::perform(
                    async {
                        std::thread::sleep(PAUSE);
                    },
                    move |()| SearchMsg::Typed(n),
                );
            }
            SearchMsg::Typed(n) => {
                if n == self.typed {
                    return self.go(sh);
                }
            }
            SearchMsg::Go => return self.go(sh),
            SearchMsg::Run => return self.run(sh),
            SearchMsg::PickModule(ModuleChoice(module, _)) => {
                self.module = module;
                self.ran = None;
                return self.go(sh);
            }
            // How to match changes what is found: it is searched again.
            SearchMsg::Case(on) => {
                self.options.case = on;
                return self.go(sh);
            }
            SearchMsg::Stem(on) => {
                self.options.stem = on;
                return self.go(sh);
            }
            SearchMsg::Best(on) => {
                self.options.by_relevance = on;
                return self.go(sh);
            }
            SearchMsg::Book(book) => {
                self.book = book;
                self.shown = PAGE;
            }
            SearchMsg::More => self.shown += 2 * PAGE,
            // Showing a verse in the reader is for the workspace to do.
            SearchMsg::Open(..) => {}
        }
        iced::Task::none()
    }

    /// The verses listed: all, or one book's.
    fn listed<'a>(&'a self, ran: &'a Ran) -> Vec<&'a Found> {
        ran.results.found.iter().filter(|f| self.book.is_none_or(|b| f.range.book == b)).collect()
    }

    pub fn view<'a>(&'a self, sh: &'a Shared) -> Element<'a, SearchMsg> {
        let modules: Vec<ModuleChoice> = sh.backend.modules().into_iter().map(|m| ModuleChoice(m.id, m.abbreviation)).collect();
        let module = modules.iter().find(|m| m.0 == self.module).cloned();
        let bar = row![
            pick_list(modules, module, SearchMsg::PickModule),
            text_input("Search the Bible\u{2026}", &self.draft)
                .id(self.input_id())
                .on_input(SearchMsg::Draft)
                .on_submit(SearchMsg::Go)
                .size(16)
                .padding([4, 10]),
        ]
        .spacing(12)
        .align_y(iced::Center);
        let option = |label: &'static str, on: bool, msg: fn(bool) -> SearchMsg| checkbox(on).label(label).on_toggle(msg).size(15).text_size(13);
        let options = row![
            option("Match case", self.options.case, SearchMsg::Case),
            option("Other forms of a word", self.options.stem, SearchMsg::Stem),
            option("Best matches first", self.options.by_relevance, SearchMsg::Best),
        ]
        .spacing(20);

        let secondary = |t: String| text(t).size(13).style(text::secondary);
        let mut page = column![].spacing(10);
        if let Some(why) = &self.busy {
            page = page.push(secondary(why.clone()));
        }
        if let Some(e) = &self.error {
            page = page.push(text(format!("The search failed: {e}")).size(13).style(text::danger));
        }
        match &self.ran {
            None if self.busy.is_none() && self.error.is_none() => page = page.push(self.help()),
            None => {}
            Some(ran) => {
                for problem in &ran.query.problems {
                    page = page.push(secondary(problem.clone()));
                }
                let name = sh.module_abbreviation(&ran.module);
                let total = ran.results.found.len();
                let listed = self.listed(ran);
                let summary = match (total, ran.results.cut) {
                    (0, _) => format!("No verses in {name} fit \u{201c}{}\u{201d}.", ran.text.trim()),
                    (n, true) => format!("The first {n} verses of many in {name}"),
                    (1, false) => format!("1 verse in {name}"),
                    (n, false) => format!("{n} verses in {name}"),
                };
                page = page.push(secondary(summary));
                if total > 0 {
                    page = page.push(secondary("Verses in each book (click to list only that book)".into()));
                    page = page.push(self.books(ran));
                    page = page.push(rule::horizontal(1));
                    let mut rows = column![].spacing(2);
                    for found in listed.iter().take(self.shown) {
                        rows = rows.push(self.row_of(sh, ran, found));
                    }
                    page = page.push(rows);
                    if listed.len() > self.shown {
                        page = page.push(
                            button(text(format!("Show more ({} left)", listed.len() - self.shown)).size(13)).padding([4, 12]).style(button::secondary).on_press(SearchMsg::More),
                        );
                    }
                }
            }
        }
        column![
            container(column![bar, options].spacing(8)).padding([8, 16]),
            rule::horizontal(1),
            scrollable(container(page).padding([12, 20]).width(Length::Fill)).height(Length::Fill),
            space::vertical().height(0),
        ]
        .into()
    }

    /// What to type, shown before the first search.
    fn help<'a>(&self) -> Element<'a, SearchMsg> {
        let line = |a: &'static str, b: &'static str| row![text(a).size(13).width(180), text(b).size(13).style(text::secondary)].spacing(12);
        column![
            text("Words are all looked for in each verse.").size(14),
            line("love world", "verses with both words"),
            line("\"only begotten\"", "that phrase"),
            line("grace OR mercy", "either word"),
            line("faith -works", "leave a word out"),
            line("beg*", "words that begin so"),
            line("G26", "a Strong's number (H430 for Hebrew)"),
            line("love in:John", "only that book (in:NT, in:Gen-Deut)"),
        ]
        .spacing(6)
        .into()
    }

    /// The books the verses are in, with how many in each: a click lists just that book.
    fn books<'a>(&'a self, ran: &'a Ran) -> Element<'a, SearchMsg> {
        let mut counts: BTreeMap<usize, (Book, usize)> = BTreeMap::new();
        for found in &ran.results.found {
            let at = Book::all().position(|b| b == found.range.book).unwrap_or(0);
            counts.entry(at).or_insert((found.range.book, 0)).1 += 1;
        }
        // The name, then how many verses in it: a smaller, fainter number set apart, so that it
        // does not read as a chapter.
        let chip = |label: String, n: usize, on: bool, msg: SearchMsg| {
            let count = text(format!("\u{00D7}{n}")).size(11).style(|theme: &iced::Theme| text::Style {
                color: Some(iced::Color { a: 0.6, ..theme.extended_palette().background.base.text }),
            });
            let count = container(count).padding([0, 4]);
            button(row![text(label).size(12), count].spacing(4).align_y(iced::Center))
                .padding([1, 8])
                .style(if on { button::primary } else { button::secondary })
                .on_press(msg)
        };
        let mut chips = row![chip("All".into(), ran.results.found.len(), self.book.is_none(), SearchMsg::Book(None))].spacing(6);
        for (book, n) in counts.into_values() {
            chips = chips.push(chip(BookChoice(book).to_string(), n, self.book == Some(book), SearchMsg::Book(Some(book))));
        }
        chips.wrap().vertical_spacing(6).into()
    }

    /// One verse: where it is, and its words with the matches marked.
    fn row_of<'a>(&'a self, sh: &'a Shared, ran: &'a Ran, found: &'a Found) -> Element<'a, SearchMsg> {
        use iced::widget::{rich_text, span};
        let size = sh.settings.notes_size * 0.9;
        let base = sh.settings.text_font();
        let dark = sh.is_dark();
        let marked = Color::from_rgba(1.0, 0.82, 0.2, if dark { 0.5 } else { 0.6 });
        let bold = Font { weight: font::Weight::Bold, ..base };
        let mut spans: Vec<iced::widget::text::Span<'a, ()>> = vec![span(format!("{}   ", found.range)).font(bold).color(crate::palette::Palette::for_theme(&sh.theme()).link)];
        let mut at = 0;
        for &(start, end) in &found.spans {
            if start < at || end > found.text.len() || !found.text.is_char_boundary(start) || !found.text.is_char_boundary(end) {
                continue;
            }
            if start > at {
                spans.push(span(&found.text[at..start]).font(base));
            }
            spans.push(span(&found.text[start..end]).font(bold).background(marked).padding(1));
            at = end;
        }
        if at < found.text.len() {
            spans.push(span(&found.text[at..]).font(base));
        }
        let open = SearchMsg::Open(found.range, ran.module.clone(), found.spans.clone());
        button(rich_text(spans).size(size))
            .width(Length::Fill)
            .padding([6, 8])
            .style(|theme: &iced::Theme, status| {
                let p = theme.extended_palette();
                button::Style {
                    background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| p.background.weak.color.into()),
                    text_color: p.background.base.text,
                    border: iced::Border { radius: 6.0.into(), ..iced::Border::default() },
                    ..button::Style::default()
                }
            })
            .on_press(open)
            .into()
    }
}
