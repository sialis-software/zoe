//! Canonical scripture references: the shared vocabulary between SWORD
//! modules and user notes (see docs/02).
//!
//! Prototype scope: KJV book order, references limited to a single chapter.
//! Versification mapping and multi-chapter ranges come later.

use std::fmt;

/// (OSIS id, full English name)
const BOOKS: [(&str, &str); 66] = [
    ("Gen", "Genesis"),
    ("Exod", "Exodus"),
    ("Lev", "Leviticus"),
    ("Num", "Numbers"),
    ("Deut", "Deuteronomy"),
    ("Josh", "Joshua"),
    ("Judg", "Judges"),
    ("Ruth", "Ruth"),
    ("1Sam", "1 Samuel"),
    ("2Sam", "2 Samuel"),
    ("1Kgs", "1 Kings"),
    ("2Kgs", "2 Kings"),
    ("1Chr", "1 Chronicles"),
    ("2Chr", "2 Chronicles"),
    ("Ezra", "Ezra"),
    ("Neh", "Nehemiah"),
    ("Esth", "Esther"),
    ("Job", "Job"),
    ("Ps", "Psalms"),
    ("Prov", "Proverbs"),
    ("Eccl", "Ecclesiastes"),
    ("Song", "Song of Solomon"),
    ("Isa", "Isaiah"),
    ("Jer", "Jeremiah"),
    ("Lam", "Lamentations"),
    ("Ezek", "Ezekiel"),
    ("Dan", "Daniel"),
    ("Hos", "Hosea"),
    ("Joel", "Joel"),
    ("Amos", "Amos"),
    ("Obad", "Obadiah"),
    ("Jonah", "Jonah"),
    ("Mic", "Micah"),
    ("Nah", "Nahum"),
    ("Hab", "Habakkuk"),
    ("Zeph", "Zephaniah"),
    ("Hag", "Haggai"),
    ("Zech", "Zechariah"),
    ("Mal", "Malachi"),
    ("Matt", "Matthew"),
    ("Mark", "Mark"),
    ("Luke", "Luke"),
    ("John", "John"),
    ("Acts", "Acts"),
    ("Rom", "Romans"),
    ("1Cor", "1 Corinthians"),
    ("2Cor", "2 Corinthians"),
    ("Gal", "Galatians"),
    ("Eph", "Ephesians"),
    ("Phil", "Philippians"),
    ("Col", "Colossians"),
    ("1Thess", "1 Thessalonians"),
    ("2Thess", "2 Thessalonians"),
    ("1Tim", "1 Timothy"),
    ("2Tim", "2 Timothy"),
    ("Titus", "Titus"),
    ("Phlm", "Philemon"),
    ("Heb", "Hebrews"),
    ("Jas", "James"),
    ("1Pet", "1 Peter"),
    ("2Pet", "2 Peter"),
    ("1John", "1 John"),
    ("2John", "2 John"),
    ("3John", "3 John"),
    ("Jude", "Jude"),
    ("Rev", "Revelation"),
];

/// Common abbreviations (SWORD's three letter forms among them), keyed without spaces or dots
/// and in lower case. Names and unambiguous name prefixes are handled separately.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("gen", "Gen"), ("ge", "Gen"), ("exo", "Exod"), ("ex", "Exod"), ("lev", "Lev"), ("le", "Lev"),
    ("num", "Num"), ("nu", "Num"), ("deu", "Deut"), ("dt", "Deut"), ("jos", "Josh"), ("jdg", "Judg"),
    ("jg", "Judg"), ("rut", "Ruth"), ("ru", "Ruth"), ("1sa", "1Sam"), ("1sam", "1Sam"), ("2sa", "2Sam"),
    ("2sam", "2Sam"), ("1ki", "1Kgs"), ("1kgs", "1Kgs"), ("2ki", "2Kgs"), ("2kgs", "2Kgs"),
    ("1ch", "1Chr"), ("1chr", "1Chr"), ("2ch", "2Chr"), ("2chr", "2Chr"), ("ezr", "Ezra"),
    ("neh", "Neh"), ("est", "Esth"), ("esth", "Esth"), ("job", "Job"), ("psa", "Ps"), ("ps", "Ps"),
    ("pss", "Ps"), ("pro", "Prov"), ("pr", "Prov"), ("ecc", "Eccl"), ("ecclesiastes", "Eccl"),
    ("sng", "Song"), ("sos", "Song"), ("son", "Song"), ("song", "Song"), ("isa", "Isa"),
    ("jer", "Jer"), ("lam", "Lam"), ("eze", "Ezek"), ("ezk", "Ezek"), ("dan", "Dan"), ("hos", "Hos"),
    ("joe", "Joel"), ("amo", "Amos"), ("oba", "Obad"), ("ob", "Obad"), ("jon", "Jonah"), ("mic", "Mic"),
    ("nah", "Nah"), ("hab", "Hab"), ("zep", "Zeph"), ("zeph", "Zeph"), ("hag", "Hag"), ("zec", "Zech"),
    ("mal", "Mal"), ("mat", "Matt"), ("mt", "Matt"), ("mar", "Mark"), ("mrk", "Mark"), ("mk", "Mark"),
    ("luk", "Luke"), ("lk", "Luke"), ("joh", "John"), ("jhn", "John"), ("jn", "John"), ("act", "Acts"),
    ("rom", "Rom"), ("ro", "Rom"), ("1co", "1Cor"), ("1cor", "1Cor"), ("2co", "2Cor"), ("2cor", "2Cor"),
    ("gal", "Gal"), ("eph", "Eph"), ("php", "Phil"), ("phi", "Phil"), ("col", "Col"), ("1th", "1Thess"),
    ("1thess", "1Thess"), ("2th", "2Thess"), ("2thess", "2Thess"), ("1ti", "1Tim"), ("1tim", "1Tim"),
    ("2ti", "2Tim"), ("2tim", "2Tim"), ("tit", "Titus"), ("phm", "Phlm"), ("heb", "Heb"),
    ("jam", "Jas"), ("jas", "Jas"), ("1pe", "1Pet"), ("1pet", "1Pet"), ("2pe", "2Pet"),
    ("2pet", "2Pet"), ("1jo", "1John"), ("1jn", "1John"),
    ("2jo", "2John"), ("2jn", "2John"), ("3jo", "3John"), ("3jn", "3John"), ("jud", "Jude"),
    ("rev", "Rev"),
];

/// A book of the canon, as an index into the KJV-ordered book table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Book(u8);

impl Book {
    pub const GENESIS: Book = Book(0);
    pub const NUMBERS: Book = Book(3);
    pub const PSALMS: Book = Book(18);
    pub const JOHN: Book = Book(42);
    pub const ROMANS: Book = Book(44);
    pub const PHILIPPIANS: Book = Book(49);

    pub fn all() -> impl Iterator<Item = Book> {
        (0..BOOKS.len() as u8).map(Book)
    }

    pub fn osis(self) -> &'static str {
        BOOKS[self.0 as usize].0
    }

    pub fn name(self) -> &'static str {
        BOOKS[self.0 as usize].1
    }

    /// Exact lookup by OSIS id (`John`, `1Cor`).
    pub fn from_osis(osis: &str) -> Option<Book> {
        Book::all().find(|b| b.osis() == osis)
    }

    /// Lenient lookup by OSIS id, full name, or unambiguous-prefix
    /// abbreviation ("Jn" is not supported yet; "Joh" is ambiguous).
    pub fn parse(s: &str) -> Option<Book> {
        let key = normalize(s);
        if key.is_empty() {
            return None;
        }
        let alias = ABBREVIATIONS.iter().find(|(a, _)| *a == key);
        if let Some(book) = alias.and_then(|(_, osis)| Book::from_osis(osis)) {
            return Some(book);
        }
        let exact = Book::all().find(|b| normalize(b.osis()) == key || normalize(b.name()) == key);
        if exact.is_some() {
            return exact;
        }
        let mut hits = Book::all().filter(|b| normalize(b.name()).starts_with(&key));
        match (hits.next(), hits.next()) {
            (Some(b), None) => Some(b),
            _ => None,
        }
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .flat_map(char::to_lowercase)
        .collect()
}

impl fmt::Display for Book {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A single verse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref {
    pub book: Book,
    pub chapter: u16,
    pub verse: u16,
}

/// A whole chapter or a verse range within one chapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RefRange {
    pub book: Book,
    pub chapter: u16,
    /// `None` means the whole chapter.
    pub verses: Option<(u16, u16)>,
}

impl RefRange {
    pub fn chapter(book: Book, chapter: u16) -> Self {
        Self { book, chapter, verses: None }
    }

    pub fn verse(r: Ref) -> Self {
        Self { book: r.book, chapter: r.chapter, verses: Some((r.verse, r.verse)) }
    }

    pub fn contains(&self, r: Ref) -> bool {
        self.book == r.book
            && self.chapter == r.chapter
            && self.verses.is_none_or(|(a, b)| (a..=b).contains(&r.verse))
    }

    /// Whether the two ranges share at least one verse.
    pub fn intersects(&self, other: &RefRange) -> bool {
        self.book == other.book
            && self.chapter == other.chapter
            && match (self.verses, other.verses) {
                (Some((a, b)), Some((c, d))) => a <= d && c <= b,
                _ => true,
            }
    }

    /// Parse "John 3", "John 3:16", "1 Cor 13:4-7".
    pub fn parse(s: &str) -> Option<RefRange> {
        let s = s.trim();
        let (book_part, loc) = s.rsplit_once(char::is_whitespace)?;
        let book = Book::parse(book_part)?;
        match loc.split_once(':') {
            None => Some(RefRange::chapter(book, loc.parse().ok()?)),
            Some((ch, vs)) => {
                let chapter = ch.parse().ok()?;
                let (a, b) = match vs.split_once('-') {
                    Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
                    None => {
                        let v = vs.parse().ok()?;
                        (v, v)
                    }
                };
                (a >= 1 && b >= a).then_some(RefRange { book, chapter, verses: Some((a, b)) })
            }
        }
    }

    /// Inverse of [`RefRange::osis`]: `Ps.23`, `John.3.16`, `John.3.16-John.3.17`.
    pub fn from_osis(s: &str) -> Option<RefRange> {
        let (first, last) = match s.split_once('-') {
            Some((a, b)) => (a, Some(b)),
            None => (s, None),
        };
        let mut parts = first.split('.');
        let book = Book::from_osis(parts.next()?)?;
        let chapter = parts.next()?.parse().ok()?;
        let verse: Option<u16> = parts.next().map(str::parse).transpose().ok()?;
        if parts.next().is_some() {
            return None;
        }
        match (verse, last) {
            (None, None) => Some(RefRange::chapter(book, chapter)),
            (Some(v), None) => Some(RefRange { book, chapter, verses: Some((v, v)) }),
            (Some(a), Some(end)) => {
                let mut e = end.split('.');
                let same = e.next() == Some(book.osis()) && e.next()?.parse() == Ok(chapter);
                let z: u16 = e.next()?.parse().ok()?;
                (same && z >= a).then_some(RefRange { book, chapter, verses: Some((a, z)) })
            }
            (None, Some(_)) => None,
        }
    }

    /// OSIS form, e.g. `John.3.16-John.3.17`.
    pub fn osis(&self) -> String {
        let (b, c) = (self.book.osis(), self.chapter);
        match self.verses {
            None => format!("{b}.{c}"),
            Some((a, z)) if a == z => format!("{b}.{c}.{a}"),
            Some((a, z)) => format!("{b}.{c}.{a}-{b}.{c}.{z}"),
        }
    }
}

impl fmt::Display for RefRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.book, self.chapter)?;
        match self.verses {
            None => Ok(()),
            Some((a, z)) if a == z => write!(f, ":{a}"),
            Some((a, z)) => write!(f, ":{a}-{z}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_books() {
        assert_eq!(Book::parse("John"), Some(Book::JOHN));
        assert_eq!(Book::parse("ps"), Some(Book::PSALMS));
        assert_eq!(Book::parse("Psalms"), Some(Book::PSALMS));
        assert_eq!(Book::parse("1 Cor").map(Book::osis), Some("1Cor"));
        assert_eq!(Book::parse("Num"), Some(Book::NUMBERS));
        assert_eq!(Book::parse("Nonsense"), None);
        for (abbr, osis) in [("Mrk", "Mark"), ("Luk", "Luke"), ("Joh", "John"), ("Jn", "John"), ("1Co", "1Cor"),
            ("1 Co.", "1Cor"), ("Php", "Phil"), ("Jud", "Jude"), ("Jdg", "Judg"), ("Psa", "Ps"), ("Rev", "Rev")]
        {
            assert_eq!(Book::parse(abbr).map(Book::osis), Some(osis), "{abbr}");
        }
        assert_eq!(Book::GENESIS.osis(), "Gen");
        assert_eq!(Book::ROMANS.osis(), "Rom");
        assert_eq!(Book::PHILIPPIANS.osis(), "Phil");
    }

    #[test]
    fn parses_ranges() {
        let r = RefRange::parse("John 3:16-17").unwrap();
        assert_eq!(r.verses, Some((16, 17)));
        assert_eq!(r.osis(), "John.3.16-John.3.17");
        assert_eq!(r.to_string(), "John 3:16-17");
        assert_eq!(RefRange::parse("Ps 23").unwrap().osis(), "Ps.23");
        assert_eq!(RefRange::parse("1 Cor 13:4").unwrap().osis(), "1Cor.13.4");
        assert!(RefRange::parse("John 3:17-16").is_none());
    }

    #[test]
    fn osis_round_trips() {
        for s in ["Ps 23", "John 3:16", "John 3:16-17", "1 Cor 13:4-7"] {
            let r = RefRange::parse(s).unwrap();
            assert_eq!(RefRange::from_osis(&r.osis()), Some(r), "{s}");
        }
        assert_eq!(RefRange::from_osis("John.3.16-Luke.3.17"), None);
        assert_eq!(RefRange::from_osis("John.3.17-John.3.16"), None);
        assert_eq!(RefRange::from_osis("Nope.1.1"), None);
        assert_eq!(RefRange::from_osis("John.3.16.1"), None);
    }

    #[test]
    fn intersects() {
        let p = |s| RefRange::parse(s).unwrap();
        assert!(p("John 3:3-5").intersects(&p("John 3:5")));
        assert!(!p("John 3:3-5").intersects(&p("John 3:6")));
        assert!(p("John 3").intersects(&p("John 3:16")));
        assert!(!p("John 3").intersects(&p("John 4")));
    }

    #[test]
    fn contains() {
        let r = RefRange::parse("John 3:16-17").unwrap();
        let v = |verse| Ref { book: Book::JOHN, chapter: 3, verse };
        assert!(r.contains(v(16)) && r.contains(v(17)) && !r.contains(v(18)));
    }
}
