//! Sample content for testing: a few linked notes and margin marks, so a fresh vault has
//! something to click through. Marks are anchored against the real text of the chosen
//! module, so they resolve in it; a quote the module words differently is skipped.

use crate::Vault;
use scripture::Book;
use std::io;
use study_core::ast::Chapter;
use study_core::frontmatter;
use study_core::margin::{self, Mark, MarkStyle, Slice};

/// One sample mark. `quotes` are tried in order, since translations word things differently.
pub struct SeedMark {
    pub id: &'static str,
    pub book: Book,
    pub chapter: u16,
    pub verse: u16,
    /// Empty means a whole-verse mark.
    pub quotes: &'static [&'static str],
    pub style: MarkStyle,
    pub color: &'static str,
    pub tag: &'static str,
    pub body: &'static str,
}

const MARKS: &[SeedMark] = &[
    SeedMark { id: "s001", book: Book::JOHN, chapter: 3, verse: 16, quotes: &["so loved the world"],
        style: MarkStyle::Highlight, color: "#F5D90A", tag: "promise",
        body: "The heart of the gospel. Learn this one by heart." },
    SeedMark { id: "s002", book: Book::JOHN, chapter: 3, verse: 16, quotes: &["eternal life", "everlasting life"],
        style: MarkStyle::Underline, color: "#4DA3FF", tag: "promise",
        body: "Eternal life begins now, not only after death. Compare [[John 17:3]]." },
    SeedMark { id: "s003", book: Book::JOHN, chapter: 3, verse: 3, quotes: &["born anew", "born again", "born from above"],
        style: MarkStyle::Highlight, color: "#FF7AB6", tag: "prayer",
        body: "\"Again\" can also mean \"from above\". Nicodemus hears the first; Jesus may mean both." },
    SeedMark { id: "s004", book: Book::PSALMS, chapter: 23, verse: 1, quotes: &["is my shepherd"],
        style: MarkStyle::Highlight, color: "#5FD38D", tag: "growth", body: "" },
    SeedMark { id: "s005", book: Book::PSALMS, chapter: 23, verse: 4, quotes: &[],
        style: MarkStyle::Highlight, color: "#B48CFF", tag: "comfort", body: "" },
    SeedMark { id: "s006", book: Book::ROMANS, chapter: 8, verse: 28, quotes: &["work together for good", "work together for good to them"],
        style: MarkStyle::Highlight, color: "#F5D90A", tag: "promise",
        body: "Not that everything is good, but that God works in it all." },
    SeedMark { id: "s007", book: Book::GENESIS, chapter: 1, verse: 1, quotes: &["In the beginning"],
        style: MarkStyle::Underline, color: "#4DA3FF", tag: "command", body: "" },
    SeedMark { id: "s008", book: Book::PHILIPPIANS, chapter: 4, verse: 6, quotes: &["Be anxious for nothing", "In nothing be anxious", "Be careful for nothing"],
        style: MarkStyle::Highlight, color: "#FF7AB6", tag: "prayer",
        body: "Add to the [[Prayer list]] instead of carrying it." },
    SeedMark { id: "s009", book: Book::JOHN, chapter: 1, verse: 1, quotes: &["the Word was God"],
        style: MarkStyle::None, color: "#FFFFFF", tag: "question",
        body: "What does Word (*logos*) carry for a Greek reader? See [[sword:StrongsGreek/G3056]]." },
];

/// Sample notes. `{module}` is the Bible they pin their quotes to.
const NOTES: &[(&str, &str)] = &[
    (
        "notes/Nicodemus.md",
        "---\ntitle: Notes on Nicodemus\ntags: [john, new-birth]\n---\n\n# Nicodemus\n\nHe came by night ([[John 3:2]]). Jesus answers a question he never asked, in [[John 3:3@{module}]]:\n\n![[John 3:3-5@{module}]]\n\n## Questions\n\n- [ ] What does **born again** mean for Nicodemus, a teacher?\n- [x] Compare with [[Num 21:9]] and the bronze serpent.\n- [ ] Look up `agapao` in [[sword:StrongsGreek/G25]].\n\nSee also [[Memory verses]] and [[Prayer list|my prayers]].\n",
    ),
    (
        "notes/Memory verses.md",
        "# Memory verses\n\n1. [[John 3:16]] — learned with the kids, October.\n2. [[Ps 23:1-3@{module}]] — the shepherd psalm.\n3. [[Rom 8:28]]\n\nBack to [[Nicodemus]]. This link goes nowhere yet: [[Unwritten note]].\n",
    ),
    (
        "notes/Creation.md",
        "# Creation\n\n[[Gen 1:1]] opens everything. Compare [[John 1:1-3]]: the Word was there \"in the beginning\" too.\n\n- [ ] Read [[Gen 1]] aloud, slowly\n- [ ] Does [[Ps 33:6]] say the same?\n",
    ),
    (
        "prayers/Prayer list.md",
        "---\ntype: prayer\n---\n# Prayer list\n\n- [ ] Wisdom for my sister's decision  #family  [[Jas 1:5]]\n- [x] Healing for Tom (answered 2026-10-09)\n- [ ] Peace in a busy week — [[Ps 23]], [[Phil 4:6-7]]\n",
    ),
];

pub fn seed_notes(module: &str) -> Vec<(String, String)> {
    let now = frontmatter::now();
    NOTES
        .iter()
        .map(|(p, t)| {
            let text = t.replace("{module}", module);
            // Every note carries when it was made, like any the app creates.
            let edit = frontmatter::update(&text, |f| {
                f.created.get_or_insert_with(|| now.clone());
            });
            (p.to_string(), frontmatter::apply(&text, &edit))
        })
        .collect()
}

/// Mark the seed on `chapter`'s text, or `None` when this module does not word it that way.
fn build(seed: &SeedMark, module: &str, chapter: &Chapter) -> Option<Mark> {
    let canon = chapter.canonical_verses();
    let text = canon.get(&seed.verse)?;
    let (anchor, module) = if seed.quotes.is_empty() {
        (margin::anchor_for_verses(seed.book, seed.chapter, seed.verse, seed.verse), String::new())
    } else {
        let (start, quote) = seed.quotes.iter().find_map(|q| text.find(q).map(|i| (i, *q)))?;
        let slice = Slice { verse: seed.verse, start, end: start + quote.len() };
        (margin::anchor_from_selection(seed.book, seed.chapter, &canon, &[slice])?, module.to_string())
    };
    Some(Mark {
        id: seed.id.into(),
        anchor: anchor.range,
        module,
        quote: anchor.quote,
        prefix: anchor.prefix,
        suffix: anchor.suffix,
        style: seed.style,
        color: seed.color.into(),
        tags: vec![seed.tag.into()],
        body: seed.body.into(),
        created: frontmatter::now(),
    })
}

/// Write the sample notes and marks. Existing files are kept unless `overwrite`. Returns how
/// many files were written. `chapter` serves the text marks are anchored against.
pub fn seed_vault(
    vault: &Vault,
    module: &str,
    chapter: &dyn Fn(Book, u16) -> Option<Chapter>,
    overwrite: bool,
) -> io::Result<usize> {
    let mut written = 0;
    let mut put = |path: &str, text: &str| -> io::Result<()> {
        if overwrite || vault.read(path).is_err() {
            vault.write(path, text)?;
            written += 1;
        }
        Ok(())
    };
    for (path, text) in seed_notes(module) {
        put(&path, &text)?;
    }
    let mut current = Vec::new();
    for seed in MARKS {
        let Some(ch) = chapter(seed.book, seed.chapter) else { continue };
        if let Some(mark) = build(seed, module, &ch) {
            current.push(mark.path());
            put(&mark.path(), &mark.to_markdown())?;
        }
    }
    // Reseeding in another module: a sample mark it cannot anchor must not linger from before.
    if overwrite {
        for path in vault.read_all()?.into_keys() {
            let stale = MARKS.iter().any(|m| path.ends_with(&format!("-{}.md", m.id)));
            if path.starts_with("margins/") && stale && !current.contains(&path) {
                vault.remove(&path)?;
            }
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use study_core::StudyBackend;
    use study_core::mock::MockBackend;

    fn mock(book: Book, chapter: u16) -> Option<Chapter> {
        let b = MockBackend;
        let m = b.modules()[0].id.clone();
        b.chapter(&m, book, chapter).ok()
    }

    #[test]
    fn seeds_a_vault_once_and_marks_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let n = seed_vault(&v, "KJV", &mock, false).unwrap();
        // Four notes, plus the marks the mock's three chapters can anchor.
        assert!(n >= 4 + 4, "{n}");
        assert_eq!(seed_vault(&v, "KJV", &mock, false).unwrap(), 0, "nothing is overwritten");
        for (path, text) in v.read_all().unwrap().iter().filter(|(p, _)| p.starts_with("margins/")) {
            let id = path.trim_end_matches(".md").rsplit('-').next().unwrap();
            let mark = Mark::from_markdown(id, text).unwrap_or_else(|| panic!("{path}"));
            let ch = mock(mark.anchor.book, mark.anchor.chapter).unwrap();
            let r = margin::resolve(&mark, &ch.canonical_verses());
            assert_eq!(r.state, margin::Anchoring::Exact, "{path}");
        }
    }

    #[test]
    fn overwrite_restores_edited_seed_files() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        seed_vault(&v, "KJV", &mock, false).unwrap();
        v.write("notes/Creation.md", "scribbled").unwrap();
        v.write("notes/Mine.md", "keep me").unwrap();
        seed_vault(&v, "KJV", &mock, true).unwrap();
        assert!(v.read("notes/Creation.md").unwrap().contains("# Creation"));
        assert_eq!(v.read("notes/Mine.md").unwrap(), "keep me");
    }

    #[test]
    fn reseeding_drops_sample_marks_the_new_module_cannot_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        seed_vault(&v, "KJV", &mock, false).unwrap();
        let before = v.read_all().unwrap().keys().filter(|p| p.starts_with("margins/")).count();
        v.write("margins/John/03/16-note-zzzz.md", "---\ntype: margin\n---\n").unwrap();
        // A module with no John 3 at all: those samples go, the user's own mark stays.
        let no_john = |b, c| if b == Book::JOHN { None } else { mock(b, c) };
        seed_vault(&v, "NEW", &no_john, true).unwrap();
        let after: Vec<String> = v.read_all().unwrap().into_keys().filter(|p| p.starts_with("margins/")).collect();
        assert!(after.len() < before + 1, "{after:?}");
        assert!(after.iter().any(|p| p.ends_with("-zzzz.md")));
        assert!(!after.iter().any(|p| p.contains("John/03") && p.ends_with("-s001.md")));
    }

    #[test]
    fn notes_pin_to_the_chosen_module() {
        let notes = seed_notes("ASV");
        let nic = &notes.iter().find(|(p, _)| p.ends_with("Nicodemus.md")).unwrap().1;
        assert!(nic.contains("![[John 3:3-5@ASV]]") && !nic.contains("{module}"));
    }
}
