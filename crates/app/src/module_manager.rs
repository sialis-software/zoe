//! The module manager: a window of its own (opened from Settings, Modules) for the SWORD modules.
//!
//! It lists what is installed in every folder searched and whether the app can read it, removes
//! modules, imports a `.zip` or a folder of them into the app's own folder, exports one to a
//! `.zip`, and, only when asked, downloads the list of CrossWire's modules and installs from it.
//! The archive work is `sword::package`; the network is `module_repo`. Everything slow runs on a
//! thread of its own and reports back by message, so the window never waits.

use crate::module_repo::{self, Fetcher};
use crate::vault_setup::tidy;
use crate::{App, Message, live, native};
use iced::futures::channel::{mpsc, oneshot};
use iced::widget::{button, checkbox, column, container, pick_list, progress_bar, row, rule, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use sword::package::{self, Installed, Package, RemoteModule};

/// The most repository modules listed at once (the filter narrows the rest down).
const SHOWN: usize = 150;
const ALL_LANGUAGES: &str = "All languages";
const ALL_CATEGORIES: &str = "All kinds";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Installed,
    Repository,
}

/// What came back from a thread that reports as it goes (a download).
#[derive(Clone, Debug)]
pub enum Work {
    /// Bytes fetched so far, and the total if it is known.
    Progress(u64, Option<u64>),
    /// The download is in and being put in place.
    Installing,
    Installed(Result<Vec<String>, String>),
    Listed(Result<Vec<RemoteModule>, String>),
}

/// What a finished piece of work did.
#[derive(Clone, Debug)]
pub struct Done {
    text: String,
    /// The installed modules changed, so the app's modules are loaded again.
    changed: bool,
}

#[derive(Clone, Debug)]
pub enum ManagerMsg {
    /// Show the window (or bring it forward).
    Open,
    Page(Page),
    Loaded(Vec<Installed>),
    Query(String),
    Language(String),
    Category(String),
    FreeOnly(bool),
    AskRemove(usize),
    /// The question being asked was answered yes, or no.
    Yes,
    No,
    ImportZip,
    ImportFolder,
    ImportPicked(Option<PathBuf>),
    Inspected(Result<Package, String>),
    Export(usize),
    ExportTo { name: String, root: PathBuf, dest: Option<PathBuf> },
    Finished(Result<Done, String>),
    FetchList,
    Download(String),
    Work(Work),
    /// Stop what is being downloaded.
    Cancel,
    Dismiss,
}

fn mm(message: ManagerMsg) -> Message {
    Message::Manager(message)
}

/// A question to answer before something that cannot be taken back.
#[derive(Clone, Debug)]
enum Confirm {
    /// Delete a module. `foreign` when its folder is not the app's own.
    Remove { name: String, root: PathBuf, foreign: bool },
    /// Install over these modules that are already there.
    Replace { package: Package, names: Vec<String> },
    /// Download over the module of this name that is already there.
    Redownload(String),
}

struct Busy {
    what: String,
    progress: Option<(u64, Option<u64>)>,
    cancel: Arc<AtomicBool>,
    /// Whether it can be stopped (a download, not copying files).
    stoppable: bool,
}

struct Notice {
    text: String,
    error: bool,
}

pub struct Manager {
    page: Page,
    installed: Vec<Installed>,
    confirm: Option<Confirm>,
    busy: Option<Busy>,
    notice: Option<Notice>,
    /// CrossWire's modules, once the person has asked for the list.
    remote: Option<Vec<RemoteModule>>,
    query: String,
    language: String,
    category: String,
    free_only: bool,
    fetcher: Arc<dyn Fetcher>,
    /// Where modules are installed and which folders are listed, when not the usual ones (tests).
    target: Option<PathBuf>,
    roots: Option<Vec<PathBuf>>,
}

impl Manager {
    pub fn new() -> Manager {
        Manager {
            page: Page::Installed,
            installed: Vec::new(),
            confirm: None,
            busy: None,
            notice: None,
            remote: None,
            query: String::new(),
            language: String::new(),
            category: String::new(),
            free_only: false,
            fetcher: Arc::new(module_repo::Http),
            target: None,
            roots: None,
        }
    }
}

/// Run `work` on a thread of its own; the answer comes when it is done.
fn off_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = Result<T, String>> + Send {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    async move { rx.await.map_err(|_| "The work stopped unexpectedly".to_string()) }
}

/// Fetch `name`'s package and install it into the library at `target`, reporting progress. The
/// download is kept in a file that is gone again whatever happens.
fn fetch_and_install(
    fetcher: &dyn Fetcher,
    name: &str,
    target: &Path,
    replace: bool,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(Work),
) -> Result<Vec<String>, String> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    struct Remove(PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let url = module_repo::package_url(name).ok_or_else(|| format!("{name} is not a module name"))?;
    let file = Remove(std::env::temp_dir().join(format!("zoe-download-{}-{}-{name}.zip", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))));
    let mut out = std::fs::File::create(&file.0).map_err(|e| format!("Could not save the download: {e}"))?;
    fetcher.get(&url, &mut out, &mut |done, total| report(Work::Progress(done, total)), cancel)?;
    drop(out);
    report(Work::Installing);
    let package = Package::from_zip(&file.0).map_err(|e| e.to_string())?;
    if !package.modules.iter().any(|m| m.name.eq_ignore_ascii_case(name)) {
        return Err(format!("The package from the server does not hold {name}"));
    }
    package.install(target, replace).map_err(|e| e.to_string())
}

/// The repository's modules that match what is typed and chosen. An empty choice matches all.
fn filter_remote<'a>(list: &'a [RemoteModule], query: &str, language: &str, category: &str, free_only: bool) -> Vec<&'a RemoteModule> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    list.iter()
        .filter(|m| language.is_empty() || m.language == language)
        .filter(|m| category.is_empty() || m.category == category)
        .filter(|m| !free_only || (m.free && !m.locked))
        .filter(|m| {
            let hay = format!("{} {} {}", m.name, m.description, m.language).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect()
}

fn size_label(bytes: u64) -> String {
    match bytes {
        0 => "size unknown".into(),
        b if b < 1 << 10 => format!("{b} B"),
        b if b < 1 << 20 => format!("{:.0} KB", b as f64 / 1024.0),
        b if b < 1 << 30 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
    }
}

fn kind_label(kind: sword::Kind) -> &'static str {
    match kind {
        sword::Kind::Bible => "Bible",
        sword::Kind::Commentary => "Commentary",
        sword::Kind::Lexicon => "Dictionary",
        sword::Kind::Other => "Other",
    }
}

impl App {
    fn manager_target(&self) -> Option<PathBuf> {
        self.manager.target.clone().or_else(sword::own_folder)
    }

    fn manager_roots(&self) -> Vec<PathBuf> {
        self.manager.roots.clone().unwrap_or_else(|| {
            let added = self.config_dir().as_deref().map(crate::state::Config::load).map(|c| c.module_folders).unwrap_or_default();
            live::module_roots(&added)
        })
    }

    /// Look at the folders again (off the thread of the window: sizes are read from disk).
    fn manager_refresh(&self) -> Task<Message> {
        let roots = self.manager_roots();
        Task::perform(off_thread(move || package::list_installed(&roots)), |found| mm(ManagerMsg::Loaded(found.unwrap_or_default())))
    }

    fn manager_fail(&mut self, text: impl Into<String>) {
        self.manager.notice = Some(Notice { text: text.into(), error: true });
    }

    /// Start something that is done on a thread and answers once.
    fn manager_work(&mut self, what: String, work: impl FnOnce() -> Result<Done, String> + Send + 'static) -> Task<Message> {
        self.manager.busy = Some(Busy { what, progress: None, cancel: Arc::default(), stoppable: false });
        self.manager.notice = None;
        Task::perform(off_thread(work), |done| mm(ManagerMsg::Finished(done.and_then(|d| d))))
    }

    /// Start something that is done on a thread and reports as it goes.
    fn manager_stream(&mut self, what: String, work: impl FnOnce(&AtomicBool, &mut dyn FnMut(Work)) -> Work + Send + 'static) -> Task<Message> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.manager.busy = Some(Busy { what, progress: None, cancel: cancel.clone(), stoppable: true });
        self.manager.notice = None;
        let (tx, rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let last = work(&cancel, &mut |event| {
                let _ = tx.unbounded_send(event);
            });
            let _ = tx.unbounded_send(last);
        });
        Task::run(rx, |event| mm(ManagerMsg::Work(event)))
    }

    fn manager_install(&mut self, package: Package, replace: bool) -> Task<Message> {
        let Some(target) = self.manager_target() else {
            self.manager_fail("There is no folder of the app's own to install into");
            return Task::none();
        };
        let what = format!("Installing {}\u{2026}", package.modules.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", "));
        self.manager_work(what, move || {
            let names = package.install(&target, replace).map_err(|e| e.to_string())?;
            Ok(Done { text: format!("Installed {}", names.join(", ")), changed: true })
        })
    }

    fn manager_download(&mut self, name: String, replace: bool) -> Task<Message> {
        let Some(target) = self.manager_target() else {
            self.manager_fail("There is no folder of the app's own to install into");
            return Task::none();
        };
        let fetcher = self.manager.fetcher.clone();
        self.manager_stream(format!("Downloading {name}\u{2026}"), move |cancel, report| {
            Work::Installed(fetch_and_install(&*fetcher, &name, &target, replace, cancel, report))
        })
    }

    pub fn manager_message(&mut self, message: ManagerMsg) -> Task<Message> {
        // One thing at a time: what starts work waits for the work in hand.
        let starts = matches!(
            message,
            ManagerMsg::ImportZip | ManagerMsg::ImportFolder | ManagerMsg::Export(_) | ManagerMsg::FetchList | ManagerMsg::Download(_) | ManagerMsg::AskRemove(_)
        );
        if starts && self.manager.busy.is_some() {
            return Task::none();
        }
        match message {
            ManagerMsg::Open => {
                if let Some(id) = self.manager_window {
                    return iced::window::gain_focus(id);
                }
                // A fixed-size accessory of the workspace window, like the settings (`native::attach`).
                let (id, open) = iced::window::open(iced::window::Settings {
                    size: iced::Size::new(780.0, 640.0),
                    resizable: false,
                    minimizable: false,
                    exit_on_close_request: false,
                    ..iced::window::Settings::default()
                });
                self.manager_window = Some(id);
                let main = self.main;
                Task::batch([open.then(move |_| native::attach(main, id)).discard(), self.manager_refresh()])
            }
            ManagerMsg::Page(page) => {
                self.manager.page = page;
                Task::none()
            }
            ManagerMsg::Loaded(found) => {
                self.manager.installed = found;
                Task::none()
            }
            ManagerMsg::Query(q) => {
                self.manager.query = q;
                Task::none()
            }
            ManagerMsg::Language(l) => {
                self.manager.language = if l == ALL_LANGUAGES { String::new() } else { l };
                Task::none()
            }
            ManagerMsg::Category(c) => {
                self.manager.category = if c == ALL_CATEGORIES { String::new() } else { c };
                Task::none()
            }
            ManagerMsg::FreeOnly(on) => {
                self.manager.free_only = on;
                Task::none()
            }
            ManagerMsg::AskRemove(i) => {
                if let Some(m) = self.manager.installed.get(i) {
                    let foreign = self.manager_target().as_deref() != Some(m.root.as_path());
                    self.manager.confirm = Some(Confirm::Remove { name: m.name.clone(), root: m.root.clone(), foreign });
                }
                Task::none()
            }
            ManagerMsg::No => {
                self.manager.confirm = None;
                Task::none()
            }
            ManagerMsg::Yes => match self.manager.confirm.take() {
                Some(Confirm::Remove { name, root, .. }) => self.manager_work(format!("Removing {name}\u{2026}"), move || {
                    let freed = package::remove_module(&root, &name).map_err(|e| e.to_string())?;
                    Ok(Done { text: format!("Removed {name} ({} freed)", size_label(freed)), changed: true })
                }),
                Some(Confirm::Replace { package, .. }) => self.manager_install(package, true),
                Some(Confirm::Redownload(name)) => self.manager_download(name, true),
                None => Task::none(),
            },
            ManagerMsg::ImportZip => Task::perform(
                async {
                    let dialog = rfd::AsyncFileDialog::new().set_title("Choose a SWORD module package").add_filter("Module package", &["zip"]);
                    dialog.pick_file().await.map(|h| h.path().to_path_buf())
                },
                |path| mm(ManagerMsg::ImportPicked(path)),
            ),
            ManagerMsg::ImportFolder => Task::perform(
                async {
                    let dialog = rfd::AsyncFileDialog::new().set_title("Choose a folder with mods.d and modules in it");
                    dialog.pick_folder().await.map(|h| h.path().to_path_buf())
                },
                |path| mm(ManagerMsg::ImportPicked(path)),
            ),
            ManagerMsg::ImportPicked(None) => Task::none(),
            ManagerMsg::ImportPicked(Some(path)) => {
                self.manager.busy = Some(Busy { what: "Looking through it\u{2026}".into(), progress: None, cancel: Arc::default(), stoppable: false });
                self.manager.notice = None;
                let look = move || (if path.is_dir() { Package::from_dir(&path) } else { Package::from_zip(&path) }).map_err(|e| e.to_string());
                Task::perform(off_thread(look), |found| mm(ManagerMsg::Inspected(found.and_then(|f| f))))
            }
            ManagerMsg::Inspected(Err(e)) => {
                self.manager.busy = None;
                self.manager_fail(e);
                Task::none()
            }
            ManagerMsg::Inspected(Ok(package)) => {
                self.manager.busy = None;
                let Some(target) = self.manager_target() else {
                    self.manager_fail("There is no folder of the app's own to install into");
                    return Task::none();
                };
                let names = package.conflicts(&target);
                if names.is_empty() {
                    self.manager_install(package, false)
                } else {
                    self.manager.confirm = Some(Confirm::Replace { package, names });
                    Task::none()
                }
            }
            ManagerMsg::Export(i) => {
                let Some(m) = self.manager.installed.get(i) else { return Task::none() };
                let (name, root) = (m.name.clone(), m.root.clone());
                let file = format!("{name}.zip");
                Task::perform(
                    async move {
                        let dialog = rfd::AsyncFileDialog::new().set_title("Export module").set_file_name(file).add_filter("Module package", &["zip"]);
                        let dest = dialog.save_file().await.map(|h| h.path().to_path_buf());
                        (name, root, dest)
                    },
                    |(name, root, dest)| mm(ManagerMsg::ExportTo { name, root, dest }),
                )
            }
            ManagerMsg::ExportTo { dest: None, .. } => Task::none(),
            ManagerMsg::ExportTo { name, root, dest: Some(dest) } => {
                let dest = if dest.extension().is_some() { dest } else { dest.with_extension("zip") };
                self.manager_work(format!("Exporting {name}\u{2026}"), move || {
                    package::export_module(&root, &name, &dest).map_err(|e| e.to_string())?;
                    Ok(Done { text: format!("Exported {name} to {}", tidy(&dest)), changed: false })
                })
            }
            ManagerMsg::Finished(result) => {
                self.manager.busy = None;
                match result {
                    Ok(done) => {
                        self.manager.notice = Some(Notice { text: done.text, error: false });
                        if done.changed {
                            return Task::batch([self.reload_modules(), self.manager_refresh()]);
                        }
                    }
                    Err(e) => self.manager_fail(e),
                }
                Task::none()
            }
            ManagerMsg::FetchList => {
                let fetcher = self.manager.fetcher.clone();
                self.manager_stream("Downloading the module list\u{2026}".into(), move |cancel, report| {
                    let mut bytes = Vec::new();
                    let fetched = fetcher.get(module_repo::LIST_URL, &mut bytes, &mut |done, total| report(Work::Progress(done, total)), cancel);
                    Work::Listed(fetched.and_then(|()| package::parse_listing(&bytes).map_err(|e| e.to_string())))
                })
            }
            ManagerMsg::Download(name) => {
                let own = self.manager_target();
                let there = self.manager.installed.iter().any(|m| m.name.eq_ignore_ascii_case(&name) && Some(&m.root) == own.as_ref());
                if there {
                    self.manager.confirm = Some(Confirm::Redownload(name));
                    return Task::none();
                }
                self.manager_download(name, false)
            }
            ManagerMsg::Work(Work::Progress(done, total)) => {
                if let Some(busy) = &mut self.manager.busy {
                    busy.progress = Some((done, total));
                }
                Task::none()
            }
            ManagerMsg::Work(Work::Installing) => {
                if let Some(busy) = &mut self.manager.busy {
                    busy.what = "Installing\u{2026}".into();
                    busy.progress = None;
                    busy.stoppable = false;
                }
                Task::none()
            }
            ManagerMsg::Work(Work::Listed(result)) => {
                self.manager.busy = None;
                match result {
                    Ok(list) => self.manager.remote = Some(list),
                    Err(e) => self.manager_fail(if e == "Cancelled" { "Cancelled".to_string() } else { format!("Could not get the list: {e}") }),
                }
                Task::none()
            }
            ManagerMsg::Work(Work::Installed(result)) => {
                self.manager.busy = None;
                match result {
                    Ok(names) => {
                        self.manager.notice = Some(Notice { text: format!("Installed {}", names.join(", ")), error: false });
                        Task::batch([self.reload_modules(), self.manager_refresh()])
                    }
                    Err(e) => {
                        self.manager_fail(e);
                        Task::none()
                    }
                }
            }
            ManagerMsg::Cancel => {
                if let Some(busy) = &self.manager.busy {
                    busy.cancel.store(true, Ordering::Relaxed);
                }
                Task::none()
            }
            ManagerMsg::Dismiss => {
                self.manager.notice = None;
                Task::none()
            }
        }
    }

    /// The window: two pages, a line for what is going on, and the page.
    pub fn manager_panel(&self) -> Element<'_, Message> {
        let m = &self.manager;
        let tab = |label: &'static str, page: Page| {
            let on = m.page == page;
            button(container(text(label).size(14)).center_x(Length::Fill).padding([6, 0]))
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
                .on_press(mm(ManagerMsg::Page(page)))
        };
        let mut top = column![container(row![tab("Installed", Page::Installed), tab("CrossWire", Page::Repository)].spacing(2)).padding([10, 14]), rule::horizontal(1)];
        let mut status = column![].spacing(8);
        if let Some(busy) = &m.busy {
            let mut line = row![text(busy.what.clone()).size(13).width(Length::Fill)].spacing(10).align_y(iced::Center);
            if let Some((done, total)) = busy.progress {
                line = line.push(text(match total {
                    Some(t) => format!("{} of {}", size_label(done), size_label(t)),
                    None => size_label(done),
                })
                .size(12)
                .style(text::secondary));
            }
            if busy.stoppable {
                line = line.push(button(text("Cancel").size(12)).padding([3, 10]).style(button::secondary).on_press(mm(ManagerMsg::Cancel)));
            }
            let fraction = busy.progress.and_then(|(d, t)| t.filter(|t| *t > 0).map(|t| d as f32 / t as f32));
            let bar = progress_bar(0.0..=1.0, fraction.unwrap_or(0.0)).girth(4);
            status = status.push(column![line, bar].spacing(4));
        }
        if let Some(n) = &m.notice {
            let style = if n.error { text::danger } else { text::success };
            status = status.push(
                row![
                    text(n.text.clone()).size(13).style(style).width(Length::Fill),
                    button(text("\u{00D7}").size(14)).padding([0, 6]).style(button::text).on_press(mm(ManagerMsg::Dismiss)),
                ]
                .align_y(iced::Center),
            );
        }
        if let Some(c) = &m.confirm {
            status = status.push(self.confirm_card(c));
        }
        let page = match m.page {
            Page::Installed => self.installed_page(),
            Page::Repository => self.repository_page(),
        };
        top = top.push(container(status).padding(iced::Padding { top: 10.0, right: 14.0, bottom: 0.0, left: 14.0 }));
        top.push(scrollable(container(page).padding(14).width(Length::Fill)).height(Length::Fill)).into()
    }

    fn confirm_card(&self, confirm: &Confirm) -> Element<'_, Message> {
        let (question, yes, danger) = match confirm {
            Confirm::Remove { name, foreign: false, .. } => (format!("Delete {name} from the app's folder? Its files are removed from the disk."), "Delete".to_string(), true),
            Confirm::Remove { name, root, foreign: true } => (
                format!(
                    "{name} is in {}, which is not the app's own folder. Other programs may use it. Delete its files from there anyway?",
                    tidy(root)
                ),
                format!("Delete from {}", tidy(root)),
                true,
            ),
            Confirm::Replace { names, .. } => (format!("{} already installed. Replace {}?", names.join(", "), if names.len() == 1 { "it" } else { "them" }), "Replace".to_string(), true),
            Confirm::Redownload(name) => (format!("{name} is already installed. Download it again and replace it?"), "Replace".to_string(), true),
        };
        let yes = button(text(yes).size(12)).padding([3, 10]).style(if danger { button::danger } else { button::primary }).on_press(mm(ManagerMsg::Yes));
        let no = button(text("Keep it").size(12)).padding([3, 10]).style(button::secondary).on_press(mm(ManagerMsg::No));
        card(column![text(question).size(13), row![yes, no].spacing(8)].spacing(8).into())
    }

    fn installed_page(&self) -> Element<'_, Message> {
        let m = &self.manager;
        let idle = m.busy.is_none();
        let act = |label: &'static str, msg: ManagerMsg| {
            let b = button(text(label).size(12)).padding([3, 10]).style(button::secondary);
            if idle { b.on_press(mm(msg)) } else { b }
        };
        let own = self.manager_target();
        let mut list = column![
            row![act("Import .zip\u{2026}", ManagerMsg::ImportZip), act("Import folder\u{2026}", ManagerMsg::ImportFolder)].spacing(8),
            text(format!("Imported modules are copied into {}.", own.as_deref().map(tidy).unwrap_or_else(|| "(no folder)".into()))).size(11).style(text::secondary),
        ]
        .spacing(8);
        if m.installed.is_empty() {
            list = list.push(text("No modules found in the folders searched.").size(13).style(text::secondary));
        }
        for (i, module) in m.installed.iter().enumerate() {
            let mut info = vec![kind_label(module.kind).to_string()];
            if !module.language.is_empty() {
                info.push(module.language.clone());
            }
            if !module.version.is_empty() {
                info.push(format!("version {}", module.version));
            }
            if !module.license.is_empty() {
                info.push(module.license.clone());
            }
            info.push(size_label(module.size));
            let mut body = column![
                text(format!("{}  {}", module.name, module.description)).size(14),
                text(info.join("  \u{00B7}  ")).size(11).style(text::secondary),
                text(format!("in {}{}", tidy(&module.root), if own.as_deref() == Some(module.root.as_path()) { "" } else { "  (not the app's own folder)" })).size(11).style(text::secondary),
            ]
            .spacing(2)
            .width(Length::Fill);
            body = body.push(match &module.unreadable {
                Some(why) => text(format!("The app cannot read it: {why}")).size(11).style(text::danger),
                None => text("The app can read it").size(11).style(text::success),
            });
            let buttons = column![act("Export\u{2026}", ManagerMsg::Export(i)), {
                let b = button(text("Remove").size(12)).padding([3, 10]).style(button::danger);
                if idle { b.on_press(mm(ManagerMsg::AskRemove(i))) } else { b }
            }]
            .spacing(4);
            list = list.push(card(row![body, buttons].spacing(10).align_y(iced::Center).into()));
        }
        list.into()
    }

    fn repository_page(&self) -> Element<'_, Message> {
        let m = &self.manager;
        let idle = m.busy.is_none();
        let mut page = column![
            text("The list of modules is only fetched when you ask, and a module only when you press Download. Nothing else uses the network.").size(11).style(text::secondary),
        ]
        .spacing(8);
        let fetch = button(text(if m.remote.is_some() { "Fetch the list again" } else { "Get the list from CrossWire" }).size(12)).padding([3, 10]).style(button::secondary);
        page = page.push(if idle { fetch.on_press(mm(ManagerMsg::FetchList)) } else { fetch });
        let Some(list) = &m.remote else { return page.into() };
        let mut languages: Vec<String> = list.iter().map(|r| r.language.clone()).filter(|l| !l.is_empty()).collect();
        languages.sort();
        languages.dedup();
        languages.insert(0, ALL_LANGUAGES.into());
        let mut categories: Vec<String> = list.iter().map(|r| r.category.clone()).collect();
        categories.sort();
        categories.dedup();
        categories.insert(0, ALL_CATEGORIES.into());
        let language = Some(if m.language.is_empty() { ALL_LANGUAGES.to_string() } else { m.language.clone() });
        let category = Some(if m.category.is_empty() { ALL_CATEGORIES.to_string() } else { m.category.clone() });
        page = page.push(
            row![
                text_input("Search by name or description", &m.query).on_input(|q| mm(ManagerMsg::Query(q))).size(13).padding([4, 8]).width(Length::Fill),
                pick_list(languages, language, |l| mm(ManagerMsg::Language(l))).text_size(13),
                pick_list(categories, category, |c| mm(ManagerMsg::Category(c))).text_size(13),
            ]
            .spacing(8),
        );
        page = page.push(checkbox(m.free_only).label("Free to download only").on_toggle(|on| mm(ManagerMsg::FreeOnly(on))).size(16).text_size(13));
        let found = filter_remote(list, &m.query, &m.language, &m.category, m.free_only);
        page = page.push(text(format!("{} of {} modules", found.len(), list.len())).size(11).style(text::secondary));
        for r in found.iter().take(SHOWN) {
            let own = self.manager_target();
            let has = m.installed.iter().any(|i| i.name.eq_ignore_ascii_case(&r.name) && Some(&i.root) == own.as_ref());
            let mut info = vec![r.category.clone()];
            if !r.language.is_empty() {
                info.push(r.language.clone());
            }
            if !r.version.is_empty() {
                info.push(format!("version {}", r.version));
            }
            info.push(size_label(r.size));
            if !r.license.is_empty() {
                info.push(r.license.clone());
            }
            let mut body = column![
                row![
                    text(format!("{}  {}", r.name, r.description)).size(14),
                    if r.locked {
                        text("Locked: needs a key").size(11).style(text::danger)
                    } else if r.free {
                        text("Free to download").size(11).style(text::success)
                    } else {
                        text("Not free").size(11).style(text::secondary)
                    },
                ]
                .spacing(8)
                .align_y(iced::Center),
                text(info.join("  \u{00B7}  ")).size(11).style(text::secondary),
            ]
            .spacing(2)
            .width(Length::Fill);
            if let Some(why) = &r.unreadable {
                body = body.push(text(format!("The app could not read it yet: {why}")).size(11).style(text::secondary));
            }
            let label = if has { "Download again" } else { "Download" };
            let download = button(text(label).size(12)).padding([3, 10]).style(button::primary);
            let download = if idle { download.on_press(mm(ManagerMsg::Download(r.name.clone()))) } else { download };
            page = page.push(card(row![body, download].spacing(10).align_y(iced::Center).into()));
        }
        if found.len() > SHOWN {
            page = page.push(text(format!("{} more: narrow the search to see them.", found.len() - SHOWN)).size(12).style(text::secondary));
        }
        page.into()
    }
}

fn card(content: Element<'_, Message>) -> Element<'_, Message> {
    container(content)
        .padding(10)
        .width(Length::Fill)
        .style(|theme: &Theme| {
            let p = theme.extended_palette();
            container::Style {
                background: Some(p.background.weak.color.into()),
                border: iced::Border { color: iced::Color { a: 0.5, ..p.background.strong.color }, width: 1.0, radius: 8.0.into() },
                ..container::Style::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn conf(name: &str, extra: &str) -> String {
        format!("[{name}]\nDescription={name} text\nModDrv=zText\nSourceType=OSIS\nCompressType=ZIP\nDataPath=./modules/texts/ztext/{}/\n{extra}", name.to_lowercase())
    }

    /// A package of one module, as CrossWire serves them.
    fn package_bytes(name: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(format!("mods.d/{}.conf", name.to_lowercase()), options).unwrap();
        zip.write_all(conf(name, "Lang=en\n").as_bytes()).unwrap();
        zip.start_file(format!("modules/texts/ztext/{}/nt.bzv", name.to_lowercase()), options).unwrap();
        zip.write_all(b"verses").unwrap();
        zip.finish().unwrap().into_inner()
    }

    /// Serves what it was given, and counts the asks. Never the network.
    struct Fake {
        serves: Vec<(String, Vec<u8>)>,
        asked: std::sync::Mutex<Vec<String>>,
    }

    impl Fetcher for Fake {
        fn get(&self, url: &str, out: &mut dyn Write, progress: &mut dyn FnMut(u64, Option<u64>), cancel: &AtomicBool) -> Result<(), String> {
            self.asked.lock().unwrap().push(url.to_string());
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let (_, bytes) = self.serves.iter().find(|(u, _)| u == url).ok_or("It is not on the server (404)")?;
            out.write_all(bytes).map_err(|e| e.to_string())?;
            progress(bytes.len() as u64, Some(bytes.len() as u64));
            Ok(())
        }
    }

    fn fake(name: &str) -> Fake {
        Fake { serves: vec![(module_repo::package_url(name).unwrap(), package_bytes(name))], asked: Default::default() }
    }

    fn remote(name: &str, language: &str, category: &str, free: bool) -> RemoteModule {
        RemoteModule {
            name: name.into(),
            description: format!("{name} Bible"),
            language: language.into(),
            version: "1".into(),
            size: 10,
            license: String::new(),
            category: category.into(),
            locked: false,
            free,
            unreadable: None,
        }
    }

    #[test]
    fn the_list_is_narrowed_by_words_language_kind_and_freedom() {
        let list = vec![
            remote("KJV", "en", "Biblical Texts", true),
            remote("LUT", "de", "Biblical Texts", true),
            remote("MHC", "en", "Commentaries", true),
            remote("ESV", "en", "Biblical Texts", false),
        ];
        let names = |q: &str, l: &str, c: &str, f: bool| filter_remote(&list, q, l, c, f).iter().map(|m| m.name.clone()).collect::<Vec<_>>();
        assert_eq!(names("", "", "", false).len(), 4);
        assert_eq!(names("", "en", "Biblical Texts", false), ["KJV", "ESV"]);
        assert_eq!(names("", "en", "Biblical Texts", true), ["KJV"]);
        assert_eq!(names("lut bible", "", "", false), ["LUT"]);
        assert_eq!(names("nothing", "", "", false), Vec::<String>::new());
    }

    #[test]
    fn a_download_is_installed_and_refused_a_second_time_until_replacing_is_allowed() {
        let target = tempfile::tempdir().unwrap();
        let fetcher = fake("Ours");
        let mut events = Vec::new();
        let cancel = AtomicBool::new(false);
        let names = fetch_and_install(&fetcher, "Ours", target.path(), false, &cancel, &mut |e| events.push(e)).unwrap();
        assert_eq!(names, ["Ours"]);
        assert_eq!(std::fs::read(target.path().join("modules/texts/ztext/ours/nt.bzv")).unwrap(), b"verses");
        assert!(matches!(events.first(), Some(Work::Progress(..))) && matches!(events.last(), Some(Work::Installing)));
        assert_eq!(*fetcher.asked.lock().unwrap(), [module_repo::package_url("Ours").unwrap()]);
        // Again: refused, until it is said to replace.
        let again = fetch_and_install(&fetcher, "Ours", target.path(), false, &cancel, &mut |_| {});
        assert!(again.unwrap_err().contains("already installed"));
        fetch_and_install(&fetcher, "Ours", target.path(), true, &cancel, &mut |_| {}).unwrap();    }

    #[test]
    fn a_failed_or_cancelled_download_installs_nothing() {
        let target = tempfile::tempdir().unwrap();
        let fetcher = fake("Ours");
        let cancel = AtomicBool::new(false);
        let missing = fetch_and_install(&fetcher, "Other", target.path(), false, &cancel, &mut |_| {});
        assert!(missing.unwrap_err().contains("404"));
        cancel.store(true, Ordering::Relaxed);
        let cancelled = fetch_and_install(&fetcher, "Ours", target.path(), false, &cancel, &mut |_| {});
        assert_eq!(cancelled.unwrap_err(), "Cancelled");
        assert!(fetch_and_install(&fetcher, "../x", target.path(), false, &cancel, &mut |_| {}).is_err());
        assert!(!target.path().join("mods.d").exists());
        // A package that is not the module asked for is not installed either.
        let wrong = Fake { serves: vec![(module_repo::package_url("Ours").unwrap(), package_bytes("Theirs"))], asked: Default::default() };
        let cancel = AtomicBool::new(false);
        assert!(fetch_and_install(&wrong, "Ours", target.path(), false, &cancel, &mut |_| {}).unwrap_err().contains("does not hold"));
        assert!(!target.path().join("mods.d").exists());
    }

    fn app_with(target: &Path) -> App {
        let mut app = App::new();
        // No modules loaded: the reload that follows a change only says to restart.
        app.no_modules = true;
        app.manager.target = Some(target.to_path_buf());
        app.manager.roots = Some(vec![target.to_path_buf()]);
        app.manager.fetcher = Arc::new(Fake { serves: Vec::new(), asked: Default::default() });
        app
    }

    #[test]
    fn importing_a_package_that_is_already_there_asks_before_replacing() {
        let (target, source) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let zip = source.path().join("ours.zip");
        std::fs::write(&zip, package_bytes("Ours")).unwrap();
        let mut app = app_with(target.path());
        let package = Package::from_zip(&zip).unwrap();
        package.install(target.path(), false).unwrap();
        let _ = app.manager_message(ManagerMsg::Inspected(Ok(package)));
        assert!(matches!(&app.manager.confirm, Some(Confirm::Replace { names, .. }) if names == &["Ours"]));
        assert!(app.manager.busy.is_none(), "nothing is replaced until it is said yes");
        let _ = app.manager_message(ManagerMsg::No);
        assert!(app.manager.confirm.is_none());
        let _ = app.manager_message(ManagerMsg::Inspected(Err("not a usable module package".into())));
        assert!(app.manager.notice.as_ref().is_some_and(|n| n.error));
    }

    #[test]
    fn a_module_in_another_folder_is_only_removed_after_a_stronger_question() {
        let (own, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut app = app_with(own.path());
        let zip = other.path().join("ours.zip");
        std::fs::write(&zip, package_bytes("Ours")).unwrap();
        let library = other.path().join("lib");
        Package::from_zip(&zip).unwrap().install(&library, false).unwrap();
        Package::from_zip(&zip).unwrap().install(own.path(), false).unwrap();
        app.manager.roots = Some(vec![own.path().into(), library.clone()]);
        app.manager.installed = package::list_installed(&[own.path().into(), library.clone()]);
        let _ = app.manager_message(ManagerMsg::AskRemove(0));
        assert!(matches!(app.manager.confirm, Some(Confirm::Remove { foreign: false, .. })));
        let _ = app.manager_message(ManagerMsg::AskRemove(1));
        assert!(matches!(&app.manager.confirm, Some(Confirm::Remove { foreign: true, root, .. }) if *root == library));
        assert!(library.join("mods.d/ours.conf").exists(), "asking removes nothing");
    }

    #[test]
    fn a_confirmed_removal_deletes_the_module_and_the_list_is_read_again() {
        let own = tempfile::tempdir().unwrap();
        let mut app = app_with(own.path());
        let zip = own.path().join("..").join(format!("zoe-test-{}.zip", std::process::id()));
        std::fs::write(&zip, package_bytes("Ours")).unwrap();
        Package::from_zip(&zip).unwrap().install(own.path(), false).unwrap();
        let _ = std::fs::remove_file(&zip);
        app.manager.installed = package::list_installed(&[own.path().into()]);
        assert_eq!(app.manager.installed.len(), 1);
        let _ = app.manager_message(ManagerMsg::AskRemove(0));
        let _ = app.manager_message(ManagerMsg::Yes);
        assert!(app.manager.busy.is_some() && app.manager.confirm.is_none());
        // The worker ran on its own thread; wait for it to have finished its job.
        for _ in 0..200 {
            if !own.path().join("mods.d/ours.conf").exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!own.path().join("mods.d/ours.conf").exists());
        assert!(!own.path().join("modules/texts/ztext/ours").exists());
        let _ = app.manager_message(ManagerMsg::Finished(Ok(Done { text: "Removed".into(), changed: true })));
        assert!(app.manager.busy.is_none());
        assert!(app.manager.notice.as_ref().is_some_and(|n| !n.error));
    }

    #[test]
    fn nothing_new_starts_while_work_is_going_on() {
        let target = tempfile::tempdir().unwrap();
        let mut app = app_with(target.path());
        app.manager.busy = Some(Busy { what: "x".into(), progress: None, cancel: Arc::default(), stoppable: false });
        let _ = app.manager_message(ManagerMsg::Download("Ours".into()));
        assert!(app.manager.confirm.is_none() && app.manager.busy.as_ref().is_some_and(|b| b.what == "x"));
    }

    #[test]
    fn the_window_is_opened_once_and_forgotten_when_closed() {
        let mut app = app_with(&std::env::temp_dir().join("zoe-manager-none"));
        assert!(app.manager_window.is_none());
        let _ = app.manager_message(ManagerMsg::Open);
        let id = app.manager_window.expect("a window was opened");
        let _ = app.manager_message(ManagerMsg::Open);
        assert_eq!(app.manager_window, Some(id));
        let _ = app.update(Message::CloseRequested(id));
        assert_eq!(app.manager_window, None);
    }
}
