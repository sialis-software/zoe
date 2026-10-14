//! The plan log: a note that days of plans are added to as they are completed, in the order and on the
//! day they were. Like the reading log (`reading_log.rs`) it is an ordinary note: each day is a
//! heading, each plan day completed that day a bullet that links to the day's note by its path, with
//! a friendly label.
//!
//! ```text
//! ## 2026-10-13
//!
//! - [[library/Morning and Evening/01-01|Morning and Evening · January 1]]
//! ```

use chrono::{DateTime, Local};
use std::collections::HashMap;

/// The heading of the day in the log.
fn day_heading(now: &DateTime<Local>) -> String {
    format!("## {}", now.format("%Y-%m-%d"))
}

/// A note's path as it is kept in the log's links and compared: with `.md`.
fn with_extension(path: &str) -> String {
    let path = path.trim();
    if path.ends_with(".md") { path.to_string() } else { format!("{path}.md") }
}

/// What a bullet of the log links to: the note (`- [[library/X/01-01|label]]`, as a path with `.md`) and,
/// for a reading ticked on the way to completing the day (`- [[library/X/Day 1#Genesis 1|label]]`), that
/// reading.
fn link_of(line: &str) -> Option<(String, Option<String>)> {
    let rest = line.trim_start().strip_prefix("- ")?;
    let inner = rest.split_once("[[")?.1.split_once("]]")?.0;
    let target = inner.split('|').next()?.trim();
    let (path, reading) = match target.split_once('#') {
        Some((path, reading)) => (path.trim(), Some(reading.trim().to_string()).filter(|r| !r.is_empty())),
        None => (target, None),
    };
    (!path.is_empty()).then(|| (with_extension(path), reading))
}

/// The note a completion bullet links to; a ticked reading is not a completion.
fn target_of(line: &str) -> Option<String> {
    link_of(line).and_then(|(path, reading)| reading.is_none().then_some(path))
}

/// The log as it begins: its properties and title.
pub fn new_log(title: &str, created: &str) -> String {
    format!("---\ncreated: {created}\ntags: [plan-log]\n---\n# {title}\n")
}

/// `log` with a line for the day note at `day_path`, labelled `label`, completed on the day of `now`,
/// added to the end under the day's heading (made if the last heading is another day's). `None` when
/// that day's note is already under today's heading.
pub fn add(log: &str, day_path: &str, label: &str, now: &DateTime<Local>) -> Option<String> {
    let heading = day_heading(now);
    let path = with_extension(day_path);
    let lines: Vec<&str> = log.lines().collect();
    let last_heading = lines.iter().rposition(|l| l.starts_with("## "));
    let is_today = last_heading.is_some_and(|at| lines[at] == heading);
    if let Some(at) = last_heading.filter(|_| is_today)
        && lines[at + 1..].iter().any(|l| target_of(l).is_some_and(|t| t == path))
    {
        return None;
    }
    let label: String = label.chars().map(|c| if matches!(c, '|' | '[' | ']') { ' ' } else { c }).collect();
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let target = path.strip_suffix(".md").unwrap_or(&path);
    let link = if label.is_empty() { format!("[[{target}]]") } else { format!("[[{target}|{label}]]") };
    let mut out = log.trim_end().to_string();
    if !is_today {
        out.push_str(&format!("\n\n{heading}\n"));
    }
    out.push_str(&format!("\n- {link}\n"));
    Some(out)
}

/// `log` without the line for the day note at `day_path` under the day of `now`; `None` when it has
/// none there.
pub fn remove(log: &str, day_path: &str, now: &DateTime<Local>) -> Option<String> {
    let heading = day_heading(now);
    let path = with_extension(day_path);
    let mut in_day = false;
    let mut removed = false;
    let kept: Vec<&str> = log
        .lines()
        .filter(|line| {
            if line.starts_with("## ") {
                in_day = *line == heading;
            }
            let gone = in_day && !removed && target_of(line).is_some_and(|t| t == path);
            removed |= gone;
            !gone
        })
        .collect();
    removed.then(|| {
        let mut text = kept.join("\n");
        text.push('\n');
        text
    })
}

/// `log` with a line that says the reading (such as `Genesis 1`) of the day note at `day_path` was
/// done on the day of `now`, under that day's heading. `None` when it is already there today.
pub fn tick(log: &str, day_path: &str, reading: &str, label: &str, now: &DateTime<Local>) -> Option<String> {
    let heading = day_heading(now);
    let path = with_extension(day_path);
    let reading: String = reading.chars().filter(|c| !matches!(c, '|' | '[' | ']' | '#' | '\n')).collect();
    let lines: Vec<&str> = log.lines().collect();
    let last_heading = lines.iter().rposition(|l| l.starts_with("## "));
    let is_today = last_heading.is_some_and(|at| lines[at] == heading);
    if let Some(at) = last_heading.filter(|_| is_today)
        && lines[at + 1..].iter().any(|l| link_of(l).is_some_and(|(p, r)| p == path && r.as_deref() == Some(reading.trim())))
    {
        return None;
    }
    let label: String = label.chars().map(|c| if matches!(c, '|' | '[' | ']') { ' ' } else { c }).collect();
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let target = path.strip_suffix(".md").unwrap_or(&path);
    let link = if label.is_empty() { format!("[[{target}#{}]]", reading.trim()) } else { format!("[[{target}#{}|{label}]]", reading.trim()) };
    let mut out = log.trim_end().to_string();
    if !is_today {
        out.push_str(&format!("\n\n{heading}\n"));
    }
    out.push_str(&format!("\n- {link}\n"));
    Some(out)
}

/// `log` without any line (under any day) that ticks `reading` of the day note at `day_path`; `None`
/// when there is none.
pub fn untick(log: &str, day_path: &str, reading: &str) -> Option<String> {
    drop_lines(log, |l| link_of(l).is_some_and(|(p, r)| p == with_extension(day_path) && r.as_deref() == Some(reading.trim())))
}

/// `log` without any line (under any day) that completes the day note at `day_path`; `None` when there
/// is none.
pub fn uncomplete(log: &str, day_path: &str) -> Option<String> {
    drop_lines(log, |l| target_of(l).is_some_and(|t| t == with_extension(day_path)))
}

fn drop_lines(log: &str, gone: impl Fn(&str) -> bool) -> Option<String> {
    let mut removed = false;
    let kept: Vec<&str> = log
        .lines()
        .filter(|line| {
            let drop = gone(line);
            removed |= drop;
            !drop
        })
        .collect();
    removed.then(|| {
        let mut text = kept.join("\n");
        text.push('\n');
        text
    })
}

/// The readings the log says were ticked, by day note path (with `.md`) and reading, each with the
/// latest day (`2026-10-13`) it was ticked on.
pub fn ticked(log: &str) -> HashMap<String, HashMap<String, String>> {
    let mut found: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut day: Option<&str> = None;
    for line in log.lines() {
        if let Some(date) = line.strip_prefix("## ") {
            day = Some(date.trim()).filter(|d| crate::frontmatter::is_date(d));
        } else if let (Some(date), Some((path, Some(reading)))) = (day, link_of(line)) {
            let slot = found.entry(path).or_default().entry(reading).or_default();
            if date > slot.as_str() {
                *slot = date.to_string();
            }
        }
    }
    found
}

/// The day notes the log says are completed, by path (with `.md`), each with the latest day
/// (`2026-10-13`) it was completed on.
pub fn completed(log: &str) -> HashMap<String, String> {
    let mut found: HashMap<String, String> = HashMap::new();
    let mut day: Option<&str> = None;
    for line in log.lines() {
        if let Some(date) = line.strip_prefix("## ") {
            day = Some(date.trim()).filter(|d| crate::frontmatter::is_date(d));
        } else if let (Some(date), Some(target)) = (day, target_of(line)) {
            let slot = found.entry(target).or_default();
            if date > slot.as_str() {
                *slot = date.to_string();
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, h: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, day, h, 0, 0).unwrap()
    }

    const JAN_1: &str = "library/Morning and Evening/01-01.md";

    #[test]
    fn a_completed_day_is_a_bullet_that_links_to_its_note_by_path_under_the_day() {
        let log = add(&new_log("Plan log", "2026-10-13T08:00:00"), JAN_1, "Morning and Evening · January 1", &at(13, 9)).unwrap();
        assert!(log.starts_with("---\ncreated: 2026-10-13T08:00:00\ntags: [plan-log]\n---\n# Plan log\n"));
        assert!(log.ends_with("## 2026-10-13\n\n- [[library/Morning and Evening/01-01|Morning and Evening · January 1]]\n"), "{log}");
        let log = add(&log, "library/Gospels/Day 1", "Gospels · Day 1", &at(13, 10)).unwrap();
        let log = add(&log, JAN_1, "Morning and Evening · January 1", &at(14, 7)).unwrap();
        assert!(log.ends_with("\n## 2026-10-14\n\n- [[library/Morning and Evening/01-01|Morning and Evening · January 1]]\n"), "{log}");
    }

    #[test]
    fn a_day_is_under_a_heading_once_whether_or_not_its_path_has_the_extension() {
        let log = add(&new_log("L", "x"), JAN_1, "a", &at(13, 9)).unwrap();
        assert_eq!(add(&log, JAN_1, "a", &at(13, 20)), None);
        assert_eq!(add(&log, "library/Morning and Evening/01-01", "a", &at(13, 20)), None);
        assert!(add(&log, JAN_1, "a", &at(14, 9)).is_some(), "the next day has it again");
        assert!(add(&log, "library/Morning and Evening/01-02.md", "b", &at(13, 20)).is_some());
    }

    #[test]
    fn labels_cannot_break_the_link_and_an_empty_one_is_left_off() {
        let log = add(&new_log("L", "x"), JAN_1, "a | [b]\nc", &at(13, 9)).unwrap();
        assert!(log.ends_with("- [[library/Morning and Evening/01-01|a b c]]\n"), "{log}");
        let bare = add(&new_log("L", "x"), JAN_1, "  ", &at(13, 9)).unwrap();
        assert!(bare.ends_with("- [[library/Morning and Evening/01-01]]\n"));
        assert!(completed(&bare).contains_key(JAN_1));
    }

    #[test]
    fn completion_is_read_back_with_the_latest_day_of_each_note() {
        let mut log = new_log("L", "x");
        for (day, path) in [(10, JAN_1), (11, "library/G/Day 1.md"), (12, JAN_1)] {
            log = add(&log, path, "x", &at(day, 9)).unwrap();
        }
        log.push_str("\n## 2026-10-01\n\n- [[library/Old/01-01|old]]\n\n## not a date\n\n- [[library/Bad/01-01]]\n\n- plain text\n");
        let done = completed(&log);
        assert_eq!(done.get(JAN_1).map(String::as_str), Some("2026-10-12"));
        assert_eq!(done.get("library/G/Day 1.md").map(String::as_str), Some("2026-10-11"));
        assert_eq!(done.get("library/Old/01-01.md").map(String::as_str), Some("2026-10-01"));
        assert!(!done.contains_key("library/Bad/01-01.md"));
        assert_eq!(done.len(), 3);
    }

    #[test]
    fn a_days_completion_can_be_taken_back_for_that_day_only() {
        let mut log = new_log("L", "x");
        for day in [10, 12] {
            log = add(&log, JAN_1, "x", &at(day, 9)).unwrap();
        }
        let taken = remove(&log, JAN_1, &at(12, 20)).unwrap();
        assert_eq!(completed(&taken).get(JAN_1).map(String::as_str), Some("2026-10-10"));
        assert_eq!(remove(&taken, JAN_1, &at(12, 20)), None);
        assert_eq!(remove(&taken, "library/Other/01-01.md", &at(10, 9)), None);
    }

    #[test]
    fn a_ticked_reading_is_a_line_that_is_not_a_completion_and_can_be_taken_back() {
        let log = new_log("L", "x");
        let log = tick(&log, "library/Gospels/Day 1", "Matthew 1", "Gospels \u{b7} Day 1 \u{b7} Matthew 1", &at(13, 9)).unwrap();
        assert!(log.ends_with("## 2026-10-13\n\n- [[library/Gospels/Day 1#Matthew 1|Gospels \u{b7} Day 1 \u{b7} Matthew 1]]\n"), "{log}");
        assert_eq!(tick(&log, "library/Gospels/Day 1.md", "Matthew 1", "x", &at(13, 20)), None, "once a day");
        let log = tick(&log, "library/Gospels/Day 1", "Matthew 2", "", &at(13, 21)).unwrap();
        assert!(completed(&log).is_empty(), "a tick does not complete the day");
        let seen = ticked(&log);
        assert_eq!(seen["library/Gospels/Day 1.md"]["Matthew 1"], "2026-10-13");
        assert_eq!(seen["library/Gospels/Day 1.md"].len(), 2);
        let log = untick(&log, "library/Gospels/Day 1", "Matthew 1").unwrap();
        assert_eq!(ticked(&log)["library/Gospels/Day 1.md"].len(), 1);
        assert_eq!(untick(&log, "library/Gospels/Day 1", "Matthew 1"), None);
    }

    #[test]
    fn a_day_completed_on_any_date_can_be_taken_back_and_its_ticks_stay() {
        let log = tick(&new_log("L", "x"), JAN_1, "Genesis 1", "", &at(12, 9)).unwrap();
        let log = add(&log, JAN_1, "a", &at(12, 10)).unwrap();
        let log = add(&log, JAN_1, "a", &at(14, 10)).unwrap();
        assert_eq!(completed(&log)[JAN_1], "2026-10-14");
        let log = uncomplete(&log, JAN_1).unwrap();
        assert!(completed(&log).is_empty());
        assert_eq!(ticked(&log)[JAN_1].len(), 1);
        assert_eq!(uncomplete(&log, JAN_1), None);
    }
}
