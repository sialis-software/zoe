//! zText: compressed Bible/commentary modules (docs/01 §3). Read-only.
//!
//! Per testament, three files: `.bzs` (block table), `.bzv` (verse index), `.bzz` (the
//! compressed blocks). A verse's slot in `.bzv` follows from the KJV versification:
//! module heading, testament heading, then per book: book heading, then per chapter a
//! chapter heading followed by its verses.

use crate::canon_data::VERSES;
use flate2::read::ZlibDecoder;
use scripture::{Book, Versification};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const OT_BOOKS: usize = 39;
/// 3 John and Revelation, which the NRSV numbers differently (their places among the books).
const THREE_JOHN: usize = 63;
const REVELATION: usize = 65;
/// The books of the Protestant canon: those from here on are the Apocrypha.
const CANON_BOOKS: usize = 66;

/// A decompressed block, shared between readers.
type Block = Arc<Vec<u8>>;

pub struct ZText {
    dir: PathBuf,
    /// zText4 uses 4-byte verse lengths where zText uses 2.
    long_lengths: bool,
    /// The letter in the files' extensions: `b` for a Bible (`ot.bzv`), `c` for a commentary.
    kind: char,
    versification: Versification,
    index: [Mutex<Option<Arc<Vec<u8>>>>; 2],
    blocks: Mutex<HashMap<(usize, u32), Block>>,
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn u32_at(b: &[u8], at: usize) -> io::Result<u32> {
    let s = b.get(at..at + 4).ok_or_else(|| bad("truncated index"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The verses of each chapter of a book in a versification: the table's, but for the few places
/// where the NRSV numbers differently (3 John has 15 verses, Revelation 12 has 18).
fn counts(v: Versification, book_idx: usize) -> Cow<'static, [u8]> {
    match (v, VERSES[book_idx]) {
        (Versification::Nrsv, [14]) if book_idx == THREE_JOHN => Cow::Owned(vec![15]),
        (Versification::Nrsv, table) if book_idx == REVELATION => {
            let mut table = table.to_vec();
            table[11] = 18;
            Cow::Owned(table)
        }
        (_, table) => Cow::Borrowed(table),
    }
}

/// How many chapters a book has (the same in every versification read).
pub fn chapter_count(book: Book) -> u16 {
    VERSES[book_index(book)].len() as u16
}

/// How many verses a chapter has in the KJV.
pub fn verse_count(book: Book, chapter: u16) -> u16 {
    verse_count_in(Versification::Kjv, book, chapter)
}

/// How many verses a chapter has in `versification`.
pub fn verse_count_in(v: Versification, book: Book, chapter: u16) -> u16 {
    counts(v, book_index(book)).get(chapter as usize - 1).copied().unwrap_or(0) as u16
}

fn book_index(book: Book) -> usize {
    Book::all().position(|b| b == book).expect("every Book is in the canon")
}

/// Whether a book is stored in the New Testament's file (the Apocrypha are in the Old Testament's,
/// after Malachi).
fn in_nt(book_idx: usize) -> bool {
    (OT_BOOKS..CANON_BOOKS).contains(&book_idx)
}

/// The books of a testament's file, in the order they are stored.
fn testament_books(nt: bool) -> Vec<usize> {
    if nt { (OT_BOOKS..CANON_BOOKS).collect() } else { (0..OT_BOOKS).chain(CANON_BOOKS..VERSES.len()).collect() }
}

/// Slot of `(book, chapter, verse)` in its testament's verse index. Verse 0 is the chapter
/// heading and chapter 0 the book heading.
fn slot(v: Versification, book_idx: usize, chapter: u16, verse: u16) -> usize {
    let book_len = |b: usize| 1 + counts(v, b).iter().map(|&n| 1 + n as usize).sum::<usize>();
    let before: usize = testament_books(in_nt(book_idx)).into_iter().take_while(|&b| b != book_idx).map(book_len).sum();
    let mut at = 2 + before;
    if chapter == 0 {
        return at;
    }
    at += 1;
    at += counts(v, book_idx)[..chapter as usize - 1].iter().map(|&n| 1 + n as usize).sum::<usize>();
    at + verse as usize
}

impl ZText {
    pub fn open(dir: PathBuf, long_lengths: bool, versification: Versification) -> ZText {
        ZText::open_as(dir, long_lengths, 'b', versification)
    }

    pub fn open_as(dir: PathBuf, long_lengths: bool, kind: char, versification: Versification) -> ZText {
        ZText { dir, long_lengths, kind, versification, index: Default::default(), blocks: Default::default() }
    }

    /// How many verses a chapter has in this module's versification.
    pub fn verse_count(&self, book: Book, chapter: u16) -> u16 {
        verse_count_in(self.versification, book, chapter)
    }

    /// The file named `stem` with this module's extension `z` + `ext`: `ot.czv`.
    fn file(&self, stem: &str, ext: char) -> PathBuf {
        self.dir.join(format!("{stem}.{}z{ext}", self.kind))
    }

    pub fn has_testament(&self, nt: bool) -> bool {
        self.file(if nt { "nt" } else { "ot" }, 'v').is_file()
    }

    /// Whether the module has anything of the book: its testament's files are there and some entry of the book
    /// is not empty. (A module can have only some books: a New Testament may come with an Old Testament's files
    /// that are empty, and one with the Apocrypha may have some of its 14 books.)
    pub fn has_book(&self, book: Book) -> bool {
        let bi = book_index(book);
        let nt = in_nt(bi);
        if !self.has_testament(nt) {
            return false;
        }
        let Ok(index) = self.index_file(usize::from(nt)) else { return false };
        let rec = if self.long_lengths { 12 } else { 10 };
        let first = slot(self.versification, bi, 0, 0);
        let slots = 1 + counts(self.versification, bi).iter().map(|&n| 1 + n as usize).sum::<usize>();
        (first..first + slots).take_while(|s| (s + 1) * rec <= index.len()).any(|s| self.entry_len(&index, s * rec) > 0)
    }

    /// The length of the entry whose record is at `at` in an index.
    fn entry_len(&self, index: &[u8], at: usize) -> usize {
        if self.long_lengths {
            u32_at(index, at + 8).map_or(0, |n| n as usize)
        } else {
            u16::from_le_bytes([index[at + 8], index[at + 9]]) as usize
        }
    }

    /// The raw (marked-up) entry, or `None` when the slot is empty or the module lacks it.
    pub fn read(&self, book: Book, chapter: u16, verse: u16) -> io::Result<Option<String>> {
        let bi = book_index(book);
        let t = usize::from(in_nt(bi));
        if !self.has_testament(t == 1) {
            return Ok(None);
        }
        let index = self.index_file(t)?;
        let rec = if self.long_lengths { 12 } else { 10 };
        let at = slot(self.versification, bi, chapter, verse) * rec;
        if at + rec > index.len() {
            return Ok(None);
        }
        let block = u32_at(&index, at)?;
        let offset = u32_at(&index, at + 4)? as usize;
        let len = self.entry_len(&index, at);
        if len == 0 {
            return Ok(None);
        }
        let data = self.block(t, block)?;
        let bytes = data.get(offset..offset + len).ok_or_else(|| bad("verse outside its block"))?;
        Ok(Some(String::from_utf8_lossy(bytes).trim_matches(['\0', '\n', '\r']).to_string()))
    }

    fn index_file(&self, t: usize) -> io::Result<Arc<Vec<u8>>> {
        let mut slot = self.index[t].lock().unwrap();
        if let Some(i) = &*slot {
            return Ok(i.clone());
        }
        let bytes = Arc::new(fs::read(self.file(if t == 0 { "ot" } else { "nt" }, 'v'))?);
        *slot = Some(bytes.clone());
        Ok(bytes)
    }

    fn block(&self, t: usize, n: u32) -> io::Result<Arc<Vec<u8>>> {
        if let Some(b) = self.blocks.lock().unwrap().get(&(t, n)) {
            return Ok(b.clone());
        }
        let stem = if t == 0 { "ot" } else { "nt" };
        let table = fs::read(self.file(stem, 's'))?;
        let entry = n as usize * 12;
        let offset = u32_at(&table, entry)? as u64;
        let size = u32_at(&table, entry + 4)? as usize;
        let mut compressed = vec![0u8; size];
        {
            use std::io::{Seek, SeekFrom};
            let mut f = fs::File::open(self.file(stem, 'z'))?;
            f.seek(SeekFrom::Start(offset))?;
            f.read_exact(&mut compressed)?;
        }
        let mut out = Vec::new();
        ZlibDecoder::new(&compressed[..]).read_to_end(&mut out)?;
        let out = Arc::new(out);
        self.blocks.lock().unwrap().insert((t, n), out.clone());
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_match_the_index_size_of_real_modules() {
        // ot.bzv has 24115 entries and nt.bzv 8246 in every KJV-versified zText module, and 8248 in an NRSV one.
        let kjv = Versification::Kjv;
        assert_eq!(slot(kjv, OT_BOOKS - 1, 4, 6) + 1, 24115);
        assert_eq!(slot(kjv, 65, 22, 21) + 1, 8246);
        assert_eq!(slot(Versification::Nrsv, OT_BOOKS - 1, 4, 6) + 1, 24115);
        assert_eq!(slot(Versification::Nrsv, 65, 22, 21) + 1, 8248);
    }

    #[test]
    fn the_nrsv_has_the_two_extra_verses_where_the_kjv_does_not() {
        let (kjv, nrsv) = (Versification::Kjv, Versification::Nrsv);
        let book = |osis: &str| Book::from_osis(osis).unwrap();
        assert_eq!((verse_count_in(kjv, book("3John"), 1), verse_count_in(nrsv, book("3John"), 1)), (14, 15));
        assert_eq!((verse_count_in(kjv, book("Rev"), 12), verse_count_in(nrsv, book("Rev"), 12)), (17, 18));
        for (osis, chapters) in [("John", 21), ("Rev", 22), ("Gen", 50)] {
            for c in 1..=chapters {
                if (osis, c) != ("Rev", 12) {
                    assert_eq!(verse_count_in(kjv, book(osis), c), verse_count_in(nrsv, book(osis), c), "{osis} {c}");
                }
            }
        }
    }

    #[test]
    fn heading_slots_come_before_their_verses() {
        let kjv = Versification::Kjv;
        assert_eq!(slot(kjv, 0, 0, 0), 2);
        assert_eq!(slot(kjv, 0, 1, 0), 3);
        assert_eq!(slot(kjv, 0, 1, 1), 4);
        assert_eq!(slot(kjv, 0, 2, 0), 4 + 31);
    }
}
