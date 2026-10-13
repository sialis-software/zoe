//! GUI-agnostic application services. The UI talks only to [`StudyBackend`].
//! Phase 1 ships [`mock::MockBackend`]; the real SWORD + vault backend replaces it later.

pub mod ast;
pub mod frontmatter;
pub mod margin;
pub mod markdown;
pub mod mock;

use ast::Chapter;
use scripture::{Book, RefRange};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModuleId(pub String);

#[derive(Clone, Debug)]
pub struct ModuleInfo {
    pub id: ModuleId,
    pub abbreviation: String,
    pub description: String,
    pub has_strongs: bool,
}

#[derive(Clone, Debug)]
pub struct LexiconEntry {
    pub key: String,
    pub lemma: String,
    pub transliteration: String,
    pub definition: String,
}

#[derive(Clone, Debug)]
pub struct CrossRef {
    pub target: RefRange,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NotAvailable(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotAvailable(what) => write!(f, "{what} is not available"),
        }
    }
}

impl std::error::Error for Error {}

pub trait StudyBackend {
    fn modules(&self) -> Vec<ModuleInfo>;
    /// Chapters the given module can serve, in canon order.
    fn available_chapters(&self, module: &ModuleId) -> Vec<(Book, u16)>;
    fn chapter(&self, module: &ModuleId, book: Book, chapter: u16) -> Result<Chapter, Error>;
    fn lexicon_entry(&self, key: &str) -> Option<LexiconEntry>;
    /// The dictionaries and lexicons installed, in the order to offer them.
    fn dictionaries(&self) -> Vec<ModuleInfo>;
    /// An entry of a named dictionary. A Strong's dictionary takes numbers as people write them
    /// (`G25`).
    fn dictionary_entry(&self, module: &ModuleId, key: &str) -> Option<LexiconEntry>;
    /// The dictionary that defines a Strong's number (`G25`), if any does.
    fn strongs_dictionary(&self, key: &str) -> Option<ModuleId>;
    fn cross_references(&self, range: &RefRange) -> Vec<CrossRef>;
    /// Vault-relative paths of all notes.
    fn note_paths(&self) -> Vec<String>;
    fn note_text(&self, path: &str) -> Option<String>;
    /// Margin files `(vault-relative path, markdown)`.
    fn mark_files(&self) -> Vec<(String, String)>;
}
