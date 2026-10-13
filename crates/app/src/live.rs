//! The real backend: SWORD modules from the user's libraries, notes from the vault.

use scripture::{Book, RefRange};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use study_core::ast::Chapter;
use crate::persist::Persist;
use std::collections::BTreeMap;
use std::path::Path;
use study_core::mock::MockBackend;
use study_core::{CrossRef, Error, LexiconEntry, ModuleId, ModuleInfo, StudyBackend};
use sword::{Bible, Kind, Lexicon, Library};

/// The module opened first, and the one the sample marks are anchored in.
const DEFAULT_MODULE: &str = "LSB";

struct Open {
    bible: Bible,
    chapters: Vec<(Book, u16)>,
}

/// A Strong's dictionary and which testament's numbers it covers.
struct Dictionary {
    greek: bool,
    lexicon: Lexicon,
}

pub struct LiveBackend {
    infos: Vec<ModuleInfo>,
    open: HashMap<String, Arc<Open>>,
    /// Plain Strong's dictionaries first: they are the public domain reference ones.
    dictionaries: Vec<Dictionary>,
}

impl LiveBackend {
    pub fn open(roots: &[PathBuf]) -> LiveBackend {
        let lib = Library::open(roots);
        let (mut infos, mut open) = (Vec::new(), HashMap::new());
        for m in lib.modules().filter(|m| m.kind() == Kind::Bible) {
            // Locked, or a driver, compression or versification we cannot read yet: left out.
            if let Ok(bible) = m.bible() {
                let chapters = bible.chapters();
                infos.push(ModuleInfo {
                    id: ModuleId(m.name().to_string()),
                    abbreviation: m.conf.get("Abbreviation").unwrap_or(m.name()).to_string(),
                    description: m.description().to_string(),
                    has_strongs: m.has_strongs(),
                });
                open.insert(m.name().to_string(), Arc::new(Open { bible, chapters }));
            }
        }
        // The first module is the one the app opens: LSB for now, or `BIBLE_STUDY_MODULE`.
        let preferred = std::env::var("BIBLE_STUDY_MODULE").unwrap_or_else(|_| DEFAULT_MODULE.into());
        infos.sort_by_key(|m: &ModuleInfo| !m.abbreviation.eq_ignore_ascii_case(&preferred));
        let mut named: Vec<(bool, Dictionary)> = Vec::new();
        for m in lib.modules().filter(|m| m.kind() == Kind::Lexicon) {
            let greek = m.conf.all("Feature").any(|f| f == "GreekDef");
            if !greek && !m.conf.all("Feature").any(|f| f == "HebrewDef") {
                continue;
            }
            if let Ok(lexicon) = m.lexicon() {
                named.push((!m.name().to_lowercase().starts_with("strongs"), Dictionary { greek, lexicon }));
            }
        }
        named.sort_by_key(|(secondary, _)| *secondary);
        let dictionaries = named.into_iter().map(|(_, d)| d).collect();
        LiveBackend { infos, open, dictionaries }
    }
}

impl StudyBackend for LiveBackend {
    fn modules(&self) -> Vec<ModuleInfo> {
        self.infos.clone()
    }

    fn available_chapters(&self, module: &ModuleId) -> Vec<(Book, u16)> {
        self.open.get(&module.0).map(|o| o.chapters.clone()).unwrap_or_default()
    }

    fn chapter(&self, module: &ModuleId, book: Book, chapter: u16) -> Result<Chapter, Error> {
        let open = self.open.get(&module.0).ok_or_else(|| Error::NotAvailable(module.0.clone()))?;
        open.bible.chapter(book, chapter).map_err(|e| Error::NotAvailable(format!("{book} {chapter}: {e}")))
    }

    fn lexicon_entry(&self, key: &str) -> Option<LexiconEntry> {
        let key = key.trim();
        let (kind, digits) = key.split_at(key.find(|c: char| c.is_ascii_digit())?);
        let number: u32 = digits.parse().ok()?;
        let greek = match kind.to_ascii_uppercase().as_str() {
            "G" => true,
            "H" => false,
            _ => return None,
        };
        let letter = if greek { 'G' } else { 'H' };
        // Dictionaries key Strong's numbers differently: 00025, G0025, G25.
        let spellings = [format!("{number:05}"), format!("{letter}{number:04}"), format!("{letter}{number:05}"), format!("{letter}{number}"), number.to_string()];
        let mut best: Option<sword::Entry> = None;
        for dict in self.dictionaries.iter().filter(|d| d.greek == greek) {
            let Some(markup) = spellings.iter().find_map(|k| dict.lexicon.entry(k).ok().flatten()) else { continue };
            let parsed = sword::parse_entry(&markup);
            let has_lemma = !parsed.lemma.is_empty();
            if best.is_none() || has_lemma && best.as_ref().is_some_and(|b| b.lemma.is_empty()) {
                best = Some(parsed);
            }
            if has_lemma {
                break;
            }
        }
        let e = best?;
        Some(LexiconEntry {
            key: format!("{letter}{number}"),
            lemma: e.lemma,
            transliteration: e.transliteration,
            definition: e.definition,
        })
    }

    fn cross_references(&self, _range: &RefRange) -> Vec<CrossRef> {
        Vec::new()
    }

    fn note_paths(&self) -> Vec<String> {
        Vec::new()
    }

    fn note_text(&self, _path: &str) -> Option<String> {
        None
    }

    fn mark_files(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// Everything the app starts from.
pub struct Boot {
    pub backend: Box<dyn StudyBackend>,
    pub notes: BTreeMap<String, String>,
    pub persist: Option<Persist>,
    /// Shown in the toolbar until something else replaces it.
    pub status: String,
}

/// Start with the user's modules and vault. `--reseed` restores the sample notes and marks.
pub fn boot() -> Boot {
    let reseed = std::env::args().any(|a| a == "--reseed");
    boot_at(&vault::data_home(), &sword::default_roots(), reseed)
}

/// The module sample marks are anchored in: the one the app opens first.
fn seed_module(backend: &LiveBackend) -> Option<ModuleInfo> {
    backend.modules().into_iter().next()
}

pub fn boot_at(home: &Path, roots: &[PathBuf], reseed: bool) -> Boot {
    let backend = LiveBackend::open(roots);
    let Some(seed) = seed_module(&backend) else {
        let mock = MockBackend;
        let notes = mock.note_paths().into_iter().filter_map(|p| mock.note_text(&p).map(|t| (p, t))).collect();
        return Boot {
            backend: Box::new(mock),
            notes,
            persist: None,
            status: "No readable SWORD modules found: showing sample text, nothing is saved".into(),
        };
    };
    let vault = match vault::Vault::open(home.join("vault")) {
        Ok(v) => v,
        Err(e) => {
            return Boot {
                backend: Box::new(backend),
                notes: BTreeMap::new(),
                persist: None,
                status: format!("Could not open the vault at {}: {e}; nothing is saved", home.display()),
            };
        }
    };
    let mut status = String::new();
    if reseed || vault.is_empty() {
        let chapter = |b, c| backend.chapter(&seed.id, b, c).ok();
        if let Err(e) = vault::seed_vault(&vault, &seed.abbreviation, &chapter, reseed) {
            status = format!("Could not write the sample notes: {e}");
        }
    }
    let notes = vault.read_all().unwrap_or_default();
    // The index is a cache: if it cannot open, the app works without it.
    let index = vault::Index::open(&home.join("cache").join("index.sqlite")).ok().and_then(|mut ix| {
        ix.sync(&notes).ok()?;
        Some(ix)
    });
    Boot { backend: Box::new(backend), notes, persist: Some(Persist::new(vault, index)), status }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_chapters_from_an_installed_module() {
        let live = LiveBackend::open(&sword::default_roots());
        let Some(info) = live.modules().into_iter().find(|m| m.abbreviation == "ASV") else {
            return eprintln!("skipped: ASV is not installed");
        };
        let chapters = live.available_chapters(&info.id);
        assert_eq!(chapters.len(), 1189);
        let john = live.chapter(&info.id, Book::JOHN, 3).unwrap();
        assert!(john.canonical_verses()[&16].starts_with("For God so loved the world"));
        assert!(live.chapter(&info.id, Book::JOHN, 99).is_err());
        assert!(live.chapter(&ModuleId("Nope".into()), Book::JOHN, 3).is_err());
    }

    #[test]
    fn looks_up_strongs_numbers_in_either_testament() {
        let live = LiveBackend::open(&sword::default_roots());
        if live.dictionaries.is_empty() {
            return eprintln!("skipped: no Strong's dictionaries");
        }
        let love = live.lexicon_entry("G25").unwrap();
        assert_eq!((love.lemma.as_str(), love.transliteration.as_str()), ("ἀγαπάω", "agapao"));
        assert!(love.definition.contains("to love"), "{}", love.definition);
        assert_eq!(live.lexicon_entry("g0025").unwrap().key, "G25");
        let god = live.lexicon_entry("H430").unwrap();
        assert!(god.definition.to_lowercase().contains("god"), "{}", god.definition);
        assert!(!god.lemma.is_empty(), "a Hebrew entry with its own script is preferred");
        assert!(live.lexicon_entry("G99999").is_none());
        assert!(live.lexicon_entry("X25").is_none() && live.lexicon_entry("").is_none());
    }

    fn roots() -> Vec<PathBuf> {
        sword::default_roots()
    }

    fn has_modules() -> bool {
        !LiveBackend::open(&roots()).modules().is_empty()
    }

    fn first_module() -> String {
        LiveBackend::open(&roots()).modules()[0].abbreviation.clone()
    }

    #[test]
    fn lsb_opens_first_when_installed() {
        let live = LiveBackend::open(&roots());
        if !live.modules().iter().any(|m| m.abbreviation == "LSB") {
            return eprintln!("skipped: LSB is not installed");
        }
        assert_eq!(live.modules()[0].abbreviation, "LSB");
        assert!(live.modules().len() > 1 || std::env::var("BIBLE_STUDY_SWORD_PATH").is_ok());
    }

    #[test]
    fn first_start_seeds_the_vault_and_later_starts_keep_edits() {
        if !has_modules() {
            return eprintln!("skipped: no modules");
        }
        let home = tempfile::tempdir().unwrap();
        let first = boot_at(home.path(), &roots(), false);
        assert!(first.status.is_empty(), "{}", first.status);
        let marks = first.notes.keys().filter(|p| p.starts_with("margins/")).count();
        assert!(marks >= 7, "{marks} seeded marks");
        let pin = format!("![[John 3:3-5@{}]]", first_module());
        assert!(first.notes["notes/Nicodemus.md"].contains(&pin), "{pin}");
        // A user's own note and an edit to a sample survive a restart; nothing is re-seeded.
        let v = first.persist.as_ref().unwrap().vault();
        v.write("notes/Mine.md", "mine").unwrap();
        v.write("notes/Creation.md", "edited").unwrap();
        drop(first);
        let second = boot_at(home.path(), &roots(), false);
        assert_eq!(second.notes["notes/Mine.md"], "mine");
        assert_eq!(second.notes["notes/Creation.md"], "edited");
        // --reseed restores the samples without touching anyone's own notes.
        let third = boot_at(home.path(), &roots(), true);
        assert!(third.notes["notes/Creation.md"].contains("# Creation"));
        assert_eq!(third.notes["notes/Mine.md"], "mine");
    }

    #[test]
    fn seeded_marks_all_resolve_in_the_module_they_were_made_in() {
        if !has_modules() {
            return eprintln!("skipped: no modules");
        }
        let home = tempfile::tempdir().unwrap();
        let boot = boot_at(home.path(), &roots(), false);
        let asv = boot.backend.modules().into_iter().next().unwrap();
        for (path, text) in boot.notes.iter().filter(|(p, _)| p.starts_with("margins/")) {
            let id = path.trim_end_matches(".md").rsplit('-').next().unwrap();
            let mark = study_core::margin::Mark::from_markdown(id, text).unwrap();
            let ch = boot.backend.chapter(&asv.id, mark.anchor.book, mark.anchor.chapter).unwrap();
            let r = study_core::margin::resolve(&mark, &ch.canonical_verses());
            assert_eq!(r.state, study_core::margin::Anchoring::Exact, "{path}");
        }
    }

    #[test]
    fn the_index_is_written_next_to_the_vault() {
        if !has_modules() {
            return eprintln!("skipped: no modules");
        }
        let home = tempfile::tempdir().unwrap();
        let boot = boot_at(home.path(), &roots(), false);
        drop(boot);
        let ix = vault::Index::open(&home.path().join("cache/index.sqlite")).unwrap();
        let hits = ix.touching(&RefRange::parse("John 3:4").unwrap()).unwrap();
        assert!(hits.iter().any(|h| h.path == "notes/Nicodemus.md" && h.kind == "embed"), "{hits:?}");
    }

    #[test]
    fn with_no_modules_it_falls_back_to_sample_text_and_saves_nothing() {
        let home = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let boot = boot_at(home.path(), &[empty.path().to_path_buf()], false);
        assert!(boot.persist.is_none());
        assert!(boot.status.contains("No readable SWORD modules"));
        assert!(!home.path().join("vault").exists());
    }
}
