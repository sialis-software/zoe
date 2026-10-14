//! The reading log: a note that chapters are added to, as they are read, in the order and at the time
//! they were. It is an ordinary note (open it, link to it, search it): each day is a heading, each
//! chapter a bullet with the time and a link.
//!
//! ```text
//! ## 2025-10-13
//!
//! - 16:20 [[John 3]]
//! - 16:41 [[John 4]]
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

/// `log` with a line for `link` (`[[John 3]]`) read at `now` added to the end, under the day's
/// heading (made if the last heading is another day's). `None` when the last line is already that
/// link: reading the same chapter again at once is not a second line.
pub fn add_entry(log: &str, link: &str, now: &DateTime<Local>) -> Option<String> {
    let last_bullet = log.lines().rev().find(|l| l.trim_start().starts_with("- "));
    if last_bullet.is_some_and(|l| l.trim_end().ends_with(link)) {
        return None;
    }
    let mut out = log.trim_end().to_string();
    let heading = day_heading(now);
    let last_heading = log.lines().rev().find(|l| l.starts_with("## "));
    if last_heading != Some(heading.as_str()) {
        out.push_str(&format!("\n\n{heading}\n"));
    }
    out.push_str(&format!("\n- {} {link}\n", now.format("%H:%M")));
    Some(out)
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
    fn chapters_are_added_under_the_day_in_the_order_read() {
        let log = new_log("Reading log", "2025-10-13T08:00:00");
        let log = add_entry(&log, "[[John 3]]", &at(13, 16, 20)).unwrap();
        let log = add_entry(&log, "[[John 4]]", &at(13, 16, 41)).unwrap();
        let log = add_entry(&log, "[[Psalm 23]]", &at(14, 7, 5)).unwrap();
        assert!(log.starts_with("---\ncreated: 2025-10-13T08:00:00\ntags: [reading-log]\n---\n# Reading log\n"));
        let body = &log[log.find("## 2025-10-13").unwrap()..];
        assert_eq!(body, "## 2025-10-13\n\n- 16:20 [[John 3]]\n- 16:41 [[John 4]]\n\n## 2025-10-14\n\n- 07:05 [[Psalm 23]]\n");
    }

    #[test]
    fn the_same_chapter_again_at_once_is_not_a_second_line_but_after_another_it_is() {
        let log = add_entry(&new_log("Reading log", "x"), "[[John 3]]", &at(13, 9, 0)).unwrap();
        assert_eq!(add_entry(&log, "[[John 3]]", &at(13, 9, 5)), None);
        let log = add_entry(&log, "[[John 4]]", &at(13, 9, 10)).unwrap();
        assert!(add_entry(&log, "[[John 3]]", &at(13, 9, 20)).is_some());
    }

    #[test]
    fn a_day_with_no_heading_yet_in_a_log_somebody_wrote_gets_one() {
        let log = "# My log\n\nSome words.\n";
        let made = add_entry(log, "[[John 3]]", &at(13, 9, 0)).unwrap();
        assert_eq!(made, "# My log\n\nSome words.\n\n## 2025-10-13\n\n- 09:00 [[John 3]]\n");
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
}
