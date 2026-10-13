//! Dictionary / lexicon modules: RawLD and zLD (docs/01 §3). Read-only.
//!
//! Both keep a key index (`.idx`) and entry text (`.dat`). RawLD stores each entry as
//! `key`, a newline, then the body. zLD's `.dat` only holds `key` plus the entry's place
//! `(block, index)`, and the bodies live in zlib blocks in `.zdt` found through `.zdx`.

use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::OnceLock;

enum Place {
    /// Byte range of the whole entry in `.dat`.
    Raw { offset: usize, size: usize },
    Block { block: usize, entry: usize },
}

pub struct Lexicon {
    stem: PathBuf,
    compressed: bool,
    /// Lower-cased key to where its entry is.
    keys: OnceLock<HashMap<String, Place>>,
    dat: OnceLock<Vec<u8>>,
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn u32_at(b: &[u8], at: usize) -> io::Result<usize> {
    let s = b.get(at..at + 4).ok_or_else(|| bad("truncated"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
}

impl Lexicon {
    /// `stem` is the data path without extension (`…/dict` for `dict.idx`, `dict.dat`).
    pub fn open(stem: PathBuf, compressed: bool) -> Lexicon {
        Lexicon { stem, compressed, keys: OnceLock::new(), dat: OnceLock::new() }
    }

    fn file(&self, ext: &str) -> PathBuf {
        let mut name = self.stem.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".");
        name.push(ext);
        self.stem.with_file_name(name)
    }

    fn dat(&self) -> io::Result<&Vec<u8>> {
        if self.dat.get().is_none() {
            let _ = self.dat.set(fs::read(self.file("dat"))?);
        }
        Ok(self.dat.get().unwrap())
    }

    fn keys(&self) -> io::Result<&HashMap<String, Place>> {
        if let Some(k) = self.keys.get() {
            return Ok(k);
        }
        let idx = fs::read(self.file("idx"))?;
        let dat = self.dat()?;
        let mut map = HashMap::new();
        let record = if self.compressed { 8 } else { 6 };
        for rec in idx.chunks_exact(record) {
            let offset = u32_at(rec, 0)?;
            let size = if self.compressed { u32_at(rec, 4)? } else { u16::from_le_bytes([rec[4], rec[5]]) as usize };
            let Some(entry) = dat.get(offset..offset + size) else { continue };
            let (key, place) = if self.compressed {
                let Some(split) = entry.len().checked_sub(8) else { continue };
                (&entry[..split], Place::Block { block: u32_at(entry, split)?, entry: u32_at(entry, split + 4)? })
            } else {
                let end = entry.iter().position(|&b| b == b'\n').unwrap_or(entry.len());
                (&entry[..end], Place::Raw { offset, size })
            };
            let key = String::from_utf8_lossy(key);
            let key = key.trim_end_matches(['\r', '\n', '\0', '\\']).trim().to_lowercase();
            map.entry(key).or_insert(place);
        }
        let _ = self.keys.set(map);
        Ok(self.keys.get().unwrap())
    }

    /// The entry's markup for `key` (case-insensitive), or `None` if there is none.
    pub fn entry(&self, key: &str) -> io::Result<Option<String>> {
        let Some(place) = self.keys()?.get(&key.trim().to_lowercase()) else { return Ok(None) };
        let bytes = match *place {
            Place::Raw { offset, size } => {
                let raw = &self.dat()?[offset..offset + size];
                let start = raw.iter().position(|&b| b == b'\n').map_or(raw.len(), |i| i + 1);
                raw[start..].to_vec()
            }
            Place::Block { block, entry } => self.block_entry(block, entry)?,
        };
        Ok(Some(String::from_utf8_lossy(&bytes).trim_matches(['\0', '\r', '\n']).to_string()))
    }

    fn block_entry(&self, block: usize, entry: usize) -> io::Result<Vec<u8>> {
        let zdx = fs::read(self.file("zdx"))?;
        let offset = u32_at(&zdx, block * 8)?;
        let size = u32_at(&zdx, block * 8 + 4)?;
        let mut compressed = vec![0u8; size];
        {
            use std::io::{Seek, SeekFrom};
            let mut f = fs::File::open(self.file("zdt"))?;
            f.seek(SeekFrom::Start(offset as u64))?;
            f.read_exact(&mut compressed)?;
        }
        let mut data = Vec::new();
        ZlibDecoder::new(&compressed[..]).read_to_end(&mut data)?;
        if entry >= u32_at(&data, 0)? {
            return Err(bad("entry outside its block"));
        }
        let start = u32_at(&data, 4 + entry * 8)?;
        let len = u32_at(&data, 8 + entry * 8)?;
        data.get(start..start + len).map(<[u8]>::to_vec).ok_or_else(|| bad("entry outside its block"))
    }

    /// Every key, lower-cased (for tests and tools).
    pub fn key_list(&self) -> io::Result<Vec<String>> {
        let mut k: Vec<String> = self.keys()?.keys().cloned().collect();
        k.sort();
        Ok(k)
    }
}
