//! The note tab: one markdown document (a note, or a mark's margin file) in the live editor.
//! Like the Bible tab it has a header bar: a selector for which note it holds (a panel of
//! notes grouped by type, with an optional search), and delete for a margin note.
//! Below the editor, a peek at the link under the cursor.
//! Each note tab holds one document.

use crate::bible_tab::{BibleTab, start_of_note};
use crate::live_editor::live_editor;
use crate::palette::Palette;
use crate::shared::{Shared, TabId};
use iced::widget::{
    button, checkbox, column, container, mouse_area, opaque, pick_list, row, rule, scrollable, space, stack, text,
    text_input,
};
use iced::{Element, Length, Task, Theme};
use md_editor::{Action as EditAction, Document};
use scripture::RefRange;
use std::collections::BTreeSet;
use std::fmt;
use study_core::ModuleId;
use std::collections::BTreeMap;
use study_core::frontmatter::{self, Frontmatter, PropType, Property, Value};
use study_core::margin::MarkStyle;
use study_core::markdown::{LinkTarget, WikiLink, find_links, resolve_note};

/// Height of a header bar control: a pick list in the Bible tab's toolbar is this tall, so the
/// bars of the two tab types match.
const CONTROL_HEIGHT: f32 = 31.0;
/// Where the selector panel starts: just under the selector (its padding, its height, a gap).
const PANEL_TOP: f32 = 8.0 + CONTROL_HEIGHT + 3.0;

/// A button that looks like the Bible tab's pick lists, for the note selector.
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
    /// Open the link under the editor cursor, or one clicked in the text.
    OpenLink(LinkTarget),
    PickNote(NoteChoice),
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
    PickerQuery(String),
    ToggleGroup(String),
    /// A new note, made with its properties in place.
    NewNote,
    // The properties strip
    TagDraft(String),
    AddTag,
    RemoveTag(String),
    /// Give a note that has none its frontmatter, stamped with the time.
    AddProperties,
    // The properties editor
    ToggleProps,
    SetTitle(String),
    SetKind(KindChoice),
    /// Text typed into the value of the property at this place.
    PropEdit(usize, String),
    PropBool(usize, bool),
    PropRemove(usize),
    NewKey(String),
    NewType(PropType),
    AddProp,
}

/// A kind of note for the type picker: none, or one of the app's, or one written by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindChoice(pub Option<String>);

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

/// Something the tab cannot do alone: it concerns a Bible tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    ShowVerse(RefRange),
    Word { module: String, key: String },
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
    /// Groups the person opened or closed by hand, against their usual state: the group of
    /// the open note starts open, the rest closed.
    pub toggled: BTreeSet<String>,
}

pub struct NoteTab {
    pub id: TabId,
    pub path: String,
    pub doc: Document,
    pub picker: Option<Picker>,
    /// The confirmation to delete the open margin note is showing.
    pub confirm_delete: bool,
    /// What is being typed in the strip's tag box.
    pub tag_draft: String,
    /// The name being typed, while the note is being renamed.
    pub renaming: Option<String>,
    /// The full properties editor is open.
    pub props_open: bool,
    /// What a value box shows while what is typed is not (yet) a value of its type: a number
    /// half typed, a list with its comma still to be followed. By place in the properties.
    pub drafts: BTreeMap<usize, String>,
    /// The new property being named.
    pub new_key: String,
    pub new_type: PropType,
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

/// The type of note a path is: by its top folder.
fn group_of(path: &str) -> (String, String) {
    let top = path.split_once('/').map_or("notes", |(top, _)| top);
    let label = match top {
        "notes" => "Notes".to_string(),
        "margins" => "Margin notes".to_string(),
        other => {
            let mut c = other.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }
    };
    (top.to_string(), label)
}

/// Every note, grouped by type: Notes first, other types alphabetically, margin notes last. A
/// `query` keeps only the notes whose name or text contains it (any case).
pub fn groups(sh: &Shared, query: &str) -> Vec<Group> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<Group> = Vec::new();
    for (path, body) in sh.notes.iter() {
        let label = label_of(sh, path);
        if !q.is_empty() && !label.to_lowercase().contains(&q) && !path.to_lowercase().contains(&q) && !body.to_lowercase().contains(&q) {
            continue;
        }
        let (key, group_label) = group_of(path);
        match out.iter_mut().find(|g| g.key == key) {
            Some(g) => g.notes.push((path.clone(), label)),
            None => out.push(Group { key, label: group_label, notes: vec![(path.clone(), label)] }),
        }
    }
    let rank = |key: &str| match key {
        "notes" => 0,
        "margins" => 2,
        _ => 1,
    };
    out.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then(a.label.cmp(&b.label)));
    for g in &mut out {
        // Margin notes keep their order in the canon; the rest go by name. When searching,
        // notes whose name matches come before those that only mention it.
        if g.key != "margins" {
            g.notes.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        }
        if !q.is_empty() {
            g.notes.sort_by_key(|(_, label)| !label.to_lowercase().contains(&q));
        }
    }
    out
}

impl Picker {
    /// Whether a group shows its notes: always while searching, else as the person left it.
    pub fn is_open(&self, group: &Group, current: &str) -> bool {
        if !self.query.trim().is_empty() {
            return true;
        }
        let usual = group.notes.iter().any(|(p, _)| p == current);
        usual != self.toggled.contains(&group.key)
    }
}

impl NoteTab {
    pub fn new(id: TabId, sh: &Shared, path: String) -> Self {
        let doc = start_of_note(sh.notes.get(&path).map_or("", String::as_str));
        NoteTab {
            id,
            path,
            doc,
            picker: None,
            confirm_delete: false,
            tag_draft: String::new(),
            renaming: None,
            props_open: false,
            drafts: BTreeMap::new(),
            new_key: String::new(),
            new_type: PropType::Text,
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

    pub fn open(&mut self, sh: &mut Shared, path: String) {
        let text = sh.notes.get(&path).cloned().unwrap_or_default();
        self.doc = start_of_note(&text);
        self.path = path;
        self.picker = None;
        self.renaming = None;
        self.confirm_delete = false;
        self.tag_draft.clear();
        self.drafts.clear();
        sh.status.clear();
    }

    /// Reload the document from the shared text (it changed behind the tab).
    pub fn reload(&mut self, sh: &Shared) {
        self.doc = start_of_note(sh.notes.get(&self.path).map_or("", String::as_str));
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

    /// Make a new note, named so as not to clash, and open it.
    fn new_note(&mut self, sh: &mut Shared) {
        let path = (1..)
            .map(|n| if n == 1 { "notes/Untitled.md".to_string() } else { format!("notes/Untitled {n}.md") })
            .find(|p| !sh.notes.contains_key(p))
            .unwrap_or_default();
        let title = path.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string();
        sh.notes.insert(path.clone(), frontmatter::new_note(Some(&title)));
        self.open(sh, path);
    }

    /// Index of the mark whose file is open here.
    pub fn open_mark_index(&self, sh: &Shared) -> Option<usize> {
        sh.mark_index_of(&self.path)
    }

    fn link_at_cursor(&self) -> Option<WikiLink> {
        let head = self.doc.selection().head;
        let line = self.doc.line_range(head);
        let column = head - line.start;
        find_links(&self.doc.source()[line]).into_iter().find(|w| w.span.start <= column && column <= w.span.end)
    }

    pub fn update(&mut self, sh: &mut Shared, message: NoteMsg) -> (Task<NoteMsg>, Option<Request>) {
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
            }
            NoteMsg::AskDelete => {
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
            NoteMsg::OpenLink(target) => match target {
                LinkTarget::Verse { range, .. } => return (task, Some(Request::ShowVerse(range))),
                LinkTarget::Note(name) => match resolve_note(&sh.note_paths(), &name) {
                    Some(p) => self.open(sh, p.clone()),
                    None => sh.status = format!("No note named \"{name}\" yet"),
                },
                LinkTarget::Sword { module, key } => return (task, Some(Request::Word { module, key })),
            },
            NoteMsg::PickNote(NoteChoice(path)) => self.open(sh, path),
            NoteMsg::TogglePicker => {
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
                            self.path = new;
                            self.renaming = None;
                        }
                        // Stay in the box so the name can be fixed.
                        Err(why) => sh.status = why,
                    }
                }
            }
            NoteMsg::NewNote => self.new_note(sh),
            NoteMsg::TagDraft(draft) => self.tag_draft = draft,
            NoteMsg::AddTag => {
                let tag = std::mem::take(&mut self.tag_draft);
                self.change_properties(sh, |f| {
                    f.add_tag(&tag);
                });
            }
            NoteMsg::RemoveTag(tag) => self.change_properties(sh, |f| f.tags.retain(|t| *t != tag)),
            NoteMsg::AddProperties => self.change_properties(sh, |f| {
                f.created.get_or_insert_with(frontmatter::now);
            }),
            NoteMsg::ToggleProps => self.props_open = !self.props_open,
            NoteMsg::SetTitle(title) => {
                self.change_properties(sh, |f| f.title = (!title.is_empty()).then_some(title));
            }
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
            NoteMsg::AddProp => {
                let key = self.new_key.trim().to_string();
                let taken = |f: &Frontmatter| f.props.iter().any(|p| p.key.eq_ignore_ascii_case(&key));
                let fresh = frontmatter::parse(self.doc.source()).front;
                if !key.is_empty() && !key.contains(':') && !frontmatter::RESERVED_KEYS.iter().any(|r| r.eq_ignore_ascii_case(&key)) && !taken(&fresh) {
                    let kind = self.new_type;
                    self.drafts.clear();
                    self.change_properties(sh, |f| f.props.push(Property { key, value: Value::empty(kind) }));
                    self.new_key.clear();
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
        let mut header = row![container(who).max_width(360).width(Length::Fill), space::horizontal().width(Length::Shrink)]
            .spacing(16)
            .align_y(iced::Center);
        // A margin note is named by the passage it marks, so only ordinary notes are renamed.
        if self.renaming.is_none() && self.open_mark_index(sh).is_none() {
            header = header.push(square(crate::icons::Icon::Pencil.view(18.0), NoteMsg::StartRename));
        }
        header = header.push(square(crate::icons::Icon::Trash.view(18.0), NoteMsg::AskDelete));

        let mut pane = column![container(header).padding([8, 16]), rule::horizontal(1)].spacing(0);

        // The note's properties, shown and edited here rather than as raw lines in the text.
        pane = pane.push(self.properties(sh));

        let module = reference.map(|b| b.module.clone()).or_else(|| sh.backend.modules().first().map(|m| m.id.clone()));
        let module = module.unwrap_or(ModuleId(String::new()));
        let embed_module = module.clone();
        let editor = live_editor(&self.doc, Palette::for_theme(&sh.theme()))
            .id(self.editor_id())
            .on_action(NoteMsg::Action)
            .follow_links(true)
            .on_link(NoteMsg::OpenLink)
            .embed(move |target| sh.embed_for(&embed_module, target))
            .placeholder("Write here…")
            .size(17)
            .padding(16)
            .height(Length::Fill);
        pane = pane.push(editor).push(rule::horizontal(1)).push(self.peek(sh, &module));

        if self.confirm_delete {
            let what = match self.open_mark_index(sh).and_then(|i| sh.marks.get(i)) {
                Some(mark) => Deleting::Margin(mark.anchor.to_string()),
                None => Deleting::Note(label_of(sh, &self.path)),
            };
            return self.confirmation(pane.into(), what);
        }
        match &self.picker {
            None => pane.into(),
            // The panel sits over the note under the selector; a click anywhere else closes it.
            Some(picker) => {
                let dismiss = mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill))
                    .on_press(NoteMsg::ClosePicker);
                let panel = container(opaque(self.panel(sh, picker)))
                    .padding(iced::Padding { top: PANEL_TOP, right: 0.0, bottom: 0.0, left: 16.0 })
                    .width(Length::Fill)
                    .height(Length::Fill);
                stack![pane, dismiss, panel].into()
            }
        }
    }

    /// The note with a dialog over it that dims what is behind: nothing else can be clicked, and
    /// a click on the dimmed part cancels.
    fn confirmation<'a>(&self, under: Element<'a, NoteMsg>, what: Deleting) -> Element<'a, NoteMsg> {
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
        .on_press(NoteMsg::CancelDelete);
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
        stack![under, dim, centred].into()
    }

    /// The properties, above the text: a strip with when the note was made and its tags (and a
    /// button to open the full editor below it). A margin note's are the mark's, shown not edited.
    fn properties<'a>(&'a self, sh: &'a Shared) -> Element<'a, NoteMsg> {
        let front = frontmatter::parse(self.doc.source()).front;
        if let Some(mark) = self.open_mark_index(sh).and_then(|i| sh.marks.get(i)) {
            return self.mark_summary(mark);
        }
        let mut strip = row![].spacing(8).align_y(iced::Center);
        match &front.created {
            Some(created) => {
                strip = strip.push(text(format!("Created {}", frontmatter::display(created))).size(12).style(text::secondary));
            }
            None => {
                strip = strip
                    .push(text("No created time").size(12).style(text::secondary))
                    .push(button(text("Stamp it now").size(12)).padding([2, 8]).style(button::secondary).on_press(NoteMsg::AddProperties));
            }
        }
        for tag in &front.tags {
            strip = strip.push(
                button(row![text(format!("#{tag}")).size(12), text("\u{00D7}").size(14)].spacing(4).align_y(iced::Center))
                    .padding([1, 8])
                    .style(button::secondary)
                    .on_press(NoteMsg::RemoveTag(tag.clone())),
            );
        }
        strip = strip.push(
            text_input("Add tag", &self.tag_draft)
                .on_input(NoteMsg::TagDraft)
                .on_submit(NoteMsg::AddTag)
                .size(12)
                .padding([2, 6])
                .width(90),
        );
        let toggle = button(
            row![text("Properties").size(12), crate::icons::Icon::chevron(self.props_open).view(14.0)]
                .spacing(4)
                .align_y(iced::Center),
        )
        .padding([2, 8])
        .style(button::text)
        .on_press(NoteMsg::ToggleProps);
        let bar = row![
            scrollable(strip).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(0).scroller_width(0))).width(Length::Fill),
            toggle,
        ]
        .spacing(8)
        .align_y(iced::Center);
        let mut out = column![container(bar).padding([6, 16])];
        if self.props_open {
            out = out.push(self.properties_editor(&front));
        }
        out.into()
    }

    /// What is known of a margin note's mark, for reading: the file's lines are the app's.
    fn mark_summary<'a>(&self, mark: &'a study_core::margin::Mark) -> Element<'a, NoteMsg> {
        let mut bits = vec![format!("{}", mark.anchor), format!("{:?}", mark.style).to_lowercase()];
        if !mark.module.is_empty() {
            bits.push(mark.module.clone());
        }
        if !mark.created.is_empty() {
            bits.push(format!("created {}", frontmatter::display(&mark.created)));
        }
        bits.extend(mark.tags.iter().map(|t| format!("#{t}")));
        container(text(bits.join("  \u{00B7}  ")).size(12).style(text::secondary)).padding([6, 16]).into()
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
            row![label("Title"), text_input("Untitled", front.title.as_deref().unwrap_or("")).on_input(NoteMsg::SetTitle).size(13).padding([3, 6])]
                .spacing(8)
                .align_y(iced::Center),
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
        rows = rows.push(
            row![
                label("New property"),
                text_input("name", &self.new_key).on_input(NoteMsg::NewKey).on_submit(NoteMsg::AddProp).size(13).padding([3, 6]).width(140),
                pick_list(PropType::ALL.to_vec(), Some(self.new_type), NoteMsg::NewType).text_size(13).padding([3, 8]),
                button(text("Add").size(12)).padding([3, 10]).style(button::secondary).on_press_maybe(ok.then_some(NoteMsg::AddProp)),
            ]
            .spacing(8)
            .align_y(iced::Center),
        );
        // It scrolls inside a limited height, so a long list never leaves the text no room.
        container(scrollable(container(rows).padding(iced::Padding { top: 4.0, right: 12.0, bottom: 4.0, left: 0.0 })))
            .padding(iced::Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 16.0 })
            .max_height(200)
            .into()
    }

    /// The selector panel: a search box, then each type of note with its notes.
    fn panel<'a>(&'a self, sh: &'a Shared, picker: &'a Picker) -> Element<'a, NoteMsg> {
        let search = text_input("Search notes…", &picker.query)
            .id(self.search_id())
            .on_input(NoteMsg::PickerQuery)
            .size(14)
            .padding(6);
        let all = groups(sh, &picker.query);
        let mut list = column![].spacing(2);
        list = list.push(
            button(text("+ New note").size(13))
                .width(Length::Fill)
                .padding([4, 8])
                .style(button::text)
                .on_press(NoteMsg::NewNote),
        );
        if all.is_empty() {
            list = list.push(text("No note matches.").size(13).style(text::secondary));
        }
        for group in &all {
            let open = picker.is_open(group, &self.path);
            let header = button(
                row![
                    text(if open { "▾" } else { "▸" }).size(12),
                    text(group.label.clone()).size(13),
                    space::horizontal(),
                    text(group.notes.len().to_string()).size(12).style(text::secondary),
                ]
                .spacing(8)
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .padding([4, 8])
            .style(button::text)
            .on_press(NoteMsg::ToggleGroup(group.key.clone()));
            list = list.push(header);
            if open {
                for (path, label) in &group.notes {
                    let current = *path == self.path;
                    list = list.push(
                        button(text(label.clone()).size(13))
                            .width(Length::Fill)
                            .padding(iced::Padding { top: 3.0, right: 8.0, bottom: 3.0, left: 28.0 })
                            .style(if current { button::secondary } else { button::text })
                            .on_press(NoteMsg::PickNote(NoteChoice(path.clone()))),
                    );
                }
            }
        }
        container(column![search, scrollable(list).height(Length::Shrink)].spacing(8))
            .padding(10)
            .width(300)
            .max_height(420)
            .style(container::rounded_box)
            .into()
    }

    /// Preview of the link under the editor cursor, with a way to follow it.
    fn peek<'a>(&'a self, sh: &'a Shared, module: &ModuleId) -> Element<'a, NoteMsg> {
        let body: Element<'_, NoteMsg> = match self.link_at_cursor() {
            None => text("Place the cursor in a [[link]] to peek at it.").size(13).style(text::secondary).into(),
            Some(link) => {
                let preview = sh.preview(module, &link.target).unwrap_or_else(|| "No preview available.".into());
                let label = match link.target {
                    LinkTarget::Verse { .. } => "Show in reader",
                    LinkTarget::Note(_) => "Open note",
                    LinkTarget::Sword { .. } => "Look up",
                };
                row![
                    column![text(link.label()).size(15), text(preview).size(13)].spacing(4).width(Length::Fill),
                    button(text(label).size(13)).style(button::secondary).on_press(NoteMsg::OpenLink(link.target)),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Start)
                .into()
            }
        };
        container(scrollable(body)).padding(12).width(Length::Fill).height(110).into()
    }
}
