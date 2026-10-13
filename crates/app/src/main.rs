//! A study workspace: panes holding tabs. A tab is a Bible reader or a note; each kind is a
//! self-contained module (`bible_tab`, `note_tab`) that keeps its own state and works on the
//! `Shared` notes, marks and backend. This file lays the tabs out in panes and carries out
//! what one tab asks of another (a note's link shows a verse in a Bible tab).

mod bible_tab;
mod dict_tab;
mod drag;
mod fonts;
mod history;
mod icons;
mod live;
mod live_editor;
mod native;
mod note_tab;
mod opening;
mod palette;
mod panes;
mod persist;
mod reader;
mod reader_cards;
mod reader_text;
mod shared;
mod split;
mod state;
mod workspace;

use bible_tab::{BibleMsg, BibleTab};
use dict_tab::{DictMsg, DictTab};
use drag::{Drag, Over, Payload, Pressed, Zone};
use iced::widget::operation::{RelativeOffset, snap_to};
use iced::widget::{button, column, container, pick_list, row, rule, space, stack, text, text_input, tooltip};
use iced::{Element, Length, Task, Theme};
use note_tab::{NoteMsg, NoteTab};
use opening::{Dest, Spec};
use panes::{Node, PaneId, SplitId};
use persist::Persist;
use shared::{Event, Shared, TabId};
use state::{NamedLayout, Settings, Store, Workspace};
use study_core::markdown::resolve_note;
use std::collections::BTreeMap;
use std::fmt;
use study_core::{ModuleId, StudyBackend};

fn main() -> iced::Result {
    // A daemon, since there is more than one window: the workspace and the settings. It does not
    // end by itself when the last window closes; closing the workspace's window does it.
    iced::daemon(App::live, App::update, App::window_view)
        .title(App::window_title)
        .theme(App::window_theme)
        .default_font(fonts::UI)
        .subscription(App::subscription)
        .run()
}

/// What a tab holds. Each kind is one self-contained type of tab.
enum Tab {
    /// A new tab: it offers a choice of what to open, then becomes that.
    Blank(TabId),
    Bible(Box<BibleTab>),
    Note(Box<NoteTab>),
    Dict(Box<DictTab>),
}

/// A column of tabs, one of them showing.
struct Pane {
    id: PaneId,
    tabs: Vec<TabId>,
    active: Option<TabId>,
}

/// Pop-ups float over panes that are themselves panel-colored, so they get the page's own
/// background, an edge and a shadow to stand apart.
fn popup_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.base.color.into()),
        border: iced::Border { color: p.background.strong.color, width: 1.0, radius: 6.0.into() },
        shadow: iced::Shadow { color: iced::Color { a: 0.35, ..iced::Color::BLACK }, offset: iced::Vector::new(0.0, 4.0), blur_radius: 14.0 },
        text_color: Some(p.background.base.text),
        ..container::Style::default()
    }
}

/// What a blank tab can become.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Choice {
    Bible(ModuleId, String),
    Note(String),
    Dict(ModuleId, String),
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Choice::Bible(_, abbreviation) => write!(f, "Bible · {abbreviation}"),
            Choice::Note(path) => write!(f, "Note · {}", path.trim_end_matches(".md")),
            Choice::Dict(_, abbreviation) => write!(f, "Dictionary · {abbreviation}"),
        }
    }
}

struct App {
    sh: Shared,
    tabs: BTreeMap<TabId, Tab>,
    panes: Vec<Pane>,
    /// How the panes are divided up.
    layout: Node,
    /// The pane that last had activity: where new tabs go.
    focus: PaneId,
    /// The pane shown alone, filling the window, if any.
    zoomed: Option<PaneId>,
    /// The Bible and note tab last used, which others work with: a note's links show their
    /// verses in the Bible tab, and inserting a reference takes the Bible tab's selection.
    last_bible: Option<TabId>,
    last_note: Option<TabId>,
    last_dict: Option<TabId>,
    next_tab: TabId,
    next_pane: PaneId,
    next_split: SplitId,
    /// The keys held down, for choosing where a link opens.
    modifiers: iced::keyboard::Modifiers,
    /// The menu of where to open a link, while it is open.
    link_menu: Option<LinkMenu>,
    /// Something being carried across the window, and a tab pressed that may become that.
    drag: Option<Drag>,
    pressed: Option<Pressed>,
    /// The window with the workspace, and the settings window while it is open.
    main: iced::window::Id,
    settings_window: Option<iced::window::Id>,
    /// The layouts panel, while it is open.
    panel: Option<Panel>,
    /// The tag typed for a new pen, and the color it will have (an index into `state::PEN_COLORS`).
    pen_draft: String,
    pen_color: usize,
    /// What is typed in the box that names a layout to save, and whether that box is showing.
    layout_draft: String,
    naming: bool,
    /// The font families to choose from, listed when the settings panel opens.
    families: Vec<String>,
    /// The workspace as last written as the session, and one changed since that is waiting for
    /// the next tick to be written.
    saved: Option<Workspace>,
    unsaved: Option<Workspace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Layouts,
}

/// A font to choose; `None` is the interface font.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FontChoice(Option<String>);

impl fmt::Display for FontChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_deref().unwrap_or("Interface font"))
    }
}

/// The layout that is always there: it cannot be deleted or replaced.
const DEFAULT_LAYOUT: &str = "Default";

fn layout_name_id() -> iced::widget::Id {
    iced::widget::Id::new("layout-name")
}

/// How long a change to the workspace waits before it is written, so scrolling is not a write
/// for every pixel.
const SESSION_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// A link that was right clicked, and where.
struct LinkMenu {
    at: iced::Point,
    spec: Spec,
    from: TabId,
}

#[derive(Debug, Clone)]
enum Message {
    Bible(TabId, BibleMsg),
    Note(TabId, NoteMsg),
    Dict(TabId, DictMsg),
    CloseTab(TabId),
    /// A new blank tab at the end of a pane.
    NewTab(PaneId),
    /// A blank tab chose what to be.
    Open(TabId, Choice),
    /// Divide a pane in two, side by side or top and bottom; the new half starts with a blank tab.
    SplitPane(PaneId, panes::Axis),
    ResizeSplit(SplitId, f32),
    ToggleZoom(PaneId),
    SetTheme(Theme),
    SetBibleSize(f32),
    SetNotesSize(f32),
    SetFont(FontChoice),
    ResetSettings,
    /// The pens (tag shortcuts): the one being named, and changes to them.
    PenDraft(String),
    AddPen,
    RemovePen(usize),
    CyclePenColor(usize),
    CyclePenStyle(usize),
    CycleDraftColor,
    TogglePanel(Panel),
    /// Show the settings window (or bring it forward).
    OpenSettings,
    /// Ask for a name to save the workspace as a layout under.
    NameLayout,
    CancelNaming,
    LayoutDraft(String),
    SaveLayout,
    LoadLayout(String),
    DeleteLayout(String),
    /// Write the workspace if it changed.
    SaveSession,
    /// The window is closing.
    CloseRequested(iced::window::Id),
    /// A press nothing else took (the window's background, plain text): it closes any open
    /// note selector, which is a pop-up.
    PressedElsewhere,
    /// The clock, while a toast is showing.
    Tick(std::time::Instant),
    /// The keys held down changed.
    Modifiers(iced::keyboard::Modifiers),
    /// Back (or forward) in the history of the tab being used.
    Navigate { back: bool },
    /// A choice from the link menu.
    MenuOpen(Dest),
    MenuCopy,
    /// A tab was pressed (it is shown, and may be about to be dragged).
    TabPressed(TabId),
    /// The pointer moved, while a tab is pressed or something is being dragged.
    PointerMoved(iced::Point),
    /// The button came up.
    PointerUp,
    DragCancel,
    /// The pointer entered or left a place something can be dropped.
    DragOver(Over),
    DragOut(Over),
    /// The pointer is moving over the empty part of a pane's tab bar.
    DragBar(PaneId),
}

impl App {
    /// The prototype's mock data, kept in memory only: what the tests run against.
    #[cfg(test)]
    fn new() -> Self {
        let backend = study_core::mock::MockBackend;
        let mut notes: BTreeMap<String, String> = backend
            .note_paths()
            .into_iter()
            .filter_map(|p| backend.note_text(&p).map(|t| (p, t)))
            .collect();
        // Marks are files like any other note; the writer can open and edit them.
        notes.extend(backend.mark_files());
        notes.extend(study_core::templates::builtin());
        App::build(Box::new(backend), notes, None)
    }

    /// Real SWORD modules and the vault on disk, seeded on first run.
    fn live() -> (Self, Task<Message>) {
        let boot = live::boot();
        let mut app = App::build(boot.backend, boot.notes, boot.persist);
        app.sh.status = boot.status;
        let (main, open) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            // Closing saves the session first (see `Message::CloseRequested`).
            exit_on_close_request: false,
            ..iced::window::Settings::default()
        });
        app.main = main;
        let mut task = open.then(native::install_menu).discard();
        if let Some(home) = boot.home {
            let store = Store::new(home.join("state"));
            let session = store.session();
            app.sh.attach(store);
            // Pick up where the last run left off.
            if let Some(Ok(restored)) = session.map(|s| app.apply_workspace(&s)) {
                task = Task::batch([task, restored]);
            }
            app.saved = Some(app.workspace());
        }
        (app, task)
    }

    /// The starting layout: a Bible tab on the left, a note on the right.
    fn build(backend: Box<dyn StudyBackend>, notes: BTreeMap<String, String>, persist: Option<Persist>) -> Self {
        let sh = Shared::new(backend, notes, persist);
        let mut app = App {
            sh,
            tabs: BTreeMap::new(),
            panes: Vec::new(),
            layout: Node::Leaf(0),
            focus: 0,
            zoomed: None,
            last_bible: None,
            last_note: None,
            last_dict: None,
            next_tab: 1,
            next_pane: 1,
            next_split: 1,
            modifiers: iced::keyboard::Modifiers::empty(),
            link_menu: None,
            drag: None,
            pressed: None,
            main: iced::window::Id::unique(),
            settings_window: None,
            panel: None,
            pen_draft: String::new(),
            pen_color: 0,
            layout_draft: String::new(),
            naming: false,
            families: Vec::new(),
            saved: None,
            unsaved: None,
        };
        let (left, right) = (app.new_pane(), app.new_pane());
        app.layout = Node::Leaf(left);
        app.layout.split(left, right, app.next_split, panes::Axis::Row);
        app.next_split += 1;
        let module = app.first_module();
        let bible = app.new_bible(module, None);
        app.add_tab(0, bible);
        let note = app.new_note(app.sh.first_ordinary_note());
        app.add_tab(1, note);
        app.focus = left;
        app
    }

    // ---- tabs and panes ----------------------------------------------------------------

    fn first_module(&self) -> ModuleId {
        self.sh.backend.modules().first().map(|m| m.id.clone()).unwrap_or(ModuleId(String::new()))
    }

    fn new_pane(&mut self) -> PaneId {
        let id = self.next_pane;
        self.next_pane += 1;
        self.panes.push(Pane { id, tabs: Vec::new(), active: None });
        id
    }

    fn new_bible(&mut self, module: ModuleId, near: Option<TabId>) -> Tab {
        let id = self.take_id();
        let near = near.and_then(|n| self.bible_tab(n));
        Tab::Bible(Box::new(BibleTab::started(id, &self.sh, module, near)))
    }

    fn new_note(&mut self, path: String) -> Tab {
        let id = self.take_id();
        Tab::Note(Box::new(NoteTab::new(id, &self.sh, path)))
    }

    fn new_blank(&mut self) -> Tab {
        Tab::Blank(self.take_id())
    }

    fn take_id(&mut self) -> TabId {
        let id = self.next_tab;
        self.next_tab += 1;
        id
    }

    fn id_of(tab: &Tab) -> TabId {
        match tab {
            Tab::Blank(id) => *id,
            Tab::Bible(t) => t.id,
            Tab::Note(t) => t.id,
            Tab::Dict(t) => t.id,
        }
    }

    fn pane_index(&self, pane: PaneId) -> Option<usize> {
        self.panes.iter().position(|p| p.id == pane)
    }

    /// Put `tab` at the end of the pane at index `pane`, showing it.
    fn add_tab(&mut self, pane: usize, tab: Tab) -> TabId {
        let id = App::id_of(&tab);
        match &tab {
            Tab::Bible(_) => self.last_bible = Some(id),
            Tab::Note(_) => self.last_note = Some(id),
            Tab::Dict(_) => self.last_dict = Some(id),
            Tab::Blank(_) => {}
        }
        self.tabs.insert(id, tab);
        let pane = pane.min(self.panes.len() - 1);
        self.panes[pane].tabs.push(id);
        self.panes[pane].active = Some(id);
        self.focus = self.panes[pane].id;
        id
    }

    /// Index of the pane holding tab `id`.
    fn pane_of(&self, id: TabId) -> Option<usize> {
        self.panes.iter().position(|p| p.tabs.contains(&id))
    }

    /// Show tab `id` and make its pane the focus.
    fn activate(&mut self, id: TabId) {
        let Some(pane) = self.pane_of(id) else { return };
        self.panes[pane].active = Some(id);
        self.focus = self.panes[pane].id;
        // Asking for a tab that is hidden behind a zoomed pane shows it.
        if self.zoomed.is_some_and(|z| z != self.focus) {
            self.zoomed = None;
        }
        self.remember(id);
    }

    fn remember(&mut self, id: TabId) {
        match self.tabs.get(&id) {
            Some(Tab::Bible(_)) => self.last_bible = Some(id),
            Some(Tab::Note(_)) => self.last_note = Some(id),
            Some(Tab::Dict(_)) => self.last_dict = Some(id),
            _ => {}
        }
    }

    fn close_tab(&mut self, id: TabId) {
        if self.pane_of(id).is_none() {
            return;
        }
        if let Some(Tab::Bible(tab)) = self.tabs.get_mut(&id) {
            tab.stop_edit(&mut self.sh);
        }
        self.detach_tab(id);
        self.tabs.remove(&id);
    }

    /// Take a tab out of its pane (not out of the app). A pane left with no tabs goes away.
    fn detach_tab(&mut self, id: TabId) {
        let Some(pane) = self.pane_of(id) else { return };
        let p = &mut self.panes[pane];
        let at = p.tabs.iter().position(|&t| t == id).unwrap_or(0);
        p.tabs.remove(at);
        if p.active == Some(id) {
            p.active = p.tabs.get(at).or_else(|| p.tabs.last()).copied();
        }
        if self.panes[pane].tabs.is_empty() {
            self.pane_emptied(pane);
        }
    }

    /// A pane with no tabs goes away and its neighbour takes the space; the last pane stays,
    /// with a blank tab.
    fn pane_emptied(&mut self, pane: usize) {
        let id = self.panes[pane].id;
        if self.panes.len() > 1 && self.layout.remove(id) {
            self.panes.remove(pane);
            if self.zoomed == Some(id) {
                self.zoomed = None;
            }
            if self.focus == id {
                self.focus = self.panes.first().map_or(0, |p| p.id);
            }
        } else {
            let blank = self.new_blank();
            self.add_tab(pane, blank);
        }
    }

    /// Whether `id` is the tab showing in its pane, and that pane is on screen.
    fn is_showing(&self, id: TabId) -> bool {
        self.pane_of(id).is_some_and(|p| {
            self.panes[p].active == Some(id) && self.zoomed.is_none_or(|z| z == self.panes[p].id)
        })
    }

    /// The Bible tab others work with: the one last used if it is showing, else any showing
    /// one, else the one last used, else any.
    fn reference_bible_id(&self) -> Option<TabId> {
        let bibles = || self.tabs.iter().filter(|(_, t)| matches!(t, Tab::Bible(_))).map(|(id, _)| *id);
        let last = self.last_bible.filter(|id| matches!(self.tabs.get(id), Some(Tab::Bible(_))));
        last.filter(|&id| self.is_showing(id))
            .or_else(|| bibles().find(|&id| self.is_showing(id)))
            .or(last)
            .or_else(|| bibles().next())
    }

    fn bible_tab(&self, id: TabId) -> Option<&BibleTab> {
        match self.tabs.get(&id) {
            Some(Tab::Bible(t)) => Some(t),
            _ => None,
        }
    }

    /// Show a note in a note tab: the one already holding it, else the one last used (which
    /// follows the link), else a new one in the last pane.
    fn show_note(&mut self, path: String) {
        let holding = self.tabs.iter().find_map(|(id, t)| match t {
            Tab::Note(n) if n.path == path => Some(*id),
            _ => None,
        });
        if let Some(id) = holding {
            self.activate(id);
            return;
        }
        let last = self.last_note.filter(|id| matches!(self.tabs.get(id), Some(Tab::Note(_))));
        match last {
            Some(id) => {
                if let Some(Tab::Note(tab)) = self.tabs.get_mut(&id) {
                    tab.open(&mut self.sh, path);
                }
                self.activate(id);
            }
            None => {
                let tab = self.new_note(path);
                let pane = self.panes.len() - 1;
                let id = self.add_tab(pane, tab);
                self.activate(id);
            }
        }
    }

    /// A Bible tab to show something in, making one in the first pane if there is none.
    fn bible_for_showing(&mut self) -> TabId {
        if let Some(id) = self.reference_bible_id() {
            self.activate(id);
            return id;
        }
        let module = self.first_module();
        let tab = self.new_bible(module, None);
        self.add_tab(0, tab)
    }

    /// What a blank tab can turn into: each Bible module, each note.
    fn choices(&self) -> Vec<Choice> {
        let modules = self.sh.backend.modules().into_iter().map(|m| Choice::Bible(m.id, m.abbreviation));
        let notes = self.sh.notes.keys().filter(|p| !p.starts_with("margins/") && !study_core::templates::is_template(p)).cloned().map(Choice::Note);
        let dictionaries = self.sh.backend.dictionaries().into_iter().map(|d| Choice::Dict(d.id, d.abbreviation));
        modules.chain(dictionaries).chain(notes).collect()
    }

    /// Turn blank tab `id` into what was chosen, keeping its place in its pane.
    fn open_choice(&mut self, id: TabId, choice: Choice) {
        if !matches!(self.tabs.get(&id), Some(Tab::Blank(_))) {
            return;
        }
        let tab = match choice {
            Choice::Bible(module, _) => {
                let near = self.reference_bible_id();
                let tab = BibleTab::started(id, &self.sh, module, near.and_then(|n| self.bible_tab(n)));
                Tab::Bible(Box::new(tab))
            }
            Choice::Note(path) => Tab::Note(Box::new(NoteTab::new(id, &self.sh, path))),
            Choice::Dict(module, _) => Tab::Dict(Box::new(DictTab::new(id, &self.sh, module, ""))),
        };
        self.tabs.insert(id, tab);
        self.activate(id);
    }

    // ---- update --------------------------------------------------------------------

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.settle();
        self.sh.save();
        self.note_session();
        task
    }

    /// Remember that the workspace changed, to be written soon.
    fn note_session(&mut self) {
        if self.sh.store.is_none() {
            return;
        }
        let now = self.workspace();
        if self.saved.as_ref() != Some(&now) {
            self.unsaved = Some(now);
        } else {
            self.unsaved = None;
        }
    }

    /// Write the workspace, if it changed since it was last written.
    fn save_session(&mut self) {
        let Some(now) = self.unsaved.take() else { return };
        let Some(store) = &self.sh.store else { return };
        match store.save_session(&now) {
            Ok(()) => self.saved = Some(now),
            Err(e) => self.sh.status = format!("Could not save the session: {e}"),
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        // A note selector is a pop-up: whatever is done outside it puts it away.
        // (The reader's reports of its own layout are not something done.)
        let ambient = matches!(
            &message,
            Message::Bible(_, BibleMsg::MarginChanged(_) | BibleMsg::AutoScroll(_) | BibleMsg::SelectionChanged(None))
                | Message::Tick(_)
                | Message::SaveSession
                | Message::Modifiers(_)
                | Message::Navigate { .. }
                | Message::PointerMoved(_)
                | Message::PointerUp
                | Message::DragOver(_)
                | Message::DragOut(_)
                | Message::DragBar(_)
        );
        let keeps = match &message {
            Message::Note(id, _) | Message::Bible(id, _) => Some(*id),
            _ => None,
        };
        if !ambient && !matches!(message, Message::MenuOpen(_) | Message::MenuCopy) {
            self.link_menu = None;
        }
        // The layouts panel is a pop-up too: it stays for what is done in it.
        let in_panel = matches!(
            message,
            Message::TogglePanel(_)
                | Message::OpenSettings
                | Message::CancelNaming
                | Message::NameLayout
                | Message::LayoutDraft(_)
                | Message::SaveLayout
                | Message::DeleteLayout(_)
        );
        if !ambient && !in_panel {
            self.panel = None;
        }
        for (id, tab) in self.tabs.iter_mut() {
            if Some(*id) == keeps || ambient {
                continue;
            }
            match tab {
                Tab::Note(t) => {
                    t.picker = None;
                    t.renaming = None;
                    t.menu_open = false;
                    t.choosing = false;
                }
                Tab::Bible(t) => {
                    t.menu_open = false;
                    t.margin_menu_open = false;
                    t.history_open = false;
                }
                Tab::Blank(_) | Tab::Dict(_) => {}
            }
        }
        match message {
            Message::PressedElsewhere => Task::none(),
            Message::Modifiers(m) => {
                self.modifiers = m;
                Task::none()
            }
            Message::Navigate { back } => {
                let Some(id) = self.panes.iter().find(|p| p.id == self.focus).and_then(|p| p.active) else { return Task::none() };
                match self.tabs.get(&id) {
                    Some(Tab::Bible(_)) => self.handle(Message::Bible(id, if back { BibleMsg::Back } else { BibleMsg::Forward })),
                    Some(Tab::Note(_)) => self.handle(Message::Note(id, if back { NoteMsg::Back } else { NoteMsg::Forward })),
                    Some(Tab::Dict(_)) => self.handle(Message::Dict(id, if back { DictMsg::Back } else { DictMsg::Forward })),
                    _ => Task::none(),
                }
            }
            Message::MenuOpen(dest) => match self.link_menu.take() {
                Some(menu) => self.open_spec(menu.from, menu.spec, dest),
                None => Task::none(),
            },
            Message::MenuCopy => match self.link_menu.take() {
                Some(menu) => {
                    self.sh.toast("Copied link");
                    iced::clipboard::write(menu.spec.link_text())
                }
                None => Task::none(),
            },
            Message::Tick(now) => {
                if self.sh.toast.as_ref().is_some_and(|t| now >= t.until) {
                    self.sh.toast = None;
                }
                Task::none()
            }
            Message::Bible(id, msg) => {
                self.touch(id);
                let Some(Tab::Bible(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
                let (task, request) = tab.update(&mut self.sh, msg);
                let task = task.map(move |m| Message::Bible(id, m));
                match request {
                    Some(bible_tab::Request::Open(spec)) => {
                        let dest = Dest::from_modifiers(self.modifiers);
                        return Task::batch([task, self.open_spec(id, spec, dest)]);
                    }
                    Some(bible_tab::Request::Menu(spec, at)) => self.link_menu = Some(LinkMenu { at, spec, from: id }),
                    Some(bible_tab::Request::Drag(spec, at)) => self.start_drag(Payload::Link(spec), at),
                    Some(bible_tab::Request::OpenMark(index)) => {
                        if let Some(mark) = self.sh.marks.get(index) {
                            let (path, markdown) = (mark.path(), mark.to_markdown());
                            self.sh.notes.or_insert_with(path.clone(), || markdown);
                            self.show_note(path);
                        }
                    }
                    Some(bible_tab::Request::OpenNote(path)) => self.show_note(path),
                    None => {}
                }
                task
            }
            Message::Note(id, msg) => {
                self.touch(id);
                let Some(Tab::Note(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
                let (task, request) = tab.update(&mut self.sh, msg);
                let task = task.map(move |m| Message::Note(id, m));
                self.sync_same_note(id);
                match request {
                    Some(note_tab::Request::Open(spec)) => {
                        let dest = Dest::from_modifiers(self.modifiers);
                        Task::batch([task, self.open_spec(id, spec, dest)])
                    }
                    Some(note_tab::Request::Menu(spec, at)) => {
                        self.link_menu = Some(LinkMenu { at, spec, from: id });
                        task
                    }
                    Some(note_tab::Request::Drag(spec, at)) => {
                        self.start_drag(Payload::Link(spec), at);
                        task
                    }
                    None => task,
                }
            }
            Message::Dict(id, msg) => {
                self.touch(id);
                if let Some(Tab::Dict(tab)) = self.tabs.get_mut(&id) {
                    tab.update(&mut self.sh, msg);
                }
                Task::none()
            }
            Message::TabPressed(id) => {
                self.activate(id);
                self.pressed = Some(Pressed { tab: id, origin: None });
                Task::none()
            }
            Message::PointerMoved(at) => {
                if let Some(drag) = &mut self.drag {
                    drag.at = at;
                } else if let Some(pressed) = &mut self.pressed {
                    let origin = *pressed.origin.get_or_insert(at);
                    if drag::far_enough(origin, at) {
                        let tab = pressed.tab;
                        self.pressed = None;
                        self.start_drag(Payload::Tab(tab), at);
                    }
                }
                Task::none()
            }
            Message::PointerUp => {
                self.pressed = None;
                self.finish_drag()
            }
            Message::DragCancel => {
                self.drag = None;
                self.pressed = None;
                Task::none()
            }
            Message::DragOver(over) => {
                if let Some(drag) = &mut self.drag {
                    drag.over = Some(over);
                }
                Task::none()
            }
            Message::DragBar(pane) => {
                // The bar is under the tabs in it: a tab the pointer is over wins.
                if let Some(drag) = self.drag.as_mut().filter(|d| !matches!(d.over, Some(Over::Chip(_)))) {
                    drag.over = Some(Over::Bar(pane));
                }
                Task::none()
            }
            Message::DragOut(over) => {
                if let Some(drag) = self.drag.as_mut().filter(|d| d.over == Some(over)) {
                    drag.over = None;
                }
                Task::none()
            }
            Message::CloseTab(id) => {
                self.close_tab(id);
                Task::none()
            }
            Message::NewTab(pane) => {
                if let Some(index) = self.pane_index(pane) {
                    let blank = self.new_blank();
                    self.add_tab(index, blank);
                }
                Task::none()
            }
            Message::Open(id, choice) => {
                self.open_choice(id, choice);
                Task::none()
            }
            Message::SplitPane(pane, axis) => {
                if let Some(at) = self.pane_index(pane) {
                    let new = self.split_pane(at, axis);
                    if new != at {
                        let blank = self.new_blank();
                        self.add_tab(new, blank);
                    }
                }
                Task::none()
            }
            Message::ResizeSplit(id, fraction) => {
                self.layout.set_fraction(id, fraction);
                Task::none()
            }
            Message::ToggleZoom(pane) => {
                self.zoomed = if self.zoomed == Some(pane) { None } else { Some(pane) };
                self.focus = pane;
                Task::none()
            }
            Message::SetTheme(theme) => self.change_settings(|s| s.theme = theme.to_string()),
            Message::SetBibleSize(size) => self.change_settings(|s| s.bible_size = size.round()),
            Message::SetNotesSize(size) => self.change_settings(|s| s.notes_size = size.round()),
            Message::SetFont(FontChoice(font)) => self.change_settings(|s| s.font = font),
            Message::ResetSettings => self.change_settings(|s| *s = Settings::default()),
            Message::PenDraft(text) => {
                self.pen_draft = text;
                Task::none()
            }
            Message::AddPen => {
                let tag = study_core::frontmatter::normalize_tag(&self.pen_draft);
                if tag.is_empty() {
                    return Task::none();
                }
                let color = state::PEN_COLORS[self.pen_color % state::PEN_COLORS.len()].to_string();
                self.pen_draft.clear();
                self.pen_color = (self.pen_color + 1) % state::PEN_COLORS.len();
                self.change_settings(|s| {
                    // A pen for a tag there already is one for changes that pen's color instead.
                    match s.pens.iter_mut().find(|p| p.tag == tag) {
                        Some(pen) => pen.color = color,
                        None => s.pens.push(state::PenDef { tag, color, style: state::PenStyle::Highlight }),
                    }
                })
            }
            Message::RemovePen(i) => self.change_settings(|s| {
                if i < s.pens.len() {
                    s.pens.remove(i);
                }
            }),
            Message::CyclePenColor(i) => self.change_settings(|s| {
                if let Some(pen) = s.pens.get_mut(i) {
                    let at = state::PEN_COLORS.iter().position(|c| c.eq_ignore_ascii_case(&pen.color)).map_or(0, |n| n + 1);
                    pen.color = state::PEN_COLORS[at % state::PEN_COLORS.len()].to_string();
                }
            }),
            Message::CyclePenStyle(i) => self.change_settings(|s| {
                if let Some(pen) = s.pens.get_mut(i) {
                    pen.style = match pen.style {
                        state::PenStyle::Highlight => state::PenStyle::Underline,
                        state::PenStyle::Underline => state::PenStyle::Highlight,
                    };
                }
            }),
            Message::CycleDraftColor => {
                self.pen_color = (self.pen_color + 1) % state::PEN_COLORS.len();
                Task::none()
            }
            Message::TogglePanel(panel) => {
                self.panel = if self.panel == Some(panel) { None } else { Some(panel) };
                Task::none()
            }
            Message::OpenSettings => {
                if let Some(id) = self.settings_window {
                    return iced::window::gain_focus(id);
                }
                if self.families.is_empty() {
                    self.families = state::installed_families();
                }
                // A fixed-size accessory of the workspace window: it cannot be zoomed, full
                // screened or minimized by itself (see `native::attach` for the rest).
                let (id, open) = iced::window::open(iced::window::Settings {
                    size: iced::Size::new(380.0, 600.0),
                    resizable: false,
                    minimizable: false,
                    exit_on_close_request: false,
                    ..iced::window::Settings::default()
                });
                self.settings_window = Some(id);
                let main = self.main;
                open.then(move |_| native::attach(main, id)).discard()
            }
            Message::NameLayout => {
                self.panel = None;
                self.naming = true;
                self.layout_draft.clear();
                iced::widget::operation::focus(layout_name_id())
            }
            Message::CancelNaming => {
                self.naming = false;
                self.layout_draft.clear();
                Task::none()
            }
            Message::LayoutDraft(text) => {
                self.layout_draft = text;
                Task::none()
            }
            Message::SaveLayout => {
                let name = self.layout_draft.trim().to_string();
                if !name.is_empty() && !name.eq_ignore_ascii_case(DEFAULT_LAYOUT) {
                    let workspace = self.workspace();
                    self.sh.save_layout(NamedLayout { name: name.clone(), workspace });
                    self.layout_draft.clear();
                    self.naming = false;
                    self.sh.toast(format!("Saved layout \u{201c}{name}\u{201d}"));
                }
                Task::none()
            }
            Message::LoadLayout(name) => {
                let workspace = if name == DEFAULT_LAYOUT {
                    self.default_workspace()
                } else {
                    let Some(layout) = self.sh.layouts.iter().find(|l| l.name == name) else { return Task::none() };
                    layout.workspace.clone()
                };
                match self.apply_workspace(&workspace) {
                    Ok(task) => {
                        self.panel = None;
                        task
                    }
                    Err(e) => {
                        self.sh.status = e;
                        Task::none()
                    }
                }
            }
            Message::DeleteLayout(name) => {
                self.sh.delete_layout(&name);
                Task::none()
            }
            Message::SaveSession => {
                self.save_session();
                Task::none()
            }
            Message::CloseRequested(id) if id == self.main => {
                self.note_session();
                self.save_session();
                iced::exit()
            }
            Message::CloseRequested(id) => {
                if self.settings_window == Some(id) {
                    self.settings_window = None;
                }
                iced::window::close(id)
            }
        }
    }

    fn change_settings(&mut self, change: impl FnOnce(&mut Settings)) -> Task<Message> {
        let mut settings = self.sh.settings.clone();
        change(&mut settings);
        self.sh.set_settings(settings);
        Task::none()
    }

    /// A message from tab `id` means that tab is showing and in use.
    fn touch(&mut self, id: TabId) {
        if let Some(pane) = self.pane_of(id) {
            self.panes[pane].active = Some(id);
            self.focus = self.panes[pane].id;
        }
        self.remember(id);
    }

    /// Other note tabs holding the same note as `id` follow what was typed in it.
    fn sync_same_note(&mut self, id: TabId) {
        let Some(Tab::Note(edited)) = self.tabs.get(&id) else { return };
        let (path, text) = (edited.path.clone(), edited.doc.source().to_string());
        for (other, tab) in self.tabs.iter_mut() {
            if let Tab::Note(t) = tab
                && *other != id
                && t.path == path
                && t.doc.source() != text
            {
                t.reload(&self.sh);
            }
        }
    }

    fn show_verse_in(&mut self, id: TabId, range: scripture::RefRange) -> Task<Message> {
        let Some(Tab::Bible(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
        let before = (tab.book, tab.chapter_no);
        tab.show_in_reader(&mut self.sh, range);
        if before == (tab.book, tab.chapter_no) {
            Task::none()
        } else {
            snap_to(tab.reader_id(), RelativeOffset::START)
        }
    }

    // ---- dragging --------------------------------------------------------------------------

    fn start_drag(&mut self, payload: Payload, at: iced::Point) {
        self.link_menu = None;
        self.drag = Some(Drag { payload, at, over: None });
    }

    /// Put down what is being carried where the pointer is.
    fn finish_drag(&mut self) -> Task<Message> {
        let Some(drag) = self.drag.take() else { return Task::none() };
        let Some(over) = drag.over else { return Task::none() };
        match (drag.payload, over) {
            (Payload::Tab(id), Over::Pane(pane, zone)) => self.drop_tab(id, pane, zone),
            (Payload::Tab(id), Over::Bar(pane)) => self.drop_tab(id, pane, Zone::Center),
            (Payload::Tab(id), Over::Chip(before)) => self.drop_tab_before(id, before),
            (Payload::Link(spec), Over::Pane(pane, zone)) => self.drop_link(spec, pane, zone),
            (Payload::Link(spec), Over::Bar(pane)) => self.drop_link(spec, pane, Zone::Center),
            (Payload::Link(spec), Over::Chip(tab)) => {
                if let Some(pane) = self.pane_of(tab).map(|i| self.panes[i].id) {
                    self.drop_link(spec, pane, Zone::Center);
                }
            }
        }
        Task::none()
    }

    /// Move a tab to a pane: onto its middle, it joins that pane; onto an edge, the pane is split
    /// and the tab takes the new half.
    fn drop_tab(&mut self, id: TabId, pane: PaneId, zone: Zone) {
        let Some(from) = self.pane_of(id) else { return };
        let from_id = self.panes[from].id;
        let target = match zone.split() {
            None if from_id == pane => {
                // Already there: it goes to the end.
                self.panes[from].tabs.retain(|&t| t != id);
                self.panes[from].tabs.push(id);
                pane
            }
            None => {
                self.detach_tab(id);
                pane
            }
            Some((axis, before)) => {
                // A pane cannot be split to make room for its own only tab.
                if from_id == pane && self.panes[from].tabs.len() == 1 {
                    return;
                }
                let Some(at) = self.pane_index(pane) else { return };
                let new = self.split_pane_side(at, axis, before);
                if new == at {
                    return;
                }
                let new_id = self.panes[new].id;
                self.detach_tab(id);
                new_id
            }
        };
        if let Some(at) = self.pane_index(target) {
            if !self.panes[at].tabs.contains(&id) {
                self.panes[at].tabs.push(id);
            }
            self.panes[at].active = Some(id);
            self.focus = target;
            self.zoomed = None;
            self.remember(id);
        }
    }

    /// Move a tab to the place just before another, in that tab's pane.
    fn drop_tab_before(&mut self, id: TabId, before: TabId) {
        if id == before {
            return;
        }
        let Some(target) = self.pane_of(before).map(|i| self.panes[i].id) else { return };
        self.detach_tab(id);
        let Some(at) = self.pane_index(target) else { return };
        let place = self.panes[at].tabs.iter().position(|&t| t == before).unwrap_or(self.panes[at].tabs.len());
        self.panes[at].tabs.insert(place, id);
        self.panes[at].active = Some(id);
        self.focus = target;
        self.remember(id);
    }

    /// Open a link in a new tab of a pane (onto its edge: in a new pane split from it).
    fn drop_link(&mut self, spec: Spec, pane: PaneId, zone: Zone) {
        let Some(at) = self.pane_index(pane) else { return };
        let Some(tab) = self.new_tab_for(&spec) else { return };
        let at = match zone.split() {
            None => at,
            Some((axis, before)) => self.split_pane_side(at, axis, before),
        };
        let id = self.add_tab(at, tab);
        self.activate(id);
    }

    /// A tab showing what a link points at, or `None` (with the reason in the status) if it
    /// cannot be shown.
    fn new_tab_for(&mut self, spec: &Spec) -> Option<Tab> {
        match spec {
            Spec::Verse { range, .. } => {
                let near = self.reference_bible_id();
                let module = near.and_then(|n| self.bible_tab(n)).map(|t| t.module.clone()).unwrap_or_else(|| self.first_module());
                let id = self.take_id();
                let mut tab = BibleTab::started(id, &self.sh, module, near.and_then(|n| self.bible_tab(n)));
                tab.show_in_reader(&mut self.sh, *range);
                Some(Tab::Bible(Box::new(tab)))
            }
            Spec::Note(name) => match resolve_note(&self.sh.note_paths(), name).cloned() {
                Some(path) => Some(self.new_note(path)),
                None => {
                    self.sh.status = format!("No note named \u{201C}{name}\u{201D} yet");
                    None
                }
            },
            Spec::Word { key } => match self.sh.backend.strongs_dictionary(key) {
                Some(module) => Some(self.new_dict(module, key)),
                None => {
                    self.sh.status = format!("No installed dictionary has {key}");
                    None
                }
            },
            Spec::Dict { module, key } => match self.resolve_dictionary(module, key) {
                Some(module) => Some(self.new_dict(module, key)),
                None => {
                    self.sh.status = format!("The dictionary \u{201C}{module}\u{201D} is not installed");
                    None
                }
            },
        }
    }

    fn new_dict(&mut self, module: ModuleId, key: &str) -> Tab {
        let id = self.take_id();
        Tab::Dict(Box::new(DictTab::new(id, &self.sh, module, key)))
    }

    // ---- where links open ------------------------------------------------------------------

    /// Divide the pane at index `at` in two along `axis`, returning the new pane's index (empty
    /// of tabs). If it cannot be divided, `at`.
    fn split_pane(&mut self, at: usize, axis: panes::Axis) -> usize {
        self.split_pane_side(at, axis, false)
    }

    /// Like [`App::split_pane`], with the new pane before (left of, above) the old if `before`.
    fn split_pane_side(&mut self, at: usize, axis: panes::Axis, before: bool) -> usize {
        let pane = self.panes[at].id;
        let new = self.new_pane();
        if self.layout.split_side(pane, new, self.next_split, axis, before) {
            self.next_split += 1;
            self.zoomed = None;
            self.pane_index(new).unwrap_or(at)
        } else {
            self.panes.pop();
            at
        }
    }

    /// The pane a link goes to, given the pane it was followed from: the same one, a new one split
    /// off it, or the next one in order (splitting if there is only one).
    fn target_pane(&mut self, from: usize, dest: Dest) -> usize {
        match dest {
            Dest::Here | Dest::NewTab => from,
            Dest::NewPane => self.split_pane(from, panes::Axis::Row),
            Dest::OtherPane => {
                let order = self.layout.leaves();
                let here = order.iter().position(|&p| p == self.panes[from].id).unwrap_or(0);
                if order.len() > 1 {
                    self.zoomed = None;
                    self.pane_index(order[(here + 1) % order.len()]).unwrap_or(from)
                } else {
                    self.split_pane(from, panes::Axis::Row)
                }
            }
        }
    }

    /// Follow a link from tab `from`, as `dest` says.
    fn open_spec(&mut self, from: TabId, spec: Spec, dest: Dest) -> Task<Message> {
        match spec {
            Spec::Verse { range, .. } => self.open_verse(from, range, dest),
            Spec::Note(name) => {
                self.open_note_link(from, &name, dest);
                Task::none()
            }
            Spec::Word { key } => {
                match self.sh.backend.strongs_dictionary(&key) {
                    Some(module) => self.open_dictionary(from, module, key, dest),
                    None => self.sh.status = format!("No installed dictionary has {key}"),
                }
                Task::none()
            }
            Spec::Dict { module, key } => {
                match self.resolve_dictionary(&module, &key) {
                    Some(module) => self.open_dictionary(from, module, key, dest),
                    None => self.sh.status = format!("The dictionary \u{201C}{module}\u{201D} is not installed"),
                }
                Task::none()
            }
        }
    }

    /// The dictionary a link names: by its name, or what it is called in the list; failing that,
    /// a Strong's number goes wherever one is defined.
    fn resolve_dictionary(&self, module: &str, key: &str) -> Option<ModuleId> {
        let wanted = module.to_lowercase();
        self.sh
            .backend
            .dictionaries()
            .into_iter()
            .find(|d| d.id.0.to_lowercase() == wanted || d.abbreviation.to_lowercase() == wanted)
            .map(|d| d.id)
            .or_else(|| self.sh.backend.strongs_dictionary(key))
    }

    /// Show an entry in a dictionary tab. Followed plainly, it goes to the dictionary tab already
    /// open (or the one it came from), and failing that to the other pane, so that what was being
    /// read stays in view.
    fn open_dictionary(&mut self, from: TabId, module: ModuleId, key: String, dest: Dest) {
        let existing = |app: &App| app.last_dict.filter(|id| matches!(app.tabs.get(id), Some(Tab::Dict(_))));
        let target = if dest == Dest::Here {
            let source = matches!(self.tabs.get(&from), Some(Tab::Dict(_))).then_some(from);
            source.or_else(|| existing(self))
        } else {
            None
        };
        let target = match target {
            Some(id) => id,
            None => {
                let from_pane = self.pane_of(from).unwrap_or(0);
                let pane = self.target_pane(from_pane, if dest == Dest::Here { Dest::OtherPane } else { dest });
                // The other pane's own dictionary tab, if that is what it is showing.
                let reusable = (matches!(dest, Dest::Here | Dest::OtherPane))
                    .then(|| self.panes[pane].active)
                    .flatten()
                    .filter(|id| matches!(self.tabs.get(id), Some(Tab::Dict(_))));
                match reusable {
                    Some(id) => id,
                    None => {
                        let tab = Tab::Dict(Box::new(DictTab::new(self.next_tab, &self.sh, module.clone(), "")));
                        self.next_tab += 1;
                        self.add_tab(pane, tab)
                    }
                }
            }
        };
        if let Some(Tab::Dict(tab)) = self.tabs.get_mut(&target) {
            tab.open(&self.sh, module, &key);
        }
        self.activate(target);
    }

    fn open_verse(&mut self, from: TabId, range: scripture::RefRange, dest: Dest) -> Task<Message> {
        // The Bible tab the link belongs to: the one it was followed in, or the one a note works with.
        let natural = if matches!(self.tabs.get(&from), Some(Tab::Bible(_))) { from } else { self.bible_for_showing() };
        if dest == Dest::Here {
            self.activate(natural);
            return self.show_verse_in(natural, range);
        }
        let from_pane = self.pane_of(natural).unwrap_or(0);
        let pane = self.target_pane(from_pane, dest);
        // The other pane's own Bible tab is used if it is showing one.
        let reusable = (dest == Dest::OtherPane)
            .then(|| self.panes[pane].active)
            .flatten()
            .filter(|id| matches!(self.tabs.get(id), Some(Tab::Bible(_))));
        let target = match reusable {
            Some(id) => {
                self.activate(id);
                id
            }
            None => {
                let module = self.bible_tab(natural).map(|t| t.module.clone()).unwrap_or_else(|| self.first_module());
                let tab = self.new_bible(module, Some(natural));
                self.add_tab(pane, tab)
            }
        };
        self.show_verse_in(target, range)
    }

    fn open_note_link(&mut self, from: TabId, name: &str, dest: Dest) {
        let Some(path) = resolve_note(&self.sh.note_paths(), name).cloned() else {
            self.sh.status = format!("No note named \u{201C}{name}\u{201D} yet");
            return;
        };
        let from_note = matches!(self.tabs.get(&from), Some(Tab::Note(_)));
        if dest == Dest::Here {
            if from_note {
                if let Some(Tab::Note(tab)) = self.tabs.get_mut(&from) {
                    tab.open(&mut self.sh, path);
                }
                self.activate(from);
            } else {
                self.show_note(path);
            }
            return;
        }
        let from_pane = self.pane_of(from).unwrap_or(self.panes.len() - 1);
        let pane = self.target_pane(from_pane, dest);
        let reusable = (dest == Dest::OtherPane)
            .then(|| self.panes[pane].active)
            .flatten()
            .filter(|id| matches!(self.tabs.get(id), Some(Tab::Note(_))));
        match reusable {
            Some(id) => {
                if let Some(Tab::Note(tab)) = self.tabs.get_mut(&id) {
                    tab.open(&mut self.sh, path);
                }
                self.activate(id);
            }
            None => {
                let tab = self.new_note(path);
                self.add_tab(pane, tab);
            }
        }
    }

    /// Pass what one tab did on to the others that show it.
    fn settle(&mut self) {
        for event in std::mem::take(&mut self.sh.events) {
            match event {
                Event::MarkRemoved { by } => {
                    for tab in self.tabs.values_mut() {
                        if let Tab::Bible(t) = tab
                            && Some(t.id) != by
                        {
                            t.marks_changed();
                        }
                    }
                }
                Event::NoteRewritten { old, new } => {
                    for tab in self.tabs.values_mut() {
                        if let Tab::Note(t) = tab
                            && (t.path == old || t.path == new)
                        {
                            t.path = new.clone();
                            t.reload(&self.sh);
                        }
                    }
                }
                Event::NoteRenamed { old, new } => {
                    for tab in self.tabs.values_mut() {
                        if let Tab::Note(t) = tab
                            && t.path == old
                        {
                            t.rename_in_history(&old, &new);
                            t.path = new.clone();
                        }
                    }
                }
                Event::NoteRemoved { old, next } => {
                    for tab in self.tabs.values_mut() {
                        if let Tab::Note(t) = tab
                            && t.path == old
                        {
                            t.open(&mut self.sh, next.clone());
                        }
                        if let Tab::Note(t) = tab {
                            t.history.retain_others(|p| *p != old);
                        }
                    }
                }
            }
        }
    }

    fn theme(&self) -> Theme {
        self.sh.theme()
    }

    fn window_theme(&self, _window: iced::window::Id) -> Theme {
        self.theme()
    }

    fn window_title(&self, window: iced::window::Id) -> String {
        if Some(window) == self.settings_window { "Settings".into() } else { "Bible Study (prototype)".into() }
    }

    fn window_view(&self, window: iced::window::Id) -> Element<'_, Message> {
        if Some(window) == self.settings_window {
            iced::widget::scrollable(container(self.settings_panel()).padding(18).width(Length::Fill)).height(Length::Fill).into()
        } else {
            self.view()
        }
    }

    /// The keys held (they decide where a link opens), and a clock only while there is a toast to
    /// take down.
    fn subscription(&self) -> iced::Subscription<Message> {
        let keys = iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => Some(Message::Modifiers(m)),
            // ⌘[ and ⌘] (Ctrl elsewhere), and the mouse's own back and forward buttons.
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) if modifiers.command() => {
                match key.as_ref() {
                    iced::keyboard::Key::Character("[") => Some(Message::Navigate { back: true }),
                    iced::keyboard::Key::Character("]") => Some(Message::Navigate { back: false }),
                    // The platform's shortcut for preferences (on a Mac the menu bar item takes it).
                    iced::keyboard::Key::Character(",") => Some(Message::OpenSettings),
                    _ => None,
                }
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back)) => Some(Message::Navigate { back: true }),
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Forward)) => Some(Message::Navigate { back: false }),
            _ => None,
        });
        let mut all = vec![
            keys,
            iced::window::close_requests().map(Message::CloseRequested),
            native::settings_chosen().map(|()| Message::OpenSettings),
        ];
        if self.naming {
            all.push(iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::CancelNaming),
                _ => None,
            }));
        }
        if self.unsaved.is_some() {
            all.push(iced::time::every(SESSION_DELAY).map(|_| Message::SaveSession));
        }
        if self.sh.toast.is_some() {
            all.push(iced::time::every(std::time::Duration::from_millis(200)).map(Message::Tick));
        }
        // While a tab is pressed or something carried, follow the pointer wherever it goes.
        if self.drag.is_some() || self.pressed.is_some() {
            all.push(iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => Some(Message::PointerMoved(position)),
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => Some(Message::PointerUp),
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::DragCancel),
                _ => None,
            }));
        }
        iced::Subscription::batch(all)
    }

    // ---- view ----------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match self.zoomed.and_then(|z| self.pane_index(z)) {
            Some(index) => self.pane_view(index),
            None => self.node_view(&self.layout),
        };
        // The window's press catch-all sits outside everything, so it only sees presses the
        // widgets inside left alone.
        let window = iced::widget::mouse_area(column![self.top_bar(), rule::horizontal(1), body])
            .on_press(Message::PressedElsewhere);
        // The toast layer is always there (empty when there is none) so that the windows' widget
        // state is not rebuilt when one comes and goes.
        let toast: Element<'_, Message> = match &self.sh.toast {
            Some(toast) => container(
                container(text(toast.text.clone()).size(14)).padding([8, 16]).style(container::rounded_box),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding(28)
            .into(),
            None => space::horizontal().width(0).height(0).into(),
        };
        let mut layers = vec![window.into(), toast];
        if let Some(drag) = &self.drag {
            layers.push(self.drag_label(drag));
        }
        if let Some(panel) = self.panel {
            layers.push(self.panel_view(panel));
        }
        if self.naming {
            layers.push(self.name_layout_view());
        }
        if let Some(menu) = &self.link_menu {
            layers.push(self.link_menu_view(menu));
        }
        stack(layers).into()
    }

    fn node_view<'a>(&'a self, node: &'a Node) -> Element<'a, Message> {
        match node {
            Node::Leaf(pane) => match self.pane_index(*pane) {
                Some(index) => self.pane_view(index),
                None => space::horizontal().into(),
            },
            Node::Split { id, axis, fraction, a, b } => {
                let id = *id;
                split::Split::new(*axis, *fraction, self.node_view(a), self.node_view(b), move |f| {
                    Message::ResizeSplit(id, f)
                })
                .into()
            }
        }
    }

    /// The menu of where to open a link, at the pointer.
    fn link_menu_view(&self, menu: &LinkMenu) -> Element<'_, Message> {
        let mut items = column![].spacing(1).width(190);
        for dest in Dest::ALL {
            items = items.push(
                button(text(dest.label()).size(13)).width(Length::Fill).padding([4, 10]).style(button::text).on_press(Message::MenuOpen(dest)),
            );
        }
        items = items.push(rule::horizontal(1));
        items = items.push(
            button(text("Copy link").size(13)).width(Length::Fill).padding([4, 10]).style(button::text).on_press(Message::MenuCopy),
        );
        container(iced::widget::opaque(container(items).padding(4).style(popup_style)))
            .padding(iced::Padding { top: menu.at.y, left: menu.at.x, right: 0.0, bottom: 0.0 })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The settings or the saved layouts, hanging from the top right of the window.
    fn panel_view(&self, panel: Panel) -> Element<'_, Message> {
        let content = match panel {
            Panel::Layouts => self.layouts_panel(),
        };
        container(iced::widget::opaque(container(content).padding(14).width(320).style(popup_style)))
            .padding(iced::Padding { top: 36.0, right: 12.0, left: 0.0, bottom: 0.0 })
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .into()
    }

    fn settings_panel(&self) -> Element<'_, Message> {
        use iced::widget::slider;
        let settings = &self.sh.settings;
        let label = |t: &'static str| text(t).size(12).style(text::secondary);
        let size_row = |value: f32, range: std::ops::RangeInclusive<f32>, msg: fn(f32) -> Message| {
            row![slider(range, value, msg).step(1.0), text(format!("{value:.0}")).size(13).width(26)].spacing(10).align_y(iced::Center)
        };
        let mut fonts = vec![FontChoice(None)];
        fonts.extend(self.families.iter().cloned().map(|f| FontChoice(Some(f))));
        column![
            text("Settings").size(16),
            column![
                label("Color scheme"),
                pick_list(Theme::ALL, Some(self.sh.theme()), Message::SetTheme).text_size(13).width(Length::Fill)
            ]
            .spacing(4),
            column![label("Bible pane text size"), size_row(settings.bible_size, state::BIBLE_SIZES, Message::SetBibleSize)].spacing(4),
            column![label("Notes pane text size"), size_row(settings.notes_size, state::NOTES_SIZES, Message::SetNotesSize)].spacing(4),
            column![
                label("Bible and note font"),
                pick_list(fonts, Some(FontChoice(settings.font.clone())), Message::SetFont).text_size(13).width(Length::Fill)
            ]
            .spacing(4),
            self.pens_settings(),
            button(text("Reset to defaults").size(13)).padding([4, 10]).style(button::secondary).on_press(Message::ResetSettings),
        ]
        .spacing(12)
        .into()
    }

    /// The pens: tags with a color, each a shortcut on the reader's toolbar for a note with that
    /// tag on the selected text.
    fn pens_settings(&self) -> Element<'_, Message> {
        let swatch = |hex: &str, msg: Message| -> Element<'_, Message> {
            let color = bible_tab::hex_color(hex, 1.0).unwrap_or(iced::Color::WHITE);
            button(space::horizontal())
                .width(22)
                .height(22)
                .padding(0)
                .style(move |_: &Theme, _| button::Style {
                    background: Some(color.into()),
                    border: iced::Border { radius: 11.0.into(), color: iced::Color { a: 0.5, ..iced::Color::WHITE }, width: 1.0 },
                    ..button::Style::default()
                })
                .on_press(msg)
                .into()
        };
        let mut list = column![].spacing(4);
        for (i, pen) in self.sh.settings.pens.iter().enumerate() {
            let style = match pen.style {
                state::PenStyle::Highlight => "Highlight",
                state::PenStyle::Underline => "Underline",
            };
            list = list.push(
                row![
                    swatch(&pen.color, Message::CyclePenColor(i)),
                    text(format!("#{}", pen.tag)).size(13).width(Length::Fill),
                    button(text(style).size(12)).padding([1, 8]).style(button::secondary).on_press(Message::CyclePenStyle(i)),
                    button(text("\u{00D7}").size(14)).padding([0, 6]).style(button::text).on_press(Message::RemovePen(i)),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
        }
        let draft = &self.pen_draft;
        let new_pen = row![
            swatch(state::PEN_COLORS[self.pen_color % state::PEN_COLORS.len()], Message::CycleDraftColor),
            text_input("A tag for a new pen", draft).on_input(Message::PenDraft).on_submit(Message::AddPen).size(13).padding([3, 6]),
            button(text("Add").size(12)).padding([3, 10]).style(button::secondary).on_press_maybe((!draft.trim().is_empty()).then_some(Message::AddPen)),
        ]
        .spacing(8)
        .align_y(iced::Center);
        // Tags already in use that fit what is typed, to take instead of typing the rest.
        let have: Vec<String> = self.sh.settings.pens.iter().map(|p| p.tag.clone()).collect();
        let mut offered = row![].spacing(4);
        for (tag, _) in self.sh.tag_suggestions(draft, &have, 5) {
            offered = offered.push(
                button(text(format!("#{tag}")).size(11)).padding([1, 6]).style(button::secondary).on_press(Message::PenDraft(tag)),
            );
        }
        column![
            text("Pens").size(12).style(text::secondary),
            text("A pen marks the selected text and makes a note with its tag, in its color.").size(11).style(text::secondary),
            list,
            new_pen,
            offered,
        ]
        .spacing(6)
        .into()
    }

    /// The saved layouts, and a way to save the workspace as another.
    fn layouts_panel(&self) -> Element<'_, Message> {
        let mut list = column![
            button(text(DEFAULT_LAYOUT).size(13))
                .width(Length::Fill)
                .padding([4, 8])
                .style(button::text)
                .on_press(Message::LoadLayout(DEFAULT_LAYOUT.into()))
        ]
        .spacing(2);
        for layout in &self.sh.layouts {
            list = list.push(
                row![
                    button(text(layout.name.clone()).size(13))
                        .width(Length::Fill)
                        .padding([4, 8])
                        .style(button::text)
                        .on_press(Message::LoadLayout(layout.name.clone())),
                    button(text("\u{00D7}").size(14)).padding([2, 8]).style(button::text).on_press(Message::DeleteLayout(layout.name.clone())),
                ]
                .align_y(iced::Center),
            );
        }
        column![
            list,
            rule::horizontal(1),
            button(text("Save as new layout\u{2026}").size(13)).width(Length::Fill).padding([4, 8]).style(button::text).on_press(Message::NameLayout),
        ]
        .spacing(8)
        .into()
    }

    /// Asks for the name to save a layout under, over a dimmed window.
    fn name_layout_view(&self) -> Element<'_, Message> {
        let name = self.layout_draft.trim();
        let reserved = name.eq_ignore_ascii_case(DEFAULT_LAYOUT);
        let replaces = self.sh.layouts.iter().any(|l| l.name == name);
        let mut card = column![
            text("Save as new layout").size(16),
            text("The panes and tabs as they are now.").size(12).style(text::secondary),
            text_input("Name", &self.layout_draft)
                .id(layout_name_id())
                .on_input(Message::LayoutDraft)
                .on_submit(Message::SaveLayout)
                .size(14)
                .padding([5, 8]),
        ]
        .spacing(10);
        if reserved {
            card = card.push(text("Default is built in: choose another name.").size(12).style(text::secondary));
        } else if replaces {
            card = card.push(text("A layout with this name will be replaced.").size(12).style(text::secondary));
        }
        card = card.push(
            row![
                space::horizontal(),
                button(text("Cancel").size(13)).padding([4, 12]).style(button::secondary).on_press(Message::CancelNaming),
                button(text("Save").size(13)).padding([4, 12]).on_press_maybe((!name.is_empty() && !reserved).then_some(Message::SaveLayout)),
            ]
            .spacing(8),
        );
        let dim = |_: &Theme| container::Style {
            background: Some(iced::Color { a: 0.4, ..iced::Color::BLACK }.into()),
            ..container::Style::default()
        };
        let backdrop = iced::widget::mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill).style(dim))
            .on_press(Message::CancelNaming);
        let centered = container(iced::widget::opaque(container(card).padding(16).width(340).style(popup_style)))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .padding(iced::Padding { top: 120.0, ..iced::Padding::ZERO });
        stack![backdrop, centered].into()
    }

    /// What applies to the whole window.
    fn top_bar(&self) -> Element<'_, Message> {
        let mut bar = row![space::horizontal()].spacing(16).align_y(iced::Center);
        if !self.sh.status.is_empty() {
            bar = bar.push(text(self.sh.status.clone()).size(13).style(text::danger));
        }
        let tool = |label: &'static str, panel: Panel| {
            let on = self.panel == Some(panel);
            button(text(label).size(13))
                .padding([2, 8])
                .style(move |theme: &Theme, status| if on { button::secondary(theme, status) } else { button::text(theme, status) })
                .on_press(Message::TogglePanel(panel))
        };
        let settings = button(text("Settings").size(13)).padding([2, 8]).style(button::text).on_press(Message::OpenSettings);
        bar = bar.push(row![tool("Layouts", Panel::Layouts), settings].spacing(4));
        container(bar).padding([6, 16]).into()
    }

    fn pane_view(&self, index: usize) -> Element<'_, Message> {
        let pane = &self.panes[index];
        let content: Element<'_, Message> = match pane.active.and_then(|id| self.tabs.get(&id)) {
            Some(Tab::Bible(tab)) => {
                let id = tab.id;
                tab.view(&self.sh).map(move |m| Message::Bible(id, m))
            }
            Some(Tab::Dict(tab)) => {
                let id = tab.id;
                tab.view(&self.sh).map(move |m| Message::Dict(id, m))
            }
            Some(Tab::Note(tab)) => {
                let id = tab.id;
                let reference = self.reference_bible_id().and_then(|b| self.bible_tab(b));
                tab.view(&self.sh, reference).map(move |m| Message::Note(id, m))
            }
            Some(Tab::Blank(id)) => self.blank_view(*id),
            None => space::vertical().into(),
        };
        // What can be dropped on the pane: only there while something is being dragged, and the
        // layer is always in the tree so that what is under it keeps its state.
        let zones: Element<'_, Message> = if self.drag.is_some() { self.drop_zones(pane.id) } else { space::horizontal().width(0).height(0).into() };
        let body = column![self.tab_bar(index), stack![content, zones]];
        // The pane's own controls sit in its bottom right corner, over the content.
        let corner = container(self.pane_controls(pane.id))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding(10);
        stack![body, corner].into()
    }

    /// The five places on a pane something can be dropped (the middle, and a band along each
    /// edge), with the one the pointer is over lit to show what will happen.
    fn drop_zones(&self, pane: PaneId) -> Element<'_, Message> {
        let lit = match self.drag.as_ref().and_then(|d| d.over) {
            Some(Over::Pane(p, zone)) if p == pane => Some(zone),
            _ => None,
        };
        // The hit areas: transparent.
        let region = |zone: Zone, w: Length, h: Length| -> Element<'_, Message> {
            let over = Over::Pane(pane, zone);
            iced::widget::mouse_area(container(space::horizontal()).width(w).height(h))
                .on_enter(Message::DragOver(over))
                .on_exit(Message::DragOut(over))
                .into()
        };
        let hit = column![
            region(Zone::Top, Length::Fill, Length::FillPortion(1)),
            row![
                region(Zone::Left, Length::FillPortion(1), Length::Fill),
                region(Zone::Center, Length::FillPortion(2), Length::Fill),
                region(Zone::Right, Length::FillPortion(1), Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::FillPortion(2)),
            region(Zone::Bottom, Length::Fill, Length::FillPortion(1)),
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        // What it would look like: the lit zone's half of the pane (all of it, for the middle).
        let light = || -> Element<'_, Message> {
            container(space::horizontal())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|theme: &Theme| {
                    let primary = theme.extended_palette().primary.base.color;
                    container::Style {
                        background: Some(iced::Color { a: 0.09, ..primary }.into()),
                        border: iced::Border { color: iced::Color { a: 0.45, ..primary }, width: 1.0, radius: 4.0.into() },
                        ..container::Style::default()
                    }
                })
                .into()
        };
        let none = || -> Element<'_, Message> { container(space::horizontal()).width(Length::Fill).height(Length::Fill).into() };
        let preview: Element<'_, Message> = match lit {
            None => none(),
            Some(Zone::Center) => light(),
            Some(Zone::Left) => row![light(), none()].width(Length::Fill).height(Length::Fill).into(),
            Some(Zone::Right) => row![none(), light()].width(Length::Fill).height(Length::Fill).into(),
            Some(Zone::Top) => column![light(), none()].width(Length::Fill).height(Length::Fill).into(),
            Some(Zone::Bottom) => column![none(), light()].width(Length::Fill).height(Length::Fill).into(),
        };
        stack![preview, hit].into()
    }

    /// What is being carried, written next to the pointer.
    fn drag_label(&self, drag: &Drag) -> Element<'_, Message> {
        let name = match &drag.payload {
            Payload::Tab(id) => match self.tabs.get(id) {
                Some(Tab::Bible(t)) => t.title(&self.sh),
                Some(Tab::Note(t)) => t.title(&self.sh),
                Some(Tab::Dict(t)) => t.title(&self.sh),
                _ => "New tab".to_string(),
            },
            Payload::Link(spec) => spec.label(),
        };
        container(container(text(name).size(13)).padding([4, 10]).style(container::rounded_box))
            .padding(iced::Padding { top: drag.at.y + 14.0, left: drag.at.x + 14.0, right: 0.0, bottom: 0.0 })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn pane_controls(&self, pane: PaneId) -> Element<'_, Message> {
        let zoomed = self.zoomed == Some(pane);
        let small = |icon: icons::Icon, tip: &'static str, msg: Message| {
            tooltip(
                button(container(icon.view(18.0)).center(Length::Fill))
                    .width(28)
                    .height(28)
                    .padding(0)
                    .style(|theme: &Theme, status| {
                        // They float over the text, so see-through (a little less so under the pointer).
                        let base = button::secondary(theme, status);
                        let alpha = if matches!(status, button::Status::Hovered | button::Status::Pressed) { 0.85 } else { 0.5 };
                        button::Style {
                            background: base.background.map(|b| match b {
                                iced::Background::Color(c) => iced::Background::Color(iced::Color { a: c.a * alpha, ..c }),
                                other => other,
                            }),
                            ..base
                        }
                    })
                    .on_press(msg),
                container(text(tip).size(12)).padding(4).style(container::rounded_box),
                tooltip::Position::Top,
            )
        };
        let mut controls = row![].spacing(6);
        // Filling the window means nothing with only one pane (and a zoomed pane needs its button
        // to get back).
        if self.panes.len() > 1 || zoomed {
            controls = controls.push(small(
                icons::Icon::from_zoom(zoomed),
                if zoomed { "Show all panes" } else { "Fill the window with this pane" },
                Message::ToggleZoom(pane),
            ));
        }
        controls
            .push(small(icons::Icon::PanesAcross, "Split side by side", Message::SplitPane(pane, panes::Axis::Row)))
            .push(small(icons::Icon::PanesDown, "Split top and bottom", Message::SplitPane(pane, panes::Axis::Column)))
            .into()
    }

    /// A new tab: choose what it becomes.
    fn blank_view(&self, id: TabId) -> Element<'_, Message> {
        let choices = pick_list(self.choices(), None::<Choice>, move |c| Message::Open(id, c)).placeholder("Open…");
        container(
            column![text("New tab").size(22), text("Open a Bible or a note here.").size(14).style(text::secondary), choices]
                .spacing(12)
                .align_x(iced::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }

    /// Tabs like a web browser's: each a chip with its name and a close button inside, and
    /// the new tab button right after the last.
    fn tab_bar(&self, index: usize) -> Element<'_, Message> {
        let pane = &self.panes[index];
        let mut bar = row![].spacing(2).align_y(iced::Alignment::End);
        for &id in &pane.tabs {
            let Some(tab) = self.tabs.get(&id) else { continue };
            let title = match tab {
                Tab::Blank(_) => "New tab".to_string(),
                Tab::Bible(t) => t.title(&self.sh),
                Tab::Note(t) => t.title(&self.sh),
                Tab::Dict(t) => t.title(&self.sh),
            };
            let title: String = if title.chars().count() > 24 {
                title.chars().take(23).chain(['…']).collect()
            } else {
                title
            };
            let active = pane.active == Some(id);
            let label = if active { text(title).size(13) } else { text(title).size(13).style(text::secondary) };
            // Pressing the name selects the tab, and moving the pointer on from there drags it.
            let name = iced::widget::mouse_area(
                container(label).padding(iced::Padding { top: 7.0, right: 4.0, bottom: 7.0, left: 12.0 }),
            )
            .on_press(Message::TabPressed(id))
            .interaction(iced::mouse::Interaction::Pointer);
            let chip = row![
                name,
                button(text("×").size(15)).padding([2, 6]).style(button::text).on_press(Message::CloseTab(id)),
            ]
            .align_y(iced::Center);
            let lit = self.drag.as_ref().is_some_and(|d| d.over == Some(Over::Chip(id)));
            let styled = container(chip).style(move |theme: &Theme| {
                let p = theme.extended_palette();
                let r = 7.0;
                let background = if lit {
                    iced::Color { a: 0.2, ..p.primary.base.color }
                } else if active {
                    p.background.base.color
                } else {
                    p.background.weak.color
                };
                container::Style {
                    background: Some(background.into()),
                    border: iced::Border {
                        radius: iced::border::Radius { top_left: r, top_right: r, bottom_right: 0.0, bottom_left: 0.0 },
                        ..iced::Border::default()
                    },
                    ..container::Style::default()
                }
            });
            // While something is carried, a tab is a place to drop it (in front of that tab).
            let mut place = iced::widget::mouse_area(styled);
            if self.drag.is_some() {
                place = place.on_enter(Message::DragOver(Over::Chip(id))).on_exit(Message::DragOut(Over::Chip(id)));
            }
            bar = bar.push(place);
        }
        bar = bar.push(
            // As tall as a tab, with the plus in the middle of it.
            button(container(text("+").size(17)).center(Length::Fill))
                .width(30)
                .height(30)
                .padding(0)
                .style(button::text)
                .on_press(Message::NewTab(pane.id)),
        );
        let bar = container(bar)
            .width(Length::Fill)
            .padding(iced::Padding { top: 6.0, right: 8.0, bottom: 0.0, left: 8.0 })
            .style(|theme: &Theme| container::Style {
                background: Some(theme.extended_palette().background.weak.color.into()),
                ..container::Style::default()
            });
        let mut area = iced::widget::mouse_area(bar);
        if self.drag.is_some() {
            let pane = pane.id;
            area = area.on_move(move |_| Message::DragBar(pane)).on_exit(Message::DragOut(Over::Bar(pane)));
        }
        area.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bible_tab::*;
    use crate::note_tab::{KindChoice, NoteChoice};
    use crate::reader_text::{CanonSlice, CardSpec, MarkKind};
    use iced::Font;
    use md_editor::{Action as EditAction, Document};
    use scripture::{Book, RefRange};
    use study_core::margin::{self, Mark, MarkStyle, Slice};
    use study_core::markdown::LinkTarget;
    fn template_path(name: &str) -> String {
        format!("templates/{name}.md")
    }

    // ---- saved state ---------------------------------------------------------------------

    /// A workspace without its ids: the layout with each pane's tabs, so two can be compared.
    fn shape(ws: &Workspace) -> String {
        fn walk(l: &state::Layout, ws: &Workspace) -> String {
            match l {
                state::Layout::Pane { id } => {
                    let pane = ws.panes.iter().find(|p| p.id == *id).unwrap();
                    format!("{:?}@{:?}", pane.tabs, pane.active)
                }
                state::Layout::Split { axis, fraction, a, b } => format!("{axis:?} {fraction:.2} ({}) ({})", walk(a, ws), walk(b, ws)),
            }
        }
        walk(&ws.layout, ws)
    }

    fn with_store() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.sh.attach(Store::new(dir.path().join("state")));
        (app, dir)
    }

    #[test]
    fn a_workspace_comes_back_as_it_was() {
        let mut app = App::new();
        app.add_bible(1);
        let _ = app.update(Message::SplitPane(app.panes[0].id, panes::Axis::Column));
        let bible = app.panes[0].tabs[0];
        if let Some(Tab::Bible(t)) = app.tabs.get_mut(&bible) {
            t.scroll_y = 321.0;
            t.show_strongs = true;
        }
        let saved = app.workspace();
        let json = serde_json::to_string(&saved).unwrap();
        let saved: Workspace = serde_json::from_str(&json).unwrap();

        let mut other = App::new();
        let _ = other.update(Message::SplitPane(other.panes[0].id, panes::Axis::Row));
        let _ = other.apply_workspace(&saved).unwrap();
        assert_eq!(shape(&other.workspace()), shape(&saved));
        let restored = other.panes.iter().flat_map(|p| p.tabs.iter()).find_map(|id| match other.tabs.get(id) {
            Some(Tab::Bible(t)) if t.scroll_y == 321.0 => Some(t),
            _ => None,
        });
        assert!(restored.is_some_and(|t| t.show_strongs), "scroll and view options come back");
        assert_eq!(other.tabs.len(), saved.panes.iter().map(|p| p.tabs.len()).sum::<usize>(), "the old tabs are gone");
        let leaves = other.layout.leaves();
        assert!(other.panes.iter().all(|p| leaves.contains(&p.id)) && other.panes.len() == leaves.len());
        assert!(leaves.contains(&other.focus));
    }

    #[test]
    fn tabs_that_cannot_come_back_are_left_out_and_empty_panes_collapse() {
        let mut app = App::new();
        let mut saved = app.workspace();
        // The note is gone from the vault; the Bible tab's module and book are gone too.
        saved.panes[1].tabs = vec![state::TabSave::Note { path: "gone.md".into() }];
        saved.panes[0].tabs = vec![state::TabSave::Bible {
            module: "NOPE".into(),
            book: "John".into(),
            chapter: 3,
            scroll: 0.0,
            strongs: false,
            marks: true,
            footnotes: true,
            refs: true,
            in_margin: state::MarginKinds::default(),
        }];
        assert!(app.apply_workspace(&saved).is_err());
        assert_eq!(app.panes.len(), 2, "nothing was changed");

        saved.panes[0].tabs.push(state::TabSave::Blank);
        saved.zoomed = Some(saved.panes[1].id);
        let _ = app.apply_workspace(&saved).unwrap();
        assert_eq!(app.panes.len(), 1);
        assert!(matches!(app.layout, Node::Leaf(_)));
        assert_eq!(app.zoomed, None, "a zoomed pane that is gone is not zoomed");
        assert!(matches!(app.tabs.values().next(), Some(Tab::Blank(_))));

        // A chapter the module does not have opens at its first chapter.
        let mut saved = App::new().workspace();
        if let state::TabSave::Bible { chapter, scroll, .. } = &mut saved.panes[0].tabs[0] {
            *chapter = 999;
            *scroll = 50.0;
        }
        let _ = app.apply_workspace(&saved).unwrap();
        let Some(Tab::Bible(t)) = app.tabs.values().find(|t| matches!(t, Tab::Bible(_))) else { panic!("no Bible tab") };
        assert!(t.chapter.is_ok());
        assert_eq!(t.scroll_y, 0.0);
    }

    #[test]
    fn layouts_are_saved_loaded_replaced_and_deleted() {
        let (mut app, dir) = with_store();
        let original = shape(&app.workspace());
        // Naming asks first, and can be backed out of.
        let _ = app.update(Message::TogglePanel(Panel::Layouts));
        let _ = app.update(Message::NameLayout);
        assert!(app.naming && app.panel.is_none(), "the name box replaces the list");
        let _ = app.update(Message::LayoutDraft("Nope".into()));
        let _ = app.update(Message::CancelNaming);
        assert!(!app.naming && app.layout_draft.is_empty() && app.sh.layouts.is_empty());
        let _ = app.update(Message::NameLayout);
        let _ = app.update(Message::LayoutDraft("  Study  ".into()));
        let _ = app.update(Message::SaveLayout);
        assert_eq!(app.sh.layouts.len(), 1);
        assert_eq!(app.sh.layouts[0].name, "Study");
        assert!(app.layout_draft.is_empty() && !app.naming);

        // Change the workspace, then bring the saved one back.
        let _ = app.update(Message::SplitPane(app.panes[0].id, panes::Axis::Column));
        assert_ne!(shape(&app.workspace()), original);
        let _ = app.update(Message::TogglePanel(Panel::Layouts));
        let _ = app.update(Message::LoadLayout("Study".into()));
        assert_eq!(shape(&app.workspace()), original);
        assert_eq!(app.panel, None, "loading puts the panel away");

        // Saving under the same name replaces it; a blank name saves nothing.
        let _ = app.update(Message::SplitPane(app.panes[0].id, panes::Axis::Column));
        let _ = app.update(Message::NameLayout);
        let _ = app.update(Message::LayoutDraft("Study".into()));
        let _ = app.update(Message::SaveLayout);
        let _ = app.update(Message::NameLayout);
        let _ = app.update(Message::LayoutDraft("   ".into()));
        let _ = app.update(Message::SaveLayout);
        assert!(app.naming, "a blank name does not save, and the box stays");
        let _ = app.update(Message::CancelNaming);
        assert_eq!(app.sh.layouts.len(), 1);
        assert_eq!(app.sh.layouts[0].workspace.panes.len(), 3);

        // They are on disk for the next run.
        assert_eq!(Store::new(dir.path().join("state")).layouts().len(), 1);
        let _ = app.update(Message::DeleteLayout("Study".into()));
        assert!(app.sh.layouts.is_empty());
        assert!(Store::new(dir.path().join("state")).layouts().is_empty());
        let _ = app.update(Message::LoadLayout("Study".into()));
    }

    #[test]
    fn the_default_layout_is_always_there_and_puts_the_bible_left_and_a_note_right() {
        let mut app = App::new();
        let fresh = shape(&app.workspace());
        assert!(app.sh.layouts.is_empty());
        // Rearrange, then ask for the default.
        let _ = app.update(Message::SplitPane(app.panes[0].id, panes::Axis::Column));
        app.add_note(0);
        assert_ne!(shape(&app.workspace()), fresh);
        let _ = app.update(Message::LoadLayout(DEFAULT_LAYOUT.into()));
        assert_eq!(shape(&app.workspace()), fresh);
        let ws = app.workspace();
        assert!(matches!(&ws.layout, state::Layout::Split { axis: state::AxisSave::Row, fraction, .. } if *fraction == 0.5));
        assert!(matches!(ws.panes[0].tabs[..], [state::TabSave::Bible { .. }]));
        assert!(matches!(ws.panes[1].tabs[..], [state::TabSave::Note { .. }]));

        // It is not kept in the saved layouts, and the name cannot be taken.
        let _ = app.update(Message::NameLayout);
        let _ = app.update(Message::LayoutDraft(" default ".into()));
        let _ = app.update(Message::SaveLayout);
        assert!(app.sh.layouts.is_empty() && app.naming);
        let _ = app.update(Message::DeleteLayout(DEFAULT_LAYOUT.into()));
    }

    #[test]
    fn the_session_is_written_soon_after_a_change_and_only_when_it_changed() {
        let (mut app, dir) = with_store();
        let store = || Store::new(dir.path().join("state"));
        app.saved = Some(app.workspace());
        let _ = app.update(Message::Modifiers(iced::keyboard::Modifiers::empty()));
        assert!(app.unsaved.is_none(), "nothing changed");

        let _ = app.update(Message::SplitPane(app.panes[0].id, panes::Axis::Column));
        assert!(app.unsaved.is_some());
        assert_eq!(store().session(), None, "not written yet");
        let _ = app.update(Message::SaveSession);
        assert_eq!(store().session(), Some(app.workspace()));
        assert!(app.unsaved.is_none());

        // Closing writes what is waiting.
        let _ = app.update(Message::SplitPane(app.panes[0].id, panes::Axis::Row));
        let main = app.main;
        let _ = app.update(Message::CloseRequested(main));
        assert_eq!(store().session(), Some(app.workspace()));
    }

    #[test]
    fn settings_apply_at_once_and_are_kept() {
        let (mut app, dir) = with_store();
        let id = app.panes[0].tabs[0];
        let _ = app.update(Message::SetTheme(Theme::Light));
        assert_eq!(app.theme(), Theme::Light);
        let _ = app.update(Message::SetBibleSize(24.4));
        let _ = app.update(Message::SetNotesSize(15.0));
        let _ = app.update(Message::SetFont(FontChoice(Some("Georgia".into()))));
        let Some(Tab::Bible(tab)) = app.tabs.get(&id) else { panic!("no Bible tab") };
        let options = tab.view_options(&app.sh);
        assert_eq!(options.body, 24.0);
        assert_eq!(options.font, Font::with_name("Georgia"));

        let kept = Store::new(dir.path().join("state")).settings();
        assert_eq!(kept, app.sh.settings);
        assert_eq!((kept.theme.as_str(), kept.bible_size, kept.notes_size, kept.font.as_deref()), ("Light", 24.0, 15.0, Some("Georgia")));

        let _ = app.update(Message::SetBibleSize(500.0));
        assert_eq!(app.sh.settings.bible_size, *state::BIBLE_SIZES.end());
        let _ = app.update(Message::ResetSettings);
        assert_eq!(Store::new(dir.path().join("state")).settings(), Settings::default());
    }

    #[test]
    fn the_layouts_panel_closes_on_outside_clicks_and_settings_has_its_own_window() {
        let mut app = App::new();
        let _ = app.update(Message::TogglePanel(Panel::Layouts));
        assert_eq!(app.panel, Some(Panel::Layouts));
        let _ = app.update(Message::LayoutDraft("x".into()));
        assert_eq!(app.panel, Some(Panel::Layouts), "typing in it keeps it");
        let _ = app.update(Message::PressedElsewhere);
        assert_eq!(app.panel, None);
        let _ = app.update(Message::TogglePanel(Panel::Layouts));
        let _ = app.update(Message::TogglePanel(Panel::Layouts));
        assert_eq!(app.panel, None, "the button toggles");

        // Settings: one window, however often it is asked for, and changes to it leave the
        // workspace alone (they do not put pop-ups away).
        assert_eq!(app.settings_window, None);
        let _ = app.update(Message::OpenSettings);
        let window = app.settings_window.expect("a window was opened");
        let _ = app.update(Message::OpenSettings);
        assert_eq!(app.settings_window, Some(window));
        assert!(!app.families.is_empty() || state::installed_families().is_empty());
        let _ = app.update(Message::TogglePanel(Panel::Layouts));
        let _ = app.update(Message::SetBibleSize(30.0));
        assert_eq!(app.sh.settings.bible_size, 30.0);
        let _ = app.update(Message::CloseRequested(window));
        assert_eq!(app.settings_window, None);
        let _ = app.update(Message::OpenSettings);
        assert_ne!(app.settings_window, Some(window), "closed means a new window next time");
    }

    /// Test-side access to the tabs and the old single-reader operations: the app under test
    /// has one Bible tab and one note tab, so these name them.
    impl App {
        /// A new Bible tab at the end of the pane at `pane`, opened the way a blank tab opens.
        fn add_bible(&mut self, pane: usize) -> TabId {
            let blank = self.new_blank();
            let id = self.add_tab(pane, blank);
            let module = self.first_module();
            let abbreviation = self.sh.module_abbreviation(&module);
            let _ = self.update(Message::Open(id, Choice::Bible(module, abbreviation)));
            id
        }

        fn add_note(&mut self, pane: usize) -> TabId {
            let blank = self.new_blank();
            let id = self.add_tab(pane, blank);
            let path = self.sh.first_ordinary_note();
            let _ = self.update(Message::Open(id, Choice::Note(path)));
            id
        }

        fn persist_ref(&self) -> Option<&Persist> {
            self.sh.persist.as_ref()
        }

        fn bid(&self) -> TabId {
            self.reference_bible_id().expect("a Bible tab")
        }

        fn nid(&self) -> TabId {
            self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Note(_)).then_some(*id)).expect("a note tab")
        }

        fn bible(&self) -> &BibleTab {
            self.bible_tab(self.bid()).expect("a Bible tab")
        }

        fn bible_mut(&mut self) -> &mut BibleTab {
            let id = self.bid();
            match self.tabs.get_mut(&id) {
                Some(Tab::Bible(t)) => t,
                _ => unreachable!(),
            }
        }

        fn note(&self) -> &NoteTab {
            match self.tabs.get(&self.nid()) {
                Some(Tab::Note(t)) => t,
                _ => unreachable!(),
            }
        }

        fn note_mut(&mut self) -> &mut NoteTab {
            let id = self.nid();
            match self.tabs.get_mut(&id) {
                Some(Tab::Note(t)) => t,
                _ => unreachable!(),
            }
        }

        fn b(&self, m: BibleMsg) -> Message {
            Message::Bible(self.bid(), m)
        }

        fn n(&self, m: NoteMsg) -> Message {
            Message::Note(self.nid(), m)
        }

        fn load(&mut self, book: Book, chapter: u16) {
            let id = self.bid();
            if let Some(Tab::Bible(t)) = self.tabs.get_mut(&id) {
                t.load(&mut self.sh, book, chapter);
            }
        }

        fn open_note(&mut self, path: String) {
            let id = self.nid();
            if let Some(Tab::Note(t)) = self.tabs.get_mut(&id) {
                t.open(&mut self.sh, path);
            }
        }

        fn open_note_card(&mut self, at: Option<CanonSlice>, content: NoteContent) {
            let id = self.bid();
            if let Some(Tab::Bible(t)) = self.tabs.get_mut(&id) {
                t.open_note_card(&mut self.sh, at, content);
            }
        }

        fn set_body(&mut self, index: usize, body: String) {
            self.sh.set_body(index, body);
            self.settle();
        }

        fn sync_mark(&mut self, path: &str) {
            self.sh.sync_mark(path);
            self.settle();
        }

        fn cards(&self) -> Vec<CardSpec<'_, BibleMsg>> {
            self.bible().cards(&self.sh)
        }

        fn card_view(&self, index: usize, popup: bool) -> Element<'_, BibleMsg> {
            self.bible().card_view(&self.sh, index, popup)
        }

        fn visible_marks(&self) -> Vec<(usize, Vec<Slice>)> {
            self.bible().visible_marks(&self.sh)
        }

        fn mark_draws(&self) -> Vec<crate::reader_text::MarkDraw> {
            self.bible().mark_draws(&self.sh)
        }

        fn module_abbreviation(&self) -> String {
            self.sh.module_abbreviation(&self.bible().module)
        }

        fn selected_refs(&self) -> Vec<RefRange> {
            self.bible().selected_refs()
        }

        fn annotated_verses(&self) -> std::collections::BTreeSet<u16> {
            self.sh.annotated_verses(self.bible().book, self.bible().chapter_no)
        }

        fn finish_editing(&mut self) -> Option<usize> {
            let id = self.bid();
            match self.tabs.get_mut(&id) {
                Some(Tab::Bible(t)) => t.finish_editing(&mut self.sh),
                _ => None,
            }
        }

        fn selection_label(&self) -> String {
            self.bible().selection_label()
        }

        fn embed_for(&self, target: &LinkTarget) -> Option<crate::live_editor::EmbedContent> {
            self.sh.embed_for(&self.bible().module, target)
        }

        fn reference_card<'a>(
            &self,
            label: &str,
            body: Element<'a, BibleMsg>,
            closable: bool,
            fill: bool,
        ) -> Element<'a, BibleMsg> {
            crate::bible_tab::reference_card(&self.sh, label, body, closable, fill, None)
        }

    }

    /// Make the reader's text selection cover these whole verses, as a verse-number click would.
    fn select_verses(app: &mut App, verses: &[u16]) {
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        app.bible_mut().text_selection = Some(
            verses.iter().map(|&v| CanonSlice { verse: v, start: 0, end: canon[&v].len() }).collect(),
        );
    }

    /// A live app (real modules, a vault in `home`), or `None` where ASV is not installed.
    fn live_app(home: &std::path::Path) -> Option<App> {
        let boot = live::boot_at(home, &sword::default_roots(), false);
        boot.persist.as_ref()?;
        Some(App::build(boot.backend, boot.notes, boot.persist))
    }

    #[test]
    fn marking_text_in_a_real_module_is_saved_and_survives_a_restart() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3));
        assert!(app.bible().chapter.as_ref().unwrap().canonical_verses()[&16].contains("God so loved the world"));
        let seeded = app.sh.marks.len();
        assert!(seeded >= 7, "{seeded}");

        select_verses(&mut app, &[17]);
        let _ = app.update(app.b(BibleMsg::ApplyPen(2)));
        let mark = app.sh.marks.last().unwrap().clone();
        assert!(app.sh.marks.iter().filter(|m| m.id == mark.id).count() == 1, "ids are unique");
        let on_disk = app.sh.persist.as_ref().unwrap().vault().read(&mark.path()).unwrap();
        assert_eq!(Mark::from_markdown(&mark.id, &on_disk).as_ref(), Some(&mark));

        // Writing a comment renames the file (a bare highlight becomes a note) without
        // leaving the old one behind.
        let old = mark.path();
        app.set_body(app.sh.marks.len() - 1, "Why he came.".into());
        let new = app.sh.marks.last().unwrap().path();
        assert_ne!(old, new);
        let _ = app.update(Message::SetTheme(Theme::Light));
        let vault = app.sh.persist.as_ref().unwrap().vault();
        assert!(vault.read(&old).is_err() && vault.read(&new).unwrap().contains("Why he came."));
        drop(app);

        let again = live_app(home.path()).unwrap();
        assert_eq!(again.sh.marks.len(), seeded + 1);
        assert_eq!(again.sh.marks.iter().find(|m| m.id == mark.id).unwrap().body, "Why he came.");
    }

    #[test]
    fn switching_module_keeps_the_chapter_and_shows_only_that_modules_word_marks() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let Some(other) = app.sh.backend.modules().into_iter().find(|m| m.abbreviation != app.module_abbreviation()) else {
            return eprintln!("skipped: only one module");
        };
        let word_marks = |app: &App| app.visible_marks().iter().filter(|(i, _)| !app.sh.marks[*i].is_verse_level()).count();
        let before = word_marks(&app);
        assert!(before > 0);
        let _ = app.update(app.b(BibleMsg::PickModule(ModuleChoice(other.id.clone(), other.abbreviation.clone()))));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3));
        assert_eq!(app.bible().module, other.id);
        assert_eq!(word_marks(&app), 0, "word marks were made in ASV, and stay there");
        assert!(app.bible().chapter.is_ok());
    }

    /// An app with the seeded sample marks removed.
    fn blank_app() -> App {
        let mut app = App::new();
        app.sh.marks.clear();
        app.sh.next_mark = 0;
        app
    }

    // ---- headless end-to-end harness ------------------------------------------------

    use iced::{Event, Pixels, Point, Rectangle, Size, keyboard, mouse};
    use iced_test::core::renderer::Headless;
    use iced_test::runtime::{UserInterface, user_interface::Cache};

    /// Drives the real view like the runtime does: build, feed events, apply the resulting
    /// messages to the app, rebuild (reusing widget state between steps).
    /// A clipboard that remembers what was written to it.
    #[derive(Default)]
    struct TestClipboard(Option<String>);

    impl iced::advanced::Clipboard for TestClipboard {
        fn read(&self, _kind: iced::advanced::clipboard::Kind) -> Option<String> {
            self.0.clone()
        }

        fn write(&mut self, _kind: iced::advanced::clipboard::Kind, contents: String) {
            self.0 = Some(contents);
        }
    }

    struct Harness {
        app: App,
        cache: Cache,
        renderer: iced::Renderer,
        cursor: mouse::Cursor,
        size: Size,
        clipboard: TestClipboard,
    }

    impl Harness {
        fn new(app: App) -> Self {
            let renderer = iced_test::futures::futures::executor::block_on(
                <iced::Renderer as Headless>::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia")),
            )
            .expect("headless renderer");
            let mut h = Harness {
                app,
                cache: Cache::default(),
                renderer,
                cursor: mouse::Cursor::Unavailable,
                size: Size::new(1024.0, 1500.0),
                clipboard: TestClipboard::default(),
            };
            // Like the runtime's first frame: lets the reader report what layout it chose.
            h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            h
        }

        fn feed(&mut self, event: Event) {
            let mut messages = Vec::new();
            {
                let mut ui = UserInterface::build(
                    self.app.view(),
                    self.size,
                    std::mem::take(&mut self.cache),
                    &mut self.renderer,
                );
                ui.update(&[event], self.cursor, &mut self.renderer, &mut self.clipboard, &mut messages);
                self.cache = ui.into_cache();
            }
            for m in messages {
                let _ = self.app.update(m);
            }
        }

        fn move_to(&mut self, x: f32, y: f32) {
            let position = Point::new(x, y);
            self.cursor = mouse::Cursor::Available(position);
            self.feed(Event::Mouse(mouse::Event::CursorMoved { position }));
        }

        fn press(&mut self) {
            self.feed(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)));
        }

        fn release(&mut self) {
            self.feed(Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)));
        }

        fn drag(&mut self, from: (f32, f32), to: (f32, f32)) {
            self.move_to(from.0, from.1);
            self.press();
            self.move_to((from.0 + to.0) / 2.0, to.1);
            self.move_to(to.0, to.1);
            self.release();
        }

        /// Bounds of the focusable widget with this id, looked up in the live UI.
        fn bounds_of(&mut self, id: impl Into<iced::advanced::widget::Id>) -> Rectangle {
            let id = id.into();
            use iced::advanced::widget::{Id, Operation, operation::Focusable};
            struct Find {
                id: Id,
                found: Option<Rectangle>,
            }
            impl Operation for Find {
                fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                    operate(self);
                }

                fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
                    if id == Some(&self.id) {
                        self.found = Some(bounds);
                    }
                }
            }
            let mut ui = UserInterface::build(
                self.app.view(),
                self.size,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let mut find = Find { id: id.clone(), found: None };
            ui.operate(&self.renderer, &mut find);
            self.cache = ui.into_cache();
            find.found.unwrap_or_else(|| panic!("no focusable widget with id {id:?}"))
        }

        fn click(&mut self, x: f32, y: f32) {
            self.move_to(x, y);
            self.press();
            self.release();
        }

        /// Click `n` times in quick succession.
        fn click_n(&mut self, x: f32, y: f32, n: usize) {
            self.move_to(x, y);
            for _ in 0..n {
                self.press();
                self.release();
            }
        }

        fn type_text(&mut self, text: &str) {
            for event in iced_test::simulator::typewrite(text) {
                self.feed(event);
            }
        }

        fn key(&mut self, key: impl Into<keyboard::Key>, mods: keyboard::Modifiers) {
            let key = key.into();
            self.feed(Event::Keyboard(keyboard::Event::ModifiersChanged(mods)));
            self.feed(Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
                location: keyboard::Location::Standard,
                modifiers: mods,
                text: None,
                repeat: false,
            }));
            self.feed(Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::default())));
        }

        fn named(&mut self, key: keyboard::key::Named) {
            self.key(key, keyboard::Modifiers::default());
        }

        fn shortcut(&mut self, letter: &str, extra: keyboard::Modifiers) {
            self.key(keyboard::Key::Character(letter.into()), keyboard::Modifiers::COMMAND | extra);
        }

        /// Absolute position of a point inside the editor with this id: `x` across, and `y`
        /// down from the first line's top, past the 16px padding the writer uses.
        fn in_editor(&mut self, id: impl Into<iced::advanced::widget::Id>, pad: f32, x: f32, y: f32) -> (f32, f32) {
            let b = self.bounds_of(id);
            (b.x + pad + x, b.y + pad + y)
        }

        /// Centre of the on-screen text `label` (first match), looked up in the live UI.
        fn center_of(&mut self, label: &str) -> Option<(f32, f32)> {
            self.bounds_of_text(label).map(|b| {
                let c = b.center();
                (c.x, c.y)
            })
        }

        /// Visible bounds of the on-screen text `label` (first match).
        fn bounds_of_text(&mut self, label: &str) -> Option<Rectangle> {
            use iced_test::core::widget::operation::{Operation as _, Outcome, black_box};
            use iced_test::selector::Selector;
            let mut ui = UserInterface::build(
                self.app.view(),
                self.size,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let mut operation = <&str as Selector>::find(label);
            ui.operate(&self.renderer, &mut black_box(&mut operation));
            let found = match operation.finish() {
                Outcome::Some(Some(target)) => target.visible_bounds(),
                _ => None,
            };
            self.cache = ui.into_cache();
            found
        }

        /// Drag across reader text until something is selected; returns where it worked.
        fn select_some_reader_text(&mut self) -> (f32, f32) {
            for y in (150..600).step_by(10) {
                let y = y as f32;
                self.drag((60.0, y), (300.0, y));
                if self.app.bible().text_selection.is_some() {
                    return (300.0, y);
                }
            }
            panic!("no reader text found to select");
        }
    }

    impl Harness {
        /// Single-click over a grid of reader points until `done(app)` holds.
        fn click_scan(&mut self, done: impl Fn(&App) -> bool) -> Option<(f32, f32)> {
            for y in (140..1400).step_by(14) {
                for x in (50..560).step_by(10) {
                    let (x, y) = (x as f32, y as f32);
                    self.cursor = mouse::Cursor::Available(Point::new(x, y));
                    self.press();
                    self.release();
                    if done(&self.app) {
                        return Some((x, y));
                    }
                    // A verse number selects its verse and shows the pen toolbar, which would
                    // swallow later clicks; clear it as a user would.
                    if self.app.bible().text_selection.is_some() {
                        self.feed(iced_test::simulator::press_key(keyboard::key::Named::Escape, None));
                    }
                    // Clicking an inline card starts editing it, which grows it and moves the
                    // text below; undo that so the grid keeps hitting the same places.
                    self.app.bible_mut().editing = None;
                    // A click on a cross-reference link navigates away; come back.
                    if (self.app.bible().book, self.app.bible().chapter_no) != (Book::JOHN, 3) {
                        self.app.load(Book::JOHN, 3);
                    }
                }
            }
            None
        }
    }

    #[test]
    fn in_a_narrow_reader_clicking_a_mark_floats_its_card() {
        // Writer shown: the reader is too narrow for a margin, so cards start hidden.
        let mut h = Harness::new(App::new());
        assert!(!h.app.bible().margin_showing);
        assert!(h.center_of(HEART).is_none(), "no cards until a mark is clicked");

        let (_, y) = h.click_scan(|app| app.bible().popup.is_some()).expect("clicking marked text shows its card");
        let ids = h.app.bible().popup.clone().unwrap();
        let first = &h.app.sh.marks[ids[0]];
        let shown = if first.body.trim().is_empty() { "Add a note…".to_string() } else { first.body.trim().to_string() };
        let (_, cy) = h.center_of(&shown).expect("the card's text is on screen");
        assert!(cy > y, "the card floats under the click (click y={y}, card y={cy})");
        assert!(!h.app.note().path.starts_with("margins/"), "the writer is not involved");

        // From the card, the note can still be opened in the writer.
        let (x, y) = h.center_of("Open").expect("the card offers it");
        h.move_to(x, y);
        h.press();
        h.release();
        assert_eq!(h.app.note().path, first_path(&h.app, ids[0]));
        assert!(h.app.bible().popup.is_none());
    }

    fn first_path(app: &App, index: usize) -> String {
        app.sh.marks[index].path()
    }

    #[test]
    fn editing_a_mark_note_updates_the_mark() {
        let mut app = App::new();
        let path = app.sh.marks[0].path();
        app.open_note(path.clone());
        let edited = app.sh.notes[&path].replace("#F5D90A", "#112233");
        app.sh.notes.insert(path.clone(), edited);
        app.sync_mark(&path);
        assert_eq!(app.sh.marks[0].color, "#112233");
        // Text that no longer parses leaves the mark as it was.
        app.sh.notes.insert(path.clone(), "not a mark any more".into());
        app.sync_mark(&path);
        assert_eq!(app.sh.marks[0].color, "#112233");
    }

    #[test]
    fn overlapping_marks_show_all_their_cards_in_the_popup() {
        let mut app = App::new();
        // A second mark, with a comment, over "loved" inside the seeded "so loved the world".
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        let start = canon[&16].find("loved").unwrap();
        app.bible_mut().text_selection = Some(vec![CanonSlice { verse: 16, start, end: start + 5 }]);
        let _ = app.update(app.b(BibleMsg::ApplyPen(1)));
        app.sh.marks.last_mut().unwrap().body = "The overlapping note.".into();

        let mut h = Harness::new(app);
        let (_, y) = h
            .click_scan(|app| app.bible().popup.as_ref().is_some_and(|p| p.len() == 2))
            .expect("a click on the overlap floats both cards");
        let (_, c1) = h.center_of(HEART).expect("the first mark's card");
        let (_, c2) = h.center_of("The overlapping note.").expect("the second mark's card");
        assert!(c1 > y && c2 > y, "both under the click: click y={y}, cards at {c1} and {c2}");
        assert_ne!(c1, c2, "stacked, not overlapping");
    }

    #[test]
    fn closing_a_popup_finishes_the_edit_like_pressing_done() {
        let mut h = Harness::new(App::new());
        h.click_scan(|app| app.bible().popup.is_some()).expect("a card pops up");
        let index = h.app.bible().popup.as_ref().unwrap()[0];
        let _ = h.app.update(h.app.b(BibleMsg::StartEdit(index)));
        for c in " More.".chars() {
            let _ = h.app.update(h.app.b(BibleMsg::CardAction(EditAction::Insert(c.to_string()))));
        }
        assert_eq!(h.app.bible().editing, Some(index));

        h.cursor = mouse::Cursor::Available(Point::new(700.0, 700.0));
        h.press();
        h.release();
        assert!(h.app.bible().popup.is_none() && h.app.bible().editing.is_none(), "closing the card ends the edit");
        assert!(h.app.sh.marks[index].body.trim_end().ends_with(" More."), "what was typed is kept");
    }

    #[test]
    fn in_a_narrow_reader_a_new_comment_gets_a_card_under_the_selection() {
        let mut h = Harness::new(App::new());
        let marks = h.app.sh.marks.len();
        h.select_some_reader_text();
        let (x, y) = h.center_of("Comment").expect("the pen toolbar offers Comment");
        h.move_to(x, y);
        h.press();
        h.release();

        assert_eq!(h.app.sh.marks.len(), marks + 1);
        assert!(h.app.bible().editing.is_some(), "the new comment is being written");
        assert!(h.app.bible().text_selection.is_some(), "the selection stays: the card hangs under it");
        assert!(h.center_of("Delete").is_some(), "its card is on screen to type in");
        assert!(h.center_of("Promise").is_none(), "instead of the pen toolbar");

        // Closing it (Esc) with nothing written discards the empty comment.
        h.feed(iced_test::simulator::press_key(keyboard::key::Named::Escape, None));
        assert!(h.app.bible().editing.is_none());
        assert_eq!(h.app.sh.marks.len(), marks, "an empty comment is not kept");
        assert!(h.center_of("Delete").is_none(), "and the card is gone");
    }

    #[test]
    fn clicking_elsewhere_dismisses_the_popup() {
        let mut h = Harness::new(App::new());
        h.click_scan(|app| app.bible().popup.is_some()).expect("a card pops up");
        let note_before = h.app.note().path.clone();
        h.cursor = mouse::Cursor::Available(Point::new(700.0, 700.0));
        h.press();
        h.release();
        assert!(h.app.bible().popup.is_none());
        assert_eq!(h.app.note().path, note_before, "dismissing opens nothing");

        // Escape dismisses it too.
        h.click_scan(|app| app.bible().popup.is_some()).expect("a card pops up again");
        h.feed(iced_test::simulator::press_key(keyboard::key::Named::Escape, None));
        assert!(h.app.bible().popup.is_none());
    }

    #[test]
    fn clicking_a_verse_number_selects_that_verses_text_and_offers_the_toolbar() {
        let mut h = Harness::new(App::new());
        assert!(h.app.bible().text_selection.is_none());
        // Only a verse number turns a plain click into a selection, so scanning finds one.
        h.click_scan(|app| app.bible().text_selection.is_some()).expect("a verse number selects its verse");

        let canon = h.app.bible().chapter.as_ref().unwrap().canonical_verses();
        let slices: Vec<margin::Slice> = h
            .app
            .bible()
            .text_selection
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| margin::Slice { verse: s.verse, start: s.start, end: s.end })
            .collect();
        assert_eq!(slices.len(), 1);
        assert!(margin::whole_verses(&canon, &slices).is_some(), "the whole verse is selected");
        assert!(h.center_of("Promise").is_some(), "the pen toolbar appears for it");
    }

    #[test]
    fn a_pen_on_a_verse_number_selection_makes_a_verse_level_mark() {
        let mut h = Harness::new(App::new());
        let before = h.app.sh.marks.len();
        h.click_scan(|app| app.bible().text_selection.is_some()).expect("verse selected");
        let (px, py) = h.center_of("Prayer").expect("toolbar");
        h.move_to(px, py);
        h.press();
        h.release();

        assert_eq!(h.app.sh.marks.len(), before + 1);
        let mark = h.app.sh.marks.last().unwrap();
        assert!(mark.is_verse_level(), "no quote: tied to the reference");
        assert!(mark.module.is_empty(), "no module: shows in every module");
        assert!(!h.app.sh.notes[&mark.path()].contains("quote:"));
    }

    #[test]
    fn verse_level_marks_paint_whole_verses_whatever_the_module() {
        let mut app = blank_app();
        select_verses(&mut app, &[4]);
        let _ = app.update(app.b(BibleMsg::ApplyPen(0)));
        assert_eq!(app.sh.marks.len(), 1);
        assert!(app.sh.marks[0].is_verse_level());

        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        let draws = app.mark_draws();
        assert_eq!(draws.len(), 1);
        assert_eq!((draws[0].slice.verse, draws[0].slice.start, draws[0].slice.end), (4, 0, canon[&4].len()));

        // Even a module name that does not match: it belongs to the reference.
        app.sh.marks[0].module = "ASV".into();
        assert_eq!(app.mark_draws().len(), 1);
        // A partial selection still makes a word-level mark in the current module.
        select_so_loved(&mut app);
        let _ = app.update(app.b(BibleMsg::ApplyPen(1)));
        assert!(!app.sh.marks[1].is_verse_level());
        assert_eq!(app.sh.marks[1].module, "KJV");
    }

    #[test]
    fn deleting_a_margin_note_removes_the_mark_and_the_file() {
        let mut app = App::new();
        let marks = app.sh.marks.len();
        let path = app.sh.marks[0].path();
        app.open_note(path.clone());
        let _ = app.update(app.n(NoteMsg::ConfirmDelete));
        assert_eq!(app.sh.marks.len(), marks - 1);
        assert!(!app.sh.notes.contains_key(&path));
        assert!(!app.note().path.starts_with("margins/"), "the writer moves to an ordinary note");
        assert!(app.mark_draws().iter().all(|d| d.mark < app.sh.marks.len()));
    }

    #[test]
    fn unlinking_a_margin_note_keeps_a_plain_note_and_drops_the_highlight() {
        let mut app = App::new();
        let marks = app.sh.marks.len();
        let path = app.sh.marks[0].path();
        let quote = app.sh.marks[0].quote.clone();
        app.open_note(path.clone());
        // Unlinking is offered on the mark's card (a Bible tab action); the note tab follows.
        let _ = app.update(app.b(BibleMsg::UnlinkMarkAt(0)));

        assert_eq!(app.sh.marks.len(), marks - 1, "no longer a mark");
        assert!(!app.sh.notes.contains_key(&path));
        assert!(app.note().path.starts_with("notes/"), "{}", app.note().path);
        let text = &app.sh.notes[&app.note().path];
        assert!(text.contains("[[John 3:16]]") && text.contains(&format!("> {quote}")), "{text}");
        assert_eq!(&app.note().doc.source(), text);
    }

    #[test]
    fn a_margin_note_has_a_trash_button_in_the_header_that_asks_before_deleting() {
        let mut h = Harness::new(App::new());
        let marks = h.app.sh.marks.len();
        assert!(h.center_of("Delete").is_none(), "an ordinary note has no delete");
        h.app.open_note(h.app.sh.marks[0].path());
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Unlink from text").is_none(), "that button is gone");
        // The trash is the last control of the header bar, at the right of the note pane.
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::AskDelete));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Delete this margin note?").is_some(), "a confirmation appears");
        assert_eq!(h.app.sh.marks.len(), marks, "nothing is deleted yet");
        // Cancel keeps it.
        let (x, y) = h.center_of("Cancel").expect("a cancel button");
        h.click(x, y);
        assert!(!h.app.note().confirm_delete && h.app.sh.marks.len() == marks);
        // Asking again and confirming deletes.
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::AskDelete));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let (x, y) = h.center_of("Delete").expect("the confirm button");
        h.click(x, y);
        assert_eq!(h.app.sh.marks.len(), marks - 1);
        assert!(!h.app.note().confirm_delete);
    }

    #[test]
    fn clicking_outside_the_confirmation_cancels_it() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::AskDelete));
        assert!(app.note().confirm_delete, "any note can be deleted");
        let mut h = Harness::new(app);
        h.click(700.0, 520.0);
        assert!(!h.app.note().confirm_delete, "a click away from the dialog cancels");
        assert!(h.app.sh.notes.contains_key(&h.app.note().path));
    }

    #[test]
    fn deleting_an_ordinary_note_asks_by_name_removes_the_file_and_moves_tabs_on() {
        let mut app = App::new();
        app.add_note(1);
        let (a, b) = (app.panes[1].tabs[0], app.panes[1].tabs[1]);
        let path = match &app.tabs[&a] {
            Tab::Note(n) => n.path.clone(),
            _ => unreachable!(),
        };
        let _ = app.update(Message::Note(a, NoteMsg::AskDelete));
        let mut h = Harness::new(app);
        let name = path.trim_end_matches(".md").rsplit('/').next().unwrap().to_string();
        assert!(h.center_of(&format!("Delete \u{201C}{name}\u{201D}?")).is_some(), "the dialog names the note");
        let (x, y) = h.center_of("Delete").expect("the confirm button");
        h.click(x, y);
        assert!(!h.app.sh.notes.contains_key(&path), "the file is gone");
        for id in [a, b] {
            let Some(Tab::Note(n)) = h.app.tabs.get(&id) else { unreachable!() };
            assert_ne!(n.path, path, "tabs showing it moved to another note");
            assert!(h.app.sh.notes.contains_key(&n.path));
        }
        assert!(h.app.sh.marks.len() >= 5, "no marks were harmed");
    }

    #[test]
    fn deleting_the_last_note_leaves_a_fresh_empty_one_to_write_in() {
        let mut app = App::new();
        let ordinary: Vec<String> = app.sh.notes.keys().filter(|p| !p.starts_with("margins/")).cloned().collect();
        for path in &ordinary {
            app.sh.delete_note(path);
        }
        app.settle();
        assert_eq!(app.note().path, "notes/Untitled.md");
        assert_eq!(app.sh.notes["notes/Untitled.md"], "");
        assert!(ordinary.iter().all(|p| !app.sh.notes.contains_key(p)));
    }

    #[test]
    fn the_trash_deletes_the_file_from_the_vault_on_disk() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let path = app.note().path.clone();
        assert!(app.persist_ref().unwrap().vault().read(&path).is_ok());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::AskDelete));
        let _ = app.update(Message::Note(id, NoteMsg::ConfirmDelete));
        assert!(app.persist_ref().unwrap().vault().read(&path).is_err(), "removed from disk");
    }

    const HEART: &str = "The heart of the gospel. Learn this one by heart.";
    const ETERNAL: &str = "Eternal life begins now, not only after death. Compare [[John 17:3]].";

    #[test]
    fn a_wide_reader_shows_a_margin_with_every_card() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        assert!(h.app.bible().margin_showing, "the reader reports that it has a margin");
        let (x1, _) = h.center_of(HEART).expect("a card for the first mark");
        // Right of the 640px text column (the reader starts at x=32), always visible.
        assert!(x1 > 32.0 + 640.0, "card at x={x1}");
        // Two marks on one verse make one stack: the top one is built and shown, the other is
        // underneath (and is not built at all, being out of sight).
        assert!(h.center_of(ETERNAL).is_none(), "the second mark's card is under the first");
        assert!(h.app.cards().iter().filter(|c| c.key < FOOTNOTE_KEY).count() >= 2);
    }

    #[test]
    fn a_narrow_reader_has_no_margin_and_no_cards() {
        let mut h = Harness::new(App::new());
        assert!(!h.app.bible().margin_showing);
        for body in [HEART, ETERNAL] {
            assert!(h.center_of(body).is_none(), "{body:?} should be hidden");
        }
    }

    #[test]
    fn clicking_a_card_edits_it_in_place() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        let (x, y) = h.center_of(HEART).unwrap();
        h.move_to(x, y);
        h.press();
        h.release();
        let index = h.app.bible().editing.expect("clicking the card starts editing it");
        assert!(h.app.sh.marks[index].body.starts_with("The heart of the gospel"));
        assert_eq!(h.app.bible().card_doc.source(), h.app.sh.marks[index].body);
        assert!(h.center_of("Delete").is_some(), "the card offers its actions while editing");
        assert!(h.center_of("Done").is_none(), "there is no Done: clicking away finishes");
    }

    #[test]
    fn typing_in_a_card_updates_the_mark_and_its_file() {
        let mut app = App::new();
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        for c in " Amen.".chars() {
            let _ = app.update(app.b(BibleMsg::CardAction(EditAction::Insert(c.to_string()))));
        }
        let path = app.sh.marks[0].path();
        assert!(app.sh.marks[0].body.contains(" Amen."), "{:?}", app.sh.marks[0].body);
        assert!(app.sh.notes[&path].contains(" Amen."));
        // The file still parses back to the same mark.
        let id = app.sh.marks[0].id.clone();
        assert_eq!(Mark::from_markdown(&id, &app.sh.notes[&path]).as_ref(), Some(&app.sh.marks[0]));
    }

    #[test]
    fn with_a_margin_a_click_on_marked_text_edits_its_card() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let before = app.note().path.clone();
        let mut h = Harness::new(app);
        h.click_scan(|app| app.bible().editing.is_some()).expect("a click on marked text edits its card");
        assert!(h.app.bible().popup.is_none(), "no popup: the card is already in the margin");
        assert_eq!(h.app.note().path, before, "the writer is not involved");
    }

    #[test]
    fn comment_adds_a_note_only_mark_and_opens_its_card() {
        let mut app = blank_app();
        app.bible_mut().margin_showing = true; // with a margin the selection is spent; see the narrow test
        select_so_loved(&mut app);
        let _ = app.update(app.b(BibleMsg::AddComment));
        let index = app.bible().editing.expect("a new comment is being written");
        let mark = &app.sh.marks[index];
        assert_eq!(mark.style, MarkStyle::None);
        assert!(mark.body.is_empty() && app.bible().card_doc.source().is_empty());
        assert!(app.bible().text_selection.is_none(), "the selection is spent");
        // Its card exists even though it is still empty, so there is somewhere to type.
        assert_eq!(app.cards().iter().filter(|c| c.key < FOOTNOTE_KEY).count(), 1);
        // Drawn only as a faint line.
        assert_eq!(app.mark_draws()[0].kind, MarkKind::Note);

        // Typing something keeps it...
        let _ = app.update(app.b(BibleMsg::CardAction(EditAction::Insert('x'.to_string()))));
        app.finish_editing();
        assert_eq!(app.sh.marks.len(), 1);
        assert!(app.bible().editing.is_none());
    }

    #[test]
    fn an_empty_comment_is_discarded_when_editing_stops() {
        let mut app = blank_app();
        select_so_loved(&mut app);
        let _ = app.update(app.b(BibleMsg::AddComment));
        assert_eq!(app.sh.marks.len(), 1);
        let path = app.sh.marks[0].path();
        app.finish_editing();
        assert!(app.sh.marks.is_empty(), "nothing written, nothing kept");
        assert!(!app.sh.notes.contains_key(&path));
        // A highlight with no text is a different thing and stays.
        select_so_loved(&mut app);
        let _ = app.update(app.b(BibleMsg::ApplyPen(0)));
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        app.finish_editing();
        assert_eq!(app.sh.marks.len(), 1);
    }

    #[test]
    fn delete_and_unlink_from_a_card() {
        let mut app = App::new();
        let total = app.sh.marks.len();
        let _ = app.update(app.b(BibleMsg::DeleteMarkAt(0)));
        assert_eq!(app.sh.marks.len(), total - 1);
        assert!(app.bible().editing.is_none());
        let _ = app.update(app.b(BibleMsg::UnlinkMarkAt(0)));
        assert_eq!(app.sh.marks.len(), total - 2);
        assert!(app.sh.notes.keys().any(|p| p.starts_with("notes/John 3.")), "the unlinked note stays");
    }

    #[test]
    fn a_long_card_being_edited_is_capped_too() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.sh.marks[0].body = "line of text\n".repeat(60);
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        let mut h = Harness::new(app);
        let (_, header) = h.center_of("“so loved the world”").unwrap();
        let (_, actions) = h.center_of("Delete").expect("the action row is still reachable");
        // header + editor (capped) + action row + padding: nowhere near sixty lines.
        assert!(actions - header < CARD_BODY_MAX + 140.0, "the actions are {}px below the header", actions - header);
    }

    #[test]
    fn hiding_the_writer_widens_the_reader_enough_for_a_margin() {
        let mut split = Harness::new(App::new());
        assert!(split.center_of(NOTE_PANE_MARK).is_some(), "the writer is there by default");
        assert!(!split.app.bible().margin_showing);

        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut alone = Harness::new(app);
        assert!(alone.center_of(NOTE_PANE_MARK).is_none(), "no writer pane");
        assert!(alone.app.bible().margin_showing, "the freed width becomes a margin");

        // And back: showing the writer again takes the margin away.
        let first = alone.app.panes[0].id;
        let _ = alone.app.update(Message::ToggleZoom(first));
        alone.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(!alone.app.bible().margin_showing);
    }

    #[test]
    fn double_click_then_drag_selects_whole_words() {
        let mut h = Harness::new(App::new());
        let (_, y) = h.select_some_reader_text();
        // Clear the exploratory selection, then double-click and drag along the same line.
        h.move_to(700.0, 700.0);
        h.press();
        h.release();
        h.move_to(100.0, y);
        h.press();
        h.release();
        h.press();
        h.move_to(200.0, y);
        h.move_to(300.0, y);
        h.release();

        let sel = h.app.bible().text_selection.clone().expect("selection after double-click drag");
        let canon = h.app.bible().chapter.as_ref().unwrap().canonical_verses();
        let s = sel[0];
        let text = &canon[&s.verse];
        let picked = &text[s.start..s.end];
        assert!(picked.contains(' '), "drag after a double-click spans several words: {picked:?}");
        let word = |c: char| c.is_alphanumeric() || c == '\'';
        let before = text[..s.start].chars().next_back();
        let after = text[s.end..].chars().next();
        assert!(before.is_none_or(|c| !word(c)), "starts at a word boundary: {text:?} {s:?}");
        assert!(after.is_none_or(|c| !word(c)), "ends at a word boundary: {text:?} {s:?}");
    }

    #[test]
    fn dragging_selects_text_and_the_pen_buttons_then_apply_it() {
        let mut h = Harness::new(App::new());
        let before = h.app.sh.marks.len();
        assert!(h.app.bible().text_selection.is_none());

        assert!(h.center_of("Promise").is_none(), "no toolbar without a selection");

        let (_, line_y) = h.select_some_reader_text();
        let sel = h.app.bible().text_selection.clone().expect("drag reports a selection");
        assert!(!sel.is_empty());

        // The toolbar appears directly under the selected line (and not far from it).
        let (px, py) = h.center_of("Promise").expect("pen toolbar appears once text is selected");
        assert!(py > line_y && py < line_y + 90.0, "toolbar at y={py}, selection line at y={line_y}");

        // Clicking a pen must apply to the still-present selection.
        h.move_to(px, py);
        h.press();
        assert!(h.app.bible().text_selection.is_some(), "pressing a pen must not clear the selection");
        h.release();
        assert_eq!(h.app.sh.marks.len(), before + 1, "pen click adds a mark");
        // With no margin the new mark's card opens under the selection, for a note, like Comment.
        assert!(h.app.bible().editing.is_some(), "the mark's card is open to write in");
        assert!(h.app.bible().text_selection.is_some(), "the selection stays, for the card to sit under");
        assert!(h.center_of("Promise").is_none(), "the card takes the toolbar's place");
    }

    #[test]
    fn selection_becomes_runs_and_a_label() {
        let mut app = App::new();
        select_verses(&mut app, &[1, 2, 3, 5, 7, 8]);
        let refs: Vec<String> = app.selected_refs().iter().map(ToString::to_string).collect();
        assert_eq!(refs, ["John 3:1-3", "John 3:5", "John 3:7-8"]);
        assert_eq!(app.selection_label(), "John 3:1-3, 5, 7-8");
    }

    #[test]
    fn copy_reference_and_quote_make_text_to_paste_into_a_note() {
        let mut app = App::new();
        select_verses(&mut app, &[3, 4, 5, 8]);
        let refs = app.selected_refs();
        let module = app.module_abbreviation();
        assert_eq!(copy_text(&refs, &module, false), "[[John 3:3-5]], [[John 3:8]]");
        assert_eq!(copy_text(&refs, &module, true), "![[John 3:3-5@KJV]]\n\n![[John 3:8@KJV]]");
        assert_eq!(copy_text(&[], &module, false), "");
        // The messages exist and, with a selection, ask for something to be done (a clipboard write).
        let (task, _) = {
            let id = app.bid();
            let Some(Tab::Bible(t)) = app.tabs.get_mut(&id) else { unreachable!() };
            t.update(&mut app.sh, BibleMsg::CopyReference)
        };
        assert!(task.units() > 0, "a selection is put on the clipboard");
    }

    /// Text only the note pane shows, to tell whether that pane is on screen.
    const NOTE_PANE_MARK: &str = "Properties";

    #[test]
    fn the_selection_toolbar_offers_copy_reference_and_copy_quote() {
        let mut h = Harness::new(App::new());
        h.click_scan(|a| a.bible().text_selection.is_some()).expect("a verse number selects its verse");
        // Icons, with their names as the hover text.
        assert!(h.center_of("Promise").is_some(), "a pen is its name");
        assert!(h.center_of("Insert reference").is_none(), "the note tab no longer has insert buttons");
    }

    #[test]
    fn copying_closes_the_toolbar_and_shows_a_toast_that_goes_away() {
        for quote in [false, true] {
            let mut h = Harness::new(App::new());
            h.click_scan(|a| a.bible().text_selection.is_some()).expect("a verse number selects its verse");
            // Let anything the click set going settle, as it would before a person's next click.
            h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            let id = h.app.bid();
            let message = if quote { BibleMsg::CopyQuote } else { BibleMsg::CopyReference };
            let _ = h.app.update(Message::Bible(id, message));
            assert!(h.app.bible().text_selection.is_none(), "the selection, and so the toolbar, is put away");
            h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            let text = h.app.sh.toast.as_ref().expect("a toast").text.clone();
            assert!(text.starts_with(if quote { "Copied quote" } else { "Copied [[" }), "{text}");
            assert!(h.bounds_of_text(&text).is_some(), "it is on screen");
            assert!(h.center_of("Promise").is_none(), "the pens are gone with the toolbar");
            // It stays a moment, then goes.
            let _ = h.app.update(Message::Tick(std::time::Instant::now()));
            assert!(h.app.sh.toast.is_some(), "not yet");
            let later = std::time::Instant::now() + shared::TOAST_FOR + std::time::Duration::from_millis(50);
            let _ = h.app.update(Message::Tick(later));
            assert!(h.app.sh.toast.is_none());
        }
    }

    #[test]
    fn copying_with_nothing_selected_does_nothing() {
        let mut app = App::new();
        let id = app.bid();
        let Some(Tab::Bible(t)) = app.tabs.get_mut(&id) else { unreachable!() };
        let (task, _) = t.update(&mut app.sh, BibleMsg::CopyQuote);
        assert_eq!(task.units(), 0);
    }

    /// A selection of "so loved" in John 3:16, as the reader would report it.
    fn select_so_loved(app: &mut App) {
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        let start = canon[&16].find("so loved").unwrap();
        app.bible_mut().text_selection = Some(vec![CanonSlice { verse: 16, start, end: start + 8 }]);
    }

    #[test]
    fn pen_makes_a_mark_that_resolves_back_to_the_selection() {
        let mut app = blank_app();
        select_so_loved(&mut app);
        app.bible_mut().margin_showing = true; // the card is in the margin: the selection is spent
        let token = app.bible().clear_token;
        let _ = app.update(app.b(BibleMsg::ApplyPen(0)));

        assert_eq!(app.sh.marks.len(), 1);
        let m = &app.sh.marks[0];
        assert_eq!((m.quote.as_str(), m.color.as_str(), m.module.as_str()), ("so loved", "#F5D90A", "KJV"));
        assert_eq!(m.tags, ["promise"]);
        assert_eq!(m.prefix, "For God ");
        assert!(m.path().starts_with("margins/John/03/16-highlight-"), "{}", m.path());
        assert!(app.bible().text_selection.is_none() && app.bible().clear_token == token + 1);
        assert_eq!(app.bible().editing, Some(0), "the new mark's card is open to write a note in");
        assert!(app.sh.marks[0].to_markdown().contains("quote: \"so loved\""));

        let draws = app.mark_draws();
        assert_eq!(draws.len(), 1);
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        let s = draws[0].slice;
        assert_eq!(&canon[&16][s.start..s.end], "so loved");
        assert_eq!(draws[0].kind, MarkKind::Highlight);
    }

    #[test]
    fn marks_only_paint_in_their_own_chapter_and_module() {
        let mut app = blank_app();
        select_so_loved(&mut app);
        let _ = app.update(app.b(BibleMsg::ApplyPen(1)));
        assert_eq!(app.mark_draws()[0].kind, MarkKind::Underline);

        let _ = app.update(app.b(BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        assert!(app.mark_draws().is_empty());
        let _ = app.update(app.b(BibleMsg::PickChapter(ChapterChoice(Book::JOHN, 3))));
        assert_eq!(app.mark_draws().len(), 1);

        app.sh.marks[0].module = "ASV".into();
        assert!(app.mark_draws().is_empty(), "word-level marks stay with their module");
    }

    #[test]
    fn pens_without_a_selection_do_nothing() {
        let mut app = blank_app();
        let _ = app.update(app.b(BibleMsg::ApplyPen(0)));
        assert!(app.sh.marks.is_empty());
    }


    #[test]
    fn verses_that_notes_link_to_are_marked_in_the_reader() {
        let app = App::new();
        // Nicodemus links John 3:2 and John 3:3-5; Memory verses links John 3:16.
        let annotated = app.annotated_verses();
        assert!(annotated.contains(&2) && annotated.contains(&4) && annotated.contains(&16));
        assert!(!annotated.contains(&7));
    }

    #[test]
    fn show_in_reader_switches_chapter_marks_and_requests_reveal() {
        let mut app = App::new();
        let _ = app.update(app.n(NoteMsg::OpenLink(LinkTarget::Verse {
            range: RefRange::parse("Ps 23:2-3").unwrap(),
            module: None,
        })));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::PSALMS, 23));
        assert_eq!(app.bible().select_request.map(|(_, a, z)| (a, z)), Some((2, 3)));
        let (first, verse) = app.bible().reveal.unwrap();
        assert_eq!(verse, 2);

        // Asking again for the same verse needs a fresh token, or the widget would ignore it.
        let _ = app.update(app.n(NoteMsg::OpenLink(LinkTarget::Verse {
            range: RefRange::parse("Ps 23:2-3").unwrap(),
            module: None,
        })));
        assert_ne!(app.bible().reveal.unwrap().0, first);

        // A chapter that is not available leaves everything alone and says so.
        let _ = app.update(app.n(NoteMsg::OpenLink(LinkTarget::Verse {
            range: RefRange::parse("Gen 1:1").unwrap(),
            module: None,
        })));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::PSALMS, 23));
        assert!(app.sh.status.contains("not in this module"));
    }

    // ---- the live editor, end to end ----------------------------------------------

    use iced::keyboard::Modifiers;
    use iced::keyboard::key::Named;

    fn app_with_writer(text: &str) -> App {
        let mut app = App::new();
        app.note_mut().doc = Document::new(text);
        app
    }

    /// Focus the writer by clicking in its first line.
    fn focus_writer(h: &mut Harness) {
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 30.0, 10.0);
        h.click(x, y);
    }

    #[test]
    fn typing_in_the_writer_edits_the_document_and_the_note() {
        let mut h = Harness::new(app_with_writer(""));
        h.type_text("lost");
        assert_eq!(h.app.note().doc.source(), "", "an unfocused editor ignores the keyboard");
        focus_writer(&mut h);
        h.type_text("hello world");
        assert_eq!(h.app.note().doc.source(), "hello world");
        assert_eq!(h.app.sh.notes[&h.app.note().path], "hello world", "and the note is kept in step");
    }

    #[test]
    fn escape_lets_go_of_the_keyboard() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("a");
        h.named(Named::Escape);
        h.type_text("b");
        assert_eq!(h.app.note().doc.source(), "a");
    }

    #[test]
    fn enter_continues_a_list_and_ends_it_on_an_empty_item() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("- one");
        h.named(Named::Enter);
        h.type_text("two");
        assert_eq!(h.app.note().doc.source(), "- one\n- two");
        h.named(Named::Enter);
        h.named(Named::Enter);
        assert_eq!(h.app.note().doc.source(), "- one\n- two\n");
    }

    #[test]
    fn arrows_backspace_and_delete_work_like_any_editor() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("abc");
        h.named(Named::ArrowLeft);
        h.named(Named::ArrowLeft);
        h.named(Named::Backspace);
        assert_eq!(h.app.note().doc.source(), "bc");
        h.named(Named::Delete);
        assert_eq!(h.app.note().doc.source(), "c");
        h.key(Named::ArrowLeft, Modifiers::SHIFT);
        assert_eq!(h.app.note().doc.selection().range(), 0..0, "nothing before the start to select");
        h.key(Named::ArrowRight, Modifiers::SHIFT);
        assert_eq!(h.app.note().doc.selected_text().as_deref(), Some("c"));
    }

    #[test]
    fn undo_redo_select_all_and_bold_shortcuts() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("hello world");
        h.shortcut("z", Modifiers::default());
        assert_eq!(h.app.note().doc.source(), "hello ");
        h.shortcut("z", Modifiers::SHIFT);
        assert_eq!(h.app.note().doc.source(), "hello world", "Shift-undo redoes");
        h.shortcut("a", Modifiers::default());
        h.shortcut("b", Modifiers::default());
        assert_eq!(h.app.note().doc.source(), "**hello world**");
        h.shortcut("z", Modifiers::default());
        assert_eq!(h.app.note().doc.source(), "hello world");
    }

    #[test]
    fn copy_cut_and_paste_go_through_the_clipboard() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("copy me");
        h.shortcut("a", Modifiers::default());
        h.shortcut("c", Modifiers::default());
        assert_eq!(h.clipboard.0.as_deref(), Some("copy me"));
        h.named(Named::ArrowRight);
        h.shortcut("v", Modifiers::default());
        assert_eq!(h.app.note().doc.source(), "copy mecopy me");
        h.shortcut("a", Modifiers::default());
        h.shortcut("x", Modifiers::default());
        assert_eq!(h.app.note().doc.source(), "");
        assert_eq!(h.clipboard.0.as_deref(), Some("copy mecopy me"));
    }

    #[test]
    fn input_method_text_is_inserted_where_the_caret_is() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("caf");
        h.feed(Event::InputMethod(iced::advanced::input_method::Event::Commit("é".into())));
        assert_eq!(h.app.note().doc.source(), "café");
    }

    #[test]
    fn dragging_and_multi_clicking_select_text() {
        let mut h = Harness::new(app_with_writer("alpha beta gamma\nsecond"));
        let (x0, y) = h.in_editor(h.app.note().editor_id(), 16.0, 1.0, 10.0);
        let (x1, _) = h.in_editor(h.app.note().editor_id(), 16.0, 70.0, 10.0);
        h.drag((x0, y), (x1, y));
        let dragged = h.app.note().doc.selected_text().expect("a drag selects");
        assert!("alpha beta gamma".starts_with(&dragged) && dragged.len() >= 5, "{dragged:?}");

        let (bx, by) = h.in_editor(h.app.note().editor_id(), 16.0, 72.0, 10.0);
        h.click_n(bx, by, 2);
        assert_eq!(h.app.note().doc.selected_text().as_deref(), Some("beta"), "double-click selects the word");
        // Elsewhere first, so the next clicks start a fresh count rather than continuing this one.
        h.click(bx + 200.0, by);
        h.click_n(bx, by, 3);
        assert_eq!(h.app.note().doc.selected_text().as_deref(), Some("alpha beta gamma"), "triple-click the line");
    }

    #[test]
    fn only_the_line_with_the_caret_shows_its_markdown() {
        let mut h = Harness::new(app_with_writer("# Title\nbody"));
        assert_eq!(h.app.note().doc.revealed_block(), Some(1), "the caret starts on the last line");
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 30.0, 12.0);
        h.click(x, y);
        assert_eq!(h.app.note().doc.revealed_block(), Some(0), "clicking the heading reveals its raw source");
        assert!(h.app.note().doc.selection().head <= 7);
    }

    #[test]
    fn up_and_down_move_between_lines_keeping_the_column() {
        let mut h = Harness::new(app_with_writer("a long first line here\nx\nanother long line here"));
        let b = { let id = h.app.note().editor_id(); h.bounds_of(id) };
        h.click(b.x + b.width - 30.0, b.y + 16.0 + 12.0);
        let first_end = "a long first line here".len();
        assert_eq!(h.app.note().doc.selection().head, first_end, "a click far right lands at the line's end");
        h.named(Named::ArrowDown);
        assert_eq!(h.app.note().doc.block_index_at(h.app.note().doc.selection().head), 1);
        h.named(Named::ArrowDown);
        let head = h.app.note().doc.selection().head;
        assert_eq!(h.app.note().doc.block_index_at(head), 2);
        assert!(head >= h.app.note().doc.source().len() - 4, "the remembered column carries across the short line");
        h.named(Named::ArrowUp);
        h.named(Named::ArrowUp);
        assert_eq!(h.app.note().doc.block_index_at(h.app.note().doc.selection().head), 0);
        h.named(Named::ArrowUp);
        assert_eq!(h.app.note().doc.selection().head, 0, "Up from the first line goes to the start");
    }

    #[test]
    fn clicking_a_rendered_checkbox_ticks_it() {
        let mut h = Harness::new(app_with_writer("- [ ] todo\nlast"));
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 10.0, 12.0);
        h.click(x, y);
        assert_eq!(h.app.note().doc.source(), "- [x] todo\nlast");
        // It is a normal edit, so it undoes like one.
        h.shortcut("z", Modifiers::default());
        assert_eq!(h.app.note().doc.source(), "- [ ] todo\nlast");
    }

    /// A writer showing "see Nicodemus now" (the link rendered, the caret on the line below).
    fn writer_with_link() -> Harness {
        Harness::new(app_with_writer("see [[Nicodemus]] now\nlast"))
    }

    fn with_modifiers(h: &mut Harness, mods: Modifiers, then: impl FnOnce(&mut Harness)) {
        h.feed(Event::Keyboard(keyboard::Event::ModifiersChanged(mods)));
        then(h);
        h.feed(Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::default())));
    }

    #[test]
    fn in_the_writer_a_plain_click_on_a_link_follows_it() {
        let mut h = writer_with_link();
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 70.0, 12.0);
        h.click(x, y);
        assert_eq!(h.app.note().path, "notes/Nicodemus.md");
        assert!(h.app.note().doc.source().contains("Nicodemus"), "the writer now holds that note");

        // A verse link shows the passage in the reader and leaves the writer's text and caret alone.
        let mut h = Harness::new(app_with_writer("see [[Ps 23:1]] now\nlast"));
        let end = h.app.note().doc.selection().head;
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 60.0, 12.0);
        h.click(x, y);
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::PSALMS, 23));
        assert_eq!(h.app.note().doc.selection().head, end, "following does not move the caret first");
        assert_eq!(h.app.note().doc.source(), "see [[Ps 23:1]] now\nlast");
    }

    #[test]
    fn alt_click_edits_a_link_instead_of_following_it() {
        let mut h = writer_with_link();
        let before = h.app.note().path.clone();
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 70.0, 12.0);
        with_modifiers(&mut h, Modifiers::ALT, |h| h.click(x, y));
        assert_eq!(h.app.note().path, before, "no navigation");
        let head = h.app.note().doc.selection().head;
        assert!((4..=17).contains(&head), "the caret is inside the link: {head}");
        // Now it shows its raw source, ready to edit.
        let shown = md_editor::render(h.app.note().doc.source(), &h.app.note().doc.blocks()[0], &h.app.note().doc.reveal(0));
        assert_eq!(shown.text, "see [[Nicodemus]] now");

        // And while it is showing raw, a plain click on it is editing too, not following.
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 95.0, 12.0);
        h.click(x, y);
        assert_eq!(h.app.note().path, before);
        assert!((4..=17).contains(&h.app.note().doc.selection().head));
    }

    #[test]
    fn arrow_keys_reach_a_link_to_edit_it_and_leaving_it_makes_it_followable_again() {
        let mut h = writer_with_link();
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 6.0, 12.0);
        h.click(x, y);
        for _ in 0..8 {
            h.named(Named::ArrowRight);
        }
        let shown = md_editor::render(h.app.note().doc.source(), &h.app.note().doc.blocks()[0], &h.app.note().doc.reveal(0));
        assert!(shown.text.contains("[[Nicodemus]]"), "the caret reached the link: {:?}", shown.text);
        h.named(Named::ArrowDown);
        let shown = md_editor::render(h.app.note().doc.source(), &h.app.note().doc.blocks()[0], &h.app.note().doc.reveal(0));
        assert_eq!(shown.text, "see Nicodemus now", "away from it, it is rendered again");
    }

    #[test]
    fn dragging_from_a_link_in_a_note_carries_the_link_rather_than_selecting_text() {
        let mut h = writer_with_link();
        let before = h.app.note().path.clone();
        let (x0, y) = h.in_editor(h.app.note().editor_id(), 16.0, 50.0, 12.0);
        let (x1, _) = h.in_editor(h.app.note().editor_id(), 16.0, 130.0, 12.0);
        h.drag((x0, y), (x1, y));
        assert_eq!(h.app.note().path, before, "it was not followed");
        assert!(h.app.note().doc.selection().is_empty(), "and nothing was selected");
        assert!(matches!(&h.app.drag, Some(Drag { payload: Payload::Link(_), .. })), "the link is being carried");
    }

    #[test]
    fn cmd_click_follows_a_link_even_while_it_is_being_edited() {
        let mut h = writer_with_link();
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 70.0, 12.0);
        with_modifiers(&mut h, Modifiers::ALT, |h| h.click(x, y));
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 95.0, 12.0);
        with_modifiers(&mut h, Modifiers::COMMAND, |h| h.click(x, y));
        assert_eq!(h.app.note().path, "notes/Nicodemus.md");
    }

    #[test]
    fn a_card_that_is_being_edited_edits_on_click_where_a_resting_card_follows() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.sh.marks[0].body = "[[Nicodemus]] and more".into();
        let before = app.note().path.clone();
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        let mut h = Harness::new(app);
        let b = { let id = h.app.bible().card_editor_id(); h.bounds_of(id) };
        h.click(b.x + 30.0, b.y + 14.0);
        assert_eq!(h.app.note().path, before, "editing: a click puts the caret there, it does not navigate");
        assert_eq!(h.app.bible().editing, Some(0));
        // Cmd-click still follows, for when you do want to.
        h.feed(Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)));
        h.click(b.x + 60.0, b.y + 14.0);
        assert_eq!(h.app.note().path, "notes/Nicodemus.md");
    }

    #[test]
    fn an_embed_line_shows_the_passage_and_pushes_the_text_below_it_down() {
        let mut h = Harness::new(app_with_writer("![[John 3:16]]\nlast line"));
        assert!(h.app.embed_for(&LinkTarget::Verse { range: RefRange::parse("John 3:16").unwrap(), module: None }).is_some());
        // Without the passage, "last line" would start at the top. With it, that spot is the
        // embed (belonging to line 0), and the last line starts lower.
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 30.0, 60.0);
        h.click(x, y);
        assert_eq!(h.app.note().doc.revealed_block(), Some(0), "the body of the passage belongs to line 0");
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 30.0, 220.0);
        h.click(x, y);
        assert_eq!(h.app.note().doc.revealed_block(), Some(1), "the last line is below it");
    }

    #[test]
    fn the_editor_scrolls_to_keep_the_caret_in_view_and_with_the_wheel() {
        let text: String = (0..120).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut h = Harness::new(app_with_writer(&text));
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 30.0, 10.0);
        h.click(x, y);
        h.key(Named::ArrowDown, Modifiers::COMMAND);
        assert_eq!(h.app.note().doc.selection().head, text.len());
        // The caret is at the end, so the end of the document must be on screen.
        let b = { let id = h.app.note().editor_id(); h.bounds_of(id) };
        h.click(b.x + 40.0, b.y + b.height - 30.0);
        assert!(h.app.note().doc.selection().head + 40 > text.len(), "head {} of {}", h.app.note().doc.selection().head, text.len());

        // Scroll back up with the wheel and click the top: now an early line.
        h.move_to(b.x + 40.0, b.y + 40.0);
        for _ in 0..40 {
            h.feed(Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Lines { x: 0.0, y: 5.0 } }));
        }
        h.click(b.x + 40.0, b.y + 30.0);
        assert!(h.app.note().doc.selection().head < 60, "head {}", h.app.note().doc.selection().head);
    }

    #[test]
    fn a_card_edits_with_the_same_editor() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        let before = app.sh.marks[0].body.clone();
        let mut h = Harness::new(app);
        let b = { let id = h.app.bible().card_editor_id(); h.bounds_of(id) };
        h.click(b.x + 20.0, b.y + 10.0);
        h.type_text(" Amen.");
        h.key(Named::ArrowDown, Modifiers::COMMAND);
        h.type_text("!");
        assert!(h.app.sh.marks[0].body.starts_with(before.trim_end()) || h.app.sh.marks[0].body.contains("Amen"), "{:?}", h.app.sh.marks[0].body);
        assert!(h.app.sh.marks[0].body.contains(" Amen."), "{:?}", h.app.sh.marks[0].body);
        assert!(h.app.sh.marks[0].body.ends_with('!'));
        let path = h.app.sh.marks[0].path();
        assert!(h.app.sh.notes[&path].contains("Amen."), "the mark's file follows every keystroke");
    }

    #[test]
    fn a_click_whose_line_reflows_does_not_turn_into_a_selection() {
        // Clicking a heading reveals its `## `, which moves the text under the still pointer.
        let mut h = Harness::new(app_with_writer("## Title\nbody"));
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 60.0, 12.0);
        h.move_to(x, y);
        h.press();
        // The pointer has not moved, but the runtime may still report it (and jitter by a pixel).
        h.move_to(x, y);
        h.move_to(x + 1.0, y);
        h.release();
        assert!(h.app.note().doc.selection().is_empty(), "selection: {:?}", h.app.note().doc.selection());
        assert_eq!(h.app.note().doc.revealed_block(), Some(0));

        // A real drag still selects.
        let (x2, _) = h.in_editor(h.app.note().editor_id(), 16.0, 140.0, 12.0);
        h.drag((x, y), (x2, y));
        assert!(!h.app.note().doc.selection().is_empty());
    }

    #[test]
    fn clicking_plain_text_does_not_make_the_rest_of_the_line_jump() {
        // A line with bold in it, caret elsewhere so the line is rendered ("ab bold cd ef gh").
        let mut h = Harness::new(app_with_writer("ab **bold** cd ef gh\nlast"));
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 95.0, 6.0);
        h.click(x, y);
        let first = h.app.note().doc.selection().head;
        // Same column, lower in the same line (far enough from the first click not to be a
        // double-click). If the first click had re-flowed the line, this would land elsewhere.
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 95.0, 20.0);
        h.click(x, y);
        assert_eq!(h.app.note().doc.selection().head, first, "the text stayed where it was");
        assert!(h.app.note().doc.reveal(0).selection.is_some() && !h.app.note().doc.reveal(0).line);
    }

    #[test]
    fn clicking_inside_bold_shows_only_that_elements_markers() {
        let mut h = Harness::new(app_with_writer("ab **bold** cd\nlast"));
        // "bold" is the third visual word; the line is rendered, so it starts at "ab " (~25px).
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 40.0, 10.0);
        h.click(x, y);
        let head = h.app.note().doc.selection().head;
        assert!((3..=9).contains(&head), "the click landed in the bold element: {head}");
        let shown = md_editor::render(
            h.app.note().doc.source(),
            &h.app.note().doc.blocks()[0],
            &h.app.note().doc.reveal(0),
        );
        assert_eq!(shown.text, "ab **bold** cd");
        // Moving the caret out of it puts the markers away again.
        h.named(Named::End);
        let shown = md_editor::render(
            h.app.note().doc.source(),
            &h.app.note().doc.blocks()[0],
            &h.app.note().doc.reveal(0),
        );
        assert_eq!(shown.text, "ab bold cd");
    }

    #[test]
    fn a_resting_card_shows_its_markdown_rendered_and_a_plain_click_follows_a_link() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.sh.marks[0].body = "[[Nicodemus]] and **bold**".into();
        let before = app.note().path.clone();
        let mut h = Harness::new(app);
        let b = h.bounds_of_text("[[Nicodemus]] and **bold**").expect("the card shows its comment");
        // Rendered, the link is just its label at the start of the line: a plain click follows it.
        h.click(b.x + 20.0, b.y + 12.0);
        assert_eq!(h.app.note().path, "notes/Nicodemus.md");
        assert_ne!(before, h.app.note().path);
        assert!(h.app.bible().editing.is_none(), "following a link is not editing");
        // Anywhere else in the card (the far end of the first line) starts editing it.
        // Following the link brought the note's pane into view; put it away again so the
        // reader has its margin back.
        assert!(h.app.zoomed.is_none(), "a followed link shows where it went");
        h.app.zoomed = Some(h.app.panes[0].id);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let b = h.bounds_of_text("[[Nicodemus]] and **bold**").expect("the card is back");
        h.click(b.x + b.width - 10.0, b.y + 12.0);
        assert_eq!(h.app.bible().editing, Some(0));
    }

    #[test]
    fn a_task_in_a_resting_card_ticks_without_starting_an_edit() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.sh.marks[0].body = "- [ ] read it\n- [x] pray".into();
        let path = app.sh.marks[0].path();
        let mut h = Harness::new(app);
        let b = h.bounds_of_text("- [ ] read it\n- [x] pray").expect("the card shows its comment");
        // The box sits in the gutter left of the item's text.
        h.click(b.x + 8.0, b.y + 12.0);
        assert!(h.app.sh.marks[0].body.starts_with("- [x] read it"), "{:?}", h.app.sh.marks[0].body);
        assert!(h.app.sh.notes[&path].contains("- [x] read it"), "the mark's file follows");
        assert!(h.app.bible().editing.is_none());
    }

    #[test]
    fn typing_goes_nowhere_while_a_card_is_only_being_read() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let before = app.sh.marks[0].body.clone();
        let mut h = Harness::new(app);
        h.type_text("zzz");
        assert_eq!(h.app.sh.marks[0].body, before);
        assert!(h.app.bible().editing.is_none());
    }

    #[test]
    fn double_clicking_a_checkbox_never_selects_a_word() {
        let mut h = Harness::new(app_with_writer("- [ ] todo item\nlast"));
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 10.0, 12.0);
        h.click_n(x, y, 2);
        assert!(h.app.note().doc.selection().is_empty(), "selection: {:?}", h.app.note().doc.selection());
        // Each click ticks or unticks it, like any checkbox: two clicks, back where it began.
        assert_eq!(h.app.note().doc.source(), "- [ ] todo item\nlast");
        h.click_n(x, y + 40.0, 1);
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 10.0, 12.0);
        h.click(x + 8.0, y);
        assert_eq!(h.app.note().doc.source(), "- [x] todo item\nlast");
        assert!(h.app.note().doc.selection().is_empty());
    }

    #[test]
    fn the_title_of_an_embedded_passage_links_to_its_source() {
        let mut h = Harness::new(app_with_writer("![[John 3:16]]\nlast"));
        let end = h.app.note().doc.selection().head;
        // The first line of the embed is its title ("John 3:16 · KJV").
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 40.0, 14.0);
        h.click(x, y);
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::JOHN, 3));
        assert_eq!(h.app.bible().select_request.map(|(_, a, z)| (a, z)), Some((16, 16)), "the passage is shown in the reader");
        assert_eq!(h.app.note().doc.selection().head, end, "the writer's caret did not move");
        assert_eq!(h.app.note().doc.source(), "![[John 3:16]]\nlast");
    }

    #[test]
    fn an_embed_pinned_to_a_module_we_lack_says_so() {
        let app = App::new();
        let target = |m: Option<&str>| LinkTarget::Verse { range: RefRange::parse("John 3:16").unwrap(), module: m.map(String::from) };
        assert!(app.embed_for(&target(None)).is_some());
        assert!(app.embed_for(&target(Some("kjv"))).is_some(), "module names compare without case");
        assert!(app.embed_for(&target(Some("ASV"))).is_none(), "not installed: the placeholder shows");
        assert_eq!(app.embed_for(&target(Some("KJV"))).unwrap().title, "John 3:16 · KJV");
    }

    fn opened(app: &App) -> bool {
        app.bible().note_card.is_some()
    }

    fn is_word(app: &App) -> bool {
        matches!(app.bible().note_card, Some(NoteCard { content: NoteContent::Word { .. }, .. }))
    }

    /// A wide reader with the writer hidden, so the margin shows.
    fn wide_reader() -> Harness {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        // No notes on the marks, so the footnote has the margin to itself (it would be under a
        // note of the user's, in the stack they make).
        for mark in &mut app.sh.marks {
            mark.body.clear();
        }
        Harness::new(app)
    }

    /// A reader with no room for a margin (the writer takes half the window).
    fn narrow_reader() -> Harness {
        let h = Harness::new(App::new());
        assert!(h.app.zoomed.is_none() && !h.app.bible().margin_showing);
        h
    }

    #[test]
    fn with_a_margin_every_footnote_is_shown_and_none_can_be_closed() {
        let mut h = wide_reader();
        assert!(h.app.bible().margin_showing);
        assert!(h.app.bible().note_card.is_none(), "nothing was clicked");
        let card = h.bounds_of_text("Footnote").expect("John 3:2's footnote is already showing");
        assert!(card.x > 640.0, "in the margin, beside the text column: {}", card.x);
        assert!(h.center_of("✕").is_none(), "no close button");
        // Clicking its marker changes nothing: it is already there.
        assert!(h.click_scan(opened).is_none());
        assert!(h.bounds_of_text("Footnote").is_some());
    }

    #[test]
    fn a_footnote_card_is_level_with_its_marker() {
        let mut h = wide_reader();
        let (_, y) = {
            // Find the marker by where a narrow reader's click lands: same text, same line.
            let mut narrow = narrow_reader();
            narrow.click_scan(opened).expect("a footnote marker in John 3")
        };
        let _ = y;
        let card = h.bounds_of_text("Footnote").unwrap();
        let anchor = h.app.bible().chapter.as_ref().unwrap().footnotes();
        assert_eq!((anchor.len(), anchor[0].verse), (1, 2));
        assert!(card.y > 100.0 && card.y < 700.0, "beside verse 2, near the top of the chapter: {}", card.y);
    }

    #[test]
    fn footnotes_are_anchored_on_the_text_before_the_marker() {
        let canon: margin::CanonicalText = [(2, "by night é".to_string()), (3, "next".to_string()), (5, String::new())].into();
        let note = |verse, offset| study_core::ast::FootnoteAt { verse, offset, text: String::new() };
        let slice = |verse, start, end| Some(CanonSlice { verse, start, end });
        assert_eq!(footnote_anchor(&canon, &note(2, 11)), slice(2, 9, 11), "the last character, whole");
        assert_eq!(footnote_anchor(&canon, &note(2, 8)), slice(2, 7, 8));
        assert_eq!(footnote_anchor(&canon, &note(3, 0)), slice(3, 0, 1), "at a verse's start, the first character");
        assert_eq!(footnote_anchor(&canon, &note(4, 0)), slice(3, 3, 4), "a verse with no text borrows the one before");
        assert_eq!(footnote_anchor(&canon, &note(5, 0)), slice(3, 3, 4));
        assert_eq!(footnote_anchor(&canon, &note(1, 0)), None, "nothing before it to sit beside");
    }

    #[test]
    fn without_a_margin_clicking_a_footnote_opens_a_floating_card_that_can_be_closed() {
        let mut h = narrow_reader();
        let (_, y) = h.click_scan(opened).expect("a footnote marker in John 3");
        let Some(NoteCard { content: NoteContent::Footnote(note), .. }) = &h.app.bible().note_card else {
            panic!("expected a footnote card");
        };
        assert!(note.contains("by night"), "{note}");
        let card = h.bounds_of_text("Footnote").expect("floating under the click");
        assert!(card.y > y - 4.0, "under the line it came from: {} vs {y}", card.y);
        let (cx, cy) = h.center_of("✕").expect("a close button");
        h.click(cx, cy);
        assert!(h.app.bible().note_card.is_none(), "the close button dismisses it");
    }

    #[test]
    fn a_floating_footnote_card_closes_on_escape_or_a_click_in_the_text() {
        let mut h = narrow_reader();
        h.click_scan(opened).unwrap();
        h.feed(iced_test::simulator::press_key(keyboard::key::Named::Escape, None));
        assert!(h.app.bible().note_card.is_none());
        h.click_scan(opened).unwrap();
        h.click(60.0, 1300.0);
        assert!(h.app.bible().note_card.is_none());
    }

    #[test]
    fn clicking_inside_a_floating_card_keeps_it_open() {
        let mut h = narrow_reader();
        h.click_scan(opened).unwrap();
        let card = h.bounds_of_text("Footnote").unwrap();
        h.click(card.x + 30.0, card.y + 4.0);
        assert!(h.app.bible().note_card.is_some());
    }

    #[test]
    fn a_footnote_popup_gives_way_when_the_window_grows_a_margin() {
        let mut app = App::new();
        app.open_note_card(Some(CanonSlice { verse: 2, start: 0, end: 3 }), NoteContent::Footnote("x".into()));
        let _ = app.update(app.b(BibleMsg::MarginChanged(true)));
        assert!(app.bible().note_card.is_none());
        // A dictionary card has no such twin in the margin, so it stays.
        app.open_note_card(Some(CanonSlice { verse: 2, start: 0, end: 3 }), NoteContent::Word { key: "G25".into(), entry: None });
        let _ = app.update(app.b(BibleMsg::MarginChanged(true)));
        assert!(app.bible().note_card.is_some());
    }

    #[test]
    fn a_reference_in_a_footnote_is_a_link_that_shows_that_verse_in_the_reader() {
        let mut h = narrow_reader();
        h.click_scan(opened).unwrap();
        let card = h.bounds_of_text("Footnote").expect("the card");
        assert!(h.app.bible().select_request.is_none());
        // Find the reference by clicking along the card's body lines.
        'scan: for dy in (22..110).step_by(8) {
            for dx in (10..340).step_by(10) {
                h.click(card.x + dx as f32, card.y + dy as f32);
                if h.app.bible().select_request.is_some() {
                    break 'scan;
                }
                if h.app.bible().note_card.is_none() {
                    h.click_scan(opened).unwrap();
                }
            }
        }
        let (_, first, last) = h.app.bible().select_request.expect("clicking the reference showed its verse");
        assert_eq!((first, last), (16, 16));
    }

    #[test]
    fn a_strongs_word_pops_its_dictionary_entry_over_the_text_when_not_kept_in_the_margin() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        app.bible_mut().in_margin.strongs = false;
        let mut h = Harness::new(app);
        assert!(h.app.bible().margin_showing);
        let cards_before = h.app.cards().len();
        let (_, y) = h.click_scan(is_word).expect("a Strong's word in John 3");
        assert_eq!(h.app.cards().len(), cards_before, "chosen out of the margin, it takes no place in it");
        let Some(NoteCard { content: NoteContent::Word { key, entry }, .. }) = &h.app.bible().note_card else { unreachable!() };
        let entry = entry.clone().unwrap_or_else(|| panic!("the mock has an entry for {key}"));
        let label = format!("Strong's {key}");
        let card = h.bounds_of_text(&label).expect("the pop-over header");
        assert!(card.y > y - 4.0, "under the line of the word: card {} word {y}", card.y);
        assert!(h.center_of(&entry.lemma).is_some(), "the lemma is shown");
        assert!(h.center_of(&entry.transliteration).is_some(), "and its transliteration");
        // It closes on a click elsewhere, or Escape, as in a narrow reader.
        h.feed(iced_test::simulator::press_key(keyboard::key::Named::Escape, None));
        assert!(h.app.bible().note_card.is_none());
    }

    #[test]
    fn a_strongs_word_inside_a_mark_offers_both_its_entry_and_the_mark() {
        for narrow in [true, false] {
            let mut app = App::new();
            if !narrow {
                app.zoomed = Some(app.panes[0].id);
            }
            app.bible_mut().show_strongs = true;
            app.bible_mut().in_margin.strongs = false;
            // Mark all of verse 16, where the Strong's words are.
            select_verses(&mut app, &[16]);
            let _ = app.update(app.b(BibleMsg::ApplyPen(0)));
            let marks = app.sh.marks.len();
            let new = marks - 1;
            let mut h = Harness::new(app);
            let both = |a: &App| is_word(a) && a.bible().popup.as_ref().is_some_and(|p| p.contains(&new));
            h.click_scan(both).unwrap_or_else(|| panic!("a click on a marked Strong's word shows both (narrow: {narrow})"));
            // Both are on screen: the entry's header and the mark's card ("Unlink" is on popped-up cards).
            // (With a margin the mark's card is in it, not popped up, so there is nothing to find here.)
            assert!(narrow == h.bounds_of_text("Unlink").is_some(), "the mark's card is offered when popped up (narrow: {narrow})");
            assert!(h.app.bible().editing.is_none(), "a lookup does not start an edit");
            // A click elsewhere closes both.
            h.click(60.0, 1300.0);
            assert!(h.app.bible().note_card.is_none() && h.app.bible().popup.is_none());
        }
    }

    #[test]
    fn a_dictionary_popover_closes_on_a_click_in_the_text_and_by_its_button() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        app.bible_mut().in_margin.strongs = false;
        let mut h = Harness::new(app);
        h.click_scan(is_word).unwrap();
        let (cx, cy) = h.center_of("✕").expect("a close button");
        h.click(cx, cy);
        assert!(h.app.bible().note_card.is_none());
        h.click_scan(is_word).unwrap();
        h.click(60.0, 1300.0);
        assert!(h.app.bible().note_card.is_none(), "a click elsewhere in the reader");
    }

    #[test]
    fn a_number_no_dictionary_knows_still_gets_a_popover_saying_so() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        let mut h = Harness::new(app);
        h.click_scan(is_word).unwrap();
        if let Some(card) = h.app.bible_mut().note_card.as_mut() {
            card.content = NoteContent::Word { key: "G99999".into(), entry: None };
        }
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Strong's G99999").is_some());
        assert!(h.bounds_of_text("No installed dictionary has an entry for this number.").is_some());
    }

    #[test]
    fn in_a_reader_with_no_margin_the_card_floats_and_escape_closes_it() {
        let mut h = Harness::new(App::new());
        assert!(h.app.zoomed.is_none());
        let (x, y) = h.click_scan(opened).expect("a footnote marker");
        assert!(!h.app.bible().margin_showing);
        let card = h.bounds_of_text("Footnote").expect("floating under the click");
        assert!(card.y > y - 4.0, "under the line it came from: {} vs {y}", card.y);
        let _ = x;
        h.feed(iced_test::simulator::press_key(keyboard::key::Named::Escape, None));
        assert!(h.app.bible().note_card.is_none());
    }

    #[test]
    fn opening_a_mark_card_or_another_chapter_dismisses_the_note_card() {
        let mut app = App::new();
        app.open_note_card(Some(CanonSlice { verse: 2, start: 0, end: 3 }), NoteContent::Footnote("x".into()));
        let _ = app.update(app.b(BibleMsg::MarksClicked(vec![0])));
        assert!(app.bible().note_card.is_none());
        app.open_note_card(Some(CanonSlice { verse: 2, start: 0, end: 3 }), NoteContent::Footnote("x".into()));
        app.load(Book::PSALMS, 23);
        assert!(app.bible().note_card.is_none());
        // A link whose place in the text is unknown opens nothing.
        app.open_note_card(None, NoteContent::Footnote("x".into()));
        assert!(app.bible().note_card.is_none());
    }

    /// How wide `element` lays out when offered `max` pixels.
    fn laid_width<M>(h: &Harness, element: Element<'_, M>, max: f32) -> f32 {
        use iced::advanced::{layout::Limits, widget::Tree};
        let mut element = element;
        let mut tree = Tree::new(element.as_widget());
        element
            .as_widget_mut()
            .layout(&mut tree, &h.renderer, &Limits::new(Size::ZERO, Size::new(max, f32::INFINITY)))
            .size()
            .width
    }

    #[test]
    fn margin_cards_are_only_as_wide_as_their_text() {
        let h = Harness::new(App::new());
        let mut app = App::new();
        app.sh.marks[0].body = "Short.".into();
        let short = laid_width(&h, app.card_view(0, false), 380.0);
        assert!(short < 200.0, "a short comment makes a narrow card: {short}");
        app.sh.marks[0].body = "A much longer comment that keeps going well past the width of the column, so that it has to wrap onto several lines instead.".into();
        let long = laid_width(&h, app.card_view(0, false), 380.0);
        assert_eq!(long, 380.0, "a long one uses the whole column");
        assert_eq!(laid_width(&h, app.card_view(0, true), 380.0), 380.0, "a floating popup takes its width");
        app.sh.marks[0].body = "Short.".into();
        app.bible_mut().editing = Some(0);
        assert_eq!(laid_width(&h, app.card_view(0, false), 380.0), 380.0, "an editor needs room to type");
    }

    #[test]
    fn footnote_and_dictionary_cards_are_only_as_wide_as_their_text_too() {
        let h = Harness::new(App::new());
        let app = App::new();
        let short = laid_width(&h, app.reference_card("Footnote", text("Hi.").into(), false, false), 380.0);
        assert!(short < 120.0, "{short}");
        let filled = laid_width(&h, app.reference_card("Footnote", text("Hi.").into(), false, true), 380.0);
        assert_eq!(filled, 380.0);
    }

    // ---- panes and tabs --------------------------------------------------------------

    fn bibles(app: &App) -> Vec<TabId> {
        app.tabs.iter().filter(|(_, t)| matches!(t, Tab::Bible(_))).map(|(id, _)| *id).collect()
    }

    fn notes_tabs(app: &App) -> Vec<TabId> {
        app.tabs.iter().filter(|(_, t)| matches!(t, Tab::Note(_))).map(|(id, _)| *id).collect()
    }

    fn bible_of(app: &App, id: TabId) -> &BibleTab {
        app.bible_tab(id).unwrap()
    }

    #[test]
    fn it_starts_with_a_bible_tab_on_the_left_and_a_note_on_the_right() {
        let app = App::new();
        assert_eq!(app.panes.len(), 2);
        assert_eq!(app.panes[0].tabs, bibles(&app));
        assert_eq!(app.panes[1].tabs, notes_tabs(&app));
        assert_eq!(app.panes[0].active, app.panes[0].tabs.first().copied());
        assert!(app.zoomed.is_none());
        assert!(app.note().path.starts_with("notes/"));
    }

    #[test]
    fn the_tab_bar_shows_each_tab_by_name_with_a_new_tab_button_after_the_last() {
        let mut h = Harness::new(App::new());
        assert!(h.center_of("John 3 · KJV").is_some(), "the Bible tab is named by what it reads");
        assert!(h.center_of(&h.app.note().title(&h.app.sh)).is_some());
        let first = h.app.panes[0].id;
        let (x, y) = h.center_of("+").expect("a new tab button");
        h.click(x, y);
        assert_eq!(h.app.panes[0].tabs.len(), 2);
        let blank = *h.app.panes[0].tabs.last().unwrap();
        assert!(matches!(h.app.tabs[&blank], Tab::Blank(_)), "a new tab starts blank");
        assert_eq!(h.app.panes[0].active, Some(blank));
        assert_eq!(h.app.panes[1].tabs.len(), 1, "the other pane is untouched");
        assert!(h.center_of("New tab").is_some());
        let _ = first;
    }

    #[test]
    fn a_blank_tab_becomes_what_is_chosen_and_keeps_its_place() {
        let mut app = App::new();
        let pane = app.panes[0].id;
        let _ = app.update(Message::NewTab(pane));
        let blank = *app.panes[0].tabs.last().unwrap();
        let choices = app.choices();
        assert!(choices.iter().any(|c| matches!(c, Choice::Bible(..))) && choices.iter().any(|c| matches!(c, Choice::Note(_))));
        assert!(choices.iter().all(|c| !matches!(c, Choice::Note(p) if p.starts_with("margins/"))), "margin files are not offered");
        let _ = app.update(Message::Open(blank, Choice::Note("notes/Memory verses.md".into())));
        assert!(matches!(&app.tabs[&blank], Tab::Note(n) if n.path == "notes/Memory verses.md"));
        assert_eq!(app.panes[0].tabs.last(), Some(&blank), "same tab, same place in the bar");
        // Choosing again does nothing: it is no longer blank.
        let _ = app.update(Message::Open(blank, Choice::Note("notes/Nicodemus.md".into())));
        assert!(matches!(&app.tabs[&blank], Tab::Note(n) if n.path == "notes/Memory verses.md"));
        // A Bible opens where the other is reading.
        let _ = app.update(Message::NewTab(pane));
        let second = *app.panes[0].tabs.last().unwrap();
        let _ = app.update(Message::Bible(app.bid(), BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        let module = app.first_module();
        let _ = app.update(Message::Open(second, Choice::Bible(module, "KJV".into())));
        let b = app.bible_tab(second).unwrap();
        assert_eq!((b.book, b.chapter_no), (Book::PSALMS, 23));
    }

    #[test]
    fn splitting_a_pane_adds_a_blank_one_beside_it_and_dividers_move() {
        let mut app = App::new();
        let left = app.panes[0].id;
        let _ = app.update(Message::SplitPane(left, panes::Axis::Column));
        assert_eq!(app.panes.len(), 3);
        let new = app.panes.last().unwrap();
        assert_eq!(new.tabs.len(), 1);
        assert!(matches!(app.tabs[&new.tabs[0]], Tab::Blank(_)), "the new pane starts blank");
        assert_eq!(app.focus, new.id);
        assert_eq!(app.layout.leaves().len(), 3);
        // The left half was split again: its two halves stack, the one inside the first split.
        let panes::Node::Split { a, .. } = &app.layout else { unreachable!() };
        assert!(matches!(&**a, panes::Node::Split { axis: panes::Axis::Column, .. }));
        let panes::Node::Split { id, .. } = &app.layout else { unreachable!() };
        let id = *id;
        let _ = app.update(Message::ResizeSplit(id, 0.3));
        assert!(matches!(&app.layout, panes::Node::Split { fraction, .. } if (*fraction - 0.3).abs() < 1e-6));
    }

    #[test]
    fn closing_the_last_tab_of_a_pane_closes_the_pane_and_the_neighbour_takes_its_space() {
        let mut app = App::new();
        let note = app.nid();
        let _ = app.update(Message::CloseTab(note));
        assert_eq!(app.panes.len(), 1);
        assert_eq!(app.layout.leaves(), [app.panes[0].id]);
        // The only pane keeps a blank tab rather than vanishing.
        let bible = app.bid();
        let _ = app.update(Message::CloseTab(bible));
        assert_eq!(app.panes.len(), 1);
        assert!(matches!(app.tabs[&app.panes[0].tabs[0]], Tab::Blank(_)));
    }

    #[test]
    fn a_zoomed_pane_fills_the_window_and_zooming_again_restores_the_rest() {
        let mut h = Harness::new(App::new());
        let first = h.app.panes[0].id;
        assert!(h.center_of(&h.app.note().title(&h.app.sh)).is_some());
        let _ = h.app.update(Message::ToggleZoom(first));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of(NOTE_PANE_MARK).is_none(), "the other pane is out of the way");
        let _ = h.app.update(Message::ToggleZoom(first));
        assert!(h.app.zoomed.is_none());
    }

    #[test]
    fn dragging_a_divider_resizes_the_panes_either_side() {
        let mut h = Harness::new(App::new());
        let before = match &h.app.layout {
            panes::Node::Split { fraction, .. } => *fraction,
            _ => unreachable!(),
        };
        assert_eq!(before, 0.5);
        // The divider is in the middle of the 1024 wide window, below the top bar.
        h.move_to(512.0, 400.0);
        h.press();
        h.move_to(400.0, 400.0);
        h.move_to(300.0, 400.0);
        h.release();
        let after = match &h.app.layout {
            panes::Node::Split { fraction, .. } => *fraction,
            _ => unreachable!(),
        };
        assert!((after - 300.0 / 1024.0).abs() < 0.03, "the divider followed the pointer: {after}");
        // And the reader got narrower, so the writer got wider.
        let reader = h.app.bible().margin_showing;
        assert!(!reader);
    }


    #[test]
    fn each_bible_tab_keeps_its_own_place_and_settings() {
        let mut app = App::new();
        app.add_bible(0);
        let (first, second) = (app.panes[0].tabs[0], app.panes[0].tabs[1]);
        assert_ne!(bible_of(&app, first).reader_id(), bible_of(&app, second).reader_id(), "no shared widget ids");
        let _ = app.update(Message::Bible(second, BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        let _ = app.update(Message::Bible(second, BibleMsg::ToggleStrongs(true)));
        let (a, b) = (bible_of(&app, first), bible_of(&app, second));
        assert_eq!((a.book, a.chapter_no, a.show_strongs), (Book::JOHN, 3, false));
        assert_eq!((b.book, b.chapter_no, b.show_strongs), (Book::PSALMS, 23, true));
    }

    #[test]
    fn closing_tabs_shows_a_neighbour() {
        let mut app = App::new();
        app.add_bible(0);
        app.add_bible(0);
        let ids = app.panes[0].tabs.clone();
        assert_eq!(ids.len(), 3);
        let _ = app.update(Message::TabPressed(ids[1]));
        let _ = app.update(Message::CloseTab(ids[1]));
        assert_eq!(app.panes[0].tabs, [ids[0], ids[2]]);
        assert_eq!(app.panes[0].active, Some(ids[2]), "the tab that took its place");
        let _ = app.update(Message::CloseTab(ids[2]));
        assert_eq!(app.panes[0].active, Some(ids[0]));
    }

    #[test]
    fn a_notes_verse_link_shows_in_the_bible_tab_last_used() {
        let mut app = App::new();
        app.add_bible(0);
        let (first, second) = (app.panes[0].tabs[0], app.panes[0].tabs[1]);
        // Use the first tab, then follow a link in the note.
        let _ = app.update(Message::Bible(first, BibleMsg::ToggleStrongs(false)));
        let target = LinkTarget::Verse { range: RefRange::parse("Num 21:9").unwrap(), module: None };
        let note = app.nid();
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(target)));
        assert_eq!(bible_of(&app, first).book, Book::NUMBERS, "the last used tab took it");
        assert_eq!(bible_of(&app, second).book, Book::JOHN);
        assert_eq!(app.panes[0].active, Some(first));
    }

    #[test]
    fn with_no_bible_tab_a_verse_link_opens_one() {
        let mut app = App::new();
        let id = app.bid();
        let _ = app.update(Message::CloseTab(id));
        assert!(bibles(&app).is_empty());
        let note = app.nid();
        let target = LinkTarget::Verse { range: RefRange::parse("Ps 23:1").unwrap(), module: None };
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(target)));
        assert_eq!(bibles(&app).len(), 1);
        assert_eq!(app.bible().book, Book::PSALMS);
        assert_eq!(app.pane_of(app.bid()), Some(0), "on the left");
    }


    #[test]
    fn zooming_a_pane_keeps_the_others_tabs_and_selecting_one_brings_it_back() {
        let mut app = App::new();
        let note = app.nid();
        let first = app.panes[0].id;
        let _ = app.update(Message::ToggleZoom(first));
        assert!(app.zoomed == Some(first) && app.tabs.contains_key(&note));
        let _ = app.update(Message::TabPressed(note));
        assert!(app.zoomed.is_none(), "asking for a tab in a hidden pane shows it");
    }

    #[test]
    fn two_note_tabs_hold_one_document_each_and_follow_a_shared_note() {
        let mut app = App::new();
        app.add_note(1);
        let (a, b) = (app.panes[1].tabs[0], app.panes[1].tabs[1]);
        let Some(Tab::Note(first)) = app.tabs.get(&a) else { unreachable!() };
        let path = first.path.clone();
        // The new tab opened the same note; typing in one shows in the other.
        let _ = app.update(Message::Note(a, NoteMsg::Action(EditAction::Paste("HELLO ".into()))));
        let Some(Tab::Note(second)) = app.tabs.get(&b) else { unreachable!() };
        assert_eq!(second.path, path);
        assert!(second.doc.source().contains("HELLO "));
        // Pointing one at a different note leaves the other alone.
        let other = app.sh.note_paths().into_iter().find(|p| p != &path && !p.starts_with("margins/")).unwrap();
        let _ = app.update(Message::Note(b, NoteMsg::PickNote(NoteChoice(other.clone()))));
        let Some(Tab::Note(first)) = app.tabs.get(&a) else { unreachable!() };
        assert_eq!(first.path, path);
    }

    #[test]
    fn opening_a_margin_note_reuses_the_note_tab_last_used() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let notes_before = notes_tabs(&app).len();
        let mark_path = app.sh.marks[0].path();
        let bible = app.bid();
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenMarkInWriter(0)));
        assert_eq!(notes_tabs(&app).len(), notes_before, "no new tab");
        assert_eq!(app.note().path, mark_path);
        assert!(app.zoomed.is_none(), "and its pane is shown");
    }

    #[test]
    fn opening_a_note_with_no_note_tab_makes_one() {
        let mut app = App::new();
        let note = app.nid();
        let _ = app.update(Message::CloseTab(note));
        assert!(notes_tabs(&app).is_empty());
        let bible = app.bid();
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenMarkInWriter(0)));
        assert_eq!(notes_tabs(&app).len(), 1);
        assert_eq!(app.pane_of(app.nid()), Some(app.panes.len() - 1), "in the last pane");
    }

    #[test]
    fn deleting_a_mark_clears_its_cards_in_every_bible_tab_and_the_note_showing_it() {
        let mut app = App::new();
        app.add_bible(0);
        let (first, second) = (app.panes[0].tabs[0], app.panes[0].tabs[1]);
        let path = app.sh.marks[0].path();
        let note = app.nid();
        let _ = app.update(Message::Note(note, NoteMsg::PickNote(NoteChoice(path))));
        let _ = app.update(Message::Bible(second, BibleMsg::MarksClicked(vec![1])));
        assert!(bible_of(&app, second).popup.is_some());
        let _ = app.update(Message::Bible(first, BibleMsg::DeleteMarkAt(0)));
        assert!(bible_of(&app, second).popup.is_none(), "the other tab's card numbers moved: it drops them");
        assert!(app.note().path.starts_with("notes/"), "the note tab moved off the deleted margin note");
    }

    #[test]
    #[ignore = "writes a picture for a person to look at"]
    fn snapshot_bold_while_editing() {
        let out = std::env::var("SNAP_OUT").unwrap_or_else(|_| "/tmp/snap.png".into());
        let mut app = App::new();
        let src = "- [ ] What does **born again** mean for Nicodemus, a teacher?\n\nPlain **bold words here** and *italic words* end.\n";
        app.note_mut().doc = Document::new(src);
        let caret = src.find(&std::env::var("SNAP_AT").unwrap_or("again".into())).unwrap() + 2;
        app.note_mut().doc.perform(EditAction::SetCaret { offset: caret, extend: false });
        if let Ok(path) = std::env::var("SNAP_NOTE") {
            let id = app.nid();
            let _ = app.update(Message::Note(id, NoteMsg::PickNote(NoteChoice(path))));
            let _ = app.update(Message::Note(id, NoteMsg::AddProperties));
            if std::env::var("SNAP_PROPS").is_ok() {
                let _ = app.update(Message::Note(id, NoteMsg::ToggleProps));
                for (key, kind) in [("read", study_core::frontmatter::PropType::Bool), ("pages", study_core::frontmatter::PropType::Number), ("with", study_core::frontmatter::PropType::List), ("due", study_core::frontmatter::PropType::Date)] {
                    let _ = app.update(Message::Note(id, NoteMsg::NewKey(key.into())));
                    let _ = app.update(Message::Note(id, NoteMsg::NewType(kind)));
                    let _ = app.update(Message::Note(id, NoteMsg::AddProp));
                }
            }
        }
        if std::env::var("SNAP_DRAG").is_ok() {
            let note = app.nid();
            let right = app.panes[1].id;
            app.start_drag(Payload::Tab(note), p(300.0, 250.0));
            let zone = match std::env::var("SNAP_DRAG").unwrap().as_str() {
                "left" => Zone::Left,
                "bottom" => Zone::Bottom,
                _ => Zone::Center,
            };
            let _ = app.update(Message::DragOver(Over::Pane(right, zone)));
        }
        if std::env::var("SNAP_MENU").is_ok() {
            let id = app.bid();
            let _ = app.update(Message::Bible(id, BibleMsg::ToggleMenu));
        }
        if std::env::var("SNAP_TOAST").is_ok() {
            app.sh.toast("Copied [[John 3:2-5]]");
        }
        if std::env::var("SNAP_MARGIN").is_ok() {
            let p = app.sh.marks[0].path();
            app.open_note(p);
        }
        if std::env::var("SNAP_NOTEMENU").is_ok() {
            let id = app.nid();
            let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        }
        if std::env::var("SNAP_PICKER").is_ok() {
            app.note_mut().picker = Some(crate::note_tab::Picker::default());
        }
        if std::env::var("SNAP_STACK").is_ok() {
            let _ = app.update(Message::Bible(app.bid(), BibleMsg::ToggleMarks(true)));
            let new = app.split_pane(0, panes::Axis::Column);
            let tab = app.new_blank();
            app.add_tab(new, tab);
        }
        if std::env::var("SNAP_ZOOM").is_ok() {
            app.zoomed = Some(app.panes[0].id);
        }
        let mut h = Harness::new(app);
        h.size = Size::new(1024.0, 560.0);
        if let Ok(w) = std::env::var("SNAP_W") {
            h.size = Size::new(w.parse().unwrap(), 800.0);
        }
        if let Ok(count) = std::env::var("SNAP_CROWD") {
            // Many notes on the words of one verse.
            let canon = h.app.bible().chapter.as_ref().unwrap().canonical_verses();
            let text = canon[&16].clone();
            let mut at = 0;
            for n in 0..count.parse::<usize>().unwrap() {
                let Some(found) = text[at..].find(' ') else { break };
                let (start, end) = (at, at + found);
                at = end + 1;
                h.app.bible_mut().text_selection = Some(vec![CanonSlice { verse: 16, start, end }]);
                let _ = h.app.update(h.app.b(BibleMsg::ApplyPen(n % 3)));
                h.app.sh.marks.last_mut().unwrap().body = format!("Note {} on this word. It says a good deal, over a line or two, to make a tall card.", n + 1);
            }
        }
        if let Ok(at) = std::env::var("SNAP_HOVER") {
            let (x, y) = at.split_once(',').unwrap();
            h.move_to(x.parse().unwrap(), y.parse().unwrap());
        }
        if std::env::var("SNAP_MARGINMENU").is_ok() {
            let id = h.app.bid();
            let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMarginMenu));
        }
        if std::env::var("SNAP_NEWKIND").is_ok() {
            let id = h.app.nid();
            let _ = h.app.update(Message::Note(id, NoteMsg::NewNote));
        }
        if std::env::var("SNAP_STRONGSPOP").is_ok() {
            h.app.bible_mut().show_strongs = true;
        }
        if std::env::var("SNAP_STRONGS").is_ok() {
            let id = h.app.bid();
            h.app.bible_mut().show_strongs = true;
            let _ = h.app.update(Message::Bible(id, BibleMsg::SetInMargin(NoteKind::Strongs, true)));
        }
        if let Ok(size) = std::env::var("SNAP_BIBLE") {
            let _ = h.app.update(Message::SetBibleSize(size.parse().unwrap()));
        }
        if let Ok(panel) = std::env::var("SNAP_PANEL") {
            if panel == "layouts" {
                let _ = h.app.update(Message::LayoutDraft("Morning study".into()));
                let _ = h.app.update(Message::SaveLayout);
                if std::env::var("SNAP_NAMING").is_ok() {
                    let _ = h.app.update(Message::NameLayout);
                    let _ = h.app.update(Message::LayoutDraft("Evening".into()));
                }
            }
            let _ = h.app.update(Message::SetBibleSize(24.0));
            let _ = h.app.update(Message::SetNotesSize(20.0));
            let _ = h.app.update(Message::SetFont(FontChoice(Some("Georgia".into()))));
            if panel == "layouts" {
                if !h.app.naming {
                    let _ = h.app.update(Message::TogglePanel(Panel::Layouts));
                }
            } else {
                let _ = h.app.update(Message::OpenSettings);
                h.size = Size::new(360.0, 400.0);
            }
        }
        if std::env::var("SNAP_SELECT").is_ok() {
            h.click_scan(|a| a.bible().text_selection.is_some());
        }
        let theme = h.app.theme();
        let shown = match h.app.settings_window {
            Some(id) => h.app.window_view(id),
            None => h.app.view(),
        };
        let mut ui = UserInterface::build(shown, h.size, std::mem::take(&mut h.cache), &mut h.renderer);
        ui.draw(
            &mut h.renderer,
            &theme,
            &iced::advanced::renderer::Style { text_color: iced::Color::WHITE },
            h.cursor,
        );
        h.cache = ui.into_cache();
        let (w, hh) = (h.size.width as u32, h.size.height as u32);
        let rgba = h.renderer.screenshot(Size::new(w, hh), 1.0, iced::Color::BLACK);
        let file = std::fs::File::create(&out).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, hh);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    }

    // ---- the note selector -----------------------------------------------------------

    #[test]
    fn notes_are_listed_by_name_with_margin_notes_apart_and_labelled_by_their_passage() {
        let app = App::new();
        let none = std::collections::BTreeSet::new();
        let all = crate::note_tab::groups(&app.sh, "", &none);
        let keys: Vec<&str> = all.iter().map(|g| g.key.as_str()).collect();
        assert_eq!(keys, ["notes", "margins"], "no folders: notes, then margin notes (and no templates)");
        assert!(all.iter().flat_map(|g| &g.notes).all(|(p, _)| !p.starts_with("templates/")), "the kinds of note are not found among the notes");
        let labels: Vec<&str> = all[0].notes.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(labels, ["Memory verses", "Nicodemus", "Prayer list"], "sorted by name, shown without folder or .md");
        let margins = &all[1];
        assert_eq!(margins.label, "Margin notes");
        assert!(margins.notes.iter().any(|(_, l)| l.starts_with("John 3:16 · ")), "{:?}", margins.notes);
        assert!(margins.notes.iter().all(|(p, _)| p.starts_with("margins/")));
    }

    #[test]
    fn searching_narrows_by_name_text_or_tag_and_hides_empty_groups() {
        let app = App::new();
        let none = std::collections::BTreeSet::new();
        let find = |q: &str| -> Vec<String> {
            crate::note_tab::groups(&app.sh, q, &none).into_iter().flat_map(|g| g.notes).map(|(p, _)| p).collect()
        };
        assert_eq!(find("nicod")[0], "notes/Nicodemus.md", "a name match comes first");
        assert!(find("nicod").len() > 1, "and notes that mention it follow");
        assert!(find("PRAYER LIST").contains(&"prayers/Prayer list.md".to_string()), "any case");
        assert!(find("bronze serpent").contains(&"notes/Nicodemus.md".to_string()), "by text");
        assert!(find("zzz nothing").is_empty());
        // By tag: the plain word finds a note that has it among its tags, and #word only tags.
        assert!(find("new-birth").contains(&"notes/Nicodemus.md".to_string()));
        let hashed = find("#new-bir");
        assert!(hashed.contains(&"notes/Nicodemus.md".to_string()) && !hashed.contains(&"notes/Memory verses.md".to_string()));
        assert!(find("#nonesuchtag").is_empty());
        let groups = crate::note_tab::groups(&app.sh, "prayer", &none);
        assert!(groups.iter().all(|g| !g.notes.is_empty()));
    }

    #[test]
    fn tags_narrow_the_selector_and_a_tag_includes_the_tags_inside_it() {
        let mut app = App::new();
        app.sh.notes.insert(
            "notes/Grace.md".into(),
            "---\ntitle: Grace\ntags: [topic/grace, john]\n---\n\nText\n".into(),
        );
        let pick = |tags: &[&str]| -> Vec<String> {
            let set = tags.iter().map(|t| t.to_string()).collect();
            crate::note_tab::groups(&app.sh, "", &set).into_iter().filter(|g| g.key == "notes").flat_map(|g| g.notes).map(|(p, _)| p).collect()
        };
        assert_eq!(pick(&["john"]), ["notes/Grace.md", "notes/Nicodemus.md"]);
        assert_eq!(pick(&["topic"]), ["notes/Grace.md"], "topic/grace is under topic");
        assert_eq!(pick(&["john", "topic/grace"]), ["notes/Grace.md"], "all the chosen tags");
        assert!(pick(&["topic/gr"]).is_empty(), "a part of a tag is not the tag");
        // The counts the chips show.
        let counts = app.sh.tag_counts();
        assert!(counts.contains(&("john".to_string(), 2)) && counts.contains(&("topic/grace".to_string(), 1)));
        // Editing a note's tags changes them.
        app.sh.notes.insert("notes/Grace.md".into(), "---\ntitle: Grace\ntags: [other]\n---\n".into());
        assert!(app.sh.tag_counts().contains(&("other".to_string(), 1)));
        assert!(!app.sh.tag_counts().iter().any(|(t, _)| t == "topic/grace"));
    }

    #[test]
    fn the_selector_lists_the_notes_and_tag_chips_and_choosing_a_tag_narrows_it() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::TogglePicker));
        let mut h = Harness::new(app);
        assert!(h.center_of("Margin notes").is_some(), "the margin notes are a group");
        assert!(h.center_of("Prayer list").is_some() && h.center_of("Nicodemus").is_some(), "the notes are just listed");
        assert!(h.center_of("#new-birth").is_some(), "with chips for the tags in use");
        let (x, y) = h.center_of("#new-birth").unwrap();
        h.click(x, y);
        assert!(h.app.note().picker.as_ref().unwrap().tags.contains("new-birth"));
        assert!(h.center_of("Prayer list").is_none(), "only the notes with the tag are left");
        assert!(h.center_of("Nicodemus").is_some());
        let (x, y) = h.center_of("Clear tags").expect("a way to lift it");
        h.click(x, y);
        assert!(h.center_of("Prayer list").is_some());
        // Choosing a note closes the selector.
        let (x, y) = h.center_of("Prayer list").unwrap();
        h.click(x, y);
        assert_eq!(h.app.note().path, "prayers/Prayer list.md");
        assert!(h.app.note().picker.is_none(), "choosing closes the selector");
    }

    #[test]
    fn a_click_in_another_pane_closes_the_selector() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::TogglePicker));
        let mut h = Harness::new(app);
        assert!(h.app.note().picker.is_some(), "the reader's start-up reports do not close it");
        // Plain text in the reader, with nothing there to take the click.
        h.click(60.0, 300.0);
        assert!(h.app.note().picker.is_none(), "a click in the reader puts the pop-up away");
        // So does using something in the other pane.
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::TogglePicker));
        let bible = h.app.bid();
        let _ = h.app.update(Message::Bible(bible, BibleMsg::ToggleStrongs(true)));
        assert!(h.app.note().picker.is_none());
    }

    #[test]
    fn typing_in_the_search_box_filters_and_a_click_elsewhere_closes_the_selector() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::TogglePicker));
        let _ = app.update(Message::Note(id, NoteMsg::PickerQuery("healing".into())));
        let mut h = Harness::new(app);
        assert!(h.center_of("Prayer list").is_some(), "the match is listed, its group open while searching");
        assert!(h.center_of("Margin notes").is_none(), "groups with no match are not");
        h.click(700.0, 520.0);
        assert!(h.app.note().picker.is_none(), "a click outside the panel closes it");
        // Opening it again starts fresh.
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::TogglePicker));
        assert_eq!(h.app.note().picker.as_ref().unwrap().query, "");
    }

    // ---- note properties --------------------------------------------------------------

    #[test]
    fn a_note_opens_with_the_caret_below_its_frontmatter() {
        let mut app = App::new();
        app.open_note("notes/Nicodemus.md".into());
        let source = app.note().doc.source().to_string();
        let end = app.note().doc.front_end();
        assert_eq!(app.note().doc.selection().head, end);
        assert!(source[end..].starts_with("# Nicodemus"), "writing starts in the text, not the properties or the gap under them");
        // A note with none starts at the top as before.
        app.open_note("prayers/Prayer list.md".into());
        assert!(app.note().doc.selection().head > 0 || !app.note().doc.source().starts_with("---"));
    }

    #[test]
    fn tags_are_added_and_removed_in_the_strip_and_the_frontmatter_follows() {
        let mut app = App::new();
        app.open_note("notes/Nicodemus.md".into());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::TagDraft("  #Grace ".into())));
        let _ = app.update(Message::Note(id, NoteMsg::AddTag));
        let text = app.sh.notes["notes/Nicodemus.md"].clone();
        let front = study_core::frontmatter::parse(&text).front;
        assert_eq!(front.tags, ["john", "new-birth", "grace"]);
        assert_eq!(front.title.as_deref(), Some("Notes on Nicodemus"), "the rest is untouched");
        assert!(text.contains("\n# Nicodemus"), "and so is the text");
        assert!(app.note().tag_draft.is_empty(), "the box is cleared");
        // The same tag again, or nothing, adds nothing.
        let _ = app.update(Message::Note(id, NoteMsg::TagDraft("grace".into())));
        let _ = app.update(Message::Note(id, NoteMsg::AddTag));
        assert_eq!(study_core::frontmatter::parse(&app.sh.notes["notes/Nicodemus.md"]).front.tags.len(), 3);
        let _ = app.update(Message::Note(id, NoteMsg::RemoveTag("john".into())));
        assert_eq!(study_core::frontmatter::parse(&app.sh.notes["notes/Nicodemus.md"]).front.tags, ["new-birth", "grace"]);
        // It is a normal edit: undo brings the tag back.
        app.note_mut().doc.perform(EditAction::Undo);
        assert!(app.note().doc.source().contains("john"));
    }

    #[test]
    fn a_note_with_no_created_time_offers_to_stamp_it() {
        let mut app = App::new();
        app.open_note("prayers/Prayer list.md".into());
        let id = app.nid();
        assert!(study_core::frontmatter::parse(app.note().doc.source()).front.created.is_none());
        let mut h = Harness::new(app);
        assert!(h.center_of("No created time").is_some() && h.center_of("Stamp it now").is_some());
        let _ = h.app.update(Message::Note(id, NoteMsg::AddProperties));
        let front = study_core::frontmatter::parse(h.app.note().doc.source()).front;
        assert!(front.created.is_some());
        assert_eq!(front.kind.as_deref(), Some("prayer"), "existing properties are kept");
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Stamp it now").is_none());
        assert!(h.bounds_of_text(&format!("Created {}", study_core::frontmatter::display(&front.created.unwrap()))).is_some());
    }

    #[test]
    fn a_new_note_has_its_properties_and_is_named_so_as_not_to_clash() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::NewNote));
        assert!(app.note().choosing, "it asks what kind first");
        assert_eq!(app.note().path, "notes/Memory verses.md", "and has made nothing yet");
        let _ = app.update(Message::Note(id, NoteMsg::CreateNote(template_path("Blank"))));
        assert!(!app.note().choosing);
        assert_eq!(app.note().path, "notes/Untitled.md");
        let front = study_core::frontmatter::parse(&app.sh.notes["notes/Untitled.md"]).front;
        assert_eq!(front.title.as_deref(), Some("Untitled"));
        assert!(front.created.is_some());
        let _ = app.update(Message::Note(id, NoteMsg::CreateNote(template_path("Blank"))));
        assert_eq!(app.note().path, "notes/Untitled 2.md");
        // The caret is ready below the properties.
        assert_eq!(app.note().doc.selection().head, app.note().doc.front_end());
    }

    #[test]
    fn marks_made_in_the_reader_are_stamped_and_the_stamp_is_in_the_file() {
        let mut app = blank_app();
        select_verses(&mut app, &[16]);
        let _ = app.update(app.b(BibleMsg::ApplyPen(0)));
        let mark = app.sh.marks.last().unwrap();
        assert!(chrono_parses(&mark.created), "{}", mark.created);
        assert!(app.sh.notes[&mark.path()].contains(&format!("created: {}", mark.created)));
    }

    fn chrono_parses(stamp: &str) -> bool {
        !stamp.is_empty() && study_core::frontmatter::display(stamp) != stamp
    }

    // ---- the properties editor -----------------------------------------------------------

    const PROPS_NOTE: &str = "---\ntitle: Props\ncreated: 2026-10-12T23:41:05-07:00\ntags: [a]\nread: true\nchapters: 12\nwith: [Ann, Sam]\ndue: 2026-10-12\nmood: calm\nauthors:\n  - Ann\n---\n\nBody text\n";

    fn props_app() -> (App, TabId) {
        let mut app = App::new();
        app.sh.notes.insert("notes/Props.md".into(), PROPS_NOTE.into());
        app.open_note("notes/Props.md".into());
        let id = app.nid();
        (app, id)
    }

    fn front_of(app: &App) -> study_core::frontmatter::Frontmatter {
        study_core::frontmatter::parse(app.note().doc.source()).front
    }

    fn value_of(app: &App, key: &str) -> study_core::frontmatter::Value {
        front_of(app).props.iter().find(|p| p.key == key).unwrap().value.clone()
    }

    #[test]
    fn the_raw_frontmatter_cannot_be_reached_or_edited_in_the_text() {
        let (mut app, id) = props_app();
        let front_end = app.note().doc.front_end();
        for action in [
            EditAction::SetCaret { offset: 0, extend: false },
            EditAction::Move { motion: md_editor::Motion::DocStart, extend: false },
            EditAction::Select(0..10),
            EditAction::SelectAll,
        ] {
            let _ = app.update(Message::Note(id, NoteMsg::Action(action)));
            let s = app.note().doc.selection();
            assert!(s.anchor >= front_end && s.head >= front_end);
        }
        let before = app.note().doc.source().to_string();
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::Move { motion: md_editor::Motion::DocStart, extend: false })));
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::Backspace)));
        assert_eq!(app.note().doc.source(), before, "nothing is deleted into the properties");
        assert!(app.note().doc.source().starts_with("---\ntitle: Props"));
        // Margin notes' lines are the app's too.
        app.open_note(app.sh.marks[0].path());
        assert!(app.note().doc.front_end() > 0 && app.note().doc.selection().head >= app.note().doc.front_end());
    }

    #[test]
    fn title_and_type_are_set_with_their_own_controls_and_undo() {
        let (mut app, id) = props_app();
        let _ = app.update(Message::Note(id, NoteMsg::SetTitle("A new title".into())));
        let _ = app.update(Message::Note(id, NoteMsg::SetKind(KindChoice(Some("sermon".into())))));
        let front = front_of(&app);
        assert_eq!((front.title.as_deref(), front.kind.as_deref()), (Some("A new title"), Some("sermon")));
        assert_eq!(front.props.len(), 6, "the other properties are untouched");
        assert!(app.note().doc.source().ends_with("Body text\n"));
        let _ = app.update(Message::Note(id, NoteMsg::SetKind(KindChoice(None))));
        assert!(front_of(&app).kind.is_none(), "None takes it away");
        let _ = app.update(Message::Note(id, NoteMsg::SetTitle(String::new())));
        assert!(front_of(&app).title.is_none());
        app.note_mut().doc.perform(EditAction::Undo);
        assert_eq!(front_of(&app).title.as_deref(), Some("A new title"));
    }

    #[test]
    fn each_kind_of_other_property_is_edited_as_its_kind() {
        use study_core::frontmatter::Value;
        let (mut app, id) = props_app();
        let index = |app: &App, key: &str| front_of(app).props.iter().position(|p| p.key == key).unwrap();
        // Yes / No
        let i = index(&app, "read");
        let _ = app.update(Message::Note(id, NoteMsg::PropBool(i, false)));
        assert_eq!(value_of(&app, "read"), Value::Bool(false));
        // A number: valid input is stored; a half-typed one is only shown.
        let i = index(&app, "chapters");
        let _ = app.update(Message::Note(id, NoteMsg::PropEdit(i, "13".into())));
        assert_eq!(value_of(&app, "chapters"), Value::Number("13".into()));
        let _ = app.update(Message::Note(id, NoteMsg::PropEdit(i, "13x".into())));
        assert_eq!(value_of(&app, "chapters"), Value::Number("13".into()), "not a number: not stored");
        assert_eq!(app.note().drafts.get(&i).map(String::as_str), Some("13x"), "but what was typed stays in the box");
        // A list, typed with commas.
        let i = index(&app, "with");
        let _ = app.update(Message::Note(id, NoteMsg::PropEdit(i, "Ann, Sam, Lee, ".into())));
        assert_eq!(value_of(&app, "with"), Value::List(vec!["Ann".into(), "Sam".into(), "Lee".into()]));
        assert_eq!(app.note().drafts[&i], "Ann, Sam, Lee, ", "the trailing comma is not eaten while typing");
        // A date must be a date; text is anything.
        let i = index(&app, "due");
        let _ = app.update(Message::Note(id, NoteMsg::PropEdit(i, "tomorrow".into())));
        assert_eq!(value_of(&app, "due"), Value::Date("2026-10-12".into()));
        let _ = app.update(Message::Note(id, NoteMsg::PropEdit(i, "2027-01-02".into())));
        assert_eq!(value_of(&app, "due"), Value::Date("2027-01-02".into()));
        let i = index(&app, "mood");
        let _ = app.update(Message::Note(id, NoteMsg::PropEdit(i, "42".into())));
        assert_eq!(value_of(&app, "mood"), Value::Text("42".into()), "text stays text even if it looks like a number");
        // What the editor does not model is untouched by all this.
        assert!(app.note().doc.source().contains("authors:\n  - Ann\n---"), "{}", app.note().doc.source());
        assert!(app.note().doc.source().ends_with("Body text\n"));
    }

    #[test]
    fn properties_are_added_by_name_and_type_and_removed() {
        use study_core::frontmatter::{PropType, Value};
        let (mut app, id) = props_app();
        let count = front_of(&app).props.len();
        // Reserved, duplicate and malformed names are refused.
        for bad in ["title", "created", "TAGS", "type", "read", "a: b", "  "] {
            let _ = app.update(Message::Note(id, NoteMsg::NewKey(bad.into())));
            let _ = app.update(Message::Note(id, NoteMsg::AddProp));
            assert_eq!(front_of(&app).props.len(), count, "{bad:?}");
        }
        let _ = app.update(Message::Note(id, NoteMsg::NewKey("pages".into())));
        let _ = app.update(Message::Note(id, NoteMsg::NewType(PropType::Number)));
        let _ = app.update(Message::Note(id, NoteMsg::AddProp));
        assert_eq!(value_of(&app, "pages"), Value::Number("0".into()));
        assert!(app.note().new_key.is_empty(), "the name box is ready for the next");
        let _ = app.update(Message::Note(id, NoteMsg::NewKey("finished".into())));
        let _ = app.update(Message::Note(id, NoteMsg::NewType(PropType::Bool)));
        let _ = app.update(Message::Note(id, NoteMsg::AddProp));
        assert_eq!(value_of(&app, "finished"), Value::Bool(false));
        let i = front_of(&app).props.iter().position(|p| p.key == "pages").unwrap();
        let _ = app.update(Message::Note(id, NoteMsg::PropRemove(i)));
        assert!(front_of(&app).props.iter().all(|p| p.key != "pages"));
        assert_eq!(front_of(&app).props.len(), count + 1);
    }

    #[test]
    fn the_editor_opens_from_the_strip_and_shows_every_property() {
        let (mut app, id) = props_app();
        let mut h = Harness::new({
            app.open_note("notes/Props.md".into());
            app
        });
        assert!(h.center_of("Properties").is_some(), "a button for the editor");
        assert!(h.center_of("New property").is_none(), "closed to begin with");
        let _ = h.app.update(Message::Note(id, NoteMsg::ToggleProps));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Title").is_some() && h.center_of("Type").is_some());
        assert!(h.center_of("chapters \u{00B7} number").is_some());
        assert!(h.center_of("with \u{00B7} list").is_some());
        assert!(h.center_of("due \u{00B7} date").is_some());
        assert!(h.center_of("mood \u{00B7} text").is_some());
        // (The rows below these scroll inside the editor; a block it does not model is among them.)
        assert!(matches!(value_of(&h.app, "authors"), study_core::frontmatter::Value::Raw(_)), "kept as written, not editable");
    }

    #[test]
    fn a_margin_note_shows_its_marks_details_instead() {
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        let mut h = Harness::new(app);
        assert!(h.center_of("Add tag").is_none(), "no tag box: the file's lines are the app's");
        assert!(h.center_of("Properties").is_none());
        let mark = &h.app.sh.marks[0];
        let summary = format!("{}  \u{00B7}  highlight  \u{00B7}  KJV  \u{00B7}  #promise", mark.anchor);
        assert!(h.bounds_of_text(&summary).is_some(), "{summary}");
    }

    // ---- the reader's menu and references ------------------------------------------------

    #[test]
    fn the_reader_menu_lists_what_the_text_can_show_and_closes_on_other_activity() {
        let mut app = App::new();
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::ToggleMenu));
        let mut h = Harness::new(app);
        for label in ["Strong\u{2019}s numbers".replace('\u{2019}', "'").as_str(), "Marks", "Footnotes", "References"] {
            assert!(h.center_of(label).is_some(), "{label}");
        }
        assert!(h.center_of("Strong's words").is_none(), "the old checkbox is gone from the toolbar");
        // Using the menu keeps it open; anything else closes it.
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMarks(false)));
        assert!(h.app.bible().menu_open);
        let note = h.app.nid();
        let _ = h.app.update(Message::Note(note, NoteMsg::ToggleProps));
        assert!(!h.app.bible().menu_open, "a pop-up closes when something else is used");
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMenu));
        h.click(60.0, 300.0);
        assert!(!h.app.bible().menu_open, "and when the text is clicked");
    }

    #[test]
    fn opening_the_reader_menu_leaves_the_reader_where_it_was_scrolled() {
        let mut h = Harness::new(App::new());
        h.size = Size::new(1024.0, 560.0);
        let id = h.app.bid();
        // The reader's text (left of where the menu hangs), as pixels.
        let text_pixels = |h: &mut Harness| -> Vec<u8> {
            let theme = h.app.theme();
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            h.cache = ui.into_cache();
            let (w, hh) = (h.size.width as u32, h.size.height as u32);
            let all = h.renderer.screenshot(Size::new(w, hh), 1.0, iced::Color::BLACK);
            let mut part = Vec::new();
            for y in 150..540usize {
                part.extend_from_slice(&all[(y * w as usize) * 4..(y * w as usize + 240) * 4]);
            }
            part
        };
        h.move_to(200.0, 300.0);
        for _ in 0..3 {
            h.feed(Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Lines { x: 0.0, y: -5.0 } }));
        }
        assert!(h.app.bible().scroll_y > 0.0, "the reader scrolled");
        // (With the pointer over the text it is lit by what is under it, which the menu's layer covers.)
        h.cursor = mouse::Cursor::Unavailable;
        let before = text_pixels(&mut h);
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMenu));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let after = text_pixels(&mut h);
        let differing = before.iter().zip(&after).filter(|(a, b)| a != b).count();
        assert!(differing == 0, "the text moved: {differing} of {} bytes differ", before.len());
    }

    #[test]
    fn each_toggle_hides_its_kind_of_thing() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        let id = app.bid();
        let mut h = Harness::new(app);
        assert!(!h.app.mark_draws().is_empty());
        let foot = |a: &App| a.cards().iter().filter(|c| (FOOTNOTE_KEY..WORD_KEY).contains(&c.key)).count();
        assert_eq!(foot(&h.app), 1);
        // Marks.
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMarks(false)));
        assert!(h.app.mark_draws().is_empty() && h.app.visible_marks().is_empty());
        assert!(h.app.cards().iter().all(|c| c.key >= FOOTNOTE_KEY), "no mark cards either");
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMarks(true)));
        assert!(!h.app.mark_draws().is_empty());
        // Footnotes: no margin card, and no dagger to click.
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleFootnotes(false)));
        assert_eq!(foot(&h.app), 0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.click_scan(|a| matches!(a.bible().note_card, Some(NoteCard { content: NoteContent::Footnote(_), .. }))).is_none());
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleFootnotes(true)));
        assert_eq!(foot(&h.app), 1);
    }

    #[test]
    fn cross_reference_markers_show_when_asked_and_open_a_list_of_links() {
        let mut app = App::new();
        app.bible_mut().show_refs = false;
        let mut h = Harness::new(app);
        let found = |a: &App| matches!(a.bible().note_card, Some(NoteCard { content: NoteContent::References(_), .. }));
        assert!(h.click_scan(found).is_none(), "no markers while References is off");
        let id = h.app.bid();
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleRefs(true)));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        h.click_scan(found).expect("a cross reference marker");
        assert!(h.center_of("References").is_some(), "the pop-over is titled");
        let Some(NoteCard { content: NoteContent::References(refs), .. }) = &h.app.bible().note_card else { unreachable!() };
        assert_eq!(refs.iter().map(ToString::to_string).collect::<Vec<_>>(), ["Numbers 21:9", "Psalms 23:1"]);
        // Each reference is a link with the start of its text.
        // (Each reference is drawn as the writer draws a link, so it has the same right click and
        // pull-away: those are the editor's own, tested with it.)
        assert!(h.center_of("The Lord is my shepherd; I shall not want.").is_some(), "its text, to tell it apart");
        // Following one takes this tab there.
        let id = h.app.bid();
        let target = LinkTarget::Verse { range: RefRange::parse("Ps 23:1").unwrap(), module: None };
        let _ = h.app.update(Message::Bible(id, BibleMsg::OpenLink(target)));
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::PSALMS, 23));
    }

    #[test]
    fn a_chapter_of_only_footnotes_still_gets_its_margin() {
        let mut app = App::new();
        // No note on any mark, and the reader has not (yet) said it has a margin: the footnotes
        // alone must be enough for it to make one, or a chapter like that never would.
        for mark in &mut app.sh.marks {
            mark.body.clear();
        }
        assert!(!app.bible().margin_showing);
        assert!(!app.bible().cards(&app.sh).is_empty(), "the footnotes are offered as cards");
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.app.bible().margin_showing, "a wide reader with footnotes has a margin");
    }

    #[test]
    fn links_in_footnotes_and_reference_lists_offer_the_writers_menu_and_pull() {
        let mut app = App::new();
        let id = app.bid();
        // A footnote that mentions a passage is a link view, whose right click and pull-away go
        // through the card messages (as a mark card's do).
        let task = app.update(Message::Bible(id, BibleMsg::CardLinkMenu(verse_link("Joh 3:16"), Point::new(40.0, 50.0))));
        drop(task);
        assert!(app.link_menu.is_some(), "right clicking the link offers where to open it");
        let _ = app.update(Message::Bible(id, BibleMsg::CardLinkDrag(verse_link("Joh 3:16"), Point::new(60.0, 70.0))));
        assert!(matches!(app.drag.as_ref().map(|d| &d.payload), Some(Payload::Link(_))), "and pulling it picks it up");
    }

    #[test]
    fn notes_on_one_line_stack_and_the_arrows_cycle_through_them() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        // The note on "born again" and the footnote on the same line are one stack: the margin
        // card region, as pixels.
        let card = |h: &mut Harness| -> Vec<u8> {
            let theme = h.app.theme();
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            h.cache = ui.into_cache();
            let w = h.size.width as usize;
            let all = h.renderer.screenshot(Size::new(h.size.width as u32, h.size.height as u32), 1.0, iced::Color::BLACK);
            let mut part = Vec::new();
            for y in 330..430usize {
                part.extend_from_slice(&all[(y * w + 1060) * 4..(y * w + 1450) * 4]);
            }
            part
        };
        h.cursor = mouse::Cursor::Unavailable;
        let first = card(&mut h);
        // The bar's right arrow, under the top card.
        h.click(1298.0, 446.0);
        h.cursor = mouse::Cursor::Unavailable;
        let second = card(&mut h);
        assert_ne!(first, second, "the next note is on top");
        // It stays on top as the reader is drawn again.
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        h.cursor = mouse::Cursor::Unavailable;
        assert_eq!(card(&mut h), second);
    }

    #[test]
    fn pointing_at_a_mark_brings_its_note_to_the_top_of_its_stack_and_a_click_then_edits() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let card = |h: &mut Harness| -> Vec<u8> {
            let theme = h.app.theme();
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            h.cache = ui.into_cache();
            let w = h.size.width as usize;
            let all = h.renderer.screenshot(Size::new(h.size.width as u32, h.size.height as u32), 1.0, iced::Color::BLACK);
            let mut part = Vec::new();
            for y in 330..430usize {
                part.extend_from_slice(&all[(y * w + 1060) * 4..(y * w + 1450) * 4]);
            }
            part
        };
        // The pointer on the footnote's marker, then on "born again" a few words on.
        h.move_to(399.0, 343.0);
        let footnote = card(&mut h);
        h.move_to(880.0, 343.0);
        let mark = card(&mut h);
        assert_ne!(footnote, mark, "each comes to the top as it is pointed at");
        h.move_to(399.0, 343.0);
        assert_eq!(card(&mut h), footnote);
        // With its note on top, a click on the mark edits it.
        h.click(880.0, 343.0);
        assert!(h.app.bible().editing.is_some());
    }

    #[test]
    fn what_is_under_the_open_margin_menu_is_not_pointed_at() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let id = h.app.bid();
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMarginMenu));
        // The lower note, as pixels: it is dimmed if a note is thought to be pointed at.
        let lower = |h: &mut Harness| -> Vec<u8> {
            let theme = h.app.theme();
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            h.cache = ui.into_cache();
            let w = h.size.width as usize;
            let all = h.renderer.screenshot(Size::new(h.size.width as u32, h.size.height as u32), 1.0, iced::Color::BLACK);
            let mut part = Vec::new();
            for y in 600..700usize {
                part.extend_from_slice(&all[(y * w + 1060) * 4..(y * w + 1450) * 4]);
            }
            part
        };
        h.cursor = mouse::Cursor::Unavailable;
        let calm = lower(&mut h);
        // On the menu's panel, over the note beneath it.
        h.move_to(1250.0, 380.0);
        assert_eq!(lower(&mut h), calm, "nothing under the menu is pointed at");
    }

    #[test]
    fn strongs_words_have_their_entries_in_the_margin_when_kept_there_and_nothing_opens_on_a_click() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        let words = |a: &App| a.cards().iter().filter(|c| (WORD_KEY..WORD_KEY * 2).contains(&c.key)).count();
        assert_eq!(words(&app), 0, "not kept in the margin by default");
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::SetInMargin(NoteKind::Strongs, true)));
        assert!(words(&app) > 0, "an entry for each word with a number");
        let mut h = Harness::new(app);
        assert!(h.click_scan(is_word).is_none() || h.app.bible().note_card.is_none(), "a click opens no pop-over");
        assert!(h.app.bible().note_card.is_none());
        // Not shown at all, no cards.
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleStrongs(false)));
        assert_eq!(words(&h.app), 0);
    }

    #[test]
    fn every_cross_reference_marker_has_its_list_in_the_margin_when_references_are_kept_there() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().in_margin.refs = true;
        let _ = app.update(app.b(BibleMsg::ToggleRefs(true)));
        let refs = |a: &App| a.cards().iter().filter(|c| c.key >= (1 << 43)).count();
        assert_eq!(refs(&app), 1, "John 3 has one cross reference marker in the mock");
        // A click on the marker has nothing to open: the list is showing already.
        let mut h = Harness::new(app);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.click_scan(|a| a.bible().note_card.is_some()).is_none(), "no pop-over for a marker whose list is in the margin");
        // Kept out of the margin, the cards go and the marker opens its list over the text.
        let id = h.app.bid();
        let _ = h.app.update(Message::Bible(id, BibleMsg::SetInMargin(NoteKind::Refs, false)));
        assert_eq!(refs(&h.app), 0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.click_scan(|a| a.bible().note_card.is_some()).is_some());
        // Not shown at all, no cards either way.
        let _ = h.app.update(Message::Bible(id, BibleMsg::SetInMargin(NoteKind::Refs, true)));
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleRefs(false)));
        assert_eq!(refs(&h.app), 0);
    }

    #[test]
    fn marks_kept_out_of_the_margin_have_no_cards_in_it_and_pop_up_instead() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let id = app.bid();
        let marks = |a: &App| a.cards().iter().filter(|c| c.key < FOOTNOTE_KEY).count();
        assert!(marks(&app) > 0);
        let _ = app.update(Message::Bible(id, BibleMsg::SetInMargin(NoteKind::Marks, false)));
        assert_eq!(marks(&app), 0, "no mark card in the margin");
        assert!(!app.mark_draws().is_empty(), "the marks are still drawn in the text");
        let mut h = Harness::new(app);
        // Clicking a mark floats its card under the click, as in a reader with no margin.
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        h.click(880.0, 343.0);
        assert!(h.app.bible().popup.is_some(), "the click on \"born again\" floated its card");
        assert!(h.app.bible().editing.is_none(), "and did not start writing in the margin");
        assert!(h.app.bible().floating(&h.app.sh).is_some());
    }

    #[test]
    #[ignore = "measures how long a crowded margin takes to lay out and draw"]
    fn timing_a_crowded_margin() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        // Many notes on the words of verses 1 to 8.
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        for verse in 1..=8u16 {
            let text = canon[&verse].clone();
            let mut at = 0;
            while let Some(found) = text[at..].find(' ') {
                let (start, end) = (at, at + found);
                at = end + 1;
                if end <= start {
                    continue;
                }
                app.bible_mut().text_selection = Some(vec![CanonSlice { verse, start, end }]);
                let _ = app.update(app.b(BibleMsg::ApplyPen(0)));
                app.sh.marks.last_mut().unwrap().body = "A note on this word, a line or two long, to give the card some height.".into();
            }
        }
        // And a dictionary entry for every word of the chapter, as with Strong's numbers kept in
        // the margin (the mock has too few of its own).
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        let mut words = Vec::new();
        for (verse, text) in &canon {
            let mut at = 0;
            while let Some(found) = text[at..].find(' ') {
                let (start, end) = (at, at + found);
                at = end + 1;
                if end > start {
                    words.push(crate::bible_tab::WordCard {
                        anchor: CanonSlice { verse: *verse, start, end },
                        entry: study_core::LexiconEntry {
                            key: "G25".into(),
                            lemma: "\u{3b1}\u{3b3}\u{3b1}\u{3c0}\u{3ac}\u{3c9}".into(),
                            transliteration: "agap\u{e1}\u{14d}".into(),
                            definition: "to love, to wish well to, to have a preference for, to regard with favour; used of the love of God for Christ and of Christ for His disciples.".into(),
                        },
                    });
                }
            }
        }
        app.bible_mut().words = words;
        app.bible_mut().show_strongs = true;
        app.bible_mut().in_margin.strongs = true;
        println!("cards: {}", app.cards().len());
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let theme = h.app.theme();
        let start = std::time::Instant::now();
        for i in 0..10 {
            h.move_to(100.0 + i as f32, 300.0);
        }
        println!("10 pointer moves: {:?}", start.elapsed());
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            h.cache = ui.into_cache();
        }
        println!("10 view+build+draw: {:?}", start.elapsed());
        let (mut built, mut drawn) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        for _ in 0..10 {
            let t = std::time::Instant::now();
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            built += t.elapsed();
            let t = std::time::Instant::now();
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            drawn += t.elapsed();
            h.cache = ui.into_cache();
        }
        println!("of which in 10 frames: build {built:?}, draw {drawn:?}");
        let id = h.app.bid();
        let start = std::time::Instant::now();
        for i in 0..10 {
            let _ = h.app.update(Message::Bible(id, BibleMsg::Scrolled(10.0 * i as f32)));
        }
        println!("10 scrolled updates: {:?}", start.elapsed());
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let _ = h.app.view();
        }
        println!("10 views: {:?}", start.elapsed());
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let _ = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer).into_cache();
        }
        println!("10 view+build (no draw, cache dropped): {:?}", start.elapsed());
    }

    #[test]
    fn the_margin_menu_goes_when_a_click_lands_anywhere_else() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let id = h.app.bid();
        for (name, at) in [("text", (200.0, 330.0)), ("an empty part of the margin", (1250.0, 520.0)), ("a note", (1250.0, 650.0))] {
            let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleMarginMenu));
            assert!(h.app.bible().margin_menu_open);
            h.click(at.0, at.1);
            assert!(!h.app.bible().margin_menu_open, "a click on {name} puts the menu away");
        }
    }

    #[test]
    fn pointing_at_a_strongs_word_brings_its_entry_to_the_top_of_its_stack() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::SetInMargin(NoteKind::Strongs, true)));
        let mut h = Harness::new(app);
        h.size = Size::new(1500.0, 800.0);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let stack = |h: &mut Harness| -> Vec<u8> {
            let theme = h.app.theme();
            let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
            ui.draw(&mut h.renderer, &theme, &iced::advanced::renderer::Style { text_color: iced::Color::WHITE }, h.cursor);
            h.cache = ui.into_cache();
            let w = h.size.width as usize;
            let all = h.renderer.screenshot(Size::new(h.size.width as u32, h.size.height as u32), 1.0, iced::Color::BLACK);
            let mut part = Vec::new();
            for y in 590..720usize {
                part.extend_from_slice(&all[(y * w + 1060) * 4..(y * w + 1450) * 4]);
            }
            part
        };
        h.cursor = mouse::Cursor::Unavailable;
        let calm = stack(&mut h);
        // "God" and "world" are different words with different entries.
        h.move_to(767.0, 621.0);
        let god = stack(&mut h);
        assert_ne!(god, calm, "its entry came up");
        h.move_to(935.0, 621.0);
        let world = stack(&mut h);
        assert_ne!(world, god, "each word has its own entry");
        h.move_to(767.0, 621.0);
        assert_eq!(stack(&mut h), god);
    }

    #[test]
    fn links_to_notes_that_pop_over_the_text_are_quiet_until_pointed_at() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        let h = Harness::new(app);
        let options = h.app.bible().view_options(&h.app.sh);
        // Strong's words are popovers by default, footnotes are in the margin.
        assert!(!options.strongs_in_margin && options.footnotes_in_margin && !options.refs_in_margin);
    }

    #[test]
    fn unlinking_a_note_shows_it_in_a_note_tab_so_it_is_not_lost() {
        let mut app = App::new();
        let was = app.note().path.clone();
        assert!(!was.starts_with("margins/"), "a note tab on some other note");
        let _ = app.update(app.b(BibleMsg::UnlinkMarkAt(0)));
        let shown = app.note().path.clone();
        assert!(shown.starts_with("notes/John 3."), "the plain note that was left: {shown}");
        assert!(app.sh.notes.contains_key(&shown));
    }

    #[test]
    fn a_new_note_asks_what_kind_and_starts_from_that_template() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::NewNote));
        let mut h = Harness::new(app);
        for template in h.app.sh.templates() {
            assert!(h.center_of(&template.name).is_some(), "{} is offered", template.name);
            assert!(h.center_of(&template.description).is_some(), "with its description");
        }
        assert_eq!(h.app.sh.templates().len(), 4);
        // Pick one: the note is filed with its folder and type, and opens.
        let (x, y) = h.center_of("Prayer list").unwrap();
        h.click(x, y);
        assert_eq!(h.app.note().pending, Some(template_path("Prayer list")), "the kind first; then it asks for a title");
        assert!(h.center_of("Title").is_some() || h.app.note().choosing);
        let (x, y) = h.center_of("Create").expect("the title step has its button");
        h.click(x, y);
        assert!(!h.app.note().choosing && h.app.note().pending.is_none());
        assert_eq!(h.app.note().path, "notes/Prayer list.md", "filed with the notes: tags say what it is");
        let text = h.app.sh.notes[&h.app.note().path].clone();
        let front = study_core::frontmatter::parse(&text).front;
        assert_eq!(front.tags, ["prayer"]);
        assert!(text.contains("## Requests"));

        // Daily: one a day. The second ask shows the first.
        let id = h.app.nid();
        // (It is named by its date, so there is no title to ask for: choosing it makes it.)
        for _ in 0..2 {
            let _ = h.app.update(Message::Note(id, NoteMsg::NewNote));
            let _ = h.app.update(Message::Note(id, NoteMsg::PickKind(template_path("Daily"))));
            assert!(!h.app.note().choosing && h.app.note().pending.is_none());
        }
        let today = h.app.note().path.clone();
        assert!(study_core::frontmatter::parse(&h.app.sh.notes[&today]).front.tags == ["daily"], "{today}");
        assert_eq!(h.app.sh.tag_counts().iter().find(|(t, _)| t == "daily"), Some(&("daily".to_string(), 1)), "one a day");
        // A click outside the choices cancels.
        let _ = h.app.update(Message::Note(id, NoteMsg::NewNote));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        h.click(5.0, 700.0);
        assert!(!h.app.note().choosing);
    }

    #[test]
    fn typing_a_tag_offers_the_ones_in_use_and_picking_one_adds_it() {
        let mut app = App::new();
        app.open_note("notes/Memory verses.md".into());
        let id = app.nid();
        assert!(app.sh.tag_suggestions("", &[], 8).is_empty(), "nothing typed, nothing offered");
        let _ = app.update(Message::Note(id, NoteMsg::TagDraft("  #JOH".into())));
        let have = app.sh.tags_of("notes/Memory verses.md");
        let offered = app.sh.tag_suggestions(&app.note().tag_draft, &have, 8);
        assert_eq!(offered, [("john".to_string(), 1)], "found whatever the case, and with its count");
        assert!(app.sh.tag_suggestions("new", &["new-birth".into()], 8).is_empty(), "not the ones it has");
        assert!(app.sh.tag_suggestions("zzz", &[], 8).is_empty());
        // The ones that start with it come before the ones that only contain it.
        app.sh.notes.insert("notes/A.md".into(), "---\ntags: [born-again, a-born]\n---\n".into());
        let order: Vec<String> = app.sh.tag_suggestions("born", &[], 8).into_iter().map(|(t, _)| t).collect();
        assert_eq!(order, ["born-again", "a-born"]);
        // Picking adds it to the note's tags and clears the box.
        let mut h = Harness::new(app);
        assert!(h.center_of("Use").is_some() && h.center_of("#john").is_some(), "the suggestion shows");
        let (x, y) = h.center_of("#john").unwrap();
        h.click(x, y);
        assert_eq!(h.app.sh.tags_of("notes/Memory verses.md"), ["john"]);
        assert!(h.app.note().tag_draft.is_empty());
    }

    #[test]
    fn pens_are_tags_with_colors_set_in_the_settings_and_the_toolbar_follows() {
        let mut app = App::new();
        let names = |a: &App| a.sh.settings.pens.iter().map(|p| p.name()).collect::<Vec<_>>();
        assert_eq!(names(&app), ["Promise", "Command", "Growth", "Prayer"]);
        // A new pen: a tag (in one form), the next color; its color is the tag's everywhere.
        let _ = app.update(Message::PenDraft("  #Daily Reading".into()));
        let _ = app.update(Message::AddPen);
        let pen = app.sh.settings.pens.last().unwrap().clone();
        assert_eq!((pen.tag.as_str(), pen.name().as_str()), ("daily-reading", "Daily reading"));
        assert!(app.pen_draft.is_empty());
        assert_eq!(app.sh.tag_color("daily-reading"), Some(pen.color.clone()));
        assert_eq!(app.sh.tag_color("daily-reading/early"), Some(pen.color.clone()), "tags inside it have its color");
        assert_eq!(app.sh.tag_color("other"), None);
        // The same tag again changes the color rather than adding a second pen.
        let before = app.sh.settings.pens.len();
        let _ = app.update(Message::PenDraft("daily reading".into()));
        let _ = app.update(Message::AddPen);
        assert_eq!(app.sh.settings.pens.len(), before);
        // Cycling the color and the style, and removing a pen.
        let last = before - 1;
        let color = app.sh.settings.pens[last].color.clone();
        let _ = app.update(Message::CyclePenColor(last));
        assert_ne!(app.sh.settings.pens[last].color, color);
        let _ = app.update(Message::CyclePenStyle(last));
        assert_eq!(app.sh.settings.pens[last].style, state::PenStyle::Underline);
        let _ = app.update(Message::RemovePen(last));
        assert_eq!(names(&app), ["Promise", "Command", "Growth", "Prayer"]);

        // The toolbar offers each pen, and using one marks the text with its tag and color.
        app.sh.settings.pens.push(state::PenDef { tag: "grace".into(), color: "#FF9F43".into(), style: state::PenStyle::Highlight });
        select_so_loved(&mut app);
        let marks = app.sh.marks.len();
        let pen = app.sh.settings.pens.len() - 1;
        let _ = app.update(app.b(BibleMsg::ApplyPen(pen)));
        assert_eq!(app.sh.marks.len(), marks + 1);
        let mark = app.sh.marks.last().unwrap();
        assert_eq!((mark.tags.as_slice(), mark.color.as_str()), (&["grace".to_string()][..], "#FF9F43"));
        // It is a note like any other: its tag is counted.
        assert!(app.sh.tag_counts().iter().any(|(t, n)| t == "grace" && *n >= 1));
    }

    #[test]
    fn settings_files_with_odd_pens_are_repaired() {
        let s = Settings {
            pens: vec![
                state::PenDef { tag: " Promise ".into(), color: "nonsense".into(), style: state::PenStyle::Highlight },
                state::PenDef { tag: "promise".into(), color: "#112233".into(), style: state::PenStyle::Underline },
                state::PenDef { tag: "  ".into(), color: "#112233".into(), style: state::PenStyle::Underline },
            ],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.pens.len(), 1, "a tag once, and not an empty one");
        assert_eq!((s.pens[0].tag.as_str(), s.pens[0].color.as_str()), ("promise", "#F5D90A"));
    }

    #[test]
    fn a_mark_being_written_in_can_be_tagged_with_autocomplete_and_the_file_follows() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let id = app.bid();
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        let path = app.sh.marks[0].path();
        let before = app.sh.marks[0].tags.clone();
        // A new tag, typed and entered (in one form).
        let _ = app.update(Message::Bible(id, BibleMsg::CardTagDraft("  #Hard Sayings".into())));
        let offered = app.sh.tag_suggestions(&app.bible().card_tag_draft, &app.sh.marks[0].tags, 5);
        assert!(offered.is_empty(), "nothing in use fits yet");
        let _ = app.update(Message::Bible(id, BibleMsg::AddCardTag(String::new())));
        assert!(app.sh.marks[0].tags.contains(&"hard-sayings".to_string()));
        assert!(app.bible().card_tag_draft.is_empty());
        assert!(app.sh.notes[&path].contains("hard-sayings"), "in the note file too");
        // One already in use is offered as it is typed, and picking it adds it.
        let _ = app.update(Message::Bible(id, BibleMsg::CardTagDraft("new-b".into())));
        let offered = app.sh.tag_suggestions(&app.bible().card_tag_draft, &app.sh.marks[0].tags, 5);
        assert_eq!(offered[0].0, "new-birth");
        let _ = app.update(Message::Bible(id, BibleMsg::AddCardTag("new-birth".into())));
        assert!(app.sh.marks[0].tags.contains(&"new-birth".to_string()));
        // The same again changes nothing; taking one off does.
        let count = app.sh.marks[0].tags.len();
        let _ = app.update(Message::Bible(id, BibleMsg::AddCardTag("New-Birth".into())));
        assert_eq!(app.sh.marks[0].tags.len(), count);
        let _ = app.update(Message::Bible(id, BibleMsg::RemoveCardTag("hard-sayings".into())));
        assert!(!app.sh.marks[0].tags.contains(&"hard-sayings".to_string()) && !app.sh.notes[&path].contains("hard-sayings"));
        assert!(before.iter().all(|t| app.sh.marks[0].tags.contains(t)), "the pen's tag stays");
        // The tag counts follow, so the selector can filter by it.
        assert!(app.sh.tag_counts().iter().any(|(t, n)| t == "new-birth" && *n >= 2));
        // The card shows the box while it is written in.
        let mut h = Harness::new(app);
        assert!(h.center_of("Add tag").is_some());
    }

    #[test]
    fn a_new_note_takes_the_title_typed_as_its_heading_and_file_name() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::NewNote));
        let _ = app.update(Message::Note(id, NoteMsg::PickKind(template_path("Topic"))));
        assert_eq!(app.note().pending, Some(template_path("Topic")));
        // Back to the kinds, and on again.
        let _ = app.update(Message::Note(id, NoteMsg::BackToKinds));
        assert!(app.note().choosing && app.note().pending.is_none());
        let _ = app.update(Message::Note(id, NoteMsg::PickKind(template_path("Topic"))));
        let _ = app.update(Message::Note(id, NoteMsg::NewTitle("Grace and Truth".into())));
        let mut h = Harness::new(app);
        let (x, y) = h.center_of("Create").unwrap();
        h.click(x, y);
        assert_eq!(h.app.note().path, "notes/Grace and Truth.md");
        let text = h.app.sh.notes["notes/Grace and Truth.md"].clone();
        let parsed = study_core::frontmatter::parse(&text);
        assert_eq!(parsed.front.title.as_deref(), Some("Grace and Truth"));
        assert!(text.contains("\n# Grace and Truth\n") && text.contains("## Passages"), "{text}");
        assert!(h.app.note().new_title.is_empty(), "the box starts empty next time");
        // Blank with a title is a note that starts with its heading; the same title again is numbered.
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::NewNote));
        let _ = h.app.update(Message::Note(id, NoteMsg::PickKind(template_path("Blank"))));
        let _ = h.app.update(Message::Note(id, NoteMsg::NewTitle("Grace and Truth".into())));
        let _ = h.app.update(Message::Note(id, NoteMsg::CreateNote(template_path("Blank"))));
        assert_eq!(h.app.note().path, "notes/Grace and Truth 2.md");
        assert!(h.app.sh.notes[&h.app.note().path].contains("\n# Grace and Truth\n"));
        // A title typed before choosing the daily note (a stray one) is not used for it.
        let _ = h.app.update(Message::Note(id, NoteMsg::NewTitle("Ignored".into())));
        let _ = h.app.update(Message::Note(id, NoteMsg::CreateNote(template_path("Daily"))));
        assert!(!h.app.note().path.contains("Ignored"), "{}", h.app.note().path);
    }

    #[test]
    fn adding_a_template_file_adds_a_kind_and_editing_its_frontmatter_changes_how_it_behaves() {
        let mut app = App::new();
        let id = app.nid();
        app.sh.notes.insert(
            "templates/Sermon.md".into(),
            "---\ntemplate_description: Notes on a sermon.\ntype: sermon\ntags: [sermon]\npreacher: Me\n---\n# {{title}}\n\nPreached {{date}}.\n".into(),
        );
        let _ = app.update(Message::Note(id, NoteMsg::NewNote));
        let mut h = Harness::new(app);
        assert!(h.center_of("Sermon").is_some() && h.center_of("Notes on a sermon.").is_some(), "the new kind is offered");
        let (x, y) = h.center_of("Sermon").unwrap();
        h.click(x, y);
        let _ = h.app.update(Message::Note(id, NoteMsg::NewTitle("Grace".into())));
        let (x, y) = h.center_of("Create").unwrap();
        h.click(x, y);
        assert_eq!(h.app.note().path, "notes/Grace.md");
        let text = h.app.sh.notes["notes/Grace.md"].clone();
        let front = study_core::frontmatter::parse(&text).front;
        assert_eq!((front.kind.as_deref(), front.tags.as_slice()), (Some("sermon"), &["sermon".to_string()][..]));
        assert!(front.props.iter().any(|p| p.key == "preacher"), "its own properties come along");
        assert!(!text.contains("template_"), "the template's logic does not");
        assert!(text.contains("\n# Grace\n\nPreached 20"), "{text}");

        // The daily template, edited to ask for a title: now it does.
        let daily = h.app.sh.notes["templates/Daily.md"].replace("template_ask_title: false", "template_ask_title: true");
        h.app.sh.notes.insert("templates/Daily.md".into(), daily);
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::NewNote));
        let _ = h.app.update(Message::Note(id, NoteMsg::PickKind(template_path("Daily"))));
        assert_eq!(h.app.note().pending, Some(template_path("Daily")), "it asks for a title now");
        // And a kind taken away is no longer offered.
        let _ = h.app.update(Message::Note(id, NoteMsg::CancelNew));
        h.app.sh.notes.remove("templates/Topic.md");
        assert!(!h.app.sh.templates().iter().any(|t| t.name == "Topic"));
    }

    #[test]
    fn templates_are_notes_to_open_but_not_in_the_tag_counts_or_the_blank_tab_choices() {
        let app = App::new();
        assert!(!app.sh.tag_counts().iter().any(|(t, _)| t == "daily" || t == "topic"), "their tags are for the notes made from them");
        assert!(app.choices().iter().all(|c| !matches!(c, Choice::Note(p) if p.starts_with("templates/"))));
        assert!(!app.sh.first_ordinary_note().starts_with("templates/"));
    }

    #[test]
    fn up_from_a_heading_goes_to_the_blank_line_above_it_in_a_template() {
        let mut app = App::new();
        app.open_note("templates/Daily.md".into());
        let mut h = Harness::new(app);
        // Focus the writer with a click, then put the caret at the start of the heading.
        let b = { let id = h.app.note().editor_id(); h.bounds_of(id) };
        h.click(b.x + b.width - 30.0, b.y + 40.0);
        let source = h.app.note().doc.source().to_string();
        let at = source.find("## Reading").unwrap();
        h.app.note_mut().doc.perform(EditAction::SetCaret { offset: at, extend: false });
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let before = h.app.note().doc.selection().head;
        h.named(Named::ArrowUp);
        let after = h.app.note().doc.selection().head;
        assert!(after < before, "up moved the caret from {before} to {after}");
        h.named(Named::ArrowUp);
        let heading = h.app.note().doc.selection().head;
        assert!(heading < after, "and again, to the heading above the blank line: {after} -> {heading}");
        // And Down goes back the same way, over the space under a heading.
        h.named(Named::ArrowDown);
        let down = h.app.note().doc.selection().head;
        assert!(down > heading, "down from the heading: {heading} -> {down}");
        h.named(Named::ArrowDown);
        assert!(h.app.note().doc.selection().head > down, "and on to the next heading");
    }

    #[test]
    fn a_template_is_edited_from_the_pencil_by_its_kind_not_found_by_searching() {
        let mut app = App::new();
        let id = app.nid();
        let none = std::collections::BTreeSet::new();
        assert!(crate::note_tab::groups(&app.sh, "daily", &none).iter().flat_map(|g| &g.notes).all(|(p, _)| !p.starts_with("templates/")));
        assert!(crate::note_tab::groups(&app.sh, "template_description", &none).is_empty(), "nor by what they say");
        let _ = app.update(Message::Note(id, NoteMsg::NewNote));
        let _ = app.update(Message::Note(id, NoteMsg::EditTemplate(template_path("Daily"))));
        assert!(!app.note().choosing);
        assert_eq!(app.note().path, "templates/Daily.md", "the template opens as a note");
    }

    // ---- where links open ------------------------------------------------------------------

    fn verse_link(range: &str) -> LinkTarget {
        LinkTarget::Verse { range: RefRange::parse(range).unwrap(), module: None }
    }

    fn bible_tabs(app: &App) -> Vec<TabId> {
        app.tabs.iter().filter(|(_, t)| matches!(t, Tab::Bible(_))).map(|(id, _)| *id).collect()
    }

    #[test]
    fn a_plain_click_follows_a_link_here_and_a_notes_link_goes_to_its_bible_tab() {
        let mut app = App::new();
        let (bible, note) = (app.bid(), app.nid());
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(verse_link("Ps 23:1"))));
        assert_eq!(app.bible().book, Book::PSALMS);
        assert_eq!(bible_tabs(&app), [bible], "no new tab");
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Num 21:9"))));
        assert_eq!(app.bible().book, Book::NUMBERS, "a link in a Bible tab navigates that tab");
    }

    #[test]
    fn command_click_opens_in_a_new_tab_beside_the_current_one() {
        let mut app = App::new();
        let bible = app.bid();
        app.modifiers = Modifiers::COMMAND;
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Ps 23:1"))));
        assert_eq!(bible_tabs(&app).len(), 2);
        let new = *app.panes[0].tabs.last().unwrap();
        assert_ne!(new, bible);
        assert_eq!(app.panes[0].active, Some(new), "and it is shown");
        assert_eq!((bible_of(&app, new).book, bible_of(&app, new).chapter_no), (Book::PSALMS, 23));
        assert_eq!((bible_of(&app, bible).book, bible_of(&app, bible).chapter_no), (Book::JOHN, 3), "the first stays put");
        assert_eq!(bible_of(&app, new).module, bible_of(&app, bible).module);
        // From a note, the new tab goes beside the Bible tab the note works with.
        let note = app.nid();
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(verse_link("Num 21:9"))));
        assert_eq!(bible_tabs(&app).len(), 3);
        assert_eq!(app.panes[1].tabs.len(), 1, "not in the note's pane");
    }

    #[test]
    fn alt_click_opens_in_the_other_pane_reusing_its_bible_tab() {
        let mut app = App::new();
        let bible = app.bid();
        app.modifiers = Modifiers::ALT;
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Ps 23:1"))));
        // The other pane held a note: it gets a Bible tab beside it, showing the verse.
        assert_eq!(app.panes[1].tabs.len(), 2);
        let shown = app.panes[1].active.unwrap();
        assert_eq!(bible_of(&app, shown).book, Book::PSALMS);
        assert_eq!(bible_of(&app, bible).book, Book::JOHN, "the first tab did not move");
        // Again: that pane's Bible tab is used, not another made.
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Num 21:9"))));
        assert_eq!(app.panes[1].tabs.len(), 2);
        assert_eq!(bible_of(&app, shown).book, Book::NUMBERS);
        // With one pane only, the other pane is made.
        let mut single = App::new();
        let note = single.nid();
        let _ = single.update(Message::CloseTab(note));
        assert_eq!(single.panes.len(), 1);
        let b = single.bid();
        single.modifiers = Modifiers::ALT;
        let _ = single.update(Message::Bible(b, BibleMsg::OpenLink(verse_link("Ps 23:1"))));
        assert_eq!(single.panes.len(), 2);
        assert_eq!(single.layout.leaves().len(), 2);
    }

    #[test]
    fn shift_click_opens_in_a_new_pane_split_off_the_current_one() {
        let mut app = App::new();
        let bible = app.bid();
        app.modifiers = Modifiers::SHIFT;
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Ps 23:1"))));
        assert_eq!(app.panes.len(), 3);
        assert_eq!(app.layout.leaves().len(), 3);
        let new = app.panes.last().unwrap();
        assert_eq!(new.tabs.len(), 1, "no stray blank tab");
        assert_eq!(bible_of(&app, new.tabs[0]).book, Book::PSALMS);
        assert_eq!(app.focus, new.id);
        // Another one: a pane each time.
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Num 21:9"))));
        assert_eq!(app.panes.len(), 4);
    }

    #[test]
    fn note_links_open_where_asked_too() {
        let mut app = App::new();
        let note = app.nid();
        let link = LinkTarget::Note("Nicodemus".into());
        // Here: this tab.
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link.clone())));
        assert_eq!(app.note().path, "notes/Nicodemus.md");
        // New tab: another tab, the first stays on its note.
        app.modifiers = Modifiers::COMMAND;
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(LinkTarget::Note("Prayer list".into()))));
        assert_eq!(app.panes[1].tabs.len(), 2);
        let Some(Tab::Note(first)) = app.tabs.get(&note) else { unreachable!() };
        assert_eq!(first.path, "notes/Nicodemus.md");
        let Some(Tab::Note(second)) = app.tabs.get(app.panes[1].tabs.last().unwrap()) else { unreachable!() };
        assert_eq!(second.path, "prayers/Prayer list.md");
        // From the Bible tab, a card's note link without keys uses the note tab last used.
        app.modifiers = Modifiers::empty();
        let bible = app.bid();
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(LinkTarget::Note("Memory verses".into()))));
        assert_eq!(app.panes[1].tabs.len(), 2, "no new tab");
        // An unknown note says so.
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(LinkTarget::Note("Nowhere".into()))));
        assert!(app.sh.status.contains("No note named"));
    }

    fn right_click(h: &mut Harness, x: f32, y: f32) {
        h.move_to(x, y);
        h.feed(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)));
        h.feed(Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)));
    }

    #[test]
    fn right_clicking_a_link_in_the_reader_offers_where_to_open_it() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        // The "(Num 21:9)" reference in John 3:14.
        let mut found = None;
        'scan: for y in (140..1400).step_by(10) {
            for x in (50..560).step_by(8) {
                right_click(&mut h, x as f32, y as f32);
                if h.app.link_menu.is_some() {
                    found = Some((x as f32, y as f32));
                    break 'scan;
                }
            }
        }
        let (x, y) = found.expect("a link to right click");
        let menu = h.app.link_menu.as_ref().unwrap();
        assert!(matches!(&menu.spec, Spec::Verse { range, .. } if range.to_string() == "Numbers 21:9"));
        assert!((menu.at.x - x).abs() < 1.0 && (menu.at.y - y).abs() < 1.0, "at the pointer");
        for label in ["Open", "Open in new tab", "Open in other pane", "Open in new pane", "Copy link"] {
            assert!(h.center_of(label).is_some(), "{label}");
        }
        // Choosing one does it, and puts the menu away.
        let (cx, cy) = h.center_of("Open in new tab").unwrap();
        h.click(cx, cy);
        assert!(h.app.link_menu.is_none());
        assert_eq!(bible_tabs(&h.app).len(), 2);
        assert_eq!(h.app.bible().book, Book::NUMBERS, "the new tab shows it");
    }

    #[test]
    fn the_link_menu_closes_on_any_other_click_and_copies_the_link() {
        let mut app = App::new();
        let id = app.bid();
        app.link_menu = Some(LinkMenu {
            at: iced::Point::new(100.0, 100.0),
            spec: Spec::Verse { range: RefRange::parse("John 3:16").unwrap(), module: None },
            from: id,
        });
        let mut h = Harness::new(app);
        let (x, y) = h.center_of("Copy link").unwrap();
        h.click(x, y);
        assert!(h.app.link_menu.is_none());
        assert_eq!(h.app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Copied link"));
        // Elsewhere: gone, nothing done.
        h.app.link_menu = Some(LinkMenu {
            at: iced::Point::new(100.0, 100.0),
            spec: Spec::Note("Creation".into()),
            from: id,
        });
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        h.click(60.0, 900.0);
        assert!(h.app.link_menu.is_none());
    }

    #[test]
    fn right_clicking_a_link_in_a_note_works_too() {
        let mut app = App::new();
        app.open_note("notes/Nicodemus.md".into());
        let mut h = Harness::new(app);
        // The note pane starts at the middle of the window: scan it for a link.
        'scan: for y in (120..600).step_by(6) {
            for x in (540..1000).step_by(8) {
                right_click(&mut h, x as f32, y as f32);
                if h.app.link_menu.is_some() {
                    break 'scan;
                }
            }
        }
        let menu = h.app.link_menu.as_ref().expect("a menu");
        assert!(matches!(&menu.spec, Spec::Verse { range, .. } if range.to_string() == "John 3:2"), "{:?}", menu.spec);
        let (x, y) = h.center_of("Open in other pane").unwrap();
        h.click(x, y);
        assert!(!h.app.panes[1].tabs.is_empty() && bible_tabs(&h.app).len() == 2, "a second Bible tab, in the note's pane");
    }

    // ---- dictionary tabs -------------------------------------------------------------------

    fn dict_tabs(app: &App) -> Vec<TabId> {
        app.tabs.iter().filter(|(_, t)| matches!(t, Tab::Dict(_))).map(|(id, _)| *id).collect()
    }

    fn dict_of(app: &App, id: TabId) -> &DictTab {
        match &app.tabs[&id] {
            Tab::Dict(t) => t,
            _ => panic!("not a dictionary tab"),
        }
    }

    #[test]
    fn a_strongs_card_has_a_button_that_opens_the_definition_in_a_tab_beside_the_reader() {
        let mut app = App::new();
        app.bible_mut().show_strongs = true;
        let mut h = Harness::new(app);
        h.click_scan(is_word).expect("a Strong's word");
        let Some(NoteCard { content: NoteContent::Word { key, .. }, .. }) = &h.app.bible().note_card else { unreachable!() };
        let key = key.clone();
        let (x, y) = h.center_of("Open in tab").expect("a button on the card");
        h.click(x, y);
        let tabs = dict_tabs(&h.app);
        assert_eq!(tabs.len(), 1);
        let tab = dict_of(&h.app, tabs[0]);
        assert_eq!(tab.key, key);
        assert!(tab.entry.is_some(), "the definition is shown");
        assert_eq!(h.app.pane_of(tabs[0]), Some(1), "in the other pane, so the reader stays in view");
        assert_eq!(h.app.panes[1].active, Some(tabs[0]));
        assert!(h.app.bible().note_card.is_none(), "the pop-over is put away");
        // Another number goes to that same tab, not a new one.
        let bible = h.app.bid();
        let _ = h.app.update(Message::Bible(bible, BibleMsg::OpenWord("G2316".into())));
        assert_eq!(dict_tabs(&h.app).len(), 1);
        assert_eq!(dict_of(&h.app, tabs[0]).key, "G2316");
    }

    #[test]
    fn links_to_dictionaries_open_in_a_dictionary_tab_wherever_they_are_followed_from() {
        let mut app = App::new();
        let note = app.nid();
        // [[sword:StrongsGreek/G25]], a dictionary this machine may not have by that name.
        let link = LinkTarget::Sword { module: "StrongsGreek".into(), key: "G25".into() };
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link.clone())));
        assert_eq!(dict_tabs(&app).len(), 1);
        let first = dict_tabs(&app)[0];
        assert_eq!(dict_of(&app, first).key, "G25");
        assert_eq!(app.pane_of(first), Some(0), "beside the note it was followed from, not on top of it");
        // With keys: a new tab, another pane, a new pane.
        app.modifiers = Modifiers::COMMAND;
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link.clone())));
        assert_eq!(dict_tabs(&app).len(), 2);
        app.modifiers = Modifiers::SHIFT;
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link)));
        assert_eq!(dict_tabs(&app).len(), 3);
        assert_eq!(app.panes.len(), 3);
        // A dictionary that is not installed says so.
        app.modifiers = Modifiers::empty();
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(LinkTarget::Sword { module: "Webster".into(), key: "grace".into() })));
        assert!(app.sh.status.contains("not installed"), "{}", app.sh.status);
        // A number nobody defines too.
        let bible = app.bid();
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenWord("G99999".into())));
        assert!(app.sh.status.contains("No installed dictionary has G99999"));
    }

    #[test]
    fn the_dictionary_tab_looks_up_what_is_typed_and_says_when_there_is_nothing() {
        let mut app = App::new();
        let blank = app.new_blank();
        let id = app.add_tab(1, blank);
        let module = app.sh.backend.dictionaries()[0].id.clone();
        let _ = app.update(Message::Open(id, Choice::Dict(module, "Strong's (mock)".into())));
        assert!(dict_of(&app, id).entry.is_none(), "empty to begin with");
        let _ = app.update(Message::Dict(id, DictMsg::Draft("G25".into())));
        assert!(dict_of(&app, id).entry.is_none(), "typing does not look up");
        let _ = app.update(Message::Dict(id, DictMsg::Go));
        let tab = dict_of(&app, id);
        assert_eq!((tab.key.as_str(), tab.entry.as_ref().map(|e| e.lemma.as_str())), ("G25", Some("ἀγαπάω")));
        assert_eq!(tab.title(&app.sh), "G25 · Strong's (mock)");
        let _ = app.update(Message::Dict(id, DictMsg::Draft("  G7  ".into())));
        let _ = app.update(Message::Dict(id, DictMsg::Go));
        assert!(dict_of(&app, id).entry.is_none());
        let mut h = Harness::new(app);
        assert!(h.center_of("Strong's (mock) has no entry for \u{201C}G7\u{201D}.").is_some());
    }

    #[test]
    fn a_dictionary_entry_is_laid_out_in_the_tab() {
        let mut app = App::new();
        let module = app.sh.backend.dictionaries()[0].id.clone();
        let blank = app.new_blank();
        let id = app.add_tab(1, blank);
        let _ = app.update(Message::Open(id, Choice::Dict(module.clone(), String::new())));
        if let Some(Tab::Dict(t)) = app.tabs.get_mut(&id) {
            t.open(&app.sh, module, "G2316");
        }
        let mut h = Harness::new(app);
        let e = h.app.sh.backend.lexicon_entry("G2316").unwrap();
        for text in [e.lemma.as_str(), e.transliteration.as_str(), e.definition.as_str()] {
            assert!(h.center_of(text).is_some(), "{text}");
        }
        assert!(h.center_of("Go").is_some(), "the lookup button");
    }

    // ---- history ---------------------------------------------------------------------------

    fn ps23() -> RefRange {
        RefRange::parse("Ps 23:1").unwrap()
    }

    #[test]
    fn a_bible_tab_goes_back_and_forward_through_where_it_has_been() {
        let mut app = App::new();
        let id = app.panes[0].tabs[0];
        assert!(!app.bible().history.can_back());
        let _ = app.open_spec(app.panes[0].tabs[0], Spec::Verse { range: ps23(), module: None }, Dest::Here);
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::PSALMS, 23));
        let _ = app.update(Message::Bible(id, BibleMsg::PickChapter(ChapterChoice(Book::NUMBERS, 21))));
        let _ = app.update(Message::Navigate { back: true });
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::PSALMS, 23));
        let _ = app.update(Message::Navigate { back: true });
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3));
        assert!(!app.bible().history.can_back() && app.bible().history.can_forward());
        let _ = app.update(Message::Navigate { back: false });
        assert_eq!(app.bible().book, Book::PSALMS);
        // Somewhere new forgets what was ahead.
        let _ = app.open_spec(app.panes[0].tabs[0], Spec::Verse { range: RefRange::parse("Num 21:9").unwrap(), module: None }, Dest::Here);
        assert!(!app.bible().history.can_forward());
    }

    #[test]
    fn going_back_returns_to_how_far_down_the_chapter_was_read() {
        let mut app = App::new();
        let id = app.panes[0].tabs[0];
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(420.0)));
        let _ = app.open_spec(app.panes[0].tabs[0], Spec::Verse { range: ps23(), module: None }, Dest::Here);
        assert_eq!(app.bible().scroll_y, 0.0, "a new chapter starts at the top");
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(90.0)));
        let _ = app.update(Message::Navigate { back: true });
        assert_eq!(app.bible().scroll_y, 420.0);
        let _ = app.update(Message::Navigate { back: false });
        assert_eq!(app.bible().scroll_y, 90.0, "and forward likewise");
    }

    #[test]
    fn changing_module_counts_as_a_move_but_a_scroll_does_not() {
        let app = App::new();
        let here = app.bible().history.current().clone();
        let mut other = here.clone();
        other.module = ModuleId("Other".into());
        let mut h = history::History::new(here.clone());
        h.go(other.clone(), bible_tab::Place::is_same);
        assert!(h.can_back(), "the same chapter in another module is a new place");
        let mut scrolled = other.clone();
        scrolled.scroll = 300.0;
        h.go(scrolled, bible_tab::Place::is_same);
        assert_eq!(h.entries().0.len(), 2, "scrolling is not");
    }

    #[test]
    fn each_tab_has_its_own_history_and_keys_act_on_the_focused_one() {
        let mut app = App::new();
        let bible = app.panes[0].tabs[0];
        let note = app.nid();
        let _ = app.open_spec(app.panes[0].tabs[0], Spec::Verse { range: ps23(), module: None }, Dest::Here);
        // Focus is on the note's pane: the Bible tab does not move.
        app.focus = app.panes[1].id;
        let _ = app.update(Message::Navigate { back: true });
        assert_eq!(app.bible().book, Book::PSALMS);
        let other = "notes/Nicodemus.md".to_string();
        let _ = app.update(Message::Note(note, NoteMsg::PickNote(NoteChoice(other.clone()))));
        assert_eq!(app.note().path, other);
        let _ = app.update(Message::Navigate { back: true });
        assert_ne!(app.note().path, other);
        let _ = app.update(Message::Navigate { back: false });
        assert_eq!(app.note().path, other);
        let _ = bible;
    }

    #[test]
    fn a_deleted_note_drops_out_of_the_history_and_a_renamed_one_keeps_its_place() {
        let mut app = App::new();
        let note = app.nid();
        let first = app.note().path.clone();
        let other = "notes/Nicodemus.md".to_string();
        let _ = app.update(Message::Note(note, NoteMsg::PickNote(NoteChoice(other.clone()))));
        let _ = app.update(Message::Note(note, NoteMsg::StartRename));
        let _ = app.update(Message::Note(note, NoteMsg::RenameDraft("Renamed".into())));
        let _ = app.update(Message::Note(note, NoteMsg::CommitRename));
        let _ = app.update(Message::Navigate { back: true });
        assert_eq!(app.note().path, first);
        let _ = app.update(Message::Navigate { back: false });
        assert_eq!(app.note().path, "notes/Renamed.md", "the entry follows the rename");
    }

    #[test]
    fn the_dictionary_tab_steps_through_the_entries_it_showed() {
        let mut app = App::new();
        let from = app.nid();
        let _ = app.open_spec(from, Spec::Word { key: "G25".into() }, Dest::Here);
        let id = dict_tabs(&app)[0];
        let _ = app.update(Message::Dict(id, DictMsg::Draft("G26".into())));
        let _ = app.update(Message::Dict(id, DictMsg::Go));
        let key = |app: &App| match &app.tabs[&id] {
            Tab::Dict(d) => d.key.clone(),
            _ => unreachable!(),
        };
        assert_eq!(key(&app), "G26");
        let _ = app.update(Message::Navigate { back: true });
        assert_eq!(key(&app), "G25");
    }

    #[test]
    fn chapter_buttons_step_through_the_modules_chapters_and_stop_at_the_ends() {
        let mut app = App::new();
        let id = app.panes[0].tabs[0];
        // The mock's chapters are Psalm 23, Numbers 21, John 3 (the last is where it starts).
        let _ = app.update(Message::Bible(id, BibleMsg::NextChapter));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3), "nothing after the last");
        let _ = app.update(Message::Bible(id, BibleMsg::PrevChapter));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::NUMBERS, 21));
        let _ = app.update(Message::Bible(id, BibleMsg::PrevChapter));
        let _ = app.update(Message::Bible(id, BibleMsg::PrevChapter));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::PSALMS, 23));
        assert!(app.bible().history.can_back(), "each step is a move Back undoes");
        let mut h = Harness::new(app);
        assert!(h.center_of("Numbers 21 \u{2192}").is_some(), "the link names where it goes");
    }

    #[test]
    fn the_note_menu_holds_new_rename_and_delete_and_closes_when_used_or_left() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        assert!(app.note().menu_open);
        let mut h = Harness::new(app);
        assert!(h.center_of("New note\u{2026}").is_some(), "a new note is started from the menu");
        assert!(h.center_of("Rename").is_some() && h.center_of("Delete").is_some(), "with rename and delete");
        let mut app = h.app;
        let _ = app.update(Message::Note(id, NoteMsg::NewNote));
        assert!(!app.note().menu_open && app.note().choosing, "choosing it closes the menu and asks what kind");
        let _ = app.update(Message::Note(id, NoteMsg::CancelNew));
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        let _ = app.update(Message::Note(id, NoteMsg::StartRename));
        assert!(!app.note().menu_open && app.note().renaming.is_some());
        let _ = app.update(Message::Note(id, NoteMsg::CancelRename));
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        let _ = app.update(Message::Note(id, NoteMsg::AskDelete));
        assert!(!app.note().menu_open && app.note().confirm_delete);
        let _ = app.update(Message::Note(id, NoteMsg::CancelDelete));
        // Acting in another pane puts it away.
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        let _ = app.update(Message::Bible(app.bid(), BibleMsg::ToggleMenu));
        assert!(!app.note().menu_open);
    }

    #[test]
    fn the_arrows_are_only_live_when_there_is_somewhere_to_go() {
        let mut app = App::new();
        let _ = app.open_spec(app.panes[0].tabs[0], Spec::Verse { range: ps23(), module: None }, Dest::Here);
        assert!(app.bible().history.can_back() && !app.bible().history.can_forward());
    }

    // ---- dragging --------------------------------------------------------------------------

    fn p(x: f32, y: f32) -> iced::Point {
        iced::Point::new(x, y)
    }

    /// Press a tab and drag it a good way, as the pointer would (the tab, then moves).
    fn pick_up(app: &mut App, tab: TabId) {
        let _ = app.update(Message::TabPressed(tab));
        let _ = app.update(Message::PointerMoved(p(100.0, 40.0)));
        let _ = app.update(Message::PointerMoved(p(140.0, 60.0)));
    }

    fn drop_on(app: &mut App, over: Over) {
        let _ = app.update(Message::DragOver(over));
        let _ = app.update(Message::PointerUp);
    }

    #[test]
    fn a_tab_pressed_and_moved_is_dragged_but_a_wobble_or_a_click_is_not() {
        let mut app = App::new();
        let note = app.nid();
        let _ = app.update(Message::TabPressed(note));
        assert!(app.pressed.is_some() && app.drag.is_none(), "pressing only selects");
        let _ = app.update(Message::PointerMoved(p(100.0, 40.0)));
        let _ = app.update(Message::PointerMoved(p(103.0, 42.0)));
        assert!(app.drag.is_none(), "a wobble is not a drag");
        let _ = app.update(Message::PointerUp);
        assert!(app.pressed.is_none() && app.drag.is_none(), "a click ends cleanly");
        pick_up(&mut app, note);
        assert!(matches!(&app.drag, Some(Drag { payload: Payload::Tab(t), .. }) if *t == note));
        assert!(app.pressed.is_none());
        let _ = app.update(Message::PointerMoved(p(300.0, 200.0)));
        assert_eq!(app.drag.as_ref().unwrap().at, p(300.0, 200.0), "it follows the pointer");
    }

    #[test]
    fn dropping_a_tab_on_another_pane_moves_it_there_and_an_emptied_pane_goes() {
        let mut app = App::new();
        let (left, right) = (app.panes[0].id, app.panes[1].id);
        let note = app.nid();
        pick_up(&mut app, note);
        drop_on(&mut app, Over::Pane(left, Zone::Center));
        assert_eq!(app.panes.len(), 1, "the pane it left had nothing else, and is gone");
        assert_eq!(app.panes[0].tabs.len(), 2);
        assert_eq!(app.panes[0].active, Some(note), "and it is the tab showing");
        assert_eq!(app.layout.leaves(), [left]);
        let _ = right;
        assert!(app.drag.is_none());
    }

    #[test]
    fn dropping_on_the_bar_or_the_same_pane_puts_a_tab_at_the_end() {
        let mut app = App::new();
        app.add_bible(0);
        let first = app.panes[0].tabs[0];
        let pane = app.panes[0].id;
        pick_up(&mut app, first);
        drop_on(&mut app, Over::Bar(pane));
        assert_eq!(app.panes[0].tabs.last(), Some(&first));
        assert_eq!(app.panes.len(), 2, "nothing else changed");
    }

    #[test]
    fn dropping_a_tab_on_another_tab_puts_it_in_front_of_that_tab() {
        let mut app = App::new();
        app.add_bible(0);
        app.add_bible(0);
        let ids = app.panes[0].tabs.clone();
        pick_up(&mut app, ids[2]);
        drop_on(&mut app, Over::Chip(ids[0]));
        assert_eq!(app.panes[0].tabs, [ids[2], ids[0], ids[1]]);
        assert_eq!(app.panes[0].active, Some(ids[2]));
        // From another pane too: it leaves its pane and joins this order.
        let note = app.nid();
        pick_up(&mut app, note);
        drop_on(&mut app, Over::Chip(ids[1]));
        assert_eq!(app.panes.len(), 1);
        assert_eq!(app.panes[0].tabs, [ids[2], ids[0], note, ids[1]]);
        // Onto itself changes nothing.
        pick_up(&mut app, note);
        drop_on(&mut app, Over::Chip(note));
        assert_eq!(app.panes[0].tabs, [ids[2], ids[0], note, ids[1]]);
    }

    #[test]
    fn dropping_a_tab_on_an_edge_splits_that_pane_with_the_tab_in_the_new_half() {
        for (zone, axis, before) in [
            (Zone::Left, panes::Axis::Row, true),
            (Zone::Right, panes::Axis::Row, false),
            (Zone::Top, panes::Axis::Column, true),
            (Zone::Bottom, panes::Axis::Column, false),
        ] {
            let mut app = App::new();
            let moved = app.add_bible(0);
            let (left, original) = (app.panes[0].id, app.panes[0].tabs[0]);
            pick_up(&mut app, moved);
            drop_on(&mut app, Over::Pane(left, zone));
            assert_eq!(app.panes.len(), 3, "{zone:?}");
            let new = app.pane_of(moved).map(|i| app.panes[i].id).unwrap();
            assert_ne!(new, left);
            assert_eq!(app.panes[app.pane_index(left).unwrap()].tabs, [original], "{zone:?}: the old pane kept the rest");
            let leaves = app.layout.leaves();
            let (at_new, at_old) = (leaves.iter().position(|&p| p == new).unwrap(), leaves.iter().position(|&p| p == left).unwrap());
            assert_eq!(at_new < at_old, before, "{zone:?}: the new pane is {}", if before { "first" } else { "second" });
            // The split's direction.
            fn axis_of(node: &panes::Node, pane: PaneId) -> Option<panes::Axis> {
                match node {
                    panes::Node::Leaf(_) => None,
                    panes::Node::Split { axis, a, b, .. } => {
                        let direct = matches!(**a, panes::Node::Leaf(p) if p == pane) || matches!(**b, panes::Node::Leaf(p) if p == pane);
                        if direct { Some(*axis) } else { axis_of(a, pane).or_else(|| axis_of(b, pane)) }
                    }
                }
            }
            assert_eq!(axis_of(&app.layout, new), Some(axis), "{zone:?}");
            assert_eq!(app.panes[app.pane_index(new).unwrap()].active, Some(moved));
        }
    }

    #[test]
    fn a_pane_cannot_be_split_to_make_room_for_its_own_only_tab() {
        let mut app = App::new();
        let note = app.nid();
        let right = app.panes[1].id;
        pick_up(&mut app, note);
        drop_on(&mut app, Over::Pane(right, Zone::Left));
        assert_eq!(app.panes.len(), 2);
        assert_eq!(app.pane_of(note), Some(1));
    }

    #[test]
    fn dropping_a_link_on_a_pane_opens_it_in_a_new_tab_there_and_on_an_edge_in_a_new_pane() {
        let mut app = App::new();
        let (left, right) = (app.panes[0].id, app.panes[1].id);
        let verse = |r: &str| Spec::Verse { range: RefRange::parse(r).unwrap(), module: None };
        // The middle of the right pane (the note's): a Bible tab joins it.
        app.start_drag(Payload::Link(verse("Ps 23:1")), p(10.0, 10.0));
        drop_on(&mut app, Over::Pane(right, Zone::Center));
        assert_eq!(app.panes[1].tabs.len(), 2);
        let shown = app.panes[1].active.unwrap();
        assert_eq!((bible_of(&app, shown).book, bible_of(&app, shown).chapter_no), (Book::PSALMS, 23));
        assert_eq!(bible_of(&app, app.panes[0].tabs[0]).book, Book::JOHN, "the other Bible tab did not move");
        // An edge: a new pane, holding only it.
        app.start_drag(Payload::Link(verse("Num 21:9")), p(10.0, 10.0));
        drop_on(&mut app, Over::Pane(left, Zone::Bottom));
        assert_eq!(app.panes.len(), 3);
        let new = app.panes.last().unwrap();
        assert_eq!(new.tabs.len(), 1);
        assert_eq!(bible_of(&app, new.tabs[0]).book, Book::NUMBERS);
        // Notes and dictionary entries are carried the same way.
        app.start_drag(Payload::Link(Spec::Note("Memory verses".into())), p(10.0, 10.0));
        drop_on(&mut app, Over::Pane(left, Zone::Center));
        let note_tab = app.panes[app.pane_index(left).unwrap()].active.unwrap();
        assert!(matches!(&app.tabs[&note_tab], Tab::Note(n) if n.path == "notes/Memory verses.md"));
        app.start_drag(Payload::Link(Spec::Word { key: "G25".into() }), p(10.0, 10.0));
        drop_on(&mut app, Over::Pane(left, Zone::Right));
        assert_eq!(app.panes.len(), 4);
        assert_eq!(dict_tabs(&app).len(), 1);
    }

    #[test]
    fn a_link_that_cannot_be_opened_leaves_no_empty_pane_behind() {
        let mut app = App::new();
        let left = app.panes[0].id;
        app.start_drag(Payload::Link(Spec::Note("Nowhere".into())), p(10.0, 10.0));
        drop_on(&mut app, Over::Pane(left, Zone::Right));
        assert_eq!(app.panes.len(), 2, "no pane was made");
        assert!(app.sh.status.contains("No note named"));
    }

    #[test]
    fn a_drag_can_be_cancelled_or_dropped_nowhere() {
        let mut app = App::new();
        let note = app.nid();
        let panes_before = app.panes.len();
        pick_up(&mut app, note);
        let _ = app.update(Message::DragCancel);
        assert!(app.drag.is_none() && app.pressed.is_none());
        // Dropped where nothing is: nothing happens.
        pick_up(&mut app, note);
        let _ = app.update(Message::PointerUp);
        assert!(app.drag.is_none());
        // Left a drop place before the button came up: nothing happens either.
        pick_up(&mut app, note);
        let over = Over::Pane(app.panes[0].id, Zone::Center);
        let _ = app.update(Message::DragOver(over));
        let _ = app.update(Message::DragOut(over));
        let _ = app.update(Message::PointerUp);
        assert_eq!(app.panes.len(), panes_before);
        assert_eq!(app.pane_of(note), Some(1));
    }

    #[test]
    fn the_pointer_over_a_pane_lights_the_zone_it_is_in() {
        let mut app = App::new();
        let note = app.nid();
        let (left, right) = (app.panes[0].id, app.panes[1].id);
        pick_up(&mut app, note);
        let mut h = Harness::new(app);
        // The window is 1024 wide and 1500 tall: the right pane is the right half.
        for (x, y, zone) in [
            (560.0, 700.0, Zone::Left),
            (770.0, 700.0, Zone::Center),
            (1000.0, 700.0, Zone::Right),
            (770.0, 180.0, Zone::Top),
            (770.0, 1400.0, Zone::Bottom),
        ] {
            h.move_to(x, y);
            assert_eq!(h.app.drag.as_ref().unwrap().over, Some(Over::Pane(right, zone)), "at {x},{y}");
        }
        h.move_to(250.0, 700.0);
        assert_eq!(h.app.drag.as_ref().unwrap().over, Some(Over::Pane(left, Zone::Center)));
        // Over the tab bar: the empty part, then a tab.
        h.move_to(500.0, 50.0);
        assert!(matches!(h.app.drag.as_ref().unwrap().over, Some(Over::Bar(p)) if p == left));
        let first = h.app.panes[0].tabs[0];
        let b = h.bounds_of_text(&h.app.bible().title(&h.app.sh)).expect("the Bible tab's name");
        h.move_to(b.x + 4.0, b.y + 4.0);
        assert_eq!(h.app.drag.as_ref().unwrap().over, Some(Over::Chip(first)));
        // Dropping there puts the note tab in front of it.
        let _ = h.app.update(Message::PointerUp);
        assert_eq!(h.app.panes[0].tabs[0], note);
    }

    #[test]
    fn the_drop_layer_is_only_there_while_dragging_and_does_not_disturb_the_reader() {
        let mut app = App::new();
        app.bible_mut().show_strongs = true;
        let mut h = Harness::new(app);
        h.click_scan(|a| a.bible().text_selection.is_some()).expect("a selection");
        // Let anything the click set going settle first.
        for _ in 0..2 {
            h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        }
        let before = h.app.bible().text_selection.clone();
        let note = h.app.nid();
        pick_up(&mut h.app, note);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert_eq!(h.app.bible().text_selection, before, "the reader kept its selection");
        let _ = h.app.update(Message::DragCancel);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert_eq!(h.app.bible().text_selection, before);
    }

    #[test]
    fn pulling_a_link_in_the_reader_picks_it_up_and_a_click_on_it_still_follows_it() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
        // Find the "(Num 21:9)" reference by right clicking until a menu appears.
        let mut found = None;
        'scan: for y in (140..1400).step_by(10) {
            for x in (50..560).step_by(8) {
                right_click(&mut h, x as f32, y as f32);
                if h.app.link_menu.is_some() {
                    found = Some((x as f32, y as f32));
                    break 'scan;
                }
            }
        }
        let (x, y) = found.expect("a link");
        h.app.link_menu = None;
        // A click on it follows it.
        h.click(x, y);
        assert_eq!(h.app.bible().book, Book::NUMBERS, "clicking still follows");
        h.app.load(Book::JOHN, 3);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        // Pulled away from it, it is carried.
        h.move_to(x, y);
        h.press();
        h.move_to(x + 30.0, y + 30.0);
        match &h.app.drag {
            Some(Drag { payload: Payload::Link(Spec::Verse { range, .. }), .. }) => assert_eq!(range.to_string(), "Numbers 21:9"),
            other => panic!("not carried: {other:?}"),
        }
        h.release();
        assert_eq!(h.app.bible().book, Book::JOHN, "and it was not followed");
        assert!(h.app.bible().text_selection.is_none(), "or made a selection");
    }

    // ---- renaming ----------------------------------------------------------------------

    #[test]
    fn renaming_moves_the_file_follows_the_title_and_rewrites_links_to_it() {
        let mut app = App::new();
        // A second tab on the same note, a note linking to it, and a mark whose comment does.
        app.add_note(1);
        app.sh.notes.insert("notes/Nicodemus.md".into(), "---\ntitle: Nicodemus\n---\n\nSee [[Memory verses|my verses]] and [[Memory verses]].\n".into());
        app.sh.notes.insert("notes/Memory verses.md".into(), "# Memory verses\n".into());
        app.sh.marks[0].body = "Also [[Nicodemus]].".into();
        let mark_path = app.sh.marks[0].path();
        let mark_text = app.sh.marks[0].to_markdown();
        app.sh.notes.insert(mark_path.clone(), mark_text);
        let (a, b) = (app.panes[1].tabs[0], app.panes[1].tabs[1]);
        for id in [a, b] {
            let _ = app.update(Message::Note(id, NoteMsg::PickNote(NoteChoice("notes/Nicodemus.md".into()))));
        }
        let _ = app.update(Message::Note(a, NoteMsg::StartRename));
        assert_eq!(app.tabs.get(&a).map(|t| matches!(t, Tab::Note(n) if n.renaming.as_deref() == Some("Nicodemus"))), Some(true));
        let _ = app.update(Message::Note(a, NoteMsg::RenameDraft("  The Pharisee  ".into())));
        let _ = app.update(Message::Note(a, NoteMsg::CommitRename));

        assert!(!app.sh.notes.contains_key("notes/Nicodemus.md"), "the old file name is gone");
        let text = &app.sh.notes["notes/The Pharisee.md"];
        assert!(text.contains("title: The Pharisee"), "the title that repeated the name follows it: {text}");
        assert!(text.contains("See [[Memory verses|my verses]]"), "its own links are untouched");
        assert!(app.sh.toast.as_ref().is_some_and(|t| t.text.contains("The Pharisee")));
        for id in [a, b] {
            let Some(Tab::Note(n)) = app.tabs.get(&id) else { unreachable!() };
            assert_eq!(n.path, "notes/The Pharisee.md", "every tab on it follows");
            assert!(n.renaming.is_none());
            assert!(n.doc.source().contains("title: The Pharisee"), "and shows the new text");
        }
        // The mark's comment linked to it by name: the file and the mark both say the new name.
        assert_eq!(app.sh.marks[0].body, "Also [[The Pharisee]].");
        assert!(app.sh.notes[&mark_path].contains("[[The Pharisee]]"));
    }

    #[test]
    fn a_title_you_wrote_yourself_is_left_alone() {
        let mut app = App::new();
        app.sh.notes.insert("notes/A.md".into(), "---\ntitle: My own title\n---\n\nText\n".into());
        app.open_note("notes/A.md".into());
        assert_eq!(app.sh.rename_note("notes/A.md", "B").unwrap(), "notes/B.md");
        assert!(app.sh.notes["notes/B.md"].contains("title: My own title"));
        // A note with no frontmatter does not get one.
        app.sh.notes.insert("notes/C.md".into(), "Plain\n".into());
        app.sh.rename_note("notes/C.md", "D").unwrap();
        assert_eq!(app.sh.notes["notes/D.md"], "Plain\n");
    }

    #[test]
    fn names_that_cannot_be_used_are_refused_and_the_box_stays_open() {
        let mut app = App::new();
        let id = app.nid();
        let start = app.note().path.clone();
        let _ = app.update(Message::Note(id, NoteMsg::StartRename));
        for (bad, why) in [
            ("   ", "needs a name"),
            ("a/b", "cannot"),
            ("what?", "cannot"),
            (".hidden", "cannot"),
            ("Nicodemus", "already exists"),
            ("nicodemus", "already exists"),
        ] {
            let _ = app.update(Message::Note(id, NoteMsg::RenameDraft(bad.into())));
            let _ = app.update(Message::Note(id, NoteMsg::CommitRename));
            assert_eq!(app.note().path, start, "{bad:?} did not rename it");
            assert!(app.note().renaming.is_some(), "{bad:?}: still being named");
            assert!(app.sh.status.contains(why), "{bad:?} -> {}", app.sh.status);
        }
        // The same name again is no change, and cancelling leaves things as they were.
        let _ = app.update(Message::Note(id, NoteMsg::RenameDraft(crate::note_tab::file_stem(&start))));
        let _ = app.update(Message::Note(id, NoteMsg::CommitRename));
        assert_eq!(app.note().path, start);
        assert!(app.note().renaming.is_none());
        let _ = app.update(Message::Note(id, NoteMsg::StartRename));
        let _ = app.update(Message::Note(id, NoteMsg::CancelRename));
        assert!(app.note().renaming.is_none());
    }

    #[test]
    fn a_margin_note_cannot_be_renamed() {
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::StartRename));
        assert!(app.note().renaming.is_none());
        assert!(app.sh.rename_note(&app.note().path.clone(), "x").is_err());
    }

    #[test]
    fn clicking_elsewhere_stops_a_rename() {
        let mut app = App::new();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::StartRename));
        let bible = app.bid();
        let _ = app.update(Message::Bible(bible, BibleMsg::ToggleStrongs(true)));
        assert!(app.note().renaming.is_none());
    }

    #[test]
    fn a_rename_is_carried_to_the_vault_on_disk_even_when_only_capitals_change() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        app.sh.notes.insert("notes/Lower case.md".into(), "text\n".into());
        app.sh.save();
        let id = app.nid();
        app.open_note("notes/Lower case.md".into());
        let on_disk = |app: &App, p: &str| app.persist_ref().unwrap().vault().read(p).is_ok();
        assert!(on_disk(&app, "notes/Lower case.md"));
        for name in ["Fresh start", "FRESH START"] {
            let _ = app.update(Message::Note(id, NoteMsg::StartRename));
            let _ = app.update(Message::Note(id, NoteMsg::RenameDraft(name.into())));
            let _ = app.update(Message::Note(id, NoteMsg::CommitRename));
            assert_eq!(app.note().path, format!("notes/{name}.md"));
            assert!(on_disk(&app, &format!("notes/{name}.md")), "{name}: the new file is there");
        }
        assert!(!on_disk(&app, "notes/Lower case.md") && !on_disk(&app, "notes/Fresh start.md") || on_disk(&app, "notes/FRESH START.md"));
        // And what is on disk reads back as the same notes after a restart.
        drop(app);
        let again = live_app(home.path()).unwrap();
        assert!(again.sh.notes.contains_key("notes/FRESH START.md"));
        assert!(!again.sh.notes.contains_key("notes/Lower case.md"));
    }

    /// A margin with one card being edited: (harness, the point in the empty margin below it).
    fn editing_in_the_margin() -> (Harness, (f32, f32)) {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        let mut h = Harness::new(app);
        let b = { let id = h.app.bible().card_editor_id(); h.bounds_of(id) };
        h.click(b.x + 20.0, b.y + 10.0);
        // The empty part of the margin, well below every card.
        (h, (b.x + 20.0, b.y + 1100.0))
    }

    #[test]
    fn clicking_away_from_a_card_being_edited_finishes_the_edit() {
        let (mut h, away) = editing_in_the_margin();
        h.type_text(" kept");
        assert_eq!(h.app.bible().editing, Some(0));
        h.click(away.0, away.1);
        assert!(h.app.bible().editing.is_none(), "a click outside the card ends the edit");
        assert!(h.app.sh.marks[0].body.contains(" kept"), "what was typed is kept: {:?}", h.app.sh.marks[0].body);
    }

    #[test]
    fn clicking_inside_the_card_being_edited_does_not_end_it() {
        let (mut h, _) = editing_in_the_margin();
        let b = { let id = h.app.bible().card_editor_id(); h.bounds_of(id) };
        h.click(b.x + 40.0, b.y + 10.0);
        h.click(b.x + 90.0, b.y + 10.0);
        assert_eq!(h.app.bible().editing, Some(0));
    }

    #[test]
    fn clicking_another_card_moves_the_edit_to_it() {
        let (mut h, _) = editing_in_the_margin();
        // The other note is underneath the one being written (they are one stack), so it is
        // reached through its mark in the text.
        let id = h.app.bid();
        let _ = h.app.update(Message::Bible(id, BibleMsg::MarksClicked(vec![1])));
        assert_eq!(h.app.bible().editing, Some(1), "now writing in the card that was clicked");
    }

    #[test]
    fn an_empty_comment_is_dropped_when_you_click_away() {
        let mut app = blank_app();
        app.zoomed = Some(app.panes[0].id);
        select_so_loved(&mut app);
        app.bible_mut().margin_showing = true;
        let _ = app.update(app.b(BibleMsg::AddComment));
        assert_eq!(app.sh.marks.len(), 1);
        let mut h = Harness::new(app);
        let b = { let id = h.app.bible().card_editor_id(); h.bounds_of(id) };
        h.click(b.x + 600.0, b.y + 1100.0);
        assert!(h.app.sh.marks.is_empty(), "nothing written, nothing kept");
        assert!(h.app.bible().editing.is_none());
    }

    #[test]
    fn editing_a_card_starts_after_its_last_character_not_on_a_new_line() {
        let mut app = App::new();
        let body = app.sh.marks[0].body.clone();
        assert!(!body.ends_with('\n'), "the body does not carry the file's final newline");
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        assert_eq!(app.bible().card_doc.selection().head, body.trim_end().len());
        let _ = app.update(app.b(BibleMsg::CardAction(EditAction::Insert("!".into()))));
        assert!(app.bible().card_doc.source().starts_with(body.trim_end()));
        assert!(app.bible().card_doc.source().ends_with('!'));
        assert_eq!(app.bible().card_doc.source().matches('\n').count(), body.matches('\n').count(), "no new line");
    }
}
