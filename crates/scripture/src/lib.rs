//! Canonical scripture references: the shared vocabulary between SWORD
//! modules and user notes (see docs/02).
//!
//! Prototype scope: KJV book order (the 66 books, then the 14 of the Apocrypha as the KJV with Apocrypha
//! has them), references limited to a single chapter. Versification mapping and multi-chapter ranges
//! come later.

use std::fmt;

/// (OSIS id, full English name)
/// The 66 books of the Protestant canon come first, then the Apocrypha, so that the position of a
/// canonical book is the same with or without it.
const BOOKS: [(&str, &str); 80] = [
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
    // The Apocrypha, in the order of the KJV with Apocrypha (the SWORD module KJVA) and its OSIS ids.
    ("1Esd", "1 Esdras"),
    ("2Esd", "2 Esdras"),
    ("Tob", "Tobit"),
    ("Jdt", "Judith"),
    ("AddEsth", "Additions to Esther"),
    ("Wis", "Wisdom"),
    ("Sir", "Sirach"),
    ("Bar", "Baruch"),
    ("PrAzar", "Prayer of Azariah"),
    ("Sus", "Susanna"),
    ("Bel", "Bel and the Dragon"),
    ("PrMan", "Prayer of Manasseh"),
    ("1Macc", "1 Maccabees"),
    ("2Macc", "2 Maccabees"),
];

/// How many books are the Protestant canon: the books before the Apocrypha.
const CANON: usize = 66;

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
    ("1es", "1Esd"), ("1esd", "1Esd"), ("2es", "2Esd"), ("2esd", "2Esd"), ("tob", "Tob"), ("tb", "Tob"),
    ("jdt", "Jdt"), ("jth", "Jdt"), ("addesth", "AddEsth"), ("aesth", "AddEsth"), ("wis", "Wis"),
    ("wisdomofsolomon", "Wis"), ("sir", "Sir"), ("ecclesiasticus", "Sir"), ("wisdomofsirach", "Sir"),
    ("bar", "Bar"), ("praz", "PrAzar"), ("prayerofazariah", "PrAzar"), ("sus", "Sus"), ("bel", "Bel"),
    ("prman", "PrMan"), ("prayerofmanasses", "PrMan"), ("1ma", "1Macc"), ("1mac", "1Macc"), ("1macc", "1Macc"),
    ("2ma", "2Macc"), ("2mac", "2Macc"), ("2macc", "2Macc"),
];

/// A book, as an index into the KJV-ordered book table (the canon, then the Apocrypha).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Book(u8);

impl Book {
    pub const GENESIS: Book = Book(0);
    pub const NUMBERS: Book = Book(3);
    pub const PSALMS: Book = Book(18);
    pub const JOHN: Book = Book(42);
    pub const ROMANS: Book = Book(44);
    pub const PHILIPPIANS: Book = Book(49);

    /// Every book there is: the 66 of the canon, then the 14 of the Apocrypha.
    pub fn all() -> impl Iterator<Item = Book> {
        (0..BOOKS.len() as u8).map(Book)
    }

    /// The 66 books of the Protestant canon.
    pub fn canon() -> impl Iterator<Item = Book> {
        (0..CANON as u8).map(Book)
    }

    /// The 14 books of the Apocrypha (the deuterocanonical books), in order.
    pub fn apocrypha() -> impl Iterator<Item = Book> {
        (CANON as u8..BOOKS.len() as u8).map(Book)
    }

    pub fn is_apocrypha(self) -> bool {
        self.0 as usize >= CANON
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

/// How a module numbers its chapters and verses. Notes and links name passages in the reference
/// versification, the KJV's; a module in another one is read by its own numbering, and a passage is
/// mapped into it to be shown (`map_from_kjv`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Versification {
    /// The KJV: the 66 books.
    #[default]
    Kjv,
    /// The KJV with the Apocrypha after the New Testament's books (14 more books, see `Book::apocrypha`).
    Kjva,
    /// SWORD's NRSV: the KJV's but for 3 John (15 verses, not 14) and Revelation 12 (18, not 17).
    Nrsv,
}

impl Versification {
    /// The versification a SWORD conf names (`Versification=`), if it is one that is read.
    pub fn from_name(name: &str) -> Option<Versification> {
        match name {
            "KJV" => Some(Versification::Kjv),
            "KJVA" => Some(Versification::Kjva),
            "NRSV" => Some(Versification::Nrsv),
            _ => None,
        }
    }

    pub fn has_apocrypha(self) -> bool {
        self == Versification::Kjva
    }

    /// `passage`, named in the KJV's numbering, as this versification numbers it. A passage that is
    /// numbered the same (nearly all) is returned as it is; the few that differ are: 3 John 14 is 14
    /// and 15 in the NRSV, where the KJV has them as one verse.
    pub fn map_from_kjv(self, passage: RefRange) -> RefRange {
        match (self, passage.verses) {
            (Versification::Nrsv, Some((a, 14))) if is_3_john(passage) => RefRange { verses: Some((a, 15)), ..passage },
            _ => passage,
        }
    }
}

fn is_3_john(passage: RefRange) -> bool {
    passage.book.osis() == "3John" && passage.chapter == 1
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
    fn versifications_are_named_by_the_conf_and_a_passage_is_mapped_into_the_nrsv() {
        assert_eq!(Versification::from_name("KJV"), Some(Versification::Kjv));
        assert_eq!(Versification::from_name("KJVA"), Some(Versification::Kjva));
        assert_eq!(Versification::from_name("NRSV"), Some(Versification::Nrsv));
        assert_eq!(Versification::from_name("Catholic"), None);
        assert!(Versification::Kjva.has_apocrypha() && !Versification::Nrsv.has_apocrypha());
        let nrsv = Versification::Nrsv;
        let range = |s: &str| RefRange::parse(s).unwrap();
        assert_eq!(nrsv.map_from_kjv(range("3 John 1:14")), range("3 John 1:14-15"));
        assert_eq!(nrsv.map_from_kjv(range("3 John 1:12-14")), range("3 John 1:12-15"));
        for same in ["3 John 1:13", "3 John 1", "John 3:16", "Revelation 12:17"] {
            assert_eq!(nrsv.map_from_kjv(range(same)), range(same), "{same}");
        }
        assert_eq!(Versification::Kjv.map_from_kjv(range("3 John 1:14")), range("3 John 1:14"), "the KJV is the reference");
    }

    #[test]
    fn the_apocrypha_follow_the_canon_so_that_the_canon_keeps_its_places() {
        assert_eq!(Book::all().count(), 80);
        assert_eq!((Book::canon().count(), Book::apocrypha().count()), (66, 14));
        assert_eq!(Book::all().position(|b| b == Book::JOHN), Some(42), "the places of the canon are as they were");
        assert_eq!(Book::all().nth(65).map(Book::osis), Some("Rev"));
        assert!(!Book::JOHN.is_apocrypha() && Book::from_osis("Tob").unwrap().is_apocrypha());
        let osis: Vec<_> = Book::apocrypha().map(Book::osis).collect();
        assert_eq!(osis, ["1Esd", "2Esd", "Tob", "Jdt", "AddEsth", "Wis", "Sir", "Bar", "PrAzar", "Sus", "Bel", "PrMan", "1Macc", "2Macc"]);
    }

    #[test]
    fn apocryphal_books_are_found_by_name_and_abbreviation_and_the_old_ones_still_are() {
        for (name, osis) in [("Tobit", "Tob"), ("Judith", "Jdt"), ("Jdt", "Jdt"), ("Wisdom", "Wis"), ("Wisdom of Solomon", "Wis"), ("Sirach", "Sir"),
            ("Ecclesiasticus", "Sir"), ("Baruch", "Bar"), ("Susanna", "Sus"), ("Bel and the Dragon", "Bel"), ("1 Maccabees", "1Macc"),
            ("2 Macc", "2Macc"), ("1 Esdras", "1Esd"), ("2Esd", "2Esd"), ("Prayer of Manasseh", "PrMan"), ("Additions to Esther", "AddEsth")]
        {
            assert_eq!(Book::parse(name).map(Book::osis), Some(osis), "{name}");
        }
        assert_eq!(Book::parse("Est").map(Book::osis), Some("Esth"));
        assert_eq!(Book::parse("Esther").map(Book::osis), Some("Esth"));
        assert_eq!(Book::parse("Jude").map(Book::osis), Some("Jude"));
        assert_eq!(RefRange::parse("Sirach 3:21").unwrap().osis(), "Sir.3.21");
        assert_eq!(RefRange::parse("Bel and the Dragon 1:5").unwrap().osis(), "Bel.1.5");
        assert_eq!(RefRange::from_osis("1Macc.2.3-1Macc.2.5").unwrap().to_string(), "1 Maccabees 2:3-5");
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
