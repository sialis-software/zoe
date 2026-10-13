//! What every tab works on: the backend, the notes and marks, the vault, and the app-wide
//! settings. Tabs hold their own view state and read and change this through `&Shared`.
//!
//! When a change here affects tabs other than the one that made it (a mark removed, a note
//! file rewritten), it is recorded as an [`Event`] for the workspace to pass on.

use crate::live_editor::EmbedContent;
use crate::persist::{NoteMap, Persist};
use iced::Theme;
use scripture::Book;
use std::collections::{BTreeMap, BTreeSet};
use study_core::margin::Mark;
use study_core::frontmatter;
use study_core::markdown::{self, LinkTarget, find_links, resolve_note};
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
    /// Every mark, in memory; each is also a file under `margins/`.
    pub marks: Vec<Mark>,
    pub next_mark: u32,
    pub dark: bool,
    pub status: String,
    pub toast: Option<Toast>,
    pub events: Vec<Event>,
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
            next_mark: marks.len() as u32,
            marks,
            dark: true,
            status: String::new(),
            toast: None,
            events: Vec::new(),
        }
    }

    /// Show `text` for a moment.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), until: std::time::Instant::now() + TOAST_FOR });
    }

    pub fn theme(&self) -> Theme {
        if self.dark { Theme::Dark } else { Theme::Light }
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

    pub fn note_paths(&self) -> Vec<String> {
        self.notes.keys().cloned().collect()
    }

    /// The first note that is not a margin file: what a note tab opens when it has nothing.
    pub fn first_ordinary_note(&self) -> String {
        self.notes.keys().find(|p| !p.starts_with("margins/")).cloned().unwrap_or_default()
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

    /// Plain-text preview of what a link points at.
    pub fn preview(&self, module: &ModuleId, target: &LinkTarget) -> Option<String> {
        match target {
            LinkTarget::Verse { range, .. } => self
                .backend
                .chapter(module, range.book, range.chapter)
                .ok()
                .map(|c| c.plain_text(range))
                .filter(|t| !t.is_empty()),
            LinkTarget::Sword { key, .. } => self
                .backend
                .lexicon_entry(key)
                .map(|e| format!("{} ({}): {}", e.lemma, e.transliteration, e.definition)),
            LinkTarget::Note(name) => resolve_note(&self.note_paths(), name).map(|p| format!("→ {p}")),
        }
    }

    /// Verses of `book chapter` that some note links to by number.
    pub fn annotated_verses(&self, book: Book, chapter: u16) -> BTreeSet<u16> {
        let mut out = BTreeSet::new();
        for body in self.notes.values() {
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

/// The id at the end of a margin file's name: `margins/John/03/16-note-s1.md` is `s1`.
pub fn mark_id(path: &str) -> Option<&str> {
    path.trim_end_matches(".md").rsplit('-').next()
}
