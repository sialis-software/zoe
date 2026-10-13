//! Markdown dialect helpers (docs/02 §3). Line-oriented so editor highlighting can reuse them.

use scripture::RefRange;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    /// `[[John 3:16]]` or `[[John 3:16@KJV]]`.
    Verse { range: RefRange, module: Option<String> },
    /// `[[sword:StrongsGreek/G5547]]`.
    Sword { module: String, key: String },
    /// `[[Nicodemus]]`, resolved against note names.
    Note(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WikiLink {
    /// Byte range within the line, including `[[`/`]]` (and a leading `!` for embeds).
    pub span: Range<usize>,
    pub target: LinkTarget,
    pub display: Option<String>,
    pub embed: bool,
}

impl WikiLink {
    pub fn label(&self) -> String {
        if let Some(d) = &self.display {
            return d.clone();
        }
        match &self.target {
            LinkTarget::Verse { range, .. } => range.to_string(),
            LinkTarget::Sword { module, key } => format!("{module}/{key}"),
            LinkTarget::Note(n) => n.clone(),
        }
    }
}

/// Find all `[[…]]` links on one line. Unclosed or empty links are ignored.
pub fn find_links(line: &str) -> Vec<WikiLink> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = line[from..].find("[[").map(|i| i + from) {
        let Some(close) = line[open + 2..].find("]]").map(|i| i + open + 2) else { break };
        let inner = &line[open + 2..close];
        let embed = line[..open].ends_with('!');
        let start = if embed { open - 1 } else { open };
        if !inner.trim().is_empty() && !inner.contains("[[") {
            let (target, display) = match inner.split_once('|') {
                Some((t, d)) => (t.trim(), Some(d.trim().to_string())),
                None => (inner.trim(), None),
            };
            out.push(WikiLink { span: start..close + 2, target: classify(target), display, embed });
        }
        from = close + 2;
    }
    out
}

fn classify(target: &str) -> LinkTarget {
    if let Some(rest) = target.strip_prefix("sword:") {
        let (module, key) = rest.split_once('/').unwrap_or((rest, ""));
        return LinkTarget::Sword { module: module.into(), key: key.into() };
    }
    let (ref_part, module) = match target.split_once('@') {
        Some((r, m)) => (r.trim(), Some(m.trim().to_string())),
        None => (target, None),
    };
    match RefRange::parse(ref_part) {
        Some(range) => LinkTarget::Verse { range, module },
        None => LinkTarget::Note(target.to_string()),
    }
}

/// Scripture references written in plain prose ("See Mrk 9:29; Luk 19:10"), with their byte
/// ranges. Needs a book, a chapter and a verse (`John 3:16`, `1 Cor 13:4-7`) so that ordinary
/// words and numbers are left alone.
pub fn find_refs(text: &str) -> Vec<(Range<usize>, RefRange)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let boundary = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if !boundary || !(bytes[i].is_ascii_alphanumeric()) {
            i += 1;
            continue;
        }
        // Optional book number ("1 Cor", "2Tim"), then the name.
        let mut j = i;
        if bytes[j].is_ascii_digit() {
            j += 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
        }
        let name_start = j;
        while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j - name_start < 2 {
            i += 1;
            continue;
        }
        if bytes.get(j) == Some(&b'.') {
            j += 1;
        }
        let book_end = j;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        let digits = |mut k: usize| {
            let from = k;
            while k < bytes.len() && bytes[k].is_ascii_digit() {
                k += 1;
            }
            (k > from).then_some(k)
        };
        let Some(chapter_end) = digits(j) else {
            i = book_end.max(i + 1);
            continue;
        };
        if bytes.get(chapter_end) != Some(&b':') {
            i = book_end.max(i + 1);
            continue;
        }
        let Some(mut end) = digits(chapter_end + 1) else {
            i = book_end.max(i + 1);
            continue;
        };
        // A range: "3:16-18" (hyphen or en dash).
        let rest = &text[end..];
        if let Some(after) = rest.strip_prefix('-').or_else(|| rest.strip_prefix('–'))
            && let Some(last) = digits(end + (rest.len() - after.len()))
        {
            end = last;
        }
        match RefRange::parse(&text[i..end].replace('–', "-")) {
            Some(range) => {
                out.push((i..end, range));
                i = end;
            }
            None => i = book_end.max(i + 1),
        }
    }
    out
}

/// `text` with the links to a note that was renamed from `old_path` to `new_path` pointing at its
/// new name, or `None` if it had none. A link by name (`[[Old]]`) takes the new name, a link by
/// path (`[[notes/Old]]`) the new path; an alias (`[[Old|that note]]`) and an embed (`![[Old]]`)
/// are kept.
pub fn rename_note_links(text: &str, old_path: &str, new_path: &str) -> Option<String> {
    let bare = |p: &str| p.trim_end_matches(".md").to_string();
    let stem = |p: &str| bare(p).rsplit('/').next().unwrap_or_default().to_string();
    let (old_stem, old_full) = (stem(old_path).to_lowercase(), bare(old_path).to_lowercase());
    let mut changed = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let mut line = line.to_string();
        let mut links = find_links(&line);
        // From the end, so earlier spans stay where they were.
        links.reverse();
        for link in links {
            let LinkTarget::Note(name) = &link.target else { continue };
            let name = name.trim_end_matches(".md").to_lowercase();
            let replacement = if name == old_full {
                bare(new_path)
            } else if name == old_stem {
                stem(new_path)
            } else {
                continue;
            };
            let embed = if link.embed { "!" } else { "" };
            let alias = link.display.as_ref().map_or(String::new(), |d| format!("|{d}"));
            line.replace_range(link.span.clone(), &format!("{embed}[[{replacement}{alias}]]"));
            changed = true;
        }
        out.push_str(&line);
    }
    changed.then_some(out)
}

/// Resolve a `[[Note name]]` against note paths by case-insensitive file stem or full path.
pub fn resolve_note<'a>(paths: &'a [String], name: &str) -> Option<&'a String> {
    let want = name.trim().trim_end_matches(".md").to_lowercase();
    paths.iter().find(|p| {
        let no_ext = p.trim_end_matches(".md").to_lowercase();
        no_ext == want || no_ext.rsplit('/').next() == Some(want.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scripture::Book;

    #[test]
    fn finds_verse_note_and_sword_links() {
        let line = "See [[John 3:16]], [[Nicodemus|him]] and ![[Ps 23@KJV]] or [[sword:StrongsGreek/G25]].";
        let links = find_links(line);
        assert_eq!(links.len(), 4);
        assert_eq!(&line[links[0].span.clone()], "[[John 3:16]]");
        assert!(matches!(&links[0].target, LinkTarget::Verse { range, module: None } if range.book == Book::JOHN));
        assert_eq!(links[1].target, LinkTarget::Note("Nicodemus".into()));
        assert_eq!(links[1].label(), "him");
        assert!(links[2].embed);
        assert_eq!(&line[links[2].span.clone()], "![[Ps 23@KJV]]");
        assert!(matches!(&links[2].target, LinkTarget::Verse { module: Some(m), .. } if m == "KJV"));
        assert_eq!(
            links[3].target,
            LinkTarget::Sword { module: "StrongsGreek".into(), key: "G25".into() }
        );
    }

    #[test]
    fn finds_plain_references_in_prose() {
        let text = "Many authorities insert v. 21. See Mrk 9:29; Luk 19:10-12 and 1 Cor 13:4–7, also Joh 3.";
        let found: Vec<(&str, String)> =
            find_refs(text).into_iter().map(|(r, range)| (&text[r], range.osis())).collect();
        assert_eq!(
            found,
            [
                ("Mrk 9:29", "Mark.9.29".to_string()),
                ("Luk 19:10-12", "Luke.19.10-Luke.19.12".to_string()),
                ("1 Cor 13:4–7", "1Cor.13.4-1Cor.13.7".to_string()),
            ]
        );
    }

    #[test]
    fn plain_words_and_numbers_are_not_references() {
        for text in ["in 1996, at 10:30 we met", "Some read v. 21 here", "Chapter 3:16 alone", "ab 3:4x", "Nothing 3:4"] {
            let found = find_refs(text);
            assert!(found.iter().all(|(_, r)| r.book.name() != "Nothing"), "{text}: {found:?}");
        }
        assert!(find_refs("at 10:30 we met").is_empty());
        assert!(find_refs("see John 3:17-16").iter().all(|(r, _)| r.end <= 15));
    }

    #[test]
    fn renaming_a_note_updates_the_links_to_it_and_only_those() {
        let text = "See [[Old name]], [[old name|that note]], ![[Old name]] and [[notes/Old name]].\nNot [[Other]] or [[John 3:16]].\n";
        let out = rename_note_links(text, "notes/Old name.md", "notes/Fresh.md").unwrap();
        assert_eq!(
            out,
            "See [[Fresh]], [[Fresh|that note]], ![[Fresh]] and [[notes/Fresh]].\nNot [[Other]] or [[John 3:16]].\n"
        );
        assert_eq!(rename_note_links("Nothing here [[Other]]\n", "notes/Old name.md", "notes/Fresh.md"), None);
    }

    #[test]
    fn ignores_malformed() {
        assert!(find_links("[[]] and [[unclosed").is_empty());
    }

    #[test]
    fn resolves_notes() {
        let paths = vec!["notes/Nicodemus.md".to_string(), "prayers/list.md".to_string()];
        assert_eq!(resolve_note(&paths, "nicodemus"), Some(&paths[0]));
        assert_eq!(resolve_note(&paths, "prayers/list"), Some(&paths[1]));
        assert_eq!(resolve_note(&paths, "missing"), None);
    }
}
