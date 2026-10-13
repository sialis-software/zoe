//! What every tab works on: the backend, the notes and marks, the vault, and the app-wide
//! settings. Tabs hold their own view state and read and change this through `&Shared`.
//!
//! When a change here affects tabs other than the one that made it (a mark removed, a note
//! file rewritten), it is recorded as an [`Event`] for the workspace to pass on.

use crate::live_editor::EmbedContent;
use crate::persist::{NoteMap, Persist};
use crate::state::{NamedLayout, Settings, Store};
use iced::Theme;
use scripture::Book;
use std::collections::{BTreeMap, BTreeSet};
use study_core::margin::{Mark, MarkStyle};
use study_core::frontmatter;
use study_core::templates::{self, Template};
use study_core::markdown::{self, LinkTarget, find_links};
use study_core::{ModuleId, StudyBackend};

/// Identifies a tab for as long as the app runs.
pub type TabId = u32;

/// A change other tabs may need to follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A mark was removed, so mark numbers after it moved down. Tabs showing mark cards or
    /// popups (other than `by`, which has dealt with itself) drop them.
    MarkRemoved { by: Option<TabId> },
    /// A file's text was changed behind the tabs showing it (a comment typed in a card), and
    /// it may have been renamed. Note tabs on it reload.
    NoteRewritten { old: String, new: String },
    /// A file was renamed (an edit to a margin file's own text changed its name).
    NoteRenamed { old: String, new: String },
    /// A file was deleted. Note tabs on it open `next` instead.
    NoteRemoved { old: String, next: String },
}

/// A note that links to a passage, and the lines of it that do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlink {
    pub path: String,
    pub lines: Vec<String>,
}

/// A short message shown over the window for a moment ("Copied …").
pub struct Toast {
    pub text: String,
    pub until: std::time::Instant,
}

/// How long a toast stays.
pub const TOAST_FOR: std::time::Duration = std::time::Duration::from_millis(2200);

pub struct Shared {
    pub backend: Box<dyn StudyBackend>,
    /// Current text of every note, keyed by path. Changes are written to the vault after
    /// each update, when there is one.
    pub notes: NoteMap,
    pub persist: Option<Persist>,
    /// The folder of the vault the notes are from.
    pub vault: Option<std::path::PathBuf>,
    /// Every mark, in memory; each is also a file under `margins/`.
    pub marks: Vec<Mark>,
    pub next_mark: u32,
    pub settings: Settings,
    /// Where settings, the session and saved layouts are kept; none for the in-memory app.
    pub store: Option<Store>,
    /// The workspace layouts the user saved.
    pub layouts: Vec<NamedLayout>,
    pub status: String,
    pub toast: Option<Toast>,
    pub events: Vec<Event>,
    /// Every tag with the number of notes that have it, as of the notes' revision it is for.
    tag_cache: std::cell::RefCell<(u64, Vec<(String, usize)>)>,
    /// Where each Strong's number occurs in a module, read from the whole module the first time it
    /// is asked for.
    strongs_index: std::cell::RefCell<Option<(ModuleId, std::rc::Rc<StrongsIndex>)>>,
    /// Where the Bible search indexes are kept (none: in memory), and those opened.
    pub cache_dir: Option<std::path::PathBuf>,
    pub bible_indexes: std::cell::RefCell<std::collections::HashMap<ModuleId, std::rc::Rc<vault::BibleIndex>>>,
}

/// Strong's numbers (as `G26`) with the verses they are in, in reading order.
type StrongsIndex = BTreeMap<String, Vec<Occurrence>>;

/// A verse with words that have a Strong's number, and where those words are in the verse's
/// canonical text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub range: scripture::RefRange,
    pub words: Vec<(usize, usize)>,
}

impl Shared {
    pub fn new(backend: Box<dyn StudyBackend>, notes: BTreeMap<String, String>, persist: Option<Persist>) -> Self {
        let mut marks: Vec<Mark> = notes
            .iter()
            .filter(|(path, _)| path.starts_with("margins/"))
            .filter_map(|(path, text)| Mark::from_markdown(mark_id(path)?, text))
            .collect();
        marks.sort_by(|a, b| a.id.cmp(&b.id));
        Shared {
            backend,
            notes: NoteMap::loaded(notes),
            persist,
            vault: None,
            next_mark: marks.len() as u32,
            marks,
            settings: Settings::default(),
            store: None,
            layouts: Vec::new(),
            status: String::new(),
            toast: None,
            events: Vec::new(),
            tag_cache: std::cell::RefCell::new((u64::MAX, Vec::new())),
            strongs_index: std::cell::RefCell::new(None),
            cache_dir: None,
            bible_indexes: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// Take the notes of another vault in place of these (the old ones were saved first).
    pub fn replace_vault(&mut self, dir: std::path::PathBuf, notes: BTreeMap<String, String>, persist: Persist) {
        let fresh = Shared::new(Box::new(study_core::mock::MockBackend), notes, Some(persist));
        self.notes = fresh.notes;
        self.persist = fresh.persist;
        self.marks = fresh.marks;
        self.next_mark = fresh.next_mark;
        self.vault = Some(dir);
        *self.tag_cache.borrow_mut() = (u64::MAX, Vec::new());
        self.load_pens(false);
    }

    /// The file in the vault that keeps its pens, so they go where the vault goes.
    fn pens_file(&self) -> Option<std::path::PathBuf> {
        let folder = self.vault.as_ref()?.join(PENS_FOLDER);
        // A vault from before the app was called Zoe kept them in a folder of the old name.
        let old = folder.with_file_name(OLD_PENS_FOLDER);
        if !folder.exists() && old.is_dir() {
            let _ = std::fs::rename(&old, &folder);
        }
        Some(folder.join("pens.json"))
    }

    /// Take the pens the vault keeps. A vault with none yet gets the ones in use if `carry` (the
    /// start-up of a vault that has had them in the settings until now, which are written into
    /// it), else the starting pens.
    pub fn load_pens(&mut self, carry: bool) {
        let Some(file) = self.pens_file() else { return };
        let kept: Option<Vec<crate::state::PenDef>> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok());
        match kept {
            Some(pens) => self.settings.pens = Settings { pens, ..self.settings.clone() }.sanitized().pens,
            None if carry => self.keep_pens(),
            None => self.settings.pens = crate::state::default_pens(),
        }
    }

    /// Write the pens in use into the vault.
    fn keep_pens(&mut self) {
        let Some(file) = self.pens_file() else { return };
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| serde_json::to_string_pretty(&self.settings.pens).map_err(std::io::Error::other))
            .and_then(|text| std::fs::write(&file, text));
        if let Err(e) = written {
            self.status = format!("Could not save the pens in the vault: {e}");
        }
    }

    /// Forget what was read of the modules (they have changed).
    pub fn forget_modules(&self) {
        *self.strongs_index.borrow_mut() = None;
        self.bible_indexes.borrow_mut().clear();
    }

    /// The verses of `module` that have a word with Strong's number `key` (`G26`, `H430`), in
    /// reading order, once per verse. Empty if the module has no such words. The first ask reads
    /// the whole module; the next are instant.
    pub fn strongs_occurrences(&self, module: &ModuleId, key: &str) -> Vec<Occurrence> {
        let index = {
            let mut cache = self.strongs_index.borrow_mut();
            if cache.as_ref().is_none_or(|(m, _)| m != module) {
                let mut index = StrongsIndex::new();
                for (book, chapter) in self.backend.available_chapters(module) {
                    let Ok(chapter_text) = self.backend.chapter(module, book, chapter) else { continue };
                    for word in chapter_text.strongs_words() {
                        for number in word.strongs.split_whitespace().filter_map(strongs_name) {
                            let range = scripture::RefRange { book, chapter, verses: Some((word.verse, word.verse)) };
                            let places = index.entry(number).or_default();
                            match places.last_mut() {
                                Some(last) if last.range == range => last.words.push((word.start, word.end)),
                                _ => places.push(Occurrence { range, words: vec![(word.start, word.end)] }),
                            }
                        }
                    }
                }
                *cache = Some((module.clone(), std::rc::Rc::new(index)));
            }
            cache.as_ref().map(|(_, index)| index.clone())
        };
        let Some(name) = strongs_name(key) else { return Vec::new() };
        index.and_then(|index| index.get(&name).cloned()).unwrap_or_default()
    }

    /// Show `text` for a moment.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), until: std::time::Instant::now() + TOAST_FOR });
    }

    pub fn is_dark(&self) -> bool {
        self.theme().extended_palette().is_dark
    }

    pub fn theme(&self) -> Theme {
        self.settings.theme()
    }

    /// Keep settings and layouts in `store`, and take what it has.
    pub fn attach(&mut self, store: Store) {
        self.settings = store.settings();
        self.layouts = store.layouts();
        self.store = Some(store);
    }

    /// Change a setting and keep the change.
    pub fn set_settings(&mut self, settings: Settings) {
        let settings = settings.sanitized();
        if settings == self.settings {
            return;
        }
        let pens_changed = settings.pens != self.settings.pens;
        self.settings = settings;
        if pens_changed {
            self.keep_pens();
        }
        let saved = self.store.as_ref().map(|s| s.save_settings(&self.settings));
        if let Some(Err(e)) = saved {
            self.status = format!("Could not save settings: {e}");
        }
    }

    /// Put `layout` among the saved ones (replacing one of the same name) and keep them.
    pub fn save_layout(&mut self, layout: NamedLayout) {
        match self.layouts.iter_mut().find(|l| l.name == layout.name) {
            Some(existing) => *existing = layout,
            None => self.layouts.push(layout),
        }
        self.keep_layouts();
    }

    pub fn delete_layout(&mut self, name: &str) {
        self.layouts.retain(|l| l.name != name);
        self.keep_layouts();
    }

    fn keep_layouts(&mut self) {
        let saved = self.store.as_ref().map(|s| s.save_layouts(&self.layouts));
        if let Some(Err(e)) = saved {
            self.status = format!("Could not save layouts: {e}");
        }
    }

    /// Write what changed to the vault. A failure shows in the status and the edit stays in
    /// memory.
    pub fn save(&mut self) {
        let changes = self.notes.take_changes();
        let Some(persist) = self.persist.as_mut().filter(|_| !changes.is_empty()) else { return };
        if let Err(e) = persist.flush(changes) {
            self.status = e;
        }
    }

    /// Every tag in use, with how many notes have it: the most used first, then by name.
    pub fn tag_counts(&self) -> Vec<(String, usize)> {
        let revision = self.notes.revision();
        let mut cache = self.tag_cache.borrow_mut();
        if cache.0 != revision {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for (path, text) in self.notes.iter().filter(|(path, _)| !templates::is_template(path)) {
                let _ = path;
                for tag in frontmatter::parse(text).front.tags {
                    *counts.entry(frontmatter::normalize_tag(&tag)).or_default() += 1;
                }
            }
            let mut all: Vec<(String, usize)> = counts.into_iter().filter(|(t, _)| !t.is_empty()).collect();
            all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            *cache = (revision, all);
        }
        cache.1.clone()
    }

    /// Tags to offer for what has been typed in a tag box: those in use that contain it (those
    /// that start with it first, then the more used), but not `have`. At most `limit`.
    pub fn tag_suggestions(&self, typed: &str, have: &[String], limit: usize) -> Vec<(String, usize)> {
        let typed = frontmatter::normalize_tag(typed);
        if typed.is_empty() {
            return Vec::new();
        }
        let have: Vec<String> = have.iter().map(|t| frontmatter::normalize_tag(t)).collect();
        let mut found: Vec<(String, usize)> =
            self.tag_counts().into_iter().filter(|(t, _)| t.contains(&typed) && !have.contains(t)).collect();
        found.sort_by_key(|(t, n)| (!t.starts_with(&typed), std::cmp::Reverse(*n), t.clone()));
        found.truncate(limit);
        found
    }

    /// The color a tag has, if a pen is for it (or for the tag it is inside).
    pub fn tag_color(&self, tag: &str) -> Option<String> {
        let tag = frontmatter::normalize_tag(tag);
        self.settings
            .pens
            .iter()
            .find(|pen| pen.tag == tag || tag.starts_with(&format!("{}/", pen.tag)))
            .map(|pen| pen.color.clone())
    }

    /// The tags of the note at `path`.
    pub fn tags_of(&self, path: &str) -> Vec<String> {
        self.notes.get(path).map(|text| frontmatter::parse(text).front.tags).unwrap_or_default()
    }

    pub fn note_paths(&self) -> Vec<String> {
        self.notes.keys().cloned().collect()
    }

    /// The first note that is not a margin file: what a note tab opens when it has nothing.
    pub fn first_ordinary_note(&self) -> String {
        self.notes.keys().find(|p| !p.starts_with("margins/") && !templates::is_template(p)).cloned().unwrap_or_default()
    }

    /// The kinds of note there are: the templates in the vault, by name.
    pub fn templates(&self) -> Vec<Template> {
        templates::all(self.notes.iter())
    }

    pub fn module_abbreviation(&self, module: &ModuleId) -> String {
        let info = self.backend.modules().into_iter().find(|m| &m.id == module);
        info.map_or_else(String::new, |m| m.abbreviation)
    }

    /// Where to start reading: John 3 when the module has it, else its first chapter.
    pub fn first_chapter(&self, module: &ModuleId) -> (Book, u16) {
        let chapters = self.backend.available_chapters(module);
        if chapters.contains(&(Book::JOHN, 3)) {
            (Book::JOHN, 3)
        } else {
            chapters.first().copied().unwrap_or((Book::JOHN, 3))
        }
    }

    /// The passage an `![[...]]` line shows, when the backend can serve it in `module`.
    pub fn embed_for(&self, module: &ModuleId, target: &LinkTarget) -> Option<EmbedContent> {
        let LinkTarget::Verse { range, module: pin } = target else { return None };
        let abbreviation = self.module_abbreviation(module);
        // A pin to some other module than we have means the passage is not available here.
        if pin.as_ref().is_some_and(|m| !m.eq_ignore_ascii_case(&abbreviation)) {
            return None;
        }
        let chapter = self.backend.chapter(module, range.book, range.chapter).ok()?;
        let body = chapter.plain_text(range);
        (!body.is_empty()).then(|| EmbedContent { title: format!("{range} · {abbreviation}"), body })
    }

    /// The notes that link to `passage` (a link to any of its verses, or to the whole chapter
    /// it is in), with the lines that do, in the order of the notes' paths.
    pub fn backlinks(&self, passage: &scripture::RefRange) -> Vec<Backlink> {
        let overlaps = |range: &scripture::RefRange| {
            range.book == passage.book
                && range.chapter == passage.chapter
                && match (range.verses, passage.verses) {
                    (Some((a, z)), Some((from, to))) => a <= to && from <= z,
                    _ => true,
                }
        };
        let mut out = Vec::new();
        for (path, body) in self.notes.iter().filter(|(path, _)| !templates::is_template(path)) {
            let lines: Vec<String> = body
                .lines()
                .filter(|line| {
                    find_links(line).iter().any(|link| matches!(&link.target, LinkTarget::Verse { range, .. } if overlaps(range)))
                })
                .map(|line| line.trim().to_string())
                .collect();
            if !lines.is_empty() {
                out.push(Backlink { path: path.clone(), lines });
            }
        }
        out
    }

    /// The notes that link to the note at `path` by name (`[[Nicodemus]]`, or with another name
    /// shown), with the lines that do. Not the note itself, nor the templates.
    pub fn note_backlinks(&self, path: &str) -> Vec<Backlink> {
        let paths = self.note_paths();
        let mut out = Vec::new();
        for (from, body) in self.notes.iter().filter(|(p, _)| p.as_str() != path && !templates::is_template(p)) {
            let lines: Vec<String> = body
                .lines()
                .filter(|line| {
                    find_links(line).iter().any(|link| {
                        matches!(&link.target, LinkTarget::Note(name) if markdown::resolve_note(&paths, name).is_some_and(|p| p == path))
                    })
                })
                .map(|line| line.trim().to_string())
                .collect();
            if !lines.is_empty() {
                out.push(Backlink { path: from.clone(), lines });
            }
        }
        out
    }

    /// Verses of `book chapter` that some note links to by number.
    pub fn annotated_verses(&self, book: Book, chapter: u16) -> BTreeSet<u16> {
        let mut out = BTreeSet::new();
        for body in self.notes.iter().filter(|(path, _)| !templates::is_template(path)).map(|(_, body)| body) {
            for link in body.lines().flat_map(find_links) {
                if let LinkTarget::Verse { range, .. } = link.target
                    && range.book == book
                    && range.chapter == chapter
                    && let Some((a, z)) = range.verses
                {
                    out.extend(a..=z);
                }
            }
        }
        out
    }

    /// Index of the mark whose file is `path`.
    pub fn mark_index_of(&self, path: &str) -> Option<usize> {
        if !path.starts_with("margins/") {
            return None;
        }
        let id = mark_id(path)?;
        self.marks.iter().position(|m| m.id == id)
    }

    /// Remove mark `index` along with its note file. Later marks move down one.
    pub fn delete_mark(&mut self, index: usize, by: Option<TabId>) {
        if index >= self.marks.len() {
            return;
        }
        let path = self.marks.remove(index).path();
        self.notes.remove(&path);
        self.events.push(Event::MarkRemoved { by });
        let next = self.first_ordinary_note();
        self.events.push(Event::NoteRemoved { old: path, next });
    }

    /// Delete an ordinary note's file. Tabs showing it move to another note; if it was the last
    /// one, to a fresh empty note, so there is always somewhere to write.
    pub fn delete_note(&mut self, path: &str) {
        if self.notes.remove_if_present(path).is_none() {
            return;
        }
        let mut next = self.first_ordinary_note();
        if next.is_empty() {
            next = "notes/Untitled.md".to_string();
            self.notes.insert(next.clone(), String::new());
        }
        self.events.push(Event::NoteRemoved { old: path.to_string(), next });
    }

    /// Give an ordinary note a new name (its file's name, in the same folder). The links to it from
    /// other notes follow, and so does its title if that was just its old name. Returns the new
    /// path, or why the name cannot be used.
    pub fn rename_note(&mut self, old: &str, name: &str) -> Result<String, String> {
        let name = name.trim();
        if old.starts_with("margins/") {
            return Err("A margin note is named by the passage it marks".into());
        }
        if name.is_empty() {
            return Err("A note needs a name".into());
        }
        if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
            return Err("A name cannot start with a dot or contain / \\ : * ? \" < > |".into());
        }
        let dir = old.rsplit_once('/').map_or("", |(d, _)| d);
        let new = if dir.is_empty() { format!("{name}.md") } else { format!("{dir}/{name}.md") };
        if new == old {
            return Ok(new);
        }
        // (Names differing only in capitals are the same file on many systems.)
        if self.notes.keys().any(|p| p != old && p.eq_ignore_ascii_case(&new)) {
            return Err(format!("A note called \u{201C}{name}\u{201D} already exists"));
        }
        let Some(text) = self.notes.remove_if_present(old) else { return Err("That note is gone".into()) };
        let old_stem = old.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string();
        // A title that only repeated the old name now repeats the new one.
        let edit = frontmatter::update(&text, |f| {
            if f.title.as_deref() == Some(old_stem.as_str()) {
                f.title = Some(name.to_string());
            }
        });
        let text = if frontmatter::parse(&text).range.is_some() { frontmatter::apply(&text, &edit) } else { text };
        self.notes.insert(new.clone(), text);
        // Links to it, wherever they are.
        let changed: Vec<(String, String)> = self
            .notes
            .iter()
            .filter_map(|(path, body)| markdown::rename_note_links(body, old, &new).map(|b| (path.clone(), b)))
            .collect();
        for (path, body) in changed {
            self.notes.insert(path.clone(), body);
            self.sync_mark(&path);
            self.events.push(Event::NoteRewritten { old: path.clone(), new: path });
        }
        self.events.push(Event::NoteRewritten { old: old.to_string(), new: new.clone() });
        Ok(new)
    }

    /// Detach mark `index` from the text. It stops being drawn; its note stays, as an
    /// ordinary note that keeps the quote and a link back to the verses.
    pub fn unlink_mark(&mut self, index: usize, by: Option<TabId>) {
        if index >= self.marks.len() {
            return;
        }
        let mark = self.marks.remove(index);
        let old = mark.path();
        self.notes.remove(&old);
        let (path, text) = mark.unlinked_note();
        self.notes.insert(path.clone(), text);
        self.events.push(Event::MarkRemoved { by });
        self.events.push(Event::NoteRemoved { old, next: path });
    }

    /// After an edit to a margin file, bring the matching mark in line with its text.
    /// Text that no longer parses leaves the mark as it was.
    pub fn sync_mark(&mut self, path: &str) {
        let Some(id) = mark_id(path).filter(|_| path.starts_with("margins/")) else { return };
        let Some(parsed) = self.notes.get(path).and_then(|t| Mark::from_markdown(id, t)) else {
            return;
        };
        let new_path = parsed.path();
        match self.marks.iter_mut().find(|m| m.id == id) {
            Some(existing) => *existing = parsed,
            None => self.marks.push(parsed),
        }
        // Editing a margin file by hand can change what it should be called.
        if new_path != path
            && let Some(text) = self.notes.get(path).cloned()
        {
            self.notes.remove(path);
            self.notes.insert(new_path.clone(), text);
            self.events.push(Event::NoteRenamed { old: path.to_string(), new: new_path });
        }
    }

    /// Give mark `index` this comment, in the mark and in its note file.
    pub fn set_body(&mut self, index: usize, body: String) {
        let Some(mark) = self.marks.get_mut(index) else { return };
        let old = mark.path();
        mark.body = body;
        let (path, markdown) = (mark.path(), mark.to_markdown());
        // A highlight that gains a comment is filed as a note, so its file name changes.
        if path != old {
            self.notes.remove(&old);
        }
        self.notes.insert(path.clone(), markdown);
        self.events.push(Event::NoteRewritten { old, new: path });
    }

    /// Give mark `index` another look: how it marks the text, and in what color (`#RRGGBB`). A bare
    /// highlight is filed under its style, so its file name can change.
    pub fn set_mark_look(&mut self, index: usize, style: MarkStyle, color: &str) {
        let Some(mark) = self.marks.get_mut(index) else { return };
        if mark.style == style && mark.color == color {
            return;
        }
        let old = mark.path();
        mark.style = style;
        mark.color = color.to_string();
        let (path, markdown) = (mark.path(), mark.to_markdown());
        if path != old {
            self.notes.remove(&old);
        }
        self.notes.insert(path.clone(), markdown);
        self.events.push(Event::NoteRewritten { old, new: path });
    }

    /// Give mark `index` these tags (in one form, each once) and keep its note file in step.
    pub fn set_mark_tags(&mut self, index: usize, tags: Vec<String>) {
        let Some(mark) = self.marks.get_mut(index) else { return };
        let mut kept: Vec<String> = Vec::new();
        for tag in tags {
            let tag = frontmatter::normalize_tag(&tag);
            if !tag.is_empty() && !kept.contains(&tag) {
                kept.push(tag);
            }
        }
        if mark.tags == kept {
            return;
        }
        mark.tags = kept;
        let (path, markdown) = (mark.path(), mark.to_markdown());
        self.notes.insert(path.clone(), markdown);
        self.events.push(Event::NoteRewritten { old: path.clone(), new: path });
    }

    /// A fresh mark id: unique across sessions, since a vault can already hold marks.
    pub fn new_mark_id(&mut self) -> String {
        loop {
            self.next_mark += 1;
            let id = format!("{:04x}", self.next_mark);
            if self.marks.iter().all(|m| m.id != id) {
                return id;
            }
        }
    }
}

/// The hidden folder in a vault where the app keeps what is not a note, and its earlier name.
const PENS_FOLDER: &str = ".zoe";
const OLD_PENS_FOLDER: &str = ".bible-study";

/// A Strong's number in one spelling: `g0026` and `G26` are `G26`.
pub fn strongs_name(key: &str) -> Option<String> {
    let key = key.trim();
    let letter = key.chars().next()?.to_ascii_uppercase();
    let number: u32 = key.get(1..)?.parse().ok()?;
    matches!(letter, 'G' | 'H').then(|| format!("{letter}{number}"))
}

/// The id at the end of a margin file's name: `margins/John/03/16-note-s1.md` is `s1`.
pub fn mark_id(path: &str) -> Option<&str> {
    path.trim_end_matches(".md").rsplit('-').next()
}
