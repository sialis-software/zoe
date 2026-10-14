//! The plan tab: the plans (see docs/08 and docs/09), each a
//! note a day, read here and not written in. It shows how far each has got, one day at a time with
//! the means to go to the day before or after, or to today. A plan has no pace: days are done or not. Importing,
//! starting and removing plans is done in the plans window (`plans_window`).

use crate::live_editor::live_editor;
use crate::palette::Palette;
use crate::plans_window::{DayRef, Entry, PlansMsg, day_label, day_ref, plural};
use crate::shared::{Shared, TabId};
use chrono::{Datelike, NaiveDate};
use iced::widget::{button, column, container, progress_bar, row, rule, scrollable, space, text};
use iced::{Element, Length, Theme};
use md_editor::{Action, Document};
use scripture::RefRange;
use study_core::markdown::LinkTarget;
use study_core::plan::{Day, DayKey, Kind, Plan};

#[derive(Debug, Clone)]
pub enum PlanMsg {
    /// Back to the list of plans.
    Back,
    /// Show this plan (by its key) at this day, or at its overview.
    Select(String, Option<usize>),
    Prev,
    Next,
    Today,
    /// Selecting and moving in the text (what would change it is ignored).
    Action(Action),
    /// A link in the text was followed.
    Link(LinkTarget),
    Scrolled(f32),
    /// Something to do to the plans themselves: import, start, mark done, remove.
    App(PlansMsg),
}

pub struct PlanTab {
    pub id: TabId,
    /// The plan being read (its key), or `None` for the list of them.
    pub plan: Option<String>,
    /// Which of its days is shown.
    pub day: Option<usize>,
    /// The day as markdown, shown by the reader's own text widget so that it can be selected and
    /// copied like any text. It is never changed.
    pub doc: Document,
    pub scroll: f32,
}

impl PlanTab {
    pub fn new(id: TabId) -> Self {
        PlanTab { id, plan: None, day: None, doc: Document::new(""), scroll: 0.0 }
    }

    pub fn editor_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("plan-{}", self.id))
    }

    pub fn scroll_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("plan-list-{}", self.id))
    }

    pub fn title(&self, entries: &[Entry]) -> String {
        match self.plan.as_ref().and_then(|k| entries.iter().find(|e| &e.key() == k)) {
            Some(e) => format!("Plan \u{b7} {}", e.plan.name),
            None => "Plans".into(),
        }
    }

    /// Show a plan at a day (or its overview), making the text of the day.
    pub fn show(&mut self, sh: &Shared, entries: &[Entry], plan: Option<String>, day: Option<usize>) {
        self.plan = plan;
        self.day = day;
        self.rebuild(sh, entries);
    }

    fn entry<'a>(&self, entries: &'a [Entry]) -> Option<&'a Entry> {
        self.plan.as_ref().and_then(|k| entries.iter().find(|e| &e.key() == k))
    }

    fn rebuild(&mut self, sh: &Shared, entries: &[Entry]) {
        self.scroll = 0.0;
        let source = match self.entry(entries).zip(self.day).and_then(|(e, i)| e.plan.days.get(i)) {
            Some(day) => day_source(sh, day),
            None => String::new(),
        };
        let mut doc = Document::new(&source);
        // The widget scrolls to the caret when it moves, so put it somewhere other than the top.
        doc.perform(Action::SetCaret { offset: 1.min(source.len()), extend: false });
        self.doc = doc;
    }

    /// What the tab does to itself; links and the plans' own messages are for the workspace.
    pub fn update(&mut self, sh: &Shared, entries: &[Entry], message: PlanMsg) {
        match message {
            PlanMsg::Back => self.show(sh, entries, None, None),
            PlanMsg::Select(key, day) => {
                let day = day.or_else(|| entries.iter().find(|e| e.key() == key).and_then(|e| e.progress.today));
                self.show(sh, entries, Some(key), day.or(Some(0)));
            }
            PlanMsg::Prev | PlanMsg::Next => {
                let Some(e) = self.entry(entries) else { return };
                let at = self.day.unwrap_or(0);
                let to = if matches!(message, PlanMsg::Next) { (at + 1).min(e.plan.days.len().saturating_sub(1)) } else { at.saturating_sub(1) };
                self.day = Some(to);
                        self.rebuild(sh, entries);
            }
            PlanMsg::Today => {
                if let Some(i) = self.entry(entries).and_then(|e| e.progress.today) {
                    self.day = Some(i);
                                self.rebuild(sh, entries);
                }
            }
            // The text is read, not written: only what selects or moves is taken.
            PlanMsg::Action(action) => {
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
            PlanMsg::Scrolled(y) => self.scroll = y,
            PlanMsg::Link(_) | PlanMsg::App(_) => {}
        }
    }

    // ---- view --------------------------------------------------------------------------

    pub fn view<'a>(&'a self, sh: &'a Shared, entries: &'a [Entry], confirm_remove: Option<&'a String>) -> Element<'a, PlanMsg> {
        let today = chrono::Local::now().date_naive();
        let selected = self.entry(entries);
        let mut bar = row![].spacing(8).align_y(iced::Center);
        if selected.is_some() {
            bar = bar.push(button(text("\u{2039} Plans").size(13)).padding([3, 10]).style(button::secondary).on_press(PlanMsg::Back));
        }
        if let Some(e) = selected {
            let arrow = |label: &'static str, on: bool, msg: PlanMsg| {
                let b = button(text(label).size(14)).padding([2, 10]).style(button::secondary);
                if on { b.on_press(msg) } else { b }
            };
            let at = self.day.unwrap_or(0);
            bar = bar
                .push(arrow("\u{2039}", at > 0, PlanMsg::Prev))
                .push(arrow("\u{203a}", at + 1 < e.plan.days.len(), PlanMsg::Next))
                .push(button(text("Today").size(13)).padding([3, 10]).style(button::secondary).on_press_maybe(e.progress.today.map(|_| PlanMsg::Today)));
        }
        let page = column![container(bar).padding([8, 16]), rule::horizontal(1)];
        let body: Element<'a, PlanMsg> = {
            match (selected, self.day) {
                (Some(e), Some(i)) if i < e.plan.days.len() => self.day_view(sh, e, i, today),
                (Some(e), _) => scrollable(container(self.overview(e, today, confirm_remove)).padding(24).width(Length::Fill)).height(Length::Fill).into(),
                (None, _) => scrollable(container(self.plan_list(entries)).padding(24).width(Length::Fill)).id(self.scroll_id()).height(Length::Fill).into(),
            }
        };
        page.push(body).into()
    }

    fn plan_list<'a>(&'a self, entries: &'a [Entry]) -> Element<'a, PlanMsg> {
        if entries.is_empty() {
            return column![
                text("No plans yet").size(15),
                text("Import one (a markdown plan, a CSV of day and readings, or an iCalendar file). See docs/09-reading-plans.md.")
                    .size(12)
                    .style(text::secondary),
            ]
            .spacing(8)
            .into();
        }
        let mut list = column![].spacing(14);
        for p in entries {
            let name = button(text(p.plan.name.clone()).size(16)).padding(0).style(button::text).on_press(PlanMsg::Select(p.key(), None));
            let mut card = column![name].spacing(6);
            if !p.plan.description.trim().is_empty() {
                card = card.push(text(p.plan.description.clone()).size(12).style(text::secondary));
            }
            if !p.plan.is_started() {
                card = card.push(
                    row![
                        text(format!("{} day{}, not started", p.progress.total, plural(p.progress.total))).size(12).style(text::secondary).width(Length::Fill),
                        button(text("Start today").size(12)).padding([3, 10]).on_press(PlanMsg::App(PlansMsg::Start(p.key()))),
                    ]
                    .align_y(iced::Center),
                );
            } else {
                card = card.push(progress_line(p));
                if let Some(i) = p.progress.today {
                    card = card.push(today_line(p, i));
                }
            }
            list = list.push(
                container(card)
                    .padding(12)
                    .width(Length::Fill)
                    .style(|theme: &Theme| {
                        let pal = theme.extended_palette();
                        container::Style {
                            background: Some(pal.background.weak.color.into()),
                            border: iced::Border { color: iced::Color { a: 0.5, ..pal.background.strong.color }, width: 1.0, radius: 8.0.into() },
                            ..container::Style::default()
                        }
                    }),
            );
        }
        list.into()
    }

    /// A plan with no day shown: how far, and its days to pick from.
    fn overview<'a>(&'a self, e: &'a Entry, today: NaiveDate, confirm_remove: Option<&String>) -> Element<'a, PlanMsg> {
        let mut page = column![text(e.plan.name.clone()).size(20)].spacing(10);
        if !e.plan.description.trim().is_empty() {
            page = page.push(text(e.plan.description.clone()).size(12).style(text::secondary));
        }
        page = page.push(self.plan_actions(e, confirm_remove));
        if e.plan.is_started() {
            page = page.push(progress_line(e));
        }
        page = page.push(rule::horizontal(1));
        let mut days = column![].spacing(2);
        for (i, day) in e.plan.days.iter().enumerate() {
            days = days.push(day_line(e, i, day, today));
        }
        page.push(days).into()
    }

    /// Start, Open note, Remove.
    fn plan_actions<'a>(&self, e: &'a Entry, confirm_remove: Option<&String>) -> Element<'a, PlanMsg> {
        let mut actions = row![].spacing(8);
        if e.plan.kind == Kind::Sequential && e.plan.manifest.is_some() {
            let label = if e.plan.is_started() { "Start over today" } else { "Start today" };
            let b = button(text(label).size(12)).padding([3, 10]);
            actions = actions.push(if e.plan.is_started() { b.style(button::secondary) } else { b }.on_press(PlanMsg::App(PlansMsg::Start(e.key()))));
        }
        actions = actions.push(space::horizontal());
        actions = actions.push(crate::plans_window::remove_button(e.key(), confirm_remove == Some(&e.key()), PlanMsg::App));
        actions.into()
    }

    /// One day: where it falls, how the plan stands, and its text.
    fn day_view<'a>(&'a self, sh: &'a Shared, e: &'a Entry, i: usize, today: NaiveDate) -> Element<'a, PlanMsg> {
        let day = &e.plan.days[i];
        let done = e.done.get(i).copied().unwrap_or(false);
        let mut head = column![
            row![
                button(text(e.plan.name.clone()).size(12)).padding(0).style(button::text).on_press(PlanMsg::Select(e.key(), None)),
                text(format!("\u{b7} {}", e.plan.days.len().min(i + 1) as u32)).size(12).style(text::secondary),
                text(format!("of {}", e.plan.days.len())).size(12).style(text::secondary),
            ]
            .spacing(6),
        ]
        .spacing(4);
        let date = date_of(&e.plan, day, today).map(|d| d.format("%A, %-d %B").to_string());
        let mut title = row![text(day_label(day)).size(20)].spacing(12).align_y(iced::Center);
        if let Some(date) = date {
            title = title.push(text(date).size(13).style(text::secondary));
        }
        title = title.push(space::horizontal());
        let mark = if done { ("\u{2713} Done", true) } else { ("Mark as done", false) };
        let target: DayRef = day_ref(&e.plan, day);
        title = title.push(
            button(text(mark.0).size(12))
                .padding([3, 10])
                .style(move |theme: &Theme, status| if mark.1 { button::success(theme, status) } else { button::secondary(theme, status) })
                .on_press(PlanMsg::App(PlansMsg::ToggleDay(target, done))),
        );
        head = head.push(title);
        if e.plan.is_started() {
            head = head.push(progress_line(e));
        } else {
            head = head.push(
                row![
                    text("Not started: the days are counted from the day you start.").size(12).style(text::secondary),
                    button(text("Start today").size(12)).padding([3, 10]).on_press(PlanMsg::App(PlansMsg::Start(e.key()))),
                ]
                .spacing(10)
                .align_y(iced::Center),
            );
        }
        // The readings of the day, each ticked as it is done; the last one ticked completes the day.
        let readings = crate::plans_window::readings_of(day);
        if day.note.is_some() && !readings.is_empty() {
            let mut strip = row![].spacing(14).align_y(iced::Center);
            for (k, range) in readings.iter().enumerate() {
                let on = e.ticks.get(i).and_then(|t| t.get(k)).copied().unwrap_or(false);
                let range = *range;
                let target = day_ref(&e.plan, day);
                let tick = button(text(if on { "\u{2611}" } else { "\u{2610}" }).size(16))
                    .padding([0, 2])
                    .style(move |theme: &Theme, _status| {
                        let pal = theme.extended_palette();
                        button::Style { text_color: if on { pal.success.base.color } else { iced::Color { a: 0.7, ..pal.background.base.text } }, ..button::Style::default() }
                    })
                    .on_press(PlanMsg::App(PlansMsg::ToggleReading(target, range, on)));
                let open = button(text(range.to_string()).size(14))
                    .padding(0)
                    .style(button::text)
                    .on_press(PlanMsg::Link(LinkTarget::Verse { range, module: None }));
                strip = strip.push(row![tick, open].spacing(4).align_y(iced::Center));
            }
            head = head.push(strip.wrap().vertical_spacing(4));
        }
        let reader = live_editor(&self.doc, Palette::for_theme(&sh.theme()))
            .id(self.editor_id())
            .reading(true)
            .copy_as(crate::commentary_tab::plain_text)
            .follow_links(true)
            .on_action(PlanMsg::Action)
            .on_link(PlanMsg::Link)
            .on_scroll(PlanMsg::Scrolled)
            .size(sh.settings.notes_size)
            .font(sh.settings.text_font())
            .padding(24)
            .height(Length::Fill);
        column![container(head).padding([10, 16]), rule::horizontal(1), reader].into()
    }
}

/// A line of a plan's days: where it stands, its name and its chapters.
fn day_line<'a>(e: &'a Entry, i: usize, day: &'a Day, today: NaiveDate) -> Element<'a, PlanMsg> {
    let finished = e.done.get(i).copied().unwrap_or(false);
    let is_today = e.progress.today == Some(i);
    let mark = if finished { "\u{2713}" } else { "\u{25cb}" };
    let state = text(mark).size(13).width(18).style(move |theme: &Theme| {
        let pal = theme.extended_palette();
        text::Style {
            color: Some(if finished {
                pal.success.base.color
            } else {
                iced::Color { a: 0.4, ..pal.background.base.text }
            }),
        }
    });
    let mut seen: Vec<(scripture::Book, u16)> = Vec::new();
    let readings: Vec<String> = day
        .passages
        .iter()
        .filter(|p| {
            let new = !seen.contains(&(p.book, p.chapter));
            seen.push((p.book, p.chapter));
            new
        })
        .map(|p| RefRange::chapter(p.book, p.chapter).to_string())
        .collect();
    let date = date_of(&e.plan, day, today).map(|d| d.format("%a %-d %b").to_string()).unwrap_or_default();
    let mut line = row![
        state,
        text(date).size(12).style(text::secondary).width(84),
        button(text(day_label(day)).size(13)).padding(0).style(button::text).width(200).on_press(PlanMsg::Select(e.key(), Some(i))),
        text(readings.join("; ")).size(13).style(text::secondary),
    ]
    .spacing(8)
    .align_y(iced::Center);
    if is_today {
        line = line.push(space::horizontal()).push(text("today").size(11).style(text::secondary));
    }
    container(line)
        .padding([3, 6])
        .width(Length::Fill)
        .style(move |theme: &Theme| container::Style {
            background: is_today.then(|| theme.extended_palette().background.weak.color.into()),
            border: iced::Border { radius: 4.0.into(), ..iced::Border::default() },
            ..container::Style::default()
        })
        .into()
}

/// A bar of the days done.
fn progress_line<'a>(p: &Entry) -> Element<'a, PlanMsg> {
    let total = p.progress.total.max(1);
    row![
        container(progress_bar(0.0..=total as f32, p.progress.days_done as f32)).width(Length::Fill).height(8),
        text(format!("{} of {} day{}", p.progress.days_done, p.progress.total, plural(p.progress.total))).size(12).style(text::secondary),
        text(if p.complete() { "Complete" } else { "" }).size(12).style(text::success),
    ]
    .spacing(10)
    .align_y(iced::Center)
    .into()
}

/// Today's day in a plan's card: its name, and the chapters to open.
fn today_line<'a>(p: &Entry, index: usize) -> Element<'a, PlanMsg> {
    let day = &p.plan.days[index];
    let done = p.done.get(index).copied().unwrap_or(false);
    let target = day_ref(&p.plan, day);
    let check = button(text(if done { "\u{2713}" } else { "\u{25cb}" }).size(14))
        .padding([0, 4])
        .style(move |theme: &Theme, _status| {
            let pal = theme.extended_palette();
            button::Style { text_color: if done { pal.success.base.color } else { iced::Color { a: 0.6, ..pal.background.base.text } }, ..button::Style::default() }
        })
        .on_press(PlanMsg::App(PlansMsg::ToggleDay(target, done)));
    let open = button(text(format!("Today: {}", day_label(day))).size(13)).padding(0).style(button::text).on_press(PlanMsg::Select(p.key(), Some(index)));
    row![check, open].spacing(6).align_y(iced::Center).into()
}

/// The date a day falls on: a dated day its own, a day of the year this year's, a numbered day the
/// date counted from the plan's start (none until it is started).
pub fn date_of(plan: &Plan, day: &Day, today: NaiveDate) -> Option<NaiveDate> {
    match day.key {
        DayKey::Date(d) => Some(d),
        DayKey::MonthDay(m, d) => NaiveDate::from_ymd_opt(today.year(), m, d),
        DayKey::Number(n) => plan.start.and_then(|s| s.checked_add_signed(chrono::Duration::days(i64::from(n) - 1))),
    }
}

/// What the day shows: its note without the properties, with its chapters as links above it when
/// the note does not have them; a day with no note of its own is only its chapters.
fn day_source(sh: &Shared, day: &Day) -> String {
    let links: Vec<String> = {
        let mut seen: Vec<(scripture::Book, u16)> = Vec::new();
        day.passages
            .iter()
            .filter(|p| {
                let new = !seen.contains(&(p.book, p.chapter));
                seen.push((p.book, p.chapter));
                new
            })
            .map(|p| format!("[[{}]]", RefRange::chapter(p.book, p.chapter)))
            .collect()
    };
    let body = day.note.as_ref().and_then(|n| sh.notes.get(n)).map(|text| {
        let parsed = study_core::frontmatter::parse(text);
        match parsed.range {
            Some(r) => text[r.end..].trim_start_matches('\n').to_string(),
            None => text.clone(),
        }
    });
    match body {
        // A day with a note shows its readings as ticks above the text, so the text does not list them.
        Some(body) if !links.is_empty() => without_reading_section(&body),
        Some(body) => body,
        None if links.is_empty() => String::new(),
        None => format!("# {}\n\nReading: {}", day_label(day), links.join(", ")),
    }
}

/// `body` without its `## Reading` section (the heading and what follows it up to the next heading).
fn without_reading_section(body: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in body.lines() {
        if line.trim_end() == "## Reading" {
            skipping = true;
            continue;
        }
        if skipping && line.starts_with('#') {
            skipping = false;
        }
        if !skipping {
            out.push(line);
        }
    }
    let mut text = out.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text
}
