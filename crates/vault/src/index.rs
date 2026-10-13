//! The derived index: links, anchors and full text of every file, in SQLite.
//!
//! It is a pure function of the vault's files. Delete the database and `sync` rebuilds it
//! (a test enforces that). Nothing user-authored lives only here.

use rusqlite::{Connection, params};
use scripture::{Book, RefRange};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use study_core::markdown::{LinkTarget, find_links};

/// Bump when the tables change: an older database is dropped and rebuilt.
const SCHEMA: i32 = 1;

/// A whole-chapter link covers every verse.
const ALL_VERSES: u16 = 999;

pub struct Index {
    conn: Connection,
}

/// What a `sync` changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
}

/// A file that links to or is anchored at a passage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    /// `verse`, `embed` or `anchor` (a margin mark's own passage).
    pub kind: String,
}

fn fnv(text: &str) -> i64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h as i64
}

fn book_no(book: Book) -> i64 {
    Book::all().position(|b| b == book).unwrap_or(0) as i64
}

impl Index {
    pub fn open(path: &Path) -> rusqlite::Result<Index> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Index::init(Connection::open(path)?)
    }

    pub fn in_memory() -> rusqlite::Result<Index> {
        Index::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Index> {
        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA {
            conn.execute_batch(
                "DROP TABLE IF EXISTS links; DROP TABLE IF EXISTS files; DROP TABLE IF EXISTS fts;",
            )?;
        }
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS files(
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                hash INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS links(
                file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                target TEXT NOT NULL,
                book INTEGER, chapter INTEGER, v1 INTEGER, v2 INTEGER,
                module TEXT,
                line INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS links_verse ON links(book, chapter, v1, v2);
             CREATE INDEX IF NOT EXISTS links_file ON links(file_id);
             CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(path UNINDEXED, title, body);
             PRAGMA user_version = {SCHEMA};"
        ))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Index { conn })
    }

    /// Bring the index in line with `files`: only changed files are re-read.
    pub fn sync(&mut self, files: &BTreeMap<String, String>) -> rusqlite::Result<Stats> {
        let mut stats = Stats::default();
        let known: BTreeMap<String, i64> = {
            let mut q = self.conn.prepare("SELECT path, hash FROM files")?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
        };
        let tx = self.conn.transaction()?;
        for gone in known.keys().filter(|p| !files.contains_key(*p)) {
            remove(&tx, gone)?;
            stats.removed += 1;
        }
        for (path, text) in files {
            match known.get(path) {
                Some(&h) if h == fnv(text) => stats.unchanged += 1,
                Some(_) => {
                    put(&tx, path, text)?;
                    stats.updated += 1;
                }
                None => {
                    put(&tx, path, text)?;
                    stats.added += 1;
                }
            }
        }
        tx.commit()?;
        Ok(stats)
    }

    /// Index one file after it was written.
    pub fn update(&mut self, path: &str, text: &str) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        put(&tx, path, text)?;
        tx.commit()
    }

    pub fn remove(&mut self, path: &str) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        remove(&tx, path)?;
        tx.commit()
    }

    pub fn file_count(&self) -> rusqlite::Result<usize> {
        self.conn.query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0)).map(|n| n as usize)
    }

    pub fn link_count(&self) -> rusqlite::Result<usize> {
        self.conn.query_row("SELECT count(*) FROM links", [], |r| r.get::<_, i64>(0)).map(|n| n as usize)
    }

    /// Files that link to, embed, or are anchored at any verse of `range`.
    pub fn touching(&self, range: &RefRange) -> rusqlite::Result<Vec<Hit>> {
        let (a, z) = range.verses.unwrap_or((0, ALL_VERSES));
        let mut q = self.conn.prepare(
            "SELECT f.path, l.line, l.kind FROM links l JOIN files f ON f.id = l.file_id
             WHERE l.book = ?1 AND l.chapter = ?2 AND l.v1 <= ?4 AND l.v2 >= ?3
             ORDER BY f.path, l.line",
        )?;
        let rows = q.query_map(params![book_no(range.book), range.chapter, a, z], |r| {
            Ok(Hit { path: r.get(0)?, line: r.get::<_, i64>(1)? as usize, kind: r.get(2)? })
        })?;
        rows.collect()
    }

    /// Verses of a chapter that any non-margin note links to by number.
    pub fn annotated_verses(&self, book: Book, chapter: u16) -> rusqlite::Result<BTreeSet<u16>> {
        let mut q = self.conn.prepare(
            "SELECT l.v1, l.v2 FROM links l JOIN files f ON f.id = l.file_id
             WHERE l.book = ?1 AND l.chapter = ?2 AND f.kind = 'note' AND l.kind IN ('verse', 'embed')
               AND l.v1 > 0",
        )?;
        let spans = q.query_map(params![book_no(book), chapter], |r| Ok((r.get::<_, u16>(0)?, r.get::<_, u16>(1)?)))?;
        let mut out = BTreeSet::new();
        for span in spans {
            let (a, z) = span?;
            out.extend(a..=z.min(ALL_VERSES - 1));
        }
        Ok(out)
    }

    /// Notes that link to the note called `name` (file stem, case-insensitive).
    pub fn backlinks(&self, name: &str) -> rusqlite::Result<Vec<String>> {
        let mut q = self.conn.prepare(
            "SELECT DISTINCT f.path FROM links l JOIN files f ON f.id = l.file_id
             WHERE l.kind = 'note' AND lower(l.target) = lower(?1) ORDER BY f.path",
        )?;
        let rows = q.query_map(params![name], |r| r.get(0))?;
        rows.collect()
    }

    /// Full-text search over titles and bodies; the best matches first.
    pub fn search(&self, query: &str) -> rusqlite::Result<Vec<String>> {
        // Quote each word so punctuation in what someone types cannot break the query.
        let q: Vec<String> = query
            .split_whitespace()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare("SELECT path FROM fts WHERE fts MATCH ?1 ORDER BY rank")?;
        let rows = stmt.query_map(params![q.join(" ")], |r| r.get(0))?;
        rows.collect()
    }

    pub fn title(&self, path: &str) -> rusqlite::Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.conn.query_row("SELECT title FROM files WHERE path = ?1", params![path], |r| r.get(0)).optional()
    }
}

fn remove(tx: &rusqlite::Transaction<'_>, path: &str) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM links WHERE file_id IN (SELECT id FROM files WHERE path = ?1)", params![path])?;
    tx.execute("DELETE FROM fts WHERE path = ?1", params![path])?;
    tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
    Ok(())
}

fn put(tx: &rusqlite::Transaction<'_>, path: &str, text: &str) -> rusqlite::Result<()> {
    remove(tx, path)?;
    let kind = if path.starts_with("margins/") { "margin" } else { "note" };
    let title = title_of(path, text);
    tx.execute(
        "INSERT INTO files(path, kind, title, hash) VALUES (?1, ?2, ?3, ?4)",
        params![path, kind, title, fnv(text)],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO fts(path, title, body) VALUES (?1, ?2, ?3)", params![path, title, text])?;

    let mut insert = tx.prepare(
        "INSERT INTO links(file_id, kind, target, book, chapter, v1, v2, module, line)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    // A margin mark is anchored at its passage even when its text never links there.
    if let Some(anchor) = anchor_of(text) {
        let (a, z) = anchor.verses.unwrap_or((0, ALL_VERSES));
        insert.execute(params![id, "anchor", anchor.osis(), book_no(anchor.book), anchor.chapter, a, z, None::<String>, 0])?;
    }
    for (n, line) in text.lines().enumerate() {
        let n = n as i64;
        for link in find_links(line) {
            match link.target {
                LinkTarget::Verse { range, module } => {
                    let (a, z) = range.verses.unwrap_or((0, ALL_VERSES));
                    let kind = if link.embed { "embed" } else { "verse" };
                    insert.execute(params![id, kind, range.osis(), book_no(range.book), range.chapter, a, z, module, n + 1])?;
                }
                LinkTarget::Note(name) => {
                    insert.execute(params![id, "note", name, None::<i64>, None::<i64>, None::<i64>, None::<i64>, None::<String>, n + 1])?;
                }
                LinkTarget::Sword { module, key } => {
                    insert.execute(params![id, "sword", format!("{module}/{key}"), None::<i64>, None::<i64>, None::<i64>, None::<i64>, Some(module), n + 1])?;
                }
            }
        }
    }
    Ok(())
}

/// `anchor:` from a margin file's front matter.
fn anchor_of(text: &str) -> Option<RefRange> {
    let rest = text.strip_prefix("---\n")?;
    let front = rest.split("\n---").next()?;
    let value = front.lines().find_map(|l| l.strip_prefix("anchor:"))?;
    RefRange::from_osis(value.trim())
}

/// Front-matter `title:`, else the first `# heading`, else the file name.
fn title_of(path: &str, text: &str) -> String {
    let mut body = text;
    if let Some(rest) = text.strip_prefix("---\n")
        && let Some((front, after)) = rest.split_once("\n---")
    {
        if let Some(t) = front.lines().find_map(|l| l.strip_prefix("title:")) {
            return t.trim().trim_matches('"').to_string();
        }
        body = after;
    }
    if let Some(h) = body.lines().find_map(|l| l.strip_prefix("# ")) {
        return h.trim().to_string();
    }
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> BTreeMap<String, String> {
        list.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect()
    }

    fn r(s: &str) -> RefRange {
        RefRange::parse(s).unwrap()
    }

    fn sample() -> BTreeMap<String, String> {
        files(&[
            ("notes/Nic.md", "---\ntitle: On Nicodemus\n---\n# Nic\nHe came ([[John 3:2]]).\n\n![[John 3:3-5@ASV]]\nSee [[Memory verses]] and [[sword:StrongsGreek/G25]].\n"),
            ("notes/Memory verses.md", "# Memory verses\n\n1. [[John 3:16]]\n2. [[Ps 23]]\nBack to [[Nic]].\n"),
            ("margins/John/03/16-note-s1.md", "---\ntype: margin\nanchor: John.3.16\nstyle: none\ncolor: \"#FFFFFF\"\ntags: []\n---\nSee [[Rom 5:8]].\n"),
        ])
    }

    #[test]
    fn finds_what_touches_a_passage() {
        let mut ix = Index::in_memory().unwrap();
        ix.sync(&sample()).unwrap();
        let paths = |s: &str| ix.touching(&r(s)).unwrap().into_iter().map(|h| h.path).collect::<Vec<_>>();
        assert_eq!(paths("John 3:4"), ["notes/Nic.md"], "inside the embedded range");
        assert_eq!(paths("John 3:16"), ["margins/John/03/16-note-s1.md", "notes/Memory verses.md"]);
        assert_eq!(paths("John 3"), ["margins/John/03/16-note-s1.md", "notes/Memory verses.md", "notes/Nic.md", "notes/Nic.md"]);
        assert_eq!(paths("Ps 23:5"), ["notes/Memory verses.md"], "a chapter link touches every verse");
        assert!(paths("John 4").is_empty());
        let hit = &ix.touching(&r("John 3:2")).unwrap()[0];
        assert_eq!((hit.kind.as_str(), hit.line), ("verse", 5));
    }

    #[test]
    fn annotated_verses_ignore_margin_files_and_chapter_links() {
        let mut ix = Index::in_memory().unwrap();
        ix.sync(&sample()).unwrap();
        let v = ix.annotated_verses(Book::JOHN, 3).unwrap();
        assert_eq!(v.into_iter().collect::<Vec<_>>(), [2, 3, 4, 5, 16]);
        assert!(ix.annotated_verses(Book::PSALMS, 23).unwrap().is_empty());
    }

    #[test]
    fn backlinks_titles_and_search() {
        let mut ix = Index::in_memory().unwrap();
        ix.sync(&sample()).unwrap();
        assert_eq!(ix.backlinks("nic").unwrap(), ["notes/Memory verses.md"]);
        assert_eq!(ix.backlinks("Memory verses").unwrap(), ["notes/Nic.md"]);
        assert_eq!(ix.title("notes/Nic.md").unwrap().as_deref(), Some("On Nicodemus"));
        assert_eq!(ix.title("notes/Memory verses.md").unwrap().as_deref(), Some("Memory verses"));
        assert_eq!(ix.search("nicodemus").unwrap(), ["notes/Nic.md"]);
        assert_eq!(ix.search("shepherd-ish \"quote").unwrap(), Vec::<String>::new());
        assert!(ix.search("   ").unwrap().is_empty());
    }

    #[test]
    fn sync_is_incremental() {
        let mut ix = Index::in_memory().unwrap();
        let mut f = sample();
        assert_eq!(ix.sync(&f).unwrap(), Stats { added: 3, ..Stats::default() });
        assert_eq!(ix.sync(&f).unwrap(), Stats { unchanged: 3, ..Stats::default() });
        f.get_mut("notes/Nic.md").unwrap().push_str("More about [[Luke 1:1]].\n");
        f.remove("margins/John/03/16-note-s1.md");
        assert_eq!(ix.sync(&f).unwrap(), Stats { updated: 1, removed: 1, unchanged: 1, added: 0 });
        assert_eq!(ix.file_count().unwrap(), 2);
        assert!(ix.touching(&r("Rom 5:8")).unwrap().is_empty(), "a removed file's links go with it");
        assert_eq!(ix.touching(&r("Luke 1:1")).unwrap().len(), 1);
    }

    #[test]
    fn the_database_is_rebuildable_from_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cache/index.sqlite");
        let f = sample();
        let mut first = Index::open(&db).unwrap();
        first.sync(&f).unwrap();
        let counts = (first.file_count().unwrap(), first.link_count().unwrap());
        let hits = first.touching(&r("John 3")).unwrap();
        drop(first);
        std::fs::remove_file(&db).unwrap();
        let mut again = Index::open(&db).unwrap();
        again.sync(&f).unwrap();
        assert_eq!((again.file_count().unwrap(), again.link_count().unwrap()), counts);
        assert_eq!(again.touching(&r("John 3")).unwrap(), hits);
    }

    #[test]
    fn a_reopened_database_keeps_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("index.sqlite");
        Index::open(&db).unwrap().sync(&sample()).unwrap();
        let mut ix = Index::open(&db).unwrap();
        assert_eq!(ix.file_count().unwrap(), 3);
        assert_eq!(ix.sync(&sample()).unwrap().unchanged, 3);
    }

    #[test]
    fn an_older_schema_is_dropped_and_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("index.sqlite");
        Index::open(&db).unwrap().sync(&sample()).unwrap();
        Connection::open(&db).unwrap().execute_batch("PRAGMA user_version = 0;").unwrap();
        let mut ix = Index::open(&db).unwrap();
        assert_eq!(ix.file_count().unwrap(), 0);
        assert_eq!(ix.sync(&sample()).unwrap().added, 3);
    }

    #[test]
    fn single_file_updates_and_removals() {
        let mut ix = Index::in_memory().unwrap();
        ix.update("notes/A.md", "[[John 1:1]]").unwrap();
        ix.update("notes/A.md", "[[John 1:2]]").unwrap();
        assert!(ix.touching(&r("John 1:1")).unwrap().is_empty());
        assert_eq!(ix.touching(&r("John 1:2")).unwrap().len(), 1);
        ix.remove("notes/A.md").unwrap();
        assert_eq!(ix.file_count().unwrap(), 0);
        assert_eq!(ix.link_count().unwrap(), 0);
    }
}
