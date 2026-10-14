//! The editable document: raw markdown text, a selection, and an undo log. Every change is an
//! edit to the source string, so what is on screen can never disagree with what is saved
//! (docs/06 §6). No layout here: vertical movement and visual line ends are the widget's job.

use crate::blocks::{self, Block, Kind, block_at};
use crate::render::Reveal;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    /// The fixed end.
    pub anchor: usize,
    /// The end that moves, where the caret is drawn.
    pub head: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Selection { anchor: at, head: at }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected bytes, in order.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
}

/// Inline formatting that can be toggled around the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Bold,
    Italic,
    Code,
    Strike,
}

impl Wrap {
    fn marker(self) -> &'static str {
        match self {
            Wrap::Bold => "**",
            Wrap::Italic => "*",
            Wrap::Code => "`",
            Wrap::Strike => "~~",
        }
    }
}

/// Everything the widget can ask of the document.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Typed text; replaces the selection.
    Insert(String),
    /// Pasted text; always its own undo step.
    Paste(String),
    Newline,
    Backspace,
    Delete,
    DeleteWordBack,
    DeleteToLineStart,
    Move { motion: Motion, extend: bool },
    /// Put the caret (or extend the selection) at a source offset.
    SetCaret { offset: usize, extend: bool },
    Select(Range<usize>),
    /// Select from `anchor` to `head`, in that direction (a drag).
    SelectBetween { anchor: usize, head: usize },
    SelectAll,
    SelectWord(usize),
    SelectLine(usize),
    Undo,
    Redo,
    Wrap(Wrap),
    /// Flip the checkbox of the task item on the line holding this offset.
    ToggleTask(usize),
    Indent,
    Outdent,
}

#[derive(Clone, Debug, PartialEq)]
struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct Step {
    edits: Vec<Edit>,
    before: Selection,
    after: Selection,
}

/// What kind of edit the last undo step was, for merging a burst of typing into one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Typing,
    DeleteBack,
}

#[derive(Debug, Clone)]
pub struct Document {
    source: String,
    blocks: Vec<Block>,
    selection: Selection,
    undo: Vec<Step>,
    redo: Vec<Step>,
    group: Option<Group>,
    /// The frontmatter cannot be reached by the caret, the selection or any edit; only
    /// [`Document::replace_frontmatter`] changes it.
    front_locked: bool,
    /// The first heading (the note's title) is as out of reach as the frontmatter, but is shown.
    title_locked: bool,
}

impl Document {
    /// A document with the caret at the end.
    pub fn new(text: &str) -> Self {
        let source = text.replace("\r\n", "\n");
        let end = source.len();
        Document {
            blocks: blocks::parse(&source),
            source,
            selection: Selection::caret(end),
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
            front_locked: false,
            title_locked: false,
        }
    }

    /// Keep the frontmatter out of reach of the caret and of editing: for a document whose
    /// properties are changed by the app's own controls.
    pub fn lock_frontmatter(mut self) -> Self {
        self.front_locked = true;
        self.keep_out_of_frontmatter();
        self
    }

    /// Keep the title (the first heading of level 1, below the frontmatter if there is any) out of reach of the
    /// caret, the selection and editing too, for a note whose name is not for changing. It stays shown. Needs the
    /// frontmatter to be locked (this does that).
    pub fn lock_title(mut self) -> Self {
        self.front_locked = true;
        self.title_locked = true;
        self.keep_out_of_frontmatter();
        self
    }

    /// How many leading blocks are the frontmatter.
    fn front_blocks(&self) -> usize {
        self.blocks.iter().take_while(|b| b.kind == Kind::Frontmatter).count()
    }

    /// Where the frontmatter block itself ends (0 if there is none).
    fn front_block_end(&self) -> usize {
        match self.front_blocks() {
            0 => 0,
            n => self.blocks.get(n).map_or(self.source.len(), |b| b.range.start),
        }
    }

    /// Where the text after the frontmatter starts (0 if there is none): past the one blank line
    /// that separates them, which belongs to the frontmatter.
    pub fn front_end(&self) -> usize {
        let n = self.front_blocks();
        let blank_after = n > 0 && self.blocks.get(n).is_some_and(|b| b.kind == Kind::Blank);
        let next = n + usize::from(blank_after);
        let start_of = |index: usize| self.blocks.get(index).map_or(self.source.len(), |b| b.range.start);
        // A locked title is part of what the text starts below, with the blank line under it.
        if self.title_locked && self.blocks.get(next).is_some_and(|b| b.kind == Kind::Heading(1)) {
            let blank_under = self.blocks.get(next + 1).is_some_and(|b| b.kind == Kind::Blank);
            return start_of(next + 1 + usize::from(blank_under));
        }
        if n == 0 { 0 } else { start_of(next) }
    }

    /// How many leading blocks a locked document does not show at all (the frontmatter and the
    /// blank line below it), so the layout can give them no height.
    pub fn hidden_blocks(&self) -> usize {
        if !self.front_locked || self.front_blocks() == 0 {
            return 0;
        }
        let n = self.front_blocks();
        n + usize::from(self.blocks.get(n).is_some_and(|b| b.kind == Kind::Blank))
    }

    fn keep_out_of_frontmatter(&mut self) {
        let front = self.front_end();
        self.selection.anchor = self.selection.anchor.max(front);
        self.selection.head = self.selection.head.max(front);
    }

    /// Replace the frontmatter block with `block` (a whole `---` block, or empty to remove it),
    /// as one undoable edit. The caret stays where it was in the text.
    pub fn replace_frontmatter(&mut self, block: &str) -> bool {
        // The block is replaced and the blank line below it stays; taking the block away takes
        // that with it, and a new block brings its own.
        let end = if block.is_empty() { self.front_end() } else { self.front_block_end() };
        let insert = if end == 0 && !block.is_empty() && !self.source.is_empty() { format!("{block}\n") } else { block.to_string() };
        if insert == self.source[..end] {
            return false;
        }
        let before = self.selection;
        let delta = insert.len() as isize - end as isize;
        self.group = None;
        self.replace(0..end, &insert, None);
        let shift = |o: usize| if o >= end { (o as isize + delta) as usize } else { insert.len() };
        self.selection = Selection { anchor: shift(before.anchor), head: shift(before.head) };
        self.keep_out_of_frontmatter();
        true
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The text of the selection, if any (what Copy takes).
    pub fn selected_text(&self) -> Option<String> {
        (!self.selection.is_empty()).then(|| self.source[self.selection.range()].to_string())
    }

    /// The block that shows its raw source: the one holding the caret, unless the selection
    /// covers several lines (then everything stays rendered, so the view holds still).
    pub fn revealed_block(&self) -> Option<usize> {
        let (a, h) = (block_at(&self.blocks, self.selection.anchor), block_at(&self.blocks, self.selection.head));
        (a == h).then_some(h)
    }

    /// What block `index` shows raw. Only the block holding the selection reveals anything.
    /// Headings, rules, embeds and code reveal the whole line; paragraphs, list items and quotes
    /// reveal just the inline element (bold, link...) the caret is in or touches, so the rest of
    /// the line does not jump about.
    pub fn reveal(&self, index: usize) -> Reveal {
        // The frontmatter is one thing: with the caret on any of its lines, all of it shows.
        let in_front = self.revealed_block().is_some_and(|b| self.blocks[b].kind == Kind::Frontmatter);
        if in_front && self.blocks[index].kind == Kind::Frontmatter {
            return Reveal::line();
        }
        if self.revealed_block() != Some(index) {
            return Reveal::none();
        }
        match self.blocks[index].kind {
            Kind::Paragraph | Kind::Item { .. } | Kind::Quote => Reveal::inline(self.selection.range()),
            _ => Reveal::line(),
        }
    }

    /// The caret must not rest inside the hidden marker of a list item or quote (it would sit
    /// still on screen while moving through invisible characters): it goes to where the text
    /// starts.
    fn snap_caret(&self, offset: usize) -> usize {
        let block = &self.blocks[block_at(&self.blocks, offset)];
        let hides_prefix = block.prefix > 0 && matches!(block.kind, Kind::Item { .. } | Kind::Quote);
        if hides_prefix && offset >= block.range.start && offset < block.content_start() {
            block.content_start()
        } else {
            offset
        }
    }

    pub fn block_index_at(&self, offset: usize) -> usize {
        block_at(&self.blocks, offset)
    }

    /// Start and end of the line holding `offset`.
    pub fn line_range(&self, offset: usize) -> Range<usize> {
        self.blocks[block_at(&self.blocks, offset)].range.clone()
    }

    /// The word (or run of spaces, or of punctuation) around `offset`.
    pub fn word_range(&self, offset: usize) -> Range<usize> {
        let line = self.line_range(offset);
        let text = &self.source[line.clone()];
        let rel = (offset - line.start).min(text.len());
        let class_at = |i: usize| text[i..].chars().next().map(word_class);
        // Prefer a word: the character under the caret, or else the one before it, as long
        // as that is part of a word. Otherwise whatever is under the caret (spaces, punctuation).
        let before = text[..rel].char_indices().next_back();
        let pivot = match (class_at(rel), before) {
            (Some(2), _) => rel,
            (_, Some((i, c))) if word_class(c) == 2 => i,
            (Some(_), _) => rel,
            (None, Some((i, _))) => i,
            (None, None) => rel,
        };
        let Some(class) = class_at(pivot) else { return offset..offset };
        let start = text[..pivot]
            .char_indices()
            .rev()
            .take_while(|&(_, c)| word_class(c) == class)
            .last()
            .map_or(pivot, |(i, _)| i);
        let end = text[pivot..]
            .char_indices()
            .find(|&(_, c)| word_class(c) != class)
            .map_or(text.len(), |(i, _)| pivot + i);
        line.start + start..line.start + end
    }

    /// Run an action. Returns whether the text changed.
    pub fn perform(&mut self, action: Action) -> bool {
        if !self.front_locked {
            return self.perform_unlocked(action);
        }
        // Nothing deletes backwards into the frontmatter, and nothing selects it.
        let sel = self.selection;
        let at_edge = sel.is_empty() && sel.head <= self.front_end() && self.front_end() > 0;
        if at_edge && matches!(action, Action::Backspace | Action::DeleteWordBack | Action::DeleteToLineStart) {
            return false;
        }
        let changed = self.perform_unlocked(action);
        self.keep_out_of_frontmatter();
        changed
    }

    fn perform_unlocked(&mut self, action: Action) -> bool {
        let before = self.source.len();
        let version = self.undo.len() + self.redo.len();
        let text_before = self.source.clone();
        let _ = (before, version);
        match action {
            Action::Insert(text) => self.insert(&text.replace("\r\n", "\n"), true),
            Action::Paste(text) => {
                self.group = None;
                self.insert(&text.replace("\r\n", "\n").replace('\r', "\n"), false);
            }
            Action::Newline => self.newline(),
            Action::Backspace => self.backspace(),
            Action::Delete => self.delete_forward(),
            Action::DeleteWordBack => {
                let sel = self.selection;
                if sel.is_empty() {
                    let from = word_left(&self.source, sel.head);
                    self.replace(from..sel.head, "", None);
                } else {
                    self.replace(sel.range(), "", None);
                }
            }
            Action::DeleteToLineStart => {
                let sel = self.selection;
                let start = self.line_range(sel.head).start;
                if sel.is_empty() && sel.head > start {
                    self.replace(start..sel.head, "", None);
                } else if !sel.is_empty() {
                    self.replace(sel.range(), "", None);
                }
            }
            Action::Move { motion, extend } => self.move_by(motion, extend),
            Action::SetCaret { offset, extend } => {
                let to = self.clamp(offset);
                self.set_head(to, extend);
            }
            Action::Select(range) => {
                self.group = None;
                self.selection = Selection { anchor: self.clamp(range.start), head: self.clamp(range.end) };
            }
            Action::SelectBetween { anchor, head } => {
                self.group = None;
                self.selection = Selection { anchor: self.clamp(anchor), head: self.clamp(head) };
            }
            Action::SelectAll => {
                self.group = None;
                self.selection = Selection { anchor: 0, head: self.source.len() };
            }
            Action::SelectWord(offset) => {
                let r = self.word_range(offset);
                self.group = None;
                self.selection = Selection { anchor: r.start, head: r.end };
            }
            Action::SelectLine(offset) => {
                let r = self.line_range(offset);
                self.group = None;
                self.selection = Selection { anchor: r.start, head: r.end };
            }
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Wrap(wrap) => self.toggle_wrap(wrap),
            Action::ToggleTask(offset) => self.toggle_task(offset),
            Action::Indent => self.indent(),
            Action::Outdent => self.outdent(),
        }
        self.source != text_before
    }

    // ---- editing core ---------------------------------------------------------------

    fn clamp(&self, offset: usize) -> usize {
        let mut o = offset.min(self.source.len());
        while !self.source.is_char_boundary(o) {
            o -= 1;
        }
        o
    }

    fn reparse(&mut self) {
        self.blocks = blocks::parse(&self.source);
    }

    /// Replace `range` with `text`, leaving the caret after it. `group` says whether this can
    /// merge into the previous undo step (a burst of typing, a run of backspaces).
    fn replace(&mut self, range: Range<usize>, text: &str, group: Option<Group>) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        let before = self.selection;
        let edit = Edit {
            at: range.start,
            removed: self.source[range.clone()].to_string(),
            inserted: text.to_string(),
        };
        let after = Selection::caret(range.start + text.len());
        self.apply_step(vec![edit], before, after, group);
    }

    fn apply_step(&mut self, edits: Vec<Edit>, before: Selection, after: Selection, group: Option<Group>) {
        for e in &edits {
            self.source.replace_range(e.at..e.at + e.removed.len(), &e.inserted);
        }
        self.reparse();
        self.redo.clear();
        self.selection = after;

        let merge = group.is_some()
            && self.group == group
            && before.is_empty()
            && self.undo.last().is_some_and(|last| last.after == before && mergeable(last, &edits, group));
        match (merge, self.undo.last_mut()) {
            (true, Some(last)) => {
                last.edits.extend(edits);
                last.after = after;
            }
            _ => self.undo.push(Step { edits, before, after }),
        }
        self.group = group;
    }

    fn undo(&mut self) {
        let Some(step) = self.undo.pop() else { return };
        for e in step.edits.iter().rev() {
            self.source.replace_range(e.at..e.at + e.inserted.len(), &e.removed);
        }
        self.reparse();
        self.selection = step.before;
        self.redo.push(step);
        self.group = None;
    }

    fn redo(&mut self) {
        let Some(step) = self.redo.pop() else { return };
        for e in &step.edits {
            self.source.replace_range(e.at..e.at + e.removed.len(), &e.inserted);
        }
        self.reparse();
        self.selection = step.after;
        self.undo.push(step);
        self.group = None;
    }

    fn insert(&mut self, text: &str, typing: bool) {
        if text.is_empty() {
            return;
        }
        // A bracket or quote typed comes with its partner, a partner typed over its twin steps past
        // it, and a selection is wrapped.
        let mut chars = text.chars();
        if typing && let (Some(c), None) = (chars.next(), chars.next()) && self.auto_close(c) {
            return;
        }
        let range = self.selection.range();
        let group = (typing && !text.contains('\n')).then_some(Group::Typing);
        self.replace(range, text, group);
    }

    /// What typing `c` does when it opens or closes a pair (see [`Document::insert`]); whether it was
    /// that, and so done.
    fn auto_close(&mut self, c: char) -> bool {
        let sel = self.selection;
        let (head, range) = (sel.head, sel.range());
        // Typing the closing half of a pair right before it steps over it.
        if matches!(c, ')' | ']' | '}' | '"' | '`') && sel.is_empty() && self.source[head..].starts_with(c) {
            self.set_head(head + c.len_utf8(), false);
            return true;
        }
        let Some(close) = pair_for(c) else { return false };
        let quote = c == close;
        if !sel.is_empty() {
            // A selection is put inside the pair and stays selected.
            let inner = self.source[range.clone()].to_string();
            self.replace(range.clone(), &format!("{c}{inner}{close}"), None);
            let at = Selection { anchor: range.start + c.len_utf8(), head: range.start + c.len_utf8() + inner.len() };
            self.set_selection_after(at);
            return true;
        }
        // Only where nothing follows that the pair would be put in front of, and (for quotes) where a
        // word is not ending: `don't` has no pair, and neither has the quote that closes a phrase.
        let next = self.source[head..].chars().next();
        let prev = self.source[..head].chars().next_back();
        if next.is_some_and(|n| n.is_alphanumeric() || n == c) || (quote && prev.is_some_and(|p| p.is_alphanumeric() || p == c)) {
            return false;
        }
        self.replace(range, &format!("{c}{close}"), None);
        let inside = head + c.len_utf8();
        self.set_selection_after(Selection { anchor: inside, head: inside });
        true
    }

    /// Make the selection what the last edit leaves behind, in the history as well (so that going
    /// forward again puts it there).
    fn set_selection_after(&mut self, selection: Selection) {
        self.selection = selection;
        if let Some(last) = self.undo.last_mut() {
            last.after = selection;
        }
        self.group = None;
    }

    fn backspace(&mut self) {
        let sel = self.selection;
        if !sel.is_empty() {
            return self.replace(sel.range(), "", None);
        }
        if sel.head == 0 {
            return;
        }
        // At the start of a line's content, Backspace removes the line's syntax first
        // (the `- `, `> ` or `## `), as in most markdown editors.
        let block = &self.blocks[block_at(&self.blocks, sel.head)];
        if block.prefix > 0 && sel.head == block.content_start() && !matches!(block.kind, Kind::Rule) {
            let prefix = block.range.start..block.content_start();
            return self.replace(prefix, "", None);
        }
        let from = prev_boundary(&self.source, sel.head);
        // Between the halves of a pair that was just made, both go.
        let between = self.source[from..sel.head].chars().next().and_then(pair_for).is_some_and(|close| self.source[sel.head..].starts_with(close));
        if between {
            let to = sel.head + self.source[sel.head..].chars().next().map_or(0, char::len_utf8);
            return self.replace(from..to, "", None);
        }
        self.replace(from..sel.head, "", Some(Group::DeleteBack));
    }

    fn delete_forward(&mut self) {
        let sel = self.selection;
        if !sel.is_empty() {
            return self.replace(sel.range(), "", None);
        }
        let to = next_boundary(&self.source, sel.head);
        self.replace(sel.head..to, "", None);
    }

    // ---- motion ---------------------------------------------------------------------

    fn set_head(&mut self, to: usize, extend: bool) {
        self.group = None;
        self.selection = if extend {
            Selection { anchor: self.selection.anchor, head: to }
        } else {
            Selection::caret(self.snap_caret(to))
        };
    }

    fn move_by(&mut self, motion: Motion, extend: bool) {
        let sel = self.selection;
        let src = &self.source;
        // Arrow keys with a selection collapse to its edge first, unless extending.
        let collapsed = !extend && !sel.is_empty();
        // From the start of a list item's text, "left" is the end of the line above: the marker
        // in between is not a place the caret can be.
        let block = &self.blocks[block_at(&self.blocks, sel.head)];
        let at_text_start = block.prefix > 0
            && matches!(block.kind, Kind::Item { .. } | Kind::Quote)
            && sel.head == block.content_start();
        let line_above_end = block.range.start.saturating_sub(1);
        let to = match motion {
            Motion::Left if collapsed => sel.range().start,
            Motion::Right if collapsed => sel.range().end,
            Motion::Left | Motion::WordLeft if at_text_start && block.range.start > 0 => line_above_end,
            Motion::Left => prev_boundary(src, sel.head),
            Motion::Right => next_boundary(src, sel.head),
            Motion::WordLeft => word_left(src, sel.head),
            Motion::WordRight => word_right(src, sel.head),
            // Home goes to where the text starts, past a list marker or quote bar.
            Motion::LineStart if block.prefix > 0 && matches!(block.kind, Kind::Item { .. } | Kind::Quote) => {
                block.content_start()
            }
            Motion::LineStart => self.line_range(sel.head).start,
            Motion::LineEnd => self.line_range(sel.head).end,
            Motion::DocStart => 0,
            Motion::DocEnd => src.len(),
        };
        self.set_head(to, extend);
    }

    // ---- structure ------------------------------------------------------------------

    fn newline(&mut self) {
        let sel = self.selection;
        let block = self.blocks[block_at(&self.blocks, sel.head)].clone();
        let line = block.text(&self.source).to_string();
        let in_content = sel.is_empty() && sel.head >= block.content_start();
        let content = &line[block.prefix..];

        match &block.kind {
            Kind::Item { indent, ordered, task } if in_content => {
                if content.trim().is_empty() {
                    // Enter on an empty item ends the list.
                    return self.replace(block.range.clone(), "", None);
                }
                let marker = match ordered {
                    Some(number) => {
                        let digits: String = number.chars().take_while(char::is_ascii_digit).collect();
                        let next = digits.parse::<u64>().unwrap_or(0) + 1;
                        format!("{next}{}", &number[digits.len()..])
                    }
                    None => line[*indent..].chars().next().map_or("-".into(), |c| c.to_string()),
                };
                let mut text = format!("\n{}{marker} ", " ".repeat(*indent));
                if task.is_some() {
                    text.push_str("[ ] ");
                }
                self.replace(sel.range(), &text, None);
            }
            Kind::Quote if in_content => {
                if content.trim().is_empty() {
                    return self.replace(block.range.clone(), "", None);
                }
                let text = format!("\n{}", &line[..block.prefix]);
                self.replace(sel.range(), &text, None);
            }
            _ => self.replace(sel.range(), "\n", None),
        }
    }

    fn toggle_task(&mut self, offset: usize) {
        let block = self.blocks[block_at(&self.blocks, offset)].clone();
        let Kind::Item { task: Some(done), indent, .. } = block.kind else { return };
        // The box is the three bytes before the prefix's trailing space.
        let line = block.text(&self.source);
        let Some(open) = line[indent..].find('[').map(|i| block.range.start + indent + i) else { return };
        let before = self.selection;
        let edit = Edit {
            at: open + 1,
            removed: self.source[open + 1..open + 2].to_string(),
            inserted: if done { " " } else { "x" }.to_string(),
        };
        self.apply_step(vec![edit], before, before, None);
    }

    fn toggle_wrap(&mut self, wrap: Wrap) {
        let m = wrap.marker();
        let sel = self.selection;
        let r = sel.range();
        let before = sel;
        let src = &self.source;
        let wrapped_outside = src[..r.start].ends_with(m) && src[r.end..].starts_with(m);
        let wrapped_inside = r.len() >= 2 * m.len() && src[r.clone()].starts_with(m) && src[r.clone()].ends_with(m);
        let (edits, after) = if wrapped_outside && !r.is_empty() {
            let edits = vec![
                Edit { at: r.end, removed: m.into(), inserted: String::new() },
                Edit { at: r.start - m.len(), removed: m.into(), inserted: String::new() },
            ];
            (edits, Selection { anchor: r.start - m.len(), head: r.end - m.len() })
        } else if wrapped_inside {
            let edits = vec![
                Edit { at: r.end - m.len(), removed: m.into(), inserted: String::new() },
                Edit { at: r.start, removed: m.into(), inserted: String::new() },
            ];
            (edits, Selection { anchor: r.start, head: r.end - 2 * m.len() })
        } else {
            let edits = vec![
                Edit { at: r.end, removed: String::new(), inserted: m.into() },
                Edit { at: r.start, removed: String::new(), inserted: m.into() },
            ];
            (edits, Selection { anchor: r.start + m.len(), head: r.end + m.len() })
        };
        self.apply_step(edits, before, after, None);
    }

    fn indent(&mut self) {
        let sel = self.selection;
        let block = self.blocks[block_at(&self.blocks, sel.head)].clone();
        let at = if matches!(block.kind, Kind::Item { .. }) { block.range.start } else { sel.range().start };
        let range_end = if matches!(block.kind, Kind::Item { .. }) { at } else { sel.range().end };
        let before = sel;
        let edit = Edit { at, removed: self.source[at..range_end].to_string(), inserted: "  ".into() };
        let shift = |o: usize| if o >= at { o + 2 - (range_end - at).min(o - at) } else { o };
        let after = if matches!(block.kind, Kind::Item { .. }) {
            Selection { anchor: shift(sel.anchor), head: shift(sel.head) }
        } else {
            Selection::caret(at + 2)
        };
        self.apply_step(vec![edit], before, after, None);
    }

    fn outdent(&mut self) {
        let sel = self.selection;
        let block = self.blocks[block_at(&self.blocks, sel.head)].clone();
        if !matches!(block.kind, Kind::Item { .. }) {
            return;
        }
        let line = block.text(&self.source);
        let n = line.bytes().take_while(|&b| b == b' ').count().min(2);
        if n == 0 {
            return;
        }
        let at = block.range.start;
        let edit = Edit { at, removed: " ".repeat(n), inserted: String::new() };
        let shift = |o: usize| if o >= at + n { o - n } else { o.min(at) };
        let after = Selection { anchor: shift(sel.anchor), head: shift(sel.head) };
        self.apply_step(vec![edit], sel, after, None);
    }
}

/// Whether `edits` continue the burst that `last` ended: typing that doesn't start a new word,
/// or backspaces that run on backwards from where the last one stopped.
fn mergeable(last: &Step, edits: &[Edit], group: Option<Group>) -> bool {
    let (Some(prev), Some(new)) = (last.edits.last(), edits.first()) else { return false };
    match group {
        Some(Group::Typing) => {
            let ends_in_space = prev.inserted.ends_with(char::is_whitespace);
            let starts_word = new.inserted.starts_with(|c: char| !c.is_whitespace());
            new.at == prev.at + prev.inserted.len() && !(ends_in_space && starts_word)
        }
        Some(Group::DeleteBack) => new.at + new.removed.len() == prev.at,
        None => false,
    }
}

fn word_class(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        2
    } else if c.is_whitespace() {
        0
    } else {
        1
    }
}

/// Start of the previous grapheme cluster (a combining accent or an emoji sequence is one).
pub fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].grapheme_indices(true).next_back().map_or(0, |(k, _)| k)
}

pub fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].graphemes(true).next().map_or(i, |g| i + g.len())
}

/// The closing half of the pair that `c` opens, if it does.
fn pair_for(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '`' => Some('`'),
        _ => None,
    }
}

fn word_left(s: &str, i: usize) -> usize {
    let mut chars = s[..i].char_indices().rev().peekable();
    while chars.next_if(|&(_, c)| word_class(c) == 0 && c != '\n').is_some() {}
    let Some(&(_, first)) = chars.peek() else { return 0 };
    if first == '\n' {
        return i - 1;
    }
    let class = word_class(first);
    let mut at = i;
    while let Some(&(k, c)) = chars.peek() {
        if word_class(c) != class {
            break;
        }
        at = k;
        chars.next();
    }
    at.min(i)
}

pub fn word_right(s: &str, i: usize) -> usize {
    let mut chars = s[i..].char_indices().peekable();
    while chars.next_if(|&(_, c)| word_class(c) == 0 && c != '\n').is_some() {}
    let Some(&(k, first)) = chars.peek() else { return s.len() };
    if first == '\n' {
        return i + k + 1;
    }
    let class = word_class(first);
    let mut at = s.len() - i;
    while let Some(&(k, c)) = chars.peek() {
        if word_class(c) != class {
            at = k;
            break;
        }
        chars.next();
    }
    i + at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::new(text)
    }

    fn type_str(d: &mut Document, s: &str) {
        for c in s.chars() {
            d.perform(Action::Insert(c.to_string()));
        }
    }

    fn caret_at(d: &mut Document, at: usize) {
        d.perform(Action::SetCaret { offset: at, extend: false });
    }

    #[test]
    fn typing_inserts_at_the_caret_and_replaces_a_selection() {
        let mut d = doc("hello");
        assert!(d.perform(Action::Insert(" world".into())));
        assert_eq!(d.source(), "hello world");
        d.perform(Action::Select(0..5));
        d.perform(Action::Insert("bye".into()));
        assert_eq!(d.source(), "bye world");
        assert_eq!(d.selection(), Selection::caret(3));
        assert!(!d.perform(Action::Insert(String::new())), "empty insert changes nothing");
    }

    #[test]
    fn backspace_and_delete_remove_whole_grapheme_clusters() {
        // "e" + combining acute, a family emoji (one cluster of several scalars), and "x".
        let mut d = doc("e\u{301}👨‍👩‍👧x");
        d.perform(Action::Backspace);
        assert_eq!(d.source(), "e\u{301}👨‍👩‍👧");
        d.perform(Action::Backspace);
        assert_eq!(d.source(), "e\u{301}", "the whole emoji family goes at once");
        d.perform(Action::Move { motion: Motion::LineStart, extend: false });
        d.perform(Action::Delete);
        assert_eq!(d.source(), "", "the accent goes with its letter");
        assert!(!d.perform(Action::Backspace), "nothing before the start");
    }

    #[test]
    fn moving_and_extending_by_character_word_and_line() {
        let mut d = doc("alpha beta\ngamma");
        d.perform(Action::Move { motion: Motion::DocStart, extend: false });
        d.perform(Action::Move { motion: Motion::WordRight, extend: false });
        assert_eq!(d.selection().head, 5, "end of the first word");
        d.perform(Action::Move { motion: Motion::WordRight, extend: true });
        assert_eq!(d.selection(), Selection { anchor: 5, head: 10 });
        d.perform(Action::Move { motion: Motion::Right, extend: false });
        assert_eq!(d.selection(), Selection::caret(10), "Right collapses to the selection's end... and then moves");
    }

    #[test]
    fn arrow_keys_collapse_a_selection_to_its_edge_without_moving_further() {
        let mut d = doc("abcdef");
        d.perform(Action::Select(1..4));
        d.perform(Action::Move { motion: Motion::Left, extend: false });
        assert_eq!(d.selection(), Selection::caret(1));
        d.perform(Action::Select(1..4));
        d.perform(Action::Move { motion: Motion::Right, extend: false });
        assert_eq!(d.selection(), Selection::caret(4));
    }

    #[test]
    fn word_motion_stops_at_punctuation_and_line_breaks() {
        let s = "foo, bar_baz\nqux";
        assert_eq!(word_right(s, 0), 3);
        assert_eq!(word_right(s, 3), 4, "the comma");
        assert_eq!(word_right(s, 4), 12, "bar_baz is one word");
        assert_eq!(word_right(s, 12), 13, "past the line break");
        assert_eq!(word_left(s, 12), 5);
        assert_eq!(word_left(s, 5), 3, "back over the space and the comma together");
        assert_eq!(word_left(s, 3), 0);
        assert_eq!(word_left(s, 13), 12);
        assert_eq!(word_left(s, 0), 0);
        assert_eq!(word_right(s, s.len()), s.len());
    }

    #[test]
    fn select_word_line_and_all() {
        let mut d = doc("alpha beta\ngamma");
        d.perform(Action::SelectWord(7));
        assert_eq!(d.selected_text().as_deref(), Some("beta"));
        d.perform(Action::SelectWord(5));
        assert_eq!(d.selected_text().as_deref(), Some("alpha"), "at a word's end, that word");
        d.perform(Action::SelectLine(12));
        assert_eq!(d.selected_text().as_deref(), Some("gamma"));
        d.perform(Action::SelectAll);
        assert_eq!(d.selected_text().as_deref(), Some("alpha beta\ngamma"));
    }

    #[test]
    fn undo_groups_typing_by_word_and_restores_the_caret() {
        let mut d = doc("");
        type_str(&mut d, "hello world");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "hello ", "the second word goes first");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "");
        assert_eq!(d.selection(), Selection::caret(0));
        d.perform(Action::Redo);
        assert_eq!(d.source(), "hello ");
        assert_eq!(d.selection(), Selection::caret(6));
        d.perform(Action::Redo);
        assert_eq!(d.source(), "hello world");
        assert!(!d.can_redo());
    }

    #[test]
    fn moving_the_caret_ends_a_typing_group_and_a_new_edit_clears_redo() {
        let mut d = doc("");
        type_str(&mut d, "abc");
        caret_at(&mut d, 1);
        type_str(&mut d, "X");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "abc", "only the X is undone");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "");
        type_str(&mut d, "n");
        assert!(!d.can_redo(), "typing after an undo drops the redo history");
    }

    #[test]
    fn a_run_of_backspaces_is_one_undo_step() {
        let mut d = doc("abcdef");
        for _ in 0..3 {
            d.perform(Action::Backspace);
        }
        assert_eq!(d.source(), "abc");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "abcdef");
    }

    #[test]
    fn paste_is_one_step_and_normalises_line_endings() {
        let mut d = doc("ab");
        type_str(&mut d, "c");
        d.perform(Action::Paste("1\r\n2\r3".into()));
        assert_eq!(d.source(), "abc1\n2\n3");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "abc", "the paste alone");
    }

    #[test]
    fn enter_continues_lists_numbers_tasks_and_quotes() {
        let mut d = doc("- one");
        d.perform(Action::Newline);
        assert_eq!(d.source(), "- one\n- ");
        type_str(&mut d, "two");
        d.perform(Action::Newline);
        assert_eq!(d.source(), "- one\n- two\n- ");

        let mut n = doc("9. nine");
        n.perform(Action::Newline);
        assert_eq!(n.source(), "9. nine\n10. ");

        let mut t = doc("  - [x] done");
        t.perform(Action::Newline);
        assert_eq!(t.source(), "  - [x] done\n  - [ ] ", "a new task starts unchecked, at the same indent");

        let mut q = doc("> quote");
        q.perform(Action::Newline);
        assert_eq!(q.source(), "> quote\n> ");
    }

    #[test]
    fn enter_on_an_empty_item_or_quote_ends_the_list() {
        let mut d = doc("- one\n- ");
        d.perform(Action::Newline);
        assert_eq!(d.source(), "- one\n");
        assert_eq!(d.selection(), Selection::caret(6));
        let mut q = doc("> a\n> ");
        q.perform(Action::Newline);
        assert_eq!(q.source(), "> a\n");
    }

    #[test]
    fn enter_in_the_middle_of_an_item_splits_it_and_in_other_lines_is_plain() {
        let mut d = doc("- onetwo");
        caret_at(&mut d, 5);
        d.perform(Action::Newline);
        assert_eq!(d.source(), "- one\n- two");
        let mut p = doc("plain text");
        caret_at(&mut p, 5);
        p.perform(Action::Newline);
        assert_eq!(p.source(), "plain\n text");
        // The caret cannot rest inside the marker, so asking for it there means the start of
        // the text; Enter at the start of an item's text pushes the whole item down.
        let mut m = doc("- item");
        caret_at(&mut m, 1);
        assert_eq!(m.selection().head, 2);
        m.perform(Action::Newline);
        assert_eq!(m.source(), "- \n- item");
    }

    #[test]
    fn backspace_at_the_start_of_content_removes_the_syntax_first() {
        let mut d = doc("- item");
        caret_at(&mut d, 2);
        d.perform(Action::Backspace);
        assert_eq!(d.source(), "item");
        let mut h = doc("## Title");
        caret_at(&mut h, 3);
        h.perform(Action::Backspace);
        assert_eq!(h.source(), "Title");
        // One step: undo restores the prefix.
        h.perform(Action::Undo);
        assert_eq!(h.source(), "## Title");
        // A plain line just joins with the one above.
        let mut p = doc("a\nb");
        caret_at(&mut p, 2);
        p.perform(Action::Backspace);
        assert_eq!(p.source(), "ab");
    }

    #[test]
    fn wrap_toggles_bold_italic_and_code_around_the_selection() {
        let mut d = doc("make bold now");
        d.perform(Action::Select(5..9));
        d.perform(Action::Wrap(Wrap::Bold));
        assert_eq!(d.source(), "make **bold** now");
        assert_eq!(d.selected_text().as_deref(), Some("bold"), "still selected, inside the markers");
        d.perform(Action::Wrap(Wrap::Bold));
        assert_eq!(d.source(), "make bold now", "a second press takes it back off");
        assert_eq!(d.selected_text().as_deref(), Some("bold"));
        d.perform(Action::Wrap(Wrap::Italic));
        assert_eq!(d.source(), "make *bold* now");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "make bold now", "each toggle is one step");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "make **bold** now");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "make bold now");

        // With nothing selected, the markers go in as a pair with the caret between.
        let mut e = doc("");
        e.perform(Action::Wrap(Wrap::Code));
        assert_eq!(e.source(), "``");
        assert_eq!(e.selection(), Selection::caret(1));
    }

    #[test]
    fn toggling_a_task_flips_the_box_in_the_source_as_one_undo_step() {
        let mut d = doc("- [ ] write\n- [x] read");
        d.perform(Action::ToggleTask(3));
        assert_eq!(d.source(), "- [x] write\n- [x] read");
        d.perform(Action::ToggleTask(15));
        assert_eq!(d.source(), "- [x] write\n- [ ] read");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "- [x] write\n- [x] read");
        let mut plain = doc("- not a task");
        assert!(!plain.perform(Action::ToggleTask(0)));
    }

    #[test]
    fn tab_and_shift_tab_indent_list_items() {
        let mut d = doc("- a\n- b");
        caret_at(&mut d, 7);
        d.perform(Action::Indent);
        assert_eq!(d.source(), "- a\n  - b");
        assert_eq!(d.selection(), Selection::caret(9), "the caret keeps its place in the text");
        d.perform(Action::Outdent);
        assert_eq!(d.source(), "- a\n- b");
        assert_eq!(d.selection(), Selection::caret(7));
        assert!(!d.perform(Action::Outdent), "nothing to take off");
        // Outside a list, Tab types spaces.
        let mut p = doc("x");
        p.perform(Action::Indent);
        assert_eq!(p.source(), "x  ");
    }

    #[test]
    fn delete_word_and_to_line_start() {
        let mut d = doc("one two three");
        d.perform(Action::DeleteWordBack);
        assert_eq!(d.source(), "one two ");
        d.perform(Action::DeleteToLineStart);
        assert_eq!(d.source(), "");
        let mut multi = doc("keep\nremove me");
        multi.perform(Action::DeleteToLineStart);
        assert_eq!(multi.source(), "keep\n");
    }

    #[test]
    fn headings_reveal_the_whole_line_but_prose_only_the_element_at_the_caret() {
        let mut d = doc("## Title\nplain **bold** text\n- item");
        caret_at(&mut d, 3);
        assert_eq!(d.reveal(0), Reveal::line());
        assert_eq!(d.reveal(1), Reveal::none(), "other lines show nothing");
        caret_at(&mut d, 9 + 8);
        assert_eq!(d.reveal(0), Reveal::none());
        assert_eq!(d.reveal(1), Reveal::inline(17..17), "a paragraph reveals by element");
        let end = d.source().len();
        caret_at(&mut d, end);
        assert!(matches!(d.reveal(2), Reveal { line: false, selection: Some(_) }), "so do list items");
        d.perform(Action::Select(0..20));
        assert_eq!(d.reveal(0), Reveal::none(), "a selection over several lines shows nothing");
    }

    #[test]
    fn the_caret_never_rests_inside_a_list_marker() {
        let mut d = doc("intro\n- item\n> quote");
        // Right from the end of the line above lands at the item's text, not inside "- ".
        caret_at(&mut d, 5);
        d.perform(Action::Move { motion: Motion::Right, extend: false });
        assert_eq!(d.selection().head, 8);
        // Left from the start of its text goes to the end of the line above.
        d.perform(Action::Move { motion: Motion::Left, extend: false });
        assert_eq!(d.selection().head, 5);
        // The same for a quote.
        caret_at(&mut d, 12);
        d.perform(Action::Move { motion: Motion::Right, extend: false });
        assert_eq!(d.selection().head, 15);
        d.perform(Action::Move { motion: Motion::WordLeft, extend: false });
        assert_eq!(d.selection().head, 12);
        // Home goes to the text, and clicking into the marker snaps there too.
        caret_at(&mut d, 20);
        d.perform(Action::Move { motion: Motion::LineStart, extend: false });
        assert_eq!(d.selection().head, 15);
        caret_at(&mut d, 7);
        assert_eq!(d.selection().head, 8);
        // The first line of the document has nowhere above to go.
        let mut first = doc("- only");
        first.perform(Action::Move { motion: Motion::DocStart, extend: false });
        assert_eq!(first.selection().head, 2);
        first.perform(Action::Move { motion: Motion::Left, extend: false });
        assert_eq!(first.selection().head, 2);
    }

    #[test]
    fn selecting_through_a_marker_is_allowed() {
        let mut d = doc("- item");
        d.perform(Action::SelectLine(3));
        assert_eq!(d.selected_text().as_deref(), Some("- item"));
        d.perform(Action::Move { motion: Motion::DocEnd, extend: false });
        d.perform(Action::Move { motion: Motion::DocStart, extend: true });
        assert_eq!(d.selection(), Selection { anchor: 6, head: 0 }, "an extending move keeps its raw end");
    }

    #[test]
    fn the_cursor_line_is_revealed_unless_a_selection_spans_lines() {
        let mut d = doc("one\ntwo\nthree");
        caret_at(&mut d, 5);
        assert_eq!(d.revealed_block(), Some(1));
        d.perform(Action::Select(1..6));
        assert_eq!(d.revealed_block(), None, "everything stays rendered while a block of text is selected");
        d.perform(Action::Select(4..7));
        assert_eq!(d.revealed_block(), Some(1), "a selection within one line reveals it");
    }

    #[test]
    fn blocks_stay_in_step_with_edits() {
        let mut d = doc("text");
        assert_eq!(d.blocks()[0].kind, Kind::Paragraph);
        d.perform(Action::Move { motion: Motion::DocStart, extend: false });
        type_str(&mut d, "# ");
        assert_eq!(d.blocks()[0].kind, Kind::Heading(1));
        d.perform(Action::Undo);
        assert_eq!(d.blocks()[0].kind, Kind::Paragraph);
    }

    #[test]
    fn multibyte_text_never_splits_a_character() {
        let mut d = doc("ἀγαπάω");
        d.perform(Action::SetCaret { offset: 1, extend: false });
        assert!(d.source().is_char_boundary(d.selection().head), "clamped to a boundary");
        d.perform(Action::Move { motion: Motion::DocEnd, extend: false });
        d.perform(Action::Backspace);
        assert_eq!(d.source(), "ἀγαπάω".trim_end_matches('ω'));
    }

    #[test]
    fn the_whole_frontmatter_reveals_when_the_caret_is_on_any_line_of_it() {
        let mut d = doc("---\ntitle: x\ntags: [a]\n---\n\nText\n");
        d.perform(Action::SetCaret { offset: 20, extend: false });
        let front: Vec<bool> = (0..4).map(|i| d.reveal(i) == Reveal::line()).collect();
        assert_eq!(front, [true, true, true, true], "every line of it shows");
        assert_eq!(d.reveal(5), Reveal::none(), "the text does not");
        // With the caret in the text, none of it does.
        d.perform(Action::Move { motion: Motion::DocEnd, extend: false });
        assert!((0..4).all(|i| d.reveal(i) == Reveal::none()));
    }

    const NOTE: &str = "---\ntitle: x\ntags: [a]\n---\n\nText here\n";

    fn locked() -> Document {
        let mut d = Document::new(NOTE).lock_frontmatter();
        d.perform(Action::SetCaret { offset: NOTE.find("Text").unwrap(), extend: false });
        d
    }

    const LOG: &str = "---\ncreated: x\ntags: [reading-log]\n---\n# Reading log\n\n## 2026-10-13\n\n- [[John 3]]\n";

    #[test]
    fn a_locked_title_is_shown_but_the_caret_selection_and_edits_stay_out_of_it() {
        let mut d = Document::new(LOG).lock_title();
        assert_eq!(d.front_end(), LOG.find("## 2026").unwrap(), "the text starts below the title and the blank line under it");
        assert_eq!(d.hidden_blocks(), 4, "the four lines of frontmatter are not shown: the title is");
        assert!(d.selection().head >= d.front_end());
        for action in [
            Action::SetCaret { offset: 2, extend: false },
            Action::SetCaret { offset: LOG.find("Reading log").unwrap() + 3, extend: false },
            Action::SelectAll,
            Action::Move { motion: Motion::DocStart, extend: false },
            Action::Move { motion: Motion::DocStart, extend: true },
        ] {
            d.perform(action.clone());
            let s = d.selection();
            assert!(s.anchor >= d.front_end() && s.head >= d.front_end(), "{action:?} -> {s:?}");
        }
        // Backspace at the edge deletes nothing; the words below are written in as usual.
        d.perform(Action::SetCaret { offset: d.front_end(), extend: false });
        assert!(!d.perform(Action::Backspace));
        assert!(!d.perform(Action::DeleteToLineStart));
        assert!(d.perform(Action::Insert("A note. ".into())));
        assert!(d.source().starts_with("---\ncreated: x\ntags: [reading-log]\n---\n# Reading log\n\nA note. ## 2026"), "{:?}", d.source());
        d.perform(Action::Undo);
        assert_eq!(d.source(), LOG);
        assert!(d.selection().head >= d.front_end(), "undo does not leave the caret in the title");
    }

    #[test]
    fn a_locked_title_with_no_frontmatter_and_nothing_below_it_leaves_the_end_to_write_at() {
        let d = Document::new("# Log\n").lock_title();
        assert_eq!(d.front_end(), "# Log\n".len());
        assert_eq!(d.selection().head, "# Log\n".len());
        let plain = Document::new("# Log\n\ntext\n").lock_frontmatter();
        assert_eq!(plain.front_end(), 0, "without the lock a title is text like any other");
    }

    #[test]
    fn a_locked_documents_caret_and_selection_never_enter_the_frontmatter() {
        let mut d = Document::new(NOTE).lock_frontmatter();
        assert_eq!(d.front_end(), NOTE.find("Text").unwrap(), "the text starts below the block and the blank line after it");
        assert!(d.selection().head >= d.front_end());
        for action in [
            Action::SetCaret { offset: 0, extend: false },
            Action::Move { motion: Motion::DocStart, extend: false },
            Action::SelectBetween { anchor: 3, head: 9 },
            Action::Select(0..5),
            Action::SelectLine(6),
            Action::SelectWord(8),
            Action::SelectAll,
        ] {
            d.perform(action.clone());
            let s = d.selection();
            assert!(s.anchor >= d.front_end() && s.head >= d.front_end(), "{action:?} -> {s:?}");
        }
        d.perform(Action::SelectAll);
        assert_eq!(d.selected_text().unwrap(), "Text here\n", "select all takes the text, not the properties");
    }

    #[test]
    fn nothing_deletes_backwards_into_the_frontmatter() {
        let mut d = locked();
        let edge = d.front_end();
        d.perform(Action::Move { motion: Motion::DocStart, extend: false });
        assert_eq!(d.selection().head, edge);
        for action in [Action::Backspace, Action::DeleteWordBack, Action::DeleteToLineStart] {
            assert!(!d.perform(action));
        }
        assert_eq!(d.source(), NOTE);
        // Typing at the edge is fine, and goes below the properties.
        d.perform(Action::Insert("A".into()));
        assert!(d.source().starts_with("---\ntitle: x\ntags: [a]\n---\n\nAText"), "{:?}", d.source());
    }

    #[test]
    fn undo_cannot_leave_the_caret_in_the_frontmatter() {
        let mut d = locked();
        d.perform(Action::Insert("hello".into()));
        d.perform(Action::Undo);
        d.perform(Action::Undo);
        assert!(d.selection().head >= d.front_end());
        assert_eq!(d.source(), NOTE);
    }

    #[test]
    fn replacing_the_frontmatter_is_one_undo_and_keeps_the_caret_in_the_text() {
        let mut d = locked();
        let caret_in_text = d.selection().head - d.front_end();
        let steps_before = d.can_undo();
        assert!(d.replace_frontmatter("---\ntitle: a much longer title\ntags: [a, b]\n---\n"));
        assert!(d.source().starts_with("---\ntitle: a much longer title\n"));
        assert_eq!(d.selection().head - d.front_end(), caret_in_text, "the caret did not move in the text");
        assert!(d.source().ends_with("Text here\n"));
        assert!(!steps_before);
        d.perform(Action::Undo);
        assert_eq!(d.source(), NOTE, "one step undoes it");
        // The same block again changes nothing.
        assert!(!d.replace_frontmatter("---\ntitle: x\ntags: [a]\n---\n"));
    }

    #[test]
    fn frontmatter_can_be_added_and_removed_and_a_plain_document_is_not_locked_down() {
        let mut d = Document::new("Just text\n").lock_frontmatter();
        assert_eq!(d.front_end(), 0);
        assert!(d.replace_frontmatter("---\ncreated: T\n---\n"));
        assert_eq!(d.source(), "---\ncreated: T\n---\n\nJust text\n");
        assert!(d.replace_frontmatter(""));
        assert_eq!(d.source(), "Just text\n", "removing the block takes the blank line below it too");
        // Unlocked documents (a card's editor) can be edited anywhere, frontmatter or not.
        let mut free = Document::new(NOTE);
        free.perform(Action::SetCaret { offset: 2, extend: false });
        assert_eq!(free.selection().head, 2);
    }

    fn typed(d: &mut Document, text: &str) {
        for c in text.chars() {
            d.perform(Action::Insert(c.to_string()));
        }
    }

    #[test]
    fn a_bracket_or_quote_typed_comes_with_its_partner_and_typing_the_partner_steps_over_it() {
        let mut d = Document::new("");
        typed(&mut d, "a (b [c {d \"e `f");
        assert_eq!(d.source(), "a (b [c {d \"e `f`\"}])", "each opener has its closer, nested");
    }

    #[test]
    fn pairs_close_where_it_makes_sense_and_otherwise_leave_the_typing_alone() {
        let mut d = Document::new("");
        typed(&mut d, "see (word)");
        assert_eq!(d.source(), "see (word)", "typing the closer steps over the one that came with the opener");
        assert_eq!(d.selection().head, d.source().len());
        // A wiki link: the second bracket goes inside the first pair.
        let mut d = Document::new("");
        typed(&mut d, "[[John 3:16]]");
        assert_eq!(d.source(), "[[John 3:16]]");
        // A word's apostrophe and the quote that ends a phrase are not pairs.
        let mut d = Document::new("");
        typed(&mut d, "don't say \"hi\" now");
        assert_eq!(d.source(), "don't say \"hi\" now");
        // Before a word, there is no pair.
        let mut d = Document::new("word");
        d.perform(Action::SetCaret { offset: 0, extend: false });
        typed(&mut d, "(");
        assert_eq!(d.source(), "(word");
    }

    #[test]
    fn a_selection_is_wrapped_by_a_pair_and_stays_selected_and_backspace_takes_both_halves_of_an_empty_pair() {
        let mut d = Document::new("hello world");
        d.perform(Action::Select(6..11));
        typed(&mut d, "(");
        assert_eq!(d.source(), "hello (world)");
        assert_eq!(d.selected_text().as_deref(), Some("world"));
        typed(&mut d, "\"");
        assert_eq!(d.source(), "hello (\"world\")");
        let mut d = Document::new("");
        typed(&mut d, "[");
        assert_eq!(d.source(), "[]");
        d.perform(Action::Backspace);
        assert_eq!(d.source(), "", "both halves");
        // Backspace after a word typed inside the pair removes only the letter.
        typed(&mut d, "(ab");
        d.perform(Action::Backspace);
        assert_eq!(d.source(), "(a)");
    }

    #[test]
    fn making_a_pair_is_one_step_to_undo_and_pasting_makes_none() {
        let mut d = Document::new("x ");
        d.perform(Action::SetCaret { offset: 2, extend: false });
        typed(&mut d, "(");
        assert_eq!(d.source(), "x ()");
        d.perform(Action::Undo);
        assert_eq!(d.source(), "x ");
        d.perform(Action::Redo);
        assert_eq!(d.source(), "x ()");
        assert_eq!(d.selection().head, 3, "the caret is between");
        let mut p = Document::new("");
        p.perform(Action::Paste("(".into()));
        assert_eq!(p.source(), "(");
    }
}
