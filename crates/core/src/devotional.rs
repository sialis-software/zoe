//! The markdown standard for a daily devotional (docs/08): one file for each day, in the vault at
//! `library/<Work>/<MM-DD>.md`, made from the OSIS entries a SWORD devotional keeps (`MM.DD`).
//!
//! ```text
//! ---
//! type: devotional
//! work: Morning and Evening
//! date: 01-01
//! tags: [devotional]
//! ---
//! # January 1
//!
//! ## Morning
//!
//! > *“They did eat of the fruit of the land of Canaan that year.”* — [[Joshua 5:12]]
//!
//! Israel’s weary wanderings …
//! ```

use crate::commentary::{self, Paragraph, markdown, paragraphs};
use scripture::RefRange;

/// One day of a devotional, ready to be written into the vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    /// Where the file goes, relative to the vault: `library/<work>/<MM-DD>.md`.
    pub path: String,
    pub text: String,
}

const MONTHS: [(&str, u8); 12] = [
    ("January", 31),
    ("February", 29),
    ("March", 31),
    ("April", 30),
    ("May", 31),
    ("June", 30),
    ("July", 31),
    ("August", 31),
    ("September", 30),
    ("October", 31),
    ("November", 30),
    ("December", 31),
];

/// The month and day a key `MM.DD` names, if it names one that exists (February 29 does).
fn date_of(key: &str) -> Option<(u8, u8)> {
    let (month, day) = key.trim().split_once('.')?;
    if month.len() != 2 || day.len() != 2 {
        return None;
    }
    let (month, day): (u8, u8) = (month.parse().ok()?, day.parse().ok()?);
    let (_, days) = MONTHS.get(usize::from(month).checked_sub(1)?)?;
    (day >= 1 && day <= *days).then_some((month, day))
}

/// A work's name as a folder name: without what a file name cannot hold.
pub fn folder_name(work: &str) -> String {
    let cleaned: String = work.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.trim_matches('.').trim().to_string()
}

/// A value for frontmatter, quoted when it would not read back as it is.
fn scalar(value: &str) -> String {
    let plain = !value.is_empty()
        && !value.starts_with(['"', '\'', '[', '{', '#', '&', '*', '!', '|', '>', '%', '@', '`', ' '])
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(' ');
    if plain { value.to_string() } else { format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")) }
}

/// The words of some markup, in one line.
fn words(raw: &str) -> String {
    let text: Vec<String> = paragraphs(raw).iter().map(|p| p.iter().map(|s| s.text.as_str()).collect::<String>()).collect();
    text.join(" ")
}

/// A section's heading: its title without the date the devotional repeats in it.
fn heading_of(title: &str, date: &str) -> String {
    let title = title.trim();
    let title = title.strip_suffix(date).map(|t| t.trim_end().trim_end_matches(',').trim_end()).unwrap_or(title);
    title.to_string()
}

/// Where a part of an entry begins: the sections of the entry, each as its title and the markup
/// after the title.
fn sections(raw: &str) -> Vec<String> {
    let mut starts = Vec::new();
    let mut from = 0;
    while let Some(at) = raw[from..].find("<div").map(|i| i + from) {
        let Some(end) = raw[at..].find('>').map(|i| i + at) else { break };
        let tag = &raw[at..end];
        if tag.contains("type=\"section\"") && !tag.ends_with('/') && !tag.contains(" sID=") && !tag.contains(" eID=") {
            starts.push(at);
        }
        from = end;
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &at)| raw[at..starts.get(i + 1).copied().unwrap_or(raw.len())].to_string())
        .collect()
}

/// The title of some markup and the markup without it.
fn split_title(raw: &str) -> (Option<String>, String) {
    let Some(open) = raw.find("<title") else { return (None, raw.to_string()) };
    let Some(close) = raw[open..].find("</title>").map(|i| i + open) else { return (None, raw.to_string()) };
    let inner_start = raw[open..].find('>').map_or(open, |i| open + i + 1);
    let title = words(&raw[inner_start..close.max(inner_start)]);
    let rest = format!("{}{}", &raw[..open], &raw[close + "</title>".len()..]);
    ((!title.is_empty()).then_some(title), rest)
}

/// A reference as a link the app's verse parser reads: the words the module used when they name a
/// passage, else the passage its OSIS reference names.
fn link(span: &commentary::Span) -> Option<String> {
    let shown = span.text.trim();
    if RefRange::parse(shown).is_some() {
        return Some(format!("[[{shown}]]"));
    }
    span.passage.as_deref().and_then(commentary::passage_range).map(|range| format!("[[{range}]]"))
}

/// What a section's paragraphs are as markdown. The key verse, which comes first as the quotation
/// and then the reference, is a blockquote.
fn body(mut paras: Vec<Paragraph>) -> Vec<String> {
    let passage_only = |p: &Paragraph| !p.is_empty() && p.iter().all(|s| s.passage.is_some() || s.text.trim().is_empty());
    let mut out = Vec::new();
    // The quotation and then the reference on a line of its own (`<lb/>`), or both in one paragraph.
    let verse_len = match paras.as_slice() {
        [quote, refs, ..] if passage_only(refs) && !passage_only(quote) && quote.iter().all(|s| s.passage.is_none()) => 2,
        [one, ..] if one.iter().any(|s| s.passage.is_some()) && one.iter().any(|s| s.passage.is_none() && s.italic) => 1,
        _ => 0,
    };
    if verse_len > 0 {
        let rest = paras.split_off(verse_len);
        let (quote, refs): (Paragraph, Paragraph) = paras.into_iter().flatten().partition(|s| s.passage.is_none());
        let links: Vec<String> = refs
            .iter()
            .filter_map(|s| link(s).or_else(|| Some(commentary::escape(s.text.trim()))))
            .collect();
        let mut line = format!("> {}", markdown(&quote).trim_start_matches('\\'));
        if !links.is_empty() {
            line.push_str(" \u{2014} ");
            line.push_str(&links.join("; "));
        }
        out.push(line);
        paras = rest;
    }
    out.extend(paras.iter().map(markdown));
    out
}

/// One day of a devotional as markdown. `work` names the devotional, `key` is the entry's key
/// (`MM.DD`) and `raw` the entry's OSIS. `None` when the key is not a day of the year, or the
/// entry has no text.
pub fn day_markdown(work: &str, key: &str, raw: &str) -> Option<Day> {
    let (month, day) = date_of(key)?;
    let name = MONTHS[usize::from(month) - 1].0;
    let date = format!("{name} {day}");
    let mut parts: Vec<(String, Vec<String>)> = Vec::new();
    let found = sections(raw);
    if found.is_empty() {
        // An entry that is not divided: one section, named by its title if it has one.
        let (title, rest) = split_title(raw);
        let heading = title.map(|t| heading_of(&t, &date)).filter(|t| !t.is_empty()).unwrap_or_else(|| "Today".to_string());
        parts.push((heading, body(paragraphs(&rest))));
    }
    for section in found {
        let (title, rest) = split_title(&section);
        let heading = title.map(|t| heading_of(&t, &date)).filter(|t| !t.is_empty()).unwrap_or_else(|| "Today".to_string());
        parts.push((heading, body(paragraphs(&rest))));
    }
    if parts.iter().all(|(_, paras)| paras.is_empty()) {
        return None;
    }
    let folder = folder_name(work);
    let mut text = format!("---\ntype: devotional\nwork: {}\ndate: {month:02}-{day:02}\ntags: [devotional]\n---\n# {date}\n", scalar(work.trim()));
    for (heading, paras) in parts {
        text.push_str(&format!("\n## {}\n", commentary::escape(&heading)));
        for p in paras {
            text.push('\n');
            text.push_str(&p);
            text.push('\n');
        }
    }
    Some(Day { path: format!("library/{folder}/{month:02}-{day:02}.md"), text })
}

/// Every day of a devotional that can be written: the entries as `(key, raw)`, those that are not
/// days of the year left out.
pub fn import(work: &str, entries: impl IntoIterator<Item = (String, String)>) -> Vec<Day> {
    entries.into_iter().filter_map(|(key, raw)| day_markdown(work, &key, &raw)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter;
    use crate::markdown::{LinkTarget, find_links};

    const JAN_1: &str = "<div type=\"entry\" osisID=\"01.01\"><div type=\"section\" osisID=\"01.01.am\"><title>Morning, January 1</title><p><hi type=\"italic\">\u{201c}They did eat of the fruit.\u{201d}</hi><lb/><reference osisRef=\"Bible:Josh.5.12\">Joshua 5:12</reference></p><p>Israel\u{2019}s weary <hi type=\"italic\">wanderings</hi> were over, *not* a [list].</p></div><div type=\"section\" osisID=\"01.01.pm\"><title>Evening, January 1</title><p><hi type=\"italic\">\u{201c}Glad.\u{201d}</hi><lb/><reference osisRef=\"Bible:Song.1.4\">Song of Solomon 1:4</reference></p><p>We will.</p></div></div>";

    #[test]
    fn a_day_is_a_file_with_a_heading_for_each_section_and_the_key_verse_quoted() {
        let day = day_markdown("Morning and Evening", "01.01", JAN_1).unwrap();
        assert_eq!(day.path, "library/Morning and Evening/01-01.md");
        let expected = "---\ntype: devotional\nwork: Morning and Evening\ndate: 01-01\ntags: [devotional]\n---\n# January 1\n\n## Morning\n\n> *\u{201c}They did eat of the fruit.\u{201d}* \u{2014} [[Joshua 5:12]]\n\nIsrael\u{2019}s weary *wanderings* were over, \\*not\\* a \\[list\\].\n\n## Evening\n\n> *\u{201c}Glad.\u{201d}* \u{2014} [[Song of Solomon 1:4]]\n\nWe will.\n";
        assert_eq!(day.text, expected);
    }

    #[test]
    fn the_file_reads_back_as_a_devotional_with_links_the_app_follows() {
        let day = day_markdown("Morning and Evening", "02.29", JAN_1).unwrap();
        let parsed = frontmatter::parse(&day.text);
        assert_eq!(parsed.front.kind.as_deref(), Some("devotional"));
        assert_eq!(parsed.front.tags, ["devotional"]);
        assert_eq!(day.path, "library/Morning and Evening/02-29.md");
        let links: Vec<_> = day.text.lines().flat_map(find_links).collect();
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|l| matches!(l.target, LinkTarget::Verse { .. })));
    }

    #[test]
    fn a_reference_the_parser_cannot_read_falls_back_to_its_osis_and_then_to_plain_text() {
        let entry = |reference: &str| format!("<div type=\"section\"><title>Morning, March 3</title><p><hi type=\"italic\">Quote.</hi><lb/>{reference}</p></div>");
        let text = |reference: &str| day_markdown("W", "03.03", &entry(reference)).unwrap().text;
        assert!(text("<reference osisRef=\"Bible:Ps.23.1\">Psalm 23:1, 2</reference>").contains("\u{2014} [[Psalms 23:1]]"));
        assert!(text("<reference osisRef=\"nonsense\">Somewhere</reference>").contains("*Quote.* \u{2014} Somewhere"));
    }

    #[test]
    fn an_entry_without_sections_is_one_section() {
        let titled = day_markdown("W", "12.25", "<div type=\"entry\"><title>Christmas, December 25</title><p>Glory.</p></div>").unwrap();
        assert!(titled.text.contains("# December 25\n\n## Christmas\n\nGlory.\n"), "{}", titled.text);
        let plain = day_markdown("W", "12.25", "Glory to God.").unwrap();
        assert!(plain.text.contains("## Today\n\nGlory to God.\n"));
    }

    #[test]
    fn keys_that_are_not_days_are_left_out() {
        for key in ["13.01", "00.10", "02.30", "04.31", "1.1", "01-01", "Grace", "01.01.am", ""] {
            assert_eq!(day_markdown("W", key, JAN_1), None, "{key}");
        }
        assert!(day_markdown("W", "02.29", JAN_1).is_some());
        assert_eq!(day_markdown("W", "01.01", "<div/>"), None);
        let days = import("W", [("01.01".to_string(), JAN_1.to_string()), ("zzz".to_string(), JAN_1.to_string())]);
        assert_eq!(days.len(), 1);
    }

    #[test]
    fn a_works_name_is_a_safe_folder_and_a_safe_value() {
        assert_eq!(folder_name("Spurgeon: Morning/Evening?"), "Spurgeon MorningEvening");
        assert_eq!(folder_name(".. x"), "x");
        let day = day_markdown("Faith: a day", "01.01", JAN_1).unwrap();
        assert!(day.text.contains("work: \"Faith: a day\"\n"));
        assert_eq!(day.path, "library/Faith a day/01-01.md");
    }

    #[test]
    fn verse_in_an_entry_keeps_its_lines_as_paragraphs() {
        let raw = "<div type=\"section\"><title>Morning, February 29</title><p>Before.</p><lg>  <l>\u{201c}Law and terrors</l>  <l>All the while</l></lg><p>After.</p></div>";
        let day = day_markdown("W", "02.29", raw).unwrap();
        assert!(day.text.contains("Before.\n\n\u{201c}Law and terrors\n\nAll the while\n\nAfter.\n"), "{}", day.text);
    }
}
