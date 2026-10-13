//! A vault is a folder of `.md` files. Paths are vault-relative with `/` separators.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub struct Vault {
    root: PathBuf,
}

/// Where this app keeps its data: `ZOE_HOME` (or the older `BIBLE_STUDY_HOME`), else the platform's
/// data directory.
pub fn data_home() -> PathBuf {
    if let Some(p) = std::env::var_os("ZOE_HOME").or_else(|| std::env::var_os("BIBLE_STUDY_HOME")) {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/zoe")
    } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(x).join("zoe")
    } else {
        home.join(".local/share/zoe")
    }
}

/// The folder the data was kept in before the app was called Zoe, next to where it is now.
pub const OLD_DATA_FOLDER: &str = "bible-study";

/// Move the data folder of the app's earlier name to `home` (the new one), if there is one and
/// `home` does not exist yet. Gives the old place, if it was moved.
pub fn migrate_data_home(home: &Path) -> Option<PathBuf> {
    let old = home.parent()?.join(OLD_DATA_FOLDER);
    if home.file_name()? != "zoe" || home.exists() || !old.is_dir() {
        return None;
    }
    fs::rename(&old, home).ok().map(|()| old)
}

fn invalid(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("not a vault path: {path:?}"))
}

impl Vault {
    /// Open the vault at `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Vault> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Vault { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &str) -> io::Result<PathBuf> {
        let rel = Path::new(path);
        let clean = !path.is_empty()
            && path.ends_with(".md")
            && rel.components().all(|c| matches!(c, Component::Normal(_)));
        if !clean {
            return Err(invalid(path));
        }
        Ok(self.root.join(rel))
    }

    /// Every markdown file, keyed by vault-relative path. Hidden folders are skipped.
    pub fn read_all(&self) -> io::Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        walk(&self.root, "", &mut out)?;
        Ok(out)
    }

    pub fn read(&self, path: &str) -> io::Result<String> {
        fs::read_to_string(self.resolve(path)?)
    }

    /// Write a file whole, never leaving a half-written one behind.
    pub fn write(&self, path: &str, text: &str) -> io::Result<()> {
        let full = self.resolve(path)?;
        if let Some(dir) = full.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = full.with_extension("md.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &full)
    }

    /// Delete a file (already gone is fine) and any folders it leaves empty.
    pub fn remove(&self, path: &str) -> io::Result<()> {
        let full = self.resolve(path)?;
        match fs::remove_file(&full) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let mut dir = full.parent();
        while let Some(d) = dir.filter(|d| *d != self.root) {
            if fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.read_all().map(|f| f.is_empty()).unwrap_or(true)
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk(&entry.path(), &rel, out)?;
        } else if kind.is_file()
            && name.ends_with(".md")
            && let Ok(text) = fs::read_to_string(entry.path())
        {
            out.insert(rel, text);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_reads_and_removes_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        assert!(v.is_empty());
        v.write("margins/John/03/16-highlight-a1.md", "hello\n").unwrap();
        v.write("notes/Top.md", "top").unwrap();
        assert_eq!(v.read("notes/Top.md").unwrap(), "top");
        let all = v.read_all().unwrap();
        assert_eq!(all.keys().collect::<Vec<_>>(), ["margins/John/03/16-highlight-a1.md", "notes/Top.md"]);
        v.remove("margins/John/03/16-highlight-a1.md").unwrap();
        assert!(!dir.path().join("margins").exists(), "empty folders are pruned");
        v.remove("notes/Top.md").unwrap();
        v.remove("notes/Top.md").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn data_kept_under_the_old_name_moves_to_the_new_folder_once() {
        let base = tempfile::tempdir().unwrap();
        let (old, new) = (base.path().join(OLD_DATA_FOLDER), base.path().join("zoe"));
        assert_eq!(migrate_data_home(&new), None, "nothing to move");
        fs::create_dir_all(old.join("vault")).unwrap();
        fs::write(old.join("vault/Note.md"), "# Note").unwrap();
        assert_eq!(migrate_data_home(&base.path().join("elsewhere")), None, "only for the app's own folder");
        assert_eq!(migrate_data_home(&new), Some(old.clone()));
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new.join("vault/Note.md")).unwrap(), "# Note");
        // A new folder that is there is never written over by an old one.
        fs::create_dir_all(&old).unwrap();
        assert_eq!(migrate_data_home(&new), None);
        assert!(old.exists());
    }

    #[test]
    fn refuses_paths_that_leave_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        for bad in ["../x.md", "/etc/x.md", "a/../../x.md", "notes.txt", ""] {
            assert!(v.write(bad, "x").is_err(), "{bad}");
        }
    }

    #[test]
    fn hidden_folders_and_other_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/x.md"), "x").unwrap();
        std::fs::write(dir.path().join("image.png"), "x").unwrap();
        let v = Vault::open(dir.path()).unwrap();
        assert!(v.read_all().unwrap().is_empty());
    }
}
