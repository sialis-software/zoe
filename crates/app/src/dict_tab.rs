//! The dictionary tab: one entry of a dictionary or lexicon (a Strong's number, a word), with a
//! picker for which dictionary and a box for which entry. Links to dictionaries open here.
//! Self-contained like the other tab types: its own state, messages, update and view.

use crate::fonts;
use crate::history::History;
use crate::shared::{Shared, TabId};
use iced::widget::{button, column, container, pick_list, row, rule, scrollable, space, text, text_input};
use iced::{Element, Length};
use md_editor::{Action, Document};
use std::fmt;
use study_core::{LexiconEntry, ModuleId};

/// A definition as markdown, with the Strong's numbers in it (`From H215;`) as links to their entries, where
/// `defined` says there is an entry for the number.
pub fn linked_definition(text: &str, defined: impl Fn(&str) -> bool) -> String {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = String::with_capacity(text.len());
    let mut plain_from = 0;
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        let starts_word = i == 0 || !chars[i - 1].1.is_alphanumeric();
        if starts_word && (c == 'G' || c == 'H') {
            let digits = chars[i + 1..].iter().take_while(|(_, d)| d.is_ascii_digit()).count();
            let end = i + 1 + digits;
            let ends_word = chars.get(end).is_none_or(|(_, d)| !d.is_alphanumeric());
            if (1..=4).contains(&digits) && ends_word {
                let key = &text[at..chars.get(end).map_or(text.len(), |(e, _)| *e)];
                if defined(key) {
                    out.push_str(&crate::bible_tab::markdown_text(&text[plain_from..at]));
                    out.push_str(&format!("[[%{key}|{key}]]"));
                    plain_from = at + key.len();
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&crate::bible_tab::markdown_text(&text[plain_from..]));
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictChoice(pub ModuleId, pub String);

impl fmt::Display for DictChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

#[derive(Debug, Clone)]
pub enum DictMsg {
    PickModule(DictChoice),
    /// What is typed in the entry box.
    Draft(String),
    /// Look up what is typed.
    Go,
    Back,
    /// Say the entry's word aloud.
    Speak(String),
    /// The entry was scrolled this far.
    Scrolled(f32),
    Forward,
    /// Show a verse that has the word (the app decides where, as for any link).
    OpenVerse(scripture::RefRange, ModuleId, Vec<(usize, usize)>),
    /// Something done in a piece of the entry's text (by its name): only what selects or moves is taken.
    Text(String, Action),
    /// A link in the entry's text (to another entry): the app decides where it opens, as for any link.
    Link(study_core::markdown::LinkTarget),
}

pub struct DictTab {
    pub id: TabId,
    pub module: ModuleId,
    /// The entry being shown (as asked for), and what the box says.
    pub key: String,
    pub draft: String,
    /// What the dictionary says for `key`, if it has an entry.
    pub entry: Option<LexiconEntry>,
    /// How far down the entry is scrolled, to put back when the tab is shown again.
    pub scroll: f32,
    /// The entries this tab has shown, as dictionary and key.
    pub history: History<(ModuleId, String)>,
    /// The pieces of the entry as they are selected, by name, with the markdown each was made from.
    texts: std::cell::RefCell<std::collections::HashMap<String, (String, Document)>>,
}

impl DictTab {
    pub fn new(id: TabId, sh: &Shared, module: ModuleId, key: &str) -> Self {
        let mut tab = DictTab { id, history: History::new((module.clone(), String::new())), module, key: String::new(), draft: String::new(), entry: None, scroll: 0.0, texts: Default::default() };
        tab.show(sh, key);
        tab.history = History::new((tab.module.clone(), tab.key.clone()));
        tab
    }

    /// Start the tab's own history over at where it is (a tab made to show a link is not a tab that went there).
    pub fn forget_history(&mut self) {
        self.history = History::new((self.module.clone(), self.key.clone()));
    }

    /// Show `key` of the current dictionary.
    pub fn show(&mut self, sh: &Shared, key: &str) {
        self.look_up(sh, key);
        self.history.go((self.module.clone(), self.key.clone()), |a, b| a == b);
    }

    /// Show `key`, with the history left as it is.
    fn look_up(&mut self, sh: &Shared, key: &str) {
        self.key = key.trim().to_string();
        self.draft = self.key.clone();
        self.entry = (!self.key.is_empty()).then(|| sh.backend.dictionary_entry(&self.module, &self.key)).flatten();
    }

    /// Show `key` of `module`.
    pub fn open(&mut self, sh: &Shared, module: ModuleId, key: &str) {
        self.module = module;
        self.show(sh, key);
    }

    fn module_name(&self, sh: &Shared) -> String {
        sh.backend
            .dictionaries()
            .into_iter()
            .find(|d| d.id == self.module)
            .map_or_else(|| self.module.0.clone(), |d| d.abbreviation)
    }

    pub fn scroll_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("dict-{}", self.id))
    }

    pub fn title(&self, sh: &Shared) -> String {
        let module = self.module_name(sh);
        if self.key.is_empty() { module } else { format!("{} · {module}", self.key) }
    }

    pub fn update(&mut self, sh: &mut Shared, message: DictMsg) {
        match message {
            DictMsg::PickModule(DictChoice(module, _)) => {
                let key = self.draft.clone();
                self.open(sh, module, &key);
            }
            DictMsg::Draft(draft) => self.draft = draft,
            DictMsg::Scrolled(y) => self.scroll = y,
            DictMsg::Speak(word) => {
                if let Err(why) = crate::speech::speak(&word) {
                    sh.toast(why);
                }
            }
            DictMsg::Back | DictMsg::Forward => {
                let place = if matches!(message, DictMsg::Back) { self.history.back() } else { self.history.forward() }.cloned();
                if let Some((module, key)) = place {
                    self.module = module;
                    self.look_up(sh, &key);
                }
            }
            // Opening a verse is for the workspace to do.
            DictMsg::OpenVerse(..) => {}
            // Following a link is for the workspace to do.
            DictMsg::Link(_) => {}
            // The text is read, not written: only what selects or moves is taken.
            DictMsg::Text(name, action) => {
                if matches!(
                    action,
                    Action::Move { .. } | Action::SetCaret { .. } | Action::Select(_) | Action::SelectBetween { .. } | Action::SelectAll | Action::SelectWord(_) | Action::SelectLine(_)
                ) && let Some((_, doc)) = self.texts.borrow_mut().get_mut(&name)
                {
                    doc.perform(action);
                }
            }
            DictMsg::Go => {
                let key = self.draft.clone();
                self.show(sh, &key);
            }
        }
    }

    /// `searched`: the Bible whose verses a Strong's entry lists, if there is one with Strong's numbers.
    /// Every verse of `module` that has a word with this Strong's number, as links, by book.
    fn occurrences<'a>(&self, sh: &Shared, module: &ModuleId, key: &str) -> Element<'a, DictMsg> {
        let verses = sh.strongs_occurrences(module, key);
        if verses.is_empty() {
            return space::vertical().height(0).into();
        }
        let name = sh.module_abbreviation(module);
        let mut out = column![
            rule::horizontal(1),
            text(format!("{} verse{} in {name}", verses.len(), if verses.len() == 1 { "" } else { "s" })).size(14).style(text::secondary),
        ]
        .spacing(10);
        let mut at = 0;
        while at < verses.len() {
            let book = verses[at].range.book;
            let end = at + verses[at..].iter().take_while(|v| v.range.book == book).count();
            let mut links = row![].spacing(10);
            for found in &verses[at..end] {
                let (chapter, verse) = (found.range.chapter, found.range.verses.map_or(0, |v| v.0));
                links = links.push(
                    button(text(format!("{chapter}:{verse}")).size(14))
                        .padding(0)
                        .style(|theme: &iced::Theme, status| {
                            let link = crate::palette::Palette::for_theme(theme).link;
                            let shown = if matches!(status, button::Status::Hovered) { iced::Color { a: 0.8, ..link } } else { link };
                            button::Style { text_color: shown, ..button::Style::default() }
                        })
                        .on_press(DictMsg::OpenVerse(found.range, module.clone(), found.words.clone())),
                );
            }
            out = out.push(column![text(book.name()).size(13).style(text::secondary), links.wrap().vertical_spacing(4)].spacing(3));
            at = end;
        }
        out.into()
    }

    /// The markdown a piece of the entry's text is made from, and whether any of it is selected (for the tests to
    /// look in: the text is drawn by the editor widget, which a search of the screen for words does not look inside).
    #[cfg(test)]
    pub fn piece(&self, name: &str) -> Option<(String, bool)> {
        self.texts.borrow().get(name).map(|(markdown, doc)| (markdown.clone(), doc.selection().anchor != doc.selection().head))
    }

    /// A piece of the entry's text that can be selected and copied: `name` says which, so that its selection
    /// is kept as the tab is drawn again.
    fn selectable<'a>(&self, sh: &Shared, name: &str, markdown: String, size: f32, quiet: bool) -> Element<'a, DictMsg> {
        let doc = {
            let mut texts = self.texts.borrow_mut();
            let kept = texts.entry(name.to_string()).or_insert_with(|| (markdown.clone(), Document::new(&markdown)));
            if kept.0 != markdown {
                *kept = (markdown.clone(), Document::new(&markdown));
            }
            kept.1.clone()
        };
        let name = name.to_string();
        let editor: Element<'a, DictMsg> = crate::live_editor::live_reader(doc, crate::palette::Palette::for_theme(&sh.theme()))
            .reading(true)
            .copy_as(crate::commentary_tab::plain_text)
            .fit_width()
            .height(Length::Shrink)
            .size(size)
            .font(fonts::UI)
            .padding([2.0, 0.0])
            .on_action(move |action| DictMsg::Text(name.clone(), action))
            .follow_links(true)
            .on_link(DictMsg::Link)
            .into();
        if !quiet {
            return editor;
        }
        container(editor)
            .style(|theme: &iced::Theme| container::Style { text_color: Some(theme.extended_palette().secondary.base.color), ..container::Style::default() })
            .into()
    }

    pub fn view<'a>(&'a self, sh: &'a Shared, searched: Option<&ModuleId>, outer: crate::shared::Outer) -> Element<'a, DictMsg> {
        let choices: Vec<DictChoice> =
            sh.backend.dictionaries().into_iter().map(|d| DictChoice(d.id, d.abbreviation)).collect();
        let current = choices.iter().find(|c| c.0 == self.module).cloned();
        // The same bar as the Bible tab's: a dictionary picker, then the entry wanted.
        let arrow = |icon: crate::icons::Icon, on: bool, msg: DictMsg| icon.arrow_button(on, msg);
        let bar = row![
            row![
                arrow(crate::icons::Icon::Back, self.history.can_back() || outer.back, DictMsg::Back),
                arrow(crate::icons::Icon::Forward, self.history.can_forward() || outer.forward, DictMsg::Forward),
            ]
            .spacing(4),
            pick_list(choices, current, DictMsg::PickModule),
            container(text_input("Look up…", &self.draft).on_input(DictMsg::Draft).on_submit(DictMsg::Go).size(16).padding([4, 10]))
                .max_width(220)
                .width(Length::Fill),
            button(container(text("Go").size(14)).center_y(Length::Fill)).height(31).padding([0, 14]).style(button::secondary).on_press(DictMsg::Go),
        ]
        .spacing(16)
        .align_y(iced::Center);

        let body: Element<'_, DictMsg> = match (&self.entry, self.key.is_empty()) {
            (Some(e), _) => {
                // Centered, so that a long transliteration wrapping onto two lines sits level with the word and
                // its speaker button rather than hanging above them.
                let mut head = row![].spacing(12).align_y(iced::Alignment::Center);
                if !e.lemma.is_empty() {
                    head = head.push(self.selectable(sh, "lemma", crate::bible_tab::markdown_text(&e.lemma), 34.0, false));
                    if crate::speech::can_say(&e.lemma) {
                        head = head.push(
                            button(crate::icons::Icon::Speaker.view(22.0)).padding([2, 6]).style(button::text).on_press(DictMsg::Speak(e.lemma.clone())),
                        );
                    }
                }
                if !e.transliteration.is_empty() {
                    head = head.push(self.selectable(sh, "transliteration", format!("*{}*", crate::bible_tab::markdown_text(&e.transliteration)), 18.0, true));
                }
                let mut entry = column![
                    self.selectable(sh, "key", crate::bible_tab::markdown_text(&e.key), 12.0, true),
                    head,
                    self.selectable(sh, "definition", linked_definition(&e.definition, |key| sh.backend.strongs_dictionary(key).is_some()), 17.0, false),
                ]
                .spacing(10);
                if let Some(module) = searched {
                    entry = entry.push(self.occurrences(sh, module, &e.key));
                }
                entry.into()
            }
            (None, true) => text("Type an entry to look up.").size(14).style(text::secondary).into(),
            (None, false) => text(format!("{} has no entry for \u{201C}{}\u{201D}.", self.module_name(sh), self.key))
                .size(15)
                .style(text::secondary)
                .into(),
        };
        column![
            container(bar).padding([8, 16]),
            rule::horizontal(1),
            scrollable(container(body).padding(24).width(Length::Fill))
                .id(self.scroll_id())
                .on_scroll(|v| DictMsg::Scrolled(v.absolute_offset().y))
                .height(Length::Fill),
            space::vertical().height(0),
        ]
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strongs_numbers_in_a_definition_link_to_their_entries_where_there_are_any() {
        let defined = |key: &str| key != "H9999";
        assert_eq!(
            linked_definition("From H215; a {luminary} (see G25, not H9999 or xH5 or H12345).", defined),
            "From [[%H215|H215]]; a {luminary} (see [[%G25|G25]], not H9999 or xH5 or H12345)."
        );
        assert_eq!(linked_definition("H1", defined), "[[%H1|H1]]");
        assert_eq!(linked_definition("No numbers *here*.", defined), "No numbers \\*here\\*.");
        assert_eq!(linked_definition("", defined), "");
    }
}
