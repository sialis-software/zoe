//! Keeping the in-memory notes and the vault on disk in step.
//!
//! The app edits a map of path to text. [`NoteMap`] remembers which paths changed, and
//! [`Persist::flush`] writes just those to the vault and updates the SQLite index.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use vault::{Index, Vault};

/// Path to text, noting what changed since the last [`NoteMap::take_changes`].
#[derive(Default)]
pub struct NoteMap {
    map: BTreeMap<String, String>,
    dirty: BTreeSet<String>,
    /// Counts every change, so what is worked out from the notes can tell when to be again.
    revision: u64,
}

pub enum Change {
    Write(String, String),
    Delete(String),
}

impl Deref for NoteMap {
    type Target = BTreeMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl NoteMap {
    /// Notes already on disk: nothing to write back.
    pub fn loaded(map: BTreeMap<String, String>) -> Self {
        NoteMap { map, dirty: BTreeSet::new(), revision: 0 }
    }

    pub fn insert(&mut self, path: String, text: String) {
        if self.map.get(&path) != Some(&text) {
            self.revision += 1;
            self.dirty.insert(path.clone());
            self.map.insert(path, text);
        }
    }

    pub fn remove(&mut self, path: &str) {
        if self.map.remove(path).is_some() {
            self.revision += 1;
            self.dirty.insert(path.to_string());
        }
    }

    /// Remove a note, returning its text (`None` if there was none).
    pub fn remove_if_present(&mut self, path: &str) -> Option<String> {
        let text = self.map.remove(path)?;
        self.revision += 1;
        self.dirty.insert(path.to_string());
        Some(text)
    }

    pub fn or_insert_with(&mut self, path: String, make: impl FnOnce() -> String) {
        if !self.map.contains_key(&path) {
            self.insert(path, make());
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|p| match self.map.get(&p) {
                Some(t) => Change::Write(p, t.clone()),
                None => Change::Delete(p),
            })
            .collect()
    }
}

pub struct Persist {
    vault: Vault,
    index: Option<Index>,
}

impl Persist {
    pub fn new(vault: Vault, index: Option<Index>) -> Self {
        Persist { vault, index }
    }

    #[cfg(test)]
    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    /// Write `changes` to disk. A failure is reported, never fatal: the edit stays in memory
    /// and is retried by the next change to that file.
    pub fn flush(&mut self, changes: Vec<Change>) -> Result<(), String> {
        let mut failed = Vec::new();
        // Removals first: a rename that only changes capitals names the same file on some systems,
        // and must not write the new name and then delete it.
        let (removals, writes): (Vec<_>, Vec<_>) = changes.into_iter().partition(|c| matches!(c, Change::Delete(_)));
        for change in removals.into_iter().chain(writes) {
            let (path, result) = match &change {
                Change::Write(p, t) => (p, self.vault.write(p, t)),
                Change::Delete(p) => (p, self.vault.remove(p)),
            };
            if let Err(e) = result {
                failed.push(format!("{path}: {e}"));
                continue;
            }
            // The index is only a cache: if it fails, the next start rebuilds it.
            if let Some(ix) = &mut self.index {
                let _ = match &change {
                    Change::Write(p, t) => ix.update(p, t),
                    Change::Delete(p) => ix.remove(p),
                };
            }
        }
        if failed.is_empty() { Ok(()) } else { Err(format!("Could not save {}", failed.join("; "))) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_changes_are_dirty() {
        let mut n = NoteMap::loaded([("a.md".to_string(), "one".to_string())].into());
        assert!(n.take_changes().is_empty());
        n.insert("a.md".into(), "one".into());
        assert!(n.take_changes().is_empty(), "same text is not a change");
        n.insert("a.md".into(), "two".into());
        n.insert("b.md".into(), "new".into());
        n.or_insert_with("b.md".into(), || unreachable!());
        let changes = n.take_changes();
        assert_eq!(changes.len(), 2);
        assert!(matches!(&changes[0], Change::Write(p, t) if p == "a.md" && t == "two"));
        assert!(n.take_changes().is_empty());
    }

    #[test]
    fn removal_and_rewrite_collapse_to_the_final_state() {
        let mut n = NoteMap::loaded([("a.md".to_string(), "one".to_string())].into());
        n.remove("a.md");
        n.remove("missing.md");
        assert!(matches!(&n.take_changes()[..], [Change::Delete(p)] if p == "a.md"));
        n.insert("a.md".into(), "x".into());
        n.remove("a.md");
        assert!(matches!(&n.take_changes()[..], [Change::Delete(_)]));
    }

    #[test]
    fn flush_writes_files_and_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("vault")).unwrap();
        let mut p = Persist::new(vault, Some(Index::in_memory().unwrap()));
        p.flush(vec![Change::Write("notes/A.md".into(), "[[John 3:16]]".into())]).unwrap();
        assert_eq!(p.vault().read("notes/A.md").unwrap(), "[[John 3:16]]");
        p.flush(vec![Change::Delete("notes/A.md".into())]).unwrap();
        assert!(p.vault().read("notes/A.md").is_err());
        assert!(p.flush(vec![Change::Write("../escape.md".into(), "x".into())]).is_err());
    }
}
