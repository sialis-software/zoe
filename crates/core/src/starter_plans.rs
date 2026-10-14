//! The reading plans a new vault starts with: plans in the public domain, written out as
//! library notes like any imported plan (docs/09).

use crate::plan::{Kind, Plan, plan_from_csv};

/// Robert Murray M'Cheyne's calendar of 1842: four chapters a day, by date. Two are read in the
/// family (the Old Testament from Genesis, the New Testament and the Psalms) and two in private
/// (the rest of the Old Testament, and the New Testament from Acts). The Old Testament is read
/// once, the New Testament and the Psalms twice. February 29 has no reading.
const MCHEYNE: &str = include_str!("../data/mcheyne.csv");

/// Genesis to Revelation in order, in 365 days of about the same number of verses, whole chapters
/// only. Made by `cargo run -p sword --example canonical_plan > crates/core/data/canonical.csv`.
const CANONICAL: &str = include_str!("../data/canonical.csv");

/// The whole Bible in a rough chronological order, in 365 days of about the same number of verses,
/// whole chapters only. The order is that of a list of passages (`chronological_bible.csv`) compiled by
/// the project owner, who released it into the public domain;
/// made by `cargo run -q -p sword --example chronological_plan -- chronological_bible.csv >
/// crates/core/data/chronological.csv`.
const CHRONOLOGICAL: &str = include_str!("../data/chronological.csv");

/// The plans a new vault gets, in the order they are listed.
pub fn starter_plans() -> Vec<Plan> {
    let mut canonical = plan_from_csv("Bible in a year", CANONICAL);
    canonical.description = "Genesis to Revelation in order, in 365 days of about the same length: a few chapters a day, started when you choose.".into();
    canonical.source = Some("Made for Zoe: the books in order, split by the number of verses".into());
    let mut mcheyne = plan_from_csv("M'Cheyne", MCHEYNE);
    mcheyne.description = "Four readings a day by date: the Old Testament once, and the New Testament and the Psalms twice, in a year.".into();
    mcheyne.source = Some("Robert Murray M'Cheyne, 1842 (public domain)".into());
    debug_assert_eq!(mcheyne.kind, Kind::Calendar);
    let mut chronological = plan_from_csv("Chronological", CHRONOLOGICAL);
    chronological.description = "The whole Bible in a chronological order, in 365 days of about the same length, with the Gospels read side by side and the letters set among Acts.".into();
    chronological.source = Some("Chronological order compiled by the Zoe project owner and released into the public domain; cut into days by verse count".into());
    vec![canonical, mcheyne, chronological]
}

#[cfg(test)]
mod tests {
    use super::*;
    use scripture::Book;
    use std::collections::HashMap;

    #[test]
    fn the_bible_in_a_year_reads_every_chapter_once_in_order_in_365_numbered_days() {
        let plan = &starter_plans()[0];
        assert_eq!(plan.name, "Bible in a year");
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
        assert_eq!(plan.kind, Kind::Sequential);
        assert_eq!(plan.days.len(), 365);
        let read: Vec<(Book, u16)> = plan.days.iter().flat_map(|d| d.passages.iter().map(|p| (p.book, p.chapter))).collect();
        let all: Vec<(Book, u16)> = Book::canon().flat_map(|b| (1..=crate::plan::chapter_count(b)).map(move |c| (b, c))).collect();
        assert_eq!(read, all, "each chapter once, in the order of the books");
        assert!(plan.days.iter().all(|d| !d.passages.is_empty()), "no empty day");
    }

    #[test]
    fn mcheyne_has_a_day_for_each_date_and_reads_every_chapter_as_often_as_it_should_be() {
        let plan = &starter_plans()[1];
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
        assert_eq!(plan.kind, Kind::Calendar);
        assert_eq!(plan.days.len(), 365, "a day for each date of a year without a leap day");
        let mut times: HashMap<(Book, u16), u32> = HashMap::new();
        for day in &plan.days {
            // A chapter in the day twice (read in pieces) counts once.
            let mut seen = Vec::new();
            for p in &day.passages {
                if !seen.contains(&(p.book, p.chapter)) {
                    seen.push((p.book, p.chapter));
                    *times.entry((p.book, p.chapter)).or_default() += 1;
                }
            }
        }
        for (i, book) in Book::canon().enumerate() {
            for chapter in 1..=crate::plan::chapter_count(book) {
                let want = if i >= 39 || book == Book::PSALMS { 2 } else { 1 };
                // (Psalm 78 and 119 and Luke 1 are read in pieces on some days, so they are in more days.)
                let got = times.get(&(book, chapter)).copied().unwrap_or(0);
                let pieces = |b: Book, c: u16| (b == Book::PSALMS && (c == 78 || c == 119)) || (b == Book::parse("Luke").unwrap() && c == 1);
                if pieces(book, chapter) {
                    assert!(got >= want, "{book} {chapter}");
                } else {
                    assert_eq!(got, want, "{book} {chapter}");
                }
            }
        }
    }

    #[test]
    fn mcheyne_starts_and_ends_where_it_should() {
        let plan = &starter_plans()[1];
        let first = |i: usize| plan.days[i].passages.iter().map(|p| format!("{} {}", p.book, p.chapter)).collect::<Vec<_>>();
        assert_eq!(first(0), ["Genesis 1", "Matthew 1", "Ezra 1", "Acts 1"]);
        assert_eq!(first(364), ["2 Chronicles 36", "Revelation 22", "Malachi 4", "Psalms 150"]);
    }

    /// The passages of the source list, in its order (`seq` 1 to 77), as (book, first chapter, last chapter).
    const SOURCE: [(&str, u16, u16); 77] = [
        ("Genesis", 1, 50),
        ("Job", 1, 42),
        ("Exodus", 1, 40),
        ("Leviticus", 1, 27),
        ("Numbers", 1, 36),
        ("Deuteronomy", 1, 34),
        ("Joshua", 1, 24),
        ("Judges", 1, 21),
        ("Ruth", 1, 4),
        ("1 Samuel", 1, 31),
        ("2 Samuel", 1, 24),
        ("1 Chronicles", 1, 29),
        ("Psalms", 1, 150),
        ("Song of Solomon", 1, 8),
        ("Proverbs", 1, 31),
        ("Ecclesiastes", 1, 12),
        ("1 Kings", 1, 22),
        ("2 Kings", 1, 25),
        ("2 Chronicles", 1, 36),
        ("Isaiah", 1, 66),
        ("Jeremiah", 1, 52),
        ("Lamentations", 1, 5),
        ("Hosea", 1, 14),
        ("Joel", 1, 3),
        ("Amos", 1, 9),
        ("Obadiah", 1, 1),
        ("Jonah", 1, 4),
        ("Micah", 1, 7),
        ("Nahum", 1, 3),
        ("Habakkuk", 1, 3),
        ("Zephaniah", 1, 3),
        ("Ezekiel", 1, 42),
        ("Daniel", 1, 6),
        ("Ezekiel", 43, 48),
        ("Daniel", 7, 12),
        ("Ezra", 1, 10),
        ("Esther", 1, 10),
        ("Nehemiah", 1, 13),
        ("Haggai", 1, 2),
        ("Zechariah", 1, 14),
        ("Malachi", 1, 4),
        ("Matthew", 1, 2),
        ("Luke", 1, 2),
        ("Mark", 1, 16),
        ("Matthew", 3, 28),
        ("Luke", 3, 24),
        ("John", 1, 21),
        ("Acts", 1, 12),
        ("James", 1, 5),
        ("Acts", 13, 14),
        ("Galatians", 1, 6),
        ("Acts", 15, 17),
        ("1 Thessalonians", 1, 5),
        ("2 Thessalonians", 1, 3),
        ("Acts", 18, 18),
        ("Acts", 19, 19),
        ("1 Corinthians", 1, 16),
        ("Acts", 20, 20),
        ("2 Corinthians", 1, 13),
        ("Romans", 1, 16),
        ("Acts", 21, 26),
        ("Acts", 27, 28),
        ("Ephesians", 1, 6),
        ("Philippians", 1, 4),
        ("Colossians", 1, 4),
        ("Philemon", 1, 1),
        ("Hebrews", 1, 13),
        ("1 Timothy", 1, 6),
        ("Titus", 1, 3),
        ("2 Timothy", 1, 4),
        ("1 Peter", 1, 5),
        ("2 Peter", 1, 3),
        ("1 John", 1, 5),
        ("2 John", 1, 1),
        ("3 John", 1, 1),
        ("Jude", 1, 1),
        ("Revelation", 1, 22),
    ];

    #[test]
    fn the_chronological_plan_reads_every_chapter_once_in_the_order_of_the_source_in_365_days() {
        let plan = &starter_plans()[2];
        assert_eq!(plan.name, "Chronological");
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
        assert_eq!(plan.kind, Kind::Sequential);
        assert_eq!(plan.days.len(), 365);
        assert!(plan.days.iter().all(|d| !d.passages.is_empty()), "no empty day");
        let read: Vec<(Book, u16)> = plan.days.iter().flat_map(|d| d.passages.iter().map(|p| (p.book, p.chapter))).collect();
        let mut want: Vec<(Book, u16)> = Vec::new();
        for (name, from, to) in SOURCE {
            let book = Book::parse(name).unwrap();
            want.extend((from..=to).map(|c| (book, c)));
        }
        assert_eq!(read, want, "the chapters in the order of the source");
        // The source misses none and repeats none (no exceptions).
        let mut sorted = read.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), read.len(), "no chapter twice");
        let all: usize = Book::canon().map(|b| usize::from(crate::plan::chapter_count(b))).sum();
        assert_eq!(read.len(), all, "every chapter of the Bible");
    }

    #[test]
    fn the_chronological_plan_starts_and_ends_where_the_source_does() {
        let plan = &starter_plans()[2];
        let day = |i: usize| plan.days[i].passages.iter().map(|p| format!("{} {}", p.book, p.chapter)).collect::<Vec<_>>();
        assert_eq!(day(0).first().map(String::as_str), Some("Genesis 1"));
        assert_eq!(day(364).last().map(String::as_str), Some("Revelation 22"));
    }
}
