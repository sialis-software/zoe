//! Read-only access to CrossWire SWORD module libraries (docs/01).
//!
//! A *library* is a directory holding `mods.d/*.conf` and `modules/…`; the same layout
//! `~/.sword`, Xiphos and `installmgr` use. Reading never writes to one; only [`package`]
//! installs, removes and exports modules.

mod canon_data;
mod conf;
mod entry;
mod lexicon;
mod osis;
pub mod package;
mod ztext;

pub use ztext::{chapter_count, verse_count, verse_count_in};

pub use conf::Conf;
pub use entry::{Entry, parse_entry};
pub use lexicon::Lexicon;

use scripture::{Book, Versification};
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

/// The search path the environment sets, if it does: it replaces every other place.
pub fn path_override() -> Option<std::ffi::OsString> {
    std::env::var_os("ZOE_SWORD_PATH").or_else(|| std::env::var_os("BIBLE_STUDY_SWORD_PATH"))
}

/// Where modules are looked for, in search order: `SWORD_PATH` and the app's own folder
/// [`own_folder`] (`~/.config/sword`). `ZOE_SWORD_PATH` (colon-separated; the older name `BIBLE_STUDY_SWORD_PATH`
/// also works) replaces both.
pub fn default_roots() -> Vec<PathBuf> {
    if let Some(v) = path_override() {
        return std::env::split_paths(&v).collect();
    }
    let mut roots = Vec::new();
    if let Some(v) = std::env::var_os("SWORD_PATH") {
        roots.extend(std::env::split_paths(&v));
    }
    roots.extend(own_folder());
    roots
}

/// The folder of modules that is the app's own: `sword` in the user's config folder
/// (`$XDG_CONFIG_HOME/sword`, which is `~/.config/sword`; `%APPDATA%\sword` on Windows), where other SWORD programs look
/// too. Where the app has been given a home of its own (`ZOE_CONFIG_HOME`, or `ZOE_HOME`: a sandbox) it is `sword` in
/// that, and a build for developing (a debug build) keeps its modules apart in `zoe-dev/sword`: neither adds to
/// or reads from the modules the person uses.
pub fn own_folder() -> Option<PathBuf> {
    let set = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(config) = set("ZOE_CONFIG_HOME") {
        return Some(config.join("sword"));
    }
    if let Some(home) = set("ZOE_HOME").or_else(|| set("BIBLE_STUDY_HOME")) {
        return Some(home.join("config").join("sword"));
    }
    let apart = cfg!(debug_assertions);
    if cfg!(windows)
        && let Some(appdata) = set("APPDATA")
    {
        return Some(if apart { appdata.join("zoe-dev").join("sword") } else { appdata.join("sword") });
    }
    let config = set("XDG_CONFIG_HOME").or_else(|| set("HOME").or_else(|| set("USERPROFILE")).map(|h| h.join(".config")))?;
    Some(if apart { config.join("zoe-dev").join("sword") } else { config.join("sword") })
}

/// The folder of modules that SWORD programs share (`~/.config/sword`), whatever kind of build this is: what a build for
/// developing leaves alone, but can be read.
pub fn shared_folder() -> Option<PathBuf> {
    let set = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows)
        && let Some(appdata) = set("APPDATA")
    {
        return Some(appdata.join("sword"));
    }
    Some(set("XDG_CONFIG_HOME").or_else(|| set("HOME").or_else(|| set("USERPROFILE")).map(|h| h.join(".config")))?.join("sword"))
}

/// Where the tests look for real modules (which some of them need, and skip without): `ZOE_TEST_SWORD_PATH` (colon
/// separated) if it is set, else the usual places and the shared folder. Only ever read from.
pub fn test_roots() -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("ZOE_TEST_SWORD_PATH").filter(|v| !v.is_empty()) {
        return std::env::split_paths(&v).collect();
    }
    let mut roots = default_roots();
    if let Some(root) = shared_folder()
        && !roots.contains(&root)
    {
        roots.push(root);
    }
    roots
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bible,
    Lexicon,
    /// A daily devotional (`Feature=DailyDevotion`). Only recognised: it is stored like a dictionary and
    /// would be taken for one, but the app does not read devotionals.
    Devotional,
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
        // A devotional is stored as a dictionary, so it is told apart (to be left alone) before the driver is looked at.
        if self.conf.all("Feature").any(|f| f == "DailyDevotion") {
            return Kind::Devotional;
        }
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
        let versification = self.versification()?;
        if !matches!(self.conf.get("SourceType"), Some("OSIS")) {
            return Err(Error::Unsupported(format!("{} markup in {}", self.conf.get("SourceType").unwrap_or("unknown"), self.name())));
        }
        let dir = self.conf.data_dir(&self.root).ok_or_else(|| Error::Unsupported("no DataPath".into()))?;
        Ok(Bible { zt: ztext::ZText::open(dir, driver == "zText4", versification) })
    }
}

impl Module {
    /// How the module numbers its chapters and verses, or why that is not read: the KJV's, the KJV's with the
    /// Apocrypha (`KJVA`) and SWORD's NRSV are.
    pub fn versification(&self) -> Result<Versification, Error> {
        let name = self.conf.get("Versification").unwrap_or("KJV");
        Versification::from_name(name).ok_or_else(|| Error::Unsupported(format!("{} uses the {name} versification", self.name())))
    }

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
        let versification = self.versification()?;
        let dir = self.conf.data_dir(&self.root).ok_or_else(|| Error::Unsupported("no DataPath".into()))?;
        // The files are `.czv` for some commentaries and `.bzv` (as a Bible's) for others.
        let kind = if dir.join("nt.czv").is_file() || dir.join("ot.czv").is_file() { 'c' } else { 'b' };
        Ok(Commentary { zt: ztext::ZText::open_as(dir, driver == "zCom4", kind, versification) })
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
        let verses = self.zt.verse_count(book, chapter);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn module(conf: &str) -> Module {
        Module { conf: Conf::parse(conf).unwrap(), root: PathBuf::from("/nowhere") }
    }

    #[test]
    fn a_daily_devotion_is_a_devotional_not_a_lexicon_whatever_its_driver() {
        for driver in ["zLD", "RawLD"] {
            let m = module(&format!("[D]\nModDrv={driver}\nFeature=DailyDevotion\nDataPath=./d/d\n"));
            assert_eq!(m.kind(), Kind::Devotional);
        }
        assert_eq!(module("[S]\nModDrv=zLD\nFeature=GreekDef\n").kind(), Kind::Lexicon);
    }

}
