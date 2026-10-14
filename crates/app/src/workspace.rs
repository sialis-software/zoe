//! The workspace as saved data: [`App::workspace`] describes the panes and tabs, and
//! [`App::apply_workspace`] puts such a description back (for the session restored at start-up
//! and for the layouts the user saved).

use crate::bible_tab::BibleTab;
use crate::dict_tab::DictTab;
use crate::panes::{Axis, Node};
use crate::state::{AxisSave, Layout, PaneSave, TabSave, Workspace};
use crate::{App, Message, Pane, Tab};
use iced::Task;
use iced::widget::operation::{AbsoluteOffset, scroll_to};
use scripture::Book;
use std::collections::BTreeMap;
use study_core::ModuleId;

impl App {
    /// The panes and tabs as they are now.
    pub fn workspace(&self) -> Workspace {
        let panes = self
            .panes
            .iter()
            .map(|pane| PaneSave {
                id: pane.id,
                tabs: pane.tabs.iter().filter_map(|id| self.tabs.get(id)).map(save_tab).collect(),
                active: pane.active.and_then(|a| pane.tabs.iter().position(|&t| t == a)),
            })
            .collect();
        Workspace { layout: save_node(&self.layout), panes, focus: self.focus, zoomed: self.zoomed }
    }

    /// The built-in layout: the Bible on the left half, a note on the right.
    pub fn default_workspace(&self) -> Workspace {
        let module = self.first_module();
        let (book, chapter) = self.sh.first_chapter(&module);
        // The Bible tab's usual view options, as a new tab has them.
        let tab = BibleTab::started(0, &self.sh, module.clone(), None);
        let bible = TabSave::Bible {
            module: module.0,
            book: book.osis().to_string(),
            chapter,
            scroll: 0.0,
            strongs: tab.show_strongs,
            marks: tab.show_marks,
            footnotes: tab.show_footnotes,
            refs: tab.show_refs,
            in_margin: tab.in_margin,
        };
        let note = match self.sh.first_ordinary_note() {
            path if path.is_empty() => TabSave::Blank,
            path => TabSave::Note { path },
        };
        Workspace {
            layout: Layout::Split {
                axis: AxisSave::Row,
                fraction: 0.5,
                a: Box::new(Layout::Pane { id: 1 }),
                b: Box::new(Layout::Pane { id: 2 }),
            },
            panes: vec![
                // With no Bible modules there is nothing for a Bible tab to read: a new tab, whose
                // page says where to get some.
                PaneSave { id: 1, tabs: vec![if self.sh.backend.modules().is_empty() { TabSave::Blank } else { bible }], active: Some(0) },
                PaneSave { id: 2, tabs: vec![note], active: Some(0) },
            ],
            focus: 1,
            zoomed: None,
        }
    }

    /// Replace what is open with `saved`. A tab whose module or note is gone is left out (and a
    /// pane left with no tabs with it); if nothing at all can be put back, nothing changes and
    /// this says so.
    pub fn apply_workspace(&mut self, saved: &Workspace) -> Result<Task<Message>, String> {
        let mut built: BTreeMap<u32, (u32, Vec<Tab>, Option<usize>)> = BTreeMap::new();
        let mut scrolls: Vec<(iced::widget::Id, f32)> = Vec::new();
        for pane in &saved.panes {
            let mut tabs = Vec::new();
            let mut active = None;
            for (i, tab) in pane.tabs.iter().enumerate() {
                let Some(tab) = self.build_tab(tab) else { continue };
                if pane.active == Some(i) {
                    active = Some(tabs.len());
                }
                if let Tab::Bible(b) = &tab
                    && b.scroll_y > 0.0
                {
                    scrolls.push((b.reader_id(), b.scroll_y));
                }
                tabs.push(tab);
            }
            if !tabs.is_empty() {
                let id = self.next_pane;
                self.next_pane += 1;
                built.insert(pane.id, (id, tabs, active));
            }
        }
        let first_split = self.next_split;
        let mut next_split = first_split;
        let Some(layout) = build_node(&saved.layout, &built, &mut next_split) else {
            return Err("Nothing in that layout could be opened".into());
        };
        self.next_split = next_split;

        // Commit: put away the old tabs, then set up the new.
        let old: Vec<_> = self.tabs.keys().copied().collect();
        for id in old {
            if let Some(Tab::Bible(tab)) = self.tabs.get_mut(&id) {
                tab.stop_edit(&mut self.sh);
            }
        }
        self.tabs.clear();
        self.panes.clear();
        self.last_bible = None;
        self.last_note = None;
        self.last_dict = None;
        self.link_menu = None;
        self.drag = None;
        self.pressed = None;
        self.layout = layout;
        let mut new_focus = None;
        let new_zoomed = saved.zoomed.and_then(|z| built.get(&z)).map(|(id, ..)| *id);
        for (old_id, (id, tabs, active)) in built {
            let mut ids = Vec::new();
            for tab in tabs {
                let tab_id = App::id_of(&tab);
                self.tabs.insert(tab_id, tab);
                ids.push(tab_id);
            }
            let active = active.and_then(|a| ids.get(a).copied()).or_else(|| ids.first().copied());
            if old_id == saved.focus {
                new_focus = Some(id);
            }
            self.panes.push(Pane { id, tabs: ids, active });
        }
        // Panes in the order they are laid out.
        let order = self.layout.leaves();
        self.panes.sort_by_key(|p| order.iter().position(|&o| o == p.id));
        self.focus = new_focus.or_else(|| order.first().copied()).unwrap_or(0);
        self.zoomed = new_zoomed;
        // The tab last used of each kind: the focused pane's first, else any.
        let showing: Vec<_> = self.panes.iter().filter_map(|p| p.active).collect();
        let focused = self.panes.iter().find(|p| p.id == self.focus).and_then(|p| p.active);
        for id in focused.into_iter().chain(showing).chain(self.tabs.keys().copied().collect::<Vec<_>>()) {
            match self.tabs.get(&id) {
                Some(Tab::Bible(_)) if self.last_bible.is_none() => self.last_bible = Some(id),
                Some(Tab::Note(_)) if self.last_note.is_none() => self.last_note = Some(id),
                Some(Tab::Dict(_)) if self.last_dict.is_none() => self.last_dict = Some(id),
                _ => {}
            }
        }
        let tasks = scrolls.into_iter().map(|(id, y)| scroll_to(id, AbsoluteOffset { x: 0.0, y }));
        Ok(Task::batch(tasks))
    }

    /// A tab from its description, if what it shows still exists.
    fn build_tab(&mut self, saved: &TabSave) -> Option<Tab> {
        match saved {
            TabSave::Blank => Some(self.new_blank()),
            TabSave::Note { path } => {
                self.sh.notes.contains_key(path).then(|| self.new_note(path.clone()))
            }
            TabSave::Backlinks { passage } => {
                let passage = scripture::RefRange::from_osis(passage)?;
                let id = self.take_id();
                Some(Tab::Backlinks(Box::new(crate::backlinks_tab::BacklinksTab::new(id, crate::backlinks_tab::Target::Passage(passage)))))
            }
            TabSave::NoteBacklinks { path } => {
                self.sh.notes.contains_key(path).then(|| {
                    let id = self.take_id();
                    Tab::Backlinks(Box::new(crate::backlinks_tab::BacklinksTab::new(id, crate::backlinks_tab::Target::Note(path.clone()))))
                })
            }
            TabSave::Search { module, query, case, stem, best } => {
                let module = ModuleId(module.clone());
                self.sh.backend.modules().iter().any(|m| m.id == module).then(|| {
                    let id = self.take_id();
                    let mut tab = crate::search_tab::SearchTab::new(id, module);
                    tab.options = crate::bible_search::Options { case: *case, stem: *stem, by_relevance: *best };
                    tab.draft = query.clone();
                    // Searched again if that costs nothing (the index is made).
                    if !query.trim().is_empty() && self.sh.bible_index_ready(&tab.module) {
                        let _ = tab.run(&self.sh);
                    }
                    Tab::Search(Box::new(tab))
                })
            }
            TabSave::Commentary { module, book, chapter } => {
                let module = ModuleId(module.clone());
                let book = Book::from_osis(book)?;
                (module.0 == crate::commentary_tab::MINE || self.sh.backend.commentaries().iter().any(|c| c.id == module)).then(|| {
                    let id = self.take_id();
                    Tab::Commentary(Box::new(crate::commentary_tab::CommentaryTab::new(id, &self.sh, module, book, (*chapter).max(1), None)))
                })
            }
            TabSave::Plan { plan, day } => {
                let id = self.take_id();
                let mut tab = crate::plan_tab::PlanTab::new(id);
                let entries = crate::plans_window::entries_of(&self.sh);
                // What was shown is shown again if it is still there.
                if let Some(key) = plan.as_ref().filter(|k| entries.iter().any(|e| &e.key() == *k)) {
                    tab.show(&self.sh, &entries, Some(key.clone()), *day);
                }
                Some(Tab::Plan(Box::new(tab)))
            }
            TabSave::Dict { module, key } => {
                let module = ModuleId(module.clone());
                self.sh.backend.dictionaries().iter().any(|d| d.id == module).then(|| {
                    let id = self.take_id();
                    Tab::Dict(Box::new(DictTab::new(id, &self.sh, module, key)))
                })
            }
            TabSave::Bible { module, book, chapter, scroll, strongs, marks, footnotes, refs, in_margin } => {
                let module = ModuleId(module.clone());
                if !self.sh.backend.modules().iter().any(|m| m.id == module) {
                    return None;
                }
                let available = self.sh.backend.available_chapters(&module);
                let wanted = Book::from_osis(book).map(|b| (b, *chapter)).filter(|at| available.contains(at));
                let (book, chapter, scroll) = match wanted {
                    Some((b, c)) => (b, c, *scroll),
                    None => {
                        let (b, c) = self.sh.first_chapter(&module);
                        (b, c, 0.0)
                    }
                };
                let id = self.take_id();
                let mut tab = BibleTab::restored(id, &self.sh, module, book, chapter, scroll);
                (tab.show_strongs, tab.show_marks, tab.show_footnotes, tab.show_refs) = (*strongs, *marks, *footnotes, *refs);
                tab.in_margin = *in_margin;
                Some(Tab::Bible(Box::new(tab)))
            }
        }
    }
}

fn save_tab(tab: &Tab) -> TabSave {
    match tab {
        Tab::Blank(_) => TabSave::Blank,
        Tab::Bible(t) => TabSave::Bible {
            module: t.module.0.clone(),
            book: t.book.osis().to_string(),
            chapter: t.chapter_no,
            scroll: t.scroll_y,
            strongs: t.show_strongs,
            marks: t.show_marks,
            footnotes: t.show_footnotes,
            refs: t.show_refs,
            in_margin: t.in_margin,
        },
        Tab::Note(t) => TabSave::Note { path: t.path.clone() },
        Tab::Dict(t) => TabSave::Dict { module: t.module.0.clone(), key: t.key.clone() },
        Tab::Search(t) => TabSave::Search { module: t.module.0.clone(), query: t.draft.clone(), case: t.options.case, stem: t.options.stem, best: t.options.by_relevance },
        Tab::Commentary(t) => TabSave::Commentary { module: t.module.0.clone(), book: t.book.osis().to_string(), chapter: t.chapter },
        Tab::Plan(t) => TabSave::Plan { plan: t.plan.clone(), day: t.day },
        Tab::Backlinks(t) => match &t.target {
            crate::backlinks_tab::Target::Passage(range) => TabSave::Backlinks { passage: range.osis() },
            crate::backlinks_tab::Target::Note(path) => TabSave::NoteBacklinks { path: path.clone() },
        },
    }
}

fn save_node(node: &Node) -> Layout {
    match node {
        Node::Leaf(id) => Layout::Pane { id: *id },
        Node::Split { axis, fraction, a, b, .. } => Layout::Split {
            axis: match axis {
                Axis::Row => AxisSave::Row,
                Axis::Column => AxisSave::Column,
            },
            fraction: *fraction,
            a: Box::new(save_node(a)),
            b: Box::new(save_node(b)),
        },
    }
}

/// The layout over the panes that could be built; a split with one side gone is the other side.
fn build_node(layout: &Layout, built: &BTreeMap<u32, (u32, Vec<Tab>, Option<usize>)>, next_split: &mut u32) -> Option<Node> {
    match layout {
        Layout::Pane { id } => built.get(id).map(|(new, ..)| Node::Leaf(*new)),
        Layout::Split { axis, fraction, a, b } => {
            let (a, b) = (build_node(a, built, next_split), build_node(b, built, next_split));
            match (a, b) {
                (Some(a), Some(b)) => {
                    let id = *next_split;
                    *next_split += 1;
                    let axis = match axis {
                        AxisSave::Row => Axis::Row,
                        AxisSave::Column => Axis::Column,
                    };
                    let fraction = if fraction.is_finite() { fraction.clamp(crate::panes::MIN_FRACTION, crate::panes::MAX_FRACTION) } else { 0.5 };
                    Some(Node::Split { id, axis, fraction, a: Box::new(a), b: Box::new(b) })
                }
                (one, other) => one.or(other),
            }
        }
    }
}
