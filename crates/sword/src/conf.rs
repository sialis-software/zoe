//! `mods.d/*.conf`: INI-like module descriptions (docs/01 §2).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Conf {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Conf {
    pub fn parse(text: &str) -> Option<Conf> {
        let mut name = None;
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut joining = false;
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if joining {
                let (cont, more) = split_continuation(line);
                if let Some((_, v)) = entries.last_mut() {
                    v.push('\n');
                    v.push_str(cont);
                }
                joining = more;
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if name.is_none() {
                name = trimmed.strip_prefix('[').and_then(|r| r.strip_suffix(']')).map(str::to_string);
                if name.is_some() {
                    continue;
                }
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let (value, more) = split_continuation(v.trim_start());
                entries.push((k.trim().to_string(), value.to_string()));
                joining = more;
            }
        }
        Some(Conf { name: name?, entries })
    }

    /// First value for `key` (case-sensitive, like SWORD).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> {
        self.entries.iter().filter(move |(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Where the module's binary data lives, under the library root.
    pub fn data_dir(&self, root: &Path) -> Option<PathBuf> {
        let rel = self.get("DataPath")?.trim_start_matches("./");
        Some(root.join(rel))
    }
}

/// A trailing backslash means the value continues on the next line.
fn split_continuation(line: &str) -> (&str, bool) {
    match line.trim_end().strip_suffix('\\') {
        Some(rest) => (rest, true),
        None => (line.trim_end(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys_repeats_and_continuations() {
        let c = Conf::parse(
            "[ASV]\nAbbreviation=ASV\nGlobalOptionFilter=A\nGlobalOptionFilter=B\nAbout=one\\\ntwo\\\nthree\nLang=en\n",
        )
        .unwrap();
        assert_eq!(c.name, "ASV");
        assert_eq!(c.get("Abbreviation"), Some("ASV"));
        assert_eq!(c.all("GlobalOptionFilter").collect::<Vec<_>>(), ["A", "B"]);
        assert_eq!(c.get("About"), Some("one\ntwo\nthree"));
        assert_eq!(c.get("Lang"), Some("en"));
    }

    #[test]
    fn data_path_is_relative_to_the_root() {
        let c = Conf::parse("[X]\nDataPath=./modules/texts/ztext/x/\n").unwrap();
        assert_eq!(c.data_dir(Path::new("/lib")).unwrap(), Path::new("/lib/modules/texts/ztext/x/"));
    }

    #[test]
    fn no_section_header_is_not_a_module() {
        assert!(Conf::parse("Lang=en\n").is_none());
    }
}
