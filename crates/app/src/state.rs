//! What is remembered between runs: the settings, the workspace as it was left, and the
//! workspace layouts the user saved. All are small JSON files in the app's data folder.
//!
//! Reading is forgiving: a missing file is the defaults, and an unreadable one is set aside
//! (as `*.bad`) rather than lost or fatal. Writing goes through a temporary file, so a crash
//! never leaves half a file.

use iced::{Font, Theme};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{fs, io};

// ---- settings ------------------------------------------------------------------------------

pub const READING_SIZES: RangeInclusive<f32> = 12.0..=36.0;
pub const NOTE_SIZES: RangeInclusive<f32> = 12.0..=32.0;

/// How the app looks. Changing one takes effect at once and is saved at once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The color scheme: the name of one of iced's themes ("Dark", "Light", "Nord", …).
    pub theme: String,
    /// Size of the Bible text.
    pub reading_size: f32,
    /// Size of the text in notes and margin cards.
    pub note_size: f32,
    /// The family the Bible and notes are set in; `None` is the interface font.
    pub font: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: Theme::Dark.to_string(), reading_size: 19.0, note_size: 17.0, font: None }
    }
}

impl Settings {
    /// Settings that make sense, whatever the file said.
    pub fn sanitized(mut self) -> Self {
        let fix = |v: f32, r: &RangeInclusive<f32>, default: f32| if v.is_finite() { v.clamp(*r.start(), *r.end()) } else { default };
        self.reading_size = fix(self.reading_size, &READING_SIZES, 19.0);
        self.note_size = fix(self.note_size, &NOTE_SIZES, 17.0);
        self.font = self.font.filter(|f| !f.trim().is_empty());
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
    Bible { module: String, book: String, chapter: u16, scroll: f32, strongs: bool, marks: bool, footnotes: bool, refs: bool },
    Note { path: String },
    Dict { module: String, key: String },
}

/// A workspace the user saved under a name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedLayout {
    pub name: String,
    pub workspace: Workspace,
}

// ---- the files -----------------------------------------------------------------------------

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Store { dir }
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
        let path = self.dir.join(file);
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
        fs::create_dir_all(&self.dir)?;
        let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
        let path = self.dir.join(file);
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

        let settings = Settings { theme: "Nord".into(), reading_size: 22.0, note_size: 15.0, font: Some("Georgia".into()) };
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
        assert_eq!(s.reading_size, *READING_SIZES.end());
        assert_eq!(s.note_size, 17.0, "missing: the default");
        assert_eq!(s.font, None);
        assert_eq!(s.theme(), Theme::Dracula);
        assert_eq!(Settings { theme: "Nonsense".into(), ..Settings::default() }.theme(), Theme::Dark);
    }

    #[test]
    fn every_theme_name_finds_its_theme() {
        for theme in Theme::ALL {
            assert_eq!(&Settings { theme: theme.to_string(), ..Settings::default() }.theme(), theme);
        }
    }
}
