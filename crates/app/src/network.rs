//! Persecution mode: everything that could use the network is off. The app's only network client
//! is `module_repo::Http`, and the only way it hands an address to another program is
//! `native::open_external` (a browser or a mail program); both ask here first, so that no part of
//! the app can forget to. The setting is kept in the config (`Config::persecution`), is read before
//! anything else at start-up, and `ZOE_PERSECUTION=1` turns it on whatever the config says.

use std::sync::atomic::{AtomicBool, Ordering};

static BLOCKED: AtomicBool = AtomicBool::new(false);

/// Why something was not done.
pub const WHY: &str = "Persecution mode is on: the network is not used";

pub fn blocked() -> bool {
    BLOCKED.load(Ordering::Relaxed)
}

pub fn set_blocked(on: bool) {
    BLOCKED.store(on, Ordering::Relaxed);
}

/// Whether the environment asks for it.
pub fn forced_by_environment() -> bool {
    std::env::var_os("ZOE_PERSECUTION").is_some_and(|v| !v.is_empty() && v != "0")
}

use crate::state::Config;
use crate::{App, Message};
use iced::Element;
use iced::widget::{checkbox, column, text};

impl App {
    /// Turn the mode on or off, and keep the choice.
    pub fn set_persecution(&mut self, on: bool) {
        if !on && forced_by_environment() {
            self.sh.toast("ZOE_PERSECUTION is set in the environment, which keeps the mode on");
            return;
        }
        set_blocked(on);
        if let Some(dir) = self.config_dir() {
            let mut config = Config::load(&dir);
            config.persecution = on;
            if let Err(e) = config.save(&dir) {
                self.sh.status = format!("Could not keep the choice: {e}");
            }
        }
        self.sh.toast(if on { "Persecution mode is on" } else { "Persecution mode is off" });
    }

    /// The switch, with what it does under it (the settings and the first run's window).
    pub fn persecution_switch(&self) -> Element<'_, Message> {
        let faint = |t: &'static str| text(t).size(11).style(text::secondary);
        column![
            checkbox(blocked()).label("Persecution mode").on_toggle(Message::SetPersecution).size(16).text_size(13),
            faint("Turns off everything that uses the network: downloading modules, and opening web and mail links."),
            faint("Modules can still be added from files, and nothing else about the app changes."),
        ]
        .spacing(3)
        .into()
    }

    pub fn privacy_settings(&self) -> Element<'_, Message> {
        column![
            self.persecution_switch(),
            text("Without it, the app uses the network only when you ask: to get the list of modules or a module from CrossWire, or to open a link in your browser. It never checks for updates or sends anything about you.")
                .size(11)
                .style(text::secondary),
        ]
        .spacing(10)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_repo::{Fetcher, Http};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn persecution_mode_stops_the_network_client_and_the_opening_of_links_and_is_kept() {
        let home = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.home = Some(home.path().into());
        let _ = app.update(Message::SetPersecution(true));
        assert!(blocked());
        let (mut out, mut report, cancel) = (Vec::new(), |_: u64, _: Option<u64>| {}, AtomicBool::new(false));
        assert_eq!(Http.get("https://example.invalid/x", &mut out, &mut report, &cancel), Err(WHY.to_string()), "refused before any connection is tried");
        assert!(!crate::native::open_external("https://example.com"));
        assert!(Config::load(home.path()).persecution, "kept for the next run");
        let _ = app.update(Message::SetPersecution(false));
        assert!(!blocked() && !Config::load(home.path()).persecution);
        let _ = app.privacy_settings();
    }
}
