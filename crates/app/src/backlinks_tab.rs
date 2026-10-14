//! The backlinks tab: every note that links to a passage, with the lines that do, in sections: the margin notes on it,
//! the notes (those with no type, then each type of note: prayer, sermon, daily...) and the plans that read it, each
//! plan once with its days. It follows the notes, so what is written while it is open shows up in it.

use crate::note_tab::label_of;
use crate::shared::{Backlink, Shared, TabId};
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Element, Length};
use scripture::RefRange;

#[derive(Debug, Clone)]
pub enum BacklinksMsg {
    /// Show this note.
    Open(String),
    /// The list was scrolled this far.
    Scrolled(f32),
    /// Fold a section (by its title) or open it.
    ToggleSection(String),
}

/// What the notes found link to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A passage: a link to any of its verses.
    Passage(RefRange),
    /// A note, by its path.
    Note(String),
}

/// What is in a section of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A note, with the lines of it that link.
    Note { path: String, label: String, tags: Vec<String>, lines: Vec<String> },
    /// A plan, with the days of it that read the passage: (path, label) of each.
    Plan { name: String, days: Vec<(String, String)> },
}

/// A kind of note that links, and what links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub entries: Vec<Entry>,
}

impl Section {
    /// How many notes it holds (a plan's days are notes).
    pub fn notes(&self) -> usize {
        self.entries.iter().map(|e| match e {
            Entry::Note { .. } => 1,
            Entry::Plan { days, .. } => days.len(),
        }).sum()
    }
}

/// A plan day's file name as a person would say the day: `Day 087` is `Day 87`, `01-05` is `Jan 5` (a day of the year, every
/// year), `2026-02-18` is `Feb 18, 2026`. Any other name is as it is.
pub fn plan_day_label(stem: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let number = |s: &str| s.parse::<u32>().ok();
    if let Some(n) = stem.strip_prefix("Day ").and_then(|n| number(n.trim())) {
        return format!("Day {n}");
    }
    let parts: Vec<&str> = stem.split('-').collect();
    match parts.as_slice() {
        [m, d] if m.len() == 2 && d.len() == 2 => match (number(m), number(d)) {
            (Some(m @ 1..=12), Some(d @ 1..=31)) => return format!("{} {d}", MONTHS[m as usize - 1]),
            _ => {}
        },
        [y, m, d] if y.len() == 4 && m.len() == 2 && d.len() == 2 => match (number(y), number(m), number(d)) {
            (Some(y), Some(m @ 1..=12), Some(d @ 1..=31)) => return format!("{} {d}, {y}", MONTHS[m as usize - 1]),
            _ => {}
        },
        _ => {}
    }
    stem.to_string()
}

/// `kind` as words to a person: `prayer` is `Prayer`.
fn capitalized(kind: &str) -> String {
    let mut chars = kind.trim().chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// Whether `range` is in the same chapter as `passage` and shares a verse with it (a whole chapter shares with any).
fn on(range: &RefRange, passage: &RefRange) -> bool {
    range.book == passage.book
        && range.chapter == passage.chapter
        && match (range.verses, passage.verses) {
            (Some((a, z)), Some((from, to))) => a <= to && from <= z,
            _ => true,
        }
}

/// The backlinks in sections, in the order they are shown: the margin notes (the marks on the passage), the notes with no
/// type, each type of note by name, then the plans (each with the days that link). Each section keeps the order of the
/// notes' paths.
pub fn sections(sh: &Shared, links: &[Backlink], passage: Option<&RefRange>) -> Vec<Section> {
    // (order, title) of a section, and its entries.
    let mut out: Vec<(u8, String, Vec<Entry>)> = Vec::new();
    fn put(out: &mut Vec<(u8, String, Vec<Entry>)>, order: u8, title: String, entry: Entry) {
        match out.iter_mut().find(|(o, t, _)| *o == order && *t == title) {
            Some((_, _, entries)) => entries.push(entry),
            None => out.push((order, title, vec![entry])),
        }
    }
    // The margin notes on the passage are on it by where they are, not by a link in their words: those with something
    // written in them, with the words they are on and the start of what is written.
    if let Some(passage) = passage {
        for mark in sh.marks.iter().filter(|m| !m.body.trim().is_empty() && on(&m.anchor, passage)) {
            let path = mark.path();
            if links.iter().any(|l| l.path == path) {
                continue;
            }
            let mut lines = Vec::new();
            if !mark.quote.is_empty() {
                lines.push(format!("\u{201C}{}\u{201D}", mark.quote));
            }
            lines.extend(mark.body.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string));
            put(&mut out, 0, "Margin notes".into(), Entry::Note { label: label_of(sh, &path), path, tags: mark.tags.clone(), lines });
        }
    }
    for link in links {
        let label = label_of(sh, &link.path);
        if link.path.starts_with("margins/") {
            put(&mut out, 0, "Margin notes".into(), Entry::Note { path: link.path.clone(), label, tags: sh.tags_of(&link.path), lines: link.lines.clone() });
        } else if let Some(rest) = link.path.strip_prefix("library/") {
            // `library/<plan>/<day>.md`: the plan has one entry, with each of its days.
            let name = rest.split('/').next().unwrap_or(rest).to_string();
            let day = (link.path.clone(), plan_day_label(&label));
            let known = out
                .iter_mut()
                .filter(|(o, _, _)| *o == 3)
                .flat_map(|(_, _, entries)| entries.iter_mut())
                .find_map(|e| match e {
                    Entry::Plan { name: n, days } if *n == name => Some(days),
                    _ => None,
                });
            match known {
                Some(days) => days.push(day),
                None => put(&mut out, 3, "Plans".into(), Entry::Plan { name, days: vec![day] }),
            }
        } else {
            let kind = sh.notes.get(&link.path).and_then(|text| study_core::frontmatter::parse(text).front.kind).filter(|k| !k.trim().is_empty());
            let entry = Entry::Note { path: link.path.clone(), label, tags: sh.tags_of(&link.path), lines: link.lines.clone() };
            match kind {
                None => put(&mut out, 1, "Notes".into(), entry),
                Some(kind) => put(&mut out, 2, format!("{} notes", capitalized(&kind)), entry),
            }
        }
    }
    out.sort_by(|a, b| (a.0, a.1.to_lowercase()).cmp(&(b.0, b.1.to_lowercase())));
    out.into_iter().map(|(_, title, entries)| Section { title, entries }).collect()
}

pub struct BacklinksTab {
    pub id: TabId,
    pub target: Target,
    /// How far down the list is scrolled, to put back when the tab is shown again.
    pub scroll: f32,
    /// The sections folded to their header. The plans are: they are a long list, and not the first thing sought.
    pub folded: std::collections::BTreeSet<String>,
}

impl BacklinksTab {
    pub fn new(id: TabId, target: Target) -> Self {
        BacklinksTab { id, target, scroll: 0.0, folded: std::iter::once("Plans".to_string()).collect() }
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
            BacklinksMsg::ToggleSection(title) => {
                if !self.folded.remove(&title) {
                    self.folded.insert(title);
                }
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
        let faint = |t: String| text(t).size(12).style(text::secondary);
        let mut list = column![].spacing(14);
        let passage = match &self.target {
            Target::Passage(range) => Some(range),
            Target::Note(_) => None,
        };
        let found = sections(sh, &links, passage);
        let total: usize = found.iter().map(Section::notes).sum();
        // A section's header is a bar of its own (a button that folds it), over the notes, which are indented under it
        // and are links: what is clicked to open a note is not what folds a section.
        let link = |theme: &iced::Theme, status: button::Status| {
            let color = crate::palette::Palette::for_theme(theme).link;
            let shown = if matches!(status, button::Status::Hovered) { iced::Color { a: 0.8, ..color } } else { color };
            button::Style { text_color: shown, ..button::Style::default() }
        };
        for section in &found {
            let folded = self.folded.contains(&section.title);
            let count = match section.notes() {
                1 => "1 note".to_string(),
                n => format!("{n} notes"),
            };
            let header = button(
                row![
                    crate::icons::Icon::chevron(!folded).view(14.0),
                    text(section.title.clone()).size(15).font(iced::Font { weight: iced::font::Weight::Bold, ..crate::fonts::UI }),
                    iced::widget::space::horizontal(),
                    faint(count),
                ]
                .spacing(8)
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .style(|theme: &iced::Theme, status| {
                let p = theme.extended_palette();
                let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(if lit { p.background.strong.color } else { p.background.weak.color }.into()),
                    text_color: p.background.base.text,
                    border: iced::Border { radius: 6.0.into(), ..iced::Border::default() },
                    ..button::Style::default()
                }
            })
            .on_press(BacklinksMsg::ToggleSection(section.title.clone()));
            let mut block = column![header].spacing(10);
            if !folded {
                let mut entries = column![].spacing(14);
                for entry in &section.entries {
                    entries = entries.push(match entry {
                        Entry::Note { path, label, tags, lines } => {
                            // The name, and its tags beside it.
                            let mut name = row![button(text(label.clone()).size(15)).padding(0).style(link).on_press(BacklinksMsg::Open(path.clone()))]
                                .spacing(10)
                                .align_y(iced::Center);
                            for tag in tags {
                                let color = crate::note_tab::tag_color(sh, tag);
                                name = name.push(text(format!("#{tag}")).size(12).style(move |theme: &iced::Theme| text::Style {
                                    color: Some(color.unwrap_or(theme.extended_palette().background.base.text.scale_alpha(0.6))),
                                }));
                            }
                            let mut entry = column![name].spacing(3);
                            for line in lines.iter().take(4) {
                                let shown: String = if line.chars().count() > 160 { line.chars().take(159).chain(['\u{2026}']).collect() } else { line.clone() };
                                entry = entry.push(text(shown).size(13).style(text::secondary));
                            }
                            if lines.len() > 4 {
                                entry = entry.push(faint(format!("and {} more lines", lines.len() - 4)));
                            }
                            entry
                        }
                        Entry::Plan { name, days } => {
                            let mut found = row![].spacing(6);
                            for (path, label) in days {
                                found = found.push(button(text(label.clone()).size(14)).padding([3, 10]).style(button::secondary).on_press(BacklinksMsg::Open(path.clone())));
                            }
                            column![text(name.clone()).size(15), found.wrap().vertical_spacing(6)].spacing(6)
                        }
                    });
                }
                // Indented under the header, with a line down its side.
                block = block.push(container(row![iced::widget::rule::vertical(2), entries].spacing(14)).padding(iced::Padding { top: 0.0, right: 0.0, bottom: 6.0, left: 10.0 }));
            }
            list = list.push(block);
        }
        if found.is_empty() {
            list = list.push(text(format!("No note links to {name}.")).size(14).style(text::secondary));
        }
        let heading = column![
            text(format!("Backlinks to {name}")).size(20),
            text(match total {
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
