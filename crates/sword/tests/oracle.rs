//! Compares our zText reader with CrossWire's own `mod2imp` on real installed modules.
//! Skipped (with a note) where the module or the tool is not installed.

use scripture::Book;
use std::collections::HashMap;
use std::process::Command;
use sword::Library;

/// `mod2imp` keys look like `Genesis 1:1`; our books are indexed in canon order.
fn dump(name: &str, root: &std::path::Path) -> Option<HashMap<String, String>> {
    let out = Command::new("mod2imp").arg(name).env("SWORD_PATH", root).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut map = HashMap::new();
    let mut key = None;
    let mut body = String::new();
    // SWORD spells some books differently ("I Samuel"); key by canon position instead.
    let (mut book_no, mut last_name) = (0usize, String::new());
    for line in text.lines() {
        if let Some(k) = line.strip_prefix("$$$") {
            let k = match k.rsplit_once(' ') {
                Some((name, loc)) if loc.contains(':') && !name.starts_with('[') => {
                    if name != last_name {
                        book_no += usize::from(!last_name.is_empty());
                        last_name = name.to_string();
                    }
                    format!("{book_no} {loc}")
                }
                _ => k.to_string(),
            };
            let k = k.as_str();
            if let Some(prev) = key.take() {
                map.insert(prev, std::mem::take(&mut body).trim().to_string());
            }
            key = Some(k.to_string());
        } else {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }
    if let Some(prev) = key {
        map.insert(prev, body.trim().to_string());
    }
    Some(map)
}

fn check(name: &str) {
    let lib = Library::open(&sword::test_roots());
    let Some(module) = lib.get(name) else {
        eprintln!("skipped: {name} is not installed");
        return;
    };
    let Some(oracle) = dump(name, &module.root) else {
        eprintln!("skipped: mod2imp is not available");
        return;
    };
    let bible = module.bible().unwrap();
    let (mut checked, mut empty) = (0, 0);
    for (book, chapter) in bible.chapters() {
        for verse in 1..=sword::verse_count(book, chapter) {
            let idx = Book::all().position(|b| b == book).unwrap();
            let key = format!("{idx} {chapter}:{verse}");
            let ours = bible.raw(book, chapter, verse).unwrap().unwrap_or_default();
            let theirs = oracle.get(&key).map(String::as_str).unwrap_or("");
            assert_eq!(ours.trim(), theirs, "{name} {key}");
            if ours.is_empty() { empty += 1 } else { checked += 1 }
        }
    }
    eprintln!("{name}: {checked} verses identical to mod2imp ({empty} empty in both)");
    assert!(checked > 7000, "{name}: only {checked} verses were compared");
}

#[test]
fn asv_matches_mod2imp() {
    check("ASV");
}

#[test]
fn lsb_matches_mod2imp() {
    check("LSB");
}

#[test]
fn every_installed_bible_converts_every_chapter_without_losing_verses() {
    let lib = Library::open(&sword::test_roots());
    for module in lib.modules().filter(|m| m.kind() == sword::Kind::Bible) {
        let Ok(bible) = module.bible() else { continue };
        for (book, chapter) in bible.chapters() {
            let c = bible.chapter(book, chapter).unwrap();
            let verses = c.canonical_verses();
            for v in 1..=sword::verse_count(book, chapter) {
                let raw = bible.raw(book, chapter, v).unwrap();
                if raw.is_some_and(|r| has_text(&r)) {
                    let text = verses.get(&v).unwrap_or_else(|| panic!("{} {book} {chapter}:{v} lost", module.name()));
                    assert!(!text.trim().is_empty(), "{} {book} {chapter}:{v} is blank", module.name());
                }
            }
        }
    }
}

/// Whether anything outside notes and tags remains: footnote-only verses have no text.
fn has_text(raw: &str) -> bool {
    let mut out = String::new();
    let (mut in_tag, mut in_note) = (false, 0);
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("<note") {
            in_note += 1;
        } else if rest.starts_with("</note>") {
            in_note -= 1;
        }
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag && in_note == 0 => out.push(c),
            _ => {}
        }
        rest = &rest[c.len_utf8()..];
    }
    out.chars().any(char::is_alphanumeric)
}

/// Every key of a dictionary, with its body, compared with `mod2imp`.
fn check_lexicon(name: &str) {
    let lib = Library::open(&sword::test_roots());
    let Some(module) = lib.get(name) else { return eprintln!("skipped: {name} is not installed") };
    // SWORD also reads `~/.sword`, and a module of the same name there (a copy of another edition, say) would be read in place
    // of this one: so it is run with a home that has nothing in it, and sees only the folder the module is in.
    let home = tempfile::tempdir().unwrap();
    let ran = Command::new("mod2imp").arg(name).env("SWORD_PATH", &module.root).env("HOME", home.path()).output().ok().filter(|o| o.status.success());
    let Some(text) = ran else {
        return eprintln!("skipped: mod2imp is not available");
    };
    let text = String::from_utf8_lossy(&text.stdout).into_owned();
    let lex = module.lexicon().unwrap();
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(k) = line.strip_prefix("$$$") {
            entries.push((k.trim().to_lowercase(), String::new()));
        } else if let Some((_, body)) = entries.last_mut() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }
    assert!(entries.len() > 5000, "{name}: {} entries", entries.len());
    for (key, body) in &entries {
        let ours = lex.entry(key).unwrap().unwrap_or_else(|| panic!("{name}: {key} missing"));
        // mod2imp renders CRLF line ends and blank lines its own way; the words are what count.
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(norm(&ours), norm(body), "{name} {key}");
    }
    eprintln!("{name}: {} entries identical to mod2imp", entries.len());
}

#[test]
fn strongs_greek_zld_matches_mod2imp() {
    check_lexicon("StrongsGreek");
}

#[test]
fn strongs_hebrew_rawld_matches_mod2imp() {
    check_lexicon("StrongsHebrew");
}

#[test]
fn lsb_dictionaries_match_mod2imp() {
    check_lexicon("LSBG");
    check_lexicon("LSBH");
}
