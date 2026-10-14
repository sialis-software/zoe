//! The backlinks tab: every note that links to a passage, with the lines that do. It follows the
//! notes, so what is written while it is open shows up in it.

use crate::note_tab::label_of;
use crate::shared::{Backlink, Shared, TabId};
use iced::widget::{button, column, container, rule, scrollable, text};
use iced::{Element, Length};
use scripture::RefRange;

#[derive(Debug, Clone)]
pub enum BacklinksMsg {
    /// Show this note.
    Open(String),
    /// The list was scrolled this far.
    Scrolled(f32),
}

/// What the notes found link to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A passage: a link to any of its verses.
    Passage(RefRange),
    /// A note, by its path.
    Note(String),
}

pub struct BacklinksTab {
    pub id: TabId,
    pub target: Target,
    /// How far down the list is scrolled, to put back when the tab is shown again.
    pub scroll: f32,
}

impl BacklinksTab {
    pub fn new(id: TabId, target: Target) -> Self {
        BacklinksTab { id, target, scroll: 0.0 }
    }

    pub fn scroll_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("backlinks-{}", self.id))
    }

    /// What is linked to, as a person would say it.
    pub fn name(&self, sh: &Shared) -> String {
        match &self.target {
            Target::Passage(range) => range.to_string(),
            Target::Note(path) => label_of(sh, path),
        }
    }

    pub fn title(&self, sh: &Shared) -> String {
        format!("Backlinks \u{b7} {}", self.name(sh))
    }

    /// The note to show, if one was picked.
    pub fn update(&mut self, message: BacklinksMsg) -> Option<String> {
        match message {
            BacklinksMsg::Open(path) => Some(path),
            BacklinksMsg::Scrolled(y) => {
                self.scroll = y;
                None
            }
        }
    }

    pub fn view<'a>(&'a self, sh: &'a Shared) -> Element<'a, BacklinksMsg> {
        let links: Vec<Backlink> = match &self.target {
            Target::Passage(range) => sh.backlinks(range),
            Target::Note(path) => sh.note_backlinks(path),
        };
        let name = self.name(sh);
        let mut list = column![].spacing(16);
        for link in links.iter() {
            let mut entry = column![
                button(text(label_of(sh, &link.path)).size(16))
                    .padding(0)
                    .style(button::text)
                    .on_press(BacklinksMsg::Open(link.path.clone()))
            ]
            .spacing(3);
            for line in link.lines.iter().take(4) {
                let shown: String = if line.chars().count() > 160 { line.chars().take(159).chain(['\u{2026}']).collect() } else { line.clone() };
                entry = entry.push(text(shown).size(13).style(text::secondary));
            }
            if link.lines.len() > 4 {
                entry = entry.push(text(format!("and {} more lines", link.lines.len() - 4)).size(12).style(text::secondary));
            }
            list = list.push(entry);
        }
        if links.is_empty() {
            list = list.push(text(format!("No note links to {name}.")).size(14).style(text::secondary));
        }
        let heading = column![
            text(format!("Backlinks to {name}")).size(20),
            text(match links.len() {
                0 => String::new(),
                1 => "In 1 note".into(),
                n => format!("In {n} notes"),
            })
            .size(12)
            .style(text::secondary),
        ]
        .spacing(2);
        column![
            container(heading).padding([10, 16]),
            rule::horizontal(1),
            scrollable(container(list).padding(24).width(Length::Fill))
                .id(self.scroll_id())
                .on_scroll(|v| BacklinksMsg::Scrolled(v.absolute_offset().y))
                .height(Length::Fill),
        ]
        .into()
    }
}
