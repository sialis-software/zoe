//! The folders Bible modules are loaded from: the Bible modules section of the settings.
//!
//! The usual places (`~/.config/sword` first) are always searched; the person can add more. A
//! folder is checked for modules before it is added, and the backend is rebuilt in place so the
//! modules are there at once; what is open is put back on the new backend.

use crate::live::{self, LiveBackend};
use crate::state::Config;
use crate::vault_setup::tidy;
use crate::{App, Message};
use iced::widget::{button, column, row, text};
use iced::{Element, Length, Task};
use std::path::PathBuf;
use study_core::StudyBackend;

#[derive(Clone, Debug)]
pub enum ModulesMsg {
    /// Ask for a folder (a system dialog).
    Add,
    Picked(Option<PathBuf>),
    /// Stop using the folder added at this place in the list.
    Remove(usize),
}

impl App {
    pub fn modules_message(&mut self, message: ModulesMsg) -> Task<Message> {
        match message {
            ModulesMsg::Add => Task::perform(
                async {
                    let dialog = rfd::AsyncFileDialog::new().set_title("Choose a folder of SWORD modules");
                    dialog.pick_folder().await.map(|handle| handle.path().to_path_buf())
                },
                |folder| Message::Modules(ModulesMsg::Picked(folder)),
            ),
            ModulesMsg::Picked(folder) => {
                let Some(folder) = folder else { return Task::none() };
                let found = LiveBackend::open(std::slice::from_ref(&folder));
                if found.modules().is_empty() && found.dictionaries().is_empty() {
                    self.sh.toast(format!("No SWORD modules in {} (it needs a mods.d folder)", tidy(&folder)));
                    return Task::none();
                }
                let (bibles, dictionaries) = (found.modules().len(), found.dictionaries().len());
                self.change_folders(
                    |folders| {
                        if !folders.contains(&folder) {
                            folders.push(folder.clone());
                        }
                    },
                    format!("Added {bibles} Bible{} and {dictionaries} dictionar{}", plural(bibles, "", "s"), plural(dictionaries, "y", "ies")),
                )
            }
            ModulesMsg::Remove(i) => self.change_folders(
                |folders| {
                    if i < folders.len() {
                        folders.remove(i);
                    }
                },
                "Folder removed".into(),
            ),
        }
    }

    /// Change the folders added, keep them, and load the modules again. If that would leave no
    /// Bible at all, nothing changes.
    fn change_folders(&mut self, change: impl FnOnce(&mut Vec<PathBuf>), done: String) -> Task<Message> {
        let Some(home) = self.config_dir() else { return Task::none() };
        let mut config = Config::load(&home);
        let before = config.module_folders.clone();
        change(&mut config.module_folders);
        let backend = LiveBackend::open(&live::module_roots(&config.module_folders));
        if backend.modules().is_empty() && !self.no_modules {
            self.sh.toast("That would leave no Bible modules");
            return Task::none();
        }
        if let Err(e) = config.save(&home) {
            self.sh.status = format!("Could not save the folders: {e}");
            return Task::none();
        }
        if config.module_folders == before {
            return Task::none();
        }
        if self.no_modules {
            return self.leave_sample_text(backend, "Saved: restart the app to use these modules");
        }
        let task = self.install_backend(backend);
        self.sh.toast(done);
        task
    }

    /// Load the modules again from the folders searched, in place: after one was installed or
    /// removed. What is open is put back on the new modules; a tab whose module is gone is dropped.
    pub fn reload_modules(&mut self) -> Task<Message> {
        let folders = self.config_dir().as_deref().map(Config::load).map(|c| c.module_folders).unwrap_or_default();
        let backend = LiveBackend::open(&live::module_roots(&folders));
        if self.no_modules {
            return self.leave_sample_text(backend, "Restart the app to use these modules");
        }
        self.install_backend(backend)
    }

    /// The app is on sample text because it found no modules. With a vault open, the first modules
    /// found take over in place; with none (the app started on sample text) it has to start over.
    fn leave_sample_text(&mut self, backend: LiveBackend, restart: &str) -> Task<Message> {
        if self.sh.vault.is_none() || backend.modules().is_empty() {
            if !backend.modules().is_empty() {
                self.sh.toast(restart.to_string());
            }
            return Task::none();
        }
        self.no_modules = false;
        // What was open was sample text: start from the default layout on the real modules.
        self.sh.backend = Box::new(backend);
        self.sh.forget_modules();
        let workspace = self.default_workspace();
        let task = self.apply_workspace(&workspace).unwrap_or_else(|e| {
            self.sh.status = e;
            Task::none()
        });
        self.sh.toast("Modules loaded");
        task
    }

    /// Swap the modules under what is open, and put the workspace back on them.
    fn install_backend(&mut self, backend: LiveBackend) -> Task<Message> {
        let open: Vec<_> = self.tabs.keys().copied().collect();
        for id in open {
            if let Some(crate::Tab::Bible(tab)) = self.tabs.get_mut(&id) {
                tab.stop_edit(&mut self.sh);
            }
        }
        let workspace = self.workspace();
        self.sh.backend = Box::new(backend);
        self.sh.forget_modules();
        self.apply_workspace(&workspace).unwrap_or_else(|e| {
            self.sh.status = e;
            Task::none()
        })
    }

    /// The folders searched, the ones added (which can be taken away) and what was found.
    pub fn modules_settings(&self) -> Element<'_, Message> {
        let faint = |t: String| text(t).size(11).style(text::secondary);
        let added = self.config_dir().as_deref().map(Config::load).map(|c| c.module_folders).unwrap_or_default();
        let mut list = column![].spacing(3);
        for (i, folder) in added.iter().enumerate() {
            list = list.push(
                row![
                    text(tidy(folder)).size(12).width(Length::Fill),
                    button(text("\u{00D7}").size(14)).padding([0, 6]).style(button::text).on_press(Message::Modules(ModulesMsg::Remove(i))),
                ]
                .align_y(iced::Center),
            );
        }
        for root in sword::default_roots().iter().filter(|r| !added.contains(r)) {
            list = list.push(text(format!("{}  (usual place)", tidy(root))).size(12).style(text::secondary));
        }
        let names = |modules: Vec<study_core::ModuleInfo>| modules.into_iter().map(|m| m.abbreviation).collect::<Vec<_>>().join(", ");
        let (bibles, dictionaries) = (self.sh.backend.modules(), self.sh.backend.dictionaries());
        let found = if self.no_modules {
            "No modules found yet.".to_string()
        } else {
            let or_none = |list: String| if list.is_empty() { "none".to_string() } else { list };
            format!("Bibles: {}. Dictionaries: {}. Commentaries: {}.", names(bibles), or_none(names(dictionaries)), or_none(names(self.sh.backend.commentaries())))
        };
        column![
            faint("SWORD modules are looked for in these folders, in this order.".into()),
            list,
            button(text("Add folder\u{2026}").size(12)).padding([3, 10]).style(button::secondary).on_press(Message::Modules(ModulesMsg::Add)),
            faint(found),
        ]
        .spacing(6)
        .into()
    }
}

fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_the_person_added_are_searched_first_and_each_place_once() {
        if sword::path_override().is_some() {
            return eprintln!("skipped: the search path is set by the environment");
        }
        let mine = PathBuf::from("/somewhere/modules");
        let usual = sword::default_roots();
        let roots = live::module_roots(&[mine.clone(), usual[0].clone()]);
        assert_eq!(roots[0], mine);
        assert_eq!(roots[1], usual[0], "a usual place that was added too keeps the place it was added at");
        assert_eq!(roots.len(), usual.len() + 1, "and is not listed twice");
    }

    #[test]
    fn the_apps_own_folder_is_the_only_usual_place() {
        if sword::path_override().is_some() || std::env::var_os("SWORD_PATH").is_some() {
            return eprintln!("skipped: the search path is set by the environment");
        }
        let roots = sword::default_roots();
        assert_eq!(Some(&roots[0]), sword::own_folder().as_ref(), "{roots:?}");
        assert!(roots.iter().all(|r| !r.ends_with(".sword") && !r.starts_with("/usr/share")), "no other frontend's folder");
    }

    #[test]
    fn a_folder_without_modules_is_refused_and_nothing_is_kept() {
        let mut app = App::new();
        let home = tempfile::tempdir().unwrap();
        app.home = Some(home.path().into());
        let empty = tempfile::tempdir().unwrap();
        let _ = app.modules_message(ModulesMsg::Picked(Some(empty.path().into())));
        assert!(app.sh.toast.as_ref().is_some_and(|t| t.text.contains("No SWORD modules")));
        assert_eq!(Config::load(home.path()), Config::default());
    }

    #[test]
    fn choosing_a_vault_keeps_the_module_folders() {
        let mut app = App::new();
        let home = tempfile::tempdir().unwrap();
        app.home = Some(home.path().into());
        Config { module_folders: vec!["/mods".into()], ..Config::default() }.save(home.path()).unwrap();
        let vault = tempfile::tempdir().unwrap();
        let _ = app.switch_vault(vault.path().into(), live::Options { samples: live::Samples::Never, templates: false, plans: false }).unwrap();
        let config = Config::load(home.path());
        assert_eq!(config.module_folders, vec![PathBuf::from("/mods")]);
        assert_eq!(config.vault.as_deref(), Some(vault.path()));
    }
}
