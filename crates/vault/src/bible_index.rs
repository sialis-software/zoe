//! The words of one Bible module, in SQLite, to search. Like the notes' index it is derived (from
//! the module) and disposable: it is rebuilt when the module it was made from is not the one now
//! installed.

use rusqlite::{Connection, params};
use scripture::Book;
use std::path::Path;

/// Bump when the tables change: an older database is dropped and rebuilt.
const SCHEMA: i32 = 1;

/// A verse that matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseHit {
    pub book: Book,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

/// What to ask for.
#[derive(Clone, Debug)]
pub struct Search<'a> {
    /// An FTS5 expression (see `study_core::search::Query::fts`).
    pub expression: &'a str,
    /// Match other forms of a word (porter stemming).
    pub stem: bool,
    /// Only these books, first to last in canon order.
    pub books: Option<(Book, Book)>,
    /// Best matches first, else in canon order.
    pub by_relevance: bool,
    pub limit: usize,
}

pub struct BibleIndex {
    conn: Connection,
}

fn book_no(book: Book) -> i64 {
    Book::all().position(|b| b == book).unwrap_or(0) as i64
}

fn book_at(no: i64) -> Book {
    Book::all().nth(no as usize).unwrap_or(Book::GENESIS)
}

impl BibleIndex {
    /// Open the index at `path`, or one in memory (for what is not kept).
    pub fn open(path: Option<&Path>) -> rusqlite::Result<BibleIndex> {
        let conn = match path {
            Some(path) => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                Connection::open(path)?
            }
            None => Connection::open_in_memory()?,
        };
        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA {
            conn.execute_batch("DROP TABLE IF EXISTS fts; DROP TABLE IF EXISTS fts_stem; DROP TABLE IF EXISTS verses; DROP TABLE IF EXISTS meta;")?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA};"))?;
        }
        conn.execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);")?;
        Ok(BibleIndex { conn })
    }

    /// Whether it was built from the module with this fingerprint.
    pub fn is_current(&self, fingerprint: &str) -> bool {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = 'fingerprint'", [], |r| r.get::<_, String>(0))
            .is_ok_and(|f| f == fingerprint)
    }

    /// Replace everything with these verses, in canon order.
    pub fn build(&mut self, fingerprint: &str, verses: impl IntoIterator<Item = (Book, u16, u16, String)>) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS fts; DROP TABLE IF EXISTS fts_stem; DROP TABLE IF EXISTS verses;
             DELETE FROM meta;
             CREATE TABLE verses (id INTEGER PRIMARY KEY, book INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE INDEX verses_place ON verses (book, chapter, verse);",
        )?;
        {
            let mut insert = tx.prepare("INSERT INTO verses (book, chapter, verse, text) VALUES (?1, ?2, ?3, ?4)")?;
            for (book, chapter, verse, text) in verses {
                insert.execute(params![book_no(book), chapter, verse, text])?;
            }
        }
        tx.execute_batch(
            "CREATE VIRTUAL TABLE fts USING fts5(text, content='verses', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
             CREATE VIRTUAL TABLE fts_stem USING fts5(text, content='verses', content_rowid='id', tokenize='porter unicode61 remove_diacritics 2');
             INSERT INTO fts(fts) VALUES ('rebuild');
             INSERT INTO fts_stem(fts_stem) VALUES ('rebuild');",
        )?;
        tx.execute("INSERT INTO meta (key, value) VALUES ('fingerprint', ?1)", params![fingerprint])?;
        tx.commit()
    }

    /// The verses matching, in the order asked.
    pub fn search(&self, search: &Search<'_>) -> rusqlite::Result<Vec<VerseHit>> {
        let table = if search.stem { "fts_stem" } else { "fts" };
        let order = if search.by_relevance { format!("bm25({table}), v.id") } else { "v.id".to_string() };
        // Without a narrowing, every book there is: the Apocrypha (after the canon) too.
        let (first, last) = search.books.map_or((0, Book::all().count() as i64 - 1), |(a, z)| (book_no(a), book_no(z)));
        let sql = format!(
            "SELECT v.book, v.chapter, v.verse, v.text FROM {table} JOIN verses v ON v.id = {table}.rowid
             WHERE {table} MATCH ?1 AND v.book BETWEEN ?2 AND ?3 ORDER BY {order} LIMIT ?4"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![search.expression, first, last, search.limit as i64], |r| {
            Ok(VerseHit { book: book_at(r.get(0)?), chapter: r.get(1)?, verse: r.get(2)?, text: r.get(3)? })
        })?;
        rows.collect()
    }

    /// One verse's words.
    pub fn verse(&self, book: Book, chapter: u16, verse: u16) -> Option<String> {
        self.conn
            .query_row("SELECT text FROM verses WHERE book = ?1 AND chapter = ?2 AND verse = ?3", params![book_no(book), chapter, verse], |r| r.get(0))
            .ok()
    }

    /// How many verses there are.
    pub fn len(&self) -> usize {
        self.conn.query_row("SELECT count(*) FROM verses", [], |r| r.get::<_, i64>(0)).map_or(0, |n| n as usize)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BibleIndex {
        let mut index = BibleIndex::open(None).unwrap();
        let john = Book::JOHN;
        index
            .build(
                "sample",
                [
                    (Book::GENESIS, 1, 1, "In the beginning God created the heavens and the earth.".to_string()),
                    (john, 3, 16, "For God so loved the world, that He gave His only begotten Son".to_string()),
                    (john, 3, 17, "For God did not send the Son into the world to judge the world".to_string()),
                    (john, 4, 1, "The Lord knew that the Pharisees had heard".to_string()),
                ],
            )
            .unwrap();
        index
    }

    fn find(index: &BibleIndex, expression: &str, stem: bool) -> Vec<(u16, u16)> {
        let search = Search { expression, stem, books: None, by_relevance: false, limit: 100 };
        index.search(&search).unwrap().into_iter().map(|h| (h.chapter, h.verse)).collect()
    }

    #[test]
    fn words_phrases_alternatives_and_exclusions_find_their_verses_in_canon_order() {
        let index = sample();
        assert_eq!(find(&index, "\"god\"", false), [(1, 1), (3, 16), (3, 17)]);
        assert_eq!(find(&index, "\"world\" AND \"gave\"", false), [(3, 16)]);
        assert_eq!(find(&index, "\"only begotten\"", false), [(3, 16)]);
        assert_eq!(find(&index, "(\"heavens\" OR \"pharisees\")", false), [(1, 1), (4, 1)]);
        assert_eq!(find(&index, "(\"god\") NOT (\"gave\")", false), [(1, 1), (3, 17)]);
        assert_eq!(find(&index, "\"begin\"*", false), [(1, 1)]);
        assert_eq!(find(&index, "\"nothing\"", false), Vec::<(u16, u16)>::new());
    }

    #[test]
    fn stemming_finds_other_forms_only_when_asked_and_books_narrow() {
        let index = sample();
        assert!(find(&index, "\"love\"", false).is_empty());
        assert_eq!(find(&index, "\"love\"", true), [(3, 16)]);
        let search = Search { expression: "\"god\"", stem: false, books: Some((Book::JOHN, Book::JOHN)), by_relevance: false, limit: 100 };
        assert_eq!(index.search(&search).unwrap().len(), 2);
        let search = Search { limit: 1, ..search };
        assert_eq!(index.search(&search).unwrap().len(), 1);
        // The most about the word comes first when asked.
        let search = Search { expression: "\"world\"", stem: false, books: None, by_relevance: true, limit: 10 };
        assert_eq!(index.search(&search).unwrap()[0].verse, 17, "two worlds beat one");
    }

    #[test]
    fn a_search_without_a_narrowing_covers_the_apocrypha_too() {
        let tobit = Book::parse("Tobit").unwrap();
        let mut index = BibleIndex::open(None).unwrap();
        index.build("a", [(Book::JOHN, 3, 16, "God so loved".to_string()), (tobit, 1, 1, "The book of the words of Tobit, a godly man".to_string())]).unwrap();
        let search = Search { expression: "\"tobit\"", stem: false, books: None, by_relevance: false, limit: 10 };
        let hits = index.search(&search).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].book, hits[0].chapter, hits[0].verse), (tobit, 1, 1));
        assert_eq!(index.verse(tobit, 1, 1).as_deref(), Some("The book of the words of Tobit, a godly man"));
    }

    #[test]
    fn a_built_index_knows_what_it_was_built_from_and_keeps_it_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache/bible-LSB.sqlite");
        {
            let mut index = BibleIndex::open(Some(&path)).unwrap();
            assert!(index.is_empty() && !index.is_current("a"));
            index.build("a", [(Book::JOHN, 3, 16, "God so loved".to_string())]).unwrap();
            assert!(index.is_current("a") && !index.is_current("b"));
        }
        let again = BibleIndex::open(Some(&path)).unwrap();
        assert!(again.is_current("a") && again.len() == 1);
        assert_eq!(again.verse(Book::JOHN, 3, 16).as_deref(), Some("God so loved"));
        assert_eq!(again.verse(Book::JOHN, 3, 17), None);
    }
}
