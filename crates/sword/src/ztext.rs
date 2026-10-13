//! zText: compressed Bible/commentary modules (docs/01 §3). Read-only.
//!
//! Per testament, three files: `.bzs` (block table), `.bzv` (verse index), `.bzz` (the
//! compressed blocks). A verse's slot in `.bzv` follows from the KJV versification:
//! module heading, testament heading, then per book: book heading, then per chapter a
//! chapter heading followed by its verses.

use crate::canon_data::VERSES;
use flate2::read::ZlibDecoder;
use scripture::Book;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const OT_BOOKS: usize = 39;

/// A decompressed block, shared between readers.
type Block = Arc<Vec<u8>>;

pub struct ZText {
    dir: PathBuf,
    /// zText4 uses 4-byte verse lengths where zText uses 2.
    long_lengths: bool,
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

pub fn chapter_count(book: Book) -> u16 {
    VERSES[book_index(book)].len() as u16
}

pub fn verse_count(book: Book, chapter: u16) -> u16 {
    VERSES[book_index(book)].get(chapter as usize - 1).copied().unwrap_or(0) as u16
}

fn book_index(book: Book) -> usize {
    Book::all().position(|b| b == book).expect("every Book is in the canon")
}

/// Slot of `(book, chapter, verse)` in its testament's verse index. Verse 0 is the chapter
/// heading and chapter 0 the book heading.
fn slot(book_idx: usize, chapter: u16, verse: u16) -> usize {
    let (first, testament_books) = if book_idx < OT_BOOKS { (0, 0..OT_BOOKS) } else { (OT_BOOKS, OT_BOOKS..66) };
    debug_assert!(testament_books.contains(&book_idx));
    let book_len = |b: usize| 1 + VERSES[b].iter().map(|&n| 1 + n as usize).sum::<usize>();
    let before: usize = (first..book_idx).map(book_len).sum();
    let mut at = 2 + before;
    if chapter == 0 {
        return at;
    }
    at += 1;
    at += VERSES[book_idx][..chapter as usize - 1].iter().map(|&n| 1 + n as usize).sum::<usize>();
    at + verse as usize
}

impl ZText {
    pub fn open(dir: PathBuf, long_lengths: bool) -> ZText {
        ZText { dir, long_lengths, index: Default::default(), blocks: Default::default() }
    }

    pub fn has_testament(&self, nt: bool) -> bool {
        self.dir.join(if nt { "nt.bzv" } else { "ot.bzv" }).is_file()
    }

    pub fn has_book(&self, book: Book) -> bool {
        self.has_testament(book_index(book) >= OT_BOOKS)
    }

    /// The raw (marked-up) entry, or `None` when the slot is empty or the module lacks it.
    pub fn read(&self, book: Book, chapter: u16, verse: u16) -> io::Result<Option<String>> {
        let bi = book_index(book);
        let t = usize::from(bi >= OT_BOOKS);
        if !self.has_testament(t == 1) {
            return Ok(None);
        }
        let index = self.index_file(t)?;
        let rec = if self.long_lengths { 12 } else { 10 };
        let at = slot(bi, chapter, verse) * rec;
        if at + rec > index.len() {
            return Ok(None);
        }
        let block = u32_at(&index, at)?;
        let offset = u32_at(&index, at + 4)? as usize;
        let len = if self.long_lengths {
            u32_at(&index, at + 8)? as usize
        } else {
            u16::from_le_bytes([index[at + 8], index[at + 9]]) as usize
        };
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
        let name = if t == 0 { "ot.bzv" } else { "nt.bzv" };
        let bytes = Arc::new(fs::read(self.dir.join(name))?);
        *slot = Some(bytes.clone());
        Ok(bytes)
    }

    fn block(&self, t: usize, n: u32) -> io::Result<Arc<Vec<u8>>> {
        if let Some(b) = self.blocks.lock().unwrap().get(&(t, n)) {
            return Ok(b.clone());
        }
        let stem = if t == 0 { "ot" } else { "nt" };
        let table = fs::read(self.dir.join(format!("{stem}.bzs")))?;
        let entry = n as usize * 12;
        let offset = u32_at(&table, entry)? as u64;
        let size = u32_at(&table, entry + 4)? as usize;
        let mut compressed = vec![0u8; size];
        {
            use std::io::{Seek, SeekFrom};
            let mut f = fs::File::open(self.dir.join(format!("{stem}.bzz")))?;
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
        // ot.bzv has 24115 entries and nt.bzv 8246 in every KJV-versified zText module.
        assert_eq!(slot(OT_BOOKS - 1, 4, 6) + 1, 24115);
        assert_eq!(slot(65, 22, 21) + 1, 8246);
    }

    #[test]
    fn heading_slots_come_before_their_verses() {
        assert_eq!(slot(0, 0, 0), 2);
        assert_eq!(slot(0, 1, 0), 3);
        assert_eq!(slot(0, 1, 1), 4);
        assert_eq!(slot(0, 2, 0), 4 + 31);
    }
}
