//! The Apocrypha of the KJV with Apocrypha (module KJVA, versification KJVA). They are after Malachi in
//! the Old Testament's file, so this reads the real module and does nothing without it.

use scripture::Book;

fn book(osis: &str) -> Book {
    Book::from_osis(osis).expect(osis)
}

#[test]
fn the_kjva_apocrypha_are_read_and_the_canon_beside_them_is_not_moved() {
    let lib = sword::Library::open(&sword::test_roots());
    let Some(kjva) = lib.get("KJVA") else { return };
    let bible = kjva.bible().expect("KJVA is readable");
    let text = |osis: &str, chapter: u16| -> String {
        bible.chapter(book(osis), chapter).unwrap().canonical_verses().into_values().collect::<Vec<_>>().join(" ")
    };
    // Builds of KJVA differ in how many of the 14 books they have (some have all, some only Tobit to Maccabees):
    // those they have are read, the others are not there.
    for (osis, words) in [("Tob", "The book of the words of Tobit"), ("Jdt", "Nabuchodonosor"), ("Wis", "the souls of the righteous"), ("Sir", "I will thank thee, O Lord and King"),
        ("Bar", "Baruch"), ("1Macc", "Alexander"), ("2Macc", "Nicanor")]
    {
        assert!(bible.has_book(book(osis)), "{osis}");
        let chapters = (1..=sword::chapter_count(book(osis))).map(|c| text(osis, c)).collect::<Vec<_>>().join(" ");
        assert!(chapters.contains(words), "{osis}: {words}");
    }
    assert!(text("Gen", 1).contains("In the beginning"));
    assert!(text("Mal", 4).contains("the day"));
    assert!(text("Matt", 1).contains("generation"));
    assert!(text("Rev", 22).contains("river of water of life"));
    // Its chapters are those of the books it has.
    let expected: usize = Book::all().filter(|b| bible.has_book(*b)).map(|b| usize::from(sword::chapter_count(b))).sum();
    assert_eq!(bible.chapters().len(), expected);
    assert!(bible.chapters().len() > 1189);
}

#[test]
fn a_module_without_the_apocrypha_says_it_has_none() {
    let lib = sword::Library::open(&sword::test_roots());
    for name in ["ASV", "BSB", "KJV"] {
        let Some(module) = lib.get(name) else { continue };
        let Ok(bible) = module.bible() else { continue };
        assert!(Book::apocrypha().all(|b| !bible.has_book(b)), "{name}");
        assert!(Book::canon().all(|b| bible.has_book(b)), "{name}");
        assert_eq!(bible.chapters().len(), 1189, "{name}");
    }
}
