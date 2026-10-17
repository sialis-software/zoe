//! The hand-entered passages of the mock backend (`study_core::mock`) are word for word the KJV's, as the KJV module has
//! them (public domain). Skipped where no KJV module is installed.

use scripture::Book;
use study_core::StudyBackend;
use study_core::mock::MockBackend;

fn norm(s: &str) -> String {
    s.replace(['\u{2019}', '\u{2018}'], "'").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_mock_backends_verses_are_the_kjvs() {
    let lib = sword::Library::open(&sword::test_roots());
    let Some(module) = lib.get("KJV") else { return eprintln!("skipped: no KJV module is installed") };
    let bible = module.bible().unwrap();
    let id = study_core::ModuleId("KJV-mock".into());
    let mut checked = 0;
    for (book, chapter) in MockBackend.available_chapters(&id) {
        let ours = MockBackend.chapter(&id, book, chapter).unwrap().canonical_verses();
        let theirs = bible.chapter(book, chapter).unwrap().canonical_verses();
        for (verse, text) in &ours {
            // The label of a link that John 3:14 carries is the mock's own, not the KJV's.
            let text = if (book, chapter, *verse) == (Book::JOHN, 3, 14) { text.replace(" (Num 21:9)", "") } else { text.clone() };
            assert_eq!(norm(&text), norm(&theirs[verse]), "{book:?} {chapter}:{verse}");
            checked += 1;
        }
    }
    assert!(checked >= 19, "{checked} verses checked");
}
