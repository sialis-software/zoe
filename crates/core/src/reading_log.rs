//! The reading log: a note that chapters are added to, as they are read, in the order and at the time
//! they were. It is an ordinary note (open it, link to it, search it): each day is a heading, each
//! chapter read that day a bullet with a link.
//!
//! ```text
//! ## 2025-10-13
//!
//! - [[John 3]]
//! - [[John 4]]
//! ```
//!
//! The day's own note (the daily note) can be given the same lines under its `## Reading`.

use chrono::{DateTime, Local};

/// The heading of the day in the log.
fn day_heading(now: &DateTime<Local>) -> String {
    format!("## {}", now.format("%Y-%m-%d"))
}

/// The log as it begins: its properties and title.
pub fn new_log(title: &str, created: &str) -> String {
    format!("---\ncreated: {created}\ntags: [reading-log]\n---\n# {title}\n")
}

/// `log` with a line for `link` (`[[John 3]]`), read on the day of `now`, added to the end, under the
/// day's heading (made if the last heading is another day's). `None` when the day already has it: a
/// chapter is in a day once, however often it is read.
pub fn add_entry(log: &str, link: &str, now: &DateTime<Local>) -> Option<String> {
    let heading = day_heading(now);
    let lines: Vec<&str> = log.lines().collect();
    let last_heading = lines.iter().rposition(|l| l.starts_with("## "));
    let is_today = last_heading.is_some_and(|at| lines[at] == heading);
    if let Some(at) = last_heading.filter(|_| is_today)
        && lines[at + 1..].iter().any(|l| l.trim_start().starts_with("- ") && l.trim_end().ends_with(link))
    {
        return None;
    }
    let mut out = log.trim_end().to_string();
    if !is_today {
        out.push_str(&format!("\n\n{heading}\n"));
    }
    out.push_str(&format!("\n- {link}\n"));
    Some(out)
}

/// The days `link` is in the log, as `2025-10-13`, the latest first.
pub fn dates_read(log: &str, link: &str) -> Vec<String> {
    let mut day: Option<&str> = None;
    let mut found: Vec<String> = Vec::new();
    for line in log.lines() {
        if let Some(date) = line.strip_prefix("## ") {
            day = Some(date.trim());
        } else if let Some(date) = day
            && line.trim_start().starts_with("- ")
            && line.trim_end().ends_with(link)
            && !found.iter().any(|d| d == date)
        {
            found.push(date.to_string());
        }
    }
    found.sort();
    found.reverse();
    found
}

/// `log` without the line for `link` under the day of `now`; `None` when it has none there.
pub fn remove_entry(log: &str, link: &str, now: &DateTime<Local>) -> Option<String> {
    let heading = day_heading(now);
    let mut in_day = false;
    let mut removed = false;
    let kept: Vec<&str> = log
        .lines()
        .filter(|line| {
            if line.starts_with("## ") {
                in_day = *line == heading;
            }
            let gone = in_day && !removed && line.trim_start().starts_with("- ") && line.trim_end().ends_with(link);
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

/// `note` without the bullet for `item`; `None` when it has none.
pub fn remove_bullet(note: &str, item: &str) -> Option<String> {
    let bullet = format!("- {item}");
    let mut removed = false;
    let kept: Vec<&str> = note
        .lines()
        .filter(|line| {
            let gone = !removed && line.trim() == bullet;
            removed |= gone;
            !gone
        })
        .collect();
    removed.then(|| {
        let mut text = kept.join("\n");
        if note.ends_with('\n') {
            text.push('\n');
        }
        text
    })
}

/// A daily note's text with `link` under its `## Reading` heading (made at the end if there is
/// none). An empty bullet there (the template's) is taken for it. `None` when it is already there.
pub fn add_to_day(note: &str, link: &str) -> Option<String> {
    add_bullet(note, "Reading", link, true)
}

/// `note` with a bullet for `item` under its `## {heading}` (made at the end if there is none and
/// `make` says to; with `make` false, `None` when there is no such heading). An empty bullet there is
/// taken for it. `None` when `item` is already in the note.
pub fn add_bullet(note: &str, heading: &str, item: &str, make: bool) -> Option<String> {
    if note.contains(item) {
        return None;
    }
    let bullet = format!("- {item}");
    let lines: Vec<&str> = note.lines().collect();
    let Some(head) = lines.iter().position(|l| l.trim().eq_ignore_ascii_case(&format!("## {heading}"))) else {
        return make.then(|| format!("{}\n\n## {heading}\n\n{bullet}\n", note.trim_end()));
    };
    let end = lines[head + 1..].iter().position(|l| l.starts_with('#')).map_or(lines.len(), |n| head + 1 + n);
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    match (head + 1..end).find(|&i| lines[i].trim() == "-") {
        Some(empty) => out[empty] = bullet,
        None => {
            // After the last thing in the section, before the blank lines that part it from the next.
            let last = (head + 1..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(head);
            if last == head {
                out.insert(head + 1, String::new());
                out.insert(head + 2, bullet);
            } else {
                out.insert(last + 1, bullet);
            }
        }
    }
    let mut text = out.join("\n");
    if note.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2025, 10, day, h, m, 0).unwrap()
    }

    #[test]
    fn chapters_are_added_under_the_day_in_the_order_read_with_no_time() {
        let log = new_log("Reading log", "2025-10-13T08:00:00");
        let log = add_entry(&log, "[[John 3]]", &at(13, 16, 20)).unwrap();
        let log = add_entry(&log, "[[John 4]]", &at(13, 16, 41)).unwrap();
        let log = add_entry(&log, "[[Psalm 23]]", &at(14, 7, 5)).unwrap();
        assert!(log.starts_with("---\ncreated: 2025-10-13T08:00:00\ntags: [reading-log]\n---\n# Reading log\n"));
        let body = &log[log.find("## 2025-10-13").unwrap()..];
        assert_eq!(body, "## 2025-10-13\n\n- [[John 3]]\n- [[John 4]]\n\n## 2025-10-14\n\n- [[Psalm 23]]\n");
    }

    #[test]
    fn a_chapter_is_in_a_day_once_however_often_it_is_read_but_each_day_has_it() {
        let log = add_entry(&new_log("Reading log", "x"), "[[John 3]]", &at(13, 9, 0)).unwrap();
        assert_eq!(add_entry(&log, "[[John 3]]", &at(13, 9, 5)), None);
        let log = add_entry(&log, "[[John 4]]", &at(13, 9, 10)).unwrap();
        assert_eq!(add_entry(&log, "[[John 3]]", &at(13, 20, 0)), None, "later the same day, after another chapter");
        let next = add_entry(&log, "[[John 4]]", &at(14, 9, 0)).expect("the next day has it again, though it was the last line");
        assert!(next.ends_with("## 2025-10-14\n\n- [[John 4]]\n"), "{next}");
    }

    #[test]
    fn a_day_with_no_heading_yet_in_a_log_somebody_wrote_gets_one() {
        let log = "# My log\n\nSome words.\n";
        let made = add_entry(log, "[[John 3]]", &at(13, 9, 0)).unwrap();
        assert_eq!(made, "# My log\n\nSome words.\n\n## 2025-10-13\n\n- [[John 3]]\n");
    }

    #[test]
    fn a_bullet_goes_under_the_heading_asked_for_and_only_makes_it_when_told_to() {
        let note = "# Day\n\n## Bookmarks\n\n- [[John 1]]\n\n## Prayer\n";
        assert_eq!(add_bullet(note, "Bookmarks", "[[John 3:16]]", false).unwrap(), "# Day\n\n## Bookmarks\n\n- [[John 1]]\n- [[John 3:16]]\n\n## Prayer\n");
        assert_eq!(add_bullet("# Day\n", "Bookmarks", "x", false), None, "no such heading, and not to be made");
        assert_eq!(add_bullet("# Day\n", "Bookmarks", "x", true).unwrap(), "# Day\n\n## Bookmarks\n\n- x\n");
        assert_eq!(add_bullet(note, "Bookmarks", "[[John 1]]", false), None, "not twice");
    }

    #[test]
    fn a_daily_note_gets_the_chapter_under_its_reading_heading() {
        let daily = "# Monday\n\n## Reading\n\n- \n\n## Reflection\n\n\n## Prayer\n";
        let one = add_to_day(daily, "[[John 3]]").unwrap();
        assert_eq!(one, "# Monday\n\n## Reading\n\n- [[John 3]]\n\n## Reflection\n\n\n## Prayer\n", "the empty bullet is filled");
        let two = add_to_day(&one, "[[John 4]]").unwrap();
        assert!(two.contains("## Reading\n\n- [[John 3]]\n- [[John 4]]\n\n## Reflection"), "{two}");
        assert_eq!(add_to_day(&two, "[[John 3]]"), None, "not twice");
        // No such section: made at the end. An empty one: filled.
        assert_eq!(add_to_day("# Day\n", "[[John 3]]").unwrap(), "# Day\n\n## Reading\n\n- [[John 3]]\n");
        assert_eq!(add_to_day("# Day\n\n## Reading\n\n## Prayer\n", "[[John 3]]").unwrap(), "# Day\n\n## Reading\n\n- [[John 3]]\n\n## Prayer\n");
    }

    #[test]
    fn the_days_a_chapter_was_read_are_listed_latest_first_and_a_days_line_can_be_taken_back() {
        let mut log = new_log("Reading log", "x");
        for (day, link) in [(10, "[[John 3]]"), (11, "[[John 4]]"), (12, "[[John 3]]")] {
            log = add_entry(&log, link, &at(day, 9, 0)).unwrap();
        }
        // An entry from before the time was dropped still counts.
        log.push_str("\n## 2025-10-09\n\n- 16:20 [[John 3]]\n");
        assert_eq!(dates_read(&log, "[[John 3]]"), ["2025-10-12", "2025-10-10", "2025-10-09"]);
        assert_eq!(dates_read(&log, "[[John 4]]"), ["2025-10-11"]);
        assert!(dates_read(&log, "[[Psalm 1]]").is_empty());
        let taken = remove_entry(&log, "[[John 3]]", &at(12, 20, 0)).unwrap();
        assert_eq!(dates_read(&taken, "[[John 3]]"), ["2025-10-10", "2025-10-09"], "only that day's");
        assert_eq!(remove_entry(&taken, "[[John 3]]", &at(12, 20, 0)), None);
        assert_eq!(remove_bullet("## Reading\n\n- [[John 3]]\n- [[John 4]]\n", "[[John 3]]").unwrap(), "## Reading\n\n- [[John 4]]\n");
        assert_eq!(remove_bullet("x\n", "[[John 3]]"), None);
    }
}
