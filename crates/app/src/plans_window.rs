//! The plans: how far each has got, and what is done to them (import, start, mark a day done,
//! remove). They are read in the plan tab (`plan_tab`); this is the window, opened from the top bar,
//! where they are imported and managed. Every plan is a folder of notes
//! under `library/`, a note for each day, with optional passages to read and optional text to read (see
//! `study_core::plan` and docs/08, docs/09). They are read-only notes that can be linked to like any.
//!
//! Progress is not kept in the plan: it is the completion log (`Plan log.md`, a link to the day's note
//! under the date; a ticked reading is a link to its passage, from the day's note). A day is done when it is there on or after the plan's baseline. There is no pace:
//! nothing is behind. Marking a day done from here records it there and also logs its chapters in the
//! reading log.

use crate::{App, Message, native};
use chrono::NaiveDate;
use iced::widget::{button, column, container, progress_bar, row, rule, scrollable, space, text};
use iced::{Element, Length, Task, Theme};
use scripture::RefRange;
use std::collections::{BTreeMap, HashMap};
use study_core::frontmatter::{self, Property, Value};
use study_core::plan::{Day, Plan, PlanProgress, find_plans};

#[derive(Clone, Debug)]
pub struct Imported {
    pub name: String,
    pub extension: String,
    pub text: String,
}

/// A day, as much as is needed to mark it done.
#[derive(Clone, Debug)]
pub struct DayRef {
    /// The day's note, if it has one.
    pub note: Option<String>,
    pub label: String,
    pub passages: Vec<RefRange>,
}

#[derive(Clone, Debug)]
pub enum PlansMsg {
    /// Show the window (or bring it forward).
    Open,
    /// Read this plan (its key) in the plan tab.
    Show(String),
    /// Ask for a file to import.
    Import,
    Imported(Option<Imported>),
    /// Begin a sequential plan today.
    Start(String),
    /// Mark a day done today, or take that back.
    /// Mark a day done (or, if it is done, take that back): the bool is whether it is done now.
    ToggleDay(DayRef, bool),
    /// Tick a reading of a day (or untick it): the bool is whether it is ticked now.
    ToggleReading(DayRef, RefRange, bool),
    AskRemove(String),
    Remove,
    No,
}

#[derive(Default)]
pub struct Plans {
    pub confirm_remove: Option<String>,
}

/// A plan, and how far it has got.
pub struct Entry {
    pub plan: Plan,
    pub progress: PlanProgress,
    /// Which days are done, in the order of the plan's days.
    pub done: Vec<bool>,
    /// For each day, which of its readings (`readings_of`) are ticked.
    pub ticks: Vec<Vec<bool>>,
}

impl Entry {
    /// What identifies the plan: where its note or folder is.
    pub fn key(&self) -> String {
        plan_key(&self.plan)
    }

    pub fn complete(&self) -> bool {
        self.progress.total > 0 && self.progress.days_done == self.progress.total
    }
}

pub fn plan_key(plan: &Plan) -> String {
    plan.manifest.clone().or_else(|| plan.folder.clone()).unwrap_or_else(|| plan.name.clone())
}

/// How far `plan` has got on `today`, given the days completed.
pub fn entry(plan: Plan, today: NaiveDate, completed: &HashMap<String, String>, ticked: &HashMap<String, HashMap<String, String>>) -> Entry {
    let progress = plan.progress(today, completed);
    let done: Vec<bool> = plan.days.iter().map(|d| plan.day_done(d, today, completed)).collect();
    let from = plan.baseline(today).map(|b| b.format("%Y-%m-%d").to_string());
    // A reading is ticked when it was since the baseline, and all of a day's are when the day is done.
    let ticks = plan
        .days
        .iter()
        .zip(&done)
        .map(|(day, done)| {
            readings_of(day)
                .iter()
                .map(|r| {
                    *done
                        || day.note.as_ref().and_then(|n| ticked.get(n)).and_then(|m| m.get(&r.to_string())).is_some_and(|date| from.as_ref().is_some_and(|f| date >= f))
                })
                .collect()
        })
        .collect();
    Entry { plan, progress, done, ticks }
}

/// What a day asks to be read, a chapter at a time (a verse range counts as its chapter).
pub fn readings_of(day: &Day) -> Vec<RefRange> {
    let mut out: Vec<RefRange> = Vec::new();
    for p in &day.passages {
        let chapter = RefRange::chapter(p.book, p.chapter);
        if !out.contains(&chapter) {
            out.push(chapter);
        }
    }
    out
}

fn pm(message: PlansMsg) -> Message {
    Message::Plans(message)
}

/// The Remove button of a plan. Pressed once it asks, in its own place, and the second press removes
/// the plan's notes from the vault (with Cancel beside it to go back).
pub fn remove_button<M: Clone + 'static>(key: String, asking: bool, wrap: impl Fn(PlansMsg) -> M) -> Element<'static, M> {
    if asking {
        row![
            button(text("Cancel").size(12)).padding([3, 10]).style(button::text).on_press(wrap(PlansMsg::No)),
            button(text("Delete its notes?").size(12)).padding([3, 10]).style(button::danger).on_press(wrap(PlansMsg::Remove)),
        ]
        .spacing(6)
        .align_y(iced::Center)
        .into()
    } else {
        button(text("Remove").size(12)).padding([3, 10]).style(button::secondary).on_press(wrap(PlansMsg::AskRemove(key))).into()
    }
}

pub fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

pub fn day_label(day: &Day) -> String {
    day.title.as_deref().map_or_else(|| day.key.to_string(), |t| format!("{} \u{b7} {t}", day.key))
}

pub fn day_ref(plan: &Plan, day: &Day) -> DayRef {
    DayRef { note: day.note.clone(), label: format!("{} \u{b7} {}", plan.name, day_label(day)), passages: day.passages.clone() }
}

impl App {
    /// The plans (those imported and those written as one note), by name, with how far each has got.
    pub fn plan_entries(&self) -> Vec<Entry> {
        entries_of(&self.sh)
    }
}

/// The plans in the vault, by name, with how far each has got.
pub fn entries_of(sh: &crate::shared::Shared) -> Vec<Entry> {
    {
        let today = chrono::Local::now().date_naive();
        let (completed, ticked) = (sh.completed_days(), sh.ticked_readings());
        // What could be part of a plan: the library, and notes that say they are a plan.
        let notes: BTreeMap<String, String> = sh
            .notes
            .iter()
            .filter(|(path, text)| crate::shared::is_library(path) || (!study_core::templates::is_template(path) && study_core::plan::Plan::is_plan(text)))
            .map(|(path, text)| (path.clone(), text.clone()))
            .collect();
        find_plans(&notes).into_iter().map(|plan| entry(plan, today, &completed, &ticked)).collect()
    }
}

/// The chapters a day's passages are in.
fn chapters_of(day: &DayRef) -> Vec<RefRange> {
    let mut out: Vec<RefRange> = Vec::new();
    for p in &day.passages {
        let chapter = RefRange::chapter(p.book, p.chapter);
        if !out.contains(&chapter) {
            out.push(chapter);
        }
    }
    out
}

impl App {
    pub fn plans_message(&mut self, message: PlansMsg) -> Task<Message> {
        match message {
            PlansMsg::Open => {
                if let Some(id) = self.plans_window {
                    return iced::window::gain_focus(id);
                }
                // A fixed-size accessory of the workspace window, like the settings (`native::attach`).
                let (id, open) = iced::window::open(iced::window::Settings {
                    size: iced::Size::new(620.0, 660.0),
                    resizable: false,
                    minimizable: false,
                    exit_on_close_request: false,
                    ..iced::window::Settings::default()
                });
                self.plans_window = Some(id);
                let main = self.main;
                open.then(move |_| native::attach(main, id)).discard()
            }
            PlansMsg::Show(key) => {
                self.show_plans(Some((key, None)));
                iced::window::gain_focus(self.main)
            }
            PlansMsg::Import => Task::perform(
                async {
                    let file = rfd::AsyncFileDialog::new()
                        .set_title("Import a plan")
                        .add_filter("Plans", &["md", "csv", "ics", "txt"])
                        .pick_file()
                        .await?;
                    let path = file.path().to_path_buf();
                    let text = std::fs::read_to_string(&path).ok()?;
                    Some(Imported {
                        name: path.file_stem()?.to_string_lossy().into_owned(),
                        extension: path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default(),
                        text,
                    })
                },
                |found| pm(PlansMsg::Imported(found)),
            ),
            PlansMsg::Imported(None) => Task::none(),
            PlansMsg::Imported(Some(file)) => {
                self.import_plan(file);
                Task::none()
            }
            PlansMsg::Start(key) => {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                if let Some(text) = self.sh.notes.get(&key).cloned() {
                    let edit = frontmatter::update(&text, |f| {
                        f.props.retain(|p| p.key != "start");
                        f.props.push(Property { key: "start".into(), value: Value::Date(today) });
                    });
                    self.sh.notes.insert(key.clone(), frontmatter::apply(&text, &edit));
                    self.sh.events.push(crate::shared::Event::NoteRewritten { old: key.clone(), new: key });
                }
                Task::none()
            }
            PlansMsg::ToggleDay(day, done) => {
                self.toggle_day(&day, done);
                Task::none()
            }
            PlansMsg::ToggleReading(day, range, ticked) => {
                self.toggle_reading(&day, range, ticked);
                Task::none()
            }
            PlansMsg::AskRemove(key) => {
                self.plans.confirm_remove = Some(key);
                Task::none()
            }
            PlansMsg::No => {
                self.plans.confirm_remove = None;
                Task::none()
            }
            PlansMsg::Remove => {
                if let Some(key) = self.plans.confirm_remove.take() {
                    let doomed: Vec<String> = match self.plan_entries().into_iter().find(|e| e.key() == key) {
                        Some(found) => match &found.plan.folder {
                            Some(folder) => self.sh.notes.keys().filter(|p| p.starts_with(&format!("{folder}/"))).cloned().chain(found.plan.manifest.clone()).collect(),
                            None => found.plan.manifest.clone().into_iter().collect(),
                        },
                        None => Vec::new(),
                    };
                    let next = self.sh.first_ordinary_note();
                    for path in doomed {
                        self.sh.notes.remove(&path);
                        self.sh.events.push(crate::shared::Event::NoteRemoved { old: path, next: next.clone() });
                    }
                }
                Task::none()
            }
        }
    }

    /// Mark the whole day done (each reading ticked and read, and the day completed), or take all that
    /// back.
    fn toggle_day(&mut self, day: &DayRef, done: bool) {
        let now = chrono::Local::now();
        let chapters = chapters_of(day);
        if done {
            if let Some(note) = &day.note {
                self.sh.uncomplete_day(note);
                for chapter in &chapters {
                    self.sh.untick_reading(note, &chapter.to_string());
                }
            }
            for c in &chapters {
                self.sh.unlog_reading(c.book, c.chapter, &now);
            }
        } else {
            for c in &chapters {
                if let Some(note) = &day.note {
                    self.sh.tick_reading(note, &c.to_string(), &day.label, &now);
                }
                if !self.sh.read_on(c.book, c.chapter, &now) {
                    let _ = self.sh.log_reading(c.book, c.chapter, &now);
                }
            }
            if let Some(note) = &day.note {
                self.sh.complete_day(note, &day.label, &now);
            }
        }
    }

    /// Tick one reading of a day (it is logged as read), completing the day when it was the last, or
    /// untick it (which also takes back a completion).
    fn toggle_reading(&mut self, day: &DayRef, range: RefRange, ticked: bool) {
        let now = chrono::Local::now();
        let Some(note) = day.note.clone() else { return };
        let reading = range.to_string();
        if ticked {
            self.sh.untick_reading(&note, &reading);
            self.sh.uncomplete_day(&note);
            self.sh.unlog_reading(range.book, range.chapter, &now);
            return;
        }
        self.sh.tick_reading(&note, &reading, &day.label, &now);
        if !self.sh.read_on(range.book, range.chapter, &now) {
            let _ = self.sh.log_reading(range.book, range.chapter, &now);
        }
        // The last reading ticked completes the day.
        let all = {
            let ticked = self.sh.ticked_readings();
            let mine = ticked.get(&note);
            chapters_of(day).iter().all(|c| mine.is_some_and(|m| m.contains_key(&c.to_string())))
        };
        if all {
            self.sh.complete_day(&note, &day.label, &now);
        }
    }

    /// Make the notes of the plan in `file` (markdown, CSV, iCalendar or the experimental SWORD lines),
    /// a folder of them in the library. A plan of the same name is replaced.
    fn import_plan(&mut self, file: Imported) {
        let plan = match file.extension.as_str() {
            "md" => {
                if !Plan::is_plan(&file.text) {
                    self.sh.toast("That note is not a plan (it needs type: reading-plan)");
                    return;
                }
                Plan::parse(&file.text)
            }
            "csv" => study_core::plan::plan_from_csv(&file.name, &file.text),
            "ics" => study_core::plan::plan_from_ics(&file.name, &file.text),
            _ => study_core::plan::plan_from_sword_lines(&file.name, &file.text),
        };
        if plan.days.is_empty() {
            self.sh.toast(format!("No days could be read from {}", file.name));
            return;
        }
        let files = plan.to_files();
        let Some((manifest, _)) = files.first().cloned() else { return };
        for (path, text) in files {
            self.sh.notes.insert(path, text);
        }
        let _ = manifest;
        let skipped = if plan.problems.is_empty() { String::new() } else { format!(", {} lines could not be read", plan.problems.len()) };
        self.sh.toast(format!("Imported {} ({} day{}{skipped})", plan.name, plan.days.len(), plural(plan.days.len())));
    }
}

impl App {
    pub fn plans_panel(&self) -> Element<'static, Message> {
        let plans = self.plan_entries();
        let page = column![
            container(row![text("Plans").size(16), space::horizontal(), button(text("Import\u{2026}").size(13)).padding([3, 12]).style(button::secondary).on_press(pm(PlansMsg::Import))].align_y(iced::Center))
                .padding([12, 16]),
            rule::horizontal(1)
        ];
        let mut body = column![].spacing(12);
        if plans.is_empty() {
            body = body.push(
                text("No plans yet. Import a markdown plan, a CSV of day and readings, or an iCalendar file. See docs/09-reading-plans.md.")
                    .size(12)
                    .style(text::secondary),
            );
        }
        for p in &plans {
            body = body.push(self.plan_card(p));
        }
        page.push(scrollable(container(body).padding(16).width(Length::Fill)).height(Length::Fill)).into()
    }

    fn plan_card(&self, p: &Entry) -> Element<'static, Message> {
        let kind = match p.plan.kind {
            study_core::plan::Kind::Sequential => "by day number",
            study_core::plan::Kind::Dated => "by date",
            study_core::plan::Kind::Calendar => "by day of the year",
        };
        let mut info = format!("{} day{} \u{b7} {kind}", p.progress.total, plural(p.progress.total));
        if p.plan.is_started() {
            info.push_str(&format!(" \u{b7} {} done", p.progress.days_done));
        }
        let mut buttons = row![button(text("Open").size(12)).padding([3, 10]).on_press(pm(PlansMsg::Show(p.key())))].spacing(6);
        if p.plan.kind == study_core::plan::Kind::Sequential && p.plan.manifest.is_some() {
            let label = if p.plan.is_started() { "Start over today" } else { "Start today" };
            let b = button(text(label).size(12)).padding([3, 10]);
            buttons = buttons.push(if p.plan.is_started() { b.style(button::secondary) } else { b }.on_press(pm(PlansMsg::Start(p.key()))));
        }
        buttons = buttons.push(remove_button(p.key(), self.plans.confirm_remove.as_ref() == Some(&p.key()), pm));
        let mut card = column![text(p.plan.name.clone()).size(15), text(info).size(12).style(text::secondary)].spacing(4);
        if p.progress.total > 0 && p.plan.is_started() {
            card = card.push(container(progress_bar(0.0..=p.progress.total as f32, p.progress.days_done as f32)).height(6));
        }
        container(card.push(buttons).spacing(8))
            .padding(12)
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let pal = theme.extended_palette();
                container::Style {
                    background: Some(pal.background.weak.color.into()),
                    border: iced::Border { color: iced::Color { a: 0.5, ..pal.background.strong.color }, width: 1.0, radius: 8.0.into() },
                    ..container::Style::default()
                }
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv() -> Imported {
        Imported { name: "Gospels".into(), extension: "csv".into(), text: "day,readings\n1,Matthew 1-2\n2,Matthew 3\n".into() }
    }

    #[test]
    fn an_imported_plan_is_a_folder_of_read_only_day_notes_and_shows_in_the_list() {
        let mut app = App::new();
        app.import_plan(csv());
        assert!(app.sh.notes.keys().any(|p| p.starts_with("library/Gospels/") && p.ends_with("Gospels.md")));
        let plans = app.plan_entries();
        let p = plans.iter().find(|p| p.plan.name == "Gospels").expect("listed");
        assert_eq!(p.progress.total, 2);
        assert!(!p.plan.is_started());
    }

    #[test]
    fn a_day_marked_done_is_in_the_completion_log_and_taking_it_back_removes_it() {
        let mut app = App::new();
        app.import_plan(csv());
        let _ = app.plans_message(PlansMsg::Start("library/Gospels/Gospels.md".into()));
        let entry = app.plan_entries().into_iter().find(|p| p.plan.name == "Gospels").unwrap();
        assert!(entry.plan.is_started());
        let day = day_ref(&entry.plan, &entry.plan.days[0]);
        let _ = app.plans_message(PlansMsg::ToggleDay(day.clone(), false));
        let entry = app.plan_entries().into_iter().find(|p| p.plan.name == "Gospels").unwrap();
        assert_eq!(entry.done, vec![true, false]);
        assert!(app.sh.notes[crate::shared::PLAN_LOG].contains("[["));
        let _ = app.plans_message(PlansMsg::ToggleDay(day, true));
        let entry = app.plan_entries().into_iter().find(|p| p.plan.name == "Gospels").unwrap();
        assert_eq!(entry.done, vec![false, false]);
    }

    #[test]
    fn removing_a_plan_deletes_its_folder() {
        let mut app = App::new();
        app.import_plan(csv());
        let key = "library/Gospels/Gospels.md".to_string();
        let _ = app.plans_message(PlansMsg::AskRemove(key));
        let _ = app.plans_message(PlansMsg::Remove);
        assert!(app.sh.notes.keys().all(|p| !p.starts_with("library/Gospels")));
    }

    #[test]
    fn ticking_each_reading_of_a_day_completes_it_with_the_last_and_unticking_takes_it_back() {
        let mut app = App::new();
        app.import_plan(Imported { name: "Gospels".into(), extension: "csv".into(), text: "day,readings\n1,Matthew 1-2\n2,Matthew 3\n".into() });
        let key = "library/Gospels/Gospels.md".to_string();
        let _ = app.plans_message(PlansMsg::Start(key));
        let get = |app: &App| app.plan_entries().into_iter().find(|p| p.plan.name == "Gospels").unwrap();
        let e = get(&app);
        let day = day_ref(&e.plan, &e.plan.days[0]);
        let (one, two) = (RefRange::parse("Matthew 1").unwrap(), RefRange::parse("Matthew 2").unwrap());
        assert_eq!(e.ticks[0], vec![false, false]);
        let _ = app.plans_message(PlansMsg::ToggleReading(day.clone(), one, false));
        let e = get(&app);
        assert_eq!((e.ticks[0].clone(), e.done[0], e.progress.today), (vec![true, false], false, Some(0)), "one reading does not complete the day");
        assert!(app.sh.read_on(one.book, one.chapter, &chrono::Local::now()), "it is in the reading log");
        let _ = app.plans_message(PlansMsg::ToggleReading(day.clone(), two, false));
        let e = get(&app);
        assert_eq!((e.ticks[0].clone(), e.done[0], e.progress.today), (vec![true, true], true, Some(1)), "the last one completes it and today moves on");
        let _ = app.plans_message(PlansMsg::ToggleReading(day.clone(), one, true));
        let e = get(&app);
        assert_eq!((e.ticks[0].clone(), e.done[0]), (vec![false, true], false), "unticking takes the completion back and keeps the other tick");
        let _ = app.plans_message(PlansMsg::ToggleDay(day.clone(), false));
        assert!(get(&app).done[0] && get(&app).ticks[0] == vec![true, true], "mark as done ticks them all");
        let _ = app.plans_message(PlansMsg::ToggleDay(day, true));
        let e = get(&app);
        assert_eq!((e.done[0], e.ticks[0].clone()), (false, vec![false, false]));
    }
}
