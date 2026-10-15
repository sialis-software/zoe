//! Where a link opens, and what a link is, independent of which kind of tab it came from.
//!
//! A link is followed *here* by default. Holding a key when clicking, or choosing from the
//! link's right-click menu, sends it elsewhere:
//!
//! | Click | Opens |
//! |---|---|
//! | plain | here: in this tab (a note's verse links go to its Bible tab) |
//! | ⌘/Ctrl | in a new tab beside this one |
//! | Alt | in the other pane (its tab if it has one of the right kind; the next pane in order) |
//! | Shift | in a new pane, split off this one |

use iced::keyboard::Modifiers;
use scripture::RefRange;
use study_core::markdown::LinkTarget;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dest {
    #[default]
    Here,
    NewTab,
    OtherPane,
    NewPane,
}

impl Dest {
    /// What the keys held down at a click ask for. When several are held the one that goes
    /// furthest wins: a new pane, then the other pane, then a new tab.
    pub fn from_modifiers(m: Modifiers) -> Dest {
        if m.shift() {
            Dest::NewPane
        } else if m.alt() {
            Dest::OtherPane
        } else if m.command() {
            Dest::NewTab
        } else {
            Dest::Here
        }
    }

    pub const ALL: [Dest; 4] = [Dest::Here, Dest::NewTab, Dest::OtherPane, Dest::NewPane];

    /// The words for the right-click menu.
    pub fn label(self) -> &'static str {
        match self {
            Dest::Here => "Open",
            Dest::NewTab => "Open in new tab",
            Dest::OtherPane => "Open in other pane",
            Dest::NewPane => "Open in new pane",
        }
    }
}

/// What a link points at.
#[derive(Clone, Debug, PartialEq)]
pub enum Spec {
    Verse { range: RefRange, module: Option<String> },
    /// A Strong's number, wherever a dictionary has it.
    Word { key: String },
    /// An entry of a named dictionary.
    Dict { module: String, key: String },
    /// A note by name.
    Note(String),
    /// A web page or a mail address, opened by the program that handles it.
    Web(String),
}

impl Spec {
    pub fn from_target(target: &LinkTarget) -> Spec {
        match target {
            LinkTarget::Verse { range, module } => Spec::Verse { range: *range, module: module.clone() },
            LinkTarget::Sword { module, key } => Spec::Dict { module: module.clone(), key: key.clone() },
            LinkTarget::Note(name) => Spec::Note(name.clone()),
            LinkTarget::Web(url) => Spec::Web(url.clone()),
        }
    }

    /// A short name for the link, to show while it is carried.
    pub fn label(&self) -> String {
        match self {
            Spec::Verse { range, .. } => range.to_string(),
            Spec::Word { key } => key.clone(),
            Spec::Dict { module, key } => format!("{key} \u{b7} {module}"),
            Spec::Note(name) => name.clone(),
            Spec::Web(url) => url.clone(),
        }
    }

    /// The link as it is written in a note, for "Copy link".
    pub fn link_text(&self) -> String {
        match self {
            Spec::Verse { range, module: None } => format!("[[@{range}]]"),
            Spec::Verse { range, module: Some(m) } => format!("[[@{range}@{m}]]"),
            Spec::Word { key } => format!("[[%{key}]]"),
            Spec::Dict { module, key } => format!("[{module}/{key}](sword:{module}/{key})"),
            Spec::Note(name) => format!("[[{name}]]"),
            Spec::Web(url) => url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_choose_where_a_link_opens_and_the_furthest_wins() {
        assert_eq!(Dest::from_modifiers(Modifiers::empty()), Dest::Here);
        assert_eq!(Dest::from_modifiers(Modifiers::COMMAND), Dest::NewTab);
        assert_eq!(Dest::from_modifiers(Modifiers::ALT), Dest::OtherPane);
        assert_eq!(Dest::from_modifiers(Modifiers::SHIFT), Dest::NewPane);
        assert_eq!(Dest::from_modifiers(Modifiers::SHIFT | Modifiers::ALT), Dest::NewPane);
        assert_eq!(Dest::from_modifiers(Modifiers::ALT | Modifiers::COMMAND), Dest::OtherPane);
    }

    #[test]
    fn links_are_copied_as_they_are_written_in_notes() {
        let range = RefRange::parse("John 3:16").unwrap();
        assert_eq!(Spec::Verse { range, module: None }.link_text(), "[[@John 3:16]]");
        assert_eq!(Spec::Verse { range, module: Some("LSB".into()) }.link_text(), "[[@John 3:16@LSB]]");
        assert_eq!(Spec::Note("Creation".into()).link_text(), "[[Creation]]");
        assert_eq!(Spec::Dict { module: "StrongsGreek".into(), key: "G25".into() }.link_text(), "[StrongsGreek/G25](sword:StrongsGreek/G25)");
        assert_eq!(Spec::Word { key: "G25".into() }.link_text(), "[[%G25]]");
    }

    #[test]
    fn a_link_target_becomes_a_spec() {
        let t = LinkTarget::Note("A".into());
        assert_eq!(Spec::from_target(&t), Spec::Note("A".into()));
    }
}
