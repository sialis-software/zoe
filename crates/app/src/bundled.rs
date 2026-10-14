//! The modules that come with the app, so that the first run has a Bible to read: the Berean
//! Standard Bible (CC0) and the two Strong's dictionaries (public domain), as CrossWire packages
//! (`assets/modules`) compiled in. They are put in the app's own module folder once, and only when
//! no Bible module is to be found anywhere; what the person does with them afterwards stands.

use crate::live::LiveBackend;
use std::path::{Path, PathBuf};
use study_core::StudyBackend;
use sword::package::Package;

const PACKAGES: [(&str, &[u8]); 3] = [
    ("BSB", include_bytes!("../assets/modules/BSB.zip")),
    ("StrongsGreek", include_bytes!("../assets/modules/StrongsGreek.zip")),
    ("StrongsHebrew", include_bytes!("../assets/modules/StrongsHebrew.zip")),
];

/// The file in the config folder that says this has been decided, so it is not done again.
const MARKER: &str = "bundled-modules";

/// Install the bundled modules into `own` if this is the first run and there is no Bible in
/// `roots`. Returns the names installed. Whatever happens, a first run is only ever done once;
/// a failure to install is left for the module manager (nothing is marked).
pub fn install_first_run(config: &Path, own: &Path, roots: &[PathBuf]) -> Vec<String> {
    let marker = config.join(MARKER);
    if marker.exists() {
        return Vec::new();
    }
    let mut installed = Vec::new();
    if LiveBackend::open(roots).modules().is_empty() {
        for (name, bytes) in PACKAGES {
            match install(name, bytes, own) {
                Ok(mut names) => installed.append(&mut names),
                Err(e) => {
                    eprintln!("Could not install the bundled module {name}: {e}");
                    return installed;
                }
            }
        }
    }
    let _ = std::fs::create_dir_all(config);
    let _ = std::fs::write(marker, "The modules that come with the app were looked at once.\n");
    installed
}

fn install(name: &str, bytes: &[u8], own: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(own).map_err(|e| e.to_string())?;
    // The package is read from a file: it is written next to where it goes, and taken away.
    let file = own.join(format!(".bundled-{name}.zip"));
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let result = Package::from_zip(&file).map_err(|e| e.to_string()).and_then(|p| p.install(own, false).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&file);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use scripture::Book;

    #[test]
    fn the_first_run_gets_a_bible_and_both_strongs_dictionaries_that_can_be_read() {
        let (config, own) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let roots = vec![own.path().to_path_buf()];
        let installed = install_first_run(config.path(), own.path(), &roots);
        assert_eq!(installed.len(), 3, "{installed:?}");
        let backend = LiveBackend::open(&roots);
        let bible = backend.modules().into_iter().next().expect("a Bible");
        assert_eq!(bible.abbreviation, "BSB");
        assert!(bible.has_strongs);
        assert!(backend.chapter(&bible.id, Book::JOHN, 3).is_ok());
        let dictionaries: Vec<_> = backend.dictionaries().into_iter().map(|d| d.id.0).collect();
        assert!(dictionaries.iter().any(|d| d == "StrongsGreek") && dictionaries.iter().any(|d| d == "StrongsHebrew"), "{dictionaries:?}");
        assert!(std::fs::read_dir(own.path()).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(".bundled")), "no leftovers");
    }

    #[test]
    fn it_is_only_done_once_and_never_over_a_bible_that_is_there() {
        let (config, own) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let roots = vec![own.path().to_path_buf()];
        assert!(!install_first_run(config.path(), own.path(), &roots).is_empty());
        // Taken away by the person: not put back.
        std::fs::remove_dir_all(own.path()).unwrap();
        assert!(install_first_run(config.path(), own.path(), &roots).is_empty());
        assert!(LiveBackend::open(&roots).modules().is_empty());
        // Another folder with a Bible in it: nothing is added.
        let (config, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        install("BSB", PACKAGES[0].1, elsewhere.path()).unwrap();
        let roots = vec![elsewhere.path().to_path_buf(), own.path().to_path_buf()];
        assert!(install_first_run(config.path(), own.path(), &roots).is_empty());
        assert!(!own.path().join("mods.d").exists());
    }
}
