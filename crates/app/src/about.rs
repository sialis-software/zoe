//! The About page (a tab of the settings window, and the text the native About panel shows on a
//! Mac): what the app is, whose work it carries, and the libraries it is built from.

use crate::{App, Message};
use iced::widget::{column, container, row, svg, text};
use iced::{Element, Length};

/// The libraries the program is built from, with their licences (`scripts/third-party.sh`).
const LIBRARIES: &str = include_str!("../assets/third-party.txt");

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whose work comes with the app, as (what, who and on what terms).
const CREDITS: [(&str, &str); 5] = [
    (
        "Bible text",
        "The Berean Standard Bible (BSB), from berean.bible, with Strong's numbers: dedicated to the public domain (CC0) by the Berean Bible Translation Committee, Bible Hub, Discovery Bible, OpenBible.com and others.",
    ),
    ("Strong's dictionaries", "Strong's Greek and Hebrew dictionaries, by James Strong (1890): public domain."),
    (
        "SWORD modules",
        "The module format, and the library the app downloads more modules from, are the work of the SWORD Project and the CrossWire Bible Society (crosswire.org).",
    ),
    (
        "Reading plans",
        "The M'Cheyne plan is Robert Murray M'Cheyne's calendar of 1842: public domain. \"Bible in a year\" was made for this app.",
    ),
    (
        "Software",
        "Zoe is built in Rust with iced, wgpu, cosmic-text, winit, rusqlite (SQLite) and many other free libraries, listed below with their licences.",
    ),
];

/// The credits as plain text, for the system's About panel.
pub fn credits_text() -> String {
    CREDITS.iter().map(|(what, who)| format!("{what}\n{who}")).collect::<Vec<_>>().join("\n\n")
}

impl App {
    pub fn about_page(&self) -> Element<'_, Message> {
        let faint = |t: String| text(t).size(12).style(text::secondary);
        let mark = svg(svg::Handle::from_memory(include_bytes!("../assets/app/zoe.svg").as_slice())).width(48).height(48);
        let head = row![
            mark,
            column![text("Zoe").size(24), faint(format!("Version {}", version())), faint("Bible study with SWORD modules and Markdown notes".into())].spacing(2),
        ]
        .spacing(14)
        .align_y(iced::Center);
        let mut credits = column![].spacing(10);
        for (what, who) in CREDITS {
            credits = credits.push(column![text(what).size(13), faint(who.into())].spacing(2));
        }
        let libraries = container(text(LIBRARIES).size(11).font(iced::Font::MONOSPACE).style(text::secondary)).width(Length::Fill);
        column![
            head,
            credits,
            column![text("Libraries and their licences").size(13), libraries].spacing(4),
        ]
        .spacing(18)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credits_name_the_bundled_work_and_the_libraries_are_listed_with_their_licences() {
        let text = credits_text();
        for name in ["Berean Standard Bible", "Strong", "CrossWire", "M'Cheyne"] {
            assert!(text.contains(name), "{name}");
        }
        assert!(LIBRARIES.lines().any(|l| l.starts_with("iced ")) && LIBRARIES.lines().any(|l| l.starts_with("cosmic-text ")));
        assert!(LIBRARIES.lines().all(|l| l.split("  ").count() == 2), "each line is a name and version, then a licence");
    }
}
