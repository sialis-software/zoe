//! The installed Morning and Evening, converted to the library's markdown standard. Core cannot read
//! SWORD modules, so the check that needs the real module is here. It does nothing without it.

use study_core::devotional::import;
use study_core::frontmatter;
use study_core::markdown::{LinkTarget, find_links};

#[test]
fn the_installed_morning_and_evening_becomes_a_file_for_every_day() {
    let lib = sword::Library::discover();
    let Some(sme) = lib.get("SME") else { return };
    let devotional = sme.devotional().unwrap();
    let keys = devotional.keys().unwrap();
    assert_eq!(keys.len(), 366);
    let entries: Vec<(String, String)> = keys.iter().map(|k| (k.clone(), devotional.raw(k).unwrap().unwrap())).collect();
    let days = import("Morning and Evening", entries);
    assert_eq!(days.len(), 366, "every key converts");
    assert!(days.iter().any(|d| d.path == "library/Morning and Evening/02-29.md"), "February 29 exists");
    for day in &days {
        let parsed = frontmatter::parse(&day.text);
        assert_eq!(parsed.front.kind.as_deref(), Some("devotional"), "{}", day.path);
        assert!(day.text.contains("\n## Morning\n") && day.text.contains("\n## Evening\n"), "{}", day.path);
        // A `<` that is left is one the text escaped (`\<`), never a tag.
        for (at, _) in day.text.match_indices('<') {
            assert!(day.text[..at].ends_with('\\'), "raw tag in {}:\n{}", day.path, day.text);
        }
        let links: Vec<_> = day.text.lines().flat_map(find_links).collect();
        assert!(links.len() >= 2, "a key verse for each section in {}", day.path);
        assert_eq!(day.text.matches("[[").count(), links.len(), "every link is found in {}", day.path);
        assert!(links.iter().all(|l| matches!(l.target, LinkTarget::Verse { .. })), "{}", day.path);
    }
}
