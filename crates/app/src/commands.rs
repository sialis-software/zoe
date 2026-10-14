//! The commands of the menu bar (macOS), and what the menu shows of the app's state.
//!
//! Each is a message the app already has for a key or a button; the menu gives it a place to be found
//! and a shortcut. On a Mac the menu takes the shortcut, so the app's own key handling leaves these keys
//! alone there (see `App::subscription`); elsewhere it handles them, and there is no menu.

use crate::bible_tab::NoteKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Settings,
    NewTab,
    NewPane,
    CloseTab,
    ReaderMode,
    Back,
    Forward,
    PreviousTab,
    NextTab,
    Search,
    ToggleStrongs,
    ToggleMarks,
    ToggleFootnotes,
    ToggleRefs,
    StrongsInMargin,
    MarksInMargin,
    FootnotesInMargin,
    RefsInMargin,
}

/// A key pressed with the command key (and shift, if `shift`, and option, if `alt`). With `alt` the
/// key is the one on the keyboard (a digit), whatever character option makes of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub shift: bool,
    pub alt: bool,
    pub key: char,
}

impl Command {
    pub const ALL: [Command; 18] = [
        Command::Settings,
        Command::NewTab,
        Command::NewPane,
        Command::CloseTab,
        Command::ReaderMode,
        Command::Back,
        Command::Forward,
        Command::PreviousTab,
        Command::NextTab,
        Command::Search,
        Command::ToggleStrongs,
        Command::ToggleMarks,
        Command::ToggleFootnotes,
        Command::ToggleRefs,
        Command::StrongsInMargin,
        Command::MarksInMargin,
        Command::FootnotesInMargin,
        Command::RefsInMargin,
    ];

    /// The menu item's id.
    pub fn id(self) -> &'static str {
        match self {
            Command::Settings => "settings",
            Command::NewTab => "new-tab",
            Command::NewPane => "new-pane",
            Command::CloseTab => "close-tab",
            Command::ReaderMode => "reader-mode",
            Command::Back => "back",
            Command::Forward => "forward",
            Command::PreviousTab => "previous-tab",
            Command::NextTab => "next-tab",
            Command::Search => "search",
            Command::ToggleStrongs => "toggle-strongs",
            Command::ToggleMarks => "toggle-marks",
            Command::ToggleFootnotes => "toggle-footnotes",
            Command::ToggleRefs => "toggle-refs",
            Command::StrongsInMargin => "strongs-in-margin",
            Command::MarksInMargin => "marks-in-margin",
            Command::FootnotesInMargin => "footnotes-in-margin",
            Command::RefsInMargin => "refs-in-margin",
        }
    }

    pub fn from_id(id: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Command::Settings => "Settings\u{2026}",
            Command::NewTab => "New Tab",
            Command::NewPane => "New Pane",
            Command::CloseTab => "Close Tab",
            Command::ReaderMode => "Reader Mode",
            Command::Back => "Back",
            Command::Forward => "Forward",
            Command::PreviousTab => "Previous Tab",
            Command::NextTab => "Next Tab",
            Command::Search => "Search the Bible",
            Command::ToggleStrongs => "Strong\u{2019}s Numbers",
            Command::ToggleMarks => "Marks",
            Command::ToggleFootnotes => "Footnotes",
            Command::ToggleRefs => "References",
            Command::StrongsInMargin => "Strong\u{2019}s Numbers in the Margin",
            Command::MarksInMargin => "Marks in the Margin",
            Command::FootnotesInMargin => "Footnotes in the Margin",
            Command::RefsInMargin => "References in the Margin",
        }
    }

    /// The setting a command toggles in the Bible tab's menu, and whether it is the 'in the margin' one.
    pub fn setting(self) -> Option<(NoteKind, bool)> {
        match self {
            Command::ToggleStrongs => Some((NoteKind::Strongs, false)),
            Command::ToggleMarks => Some((NoteKind::Marks, false)),
            Command::ToggleFootnotes => Some((NoteKind::Footnotes, false)),
            Command::ToggleRefs => Some((NoteKind::Refs, false)),
            Command::StrongsInMargin => Some((NoteKind::Strongs, true)),
            Command::MarksInMargin => Some((NoteKind::Marks, true)),
            Command::FootnotesInMargin => Some((NoteKind::Footnotes, true)),
            Command::RefsInMargin => Some((NoteKind::Refs, true)),
            _ => None,
        }
    }

    pub fn shortcut(self) -> Option<Shortcut> {
        let (shift, alt, key) = match self {
            Command::Settings => (false, false, ','),
            Command::NewTab => (false, false, 't'),
            Command::NewPane => (false, false, 'n'),
            Command::CloseTab => (false, false, 'w'),
            Command::ReaderMode => return None,
            Command::Back => (false, false, '['),
            Command::Forward => (false, false, ']'),
            Command::PreviousTab => (true, false, '['),
            Command::NextTab => (true, false, ']'),
            Command::Search => (true, false, 'f'),
            Command::ToggleStrongs => (false, true, '1'),
            Command::ToggleMarks => (false, true, '2'),
            Command::ToggleFootnotes => (false, true, '3'),
            Command::ToggleRefs => (false, true, '4'),
            Command::StrongsInMargin => (true, true, '1'),
            Command::MarksInMargin => (true, true, '2'),
            Command::FootnotesInMargin => (true, true, '3'),
            Command::RefsInMargin => (true, true, '4'),
        };
        Some(Shortcut { shift, alt, key })
    }
}

/// What the menu needs to know of the app to grey out what does nothing and tick what is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    /// The tab in front has somewhere to go back, or forward, to.
    pub back: bool,
    pub forward: bool,
    /// The pane in front has more than one tab.
    pub tabs: bool,
    /// The tab in front is a Bible tab (reader mode is for those).
    pub can_read: bool,
    pub reading: bool,
    /// There is a Bible to search.
    pub search: bool,
    /// What the Bible tab in front shows (the menu's own settings), and which of it is in the margin.
    pub strongs: bool,
    pub marks: bool,
    pub footnotes: bool,
    pub refs: bool,
    pub strongs_in_margin: bool,
    pub marks_in_margin: bool,
    pub footnotes_in_margin: bool,
    pub refs_in_margin: bool,
}

impl MenuState {
    pub fn enabled(self, command: Command) -> bool {
        match command {
            Command::Back => self.back,
            Command::Forward => self.forward,
            Command::PreviousTab | Command::NextTab => self.tabs,
            Command::ReaderMode => self.can_read || self.reading,
            Command::Search => self.search,
            Command::ToggleStrongs | Command::ToggleMarks | Command::ToggleFootnotes | Command::ToggleRefs => self.can_read,
            Command::StrongsInMargin => self.can_read && self.strongs,
            Command::MarksInMargin => self.can_read && self.marks,
            Command::FootnotesInMargin => self.can_read && self.footnotes,
            Command::RefsInMargin => self.can_read && self.refs,
            Command::Settings | Command::NewTab | Command::NewPane | Command::CloseTab => true,
        }
    }

    pub fn checked(self, command: Command) -> bool {
        match command {
            Command::ReaderMode => self.reading,
            Command::ToggleStrongs => self.can_read && self.strongs,
            Command::ToggleMarks => self.can_read && self.marks,
            Command::ToggleFootnotes => self.can_read && self.footnotes,
            Command::ToggleRefs => self.can_read && self.refs,
            Command::StrongsInMargin => self.enabled(command) && self.strongs_in_margin,
            Command::MarksInMargin => self.enabled(command) && self.marks_in_margin,
            Command::FootnotesInMargin => self.enabled(command) && self.footnotes_in_margin,
            Command::RefsInMargin => self.enabled(command) && self.refs_in_margin,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_name_one_command_each_and_shortcuts_are_not_shared() {
        for command in Command::ALL {
            assert_eq!(Command::from_id(command.id()), Some(command));
        }
        let shortcuts: Vec<Shortcut> = Command::ALL.iter().filter_map(|c| c.shortcut()).collect();
        for (i, a) in shortcuts.iter().enumerate() {
            assert!(!shortcuts[i + 1..].contains(a), "{a:?} is on two commands");
        }
        assert_eq!(Command::from_id("nothing"), None);
        let (plain, alt) = (Command::NextTab.shortcut().unwrap(), Command::ToggleStrongs.shortcut().unwrap());
        assert!(!plain.alt && alt.alt, "option is part of a shortcut");
        assert_ne!(Command::ToggleMarks.shortcut(), Command::MarksInMargin.shortcut());
    }

    #[test]
    fn the_settings_are_for_bible_tabs_and_the_margin_ones_wait_for_their_kind_to_show() {
        let other = MenuState { strongs: true, ..MenuState::default() };
        assert!(!other.enabled(Command::ToggleMarks) && !other.enabled(Command::StrongsInMargin) && !other.checked(Command::ToggleStrongs));
        let bible = MenuState { can_read: true, marks: true, marks_in_margin: true, refs_in_margin: true, ..MenuState::default() };
        assert!(bible.enabled(Command::ToggleStrongs) && !bible.checked(Command::ToggleStrongs));
        assert!(bible.enabled(Command::ToggleMarks) && bible.checked(Command::ToggleMarks));
        assert!(bible.enabled(Command::MarksInMargin) && bible.checked(Command::MarksInMargin));
        assert!(!bible.enabled(Command::RefsInMargin) && !bible.checked(Command::RefsInMargin), "References are hidden");
        assert!(Command::ALL.iter().filter(|c| c.setting().is_some()).count() == 8);
    }

    #[test]
    fn the_menu_greys_what_does_nothing_and_ticks_reader_mode_when_on() {
        let none = MenuState::default();
        assert!(!none.enabled(Command::Back) && !none.enabled(Command::NextTab) && !none.enabled(Command::ReaderMode));
        assert!(none.enabled(Command::CloseTab) && none.enabled(Command::NewTab));
        let reading = MenuState { reading: true, ..none };
        assert!(reading.enabled(Command::ReaderMode) && reading.checked(Command::ReaderMode) && !reading.checked(Command::Back));
    }
}
