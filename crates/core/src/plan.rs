//! Reading plans: one markdown standard, with importers from other formats (docs/09).
//!
//! A plan is an ordinary note. Each level 2 heading is a day, each bullet under it holds readings,
//! written as links or as plain text.
//!
//! ```text
//! ---
//! type: reading-plan
//! name: Gospels in 30 days
//! start: 2026-01-01
//! tags: [reading-plan]
//! ---
//! # Gospels in 30 days
//!
//! ## Day 1
//!
//! - [[Matthew 1]]
//! - [[Matthew 2]]
//!
//! ## Day 2
//!
//! - Matthew 3-4; Psalm 1
//! ```
//!
//! A day's heading is `Day N` (sequential), `2026-01-01` (dated) or `January 1` / `Jan 1` / `01-01`
//! (a day of the year, repeating). Chapter ranges are expanded to one passage per chapter, and a
//! verse range across chapters is split by chapter, so that progress can be counted in chapters.

use chrono::{Datelike, Days, NaiveDate};
use scripture::{Book, RefRange};

/// The verse a range that runs to the end of its chapter stops at. The number of verses in a
/// chapter is not known here; no chapter has this many.
pub const OPEN_END: u16 = 999;

/// The number of chapters of each book, in canon order.
const CHAPTERS: [u16; 66] = [
    50, 40, 27, 36, 34, 24, 21, 4, 31, 24, 22, 25, 29, 36, 10, 13, 10, 42, 150, 31, 12, 8, 66, 52, 5, 48, 12, 14,
    3, 9, 1, 4, 7, 3, 3, 3, 2, 14, 4, 28, 16, 24, 21, 28, 16, 16, 13, 6, 6, 4, 4, 5, 3, 6, 4, 3, 1, 13, 5, 5, 3,
    5, 1, 1, 1, 22,
];

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// How many chapters `book` has.
pub fn chapter_count(book: Book) -> u16 {
    CHAPTERS[Book::all().position(|b| b == book).unwrap_or(0)]
}

/// Which day of a plan a heading names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DayKey {
    /// The nth day since the plan was started.
    Number(u32),
    /// One date.
    Date(NaiveDate),
    /// A day of the year (month, day), every year.
    MonthDay(u32, u32),
}

impl std::fmt::Display for DayKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DayKey::Number(n) => write!(f, "Day {n}"),
            DayKey::Date(d) => write!(f, "{}", d.format("%Y-%m-%d")),
            DayKey::MonthDay(m, d) => write!(f, "{} {d}", MONTHS[(*m as usize).clamp(1, 12) - 1]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    pub key: DayKey,
    /// What follows the key in the heading (`## Day 3 — Creation`).
    pub title: Option<String>,
    /// The chapters (and verse ranges) of the day, in the order written.
    pub passages: Vec<RefRange>,
}

impl Day {
    /// (chapters read, chapters in the day). A verse range counts as its chapter, and a chapter that
    /// is in the day twice counts once.
    pub fn progress(&self, read: impl Fn(&RefRange) -> bool) -> (usize, usize) {
        let mut seen: Vec<(Book, u16)> = Vec::new();
        let mut done = 0;
        for p in &self.passages {
            if seen.contains(&(p.book, p.chapter)) {
                continue;
            }
            seen.push((p.book, p.chapter));
            done += read(&RefRange::chapter(p.book, p.chapter)) as usize;
        }
        (done, seen.len())
    }

    /// Whether the day has come by `date`, given the plan's start.
    fn is_due(&self, date: NaiveDate, start: Option<NaiveDate>) -> bool {
        match self.key {
            DayKey::Number(n) => start.is_some_and(|s| (date - s).num_days() + 1 >= i64::from(n)),
            DayKey::Date(d) => d <= date,
            DayKey::MonthDay(m, d) => (m, d) <= (date.month(), date.day()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub name: String,
    /// The date day 1 falls on, set when a sequential plan is started.
    pub start: Option<NaiveDate>,
    pub days: Vec<Day>,
    /// What could not be read, each with the line (or row) it was on.
    pub problems: Vec<String>,
}

impl Plan {
    /// Whether the note's properties say it is a reading plan (`type: reading-plan`).
    pub fn is_plan(text: &str) -> bool {
        let lines: Vec<&str> = text.lines().collect();
        let (front, _) = split_front(&lines);
        front.iter().filter_map(|l| front_value(l)).any(|(k, v)| k == "type" && v.eq_ignore_ascii_case("reading-plan"))
    }

    /// Read a plan from a note. Lenient: what cannot be read is left out and named in `problems`.
    pub fn parse(text: &str) -> Plan {
        let lines: Vec<&str> = text.lines().collect();
        let (front, first_body) = split_front(&lines);
        let mut plan = Plan { name: String::new(), start: None, days: Vec::new(), problems: Vec::new() };
        for (k, v) in front.iter().filter_map(|l| front_value(l)) {
            match k {
                "name" => plan.name = v,
                "start" => match parse_date(&v) {
                    Some(d) => plan.start = Some(d),
                    None => plan.problems.push(format!("start: could not read the date {v:?}")),
                },
                _ => {}
            }
        }
        let mut current = false;
        for (i, line) in lines.iter().enumerate().skip(first_body) {
            let n = i + 1;
            let trimmed = line.trim();
            if let Some(heading) = trimmed.strip_prefix("## ") {
                match parse_heading(heading) {
                    Some((key, title)) => {
                        plan.days.push(Day { key, title, passages: Vec::new() });
                        current = true;
                    }
                    None => current = false,
                }
            } else if let Some(title) = trimmed.strip_prefix("# ") {
                if plan.name.is_empty() {
                    plan.name = title.trim().to_string();
                }
            } else if let Some(bullet) = bullet_text(trimmed)
                && current
            {
                let mut found = readings(bullet, &format!("line {n}"), &mut plan.problems);
                if let Some(day) = plan.days.last_mut() {
                    day.passages.append(&mut found);
                }
            }
        }
        plan
    }

    /// The plan as canonical markdown. Reading it back gives the same name, start and days.
    pub fn render(&self) -> String {
        let name = self.name.replace('\n', " ");
        let mut out = format!("---\ntype: reading-plan\nname: {name}\n");
        if let Some(start) = self.start {
            out.push_str(&format!("start: {}\n", start.format("%Y-%m-%d")));
        }
        out.push_str(&format!("tags: [reading-plan]\n---\n# {name}\n"));
        for day in &self.days {
            out.push_str(&format!("\n## {}", day.key));
            if let Some(title) = &day.title {
                out.push_str(&format!(" — {}", title.replace('\n', " ")));
            }
            out.push_str("\n\n");
            for span in spans(&day.passages, false) {
                out.push_str(&format!("- [[{span}]]\n"));
            }
        }
        out
    }

    /// The day that falls on `date`. A sequential day is the number of days since the start (the plan's
    /// own, else `started`) plus one; a dated day is that date; a day of the year is every year's.
    pub fn day_on(&self, date: NaiveDate, started: Option<NaiveDate>) -> Option<&Day> {
        let number = self.start.or(started).map(|s| (date - s).num_days() + 1).filter(|n| *n >= 1);
        self.days.iter().find(|d| match d.key {
            DayKey::Number(n) => number == Some(i64::from(n)),
            DayKey::Date(x) => x == date,
            DayKey::MonthDay(m, x) => (m, x) == (date.month(), date.day()),
        })
    }

    /// The first day that has come by `date` with a chapter `read` says is not read: where to
    /// catch up from.
    pub fn next_unread(&self, date: NaiveDate, started: Option<NaiveDate>, read: impl Fn(&RefRange) -> bool) -> Option<&Day> {
        let start = self.start.or(started);
        self.days.iter().filter(|d| d.is_due(date, start)).find(|d| {
            let (done, all) = d.progress(&read);
            done < all
        })
    }
}

// ---- reading the standard ----

/// The property lines (between the `---` lines) and the index of the first line after them.
fn split_front<'a, 'b>(lines: &'b [&'a str]) -> (&'b [&'a str], usize) {
    if lines.first().is_some_and(|l| l.trim() == "---")
        && let Some(end) = lines[1..].iter().position(|l| l.trim() == "---")
    {
        return (&lines[1..=end], end + 2);
    }
    (&[], 0)
}

/// `key: value`, the value without a trailing ` # comment` (for the keys that have no `#` of their own)
/// and without quotes.
fn front_value(line: &str) -> Option<(&str, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    let mut value = value.trim();
    if key != "name"
        && let Some(at) = value.find(" #")
    {
        value = value[..at].trim();
    }
    Some((key, value.trim_matches(['"', '\'']).to_string()))
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let mut parts = s.trim().split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 {
        return None;
    }
    NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
}

/// A day of the year, valid in a leap year so that February 29 is allowed.
fn month_day(m: u32, d: u32) -> Option<DayKey> {
    NaiveDate::from_ymd_opt(2024, m, d).map(|_| DayKey::MonthDay(m, d))
}

fn parse_month(word: &str) -> Option<u32> {
    let word = word.trim_end_matches('.').to_lowercase();
    if word.len() < 3 {
        return None;
    }
    let at = MONTHS.iter().position(|m| {
        let m = m.to_lowercase();
        m == word || m[..3] == word || (word == "sept" && m == "september")
    })?;
    Some(at as u32 + 1)
}

/// `Day 3`, `2026-01-01`, `January 1`, `Jan. 1`, `01-01`.
fn parse_key(s: &str) -> Option<DayKey> {
    let s = s.trim();
    if let Some(d) = parse_date(s) {
        return Some(DayKey::Date(d));
    }
    if let Some((m, d)) = s.split_once('-')
        && [m, d].iter().all(|p| (1..=2).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return month_day(m.parse().ok()?, d.parse().ok()?);
    }
    let mut words = s.split_whitespace();
    let (first, second) = (words.next()?, words.next()?);
    if words.next().is_some() {
        return None;
    }
    if first.eq_ignore_ascii_case("day") {
        return second.parse().ok().filter(|n| *n >= 1).map(DayKey::Number);
    }
    month_day(parse_month(first)?, second.parse().ok()?)
}

/// A heading's key and the title that follows it after a dash or a colon.
fn parse_heading(heading: &str) -> Option<(DayKey, Option<String>)> {
    let h = heading.trim();
    if let Some(key) = parse_key(h) {
        return Some((key, None));
    }
    let (at, len) = ["—", "–", ":", " - "].iter().filter_map(|s| h.find(s).map(|i| (i, s.len()))).min()?;
    let key = parse_key(&h[..at])?;
    let title = h[at + len..].trim();
    Some((key, (!title.is_empty()).then(|| title.to_string())))
}

fn bullet_text(line: &str) -> Option<&str> {
    ["- ", "* ", "+ "].iter().find_map(|p| line.strip_prefix(p))
}

/// `text` with `[[target|label]]` links replaced by their target.
fn unlink(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open..].find("]]") else { break };
        out.push_str(&rest[..open]);
        let inner = &rest[open + 2..open + close];
        out.push_str(inner.split('|').next().unwrap_or(inner));
        rest = &rest[open + close + 2..];
    }
    out.push_str(rest);
    out
}

/// A line without the note after a spaced dash (`Genesis 1 — the beginning`).
fn without_note(line: &str) -> &str {
    let at = [" — ", " – ", " - "].iter().filter_map(|s| line.find(s)).min();
    at.map_or(line, |at| &line[..at])
}

/// The passages in a piece of text: readings separated by `;` or `,` (or lines), as links or plain
/// text. What cannot be read goes to `problems`, named by `at`.
fn readings(text: &str, at: &str, problems: &mut Vec<String>) -> Vec<RefRange> {
    let mut found = Vec::new();
    for line in unlink(text).lines() {
        for item in without_note(line).split([';', ',']) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            match parse_item(item) {
                Some(mut passages) => found.append(&mut passages),
                None => problems.push(format!("{at}: could not read {item:?}")),
            }
        }
    }
    found
}

/// One reading, expanded into the passages it covers: `John 3`, `Genesis 1-3`, `Psalm 119:1-88`,
/// `John 3:16-4:3`, or a whole book (`Jude`, `Genesis`). A label before a colon and a space
/// (`Reading: Genesis 1`) is let go when the reading cannot be read with it.
fn parse_item(item: &str) -> Option<Vec<RefRange>> {
    parse_reading(item).or_else(|| parse_reading(item.split_once(": ")?.1))
}

fn parse_reading(item: &str) -> Option<Vec<RefRange>> {
    let item = item.trim().replace(['\u{2013}', '\u{2014}'], "-");
    let (book_part, loc) = match item.rsplit_once(char::is_whitespace) {
        Some((b, l)) if l.starts_with(|c: char| c.is_ascii_digit()) => (b, Some(l)),
        _ => (item.as_str(), None),
    };
    let book = Book::parse(book_part)?;
    let count = chapter_count(book);
    let in_book = |c: u16| (1..=count).contains(&c);
    let chapters = |a: u16, b: u16| (a..=b).map(|c| RefRange::chapter(book, c)).collect::<Vec<_>>();
    let verses = |chapter: u16, a: u16, b: u16| RefRange { book, chapter, verses: Some((a, b)) };
    let Some(loc) = loc else { return Some(chapters(1, count)) };
    let Some((c, rest)) = loc.split_once(':') else {
        let (a, b) = match loc.split_once('-') {
            Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
            None => {
                let a = loc.parse().ok()?;
                (a, a)
            }
        };
        return (in_book(a) && in_book(b) && a <= b).then(|| chapters(a, b));
    };
    let c: u16 = c.parse().ok()?;
    let (a, tail) = match rest.split_once('-') {
        Some((a, t)) => (a, Some(t)),
        None => (rest, None),
    };
    let a: u16 = a.parse().ok()?;
    if !in_book(c) || a < 1 {
        return None;
    }
    match tail {
        None => Some(vec![verses(c, a, a)]),
        Some(t) => match t.split_once(':') {
            None => {
                let b: u16 = t.parse().ok()?;
                (b >= a).then(|| vec![verses(c, a, b)])
            }
            Some((d, b)) => {
                let (d, b): (u16, u16) = (d.parse().ok()?, b.parse().ok()?);
                if d <= c || !in_book(d) || b < 1 {
                    return None;
                }
                let mut out = vec![verses(c, a, OPEN_END)];
                out.extend(chapters(c + 1, d - 1));
                out.push(verses(d, 1, b));
                Some(out)
            }
        },
    }
}

/// How passages are written: one each, with a verse range across chapters put back together
/// (`John 3:16-4:3`), and with `compact` a run of whole chapters as a range (`Genesis 1-3`).
fn spans(passages: &[RefRange], compact: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < passages.len() {
        let p = passages[i];
        let next = |j: usize, chapter: u16| passages.get(j).filter(|q| q.book == p.book && q.chapter == chapter);
        match p.verses {
            Some((a, OPEN_END)) => {
                let (mut j, mut last) = (i + 1, p.chapter);
                while next(j, last + 1).is_some_and(|q| q.verses.is_none()) {
                    j += 1;
                    last += 1;
                }
                if let Some((1, b)) = next(j, last + 1).and_then(|q| q.verses) {
                    out.push(format!("{} {}:{a}-{}:{b}", p.book, p.chapter, last + 1));
                    i = j + 1;
                    continue;
                }
                out.push(p.to_string());
            }
            None if compact => {
                let (mut j, mut last) = (i + 1, p.chapter);
                while next(j, last + 1).is_some_and(|q| q.verses.is_none()) {
                    j += 1;
                    last += 1;
                }
                out.push(if last == p.chapter { p.to_string() } else { format!("{} {}-{last}", p.book, p.chapter) });
                i = j;
                continue;
            }
            _ => out.push(p.to_string()),
        }
        i += 1;
    }
    out
}

// ---- importers ----

fn new_plan(name: &str) -> Plan {
    Plan { name: name.to_string(), start: None, days: Vec::new(), problems: Vec::new() }
}

/// A plan from CSV: a day in the first column (`1`, `2026-01-01`, `Jan 1`, `01-01`) and readings in
/// the others. A row whose first cell is not a day (a header row, say) is left out. The plan keeps
/// what could not be read in its `problems`.
pub fn plan_from_csv(name: &str, text: &str) -> Plan {
    let mut plan = new_plan(name);
    for (i, row) in csv_rows(text).iter().enumerate() {
        let cell = row[0].trim();
        let key = cell.parse::<u32>().ok().filter(|n| *n >= 1).map(DayKey::Number).or_else(|| parse_key(cell));
        let Some(key) = key else { continue };
        let text = row[1..].iter().map(|c| c.trim()).filter(|c| !c.is_empty()).collect::<Vec<_>>().join("; ");
        let passages = readings(&text, &format!("row {}", i + 1), &mut plan.problems);
        plan.days.push(Day { key, title: None, passages });
    }
    plan
}

/// [`plan_from_csv`] as canonical markdown.
pub fn from_csv(name: &str, text: &str) -> String {
    plan_from_csv(name, text).render()
}

/// The rows of a CSV, with quoted fields (`"a, b"`, `""` for a quote, line breaks inside quotes).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut cell)),
            '\r' | '\n' if !quoted => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                if row.iter().any(|c| !c.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
                row.clear();
            }
            _ => cell.push(c),
        }
    }
    row.push(cell);
    if row.iter().any(|c| !c.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

/// A dated plan from an iCalendar file: each event with a start date and readings in its summary (or,
/// when the summary has none, its description). Events on one date are one day.
pub fn plan_from_ics(name: &str, text: &str) -> Plan {
    let mut plan = new_plan(name);
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        match raw.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => lines.last_mut().unwrap().push_str(rest),
            _ => lines.push(raw.to_string()),
        }
    }
    let mut event: Option<(Option<NaiveDate>, String, String)> = None;
    for line in &lines {
        let Some((head, value)) = line.split_once(':') else { continue };
        let prop = head.split(';').next().unwrap_or("").to_ascii_uppercase();
        match (prop.as_str(), &mut event) {
            ("BEGIN", _) if value.trim().eq_ignore_ascii_case("VEVENT") => event = Some((None, String::new(), String::new())),
            ("DTSTART", Some(e)) => e.0 = value.trim().get(..8).and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok()),
            ("SUMMARY", Some(e)) => e.1 = ics_unescape(value),
            ("DESCRIPTION", Some(e)) => e.2 = ics_unescape(value),
            ("END", _) if value.trim().eq_ignore_ascii_case("VEVENT") => {
                if let Some((date, summary, description)) = event.take() {
                    add_event(&mut plan, date, &summary, &description);
                }
            }
            _ => {}
        }
    }
    plan.days.sort_by_key(|d| d.key_date());
    plan
}

fn add_event(plan: &mut Plan, date: Option<NaiveDate>, summary: &str, description: &str) {
    let Some(date) = date else {
        plan.problems.push(format!("event {summary:?}: no start date"));
        return;
    };
    let at = format!("event on {date}");
    let mut problems = Vec::new();
    let mut found = readings(summary, &at, &mut problems);
    if found.is_empty() && !description.trim().is_empty() {
        let mut from_description = Vec::new();
        found = readings(description, &at, &mut from_description);
        if found.is_empty() {
            problems.extend(from_description);
        } else {
            problems = from_description;
        }
    }
    plan.problems.extend(problems);
    match plan.days.iter_mut().find(|d| d.key == DayKey::Date(date)) {
        Some(day) => day.passages.extend(found),
        None => plan.days.push(Day { key: DayKey::Date(date), title: None, passages: found }),
    }
}

impl Day {
    fn key_date(&self) -> Option<NaiveDate> {
        match self.key {
            DayKey::Date(d) => Some(d),
            _ => None,
        }
    }
}

/// [`plan_from_ics`] as canonical markdown.
pub fn from_ics(name: &str, text: &str) -> String {
    plan_from_ics(name, text).render()
}

fn ics_unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (c, c == '\\') {
            (_, true) => match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            },
            _ => out.push(c),
        }
    }
    out
}

fn ics_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// A content line folded to 75 octets, as iCalendar asks: the rest goes on lines that begin with a space.
fn ics_line(line: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}

/// The plan as an iCalendar file of all-day events, day 1 on `start` (a sequential day is that many
/// days after it, a day of the year its first date on or after it, a dated day its own date). The
/// readings are each event's summary, a day's title its description.
pub fn to_ics(plan: &Plan, start: NaiveDate) -> String {
    let slug: String = plan.name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let stamp = format!("{}T000000Z", start.format("%Y%m%d"));
    let mut out = String::new();
    for line in ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Zoe//Reading plans//EN", "CALSCALE:GREGORIAN"] {
        out.push_str(&ics_line(line));
    }
    for (i, day) in plan.days.iter().enumerate() {
        let date = match day.key {
            DayKey::Number(n) => start.checked_add_days(Days::new(u64::from(n.saturating_sub(1)))),
            DayKey::Date(d) => Some(d),
            DayKey::MonthDay(m, d) => (start.year()..start.year() + 9).find_map(|y| NaiveDate::from_ymd_opt(y, m, d).filter(|x| *x >= start)),
        };
        let Some(date) = date else { continue };
        let readings = spans(&day.passages, true).join("; ");
        let summary = if readings.is_empty() { day.title.clone().unwrap_or_else(|| plan.name.clone()) } else { readings };
        let ymd = |d: NaiveDate| d.format("%Y%m%d").to_string();
        let mut event = vec![
            "BEGIN:VEVENT".to_string(),
            format!("UID:{slug}-{}-{}@zoe", i + 1, ymd(date)),
            format!("DTSTAMP:{stamp}"),
            format!("DTSTART;VALUE=DATE:{}", ymd(date)),
            format!("DTEND;VALUE=DATE:{}", ymd(date.succ_opt().unwrap_or(date))),
            format!("SUMMARY:{}", ics_escape(&summary)),
        ];
        if let Some(title) = day.title.as_ref().filter(|t| **t != summary) {
            event.push(format!("DESCRIPTION:{}", ics_escape(title)));
        }
        event.push("END:VEVENT".to_string());
        for line in event {
            out.push_str(&ics_line(&line));
        }
    }
    out.push_str(&ics_line("END:VCALENDAR"));
    out
}

/// EXPERIMENTAL. A plan from plain text lines in the shape a CrossWire/Xiphos plan is believed to
/// have, `1:Gen.1,Gen.2` or `1=Gen 1-3`: a day number, then `:` or `=`, then readings written as OSIS
/// (`Gen.1`, `Gen.1-Gen.3`) or as words. No sample of such a file was available, so this is a guess.
/// The shape is all in [`sword_line`] and the readings in [`sword_reading`]: adjust those.
/// Blank lines and lines starting with `#` or `;` are skipped.
pub fn plan_from_sword_lines(name: &str, text: &str) -> Plan {
    let mut plan = new_plan(name);
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        let at = format!("line {}", i + 1);
        let Some((n, rest)) = sword_line(line) else {
            plan.problems.push(format!("{at}: could not read {line:?}"));
            continue;
        };
        let mut passages = Vec::new();
        for item in rest.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
            match sword_reading(item) {
                Some(mut found) => passages.append(&mut found),
                None => plan.problems.push(format!("{at}: could not read {item:?}")),
            }
        }
        plan.days.push(Day { key: DayKey::Number(n), title: None, passages });
    }
    plan
}

/// The day number and the readings of a line: `N:readings` or `N=readings`.
fn sword_line(line: &str) -> Option<(u32, &str)> {
    let (n, rest) = line.split_once([':', '='])?;
    Some((n.trim().parse().ok().filter(|n| *n >= 1)?, rest))
}

/// One reading of such a line: OSIS (`Gen.1`, `Gen.1-Gen.3`, `John.3.16-John.3.18`) or words.
fn sword_reading(item: &str) -> Option<Vec<RefRange>> {
    if let Some((a, b)) = item.split_once('-')
        && let (Some(a), Some(b)) = (RefRange::from_osis(a), RefRange::from_osis(b))
        && a.book == b.book
        && a.verses.is_none()
        && b.verses.is_none()
        && a.chapter <= b.chapter
    {
        return Some((a.chapter..=b.chapter).map(|c| RefRange::chapter(a.book, c)).collect());
    }
    if let Some(r) = RefRange::from_osis(item) {
        return Some(vec![r]);
    }
    parse_item(item)
}

/// [`plan_from_sword_lines`] as canonical markdown. Experimental.
pub fn from_sword_lines(name: &str, text: &str) -> String {
    plan_from_sword_lines(name, text).render()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn ch(s: &str) -> RefRange {
        RefRange::parse(s).unwrap()
    }

    fn keys(plan: &Plan) -> Vec<DayKey> {
        plan.days.iter().map(|d| d.key).collect()
    }

    fn all_passages(plan: &Plan) -> Vec<Vec<RefRange>> {
        plan.days.iter().map(|d| d.passages.clone()).collect()
    }

    const SEQUENTIAL: &str = "---\ntype: reading-plan\nname: Gospels in 30 days\nstart: 2026-01-01        # optional\ntags: [reading-plan]\n---\n# Gospels in 30 days\n\n## Day 1\n\n- [[Matthew 1]]\n- [[Matthew 2]]\n\n## Day 2\n\n- Matthew 3-4; Psalm 1\n";

    #[test]
    fn the_chapter_table_has_every_book_and_the_known_counts() {
        assert_eq!(CHAPTERS.iter().map(|c| u32::from(*c)).sum::<u32>(), 1189);
        for (book, count) in [("Genesis", 50), ("Psalms", 150), ("Revelation", 22), ("Obadiah", 1), ("Jude", 1), ("Isaiah", 66)] {
            assert_eq!(chapter_count(Book::parse(book).unwrap()), count, "{book}");
        }
    }

    #[test]
    fn a_sequential_plan_reads_with_its_properties_and_expands_chapter_ranges() {
        assert!(Plan::is_plan(SEQUENTIAL));
        assert!(!Plan::is_plan("---\ntype: prayer\n---\n# x\n"));
        assert!(!Plan::is_plan("# no properties\n"));
        let plan = Plan::parse(SEQUENTIAL);
        assert_eq!(plan.name, "Gospels in 30 days");
        assert_eq!(plan.start, Some(d(2026, 1, 1)));
        assert_eq!(keys(&plan), [DayKey::Number(1), DayKey::Number(2)]);
        assert_eq!(plan.days[0].passages, [ch("Matthew 1"), ch("Matthew 2")]);
        assert_eq!(plan.days[1].passages, [ch("Matthew 3"), ch("Matthew 4"), ch("Psalm 1")]);
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    }

    #[test]
    fn a_dated_plan_and_a_yearly_plan_in_every_heading_spelling_are_read() {
        let dated = Plan::parse("# Dated\n\n## 2026-03-01\n\n- John 1\n\n## 2026-03-02 — Light\n\n- John 2\n");
        assert_eq!(keys(&dated), [DayKey::Date(d(2026, 3, 1)), DayKey::Date(d(2026, 3, 2))]);
        assert_eq!(dated.days[1].title.as_deref(), Some("Light"));
        let yearly = Plan::parse("# Y\n\n## January 1\n\n- Gen 1\n\n## Jan 2\n\n- Gen 2\n\n## 01-03\n\n- Gen 3\n\n## Sept. 4\n\n- Ps 1\n\n## Feb 29\n\n- Ps 2\n");
        assert_eq!(
            keys(&yearly),
            [DayKey::MonthDay(1, 1), DayKey::MonthDay(1, 2), DayKey::MonthDay(1, 3), DayKey::MonthDay(9, 4), DayKey::MonthDay(2, 29)]
        );
        assert!(yearly.problems.is_empty());
    }

    #[test]
    fn titles_after_a_dash_or_colon_are_kept_and_other_headings_are_not_days() {
        let plan = Plan::parse("# P\n\n## Day 3 — Creation\n\n- Gen 1\n\n## Day 4: Light - and more\n\n- Gen 2\n\n## Day 5 – Rest\n\n## Notes\n\n- Gen 9\n\n## Feb 30\n\n- Gen 10\n");
        assert_eq!(keys(&plan), [DayKey::Number(3), DayKey::Number(4), DayKey::Number(5)]);
        let titles: Vec<_> = plan.days.iter().map(|d| d.title.as_deref()).collect();
        assert_eq!(titles, [Some("Creation"), Some("Light - and more"), Some("Rest")]);
        assert_eq!(all_passages(&plan)[2], []);
        assert_eq!(plan.days.len(), 3, "the bullets under other headings belong to no day");
    }

    #[test]
    fn readings_in_a_bullet_are_split_linked_or_plain_and_notes_are_dropped() {
        let plan = Plan::parse("# P\n\n## Day 1\n\n- Genesis 1-3; [[Matthew 1]], Psalm 119:1-88 — the long one\n- * x\n* [[1 Cor 13|love]] - the chapter\n- Gen 1:1\n");
        let got: Vec<String> = plan.days[0].passages.iter().map(|p| p.to_string()).collect();
        assert_eq!(got, ["Genesis 1", "Genesis 2", "Genesis 3", "Matthew 1", "Psalms 119:1-88", "1 Corinthians 13", "Genesis 1:1"]);
        assert_eq!(plan.problems.len(), 1, "{:?}", plan.problems);
    }

    #[test]
    fn book_names_and_whole_books_and_bad_chapters_are_handled() {
        let p = |s: &str| parse_item(s);
        assert_eq!(p("Gen 1"), Some(vec![ch("Genesis 1")]));
        assert_eq!(p("Psalm 23"), Some(vec![ch("Psalms 23")]));
        assert_eq!(p("Psalms 23"), Some(vec![ch("Psalms 23")]));
        assert_eq!(p("1 Cor 13"), Some(vec![ch("1 Corinthians 13")]));
        assert_eq!(p("Jude"), Some(vec![ch("Jude 1")]));
        assert_eq!(p("Obadiah 1"), Some(vec![ch("Obadiah 1")]));
        assert_eq!(p("Genesis").map(|v| v.len()), Some(50));
        assert_eq!(p("Genesis 51"), None);
        assert_eq!(p("Genesis 3-2"), None);
        assert_eq!(p("Genesis 0"), None);
        assert_eq!(p("Nonsense 3"), None);
        assert_eq!(p("Reading: Genesis 1-2").map(|v| v.len()), Some(2));
        assert_eq!(p("Genesis 1\u{2013}2").map(|v| v.len()), Some(2), "an en dash is a range");
    }

    #[test]
    fn a_verse_range_across_chapters_is_split_by_chapter_and_written_back_whole() {
        let got = parse_item("John 3:16-5:2").unwrap();
        assert_eq!(got, [RefRange { book: Book::JOHN, chapter: 3, verses: Some((16, OPEN_END)) }, ch("John 4"), ch("John 5:1-2")]);
        assert_eq!(spans(&got, false), ["John 3:16-5:2"]);
        assert_eq!(spans(&parse_item("John 3:16-4:3").unwrap(), false), ["John 3:16-4:3"]);
        assert_eq!(parse_item("John 3:16-3:2"), None);
        assert_eq!(parse_item("John 3:17-16"), None);
        assert_eq!(parse_item("John 3:1-22:2"), None);
    }

    #[test]
    fn junk_lines_are_reported_with_their_line_numbers_and_nothing_panics() {
        let plan = Plan::parse("---\nstart: soon\n---\n# P\n\n## Day 1\n\n- Gen 1\n- not a reading\n- Genesis 99; Exodus 1\n\n## Day 2\n\n- ???\n");
        assert_eq!(plan.days[0].passages, [ch("Genesis 1"), ch("Exodus 1")]);
        assert_eq!(plan.problems.len(), 4, "{:?}", plan.problems);
        assert!(plan.problems[0].starts_with("start"));
        assert!(plan.problems[1].starts_with("line 9"), "{:?}", plan.problems);
        assert!(plan.problems[2].starts_with("line 10"));
        assert!(plan.problems[3].starts_with("line 14"));
        for junk in ["", "---", "---\n", "## ", "## Day 0\n- x", "- Gen 1\n", "\u{0}\u{1}", "## Day 1\n- [[", "## Day 1\n- [[Gen 1"] {
            Plan::parse(junk);
        }
    }

    #[test]
    fn a_plan_renders_to_markdown_that_reads_back_the_same() {
        let sources = [
            SEQUENTIAL.to_string(),
            "# Y\n\n## Feb 29 — Leap\n\n- Gen 1-2\n\n## 01-03\n\n- John 3:16-5:2; Jude\n\n## Dec 31\n".to_string(),
            "# D\n\n## 2026-03-01 — Light: the first\n\n- Psalm 119:1-88\n- Psalm 119:89-176\n".to_string(),
        ];
        for source in sources {
            let plan = Plan::parse(&source);
            let text = plan.render();
            assert_eq!(Plan::parse(&text), plan, "{text}");
            assert!(Plan::is_plan(&text));
            assert_eq!(Plan::parse(&text).render(), text, "rendering is stable");
        }
        let text = Plan::parse(SEQUENTIAL).render();
        assert!(text.starts_with("---\ntype: reading-plan\nname: Gospels in 30 days\nstart: 2026-01-01\ntags: [reading-plan]\n---\n# Gospels in 30 days\n\n## Day 1\n\n- [[Matthew 1]]\n- [[Matthew 2]]\n\n## Day 2\n\n- [[Matthew 3]]\n"), "{text}");
    }

    #[test]
    fn the_day_on_a_date_follows_the_plans_shape() {
        let plan = Plan::parse(SEQUENTIAL);
        assert_eq!(plan.day_on(d(2026, 1, 1), None).unwrap().key, DayKey::Number(1));
        assert_eq!(plan.day_on(d(2026, 1, 2), None).unwrap().key, DayKey::Number(2));
        assert!(plan.day_on(d(2026, 1, 3), None).is_none());
        assert!(plan.day_on(d(2025, 12, 31), None).is_none());
        // Without a start of its own, the plan is as started.
        let unstarted = Plan::parse("# P\n\n## Day 1\n\n- Gen 1\n\n## Day 2\n\n- Gen 2\n");
        assert!(unstarted.day_on(d(2026, 5, 5), None).is_none());
        assert_eq!(unstarted.day_on(d(2026, 5, 6), Some(d(2026, 5, 5))).unwrap().key, DayKey::Number(2));
        let dated = Plan::parse("# D\n\n## 2026-03-01\n\n- John 1\n");
        assert!(dated.day_on(d(2026, 3, 1), None).is_some());
        assert!(dated.day_on(d(2027, 3, 1), None).is_none());
        let yearly = Plan::parse("# Y\n\n## Mar 1\n\n- John 1\n\n## Feb 29\n\n- John 2\n");
        assert!(yearly.day_on(d(2031, 3, 1), None).is_some());
        assert_eq!(yearly.day_on(d(2028, 2, 29), None).unwrap().key, DayKey::MonthDay(2, 29));
        assert!(yearly.day_on(d(2027, 2, 28), None).is_none(), "no February 29 in 2027");
    }

    #[test]
    fn progress_counts_chapters_read_once_each_and_verses_follow_their_chapter() {
        let plan = Plan::parse("# P\n\n## Day 1\n\n- Gen 1-3\n- Psalm 119:1-88; Psalm 119:89-176\n");
        let read = |r: &RefRange| r.verses.is_none() && (r.book == Book::GENESIS && r.chapter != 2 || r.book == Book::PSALMS);
        assert_eq!(plan.days[0].progress(read), (3, 4));
        assert_eq!(plan.days[0].progress(|_| false), (0, 4));
        assert_eq!(Day { key: DayKey::Number(1), title: None, passages: vec![] }.progress(|_| true), (0, 0));
    }

    #[test]
    fn catching_up_finds_the_first_day_that_has_come_with_a_chapter_unread() {
        let plan = Plan::parse(SEQUENTIAL);
        let nothing = |_: &RefRange| false;
        assert_eq!(plan.next_unread(d(2026, 1, 2), None, nothing).unwrap().key, DayKey::Number(1));
        let day_one = |r: &RefRange| r.book == Book::parse("Matthew").unwrap() && r.chapter <= 2;
        assert_eq!(plan.next_unread(d(2026, 1, 2), None, day_one).unwrap().key, DayKey::Number(2));
        assert!(plan.next_unread(d(2026, 1, 1), None, day_one).is_none(), "day 2 has not come");
        assert!(plan.next_unread(d(2025, 12, 1), None, nothing).is_none(), "not started yet");
        let dated = Plan::parse("# D\n\n## 2026-03-01\n\n- John 1\n\n## 2026-03-05\n\n- John 2\n");
        assert_eq!(dated.next_unread(d(2026, 3, 4), None, |_| false).unwrap().key, DayKey::Date(d(2026, 3, 1)));
        let yearly = Plan::parse("# Y\n\n## Mar 1\n\n- John 1\n\n## Mar 5\n\n- John 2\n");
        assert!(yearly.next_unread(d(2026, 2, 28), None, |_| false).is_none());
        assert_eq!(yearly.next_unread(d(2026, 3, 5), None, |r| r.chapter == 1).unwrap().key, DayKey::MonthDay(3, 5));
    }

    #[test]
    fn csv_makes_sequential_dated_and_yearly_plans_with_quoted_fields() {
        let seq = from_csv("Seq", "day,readings\r\n1,\"Genesis 1-2, Matthew 1\"\r\n2,Psalm 1\r\n");
        let plan = Plan::parse(&seq);
        assert_eq!(plan.name, "Seq");
        assert_eq!(keys(&plan), [DayKey::Number(1), DayKey::Number(2)]);
        assert_eq!(plan.days[0].passages, [ch("Genesis 1"), ch("Genesis 2"), ch("Matthew 1")]);
        assert!(plan.problems.is_empty());
        let dated = Plan::parse(&from_csv("D", "2026-01-01,John 1\n2026-01-02,\"John 2\"\"x\"\n"));
        assert_eq!(keys(&dated), [DayKey::Date(d(2026, 1, 1)), DayKey::Date(d(2026, 1, 2))]);
        let yearly = Plan::parse(&from_csv("Y", "date,readings,more\nJan 1,Gen 1,Matt 1\n01-02,\"Gen 2\nGen 3\",\nFeb 29,Ps 1\n"));
        assert_eq!(keys(&yearly), [DayKey::MonthDay(1, 1), DayKey::MonthDay(1, 2), DayKey::MonthDay(2, 29)]);
        assert_eq!(yearly.days[0].passages, [ch("Genesis 1"), ch("Matthew 1")]);
        assert_eq!(yearly.days[1].passages, [ch("Genesis 2"), ch("Genesis 3")]);
        let bad = plan_from_csv("B", "1,Gen 1\n2,nonsense\n");
        assert_eq!(bad.days.len(), 2);
        assert!(bad.problems[0].starts_with("row 2"));
    }

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20260102\r\nSUMMARY:Reading: Genesis 2\\;\r\n  Psalm 2\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20260101\r\nSUMMARY:Day one\r\nDESCRIPTION:Genesis 1\\nMatthew 1\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20260103T090000Z\r\nSUMMARY:John 3:16-4:3\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:No date\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn ics_events_become_dated_days_with_folded_lines_and_descriptions() {
        let text = from_ics("Cal", ICS);
        let plan = Plan::parse(&text);
        assert_eq!(keys(&plan), [DayKey::Date(d(2026, 1, 1)), DayKey::Date(d(2026, 1, 2)), DayKey::Date(d(2026, 1, 3))]);
        assert_eq!(plan.days[0].passages, [ch("Genesis 1"), ch("Matthew 1")], "the description, as the summary has none");
        assert_eq!(plan.days[1].passages, [ch("Genesis 2"), ch("Psalm 2")], "the folded line, and a label");
        assert_eq!(plan.days[2].passages.len(), 2);
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
        let imported = plan_from_ics("Cal", ICS);
        assert_eq!(imported.problems.len(), 1, "the event with no date: {:?}", imported.problems);
    }

    #[test]
    fn a_plan_exported_to_ics_has_crlf_folded_lines_and_comes_back_the_same() {
        let long = (1..=30).map(|c| format!("Isaiah {}", c * 2)).collect::<Vec<_>>().join("; ");
        let source = format!("# Out, é\n\n## Day 1 — Start\n\n- Genesis 1-3; John 3:16-4:3\n\n## Day 2\n\n- {long}\n\n## Day 3\n\n- Jude\n");
        let plan = Plan::parse(&source);
        let ics = to_ics(&plan, d(2026, 2, 27));
        assert!(ics.starts_with("BEGIN:VCALENDAR\r\n") && ics.ends_with("END:VCALENDAR\r\n"));
        assert!(!ics.replace("\r\n", "").contains('\n') && !ics.replace("\r\n", "").contains('\r'));
        assert!(ics.lines().all(|l| l.len() <= 75), "folded at 75 octets");
        assert!(ics.contains("\r\n "), "the long summary is folded");
        assert!(ics.contains("DTSTART;VALUE=DATE:20260227\r\n") && ics.contains("DTSTART;VALUE=DATE:20260301\r\n"));
        assert!(ics.contains("SUMMARY:Genesis 1-3\\; John 3:16-4:3\r\n"), "{ics}");
        assert!(ics.contains("DESCRIPTION:Start\r\n"));
        let uids: Vec<_> = ics.lines().filter(|l| l.starts_with("UID:")).collect();
        assert_eq!(uids.len(), 3);
        assert!(uids[0] != uids[1] && uids[1] != uids[2]);
        let back = plan_from_ics(&plan.name, &ics);
        assert!(back.problems.is_empty(), "{:?}", back.problems);
        assert_eq!(all_passages(&back), all_passages(&plan));
        assert_eq!(keys(&back), [DayKey::Date(d(2026, 2, 27)), DayKey::Date(d(2026, 2, 28)), DayKey::Date(d(2026, 3, 1))]);
        assert_eq!(Plan::parse(&back.render()), back);
    }

    #[test]
    fn a_yearly_plan_exports_from_its_first_date_on_or_after_the_start() {
        let plan = Plan::parse("# Y\n\n## Jan 5\n\n- Gen 1\n\n## Feb 29\n\n- Gen 2\n");
        let ics = to_ics(&plan, d(2026, 6, 1));
        assert!(ics.contains("DTSTART;VALUE=DATE:20270105\r\n"));
        assert!(ics.contains("DTSTART;VALUE=DATE:20280229\r\n"));
    }

    #[test]
    fn experimental_sword_lines_make_a_sequential_plan_from_osis_or_words() {
        let text = "# a comment\n1:Gen.1,Gen.2\n2=Gen 3-4; Ps.23\n\n3:Matt.1-Matt.3\n4:John.3.16-John.3.18\nnonsense\n5:Bogus.1\n";
        let imported = plan_from_sword_lines("Sword", text);
        assert_eq!(keys(&imported), [DayKey::Number(1), DayKey::Number(2), DayKey::Number(3), DayKey::Number(4), DayKey::Number(5)]);
        assert_eq!(imported.days[0].passages, [ch("Genesis 1"), ch("Genesis 2")]);
        assert_eq!(imported.days[1].passages, [ch("Genesis 3"), ch("Genesis 4"), ch("Psalm 23")]);
        assert_eq!(imported.days[2].passages, [ch("Matthew 1"), ch("Matthew 2"), ch("Matthew 3")]);
        assert_eq!(imported.days[3].passages, [ch("John 3:16-18")]);
        assert_eq!(imported.problems.len(), 2, "{:?}", imported.problems);
        let parsed = Plan::parse(&from_sword_lines("Sword", text));
        assert_eq!(all_passages(&parsed), all_passages(&imported));
        assert_eq!(keys(&parsed), keys(&imported));
    }
}
