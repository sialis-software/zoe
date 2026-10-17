//! A study workspace: panes holding tabs. A tab is a Bible reader or a note; each kind is a
//! self-contained module (`bible_tab`, `note_tab`) that keeps its own state and works on the
//! `Shared` notes, marks and backend. This file lays the tabs out in panes and carries out
//! what one tab asks of another (a note's link shows a verse in a Bible tab).

// A Windows release is a window program: without this it opens a console beside the window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod about;
mod backlinks_tab;
mod bible_search;
mod bible_tab;
mod bundled;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod commands;
mod commentary_tab;
mod completion;
mod dict_tab;
mod drag;
mod fonts;
mod history;
mod icons;
mod live;
mod module_folders;
mod module_manager;
mod module_repo;
mod link_area;
mod log_tab;
mod live_editor;
mod native;
mod network;
mod note_tab;
mod opening;
mod palette;
mod panes;
mod persist;
mod popover;
mod plan_tab;
mod plans_window;
mod reader;
mod reader_cards;
mod reader_text;
mod search_tab;
mod speech;
mod shared;
mod split;
mod upgrade;
mod state;
mod vault_setup;
mod workspace;

use backlinks_tab::{BacklinksMsg, BacklinksTab, Target};
use bible_tab::{BibleMsg, BibleTab};
use commentary_tab::{CommentaryMsg, CommentaryTab};
use plan_tab::{PlanMsg, PlanTab};
use dict_tab::{DictMsg, DictTab};
use drag::{Drag, Over, Payload, Pressed, Zone};
use iced::widget::operation::{AbsoluteOffset, RelativeOffset, scroll_to, snap_to};
use iced::widget::{button, column, container, pick_list, row, rule, space, stack, text, text_input, tooltip};
use iced::{Element, Length, Task, Theme};
use log_tab::LogTab;
use note_tab::{NoteMsg, NoteTab};
use opening::{Dest, Spec};
use search_tab::{SearchMsg, SearchTab};
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

/// The space above and below the logo in the top bar (which is the logo's 24 px and this twice).
const TOP_BAR_PADDING: f32 = 9.0;
/// The height of the top bar and the line under it.
const TOP_BAR_HEIGHT: f32 = 24.0 + 2.0 * TOP_BAR_PADDING + 1.0;

/// What a tab was before a link took it over, kept whole (with its place, its selection and its own
/// history) to go back to; and what it was when it went back, to go forward to.
#[derive(Default)]
struct Nav {
    back: Vec<Tab>,
    forward: Vec<Tab>,
}

/// What a tab holds. Each kind is one self-contained type of tab.
enum Tab {
    /// A new tab: it offers a choice of what to open, then becomes that.
    Blank(TabId),
    Bible(Box<BibleTab>),
    Note(Box<NoteTab>),
    Dict(Box<DictTab>),
    /// A search of the words of a Bible.
    Search(Box<SearchTab>),
    /// A commentary's notes on a chapter.
    Commentary(Box<CommentaryTab>),
    /// The notes that link to a passage.
    Backlinks(Box<BacklinksTab>),
    /// The reading log, newest first.
    Log(Box<LogTab>),
    /// The plans (reading plans), read a day at a time.
    Plan(Box<PlanTab>),
}

/// A column of tabs, one of them showing.
struct Pane {
    id: PaneId,
    tabs: Vec<TabId>,
    active: Option<TabId>,
}

/// A page of the settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    Appearance,
    Pens,
    Vault,
    Modules,
    Reading,
    Privacy,
    About,
}

impl SettingsTab {
    const ALL: [SettingsTab; 7] = [
        SettingsTab::Appearance,
        SettingsTab::Pens,
        SettingsTab::Vault,
        SettingsTab::Modules,
        SettingsTab::Reading,
        SettingsTab::Privacy,
        SettingsTab::About,
    ];

    fn label(self) -> &'static str {
        match self {
            SettingsTab::Appearance => "Appearance",
            SettingsTab::Pens => "Pens",
            SettingsTab::Vault => "Vault",
            SettingsTab::Modules => "Modules",
            SettingsTab::Reading => "Reading",
            SettingsTab::Privacy => "Privacy",
            SettingsTab::About => "About",
        }
    }
}

/// A group of settings: a heading over a panel a shade off the page.
fn section<'a>(title: &'static str, content: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text(title).size(15),
        container(content).padding(14).width(Length::Fill).style(|theme: &Theme| {
            let p = theme.extended_palette();
            container::Style {
                background: Some(p.background.weak.color.into()),
                border: iced::Border { color: iced::Color { a: 0.5, ..p.background.strong.color }, width: 1.0, radius: 8.0.into() },
                ..container::Style::default()
            }
        }),
    ]
    .spacing(8)
    .into()
}

/// The colors a pen can be, as a grid of dots with `chosen` ringed; `on` makes the message for the
/// one pressed (its place in `state::PEN_COLORS`).
fn color_grid<'a, M: Clone + 'a>(chosen: usize, on: impl Fn(usize) -> M) -> Element<'a, M> {
    let mut grid = column![].spacing(6);
    let rows = state::PEN_COLORS.len() / (state::PEN_HUES + 1);
    for r in 0..rows {
        let mut cells = row![].spacing(6);
        // The row's hues, then its grey (which come after all the hues in the list).
        let places = (0..state::PEN_HUES).map(|c| r * state::PEN_HUES + c).chain(std::iter::once(rows * state::PEN_HUES + r));
        for i in places {
            let color = bible_tab::hex_color(state::PEN_COLORS[i], 1.0).unwrap_or(iced::Color::WHITE);
            let ringed = i == chosen % state::PEN_COLORS.len().max(1) && chosen != usize::MAX;
            cells = cells.push(
                button(space::horizontal())
                    .width(22)
                    .height(22)
                    .padding(0)
                    .style(move |theme: &Theme, _| button::Style {
                        background: Some(color.into()),
                        border: iced::Border {
                            radius: 11.0.into(),
                            color: if ringed { theme.extended_palette().background.base.text } else { iced::Color { a: 0.3, ..iced::Color::WHITE } },
                            width: if ringed { 2.5 } else { 1.0 },
                        },
                        ..button::Style::default()
                    })
                    .on_press(on(i)),
            );
        }
        grid = grid.push(cells);
    }
    grid.into()
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

/// What a blank tab can become: a kind of tab. Which Bible, dictionary or note it shows is
/// chosen in the tab afterwards, with its own controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Bible,
    Dict,
    Commentary,
    Search,
    Note,
    Plans,
    /// The reading log.
    Log,
}

/// A chapter being looked at, and since when.
struct Reading {
    tab: TabId,
    book: scripture::Book,
    chapter: u16,
    since: std::time::Instant,
    /// Whether it has been put in the reading log already.
    logged: bool,
}

/// How long a chapter must be in front of the reader to count as read, and how often that is looked at.
const READING_TICK: std::time::Duration = std::time::Duration::from_secs(5);

struct App {
    sh: Shared,
    tabs: BTreeMap<TabId, Tab>,
    /// The tabs that a tab took over from, and ones it went back from: by the id of the tab showing.
    navs: BTreeMap<TabId, Nav>,
    panes: Vec<Pane>,
    /// How the panes are divided up.
    layout: Node,
    /// The pane that last had activity: where new tabs go.
    focus: PaneId,
    /// The pane shown alone, filling the window, if any.
    zoomed: Option<PaneId>,
    /// The main window's size, as last reported.
    window_size: iced::Size,
    /// A reader put back at a share of the way down after it changed width (reader mode begun or ended): which, how far, and
    /// how many times that has been asked for (a new scrollable may start from the top once more before it obeys).
    pending_scroll: Option<(TabId, f32, u8)>,
    /// What the menu bar was last told (None: nothing yet).
    menu_synced: Option<commands::MenuState>,
    /// The Bible tab read alone, filling the window with nothing else around it, if any.
    reader_mode: Option<ReaderMode>,
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
    /// The chapter in front of the reader, for the reading log.
    reading: Option<Reading>,
    /// The tabs that were showing at the last update, to tell which came into view and which left.
    shown: std::collections::BTreeSet<TabId>,
    /// Something being carried across the window, and a tab pressed that may become that.
    drag: Option<Drag>,
    pressed: Option<Pressed>,
    /// The window with the workspace, and the settings window while it is open.
    main: iced::window::Id,
    settings_window: Option<iced::window::Id>,
    /// The window that sets up the vault on a first run, and the folder picked for it (none: the
    /// app's own data folder).
    welcome_window: Option<iced::window::Id>,
    welcome_folder: Option<std::path::PathBuf>,
    welcome_error: Option<String>,
    /// The module manager's window while it is open, and what it shows.
    manager_window: Option<iced::window::Id>,
    manager: module_manager::Manager,
    /// The plans window while it is open, and what it shows.
    plans_window: Option<iced::window::Id>,
    plans: plans_window::Plans,
    /// The plans and how far each has got, made again after each message while a plan tab is open.
    plan_cache: Vec<plans_window::Entry>,
    /// The layouts panel, while it is open.
    panel: Option<Panel>,
    /// The tag typed for a new pen, and the color it will have (an index into `state::PEN_COLORS`).
    pen_draft: String,
    pen_color: usize,
    /// The pen whose colors are showing in the settings.
    pen_editing: Option<usize>,
    settings_tab: SettingsTab,
    /// What is typed in the box that names a layout to save, and whether that box is showing.
    layout_draft: String,
    naming: bool,
    /// The data folder, when there is one: the vault choice, settings and session are kept in it.
    home: Option<std::path::PathBuf>,
    /// Where the person's choices are kept, when that is not the data folder.
    config: Option<std::path::PathBuf>,
    /// Set while there is no vault: the welcome window asks for one.
    onboarding: Option<live::Onboarding>,
    /// No SWORD module was found at start-up: sample text stands in, and no session is kept.
    no_modules: bool,
    /// The card that confirms a folder for a vault, while it is open.
    setup: Option<vault_setup::Setup>,
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

/// A Bible tab shown alone. It ends when the number of tabs changes or another comes forward.
#[derive(Clone, Copy)]
struct ReaderMode {
    tab: TabId,
    tabs: usize,
}

#[derive(Debug, Clone)]
enum Message {
    Bible(TabId, BibleMsg),
    Note(TabId, NoteMsg),
    /// The reading log's tab (the same messages as a note's, of which it lets few through).
    Log(TabId, NoteMsg),
    Dict(TabId, DictMsg),
    Backlinks(TabId, BacklinksMsg),
    Plan(TabId, PlanMsg),
    Commentary(TabId, CommentaryMsg),
    Search(TabId, SearchMsg),
    /// Open a search tab (or bring the one there is forward), ready to type in.
    OpenSearch,
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
    /// Which page of the settings window shows.
    SettingsTab(SettingsTab),
    /// Turn persecution mode on or off (`network.rs`).
    SetPersecution(bool),
    SetReadingAuto(bool),
    SetReadingSeconds(u32),
    SetReadingDaily(bool),
    SetBookmarksDaily(bool),
    SetVimMode(bool),
    /// A moment passes: a chapter that has been in front of the reader long enough was read.
    ReadingTick,
    /// The pens (tag shortcuts): the one being named, and changes to them.
    PenDraft(String),
    AddPen,
    RemovePen(usize),
    /// Open (or put away) the colors to choose from for a pen.
    EditPenColor(usize),
    /// A pen's new color, by its place in `state::PEN_COLORS`.
    SetPenColor(usize, usize),
    CyclePenStyle(usize),
    SetDraftColor(usize),
    TogglePanel(Panel),
    /// Show the settings window (or bring it forward).
    OpenSettings,
    /// The settings, at their About page (the logo in the top bar).
    OpenAbout,
    Vault(vault_setup::VaultMsg),
    Modules(module_folders::ModulesMsg),
    Manager(module_manager::ManagerMsg),
    Plans(plans_window::PlansMsg),
    /// Escape: put away the card that is open.
    EscapeCard,
    /// A menu bar item was chosen (macOS).
    Command(commands::Command),
    /// The menu bar is in place: it is told what the app is like now.
    MenuInstalled,
    /// Leave reading mode.
    EndReading,
    /// Back or forward in this tab (its own buttons, wherever the tab has them).
    NavTab(TabId, bool),
    /// ⌘T, pressed in this window: a new tab in the pane last used.
    NewTabHere(iced::window::Id),
    /// ⌘N, pressed in this window: a new pane, split off the pane last used.
    NewPaneHere(iced::window::Id),
    /// A window opened or changed size.
    WindowSize(iced::window::Id, iced::Size),
    /// ⌘W, pressed in this window: close the tab in front in the pane last used.
    CloseFocusedTab(iced::window::Id),
    /// ⌘⇧[ or ⌘⇧], pressed in this window: bring the tab before or after it forward, in the same pane.
    CycleTab(iced::window::Id, bool),
    /// Show or put away the margins in reading mode.
    ToggleReadingMargins,
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
        let mut app = App::build(Box::new(backend), notes, None);
        // A vault starts with no pens; the tests use some.
        app.sh.settings.pens = tests::test_pens();
        app
    }

    /// Real SWORD modules and the vault on disk, seeded on first run.
    fn live() -> (Self, Task<Message>) {
        let boot = live::boot();
        let mut app = App::build(boot.backend, boot.notes, boot.persist);
        app.sh.status = boot.status;
        // Just upgraded: say so (a downgrade, or a change that could not be carried over, is a problem and stays in the bar).
        if let Some((words, problem)) = boot.upgrade.as_ref().and_then(|u| u.said()) {
            if problem {
                app.sh.status = if app.sh.status.is_empty() { words } else { format!("{} {words}", app.sh.status) };
            } else {
                app.sh.toast(words);
            }
        }
        app.sh.vault = boot.vault;
        app.onboarding = boot.onboarding;
        app.no_modules = boot.no_modules;
        app.home = boot.home.clone();
        app.config = boot.config.clone();
        let (main, open) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            // Closing saves the session first (see `Message::CloseRequested`).
            exit_on_close_request: false,
            icon: native::window_icon(),
            ..iced::window::Settings::default()
        });
        app.main = main;
        let mut task = open.then(native::install_menu).map(|()| Message::MenuInstalled);
        if app.onboarding.is_some() {
            task = Task::batch([task, app.open_welcome()]);
        }
        if let Some(home) = boot.home {
            let store = Store::split(boot.config.clone().unwrap_or_else(|| home.clone()), home.clone());
            let session = store.session();
            app.sh.attach(store);
            app.sh.cache_dir = Some(home.join("cache"));
            app.sh.load_pens(true);
            // Pick up where the last run left off.
            let session = session.filter(|_| app.onboarding.is_none() && !app.no_modules);
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
            navs: BTreeMap::new(),
            panes: Vec::new(),
            layout: Node::Leaf(0),
            focus: 0,
            zoomed: None,
            window_size: iced::Size::new(1280.0, 800.0),
            pending_scroll: None,
            menu_synced: None,
            reader_mode: None,
            last_bible: None,
            last_note: None,
            last_dict: None,
            next_tab: 1,
            next_pane: 1,
            next_split: 1,
            modifiers: iced::keyboard::Modifiers::empty(),
            link_menu: None,
            shown: std::collections::BTreeSet::new(),
            reading: None,
            drag: None,
            pressed: None,
            main: iced::window::Id::unique(),
            settings_window: None,
            welcome_window: None,
            welcome_folder: None,
            welcome_error: None,
            manager_window: None,
            manager: module_manager::Manager::new(),
            plans_window: None,
            plans: plans_window::Plans::default(),
            plan_cache: Vec::new(),
            panel: None,
            pen_draft: String::new(),
            pen_color: state::DEFAULT_PEN_COLOR,
            pen_editing: None,
            settings_tab: SettingsTab::Appearance,
            layout_draft: String::new(),
            naming: false,
            home: None,
            config: None,
            onboarding: None,
            no_modules: false,
            setup: None,
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
        let split = app.next_split - 1;
        app.layout.set_fraction(split, 0.55);
        let note = app.new_note(app.sh.default_note(&chrono::Local::now()));
        let note = app.add_tab(1, note);
        // The reading log behind it, when the vault has one (see `default_workspace`).
        if app.sh.notes.contains_key(shared::READING_LOG) {
            let log = app.new_log();
            app.add_tab(1, log);
            app.panes[1].active = Some(note);
        }
        app.focus = right;
        app
    }

    // ---- tabs and panes ----------------------------------------------------------------

    /// The chapter in front of the reader is read once it has been there long enough: it is added to
    /// the reading log. Called as time passes (`now`), and also by what is read being changed.
    fn note_reading(&mut self, now: std::time::Instant) {
        if !self.sh.settings.reading_log_auto {
            self.reading = None;
            return;
        }
        let here = self
            .reference_bible_id()
            .filter(|&id| self.is_showing(id))
            .and_then(|id| self.bible_tab(id).map(|t| (id, t.book, t.chapter_no)));
        let Some((tab, book, chapter)) = here else {
            self.reading = None;
            return;
        };
        match &mut self.reading {
            Some(r) if (r.tab, r.book, r.chapter) == (tab, book, chapter) => {
                if !r.logged && now.duration_since(r.since) >= std::time::Duration::from_secs(u64::from(self.sh.settings.reading_log_seconds)) {
                    r.logged = true;
                    let _ = self.sh.log_reading(book, chapter, &chrono::Local::now());
                }
            }
            _ => self.reading = Some(Reading { tab, book, chapter, since: now, logged: false }),
        }
    }

    /// The folder the person's choices are kept in: its own, else the data folder.
    fn config_dir(&self) -> Option<std::path::PathBuf> {
        self.config.clone().or_else(|| self.home.clone())
    }

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

    fn new_log(&mut self) -> Tab {
        let id = self.take_id();
        Tab::Log(Box::new(LogTab::new(id, &mut self.sh)))
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
            Tab::Commentary(t) => t.id,
            Tab::Search(t) => t.id,
            Tab::Backlinks(t) => t.id,
            Tab::Plan(t) => t.id,
            Tab::Log(t) => t.id(),
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
            Tab::Blank(_) | Tab::Backlinks(_) | Tab::Commentary(_) | Tab::Search(_) | Tab::Plan(_) | Tab::Log(_) => {}
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

    /// The tab in front in the pane last used.
    fn focused_tab(&self) -> Option<TabId> {
        self.panes.iter().find(|p| p.id == self.focus).and_then(|p| p.active)
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
        self.navs.remove(&id);
    }

    /// Whether tab `id` can go back (or forward) within itself, in its own history.
    fn inner_can(&self, id: TabId, back: bool) -> bool {
        match self.tabs.get(&id) {
            Some(Tab::Bible(t)) => if back { t.history.can_back() } else { t.history.can_forward() },
            Some(Tab::Note(t)) => if back { t.history.can_back() } else { t.history.can_forward() },
            Some(Tab::Dict(t)) => if back { t.history.can_back() } else { t.history.can_forward() },
            _ => false,
        }
    }

    /// Whether tab `id` has a tab to go back (or forward) to.
    fn outer(&self, id: TabId) -> shared::Outer {
        let nav = self.navs.get(&id);
        shared::Outer { back: nav.is_some_and(|n| !n.back.is_empty()), forward: nav.is_some_and(|n| !n.forward.is_empty()) }
    }

    /// Back or forward in tab `id`, as a browser's: in the tab's own history while it has somewhere to go,
    /// then to the tab it was before.
    fn go(&mut self, id: TabId, back: bool) -> Task<Message> {
        if self.inner_can(id, back) {
            return match self.tabs.get(&id) {
                Some(Tab::Bible(_)) => self.handle(Message::Bible(id, if back { BibleMsg::Back } else { BibleMsg::Forward })),
                Some(Tab::Note(_)) => self.handle(Message::Note(id, if back { NoteMsg::Back } else { NoteMsg::Forward })),
                Some(Tab::Dict(_)) => self.handle(Message::Dict(id, if back { DictMsg::Back } else { DictMsg::Forward })),
                _ => Task::none(),
            };
        }
        self.step_tab(id, back);
        Task::none()
    }

    /// Put tab `to` where tab `from` is (its place in its pane, and the history that goes with it).
    fn swap_in(&mut self, from: TabId, to: Tab) -> TabId {
        let id = App::id_of(&to);
        for pane in &mut self.panes {
            for t in &mut pane.tabs {
                if *t == from {
                    *t = id;
                }
            }
            if pane.active == Some(from) {
                pane.active = Some(id);
            }
        }
        self.tabs.insert(id, to);
        if let Some(nav) = self.navs.remove(&from) {
            self.navs.insert(id, nav);
        }
        self.remember(id);
        id
    }

    /// A link took over tab `from`: `to` shows in its place, and `from` is kept to go back to.
    fn take_over(&mut self, from: TabId, to: Tab) -> TabId {
        let Some(mut old) = self.tabs.remove(&from) else { return App::id_of(&to) };
        if let Tab::Bible(tab) = &mut old {
            tab.stop_edit(&mut self.sh);
        }
        let mut nav = self.navs.remove(&from).unwrap_or_default();
        nav.back.push(old);
        nav.forward.clear();
        self.navs.insert(from, nav);
        let id = self.swap_in(from, to);
        self.activate(id);
        id
    }

    /// Go back (or forward) in tab `id` to the tab it was before, which comes back as it was left.
    fn step_tab(&mut self, id: TabId, back: bool) {
        let Some(mut nav) = self.navs.remove(&id) else { return };
        // (A note that has gone since is passed over.)
        let target = loop {
            let popped = if back { nav.back.pop() } else { nav.forward.pop() };
            let Some(tab) = popped else { break None };
            let gone = match &tab {
                Tab::Note(t) => !self.sh.notes.contains_key(&t.path),
                _ => false,
            };
            if !gone {
                break Some(tab);
            }
        };
        let Some(mut target) = target else {
            self.navs.insert(id, nav);
            return;
        };
        let Some(mut current) = self.tabs.remove(&id) else {
            self.navs.insert(id, nav);
            return;
        };
        if let Tab::Bible(tab) = &mut current {
            tab.stop_edit(&mut self.sh);
        }
        if back { nav.forward.push(current) } else { nav.back.push(current) }
        // What changed while it was away: its marks, its text.
        match &mut target {
            Tab::Bible(t) => t.marks_changed(),
            // (A note is as it was, with its caret, unless it was changed meanwhile.)
            Tab::Note(t) if self.sh.notes.get(&t.path).is_some_and(|text| text != t.doc.source()) => t.reload(&self.sh),
            Tab::Log(t) => t.note.reload(&self.sh),
            _ => {}
        }
        self.navs.insert(id, nav);
        let new = self.swap_in(id, target);
        self.activate(new);
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
        let heir = if self.panes.len() > 1 { self.layout.remove_to(id) } else { None };
        if let Some(heir) = heir {
            self.panes.remove(pane);
            if self.zoomed == Some(id) {
                self.zoomed = None;
            }
            // The pane that takes the space is the one to go on with (not the first made, which
            // can be anywhere on the screen).
            if self.focus == id {
                self.focus = heir;
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

    /// Show the backlinks of `target`, asked for from tab `from`: in the tab that shows
    /// backlinks already, else in a new tab in another pane.
    fn show_backlinks(&mut self, from: TabId, target: Target) {
        let existing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Backlinks(_)).then_some(*id));
        if let Some(id) = existing {
            if let Some(Tab::Backlinks(tab)) = self.tabs.get_mut(&id) {
                tab.target = target;
            }
            self.activate(id);
            return;
        }
        let from_pane = self.pane_of(from).unwrap_or(self.panes.len() - 1);
        let pane = self.target_pane(from_pane, Dest::OtherPane);
        let id = self.take_id();
        let tab = Tab::Backlinks(Box::new(BacklinksTab::new(id, target)));
        let id = self.add_tab(pane, tab);
        self.activate(id);
    }

    /// The Bible a search looks in to begin with: the one being read, else the first.
    fn search_module(&self) -> Option<ModuleId> {
        self.reference_bible_id()
            .and_then(|b| self.bible_tab(b))
            .map(|t| t.module.clone())
            .or_else(|| self.sh.backend.modules().first().map(|m| m.id.clone()))
    }

    /// Bring the search tab forward (making one beside the reader if there is none), and, given
    /// words, search for them.
    fn show_search(&mut self, words: Option<String>) -> Task<Message> {
        let existing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Search(_)).then_some(*id));
        let id = match existing {
            Some(id) => id,
            None => {
                let Some(module) = self.search_module() else { return Task::none() };
                let from = self.reference_bible_id().unwrap_or(0);
                let from_pane = self.pane_of(from).unwrap_or(self.panes.len() - 1);
                let pane = self.target_pane(from_pane, Dest::OtherPane);
                let id = self.take_id();
                self.add_tab(pane, Tab::Search(Box::new(SearchTab::new(id, module))))
            }
        };
        self.activate(id);
        let Some(Tab::Search(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
        let input = iced::widget::operation::focus(tab.input_id());
        match words {
            Some(words) => tab.search_for(&self.sh, &words).map(move |m| Message::Search(id, m)),
            None => input,
        }
    }

    /// The commentary a new tab opens on: the first installed, else the person's own.
    fn first_commentary(&self) -> ModuleId {
        self.sh.backend.commentaries().first().map_or_else(|| ModuleId(commentary_tab::MINE.into()), |c| c.id.clone())
    }

    /// Add a section for the passage typed in tab `id` to the person's commentary on its book (the
    /// note, and the chapter's heading, are made if they are not there), and show the note in the
    /// writer with the caret under the new heading.
    fn begin_commentary(&mut self, id: TabId) -> Task<Message> {
        let Some(Tab::Commentary(tab)) = self.tabs.get(&id) else { return Task::none() };
        let Some(range) = scripture::RefRange::parse(&tab.passage) else {
            self.sh.toast(format!("\u{201c}{}\u{201d} is not a passage: try John 3:16-21", tab.passage.trim()));
            return Task::none();
        };
        let path = study_core::own_commentary::path(range.book);
        let text = self.sh.notes.get(&path).cloned().unwrap_or_else(|| study_core::own_commentary::new_book(range.book, &study_core::frontmatter::now()));
        let (text, at) = study_core::own_commentary::ensure(&text, range);
        if self.sh.notes.get(&path) != Some(&text) {
            self.sh.notes.insert(path.clone(), text);
        }
        self.show_note_at(path, at);
        if let Some(Tab::Commentary(tab)) = self.tabs.get_mut(&id) {
            tab.show(&self.sh, range.book, range.chapter, None);
        }
        Task::none()
    }

    /// Show the note at `path` in a new tab beside the tab `from`, whatever is open already.
    fn open_note_in_new_tab(&mut self, from: TabId, path: String) {
        let pane = self.pane_of(from).unwrap_or(0);
        let tab = self.new_note(path);
        let id = self.add_tab(pane, tab);
        self.activate(id);
    }

    /// Show a note with the caret at `offset` of its text.
    fn show_note_at(&mut self, path: String, offset: usize) {
        if let Some(id) = self.show_note(path)
            && let Some(Tab::Note(tab)) = self.tabs.get_mut(&id)
        {
            let offset = (0..=offset.min(tab.doc.source().len())).rev().find(|&i| tab.doc.source().is_char_boundary(i)).unwrap_or(0);
            tab.doc.perform(md_editor::Action::SetCaret { offset, extend: false });
        }
    }

    /// Where the Bible tab others work with is reading: its chapter, and its selected verse.
    fn reference_place(&self) -> (scripture::Book, u16, Option<u16>) {
        match self.reference_bible_id().and_then(|b| self.bible_tab(b)) {
            Some(t) => (t.book, t.chapter_no, None),
            None => (scripture::Book::GENESIS, 1, None),
        }
    }

    /// Show a commentary on a chapter, asked for from tab `from`: in the commentary tab there
    /// already is, else in a new one in another pane.
    fn show_commentary(&mut self, from: TabId, book: scripture::Book, chapter: u16, verse: Option<u16>) -> Task<Message> {
        let module = self.first_commentary();
        let existing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Commentary(_)).then_some(*id));
        let id = match existing {
            Some(id) => id,
            None => {
                let from_pane = self.pane_of(from).unwrap_or(self.panes.len() - 1);
                let pane = self.target_pane(from_pane, Dest::OtherPane);
                let id = self.take_id();
                let tab = CommentaryTab::new(id, &self.sh, module, book, chapter, verse);
                self.add_tab(pane, Tab::Commentary(Box::new(tab)))
            }
        };
        self.activate(id);
        let Some(Tab::Commentary(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
        tab.show(&self.sh, book, chapter, verse);
        Task::none()
    }

    /// Make the plans and their progress again, if a plan tab is there to show them.
    fn refresh_plans(&mut self) {
        if self.tabs.values().any(|t| matches!(t, Tab::Plan(_))) {
            self.plan_cache = plans_window::entries_of(&self.sh);
        }
    }

    /// Show the plan tab (making one in another pane if there is none), at a plan and a day, or at
    /// the list of plans.
    fn show_plans(&mut self, select: Option<(String, Option<usize>)>) -> TabId {
        let existing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Plan(_)).then_some(*id));
        let id = match existing {
            Some(id) => id,
            None => {
                let from_pane = self.pane_index(self.focus).unwrap_or(self.panes.len() - 1);
                let pane = self.target_pane(from_pane, Dest::OtherPane);
                let id = self.take_id();
                self.add_tab(pane, Tab::Plan(Box::new(PlanTab::new(id))))
            }
        };
        self.activate(id);
        self.plan_cache = plans_window::entries_of(&self.sh);
        let entries = std::mem::take(&mut self.plan_cache);
        if let Some(Tab::Plan(tab)) = self.tabs.get_mut(&id) {
            match select {
                Some((key, day)) => tab.update(&self.sh, &entries, PlanMsg::Select(key, day)),
                None if tab.plan.is_none() => tab.update(&self.sh, &entries, PlanMsg::Back),
                None => {}
            }
        }
        self.plan_cache = entries;
        id
    }

    /// Show a note of the library in the plan tab, at its day; false if it is no plan's.
    fn show_plan_note(&mut self, path: &str) -> Option<TabId> {
        let entries = plans_window::entries_of(&self.sh);
        let found = entries.iter().find_map(|e| {
            if let Some(i) = e.plan.days.iter().position(|d| d.note.as_deref() == Some(path)) {
                Some((e.key(), Some(i)))
            } else {
                (e.plan.manifest.as_deref() == Some(path)).then(|| (e.key(), None))
            }
        })?;
        Some(self.show_plans(Some(found)))
    }

    /// Show the reading log: in its tab if it has one open, else in a new one in another pane.
    fn show_log(&mut self) -> TabId {
        let existing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Log(_)).then_some(*id));
        let id = match existing {
            Some(id) => id,
            None => {
                let from_pane = self.pane_index(self.focus).unwrap_or(self.panes.len() - 1);
                let pane = self.target_pane(from_pane, Dest::OtherPane);
                let tab = self.new_log();
                self.add_tab(pane, tab)
            }
        };
        self.activate(id);
        id
    }

    /// Show a note in a note tab: the one already holding it, else the one last used (which
    /// follows the link), else a new one in the last pane.
    fn show_note(&mut self, path: String) -> Option<TabId> {
        // The reading log is not a note: it has a tab of its own, and this is the way to it from the Bible tab's
        // card of the days a chapter was read.
        if path == shared::READING_LOG {
            return Some(self.show_log());
        }
        // The library's notes are plans' days: they are read in the plan tab.
        if shared::is_library(&path)
            && let Some(id) = self.show_plan_note(&path)
        {
            return Some(id);
        }
        let holding = self.tabs.iter().find_map(|(id, t)| match t {
            Tab::Note(n) if n.path == path => Some(*id),
            _ => None,
        });
        if let Some(id) = holding {
            self.activate(id);
            return Some(id);
        }
        let last = self.last_note.filter(|id| matches!(self.tabs.get(id), Some(Tab::Note(_))));
        match last {
            Some(id) => {
                if let Some(Tab::Note(tab)) = self.tabs.get_mut(&id) {
                    tab.open(&mut self.sh, path);
                }
                self.activate(id);
                Some(id)
            }
            None => {
                let tab = self.new_note(path);
                let pane = self.panes.len() - 1;
                let id = self.add_tab(pane, tab);
                self.activate(id);
                Some(id)
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

    /// Turn blank tab `id` into what was chosen, keeping its place in its pane.
    fn open_choice(&mut self, id: TabId, choice: Choice) {
        if !matches!(self.tabs.get(&id), Some(Tab::Blank(_))) {
            return;
        }
        let tab = match choice {
            // The module the other Bible tab is reading, else the first; and the first dictionary
            // and note: the tab has its own pickers for the rest.
            Choice::Bible => {
                let near = self.reference_bible_id();
                let module = near.and_then(|n| self.bible_tab(n)).map_or_else(|| self.first_module(), |t| t.module.clone());
                let tab = BibleTab::started(id, &self.sh, module, near.and_then(|n| self.bible_tab(n)));
                Tab::Bible(Box::new(tab))
            }
            Choice::Plans => Tab::Plan(Box::new(PlanTab::new(id))),
            Choice::Log => Tab::Log(Box::new(LogTab::new(id, &mut self.sh))),
            Choice::Note => Tab::Note(Box::new(NoteTab::new(id, &self.sh, self.sh.default_note(&chrono::Local::now())))),
            Choice::Commentary => {
                let module = self.first_commentary();
                let (book, chapter, verse) = self.reference_place();
                Tab::Commentary(Box::new(CommentaryTab::new(id, &self.sh, module, book, chapter, verse)))
            }
            Choice::Search => {
                let Some(module) = self.search_module() else { return };
                Tab::Search(Box::new(SearchTab::new(id, module)))
            }
            Choice::Dict => {
                let Some(module) = self.sh.backend.dictionaries().first().map(|d| d.id.clone()) else { return };
                Tab::Dict(Box::new(DictTab::new(id, &self.sh, module, "")))
            }
        };
        self.tabs.insert(id, tab);
        self.activate(id);
    }

    // ---- update --------------------------------------------------------------------

    fn update(&mut self, message: Message) -> Task<Message> {
        let reading_before = self.reader_mode.map(|r| r.tab);
        let task = self.handle(message);
        self.refresh_plans();
        self.settle();
        self.settle_reading();
        // Reader mode begun or ended: the text is laid out again, wider or narrower, and the reader is put back at the same
        // share of the way down, so that it is where it was.
        let reading_after = self.reader_mode.map(|r| r.tab);
        let carried = if reading_before != reading_after {
            reading_before.or(reading_after).and_then(|id| match self.tabs.get(&id) {
                Some(Tab::Bible(t)) if t.scroll_rel > 0.0 => {
                    self.pending_scroll = Some((id, t.scroll_rel, 0));
                    Some(snap_to(t.reader_id(), RelativeOffset { x: 0.0, y: t.scroll_rel }))
                }
                _ => None,
            })
        } else {
            None
        }
        .unwrap_or_else(Task::none);
        self.sh.save();
        self.note_session();
        let menu = self.menu_state();
        let told = if self.menu_synced == Some(menu) {
            Task::none()
        } else {
            self.menu_synced = Some(menu);
            native::sync_menu(self.main, menu).discard()
        };
        Task::batch([task, carried, self.shown_changed(), told])
    }

    /// What the menu bar shows of the app: what has somewhere to go, and what is on.
    fn menu_state(&self) -> commands::MenuState {
        let tab = self.focused_tab().and_then(|id| self.tabs.get(&id));
        let focused = self.focused_tab();
        let outer = focused.map(|id| self.outer(id)).unwrap_or_default();
        let (back, forward) = match focused {
            Some(id) => (self.inner_can(id, true) || outer.back, self.inner_can(id, false) || outer.forward),
            None => (false, false),
        };
        let bible = match tab {
            Some(Tab::Bible(t)) => Some(t),
            _ => None,
        };
        let shown = |on: fn(&bible_tab::BibleTab) -> bool| bible.is_some_and(|t| on(t));
        commands::MenuState {
            back,
            forward,
            tabs: self.pane_index(self.focus).is_some_and(|p| self.panes[p].tabs.len() > 1),
            can_read: bible.is_some(),
            reading: self.reader_mode.is_some(),
            search: !self.sh.backend.modules().is_empty(),
            strongs: shown(|t| t.show_strongs),
            marks: shown(|t| t.show_marks),
            footnotes: shown(|t| t.show_footnotes),
            refs: shown(|t| t.show_refs),
            strongs_in_margin: shown(|t| t.in_margin.strongs),
            marks_in_margin: shown(|t| t.in_margin.marks),
            footnotes_in_margin: shown(|t| t.in_margin.footnotes),
            refs_in_margin: shown(|t| t.in_margin.refs),
        }
    }

    /// The tabs showing: the one of each pane, or only that of the pane filling the window.
    fn visible_tabs(&self) -> std::collections::BTreeSet<TabId> {
        let zoomed = self.zoomed.and_then(|z| self.pane_index(z));
        self.panes
            .iter()
            .enumerate()
            .filter(|(i, _)| zoomed.is_none_or(|z| z == *i))
            .filter_map(|(_, p)| p.active)
            .collect()
    }

    /// What is done as tabs come and go from view. A tab's widgets are made again when it is shown
    /// again, so: what it was asked to do once is forgotten as it leaves (or it would be done again),
    /// and where it was scrolled to is put back as it returns.
    fn shown_changed(&mut self) -> Task<Message> {
        let now = self.visible_tabs();
        let (left, back): (Vec<TabId>, Vec<TabId>) =
            (self.shown.difference(&now).copied().collect(), now.difference(&self.shown).copied().collect());
        self.shown = now;
        for id in left {
            if let Some(Tab::Bible(tab)) = self.tabs.get_mut(&id) {
                tab.forget_requests();
            }
            // A day written into the log by hand is linked to its daily note, once the log is left (not while it is
            // being typed in).
            if matches!(self.tabs.get(&id), Some(Tab::Log(_))) {
                self.sh.sync_log_days();
                for tab in self.tabs.values_mut() {
                    if let Tab::Log(t) = tab {
                        t.note.reload(&self.sh);
                    }
                }
            }
        }
        // The person's own commentary may have been written in since it was last in view.
        for &id in &back {
            if let Some(Tab::Commentary(tab)) = self.tabs.get_mut(&id)
                && tab.is_mine()
            {
                tab.show(&self.sh, tab.book, tab.chapter, tab.verse);
            }
        }
        let to = |id: iced::widget::Id, y: f32| (y > 0.0).then(|| scroll_to(id, AbsoluteOffset { x: 0.0, y }));
        Task::batch(back.into_iter().filter_map(|id| match self.tabs.get(&id)? {
            Tab::Bible(t) => to(t.reader_id(), t.scroll_y),
            Tab::Note(t) => to(t.editor_id(), t.scroll),
            Tab::Commentary(t) => to(t.editor_id(), t.scroll),
            Tab::Search(t) => to(t.scroll_id(), t.scroll),
            Tab::Dict(t) => to(t.scroll_id(), t.scroll),
            Tab::Backlinks(t) => to(t.scroll_id(), t.scroll),
            Tab::Plan(t) => to(t.editor_id(), t.scroll),
            // The log grows down: it is shown at its end, where the latest is, whatever it was scrolled to.
            Tab::Log(t) => Some(snap_to(t.note.editor_id(), RelativeOffset::END)),
            Tab::Blank(_) => None,
        }))
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
        // The tabs of a window with no vault are not a session worth keeping.
        if self.onboarding.is_some() || self.no_modules {
            return;
        }
        let Some(store) = &self.sh.store else { return };
        match store.save_session(&now) {
            Ok(()) => self.saved = Some(now),
            Err(e) => self.sh.status = format!("Could not save the session: {e}"),
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        // Opening something from a card while the reader is alone in the window (a dictionary entry in a tab, a note in the
        // writer, the reading log, the notes that link here, a commentary, a search for the words) leaves that mode
        // first, and then goes as it would anywhere else.
        if self.reader_mode.is_some()
            && let Message::Bible(_, m) = &message
            && (matches!(
                m,
                BibleMsg::OpenWord(_)
                    | BibleMsg::OpenMarkInWriter(_)
                    | BibleMsg::OpenReadingLog
                    | BibleMsg::OpenBacklinks
                    | BibleMsg::OpenCommentary
                    | BibleMsg::SearchSelection
            ) || (self.modifiers.command() && matches!(m, BibleMsg::StartEdit(_) | BibleMsg::CardPressed(_))))
        {
            self.reader_mode = None;
        }
        // A note selector is a pop-up: whatever is done outside it puts it away.
        // (The reader's reports of its own layout are not something done.)
        let ambient = matches!(
            &message,
            Message::Bible(_, BibleMsg::MarginChanged(_) | BibleMsg::AutoScroll(_) | BibleMsg::SelectionChanged(None))
                | Message::Tick(_)
                | Message::ReadingTick
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
                    t.history_open = false;
                }
                Tab::Blank(_) | Tab::Dict(_) | Tab::Backlinks(_) | Tab::Commentary(_) | Tab::Search(_) | Tab::Plan(_) | Tab::Log(_) => {}
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
                self.go(id, back)
            }
            Message::NavTab(id, back) => {
                self.touch(id);
                self.go(id, back)
            }
            // A tab's own back and forward buttons: its history first, then the tab it was before.
            Message::Bible(id, BibleMsg::Back) | Message::Note(id, NoteMsg::Back) | Message::Dict(id, DictMsg::Back) if !self.inner_can(id, true) => {
                self.touch(id);
                self.step_tab(id, true);
                Task::none()
            }
            Message::Bible(id, BibleMsg::Forward) | Message::Note(id, NoteMsg::Forward) | Message::Dict(id, DictMsg::Forward) if !self.inner_can(id, false) => {
                self.touch(id);
                self.step_tab(id, false);
                Task::none()
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
            Message::Bible(id, msg) if self.modifiers.command() && matches!(msg, BibleMsg::StartEdit(_) | BibleMsg::CardPressed(_)) => {
                // A card pressed with the command key is opened in a tab of its own, not written in.
                self.touch(id);
                match msg {
                    BibleMsg::StartEdit(index) => {
                        if let Some(mark) = self.sh.marks.get(index) {
                            let (path, markdown) = (mark.path(), mark.to_markdown());
                            self.sh.notes.or_insert_with(path.clone(), || markdown);
                            self.open_note_in_new_tab(id, path);
                        }
                    }
                    BibleMsg::CardPressed(key) => match self.sh.backend.strongs_dictionary(&key) {
                        Some(module) => self.open_dictionary(id, module, key, Dest::NewTab),
                        None => self.sh.status = format!("No installed dictionary has {key}"),
                    },
                    _ => {}
                }
                Task::none()
            }
            // A reader just put back at a share of the way down reports where it is: if that is not there yet (the scrollable
            // began again at the top), it is asked again, a few times.
            Message::Bible(id, BibleMsg::Scrolled(_, share))
                if self.pending_scroll.is_some_and(|(p, target, _)| p == id && (share - target).abs() > 0.02) =>
            {
                let Some((_, target, tries)) = self.pending_scroll else { return Task::none() };
                let reader = self.bible_tab(id).map(|t| t.reader_id());
                match reader {
                    Some(reader) if tries < 3 => {
                        self.pending_scroll = Some((id, target, tries + 1));
                        snap_to(reader, RelativeOffset { x: 0.0, y: target })
                    }
                    _ => {
                        self.pending_scroll = None;
                        Task::none()
                    }
                }
            }
            Message::Bible(id, msg) => {
                if matches!(msg, BibleMsg::Scrolled(..)) && self.pending_scroll.is_some_and(|(p, ..)| p == id) {
                    self.pending_scroll = None;
                }
                // What a reader reports of itself (its layout, losing its selection when something
                // else is pressed, scrolling) is not something done in it: it does not take the focus
                // from where the press was.
                if !matches!(msg, BibleMsg::MarginChanged(_) | BibleMsg::AutoScroll(_) | BibleMsg::SelectionChanged(None) | BibleMsg::Scrolled(..)) {
                    self.touch(id);
                }
                let Some(Tab::Bible(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
                let selected = matches!(msg, BibleMsg::SelectionChanged(Some(_)));
                let open_word = matches!(msg, BibleMsg::OpenWord(_));
                let (task, request) = tab.update(&mut self.sh, msg);
                let task = task.map(move |m| Message::Bible(id, m));
                // A note waiting for a passage (Reselect, Mark) takes the next selection that settles.
                if selected {
                    self.finish_pick(id);
                }
                match request {
                    Some(bible_tab::Request::Open(spec)) => {
                        // (A card's "Open in tab" button is not a link: it opens a tab, not over this one.)
                        let dest = match Dest::from_modifiers(self.modifiers) {
                            Dest::Here if open_word => Dest::OtherPane,
                            dest => dest,
                        };
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
                    Some(bible_tab::Request::OpenNote(path)) => {
                        self.show_note(path);
                    }
                    Some(bible_tab::Request::OpenBacklinks(passage)) => self.show_backlinks(id, Target::Passage(passage)),
                    Some(bible_tab::Request::ReadingMode) => self.start_reading(id),
                    Some(bible_tab::Request::OpenSearch) => return Task::batch([task, self.show_search(None)]),
                    Some(bible_tab::Request::Search(words)) => return Task::batch([task, self.show_search(Some(words))]),
                    Some(bible_tab::Request::OpenCommentary(book, chapter, verse)) => return Task::batch([task, self.show_commentary(id, book, chapter, verse)]),
                    None => {}
                }
                task
            }
            Message::Log(id, msg) => {
                self.touch(id);
                let Some(Tab::Log(tab)) = self.tabs.get_mut(&id) else { return Task::none() };
                let (task, request) = tab.update(&mut self.sh, msg);
                let task = task.map(move |m| Message::Log(id, m));
                // Another tab on the log shows what was written.
                let text = tab.note.doc.source().to_string();
                for (other, tab) in self.tabs.iter_mut() {
                    if let Tab::Log(t) = tab
                        && *other != id
                        && t.note.doc.source() != text
                    {
                        t.note.reload(&self.sh);
                    }
                }
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
                    _ => task,
                }
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
                    Some(note_tab::Request::Backlinks(path)) => {
                        self.show_backlinks(id, Target::Note(path));
                        task
                    }
                    None => task,
                }
            }
            Message::Search(id, msg) => {
                self.touch(id);
                // A verse chosen is shown in the reader, with the words that matched pointed out.
                if let SearchMsg::Open(range, module, words) = msg {
                    let dest = Dest::from_modifiers(self.modifiers);
                    let task = self.open_verse(id, range, dest);
                    let shown = self.last_bible.and_then(|b| self.tabs.get_mut(&b));
                    if let (Some(Tab::Bible(tab)), Some((verse, _))) = (shown, range.verses)
                        && tab.module == module
                        && (tab.book, tab.chapter_no) == (range.book, range.chapter)
                    {
                        tab.select_found(verse, &words);
                    }
                    return task;
                }
                match self.tabs.get_mut(&id) {
                    Some(Tab::Search(tab)) => tab.update(&mut self.sh, msg).map(move |m| Message::Search(id, m)),
                    _ => Task::none(),
                }
            }
            Message::OpenSearch => self.show_search(None),
            Message::Commentary(id, msg) => {
                self.touch(id);
                if matches!(msg, CommentaryMsg::Begin) {
                    return self.begin_commentary(id);
                }
                // A link to one of the person's own notes shows it in the writer.
                if let CommentaryMsg::Link(study_core::markdown::LinkTarget::Note(name)) = &msg {
                    // `note#heading`: the note, with the caret under that heading.
                    let (name, heading) = name.split_once('#').map_or((name.as_str(), None), |(n, h)| (n, Some(h)));
                    let paths = self.sh.note_paths();
                    if let Some(path) = study_core::markdown::resolve_note(&paths, name).cloned() {
                        let at = heading
                            .and_then(scripture::RefRange::parse)
                            .zip(study_core::own_commentary::book_of(&path))
                            .and_then(|(range, book)| {
                                let text = self.sh.notes.get(&path)?;
                                study_core::own_commentary::sections(text, book).into_iter().find(|s| s.range == range).map(|s| s.write_at(text))
                            });
                        match at {
                            Some(at) => self.show_note_at(path, at),
                            None => {
                                self.show_note(path);
                            }
                        }
                    }
                    return Task::none();
                }
                // A link in the notes shows its verse in the reader.
                if let CommentaryMsg::Link(study_core::markdown::LinkTarget::Web(url)) = &msg {
                    self.open_web(url);
                    return Task::none();
                }
                if let CommentaryMsg::Link(study_core::markdown::LinkTarget::Verse { range, .. }) = &msg {
                    let dest = Dest::from_modifiers(self.modifiers);
                    return self.open_verse(id, *range, dest);
                }
                if let Some(Tab::Commentary(tab)) = self.tabs.get_mut(&id) {
                    tab.update(&mut self.sh, msg);
                }
                Task::none()
            }
            Message::Plan(id, msg) => {
                self.touch(id);
                match msg {
                    PlanMsg::App(m) => return self.plans_message(m),
                    PlanMsg::Link(study_core::markdown::LinkTarget::Verse { range, .. }) => {
                        let dest = Dest::from_modifiers(self.modifiers);
                        return self.open_verse(id, range, dest);
                    }
                    PlanMsg::Link(study_core::markdown::LinkTarget::Web(url)) => {
                        self.open_web(&url);
                        return Task::none();
                    }
                    PlanMsg::Link(study_core::markdown::LinkTarget::Note(name)) => {
                        let name = name.split_once('#').map_or(name.as_str(), |(n, _)| n).to_string();
                        if let Some(path) = study_core::markdown::resolve_note(&self.sh.note_paths(), &name).cloned() {
                            self.open_note_at(id, path, Dest::from_modifiers(self.modifiers));
                        }
                        return Task::none();
                    }
                    msg => {
                        let entries = std::mem::take(&mut self.plan_cache);
                        if let Some(Tab::Plan(tab)) = self.tabs.get_mut(&id) {
                            tab.update(&self.sh, &entries, msg);
                        }
                        self.plan_cache = entries;
                    }
                }
                Task::none()
            }
            Message::Backlinks(id, msg) => {
                self.touch(id);
                let picked = match self.tabs.get_mut(&id) {
                    Some(Tab::Backlinks(tab)) => tab.update(msg),
                    _ => None,
                };
                if let Some(path) = picked {
                    self.open_note_at(id, path, Dest::from_modifiers(self.modifiers));
                }
                Task::none()
            }
            Message::Dict(id, msg) => {
                self.touch(id);
                // A number in an entry's text opens its entry, as any link does.
                if let DictMsg::Link(target) = &msg {
                    let dest = Dest::from_modifiers(self.modifiers);
                    return self.open_spec(id, Spec::from_target(target), dest);
                }
                if let DictMsg::OpenVerse(range, module, words) = msg {
                    let dest = Dest::from_modifiers(self.modifiers);
                    let task = self.open_verse(id, range, dest);
                    // The verse is shown with the words selected, not the whole verse.
                    let shown = self.last_bible.and_then(|b| self.tabs.get_mut(&b));
                    if let (Some(Tab::Bible(tab)), Some((verse, _))) = (shown, range.verses)
                        && tab.module == module
                        && (tab.book, tab.chapter_no) == (range.book, range.chapter)
                    {
                        tab.select_found(verse, &words);
                    }
                    return task;
                }
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
            Message::MenuInstalled => {
                self.menu_synced = None;
                Task::none()
            }
            Message::Command(command) => {
                use commands::Command as C;
                let main = self.main;
                match command {
                    C::Settings => self.handle(Message::OpenSettings),
                    C::NewTab => self.handle(Message::NewTabHere(main)),
                    C::NewPane => self.handle(Message::NewPaneHere(main)),
                    C::CloseTab => self.handle(Message::CloseFocusedTab(main)),
                    C::PreviousTab => self.handle(Message::CycleTab(main, false)),
                    C::NextTab => self.handle(Message::CycleTab(main, true)),
                    C::Back => self.handle(Message::Navigate { back: true }),
                    C::Forward => self.handle(Message::Navigate { back: false }),
                    C::Search => self.handle(Message::OpenSearch),
                    C::ReaderMode => {
                        if self.reader_mode.is_some() {
                            self.reader_mode = None;
                        } else if let Some(id) = self.focused_tab().filter(|id| matches!(self.tabs.get(id), Some(Tab::Bible(_)))) {
                            self.start_reading(id);
                        }
                        Task::none()
                    }
                    setting => {
                        // The Bible tab's own settings: each message carries the new value.
                        use bible_tab::NoteKind as K;
                        let state = self.menu_state();
                        let focused = self.focused_tab().filter(|id| matches!(self.tabs.get(id), Some(Tab::Bible(_))));
                        match (focused, setting.setting()) {
                            (Some(id), Some((kind, in_margin))) if state.enabled(setting) => {
                                let on = !state.checked(setting);
                                let msg = match (kind, in_margin) {
                                    (K::Strongs, false) => BibleMsg::ToggleStrongs(on),
                                    (K::Marks, false) => BibleMsg::ToggleMarks(on),
                                    (K::Footnotes, false) => BibleMsg::ToggleFootnotes(on),
                                    (K::Refs, false) => BibleMsg::ToggleRefs(on),
                                    (kind, true) => BibleMsg::SetInMargin(kind, on),
                                };
                                self.handle(Message::Bible(id, msg))
                            }
                            _ => Task::none(),
                        }
                    }
                }
            }
            Message::NewTabHere(window) => {
                if window == self.main {
                    return self.handle(Message::NewTab(self.focus));
                }
                Task::none()
            }
            Message::WindowSize(window, size) => {
                if window == self.main {
                    self.window_size = size;
                }
                Task::none()
            }
            // (Done at once, from the size last reported: asking the window would put the split after
            // whatever is pressed next, such as ⌘W closing the tab the split was about to leave.)
            Message::NewPaneHere(window) if window == self.main => {
                let window = self.window_size;
                // Along the longer side of the pane: side by side if it is wider than tall. (A pane
                // filling the window has all of it; the top bar is above the panes otherwise.)
                let below_bar = if self.zoomed.is_some() { 0.0 } else { TOP_BAR_HEIGHT };
                let whole = (window.width, (window.height - below_bar).max(1.0));
                let (w, h) = match self.zoomed {
                    Some(_) => whole,
                    None => self.layout.size_of(self.focus, whole).unwrap_or(whole),
                };
                let axis = if w >= h { panes::Axis::Row } else { panes::Axis::Column };
                self.handle(Message::SplitPane(self.focus, axis))
            }
            Message::NewPaneHere(_) => Task::none(),
            Message::CloseFocusedTab(window) => {
                if window == self.main
                    && let Some(id) = self.focused_tab()
                {
                    self.close_tab(id);
                }
                Task::none()
            }
            Message::CycleTab(window, forward) => {
                if window == self.main
                    && let Some(pane) = self.pane_index(self.focus)
                {
                    let tabs = &self.panes[pane].tabs;
                    let at = self.panes[pane].active.and_then(|a| tabs.iter().position(|&t| t == a));
                    if let Some(at) = at.filter(|_| tabs.len() > 1) {
                        let next = if forward { (at + 1) % tabs.len() } else { (at + tabs.len() - 1) % tabs.len() };
                        let id = tabs[next];
                        self.activate(id);
                    }
                }
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
            // The look of the app only: the pens are the vault's own and are kept.
            Message::ResetSettings => self.change_settings(|s| {
                let (pens, auto, seconds, daily, vim) =
                    (std::mem::take(&mut s.pens), s.reading_log_auto, s.reading_log_seconds, s.reading_log_daily, s.vim_mode);
                *s = Settings { pens, reading_log_auto: auto, reading_log_seconds: seconds, reading_log_daily: daily, vim_mode: vim, ..Settings::default() };
            }),
            Message::SetReadingAuto(on) => self.change_settings(|s| s.reading_log_auto = on),
            Message::SetReadingSeconds(n) => self.change_settings(|s| s.reading_log_seconds = n),
            Message::SetReadingDaily(on) => self.change_settings(|s| s.reading_log_daily = on),
            Message::SetBookmarksDaily(on) => self.change_settings(|s| s.bookmarks_daily = on),
            Message::SetVimMode(on) => self.change_settings(|s| s.vim_mode = on),
            Message::ReadingTick => {
                self.note_reading(std::time::Instant::now());
                Task::none()
            }
            Message::SetPersecution(on) => {
                self.set_persecution(on);
                Task::none()
            }
            Message::SettingsTab(tab) => {
                self.settings_tab = tab;
                self.pen_editing = None;
                Task::none()
            }
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
                self.change_settings(|s| {
                    // A pen for a tag there already is one for changes that pen's color instead.
                    match s.pens.iter_mut().find(|p| p.tag == tag) {
                        Some(pen) => pen.color = color,
                        None => s.pens.push(state::PenDef { tag, color, style: state::PenStyle::Highlight }),
                    }
                })
            }
            Message::RemovePen(i) => {
                self.pen_editing = None;
                self.change_settings(move |s| {
                    if i < s.pens.len() {
                        s.pens.remove(i);
                    }
                })
            }
            Message::EditPenColor(i) => {
                self.pen_editing = if self.pen_editing == Some(i) { None } else { Some(i) };
                Task::none()
            }
            Message::SetPenColor(i, color) => {
                self.pen_editing = None;
                self.change_settings(|s| {
                    if let (Some(pen), Some(color)) = (s.pens.get_mut(i), state::PEN_COLORS.get(color)) {
                        pen.color = color.to_string();
                    }
                })
            }
            Message::CyclePenStyle(i) => self.change_settings(|s| {
                if let Some(pen) = s.pens.get_mut(i) {
                    pen.style = pen.style.next();
                }
            }),
            Message::SetDraftColor(i) => {
                self.pen_color = i % state::PEN_COLORS.len();
                Task::none()
            }
            Message::TogglePanel(panel) => {
                self.panel = if self.panel == Some(panel) { None } else { Some(panel) };
                Task::none()
            }
            Message::OpenAbout => {
                self.settings_tab = SettingsTab::About;
                self.handle(Message::OpenSettings)
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
                    size: iced::Size::new(660.0, 600.0),
                    resizable: false,
                    minimizable: false,
                    exit_on_close_request: false,
                    ..iced::window::Settings::default()
                });
                self.settings_window = Some(id);
                let main = self.main;
                open.then(move |_| native::attach(main, id)).discard()
            }
            Message::Vault(message) => self.vault_message(message),
            Message::Modules(message) => self.modules_message(message),
            Message::Manager(message) => self.manager_message(message),
            Message::Plans(message) => self.plans_message(message),
            Message::ToggleReadingMargins => {
                let on = !self.sh.settings.reader_margins;
                self.change_settings(|s| s.reader_margins = on)
            }
            Message::EndReading => {
                self.reader_mode = None;
                Task::none()
            }
            Message::EscapeCard => {
                if self.setup.is_some() {
                    self.setup = None;
                } else {
                    self.naming = false;
                    self.layout_draft.clear();
                }
                Task::none()
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
                    let workspace = self.layout_to_save();
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
                // (What the readers read is not the layout's: the ones open carry on into it.)
                let workspace = self.layout_with_readers(&workspace);
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
                if self.welcome_window == Some(id) {
                    self.welcome_window = None;
                }
                if self.manager_window == Some(id) {
                    self.manager_window = None;
                }
                if self.plans_window == Some(id) {
                    self.plans_window = None;
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
        // The passage of a link is named in the KJV's numbering: the module shows it in its own.
        let range = self.sh.backend.versification(&tab.module).map_from_kjv(range);
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
                if self.sh.backend.modules().is_empty() {
                    self.sh.status = "No Bible modules installed: add some in Modules".into();
                    return None;
                }
                let near = self.reference_bible_id();
                let module = near.and_then(|n| self.bible_tab(n)).map(|t| t.module.clone()).unwrap_or_else(|| self.first_module());
                let id = self.take_id();
                let mut tab = BibleTab::started(id, &self.sh, module, near.and_then(|n| self.bible_tab(n)));
                let range = self.sh.backend.versification(&tab.module).map_from_kjv(*range);
                tab.show_in_reader(&mut self.sh, range);
                Some(Tab::Bible(Box::new(tab)))
            }
            // An address is for another program, not a tab.
            Spec::Web(url) => {
                self.open_web(url);
                None
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
    /// The Bible whose verses a Strong's entry lists: the one last read if it has Strong's numbers,
    /// else the first that does.
    fn strongs_module(&self) -> Option<ModuleId> {
        let modules = self.sh.backend.modules();
        let last = self.last_bible.and_then(|id| match self.tabs.get(&id) {
            Some(Tab::Bible(t)) => Some(t.module.clone()),
            _ => None,
        });
        last.filter(|m| modules.iter().any(|i| i.id == *m && i.has_strongs)).or_else(|| modules.into_iter().find(|m| m.has_strongs).map(|m| m.id))
    }

    /// Open an address in the browser (or the mail program), and say if that was not possible.
    fn open_web(&mut self, url: &str) {
        if network::blocked() {
            self.sh.toast("Persecution mode is on: links are not opened");
            return;
        }
        if !native::open_external(url) {
            self.sh.toast(format!("Could not open {url}"));
        }
    }

    fn open_spec(&mut self, from: TabId, spec: Spec, dest: Dest) -> Task<Message> {
        match spec {
            Spec::Verse { range, .. } => self.open_verse(from, range, dest),
            Spec::Web(url) => {
                self.open_web(&url);
                Task::none()
            }
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

    /// Show an entry in a dictionary tab. Followed plainly, a tab that shows it comes forward, else the tab
    /// the link is in goes to it; a new tab, the other pane or a new pane are asked for by keys.
    fn open_dictionary(&mut self, from: TabId, module: ModuleId, key: String, dest: Dest) {
        if dest == Dest::Here {
            // As a browser follows a link: a tab that shows the entry comes forward; else the tab the link
            // is in goes to it (a dictionary tab in its own history, any other kind of tab being taken over,
            // to be gone back to).
            let showing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Dict(d) if d.module == module && d.key == key).then_some(*id));
            let mut fresh = false;
            let target = match showing {
                Some(id) => id,
                None if matches!(self.tabs.get(&from), Some(Tab::Dict(_))) => from,
                None => {
                    let tab = Tab::Dict(Box::new(DictTab::new(self.next_tab, &self.sh, module.clone(), "")));
                    self.next_tab += 1;
                    fresh = true;
                    self.take_over(from, tab)
                }
            };
            if showing.is_none()
                && let Some(Tab::Dict(tab)) = self.tabs.get_mut(&target)
            {
                tab.open(&self.sh, module, &key);
                if fresh {
                    tab.forget_history();
                }
            }
            self.activate(target);
            return;
        }
        let target = {
            let from_pane = self.pane_of(from).unwrap_or(0);
            let pane = self.target_pane(from_pane, dest);
            // The other pane's own dictionary tab, if that is what it is showing.
            let reusable = (dest == Dest::OtherPane)
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
        };
        if let Some(Tab::Dict(tab)) = self.tabs.get_mut(&target) {
            tab.open(&self.sh, module, &key);
        }
        self.activate(target);
    }

    /// The Bible tab that is on screen (in front in its pane, and its pane not hidden by another filling the
    /// window): the one last used if it is, else any.
    fn visible_reader(&self) -> Option<TabId> {
        let reader = |id: &TabId| matches!(self.tabs.get(id), Some(Tab::Bible(_))) && self.is_showing(*id);
        self.last_bible.into_iter().chain(self.tabs.keys().copied()).find(|id| reader(id))
    }

    /// The Bible tab that shows the chapter of `range` already (in its module's numbering): `from` if it
    /// does, else the one last used if it does, else any.
    fn bible_showing(&self, range: scripture::RefRange, from: TabId) -> Option<TabId> {
        let shows = |id: &TabId| match self.tabs.get(id) {
            Some(Tab::Bible(t)) => {
                let mapped = self.sh.backend.versification(&t.module).map_from_kjv(range);
                (t.book, t.chapter_no) == (mapped.book, mapped.chapter)
            }
            _ => false,
        };
        std::iter::once(from)
            .chain(self.last_bible)
            .chain(self.tabs.keys().copied())
            .find(|id| shows(id))
    }

    fn open_verse(&mut self, from: TabId, range: scripture::RefRange, dest: Dest) -> Task<Message> {
        if self.sh.backend.modules().is_empty() {
            self.sh.toast("No Bible modules installed: add some in Modules");
            return Task::none();
        }
        // The Bible tab the link belongs to: the one it was followed in, or the one a note works with.
        if dest == Dest::Here {
            // As a browser follows a link: a Bible tab that shows the chapter comes forward; else the tab the
            // link is in goes to it if it is a Bible tab; else a Bible tab on screen is claimed; else the tab
            // the link is in is taken over, to be gone back to.
            let showing = self.bible_showing(range, from);
            let mut fresh = false;
            let target = match showing {
                Some(id) => id,
                None if matches!(self.tabs.get(&from), Some(Tab::Bible(_))) => from,
                // A reader that is on screen is claimed (this is the exception: a passage is for reading, so
                // it goes to the reader, whatever kind of tab the link was in).
                None if self.visible_reader().is_some() => self.visible_reader().unwrap_or(from),
                None => {
                    let module = self.reference_bible_id().and_then(|b| self.bible_tab(b)).map(|t| t.module.clone()).unwrap_or_else(|| self.first_module());
                    let near = self.reference_bible_id();
                    let tab = self.new_bible(module, near);
                    fresh = true;
                    self.take_over(from, tab)
                }
            };
            self.activate(target);
            let task = self.show_verse_in(target, range);
            if fresh && let Some(Tab::Bible(tab)) = self.tabs.get_mut(&target) {
                tab.forget_history();
            }
            return task;
        }
        let natural = if matches!(self.tabs.get(&from), Some(Tab::Bible(_))) { from } else { self.bible_for_showing() };
        // A new tab goes beside the Bible tab the link belongs with. "Other pane" and "new pane" are
        // relative to where the link is (a note's, say), which is not that tab's pane when the Bible
        // tab is the other pane already.
        let belongs = self.pane_of(natural);
        let from_pane = match dest {
            Dest::NewTab => belongs,
            _ => self.pane_of(from).or(belongs),
        }
        .unwrap_or(0);
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
        self.open_note_at(from, path, dest);
    }

    /// Follow a link from tab `from` to the note at `path`.
    fn open_note_at(&mut self, from: TabId, path: String, dest: Dest) {
        if dest == Dest::Here {
            // As a browser follows a link: a tab that shows the note comes forward; else the tab the link
            // is in goes to it (a note tab in its own history, any other kind of tab being taken over, to
            // be gone back to).
            let showing = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Note(n) if n.path == path).then_some(*id));
            match showing {
                Some(id) => self.activate(id),
                None if matches!(self.tabs.get(&from), Some(Tab::Note(_))) => {
                    if let Some(Tab::Note(tab)) = self.tabs.get_mut(&from) {
                        tab.open(&mut self.sh, path);
                    }
                    self.activate(from);
                }
                None => {
                    let tab = self.new_note(path);
                    self.take_over(from, tab);
                }
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

    /// A selection has settled in the reader `reader`: if a note is waiting for a passage (its Reselect or Mark was
    /// pressed), the selection becomes its passage. A margin note moves to it; an ordinary note becomes a margin
    /// note on it, marked as the first pen marks (or, with no pens, a yellow highlight). The selection is spent.
    fn finish_pick(&mut self, reader: TabId) {
        let Some(waiting) = self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Note(n) if n.picking).then_some(*id)) else { return };
        let Some(Tab::Bible(bible)) = self.tabs.get(&reader) else { return };
        let Some((anchor, module)) = bible.selection_anchor(&self.sh) else { return };
        let Some(Tab::Note(note)) = self.tabs.get_mut(&waiting) else { return };
        note.picking = false;
        let path = note.path.clone();
        let said = match self.sh.mark_index_of(&path) {
            Some(index) => {
                self.sh.set_mark_anchor(index, anchor, module);
                "Moved the note to the new selection"
            }
            None => {
                let (style, color) = self.sh.default_look();
                if self.sh.note_to_mark(&path, anchor, module, style, &color).is_none() {
                    return;
                }
                "The note is on the selected passage now"
            }
        };
        if let Some(Tab::Bible(bible)) = self.tabs.get_mut(&reader) {
            bible.clear_selection();
        }
        self.sh.toast(said);
    }

    /// Show the Bible tab `id` alone, as the window's whole content.
    fn start_reading(&mut self, id: TabId) {
        self.activate(id);
        self.reader_mode = Some(ReaderMode { tab: id, tabs: self.tabs.len() });
    }

    /// Reading mode lasts only while nothing else is opened: another tab (or the tab going
    /// away, or another tab coming forward) ends it.
    fn settle_reading(&mut self) {
        let Some(reading) = self.reader_mode else { return };
        let shown = self.pane_of(reading.tab).is_some_and(|p| self.panes[p].active == Some(reading.tab))
            && matches!(self.tabs.get(&reading.tab), Some(Tab::Bible(_)))
            && self.tabs.len() == reading.tabs;
        if !shown {
            self.reader_mode = None;
        }
    }

    /// The Bible tab being read alone, if it is.
    fn reader_tab(&self) -> Option<&BibleTab> {
        match self.reader_mode.and_then(|r| self.tabs.get(&r.tab)) {
            Some(Tab::Bible(tab)) => Some(tab),
            _ => None,
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
                        // The reading log's tab, when the app has added a chapter to it.
                        if let Tab::Log(t) = tab
                            && (t.note.path == old || t.note.path == new)
                        {
                            t.note.reload(&self.sh);
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
        if Some(window) == self.settings_window {
            "Settings".into()
        } else if Some(window) == self.welcome_window {
            "Welcome to Zoe".into()
        } else if Some(window) == self.manager_window {
            "Manage modules".into()
        } else if Some(window) == self.plans_window {
            "Plans".into()
        } else {
            "Zoe".into()
        }
    }

    fn window_view(&self, window: iced::window::Id) -> Element<'_, Message> {
        if Some(window) == self.settings_window {
            self.settings_panel()
        } else if Some(window) == self.welcome_window {
            self.welcome_panel()
        } else if Some(window) == self.manager_window {
            self.manager_panel()
        } else if Some(window) == self.plans_window {
            self.plans_panel()
        } else {
            self.view()
        }
    }

    /// The keys held (they decide where a link opens), and a clock only while there is a toast to
    /// take down.
    fn subscription(&self) -> iced::Subscription<Message> {
        let keys = iced::event::listen_with(|event, _status, window| match event {
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => Some(Message::Modifiers(m)),
            iced::Event::Window(iced::window::Event::Opened { size, .. } | iced::window::Event::Resized(size)) => Some(Message::WindowSize(window, size)),
            // ⌘[ and ⌘] (Ctrl elsewhere), and the mouse's own back and forward buttons. With shift
            // they change tab instead (the shifted keys are { and } on some layouts), and ⌘W closes one.
            // (On a Mac these are the menu bar's, which takes the keys before they get here.)
            // The settings of the reader (option with the command key and S, M, F or R): matched by the key
            // on the keyboard, as option changes the character. (Where Ctrl and Alt together make
            // AltGr, a layout may take these keys for itself.)
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { physical_key, modifiers, .. })
                if modifiers.command() && modifiers.alt() && !cfg!(target_os = "macos") =>
            {
                use iced::keyboard::key::{Code, Physical};
                let letter = match physical_key {
                    Physical::Code(Code::KeyS) => 's',
                    Physical::Code(Code::KeyM) => 'm',
                    Physical::Code(Code::KeyF) => 'f',
                    Physical::Code(Code::KeyR) => 'r',
                    _ => return None,
                };
                commands::Command::ALL
                    .into_iter()
                    .find(|c| c.shortcut().is_some_and(|s| s.alt && s.key == letter && s.shift == modifiers.shift()))
                    .map(Message::Command)
            }
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) if modifiers.command() && !cfg!(target_os = "macos") => {
                match key.as_ref() {
                    iced::keyboard::Key::Character("[" | "{") if modifiers.shift() => Some(Message::CycleTab(window, false)),
                    iced::keyboard::Key::Character("]" | "}") if modifiers.shift() => Some(Message::CycleTab(window, true)),
                    iced::keyboard::Key::Character("w" | "W") if !modifiers.shift() => Some(Message::CloseFocusedTab(window)),
                    iced::keyboard::Key::Character("t" | "T") if !modifiers.shift() => Some(Message::NewTabHere(window)),
                    iced::keyboard::Key::Character("n" | "N") if !modifiers.shift() => Some(Message::NewPaneHere(window)),
                    iced::keyboard::Key::Character("[") => Some(Message::Navigate { back: true }),
                    iced::keyboard::Key::Character("]") => Some(Message::Navigate { back: false }),
                    // The platform's shortcut for preferences (on a Mac the menu bar item takes it).
                    iced::keyboard::Key::Character(",") => Some(Message::OpenSettings),
                    iced::keyboard::Key::Character("f" | "F") if modifiers.shift() => Some(Message::OpenSearch),
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
            native::commands_chosen().map(Message::Command),
        ];
        // Escape leaves reading mode, unless a card is open: it is put away first.
        if self.reader_mode.is_some() && !(self.naming || self.setup.is_some()) {
            all.push(iced::event::listen_with(|event, status, _window| match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) if status == iced::event::Status::Ignored => Some(Message::EndReading),
                _ => None,
            }));
        }
        // Escape puts away whichever card is open.
        if self.naming || self.setup.is_some() {
            all.push(iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::EscapeCard),
                _ => None,
            }));
        }
        if self.sh.settings.reading_log_auto {
            all.push(iced::time::every(READING_TICK).map(|_| Message::ReadingTick));
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
        // The window's press catch-all sits outside everything, so it only sees presses the
        // widgets inside left alone.
        let window = if let Some(tab) = self.reader_tab() {
            let id = tab.id;
            iced::widget::mouse_area(stack![tab.reading_view(&self.sh, self.sh.settings.reader_margins).map(move |m| Message::Bible(id, m)), self.reading_controls()])
                .on_press(Message::PressedElsewhere)
        } else {
            let body: Element<'_, Message> = match self.zoomed.and_then(|z| self.pane_index(z)) {
                Some(index) => self.pane_view(index),
                None => self.node_view(&self.layout),
            };
            iced::widget::mouse_area(column![self.top_bar(), rule::horizontal(1), body]).on_press(Message::PressedElsewhere)
        };
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
        if self.onboarding.is_some() {
            layers.push(self.waiting_view());
        }
        if let Some(setup) = &self.setup {
            layers.push(self.setup_view(setup));
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
            // (Under the top bar: the logo's height and the bar's padding above and below it.)
            .padding(iced::Padding { top: 24.0 + 2.0 * TOP_BAR_PADDING, right: 12.0, left: 0.0, bottom: 0.0 })
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .into()
    }

    /// The settings window: a strip of pages across the top, and the page chosen below.
    fn settings_panel(&self) -> Element<'_, Message> {
        let strip = SettingsTab::ALL.iter().fold(row![].spacing(2), |strip, &tab| {
            let on = tab == self.settings_tab;
            strip.push(
                button(container(text(tab.label()).size(14)).center_x(Length::Fill).padding([6, 0]))
                    .width(Length::Fill)
                    .padding(0)
                    .style(move |theme: &Theme, status| {
                        let p = theme.extended_palette();
                        let lit = on || matches!(status, button::Status::Hovered);
                        button::Style {
                            background: lit.then(|| iced::Color { a: if on { 1.0 } else { 0.5 }, ..p.background.weak.color }.into()),
                            text_color: if on { p.background.base.text } else { iced::Color { a: 0.7, ..p.background.base.text } },
                            border: iced::Border { radius: 6.0.into(), ..iced::Border::default() },
                            ..button::Style::default()
                        }
                    })
                    .on_press(Message::SettingsTab(tab)),
            )
        });
        let page = match self.settings_tab {
            SettingsTab::Appearance => self.appearance_page(),
            SettingsTab::Pens => section("Pens", self.pens_settings()),
            SettingsTab::Vault => section("Vault", self.vault_settings()),
            SettingsTab::Modules => section("Bible modules", self.modules_settings()),
            SettingsTab::Reading => section("Reading log", self.reading_settings()),
            SettingsTab::Privacy => section("Privacy", self.privacy_settings()),
            SettingsTab::About => self.about_page(),
        };
        column![
            container(strip).padding([10, 14]),
            rule::horizontal(1),
            iced::widget::scrollable(container(page).padding(16).width(Length::Fill)).height(Length::Fill),
        ]
        .into()
    }

    fn appearance_page(&self) -> Element<'_, Message> {
        use iced::widget::{checkbox, slider};
        let settings = &self.sh.settings;
        let label = |t: &'static str| text(t).size(13);
        let size_row = |value: f32, range: std::ops::RangeInclusive<f32>, msg: fn(f32) -> Message| {
            row![slider(range, value, msg).step(1.0), text(format!("{value:.0}")).size(13).width(26)].spacing(10).align_y(iced::Center)
        };
        let mut fonts = vec![FontChoice(None)];
        fonts.extend(self.families.iter().cloned().map(|f| FontChoice(Some(f))));
        column![
            section(
                "Color",
                column![
                    label("Color scheme"),
                    pick_list(Theme::ALL, Some(self.sh.theme()), Message::SetTheme).text_size(13).width(Length::Fill)
                ]
                .spacing(6)
                .into(),
            ),
            section(
                "Text",
                column![
                    column![label("Bible pane text size"), size_row(settings.bible_size, state::BIBLE_SIZES, Message::SetBibleSize)].spacing(6),
                    column![label("Notes pane text size"), size_row(settings.notes_size, state::NOTES_SIZES, Message::SetNotesSize)].spacing(6),
                    column![
                        label("Bible and note font"),
                        pick_list(fonts, Some(FontChoice(settings.font.clone().filter(|f| !state::is_system_alias(f)))), Message::SetFont).text_size(13).width(Length::Fill)
                    ]
                    .spacing(6),
                ]
                .spacing(16)
                .into(),
            ),
            section(
                "Writer",
                checkbox(settings.vim_mode).label("Vim key bindings in the writer").on_toggle(Message::SetVimMode).size(16).text_size(13).into(),
            ),
            button(text("Reset appearance").size(13)).padding([4, 10]).style(button::secondary).on_press(Message::ResetSettings),
        ]
        .spacing(16)
        .into()
    }

    /// What the reading log does: a note that chapters are added to as they are read.
    fn reading_settings(&self) -> Element<'_, Message> {
        use iced::widget::{checkbox, slider};
        let settings = &self.sh.settings;
        column![
            text("A chapter you have read is added, under the day's date, to a note called Reading log in your vault. The button at the foot of a chapter's text does it by hand.").size(12).style(text::secondary),
            checkbox(settings.reading_log_auto).label("Log a chapter by itself once it has been read a while").on_toggle(Message::SetReadingAuto).size(16).text_size(13),
            column![
                text(format!("A while is {} seconds in front of the reader", settings.reading_log_seconds)).size(12).style(text::secondary),
                slider(state::READ_SECONDS.start..=state::READ_SECONDS.end, settings.reading_log_seconds, Message::SetReadingSeconds).step(5u32),
            ]
            .spacing(4),
            checkbox(settings.reading_log_daily)
                .label("Also add it to today's note, under Reading")
                .on_toggle(Message::SetReadingDaily)
                .size(16)
                .text_size(13),
            text("Today's note is only added to if it is already there.").size(11).style(text::secondary),
            checkbox(settings.bookmarks_daily)
                .label("Add each new highlight or note to today's note, under Bookmarks")
                .on_toggle(Message::SetBookmarksDaily)
                .size(16)
                .text_size(13),
            text("Only when today's note is there; the heading is added if it has none.").size(11).style(text::secondary),
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
            let style = pen.style.label();
            list = list.push(
                row![
                    swatch(&pen.color, Message::EditPenColor(i)),
                    text(format!("#{}", pen.tag)).size(13).width(Length::Fill),
                    button(text(style).size(12)).padding([1, 8]).style(button::secondary).on_press(Message::CyclePenStyle(i)),
                    button(text("\u{00D7}").size(14)).padding([0, 6]).style(button::text).on_press(Message::RemovePen(i)),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
            // Its colors, under it, with the one it has ringed.
            if self.pen_editing == Some(i) {
                let at = state::PEN_COLORS.iter().position(|c| c.eq_ignore_ascii_case(&pen.color)).unwrap_or(usize::MAX);
                list = list.push(container(color_grid(at, move |color| Message::SetPenColor(i, color))).padding([4, 0]));
            }
        }
        let draft = &self.pen_draft;
        let new_pen = row![
            text_input("A tag for a new pen", draft).on_input(Message::PenDraft).on_submit(Message::AddPen).size(13).padding([3, 6]),
            button(text("Add").size(12)).padding([3, 10]).style(button::secondary).on_press_maybe((!draft.trim().is_empty()).then_some(Message::AddPen)),
        ]
        .spacing(8)
        .align_y(iced::Center);
        let colors = color_grid(self.pen_color, Message::SetDraftColor);
        // Tags already in use that fit what is typed, to take instead of typing the rest.
        let have: Vec<String> = self.sh.settings.pens.iter().map(|p| p.tag.clone()).collect();
        let mut offered = row![].spacing(4);
        for (tag, _) in self.sh.tag_suggestions(draft, &have, 5) {
            offered = offered.push(
                button(text(format!("#{tag}")).size(11)).padding([1, 6]).style(button::secondary).on_press(Message::PenDraft(tag)),
            );
        }
        column![
            text("A pen marks the selected text and makes a note with its tag, in its color.").size(11).style(text::secondary),
            list,
            new_pen,
            colors,
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
        // The app's mark at the left, in the color of the text.
        let mark = iced::widget::svg(iced::widget::svg::Handle::from_memory(include_bytes!("../assets/app/zoe.svg").as_slice()))
            .width(10.2)
            .height(24)
            .style(|theme: &Theme, _status| iced::widget::svg::Style { color: Some(theme.extended_palette().background.base.text) });
        // Pressed, it says what the app is: the settings, at their About page.
        let mark = button(mark).padding(0).style(button::text).on_press(Message::OpenAbout);
        let mut bar = row![mark, space::horizontal()].spacing(16).align_y(iced::Center);
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
        let modules = button(text("Modules").size(13)).padding([2, 8]).style(button::text).on_press(Message::Manager(module_manager::ManagerMsg::Open));
        let plans = button(text("Plans").size(13)).padding([2, 8]).style(button::text).on_press(Message::Plans(plans_window::PlansMsg::Open));
        bar = bar.push(row![tool("Layouts", Panel::Layouts), plans, modules, settings].spacing(4));
        container(bar).padding([TOP_BAR_PADDING, 16.0]).into()
    }

    /// A tab without a bar of its own gets one, with the back and forward buttons where the other
    /// tabs have theirs (a link followed here takes over the tab, and these go back to what it was).
    fn with_nav_bar<'a>(&'a self, id: TabId, content: Element<'a, Message>) -> Element<'a, Message> {
        let outer = self.outer(id);
        let bar = container(
            row![
                icons::Icon::Back.arrow_button(outer.back, Message::NavTab(id, true)),
                icons::Icon::Forward.arrow_button(outer.forward, Message::NavTab(id, false)),
            ]
            .spacing(4),
        )
        .padding([8, 16]);
        column![bar, rule::horizontal(1), content].into()
    }

    fn pane_view(&self, index: usize) -> Element<'_, Message> {
        let pane = &self.panes[index];
        let content: Element<'_, Message> = match pane.active.and_then(|id| self.tabs.get(&id)) {
            Some(Tab::Bible(tab)) => {
                let id = tab.id;
                tab.view(&self.sh, self.outer(id)).map(move |m| Message::Bible(id, m))
            }
            Some(Tab::Dict(tab)) => {
                let id = tab.id;
                let searched = self.strongs_module();
                tab.view(&self.sh, searched.as_ref(), self.outer(id)).map(move |m| Message::Dict(id, m))
            }
            Some(Tab::Commentary(tab)) => {
                let id = tab.id;
                self.with_nav_bar(id, tab.view(&self.sh).map(move |m| Message::Commentary(id, m)))
            }
            Some(Tab::Search(tab)) => {
                let id = tab.id;
                self.with_nav_bar(id, tab.view(&self.sh).map(move |m| Message::Search(id, m)))
            }
            Some(Tab::Backlinks(tab)) => {
                let id = tab.id;
                self.with_nav_bar(id, tab.view(&self.sh).map(move |m| Message::Backlinks(id, m)))
            }
            Some(Tab::Plan(tab)) => {
                let id = tab.id;
                self.with_nav_bar(id, tab.view(&self.sh, &self.plan_cache, self.plans.confirm_remove.as_ref()).map(move |m| Message::Plan(id, m)))
            }
            Some(Tab::Note(tab)) => {
                let id = tab.id;
                let reference = self.reference_bible_id().and_then(|b| self.bible_tab(b));
                tab.view(&self.sh, reference, self.outer(id)).map(move |m| Message::Note(id, m))
            }
            Some(Tab::Log(tab)) => {
                let id = tab.id();
                let reference = self.reference_bible_id().and_then(|b| self.bible_tab(b));
                self.with_nav_bar(id, tab.note.log_view(&self.sh, reference).map(move |m| Message::Log(id, m)))
            }
            Some(Tab::Blank(id)) => self.with_nav_bar(*id, self.blank_view(*id)),
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
                Some(Tab::Commentary(t)) => t.title(&self.sh),
                Some(Tab::Search(t)) => t.title(&self.sh),
                Some(Tab::Backlinks(t)) => t.title(&self.sh),
                Some(Tab::Plan(t)) => t.title(&self.plan_cache),
                Some(Tab::Log(t)) => t.title(),
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

    /// The controls of reading mode, faint in the top right corner over the text: the margins,
    /// and the way out.
    fn reading_controls(&self) -> Element<'_, Message> {
        let margins = self.sh.settings.reader_margins;
        let small = |icon: icons::Icon, tip: &'static str, msg: Message| {
            tooltip(
                button(container(icon.view(18.0)).center(Length::Fill))
                    .width(28)
                    .height(28)
                    .padding(0)
                    .style(|theme: &Theme, status| {
                        let base = button::secondary(theme, status);
                        let alpha = if matches!(status, button::Status::Hovered | button::Status::Pressed) { 0.85 } else { 0.35 };
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
                tooltip::Position::Bottom,
            )
        };
        // Back and forward, quieter than the rest: the tab's own history, then the tab it was before (as anywhere).
        let tab = self.reader_mode.map(|r| r.tab);
        let can = |back: bool| tab.is_some_and(|id| self.inner_can(id, back) || if back { self.outer(id).back } else { self.outer(id).forward });
        let step = |icon: icons::Icon, tip: &'static str, back: bool| {
            let on = can(back);
            let mut b = button(container(icon.view_text(18.0, if on { 0.7 } else { 0.3 })).center(Length::Fill))
                .width(28)
                .height(28)
                .padding(0)
                .style(move |theme: &Theme, status| {
                    let lit = on && matches!(status, button::Status::Hovered | button::Status::Pressed);
                    let base = theme.extended_palette().secondary.base.color;
                    button::Style {
                        background: lit.then(|| iced::Color { a: 0.5, ..base }.into()),
                        border: iced::Border { radius: 4.0.into(), ..iced::Border::default() },
                        ..button::Style::default()
                    }
                });
            if let (true, Some(id)) = (on, tab) {
                b = b.on_press(Message::NavTab(id, back));
            }
            tooltip(b, container(text(tip).size(12)).padding(4).style(container::rounded_box), tooltip::Position::Bottom)
        };
        // Back and forward at the top left, the rest at the top right.
        let history = row![step(icons::Icon::Back, "Back", true), step(icons::Icon::Forward, "Forward", false)].spacing(2);
        let controls = row![
            small(
                if margins { icons::Icon::MarginsOn } else { icons::Icon::MarginsOff },
                if margins { "Hide the margins" } else { "Show the margins" },
                Message::ToggleReadingMargins,
            ),
            small(icons::Icon::Collapse, "Leave reading mode (Esc)", Message::EndReading),
        ]
        .spacing(6);
        stack![
            container(history)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Left)
                .align_y(iced::alignment::Vertical::Top)
                .padding(iced::Padding { top: 10.0, right: 0.0, bottom: 0.0, left: 14.0 }),
            container(controls)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Top)
                // (Clear of the scrollbar down the right edge.)
                .padding(iced::Padding { top: 10.0, right: 24.0, bottom: 0.0, left: 0.0 }),
        ]
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
        // What kind of tab: the Bible, a dictionary, the notes. Which one comes after.
        let kind = move |title: &'static str, about: &'static str, choice: Choice, on: bool| {
            // The words in the page's own text color (the faint one of a secondary button is
            // nearly the card's color), the card a shade off the page with a fine edge.
            let faint = |theme: &Theme| text::Style { color: Some(iced::Color { a: 0.7, ..theme.extended_palette().background.base.text }) };
            let card = button(column![text(title).size(17), text(about).size(13).style(faint)].spacing(4))
                .width(320)
                .padding([12, 18])
                .style(|theme: &Theme, status| {
                    let palette = theme.extended_palette();
                    let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some(if lit { palette.background.strong.color } else { palette.background.weak.color }.into()),
                        text_color: palette.background.base.text,
                        border: iced::Border {
                            color: iced::Color { a: if lit { 0.9 } else { 0.5 }, ..palette.background.strong.color },
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..button::Style::default()
                    }
                });
            if on { card.on_press(Message::Open(id, choice)) } else { card }
        };
        let has_bible = !self.sh.backend.modules().is_empty();
        let has_dictionary = !self.sh.backend.dictionaries().is_empty();
        let no_bible = move || -> Element<'_, Message> {
            if has_bible {
            space::vertical().height(0).into()
        } else {
            column![
                text("No Bible modules are installed yet, so the Bible, dictionaries and search are not available.").size(13).style(text::secondary),
                button(text("Get modules\u{2026}").size(13)).padding([4, 12]).on_press(Message::Manager(module_manager::ManagerMsg::Open)),
            ]
            .spacing(8)
            .align_x(iced::Center)
            .width(320)
            .into()
            }
        };
        let page = move || -> Element<'_, Message> { column![
            text("New tab").size(22),
            text("What would you like to open?").size(14).style(text::secondary),
            no_bible(),
            column![
                kind("Bible", "Read a text, with its notes in the margin.", Choice::Bible, has_bible),
                kind("Notes", "Write and read your notes.", Choice::Note, true),
                kind("Search", "Find words in the Bible.", Choice::Search, has_bible),
                kind("Commentary", "Read a commentary, or write your own.", Choice::Commentary, true),
                kind("Dictionary", "Look up a Strong's number or a word.", Choice::Dict, has_dictionary),
                kind("Plans", "Follow a reading plan.", Choice::Plans, true),
                kind("Reading log", "The chapters you have read, day by day.", Choice::Log, true),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .align_x(iced::Center)
        .into()
        };
        // In the middle of the pane when it all fits, and scrolled when the pane is too short for it
        // (the page is as tall as the pane or, if more, as the cards need, so it never clips).
        let needs = if has_bible { 750.0 } else { 870.0 };
        iced::widget::responsive(move |size| {
            iced::widget::scrollable(container(page()).padding([24, 16]).width(Length::Fill).height(size.height.max(needs)).center_x(Length::Fill).center_y(Length::Fill))
                .height(Length::Fill)
                .into()
        })
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
                Tab::Commentary(t) => t.title(&self.sh),
                Tab::Search(t) => t.title(&self.sh),
                Tab::Backlinks(t) => t.title(&self.sh),
                Tab::Plan(t) => t.title(&self.plan_cache),
                Tab::Log(t) => t.title(),
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
    fn text_has_heading(text: &str, heading: &str) -> bool {
        text.contains(&format!("\n# {heading}\n"))
    }

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
        assert!(matches!(&ws.layout, state::Layout::Split { axis: state::AxisSave::Row, fraction, .. } if *fraction == 0.55));
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
            let _ = self.update(Message::Open(id, Choice::Bible));
            id
        }

        fn add_note(&mut self, pane: usize) -> TabId {
            let blank = self.new_blank();
            let id = self.add_tab(pane, blank);
            let _ = self.update(Message::Open(id, Choice::Note));
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

        fn lid(&self) -> TabId {
            self.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Log(_)).then_some(*id)).expect("a log tab")
        }

        fn log(&self) -> &NoteTab {
            match self.tabs.get(&self.lid()) {
                Some(Tab::Log(t)) => &t.note,
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
            fill: bool,
        ) -> Element<'a, BibleMsg> {
            crate::bible_tab::reference_card(&self.sh, label, body, fill, None)
        }

    }

    /// Make the reader's text selection cover these whole verses, as a verse-number click would.
    fn select_verses(app: &mut App, verses: &[u16]) {
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses();
        app.bible_mut().text_selection = Some(
            verses.iter().map(|&v| CanonSlice { verse: v, start: 0, end: canon[&v].len() }).collect(),
        );
    }

    /// The pens the tests use.
    pub(crate) fn test_pens() -> Vec<state::PenDef> {
        let pen = |tag: &str, color: &str, style| state::PenDef { tag: tag.into(), color: color.into(), style };
        vec![
            pen("promise", "#F5D90A", state::PenStyle::Highlight),
            pen("command", "#4DA3FF", state::PenStyle::Underline),
            pen("growth", "#5FD38D", state::PenStyle::Highlight),
            pen("prayer", "#FF7AB6", state::PenStyle::Highlight),
        ]
    }

    /// A live app (real modules, a vault in `home`), or `None` where ASV is not installed.
    fn live_app(home: &std::path::Path) -> Option<App> {
        let boot = live::boot_at(home, &sword::test_roots(), false);
        boot.persist.as_ref()?;
        // (The sample text stands in where there are no modules: not what these tests are about.)
        if boot.no_modules {
            return None;
        }
        let mut app = App::build(boot.backend, boot.notes, boot.persist);
        app.sh.settings.pens = test_pens();
        Some(app)
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

    #[test]
    fn a_bible_with_the_apocrypha_shows_them_after_the_canon_and_a_bible_without_does_not() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let modules = app.sh.backend.modules();
        let Some(kjva) = modules.iter().find(|m| m.abbreviation == "KJVA").cloned() else { return eprintln!("skipped: KJVA is not installed") };
        let chapters = app.sh.backend.available_chapters(&kjva.id);
        let tobit = Book::parse("Tobit").unwrap();
        // The canon first and as it was, then the Apocrypha, in the order of the books.
        let revelation = chapters.iter().position(|c| *c == (Book::parse("Revelation").unwrap(), 22)).expect("Revelation 22");
        assert!(chapters[..=revelation].iter().all(|(b, _)| !b.is_apocrypha()) && chapters[revelation + 1..].iter().all(|(b, _)| b.is_apocrypha()));
        assert!(chapters.contains(&(tobit, 1)));
        let _ = app.update(app.b(BibleMsg::PickModule(ModuleChoice(kjva.id.clone(), kjva.abbreviation.clone()))));
        let _ = app.update(app.b(BibleMsg::PickBook(crate::bible_tab::BookChoice(tobit))));
        assert_eq!((app.bible().book, app.bible().chapter_no), (tobit, 1));
        let verses = app.bible().chapter.as_ref().expect("Tobit 1 reads").canonical_verses();
        assert!(verses[&1].contains("The book of the words of Tobit"), "{:?}", verses.get(&1));
        // A link in a note to the Apocrypha is a link to a verse like any other.
        assert_eq!(scripture::RefRange::parse("Tobit 1:1").map(|r| r.book), Some(tobit));
        // Other Bibles do not have them, and do not offer them.
        if let Some(plain) = modules.iter().find(|m| m.abbreviation != "KJVA") {
            assert!(app.sh.backend.available_chapters(&plain.id).iter().all(|(b, _)| !b.is_apocrypha()), "{}", plain.abbreviation);
        }
    }

    #[test]
    fn a_link_into_an_nrsv_module_is_shown_in_its_numbering() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let Some(nrsv) = app.sh.backend.modules().into_iter().find(|m| app.sh.backend.versification(&m.id) == scripture::Versification::Nrsv) else {
            return eprintln!("skipped: no module in the NRSV versification is installed");
        };
        let _ = app.update(app.b(BibleMsg::PickModule(ModuleChoice(nrsv.id.clone(), nrsv.abbreviation.clone()))));
        let reader = app.bid();
        // 3 John 14 is one verse of the KJV's, and the NRSV's 14 and 15.
        let _ = app.show_verse_in(reader, scripture::RefRange::parse("3 John 1:14").unwrap());
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::from_osis("3John").unwrap(), 1));
        assert_eq!(app.bible().select_request.map(|(_, a, z)| (a, z)), Some((14, 15)));
        // Other passages are as they were named.
        let _ = app.show_verse_in(reader, scripture::RefRange::parse("John 3:16").unwrap());
        assert_eq!(app.bible().select_request.map(|(_, a, z)| (a, z)), Some((16, 16)));
        // And an embed asks the module for the same.
        let embed = app.sh.embed_for(&nrsv.id, &study_core::markdown::LinkTarget::Verse { range: scripture::RefRange::parse("3 John 1:14").unwrap(), module: None });
        assert!(embed.is_some_and(|e| e.title.contains("3 John 1:14-15")));
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

    /// Lay text out with the same font on every system (Noto Sans, kept with the tests), not with whichever fonts the
    /// machine has: where a click lands in the reader, and how tall a card is, depend on it.
    fn use_test_fonts() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let mut system = iced::advanced::graphics::text::font_system().write().unwrap_or_else(|e| e.into_inner());
            for bytes in [
                include_bytes!("../tests-assets/fonts/NotoSans-Regular.ttf").as_slice(),
                include_bytes!("../tests-assets/fonts/NotoSans-Bold.ttf").as_slice(),
                include_bytes!("../tests-assets/fonts/NotoSans-Italic.ttf").as_slice(),
                include_bytes!("../tests-assets/fonts/NotoSans-BoldItalic.ttf").as_slice(),
            ] {
                system.load_font(std::borrow::Cow::Borrowed(bytes));
            }
            let db = system.raw().db_mut();
            db.set_sans_serif_family("Noto Sans");
            db.set_serif_family("Noto Sans");
        });
    }

    impl Harness {
        fn new(app: App) -> Self {
            use_test_fonts();
            let renderer = iced_test::futures::futures::executor::block_on(
                <iced::Renderer as Headless>::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia")),
            )
            .expect("headless renderer");
            let mut h = Harness {
                app,
                cache: Cache::default(),
                renderer,
                cursor: mouse::Cursor::Unavailable,
                size: Size::new(1024.0, 1800.0),
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
            self.click_scan_to(1700, done)
        }

        /// Like `click_scan`, down to `bottom` only.
        fn click_scan_to(&mut self, bottom: usize, done: impl Fn(&App) -> bool) -> Option<(f32, f32)> {
            // (From below the chapter's title, which has a small button beside it.)
            for y in (250..bottom).step_by(14) {
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
        let (x, y) = h.center_of("Add Note").expect("the pen toolbar offers Add Note");
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
    fn a_note_view_with_no_notes_is_just_a_create_button() {
        let mut app = App::new();
        let ordinary: Vec<String> = app.sh.notes.keys().filter(|p| !p.starts_with("templates/")).cloned().collect();
        for path in ordinary {
            app.sh.notes.remove(&path);
        }
        let mut h = Harness::new(app);
        assert!(h.center_of("Create note").is_some(), "the one thing to do");
        assert!(h.center_of("Write here\u{2026}").is_none(), "no editor");
        let (x, y) = h.center_of("Create note").unwrap();
        h.click(x, y);
        assert!(h.app.note().choosing, "it asks what kind of note");
    }

    #[test]
    fn a_margin_notes_look_and_tags_are_edited_in_its_note_but_not_where_it_is() {
        let mut app = App::new();
        let index = app.sh.marks.iter().position(|m| !m.body.is_empty()).expect("a mark with a note");
        let path = app.sh.marks[index].path();
        let before = app.sh.marks[index].clone();
        app.open_note(path.clone());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::MarkStyle(MarkStyle::Underline)));
        let _ = app.update(Message::Note(id, NoteMsg::MarkColors));
        assert!(app.note().mark_colors);
        let _ = app.update(Message::Note(id, NoteMsg::MarkColor(21)));
        assert!(!app.note().mark_colors, "choosing closes the colors");
        let _ = app.update(Message::Note(id, NoteMsg::TagDraft("Wisdom Words".into())));
        let _ = app.update(Message::Note(id, NoteMsg::AddTag));
        let mark = app.sh.marks[index].clone();
        assert_eq!(mark.style, MarkStyle::Underline);
        assert_eq!(mark.color, crate::state::PEN_COLORS[21]);
        assert!(mark.tags.contains(&"wisdom-words".to_string()));
        assert_eq!((&mark.anchor, &mark.quote, &mark.prefix, &mark.suffix, &mark.module, &mark.body), (&before.anchor, &before.quote, &before.prefix, &before.suffix, &before.module, &before.body));
        // The file and the open note agree.
        let file = app.sh.notes.get(&mark.path()).unwrap();
        assert!(file.contains("style: underline") && file.contains(crate::state::PEN_COLORS[21]) && file.contains("wisdom-words"));
        assert_eq!(app.note().doc.source(), file.as_str());
        let _ = app.update(Message::Note(id, NoteMsg::RemoveTag("wisdom-words".into())));
        assert!(!app.sh.marks[index].tags.contains(&"wisdom-words".to_string()));
    }

    /// A selection of the first words of verse `verse` of the chapter the reader shows, as the reader reports it.
    fn reader_selects(app: &mut App, verse: u16) -> String {
        let text = app.bible().chapter.as_ref().unwrap().canonical_verses()[&verse].clone();
        let end = text.find(' ').expect("a word");
        let slice = CanonSlice { verse, start: 0, end };
        let _ = app.update(app.b(BibleMsg::SelectionChanged(Some(vec![slice]))));
        text[..end].to_string()
    }

    #[test]
    fn reselect_takes_the_next_selection_in_a_reader_as_a_margin_notes_passage() {
        let mut app = App::new();
        let index = app.sh.marks.iter().position(|m| !m.body.is_empty()).expect("a mark with a note");
        let before = app.sh.marks[index].clone();
        app.open_note(before.path());
        let id = app.nid();
        let _ = app.view(); // drawn, as it is before and while it waits
        let _ = app.update(Message::Note(id, NoteMsg::PickText));
        assert!(app.note().picking);
        let _ = app.view();
        // Until a selection settles in a reader nothing has changed.
        assert_eq!(app.sh.marks[index], before);
        let verse = app.bible().chapter.as_ref().unwrap().canonical_verses().keys().copied().find(|v| Some((*v, *v)) != before.anchor.verses).unwrap();
        let words = reader_selects(&mut app, verse);
        let mark = app.sh.marks[index].clone();
        assert!(!app.note().picking);
        assert_eq!((mark.anchor.book, mark.anchor.chapter, mark.anchor.verses), (app.bible().book, app.bible().chapter_no, Some((verse, verse))));
        assert_eq!(mark.quote, words);
        // What is not where it is stays: the comment, tags, look and when it was made.
        assert_eq!((&mark.body, &mark.tags, mark.style, &mark.color, &mark.created), (&before.body, &before.tags, before.style, &before.color, &before.created));
        assert_eq!(app.note().path, mark.path(), "the tab follows the file to its new name");
        assert!(app.sh.notes.contains_key(&mark.path()) && (mark.path() == before.path() || !app.sh.notes.contains_key(&before.path())));
        assert!(app.bible().text_selection.is_none(), "the selection is spent");
        // It was one selection: the next does nothing.
        let _ = reader_selects(&mut app, verse + 1);
        assert_eq!(app.sh.marks[index], mark);
    }

    #[test]
    fn mark_makes_an_ordinary_note_a_margin_note_on_the_next_selection_and_cancel_gives_up() {
        let mut app = App::new();
        app.sh.notes.insert("notes/Idea.md".into(), "---\ncreated: 2026-01-01T10:00:00\ntags: [idea]\n---\n# Idea\n\nWords to keep.\n".into());
        app.sh.notes.insert("notes/Other.md".into(), "See [[Idea]].\n".into());
        app.open_note("notes/Idea.md".into());
        let id = app.nid();
        let _ = app.view();
        let _ = app.update(Message::Note(id, NoteMsg::PickText));
        let _ = app.update(Message::Note(id, NoteMsg::CancelPick));
        assert!(!app.note().picking);
        let _ = reader_selects(&mut app, 1);
        assert!(app.sh.notes.contains_key("notes/Idea.md"), "no one was waiting: it is as it was");
        let marks = app.sh.marks.len();
        let _ = app.update(Message::Note(id, NoteMsg::PickText));
        let verse = *app.bible().chapter.as_ref().unwrap().canonical_verses().keys().nth(1).unwrap();
        let words = reader_selects(&mut app, verse);
        assert_eq!(app.sh.marks.len(), marks + 1);
        let mark = app.sh.marks.last().unwrap().clone();
        assert!(!app.sh.notes.contains_key("notes/Idea.md"), "it is the mark's file now");
        assert_eq!((mark.quote.as_str(), mark.tags.as_slice(), mark.created.as_str()), (words.as_str(), ["idea".to_string()].as_slice(), "2026-01-01T10:00:00"));
        assert!(mark.body.contains("Words to keep.") && mark.body.contains("# Idea"));
        assert!(app.sh.notes[&mark.path()].starts_with("---\ntype: margin\n"));
        assert_eq!(app.note().path, mark.path());
        assert!(!app.sh.notes["notes/Other.md"].contains("[[Idea]]"), "what linked to the note follows it: {}", app.sh.notes["notes/Other.md"]);
        let stem = mark.path().trim_end_matches(".md").rsplit('/').next().unwrap().to_string();
        assert!(app.sh.notes["notes/Other.md"].contains(&format!("[[{stem}]]")), "{}", app.sh.notes["notes/Other.md"]);
        let _ = app.view();
    }

    #[test]
    fn unlink_in_the_note_takes_from_it_only_what_only_a_mark_has() {
        let mut app = App::new();
        let index = app.sh.marks.iter().position(|m| !m.body.is_empty()).expect("a mark with a note");
        let before = app.sh.marks[index].clone();
        let (path, marks) = (before.path(), app.sh.marks.len());
        app.open_note(path.clone());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::Unlink));
        assert_eq!(app.sh.marks.len(), marks - 1);
        assert!(!app.sh.notes.contains_key(&path));
        // The same name in notes/, the same words, tags and time; nothing of the mark's, and no added lines.
        let name = path.rsplit('/').next().unwrap();
        assert_eq!(app.note().path, format!("notes/{name}"));
        let text = app.sh.notes[&app.note().path].clone();
        assert!(text.ends_with(&format!("---\n{}\n", before.body)), "{text}");
        for gone in ["type:", "anchor:", "quote:", "prefix:", "suffix:", "module:", "style:", "color:", "refs:", before.quote.as_str()] {
            assert!(gone.is_empty() || !text.contains(gone), "{gone}");
        }
        assert!(before.created.is_empty() || text.contains(&format!("created: {}", before.created)));
        assert!(before.tags.iter().all(|t| text.contains(t.as_str())));
        let _ = app.view();
    }

    #[test]
    fn a_mark_set_plain_is_a_reference_with_no_look_and_set_back_takes_the_first_pens() {
        use crate::reader_text::MarkKind;
        let mut app = App::new();
        let index = app.sh.marks.iter().position(|m| !m.body.is_empty() && m.style == MarkStyle::Highlight).expect("a highlight with a note");
        app.open_note(app.sh.marks[index].path());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::MarkColors));
        assert!(app.note().mark_colors);
        let _ = app.update(Message::Note(id, NoteMsg::MarkStyle(MarkStyle::None)));
        let mark = app.sh.marks[index].clone();
        assert_eq!((mark.style, mark.color.as_str()), (MarkStyle::None, ""), "no look");
        assert!(!app.note().mark_colors, "the color panel is closed with it");
        let file = &app.sh.notes[&mark.path()];
        assert!(!file.contains("style:") && !file.contains("color:") && file.contains("anchor:") && file.contains("quote:"), "{file}");
        assert_eq!(app.note().path, mark.path(), "the tab follows the file");
        // The color cannot be set on it, and the reader still draws it, as the faint line.
        let _ = app.update(Message::Note(id, NoteMsg::MarkColor(21)));
        assert_eq!(app.sh.marks[index].color, "");
        let _ = app.view();
        let draws = app.mark_draws();
        assert!(draws.iter().any(|d| d.mark == index && d.kind == MarkKind::Note), "drawn as a plain mark");
        // Made a highlight again, it has the first pen's color.
        let _ = app.update(Message::Note(id, NoteMsg::MarkStyle(MarkStyle::Highlight)));
        let (style, color) = app.sh.default_look();
        let _ = style;
        assert_eq!((app.sh.marks[index].style, app.sh.marks[index].color.as_str()), (MarkStyle::Highlight, color.as_str()));
        assert!(app.sh.notes[&app.sh.marks[index].path()].contains("style: highlight"));
    }

    #[test]
    fn a_bare_highlight_changed_in_its_note_is_refiled_and_the_note_follows() {
        let mut app = App::new();
        let index = app.sh.marks.iter().position(|m| m.body.is_empty() && m.style == MarkStyle::Highlight).expect("a bare highlight");
        let old = app.sh.marks[index].path();
        app.open_note(old.clone());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::MarkStyle(MarkStyle::Underline)));
        let new = app.sh.marks[index].path();
        assert_ne!(old, new);
        assert!(!app.sh.notes.contains_key(&old) && app.sh.notes.contains_key(&new));
        assert_eq!(app.note().path, new, "the tab shows the file under its new name");
    }

    #[test]
    fn strongs_numbers_are_named_one_way() {
        use crate::shared::strongs_name;
        assert_eq!(strongs_name("G26").as_deref(), Some("G26"));
        assert_eq!(strongs_name("g0026").as_deref(), Some("G26"));
        assert_eq!(strongs_name(" H430 ").as_deref(), Some("H430"));
        assert_eq!(strongs_name("26"), None);
        assert_eq!(strongs_name("Gx"), None);
        assert_eq!(strongs_name(""), None);
    }

    #[test]
    fn a_strongs_entry_lists_every_verse_with_the_word_and_each_opens_in_the_reader() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let Some(module) = app.sh.backend.modules().into_iter().find(|m| m.has_strongs).map(|m| m.id) else {
            return eprintln!("skipped: no module with Strong's numbers");
        };
        let occurrences = app.sh.strongs_occurrences(&module, "G25");
        let found: Vec<scripture::RefRange> = occurrences.iter().map(|o| o.range).collect();
        let john316 = scripture::RefRange { book: Book::JOHN, chapter: 3, verses: Some((16, 16)) };
        assert!(found.contains(&john316), "John 3:16 has 'loved'");
        assert!(found.len() > 100, "{}", found.len());
        assert!(found.windows(2).all(|w| (w[0].book.osis(), w[0].chapter) != (w[1].book.osis(), w[1].chapter) || w[0].verses < w[1].verses), "once each, in order within a chapter");
        assert_eq!(app.sh.strongs_occurrences(&module, "g0025"), occurrences, "however it is spelled");
        assert!(app.sh.strongs_occurrences(&module, "G99999").is_empty());

        // A link in the dictionary tab shows the verse in the Bible tab.
        let Some(dict) = app.sh.backend.dictionaries().into_iter().find(|d| d.id.0.starts_with("Strongs")) else { return };
        let from = app.bid();
        app.open_dictionary(from, dict.id, "G25".into(), Dest::NewTab);
        let id = app.last_dict.expect("a dictionary tab");
        app.bible_mut().chapter_no = 1;
        let words = occurrences.iter().find(|o| o.range == john316).unwrap().words.clone();
        let _ = app.update(Message::Dict(id, DictMsg::OpenVerse(john316, module.clone(), words.clone())));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3));
        assert!(app.bible().select_request.is_none(), "the verse is not selected whole");
        // The words are selected, from the first to the last.
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses()[&16].clone();
        let slice = app.bible().select_words.expect("the words are to be selected").1;
        assert!(canon[slice.start..slice.end].contains("loved"), "{}", &canon[slice.start..slice.end]);
        // Moving on puts the request away.
        let bid = app.bid();
        let _ = app.update(Message::Bible(bid, BibleMsg::PickChapter(ChapterChoice(Book::JOHN, 4))));
        assert!(app.bible().select_words.is_none());
    }

    #[test]
    fn the_settings_pages_are_chosen_by_tab_and_resetting_the_look_keeps_the_pens() {
        let mut app = App::new();
        assert_eq!(app.settings_tab, SettingsTab::Appearance);
        app.pen_editing = Some(0);
        let _ = app.update(Message::SettingsTab(SettingsTab::Pens));
        assert_eq!((app.settings_tab, app.pen_editing), (SettingsTab::Pens, None), "an open color list is put away");
        let _ = app.update(Message::SetBibleSize(30.0));
        let pens = app.sh.settings.pens.clone();
        assert!(!pens.is_empty());
        let _ = app.update(Message::ResetSettings);
        assert_eq!(app.sh.settings.bible_size, Settings::default().bible_size);
        assert_eq!(app.sh.settings.pens, pens);
    }

    #[test]
    fn a_commentary_is_read_by_chapter_and_opened_from_the_reader_and_kept_with_the_workspace() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let Some(gill) = app.sh.backend.commentaries().into_iter().find(|c| c.id.0 == "Gill") else {
            return eprintln!("skipped: Gill is not installed");
        };
        // The notes: one per verse that has one, the first paragraph naming the verse.
        let note = app.sh.backend.commentary_note(&gill.id, Book::JOHN, 3, 16).expect("a note on John 3:16");
        assert_eq!(note[0][0].text, "Ver. 16.");
        assert!(note.iter().any(|p| p.iter().any(|s| s.bold)), "its heading is bold");
        assert!(note.iter().flatten().all(|s| !s.text.contains('<')), "no markup is left");

        // From the reader's menu: a commentary tab beside it, on the chapter and verse; once.
        select_verses(&mut app, &[16]);
        let bid = app.bid();
        let before = app.tabs.len();
        let _ = app.update(Message::Bible(bid, BibleMsg::OpenCommentary));
        assert_eq!(app.tabs.len(), before + 1);
        let tab = app.tabs.values().find_map(|t| match t { Tab::Commentary(c) => Some(c), _ => None }).expect("a commentary tab");
        assert_eq!((tab.module.0.as_str(), tab.book, tab.chapter, tab.verse), ("Gill", Book::JOHN, 3, Some(16)));
        assert_eq!(tab.title(&app.sh), "Gill \u{b7} John 3");
        let notes = tab.notes(&app.sh);
        assert!(notes.iter().any(|(v, _)| *v == 16) && notes.windows(2).all(|w| w[0].0 < w[1].0), "in verse order");
        let head = tab.doc.selection().head;
        assert!(tab.doc.source()[head..].starts_with("[[@John 3:16]]"), "the text is at the verse asked for");
        let _ = app.update(Message::Bible(bid, BibleMsg::OpenCommentary));
        assert_eq!(app.tabs.values().filter(|t| matches!(t, Tab::Commentary(_))).count(), 1, "the same tab");

        // Its pickers and links.
        let id = app.tabs.iter().find(|(_, t)| matches!(t, Tab::Commentary(_))).map(|(i, _)| *i).unwrap();
        let _ = app.update(Message::Commentary(id, CommentaryMsg::NextChapter));
        let Some(Tab::Commentary(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert_eq!((tab.book, tab.chapter), (Book::JOHN, 4));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::PickBook(bible_tab::BookChoice(Book::ROMANS))));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::PrevChapter));
        let Some(Tab::Commentary(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert_eq!((tab.book, tab.chapter), (Book::from_osis("Acts").unwrap(), 28), "back from Romans 1 into Acts");
        app.bible_mut().chapter_no = 1;
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Link(study_core::markdown::LinkTarget::Verse { range: scripture::RefRange { book: Book::JOHN, chapter: 3, verses: Some((16, 16)) }, module: None })));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3), "a verse link shows it in the reader on screen");
        assert!(matches!(app.tabs.get(&id), Some(Tab::Commentary(_))), "and the commentary stays");

        // The text selects and moves but cannot be changed; what is copied is the words.
        let _ = app.update(Message::Commentary(id, CommentaryMsg::PickBook(bible_tab::BookChoice(Book::JOHN))));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::PickChapter(3)));
        let before = match app.tabs.get(&id) { Some(Tab::Commentary(t)) => t.doc.source().to_string(), _ => unreachable!() };
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Action(md_editor::Action::Insert("typed".into()))));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Action(md_editor::Action::Backspace)));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Action(md_editor::Action::Paste("pasted".into()))));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Action(md_editor::Action::SelectAll)));
        let Some(Tab::Commentary(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert_eq!(tab.doc.source(), before, "not edited");
        let selected = tab.doc.selected_text().expect("everything is selected");
        assert_eq!(selected, before);
        let copied = crate::commentary_tab::plain_text(&selected);
        assert!(copied.contains("John 3:16") && !copied.contains("**") && !copied.contains("[["), "{}", &copied[..copied.len().min(200)]);
        assert!(copied.contains("For God so loved the world, &c.]"), "an escaped bracket copies as itself");

        // Kept in the workspace, and put back.
        let saved = app.workspace();
        assert!(saved.panes.iter().any(|p| p.tabs.iter().any(|t| matches!(t, state::TabSave::Commentary { module, .. } if module == "Gill"))));
        let _ = app.apply_workspace(&saved);
        assert!(app.tabs.values().any(|t| matches!(t, Tab::Commentary(c) if c.module.0 == "Gill")));
    }

    #[test]
    fn searching_the_bible_finds_words_phrases_alternatives_strongs_numbers_and_scopes() {
        use crate::bible_search::Options;
        use study_core::search::Query;
        let home = tempfile::tempdir().unwrap();
        let Some(app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        let module = app.sh.backend.modules().into_iter().find(|m| m.has_strongs).map(|m| m.id).expect("a module with Strong's numbers");
        let find = |text: &str, options: Options| app.sh.search_bible(&module, &Query::parse(text), options).unwrap();
        let places = |r: &crate::bible_search::Results| r.found.iter().map(|f| f.range.to_string()).collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let phrase = find("\"only begotten\" in:John", Options::default());
        eprintln!("index made and searched in {:?}", started.elapsed());
        assert!(places(&phrase).contains(&"John 3:16".to_string()), "{:?}", places(&phrase));
        let marked: Vec<&str> = phrase.found[0].spans.iter().map(|&(a, z)| &phrase.found[0].text[a..z]).collect();
        assert!(marked.iter().any(|w| w.to_lowercase() == "only begotten"), "{marked:?}");
        assert!(phrase.found.iter().all(|f| f.range.book == Book::JOHN), "only the book asked for");
        // All the words, in canon order; either word; leaving one out.
        let both = find("loved world", Options::default());
        assert!(places(&both).contains(&"John 3:16".to_string()) && both.found.len() > 3);
        let order: Vec<_> = both.found.iter().map(|f| (Book::all().position(|b| b == f.range.book), f.range.chapter, f.range.verses)).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "canon order");
        assert!(find("loved OR zzzzqq", Options::default()).found.len() > both.found.len());
        assert!(!places(&find("loved world -begotten", Options::default())).contains(&"John 3:16".to_string()));
        // Other forms, and capitals.
        assert!(find("love world", Options::default()).found.len() < find("love world", Options { stem: true, ..Options::default() }).found.len());
        assert!(find("god", Options { case: true, ..Options::default() }).found.len() < find("god", Options::default()).found.len());
        // A Strong's number, alone and with words.
        let numbered = find("G25", Options::default());
        assert!(places(&numbered).contains(&"John 3:16".to_string()) && numbered.found.len() > 100);
        assert!(numbered.found.iter().any(|f| !f.spans.is_empty()), "the words with the number are marked");
        let mixed = find("G25 world in:John", Options::default());
        assert!(places(&mixed).contains(&"John 3:16".to_string()) && mixed.found.len() < numbered.found.len());
        assert!(find("", Options::default()).found.is_empty() && find("zzzzqqqq", Options::default()).found.is_empty());
    }

    #[test]
    fn a_search_tab_searches_shows_a_verse_with_its_words_and_is_kept_with_the_workspace() {
        let home = tempfile::tempdir().unwrap();
        let Some(mut app) = live_app(home.path()) else { return eprintln!("skipped: no modules") };
        // From a new tab; the module being read.
        let blank = app.new_blank();
        let blank_id = App::id_of(&blank);
        app.add_tab(0, blank);
        app.open_choice(blank_id, Choice::Search);
        let Some(Tab::Search(tab)) = app.tabs.get(&blank_id) else { panic!("a search tab") };
        assert_eq!(tab.module, app.bible().module);
        let id = blank_id;
        let _ = app.update(Message::Search(id, SearchMsg::Draft("only".into())));
        let _ = app.update(Message::Search(id, SearchMsg::Draft("only begotten".into())));
        // Typing is searched when it pauses: not for what was typed before the last of it.
        let _ = app.update(Message::Search(id, SearchMsg::Typed(1)));
        let Some(Tab::Search(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert!(tab.busy.is_none() && tab.ran.is_none(), "an earlier pause does not search");
        let _ = app.update(Message::Search(id, SearchMsg::Typed(2)));
        // The first time the index is made a moment later, so the screen can say so.
        let Some(Tab::Search(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert!(tab.busy.is_some() && tab.ran.is_none(), "indexing is shown first");
        let _ = app.update(Message::Search(id, SearchMsg::Run));
        let Some(Tab::Search(tab)) = app.tabs.get(&id) else { unreachable!() };
        let ran = tab.ran.as_ref().expect("searched");
        assert!(tab.busy.is_none() && ran.results.found.iter().any(|f| f.range.to_string() == "John 3:16"));
        assert_eq!(tab.title(&app.sh), "only begotten \u{b7} LSB");
        // The next search needs no waiting.
        let _ = app.update(Message::Search(id, SearchMsg::Best(true)));
        let Some(Tab::Search(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert!(tab.busy.is_none() && tab.ran.is_some());
        // Choosing a verse shows it in the reader with the words pointed out.
        let found = tab.ran.as_ref().unwrap().results.found.iter().find(|f| f.range.to_string() == "John 3:16").unwrap().clone();
        app.bible_mut().chapter_no = 1;
        let module = app.bible().module.clone();
        let _ = app.update(Message::Search(id, SearchMsg::Open(found.range, module, found.spans.clone())));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3));
        assert!(app.bible().select_request.is_none());
        let slice = app.bible().select_words.expect("the words are to be selected").1;
        let canon = app.bible().chapter.as_ref().unwrap().canonical_verses()[&16].clone();
        assert!(canon[slice.start..slice.end].to_lowercase().contains("only begotten"), "{}", &canon[slice.start..slice.end]);
        // (No Bible tab was on John 3, so the search tab was taken over by a new one, which goes back to it.)
        let shown = app.last_bible.unwrap();
        assert!(app.tabs.get(&id).is_none());
        let _ = app.update(Message::NavTab(shown, true));
        assert!(matches!(app.tabs.get(&id), Some(Tab::Search(_))));
        // Narrowing to a book, and showing more.
        let _ = app.update(Message::Search(id, SearchMsg::Book(Some(Book::JOHN))));
        let _ = app.update(Message::Search(id, SearchMsg::More));
        // From the reader: the selected words are searched in the tab there is.
        select_verses(&mut app, &[16]);
        let bid = app.bid();
        let before = app.tabs.len();
        let _ = app.update(Message::Bible(bid, BibleMsg::SearchSelection));
        assert_eq!(app.tabs.len(), before, "the same search tab");
        let Some(Tab::Search(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert!(tab.draft.starts_with('"') && tab.draft.contains("God so loved"), "{}", tab.draft);
        // Kept with the workspace, searched again when it is put back.
        let saved = app.workspace();
        assert!(saved.panes.iter().any(|p| p.tabs.iter().any(|t| matches!(t, state::TabSave::Search { best: true, query, .. } if query.contains("God so loved")))));
        let _ = app.apply_workspace(&saved);
        assert!(app.tabs.values().any(|t| matches!(t, Tab::Search(s) if s.options.by_relevance && s.draft.contains("God so loved"))));
    }

    #[test]
    fn the_readers_search_button_opens_the_search_tab_once() {
        let mut app = App::new();
        let bid = app.bid();
        let searches = |app: &App| app.tabs.values().filter(|t| matches!(t, Tab::Search(_))).count();
        assert_eq!(searches(&app), 0);
        let _ = app.update(Message::Bible(bid, BibleMsg::OpenSearch));
        assert_eq!(searches(&app), 1);
        let id = app.tabs.iter().find(|(_, t)| matches!(t, Tab::Search(_))).map(|(i, _)| *i).unwrap();
        assert_eq!(app.panes.iter().find(|p| p.active == Some(id)).map(|p| p.id), Some(app.focus), "shown");
        let _ = app.update(Message::Bible(bid, BibleMsg::OpenSearch));
        assert_eq!(searches(&app), 1, "the same tab again");
    }

    #[test]
    fn pressing_the_marker_of_an_open_card_leaves_the_card_alone() {
        let mut h = Harness::new(App::new());
        let (x, y) = h.click_scan(|app| app.bible().note_card.is_some()).expect("a footnote marker opens a card");
        assert!(h.app.bible().note_card.is_some());
        // The same marker again, after a pause (not a double click): neither the press nor the release closes it to open it
        // afresh.
        std::thread::sleep(std::time::Duration::from_millis(450));
        h.move_to(x, y);
        h.press();
        assert!(h.app.bible().note_card.is_some(), "the press does not close it");
        h.release();
        assert!(h.app.bible().note_card.is_some(), "and the release does not open another");
        // Pressing somewhere else does close it.
        h.click(300.0, 900.0);
        assert!(h.app.bible().note_card.is_none());
    }

    #[test]
    fn a_new_reader_has_the_view_choices_of_the_one_last_used_in_the_page_and_the_margin() {
        let mut app = App::new();
        app.bible_mut().show_strongs = true;
        app.bible_mut().show_refs = true;
        app.bible_mut().in_margin = state::MarginKinds { strongs: true, marks: false, footnotes: true, refs: true };
        let blank = app.new_blank();
        let id = App::id_of(&blank);
        app.add_tab(1, blank);
        app.open_choice(id, Choice::Bible);
        let Some(Tab::Bible(new)) = app.tabs.get(&id) else { panic!("a reader") };
        assert!(new.show_strongs && new.show_refs);
        assert_eq!(new.in_margin, state::MarginKinds { strongs: true, marks: false, footnotes: true, refs: true });
    }

    #[test]
    fn what_a_reader_was_asked_to_do_once_is_not_done_again_when_its_tab_is_shown_again() {
        let mut app = App::new();
        let reader = app.bid();
        let range = scripture::RefRange { book: Book::JOHN, chapter: 3, verses: Some((2, 3)) };
        let _ = app.show_verse_in(reader, range);
        let _ = app.update(Message::SaveSession);
        assert!(app.bible().select_request.is_some() && app.bible().reveal.is_some(), "asked, and shown");
        // Another tab comes in front: the reader is out of view, and has done what it was asked.
        let blank = app.new_blank();
        let other = App::id_of(&blank);
        app.add_tab(0, blank);
        let _ = app.update(Message::SaveSession);
        assert!(app.bible_tab(reader).unwrap().select_request.is_none() && app.bible_tab(reader).unwrap().reveal.is_none());
        let _ = app.update(Message::TabPressed(reader));
        assert!(app.bible_tab(reader).unwrap().select_request.is_none(), "shown again: nothing to do again");
        let _ = other;
    }

    #[test]
    fn reader_modes_back_and_forward_go_through_the_readers_own_history_and_stay_in_reader_mode() {
        let mut app = App::new();
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        let _ = app.update(Message::Bible(id, BibleMsg::ReadingMode));
        assert!(app.reader_mode.is_some() && app.inner_can(id, true));
        let _ = app.update(Message::NavTab(id, true));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::JOHN, 3), "back to where it was");
        assert!(app.reader_mode.is_some(), "still reading");
        assert!(app.inner_can(id, false));
        let _ = app.update(Message::NavTab(id, false));
        assert_eq!(app.bible().book, Book::PSALMS);
    }

    #[test]
    fn reader_mode_puts_the_reader_back_at_the_same_share_of_the_way_down_and_asks_again_if_it_starts_at_the_top() {
        let mut app = App::new();
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(300.0, 0.4)));
        assert_eq!(app.bible().scroll_rel, 0.4);
        // Begun: the share is to be put back.
        let _ = app.update(Message::Bible(id, BibleMsg::ReadingMode));
        assert_eq!(app.pending_scroll, Some((id, 0.4, 0)));
        // The new scrollable says it is at the top: it is asked again (and the tab keeps what it had).
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(0.0, 0.0)));
        assert_eq!(app.pending_scroll, Some((id, 0.4, 1)));
        assert_eq!(app.bible().scroll_rel, 0.4);
        // Where it should be: done, and the tab follows it from then on.
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(500.0, 0.41)));
        assert_eq!(app.pending_scroll, None);
        assert_eq!(app.bible().scroll_rel, 0.41);
        // And ended: the same again, for the share it was left at.
        let _ = app.update(Message::EndReading);
        assert_eq!(app.pending_scroll, Some((id, 0.41, 0)));
        // At the top, there is nothing to put back.
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(0.0, 0.0)));
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(0.0, 0.0)));
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(0.0, 0.0)));
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(0.0, 0.0)));
        assert_eq!(app.pending_scroll, None, "asked three times, and then let be");
    }

    #[test]
    fn the_chapter_has_a_button_to_the_notes_that_link_to_it_and_a_card_of_the_days_it_was_read_that_closes() {
        let mut app = App::new();
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::OpenChapterBacklinks));
        let tab = app.tabs.values().find_map(|t| match t {
            Tab::Backlinks(b) => Some(b),
            _ => None,
        });
        let tab = tab.expect("a tab of the notes that link here");
        let chapter = RefRange::chapter(Book::JOHN, 3);
        assert_eq!(tab.target, crate::backlinks_tab::Target::Passage(chapter), "the whole chapter, not a verse of it");
        // Nicodemus links John 3:2, and the notes that link to any verse of the chapter are all there.
        assert!(app.sh.backlinks(&chapter).iter().any(|b| b.path.contains("Nicodemus")));
        // The card of the days read opens, and is let go of by its own message (a click elsewhere or Escape sends it).
        let _ = app.update(Message::Bible(id, BibleMsg::ToggleReadCard));
        assert!(app.bible().read_card);
        let _ = app.update(Message::Bible(id, BibleMsg::CloseReadCard));
        assert!(!app.bible().read_card);
    }

    #[test]
    fn a_notes_scroll_is_kept_by_its_tab_and_put_back_when_the_tab_is_shown_again() {
        use iced_test::core::widget::operation::black_box;
        let mut app = App::new();
        let long: String = (1..=120).map(|n| format!("Line {n} of a long note.\n\n")).collect();
        let note = app.nid();
        if let Some(Tab::Note(t)) = app.tabs.get_mut(&note) {
            t.doc = Document::new(&long);
        }
        let mut h = Harness::new(app);
        let editor = match h.app.tabs.get(&note) { Some(Tab::Note(t)) => t.editor_id(), _ => unreachable!() };
        // Scroll it, as the app does when a tab is shown again.
        let mut ui = UserInterface::build(h.app.view(), h.size, std::mem::take(&mut h.cache), &mut h.renderer);
        let mut op = iced::advanced::widget::operation::scrollable::scroll_to::<()>(editor.clone(), AbsoluteOffset { x: None, y: Some(400.0) });
        ui.operate(&h.renderer, &mut black_box(&mut op));
        h.cache = ui.into_cache();
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let Some(Tab::Note(t)) = h.app.tabs.get(&note) else { unreachable!() };
        assert!((t.scroll - 400.0).abs() < 1.0, "the tab was told how far it is scrolled: {}", t.scroll);
    }

    #[test]
    fn a_selection_is_held_as_a_place_when_something_else_is_pressed_or_its_tab_is_put_away() {
        let mut h = Harness::new(App::new());
        let reader = h.app.bid();
        let range = scripture::RefRange { book: Book::JOHN, chapter: 3, verses: Some((3, 3)) };
        let _ = h.app.show_verse_in(reader, range);
        let frames = |h: &mut Harness| {
            for _ in 0..3 {
                h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            }
        };
        frames(&mut h);
        let held = h.app.bible().text_selection.clone();
        assert!(held.is_some());
        // The focus is on the note's pane, and a press on empty ground there leaves the selection, and
        // the focus, as they were.
        let note = h.app.nid();
        let note_pane = h.app.panes[h.app.pane_of(note).unwrap()].id;
        h.app.focus = note_pane;
        h.click(900.0, 1200.0);
        assert_eq!(h.app.bible().text_selection, held, "the press did not end the selection");
        assert_eq!(h.app.focus, note_pane);
        // Another tab comes in front of the reader and goes away: the selection is where it was.
        let blank = h.app.new_blank();
        let other = App::id_of(&blank);
        let pane = h.app.pane_of(reader).unwrap();
        h.app.add_tab(pane, blank);
        let _ = h.app.update(Message::SaveSession);
        frames(&mut h);
        let _ = h.app.update(Message::TabPressed(reader));
        frames(&mut h);
        assert_eq!(h.app.bible().text_selection, held, "put back when the reader is shown again");
        let _ = other;
        // A click in the reader's own text is a new selection, as ever.
        h.click_n(300.0, 400.0, 2);
        assert_ne!(h.app.bible().text_selection, held);
    }

    #[test]
    fn a_card_pressed_with_the_command_key_is_opened_in_a_tab_of_its_own() {
        let mut app = App::new();
        let reader = app.bid();
        let index = app.sh.marks.iter().position(|m| !m.body.is_empty()).unwrap();
        let path = app.sh.marks[index].path();
        let tabs = app.tabs.len();
        let pane = app.pane_of(reader).unwrap();
        // Plain, it is written in.
        let _ = app.update(Message::Bible(reader, BibleMsg::StartEdit(index)));
        assert_eq!(app.tabs.len(), tabs, "no new tab");
        assert_eq!(app.bible().editing, Some(index));
        app.bible_mut().editing = None;
        // With the command key, it is a note tab beside the reader's own tab, in its pane.
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::COMMAND));
        let _ = app.update(Message::Bible(reader, BibleMsg::StartEdit(index)));
        assert_eq!(app.tabs.len(), tabs + 1);
        assert!(app.bible().editing.is_none(), "it was not written in");
        let shown = app.panes[pane].active.expect("a tab shows");
        assert!(matches!(app.tabs.get(&shown), Some(Tab::Note(n)) if n.path == path), "the note, in the reader's pane");
    }

    #[test]
    fn marking_a_chapter_read_and_bookmarking_say_where_they_were_added() {
        let mut app = App::new();
        let bid = app.bid();
        let now = chrono::Local::now();
        let _ = app.update(Message::Bible(bid, BibleMsg::LogChapter));
        assert_eq!(app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Added to reading log"));
        let daily = app.sh.templates().into_iter().find(|t| t.name == "Daily").unwrap();
        let made = study_core::templates::plan(&daily, &now, None, |_| false);
        app.sh.notes.insert(made.path, made.text.unwrap());
        app.sh.toast = None;
        let _ = app.update(Message::Bible(bid, BibleMsg::LogChapter));
        assert_eq!(app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Removed from reading log"), "it was not in the daily note, which did not exist yet");
        let _ = app.update(Message::Bible(bid, BibleMsg::LogChapter));
        assert_eq!(app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Added to reading log and daily note"));
        let _ = app.update(Message::Bible(bid, BibleMsg::LogChapter));
        assert_eq!(app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Removed from reading log and daily note"));
        let _ = app.update(Message::Bible(bid, BibleMsg::LogChapter));
        app.sh.toast = None;
        let _ = app.update(Message::SetBookmarksDaily(true));
        select_verses(&mut app, &[3]);
        let _ = app.update(Message::Bible(bid, BibleMsg::ApplyPen(1)));
        assert_eq!(app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Added to daily note"));
        // Deleting the mark takes its bookmark out again.
        let index = app.sh.marks.len() - 1;
        let mark_path = app.sh.marks[index].path();
        app.sh.toast = None;
        app.sh.delete_mark(index, None);
        assert_eq!(app.sh.toast.as_ref().map(|t| t.text.as_str()), Some("Removed from daily note"));
        assert!(app.sh.notes.values().all(|t| !t.contains(mark_path.trim_end_matches(".md"))), "no link to it is left in a daily note");
    }

    #[test]
    fn the_reading_logs_day_headings_link_to_their_daily_notes_while_those_exist() {
        use crate::shared::READING_LOG;
        let mut app = App::new();
        let now = chrono::Local::now();
        let day = now.format("%Y-%m-%d").to_string();
        assert!(app.sh.log_reading(Book::JOHN, 3, &now));
        assert!(app.sh.notes[READING_LOG].contains(&format!("## {day}\n")), "no daily note yet: a plain date");
        // Making today's note links its day.
        let daily = app.sh.templates().into_iter().find(|t| t.name == "Daily").unwrap();
        let made = study_core::templates::plan(&daily, &now, None, |_| false);
        let path = made.path.clone();
        let template = daily.path.clone();
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::PickKind(template)));
        assert!(app.sh.notes.contains_key(&path), "the note was made");
        let link = format!("## [[{}|{day}]]", path.trim_end_matches(".md"));
        assert!(app.sh.notes[READING_LOG].contains(&link), "{}", app.sh.notes[READING_LOG]);
        // Taking a chapter back and logging another leaves it linked; the note going away unlinks it.
        let _ = app.sh.log_reading(Book::JOHN, 4, &now);
        assert!(app.sh.notes[READING_LOG].contains(&link));
        app.sh.delete_note(&path);
        assert!(app.sh.notes[READING_LOG].contains(&format!("## {day}\n")) && !app.sh.notes[READING_LOG].contains("[[notes"), "{}", app.sh.notes[READING_LOG]);
    }

    #[test]
    fn a_new_notes_tab_opens_on_todays_note_when_there_is_one() {
        let mut app = App::new();
        let pane = app.panes[0].id;
        let first = app.sh.first_ordinary_note();
        let _ = app.update(Message::NewTab(pane));
        let blank = *app.panes[0].tabs.last().unwrap();
        let _ = app.update(Message::Open(blank, Choice::Note));
        assert!(matches!(&app.tabs[&blank], Tab::Note(n) if n.path == first), "with no daily note, the first ordinary one");
        let now = chrono::Local::now();
        let daily = app.sh.templates().into_iter().find(|t| t.name == "Daily").unwrap();
        let made = study_core::templates::plan(&daily, &now, None, |_| false);
        let path = made.path.clone();
        app.sh.notes.insert(made.path, made.text.unwrap());
        let _ = app.update(Message::NewTab(pane));
        let blank = *app.panes[0].tabs.last().unwrap();
        let _ = app.update(Message::Open(blank, Choice::Note));
        assert!(matches!(&app.tabs[&blank], Tab::Note(n) if n.path == path), "today's note, once it exists");
    }

    #[test]
    fn a_new_mark_is_added_to_todays_note_under_bookmarks_when_that_is_on_making_the_heading_if_need_be() {
        let mut app = App::new();
        let now = chrono::Local::now();
        let daily = app.sh.templates().into_iter().find(|t| t.name == "Daily").unwrap();
        let made = study_core::templates::plan(&daily, &now, None, |_| false);
        let path = made.path.clone();
        assert!(made.text.as_ref().unwrap().contains("## Bookmarks"), "a new daily note has the heading");
        app.sh.notes.insert(made.path, made.text.unwrap());
        let bid = app.bid();
        // Off to begin with.
        select_verses(&mut app, &[2]);
        let _ = app.update(Message::Bible(bid, BibleMsg::ApplyPen(0)));
        assert!(!app.sh.notes[&path].contains("|John 3:2]]"));
        // On: the next mark is a line under Bookmarks, with its tags.
        let _ = app.update(Message::SetBookmarksDaily(true));
        select_verses(&mut app, &[3]);
        let _ = app.update(Message::Bible(bid, BibleMsg::ApplyPen(1)));
        let text = &app.sh.notes[&path];
        let mark_path = app.sh.marks.last().unwrap().path();
        let link = format!("[[{}|John 3:3]]", mark_path.trim_end_matches(".md"));
        assert!(text.contains(&format!("## Bookmarks\n\n- {link}\n")), "{text}");
        // Writing in the mark's note renames its file: the link follows, still called by the passage.
        let index = app.sh.marks.len() - 1;
        app.sh.set_body(index, "Because.".into());
        let renamed = app.sh.marks[index].path();
        assert_ne!(renamed, mark_path);
        assert!(app.sh.notes[&path].contains(&format!("- [[{}|John 3:3]]", renamed.trim_end_matches(".md"))), "{}", app.sh.notes[&path]);
        let text = app.sh.notes[&path].clone();
        let text = &text;
        // No heading: it is added, at the end, as the Reading one is.
        let bare = text.replace("## Bookmarks", "## Elsewhere");
        app.sh.notes.insert(path.clone(), bare);
        select_verses(&mut app, &[16]);
        let _ = app.update(Message::Bible(bid, BibleMsg::ApplyPen(1)));
        let text = &app.sh.notes[&path];
        let made = app.sh.marks.last().unwrap().path();
        assert!(text.trim_end().ends_with(&format!("## Bookmarks\n\n- [[{}|John 3:16]]", made.trim_end_matches(".md"))), "{text}");
        // Off again: nothing is added, heading or not.
        let _ = app.update(Message::SetBookmarksDaily(false));
        select_verses(&mut app, &[2]);
        let _ = app.update(Message::Bible(bid, BibleMsg::ApplyPen(2)));
        assert!(!app.sh.notes[&path].contains("|John 3:2]]"));
    }

    #[test]
    fn a_card_left_open_when_its_tab_goes_away_does_not_come_back_unseen_in_the_way_of_the_toolbar() {
        let mut h = Harness::new(App::new());
        let reader = h.app.bid();
        h.click_scan(|app| app.bible().note_card.is_some()).expect("a footnote marker opens a card");
        let frames = |h: &mut Harness| {
            for _ in 0..3 {
                h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            }
        };
        // The tab goes away and comes back.
        let blank = h.app.new_blank();
        let pane = h.app.pane_of(reader).unwrap();
        h.app.add_tab(pane, blank);
        let _ = h.app.update(Message::SaveSession);
        let _ = h.app.update(Message::TabPressed(reader));
        frames(&mut h);
        assert!(h.app.bible().note_card.is_none(), "the card is not left behind");
        // And a selection gets its toolbar.
        h.click_n(300.0, 500.0, 2);
        frames(&mut h);
        assert!(h.app.bible().text_selection.is_some());
        assert!(h.center_of("Add Note").is_some(), "the selection's toolbar is there");
        // Even a card that is somehow still open when the widget is made again is let go of.
        h.app.bible_mut().note_card = Some(bible_tab::NoteCard { content: bible_tab::NoteContent::Footnote("x".into()) });
        h.app.bible_mut().text_selection = None;
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let _ = h.app.update(Message::Bible(reader, BibleMsg::SetInMargin(bible_tab::NoteKind::Marks, false)));
        h.app.bible_mut().note_card = Some(bible_tab::NoteCard { content: bible_tab::NoteContent::Footnote("y".into()) });
        let blank = h.app.new_blank();
        h.app.add_tab(pane, blank);
        let _ = h.app.update(Message::SaveSession);
        frames(&mut h);
        h.app.bible_mut().note_card = Some(bible_tab::NoteCard { content: bible_tab::NoteContent::Footnote("z".into()) });
        let _ = h.app.update(Message::TabPressed(reader));
        frames(&mut h);
        assert!(h.app.bible().note_card.is_none(), "a stale card is let go of by the new widget");
    }

    #[test]
    fn the_mark_as_read_button_shows_a_check_for_a_chapter_read_today_and_takes_it_back_and_a_card_lists_the_days() {
        let mut app = App::new();
        let id = app.bid();
        let now = chrono::Local::now();
        assert!(!app.sh.settings.reading_log_auto, "by itself only when asked for");
        assert!(!app.sh.read_on(Book::JOHN, 3, &now));
        let _ = app.update(Message::Bible(id, BibleMsg::LogChapter));
        assert!(app.sh.notes[crate::shared::READING_LOG].contains("- [[@John 3]]"), "a line with no time");
        assert!(app.sh.read_on(Book::JOHN, 3, &now));
        // The check mark stays: it is whether the chapter is in today's log.
        let mut h = Harness::new(app);
        assert!(h.center_of("\u{2713} Marked as read").is_some() && h.center_of("Mark as read").is_none());
        // Another day's reading is in the history, and a chapter not read today has the words.
        let log = format!("{}\n## 2025-01-02\n\n- [[@John 3]]\n- [[@John 5]]\n", h.app.sh.notes[crate::shared::READING_LOG].trim_end());
        h.app.sh.notes.insert(crate::shared::READING_LOG.into(), log);
        let today = now.format("%Y-%m-%d").to_string();
        assert_eq!(h.app.sh.days_read(Book::JOHN, 3).len(), 2);
        assert_eq!(h.app.sh.days_read(Book::JOHN, 3)[1], "2025-01-02");
        assert!(h.app.sh.days_read(Book::JOHN, 3).contains(&today));
        // The clock opens the card, and the card the log.
        let id = h.app.bid();
        let _ = h.app.update(Message::Bible(id, BibleMsg::ToggleReadCard));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("John 3 was read on 2 days").is_some());
        assert!(h.center_of("2025-01-02").is_some() && h.center_of(&format!("{today}  \u{b7}  today")).is_some());
        let _ = h.app.update(Message::Bible(id, BibleMsg::OpenReadingLog));
        assert_eq!(h.app.log().path, crate::shared::READING_LOG, "the log has a tab of its own");
        assert!(matches!(h.app.tabs.get(&h.app.panes.iter().find_map(|p| p.active.filter(|a| *a == h.app.lid())).unwrap()), Some(Tab::Log(_))), "and it is shown");
        // Pressing the check takes the mark back (from the log, and the day's note).
        let _ = h.app.update(Message::Bible(id, BibleMsg::LogChapter));
        assert!(!h.app.sh.read_on(Book::JOHN, 3, &now), "taken back");
        assert_eq!(h.app.sh.days_read(Book::JOHN, 3), ["2025-01-02"], "other days are left alone");
        assert!(h.center_of("Mark as read").is_some());
    }

    #[test]
    fn the_wait_before_a_chapter_counts_as_read_is_a_setting_within_bounds() {
        let mut app = App::new();
        let _ = app.update(Message::SetReadingSeconds(90));
        assert_eq!(app.sh.settings.reading_log_seconds, 90);
        let _ = app.update(Message::SetReadingSeconds(1));
        assert_eq!(app.sh.settings.reading_log_seconds, state::READ_SECONDS.start);
    }

    #[test]
    fn a_chapter_in_front_of_the_reader_long_enough_is_added_to_the_reading_log_and_todays_note() {
        use std::time::{Duration, Instant};
        let mut app = App::new();
        let now = chrono::Local::now();
        let daily = app.sh.templates().into_iter().find(|t| t.name == "Daily").expect("the daily template");
        let made = study_core::templates::plan(&daily, &now, None, |_| false);
        let daily_path = made.path.clone();
        app.sh.notes.insert(made.path, made.text.unwrap());
        // Off to begin with: time in front of the reader logs nothing.
        let t0 = Instant::now();
        app.note_reading(t0);
        app.note_reading(t0 + Duration::from_secs(300));
        assert!(!app.sh.notes.contains_key(crate::shared::READING_LOG));
        let _ = app.update(Message::SetReadingAuto(true));
        let t0 = t0 + Duration::from_secs(400);
        app.note_reading(t0);
        app.note_reading(t0 + Duration::from_secs(10));
        assert!(!app.sh.notes.contains_key(crate::shared::READING_LOG), "not yet");
        app.note_reading(t0 + Duration::from_secs(31));
        let log = app.sh.notes.get(crate::shared::READING_LOG).expect("the log is made").clone();
        assert!(log.contains("tags: [reading-log]") && log.contains("# Reading log"), "{log}");
        // (Today's note exists, so the day's heading is a link to it.)
        let day = now.format("%Y-%m-%d");
        assert!(log.contains(&format!("## [[{}|{day}]]", daily_path.trim_end_matches(".md"))) && log.contains("- [[@John 3]]"), "{log}");
        assert!(app.sh.notes[&daily_path].contains("## Reading\n\n- [[@John 3]]"), "{}", app.sh.notes[&daily_path]);
        app.note_reading(t0 + Duration::from_secs(120));
        assert_eq!(app.sh.notes[crate::shared::READING_LOG].matches("[[@John 3]]").count(), 1);
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::PickChapter(ChapterChoice(Book::JOHN, 4))));
        app.note_reading(t0 + Duration::from_secs(130));
        app.note_reading(t0 + Duration::from_secs(170));
        let log = &app.sh.notes[crate::shared::READING_LOG];
        assert!(log.find("[[@John 3]]").unwrap() < log.find("[[@John 4]]").unwrap(), "in the order read");
        let _ = app.update(Message::SetReadingAuto(false));
        let _ = app.update(Message::Bible(id, BibleMsg::PickChapter(ChapterChoice(Book::JOHN, 5))));
        app.note_reading(t0 + Duration::from_secs(200));
        app.note_reading(t0 + Duration::from_secs(300));
        assert!(!app.sh.notes[crate::shared::READING_LOG].contains("[[@John 5]]"));
    }

    #[test]
    fn a_notes_link_is_copied_from_its_menu_with_a_friendly_name() {
        let mut app = App::new();
        let id = app.nid();
        let link = |app: &App, path: &str| crate::note_tab::link_to(&app.sh, path);
        assert_eq!(link(&app, "notes/Nicodemus.md"), "[[Nicodemus]]");
        app.sh.notes.insert("old/Nicodemus.md".into(), "# Nicodemus\n".into());
        assert_eq!(link(&app, "notes/Nicodemus.md"), "[[notes/Nicodemus|Nicodemus]]");
        let mark = app.sh.marks[0].path();
        let copied = link(&app, &mark);
        assert!(copied.starts_with("[[margins/") && copied.contains("|John 3:16 \u{b7} "), "{copied}");
        let paths: Vec<String> = app.sh.notes.keys().cloned().collect();
        for path in ["notes/Nicodemus.md", "old/Nicodemus.md", mark.as_str()] {
            let made = link(&app, path);
            let found = study_core::markdown::find_links(&made);
            let Some(study_core::markdown::LinkTarget::Note(name)) = found.first().map(|l| l.target.clone()) else { panic!("{made}") };
            assert_eq!(study_core::markdown::resolve_note(&paths, &name).map(String::as_str), Some(path), "{made}");
        }
        app.open_note("notes/Nicodemus.md".into());
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        let _ = app.update(Message::Note(id, NoteMsg::CopyLink));
        assert!(app.sh.toast.as_ref().is_some_and(|t| t.text.contains("[[notes/Nicodemus|Nicodemus]]")));
        assert!(!app.note().menu_open);
    }

    #[test]
    fn a_note_open_while_something_is_added_to_it_keeps_the_caret_where_it_was() {
        let mut app = App::new();
        let id = app.nid();
        app.sh.notes.insert("notes/Log.md".into(), "# Log\n\nFirst words here.\n\nMore text.\n".into());
        app.open_note("notes/Log.md".into());
        let at = app.note().doc.source().find("words").unwrap();
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::SetCaret { offset: at, extend: false })));
        let end = format!("{}- added\n", app.sh.notes["notes/Log.md"]);
        app.sh.notes.insert("notes/Log.md".into(), end);
        app.sh.events.push(shared::Event::NoteRewritten { old: "notes/Log.md".into(), new: "notes/Log.md".into() });
        app.settle();
        assert!(app.note().doc.source().ends_with("- added\n"));
        assert_eq!(app.note().doc.selection().head, at, "the caret did not move");
        let middle = app.sh.notes["notes/Log.md"].replacen("# Log\n", "# Log\n\nNew line near the top.\n", 1);
        app.sh.notes.insert("notes/Log.md".into(), middle);
        app.sh.events.push(shared::Event::NoteRewritten { old: "notes/Log.md".into(), new: "notes/Log.md".into() });
        app.settle();
        let head = app.note().doc.selection().head;
        assert!(app.note().doc.source()[head..].starts_with("words"), "still in the same words: {:?}", &app.note().doc.source()[head..]);
    }

    #[test]
    fn a_commentary_of_ones_own_is_a_note_a_book_with_a_section_for_each_chapter_and_verse() {
        let mut app = App::new();
        assert!(app.sh.backend.commentaries().is_empty(), "no commentary installed in the mock");
        let blank = app.new_blank();
        let id = App::id_of(&blank);
        app.add_tab(0, blank);
        app.open_choice(id, Choice::Commentary);
        let Some(Tab::Commentary(tab)) = app.tabs.get(&id) else { panic!("a commentary tab") };
        assert!(tab.is_mine(), "with none installed it is the person's own");
        assert!(tab.doc.source().is_empty(), "nothing is said where nothing is written: {}", tab.doc.source());
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Passage("zzz".into())));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Begin));
        assert!(app.sh.toast.as_ref().is_some_and(|t| t.text.contains("is not a passage")));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Passage("John 3:16".into())));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Begin));
        let path = "notes/Commentary on John.md".to_string();
        assert_eq!(app.note().path, path);
        let text = app.sh.notes[&path].clone();
        assert!(text.contains("type: commentary") && text.contains("tags: [commentary]") && text.contains("# Commentary on John"), "{text}");
        assert!(text.contains("## John 3\n\n### John 3:16\n\n"), "{text}");
        let head = app.note().doc.selection().head;
        assert!(app.note().doc.source()[..head].ends_with("### John 3:16\n\n"), "the caret is under the heading");
        let _ = app.update(Message::Note(app.nid(), NoteMsg::Action(EditAction::Insert("God's love is the root.".into()))));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Passage("John 3".into())));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Begin));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Passage("John 4:1-3".into())));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Begin));
        let Some(Tab::Commentary(tab)) = app.tabs.get(&id) else { unreachable!() };
        assert_eq!((tab.book, tab.chapter), (Book::JOHN, 4), "the tab follows what was added to");
        let _ = app.update(Message::Commentary(id, CommentaryMsg::PickChapter(3)));
        let Some(Tab::Commentary(tab)) = app.tabs.get(&id) else { unreachable!() };
        let shown = tab.doc.source().to_string();
        assert!(shown.starts_with("[[@John 3]]  \u{b7}  [[notes/Commentary on John#John 3|Edit]]"), "{shown}");
        assert!(shown.contains("[[@John 3:16]]  \u{b7}  [[notes/Commentary on John#John 3:16|Edit]]\n\nGod's love is the root."), "{shown}");
        assert!(!shown.contains("John 4"), "only this chapter");
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Passage("John 3:16".into())));
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Begin));
        assert_eq!(app.sh.notes[&path].matches("### John 3:16").count(), 1);
        app.show_note("notes/Nicodemus.md".into());
        let _ = app.update(Message::Commentary(id, CommentaryMsg::Link(study_core::markdown::LinkTarget::Note("notes/Commentary on John#John 4:1-3".into()))));
        assert_eq!(app.note().path, path);
        let head = app.note().doc.selection().head;
        assert!(app.note().doc.source()[..head].ends_with("### John 4:1-3\n\n"));
        let saved = app.workspace();
        assert!(saved.panes.iter().any(|p| p.tabs.iter().any(|t| matches!(t, state::TabSave::Commentary { module, .. } if module == "zoe:mine"))));
    }

    #[test]
    fn saying_a_word_that_is_not_greek_or_hebrew_says_why_and_makes_no_sound() {
        let mut app = App::new();
        let id = app.bid();
        let _ = app.update(Message::Bible(id, BibleMsg::Speak("hello".into())));
        assert!(app.sh.toast.as_ref().is_some_and(|t| t.text.contains("no Greek or Hebrew")));
    }

    #[test]
    fn a_note_of_the_library_is_read_selected_and_linked_to_but_not_written_in() {
        let mut app = App::new();
        let path = "library/Morning and Evening/01-01.md";
        let text = "---\ntype: plan-day\n---\n# January 1\n\nIsrael's weary wanderings.\n";
        app.sh.notes.insert(path.into(), text.into());
        let id = app.nid();
        app.open_note(path.into());
        assert!(app.note().read_only());
        // Typing, pasting, deleting, bold, tags, properties, renaming and deleting do nothing.
        let at = app.note().doc.source().len();
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::SetCaret { offset: at, extend: false })));
        for action in [EditAction::Insert("x".into()), EditAction::Paste("y".into()), EditAction::Backspace, EditAction::Newline, EditAction::Wrap(md_editor::Wrap::Bold), EditAction::Undo] {
            let _ = app.update(Message::Note(id, NoteMsg::Action(action)));
        }
        for message in [NoteMsg::AddTag, NoteMsg::TagDraft("t".into()), NoteMsg::AddProperties, NoteMsg::StartRename, NoteMsg::AskDelete, NoteMsg::ConfirmDelete, NoteMsg::ToggleFront] {
            let _ = app.update(Message::Note(id, message));
        }
        assert_eq!(app.note().doc.source(), text);
        assert_eq!(app.sh.notes[path], text);
        assert!(app.note().renaming.is_none() && !app.note().confirm_delete);
        // Selecting works, and the link menu and copy link are there.
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::SelectAll)));
        assert!(app.note().doc.selected_text().is_some());
        let _ = app.update(Message::Note(id, NoteMsg::CopyLink));
        assert!(app.sh.toast.as_ref().is_some_and(|t| t.text.contains("[[")), "linked to like any note");
        // An ordinary note is written in as ever.
        app.open_note("notes/Nicodemus.md".into());
        assert!(!app.note().read_only());
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::SelectAll)));
        let _ = app.update(Message::Note(id, NoteMsg::Action(EditAction::Insert("changed".into()))));
        assert!(app.note().doc.source().ends_with("changed"), "{}", app.note().doc.source());
        // The menu of a library note has no Rename or Delete.
        app.open_note(path.into());
        let mut h = Harness::new(app);
        let _ = h.app.update(Message::Note(id, NoteMsg::ToggleMenu));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Copy link").is_some() && h.center_of("Rename").is_none() && h.center_of("Delete").is_none());
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
        // Only its reference and quote are gone from it: no lines are added, and the file keeps its name.
        assert!(!text.contains("[[@John 3:16]]") && !text.contains(&quote) && !text.contains("anchor:") && !text.contains("type:"), "{text}");
        assert_eq!(app.note().path, format!("notes/{}", path.rsplit('/').next().unwrap()));
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
    const ETERNAL: &str = "Eternal life begins now, not only after death. Compare [[@John 17:3]].";

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
        assert!(app.sh.notes.keys().any(|p| p.starts_with("notes/") && p.ends_with("-a002.md")), "the unlinked note stays, under its name");
    }

    #[test]
    fn a_long_card_being_edited_is_capped_too() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.sh.marks[0].body = "line of text\n".repeat(60);
        let _ = app.update(app.b(BibleMsg::StartEdit(0)));
        let mut h = Harness::new(app);
        let editor = { let id = h.app.bible().card_editor_id(); h.bounds_of(id) };
        let (_, actions) = h.center_of("Delete").expect("the action row is still reachable");
        // tags + editor (capped) + action row + padding: nowhere near sixty lines.
        assert!(actions - editor.y < CARD_BODY_MAX + 140.0, "the actions are {}px below the editor's top", actions - editor.y);
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
        // (A double-click past the end of a short line selects nothing: find a line with text at x=100.)
        let y = (0..40)
            .map(|i| y + i as f32 * 4.0)
            .find(|&y| {
                h.move_to(100.0, y);
                h.press();
                h.release();
                h.press();
                h.release();
                let hit = h.app.bible().text_selection.is_some();
                h.move_to(700.0, 700.0);
                h.press();
                h.release();
                hit
            })
            .expect("a line with text at x=100");
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
        // With no margin the new mark's card opens under the selection, for a note, like Add Note.
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
        assert_eq!(copy_text(&refs, &module, false), "[[@John 3:3-5]], [[@John 3:8]]");
        assert_eq!(copy_text(&refs, &module, true), "![[@John 3:3-5@KJV]]\n\n![[@John 3:8@KJV]]");
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
    const NOTE_PANE_MARK: &str = "Add tag";

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
        // (A link in the reader navigates the reader.)
        let _ = app.update(app.b(BibleMsg::OpenLink(LinkTarget::Verse {
            range: RefRange::parse("Ps 23:2-3").unwrap(),
            module: None,
        })));
        assert_eq!((app.bible().book, app.bible().chapter_no), (Book::PSALMS, 23));
        assert_eq!(app.bible().select_request.map(|(_, a, z)| (a, z)), Some((2, 3)));
        let (first, verse) = app.bible().reveal.unwrap();
        assert_eq!(verse, 2);

        // Asking again for the same verse needs a fresh token, or the widget would ignore it.
        let _ = app.update(app.b(BibleMsg::OpenLink(LinkTarget::Verse {
            range: RefRange::parse("Ps 23:2-3").unwrap(),
            module: None,
        })));
        assert_ne!(app.bible().reveal.unwrap().0, first);

        // A chapter that is not available leaves everything alone and says so.
        let _ = app.update(app.b(BibleMsg::OpenLink(LinkTarget::Verse {
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
        // (As typing does, the text is the note's.)
        let path = app.note().path.clone();
        app.sh.notes.insert(path, text.to_string());
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
    fn typing_a_link_in_the_writer_offers_choices_that_the_keys_take() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("see [[mem");
        assert!(h.app.note().completion.as_ref().is_some_and(|c| c.items[0].label == "Memory verses"), "the choices are there");
        h.named(Named::Enter);
        assert_eq!(h.app.note().doc.source(), "see [[Memory verses]]", "Enter took the first choice");
        assert!(h.app.note().completion.is_none());
        // Escape puts them away without taking one.
        h.type_text(" [[nic");
        assert!(h.app.note().completion.is_some());
        h.named(Named::Escape);
        assert!(h.app.note().completion.is_none());
        assert_eq!(h.app.note().doc.source(), "see [[Memory verses]] [[nic]]");
    }

    #[test]
    fn a_link_closed_by_typing_is_put_right_in_its_capitals_and_the_caret_stays_after_it() {
        let mut h = Harness::new(app_with_writer(""));
        focus_writer(&mut h);
        h.type_text("a [[@john 3:16]]");
        assert_eq!(h.app.note().doc.source(), "a [[@John 3:16]]");
        assert_eq!(h.app.note().doc.selection().head, h.app.note().doc.source().len());
        h.type_text(" b [[%g25]] c [[nicodemus]] d");
        assert_eq!(h.app.note().doc.source(), "a [[@John 3:16]] b [[%G25]] c [[Nicodemus]] d");
        assert_eq!(h.app.sh.notes[&h.app.note().path], "a [[@John 3:16]] b [[%G25]] c [[Nicodemus]] d", "the note is kept in step");
    }

    #[test]
    fn the_choices_are_a_card_that_scrolls_with_the_wheel_and_takes_clicks() {
        let mut app = app_with_writer("");
        for i in 0..30 {
            app.sh.notes.insert(format!("notes/Topic {i:02}.md"), "x\n".into());
        }
        let mut h = Harness::new(app);
        focus_writer(&mut h);
        h.type_text("see [[topic");
        assert_eq!(h.app.note().completion.as_ref().map(|c| c.items.len()), Some(30));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        // Only the first rows are there to see (the rest are scrolled to).
        let first = h.center_of("Topic 00").expect("the first row shows");
        assert!(h.center_of("Topic 29").is_none() || h.bounds_of_text("Topic 29").is_some_and(|b| b.y > first.1), "the last is out of the card's view or below it");
        // The wheel over the card scrolls it, not the page.
        h.move_to(first.0, first.1);
        let page_before = h.app.note().scroll;
        h.feed(Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Lines { x: 0.0, y: -4.0 } }));
        assert!(h.app.note().choices_offset > 0.0, "the card scrolled");
        assert_eq!(h.app.note().scroll, page_before, "and the page did not");
        // Arrow down past the rows shown keeps the choice in view.
        for _ in 0..12 {
            h.named(Named::ArrowDown);
        }
        assert_eq!(h.app.note().completion.as_ref().map(|c| c.selected), Some(12));
        let row = h.app.sh.settings.notes_size * 1.7;
        assert!(h.app.note().choices_offset + 8.0 * row >= 13.0 * row - 0.5, "row 12 is within the eight shown: {}", h.app.note().choices_offset);
        // A click on a row takes it.
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let (x, y) = h.center_of("Topic 12").expect("the chosen row shows");
        h.click(x, y);
        assert_eq!(h.app.note().doc.source(), "see [[Topic 12]]");
    }

    #[test]
    fn a_day_typed_into_the_log_by_hand_is_linked_to_its_daily_note_once_the_log_is_left() {
        let mut app = App::new();
        let now = chrono::Local::now();
        let day = now.format("%Y-%m-%d").to_string();
        let daily = app.sh.templates().into_iter().find(|t| t.name == "Daily").unwrap();
        let made = study_core::templates::plan(&daily, &now, None, |_| false);
        let path = made.path.clone();
        app.sh.notes.insert(made.path, made.text.unwrap());
        let id = app.show_log();
        let end = app.log().doc.source().len();
        let _ = app.update(Message::Log(id, NoteMsg::Action(md_editor::Action::SetCaret { offset: end, extend: false })));
        let _ = app.update(Message::Log(id, NoteMsg::Action(md_editor::Action::Insert(format!("\n## {day}\n\n- one\n")))));
        let plain = format!("## {day}\n");
        assert!(app.sh.notes[crate::shared::READING_LOG].contains(&plain), "not while it is typed in");
        // Another tab comes forward in the log's pane: the log was left.
        let other = app.panes.iter().flat_map(|p| p.tabs.iter().copied()).find(|t| *t != id && app.pane_of(*t) == app.pane_of(id)).unwrap_or_else(|| {
            let blank = app.new_blank();
            let pane = app.pane_of(id).unwrap();
            app.add_tab(pane, blank)
        });
        let _ = app.update(Message::TabPressed(other));
        let linked = format!("## [[{}|{day}]]", path.trim_end_matches(".md"));
        assert!(app.sh.notes[crate::shared::READING_LOG].contains(&linked), "{}", app.sh.notes[crate::shared::READING_LOG]);
        assert!(app.log().doc.source().contains(&linked), "and what the tab shows is the same");
    }

    #[test]
    fn the_reading_log_offers_choices_and_puts_links_right_as_a_note_does() {
        let mut app = App::new();
        let id = app.show_log();
        let mut h = Harness::new(app);
        // The caret goes to the end of the log, where the days are added.
        let editor = { let Some(Tab::Log(t)) = h.app.tabs.get(&id) else { unreachable!() }; t.note.editor_id() };
        let b = h.bounds_of(editor);
        h.click(b.x + 40.0, b.y + 40.0);
        let end = h.app.log().doc.source().len();
        let _ = h.app.update(Message::Log(id, NoteMsg::Action(md_editor::Action::SetCaret { offset: end, extend: false })));
        h.type_text("\n\nSee [[@jo");
        assert!(h.app.log().doc.source().ends_with("See [[@jo]]"), "{}", h.app.log().doc.source());
        let Some(Tab::Log(t)) = h.app.tabs.get(&id) else { unreachable!() };
        assert!(t.note.completion.as_ref().is_some_and(|c| c.items[0].label == "John"), "the log offers the choices too");
        h.named(Named::Enter);
        assert!(h.app.log().doc.source().ends_with("See [[@John ]]"), "{}", h.app.log().doc.source());
        // A link typed out in full is put right.
        h.named(Named::Escape);
        let end = h.app.log().doc.source().len();
        let _ = h.app.update(Message::Log(id, NoteMsg::Action(md_editor::Action::SetCaret { offset: end, extend: false })));
        h.type_text(" and [[%g25]]");
        assert!(h.app.log().doc.source().ends_with("See [[@John ]] and [[%G25]]"), "{}", h.app.log().doc.source());
    }

    #[test]
    fn with_vim_bindings_a_link_typed_in_insert_mode_offers_choices_too() {
        let mut app = app_with_writer("");
        let _ = app.update(Message::SetVimMode(true));
        let mut h = Harness::new(app);
        focus_writer(&mut h);
        h.type_text("i");
        h.type_text("see [[mem");
        assert!(h.app.note().completion.as_ref().is_some_and(|c| c.items[0].label == "Memory verses"), "Insert mode: the choices are there");
        h.named(Named::Enter);
        assert_eq!(h.app.note().doc.source(), "see [[Memory verses]]");
        // Not in Normal mode: nothing is being typed.
        h.named(Named::Escape);
        assert!(h.app.note().completion.is_none());
    }

    #[test]
    fn an_embedded_dictionary_entry_shows_the_word_and_what_it_means() {
        let app = App::new();
        let module = app.bible().module.clone();
        let target = LinkTarget::Sword { module: "Strongs".into(), key: "G2316".into() };
        let shown = app.sh.embed_for(&module, &target).expect("an entry for G2316");
        assert!(shown.title.starts_with("G2316 · "), "{}", shown.title);
        assert!(shown.body.contains("θεός"), "{}", shown.body);
        assert!(app.sh.embed_for(&module, &LinkTarget::Sword { module: "Strongs".into(), key: "G99999".into() }).is_none());
        assert!(study_core::markdown::find_links("![[%G2316]]")[0].embed);
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

        // A verse link shows the passage in the reader on screen and leaves the writer's text and caret alone.
        let mut h = Harness::new(app_with_writer("see [[@Ps 23:1]] now\nlast"));
        let end = h.app.note().doc.selection().head;
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 60.0, 12.0);
        h.click(x, y);
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::PSALMS, 23));
        assert_eq!(h.app.note().doc.selection().head, end, "following does not move the caret first");
        assert_eq!(h.app.note().doc.source(), "see [[@Ps 23:1]] now\nlast");
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
        assert!(h.app.tabs.values().any(|t| matches!(t, Tab::Note(n) if n.path == "notes/Nicodemus.md")), "the note it names is open");
    }

    #[test]
    fn an_embed_line_shows_the_passage_and_pushes_the_text_below_it_down() {
        let mut h = Harness::new(app_with_writer("![[@John 3:16]]\nlast line"));
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
        // The link took over the tab the reader was in (the only one), which is gone back to.
        let shown = h.app.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Note(n) if n.path == "notes/Nicodemus.md").then_some(*id)).expect("the note it names is shown");
        assert!(!h.app.tabs.values().any(|t| matches!(t, Tab::Bible(_))));
        assert_eq!(h.app.note().path, before, "the other note tab is as it was");
        let _ = h.app.update(Message::NavTab(shown, true));
        assert!(h.app.bible().editing.is_none(), "following a link is not editing");
        // Anywhere else in the card (the far end of the first line) starts editing it.
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
        let mut h = Harness::new(app_with_writer("![[@John 3:16]]\nlast"));
        let end = h.app.note().doc.selection().head;
        // The first line of the embed is its title ("John 3:16 · KJV").
        let (x, y) = h.in_editor(h.app.note().editor_id(), 16.0, 40.0, 14.0);
        h.click(x, y);
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::JOHN, 3));
        assert_eq!(h.app.bible().select_request.map(|(_, a, z)| (a, z)), Some((16, 16)), "the passage is shown in the reader");
        assert_eq!(h.app.note().doc.selection().head, end, "the writer's caret did not move");
        assert_eq!(h.app.note().doc.source(), "![[@John 3:16]]\nlast");
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
        // (Over the text only: below it are buttons that do other things.)
        assert!(h.click_scan_to(800, opened).is_none());
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
        assert!(h.center_of("✕").is_none(), "no close button: it goes with Escape or a click elsewhere");
    }

    #[test]
    fn the_reader_and_its_cards_have_one_selection_between_them() {
        let mut h = narrow_reader();
        h.click_scan(opened).expect("a footnote marker in John 3");
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let id = h.app.bid();
        // Words selected in the text, then in the card: the text's are let go of.
        select_verses(&mut h.app, &[2]);
        assert!(h.app.bible().text_selection.is_some());
        let _ = h.app.update(Message::Bible(id, BibleMsg::CardText("popup".into(), md_editor::Action::SelectAll)));
        assert!(h.app.bible().card_selected("popup") && h.app.bible().text_selection.is_none(), "the card's words only");
        // And selected in the text again: the card's are let go of.
        let slice = CanonSlice { verse: 3, start: 0, end: 4 };
        let _ = h.app.update(Message::Bible(id, BibleMsg::SelectionChanged(Some(vec![slice]))));
        assert!(!h.app.bible().card_selected("popup") && h.app.bible().text_selection.is_some(), "the text's words only");
    }

    #[test]
    fn the_text_of_a_reference_card_can_be_selected_but_not_changed() {
        let mut h = narrow_reader();
        h.click_scan(opened).expect("a footnote marker in John 3");
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let id = h.app.bid();
        let source = h.app.bible().card_source("popup").expect("the card's text");
        assert!(!h.app.bible().card_selected("popup"));
        let _ = h.app.update(Message::Bible(id, BibleMsg::CardText("popup".into(), md_editor::Action::SelectAll)));
        assert!(h.app.bible().card_selected("popup"), "everything in the card is selected");
        let _ = h.app.update(Message::Bible(id, BibleMsg::CardText("popup".into(), md_editor::Action::Insert("typed".into()))));
        let _ = h.app.update(Message::Bible(id, BibleMsg::CardText("popup".into(), md_editor::Action::Backspace)));
        assert_eq!(h.app.bible().card_source("popup"), Some(source), "reading, not writing");
        assert!(h.app.bible().card_selected("popup"), "and what was done to change it left the selection alone");
        assert!(h.app.bible().note_card.is_some(), "the card stays while its text is worked in");
    }

    #[test]
    fn dragging_across_the_text_of_a_reference_card_selects_it() {
        let mut h = narrow_reader();
        h.click_scan(opened).expect("a footnote marker in John 3");
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let card = h.bounds_of_text("Footnote").expect("the card");
        let y = card.y + 40.0;
        h.drag((card.x + 12.0, y), (card.x + 120.0, y));
        assert!(h.app.bible().note_card.is_some(), "the card stays");
        assert!(h.app.bible().card_selected("popup"), "the drag selected some of the text");
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
        let shown = h.app.bible().card_source("popup").expect("the card's text");
        assert!(shown.contains(&entry.lemma), "the lemma is shown");
        assert!(shown.contains(&entry.transliteration), "and its transliteration");
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
            h.click(8.0, 1700.0);
            assert!(h.app.bible().note_card.is_none() && h.app.bible().popup.is_none());
        }
    }

    #[test]
    fn a_dictionary_popover_closes_on_a_click_in_the_text() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        app.bible_mut().show_strongs = true;
        app.bible_mut().in_margin.strongs = false;
        let mut h = Harness::new(app);
        h.click_scan(is_word).unwrap();
        assert!(h.center_of("✕").is_none(), "no close button");
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
        let short = laid_width(&h, app.reference_card("Footnote", text("Hi.").into(), false), 380.0);
        assert!(short < 120.0, "{short}");
        let filled = laid_width(&h, app.reference_card("Footnote", text("Hi.").into(), true), 380.0);
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
    fn a_blank_tab_becomes_the_kind_chosen_and_keeps_its_place() {
        let mut app = App::new();
        let pane = app.panes[0].id;
        let _ = app.update(Message::NewTab(pane));
        let blank = *app.panes[0].tabs.last().unwrap();
        // The page offers kinds of tab, not particular ones.
        let mut h = Harness::new(app);
        for label in ["Bible", "Dictionary", "Notes"] {
            assert!(h.center_of(label).is_some(), "{label} is offered");
        }
        assert!(h.center_of("KJV").is_none() && h.center_of("Nicodemus").is_none(), "nothing more specific");
        let mut app = h.app;
        let _ = app.update(Message::Open(blank, Choice::Note));
        assert!(matches!(&app.tabs[&blank], Tab::Note(n) if n.path == app.sh.first_ordinary_note()), "it opens on a note; the tab's selector picks another");
        assert_eq!(app.panes[0].tabs.last(), Some(&blank), "same tab, same place in the bar");
        // Choosing again does nothing: it is no longer blank.
        let path = match &app.tabs[&blank] {
            Tab::Note(n) => n.path.clone(),
            _ => unreachable!(),
        };
        let _ = app.update(Message::Open(blank, Choice::Dict));
        assert!(matches!(&app.tabs[&blank], Tab::Note(n) if n.path == path));
        // A Bible opens where the other is reading.
        let _ = app.update(Message::NewTab(pane));
        let second = *app.panes[0].tabs.last().unwrap();
        let _ = app.update(Message::Bible(app.bid(), BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        let _ = app.update(Message::Open(second, Choice::Bible));
        let b = app.bible_tab(second).unwrap();
        assert_eq!((b.book, b.chapter_no), (Book::PSALMS, 23));
        assert_eq!(b.module, app.bible().module, "in the module it is reading");
        // A dictionary opens on the first one there is, with the tab's own picker for the rest.
        let _ = app.update(Message::NewTab(pane));
        let third = *app.panes[0].tabs.last().unwrap();
        let _ = app.update(Message::Open(third, Choice::Dict));
        assert!(matches!(&app.tabs[&third], Tab::Dict(d) if d.module == app.sh.backend.dictionaries()[0].id));
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
    fn reading_mode_shows_the_text_alone_and_ends_when_another_tab_opens() {
        let mut h = Harness::new(App::new());
        let bible = h.app.bible().id;
        let _ = h.app.update(Message::Bible(bible, BibleMsg::ReadingMode));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.app.reader_mode.is_some());
        assert!(h.center_of(NOTE_PANE_MARK).is_none(), "the other pane is out of the way");
        assert!(h.center_of("Settings").is_none(), "and so is the top bar");
        let _ = h.app.update(Message::ToggleReadingMargins);
        assert!(h.app.sh.settings.reader_margins);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.app.reader_mode.is_some(), "the margins are part of reading mode");
        let _ = h.app.update(Message::EndReading);
        assert!(h.app.reader_mode.is_none());
        // Opening a tab, anywhere, ends it too.
        let _ = h.app.update(Message::Bible(bible, BibleMsg::ReadingMode));
        let _ = h.app.update(Message::Bible(bible, BibleMsg::OpenSearch));
        assert!(h.app.reader_mode.is_none(), "the search tab is shown, so the text is not alone");
    }

    #[test]
    fn the_menu_bars_commands_do_what_the_keys_did_and_the_menu_is_told_what_is_possible() {
        use commands::Command as C;
        let mut app = App::new();
        let pane = app.pane_index(app.focus).unwrap();
        let tabs = app.panes[pane].tabs.len();
        let state = app.menu_state();
        assert!(state.can_read || !app.panes[pane].active.is_some_and(|a| matches!(app.tabs.get(&a), Some(Tab::Bible(_)))));
        assert!(!state.tabs, "one tab: nothing to step to");
        let _ = app.update(Message::Command(C::NewTab));
        assert_eq!(app.panes[pane].tabs.len(), tabs + 1);
        assert!(app.menu_state().tabs, "now there is");
        let before = app.panes[pane].active;
        let _ = app.update(Message::Command(C::PreviousTab));
        assert_ne!(app.panes[pane].active, before);
        let _ = app.update(Message::Command(C::CloseTab));
        assert_eq!(app.panes[pane].tabs.len(), tabs);
        let panes = app.panes.len();
        let _ = app.update(Message::Command(C::NewPane));
        assert_eq!(app.panes.len(), panes + 1);
        // Reader mode, from the menu, for the Bible tab in front.
        let bible = app.bid();
        app.activate(bible);
        let _ = app.update(Message::Command(C::ReaderMode));
        assert!(app.reader_mode.is_some() && app.menu_state().reading);
        let _ = app.update(Message::Command(C::ReaderMode));
        assert!(app.reader_mode.is_none());
    }

    #[test]
    fn the_menu_toggles_the_readers_settings_and_the_margin_ones_wait_for_their_kind() {
        use commands::Command as C;
        let mut app = App::new();
        let bible = app.bid();
        app.activate(bible);
        let _ = app.update(Message::Bible(bible, BibleMsg::ToggleStrongs(false)));
        let _ = app.update(Message::Bible(bible, BibleMsg::ToggleMarks(true)));
        let _ = app.update(Message::Bible(bible, BibleMsg::ToggleFootnotes(false)));
        let _ = app.update(Message::Bible(bible, BibleMsg::ToggleRefs(false)));
        let state = app.menu_state();
        assert!(!state.enabled(C::StrongsInMargin) && state.enabled(C::MarksInMargin), "only for what is shown");
        let before = (app.bible().in_margin.strongs, app.bible().in_margin.marks);
        let _ = app.update(Message::Command(C::StrongsInMargin));
        assert_eq!(app.bible().in_margin.strongs, before.0, "Strong's are hidden: nothing happens");
        let _ = app.update(Message::Command(C::MarksInMargin));
        assert_eq!(app.bible().in_margin.marks, !before.1);
        let _ = app.update(Message::Command(C::MarksInMargin));
        assert_eq!(app.bible().in_margin.marks, before.1);
        for command in [C::ToggleStrongs, C::ToggleMarks, C::ToggleFootnotes, C::ToggleRefs] {
            let was = (app.bible().show_strongs, app.bible().show_marks, app.bible().show_footnotes, app.bible().show_refs);
            let _ = app.update(Message::Command(command));
            let now = (app.bible().show_strongs, app.bible().show_marks, app.bible().show_footnotes, app.bible().show_refs);
            let flipped = match command {
                C::ToggleStrongs => (!was.0, was.1, was.2, was.3),
                C::ToggleMarks => (was.0, !was.1, was.2, was.3),
                C::ToggleFootnotes => (was.0, was.1, !was.2, was.3),
                _ => (was.0, was.1, was.2, !was.3),
            };
            assert_eq!(now, flipped, "{command:?}");
        }
        assert!(app.bible().show_strongs && !app.bible().show_marks, "and the menu follows");
        assert!(app.menu_state().checked(C::ToggleStrongs) && app.menu_state().enabled(C::StrongsInMargin));
        let _ = app.update(Message::Command(C::NewTab));
        assert!(!app.menu_state().enabled(C::ToggleMarks), "not a Bible tab in front");
    }

    #[test]
    fn a_layout_does_not_keep_what_the_readers_read_and_open_readers_carry_into_it() {
        let mut app = App::new();
        let first = app.bid();
        let _ = app.update(Message::Bible(first, BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        app.add_bible(0);
        let second = *app.panes[0].tabs.last().unwrap();
        let _ = app.update(Message::Bible(second, BibleMsg::PickChapter(ChapterChoice(Book::NUMBERS, 21))));
        app.layout_draft = "Two readers".into();
        let _ = app.update(Message::SaveLayout);
        let saved = &app.sh.layouts.iter().find(|l| l.name == "Two readers").unwrap().workspace;
        let readers: Vec<&state::TabSave> = saved.panes.iter().flat_map(|p| p.tabs.iter()).filter(|t| matches!(t, state::TabSave::Bible { .. })).collect();
        assert_eq!(readers.len(), 2);
        assert!(readers.iter().all(|t| matches!(t, state::TabSave::Bible { module, book, chapter: 0, .. } if module.is_empty() && book.is_empty())), "no passage is stored");
        // Open readers elsewhere (one, reading Psalm 23) go into it: the first reader gets that, the other the default passage.
        let _ = app.update(Message::LoadLayout(DEFAULT_LAYOUT.into()));
        let only = app.bid();
        let _ = app.update(Message::Bible(only, BibleMsg::PickChapter(ChapterChoice(Book::PSALMS, 23))));
        let _ = app.update(Message::LoadLayout("Two readers".into()));
        let bibles: Vec<(Book, u16)> = app.workspace().panes.iter().flat_map(|p| p.tabs.iter()).filter_map(|t| match t {
            state::TabSave::Bible { book, chapter, .. } => Some((Book::from_osis(book).unwrap(), *chapter)),
            _ => None,
        }).collect();
        assert_eq!(bibles.len(), 2);
        assert!(bibles.contains(&(Book::PSALMS, 23)), "the reader that was open keeps its place: {bibles:?}");
        assert!(bibles.contains(&(Book::JOHN, 3)), "the other opens at the default passage: {bibles:?}");
    }

    #[test]
    fn the_keys_close_the_tab_in_front_and_step_through_the_panes_tabs() {
        let mut app = App::new();
        let main = app.main;
        let pane = app.pane_index(app.focus).unwrap();
        let blank = app.new_blank();
        let first = app.panes[pane].active.unwrap();
        let second = app.add_tab(pane, blank);
        assert_eq!(app.panes[pane].active, Some(second));
        let _ = app.update(Message::CycleTab(main, true));
        assert_eq!(app.panes[pane].active, Some(app.panes[pane].tabs[0]), "after the last comes the first");
        let _ = app.update(Message::CycleTab(main, false));
        assert_eq!(app.panes[pane].active, Some(second), "and before the first, the last");
        let _ = app.update(Message::CycleTab(iced::window::Id::unique(), true));
        assert_eq!(app.panes[pane].active, Some(second), "another window's keys are not ours");
        let _ = app.update(Message::CloseFocusedTab(main));
        assert!(!app.tabs.contains_key(&second));
        assert!(app.tabs.contains_key(&first));
    }

    #[test]
    fn new_tab_and_new_pane_keys_add_to_the_pane_last_used_splitting_along_its_longer_side() {
        let mut app = App::new();
        let main = app.main;
        let panes_before = app.panes.len();
        let tabs_before = app.tabs.len();
        let _ = app.update(Message::NewTabHere(iced::window::Id::unique()));
        assert_eq!(app.tabs.len(), tabs_before, "another window's keys are not ours");
        let _ = app.update(Message::NewTabHere(main));
        assert_eq!(app.tabs.len(), tabs_before + 1);
        // The two panes are each 512 wide: in a 1024 by 1000 window they are tall, so the split is top and bottom.
        let focus = app.focus;
        app.window_size = Size::new(1024.0, 1000.0);
        let _ = app.update(Message::NewPaneHere(main));
        assert_eq!(app.panes.len(), panes_before + 1);
        assert_ne!(app.focus, focus, "the new pane is the one used");
        fn axis_over(node: &panes::Node, pane: u32) -> Option<panes::Axis> {
            match node {
                panes::Node::Split { axis, a, b, .. } if matches!(**a, panes::Node::Leaf(p) if p == pane) || matches!(**b, panes::Node::Leaf(p) if p == pane) => Some(*axis),
                panes::Node::Split { a, b, .. } => axis_over(a, pane).or_else(|| axis_over(b, pane)),
                _ => None,
            }
        }
        assert_eq!(axis_over(&app.layout, focus), Some(panes::Axis::Column));
        let wide = {
            let mut app = App::new();
            let focus = app.focus;
            app.window_size = Size::new(2400.0, 700.0);
            let main = app.main;
            let _ = app.update(Message::NewPaneHere(main));
            axis_over(&app.layout, focus)
        };
        assert_eq!(wide, Some(panes::Axis::Row), "a wide pane is split side by side");
    }

    #[test]
    fn closing_a_pane_moves_the_focus_to_the_pane_that_takes_its_space() {
        let mut app = App::new();
        let main = app.main;
        // Three panes: the two there are, then another split off the first (so the one made last sits in the middle).
        let first = app.panes[0].id;
        let _ = app.update(Message::SplitPane(first, panes::Axis::Row));
        let middle = app.focus;
        let leaves = app.layout.leaves();
        assert_eq!(leaves.iter().position(|&p| p == middle), Some(1), "split off the first, so second from the left");
        let _ = app.update(Message::CloseFocusedTab(main));
        assert_eq!(app.focus, first, "the pane it was split off takes its space");
    }

    #[test]
    fn closing_right_after_a_new_pane_closes_the_new_panes_tab_not_the_old_ones() {
        let mut app = App::new();
        let main = app.main;
        let old = app.focus;
        let _ = app.update(Message::NewPaneHere(main));
        let _ = app.update(Message::CloseFocusedTab(main));
        assert!(app.pane_index(old).is_some(), "the old pane is still there");
        assert_eq!(app.focus, old, "and it is the one left in use");
    }

    #[test]
    fn the_new_pane_keeps_the_focus_once_everything_has_been_laid_out() {
        let mut h = Harness::new(App::new());
        let old = h.app.focus;
        h.app.window_size = iced::Size::new(1024.0, 768.0);
        let main = h.app.main;
        let _ = h.app.update(Message::NewPaneHere(main));
        let new = h.app.focus;
        assert_ne!(new, old);
        for _ in 0..3 {
            h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        }
        assert_eq!(h.app.focus, new, "what the old pane reports of its new size does not take the focus back");
    }

    #[test]
    fn dragging_a_divider_resizes_the_panes_either_side() {
        let mut h = Harness::new(App::new());
        let before = match &h.app.layout {
            panes::Node::Split { fraction, .. } => *fraction,
            _ => unreachable!(),
        };
        assert_eq!(before, 0.55);
        // The divider is a little right of the middle of the 1024 wide window, below the top bar.
        h.move_to(564.0, 400.0);
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
    fn a_verse_link_claims_the_reader_on_screen_else_takes_over_its_tab_and_back_returns_to_it() {
        let mut app = App::new();
        let (bible, note) = (app.bid(), app.nid());
        let numbers = LinkTarget::Verse { range: RefRange::parse("Num 21:9").unwrap(), module: None };
        // A reader is on screen: it is claimed, and the note's tab is left as it is.
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(numbers.clone())));
        assert_eq!(bible_of(&app, bible).book, Book::NUMBERS);
        assert!(matches!(app.tabs.get(&note), Some(Tab::Note(_))));
        // The note's pane fills the window, so the reader is not on screen: the note's tab becomes a Bible tab.
        let note_pane = app.pane_of(note).unwrap();
        app.zoomed = Some(app.panes[note_pane].id);
        let psalm = LinkTarget::Verse { range: RefRange::parse("Ps 23:1").unwrap(), module: None };
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(psalm)));
        let shown = app.last_bible.unwrap();
        assert_ne!(shown, bible);
        assert_eq!(bible_of(&app, shown).book, Book::PSALMS);
        assert_eq!(app.pane_of(shown), Some(note_pane));
        assert!(!app.tabs.contains_key(&note));
        assert_eq!(bible_of(&app, bible).book, Book::NUMBERS, "the hidden reader is as it was");
        // Back is the note, as it was; forward is the Bible tab again.
        let _ = app.update(Message::NavTab(shown, true));
        assert!(matches!(app.tabs.get(&note), Some(Tab::Note(_))) && !app.tabs.contains_key(&shown));
        let _ = app.update(Message::NavTab(note, false));
        assert_eq!(bible_of(&app, shown).book, Book::PSALMS);
        let _ = app.update(Message::NavTab(shown, true));
        // A tab that shows the chapter already comes forward instead, hidden or not: the note stays.
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(numbers)));
        assert!(matches!(app.tabs.get(&note), Some(Tab::Note(_))));
        assert_eq!(bible_tabs(&app), [bible], "no new tab");
        assert!(app.zoomed.is_none_or(|z| Some(z) == app.pane_of(bible).map(|p| app.panes[p].id)), "the reader showing it is brought forward");
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
                let _ = app.update(Message::Note(id, NoteMsg::ToggleFront));
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
            app.sh.toast("Copied [[@John 3:2-5]]");
        }
        if std::env::var("SNAP_PLAIN").is_ok() {
            app.sh.set_mark_look(0, MarkStyle::None, "");
        }
        if std::env::var("SNAP_MARGIN").is_ok() {
            let p = app.sh.marks[0].path();
            app.open_note(p);
        }
        if std::env::var("SNAP_LOG").is_ok() {
            let log = format!(
                "{}\n\n## 2026-10-13\n\n- [[@Joshua 2]]\n- [[@Joshua 3]]\n\n## 2026-10-14\n\n- [[@John 3]]\n",
                study_core::reading_log::new_log("Reading log", "2026-10-13T18:27:42-05:00").trim_end()
            );
            app.sh.notes.insert(crate::shared::READING_LOG.into(), log);
            app.open_note(crate::shared::READING_LOG.into());
        }
        if std::env::var("SNAP_SECTIONS").is_ok() {
            app.sh.notes.insert("notes/Plain.md".into(), "Thinking of [[@John 3:16]], the heart of it.\n".into());
            app.sh.notes.insert("notes/Pray.md".into(), "---\ntype: prayer\n---\nLord, [[@John 3:16]]\n".into());
            app.sh.notes.insert("notes/Preach.md".into(), "---\ntype: sermon\n---\n[[@John 3:16]] again\n".into());
            for plan in study_core::starter_plans::starter_plans() {
                for (path, text) in plan.to_files() {
                    app.sh.notes.insert(path, text);
                }
            }
            let from = app.nid();
            let _ = app.show_backlinks(from, backlinks_tab::Target::Passage(scripture::RefRange::parse("John 3").unwrap()));
            // SNAP_PLANS: the plans open, the rest folded.
            let tab = app.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Backlinks(_)).then_some(*id));
            if std::env::var("SNAP_PLANS").is_ok()
                && let Some(Tab::Backlinks(t)) = tab.and_then(|id| app.tabs.get_mut(&id))
            {
                t.folded = ["Margin notes", "Notes", "Prayer notes", "Sermon notes"].map(String::from).into_iter().collect();
            }
        }
        if std::env::var("SNAP_VIM").is_ok() {
            app.sh.settings.vim_mode = true;
        }
        if std::env::var("SNAP_PICKING").is_ok() {
            app.note_mut().picking = true;
        }
        if std::env::var("SNAP_NOTEMENU").is_ok() {
            let id = app.nid();
            let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        }
        if std::env::var("SNAP_PICKER").is_ok() {
            app.note_mut().picker = Some(crate::note_tab::Picker::default());
            if std::env::var("SNAP_TAGS").is_ok() {
                app.note_mut().picker.as_mut().unwrap().tags.extend(["prayer".to_string(), "john".to_string()]);
            }
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
        if let Ok(theme) = std::env::var("SNAP_THEME") {
            app.sh.settings.theme = theme;
        }
        let mut h = Harness::new(app);
        h.size = Size::new(1024.0, 560.0);
        if let Ok(w) = std::env::var("SNAP_W") {
            h.size = Size::new(w.parse().unwrap(), 800.0);
        }
        if std::env::var("SNAP_STYLES").is_ok() {
            if std::env::var("SNAP_STRONGS").is_ok() {
                h.app.bible_mut().show_strongs = true;
            }
            // Two words of verse 1 in each way a pen marks, side by side with the plain text.
            let canon = h.app.bible().chapter.as_ref().unwrap().canonical_verses();
            let text = canon[&1].clone();
            let mut at = 0;
            for style in crate::state::PenStyle::ALL {
                h.app.sh.settings.pens[0].style = style;
                if let Ok(color) = std::env::var("SNAP_COLOR") {
                    h.app.sh.settings.pens[0].color = color;
                }
                let mut end = at;
                for _ in 0..2 {
                    end += text[end..].find(' ').map_or(text.len() - end, |f| f + 1);
                }
                let end_word = text[at..end].trim_end().len() + at;
                h.app.bible_mut().text_selection = Some(vec![CanonSlice { verse: 1, start: at, end: end_word }]);
                let _ = h.app.update(h.app.b(BibleMsg::ApplyPen(0)));
                at = end;
            }
            h.app.bible_mut().popup = None;
            h.app.bible_mut().text_selection = None;
            for _ in 0..4 {
                h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
            }
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
        if std::env::var("SNAP_HIDESHOW").is_ok() {
            let reader = h.app.bid();
            let range = scripture::RefRange { book: Book::JOHN, chapter: 3, verses: Some((2, 3)) };
            let _ = h.app.show_verse_in(reader, range);
            let frames = |h: &mut Harness| {
                for _ in 0..3 {
                    h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
                }
            };
            frames(&mut h);
            let blank = h.app.new_blank();
            let pane = h.app.pane_of(reader).unwrap();
            h.app.add_tab(pane, blank);
            let _ = h.app.update(Message::SaveSession);
            frames(&mut h);
            let _ = h.app.update(Message::TabPressed(reader));
            frames(&mut h);
        }
        if let Ok(which) = std::env::var("SNAP_PLANS") {
            let today = chrono::Local::now().date_naive();
            let start = (today - chrono::Duration::days(2)).format("%Y-%m-%d").to_string();
            let gospels = format!("---\ntype: reading-plan\nname: The Gospels in 30 days\nstart: {start}\n---\n\n## Day 1\n\n- Matthew 1-2\n\n## Day 2\n\n- Matthew 3-4\n\n## Day 3 — Sermon on the Mount\n\n- Matthew 5\n\n## Day 4\n\n- Matthew 6\n");
            h.app.sh.notes.insert("library/Plans/Gospels.md".into(), gospels);
            h.app.sh.notes.insert("library/Plans/Psalms.md".into(), "---\ntype: reading-plan\nname: Psalms in a month\n---\n\n## Day 1\n\n- Psalm 1-5\n".into());
            let m = Book::from_osis("Matt").unwrap();
            let _ = h.app.sh.log_reading(m, 1, &(chrono::Local::now() - chrono::Duration::days(1)));
            let _ = h.app.sh.log_reading(m, 2, &(chrono::Local::now() - chrono::Duration::days(1)));
            let _ = h.app.sh.log_reading(m, 3, &chrono::Local::now());
            if which == "window" {
                h.app.plans_window = Some(iced::window::Id::unique());
            }
            h.size = if which == "window" { Size::new(620.0, 660.0) } else { Size::new(900.0, 640.0) };
            let first = match which.as_str() {
                "detail" => Some(("library/Plans/Gospels.md".to_string(), None)),
                "day" => {
                    let plan = study_core::plan::Plan::parse(h.app.sh.notes.get("library/Plans/Gospels.md").map_or("", String::as_str));
                    h.app.sh.notes.remove("library/Plans/Gospels.md");
                    let files = plan.to_files();
                    let manifest = files[0].0.clone();
                    for (path, text) in files {
                        h.app.sh.notes.insert(path, text);
                    }
                    Some((manifest, Some(0)))
                }
                _ => None,
            };
            let _ = h.app.show_plans(first);
        }
        if let Ok(words) = std::env::var("SNAP_SEARCH") {
            let home = tempfile::tempdir().unwrap();
            h.app = live_app(home.path()).expect("real modules");
            let _ = h.app.show_search(Some(words));
            let id = h.app.tabs.iter().find(|(_, t)| matches!(t, Tab::Search(_))).map(|(i, _)| *i).unwrap();
            let _ = h.app.update(Message::Search(id, SearchMsg::Run));
            if std::env::var("SNAP_OPEN").is_ok() {
                let found = match h.app.tabs.get(&id) {
                    Some(Tab::Search(t)) => t.ran.as_ref().and_then(|r| r.results.found.iter().find(|f| f.range.to_string() == "John 3:16").cloned()),
                    _ => None,
                };
                if let Some(found) = found {
                    let module = h.app.bible().module.clone();
                    let _ = h.app.update(Message::Search(id, SearchMsg::Open(found.range, module, found.spans)));
                    for _ in 0..3 {
                        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
                    }
                }
            }
        }
        if let Ok(which) = std::env::var("SNAP_COMMENTARY") {
            // Needs the real modules.
            let home = tempfile::tempdir().unwrap();
            h.app = live_app(home.path()).expect("real modules");
            let from = h.app.bid();
            let _ = h.app.show_commentary(from, Book::JOHN, 3, Some(16));
            if let Some(Tab::Commentary(t)) = h.app.tabs.values_mut().find(|t| matches!(t, Tab::Commentary(_))) {
                if which != "1" {
                    t.module = ModuleId(which);
                }
                t.verse = None;
            }
        }
        if let Ok(kind) = std::env::var("SNAP_VAULT") {
            use crate::vault_setup::{Mode, Setup};
            let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Documents");
            match kind.as_str() {
                "welcome" => h.app.onboarding = Some(live::Onboarding::First),
                "missing" => h.app.onboarding = Some(live::Onboarding::Missing(dir.join("Study vault"))),
                "create" => h.app.setup = Some(Setup::new(Mode::Create, dir.clone())),
                _ => h.app.setup = Some(Setup::new(Mode::Open, dir.clone())),
            }
            h.app.home = Some(dir.join("data"));
        }
        if let Ok(at) = std::env::var("SNAP_HOVER") {
            let (x, y) = at.split_once(',').unwrap();
            h.move_to(x.parse().unwrap(), y.parse().unwrap());
        }
        if std::env::var("SNAP_BACKLINKS").is_ok() {
            let id = h.app.bid();
            let from = h.app.sh.marks.len();
            let _ = from;
            if let Some(Tab::Bible(t)) = h.app.tabs.get_mut(&id) {
                t.text_selection = Some(vec![CanonSlice { verse: 3, start: 0, end: 4 }]);
            }
            let _ = h.app.update(Message::Bible(id, BibleMsg::OpenBacklinks));
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
                h.app.home = Some(std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/zoe"));
                let _ = h.app.update(Message::OpenSettings);
                h.size = Size::new(660.0, 600.0);
                if let Ok(tab) = std::env::var("SNAP_TAB") {
                    h.app.settings_tab = match tab.as_str() {
                        "pens" => SettingsTab::Pens,
                        "vault" => SettingsTab::Vault,
                        "modules" => SettingsTab::Modules,
                        _ => SettingsTab::Appearance,
                    };
                }
            }
        }
        if std::env::var("SNAP_SELECT").is_ok() {
            h.click_scan(|a| a.bible().text_selection.is_some());
        }
        if std::env::var("SNAP_OPEN").is_ok() || std::env::var("SNAP_COLORS").is_ok() {
            h.app.note_mut().front_open = true;
        }
        if std::env::var("SNAP_COLORS").is_ok() {
            h.app.note_mut().mark_colors = true;
        }
        let theme = h.app.theme();
        let shown = match h.app.settings_window.or(h.app.plans_window) {
            Some(id) => h.app.window_view(id),
            None => h.app.view(),
        };
        let mut ui = UserInterface::build(shown, h.size, std::mem::take(&mut h.cache), &mut h.renderer);
        // (An update first, so that what floats over the window, such as a panel, is there to be drawn.)
        let mut sink = Vec::new();
        ui.update(&[Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now()))], h.cursor, &mut h.renderer, &mut h.clipboard, &mut sink);
        ui.draw(
            &mut h.renderer,
            &theme,
            &iced::advanced::renderer::Style { text_color: theme.extended_palette().background.base.text },
            h.cursor,
        );
        h.cache = ui.into_cache();
        let (w, hh) = (h.size.width as u32, h.size.height as u32);
        let rgba = h.renderer.screenshot(Size::new(w, hh), 1.0, theme.extended_palette().background.base.color);
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
        app.note_mut().front_open = true;
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
        assert_eq!(front.title, None, "no title property: the file name and the heading are the title");
        assert!(text_has_heading(&app.sh.notes["notes/Untitled.md"], "Untitled"));
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
    fn type_is_set_with_its_own_control_and_undo() {
        let (mut app, id) = props_app();
        let _ = app.update(Message::Note(id, NoteMsg::SetKind(KindChoice(Some("sermon".into())))));
        let front = front_of(&app);
        assert_eq!(front.kind.as_deref(), Some("sermon"));
        assert_eq!(front.props.len(), 6, "the other properties are untouched");
        assert!(app.note().doc.source().ends_with("Body text\n"));
        let _ = app.update(Message::Note(id, NoteMsg::SetKind(KindChoice(None))));
        assert!(front_of(&app).kind.is_none(), "None takes it away");
        app.note_mut().doc.perform(EditAction::Undo);
        assert_eq!(front_of(&app).kind.as_deref(), Some("sermon"));
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
    fn the_logo_in_the_top_bar_opens_the_settings_at_the_about_page() {
        let mut h = Harness::new(App::new());
        assert!(h.app.settings_window.is_none());
        // The logo is at the left of the bar.
        h.click(24.0, 20.0);
        assert!(h.app.settings_window.is_some(), "the settings are open");
        assert_eq!(h.app.settings_tab, SettingsTab::About);
        // With the settings open at another page, it is brought to the About page, and the window is the same one.
        let window = h.app.settings_window;
        let _ = h.app.update(Message::SettingsTab(SettingsTab::Pens));
        let _ = h.app.update(Message::OpenAbout);
        assert_eq!((h.app.settings_window, h.app.settings_tab), (window, SettingsTab::About));
    }

    #[test]
    fn the_boxes_to_add_a_property_are_behind_a_plus_button_and_go_when_one_is_added() {
        let mut app = App::new();
        app.sh.notes.insert("notes/Few.md".into(), "---\ncreated: 2026-01-01T10:00:00\n---\nText\n".into());
        app.open_note("notes/Few.md".into());
        app.note_mut().front_open = true;
        let id = app.nid();
        let mut h = Harness::new(app);
        assert!(h.center_of("New property").is_none() && h.center_of("+  Property").is_some(), "only the plus button");
        let (x, y) = h.center_of("+  Property").unwrap();
        h.click(x, y);
        assert!(h.app.note().adding && h.center_of("New property").is_some(), "its boxes");
        let _ = h.app.update(Message::Note(id, NoteMsg::NewKey("mood".into())));
        let _ = h.app.update(Message::Note(id, NoteMsg::AddProp));
        assert!(!h.app.note().adding && h.app.sh.notes["notes/Few.md"].contains("mood:"), "added, and put away");
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("New property").is_none());
        // Put away without adding one: the cross.
        let _ = h.app.update(Message::Note(id, NoteMsg::ToggleAddProp));
        let _ = h.app.update(Message::Note(id, NoteMsg::NewKey("half".into())));
        let _ = h.app.update(Message::Note(id, NoteMsg::ToggleAddProp));
        assert!(!h.app.note().adding && h.app.note().new_key.is_empty());
    }

    #[test]
    fn the_properties_editor_is_in_the_frontmatter_section_and_shows_every_property() {
        let (mut app, id) = props_app();
        let mut h = Harness::new({
            app.open_note("notes/Props.md".into());
            app
        });
        assert!(h.center_of("Type").is_none(), "folded to begin with");
        let _ = h.app.update(Message::Note(id, NoteMsg::ToggleFront));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Type").is_some(), "it is in the section, with no button of its own");
        assert!(h.center_of("Type").is_some() && h.center_of("Title").is_none(), "no Title: the name and the heading are it");
        assert!(h.center_of("chapters \u{00B7} number").is_some());
        assert!(h.center_of("with \u{00B7} list").is_some());
        assert!(h.center_of("due \u{00B7} date").is_some());
        assert!(h.center_of("mood \u{00B7} text").is_some());
        // (The rows below these scroll inside the editor; a block it does not model is among them.)
        assert!(matches!(value_of(&h.app, "authors"), study_core::frontmatter::Value::Raw(_)), "kept as written, not editable");
    }

    #[test]
    fn a_margin_note_shows_its_marks_details_and_edits_them_in_place_of_properties() {
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        app.note_mut().front_open = true;
        let mut h = Harness::new(app);
        assert!(h.center_of("Add tag").is_some(), "its tags are edited here, through the mark");
        // (How it marks the text is a drop-down, whose words the harness cannot find: `NoteMsg::MarkStyle` is its choice.)
        assert!(h.center_of("Underline").is_none(), "the choices are listed only while it is open");
        assert!(h.center_of("Note only").is_none(), "that style is not offered");
        assert!(h.center_of("Properties").is_none(), "not the free-form property editor: the file's lines are the app's");
        assert!(h.center_of("#promise").is_some(), "its tag, to remove");
        let mark = &h.app.sh.marks[0];
        let anchor = mark.anchor.to_string();
        // The passage is shown above the properties, its reference a link: it shows that verse in the
        // reader, as a link in a note does.
        let title = format!("{anchor} \u{b7} {}", h.app.module_abbreviation());
        let (x, y) = h.center_of(&title).expect("the verse is shown");
        h.app.bible_mut().chapter_no = 1;
        h.click(x, y);
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::JOHN, 3), "the reader went to {anchor}");
        assert_eq!(h.app.bible().select_request.map(|(_, a, z)| (a, z)), h.app.sh.marks[0].anchor.verses);
    }

    #[test]
    fn the_color_panel_floats_over_the_note_and_is_let_go_of_by_a_click_elsewhere_or_by_choosing() {
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        app.note_mut().front_open = true;
        app.note_mut().mark_colors = true;
        let mut h = Harness::new(app);
        // A click outside the panel closes it, and goes no further (it does not also start a tag).
        let (x, y) = h.center_of("Add tag").expect("the tag box");
        h.click(x, y);
        assert!(!h.app.note().mark_colors, "a click elsewhere let go of the panel");
        assert!(h.app.note().tag_draft.is_empty());
    }

    #[test]
    fn the_tags_are_above_the_fold_and_the_frontmatter_under_them_is_folded_to_a_line_that_says_what_is_in_it() {
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        assert!(!app.note().front_open, "folded to begin with");
        let anchor = app.sh.marks[0].anchor.to_string();
        let mut h = Harness::new(app);
        assert!(h.center_of("Add tag").is_some() && h.center_of("#promise").is_some(), "the tags are not in the fold");
        assert!(h.center_of("Reselect").is_none() && h.center_of("Unlink").is_none(), "the passage section is");
        let (x, y) = h.center_of(&anchor).expect("the passage, in a line");
        h.click(x, y);
        assert!(h.app.note().front_open);
        assert!(h.center_of("Reselect").is_some() && h.center_of("Frontmatter").is_some() && h.center_of("Add tag").is_some());
        assert!(h.center_of(&format!("{anchor}  ")).is_none());
        let (x, y) = h.center_of("Frontmatter").unwrap();
        h.click(x, y);
        assert!(!h.app.note().front_open && h.center_of("Reselect").is_none());
        // Waiting for a selection is said on the line, with the way out, as the button that asked is hidden.
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::PickText));
        assert!(h.center_of("Select text in a Bible\u{2026}").is_some());
        let (x, y) = h.center_of("Cancel").expect("a way out");
        h.click(x, y);
        assert!(!h.app.note().picking);
        // An ordinary note's line says when it was made and how many properties it has; with neither, only what it is.
        let mut app = App::new();
        app.sh.notes.insert("notes/Props.md".into(), "---\ncreated: 2026-01-01T10:00:00\ntags: [a]\nmood: calm\nread: true\n---\nText\n".into());
        app.sh.notes.insert("notes/Bare.md".into(), "Words.\n".into());
        app.open_note("notes/Props.md".into());
        let mut h = Harness::new(app);
        let line = format!("Created {}  \u{00B7}  2 properties", study_core::frontmatter::display("2026-01-01T10:00:00"));
        assert!(h.center_of(&line).is_some(), "{line}");
        assert!(h.center_of("Add tag").is_some(), "its tags show");
        let id = h.app.nid();
        let _ = h.app.update(Message::Note(id, NoteMsg::PickNote(NoteChoice("notes/Bare.md".into()))));
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Frontmatter").is_some());
    }

    fn log_text() -> String {
        format!(
            "{}\n\n## 2026-10-13\n\n- [[@Joshua 2]]\n\n## 2026-10-14\n\n- [[@John 3]]\n- [[@John 4]]\n",
            study_core::reading_log::new_log("Reading log", "2026-10-13T18:27:42-05:00").trim_end()
        )
    }

    #[test]
    fn the_reading_log_has_a_tab_of_its_own_that_grows_down_and_is_written_in_but_for_its_title_name_and_properties() {
        use crate::shared::READING_LOG;
        let mut app = App::new();
        // (The log is shown in the other pane than the one last used: the notes' pane is where it goes.)
        app.focus = app.panes[0].id;
        let log = log_text();
        app.sh.notes.insert(READING_LOG.into(), log.clone());
        let id = app.show_log();
        assert!(matches!(app.tabs.get(&id), Some(Tab::Log(_))) && app.log().is_log());
        // The text starts under the title and the blank line below it, the earliest day first and the latest last.
        let top = app.log().doc.front_end();
        assert!(app.log().doc.source()[..top].ends_with("# Reading log\n\n"));
        assert!(app.log().doc.source()[top..].starts_with("## 2026-10-13\n\n- [[@Joshua 2]]\n\n## 2026-10-14\n\n- [[@John 3]]\n- [[@John 4]]\n"));
        assert_eq!(app.log().doc.selection().head, log.len(), "the caret is at the end, where the days are added");
        // No properties, no way to choose a note, no header: the words and the bar under them.
        let mut h = Harness::new(app);
        for gone in ["No tags", "Frontmatter", "Properties", "Add tag", "Mark", "Create note"] {
            assert!(h.center_of(gone).is_none(), "{gone}");
        }
        for message in [
            NoteMsg::TagDraft("x".into()),
            NoteMsg::AddTag,
            NoteMsg::RemoveTag("reading-log".into()),
            NoteMsg::AddProperties,
            NoteMsg::SetKind(KindChoice(Some("prayer".into()))),
            NoteMsg::PickText,
            NoteMsg::StartRename,
            NoteMsg::TogglePicker,
            NoteMsg::ToggleMenu,
            NoteMsg::PickNote(NoteChoice("notes/Memory verses.md".into())),
        ] {
            let _ = h.app.update(Message::Log(id, message));
        }
        assert_eq!(h.app.sh.notes[READING_LOG], log, "nothing of it changed");
        assert!(h.app.log().picker.is_none() && h.app.log().renaming.is_none() && !h.app.log().menu_open && h.app.log().path == READING_LOG);
        // Writing at the end is as in any note; the caret cannot go into the title, and backspace stops at it.
        let _ = h.app.update(Message::Log(id, NoteMsg::Action(EditAction::Insert("A thought.\n".into()))));
        assert!(h.app.sh.notes[READING_LOG].ends_with("- [[@John 4]]\nA thought.\n"), "{}", h.app.sh.notes[READING_LOG]);
        let _ = h.app.update(Message::Log(id, NoteMsg::Action(EditAction::SetCaret { offset: 3, extend: false })));
        assert_eq!(h.app.log().doc.selection().head, top, "the caret is as near the title as it can be");
        for _ in 0..4 {
            let _ = h.app.update(Message::Log(id, NoteMsg::Action(EditAction::Backspace)));
        }
        assert!(h.app.sh.notes[READING_LOG].starts_with(&log[..top]), "backspace stops at the title");
        // A chapter the app adds is at the end, under a heading for today, in the open tab.
        let now = chrono::TimeZone::with_ymd_and_hms(&chrono::Local, 2026, 11, 1, 9, 0, 0).unwrap();
        assert!(h.app.sh.log_reading(Book::JOHN, 5, &now));
        h.app.settle();
        let text = h.app.log().doc.source().to_string();
        assert!(text.ends_with("## 2026-11-01\n\n- [[@John 5]]\n"), "{text}");
        assert!(h.app.log().doc.selection().head >= h.app.log().doc.front_end());
        let _ = h.app.view();
    }

    #[test]
    fn no_note_leads_to_the_reading_log_and_it_leads_to_no_note() {
        use crate::shared::READING_LOG;
        let mut app = App::new();
        app.sh.notes.insert(READING_LOG.into(), log_text());
        app.sh.notes.insert("notes/Linker.md".into(), "See [[Reading log]] and [[@John 3]].\n".into());
        // It is not among the notes: not listed, not found by searching, not first, not linked to, not a backlink.
        assert!(!app.sh.note_paths().iter().any(|p| p == READING_LOG));
        let none = std::collections::BTreeSet::new();
        for query in ["", "reading", "joshua"] {
            assert!(note_tab::groups(&app.sh, query, &none).iter().all(|g| g.notes.iter().all(|n| n.0 != READING_LOG)), "{query:?}");
        }
        assert_ne!(app.sh.first_ordinary_note(), READING_LOG);
        let backlinks = app.sh.backlinks(&scripture::RefRange::parse("John 3").unwrap());
        assert!(backlinks.iter().all(|b| b.path != READING_LOG) && backlinks.iter().any(|b| b.path == "notes/Linker.md"));
        // A note tab refuses to show it, and a link to it finds no note.
        app.open_note("notes/Linker.md".into());
        let note = app.nid();
        let _ = app.update(Message::Note(note, NoteMsg::PickNote(NoteChoice(READING_LOG.into()))));
        assert_eq!(app.note().path, "notes/Linker.md");
        app.open_note_link(note, "Reading log", Dest::Here);
        assert!(app.sh.status.contains("No note named"), "{}", app.sh.status);
        assert!(!app.tabs.values().any(|t| matches!(t, Tab::Log(_))), "no log tab was made by it");
        // From the log, a link to a note (a day's heading is one) opens it, as does a link to a verse.
        let id = app.show_log();
        let (mut asked, mut spec) = (None, None);
        for target in [
            study_core::markdown::LinkTarget::Note("Memory verses".into()),
            study_core::markdown::LinkTarget::Verse { range: scripture::RefRange::parse("John 3:16").unwrap(), module: None },
        ] {
            let Some(Tab::Log(tab)) = app.tabs.get_mut(&id) else { unreachable!() };
            let (_, request) = tab.update(&mut app.sh, NoteMsg::OpenLink(target.clone()));
            if matches!(target, study_core::markdown::LinkTarget::Note(_)) { asked = Some(request.is_some()) } else { spec = request }
        }
        assert_eq!(asked, Some(true), "a link to a note opens it from the log");
        assert!(matches!(spec, Some(note_tab::Request::Open(opening::Spec::Verse { .. }))), "a verse opens");
    }

    #[test]
    fn the_log_tab_is_offered_in_a_new_tab_and_kept_with_the_workspace_and_an_old_session_gets_it_for_the_log_note() {
        use crate::shared::READING_LOG;
        let mut app = App::new();
        let blank = app.new_blank();
        let id = app.add_tab(0, blank);
        let _ = app.update(Message::Open(id, Choice::Log));
        assert!(matches!(app.tabs.get(&id), Some(Tab::Log(_))), "chosen in a new tab");
        assert!(app.sh.notes.contains_key(READING_LOG), "the log is made if there was none");
        let saved = app.workspace();
        assert!(saved.panes.iter().any(|p| p.tabs.contains(&state::TabSave::Log)));
        // An older session kept it as a note.
        let mut older = saved.clone();
        for pane in &mut older.panes {
            for tab in &mut pane.tabs {
                if *tab == state::TabSave::Log {
                    *tab = state::TabSave::Note { path: READING_LOG.into() };
                }
            }
        }
        let mut fresh = App::new();
        fresh.sh.notes.insert(READING_LOG.into(), log_text());
        let _ = fresh.apply_workspace(&older);
        assert!(fresh.tabs.values().any(|t| matches!(t, Tab::Log(_))));
        assert!(!fresh.tabs.values().any(|t| matches!(t, Tab::Note(n) if n.path == READING_LOG)));
    }

    #[test]
    fn a_log_made_newest_first_is_put_right_when_its_tab_opens() {
        use crate::shared::READING_LOG;
        let mut app = App::new();
        app.sh.notes.insert(READING_LOG.into(), "---\ncreated: x\ntags: [reading-log]\norder: newest-first\n---\n# Reading log\n\n## 2026-10-14\n\n- [[@Psalm 23]]\n\n## 2026-10-13\n\n- [[@John 4]]\n- [[@John 3]]\n".into());
        let _ = app.show_log();
        let text = app.log().doc.source();
        assert!(!text.contains("order:"));
        assert!(text.find("2026-10-13").unwrap() < text.find("2026-10-14").unwrap() && text.find("[[@John 3]]").unwrap() < text.find("[[@John 4]]").unwrap(), "{text}");
        assert_eq!(app.sh.notes[READING_LOG], text);
    }

    #[test]
    fn the_tags_the_app_gives_its_own_notes_are_not_offered_to_search_by_or_to_pick() {
        use crate::shared::{PLAN_LOG, READING_LOG};
        let mut app = App::new();
        app.sh.notes.insert(READING_LOG.into(), study_core::reading_log::new_log("Reading log", "2026-10-13T18:27:42-05:00"));
        app.sh.notes.insert(PLAN_LOG.into(), study_core::plan_log::new_log("Plan log", "2026-10-13T18:27:42-05:00"));
        app.sh.notes.insert("notes/Mine.md".into(), "---\ntags: [reading-log, comfort]\n---\nWords.\n".into());
        let tags: Vec<String> = app.sh.tag_counts().into_iter().map(|(t, _)| t).collect();
        assert!(!tags.contains(&"plan-log".to_string()), "{tags:?}");
        assert!(tags.contains(&"comfort".to_string()), "the person's own are");
        // (A tag of that name on a note of the person's is theirs, and is counted once, not for the log as well.)
        assert_eq!(app.sh.tag_counts().into_iter().find(|(t, _)| t == "reading-log"), Some(("reading-log".to_string(), 1)));
        assert!(app.sh.tag_suggestions("plan", &[], 8).iter().all(|(t, _)| t != "plan-log"));
    }

    #[test]
    fn backlinks_are_in_sections_margin_notes_then_notes_by_type_then_plans_with_each_plans_days_together() {
        use crate::backlinks_tab::{Entry, sections};
        let mut app = App::new();
        app.sh.notes.insert("notes/Plain.md".into(), "Thinking of [[@John 3:16]].\n".into());
        app.sh.notes.insert("notes/Pray.md".into(), "---\ntype: prayer\ntags: [a, comfort]\n---\nLord, [[@John 3:16]]\n".into());
        app.sh.notes.insert("notes/Preach.md".into(), "---\ntype: sermon\n---\n[[@John 3:16]] again\n".into());
        app.sh.notes.insert("notes/Pray too.md".into(), "---\ntype: prayer\n---\n[[@John 3:16]]\n".into());
        for plan in study_core::starter_plans::starter_plans() {
            for (path, text) in plan.to_files() {
                app.sh.notes.insert(path, text);
            }
        }
        let passage = scripture::RefRange::parse("John 3").unwrap();
        let links = app.sh.backlinks(&passage);
        let found = sections(&app.sh, &links, Some(&passage));
        let titles: Vec<&str> = found.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Margin notes", "Notes", "Prayer notes", "Sermon notes", "Plans"]);
        assert_eq!(found[2].entries.len(), 2, "both prayers are under Prayer");
        // Each note has its tags, to show beside its name: the notes', and the margin notes' (the marks').
        let tags_of = |section: &crate::backlinks_tab::Section, path: &str| section.entries.iter().find_map(|e| match e {
            Entry::Note { path: p, tags, .. } if p == path => Some(tags.clone()),
            _ => None,
        });
        assert_eq!(tags_of(&found[2], "notes/Pray.md"), Some(vec!["a".to_string(), "comfort".to_string()]));
        assert_eq!(tags_of(&found[0], &app.sh.marks[0].path()), Some(app.sh.marks[0].tags.clone()));
        // A plan is one entry, with the days of it that read the chapter.
        let plans = &found[4];
        let names: Vec<(&str, usize)> = plans.entries.iter().filter_map(|e| match e {
            Entry::Plan { name, days } => Some((name.as_str(), days.len())),
            Entry::Note { .. } => None,
        }).collect();
        assert!(plans.entries.iter().all(|e| matches!(e, Entry::Plan { .. })));
        // The days are named as a person says them, not by their files.
        use crate::backlinks_tab::plan_day_label;
        assert_eq!(plan_day_label("Day 087"), "Day 87");
        assert_eq!(plan_day_label("01-05"), "Jan 5");
        assert_eq!(plan_day_label("12-31"), "Dec 31");
        assert_eq!(plan_day_label("2026-02-18"), "Feb 18, 2026");
        assert_eq!(plan_day_label("Overview"), "Overview");
        assert_eq!(plan_day_label("13-40"), "13-40", "not a date");
        let days: Vec<&str> = plans.entries.iter().filter_map(|e| match e {
            Entry::Plan { name, days } if name == "M'Cheyne" => Some(days.iter().map(|d| d.1.as_str())),
            _ => None,
        }).flatten().collect();
        assert!(days.iter().all(|d| d.len() >= 5 && !d.starts_with("0") && d.chars().next().is_some_and(char::is_alphabetic)), "{days:?}");
        assert!(names.contains(&("M'Cheyne", 2)) && names.contains(&("Bible in a year", 1)) && names.contains(&("Chronological", 1)), "{names:?}");
        assert_eq!(plans.notes(), names.iter().map(|(_, n)| n).sum::<usize>());
        // Drawn: the sections and what is in them.
        let from = app.nid();
        let _ = app.show_backlinks(from, backlinks_tab::Target::Passage(passage));
        let tab = app.tabs.iter().find_map(|(id, t)| matches!(t, Tab::Backlinks(_)).then_some(*id)).unwrap();
        let mut h = Harness::new(app);
        for shown in ["Margin notes", "Notes", "Prayer notes", "Sermon notes", "Plans"] {
            assert!(h.center_of(shown).is_some(), "{shown}");
        }
        // The plans are folded to their header to begin with; the other sections are open. A header folds and opens its section.
        assert!(h.center_of("M'Cheyne").is_none() && h.center_of("Pray").is_some(), "plans are folded, notes are not");
        let folded = |h: &Harness| matches!(h.app.tabs.get(&tab), Some(Tab::Backlinks(t)) if t.folded.contains("Plans"));
        assert!(folded(&h), "the plans are folded");
        // (The words "Plans" are also the top bar's button: the header's message is sent as it is pressed.)
        let _ = h.app.update(Message::Backlinks(tab, backlinks_tab::BacklinksMsg::ToggleSection("Plans".into())));
        assert!(!folded(&h), "and open when asked for");
        let (x, y) = h.center_of("Prayer notes").unwrap();
        h.click(x, y);
        assert!(h.center_of("Pray").is_none() && h.center_of("Prayer notes").is_some(), "a section folds to its header");
        let _ = tab;
    }

    #[test]
    fn the_verse_in_a_margin_notes_details_has_the_right_click_and_pull_of_a_link() {
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        let mut h = Harness::new(app);
        let anchor = h.app.sh.marks[0].anchor.to_string();
        let (x, y) = h.center_of(&format!("{anchor} \u{b7} {}", h.app.module_abbreviation())).unwrap();
        // Right click: where to open it.
        h.move_to(x, y);
        h.feed(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)));
        h.feed(Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)));
        let menu = h.app.link_menu.as_ref().expect("the menu of where to open it");
        assert!(matches!(&menu.spec, Spec::Verse { range, .. } if *range == h.app.sh.marks[0].anchor));
        h.app.link_menu = None;
        // Pulled away: it is carried (and coming up over it is not a click that opens it).
        h.app.bible_mut().chapter_no = 1;
        h.move_to(x, y);
        h.press();
        h.move_to(x + 20.0, y + 30.0);
        assert!(matches!(h.app.drag.as_ref().map(|d| &d.payload), Some(Payload::Link(_))), "picked up");
        h.move_to(x, y);
        h.release();
        assert_eq!(h.app.bible().chapter_no, 1, "not opened by the release");
        h.app.drag = None;
        // A plain click still opens it.
        h.click(x, y);
        assert_eq!(h.app.bible().chapter_no, 3);
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
        let _ = h.app.update(Message::Note(note, NoteMsg::ToggleFront));
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
        assert!(h.app.bible().card_source("popup:1:text").is_some_and(|t| t.contains("The Lord is my shepherd; I shall not want.")), "its text, to tell it apart");
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
            for y in 338..438usize {
                part.extend_from_slice(&all[(y * w + 1060) * 4..(y * w + 1450) * 4]);
            }
            part
        };
        h.cursor = mouse::Cursor::Unavailable;
        let first = card(&mut h);
        // The bar's right arrow, under the top card.
        h.click(1298.0, 454.0);
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
        h.move_to(399.0, 351.0);
        let footnote = card(&mut h);
        h.move_to(240.0, 390.0);
        let mark = card(&mut h);
        assert_ne!(footnote, mark, "each comes to the top as it is pointed at");
        h.move_to(399.0, 351.0);
        assert_eq!(card(&mut h), footnote);
        // With its note on top, a click on the mark edits it.
        h.click(240.0, 390.0);
        assert!(h.app.bible().editing.is_some());
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
        // (Over the text only: below it are buttons that do other things, such as the one to the notes that link to the chapter.)
        assert!(h.click_scan_to(1000, is_word).is_none() || h.app.bible().note_card.is_none(), "a click opens no pop-over");
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
        h.click(240.0, 390.0);
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
            let _ = h.app.update(Message::Bible(id, BibleMsg::Scrolled(10.0 * i as f32, 0.0)));
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
            for y in 590..780usize {
                part.extend_from_slice(&all[(y * w + 1060) * 4..(y * w + 1450) * 4]);
            }
            part
        };
        h.cursor = mouse::Cursor::Unavailable;
        let calm = stack(&mut h);
        // "God" and "world" are different words with different entries.
        h.move_to(767.0, 662.0);
        let god = stack(&mut h);
        assert_ne!(god, calm, "its entry came up");
        h.move_to(935.0, 662.0);
        let world = stack(&mut h);
        assert_ne!(world, god, "each word has its own entry");
        h.move_to(767.0, 662.0);
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
        assert!(shown.starts_with("notes/") && shown.contains("-note-"), "the plain note that was left, under its name: {shown}");
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
        app.note_mut().front_open = true;
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
        let _ = app.update(Message::EditPenColor(last));
        assert_eq!(app.pen_editing, Some(last), "its colors open");
        let _ = app.update(Message::SetPenColor(last, 21));
        assert_eq!(app.sh.settings.pens[last].color, state::PEN_COLORS[21]);
        assert_ne!(app.sh.settings.pens[last].color, color);
        assert_eq!(app.pen_editing, None, "and close on a choice");
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
        assert_eq!(parsed.front.title, None);
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
        assert!(!app.sh.first_ordinary_note().starts_with("templates/"));
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

    #[test]
    fn backlinks_list_the_notes_that_link_to_a_passage_and_the_toolbar_offers_them_for_green_verses() {
        let mut app = App::new();
        let john = |a, z| scripture::RefRange { book: Book::JOHN, chapter: 3, verses: Some((a, z)) };
        let found = |app: &App, range: &scripture::RefRange| -> Vec<String> { app.sh.backlinks(range).into_iter().map(|b| b.path).collect() };
        // Nicodemus links to John 3:2 and 3:3-5; Memory verses to 3:16.
        assert!(found(&app, &john(2, 2)).contains(&"notes/Nicodemus.md".to_string()));
        assert!(found(&app, &john(4, 4)).contains(&"notes/Nicodemus.md".to_string()), "3:4 is inside 3:3-5");
        assert!(found(&app, &john(16, 16)).contains(&"notes/Memory verses.md".to_string()));
        assert!(!found(&app, &john(16, 16)).contains(&"notes/Nicodemus.md".to_string()));
        let lines = &app.sh.backlinks(&john(2, 2)).iter().find(|b| b.path == "notes/Nicodemus.md").unwrap().lines.clone();
        assert!(lines.iter().any(|l| l.contains("[[@John 3:2]]")), "with the line that links");
        assert!(found(&app, &scripture::RefRange { book: Book::JOHN, chapter: 9, verses: Some((1, 1)) }).is_empty());
        assert!(app.sh.backlinks(&john(2, 2)).iter().all(|b| !b.path.starts_with("templates/")));

        // A green verse selected: the button is offered; not for one nothing links to.
        select_verses(&mut app, &[2]);
        assert!(app.bible().selection_has_backlinks(&app.sh));
        select_verses(&mut app, &[1]);
        assert!(!app.bible().selection_has_backlinks(&app.sh), "verse 1 is not green");
        select_verses(&mut app, &[2, 3]);
        assert_eq!(app.bible().selection_span(), Some(john(2, 3)));

        // Using it opens a tab, in another pane, listing them; one tab is reused.
        let id = app.bid();
        let panes = app.panes.len();
        let _ = app.update(Message::Bible(id, BibleMsg::OpenBacklinks));
        let tabs: Vec<TabId> = app.tabs.iter().filter(|(_, t)| matches!(t, Tab::Backlinks(_))).map(|(i, _)| *i).collect();
        assert_eq!(tabs.len(), 1);
        assert!(app.panes.len() >= panes && app.pane_of(tabs[0]) != app.pane_of(id), "beside the reader, not over it");
        let Some(Tab::Backlinks(tab)) = app.tabs.get(&tabs[0]) else { unreachable!() };
        assert_eq!(tab.target, crate::backlinks_tab::Target::Passage(john(2, 3)));
        assert_eq!(tab.title(&app.sh), "Backlinks \u{b7} John 3:2-3");
        select_verses(&mut app, &[16]);
        let _ = app.update(Message::Bible(id, BibleMsg::OpenBacklinks));
        assert_eq!(app.tabs.values().filter(|t| matches!(t, Tab::Backlinks(_))).count(), 1, "the same tab, now for John 3:16");

        // The tab shows them, and a click on a note shows that note.
        let mut h = Harness::new(app);
        assert!(h.center_of("Memory verses").is_some(), "the note that links to 3:16 is listed");
        assert!(h.center_of("Backlinks to John 3:16").is_some());
        let (x, y) = h.center_of("Memory verses").unwrap();
        h.click(x, y);
        assert_eq!(h.app.note().path, "notes/Memory verses.md");

        // It comes back with the workspace.
        let saved = h.app.workspace();
        assert!(saved.panes.iter().any(|p| p.tabs.iter().any(|t| matches!(t, state::TabSave::Backlinks { passage } if passage == "John.3.16"))));
        let mut other = App::new();
        let _ = other.apply_workspace(&saved).unwrap();
        assert!(other.tabs.values().any(|t| matches!(t, Tab::Backlinks(b) if b.target == crate::backlinks_tab::Target::Passage(john(16, 16)))));
    }

    #[test]
    fn the_note_menu_has_backlinks_listing_the_notes_that_link_to_the_note() {
        let mut app = App::new();
        // Nicodemus is linked to by name from Memory verses (twice) and by the prayer list's text? Check the sample.
        let nicodemus = "notes/Nicodemus.md".to_string();
        let found = app.sh.note_backlinks(&nicodemus);
        let paths: Vec<&str> = found.iter().map(|b| b.path.as_str()).collect();
        assert!(paths.contains(&"notes/Memory verses.md"), "{paths:?}");
        assert!(!paths.contains(&"notes/Nicodemus.md"), "not itself");
        assert!(found.iter().all(|b| !b.path.starts_with("templates/")));
        assert!(found.iter().find(|b| b.path == "notes/Memory verses.md").unwrap().lines.iter().any(|l| l.contains("[[Nicodemus]]")));
        // The menu offers it; using it opens the tab for this note, beside it.
        app.open_note(nicodemus.clone());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        let mut h = Harness::new(app);
        assert!(h.center_of("Backlinks").is_some(), "the menu has it");
        let (x, y) = h.center_of("Backlinks").unwrap();
        h.click(x, y);
        assert!(!h.app.note().menu_open);
        let tab = h.app.tabs.values().find_map(|t| match t { Tab::Backlinks(b) => Some(b), _ => None }).expect("a backlinks tab");
        assert_eq!(tab.target, crate::backlinks_tab::Target::Note(nicodemus.clone()));
        assert_eq!(tab.title(&h.app.sh), "Backlinks \u{b7} Nicodemus");
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        assert!(h.center_of("Backlinks to Nicodemus").is_some() && h.center_of("Memory verses").is_some());
        // It comes back with the workspace.
        let saved = h.app.workspace();
        let mut other = App::new();
        let _ = other.apply_workspace(&saved).unwrap();
        assert!(other.tabs.values().any(|t| matches!(t, Tab::Backlinks(b) if b.target == crate::backlinks_tab::Target::Note(nicodemus.clone()))));
        // A margin note is linked to by no one, so its menu does not offer it.
        let mut app = App::new();
        app.open_note(app.sh.marks[0].path());
        let id = app.nid();
        let _ = app.update(Message::Note(id, NoteMsg::ToggleMenu));
        let mut h = Harness::new(app);
        assert!(h.center_of("Backlinks").is_none());
    }

    // ---- where links open ------------------------------------------------------------------

    fn verse_link(range: &str) -> LinkTarget {
        LinkTarget::Verse { range: RefRange::parse(range).unwrap(), module: None }
    }

    fn bible_tabs(app: &App) -> Vec<TabId> {
        app.tabs.iter().filter(|(_, t)| matches!(t, Tab::Bible(_))).map(|(id, _)| *id).collect()
    }

    #[test]
    fn a_plain_click_follows_a_link_here_and_a_notes_verse_link_goes_to_the_reader_on_screen() {
        let mut app = App::new();
        let (bible, note) = (app.bid(), app.nid());
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(verse_link("Ps 23:1"))));
        assert_eq!(app.bible().book, Book::PSALMS, "the reader on screen took it");
        assert_eq!(bible_tabs(&app), [bible], "no new tab");
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(verse_link("Num 21:9"))));
        assert_eq!(bible_of(&app, bible).book, Book::NUMBERS, "a link in a Bible tab navigates that tab");
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
        // The other pane than the note's is the Bible's: its own Bible tab shows the verse.
        assert_eq!(bible_tabs(&h.app).len(), 1, "no new tab: the other pane's Bible tab is used");
        assert_eq!(h.app.panes[1].tabs.len(), 1, "and nothing went to the note's pane");
        assert_eq!((h.app.bible().book, h.app.bible().chapter_no), (Book::JOHN, 3));
        assert_eq!(h.app.bible().select_request.map(|(_, a, z)| (a, z)), Some((2, 2)));
    }

    #[test]
    fn open_in_other_pane_from_a_note_goes_to_the_pane_that_is_not_the_notes() {
        let mut app = App::new();
        let (bible, note) = (app.bid(), app.nid());
        assert_ne!(app.pane_of(bible), app.pane_of(note));
        let _ = app.open_spec(note, Spec::Verse { range: RefRange::parse("Ps 23:1").unwrap(), module: None }, Dest::OtherPane);
        assert_eq!(app.panes.len(), 2, "no pane was made");
        assert_eq!(app.focus, app.panes[app.pane_of(bible).unwrap()].id, "the Bible's pane, not the note's");
        assert_eq!(app.bible().book, Book::PSALMS);
        assert_eq!(app.panes[app.pane_of(note).unwrap()].tabs.len(), 1, "the note's pane is as it was");
        // And a note link from the Bible tab goes the other way, to the note's pane.
        let _ = app.open_spec(bible, Spec::Note("Memory verses".into()), Dest::OtherPane);
        assert_eq!(app.focus, app.panes[app.pane_of(note).unwrap()].id);
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
    fn opening_from_a_card_leaves_the_readers_own_window_first_and_then_goes_as_usual() {
        let mut app = App::new();
        app.bible_mut().show_strongs = true;
        let bible = app.bid();
        // A dictionary tab already open: opening an entry brings it forward without adding a tab, which on its own does
        // not end reading mode.
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenWord("G2316".into())));
        let _ = app.update(Message::Bible(bible, BibleMsg::ReadingMode));
        assert!(app.reader_mode.is_some());
        let tabs = app.tabs.len();
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenWord("G25".into())));
        assert!(app.reader_mode.is_none(), "reading mode is left");
        assert_eq!(app.tabs.len(), tabs, "the entry went to the dictionary tab there was");
        assert!(dict_tabs(&app).iter().any(|id| dict_of(&app, *id).key == "G25"));
        // The reading log, from the read card.
        let _ = app.update(Message::Bible(bible, BibleMsg::ReadingMode));
        assert!(app.reader_mode.is_some());
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenReadingLog));
        assert!(app.reader_mode.is_none() && app.tabs.values().any(|t| matches!(t, Tab::Log(_))));
        // A link in the text is not a card: it is followed in the reader, which stays alone.
        let _ = app.update(Message::Bible(bible, BibleMsg::ReadingMode));
        let psalm = LinkTarget::Verse { range: RefRange::parse("Ps 23:1").unwrap(), module: None };
        let _ = app.update(Message::Bible(bible, BibleMsg::OpenLink(psalm)));
        assert!(app.reader_mode.is_some(), "reading goes on");
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
        // [[%G25]], a dictionary this machine may not have by that name.
        let link = LinkTarget::Sword { module: "StrongsGreek".into(), key: "G25".into() };
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link.clone())));
        assert_eq!(dict_tabs(&app).len(), 1);
        let first = dict_tabs(&app)[0];
        assert_eq!(dict_of(&app, first).key, "G25");
        // Followed plainly it takes over the tab the link is in, which going back returns to.
        let note_pane = app.pane_of(first).unwrap();
        assert!(!app.tabs.contains_key(&note));
        let _ = app.update(Message::NavTab(first, true));
        assert!(matches!(app.tabs.get(&note), Some(Tab::Note(_))) && app.pane_of(note) == Some(note_pane));
        assert_eq!(dict_tabs(&app).len(), 0);
        // With keys: a new tab, another pane, a new pane.
        app.modifiers = Modifiers::COMMAND;
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link.clone())));
        assert_eq!(dict_tabs(&app).len(), 1);
        app.modifiers = Modifiers::SHIFT;
        let _ = app.update(Message::Note(note, NoteMsg::OpenLink(link)));
        assert_eq!(dict_tabs(&app).len(), 2);
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
        let _ = app.update(Message::Open(id, Choice::Dict));
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
    fn selecting_in_one_piece_of_a_dictionary_entry_lets_go_of_the_selection_in_another() {
        let mut app = App::new();
        let module = app.sh.backend.dictionaries()[0].id.clone();
        let blank = app.new_blank();
        let id = app.add_tab(1, blank);
        let _ = app.update(Message::Open(id, Choice::Dict));
        if let Some(Tab::Dict(t)) = app.tabs.get_mut(&id) {
            t.open(&app.sh, module, "G2316");
        }
        let mut h = Harness::new(app);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let select = |h: &mut Harness, piece: &str| {
            let _ = h.app.update(Message::Dict(id, DictMsg::Text(piece.into(), md_editor::Action::SelectAll)));
        };
        let selected = |h: &Harness, piece: &str| match h.app.tabs.get(&id) {
            Some(Tab::Dict(t)) => t.piece(piece).is_some_and(|p| p.1),
            _ => false,
        };
        select(&mut h, "definition");
        assert!(selected(&h, "definition"));
        select(&mut h, "lemma");
        assert!(selected(&h, "lemma") && !selected(&h, "definition"), "only the last is selected");
    }

    #[test]
    fn a_dictionary_entry_is_laid_out_in_the_tab() {
        let mut app = App::new();
        let module = app.sh.backend.dictionaries()[0].id.clone();
        let blank = app.new_blank();
        let id = app.add_tab(1, blank);
        let _ = app.update(Message::Open(id, Choice::Dict));
        if let Some(Tab::Dict(t)) = app.tabs.get_mut(&id) {
            t.open(&app.sh, module, "G2316");
        }
        let mut h = Harness::new(app);
        h.feed(Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now())));
        let e = h.app.sh.backend.lexicon_entry("G2316").unwrap();
        let Some(Tab::Dict(tab)) = h.app.tabs.get(&id) else { unreachable!() };
        for (name, text) in [("lemma", e.lemma.as_str()), ("transliteration", e.transliteration.as_str()), ("definition", e.definition.as_str())] {
            assert!(tab.piece(name).is_some_and(|(markdown, _)| markdown.contains(text)), "{name}: {text}");
        }
        let before = tab.piece("definition").unwrap();
        assert!(h.center_of("Go").is_some(), "the lookup button");
        // The text can be selected (but not changed).
        assert!(!before.1);
        let _ = h.app.update(Message::Dict(id, DictMsg::Text("definition".into(), md_editor::Action::SelectAll)));
        let _ = h.app.update(Message::Dict(id, DictMsg::Text("definition".into(), md_editor::Action::Insert("typed".into()))));
        let Some(Tab::Dict(tab)) = h.app.tabs.get(&id) else { unreachable!() };
        let after = tab.piece("definition").unwrap();
        assert!(after.1, "all of it is selected");
        assert_eq!(after.0, before.0, "and nothing typed went in");
    }

    #[test]
    fn a_link_in_a_dictionary_entry_opens_that_entry_in_the_tab_and_back_returns() {
        let mut app = App::new();
        let module = app.sh.backend.dictionaries()[0].id.clone();
        let blank = app.new_blank();
        let id = app.add_tab(1, blank);
        let _ = app.update(Message::Open(id, Choice::Dict));
        if let Some(Tab::Dict(t)) = app.tabs.get_mut(&id) {
            t.open(&app.sh, module, "G2316");
        }
        let link = LinkTarget::Sword { module: "Strongs".into(), key: "G25".into() };
        let _ = app.update(Message::Dict(id, DictMsg::Link(link)));
        assert_eq!(dict_of(&app, id).key, "G25", "the entry it names, in the same tab");
        let _ = app.update(Message::Dict(id, DictMsg::Back));
        assert_eq!(dict_of(&app, id).key, "G2316");
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
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(420.0, 0.5)));
        let _ = app.open_spec(app.panes[0].tabs[0], Spec::Verse { range: ps23(), module: None }, Dest::Here);
        assert_eq!(app.bible().scroll_y, 0.0, "a new chapter starts at the top");
        let _ = app.update(Message::Bible(id, BibleMsg::Scrolled(90.0, 0.1)));
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
        // The window is 1024 wide and 1500 tall: the right pane is the right 45% of it.
        for (x, y, zone) in [
            (600.0, 700.0, Zone::Left),
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
    fn a_double_click_on_a_footnote_marker_does_not_leave_its_card_open() {
        let mut h = narrow_reader();
        let (x, y) = h.click_scan(opened).expect("a footnote marker in John 3");
        assert!(h.app.bible().note_card.is_some(), "the first click opened its card");
        // The second press of a double click is a selection: the card goes.
        h.press();
        assert!(h.app.bible().note_card.is_none(), "no card for a double click");
        h.release();
        let _ = (x, y);
    }

    #[test]
    fn dragging_after_a_double_click_on_a_link_selects_text_and_does_not_carry_the_link() {
        let mut app = App::new();
        app.zoomed = Some(app.panes[0].id);
        let mut h = Harness::new(app);
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
        // Pressed twice in a row (a double click, without the first release that would follow the link) and pulled:
        // the text is selected.
        h.move_to(x, y);
        h.press();
        h.press();
        h.move_to(x + 30.0, y + 2.0);
        h.move_to(x + 60.0, y + 2.0);
        assert!(h.app.drag.is_none(), "the link was not picked up");
        h.release();
        assert!(h.app.bible().text_selection.is_some(), "the text was selected");
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
        // Pulled away from it, it is carried (after a pause: a press soon after a click is a double click, which selects).
        std::thread::sleep(std::time::Duration::from_millis(450));
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
    fn a_link_is_completed_from_a_list_of_choices_as_it_is_typed() {
        use crate::live_editor::CompletionEvent as Ev;
        let mut app = app_with_writer("");
        let id = app.nid();
        let type_text = |app: &mut App, text: &str| {
            for c in text.chars() {
                let _ = app.update(Message::Note(id, NoteMsg::Action(md_editor::Action::Insert(c.to_string()))));
            }
        };
        let choose = |app: &mut App, event: Ev| {
            let _ = app.update(Message::Note(id, NoteMsg::Completion(event)));
        };
        // After two brackets, the notes.
        type_text(&mut app, "see [[");
        assert!(app.note().completion.as_ref().is_some_and(|c| c.items.iter().any(|i| i.label == "Nicodemus")));
        type_text(&mut app, "nico");
        choose(&mut app, Ev::Accept);
        assert_eq!(app.note().doc.source(), "see [[Nicodemus]]");
        assert_eq!(app.note().doc.selection().head, app.note().doc.source().len(), "the caret is past the brackets");
        assert!(app.note().completion.is_none());
        // A passage, step by step: the book, the chapter, the verse.
        type_text(&mut app, " and [[@jo");
        choose(&mut app, Ev::Accept);
        assert!(app.note().doc.source().ends_with("[[@John ]]"), "{}", app.note().doc.source());
        assert!(app.note().completion.as_ref().is_some_and(|c| c.items[0].label == "John 3"), "the chapters come next");
        choose(&mut app, Ev::Accept);
        assert!(app.note().doc.source().ends_with("[[@John 3:]]"));
        choose(&mut app, Ev::Move(1));
        choose(&mut app, Ev::Accept);
        assert!(app.note().doc.source().ends_with("[[@John 3:1]]"), "{}", app.note().doc.source());
        assert!(app.note().completion.is_none());
        // A Strong's number.
        type_text(&mut app, " [[%G231");
        choose(&mut app, Ev::Accept);
        assert!(app.note().doc.source().ends_with("[[%G231]]") || app.note().doc.source().contains("[[%G231"), "{}", app.note().doc.source());
        // Escape puts the list away until the link is left.
        type_text(&mut app, " [[");
        assert!(app.note().completion.is_some());
        choose(&mut app, Ev::Dismiss);
        assert!(app.note().completion.is_none());
        type_text(&mut app, "x");
        assert!(app.note().completion.is_none(), "still away while it is the same link");
    }

    #[test]
    fn a_name_cannot_begin_with_a_marker_of_links_but_can_look_like_a_passage_or_a_number() {
        let mut app = App::new();
        app.sh.notes.insert("notes/A.md".into(), "x\n".into());
        for name in ["%G25", "@John 3:16", "%x"] {
            assert!(app.sh.rename_note("notes/A.md", name).is_err(), "{name}");
        }
        assert_eq!(app.sh.rename_note("notes/A.md", "John 3").unwrap(), "notes/John 3.md");
        assert_eq!(app.sh.rename_note("notes/John 3.md", "G25").unwrap(), "notes/G25.md");
        // A new note's title has any such start taken off.
        assert_eq!(study_core::templates::file_name("%@Odd name"), "Odd name");
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
