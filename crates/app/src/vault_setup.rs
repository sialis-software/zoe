//! Choosing the vault: the welcome page for a first run (or a vault that cannot be found), the
//! Vault section of the settings, and the card that confirms a folder before it is used.
//!
//! A vault is a folder of Markdown files. It is either *created* (a new folder, named, inside a
//! folder the person picked) or *opened* (a folder that already holds notes, such as an Obsidian
//! vault, or is empty). Either way the folder is looked at first and the person says what to add
//! to it: the sample notes (only to a folder with no notes) and the starting templates (only to a
//! folder with none).

use crate::live::{self, Onboarding, Options, Samples, Scan};
use crate::state::Config;
use crate::{App, Message, popup_style};
use iced::widget::{button, checkbox, column, container, opaque, row, rule, space, stack, text, text_input};
use iced::{Element, Length, Task, Theme};
use std::path::{Path, PathBuf};

/// The name offered for a new vault's folder.
pub const DEFAULT_NAME: &str = "Zoe Vault";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A new folder, inside the one picked.
    Create,
    /// The folder picked, as it is.
    Open,
}

/// The card asking for the last details of a vault.
#[derive(Clone, Debug)]
pub struct Setup {
    pub mode: Mode,
    /// The folder picked: the vault itself when opening, what holds it when creating.
    pub folder: PathBuf,
    /// The new folder's name, when creating.
    pub name: String,
    pub samples: bool,
    pub templates: bool,
    pub scan: Scan,
    /// Why the last try failed.
    pub error: Option<String>,
}

impl Setup {
    pub fn new(mode: Mode, folder: PathBuf) -> Setup {
        let mut setup = Setup {
            mode,
            folder,
            name: DEFAULT_NAME.into(),
            samples: mode == Mode::Create,
            templates: true,
            scan: Scan::default(),
            error: None,
        };
        setup.rescan();
        setup
    }

    /// The folder that becomes the vault.
    pub fn target(&self) -> PathBuf {
        match self.mode {
            Mode::Create => self.folder.join(self.name.trim()),
            Mode::Open => self.folder.clone(),
        }
    }

    fn rescan(&mut self) {
        self.scan = live::scan(&self.target());
    }

    /// What is wrong with the name of a new folder, if anything.
    pub fn name_problem(&self) -> Option<&'static str> {
        if self.mode == Mode::Open {
            return None;
        }
        let name = self.name.trim();
        if name.is_empty() {
            Some("Give the vault a name.")
        } else if name.contains(['/', '\\', ':']) || name.starts_with('.') {
            Some("A folder name cannot have slashes or colons, or start with a dot.")
        } else {
            None
        }
    }

    /// What to add as the vault is opened.
    pub fn options(&self) -> Options {
        Options {
            samples: if self.samples && self.scan.notes == 0 { Samples::IfEmpty } else { Samples::Never },
            templates: self.templates && !self.scan.templates,
            plans: self.templates && self.scan.notes == 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum VaultMsg {
    /// Ask for a folder (a system dialog), then go on to confirm it.
    Choose(Mode),
    Picked(Mode, Option<PathBuf>),
    Name(String),
    Samples(bool),
    Templates(bool),
    Cancel,
    Confirm,
    /// Start with the vault in the app's own data folder, as earlier versions did.
    UseDefault,
    /// Look again for the vault that could not be found.
    Retry,
    /// Show the vault's folder in the system's file browser.
    Reveal,
    /// Show the welcome window again (it was closed before a vault was made).
    OpenWelcome,
    /// Ask for the folder of the first-run vault (a system dialog).
    WelcomeChoose,
    /// The folder asked for (none: back to the default).
    WelcomePicked(Option<PathBuf>),
    /// Make the vault from the welcome window, then open the module manager.
    WelcomeCreate,
}

impl App {
    pub fn vault_message(&mut self, message: VaultMsg) -> Task<Message> {
        match message {
            VaultMsg::Choose(mode) => {
                let title = match mode {
                    Mode::Create => "Choose where to create the vault",
                    Mode::Open => "Choose the vault folder",
                };
                let start = self.setup.as_ref().map(|s| s.folder.clone()).or_else(|| self.sh.vault.as_ref().and_then(|v| v.parent().map(Path::to_path_buf)));
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
                        if let Some(start) = start {
                            dialog = dialog.set_directory(start);
                        }
                        dialog.pick_folder().await.map(|handle| handle.path().to_path_buf())
                    },
                    move |folder| Message::Vault(VaultMsg::Picked(mode, folder)),
                )
            }
            VaultMsg::Picked(mode, folder) => {
                let Some(folder) = folder else { return Task::none() };
                self.panel = None;
                self.setup = Some(Setup::new(mode, folder));
                // The card is in the workspace window: the settings window would cover it, so
                // it is closed.
                let focus = iced::window::gain_focus(self.main);
                match self.settings_window.take() {
                    Some(id) => Task::batch([iced::window::close(id), focus]),
                    None => focus,
                }
            }
            VaultMsg::Name(name) => {
                if let Some(setup) = &mut self.setup {
                    setup.name = name;
                    setup.error = None;
                    setup.rescan();
                }
                Task::none()
            }
            VaultMsg::Samples(on) => {
                if let Some(setup) = &mut self.setup {
                    setup.samples = on;
                }
                Task::none()
            }
            VaultMsg::Templates(on) => {
                if let Some(setup) = &mut self.setup {
                    setup.templates = on;
                }
                Task::none()
            }
            VaultMsg::Cancel => {
                self.setup = None;
                Task::none()
            }
            VaultMsg::Confirm => {
                let Some(setup) = self.setup.clone() else { return Task::none() };
                if setup.name_problem().is_some() {
                    return Task::none();
                }
                match self.switch_vault(setup.target(), setup.options()) {
                    Ok(task) => task,
                    Err(e) => {
                        if let Some(setup) = &mut self.setup {
                            setup.error = Some(e);
                        }
                        Task::none()
                    }
                }
            }
            VaultMsg::OpenWelcome => self.open_welcome(),
            VaultMsg::WelcomeChoose => {
                let start = self.welcome_target().and_then(|d| d.parent().map(Path::to_path_buf));
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new().set_title("Choose the folder for the vault");
                        if let Some(start) = start {
                            dialog = dialog.set_directory(start);
                        }
                        dialog.pick_folder().await.map(|handle| handle.path().to_path_buf())
                    },
                    |folder| match folder {
                        Some(folder) => Message::Vault(VaultMsg::WelcomePicked(Some(folder))),
                        None => Message::Vault(VaultMsg::OpenWelcome),
                    },
                )
            }
            VaultMsg::WelcomePicked(folder) => {
                self.welcome_folder = folder;
                self.welcome_error = None;
                Task::none()
            }
            VaultMsg::WelcomeCreate => {
                let Some(dir) = self.welcome_target() else { return Task::none() };
                // A folder that already has notes is left as it is; a new or empty one gets the templates.
                let has_notes = live::scan(&dir).notes > 0;
                let options = Options { samples: Samples::Never, templates: !has_notes, plans: !has_notes };
                match self.switch_vault(dir, options) {
                    Ok(task) => {
                        let closed = self.welcome_window.take().map(iced::window::close).unwrap_or_else(Task::none);
                        self.welcome_folder = None;
                        self.welcome_error = None;
                        // Modules come next: the manager is where they are found and installed.
                        let manager = self.manager_message(crate::module_manager::ManagerMsg::Open);
                        Task::batch([task, closed, manager])
                    }
                    Err(e) => {
                        self.welcome_error = Some(e);
                        Task::none()
                    }
                }
            }
            VaultMsg::UseDefault => {
                let Some(home) = self.home.clone() else { return Task::none() };
                let options = Options { samples: Samples::Never, templates: true, plans: true };
                self.switch_vault(home.join("vault"), options).unwrap_or_else(|e| {
                    self.sh.status = format!("Could not open the vault: {e}");
                    Task::none()
                })
            }
            VaultMsg::Retry => {
                let Some(Onboarding::Missing(dir)) = self.onboarding.clone() else { return Task::none() };
                if dir.is_dir() {
                    let options = Options { samples: Samples::Never, templates: false, plans: false };
                    match self.switch_vault(dir, options) {
                        Ok(task) => Task::batch([task, self.welcome_window.take().map(iced::window::close).unwrap_or_else(Task::none)]),
                        Err(e) => {
                            self.welcome_error = Some(e);
                            Task::none()
                        }
                    }
                } else {
                    self.sh.toast("Still not there");
                    Task::none()
                }
            }
            VaultMsg::Reveal => {
                if let Some(dir) = &self.sh.vault {
                    let opener = if cfg!(target_os = "macos") {
                        "open"
                    } else if cfg!(target_os = "windows") {
                        "explorer"
                    } else {
                        "xdg-open"
                    };
                    if std::process::Command::new(opener).arg(dir).spawn().is_err() {
                        self.sh.toast("Could not open the folder");
                    }
                }
                Task::none()
            }
        }
    }

    /// Make the vault at `dir` the one in use: save what is open, load the new notes, remember
    /// the choice and start from the default layout (what was open belonged to the old notes).
    pub fn switch_vault(&mut self, dir: PathBuf, options: Options) -> Result<Task<Message>, String> {
        let home = self.home.clone().ok_or("There is no data folder to keep the choice in")?;
        // Put away what is being edited, and write it, while it is still the old vault's.
        let open: Vec<_> = self.tabs.keys().copied().collect();
        for id in open {
            if let Some(crate::Tab::Bible(tab)) = self.tabs.get_mut(&id) {
                tab.stop_edit(&mut self.sh);
            }
        }
        self.sh.save();
        let loaded = live::load_vault(self.sh.backend.as_ref(), &dir, &live::index_path(&home, &dir), options)?;
        let config_dir = self.config_dir().unwrap_or_else(|| home.clone());
        let mut config = Config::load(&config_dir);
        config.vault = Some(dir.clone());
        if let Err(e) = config.save(&config_dir) {
            self.sh.status = format!("Could not remember the vault: {e}");
        }
        self.sh.replace_vault(dir.clone(), loaded.notes, loaded.persist);
        if !loaded.status.is_empty() {
            self.sh.status = loaded.status;
        }
        self.onboarding = None;
        self.setup = None;
        let workspace = self.default_workspace();
        let task = self.apply_workspace(&workspace)?;
        self.sh.toast(format!("Opened the vault {}", dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        Ok(task)
    }

    // ---- views -------------------------------------------------------------------------

    /// The folder the first-run vault is made in: the one picked, else the app's data folder.
    pub fn welcome_target(&self) -> Option<PathBuf> {
        self.welcome_folder.clone().or_else(|| self.home.as_ref().map(|home| home.join("vault")))
    }

    /// Open the window that sets up the vault (a fixed-size accessory of the workspace window,
    /// like the settings).
    pub fn open_welcome(&mut self) -> Task<Message> {
        if let Some(id) = self.welcome_window {
            return iced::window::gain_focus(id);
        }
        let (id, open) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(520.0, 560.0),
            resizable: false,
            minimizable: false,
            exit_on_close_request: false,
            ..iced::window::Settings::default()
        });
        self.welcome_window = Some(id);
        let main = self.main;
        open.then(move |_| crate::native::attach(main, id)).discard()
    }

    /// What the workspace window shows while the welcome window is closed or open: nothing to do
    /// there until a vault is chosen.
    pub fn waiting_view(&self) -> Element<'_, Message> {
        let page = column![
            text("Zoe needs a vault to keep your notes in").size(16),
            button(text("Set up\u{2026}").size(13)).padding([4, 14]).on_press(Message::Vault(VaultMsg::OpenWelcome)),
        ]
        .spacing(12)
        .align_x(iced::Center);
        let background = |theme: &Theme| container::Style {
            background: Some(theme.extended_palette().background.base.color.into()),
            text_color: Some(theme.extended_palette().background.base.text),
            ..container::Style::default()
        };
        opaque(container(page).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).style(background))
    }

    /// The first run's window: what a vault is, and where the new one goes. By default it is a
    /// blank vault (with the note templates) in the app's data folder.
    pub fn welcome_panel(&self) -> Element<'_, Message> {
        let faint = |t: String| text(t).size(12).style(text::secondary);
        let missing = match &self.onboarding {
            Some(Onboarding::Missing(dir)) => Some(dir.clone()),
            _ => None,
        };
        let intro = match &missing {
            None => column![
                text("Welcome to Zoe").size(26),
                text(
                    "Your notes, highlights and margin notes are kept in a vault: an ordinary folder of \
                     Markdown files that you own. You can back it up, sync it, or open it in any other editor."
                )
                .size(13)
                .style(text::secondary),
            ],
            Some(dir) => column![
                text("Your vault is not there").size(26),
                text(format!(
                    "The vault at {} cannot be found. If it is on a drive that is not connected, connect it \
                     and try again; or set up another.",
                    tidy(dir)
                ))
                .size(13)
                .style(text::secondary),
            ],
        }
        .spacing(8);
        let target = self.welcome_target();
        // Where the vault will be, in dim text under the button that makes it.
        let mut place = column![].spacing(3).align_x(iced::Center);
        if let Some(folder) = &target {
            place = place.push(faint(tidy(folder)));
            let scan = live::scan(folder);
            if scan.notes > 0 {
                place = place.push(faint(format!("That folder already has {}: they are used as they are.", notes_of(&scan))));
            }
        }
        let mut actions = row![].spacing(8).align_y(iced::Center);
        if let Some(dir) = &missing {
            actions = actions.push(button(text("Try again").size(14)).padding([8, 24]).style(button::secondary).on_press_maybe(dir.is_dir().then_some(Message::Vault(VaultMsg::Retry))));
        }
        actions = actions.push(button(text("Create vault").size(14)).padding([8, 24]).on_press_maybe(target.is_some().then_some(Message::Vault(VaultMsg::WelcomeCreate))));
        let mut other = row![
            button(text("Choose another folder\u{2026}").size(12)).padding([4, 10]).style(button::text).on_press(Message::Vault(VaultMsg::WelcomeChoose)),
        ]
        .spacing(8);
        if self.welcome_folder.is_some() {
            other = other.push(button(text("Use the default").size(12)).padding([4, 10]).style(button::text).on_press(Message::Vault(VaultMsg::WelcomePicked(None))));
        }
        let mut middle = column![actions, place, other].spacing(10).align_x(iced::Center);
        if let Some(dir) = &missing {
            middle = middle.push(faint(format!("It was at {}.", tidy(dir))));
        }
        if let Some(error) = self.welcome_error.as_ref() {
            middle = middle.push(text(format!("Could not use this folder: {error}")).size(12).style(text::danger));
        }
        // The create button is in the middle of the window, and the one switch is at the foot of it.
        let page = column![
            intro,
            container(middle).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill),
            self.persecution_checkbox(),
        ]
        .spacing(12);
        container(page).padding(28).width(Length::Fill).height(Length::Fill).into()
    }

    /// The card that confirms a folder, over a dimmed window.
    pub fn setup_view(&self, setup: &Setup) -> Element<'_, Message> {
        let title = match setup.mode {
            Mode::Create => "Create a new vault",
            Mode::Open => "Open a vault",
        };
        let faint = |t: String| text(t).size(12).style(text::secondary);
        let mut card = column![text(title).size(16)].spacing(10);
        match setup.mode {
            Mode::Create => {
                card = card
                    .push(faint("Name of the new folder".into()))
                    .push(
                        text_input("Name", &setup.name)
                            .on_input(|n| Message::Vault(VaultMsg::Name(n)))
                            .on_submit(Message::Vault(VaultMsg::Confirm))
                            .size(14)
                            .padding([5, 8]),
                    )
                    .push(
                        row![
                            text(format!("In {}", tidy(&setup.folder))).size(12).style(text::secondary).width(Length::Fill),
                            button(text("Change\u{2026}").size(12))
                                .padding([1, 8])
                                .style(button::secondary)
                                .on_press(Message::Vault(VaultMsg::Choose(Mode::Create))),
                        ]
                        .spacing(8)
                        .align_y(iced::Center),
                    );
                if let Some(problem) = setup.name_problem() {
                    card = card.push(faint(problem.into()));
                } else if setup.scan.notes > 0 {
                    card = card.push(faint(format!("That folder already exists and has {}: it will be used as it is.", notes_of(&setup.scan))));
                }
            }
            Mode::Open => {
                card = card.push(text(tidy(&setup.folder)).size(13));
                let found = if setup.scan.notes == 0 {
                    "There are no notes in this folder yet.".to_string()
                } else {
                    format!("It has {}.", notes_of(&setup.scan))
                };
                card = card.push(faint(found));
                if setup.scan.obsidian {
                    card = card.push(faint("It is an Obsidian vault. Its files and settings are left as they are.".into()));
                }
                if setup.scan.ours {
                    card = card.push(faint("It has highlights and margin notes from this app.".into()));
                }
            }
        }
        let mut adds = column![].spacing(6);
        if setup.scan.notes == 0 && setup.name_problem().is_none() {
            adds = adds.push(checkbox(setup.samples).label("Add a few sample notes to try").on_toggle(|on| Message::Vault(VaultMsg::Samples(on))).size(16).text_size(13));
        }
        if !setup.scan.templates {
            adds = adds.push(
                checkbox(setup.templates)
                    .label("Add note templates (Prayer list, Topic, Daily, Blank)")
                    .on_toggle(|on| Message::Vault(VaultMsg::Templates(on)))
                    .size(16)
                    .text_size(13),
            );
            adds = adds.push(faint("Templates are files in the vault's templates folder: edit them to change the kinds of note.".into()));
        }
        card = card.push(adds);
        if let Some(error) = &setup.error {
            card = card.push(text(format!("Could not use this folder: {error}")).size(12).style(text::danger));
        }
        let go = match setup.mode {
            Mode::Create => "Create",
            Mode::Open => "Open",
        };
        card = card.push(
            row![
                space::horizontal(),
                button(text("Cancel").size(13)).padding([4, 12]).style(button::secondary).on_press(Message::Vault(VaultMsg::Cancel)),
                button(text(go).size(13)).padding([4, 12]).on_press_maybe(setup.name_problem().is_none().then_some(Message::Vault(VaultMsg::Confirm))),
            ]
            .spacing(8),
        );
        let dim = |_: &Theme| container::Style {
            background: Some(iced::Color { a: 0.4, ..iced::Color::BLACK }.into()),
            ..container::Style::default()
        };
        let backdrop = iced::widget::mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill).style(dim));
        let centered = container(opaque(container(card).padding(16).width(400).style(popup_style)))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .padding(iced::Padding { top: 120.0, ..iced::Padding::ZERO });
        stack![backdrop, centered].into()
    }

    /// The Vault section of the settings.
    pub fn vault_settings(&self) -> Element<'_, Message> {
        let shown = self.sh.vault.as_deref().map(tidy).unwrap_or_else(|| "None".into());
        // The vault the app keeps in its own data folder, for when none is wanted elsewhere.
        let default_vault = self.home.as_ref().map(|home| home.join("vault"));
        let default_row: Element<'_, Message> = match default_vault {
            Some(dir) => {
                let current = self.sh.vault.as_deref() == Some(dir.as_path());
                row![
                    column![text("Default vault").size(13), text(tidy(&dir)).size(11).style(text::secondary)].spacing(2).width(Length::Fill),
                    button(text(if current { "In use" } else { "Open" }).size(12))
                        .padding([3, 10])
                        .style(button::secondary)
                        .on_press_maybe((!current).then_some(Message::Vault(VaultMsg::UseDefault))),
                ]
                .spacing(8)
                .align_y(iced::Center)
                .into()
            }
            None => space::vertical().height(0).into(),
        };
        let buttons = row![
            button(text("Open\u{2026}").size(12)).padding([3, 10]).style(button::secondary).on_press(Message::Vault(VaultMsg::Choose(Mode::Open))),
            button(text("Create new\u{2026}").size(12)).padding([3, 10]).style(button::secondary).on_press(Message::Vault(VaultMsg::Choose(Mode::Create))),
            button(text("Show in folder").size(12))
                .padding([3, 10])
                .style(button::secondary)
                .on_press_maybe(self.sh.vault.is_some().then_some(Message::Vault(VaultMsg::Reveal))),
        ]
        .spacing(6);
        column![
            text(shown).size(14),
            text("The folder of Markdown files with your notes, highlights and templates.").size(11).style(text::secondary),
            buttons,
            rule::horizontal(1),
            default_row,
        ]
        .spacing(8)
        .into()
    }
}

fn notes_of(scan: &Scan) -> String {
    match scan.notes {
        1 => "1 note".into(),
        n if n >= live::SCAN_LIMIT => format!("more than {} notes", live::SCAN_LIMIT),
        n => format!("{n} notes"),
    }
}

/// A path as the person knows it: their home folder as `~`.
pub fn tidy(path: &Path) -> String {
    let shown = path.display().to_string();
    match std::env::var_os("HOME").map(PathBuf::from).and_then(|h| path.strip_prefix(h).ok().map(|p| p.display().to_string())) {
        Some(rest) if rest.is_empty() => "~".into(),
        Some(rest) => format!("~/{rest}"),
        None => shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::Start;

    fn app_with_home() -> (App, tempfile::TempDir) {
        let home = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.home = Some(home.path().to_path_buf());
        (app, home)
    }

    #[test]
    fn the_vault_to_start_with_is_the_chosen_one_then_the_old_default_else_the_person_is_asked() {
        let home = tempfile::tempdir().unwrap();
        let chosen = tempfile::tempdir().unwrap();
        let gone = home.path().join("unplugged");
        assert!(matches!(live::start_vault(home.path(), None), Start::Ask(Onboarding::First)));
        assert!(matches!(live::start_vault(home.path(), Some(gone.clone())), Start::Ask(Onboarding::Missing(p)) if p == gone));
        std::fs::create_dir(home.path().join("vault")).unwrap();
        assert!(matches!(live::start_vault(home.path(), None), Start::Vault(p) if p == home.path().join("vault")), "an earlier version's vault is kept");
        assert!(matches!(live::start_vault(home.path(), Some(chosen.path().into())), Start::Vault(p) if p == chosen.path()));
    }

    #[test]
    fn the_choice_is_kept_in_the_data_folder_and_a_bad_file_is_ignored() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(home.path()), Config::default());
        Config { vault: Some("/somewhere/notes".into()), ..Config::default() }.save(home.path()).unwrap();
        assert_eq!(Config::load(home.path()).vault, Some("/somewhere/notes".into()));
        std::fs::write(home.path().join("config.json"), "{ not json").unwrap();
        assert_eq!(Config::load(home.path()), Config::default());
    }

    #[test]
    fn each_vault_has_its_own_index_and_the_default_keeps_its_old_one() {
        let home = Path::new("/data");
        assert_eq!(live::index_path(home, &home.join("vault")), home.join("cache/index.sqlite"));
        let (a, b) = (live::index_path(home, Path::new("/x/a")), live::index_path(home, Path::new("/x/b")));
        assert_ne!(a, b);
        assert_eq!(a, live::index_path(home, Path::new("/x/a")));
    }

    #[test]
    fn a_folder_is_looked_at_before_it_is_used() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(live::scan(dir.path()), Scan::default());
        std::fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join(".obsidian/hidden.md"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("Daily")).unwrap();
        std::fs::write(dir.path().join("Daily/2025-01-01.md"), "x").unwrap();
        std::fs::write(dir.path().join("Idea.md"), "x").unwrap();
        std::fs::write(dir.path().join("photo.png"), "x").unwrap();
        let scan = live::scan(dir.path());
        assert_eq!((scan.notes, scan.obsidian, scan.templates, scan.ours), (2, true, false, false), "hidden folders and other files are not notes");
        std::fs::create_dir_all(dir.path().join("templates")).unwrap();
        std::fs::write(dir.path().join("templates/T.md"), "x").unwrap();
        assert!(live::scan(dir.path()).templates);
        assert_eq!(live::scan(&dir.path().join("missing")), Scan::default());
    }

    #[test]
    fn samples_go_only_into_a_folder_without_notes_and_templates_only_where_there_are_none() {
        let dir = tempfile::tempdir().unwrap();
        let mut setup = Setup::new(Mode::Open, dir.path().into());
        setup.samples = true;
        assert_eq!(setup.options().samples, Samples::IfEmpty);
        assert!(setup.options().templates);
        std::fs::write(dir.path().join("Mine.md"), "x").unwrap();
        std::fs::create_dir(dir.path().join("templates")).unwrap();
        std::fs::write(dir.path().join("templates/Mine.md"), "x").unwrap();
        let setup = Setup { samples: true, ..Setup::new(Mode::Open, dir.path().into()) };
        assert_eq!(setup.options().samples, Samples::Never, "a folder with notes is not given sample ones");
        assert!(!setup.options().templates, "its own templates are the kinds of note");
    }

    #[test]
    fn a_new_vaults_folder_is_named_inside_the_one_picked_with_a_name_that_works_as_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut setup = Setup::new(Mode::Create, dir.path().into());
        assert_eq!(setup.target(), dir.path().join(DEFAULT_NAME));
        assert!(setup.name_problem().is_none());
        for bad in ["", "  ", "a/b", ".hidden", "c:d"] {
            setup.name = bad.into();
            assert!(setup.name_problem().is_some(), "{bad:?}");
        }
        setup.name = " Mine ".into();
        assert_eq!(setup.target(), dir.path().join("Mine"), "spaces around the name are not part of it");
        assert!(Setup::new(Mode::Open, dir.path().into()).name_problem().is_none(), "an opened folder keeps its name");
    }

    #[test]
    fn a_new_vault_is_made_remembered_and_replaces_what_was_open() {
        let (mut app, home) = app_with_home();
        let parent = tempfile::tempdir().unwrap();
        let mut setup = Setup::new(Mode::Create, parent.path().into());
        setup.name = "Study".into();
        app.setup = Some(setup);
        let _ = app.vault_message(VaultMsg::Confirm);
        let dir = parent.path().join("Study");
        assert!(app.setup.is_none() && app.sh.vault.as_deref() == Some(dir.as_path()));
        assert!(dir.join("templates").is_dir(), "the starting templates are files in it");
        assert!(app.sh.notes.keys().any(|p| p.starts_with("templates/")));
        assert_eq!(Config::load(home.path()).vault, Some(dir.clone()), "and it is the one next time");
        assert!(!app.panes.is_empty(), "a workspace of the new notes is open");
        // What is written afterwards goes into the new folder.
        app.sh.notes.insert("Fresh.md".into(), "# Fresh\n".into());
        app.sh.save();
        assert_eq!(std::fs::read_to_string(dir.join("Fresh.md")).unwrap(), "# Fresh\n");
    }

    #[test]
    fn an_existing_vault_is_opened_as_it_is() {
        let (mut app, home) = app_with_home();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join("Mine.md"), "# Mine\n").unwrap();
        let mut setup = Setup::new(Mode::Open, dir.path().into());
        setup.templates = false;
        app.setup = Some(setup);
        let _ = app.vault_message(VaultMsg::Confirm);
        assert_eq!(app.sh.notes.keys().cloned().collect::<Vec<_>>(), ["Mine.md"], "nothing was added, and the old notes are gone");
        assert!(!dir.path().join("templates").exists() && !dir.path().join("margins").exists());
        assert_eq!(Config::load(home.path()).vault.as_deref(), Some(dir.path()));
        // The index went to the data folder, not into the vault.
        assert!(home.path().join("cache").is_dir());
        assert!(std::fs::read_dir(dir.path()).unwrap().flatten().all(|e| e.file_name() == ".obsidian" || e.file_name() == "Mine.md"));
    }

    #[test]
    fn what_was_being_edited_is_saved_to_the_old_vault_before_the_switch() {
        let (mut app, _home) = app_with_home();
        let old = tempfile::tempdir().unwrap();
        let loaded = live::load_vault(&study_core::mock::MockBackend, old.path(), &old.path().join(".ix"), Options { samples: Samples::Never, templates: false, plans: false }).unwrap();
        app.sh.replace_vault(old.path().into(), loaded.notes, loaded.persist);
        app.sh.notes.insert("Unsaved.md".into(), "words".into());
        let new = tempfile::tempdir().unwrap();
        let _ = app.switch_vault(new.path().into(), Options { samples: Samples::Never, templates: false, plans: false }).unwrap();
        assert_eq!(std::fs::read_to_string(old.path().join("Unsaved.md")).unwrap(), "words");
        assert!(!app.sh.notes.contains_key("Unsaved.md") && !new.path().join("Unsaved.md").exists());
    }

    #[test]
    fn a_missing_vault_is_asked_about_until_it_is_back() {
        let (mut app, home) = app_with_home();
        let dir = home.path().join("unplugged");
        app.onboarding = Some(Onboarding::Missing(dir.clone()));
        let _ = app.vault_message(VaultMsg::Retry);
        assert!(app.onboarding.is_some(), "still not there");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("Mine.md"), "# Mine\n").unwrap();
        let _ = app.vault_message(VaultMsg::Retry);
        assert!(app.onboarding.is_none());
        assert!(app.sh.notes.contains_key("Mine.md"));
        assert!(!dir.join("templates").exists(), "coming back adds nothing");
    }

    #[test]
    fn the_first_run_can_just_use_the_default_vault() {
        let (mut app, home) = app_with_home();
        app.onboarding = Some(Onboarding::First);
        let _ = app.vault_message(VaultMsg::UseDefault);
        assert!(app.onboarding.is_none());
        assert_eq!(app.sh.vault, Some(home.path().join("vault")));
        assert!(home.path().join("vault/templates").is_dir());
    }

    #[test]
    fn the_first_run_makes_a_blank_vault_with_templates_in_the_data_folder_and_opens_the_module_manager() {
        let (mut app, home) = app_with_home();
        app.onboarding = Some(Onboarding::First);
        let _ = app.open_welcome();
        assert!(app.welcome_window.is_some());
        assert_eq!(app.welcome_target(), Some(home.path().join("vault")));
        let _ = app.vault_message(VaultMsg::WelcomeCreate);
        assert!(app.onboarding.is_none() && app.welcome_window.is_none());
        assert!(home.path().join("vault/templates").is_dir());
        assert!(
            !app.sh.notes.keys().any(|p| !p.starts_with("templates/") && !p.starts_with("library/") && p != crate::welcome::WELCOME_NOTE),
            "blank: only the templates, the starting plans and the note that teaches the editor"
        );
        assert_eq!(app.sh.notes.get(crate::welcome::WELCOME_NOTE).map(String::as_str), Some(crate::welcome::welcome_note()));
        let plans = study_core::plan::find_plans(&app.sh.notes);
        for name in ["M'Cheyne", "Bible in a year", "Chronological"] {
            assert!(plans.iter().any(|p| p.name == name && p.days.len() == 365 && p.problems.is_empty()), "{name} is in the library");
        }
        assert!(app.manager_window.is_some(), "the module manager follows");
    }

    #[test]
    fn the_welcome_note_is_written_once_when_a_vault_is_made_and_never_replaces_a_note() {
        let (mut app, _home) = app_with_home();
        let other = tempfile::tempdir().unwrap();
        let _ = app.switch_vault(other.path().into(), Options { samples: Samples::Never, templates: true, plans: true }).unwrap();
        assert!(app.sh.notes.contains_key(crate::welcome::WELCOME_NOTE));
        // A note of that name is never replaced, and none is added to a vault that is only opened.
        let mine = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(mine.path().join("notes")).unwrap();
        std::fs::write(mine.path().join(crate::welcome::WELCOME_NOTE), "mine").unwrap();
        let _ = app.switch_vault(mine.path().into(), Options { samples: Samples::Never, templates: false, plans: true }).unwrap();
        assert_eq!(app.sh.notes.get(crate::welcome::WELCOME_NOTE).map(String::as_str), Some("mine"));
        let plain = tempfile::tempdir().unwrap();
        let _ = app.switch_vault(plain.path().into(), Options { samples: Samples::Never, templates: false, plans: false }).unwrap();
        assert!(!app.sh.notes.contains_key(crate::welcome::WELCOME_NOTE));
    }

    #[test]
    fn the_first_run_vault_can_be_made_in_a_folder_picked() {
        let (mut app, home) = app_with_home();
        let folder = tempfile::tempdir().unwrap();
        app.onboarding = Some(Onboarding::First);
        let _ = app.vault_message(VaultMsg::WelcomePicked(Some(folder.path().into())));
        let _ = app.vault_message(VaultMsg::WelcomeCreate);
        assert_eq!(app.sh.vault.as_deref(), Some(folder.path()));
        assert!(folder.path().join("templates").is_dir() && !home.path().join("vault").exists());
    }

    #[test]
    fn with_no_modules_the_tabs_say_so_and_links_to_verses_open_nothing() {
        let empty = tempfile::tempdir().unwrap();
        let mut app = App::build(Box::new(crate::live::LiveBackend::open(&[empty.path().into()])), Default::default(), None);
        app.home = Some(tempfile::tempdir().unwrap().keep());
        app.no_modules = true;
        let vault = tempfile::tempdir().unwrap();
        let _ = app.switch_vault(vault.path().into(), Options { samples: Samples::Never, templates: true, plans: true }).unwrap();
        assert!(app.tabs.values().all(|t| !matches!(t, crate::Tab::Bible(_))), "no Bible tab with nothing to read");
        let blank = app.tabs.iter().find(|(_, t)| matches!(t, crate::Tab::Blank(_))).map(|(&id, _)| id).expect("a new tab");
        let _ = app.view();
        let _ = app.blank_view(blank);
        let before = app.tabs.len();
        let range = scripture::RefRange::parse("John 3:16").unwrap();
        let _ = app.open_verse(blank, range, crate::Dest::Here);
        assert_eq!(app.tabs.len(), before, "a link to a verse opens nothing");
        assert!(app.sh.toast.is_some() || !app.sh.status.is_empty());
    }

    #[test]
    fn choosing_a_folder_closes_the_settings_window_that_would_cover_the_card() {
        let (mut app, home) = app_with_home();
        let _ = app.update(Message::OpenSettings);
        assert!(app.settings_window.is_some());
        let _ = app.vault_message(VaultMsg::Picked(Mode::Open, Some(home.path().into())));
        assert!(app.settings_window.is_none() && app.setup.is_some());
    }

    #[test]
    fn the_default_vault_can_be_opened_from_the_settings_and_is_then_in_use() {
        let (mut app, home) = app_with_home();
        let other = tempfile::tempdir().unwrap();
        let none = Options { samples: Samples::Never, templates: false, plans: false };
        let _ = app.switch_vault(other.path().into(), none).unwrap();
        assert_ne!(app.sh.vault.as_deref(), Some(home.path().join("vault").as_path()));
        let _ = app.vault_message(VaultMsg::UseDefault);
        assert_eq!(app.sh.vault, Some(home.path().join("vault")), "the default vault is now the one in use");
        assert_eq!(Config::load(home.path()).vault, Some(home.path().join("vault")));
        // Its section of the settings says so.
        let _ = app.vault_settings();
    }

    #[test]
    fn cancelling_the_card_changes_nothing() {
        let (mut app, home) = app_with_home();
        app.setup = Some(Setup::new(Mode::Open, home.path().into()));
        let _ = app.vault_message(VaultMsg::Cancel);
        assert!(app.setup.is_none() && app.sh.vault.is_none());
        assert_eq!(Config::load(home.path()), Config::default());
    }

    #[test]
    fn an_unusable_folder_is_reported_on_the_card_and_nothing_changes() {
        let (mut app, home) = app_with_home();
        let file = home.path().join("a-file");
        std::fs::write(&file, "x").unwrap();
        app.setup = Some(Setup::new(Mode::Open, file));
        let _ = app.vault_message(VaultMsg::Confirm);
        assert!(app.setup.as_ref().is_some_and(|s| s.error.is_some()));
        assert_eq!(Config::load(home.path()), Config::default());
    }
}

#[cfg(test)]
mod pen_tests {
    use super::*;
    use crate::state::{PenDef, PenStyle};

    #[test]
    fn pens_stay_with_their_vault() {
        let home = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.home = Some(home.path().to_path_buf());
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let none = Options { samples: Samples::Never, templates: false, plans: false };
        let _ = app.switch_vault(a.path().into(), none).unwrap();
        assert_eq!(app.sh.settings.pens, crate::state::default_pens(), "a vault with no pens gets the starting ones");
        let mut settings = app.sh.settings.clone();
        settings.pens = vec![PenDef { tag: "wisdom".into(), color: "#4DA3FF".into(), style: PenStyle::Underline }];
        app.sh.set_settings(settings);
        assert!(a.path().join(".zoe/pens.json").exists());
        let _ = app.switch_vault(b.path().into(), none).unwrap();
        assert_eq!(app.sh.settings.pens, crate::state::default_pens(), "another vault has its own");
        let _ = app.switch_vault(a.path().into(), none).unwrap();
        assert_eq!(app.sh.settings.pens.len(), 1);
        assert_eq!(app.sh.settings.pens[0].tag, "wisdom", "and the first has its pen back");
    }

    #[test]
    fn a_vault_without_a_pens_file_takes_the_pens_in_use_at_start_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.sh.vault = Some(dir.path().into());
        app.sh.settings.pens.truncate(1);
        app.sh.load_pens(true);
        assert_eq!(app.sh.settings.pens.len(), 1);
        let kept = std::fs::read_to_string(dir.path().join(".zoe/pens.json")).unwrap();
        assert!(kept.contains(&app.sh.settings.pens[0].tag));
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    #[test]
    fn the_choices_follow_the_files_when_one_folder_becomes_config_and_data_and_a_vaults_pens_follow_its_folder() {
        let base = tempfile::tempdir().unwrap();
        let (old, config, data) = (base.path().join("old"), base.path().join("config/zoe"), base.path().join("data/zoe"));
        let elsewhere = base.path().join("Notes");
        std::fs::create_dir_all(old.join("vault")).unwrap();
        Config { vault: Some(old.join("vault")), module_folders: vec![old.join("mods"), elsewhere.clone()], persecution: false }.save(&old).unwrap();
        assert!(vault::migrate_home(&old, &config, &data));
        Config::moved(&config, &old, &data);
        let moved = Config::load(&config);
        assert_eq!(moved.vault, Some(data.join("vault")), "the default vault is where it moved to");
        assert_eq!(moved.module_folders, vec![data.join("mods"), elsewhere], "and only paths inside the old folder change");

        // A vault with pens in the old hidden folder keeps them in the new one.
        let vault = tempfile::tempdir().unwrap();
        std::fs::create_dir(vault.path().join(".bible-study")).unwrap();
        std::fs::write(vault.path().join(".bible-study/pens.json"), r##"[{"tag":"wisdom","color":"#4DA3FF","style":"underline"}]"##).unwrap();
        let mut app = App::new();
        app.sh.vault = Some(vault.path().into());
        app.sh.load_pens(false);
        assert_eq!(app.sh.settings.pens.len(), 1);
        assert!(vault.path().join(".zoe/pens.json").exists() && !vault.path().join(".bible-study").exists());
    }
}
