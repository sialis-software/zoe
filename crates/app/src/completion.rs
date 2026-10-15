//! What can be offered while a link is being written: after `[[`, the notes; after `[[%`, the entries of the
//! Strong's dictionaries (by their numbers); after `[[@`, the passages (the book, then the chapter, then the verse).
//! The text and the caret are all it needs: the editor shows the list and the note tab applies a choice.

use crate::shared::Shared;
use scripture::Book;
use study_core::ModuleId;

/// One thing that can be chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub label: String,
    /// Said quietly beside it.
    pub detail: String,
    /// What takes the place of what was typed after the marker.
    pub insert: String,
    /// Whether choosing it finishes the link (the closing `]]` is gone past) or goes on to the next step.
    pub close: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    /// Where what is replaced begins (just after `[[`, or after its marker).
    pub from: usize,
    pub items: Vec<Item>,
    pub selected: usize,
}

/// The most offered (the list scrolls).
const LIMIT: usize = 200;

/// The completions for a link being written at `caret` in `text`, if one is. `module` is the Bible whose chapters and
/// verses the passages are counted in.
pub fn complete(sh: &Shared, text: &str, caret: usize, module: Option<&ModuleId>) -> Option<Completion> {
    if !text.is_char_boundary(caret) {
        return None;
    }
    let before = &text[..caret];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = &before[line_start..];
    let open = line.rfind("[[")?;
    let inner = &line[open + 2..];
    // Past the target (an alias begins, or the link ended): nothing to complete.
    if inner.contains([']', '|', '[']) {
        return None;
    }
    let from = line_start + open + 2;
    let (from, items) = match inner.chars().next() {
        Some('%') => (from + 1, strongs(sh, &inner[1..])),
        Some('@') => (from + 1, passages(sh, &inner[1..], module)),
        _ => (from, notes(sh, inner)),
    };
    (!items.is_empty()).then_some(Completion { from, items, selected: 0 })
}

/// The notes whose names hold what was typed (those that begin with it first): the notes the note tab's search bar
/// shows (the ordinary ones, then the margin notes; the library only when what is typed looks for it), by the names
/// it shows them under.
fn notes(sh: &Shared, typed: &str) -> Vec<Item> {
    let needle = typed.trim().to_lowercase();
    let stem = |p: &str| p.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string();
    let all = sh.note_paths();
    let mut found: Vec<(bool, usize, String, String, String)> = Vec::new();
    for (rank, group) in crate::note_tab::groups(sh, typed.trim(), &Default::default()).into_iter().enumerate() {
        for (path, label) in group.notes {
            let name = label.to_lowercase();
            // (The bar also finds what only mentions the words: a link is to a name.)
            if !name.contains(&needle) && !stem(&path).to_lowercase().contains(&needle) {
                continue;
            }
            found.push((name.starts_with(&needle), rank, path, label, group.label.clone()));
        }
    }
    // Names that begin with it first, then the bar's own order of groups and names.
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found
        .into_iter()
        .take(LIMIT)
        .map(|(_, _, path, label, group)| {
            let name = stem(&path);
            // The name alone, when no other note has it (and it is the note's own name).
            let alone = all.iter().filter(|p| stem(p).eq_ignore_ascii_case(&name)).count() == 1 && !path.starts_with("margins/");
            let folder = path.rsplit_once('/').map_or("", |(d, _)| d);
            // (The ordinary notes are in `notes/`: that is not worth saying.)
            let detail = if group == "Notes" { if folder == "notes" { String::new() } else { folder.to_string() } } else { group };
            Item { label, detail, insert: if alone { name } else { path.trim_end_matches(".md").to_string() }, close: true }
        })
        .collect()
}

/// The entries of the Strong's dictionaries whose numbers begin with what was typed (`G`, `H25`, `G2316`).
fn strongs(sh: &Shared, typed: &str) -> Vec<Item> {
    let typed = typed.trim().to_uppercase();
    let hint = |letter: &str, what: &str| Item { label: letter.into(), detail: what.into(), insert: letter.into(), close: false };
    let Some(letter) = typed.chars().next() else {
        return vec![hint("G", "Greek"), hint("H", "Hebrew")];
    };
    let digits = &typed[1..];
    let max: u32 = match letter {
        'G' => 5624,
        'H' => 8674,
        _ => return Vec::new(),
    };
    if !digits.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 4 {
        return Vec::new();
    }
    // The numbers that begin with the digits: that one, then those with one more digit, and so on.
    let prefix: u32 = digits.parse().unwrap_or(0);
    let mut numbers: Vec<u32> = Vec::new();
    if digits.is_empty() {
        numbers.extend(1..=max);
    } else {
        let mut scale = 1;
        while scale <= 1000 {
            let lo = prefix * scale;
            numbers.extend((lo..lo + scale).filter(|n| *n >= 1 && *n <= max));
            scale *= 10;
        }
    }
    let mut out = Vec::new();
    for n in numbers {
        let key = format!("{letter}{n}");
        let Some(entry) = sh.backend.lexicon_entry(&key) else { continue };
        let said: String = entry.definition.chars().take(48).collect();
        let detail = [entry.lemma.as_str(), entry.transliteration.as_str(), said.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("  ");
        out.push(Item { label: key.clone(), detail, insert: key, close: true });
        if out.len() == LIMIT {
            break;
        }
    }
    out
}

/// The passages that what was typed after the `@` is the beginning of: books, then the chapters of the book, then its
/// verses (and, after a second `@`, the Bibles a passage can be pinned to).
fn passages(sh: &Shared, typed: &str, module: Option<&ModuleId>) -> Vec<Item> {
    let typed = typed.trim_start();
    let modules = sh.backend.modules();
    let module = module.cloned().or_else(|| modules.first().map(|m| m.id.clone()));
    // After a second `@`: which Bible.
    if let Some((reference, wanted)) = typed.split_once('@') {
        let wanted = wanted.to_lowercase();
        return modules
            .iter()
            .filter(|m| m.abbreviation.to_lowercase().starts_with(&wanted))
            .take(LIMIT)
            .map(|m| Item { label: m.abbreviation.clone(), detail: String::new(), insert: format!("{reference}@{}", m.abbreviation), close: true })
            .collect();
    }
    let Some(module) = module else { return Vec::new() };
    let chapters = sh.backend.available_chapters(&module);
    let mut books: Vec<Book> = Vec::new();
    for (book, _) in &chapters {
        if books.last() != Some(book) {
            books.push(*book);
        }
    }
    let lower = typed.to_lowercase();
    // A whole book name typed, with something after it: on to its chapters and verses.
    let named = books.iter().copied().find(|b| {
        let name = b.name().to_lowercase();
        lower.starts_with(&name) && lower[name.len()..].starts_with(' ')
    });
    if let Some(book) = named {
        let rest = typed[book.name().len()..].trim_start();
        let verses_in = |chapter: u16| {
            sh.backend.chapter(&module, book, chapter).ok().and_then(|c| c.canonical_verses().keys().max().copied()).unwrap_or(0)
        };
        if rest.contains('-') {
            return Vec::new();
        }
        if let Some((chapter, verse)) = rest.split_once(':') {
            let (Ok(chapter), true) = (chapter.trim().parse::<u16>(), verse.bytes().all(|b| b.is_ascii_digit())) else { return Vec::new() };
            let mut out = Vec::new();
            if verse.is_empty() {
                out.push(Item { label: format!("{} {chapter}", book.name()), detail: "the whole chapter".into(), insert: format!("{} {chapter}", book.name()), close: true });
            }
            for v in (1..=verses_in(chapter)).filter(|v| v.to_string().starts_with(verse)).take(LIMIT) {
                let label = format!("{} {chapter}:{v}", book.name());
                out.push(Item { label: label.clone(), detail: String::new(), insert: label, close: true });
            }
            out.truncate(LIMIT);
            return out;
        }
        if !rest.bytes().all(|b| b.is_ascii_digit()) {
            return Vec::new();
        }
        return chapters
            .iter()
            .filter(|(b, c)| *b == book && c.to_string().starts_with(rest))
            .take(LIMIT)
            .map(|(_, c)| Item {
                label: format!("{} {c}", book.name()),
                detail: format!("{} verses", verses_in(*c)),
                insert: format!("{} {c}:", book.name()),
                close: false,
            })
            .collect();
    }
    // A book still being named.
    let mut found: Vec<(bool, Book)> = books
        .iter()
        .filter_map(|b| {
            let name = b.name().to_lowercase();
            name.contains(&lower).then_some((name.starts_with(&lower), *b))
        })
        .collect();
    found.sort_by_key(|(starts, _)| !*starts);
    found
        .into_iter()
        .take(LIMIT)
        .map(|(_, b)| {
            let n = chapters.iter().filter(|(x, _)| *x == b).count();
            Item { label: b.name().to_string(), detail: format!("{n} chapter{}", if n == 1 { "" } else { "s" }), insert: format!("{} ", b.name()), close: false }
        })
        .collect()
}

/// A link just written (the caret is past its closing `]]`) put right where it is only wrong in its capitals or its spacing:
/// a passage in its book's own spelling (`[[@john 3:16]]` is `[[@John 3:16]]`), a Strong's number in capitals
/// (`[[%g25]]`), a note by the name it has (`[[nicodemus]]` is `[[Nicodemus]]`), a Bible by its abbreviation, and no
/// spaces about the target. Gives the part of the text to replace (the target, up to any alias) and what to put there.
pub fn correct(sh: &Shared, text: &str, caret: usize) -> Option<(std::ops::Range<usize>, String)> {
    if !text.is_char_boundary(caret) || !text[..caret].ends_with("]]") {
        return None;
    }
    let before = &text[..caret - 2];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let open = before[line_start..].rfind("[[")? + line_start;
    let inner = &before[open + 2..];
    if inner.contains(['[', ']']) || inner.trim().is_empty() {
        return None;
    }
    // The target is what comes before an alias.
    let target_len = inner.find('|').unwrap_or(inner.len());
    let (target, from) = (&inner[..target_len], open + 2);
    let typed = target.trim();
    let fixed = match typed.chars().next() {
        Some('%') => {
            let number = typed[1..].replace(' ', "").to_uppercase();
            format!("%{number}")
        }
        Some('@') => {
            let (reference, pin) = match typed[1..].split_once('@') {
                Some((r, m)) => (r, Some(m.trim())),
                None => (&typed[1..], None),
            };
            // (`john3:16` is `john 3:16`: a space where letters meet digits.)
            let mut spaced = String::new();
            for ch in reference.trim().chars() {
                if ch.is_ascii_digit() && spaced.chars().last().is_some_and(char::is_alphabetic) {
                    spaced.push(' ');
                }
                spaced.push(ch);
            }
            let range = scripture::RefRange::parse(&spaced)?;
            let pin = pin.map(|m| sh.backend.modules().into_iter().find(|i| i.abbreviation.eq_ignore_ascii_case(m)).map_or_else(|| m.to_string(), |i| i.abbreviation));
            match pin {
                Some(module) => format!("@{range}@{module}"),
                None => format!("@{range}"),
            }
        }
        _ => {
            // A note by the name it is called, when what was typed differs from it only in its capitals.
            let paths = sh.note_paths();
            match study_core::markdown::resolve_note(&paths, typed) {
                Some(path) => {
                    let name = if typed.contains('/') { path.trim_end_matches(".md").to_string() } else { path.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string() };
                    if name.eq_ignore_ascii_case(typed.trim_end_matches(".md")) { name } else { typed.to_string() }
                }
                None => typed.to_string(),
            }
        }
    };
    (fixed != target).then_some((from..from + target_len, fixed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use study_core::mock::MockBackend;

    fn shared() -> Shared {
        let mut sh = Shared::new(Box::new(MockBackend), Default::default(), None);
        sh.notes.insert("notes/Nicodemus.md".into(), "x\n".into());
        sh.notes.insert("notes/Memory verses.md".into(), "x\n".into());
        sh.notes.insert("old/Nicodemus.md".into(), "x\n".into());
        sh
    }

    fn at_end(sh: &Shared, text: &str) -> Option<Completion> {
        complete(sh, text, text.len(), None)
    }

    fn labels(c: &Option<Completion>) -> Vec<String> {
        c.as_ref().map_or_else(Vec::new, |c| c.items.iter().map(|i| i.label.clone()).collect())
    }

    #[test]
    fn after_two_brackets_the_notes_are_offered_and_narrowed_by_what_is_typed() {
        let sh = shared();
        let all = at_end(&sh, "See [[");
        assert_eq!(all.as_ref().map(|c| c.from), Some(6));
        assert!(labels(&all).contains(&"Memory verses".to_string()) && labels(&all).iter().all(|l| !l.starts_with("templates")));
        assert_eq!(labels(&at_end(&sh, "[[mem")), ["Memory verses"]);
        // Two notes of one name: the path says which.
        let two = at_end(&sh, "[[nico").unwrap();
        assert_eq!(two.items.len(), 2);
        assert!(two.items.iter().all(|i| i.insert.ends_with("/Nicodemus")));
        // Nothing offered once the target is done, or with no link open.
        assert!(at_end(&sh, "[[Nicodemus|him").is_none() && at_end(&sh, "[[a]] mem").is_none() && at_end(&sh, "no link").is_none());
        // A note named like a passage is a note.
        let mut sh = shared();
        sh.notes.insert("notes/John 3.md".into(), "x\n".into());
        assert_eq!(labels(&at_end(&sh, "[[john")), ["John 3"]);
    }

    #[test]
    fn after_a_percent_the_strongs_entries_are_offered_by_their_numbers() {
        let sh = shared();
        assert_eq!(labels(&at_end(&sh, "[[%")), ["G", "H"]);
        let found = at_end(&sh, "[[%G2316").unwrap();
        assert_eq!(found.items[0].label, "G2316");
        assert!(found.items[0].close && found.items[0].detail.contains("θεός"), "{:?}", found.items[0]);
        assert_eq!(found.from, 3, "what is replaced starts after the marker");
        assert!(at_end(&sh, "[[%G99999").is_none() && at_end(&sh, "[[%Gospel").is_none() && at_end(&sh, "[[%X1").is_none());
        assert!(labels(&at_end(&sh, "[[%g231")).iter().all(|l| l.starts_with("G231")), "case does not matter");
    }

    #[test]
    fn after_an_at_sign_a_passage_is_built_book_then_chapter_then_verse() {
        let sh = shared();
        let books = at_end(&sh, "[[@jo");
        assert_eq!(labels(&books), ["John"]);
        assert_eq!(books.as_ref().unwrap().items[0].insert, "John ");
        assert!(!books.unwrap().items[0].close, "on to the chapter");
        assert_eq!(labels(&at_end(&sh, "[[@Psalms ")), ["Psalms 23"]);
        assert_eq!(at_end(&sh, "[[@Psalms ").unwrap().items[0].insert, "Psalms 23:");
        let verses = at_end(&sh, "[[@Psalms 23:").unwrap();
        assert_eq!(verses.items[0].label, "Psalms 23");
        assert_eq!(verses.items[0].detail, "the whole chapter");
        assert!(verses.items[1..].iter().all(|i| i.label.starts_with("Psalms 23:") && i.close));
        assert_eq!(labels(&at_end(&sh, "[[@Psalms 23:2")).first().map(String::as_str), Some("Psalms 23:2"));
        assert!(at_end(&sh, "[[@Psalms 23:2-4").is_none(), "a range is typed by hand");
        // After a second @, the Bible.
        let bibles = at_end(&sh, "[[@John 3:16@").unwrap();
        assert!(bibles.items.iter().all(|i| i.insert.starts_with("John 3:16@") && i.close));
    }

    #[test]
    fn a_link_just_written_is_put_right_in_its_capitals_and_spacing() {
        let sh = shared();
        let fix = |text: &str| correct(&sh, text, text.len()).map(|(range, new)| format!("{}{new}{}", &text[..range.start], &text[range.end..]));
        assert_eq!(fix("see [[@john 3:16]]").as_deref(), Some("see [[@John 3:16]]"));
        assert_eq!(fix("[[ @jOHN3:16 |this verse]]").as_deref(), Some("[[@John 3:16|this verse]]"));
        assert_eq!(fix("[[%g25]]").as_deref(), Some("[[%G25]]"));
        assert_eq!(fix("[[% h 215]]").as_deref(), Some("[[%H215]]"));
        assert_eq!(fix("[[nicodemus]]").as_deref(), Some("[[Nicodemus]]"));
        assert_eq!(fix("[[memory VERSES|those]]").as_deref(), Some("[[Memory verses|those]]"));
        assert_eq!(fix("[[ Nicodemus ]]").as_deref(), Some("[[Nicodemus]]"));
        // As they should be, or not to be guessed at: left as they are.
        assert_eq!(fix("[[@John 3:16]]"), None);
        assert_eq!(fix("[[Nicodemus]]"), None);
        assert_eq!(fix("[[A note that is not there]]"), None);
        assert_eq!(fix("[[@Not a passage]]"), None);
        assert_eq!(fix("[[@John 3:16"), None, "not closed");
    }
}
