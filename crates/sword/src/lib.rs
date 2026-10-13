//! Read-only access to CrossWire SWORD module libraries (docs/01).
//!
//! A *library* is a directory holding `mods.d/*.conf` and `modules/…`; the same layout
//! `~/.sword`, Xiphos and `installmgr` use. Nothing here ever writes to one.

mod canon_data;
mod conf;
mod entry;
mod lexicon;
mod osis;
mod ztext;

pub use ztext::{chapter_count, verse_count};

pub use conf::Conf;
pub use entry::{Entry, parse_entry};
pub use lexicon::Lexicon;

use scripture::Book;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use study_core::ast::Chapter;

/// Why a module or passage could not be read.
#[derive(Debug)]
pub enum Error {
    /// A valid module we cannot decode yet (driver, compression, versification).
    Unsupported(String),
    Locked(String),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::Locked(m) => write!(f, "{m} is locked and needs a key"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Where modules are looked for, in search order: `SWORD_PATH` and the app's own folder
/// `~/.config/sword`. `BIBLE_STUDY_SWORD_PATH` (colon-separated) replaces both.
pub fn default_roots() -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("BIBLE_STUDY_SWORD_PATH") {
        return std::env::split_paths(&v).collect();
    }
    let mut roots = Vec::new();
    if let Some(v) = std::env::var_os("SWORD_PATH") {
        roots.extend(std::env::split_paths(&v));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".config").join("sword"));
    }
    roots
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bible,
    Lexicon,
    Commentary,
    Other,
}

/// One installed module: its conf and where its data lives.
#[derive(Clone, Debug)]
pub struct Module {
    pub conf: Conf,
    pub root: PathBuf,
}

impl Module {
    pub fn name(&self) -> &str {
        &self.conf.name
    }

    pub fn description(&self) -> &str {
        self.conf.get("Description").unwrap_or(self.name())
    }

    pub fn has_strongs(&self) -> bool {
        self.conf.all("Feature").any(|f| f == "StrongsNumbers")
    }

    pub fn kind(&self) -> Kind {
        match self.conf.get("ModDrv") {
            Some("zText" | "zText4" | "RawText" | "RawText4") => Kind::Bible,
            Some("zLD" | "RawLD" | "RawLD4") => Kind::Lexicon,
            Some("zCom" | "zCom4") => Kind::Commentary,
            _ => Kind::Other,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.conf.get("CipherKey").is_some_and(|k| k.trim().is_empty())
    }

    /// Open as a Bible, or say why we cannot.
    pub fn bible(&self) -> Result<Bible, Error> {
        let driver = self.conf.get("ModDrv").unwrap_or("");
        if !matches!(driver, "zText" | "zText4") {
            return Err(Error::Unsupported(format!("{} uses the {driver} driver", self.name())));
        }
        if self.is_locked() {
            return Err(Error::Locked(self.name().to_string()));
        }
        let compression = self.conf.get("CompressType").unwrap_or("LZSS");
        if compression != "ZIP" {
            return Err(Error::Unsupported(format!("{compression} compression in {}", self.name())));
        }
        let block = self.conf.get("BlockType").unwrap_or("CHAPTER");
        if block != "BOOK" && block != "CHAPTER" && block != "VERSE" {
            return Err(Error::Unsupported(format!("{block} blocks")));
        }
        let versification = self.conf.get("Versification").unwrap_or("KJV");
        if versification != "KJV" {
            return Err(Error::Unsupported(format!("{} uses the {versification} versification", self.name())));
        }
        if !matches!(self.conf.get("SourceType"), Some("OSIS")) {
            return Err(Error::Unsupported(format!("{} markup in {}", self.conf.get("SourceType").unwrap_or("unknown"), self.name())));
        }
        let dir = self.conf.data_dir(&self.root).ok_or_else(|| Error::Unsupported("no DataPath".into()))?;
        Ok(Bible { zt: ztext::ZText::open(dir, driver == "zText4") })
    }
}

impl Module {
    /// Open as a commentary (verse by verse, stored as a Bible is), or say why we cannot.
    pub fn commentary(&self) -> Result<Commentary, Error> {
        let driver = self.conf.get("ModDrv").unwrap_or("");
        if !matches!(driver, "zCom" | "zCom4") {
            return Err(Error::Unsupported(format!("{} uses the {driver} driver", self.name())));
        }
        if self.is_locked() {
            return Err(Error::Locked(self.name().to_string()));
        }
        let compression = self.conf.get("CompressType").unwrap_or("LZSS");
        if compression != "ZIP" {
            return Err(Error::Unsupported(format!("{compression} compression in {}", self.name())));
        }
        let versification = self.conf.get("Versification").unwrap_or("KJV");
        if versification != "KJV" {
            return Err(Error::Unsupported(format!("{} uses the {versification} versification", self.name())));
        }
        let dir = self.conf.data_dir(&self.root).ok_or_else(|| Error::Unsupported("no DataPath".into()))?;
        // The files are `.czv` for some commentaries and `.bzv` (as a Bible's) for others.
        let kind = if dir.join("nt.czv").is_file() || dir.join("ot.czv").is_file() { 'c' } else { 'b' };
        Ok(Commentary { zt: ztext::ZText::open_as(dir, driver == "zCom4", kind) })
    }
}

/// A commentary: notes on passages, kept like a Bible's verses.
pub struct Commentary {
    zt: ztext::ZText,
}

impl Commentary {
    pub fn has_book(&self, book: Book) -> bool {
        self.zt.has_book(book)
    }

    /// The raw marked-up note at one verse (0 is the chapter's introduction).
    pub fn raw(&self, book: Book, chapter: u16, verse: u16) -> Result<Option<String>, Error> {
        Ok(self.zt.read(book, chapter, verse)?)
    }
}

impl Module {
    /// Open as a dictionary, or say why we cannot.
    pub fn lexicon(&self) -> Result<Lexicon, Error> {
        let driver = self.conf.get("ModDrv").unwrap_or("");
        let compressed = match driver {
            "zLD" => true,
            "RawLD" => false,
            _ => return Err(Error::Unsupported(format!("{} uses the {driver} driver", self.name()))),
        };
        if self.is_locked() {
            return Err(Error::Locked(self.name().to_string()));
        }
        if compressed && self.conf.get("CompressType").unwrap_or("LZSS") != "ZIP" {
            return Err(Error::Unsupported(format!("{} compression in {}", self.conf.get("CompressType").unwrap_or("LZSS"), self.name())));
        }
        let stem = self.conf.data_dir(&self.root).ok_or_else(|| Error::Unsupported("no DataPath".into()))?;
        let stem = PathBuf::from(stem.to_string_lossy().trim_end_matches('/'));
        Ok(Lexicon::open(stem, compressed))
    }
}

pub struct Bible {
    zt: ztext::ZText,
}

impl Bible {
    pub fn has_book(&self, book: Book) -> bool {
        self.zt.has_book(book)
    }

    /// The raw marked-up entry for one verse (0 is the chapter heading).
    pub fn raw(&self, book: Book, chapter: u16, verse: u16) -> Result<Option<String>, Error> {
        Ok(self.zt.read(book, chapter, verse)?)
    }

    /// The chapter's text, converted to the shared AST. Empty verses are skipped.
    pub fn chapter(&self, book: Book, chapter: u16) -> Result<Chapter, Error> {
        let verses = ztext::verse_count(book, chapter);
        if verses == 0 || !self.has_book(book) {
            return Err(Error::Unsupported(format!("{book} {chapter} is not in this module")));
        }
        let mut b = osis::ChapterBuilder::new(book, chapter);
        for v in 0..=verses {
            if let Some(raw) = self.zt.read(book, chapter, v)? {
                b.verse(v, &raw);
            }
        }
        Ok(b.finish())
    }

    /// Every chapter the module contains, in canon order.
    pub fn chapters(&self) -> Vec<(Book, u16)> {
        Book::all()
            .filter(|b| self.has_book(*b))
            .flat_map(|b| (1..=ztext::chapter_count(b)).map(move |c| (b, c)))
            .collect()
    }
}

/// Every module found under a set of library roots. The first root to provide a name wins.
pub struct Library {
    modules: BTreeMap<String, Module>,
}

impl Library {
    pub fn open(roots: &[PathBuf]) -> Library {
        let mut modules = BTreeMap::new();
        for root in roots {
            for conf in read_confs(root) {
                modules.entry(conf.name.to_lowercase()).or_insert(Module { conf, root: root.clone() });
            }
        }
        Library { modules }
    }

    pub fn discover() -> Library {
        Library::open(&default_roots())
    }

    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }

    pub fn get(&self, name: &str) -> Option<&Module> {
        self.modules.get(&name.to_lowercase())
    }
}

fn read_confs(root: &Path) -> Vec<Conf> {
    let Ok(dir) = std::fs::read_dir(root.join("mods.d")) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "conf"))
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .filter_map(|bytes| Conf::parse(&String::from_utf8_lossy(&bytes)))
        .collect()
}
