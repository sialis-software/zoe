//! A module in SWORD's NRSV versification (NHEB, Geneva1599, EMTV...): the KJV's numbering but for 3 John 1:15
//! and Revelation 12:18, which it has the places of. It reads the real module and does nothing without one.

use scripture::{Book, Versification};

fn book(osis: &str) -> Book {
    Book::from_osis(osis).expect(osis)
}

#[test]
fn an_nrsv_module_is_read_with_its_two_extra_verses() {
    let lib = sword::Library::discover();
    let mut seen = 0;
    for name in ["NHEB", "NHEBJE", "NHEBME", "Geneva1599", "EMTV"] {
        let Some(module) = lib.get(name) else { continue };
        assert_eq!(module.versification().unwrap(), Versification::Nrsv, "{name}");
        let bible = module.bible().unwrap_or_else(|e| panic!("{name} is readable: {e}"));
        let verses = |osis: &str, chapter: u16| bible.chapter(book(osis), chapter).unwrap().canonical_verses();
        // The module has the extra verses' places (a translation may leave them empty: its text can be numbered
        // as the KJV's, with all of the end of 3 John in verse 14), and nothing after them is shifted.
        let john = verses("3John", 1);
        assert!(john.keys().copied().max() >= Some(14) && john[&14].len() > 20, "{name}: 3 John");
        assert!(verses("Rev", 12).keys().copied().max() >= Some(17), "{name}: Revelation 12");
        assert!(verses("Jude", 1)[&25].contains("God"), "{name}: Jude 25 follows 3 John");
        assert!(verses("Rev", 1)[&1].len() > 20, "{name}: Revelation 1:1");
        assert!(verses("Rev", 13)[&1].to_lowercase().contains("beast"), "{name}: Revelation 13:1 follows Revelation 12");
        let last = verses("Rev", 22);
        let end = last.values().next_back().unwrap().to_lowercase();
        assert!(last.keys().copied().max() >= Some(20) && (end.contains("grace") || end.contains("amen")), "{name}: the last verse of the Bible: {end}");
        if bible.has_book(Book::GENESIS) {
            assert!(verses("Gen", 1)[&1].contains("beginning"), "{name}");
            assert_eq!(bible.chapters().len(), 1189, "{name}");
        }
        seen += 1;
    }
    if seen == 0 {
        eprintln!("skipped: no NRSV-versified module is installed");
    }
}

#[test]
fn modules_in_versifications_that_are_not_read_say_so() {
    let lib = sword::Library::discover();
    for module in lib.modules() {
        let name = module.conf.get("Versification").unwrap_or("KJV").to_string();
        match module.versification() {
            Ok(v) => assert!(matches!(name.as_str(), "KJV" | "KJVA" | "NRSV") && Versification::from_name(&name) == Some(v)),
            Err(e) => assert!(e.to_string().contains(&name), "{e}"),
        }
    }
}
