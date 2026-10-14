//! What is remembered between runs: the settings, the workspace as it was left, and the
//! workspace layouts the user saved. All are small JSON files in the app's data folder.
//!
//! Reading is forgiving: a missing file is the defaults, and an unreadable one is set aside
//! (as `*.bad`) rather than lost or fatal. Writing goes through a temporary file, so a crash
//! never leaves half a file.

use iced::{Font, Theme};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{fs, io};

// ---- settings ------------------------------------------------------------------------------

pub const BIBLE_SIZES: RangeInclusive<f32> = 12.0..=36.0;
pub const NOTES_SIZES: RangeInclusive<f32> = 12.0..=32.0;

/// How many seconds in front of the reader make a chapter read, at least and at most.
pub const READ_SECONDS: std::ops::Range<u32> = 10..600;

/// How the app looks. Changing one takes effect at once and is saved at once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The color scheme: the name of one of iced's themes ("Dark", "Light", "Nord", …).
    pub theme: String,
    /// Size of the text in Bible panes: the reading text and everything beside it (footnotes,
    /// dictionary entries, margin cards). Other text in the pane is in proportion.
    #[serde(alias = "reading_size")]
    pub bible_size: f32,
    /// Size of the text in note panes.
    #[serde(alias = "note_size")]
    pub notes_size: f32,
    /// The family the Bible and notes are set in; `None` is the interface font.
    pub font: Option<String>,
    /// The pens: each is a shortcut for making a note on the selected text with a tag, in a color.
    pub pens: Vec<PenDef>,
    /// Add each chapter read to the reading log note by itself, once it has been in front of the
    /// reader for `reading_log_seconds` (see `study_core::reading_log`). Off unless asked for; the
    /// button at the foot of a chapter logs it by hand either way.
    pub reading_log_auto: bool,
    pub reading_log_seconds: u32,
    /// Add it to the day's note too, when there is one.
    pub reading_log_daily: bool,
    /// Add each new mark (highlight, underline or note on a passage) to today's note under its
    /// `## Bookmarks`, when there is such a note with such a heading.
    pub bookmarks_daily: bool,
    /// Vim key bindings in the writer (normal, insert and visual modes).
    pub vim_mode: bool,
}

/// How a pen marks the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PenStyle {
    Highlight,
    Underline,
}

/// A pen: a tag, the color of the tag (and of its marks) and how the text is marked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PenDef {
    pub tag: String,
    /// `#RRGGBB`.
    pub color: String,
    pub style: PenStyle,
}

impl PenDef {
    /// What the pen is called on the toolbar: its tag, capitalized.
    pub fn name(&self) -> String {
        let last = self.tag.rsplit('/').next().unwrap_or(&self.tag).replace('-', " ");
        let mut chars = last.chars();
        chars.next().map_or_else(String::new, |f| f.to_uppercase().chain(chars).collect())
    }
}

/// The color a pen has when none valid is given: the medium yellow.
pub const DEFAULT_PEN_COLOR: usize = 10;

/// The colors a pen can be: three rows of eight, light, medium and deep, each in hue order.
pub const PEN_COLORS: [&str; 24] = [
    // Light, in hue order: red, orange, yellow, green, teal, blue, purple, pink.
    "#FF9E9E", "#FFC285", "#FFF176", "#A5E6A0", "#8FE6DC", "#9CCBFF", "#D0B8FF", "#FFB3D9",
    // Medium.
    "#FF5252", "#FF9F43", "#F5D90A", "#5FD38D", "#2EC4B6", "#4DA3FF", "#B388FF", "#FF7AB6",
    // Deep.
    "#C62828", "#E65100", "#C9A800", "#2E9E5B", "#00897B", "#1E6FD9", "#7C4DFF", "#D81B7A",
];

/// A new vault starts with no pens: they are the person's own.
pub fn default_pens() -> Vec<PenDef> {
    Vec::new()
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: Theme::Dark.to_string(), bible_size: 19.0, notes_size: 17.0, font: None, pens: default_pens(), reading_log_auto: false, reading_log_seconds: 30, reading_log_daily: true, bookmarks_daily: false, vim_mode: false }
    }
}

impl Settings {
    /// Settings that make sense, whatever the file said.
    pub fn sanitized(mut self) -> Self {
        let fix = |v: f32, r: &RangeInclusive<f32>, default: f32| if v.is_finite() { v.clamp(*r.start(), *r.end()) } else { default };
        self.bible_size = fix(self.bible_size, &BIBLE_SIZES, 19.0);
        self.notes_size = fix(self.notes_size, &NOTES_SIZES, 17.0);
        self.font = self.font.filter(|f| !f.trim().is_empty());
        self.reading_log_seconds = self.reading_log_seconds.clamp(READ_SECONDS.start, READ_SECONDS.end);
        // Pens: tags in one form, each once, colors that are colors.
        let mut seen = Vec::new();
        self.pens.retain_mut(|pen| {
            pen.tag = study_core::frontmatter::normalize_tag(&pen.tag);
            let color_ok = pen.color.len() == 7 && pen.color.starts_with('#') && pen.color[1..].chars().all(|c| c.is_ascii_hexdigit());
            if !color_ok {
                pen.color = PEN_COLORS[DEFAULT_PEN_COLOR].to_string();
            }
            let fresh = !pen.tag.is_empty() && !seen.contains(&pen.tag);
            seen.push(pen.tag.clone());
            fresh
        });
        self.pens.truncate(12);
        self
    }

    pub fn theme(&self) -> Theme {
        Theme::ALL.iter().find(|t| t.to_string() == self.theme).cloned().unwrap_or(Theme::Dark)
    }

    /// The face for Bible and note text.
    pub fn text_font(&self) -> Font {
        match &self.font {
            Some(family) => Font::with_name(intern(family)),
            None => crate::fonts::UI,
        }
    }
}

/// `Font::with_name` wants a name that lives forever. Each distinct family is kept once.
fn intern(name: &str) -> &'static str {
    static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(known) = names.iter().find(|n| **n == name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.push(leaked);
    leaked
}

/// The font families installed on this machine, sorted.
pub fn installed_families() -> Vec<String> {
    let mut system = iced::advanced::graphics::text::font_system().write().unwrap_or_else(|e| e.into_inner());
    let mut names: Vec<String> = system
        .raw()
        .db()
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

// ---- the workspace -------------------------------------------------------------------------

/// The arrangement of panes and tabs, in a form that outlives the run: no ids of tabs, and what
/// each tab shows by name. (A tab's back/forward history is not kept.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub layout: Layout,
    pub panes: Vec<PaneSave>,
    /// The pane last used, and the one shown alone, if any (ids as in `layout`).
    pub focus: u32,
    pub zoomed: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisSave {
    Row,
    Column,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layout {
    Pane { id: u32 },
    Split { axis: AxisSave, fraction: f32, a: Box<Layout>, b: Box<Layout> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneSave {
    pub id: u32,
    pub tabs: Vec<TabSave>,
    /// Index into `tabs` of the one showing.
    pub active: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TabSave {
    Blank,
    Bible {
        module: String,
        book: String,
        chapter: u16,
        scroll: f32,
        strongs: bool,
        marks: bool,
        footnotes: bool,
        refs: bool,
        /// Which kinds of note sit in the margin; sessions saved before that was a choice have
        /// the default.
        #[serde(default)]
        in_margin: MarginKinds,
    },
    Note { path: String },
    Dict { module: String, key: String },
    Commentary { module: String, book: String, chapter: u16 },
    /// A Bible search: the words typed and how they match.
    Search { module: String, query: String, case: bool, stem: bool, best: bool },
    /// The notes that link to a passage (as OSIS, `John.3.16`).
    Backlinks { passage: String },
    /// The notes that link to a note.
    NoteBacklinks { path: String },
    /// The plans: the plan (its key) and the day shown.
    Plan { plan: Option<String>, day: Option<usize> },
}

/// Which kinds of note sit in a reader's margin (the rest pop over the text where clicked).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarginKinds {
    pub strongs: bool,
    pub marks: bool,
    pub footnotes: bool,
    pub refs: bool,
}

impl Default for MarginKinds {
    fn default() -> Self {
        MarginKinds { strongs: false, marks: true, footnotes: true, refs: false }
    }
}

/// A workspace the user saved under a name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedLayout {
    pub name: String,
    pub workspace: Workspace,
}

/// The one choice kept outside the vault and the state folder, since it says where they are: the
/// vault in use. A file in the data folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub vault: Option<PathBuf>,
    /// Folders of SWORD modules the person added, searched before the usual places.
    #[serde(default)]
    pub module_folders: Vec<PathBuf>,
    /// Persecution mode: nothing uses the network (see `network.rs`).
    #[serde(default)]
    pub persecution: bool,
}

impl Config {
    const FILE: &'static str = "config.json";

    pub fn load(home: &Path) -> Config {
        fs::read_to_string(home.join(Self::FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// What was in the folder `old` is now in `data` (and the choices in `config`): the folders the
    /// choices name inside it are the same ones in their new place.
    pub fn moved(config_dir: &Path, old: &Path, data: &Path) {
        let mut config = Config::load(config_dir);
        let follow = |path: &mut PathBuf| {
            if let Ok(rest) = path.strip_prefix(old) {
                *path = data.join(rest);
            }
        };
        config.vault.as_mut().map(follow);
        config.module_folders.iter_mut().for_each(follow);
        let _ = config.save(config_dir);
    }

    pub fn save(&self, home: &Path) -> io::Result<()> {
        fs::create_dir_all(home)?;
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let path = home.join(Self::FILE);
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, &path)
    }
}

// ---- the files -----------------------------------------------------------------------------

/// The files that are kept: what the person chose (settings, saved layouts) in the config folder,
/// what the app makes (the session) in the data folder.
pub struct Store {
    config: PathBuf,
    data: PathBuf,
}

impl Store {
    /// Everything in one folder.
    #[cfg(test)]
    pub fn new(dir: PathBuf) -> Self {
        Store { config: dir.clone(), data: dir }
    }

    /// Choices in `config`, the session in `data`.
    pub fn split(config: PathBuf, data: PathBuf) -> Self {
        Store { config, data }
    }

    /// The folder `file` is kept in.
    fn dir_of(&self, file: &str) -> &Path {
        if file == "session.json" { &self.data } else { &self.config }
    }

    pub fn settings(&self) -> Settings {
        self.read("settings.json").map(Settings::sanitized).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        self.write("settings.json", settings)
    }

    pub fn session(&self) -> Option<Workspace> {
        self.read("session.json")
    }

    pub fn save_session(&self, workspace: &Workspace) -> io::Result<()> {
        self.write("session.json", workspace)
    }

    pub fn layouts(&self) -> Vec<NamedLayout> {
        self.read("layouts.json").unwrap_or_default()
    }

    pub fn save_layouts(&self, layouts: &[NamedLayout]) -> io::Result<()> {
        self.write("layouts.json", &layouts)
    }

    fn read<T: for<'de> Deserialize<'de>>(&self, file: &str) -> Option<T> {
        let path = self.dir_of(file).join(file);
        let text = fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(_) => {
                let _ = fs::rename(&path, path.with_extension("json.bad"));
                None
            }
        }
    }

    fn write<T: Serialize + ?Sized>(&self, file: &str, value: &T) -> io::Result<()> {
        fs::create_dir_all(self.dir_of(file))?;
        let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
        let path = self.dir_of(file).join(file);
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, &path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Workspace {
        Workspace {
            layout: Layout::Split {
                axis: AxisSave::Row,
                fraction: 0.4,
                a: Box::new(Layout::Pane { id: 1 }),
                b: Box::new(Layout::Pane { id: 2 }),
            },
            panes: vec![
                PaneSave {
                    id: 1,
                    tabs: vec![TabSave::Bible {
                        module: "LSB".into(),
                        book: "John".into(),
                        chapter: 3,
                        scroll: 120.0,
                        strongs: true,
                        marks: true,
                        footnotes: false,
                        refs: true,
                        in_margin: MarginKinds { strongs: true, marks: false, footnotes: true, refs: true },
                    }],
                    active: Some(0),
                },
                PaneSave { id: 2, tabs: vec![TabSave::Note { path: "a.md".into() }, TabSave::Blank], active: Some(1) },
            ],
            focus: 2,
            zoomed: None,
        }
    }

    #[test]
    fn everything_round_trips_through_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("state"));
        assert_eq!(store.settings(), Settings::default());
        assert_eq!(store.session(), None);
        assert!(store.layouts().is_empty());

        let settings = Settings { theme: "Nord".into(), bible_size: 22.0, notes_size: 15.0, font: Some("Georgia".into()), pens: default_pens(), ..Settings::default() };
        store.save_settings(&settings).unwrap();
        store.save_session(&sample()).unwrap();
        store.save_layouts(&[NamedLayout { name: "Study".into(), workspace: sample() }]).unwrap();

        let again = Store::new(dir.path().join("state"));
        assert_eq!(again.settings(), settings);
        assert_eq!(again.session(), Some(sample()));
        assert_eq!(again.layouts()[0].name, "Study");
        assert_eq!(again.layouts()[0].workspace, sample());
    }

    #[test]
    fn an_unreadable_file_is_set_aside_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        fs::write(dir.path().join("session.json"), "{ not json").unwrap();
        assert_eq!(store.session(), None);
        assert!(dir.path().join("session.json.bad").exists());
        assert!(!dir.path().join("session.json").exists());
    }

    #[test]
    fn settings_files_missing_fields_or_with_bad_values_are_repaired() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        fs::write(dir.path().join("settings.json"), r#"{ "theme": "Dracula", "reading_size": 400, "font": "" }"#).unwrap();
        let s = store.settings();
        assert_eq!(s.theme, "Dracula");
        assert_eq!(s.bible_size, *BIBLE_SIZES.end());
        assert_eq!(s.notes_size, 17.0, "missing: the default");
        assert_eq!(s.font, None);
        assert_eq!(s.theme(), Theme::Dracula);
        assert_eq!(Settings { theme: "Nonsense".into(), ..Settings::default() }.theme(), Theme::Dark);
    }

    #[test]
    fn vim_mode_is_off_unless_the_settings_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        fs::write(dir.path().join("settings.json"), r#"{ "theme": "Dark" }"#).unwrap();
        assert!(!store.settings().vim_mode);
        fs::write(dir.path().join("settings.json"), r#"{ "vim_mode": true }"#).unwrap();
        assert!(store.settings().vim_mode);
    }

    #[test]
    fn every_theme_name_finds_its_theme() {
        for theme in Theme::ALL {
            assert_eq!(&Settings { theme: theme.to_string(), ..Settings::default() }.theme(), theme);
        }
    }

    #[test]
    fn choices_are_kept_in_the_config_folder_and_the_session_in_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (config, data) = (dir.path().join("config/zoe"), dir.path().join("data/zoe"));
        let store = Store::split(config.clone(), data.clone());
        store.save_settings(&Settings::default()).unwrap();
        store.save_layouts(&[]).unwrap();
        store.save_session(&sample()).unwrap();
        assert!(config.join("settings.json").is_file() && config.join("layouts.json").is_file());
        assert!(data.join("session.json").is_file());
        assert!(!config.join("session.json").exists() && !data.join("settings.json").exists());
        assert_eq!(Store::split(config, data).session(), Some(sample()));
    }
}
