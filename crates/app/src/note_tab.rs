//! The note tab: one markdown document (a note, or a mark's margin file) in the live editor.
//! Like the Bible tab it has a header bar: a selector for which note it holds (a panel of
//! notes grouped by type, with an optional search), and delete for a margin note.
//! Each note tab holds one document.

use crate::bible_tab::{BibleTab, start_of_note};
use crate::live_editor::live_editor;
use crate::history::History;
use crate::opening::Spec;
use crate::palette::Palette;
use crate::popover::popover;
use crate::shared::{Shared, TabId};
use iced::widget::{
    button, checkbox, column, container, mouse_area, opaque, pick_list, row, rule, scrollable, space, stack, text,
    text_input,
};
use iced::{Element, Length, Task, Theme};
use md_editor::{Action as EditAction, Document, Vim, VimKey, VimMode};
use std::collections::BTreeSet;
use std::fmt;
use study_core::ModuleId;
use std::collections::BTreeMap;
use study_core::frontmatter::{self, Frontmatter, PropType, Property, Value};
use study_core::margin::MarkStyle;
use study_core::markdown::LinkTarget;
use study_core::templates;

/// Height of a header bar control: a pick list in the Bible tab's toolbar is this tall, so the
/// bars of the two tab types match.
const CONTROL_HEIGHT: f32 = 31.0;
/// Where the selector panel starts: just under the selector (its padding, its height, a gap).
const PANEL_TOP: f32 = 8.0 + CONTROL_HEIGHT + 3.0;

/// A button that looks like the Bible tab's pick lists, for the note selector.
/// The height of the bar under the text, for the pane's buttons (split, fill the window) that float over its right
/// end: a 28 px button and the corner's own margin of 10 on each side.
const PANE_BUTTONS: f32 = 48.0;

/// A note's document, with the caret where writing starts. The reading log's is written in like any note but for its
/// title (shown) and its frontmatter (not shown), which are out of reach.
fn open_doc(path: &str, text: &str) -> Document {
    if path != crate::shared::READING_LOG {
        return start_of_note(text);
    }
    // The caret starts at the end, where the days are added (the tab shows that end).
    let mut doc = Document::new(text).lock_title();
    let end = doc.source().len();
    doc.perform(EditAction::SetCaret { offset: end, extend: false });
    doc
}

/// What a note's properties are changed by: the messages of the frontmatter section, and renaming.
fn changes_properties(message: &NoteMsg) -> bool {
    matches!(
        message,
        NoteMsg::MarkStyle(_)
            | NoteMsg::MarkColors
            | NoteMsg::MarkColor(_)
            | NoteMsg::TagDraft(_)
            | NoteMsg::PickTag(_)
            | NoteMsg::AddTag
            | NoteMsg::RemoveTag(_)
            | NoteMsg::AddProperties
            | NoteMsg::PickText
            | NoteMsg::Unlink
            | NoteMsg::ToggleFront
            | NoteMsg::SetKind(_)
            | NoteMsg::PropEdit(..)
            | NoteMsg::PropBool(..)
            | NoteMsg::PropRemove(_)
            | NoteMsg::NewKey(_)
            | NoteMsg::NewType(_)
            | NoteMsg::ToggleAddProp
            | NoteMsg::AddProp
            | NoteMsg::StartRename
            | NoteMsg::CommitRename
    )
}

fn dropdown_style(theme: &Theme, status: button::Status) -> button::Style {
    use iced::widget::pick_list;
    let status = match status {
        button::Status::Hovered | button::Status::Pressed => pick_list::Status::Hovered,
        _ => pick_list::Status::Active,
    };
    let look = pick_list::default(theme, status);
    button::Style {
        background: Some(look.background),
        text_color: look.text_color,
        border: look.border,
        ..button::Style::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteChoice(pub String);

impl fmt::Display for NoteChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.trim_end_matches(".md"))
    }
}

#[derive(Debug, Clone)]
pub enum NoteMsg {
    Action(EditAction),
    /// A key the Vim bindings take (see `Settings::vim_mode`).
    Vim(VimKey),
    /// Open the link under the editor cursor, or one clicked in the text.
    OpenLink(LinkTarget),
    /// A right click on a link in the text, at this point of the window.
    LinkMenu(LinkTarget, iced::Point),
    /// A link in the text pulled away, at this point of the window.
    LinkDrag(LinkTarget, iced::Point),
    PickNote(NoteChoice),
    /// Back or forward along the notes this tab has shown.
    Back,
    /// Put a link to this note on the clipboard.
    CopyLink,
    /// The text was scrolled this far.
    Scrolled(f32),
    Forward,
    /// Ask whether to delete the open note (the trash button).
    AskDelete,
    CancelDelete,
    /// Delete the open note (and its mark, for a margin note): what the confirmation does.
    ConfirmDelete,
    // Renaming
    StartRename,
    RenameDraft(String),
    CommitRename,
    CancelRename,
    // The note selector
    TogglePicker,
    ClosePicker,
    /// The hamburger menu (rename, delete).
    ToggleMenu,
    CloseMenu,
    PickerQuery(String),
    ToggleGroup(String),
    /// Narrow the list to a tag, or lift that again.
    PickerTag(String),
    ClearPickerTags,
    /// A new note, made with its properties in place.
    /// Ask what kind of note to start.
    NewNote,
    /// A kind was chosen in that dialog: the others then ask for a title, the daily note (whose
    /// title is the date) is made at once.
    PickKind(String),
    /// Open this template to change what a note of its kind starts with.
    EditTemplate(String),
    /// The notes that link to this one.
    OpenBacklinks,
    /// The title typed in that dialog, and back to the choice of kind.
    NewTitle(String),
    BackToKinds,
    CancelNew,
    /// Start a note from this template.
    CreateNote(String),
    // The properties strip
    /// A margin note's mark: how it marks the text, whether its colors are showing, and its color.
    MarkStyle(MarkStyle),
    MarkColors,
    /// The color panel is let go of (a click elsewhere, Esc).
    CloseColors,
    MarkColor(usize),
    TagDraft(String),
    /// Add this existing tag (from the suggestions).
    PickTag(String),
    AddTag,
    RemoveTag(String),
    /// Give a note that has none its frontmatter, stamped with the time.
    AddProperties,
    /// Take the next selection in a Bible as this note's passage (a margin note's: "Reselect"; an ordinary
    /// note's: "Mark", which makes it a margin note), or give up waiting for one.
    PickText,
    CancelPick,
    /// Let a margin note go from its passage: the highlight goes, the note stays as an ordinary note.
    Unlink,
    // The properties editor
    /// Open or fold the whole frontmatter section (tags, passage, properties) above the text.
    ToggleFront,
    SetKind(KindChoice),
    /// Text typed into the value of the property at this place.
    PropEdit(usize, String),
    PropBool(usize, bool),
    PropRemove(usize),
    /// Show the boxes to add a property (the plus button), or put them away.
    ToggleAddProp,
    NewKey(String),
    NewType(PropType),
    AddProp,
}

/// The inside of a tag chip: the tag, and how many notes have it (not shown for none). The count
/// takes the chip's own text color (it used to be a fixed grey, which vanished on a grey chip).
fn chip_label<'a>(tag: &str, count: usize, chosen: bool) -> Element<'a, NoteMsg> {
    let mut label = row![].spacing(5).align_y(iced::Center);
    if chosen {
        // Chosen tags are told from the rest by more than a color.
        label = label.push(text("\u{2713}").size(12));
    }
    label = label.push(text(format!("#{tag}")).size(12));
    if count > 0 {
        label = label.push(text(count.to_string()).size(11));
    }
    label.into()
}

/// The box for a new tag: rounded, and only a little lighter under the pointer.
fn tag_box_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let base = text_input::default(theme, status);
    let palette = theme.extended_palette();
    let edge = match status {
        text_input::Status::Active => palette.background.strong.color,
        text_input::Status::Hovered => iced::Color { a: 0.8, ..palette.background.strong.color },
        text_input::Status::Focused { .. } => palette.primary.base.color,
        text_input::Status::Disabled => palette.background.weak.color,
    };
    let edge = if matches!(status, text_input::Status::Hovered) {
        // A hint of the focused color, not the whole of it.
        iced::Color {
            r: (edge.r + palette.primary.weak.color.r) / 2.0,
            g: (edge.g + palette.primary.weak.color.g) / 2.0,
            b: (edge.b + palette.primary.weak.color.b) / 2.0,
            a: edge.a,
        }
    } else {
        edge
    };
    text_input::Style { border: iced::Border { color: edge, width: 1.0, radius: 12.0.into() }, ..base }
}

/// A tag as a button: filled when it is chosen, and in its color if a pen is for it.
fn tag_chip_style(theme: &Theme, status: button::Status, on: bool, color: Option<iced::Color>) -> button::Style {
    let palette = theme.extended_palette();
    let pointed = matches!(status, button::Status::Hovered);
    let text = palette.background.base.text;
    let edge = |c: iced::Color, width: f32| iced::Border { color: c, width, radius: 12.0.into() };
    match (on, color) {
        // Chosen: solid, in the tag's color if it has one, with a tick (see `chip_label`): the
        // chips that are narrowing the list stand out from the ones that are only there.
        (true, Some(color)) => {
            let luma = 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;
            button::Style {
                background: Some(iced::Color { a: if pointed { 0.9 } else { 1.0 }, ..color }.into()),
                text_color: if luma > 0.55 { iced::Color::BLACK } else { iced::Color::WHITE },
                border: edge(color, 1.0),
                ..button::Style::default()
            }
        }
        (true, None) => button::Style {
            background: Some(palette.primary.base.color.into()),
            text_color: palette.primary.base.text,
            border: edge(palette.primary.strong.color, 1.0),
            ..button::Style::default()
        },
        // Not chosen: an outline, nothing filled (a little tint of its color, if it has one).
        (false, Some(color)) => button::Style {
            background: Some(iced::Color { a: if pointed { 0.2 } else { 0.07 }, ..color }.into()),
            text_color: color,
            border: edge(iced::Color { a: 0.7, ..color }, 1.0),
            ..button::Style::default()
        },
        (false, None) => button::Style {
            background: Some(iced::Color { a: if pointed { 0.1 } else { 0.0 }, ..text }.into()),
            text_color: text,
            border: edge(iced::Color { a: 0.35, ..text }, 1.0),
            ..button::Style::default()
        },
    }
}

/// The color of tag `tag`, if it has one.
pub(crate) fn tag_color(sh: &Shared, tag: &str) -> Option<iced::Color> {
    sh.tag_color(tag).and_then(|hex| crate::bible_tab::hex_color(&hex, 1.0))
}

fn new_title_id() -> iced::widget::Id {
    iced::widget::Id::new("new-note-title")
}

/// The dialog that asks what kind of note to start: a dimmed backdrop (a click on it cancels) and
/// the choices.
fn template_dialog<'a>(sh: &Shared, title: &str, pending: Option<&str>) -> (Element<'a, NoteMsg>, Element<'a, NoteMsg>) {
    let dim = mouse_area(
        container(space::horizontal()).width(Length::Fill).height(Length::Fill).style(|_: &Theme| container::Style {
            background: Some(iced::Color { a: 0.55, ..iced::Color::BLACK }.into()),
            ..container::Style::default()
        }),
    )
    .on_press(NoteMsg::CancelNew)
    .into();
    let content: Element<'a, NoteMsg> = match pending.and_then(|p| sh.templates().into_iter().find(|t| t.path == p)) {
        // The kind is chosen: now what to call it.
        Some(template) => column![
            text(format!("New {}", template.name.to_lowercase())).size(17),
            text_input("Title", title)
                .id(new_title_id())
                .on_input(NoteMsg::NewTitle)
                .on_submit(NoteMsg::CreateNote(template.path.clone()))
                .size(14)
                .padding(6),
            text("It is the note's heading and its file name.").size(12).style(text::secondary),
            row![
                button(text("Back").size(14)).padding([6, 14]).style(button::text).on_press(NoteMsg::BackToKinds),
                space::horizontal(),
                button(text("Cancel").size(14)).padding([6, 14]).style(button::secondary).on_press(NoteMsg::CancelNew),
                button(text("Create").size(14)).padding([6, 14]).on_press(NoteMsg::CreateNote(template.path.clone())),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .into(),
        // First, what kind.
        None => {
            let mut choices = column![].spacing(4);
            for template in sh.templates() {
                choices = choices.push(
                    row![
                        button(column![text(template.name.clone()).size(15), text(template.description.clone()).size(12).style(text::secondary)].spacing(2))
                            .width(Length::Fill)
                            .padding([8, 12])
                            .style(button::text)
                            .on_press(NoteMsg::PickKind(template.path.clone())),
                        // For those who want a kind to start differently: its template is a note.
                        iced::widget::tooltip(
                            button(crate::icons::Icon::Pencil.view(14.0)).padding(6).style(button::text).on_press(NoteMsg::EditTemplate(template.path.clone())),
                            container(text("Edit this template").size(12)).padding(4).style(container::rounded_box),
                            iced::widget::tooltip::Position::Left,
                        ),
                    ]
                    .align_y(iced::Center),
                );
            }
            column![
                text("New note").size(17),
                text("What kind?").size(13).style(text::secondary),
                choices,
                row![space::horizontal(), button(text("Cancel").size(14)).padding([6, 14]).style(button::secondary).on_press(NoteMsg::CancelNew)],
            ]
            .spacing(10)
            .into()
        }
    };
    let dialog = container(content).padding(18).width(380).style(crate::popup_style);
    (dim, container(opaque(dialog)).center(Length::Fill).into())
}

/// A kind of note for the type picker: none, or one of the app's, or one written by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindChoice(pub Option<String>);

/// How a mark marks the text, as a choice in a drop-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleChoice(pub MarkStyle);

impl StyleChoice {
    /// The looks a mark can be given here.
    pub const ALL: [StyleChoice; 3] = [StyleChoice(MarkStyle::Highlight), StyleChoice(MarkStyle::Underline), StyleChoice(MarkStyle::None)];
}

impl fmt::Display for StyleChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            MarkStyle::Highlight => "Highlight",
            MarkStyle::Underline => "Underline",
            MarkStyle::None => "Plain",
        })
    }
}

impl fmt::Display for KindChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            None => f.write_str("None"),
            Some(kind) => {
                let mut c = kind.chars();
                let first = c.next().map(|f| f.to_uppercase().collect::<String>()).unwrap_or_default();
                write!(f, "{first}{}", c.as_str())
            }
        }
    }
}

/// Something the tab cannot do alone: where a link opens is for the workspace to decide.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Follow a link, where the keys held (or the menu) say.
    Open(Spec),
    /// Offer where to open a link, at this point of the window.
    Menu(Spec, iced::Point),
    /// A link has been picked up: carry it, from this point of the window.
    Drag(Spec, iced::Point),
    /// Show the notes that link to the note at this path in a tab.
    Backlinks(String),
}

/// What the confirmation dialog is about to delete.
enum Deleting {
    Margin(String),
    Note(String),
}

/// The open selector panel.
#[derive(Default)]
pub struct Picker {
    pub query: String,
    /// The tags the list is narrowed to (a note must have all of them).
    pub tags: BTreeSet<String>,
    /// Groups the person opened or closed by hand, against their usual state: the group of
    /// the open note starts open, the rest closed.
    pub toggled: BTreeSet<String>,
}

pub struct NoteTab {
    pub id: TabId,
    pub path: String,
    /// The notes this tab has shown.
    pub history: History<String>,
    pub doc: Document,
    /// The Vim state; only used while `Settings::vim_mode` is on.
    pub vim: Vim,
    pub picker: Option<Picker>,
    /// The confirmation to delete the open margin note is showing.
    pub confirm_delete: bool,
    /// The dialog that asks what kind of note to start is showing.
    pub choosing: bool,
    /// The title typed in it.
    pub new_title: String,
    /// The kind chosen, while its title is asked for.
    pub pending: Option<String>,
    /// What is being typed in the strip's tag box.
    pub tag_draft: String,
    /// The name being typed, while the note is being renamed.
    pub renaming: Option<String>,
    /// The hamburger menu is open.
    pub menu_open: bool,
    /// The frontmatter section above the text is open (it is folded to one line until it is asked for).
    pub front_open: bool,
    /// Waiting for a selection in a Bible to be this note's passage (`PickText`).
    pub picking: bool,
    /// How far down the text is scrolled, to put back when the tab is shown again.
    pub scroll: f32,
    /// Whether a margin note's colors to choose from are showing.
    pub mark_colors: bool,
    /// What a value box shows while what is typed is not (yet) a value of its type: a number
    /// half typed, a list with its comma still to be followed. By place in the properties.
    pub drafts: BTreeMap<usize, String>,
    /// The new property being named.
    pub new_key: String,
    pub new_type: PropType,
    /// The boxes to add a property are showing (they are behind a plus button).
    pub adding: bool,
}

/// One type of note in the selector, with its notes.
#[derive(Debug, PartialEq, Eq)]
pub struct Group {
    pub key: String,
    pub label: String,
    /// (path, label) of each note.
    pub notes: Vec<(String, String)>,
}

/// The file name without its folder or `.md`.
pub fn file_stem(path: &str) -> String {
    path.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string()
}

/// How a note is named in the selector and the tab: its file name, or for a margin note what
/// it is anchored to.
/// A link to the note at `path` as it is written in another note: by its name when no other note
/// has that name, else by its path, and with the name a person would call it when that is not what
/// is written (`[[margins/John/03/16-note-s1|John 3:16 · note]]`).
pub fn link_to(sh: &Shared, path: &str) -> String {
    let no_ext = path.trim_end_matches(".md");
    let stem = no_ext.rsplit('/').next().unwrap_or(no_ext);
    let same = |p: &String| p.trim_end_matches(".md").rsplit('/').next().is_some_and(|s| s.eq_ignore_ascii_case(stem));
    let unique = sh.notes.keys().filter(|p| same(p)).count() <= 1;
    let target = if unique && sh.mark_index_of(path).is_none() { stem } else { no_ext };
    // The brackets and the bar are what the link is made of: a name has none of its own.
    let label = label_of(sh, path).replace(['[', ']', '|'], " ");
    if label == target { format!("[[{target}]]") } else { format!("[[{target}|{label}]]") }
}

pub fn label_of(sh: &Shared, path: &str) -> String {
    if let Some(mark) = sh.mark_index_of(path).and_then(|i| sh.marks.get(i)) {
        let kind = match (mark.body.trim().is_empty(), mark.style) {
            (false, _) => "note",
            (true, MarkStyle::Underline) => "underline",
            (true, _) => "highlight",
        };
        return format!("{} · {kind}", mark.anchor);
    }
    let stem = path.trim_end_matches(".md").rsplit('/').next().unwrap_or_default();
    if stem.is_empty() { "Note".into() } else { stem.to_string() }
}

/// Whether a note with `have` among its tags is under tag `wanted`: that tag, or one inside it
/// (`topic/grace` is under `topic`).
pub fn under_tag(have: &[String], wanted: &str) -> bool {
    have.iter().any(|t| {
        let t = frontmatter::normalize_tag(t);
        t == wanted || t.starts_with(&format!("{wanted}/"))
    })
}

/// The notes in the selector: the ordinary ones, then the margin notes (which are tagged too).
/// Not the templates.
/// Notes are found by tag, not folder. `query` is words that must each be in a note's name,
/// text or tags (any case), and `#words` that must each be in one of its tags; `tags` are tags a
/// note must be under.
pub fn groups(sh: &Shared, query: &str, tags: &BTreeSet<String>) -> Vec<Group> {
    let query = query.trim().to_lowercase();
    let (mut words, mut wanted_tags) = (Vec::new(), Vec::new());
    for token in query.split_whitespace() {
        match token.strip_prefix('#') {
            Some(tag) if !tag.is_empty() => wanted_tags.push(frontmatter::normalize_tag(tag)),
            Some(_) => {}
            None => words.push(token),
        }
    }
    let mut out: Vec<Group> = Vec::new();
    for (path, body) in sh.notes.iter() {
        let have = sh.tags_of(path);
        if !tags.iter().all(|t| under_tag(&have, t)) {
            continue;
        }
        let normalized: Vec<String> = have.iter().map(|t| frontmatter::normalize_tag(t)).collect();
        if !wanted_tags.iter().all(|w| normalized.iter().any(|t| t.contains(w.as_str()))) {
            continue;
        }
        let label = label_of(sh, path);
        let (name, text, tag_text) = (label.to_lowercase(), body.to_lowercase(), normalized.join(" "));
        if !words.iter().all(|w| name.contains(w) || path.to_lowercase().contains(w) || text.contains(w) || tag_text.contains(w)) {
            continue;
        }
        // The reading log is not a note to open from here (it has its own tab).
        if path == crate::shared::READING_LOG {
            continue;
        }
        // The templates are not among the notes found: changing what a kind of note starts with
        // is for those who go looking (the pencil by a kind in the New note dialog).
        if templates::is_template(path) {
            continue;
        }
        // What was imported into the library (the reading plans' days) is far too many
        // for the plain list: it is found by searching, or by its tags.
        let in_library = path.starts_with("library/");
        if in_library && words.is_empty() && wanted_tags.is_empty() && tags.is_empty() {
            continue;
        }
        let (key, group_label) = if path.starts_with("margins/") {
            ("margins", "Margin notes")
        } else if in_library {
            ("library", "Library")
        } else {
            ("notes", "Notes")
        };
        match out.iter_mut().find(|g| g.key == key) {
            Some(g) => g.notes.push((path.clone(), label)),
            None => out.push(Group { key: key.into(), label: group_label.into(), notes: vec![(path.clone(), label)] }),
        }
    }
    out.sort_by_key(|g| match g.key.as_str() {
        "notes" => 0,
        "library" => 1,
        _ => 2,
    });
    for g in &mut out {
        // Margin notes keep their order in the canon; the rest go by name. When searching,
        // notes whose name matches come before those that only mention it.
        if g.key != "margins" {
            g.notes.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        }
        if let Some(first) = words.first() {
            g.notes.sort_by_key(|(_, label)| !label.to_lowercase().contains(first));
        }
    }
    out
}

impl Picker {
    /// Whether a group shows its notes: always while searching, else as the person left it.
    pub fn is_open(&self, group: &Group, current: &str) -> bool {
        if !self.query.trim().is_empty() || !self.tags.is_empty() || group.key == "notes" {
            return true;
        }
        let usual = group.notes.iter().any(|(p, _)| p == current);
        usual != self.toggled.contains(&group.key)
    }
}

impl NoteTab {
    pub fn new(id: TabId, sh: &Shared, path: String) -> Self {
        let doc = open_doc(&path, sh.notes.get(&path).map_or("", String::as_str));
        NoteTab {
            id,
            history: History::new(path.clone()),
            path,
            doc,
            vim: Vim::new(),
            picker: None,
            confirm_delete: false,
            choosing: false,
            new_title: String::new(),
            pending: None,
            tag_draft: String::new(),
            renaming: None,
            menu_open: false,
            front_open: false,
            picking: false,
            scroll: 0.0,
            mark_colors: false,
            drafts: BTreeMap::new(),
            new_key: String::new(),
            new_type: PropType::Text,
            adding: false,
        }
    }

    pub fn editor_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("writer-{}", self.id))
    }

    fn rename_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("note-rename-{}", self.id))
    }

    fn search_id(&self) -> iced::widget::Id {
        iced::widget::Id::from(format!("note-search-{}", self.id))
    }

    pub fn title(&self, sh: &Shared) -> String {
        label_of(sh, &self.path)
    }

    /// Go to a note, as a move to a new place (Back returns).
    pub fn open(&mut self, sh: &mut Shared, path: String) {
        // The reading log is not shown by a note tab: it has a tab of its own.
        if path == crate::shared::READING_LOG && !self.is_log() {
            return;
        }
        self.show(sh, path.clone());
        self.history.go(path, |a, b| a == b);
    }

    /// One step along the history, skipping notes that are gone.
    fn step(&mut self, sh: &mut Shared, back: bool) {
        loop {
            let Some(path) = (if back { self.history.back() } else { self.history.forward() }).cloned() else { return };
            if sh.notes.contains_key(&path) {
                self.show(sh, path);
                return;
            }
        }
    }

    /// Show a note, with the history left as it is.
    fn show(&mut self, sh: &mut Shared, path: String) {
        let text = sh.notes.get(&path).cloned().unwrap_or_default();
        self.doc = open_doc(&path, &text);
        self.scroll = 0.0;
        self.path = path;
        self.picker = None;
        self.renaming = None;
        self.confirm_delete = false;
        self.tag_draft.clear();
        self.mark_colors = false;
        self.picking = false;
        self.adding = false;
        self.drafts.clear();
        sh.status.clear();
    }

    /// A note this tab has shown has a new name.
    pub fn rename_in_history(&mut self, old: &str, new: &str) {
        self.history.for_each_mut(|p| {
            if p == old {
                *p = new.to_string();
            }
        });
    }

    /// Reload the document from the shared text (it changed behind the tab).
    pub fn reload(&mut self, sh: &Shared) {
        let new = sh.notes.get(&self.path).map_or("", String::as_str);
        let old = self.doc.source().to_string();
        if old == new {
            return;
        }
        let head = self.doc.selection().head;
        self.doc = open_doc(&self.path, new);
        // The caret stays where it was in the words that did not change: before what was changed
        // as it was, after it as far on as the change moved things.
        let prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
        let suffix = old.bytes().rev().zip(new.bytes().rev()).take_while(|(a, b)| a == b).count().min(old.len() - prefix).min(new.len() - prefix);
        let at = if head <= prefix {
            head
        } else if head >= old.len() - suffix {
            head + new.len() - old.len()
        } else {
            prefix
        };
        let at = (0..=at.min(new.len())).rev().find(|&i| new.is_char_boundary(i)).unwrap_or(0);
        self.doc.perform(EditAction::SetCaret { offset: at.max(self.doc.front_end()), extend: false });
    }

    /// Change the note's properties: the frontmatter block is rewritten in the app's order, as
    /// one edit in the document (so it undoes), and the rest of the note is untouched.
    fn change_properties(&mut self, sh: &mut Shared, change: impl FnOnce(&mut Frontmatter)) {
        let mut front = frontmatter::parse(self.doc.source()).front;
        change(&mut front);
        let block = if front.is_empty() { String::new() } else { front.render() };
        self.doc.replace_frontmatter(&block);
        sh.notes.insert(self.path.clone(), self.doc.source().to_string());
    }

    /// Start a note from `template` and show it (today's daily note, if there is one, is shown as
    /// it is).
    fn new_note(&mut self, sh: &mut Shared, template: &str, title: Option<&str>) {
        let Some(template) = sh.templates().into_iter().find(|t| t.path == template) else { return };
        let made = templates::plan(&template, &chrono::Local::now(), title, |path| sh.notes.contains_key(path));
        if let Some(text) = made.text {
            sh.notes.insert(made.path.clone(), text);
        }
        self.open(sh, made.path);
    }

    /// Add a tag to the note, or to the mark when this is a margin note's file (whose lines are
    /// the app's, so they are changed through the mark).
    fn add_tag(&mut self, sh: &mut Shared, tag: &str) {
        match self.open_mark_index(sh) {
            Some(i) => {
                let mut tags = sh.marks[i].tags.clone();
                tags.push(tag.to_string());
                sh.set_mark_tags(i, tags);
            }
            None => self.change_properties(sh, |f| {
                f.add_tag(tag);
            }),
        }
    }

    /// Index of the mark whose file is open here.
    pub fn open_mark_index(&self, sh: &Shared) -> Option<usize> {
        sh.mark_index_of(&self.path)
    }

    /// Whether this is the reading log: the app's own note, whose name, title and properties are the app's.
    pub fn is_log(&self) -> bool {
        self.path == crate::shared::READING_LOG
    }

    /// Whether the note is one that is only read: the library's.
    pub fn read_only(&self) -> bool {
        crate::shared::is_library(&self.path)
    }

    pub fn update(&mut self, sh: &mut Shared, message: NoteMsg) -> (Task<NoteMsg>, Option<Request>) {
        // A note of the library is not written in: only what moves or selects, follows links and
        // scrolls is let through.
        if self.read_only() {
            let selects = |a: &EditAction| {
                matches!(
                    a,
                    EditAction::Move { .. }
                        | EditAction::SetCaret { .. }
                        | EditAction::Select(_)
                        | EditAction::SelectBetween { .. }
                        | EditAction::SelectAll
                        | EditAction::SelectWord(_)
                        | EditAction::SelectLine(_)
                )
            };
            let changes = match &message {
                NoteMsg::Action(a) => !selects(a),
                NoteMsg::Vim(_)
                | NoteMsg::AskDelete
                | NoteMsg::ConfirmDelete
                | NoteMsg::StartRename
                | NoteMsg::CommitRename
                | NoteMsg::MarkStyle(_)
                | NoteMsg::MarkColor(_)
                | NoteMsg::TagDraft(_)
                | NoteMsg::PickTag(_)
                | NoteMsg::AddTag
                | NoteMsg::RemoveTag(_)
                | NoteMsg::AddProperties
                | NoteMsg::PickText
                | NoteMsg::Unlink
                | NoteMsg::SetKind(_)
                | NoteMsg::PropEdit(..)
                | NoteMsg::PropBool(..)
                | NoteMsg::PropRemove(_)
                | NoteMsg::NewKey(_)
                | NoteMsg::NewType(_)
                | NoteMsg::ToggleAddProp
                | NoteMsg::AddProp => true,
                _ => false,
            };
            if changes {
                return (Task::none(), None);
            }
        }
        // The reading log is written in, but its properties, its name and its title are the app's.
        if self.is_log() && changes_properties(&message) {
            return (Task::none(), None);
        }
        let mut task = Task::none();
        match message {
            NoteMsg::Action(action) => {
                if matches!(action, EditAction::Undo | EditAction::Redo) {
                    self.drafts.clear();
                }
                if self.doc.perform(action) {
                    sh.notes.insert(self.path.clone(), self.doc.source().to_string());
                    let path = self.path.clone();
                    sh.sync_mark(&path);
                }
                // A click or a drag moved the caret: Vim follows it (a selection is Visual mode).
                if sh.settings.vim_mode {
                    self.vim.sync(&mut self.doc);
                }
            }
            NoteMsg::Vim(key) => {
                let out = self.vim.key(&mut self.doc, key);
                if out.changed {
                    self.drafts.clear();
                    sh.notes.insert(self.path.clone(), self.doc.source().to_string());
                    let path = self.path.clone();
                    sh.sync_mark(&path);
                }
            }
            NoteMsg::ToggleMenu => {
                self.menu_open = !self.menu_open;
                self.picker = None;
            }
            NoteMsg::CloseMenu => self.menu_open = false,
            NoteMsg::AskDelete => {
                self.menu_open = false;
                self.confirm_delete = sh.notes.contains_key(&self.path);
                self.picker = None;
            }
            NoteMsg::CancelDelete => self.confirm_delete = false,
            NoteMsg::ConfirmDelete => {
                self.confirm_delete = false;
                match self.open_mark_index(sh) {
                    Some(index) => sh.delete_mark(index, None),
                    None => sh.delete_note(&self.path.clone()),
                }
            }
            NoteMsg::OpenLink(target) => return (task, Some(Request::Open(Spec::from_target(&target)))),
            NoteMsg::LinkDrag(target, at) => return (task, Some(Request::Drag(Spec::from_target(&target), at))),
            NoteMsg::LinkMenu(target, at) => return (task, Some(Request::Menu(Spec::from_target(&target), at))),
            NoteMsg::PickNote(NoteChoice(path)) => self.open(sh, path),
            NoteMsg::Back => self.step(sh, true),
            NoteMsg::Forward => self.step(sh, false),
            NoteMsg::TogglePicker => {
                self.menu_open = false;
                if self.picker.take().is_none() {
                    self.picker = Some(Picker::default());
                    task = iced::widget::operation::focus(self.search_id());
                }
            }
            NoteMsg::ClosePicker => self.picker = None,
            NoteMsg::PickerQuery(query) => {
                if let Some(p) = &mut self.picker {
                    p.query = query;
                }
            }
            NoteMsg::StartRename => {
                self.menu_open = false;
                // A margin note is named by what it marks.
                if self.open_mark_index(sh).is_none() && sh.notes.contains_key(&self.path) {
                    self.renaming = Some(file_stem(&self.path));
                    self.picker = None;
                    task = iced::widget::operation::focus(self.rename_id());
                }
            }
            NoteMsg::RenameDraft(name) => {
                if self.renaming.is_some() {
                    self.renaming = Some(name);
                }
            }
            NoteMsg::CancelRename => self.renaming = None,
            NoteMsg::CommitRename => {
                if let Some(name) = self.renaming.clone() {
                    match sh.rename_note(&self.path, &name) {
                        Ok(new) => {
                            if new != self.path {
                                sh.toast(format!("Renamed to \u{201C}{}\u{201D}", name.trim()));
                            }
                            sh.status.clear();
                            self.rename_in_history(&self.path.clone(), &new);
                            self.path = new;
                            self.renaming = None;
                        }
                        // Stay in the box so the name can be fixed.
                        Err(why) => sh.status = why,
                    }
                }
            }
            NoteMsg::NewNote => {
                self.menu_open = false;
                self.choosing = true;
                self.new_title.clear();
                self.pending = None;
            }
            NoteMsg::PickKind(path) => {
                // A kind that is made at once (the daily note, named by its date) asks for nothing.
                if sh.templates().iter().any(|t| t.path == path && !t.ask_title) {
                    return self.update(sh, NoteMsg::CreateNote(path));
                }
                self.pending = Some(path);
                self.new_title.clear();
                task = iced::widget::operation::focus(new_title_id());
            }
            NoteMsg::OpenBacklinks => {
                self.menu_open = false;
                return (task, Some(Request::Backlinks(self.path.clone())));
            }
            NoteMsg::EditTemplate(path) => {
                self.choosing = false;
                self.pending = None;
                self.open(sh, path);
            }
            NoteMsg::NewTitle(title) => self.new_title = title,
            NoteMsg::BackToKinds => self.pending = None,
            NoteMsg::CancelNew => {
                self.choosing = false;
                self.pending = None;
            }
            NoteMsg::CreateNote(template) => {
                self.choosing = false;
                self.pending = None;
                // (A kind that asks for no title is not given the one typed.)
                let title = std::mem::take(&mut self.new_title);
                self.new_note(sh, &template, Some(&title));
            }
            NoteMsg::MarkStyle(style) => {
                if let Some(i) = self.open_mark_index(sh) {
                    // A plain mark has no color: it is let go of, and is not offered again. A mark made a highlight or an
                    // underline again, with no color of its own, takes the first pen's.
                    let had = sh.marks[i].color.clone();
                    let color = match style {
                        MarkStyle::None => String::new(),
                        _ if crate::bible_tab::hex_color(&had, 1.0).is_some() => had,
                        _ => sh.default_look().1,
                    };
                    self.mark_colors = false;
                    sh.set_mark_look(i, style, &color);
                }
            }
            NoteMsg::MarkColors => self.mark_colors = !self.mark_colors,
            NoteMsg::CloseColors => self.mark_colors = false,
            NoteMsg::MarkColor(n) => {
                self.mark_colors = false;
                if let (Some(i), Some(color)) = (self.open_mark_index(sh), crate::state::PEN_COLORS.get(n))
                    && sh.marks[i].style != MarkStyle::None
                {
                    let style = sh.marks[i].style;
                    sh.set_mark_look(i, style, color);
                }
            }
            NoteMsg::TagDraft(draft) => self.tag_draft = draft,
            NoteMsg::PickTag(tag) => {
                self.tag_draft.clear();
                self.add_tag(sh, &tag);
            }
            NoteMsg::AddTag => {
                let tag = std::mem::take(&mut self.tag_draft);
                self.add_tag(sh, &tag);
            }
            NoteMsg::RemoveTag(tag) => match self.open_mark_index(sh) {
                Some(i) => {
                    let tags = sh.marks[i].tags.iter().filter(|t| **t != tag).cloned().collect();
                    sh.set_mark_tags(i, tags);
                }
                None => self.change_properties(sh, |f| f.tags.retain(|t| *t != tag)),
            },
            NoteMsg::AddProperties => self.change_properties(sh, |f| {
                f.created.get_or_insert_with(frontmatter::now);
            }),
            NoteMsg::CopyLink => {
                self.menu_open = false;
                let link = link_to(sh, &self.path);
                sh.toast(format!("Copied {link}"));
                task = iced::clipboard::write(link);
            }
            NoteMsg::PickText => self.picking = true,
            NoteMsg::CancelPick => self.picking = false,
            NoteMsg::Unlink => {
                if let Some(i) = self.open_mark_index(sh) {
                    sh.unlink_mark(i, None);
                    sh.toast("Unlinked: it is an ordinary note now");
                }
            }
            NoteMsg::ToggleFront => self.front_open = !self.front_open,
            NoteMsg::Scrolled(y) => self.scroll = y,
            NoteMsg::SetKind(KindChoice(kind)) => self.change_properties(sh, |f| f.kind = kind),
            NoteMsg::PropEdit(i, typed) => {
                let kind = frontmatter::parse(self.doc.source()).front.props.get(i).and_then(|p| p.value.prop_type());
                let value = kind.and_then(|k| match k {
                    PropType::Text => Some(Value::Text(typed.clone())),
                    PropType::Number => typed.trim().parse::<f64>().ok().map(|_| Value::Number(typed.trim().to_string())),
                    PropType::Date => frontmatter::is_date(typed.trim()).then(|| Value::Date(typed.trim().to_string())),
                    PropType::List => Some(Value::List(
                        typed.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
                    )),
                    PropType::Bool => None,
                });
                self.drafts.insert(i, typed);
                if let Some(value) = value {
                    self.change_properties(sh, |f| {
                        if let Some(p) = f.props.get_mut(i) {
                            p.value = value;
                        }
                    });
                }
            }
            NoteMsg::PropBool(i, on) => self.change_properties(sh, |f| {
                if let Some(p) = f.props.get_mut(i) {
                    p.value = Value::Bool(on);
                }
            }),
            NoteMsg::PropRemove(i) => {
                self.drafts.clear();
                self.change_properties(sh, |f| {
                    if i < f.props.len() {
                        f.props.remove(i);
                    }
                });
            }
            NoteMsg::NewKey(key) => self.new_key = key,
            NoteMsg::NewType(kind) => self.new_type = kind,
            NoteMsg::ToggleAddProp => {
                self.adding = !self.adding;
                self.new_key.clear();
            }
            NoteMsg::AddProp => {
                let key = self.new_key.trim().to_string();
                let taken = |f: &Frontmatter| f.props.iter().any(|p| p.key.eq_ignore_ascii_case(&key));
                let fresh = frontmatter::parse(self.doc.source()).front;
                if !key.is_empty() && !key.contains(':') && !frontmatter::RESERVED_KEYS.iter().any(|r| r.eq_ignore_ascii_case(&key)) && !taken(&fresh) {
                    let kind = self.new_type;
                    self.drafts.clear();
                    self.change_properties(sh, |f| f.props.push(Property { key, value: Value::empty(kind) }));
                    self.new_key.clear();
                    self.adding = false;
                }
            }
            NoteMsg::PickerTag(tag) => {
                if let Some(p) = &mut self.picker
                    && !p.tags.remove(&tag)
                {
                    p.tags.insert(tag);
                }
            }
            NoteMsg::ClearPickerTags => {
                if let Some(p) = &mut self.picker {
                    p.tags.clear();
                }
            }
            NoteMsg::ToggleGroup(key) => {
                if let Some(p) = &mut self.picker
                    && !p.toggled.remove(&key)
                {
                    p.toggled.insert(key);
                }
            }
        }
        (task, None)
    }

    /// `reference` is the Bible tab this note works with: its module serves quotes and previews.
    /// The reading log's tab (`log_tab`): its words, written in like a note's, over the bottom bar. There is no header
    /// (the tab and the title in the text say what it is) and no way to choose, rename or change a note, and no properties.
    pub fn log_view<'a>(&'a self, sh: &'a Shared, reference: Option<&'a BibleTab>) -> Element<'a, NoteMsg> {
        let module = reference.map(|b| b.module.clone()).or_else(|| sh.backend.modules().first().map(|m| m.id.clone())).unwrap_or(ModuleId(String::new()));
        let mut editor = live_editor(&self.doc, Palette::for_theme(&sh.theme()))
            .id(self.editor_id())
            .on_action(NoteMsg::Action)
            .follow_links(true)
            .on_link(NoteMsg::OpenLink)
            .on_link_menu(NoteMsg::LinkMenu)
            .on_link_drag(NoteMsg::LinkDrag)
            .on_scroll(NoteMsg::Scrolled)
            .embed(move |target| sh.embed_for(&module, target))
            .placeholder("Nothing read yet")
            .size(sh.settings.notes_size)
            .font(sh.settings.text_font())
            .padding(16)
            .height(Length::Fill);
        if sh.settings.vim_mode {
            editor = editor.vim(&self.vim, NoteMsg::Vim);
        }
        // The bottom bar: the Vim mode line at its left and the pane's own buttons over its right end.
        let status: Element<'a, NoteMsg> = if sh.settings.vim_mode { self.vim_status() } else { space::horizontal().into() };
        let bar = container(status).width(Length::Fill).height(PANE_BUTTONS).align_y(iced::Center).padding([0, 16]);
        column![editor, rule::horizontal(1), bar].into()
    }

    pub fn view<'a>(&'a self, sh: &'a Shared, reference: Option<&'a BibleTab>) -> Element<'a, NoteMsg> {
        // The header bar, like the Bible tab's: the selector on the left, actions on the right.
        // Controls are as tall as the Bible tab's pickers so the two bars line up.
        let open = self.picker.is_some();
        let square = |icon: Element<'a, NoteMsg>, msg: NoteMsg| {
            button(container(icon).center(Length::Fill))
                .width(CONTROL_HEIGHT)
                .height(CONTROL_HEIGHT)
                .padding(0)
                .style(button::secondary)
                .on_press(msg)
        };
        // Who the note is: a selector for switching, or a box while it is being renamed.
        let who: Element<'a, NoteMsg> = match &self.renaming {
            Some(name) => row![
                text_input("Name", name)
                    .id(self.rename_id())
                    .on_input(NoteMsg::RenameDraft)
                    .on_submit(NoteMsg::CommitRename)
                    .size(16)
                    .padding([4, 10]),
                square(text("\u{00D7}").size(18).into(), NoteMsg::CancelRename),
            ]
            .spacing(6)
            .into(),
            None => button(
                container(
                    row![text(label_of(sh, &self.path)).size(16), space::horizontal(), crate::icons::Icon::chevron(open).view(16.0)]
                        .spacing(10)
                        .align_y(iced::Center),
                )
                .center_y(Length::Fill),
            )
            .width(Length::Fill)
            .height(CONTROL_HEIGHT)
            .padding([0, 10])
            .style(dropdown_style)
            .on_press(NoteMsg::TogglePicker)
            .into(),
        };
        let arrow = |icon: crate::icons::Icon, on: bool, msg: NoteMsg| {
            let b = button(container(icon.view(18.0)).center(Length::Fill))
                .width(CONTROL_HEIGHT)
                .height(CONTROL_HEIGHT)
                .padding(0)
                .style(button::secondary);
            if on { b.on_press(msg) } else { b }
        };
        let mut header = row![
            row![
                arrow(crate::icons::Icon::Back, self.history.can_back(), NoteMsg::Back),
                arrow(crate::icons::Icon::Forward, self.history.can_forward(), NoteMsg::Forward),
            ]
            .spacing(4),
            container(who).width(Length::Fill),
        ]
        .spacing(16)
        .align_y(iced::Center);
        // The menu at the right edge, as in the Bible tab.
        header = header.push(square(crate::icons::Icon::Menu.view(18.0), NoteMsg::ToggleMenu));

        // With no note at all there is nothing to select or edit: just a way to make one.
        let no_notes = sh.first_ordinary_note().is_empty() && !sh.notes.contains_key(&self.path);
        let pane: Element<'a, NoteMsg> = if no_notes {
            container(
                column![
                    text("No notes yet").size(14).style(text::secondary),
                    button(text("Create note").size(15)).padding([8, 18]).on_press(NoteMsg::NewNote),
                ]
                .spacing(12)
                .align_x(iced::Center),
            )
            .center(Length::Fill)
            .into()
        } else {
            let mut pane = column![container(header).padding([8, 16]), rule::horizontal(1)].spacing(0);

            // The note's properties, shown and edited here rather than as raw lines in the text. A margin note
            // is the same note with its passage's properties in them; the passage itself is shown under the text.
        if self.read_only() {
            pane = pane.push(
                container(text("Part of the library: it is kept as it was imported, to read and link to.").size(12).style(text::secondary)).padding([6, 16]),
            );
        } else if !self.is_log() {
            pane = pane.push(self.properties(sh));
        }
        // A clear line between the note's frontmatter (above) and its words (below): the log has no frontmatter to show.
        if !self.is_log() {
            pane = pane.push(container(rule::horizontal(2).style(|theme: &Theme| rule::Style {
            color: theme.extended_palette().background.strong.color,
            ..rule::default(theme)
        }))
        .padding([4, 0]));
        }

            let module = reference.map(|b| b.module.clone()).or_else(|| sh.backend.modules().first().map(|m| m.id.clone()));
            let module = module.unwrap_or(ModuleId(String::new()));
            let embed_module = module.clone();
            let mut editor = live_editor(&self.doc, Palette::for_theme(&sh.theme()))
                .id(self.editor_id())
                .on_action(NoteMsg::Action)
                .follow_links(true)
                .on_link(NoteMsg::OpenLink)
                .on_link_menu(NoteMsg::LinkMenu)
                .on_link_drag(NoteMsg::LinkDrag)
                .on_scroll(NoteMsg::Scrolled)
                .embed(move |target| sh.embed_for(&embed_module, target))
                .placeholder("Write here…")
                .size(sh.settings.notes_size)
                .font(sh.settings.text_font())
                .padding(16)
                .height(Length::Fill);
            if self.read_only() {
                // Read, selected and copied, not written in: no raw source is shown and no caret.
                editor = editor.reading(true).copy_as(crate::commentary_tab::plain_text);
            } else if sh.settings.vim_mode {
                editor = editor.vim(&self.vim, NoteMsg::Vim);
            }
            pane = pane.push(editor);
            // The bar under the text. It is always there: the pane's own buttons (split, fill the window) float
            // over its right end, and the Vim mode line is at its left. A margin note's passage, as the Bible
            // has it, is a card in the bar above that line, so the bar grows upward with it.
            let mut bar = column![].spacing(0);
            if let Some(mark) = self.open_mark_index(sh).and_then(|i| sh.marks.get(i)) {
                let card = container(scrollable(self.passage_block(sh, mark, reference))).max_height(200).style(|theme: &Theme| container::Style {
                    background: Some(theme.extended_palette().background.weak.color.into()),
                    border: iced::Border { radius: 8.0.into(), ..iced::Border::default() },
                    ..container::Style::default()
                });
                bar = bar.push(container(card).padding(iced::Padding { top: 8.0, right: 16.0, bottom: 0.0, left: 16.0 }));
            }
            let status: Element<'a, NoteMsg> = if sh.settings.vim_mode && !self.read_only() { self.vim_status() } else { space::horizontal().into() };
            bar = bar.push(container(status).width(Length::Fill).height(PANE_BUTTONS).align_y(iced::Center).padding([0, 16]));
            // (The bar itself has no color of its own: it is the tab's, with a line above it.)
            pane = pane.push(rule::horizontal(1)).push(container(bar).width(Length::Fill));
            pane.into()
        };

        // A pop-up, or the delete dialog, goes over the note. The two layers it takes are always
        // in the tree, empty when there is none, so that opening one does not rebuild the editor
        // beneath (which would lose where it was scrolled).
        let nothing = || -> Element<'a, NoteMsg> { space::horizontal().width(0).height(0).into() };
        let dismiss = |msg: NoteMsg| -> Element<'a, NoteMsg> {
            mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill)).on_press(msg).into()
        };
        let (behind, front): (Element<'a, NoteMsg>, Element<'a, NoteMsg>) = if self.choosing {
            template_dialog(sh, &self.new_title, self.pending.as_deref())
        } else if self.confirm_delete {
            let what = match self.open_mark_index(sh).and_then(|i| sh.marks.get(i)) {
                Some(mark) => Deleting::Margin(mark.anchor.to_string()),
                None => Deleting::Note(label_of(sh, &self.path)),
            };
            self.confirmation(what)
        } else if self.menu_open {
            let panel = container(opaque(self.menu_panel(sh)))
                .padding(iced::Padding { top: PANEL_TOP, right: 16.0, bottom: 0.0, left: 0.0 })
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right);
            (dismiss(NoteMsg::CloseMenu), panel.into())
        } else if let Some(picker) = &self.picker {
            // The panel sits over the note under the selector; a click anywhere else closes it.
            let panel = container(opaque(self.panel(sh, picker)))
                .padding(iced::Padding { top: PANEL_TOP, right: 16.0, bottom: 0.0, left: 16.0 })
                .width(Length::Fill)
                .height(Length::Fill);
            (dismiss(NoteMsg::ClosePicker), panel.into())
        } else {
            (nothing(), nothing())
        };
        stack![pane, behind, front].into()
    }

    /// The mode line under the text: "-- NORMAL --", and the command being typed.
    fn vim_status<'a>(&self) -> Element<'a, NoteMsg> {
        let mode = match self.vim.mode() {
            VimMode::Normal => "-- NORMAL --",
            VimMode::Insert => "-- INSERT --",
            VimMode::Visual => "-- VISUAL --",
            VimMode::VisualLine => "-- VISUAL LINE --",
        };
        container(row![text(mode).size(12), text(self.vim.pending().to_string()).size(12)].spacing(16))
            .width(Length::Fill)
            .style(|theme: &Theme| container::Style { text_color: Some(theme.palette().text.scale_alpha(0.6)), ..container::Style::default() })
            .into()
    }

    /// What can be done: start a new note, rename this one (not a margin note, which is named by
    /// its passage), delete it.
    fn menu_panel<'a>(&self, sh: &Shared) -> Element<'a, NoteMsg> {
        let item = |icon: crate::icons::Icon, label: &'static str, msg: NoteMsg| {
            button(row![icon.view(16.0), text(label).size(14)].spacing(10).align_y(iced::Center))
                .width(Length::Fill)
                .padding([6, 10])
                .style(button::text)
                .on_press(msg)
        };
        let mut items = column![item(crate::icons::Icon::Plus, "New note\u{2026}", NoteMsg::NewNote)].spacing(2);
        items = items.push(item(crate::icons::Icon::Link, "Copy link", NoteMsg::CopyLink));
        if self.open_mark_index(sh).is_none() {
            items = items.push(item(crate::icons::Icon::Backlinks, "Backlinks", NoteMsg::OpenBacklinks));
            if !self.read_only() && !self.is_log() {
                items = items.push(item(crate::icons::Icon::Pencil, "Rename", NoteMsg::StartRename));
            }
        }
        if !self.read_only() {
            items = items.push(item(crate::icons::Icon::Trash, "Delete", NoteMsg::AskDelete));
        }
        container(items).padding(6).width(170).style(crate::popup_style).into()
    }

    /// The layers of a dialog over the note: one that dims what is behind (nothing else can be
    /// clicked, and a click on the dimmed part cancels) and the dialog.
    fn confirmation<'a>(&self, what: Deleting) -> (Element<'a, NoteMsg>, Element<'a, NoteMsg>) {
        let (title, detail) = match what {
            Deleting::Margin(passage) => (
                "Delete this margin note?".to_string(),
                format!("The highlight on {passage} and its note file will be removed. This cannot be undone."),
            ),
            Deleting::Note(name) => (
                format!("Delete \u{201C}{name}\u{201D}?"),
                "The note's file will be removed from your vault. This cannot be undone.".to_string(),
            ),
        };
        let dim = mouse_area(
            container(space::horizontal()).width(Length::Fill).height(Length::Fill).style(|_: &Theme| container::Style {
                background: Some(iced::Color { a: 0.55, ..iced::Color::BLACK }.into()),
                ..container::Style::default()
            }),
        )
        .on_press(NoteMsg::CancelDelete)
        .into();
        let dialog = container(
            column![
                text(title).size(17),
                text(detail).size(13).style(text::secondary),
                row![
                    space::horizontal(),
                    button(text("Cancel").size(14)).padding([6, 14]).style(button::secondary).on_press(NoteMsg::CancelDelete),
                    button(text("Delete").size(14)).padding([6, 14]).style(button::danger).on_press(NoteMsg::ConfirmDelete),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(380)
        .style(container::rounded_box);
        let centred = container(opaque(dialog)).center(Length::Fill);
        (dim, centred.into())
    }

    /// The properties, above the text: a strip with when the note was made and its tags (and a
    /// button to open the full editor below it). A margin note's are the mark's, edited through it.
    fn properties<'a>(&'a self, sh: &'a Shared) -> Element<'a, NoteMsg> {
        let front = frontmatter::parse(self.doc.source()).front;
        let mark = self.open_mark_index(sh).and_then(|i| sh.marks.get(i));
        // The tags are above the fold, always there: the one thing most often changed. Under it is the section that
        // folds: the note's passage (or the way to give it one), when it was made, and the rest of the properties.
        let tags: &[String] = mark.map_or(&front.tags, |m| &m.tags);
        let created = match (mark, &front.created) {
            (Some(mark), _) => (!mark.created.is_empty()).then(|| mark.created.clone()),
            (None, created) => created.clone(),
        };
        let fold = |label: String| -> Element<'a, NoteMsg> {
            button(
                row![crate::icons::Icon::chevron(self.front_open).view(14.0), text(label).size(12).style(text::secondary)]
                    .spacing(6)
                    .align_y(iced::Center),
            )
            .padding([2, 8])
            .style(button::text)
            .on_press(NoteMsg::ToggleFront)
            .into()
        };
        let mut out = column![self.tags_block(sh, tags)];
        if !self.front_open {
            // One line that says what is in the section, and opens it.
            let mut parts: Vec<String> = Vec::new();
            match mark {
                Some(mark) => parts.push(mark.anchor.to_string()),
                None => {
                    if let Some(created) = &created {
                        parts.push(format!("Created {}", frontmatter::display(created)));
                    }
                    if !front.props.is_empty() {
                        parts.push(format!("{} propert{}", front.props.len(), if front.props.len() == 1 { "y" } else { "ies" }));
                    }
                }
            }
            let summary = if parts.is_empty() { "Frontmatter".to_string() } else { parts.join("  \u{00B7}  ") };
            let mut bar = row![fold(summary)].spacing(8).align_y(iced::Center);
            // Waiting for a selection is said here too, as the button that asked is out of sight.
            if self.picking {
                bar = bar
                    .push(text("Select text in a Bible\u{2026}").size(12).style(text::secondary))
                    .push(button(text("Cancel").size(12)).padding([2, 8]).style(button::text).on_press(NoteMsg::CancelPick));
            }
            return out.push(container(bar).padding([0, 8])).into();
        }
        out = out.push(container(fold("Frontmatter".to_string())).padding([0, 8]));
        if let Some(mark) = mark {
            out = out.push(self.reference_section(mark));
        } else if !templates::is_template(&self.path) {
            out = out.push(self.mark_section());
        }
        let mut strip = row![].spacing(8).align_y(iced::Center);
        match &created {
            Some(created) => {
                strip = strip.push(text(format!("Created {}", frontmatter::display(created))).size(12).style(text::secondary));
            }
            None => {
                strip = strip.push(text("No created time").size(12).style(text::secondary));
                if mark.is_none() {
                    strip = strip.push(button(text("Stamp it now").size(12)).padding([2, 8]).style(button::secondary).on_press(NoteMsg::AddProperties));
                }
            }
        }
        out = out.push(container(strip).padding([4, 16]));
        // The rest of the properties, to read and to change (a margin file has only the mark's own, which have their
        // controls above).
        if mark.is_none() {
            out = out.push(self.properties_editor(&front));
        }
        out.into()
    }

    /// The tags: the note's, each to take away, and the box to add one; then the tags in use that fit what is
    /// being typed, to pick instead of typing the rest.
    fn tags_block<'a>(&'a self, sh: &'a Shared, tags: &[String]) -> Element<'a, NoteMsg> {
        let quiet = || scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(0).scroller_width(0));
        let mut chips_row = row![].spacing(6).align_y(iced::Center);
        for tag in tags {
            let color = tag_color(sh, tag);
            chips_row = chips_row.push(
                button(row![text(format!("#{tag}")).size(13), text("\u{00D7}").size(15)].spacing(4).align_y(iced::Center))
                    .padding([3, 10])
                    .style(move |theme: &Theme, status| tag_chip_style(theme, status, false, color))
                    .on_press(NoteMsg::RemoveTag(tag.clone())),
            );
        }
        chips_row = chips_row.push(
            text_input("Add tag", &self.tag_draft)
                .on_input(NoteMsg::TagDraft)
                .on_submit(NoteMsg::AddTag)
                .size(13)
                .padding([4, 10])
                .style(tag_box_style)
                .width(130),
        );
        let mut out = column![container(scrollable(chips_row).direction(quiet())).padding([6, 16])];
        let suggestions = sh.tag_suggestions(&self.tag_draft, tags, 8);
        if !suggestions.is_empty() {
            let mut chips = row![text("Use").size(12).style(text::secondary)].spacing(6).align_y(iced::Center);
            for (tag, count) in suggestions {
                let color = tag_color(sh, &tag);
                chips = chips.push(
                    button(chip_label(&tag, count, false))
                        .padding([1, 8])
                        .style(move |theme: &Theme, status| tag_chip_style(theme, status, false, color))
                        .on_press(NoteMsg::PickTag(tag)),
                );
            }
            out = out.push(container(scrollable(chips).direction(quiet())).padding(iced::Padding { top: 0.0, right: 16.0, bottom: 6.0, left: 16.0 }));
        }
        out.into()
    }

    /// The boxed section the passage-related controls sit in.
    fn section_box<'a>(content: Element<'a, NoteMsg>) -> Element<'a, NoteMsg> {
        container(container(content).padding([8, 12]).width(Length::Fill).style(|theme: &Theme| {
            let p = theme.extended_palette();
            container::Style {
                background: Some(p.background.weak.color.into()),
                border: iced::Border { color: iced::Color { a: 0.5, ..p.background.strong.color }, width: 1.0, radius: 8.0.into() },
                ..container::Style::default()
            }
        }))
        .padding([4, 16])
        .into()
    }

    /// Reselect (or, while it waits, what to do) and Unlink: what changes where a margin note is.
    fn pick_buttons<'a>(&'a self, label: &'static str, extra: Option<Element<'a, NoteMsg>>) -> Element<'a, NoteMsg> {
        let mut buttons = row![].spacing(6).align_y(iced::Center);
        if self.picking {
            buttons = buttons
                .push(text("Select text in a Bible\u{2026}").size(12).style(text::secondary))
                .push(button(text("Cancel").size(12)).padding([3, 10]).style(button::text).on_press(NoteMsg::CancelPick));
        } else {
            buttons = buttons.push(button(text(label).size(12)).padding([3, 10]).style(button::secondary).on_press(NoteMsg::PickText));
        }
        if let Some(extra) = extra {
            buttons = buttons.push(extra);
        }
        buttons.into()
    }

    /// A margin note's reference: the passage and the words marked (as they are, not edited here), how it marks
    /// them and in what color, and the buttons that change where it is: Reselect takes the next selection in a
    /// Bible as its passage, Unlink lets it go from the text.
    fn reference_section<'a>(&'a self, mark: &'a study_core::margin::Mark) -> Element<'a, NoteMsg> {
        let faint = |t: String| text(t).size(12).style(text::secondary);
        // (Marked words belong to the module they were chosen in; whole verses to the reference, so say nothing.)
        let mut heading = row![text(mark.anchor.to_string()).size(14)].spacing(10).align_y(iced::Center);
        if !mark.quote.is_empty() {
            heading = heading.push(faint(format!("words in {}", mark.module)));
        }
        let style = pick_list(StyleChoice::ALL.to_vec(), Some(StyleChoice(mark.style)), |choice| NoteMsg::MarkStyle(choice.0)).text_size(13).padding([3, 8]);
        let color = crate::bible_tab::hex_color(&mark.color, 1.0).unwrap_or(iced::Color::WHITE);
        let open = self.mark_colors;
        let dot = button(space::horizontal())
            .width(22)
            .height(22)
            .padding(0)
            .style(move |theme: &Theme, _| button::Style {
                background: Some(color.into()),
                border: iced::Border {
                    radius: 11.0.into(),
                    color: if open { theme.extended_palette().background.base.text } else { iced::Color { a: 0.5, ..iced::Color::WHITE } },
                    width: if open { 2.5 } else { 1.0 },
                },
                ..button::Style::default()
            })
            .on_press(NoteMsg::MarkColors);
        // The colors are a panel over the window under the swatch: it does not make the section taller.
        let panel = open.then(|| {
            container(crate::color_grid(
                crate::state::PEN_COLORS.iter().position(|c| c.eq_ignore_ascii_case(&mark.color)).unwrap_or(usize::MAX),
                NoteMsg::MarkColor,
            ))
            .padding(10)
            .style(crate::popup_style)
            .into()
        });
        // (A plain mark has no color to choose.)
        let look: Element<'a, NoteMsg> = if mark.style == MarkStyle::None {
            row![style].into()
        } else {
            row![style, popover(dot, panel, NoteMsg::CloseColors)].spacing(8).align_y(iced::Center).into()
        };
        let unlink = button(text("Unlink").size(12)).padding([3, 10]).style(button::secondary).on_press(NoteMsg::Unlink).into();
        // The section is called what the other kind of note has the button for.
        let mut body = column![text("Mark").size(11).style(text::secondary), heading].spacing(4);
        if !mark.quote.is_empty() {
            body = body.push(text(format!("\u{201C}{}\u{201D}", mark.quote)).size(13).font(iced::Font { style: iced::font::Style::Italic, ..iced::Font::DEFAULT }));
        }
        body = body.push(row![look, space::horizontal(), self.pick_buttons("Reselect", Some(unlink))].spacing(10).align_y(iced::Center));
        Self::section_box(body.into())
    }

    /// What a note that is not on a passage has where a margin note has its reference: Mark, which takes the
    /// next selection in a Bible and makes the note a margin note on it.
    fn mark_section<'a>(&'a self) -> Element<'a, NoteMsg> {
        let row = row![
            text("Not on a passage").size(12).style(text::secondary).width(Length::Fill),
            self.pick_buttons("Mark", None),
        ]
        .spacing(10)
        .align_y(iced::Center);
        Self::section_box(row.into())
    }

    /// The passage a margin note is on, as the Bible has it, with the words that were marked in the
    /// mark's color: the reference at its head is a link to it, as any `[[John 3:16]]` is (right
    /// click for where to open it, pull it away to carry it).
    fn passage_block<'a>(&'a self, sh: &'a Shared, mark: &'a study_core::margin::Mark, reference: Option<&BibleTab>) -> Element<'a, NoteMsg> {
        use iced::widget::{rich_text, span};
        let module = (!mark.module.is_empty())
            .then(|| sh.backend.modules().into_iter().find(|m| m.abbreviation == mark.module).map(|m| m.id))
            .flatten()
            .or_else(|| reference.map(|b| b.module.clone()))
            .or_else(|| sh.backend.modules().first().map(|m| m.id.clone()));
        let target = LinkTarget::Verse { range: mark.anchor, module: None };
        let found = module.as_ref().and_then(|m| sh.embed_for(m, &target));
        let title = found.as_ref().map_or_else(|| mark.anchor.to_string(), |e| e.title.clone());
        let reference_link = button(text(title).size(13))
            .padding(0)
            .style(|theme: &Theme, status| {
                let link = crate::palette::Palette::for_theme(theme).link;
                let shown = if matches!(status, button::Status::Hovered) { iced::Color { a: 0.8, ..link } } else { link };
                button::Style { text_color: shown, ..button::Style::default() }
            })
            .on_press(NoteMsg::OpenLink(target.clone()));
        let (for_menu, for_pull) = (target.clone(), target);
        let reference_link = crate::link_area::link_area(
            reference_link,
            move |at| NoteMsg::LinkMenu(for_menu.clone(), at),
            move |at| NoteMsg::LinkDrag(for_pull.clone(), at),
        );
        let size = sh.settings.notes_size * 0.95;
        let mut block = column![reference_link].spacing(6);
        if let Some(found) = found {
            let font = sh.settings.text_font();
            let marked = crate::bible_tab::hex_color(mark.shown_color(), if sh.is_dark() { 0.4 } else { 0.5 }).unwrap_or(iced::Color::TRANSPARENT);
            let mut spans: Vec<iced::widget::text::Span<'a, ()>> = Vec::new();
            match (!mark.quote.is_empty()).then(|| found.body.find(&mark.quote)).flatten() {
                Some(at) => {
                    let end = at + mark.quote.len();
                    spans.push(span(found.body[..at].to_string()).font(font));
                    spans.push(span(found.body[at..end].to_string()).font(font).background(marked).padding(1));
                    spans.push(span(found.body[end..].to_string()).font(font));
                }
                None => spans.push(span(found.body).font(font)),
            }
            block = block.push(rich_text(spans).size(size));
        }
        container(block).padding([10, 14]).width(Length::Fill).into()
    }

    /// Everything about the note that is not its text. The app's own (title, type, tags,
    /// created) get fitting controls; every other property gets a generic editor for its type.
    fn properties_editor(&self, front: &Frontmatter) -> Element<'_, NoteMsg> {
        let label = |name: &str| text(name.to_string()).size(12).style(text::secondary).width(90);
        let mut kinds: Vec<KindChoice> = std::iter::once(KindChoice(None))
            .chain(frontmatter::KINDS.iter().map(|k| KindChoice(Some((*k).to_string()))))
            .collect();
        if let Some(own) = front.kind.as_ref().filter(|k| !frontmatter::KINDS.contains(&k.as_str())) {
            kinds.push(KindChoice(Some(own.clone())));
        }
        let mut rows = column![
            row![label("Type"), pick_list(kinds, Some(KindChoice(front.kind.clone())), NoteMsg::SetKind).text_size(13).padding([3, 8])]
                .spacing(8)
                .align_y(iced::Center),
        ]
        .spacing(6);
        for (i, prop) in front.props.iter().enumerate() {
            let shown = |v: &Value| match v {
                Value::List(items) => items.join(", "),
                Value::Text(t) | Value::Number(t) | Value::Date(t) => t.clone(),
                Value::Bool(_) | Value::Raw(_) => String::new(),
            };
            let control: Element<'_, NoteMsg> = match &prop.value {
                Value::Bool(on) => checkbox(*on).on_toggle(move |v| NoteMsg::PropBool(i, v)).into(),
                Value::Raw(lines) => text(format!("Not editable here: {}", lines.first().map_or("", String::as_str).trim()))
                    .size(12)
                    .style(text::secondary)
                    .into(),
                value => {
                    let draft = self.drafts.get(&i).cloned().unwrap_or_else(|| shown(value));
                    let hint = match value.prop_type() {
                        Some(PropType::Date) => "YYYY-MM-DD",
                        Some(PropType::List) => "comma, separated",
                        Some(PropType::Number) => "0",
                        _ => "",
                    };
                    text_input(hint, &draft).on_input(move |t| NoteMsg::PropEdit(i, t)).size(13).padding([3, 6]).into()
                }
            };
            let name = if prop.key.is_empty() { "(other)".to_string() } else { prop.key.clone() };
            let kind = prop.value.prop_type().map_or(String::new(), |k| format!(" \u{00B7} {}", k.label().to_lowercase()));
            rows = rows.push(
                row![
                    text(format!("{name}{kind}")).size(12).style(text::secondary).width(90),
                    control,
                    button(text("\u{00D7}").size(14)).padding([0, 6]).style(button::text).on_press(NoteMsg::PropRemove(i)),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
        }
        let reserved = frontmatter::RESERVED_KEYS.iter().any(|r| r.eq_ignore_ascii_case(self.new_key.trim()));
        let ok = !self.new_key.trim().is_empty()
            && !self.new_key.contains(':')
            && !reserved
            && !front.props.iter().any(|p| p.key.eq_ignore_ascii_case(self.new_key.trim()));
        // The way to add one is a plus button; its boxes show when it is pressed (and go when one is added).
        rows = rows.push(if self.adding {
            row![
                label("New property"),
                text_input("name", &self.new_key).on_input(NoteMsg::NewKey).on_submit(NoteMsg::AddProp).size(13).padding([3, 6]).width(140),
                pick_list(PropType::ALL.to_vec(), Some(self.new_type), NoteMsg::NewType).text_size(13).padding([3, 8]),
                button(text("Add").size(12)).padding([3, 10]).style(button::secondary).on_press_maybe(ok.then_some(NoteMsg::AddProp)),
                button(text("\u{00D7}").size(14)).padding([0, 6]).style(button::text).on_press(NoteMsg::ToggleAddProp),
            ]
            .spacing(8)
            .align_y(iced::Center)
        } else {
            // In the color of the names of the rows above it, and under them: not a button to look at, a place to press.
            row![
                button(text("+  Property").size(12))
                    .padding([2, 0])
                    .style(|theme: &Theme, status| {
                        let faint = text::secondary(theme).color.unwrap_or(theme.extended_palette().background.base.text);
                        let shown = if matches!(status, button::Status::Hovered | button::Status::Pressed) { theme.extended_palette().background.base.text } else { faint };
                        button::Style { text_color: shown, ..button::Style::default() }
                    })
                    .on_press(NoteMsg::ToggleAddProp)
            ]
        });
        // It scrolls inside a limited height, so a long list never leaves the text no room.
        container(scrollable(container(rows).padding(iced::Padding { top: 4.0, right: 12.0, bottom: 4.0, left: 0.0 })))
            .padding(iced::Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 16.0 })
            .max_height(200)
            .into()
    }

    /// The selector panel: a search box, then each type of note with its notes.
    fn panel<'a>(&'a self, sh: &'a Shared, picker: &'a Picker) -> Element<'a, NoteMsg> {
        let search = text_input("Search notes or #tags…", &picker.query)
            .id(self.search_id())
            .on_input(NoteMsg::PickerQuery)
            .size(14)
            .padding(6);
        // The tags to narrow by: the ones chosen first, then the most used.
        let counts = sh.tag_counts();
        let mut shown: Vec<(String, usize)> = picker
            .tags
            .iter()
            .map(|t| (t.clone(), counts.iter().find(|(c, _)| c == t).map_or(0, |(_, n)| *n)))
            .collect();
        shown.extend(counts.iter().filter(|(t, _)| !picker.tags.contains(t)).take(14).cloned());
        let mut chips: Vec<Element<'a, NoteMsg>> = Vec::new();
        for (tag, count) in shown {
            let on = picker.tags.contains(&tag);
            let color = tag_color(sh, &tag);
            let chip = button(chip_label(&tag, count, on))
                .padding([1, 8])
                .style(move |theme: &Theme, status| tag_chip_style(theme, status, on, color))
                .on_press(NoteMsg::PickerTag(tag.clone()));
            chips.push(chip.into());
        }
        let chips = iced::widget::Row::with_children(chips).spacing(4).wrap().vertical_spacing(4);
        let all = groups(sh, &picker.query, &picker.tags);
        let mut list = column![].spacing(2);
        if all.is_empty() {
            list = list.push(text("No note matches.").size(13).style(text::secondary));
        }
        for group in &all {
            let open = picker.is_open(group, &self.path);
            // The ordinary notes are just the list; only the margin notes fold away.
            if group.key != "notes" {
                // A folder: a band of its own, in bold, with its count, where the notes are plain.
                let bold = iced::Font { weight: iced::font::Weight::Bold, ..crate::fonts::UI };
                let header = button(
                    row![
                        text(if open { "\u{25BE}" } else { "\u{25B8}" }).size(13),
                        text(group.label.clone()).size(13).font(bold),
                        space::horizontal(),
                        text(group.notes.len().to_string()).size(12),
                    ]
                    .spacing(8)
                    .align_y(iced::Center),
                )
                .width(Length::Fill)
                .padding([5, 8])
                .style(|theme: &Theme, status| {
                    let palette = theme.extended_palette();
                    let shade = if matches!(status, button::Status::Hovered) { 0.9 } else { 0.7 };
                    button::Style {
                        background: Some(iced::Color { a: shade, ..palette.background.strong.color }.into()),
                        text_color: palette.background.base.text,
                        border: iced::Border { radius: 4.0.into(), ..iced::Border::default() },
                        ..button::Style::default()
                    }
                })
                .on_press(NoteMsg::ToggleGroup(group.key.clone()));
                list = list.push(header);
            }
            if open {
                for (path, label) in &group.notes {
                    let current = *path == self.path;
                    let tags: Vec<String> = sh.tags_of(path).iter().map(|t| frontmatter::normalize_tag(t)).collect();
                    let tag_line = tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ");
                    list = list.push(
                        button(
                            row![
                                text(label.clone()).size(13),
                                space::horizontal(),
                                text(tag_line).size(11).style(text::secondary),
                            ]
                            .spacing(8)
                            .align_y(iced::Center),
                        )
                        .width(Length::Fill)
                        .padding(iced::Padding { top: 3.0, right: 8.0, bottom: 3.0, left: if group.key == "notes" { 8.0 } else { 28.0 } })
                        .style(if current { button::secondary } else { button::text })
                        .on_press(NoteMsg::PickNote(NoteChoice(path.clone()))),
                    );
                }
            }
        }
        let mut top = column![search, chips].spacing(8);
        if !picker.tags.is_empty() {
            top = top.push(button(text("Clear tags").size(12)).padding([1, 6]).style(button::text).on_press(NoteMsg::ClearPickerTags));
        }
        container(column![top, scrollable(list).height(Length::Shrink)].spacing(8))
            .padding(10)
            .width(Length::Fill)
            .max_height(460)
            .style(crate::popup_style)
            .into()
    }
}
