//! In-memory backend with a handful of hand-entered public-domain (KJV) passages.
//! Excerpts only: John 3 omits verses, and notes/lexicon data are invented.

use crate::ast::*;
use crate::margin::{self, Mark, MarkStyle, Slice};
use crate::{CrossRef, Error, LexiconEntry, ModuleId, ModuleInfo, StudyBackend};
use scripture::{Book, RefRange};

pub struct MockBackend;

const KJV: &str = "KJV-mock";
const MOCK_DICTIONARY: &str = "StrongsMock";

fn t(s: &str) -> Inline {
    Inline::Text(s.to_string())
}

fn word(text: &str, strongs: &str) -> Inline {
    Inline::Word { text: text.into(), strongs: strongs.into() }
}

fn red(inlines: Vec<Inline>) -> Inline {
    Inline::RedLetter(inlines)
}

fn run(verse: u16, inlines: Vec<Inline>) -> Run {
    Run { verse, show_number: true, inlines }
}

fn prose(runs: Vec<Run>) -> Block {
    Block::Paragraph(Paragraph { style: ParagraphStyle::Prose, runs })
}

fn line(indent: u8, runs: Vec<Run>) -> Block {
    Block::Paragraph(Paragraph { style: ParagraphStyle::Poetry { indent }, runs })
}

fn cont(verse: u16, inlines: Vec<Inline>) -> Run {
    Run { verse, show_number: false, inlines }
}

fn lord() -> Inline {
    Inline::DivineName("Lord".into())
}

fn john_3() -> Chapter {
    let blocks = vec![
        Block::Heading("Nicodemus visits Jesus".into()),
        prose(vec![
            run(1, vec![t("There was a man of the Pharisees, named Nicodemus, a ruler of the Jews:")]),
            run(2, vec![
                t("The same came to Jesus by night, and said unto him, Rabbi, we know that thou art a teacher come from God: for no man can do these miracles that thou doest, except God be with him."),
                Inline::Footnote {
                    id: "a".into(),
                    text: "Mock footnote: \"by night\" is often read as fear of the other Pharisees, or as a sign of seeking quiet conversation. Compare Joh 3:16 for what he came to hear.".into(),
                },
            ]),
            run(3, vec![
                red(vec![t("Verily, verily, I say unto thee, Except a man be born again, he cannot see the kingdom of God.")]),
            ]),
            run(4, vec![t("Nicodemus saith unto him, How can a man be born when he is old? can he enter the second time into his mother's womb, and be born?")]),
            run(5, vec![
                red(vec![t("Verily, verily, I say unto thee, Except a man be born of water and of the Spirit, he cannot enter into the kingdom of God.")]),
                Inline::CrossRef { refs: vec![RefRange::parse("Num 21:9").unwrap(), RefRange::parse("Ps 23:1").unwrap()] },
            ]),
            run(6, vec![
                red(vec![t("That which is born of the flesh is flesh; and that which is born of the Spirit is spirit.")]),
            ]),
            run(7, vec![red(vec![t("Marvel not that I said unto thee, Ye must be born again.")])]),
            run(8, vec![
                red(vec![t("The wind bloweth where it listeth, and thou hearest the sound thereof, but canst not tell whence it cometh, and whither it goeth: so is every one that is born of the Spirit.")]),
            ]),
        ]),
        Block::Heading("God so loved the world".into()),
        prose(vec![
            run(14, vec![
                red(vec![
                    t("And as Moses lifted up the serpent in the wilderness, even so must the Son of man be lifted up: "),
                ]),
                Inline::Ref {
                    label: "(Num 21:9)".into(),
                    target: RefRange::parse("Num 21:9").unwrap(),
                },
            ]),
            run(15, vec![red(vec![t("That whosoever believeth in him should not perish, but have eternal life.")])]),
            run(16, vec![red(vec![
                t("For "),
                word("God", "G2316"),
                t(" so "),
                word("loved", "G25"),
                t(" the "),
                word("world", "G2889"),
                t(", that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."),
            ])]),
            run(17, vec![red(vec![t("For God sent not his Son into the world to condemn the world; but that the world through him might be saved.")])]),
        ]),
    ];
    Chapter { book: Book::JOHN, chapter: 3, blocks }
}

fn psalm_23() -> Chapter {
    let blocks = vec![
        Block::Heading("A Psalm of David".into()),
        line(0, vec![run(1, vec![t("The "), lord(), t(" is my shepherd;")])]),
        line(1, vec![cont(1, vec![t("I shall not want.")])]),
        line(0, vec![run(2, vec![t("He maketh me to lie down in green pastures:")])]),
        line(1, vec![cont(2, vec![t("he leadeth me beside the still waters.")])]),
        line(0, vec![run(3, vec![t("He restoreth my soul:")])]),
        line(1, vec![cont(3, vec![t("he leadeth me in the paths of righteousness for his name's sake.")])]),
        line(0, vec![run(4, vec![t("Yea, though I walk through the valley of the shadow of death, I will fear no evil:")])]),
        line(1, vec![cont(4, vec![t("for thou art with me; thy rod and thy staff they comfort me.")])]),
        line(0, vec![run(5, vec![t("Thou preparest a table before me in the presence of mine enemies:")])]),
        line(1, vec![cont(5, vec![t("thou anointest my head with oil; my cup runneth over.")])]),
        line(0, vec![run(6, vec![t("Surely goodness and mercy shall follow me all the days of my life:")])]),
        line(1, vec![cont(6, vec![t("and I will dwell in the house of the "), lord(), t(" for ever.")])]),
    ];
    Chapter { book: Book::PSALMS, chapter: 23, blocks }
}

fn numbers_21() -> Chapter {
    Chapter {
        book: Book::NUMBERS,
        chapter: 21,
        blocks: vec![prose(vec![run(9, vec![t(
            "And Moses made a serpent of brass, and put it upon a pole, and it came to pass, that if a serpent had bitten any man, when he beheld the serpent of brass, he lived.",
        )])])],
    }
}

const NOTES: [(&str, &str); 3] = [
    (
        "notes/Nicodemus.md",
        "---\ntitle: Notes on Nicodemus\ntags: [john, new-birth]\n---\n\n# Nicodemus\n\nHe came by night ([[@John 3:2]]). Jesus answers a question he never asked, in [[@John 3:3@KJV]]:\n\n![[@John 3:3-5@KJV]]\n\n## Questions\n\n- [ ] What does **born again** mean for Nicodemus, a teacher?\n- [x] Compare with [[@Num 21:9]] and the bronze serpent.\n- [ ] Look up `agapao` in [[%G25]].\n\nSee also [[Memory verses]] and [[Prayer list|my prayers]].\n",
    ),
    (
        "notes/Memory verses.md",
        "# Memory verses\n\n1. [[@John 3:16]] — one to learn by heart.\n2. [[@Ps 23:1-3@KJV]] — the shepherd psalm.\n\nBack to [[Nicodemus]]. This link goes nowhere yet: [[Unwritten note]].\n",
    ),
    (
        "prayers/Prayer list.md",
        "---\ntype: prayer\ntags: [prayer]\n---\n# Prayer list\n\n- [ ] Wisdom for a hard decision  #family  [[@Jas 1:5]]\n- [x] Healing for a friend (answered)\n- [ ] Peace in a busy week — [[@Ps 23]]\n",
    ),
];

/// A mark on `quote` in `verse` of `chapter`, anchored the way a real selection would be.
fn sample_mark(chapter: &Chapter, id: &str, verse: u16, quote: &str, style: MarkStyle, color: &str, tag: &str) -> Mark {
    sample_mark_with(chapter, id, verse, quote, style, color, tag, "")
}

#[allow(clippy::too_many_arguments)]
fn sample_mark_with(chapter: &Chapter, id: &str, verse: u16, quote: &str, style: MarkStyle, color: &str, tag: &str, body: &str) -> Mark {
    let canon = chapter.canonical_verses();
    let start = canon[&verse].find(quote).expect("sample quote exists");
    let slice = Slice { verse, start, end: start + quote.len() };
    let a = margin::anchor_from_selection(chapter.book, chapter.chapter, &canon, &[slice]).unwrap();
    Mark {
        id: id.into(),
        anchor: a.range,
        module: "KJV".into(),
        quote: a.quote,
        prefix: a.prefix,
        suffix: a.suffix,
        style,
        color: color.into(),
        tags: vec![tag.into()],
        body: body.into(),
        created: String::new(),
    }
}

/// A whole-verse mark: tied to the reference, no quote, so it shows in every module.
fn verse_mark(chapter: &Chapter, id: &str, verse: u16, style: MarkStyle, color: &str, tag: &str) -> Mark {
    let a = margin::anchor_for_verses(chapter.book, chapter.chapter, verse, verse);
    Mark {
        id: id.into(),
        anchor: a.range,
        module: String::new(),
        quote: a.quote,
        prefix: a.prefix,
        suffix: a.suffix,
        style,
        color: color.into(),
        tags: vec![tag.into()],
        body: String::new(),
        created: String::new(),
    }
}

impl StudyBackend for MockBackend {
    fn modules(&self) -> Vec<ModuleInfo> {
        vec![ModuleInfo {
            id: ModuleId(KJV.into()),
            abbreviation: "KJV".into(),
            description: "King James Version (mock excerpts)".into(),
            has_strongs: true,
        }]
    }

    fn available_chapters(&self, _module: &ModuleId) -> Vec<(Book, u16)> {
        vec![(Book::PSALMS, 23), (Book::NUMBERS, 21), (Book::JOHN, 3)]
    }

    fn chapter(&self, _module: &ModuleId, book: Book, chapter: u16) -> Result<Chapter, Error> {
        match (book, chapter) {
            (Book::JOHN, 3) => Ok(john_3()),
            (Book::PSALMS, 23) => Ok(psalm_23()),
            (Book::NUMBERS, 21) => Ok(numbers_21()),
            _ => Err(Error::NotAvailable(format!("{} {chapter}", book.name()))),
        }
    }

    fn lexicon_entry(&self, key: &str) -> Option<LexiconEntry> {
        let (lemma, translit, def) = match key {
            "G25" => ("ἀγαπάω", "agapaō", "to love, to value; to have a preference for (mock)"),
            "G2316" => ("θεός", "theos", "a deity, especially the supreme Divinity; God (mock)"),
            "G2889" => ("κόσμος", "kosmos", "orderly arrangement; the world, its inhabitants (mock)"),
            _ => return None,
        };
        Some(LexiconEntry {
            key: key.into(),
            lemma: lemma.into(),
            transliteration: translit.into(),
            definition: def.into(),
        })
    }

    fn dictionaries(&self) -> Vec<ModuleInfo> {
        vec![ModuleInfo {
            id: ModuleId(MOCK_DICTIONARY.into()),
            abbreviation: "Strong's (mock)".into(),
            description: "A few invented entries".into(),
            has_strongs: true,
        }]
    }

    fn dictionary_entry(&self, module: &ModuleId, key: &str) -> Option<LexiconEntry> {
        (module.0 == MOCK_DICTIONARY).then(|| self.lexicon_entry(key)).flatten()
    }

    fn strongs_dictionary(&self, key: &str) -> Option<ModuleId> {
        self.lexicon_entry(key).map(|_| ModuleId(MOCK_DICTIONARY.into()))
    }

    fn mark_files(&self) -> Vec<(String, String)> {
        let (john, psalm) = (john_3(), psalm_23());
        [
            sample_mark_with(&john, "a001", 16, "so loved the world", MarkStyle::Highlight, "#F5D90A", "promise",
                "The heart of the gospel. Learn this one by heart."),
            sample_mark_with(&john, "a002", 16, "everlasting life", MarkStyle::Underline, "#4DA3FF", "promise",
                "Eternal life begins now, not only after death. Compare [[@John 17:3]]."),
            sample_mark_with(&john, "a003", 3, "born again", MarkStyle::Highlight, "#FF7AB6", "prayer",
                "\"Again\" can also mean \"from above\". Nicodemus hears the first; Jesus may mean both."),
            sample_mark(&psalm, "a004", 1, "is my shepherd", MarkStyle::Highlight, "#5FD38D", "growth"),
        ]
        .into_iter()
        .chain([verse_mark(&psalm, "a005", 4, MarkStyle::Highlight, "#B48CFF", "comfort")])
        .map(|m| (m.path(), m.to_markdown()))
        .collect()
    }

    fn note_paths(&self) -> Vec<String> {
        NOTES.iter().map(|(p, _)| p.to_string()).collect()
    }

    fn note_text(&self, path: &str) -> Option<String> {
        NOTES.iter().find(|(p, _)| *p == path).map(|(_, t)| t.to_string())
    }

    fn cross_references(&self, range: &RefRange) -> Vec<CrossRef> {
        let mut out = Vec::new();
        if range.book == Book::JOHN && range.chapter == 3 {
            let in_range = |v: u16| range.verses.is_none_or(|(a, b)| a <= v && v <= b);
            if in_range(14) {
                out.push(CrossRef {
                    target: RefRange::parse("Num 21:9").unwrap(),
                    preview: "…if a serpent had bitten any man… he lived.".into(),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_advertised_chapters() {
        let b = MockBackend;
        let m = b.modules()[0].id.clone();
        for (book, ch) in b.available_chapters(&m) {
            assert!(b.chapter(&m, book, ch).is_ok());
        }
        assert!(b.chapter(&m, Book::JOHN, 99).is_err());
    }

    #[test]
    fn sample_marks_parse_and_resolve() {
        let b = MockBackend;
        let m = b.modules()[0].id.clone();
        let files = b.mark_files();
        assert_eq!(files.len(), 5);
        for (path, text) in files {
            let id = path.trim_end_matches(".md").rsplit('-').next().unwrap();
            let mark = Mark::from_markdown(id, &text).unwrap_or_else(|| panic!("{path}"));
            let chapter = b.chapter(&m, mark.anchor.book, mark.anchor.chapter).unwrap();
            let r = margin::resolve(&mark, &chapter.canonical_verses());
            assert_eq!(r.state, margin::Anchoring::Exact, "{path}");
            assert_eq!(mark.path(), path);
        }
    }

    #[test]
    fn plain_text_extracts_verses() {
        let b = MockBackend;
        let m = b.modules()[0].id.clone();
        let ch = b.chapter(&m, Book::JOHN, 3).unwrap();
        let text = ch.plain_text(&RefRange::parse("John 3:16-17").unwrap());
        assert!(text.starts_with("For God so loved the world"));
        assert!(text.contains("might be saved."));
        assert!(!text.contains("Nicodemus"));
    }
}
