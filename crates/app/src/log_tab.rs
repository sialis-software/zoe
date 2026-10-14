//! The reading log's tab: the log (`Reading log.md`, which grows down: see `study_core::reading_log`), written in like a
//! note, in a tab of its own, shown at its end. It is the app's, and not one of the notes: no note leads to it and it leads to none (the
//! notes selector, links and backlinks leave it out, and a link in it follows only to a verse or a web address), it has no
//! name to change, no properties, no history. What there is of editing is a note tab's (it holds one), and the title (as
//! the frontmatter, which is not shown) is out of reach.

use crate::note_tab::{NoteMsg, NoteTab, Request};
use crate::opening::Spec;
use crate::shared::{READING_LOG, Shared, TabId};
use iced::Task;

pub struct LogTab {
    pub note: NoteTab,
}

impl LogTab {
    /// The log is made if there is none (and put right if it was made newest first, as a version not in use did).
    pub fn new(id: TabId, sh: &mut Shared) -> LogTab {
        match sh.notes.get(READING_LOG).cloned() {
            None => {
                sh.notes.insert(READING_LOG.to_string(), study_core::reading_log::new_log("Reading log", &study_core::frontmatter::now()));
            }
            Some(text) => {
                if let Some(turned) = study_core::reading_log::oldest_first(&text) {
                    sh.notes.insert(READING_LOG.to_string(), turned);
                }
            }
        }
        LogTab { note: NoteTab::new(id, sh, READING_LOG.to_string()) }
    }

    pub fn id(&self) -> TabId {
        self.note.id
    }

    pub fn title(&self) -> String {
        "Reading log".to_string()
    }

    /// What a message does here: what writes in the log, moves in it and follows its links (to a verse, or to the web)
    /// as in a note; nothing else of the note tab's (choosing, renaming, properties, history) is let through.
    pub fn update(&mut self, sh: &mut Shared, message: NoteMsg) -> (Task<NoteMsg>, Option<Request>) {
        if !matches!(
            message,
            NoteMsg::Action(_) | NoteMsg::Vim(_) | NoteMsg::OpenLink(_) | NoteMsg::LinkMenu(..) | NoteMsg::LinkDrag(..) | NoteMsg::Scrolled(_)
        ) {
            return (Task::none(), None);
        }
        let (task, request) = self.note.update(sh, message);
        let to_a_note = |spec: &Spec| matches!(spec, Spec::Note(_));
        let request = match request {
            Some(Request::Open(spec)) | Some(Request::Menu(spec, _)) | Some(Request::Drag(spec, _)) if to_a_note(&spec) => None,
            Some(Request::Backlinks(_)) => None,
            other => other,
        };
        (task, request)
    }
}
