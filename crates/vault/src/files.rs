//! A vault is a folder of `.md` files. Paths are vault-relative with `/` separators.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub struct Vault {
    root: PathBuf,
}

/// A folder of the user's, from an environment variable if it is set to something.
fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn user_home() -> PathBuf {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE")).unwrap_or_default()
}

/// Where the app keeps what the person chose (`config.json`, `settings.json`, `layouts.json`):
/// `ZOE_CONFIG_HOME`, else `ZOE_HOME/config`, else `$XDG_CONFIG_HOME/zoe` (`~/.config/zoe`;
/// `%APPDATA%\zoe` on Windows).
pub fn config_home() -> PathBuf {
    if let Some(p) = env_dir("ZOE_CONFIG_HOME") {
        return p;
    }
    if let Some(p) = env_dir("ZOE_HOME").or_else(|| env_dir("BIBLE_STUDY_HOME")) {
        return p.join("config");
    }
    if cfg!(windows)
        && let Some(p) = env_dir("APPDATA")
    {
        return p.join("zoe");
    }
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| user_home().join(".config")).join("zoe")
}

/// Where the app keeps what it makes (the session, the search indexes, the vault it makes when none
/// is chosen): `ZOE_DATA_HOME`, else `ZOE_HOME/data`, else `$XDG_DATA_HOME/zoe`
/// (`~/.local/share/zoe`; `%LOCALAPPDATA%\zoe` on Windows).
pub fn data_home() -> PathBuf {
    if let Some(p) = env_dir("ZOE_DATA_HOME") {
        return p;
    }
    if let Some(p) = env_dir("ZOE_HOME").or_else(|| env_dir("BIBLE_STUDY_HOME")) {
        return p.join("data");
    }
    if cfg!(windows)
        && let Some(p) = env_dir("LOCALAPPDATA")
    {
        return p.join("zoe");
    }
    env_dir("XDG_DATA_HOME").unwrap_or_else(|| user_home().join(".local/share")).join("zoe")
}

/// Where the app's files were before config and data were kept apart, and before the app was
/// called Zoe: one folder with everything in it.
pub fn old_homes() -> Vec<PathBuf> {
    let home = user_home();
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.push(home.join("Library/Application Support/zoe"));
        out.push(home.join("Library/Application Support/bible-study"));
    }
    out.push(home.join(".local/share/bible-study"));
    out
}

/// Move the files of an `old` single folder to where they now belong: what the person chose to
/// `config`, the rest to `data`. Does nothing if `config` already has its `config.json` (it has been
/// done) or if either folder to move to is where `old` is. Gives whether something was moved.
pub fn migrate_home(old: &Path, config: &Path, data: &Path) -> bool {
    if !old.is_dir() || old == config || old == data || config.join("config.json").exists() {
        return false;
    }
    if fs::create_dir_all(config).is_err() || fs::create_dir_all(data).is_err() {
        return false;
    }
    let mut moved = false;
    let mut put = |from: PathBuf, to: PathBuf| {
        if from.exists() && !to.exists() && fs::rename(&from, &to).is_ok() {
            moved = true;
        }
    };
    put(old.join("config.json"), config.join("config.json"));
    put(old.join("state/settings.json"), config.join("settings.json"));
    put(old.join("state/layouts.json"), config.join("layouts.json"));
    put(old.join("state/session.json"), data.join("session.json"));
    put(old.join("cache"), data.join("cache"));
    put(old.join("vault"), data.join("vault"));
    // What is left is nothing the app made, but if the folders are empty they go.
    let _ = fs::remove_dir(old.join("state"));
    let _ = fs::remove_dir(old);
    moved
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
    fn an_old_single_folder_is_split_into_config_and_data_once() {
        let base = tempfile::tempdir().unwrap();
        let (old, config, data) = (base.path().join("old"), base.path().join("config/zoe"), base.path().join("data/zoe"));
        assert!(!migrate_home(&old, &config, &data), "nothing to move");
        fs::create_dir_all(old.join("state")).unwrap();
        fs::create_dir_all(old.join("vault")).unwrap();
        fs::create_dir_all(old.join("cache")).unwrap();
        for file in ["config.json", "state/settings.json", "state/layouts.json", "state/session.json", "vault/Note.md", "cache/index.sqlite"] {
            fs::write(old.join(file), file).unwrap();
        }
        assert!(!migrate_home(&old, &old, &data), "not into itself");
        assert!(migrate_home(&old, &config, &data));
        for file in ["config.json", "settings.json", "layouts.json"] {
            assert!(config.join(file).is_file(), "{file} is config");
        }
        for file in ["session.json", "vault/Note.md", "cache/index.sqlite"] {
            assert!(data.join(file).is_file(), "{file} is data");
        }
        assert_eq!(fs::read_to_string(config.join("settings.json")).unwrap(), "state/settings.json");
        assert!(!old.exists(), "and nothing is left behind");
        // Done once: a config.json that is there is never written over by an old one.
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("config.json"), "old").unwrap();
        assert!(!migrate_home(&old, &config, &data));
        assert_eq!(fs::read_to_string(config.join("config.json")).unwrap(), "config.json");
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
