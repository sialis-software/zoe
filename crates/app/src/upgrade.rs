//! What happens when a different version of the app opens the folders of the last one.
//!
//! The data folder keeps `version.json`, the version that last opened it. At start the app compares that with its own:
//!
//! - none, and nothing else there: a first run.
//! - the same: nothing to do.
//! - an older one (or none, with the files of a version from before this was kept): an upgrade. What the person chose
//!   and the session are copied to `backups/<from>-to-<to>/` first, then each migration for a version after the old one
//!   and up to this one runs, in order. If one fails the new version is not noted, so that the next start tries again.
//! - a newer one: a downgrade. Nothing is changed (the file keeps the newer version), and the person is told: what the
//!   newer version wrote may not be what this one reads.
//!
//! The vault is the person's own and is not touched here: a migration of its notes says what it did and keeps what it
//! changed (see `live::mark_links_once`).

use std::fs;
use std::path::{Path, PathBuf};

const FILE: &str = "version.json";
/// How many backups of an upgrade are kept: the oldest go.
const KEPT: usize = 10;

/// The folders a migration works in. (Read by the migrations, of which there are none yet.)
#[allow(dead_code)]
pub struct Folders<'a> {
    /// What the person chose: `config.json`, `settings.json`, `layouts.json`.
    pub config: &'a Path,
    /// What the app made: `session.json`, the caches.
    pub data: &'a Path,
}

/// A change to the files of the last version, made once when the app is upgraded across `version` (to it or past it).
pub struct Migration {
    pub version: &'static str,
    pub name: &'static str,
    pub run: fn(&Folders) -> Result<(), String>,
}

/// The migrations there are, oldest first. (None yet: the files have so far only been added to.)
pub const MIGRATIONS: &[Migration] = &[];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Nothing was there: the first run.
    Fresh,
    Same,
    Upgraded,
    Downgraded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upgrade {
    pub kind: Kind,
    /// The version that last opened the folders, if it was noted.
    pub from: Option<String>,
    pub to: String,
    /// Where the files were copied before an upgrade.
    pub backup: Option<PathBuf>,
    /// The migrations that ran, and those that failed (with why).
    pub ran: Vec<String>,
    pub failed: Vec<String>,
}

impl Upgrade {
    /// What to tell the person, if anything: the words, and whether it is a problem.
    pub fn said(&self) -> Option<(String, bool)> {
        match self.kind {
            Kind::Fresh | Kind::Same => None,
            Kind::Upgraded if !self.failed.is_empty() => Some((
                format!("Updated to {}, but {} could not be carried over: {}. It will be tried again next time.", self.to, plural(self.failed.len()), self.failed.join("; ")),
                true,
            )),
            Kind::Upgraded => Some((
                match &self.from {
                    Some(from) => format!("Updated to {} from {from}", self.to),
                    None => format!("Updated to {}", self.to),
                },
                false,
            )),
            Kind::Downgraded => Some((
                format!(
                    "This is version {}, but what is kept here was last used by {}: some of it may not be read as it was written.",
                    self.to,
                    self.from.as_deref().unwrap_or("a newer one")
                ),
                true,
            )),
        }
    }
}

fn plural(n: usize) -> String {
    if n == 1 { "a change".into() } else { format!("{n} changes") }
}

/// `1.2.3` as numbers to compare (what is not a number counts as nothing: `1.2` is `1.2.0`, and `1.2.3-beta` is `1.2.3`).
fn numbers(version: &str) -> Vec<u64> {
    let mut parts: Vec<u64> = version.split('.').map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)).collect();
    while parts.len() < 3 {
        parts.push(0);
    }
    parts
}

fn older(a: &str, b: &str) -> bool {
    numbers(a) < numbers(b)
}

/// The version noted in the data folder, if it was.
fn noted(data: &Path) -> Option<String> {
    let text = fs::read_to_string(data.join(FILE)).ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?.get("version")?.as_str().map(str::to_string)
}

fn note(data: &Path, version: &str, build: &str) {
    let _ = fs::create_dir_all(data);
    let body = serde_json::json!({ "version": version, "build": build });
    let _ = fs::write(data.join(FILE), serde_json::to_string_pretty(&body).unwrap_or_default() + "\n");
}

/// Whether the folders hold anything of an earlier version's.
fn has_files(config: &Path, data: &Path) -> bool {
    ["config.json", "settings.json", "layouts.json"].iter().any(|f| config.join(f).exists()) || data.join("session.json").exists() || data.join("vault").exists()
}

/// Copy what the person chose and the session to `backups/<from>-to-<to>`. Gives where, if it could be done.
fn back_up(config: &Path, data: &Path, from: &str, to: &str) -> Option<PathBuf> {
    let at = data.join("backups").join(format!("{from}-to-{to}"));
    fs::create_dir_all(&at).ok()?;
    for (folder, file) in [(config, "config.json"), (config, "settings.json"), (config, "layouts.json"), (data, "session.json")] {
        let source = folder.join(file);
        if source.exists() {
            fs::copy(&source, at.join(file)).ok()?;
        }
    }
    // The oldest backups go.
    let mut all: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(data.join("backups"))
        .ok()?
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    all.sort();
    while all.len() > KEPT {
        let (_, gone) = all.remove(0);
        let _ = fs::remove_dir_all(gone);
    }
    Some(at)
}

/// Compare the version in the data folder with `current` (built from `build`), and do what that asks (see the top of this file).
pub fn check(config: &Path, data: &Path, current: &str, build: &str) -> Upgrade {
    check_with(MIGRATIONS, config, data, current, build)
}

pub fn check_with(migrations: &[Migration], config: &Path, data: &Path, current: &str, build: &str) -> Upgrade {
    let from = noted(data);
    let mut out = Upgrade { kind: Kind::Same, from: from.clone(), to: current.to_string(), backup: None, ran: Vec::new(), failed: Vec::new() };
    let kind = match &from {
        None if has_files(config, data) => Kind::Upgraded,
        None => Kind::Fresh,
        Some(old) if old == current || numbers(old) == numbers(current) => Kind::Same,
        Some(old) if older(old, current) => Kind::Upgraded,
        Some(_) => Kind::Downgraded,
    };
    out.kind = kind;
    match kind {
        Kind::Fresh | Kind::Same => note(data, current, build),
        Kind::Downgraded => {}
        Kind::Upgraded => {
            // The files of a version from before the versions were noted are all that this knows of: every migration is for them.
            out.backup = back_up(config, data, from.as_deref().unwrap_or("earlier"), current);
            let folders = Folders { config, data };
            for migration in migrations {
                let after_old = from.as_deref().is_none_or(|old| older(old, migration.version));
                if !after_old || older(current, migration.version) {
                    continue;
                }
                match (migration.run)(&folders) {
                    Ok(()) => out.ran.push(migration.name.to_string()),
                    Err(why) => out.failed.push(format!("{}: {why}", migration.name)),
                }
            }
            if out.failed.is_empty() {
                note(data, current, build);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static RAN: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    fn mark(name: &'static str) -> Result<(), String> {
        RAN.with(|r| r.borrow_mut().push(name));
        Ok(())
    }

    const MIGRATIONS: &[Migration] = &[
        Migration { version: "0.2.0", name: "second", run: |_| mark("0.2.0") },
        Migration { version: "0.3.0", name: "third", run: |_| mark("0.3.0") },
        Migration { version: "0.4.0", name: "fourth", run: |_| mark("0.4.0") },
    ];

    fn ran() -> Vec<&'static str> {
        RAN.with(|r| std::mem::take(&mut *r.borrow_mut()))
    }

    fn folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let (config, data) = (dir.path().join("config"), dir.path().join("data"));
        (dir, config, data)
    }

    #[test]
    fn a_first_run_notes_its_version_and_changes_nothing() {
        let (_dir, config, data) = folders();
        let up = check_with(MIGRATIONS, &config, &data, "0.3.0", "abc");
        assert_eq!((up.kind, up.from.clone()), (Kind::Fresh, None));
        assert_eq!(up.said(), None);
        assert_eq!(noted(&data).as_deref(), Some("0.3.0"));
        assert!(ran().is_empty() && !data.join("backups").exists());
        // The next start is the same version.
        assert_eq!(check_with(MIGRATIONS, &config, &data, "0.3.0", "abc").kind, Kind::Same);
    }

    #[test]
    fn an_upgrade_backs_up_what_was_chosen_and_runs_the_migrations_it_crosses_in_order() {
        let (_dir, config, data) = folders();
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(config.join("settings.json"), "{\"theme\":\"Nord\"}").unwrap();
        fs::write(data.join("session.json"), "{}").unwrap();
        note(&data, "0.1.0", "old");
        let up = check_with(MIGRATIONS, &config, &data, "0.3.0", "new");
        assert_eq!(up.kind, Kind::Upgraded);
        assert_eq!(ran(), ["0.2.0", "0.3.0"], "those after the old version and up to this one");
        assert_eq!(up.said(), Some(("Updated to 0.3.0 from 0.1.0".to_string(), false)));
        let backup = up.backup.expect("a backup");
        assert!(backup.ends_with("backups/0.1.0-to-0.3.0"));
        assert_eq!(fs::read_to_string(backup.join("settings.json")).unwrap(), "{\"theme\":\"Nord\"}");
        assert!(backup.join("session.json").exists());
        assert_eq!(noted(&data).as_deref(), Some("0.3.0"));
        // Not again.
        assert_eq!(check_with(MIGRATIONS, &config, &data, "0.3.0", "new").kind, Kind::Same);
        assert!(ran().is_empty());
    }

    #[test]
    fn files_with_no_version_noted_are_those_of_an_earlier_version_and_get_every_migration() {
        let (_dir, config, data) = folders();
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("session.json"), "{}").unwrap();
        let up = check_with(MIGRATIONS, &config, &data, "0.2.0", "new");
        assert_eq!((up.kind, up.from.clone()), (Kind::Upgraded, None));
        assert_eq!(ran(), ["0.2.0"]);
        assert_eq!(up.said().map(|s| s.0), Some("Updated to 0.2.0".to_string()));
        assert!(up.backup.is_some_and(|b| b.ends_with("backups/earlier-to-0.2.0")));
    }

    #[test]
    fn a_migration_that_fails_is_told_and_tried_again_and_the_new_version_is_not_noted() {
        static FAIL: Migration = Migration { version: "0.2.0", name: "breaks", run: |_| Err("no room".into()) };
        let (_dir, config, data) = folders();
        fs::create_dir_all(&data).unwrap();
        note(&data, "0.1.0", "old");
        let up = check_with(std::slice::from_ref(&FAIL), &config, &data, "0.2.0", "new");
        assert_eq!(up.failed, ["breaks: no room"]);
        assert!(up.said().is_some_and(|(words, problem)| problem && words.contains("breaks: no room") && words.contains("tried again")));
        assert_eq!(noted(&data).as_deref(), Some("0.1.0"), "so the next start is an upgrade again");
        assert_eq!(check_with(std::slice::from_ref(&FAIL), &config, &data, "0.2.0", "new").kind, Kind::Upgraded);
    }

    #[test]
    fn a_downgrade_changes_nothing_and_says_so() {
        let (_dir, config, data) = folders();
        fs::create_dir_all(&data).unwrap();
        note(&data, "0.4.0", "newer");
        let up = check_with(MIGRATIONS, &config, &data, "0.2.0", "older");
        assert_eq!(up.kind, Kind::Downgraded);
        assert!(up.said().is_some_and(|(words, problem)| problem && words.contains("0.4.0") && words.contains("0.2.0")));
        assert_eq!(noted(&data).as_deref(), Some("0.4.0"), "the newer version stays noted");
        assert!(ran().is_empty() && up.backup.is_none());
    }

    #[test]
    fn versions_compare_by_their_numbers_and_the_oldest_backups_go() {
        assert!(older("0.9.0", "0.10.0") && older("1.2", "1.2.1") && !older("1.2.3-beta", "1.2.3"));
        let (_dir, config, data) = folders();
        fs::create_dir_all(&data).unwrap();
        for i in 0..(KEPT + 3) {
            back_up(&config, &data, &format!("0.0.{i}"), "0.1.0").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let kept = fs::read_dir(data.join("backups")).unwrap().count();
        assert_eq!(kept, KEPT);
        assert!(!data.join("backups/0.0.0-to-0.1.0").exists() && data.join(format!("backups/0.0.{}-to-0.1.0", KEPT + 2)).exists());
    }
}
