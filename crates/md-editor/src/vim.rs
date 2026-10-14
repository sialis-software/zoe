//! Vim key bindings for the editor: a modal state machine over a [`Document`], with no GUI in
//! it, so the whole thing is tested by feeding keys and reading the text back.
//!
//! The host (the widget) turns its keyboard events into [`Key`]s and calls [`Vim::key`]. In
//! Insert mode ordinary typing is *not* consumed, so the host's own text entry (auto-pairing,
//! IME) keeps working; only Escape and Ctrl-[ are taken. In Normal and Visual mode every plain
//! key is consumed, so a stray letter never ends up in the note. A bare Escape in Normal mode
//! with nothing pending is not consumed either, so the host can let go of the keyboard.
//!
//! Every edit goes through [`Document::perform`] as a single replacement, so one Vim command is
//! one undo step. In Normal mode the Document's selection is a caret on the character under the
//! block cursor; in Visual modes it is the highlighted region, and [`Vim::block_cursor`] says
//! where the moving end is.
//!
//! Supported: counts everywhere; `h j k l` (by logical line), `w b e W B E`, `0 ^ $`, `gg G
//! {n}G`, `f t F T ; ,`, `%`, `{ }`, `+ -`, Ctrl-d / Ctrl-u (ten lines); operators `d c y > <`
//! with every motion, doubled (`dd`), and with text objects `iw aw` and the quote and bracket
//! objects; `D C Y x X s r J p P ~ u Ctrl-r .`; `i a I A o O`; Visual `v V` with the motions,
//! `d x c s y > < ~ u U J r p o` and the text objects.
//!
//! Deliberate gaps: named registers, marks, macros, search (`/ ? n N`), `*`, blockwise Visual,
//! the `g~ gu gU gJ` operators, `ip` and `ap`, `H M L`, and `.` after a Visual operator. `j` and
//! `k` move by logical line, not by wrapped screen line. `c` is two undo steps (the delete and
//! the typing), unlike Vim. Indenting uses two spaces, as the editor's Tab does.

use crate::document::{Action, Document, Selection, next_boundary, prev_boundary};
use std::ops::Range;

/// Spaces per `>` or `<`.
const SHIFT: usize = 2;
/// Lines moved by Ctrl-d and Ctrl-u.
const HALF_PAGE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// A letter with Control held, lowercase.
    Ctrl(char),
    Escape,
    Enter,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl Mode {
    /// The word for a status line ("NORMAL", "VISUAL LINE").
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "VISUAL LINE",
        }
    }
}

/// What a key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Whether the key was taken. When false the host handles it as usual.
    pub consumed: bool,
    /// Whether the text changed.
    pub changed: bool,
}

/// The unnamed register.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Register {
    pub text: String,
    /// Whole lines: the text ends with a newline and pastes on a line of its own.
    pub linewise: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
}

impl Op {
    fn from_key(c: char) -> Option<Op> {
        Some(match c {
            'd' => Op::Delete,
            'c' => Op::Change,
            'y' => Op::Yank,
            '>' => Op::Indent,
            '<' => Op::Outdent,
            _ => return None,
        })
    }

    fn key(self) -> char {
        match self {
            Op::Delete => 'd',
            Op::Change => 'c',
            Op::Yank => 'y',
            Op::Indent => '>',
            Op::Outdent => '<',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FindKind {
    F,
    T,
    BackF,
    BackT,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Find {
    kind: FindKind,
    ch: char,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    None,
    /// After `g`.
    G,
    /// After `f`, `t`, `F` or `T`.
    Find(FindKind),
    /// After `r`.
    Replace,
    /// After `i` or `a` with an operator or in Visual mode; true for inner.
    Object(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span {
    Exclusive,
    Inclusive,
    Linewise,
}

#[derive(Clone, Copy, Debug)]
struct Target {
    pos: usize,
    span: Span,
}

enum Res {
    Go(Target),
    Need(Pending),
    No,
}

/// The last change, for `.`: the keys that made it (without counts), the count, and the text
/// typed in Insert mode if there was any.
#[derive(Clone, Debug)]
struct Change {
    keys: Vec<Key>,
    count: Option<usize>,
    insert: Option<String>,
}

/// An Insert mode session in progress.
#[derive(Clone, Debug)]
struct Session {
    /// Where typing started.
    start: usize,
    /// The whole source when it started, to work out what was typed.
    orig: String,
    keys: Vec<Key>,
    /// The count to give `.`.
    count: Option<usize>,
    /// How many times to type the text (an operator's count is not that).
    times: usize,
    /// Started by `o` or `O`: a repeat count makes more lines.
    open_line: bool,
}

#[derive(Clone, Debug)]
pub struct Vim {
    mode: Mode,
    register: Register,
    count: Option<usize>,
    op: Option<Op>,
    op_count: Option<usize>,
    pending: Pending,
    last_find: Option<Find>,
    /// The column `j` and `k` aim for; `usize::MAX` after `$`.
    want_col: Option<usize>,
    /// Visual mode: the fixed end.
    anchor: usize,
    /// The block cursor. In Normal mode it follows the Document's caret.
    cursor: usize,
    /// The selection Vim last put in the Document, to tell a mouse selection from its own.
    shown: Selection,
    /// Keys of the command being typed, for `.`.
    keys: Vec<Key>,
    /// The command being typed, for a status line.
    show: String,
    last: Option<Change>,
    session: Option<Session>,
    replaying: bool,
    no_repeat: bool,
    dirty: bool,
}

impl Default for Vim {
    fn default() -> Self {
        Vim::new()
    }
}

impl Vim {
    pub fn new() -> Self {
        Vim {
            mode: Mode::Normal,
            register: Register::default(),
            count: None,
            op: None,
            op_count: None,
            pending: Pending::None,
            last_find: None,
            want_col: None,
            anchor: 0,
            cursor: 0,
            shown: Selection::caret(0),
            keys: Vec::new(),
            show: String::new(),
            last: None,
            session: None,
            replaying: false,
            no_repeat: false,
            dirty: false,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn register(&self) -> &Register {
        &self.register
    }

    /// The half-typed command ("2d"), for a status line.
    pub fn pending(&self) -> &str {
        &self.show
    }

    /// Whether Escape would do something here (leave a mode, drop a half-typed command), as
    /// opposed to being left to the host.
    pub fn wants_escape(&self) -> bool {
        self.mode != Mode::Normal || self.has_pending() || self.pending != Pending::None
    }

    /// Where to draw the block cursor; none in Insert mode, where the thin caret is right.
    pub fn block_cursor(&self, doc: &Document) -> Option<usize> {
        match self.mode {
            Mode::Insert => None,
            Mode::Normal => Some(clamp_normal(doc.source(), doc.selection().head)),
            _ => Some(self.cursor),
        }
    }

    /// Take over a selection or caret that something else (the mouse) put in the Document: a
    /// selection becomes Visual mode; a caret is pulled back from the end of its line.
    pub fn sync(&mut self, doc: &mut Document) {
        let sel = doc.selection();
        match self.mode {
            Mode::Insert => {}
            Mode::Normal => {
                if sel.is_empty() {
                    let at = clamp_normal(doc.source(), sel.head);
                    if at != sel.head {
                        doc.perform(Action::SetCaret { offset: at, extend: false });
                    }
                    self.cursor = doc.selection().head;
                } else {
                    self.adopt_selection(doc, sel);
                }
            }
            Mode::Visual | Mode::VisualLine => {
                if sel != self.shown {
                    if sel.is_empty() {
                        self.mode = Mode::Normal;
                        self.cursor = clamp_normal(doc.source(), sel.head);
                    } else {
                        self.adopt_selection(doc, sel);
                    }
                }
            }
        }
    }

    /// A mouse selection is exclusive of its head; Visual mode includes the cursor's character.
    fn adopt_selection(&mut self, doc: &mut Document, sel: Selection) {
        let s = doc.source();
        self.anchor = sel.anchor;
        self.cursor = if sel.head > sel.anchor { prev_boundary(s, sel.head) } else { sel.head };
        self.mode = Mode::Visual;
        self.update_visual(doc);
    }

    /// Feed one key.
    pub fn key(&mut self, doc: &mut Document, key: Key) -> Outcome {
        self.dirty = false;
        let consumed = match self.mode {
            Mode::Insert => self.insert_key(doc, key),
            _ => self.command(doc, key),
        };
        Outcome { consumed, changed: self.dirty }
    }

    fn insert_key(&mut self, doc: &mut Document, key: Key) -> bool {
        if matches!(key, Key::Escape | Key::Ctrl('[')) {
            self.leave_insert(doc);
            return true;
        }
        false
    }

    // ---- command parsing ------------------------------------------------------------

    fn has_pending(&self) -> bool {
        self.count.is_some() || self.op.is_some() || self.op_count.is_some()
    }

    /// The count typed before and after an operator, multiplied.
    fn count(&self) -> Option<usize> {
        match (self.count, self.op_count) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(1).saturating_mul(b.unwrap_or(1))),
        }
    }

    fn reset(&mut self) {
        self.count = None;
        self.op = None;
        self.op_count = None;
        self.pending = Pending::None;
        self.keys.clear();
        self.show.clear();
        self.no_repeat = false;
    }

    /// End a command; a changing one becomes what `.` repeats.
    fn finish(&mut self, repeatable: bool) {
        if repeatable && !self.replaying && !self.no_repeat {
            self.last = Some(Change { keys: std::mem::take(&mut self.keys), count: self.count(), insert: None });
        }
        self.reset();
    }

    fn show_key(&mut self, key: Key) {
        match key {
            Key::Char(c) => self.show.push(c),
            Key::Ctrl(c) => {
                self.show.push('^');
                self.show.push(c.to_ascii_uppercase());
            }
            _ => {}
        }
    }

    fn command(&mut self, doc: &mut Document, key: Key) -> bool {
        self.sync(doc);
        let key = if key == Key::Ctrl('[') { Key::Escape } else { key };
        if key == Key::Escape {
            if self.has_pending() || self.pending != Pending::None {
                self.reset();
                return true;
            }
            if self.mode != Mode::Normal {
                self.leave_visual(doc);
                return true;
            }
            return false;
        }
        let s = doc.source().to_string();
        if self.pending != Pending::None {
            self.keys.push(key);
            self.show_key(key);
            self.pending_key(doc, &s, key);
            return true;
        }
        if let Key::Char(c @ '0'..='9') = key {
            let digit = c as usize - '0' as usize;
            let slot = if self.op.is_some() { &mut self.op_count } else { &mut self.count };
            if digit != 0 || slot.is_some() {
                *slot = Some(slot.unwrap_or(0).saturating_mul(10).saturating_add(digit).min(99_999));
                self.show.push(c);
                return true;
            }
        }
        let key = match key {
            Key::Left | Key::Backspace => Key::Char('h'),
            Key::Right => Key::Char('l'),
            Key::Up => Key::Char('k'),
            Key::Down => Key::Char('j'),
            Key::Home => Key::Char('0'),
            Key::End => Key::Char('$'),
            Key::Delete => Key::Char('x'),
            Key::Enter => Key::Char('+'),
            Key::Char(' ') => Key::Char('l'),
            Key::Tab => return true,
            other => other,
        };
        self.keys.push(key);
        self.show_key(key);
        match key {
            Key::Char(c) => {
                if self.mode == Mode::Normal {
                    self.normal_char(doc, &s, c);
                } else {
                    self.visual_char(doc, &s, c);
                }
                true
            }
            Key::Ctrl(c) => self.ctrl(doc, &s, c),
            _ => true,
        }
    }

    fn ctrl(&mut self, doc: &mut Document, s: &str, c: char) -> bool {
        let n = self.count().unwrap_or(1);
        match c {
            'r' if self.mode == Mode::Normal => {
                for _ in 0..n {
                    self.dirty |= doc.perform(Action::Redo);
                }
                self.after_history(doc);
            }
            'd' | 'u' => {
                let cur = self.cursor;
                let ln = line_no(s, cur);
                let last = line_count(s) - 1;
                let target = if c == 'd' { (ln + HALF_PAGE * n).min(last) } else { ln.saturating_sub(HALF_PAGE * n) };
                let col = *self.want_col.get_or_insert(col_of(s, cur));
                let pos = pos_at_col(s, start_of_line(s, target), col);
                if self.op.is_none() {
                    self.place(doc, pos);
                }
                self.finish(false);
            }
            _ => {
                // Not ours (Ctrl-c, Ctrl-v...): the host may have a shortcut for it.
                self.keys.pop();
                self.reset();
                return false;
            }
        }
        true
    }

    fn after_history(&mut self, doc: &mut Document) {
        let at = doc.selection().range().start;
        self.mode = Mode::Normal;
        self.place(doc, at);
        self.finish(false);
    }

    fn normal_char(&mut self, doc: &mut Document, s: &str, c: char) {
        let cur = self.cursor;
        let n = self.count();
        let times = n.unwrap_or(1);
        if let Some(op) = self.op {
            if c == op.key() {
                self.linewise_op(doc, s, op);
                return;
            }
            if c == 'i' || c == 'a' {
                self.pending = Pending::Object(c == 'i');
                return;
            }
            self.run_motion(doc, s, c, Some(op));
            return;
        }
        match c {
            'd' | 'c' | 'y' | '>' | '<' => self.op = Op::from_key(c),
            'x' => self.shortcut(doc, s, Op::Delete, 'l'),
            'X' => self.shortcut(doc, s, Op::Delete, 'h'),
            's' => self.shortcut(doc, s, Op::Change, 'l'),
            'D' => self.shortcut(doc, s, Op::Delete, '$'),
            'C' => self.shortcut(doc, s, Op::Change, '$'),
            'Y' => {
                self.op = Some(Op::Yank);
                self.linewise_op(doc, s, Op::Yank);
            }
            'r' => self.pending = Pending::Replace,
            'J' => {
                self.join(doc, s, cur, times.max(2));
                self.finish(true);
            }
            'p' | 'P' => {
                self.put(doc, s, c == 'p', times);
                self.finish(true);
            }
            '~' => {
                self.tilde(doc, s, times);
                self.finish(true);
            }
            'u' => {
                for _ in 0..times {
                    self.dirty |= doc.perform(Action::Undo);
                }
                self.after_history(doc);
            }
            '.' => self.repeat(doc, n),
            'i' => self.start_insert(doc, cur),
            'a' => {
                let at = if cur < s.len() && s.as_bytes()[cur] != b'\n' { next_boundary(s, cur) } else { cur };
                self.start_insert(doc, at);
            }
            'I' => self.start_insert(doc, first_blank_end(s, cur)),
            'A' => self.start_insert(doc, line_end(s, cur)),
            'o' => {
                doc.perform(Action::SetCaret { offset: line_end(s, cur), extend: false });
                self.dirty |= doc.perform(Action::Newline);
                self.session_start(doc, true);
            }
            'O' => {
                let ls = line_start(s, cur);
                self.replace(doc, ls..ls, "\n");
                self.start_insert_open(doc, ls);
            }
            'v' | 'V' => {
                self.mode = if c == 'v' { Mode::Visual } else { Mode::VisualLine };
                self.anchor = cur;
                self.update_visual(doc);
                self.reset();
            }
            _ => self.run_motion(doc, s, c, None),
        }
    }

    fn shortcut(&mut self, doc: &mut Document, s: &str, op: Op, motion: char) {
        self.op = Some(op);
        self.run_motion(doc, s, motion, Some(op));
    }

    fn visual_char(&mut self, doc: &mut Document, s: &str, c: char) {
        let line_mode = self.mode == Mode::VisualLine;
        let times = self.count().unwrap_or(1);
        match c {
            'v' | 'V' => {
                let wanted = if c == 'v' { Mode::Visual } else { Mode::VisualLine };
                if self.mode == wanted {
                    self.leave_visual(doc);
                } else {
                    self.mode = wanted;
                    self.update_visual(doc);
                }
                self.reset();
            }
            'o' => {
                std::mem::swap(&mut self.anchor, &mut self.cursor);
                self.update_visual(doc);
                self.reset();
            }
            'd' | 'x' => self.visual_op(doc, s, Op::Delete, line_mode),
            'X' | 'D' => self.visual_op(doc, s, Op::Delete, true),
            'y' => self.visual_op(doc, s, Op::Yank, line_mode),
            'Y' => self.visual_op(doc, s, Op::Yank, true),
            'c' | 's' => self.visual_op(doc, s, Op::Change, line_mode),
            'C' | 'S' | 'R' => self.visual_op(doc, s, Op::Change, true),
            '>' | '<' => {
                let (range, _) = self.visual_range(s, true);
                self.leave_visual_quiet();
                self.shift(doc, s, range, c == '>', times);
                self.finish_visual();
            }
            '~' | 'u' | 'U' => {
                let (range, _) = self.visual_range(s, line_mode);
                let text = &s[range.clone()];
                let new = match c {
                    '~' => toggle_case(text),
                    'u' => text.to_lowercase(),
                    _ => text.to_uppercase(),
                };
                self.leave_visual_quiet();
                if new != text {
                    self.replace(doc, range.clone(), &new);
                }
                self.place(doc, range.start);
                self.finish_visual();
            }
            'J' => {
                let (lo, hi) = self.ends();
                let lines = line_no(s, hi) - line_no(s, lo) + 1;
                self.leave_visual_quiet();
                self.join(doc, s, lo, lines.max(2));
                self.finish_visual();
            }
            'r' => self.pending = Pending::Replace,
            'p' | 'P' => {
                let (range, lw) = self.visual_range(s, line_mode);
                self.leave_visual_quiet();
                self.put_over(doc, s, range, lw);
                self.finish_visual();
            }
            'i' | 'a' => self.pending = Pending::Object(c == 'i'),
            _ => self.run_motion(doc, s, c, None),
        }
    }

    fn finish_visual(&mut self) {
        self.no_repeat = true;
        self.finish(false);
    }

    /// An operator on the Visual region.
    fn visual_op(&mut self, doc: &mut Document, s: &str, op: Op, linewise: bool) {
        let (range, lw) = self.visual_range(s, linewise);
        self.leave_visual_quiet();
        self.no_repeat = true;
        let cur = range.start;
        self.operate(doc, s, op, range, lw, cur);
    }

    fn ends(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    /// The text the Visual region covers, and whether it is whole lines.
    fn visual_range(&self, s: &str, linewise: bool) -> (Range<usize>, bool) {
        let (lo, hi) = self.ends();
        if linewise {
            (line_start(s, lo)..line_end(s, hi), true)
        } else {
            (lo..visual_end(s, hi), false)
        }
    }

    /// Show the Visual region in the Document.
    fn update_visual(&mut self, doc: &mut Document) {
        let s = doc.source();
        let (lo, hi) = self.ends();
        let (start, end) = if self.mode == Mode::VisualLine {
            (line_start(s, lo), line_end(s, hi))
        } else {
            (lo, visual_end(s, hi))
        };
        let forward = self.cursor >= self.anchor;
        let (anchor, head) = if forward { (start, end) } else { (end, start) };
        doc.perform(Action::SelectBetween { anchor, head });
        self.shown = doc.selection();
    }

    /// Back to Normal mode with the cursor where it was.
    fn leave_visual(&mut self, doc: &mut Document) {
        self.leave_visual_quiet();
        let at = self.cursor;
        self.place(doc, at);
    }

    fn leave_visual_quiet(&mut self) {
        self.mode = Mode::Normal;
    }

    // ---- motions --------------------------------------------------------------------

    fn run_motion(&mut self, doc: &mut Document, s: &str, c: char, op: Option<Op>) {
        let n = self.count();
        match self.motion(s, self.cursor, c, n, op) {
            Res::Need(p) => self.pending = p,
            Res::No => self.finish(false),
            Res::Go(t) => self.apply_motion(doc, s, t, op),
        }
    }

    fn apply_motion(&mut self, doc: &mut Document, s: &str, t: Target, op: Option<Op>) {
        let cur = self.cursor;
        match op {
            Some(op) => {
                let (range, lw) = op_range(s, cur, t);
                self.operate(doc, s, op, range, lw, cur);
            }
            None => {
                self.place(doc, t.pos);
                self.finish(false);
            }
        }
    }

    fn pending_key(&mut self, doc: &mut Document, s: &str, key: Key) {
        let pending = std::mem::replace(&mut self.pending, Pending::None);
        let op = self.op;
        let n = self.count();
        let cur = self.cursor;
        match (pending, key) {
            (Pending::G, Key::Char(c)) => {
                let res = match c {
                    'g' => {
                        let line = n.map_or(0, |n| n - 1);
                        Res::Go(line_target(s, line))
                    }
                    'j' | 'k' => self.motion(s, cur, c, n, op),
                    _ => Res::No,
                };
                match res {
                    Res::Go(t) => self.apply_motion(doc, s, t, op),
                    _ => self.finish(false),
                }
            }
            (Pending::Find(kind), Key::Char(ch)) => {
                let find = Find { kind, ch };
                self.last_find = Some(find);
                match find_target(s, cur, find, n.unwrap_or(1), false) {
                    Some(t) => self.apply_motion(doc, s, t, op),
                    None => self.finish(false),
                }
            }
            (Pending::Replace, key) => {
                let new = match key {
                    Key::Char(c) => Some(c.to_string()),
                    Key::Enter => Some("\n".to_string()),
                    _ => None,
                };
                match new {
                    Some(new) if self.mode == Mode::Normal => {
                        self.replace_chars(doc, s, &new, n.unwrap_or(1));
                        self.finish(true);
                    }
                    Some(new) => {
                        let (range, _) = self.visual_range(s, self.mode == Mode::VisualLine);
                        self.leave_visual_quiet();
                        let text: String = s[range.clone()].chars().map(|c| if c == '\n' { c } else { new.chars().next().unwrap_or(c) }).collect();
                        if text != s[range.clone()] && new != "\n" {
                            self.replace(doc, range.clone(), &text);
                        }
                        self.place(doc, range.start);
                        self.finish_visual();
                    }
                    None => self.finish(false),
                }
            }
            (Pending::Object(inner), Key::Char(c)) => match text_object(s, cur, inner, c) {
                Some(r) if r.start < r.end => {
                    if let Some(op) = op {
                        self.operate(doc, s, op, r, false, cur);
                    } else if self.mode != Mode::Normal {
                        self.anchor = r.start;
                        self.cursor = prev_char_pos(s, r.end);
                        self.update_visual(doc);
                        self.reset();
                    } else {
                        self.finish(false);
                    }
                }
                _ => self.finish(false),
            },
            _ => self.finish(false),
        }
    }

    /// Where a motion key goes from `cur`, with the count `n`. `op` is the pending operator.
    fn motion(&mut self, s: &str, cur: usize, c: char, n: Option<usize>, op: Option<Op>) -> Res {
        let cnt = n.unwrap_or(1).max(1);
        if !matches!(c, 'j' | 'k') {
            self.want_col = None;
        }
        let go = |pos, span| Res::Go(Target { pos, span });
        match c {
            'h' => {
                let mut p = cur;
                for _ in 0..cnt {
                    if p > line_start(s, p) {
                        p = prev_boundary(s, p);
                    }
                }
                go(p, Span::Exclusive)
            }
            'l' => {
                let mut p = cur;
                for _ in 0..cnt {
                    if p < line_end(s, p) {
                        p = next_boundary(s, p);
                    }
                }
                go(p, Span::Exclusive)
            }
            'j' | 'k' => {
                let ln = line_no(s, cur);
                let last = line_count(s) - 1;
                let target = if c == 'j' {
                    if ln >= last {
                        return Res::No;
                    }
                    (ln + cnt).min(last)
                } else {
                    if ln == 0 {
                        return Res::No;
                    }
                    ln.saturating_sub(cnt)
                };
                let col = *self.want_col.get_or_insert(col_of(s, cur));
                go(pos_at_col(s, start_of_line(s, target), col), Span::Linewise)
            }
            '+' | '-' => {
                let ln = line_no(s, cur);
                let target = if c == '+' { (ln + cnt).min(line_count(s) - 1) } else { ln.saturating_sub(cnt) };
                if target == ln {
                    return Res::No;
                }
                go(first_nonblank(s, start_of_line(s, target)), Span::Linewise)
            }
            'w' | 'W' => {
                let big = c == 'W';
                if op == Some(Op::Change) && char_at(s, cur).is_some_and(|ch| !ch.is_whitespace()) {
                    // `cw` on a word changes up to its end, like `ce`.
                    let mut p = run_end(s, cur, big);
                    for _ in 1..cnt {
                        p = word_end(s, p, big);
                    }
                    return go(p, Span::Inclusive);
                }
                let mut p = cur;
                for i in 0..cnt {
                    let next = word_fwd(s, p, big);
                    if op.is_some() && i == cnt - 1 && line_start(s, next) != line_start(s, p) {
                        // The last word moved over ends the text; the next line is not eaten.
                        p = line_end(s, p);
                        break;
                    }
                    p = next;
                }
                go(p, Span::Exclusive)
            }
            'e' | 'E' => {
                let mut p = cur;
                for _ in 0..cnt {
                    p = word_end(s, p, c == 'E');
                }
                go(p, Span::Inclusive)
            }
            'b' | 'B' => {
                let mut p = cur;
                for _ in 0..cnt {
                    p = word_back(s, p, c == 'B');
                }
                go(p, Span::Exclusive)
            }
            '0' => go(line_start(s, cur), Span::Exclusive),
            '^' => go(first_nonblank(s, line_start(s, cur)), Span::Exclusive),
            '$' => {
                let target = (line_no(s, cur) + cnt - 1).min(line_count(s) - 1);
                let ls = start_of_line(s, target);
                let le = line_end(s, ls);
                self.want_col = Some(usize::MAX);
                go(if le > ls { prev_boundary(s, le) } else { ls }, Span::Inclusive)
            }
            'G' => {
                let line = n.map_or(usize::MAX, |n| n - 1);
                Res::Go(line_target(s, line))
            }
            'g' => Res::Need(Pending::G),
            'f' => Res::Need(Pending::Find(FindKind::F)),
            't' => Res::Need(Pending::Find(FindKind::T)),
            'F' => Res::Need(Pending::Find(FindKind::BackF)),
            'T' => Res::Need(Pending::Find(FindKind::BackT)),
            ';' | ',' => {
                let Some(mut find) = self.last_find else { return Res::No };
                if c == ',' {
                    find.kind = match find.kind {
                        FindKind::F => FindKind::BackF,
                        FindKind::T => FindKind::BackT,
                        FindKind::BackF => FindKind::F,
                        FindKind::BackT => FindKind::T,
                    };
                }
                find_target(s, cur, find, cnt, true).map_or(Res::No, Res::Go)
            }
            '%' => match_pair(s, cur).map_or(Res::No, |p| go(p, Span::Inclusive)),
            '}' => {
                let mut p = cur;
                for _ in 0..cnt {
                    p = para_fwd(s, p);
                }
                go(p, Span::Exclusive)
            }
            '{' => {
                let mut p = cur;
                for _ in 0..cnt {
                    p = para_back(s, p);
                }
                go(p, Span::Exclusive)
            }
            _ => Res::No,
        }
    }

    // ---- editing --------------------------------------------------------------------

    /// Replace `range` with `text` as one edit.
    fn replace(&mut self, doc: &mut Document, range: Range<usize>, text: &str) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        doc.perform(Action::Select(range));
        let changed = if text.is_empty() { doc.perform(Action::Delete) } else { doc.perform(Action::Paste(text.to_string())) };
        self.dirty |= changed;
    }

    /// Put the cursor at `pos` (pulled back onto a character).
    fn place(&mut self, doc: &mut Document, pos: usize) {
        let pos = clamp_normal(doc.source(), pos);
        self.cursor = pos;
        if self.mode == Mode::Normal {
            doc.perform(Action::SetCaret { offset: pos, extend: false });
            self.cursor = doc.selection().head;
        } else {
            self.update_visual(doc);
        }
    }

    /// `dd`, `cc`, `yy`, `>>` and `<<`: the operator's own key again, on `count` lines.
    fn linewise_op(&mut self, doc: &mut Document, s: &str, op: Op) {
        let n = self.count().unwrap_or(1);
        let cur = self.cursor;
        let target = (line_no(s, cur) + n - 1).min(line_count(s) - 1);
        let range = line_start(s, cur)..line_end(s, start_of_line(s, target));
        self.operate(doc, s, op, range, true, cur);
    }

    fn operate(&mut self, doc: &mut Document, s: &str, op: Op, range: Range<usize>, linewise: bool, cur: usize) {
        let take = |range: &Range<usize>| Register { text: format!("{}{}", &s[range.clone()], if linewise { "\n" } else { "" }), linewise };
        match op {
            Op::Yank => {
                self.register = take(&range);
                let to = if linewise {
                    if range.start < line_start(s, cur) { pos_at_col(s, range.start, col_of(s, cur)) } else { cur }
                } else {
                    range.start
                };
                self.mode = Mode::Normal;
                self.place(doc, to);
                self.finish(false);
            }
            Op::Delete => {
                self.register = take(&range);
                self.delete(doc, s, range, linewise);
                self.finish(true);
            }
            Op::Change => {
                self.register = take(&range);
                let start = range.start;
                self.replace(doc, range, "");
                self.start_insert(doc, start);
            }
            Op::Indent | Op::Outdent => {
                let lines = if linewise { range } else { line_start(s, range.start)..line_end(s, range.end) };
                self.mode = Mode::Normal;
                self.shift(doc, s, lines, op == Op::Indent, 1);
                self.finish(true);
            }
        }
    }

    fn delete(&mut self, doc: &mut Document, s: &str, range: Range<usize>, linewise: bool) {
        self.mode = Mode::Normal;
        if !linewise {
            let start = range.start;
            self.replace(doc, range, "");
            return self.place(doc, start);
        }
        let (ls, le) = (range.start, range.end);
        let gone = if le < s.len() {
            ls..le + 1
        } else if ls > 0 {
            ls - 1..le
        } else {
            0..le
        };
        self.replace(doc, gone, "");
        let new = doc.source().to_string();
        let at = if le < s.len() {
            ls
        } else if ls > 0 {
            line_start(&new, ls - 1)
        } else {
            0
        };
        self.place(doc, first_nonblank(&new, at));
    }

    fn shift(&mut self, doc: &mut Document, s: &str, range: Range<usize>, right: bool, times: usize) {
        let text = &s[range.clone()];
        let amount = SHIFT * times;
        let lines: Vec<String> = text
            .split('\n')
            .map(|l| {
                if right {
                    if l.is_empty() { String::new() } else { format!("{}{l}", " ".repeat(amount)) }
                } else {
                    let tab = l.starts_with('\t');
                    let spaces = l.bytes().take_while(|&b| b == b' ').count().min(amount);
                    l[if tab { 1 } else { spaces }..].to_string()
                }
            })
            .collect();
        let new = lines.join("\n");
        if new != text {
            self.replace(doc, range.clone(), &new);
        }
        let at = first_nonblank(doc.source(), range.start);
        self.place(doc, at);
    }

    /// Join `n` lines starting at the one holding `at`.
    fn join(&mut self, doc: &mut Document, s: &str, at: usize, n: usize) {
        self.mode = Mode::Normal;
        let first = line_start(s, at);
        let mut end = line_end(s, first);
        let mut text = s[first..end].to_string();
        let mut join_at = None;
        for _ in 1..n {
            if end >= s.len() {
                break;
            }
            let next_start = end + 1;
            let next_end = line_end(s, next_start);
            let next = s[next_start..next_end].trim_start();
            let sep = if next.is_empty() || text.ends_with(char::is_whitespace) || text.is_empty() || next.starts_with(')') { "" } else { " " };
            join_at = Some(text.len());
            text.push_str(sep);
            text.push_str(next);
            end = next_end;
        }
        let Some(join_at) = join_at else { return };
        self.replace(doc, first..end, &text);
        self.place(doc, first + join_at);
    }

    /// `p` and `P`.
    fn put(&mut self, doc: &mut Document, s: &str, after: bool, times: usize) {
        let reg = self.register.clone();
        if reg.text.is_empty() {
            return;
        }
        let cur = self.cursor;
        if reg.linewise {
            let body = reg.text.strip_suffix('\n').unwrap_or(&reg.text);
            let block = vec![body; times].join("\n");
            let (at, text, first) = if after {
                let le = line_end(s, cur);
                (le, format!("\n{block}"), le + 1)
            } else {
                let ls = line_start(s, cur);
                (ls, format!("{block}\n"), ls)
            };
            self.replace(doc, at..at, &text);
            let new = doc.source().to_string();
            self.place(doc, first_nonblank(&new, first));
        } else {
            let text = reg.text.repeat(times);
            let at = if after && cur < s.len() && s.as_bytes()[cur] != b'\n' { next_boundary(s, cur) } else { cur };
            self.replace(doc, at..at, &text);
            let new = doc.source().to_string();
            let to = if text.contains('\n') { at } else { prev_char_pos(&new, at + text.len()) };
            self.place(doc, to);
        }
    }

    /// Visual `p`: the region is replaced by the register, and what it held goes in the register.
    fn put_over(&mut self, doc: &mut Document, s: &str, range: Range<usize>, linewise: bool) {
        let reg = self.register.clone();
        let old = Register { text: format!("{}{}", &s[range.clone()], if linewise { "\n" } else { "" }), linewise };
        if reg.text.is_empty() {
            return self.place(doc, range.start);
        }
        let body = reg.text.strip_suffix('\n').unwrap_or(&reg.text);
        let new = match (linewise, reg.linewise) {
            (false, true) => format!("\n{body}\n"),
            (true, false) => reg.text.clone(),
            (true, true) => body.to_string(),
            (false, false) => reg.text.clone(),
        };
        self.replace(doc, range.clone(), &new);
        self.register = old;
        let at = if !linewise && reg.linewise { range.start + 1 } else { range.start };
        let src = doc.source().to_string();
        self.place(doc, if reg.linewise { first_nonblank(&src, at) } else { prev_char_pos(&src, range.start + new.len()) });
    }

    /// `r{char}`: replace `count` characters, or break the line with Enter.
    fn replace_chars(&mut self, doc: &mut Document, s: &str, new: &str, n: usize) {
        let cur = self.cursor;
        let le = line_end(s, cur);
        let mut end = cur;
        for _ in 0..n {
            if end >= le {
                return;
            }
            end = next_boundary(s, end);
        }
        if end == cur {
            return;
        }
        if new == "\n" {
            self.replace(doc, cur..end, "\n");
            return self.place(doc, cur + 1);
        }
        let text = new.repeat(n);
        self.replace(doc, cur..end, &text);
        let src = doc.source().to_string();
        self.place(doc, prev_char_pos(&src, cur + text.len()));
    }

    /// `~` in Normal mode: swap the case of `count` characters and move past them.
    fn tilde(&mut self, doc: &mut Document, s: &str, n: usize) {
        let cur = self.cursor;
        let le = line_end(s, cur);
        let mut end = cur;
        for _ in 0..n {
            if end < le {
                end = next_boundary(s, end);
            }
        }
        if end == cur {
            return;
        }
        let new = toggle_case(&s[cur..end]);
        self.replace(doc, cur..end, &new);
        self.place(doc, cur + new.len());
    }

    // ---- insert mode ----------------------------------------------------------------

    fn start_insert(&mut self, doc: &mut Document, at: usize) {
        doc.perform(Action::SetCaret { offset: at, extend: false });
        self.session_start(doc, false);
    }

    /// Like [`Vim::start_insert`] for a line `O` just opened.
    fn start_insert_open(&mut self, doc: &mut Document, at: usize) {
        doc.perform(Action::SetCaret { offset: at, extend: false });
        self.session_start(doc, true);
    }

    fn session_start(&mut self, doc: &Document, open_line: bool) {
        self.session = Some(Session {
            start: doc.selection().head,
            orig: doc.source().to_string(),
            keys: std::mem::take(&mut self.keys),
            count: self.count(),
            times: if self.op.is_some() { 1 } else { self.count().unwrap_or(1) },
            open_line,
        });
        self.mode = Mode::Insert;
        self.reset();
    }

    fn leave_insert(&mut self, doc: &mut Document) {
        if let Some(session) = self.session.take() {
            let typed = typed_text(&session.orig, doc.source(), session.start);
            let times = session.times;
            if times > 1
                && let Some(t) = typed.as_deref().filter(|t| !t.is_empty())
            {
                let unit = if session.open_line { format!("\n{t}") } else { t.to_string() };
                self.dirty |= doc.perform(Action::Paste(unit.repeat(times - 1)));
            }
            if !self.replaying {
                self.last = Some(Change { keys: session.keys, count: session.count, insert: typed });
            }
        }
        self.mode = Mode::Normal;
        let s = doc.source();
        let head = doc.selection().head;
        let to = if head > line_start(s, head) { prev_boundary(s, head) } else { head };
        self.place(doc, to);
    }

    // ---- dot ------------------------------------------------------------------------

    fn repeat(&mut self, doc: &mut Document, n: Option<usize>) {
        let Some(change) = self.last.clone() else { return self.finish(false) };
        let count = n.or(change.count);
        self.reset();
        self.replaying = true;
        self.count = count;
        for key in &change.keys {
            self.command(doc, *key);
        }
        if self.mode == Mode::Insert {
            if let Some(text) = change.insert.as_deref().filter(|t| !t.is_empty()) {
                self.dirty |= doc.perform(Action::Paste(text.to_string()));
            }
            self.leave_insert(doc);
        }
        self.replaying = false;
        self.last = Some(Change { count, ..change });
        self.reset();
    }
}

/// What was typed since `start`, if the text only grew there.
fn typed_text(orig: &str, now: &str, start: usize) -> Option<String> {
    let grown = now.len().checked_sub(orig.len())?;
    let end = start + grown;
    (now.get(..start)? == orig.get(..start)? && now.get(end..)? == orig.get(start..)?).then(|| now[start..end].to_string())
}

fn toggle_case(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_uppercase() {
                c.to_lowercase().collect::<Vec<_>>()
            } else {
                c.to_uppercase().collect::<Vec<_>>()
            }
        })
        .collect()
}

// ---- text helpers -------------------------------------------------------------------

fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..)?.chars().next()
}

fn next_char_pos(s: &str, i: usize) -> usize {
    i + char_at(s, i).map_or(0, char::len_utf8)
}

fn prev_char_pos(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(k, _)| k)
}

fn line_start(s: &str, pos: usize) -> usize {
    s[..pos].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(s: &str, pos: usize) -> usize {
    s[pos..].find('\n').map_or(s.len(), |i| pos + i)
}

fn line_no(s: &str, pos: usize) -> usize {
    s[..pos].matches('\n').count()
}

fn line_count(s: &str) -> usize {
    s.matches('\n').count() + 1
}

/// Start of line `n` (0-based), or of the last line if there are fewer.
fn start_of_line(s: &str, n: usize) -> usize {
    let mut start = 0;
    for _ in 0..n {
        match s[start..].find('\n') {
            Some(i) => start += i + 1,
            None => break,
        }
    }
    start
}

fn col_of(s: &str, pos: usize) -> usize {
    s[line_start(s, pos)..pos].chars().count()
}

/// The character `col` along the line starting at `ls`, or its last character.
fn pos_at_col(s: &str, ls: usize, col: usize) -> usize {
    let le = line_end(s, ls);
    match s[ls..le].char_indices().nth(col) {
        Some((i, _)) => ls + i,
        None => clamp_normal(s, le),
    }
}

/// A position in Normal mode is on a character: not on the newline or past the end, unless
/// the line is empty.
fn clamp_normal(s: &str, pos: usize) -> usize {
    let pos = pos.min(s.len());
    if pos > line_start(s, pos) && (pos >= s.len() || s.as_bytes()[pos] == b'\n') { prev_boundary(s, pos) } else { pos }
}

/// Where the text of the line holding `pos` starts, unclamped (the end of a blank line).
fn first_blank_end(s: &str, pos: usize) -> usize {
    let ls = line_start(s, pos);
    let le = line_end(s, ls);
    s[ls..le].find(|c| c != ' ' && c != '\t').map_or(le, |i| ls + i)
}

fn first_nonblank(s: &str, pos: usize) -> usize {
    clamp_normal(s, first_blank_end(s, pos))
}

/// Linewise motion to line `line` (clamped), landing on its first non-blank.
fn line_target(s: &str, line: usize) -> Target {
    Target { pos: first_nonblank(s, start_of_line(s, line)), span: Span::Linewise }
}

/// The end of an inclusive motion's range: past the character at `hi`, unless there is none.
fn incl_end(s: &str, hi: usize) -> usize {
    if hi >= s.len() || s.as_bytes()[hi] == b'\n' { hi } else { next_boundary(s, hi) }
}

/// The end of the Visual region: like [`incl_end`], but a selected empty line takes its newline.
fn visual_end(s: &str, hi: usize) -> usize {
    if hi >= s.len() {
        s.len()
    } else if s.as_bytes()[hi] == b'\n' {
        hi + 1
    } else {
        next_boundary(s, hi)
    }
}

/// The text an operator covers from `cur` to the target, and whether it is whole lines.
fn op_range(s: &str, cur: usize, t: Target) -> (Range<usize>, bool) {
    let (lo, hi) = if t.pos < cur { (t.pos, cur) } else { (cur, t.pos) };
    match t.span {
        Span::Linewise => (line_start(s, lo)..line_end(s, hi), true),
        Span::Inclusive => (lo..incl_end(s, hi), false),
        Span::Exclusive => {
            if hi > lo && hi > 0 && s.as_bytes()[hi - 1] == b'\n' {
                // An exclusive motion ending at the start of a line stops at the end of the
                // line before; from the start of a line's text it takes whole lines.
                let end = hi - 1;
                if lo <= first_blank_end(s, lo) { (line_start(s, lo)..end, true) } else { (lo..end, false) }
            } else {
                (lo..hi, false)
            }
        }
    }
}

// ---- words, finds, pairs ------------------------------------------------------------

/// 0 for white space, 1 for punctuation, 2 for word characters; with `big` everything but
/// white space is one class.
fn class(c: char, big: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if big || c.is_alphanumeric() || c == '_' {
        2
    } else {
        1
    }
}

fn is_empty_line(s: &str, p: usize) -> bool {
    char_at(s, p) == Some('\n') && (p == 0 || s.as_bytes()[p - 1] == b'\n')
}

/// `w`: the start of the next word; an empty line counts as one.
fn word_fwd(s: &str, i: usize, big: bool) -> usize {
    let mut p = i;
    match char_at(s, p).map(|c| class(c, big)) {
        None => return s.len(),
        Some(0) => {}
        Some(k) => {
            while char_at(s, p).is_some_and(|c| class(c, big) == k) {
                p = next_char_pos(s, p);
            }
        }
    }
    loop {
        match char_at(s, p) {
            None => return s.len(),
            Some('\n') if p > i && is_empty_line(s, p) => return p,
            Some(c) if class(c, big) == 0 => p = next_char_pos(s, p),
            _ => return p,
        }
    }
}

/// The last character of the run of one class that holds `i`.
fn run_end(s: &str, i: usize, big: bool) -> usize {
    let Some(k) = char_at(s, i).map(|c| class(c, big)) else { return i };
    let mut p = i;
    loop {
        let n = next_char_pos(s, p);
        if char_at(s, n).is_some_and(|c| class(c, big) == k) {
            p = n;
        } else {
            return p;
        }
    }
}

/// `e`: the end of this word, or of the next if already at the end of one.
fn word_end(s: &str, i: usize, big: bool) -> usize {
    let mut p = next_char_pos(s, i);
    if p >= s.len() {
        return if s.is_empty() { 0 } else { prev_char_pos(s, s.len()).max(i) };
    }
    while char_at(s, p).is_some_and(|c| class(c, big) == 0) {
        p = next_char_pos(s, p);
    }
    if p >= s.len() {
        return prev_char_pos(s, s.len());
    }
    run_end(s, p, big)
}

/// `b`: the start of this word, or of the previous if already at the start of one.
fn word_back(s: &str, i: usize, big: bool) -> usize {
    if i == 0 {
        return 0;
    }
    let mut p = prev_char_pos(s, i);
    loop {
        if is_empty_line(s, p) {
            return p;
        }
        if char_at(s, p).is_some_and(|c| class(c, big) == 0) {
            if p == 0 {
                return 0;
            }
            p = prev_char_pos(s, p);
        } else {
            break;
        }
    }
    let k = class(char_at(s, p).unwrap_or(' '), big);
    while p > 0 {
        let q = prev_char_pos(s, p);
        if char_at(s, q).is_some_and(|c| class(c, big) == k) {
            p = q;
        } else {
            break;
        }
    }
    p
}

fn is_blank_line(s: &str, ls: usize) -> bool {
    s[ls..line_end(s, ls)].trim().is_empty()
}

/// `}`: the next blank line after a paragraph, or the end of the text.
fn para_fwd(s: &str, i: usize) -> usize {
    let mut ls = line_start(s, i);
    let mut skipping_blank = true;
    loop {
        let blank = is_blank_line(s, ls);
        if skipping_blank && !blank {
            skipping_blank = false;
        } else if !skipping_blank && blank {
            return ls;
        }
        let le = line_end(s, ls);
        if le >= s.len() {
            return s.len();
        }
        ls = le + 1;
    }
}

/// `{`: the blank line before a paragraph, or the start of the text.
fn para_back(s: &str, i: usize) -> usize {
    let mut ls = line_start(s, i);
    let mut skipping_blank = is_blank_line(s, ls);
    loop {
        if ls == 0 {
            return 0;
        }
        ls = line_start(s, ls - 1);
        let blank = is_blank_line(s, ls);
        if skipping_blank && !blank {
            skipping_blank = false;
        } else if !skipping_blank && blank {
            return ls;
        }
    }
}

/// `f t F T` and their repeats. With `repeat`, `t` and `T` do not stay put against the
/// character they already stopped before.
fn find_target(s: &str, cur: usize, find: Find, n: usize, repeat: bool) -> Option<Target> {
    let ls = line_start(s, cur);
    let le = line_end(s, cur);
    let hits = s[ls..le].char_indices().map(|(i, c)| (ls + i, c)).filter(|&(_, c)| c == find.ch).map(|(i, _)| i);
    match find.kind {
        FindKind::F | FindKind::T => {
            let mut ahead: Vec<usize> = hits.filter(|&i| i > cur).collect();
            if repeat && find.kind == FindKind::T && ahead.first() == Some(&next_char_pos(s, cur)) {
                ahead.remove(0);
            }
            let p = *ahead.get(n - 1)?;
            let pos = if find.kind == FindKind::T { prev_char_pos(s, p) } else { p };
            Some(Target { pos, span: Span::Inclusive })
        }
        FindKind::BackF | FindKind::BackT => {
            let mut behind: Vec<usize> = hits.filter(|&i| i < cur).collect();
            behind.reverse();
            if repeat && find.kind == FindKind::BackT && behind.first().is_some_and(|&p| next_char_pos(s, p) == cur) {
                behind.remove(0);
            }
            let p = *behind.get(n - 1)?;
            let pos = if find.kind == FindKind::BackT { next_char_pos(s, p) } else { p };
            Some(Target { pos, span: Span::Exclusive })
        }
    }
}

/// `%`: the bracket matching the first one at or after `cur` on its line.
fn match_pair(s: &str, cur: usize) -> Option<usize> {
    let le = line_end(s, cur);
    let (p, ch) = s[cur..le].char_indices().map(|(i, c)| (cur + i, c)).find(|&(_, c)| "()[]{}".contains(c))?;
    let (open, close, forward) = match ch {
        '(' => ('(', ')', true),
        ')' => ('(', ')', false),
        '[' => ('[', ']', true),
        ']' => ('[', ']', false),
        '{' => ('{', '}', true),
        _ => ('{', '}', false),
    };
    let mut depth = 0usize;
    if forward {
        for (i, c) in s[p..].char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(p + i);
                }
            }
        }
    } else {
        for (i, c) in s[..=p].char_indices().rev() {
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

// ---- text objects -------------------------------------------------------------------

fn text_object(s: &str, cur: usize, inner: bool, c: char) -> Option<Range<usize>> {
    match c {
        'w' | 'W' => word_object(s, cur, inner, c == 'W'),
        '"' | '\'' | '`' => quote_object(s, cur, inner, c),
        '(' | ')' | 'b' => pair_object(s, cur, inner, '(', ')'),
        '[' | ']' => pair_object(s, cur, inner, '[', ']'),
        '{' | '}' | 'B' => pair_object(s, cur, inner, '{', '}'),
        '<' | '>' => pair_object(s, cur, inner, '<', '>'),
        _ => None,
    }
}

fn word_object(s: &str, cur: usize, inner: bool, big: bool) -> Option<Range<usize>> {
    let ls = line_start(s, cur);
    let le = line_end(s, cur);
    let k = class(char_at(s, cur).filter(|_| cur < le)?, big);
    let mut start = cur;
    while start > ls && char_at(s, prev_char_pos(s, start)).is_some_and(|c| class(c, big) == k) {
        start = prev_char_pos(s, start);
    }
    let mut end = cur;
    while end < le && char_at(s, end).is_some_and(|c| class(c, big) == k) {
        end = next_char_pos(s, end);
    }
    if inner {
        return Some(start..end);
    }
    let blank = |p: usize| char_at(s, p).is_some_and(|c| c == ' ' || c == '\t');
    if k == 0 {
        // Space and the word after it.
        while end < le && char_at(s, end).is_some_and(|c| class(c, big) == 2 || (big && class(c, big) != 0)) {
            end = next_char_pos(s, end);
        }
        if end == cur || end == start {
            return Some(start..end);
        }
        return Some(start..end);
    }
    let word_end = end;
    while end < le && blank(end) {
        end = next_char_pos(s, end);
    }
    if end == word_end {
        while start > ls && blank(prev_char_pos(s, start)) {
            start = prev_char_pos(s, start);
        }
    }
    Some(start..end)
}

fn quote_object(s: &str, cur: usize, inner: bool, q: char) -> Option<Range<usize>> {
    let ls = line_start(s, cur);
    let le = line_end(s, cur);
    let mut marks = Vec::new();
    let mut escaped = false;
    for (i, c) in s[ls..le].char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == q {
            marks.push(ls + i);
        }
    }
    let pairs: Vec<(usize, usize)> = marks.chunks_exact(2).map(|p| (p[0], p[1])).collect();
    let (open, close) = pairs.iter().copied().find(|&(a, b)| a <= cur && cur <= b).or_else(|| pairs.iter().copied().find(|&(a, _)| a > cur))?;
    if inner {
        return Some(open + 1..close);
    }
    let mut end = close + 1;
    let blank = |p: usize| char_at(s, p).is_some_and(|c| c == ' ' || c == '\t');
    let mut start = open;
    let after = end;
    while end < le && blank(end) {
        end += 1;
    }
    if end == after {
        while start > ls && blank(start - 1) {
            start -= 1;
        }
    }
    Some(start..end)
}

fn pair_object(s: &str, cur: usize, inner: bool, open: char, close: char) -> Option<Range<usize>> {
    let upto = next_char_pos(s, cur);
    let mut depth = 0usize;
    let mut found = None;
    for (i, c) in s[..upto].char_indices().rev() {
        if c == close {
            if i != cur {
                depth += 1;
            }
        } else if c == open {
            if depth == 0 {
                found = Some(i);
                break;
            }
            depth -= 1;
        }
    }
    let start = found?;
    let mut depth = 0usize;
    for (i, c) in s[start + 1..].char_indices() {
        let i = start + 1 + i;
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some(if inner { start + 1..i } else { start..i + 1 });
            }
            depth -= 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turn "dw", "<Esc>" and "<C-r>" into keys.
    fn parse(keys: &str) -> Vec<Key> {
        let mut out = Vec::new();
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            if c == '<'
                && let Some(end) = rest.find('>')
            {
                let name = &rest[1..end];
                let key = match name {
                    "Esc" => Some(Key::Escape),
                    "CR" => Some(Key::Enter),
                    "BS" => Some(Key::Backspace),
                    "Left" => Some(Key::Left),
                    "Right" => Some(Key::Right),
                    "Up" => Some(Key::Up),
                    "Down" => Some(Key::Down),
                    n if n.starts_with("C-") => n.chars().nth(2).map(Key::Ctrl),
                    _ => None,
                };
                if let Some(key) = key {
                    out.push(key);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            out.push(Key::Char(c));
            rest = &rest[c.len_utf8()..];
        }
        out
    }

    /// A document and Vim from text with `|` marking the caret.
    fn setup(marked: &str) -> (Document, Vim) {
        let at = marked.find('|').expect("a caret marker");
        let text = marked.replacen('|', "", 1);
        let mut doc = Document::new(&text);
        doc.perform(Action::SetCaret { offset: at, extend: false });
        (doc, Vim::new())
    }

    /// Feed keys the way the widget would: what Vim leaves alone is typed.
    fn feed(doc: &mut Document, vim: &mut Vim, keys: &str) {
        for key in parse(keys) {
            let out = vim.key(doc, key);
            if !out.consumed {
                match key {
                    Key::Char(c) => {
                        doc.perform(Action::Insert(c.to_string()));
                    }
                    Key::Enter => {
                        doc.perform(Action::Newline);
                    }
                    Key::Backspace => {
                        doc.perform(Action::Backspace);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The source with `|` at the cursor.
    fn shown(doc: &Document, vim: &Vim) -> String {
        let at = vim.block_cursor(doc).unwrap_or(doc.selection().head);
        let mut s = doc.source().to_string();
        s.insert(at, '|');
        s
    }

    /// Run `keys` on `marked` and give the result as text with the cursor marked.
    fn run(marked: &str, keys: &str) -> String {
        let (mut doc, mut vim) = setup(marked);
        feed(&mut doc, &mut vim, keys);
        shown(&doc, &vim)
    }

    #[test]
    fn h_and_l_move_within_the_line() {
        assert_eq!(run("a|bc\nd", "l"), "ab|c\nd");
        assert_eq!(run("ab|c\nd", "l"), "ab|c\nd");
        assert_eq!(run("ab|c", "hh"), "|abc");
        assert_eq!(run("a|bc", "h"), "|abc");
        assert_eq!(run("|abcd", "3l"), "abc|d");
        assert_eq!(run("a|bc\nd", "<Right>"), "ab|c\nd");
    }

    #[test]
    fn j_and_k_keep_the_column() {
        assert_eq!(run("ab|cd\nx\nabcd", "j"), "abcd\n|x\nabcd");
        assert_eq!(run("ab|cd\nx\nabcd", "jj"), "abcd\nx\nab|cd");
        assert_eq!(run("abcd\nx\nab|cd", "kk"), "ab|cd\nx\nabcd");
        assert_eq!(run("a|b\ncd", "2j"), "ab\nc|d");
        assert_eq!(run("ab\n|cd", "k"), "|ab\ncd");
    }

    #[test]
    fn j_on_the_last_line_stays() {
        assert_eq!(run("ab\nc|d", "j"), "ab\nc|d");
        assert_eq!(run("a|b\ncd", "k"), "a|b\ncd");
    }

    #[test]
    fn dollar_sticks_to_the_line_end_for_j() {
        assert_eq!(run("|ab\nabcdef\nxy", "$jj"), "ab\nabcdef\nx|y");
        assert_eq!(run("|ab\nabcdef", "$j"), "ab\nabcde|f");
    }

    #[test]
    fn word_motions_follow_vims_classes() {
        assert_eq!(run("|foo bar", "w"), "foo |bar");
        assert_eq!(run("|foo.bar baz", "w"), "foo|.bar baz");
        assert_eq!(run("|foo.bar baz", "W"), "foo.bar |baz");
        assert_eq!(run("|foo bar baz", "2w"), "foo bar |baz");
        assert_eq!(run("foo| bar", "b"), "|foo bar");
        assert_eq!(run("foo b|ar", "b"), "foo |bar");
        assert_eq!(run("foo.ba|r", "B"), "|foo.bar");
        assert_eq!(run("|foo bar", "e"), "fo|o bar");
        assert_eq!(run("fo|o bar", "e"), "foo ba|r");
        assert_eq!(run("|foo.bar", "E"), "foo.ba|r");
    }

    #[test]
    fn w_crosses_lines_and_stops_on_empty_lines() {
        assert_eq!(run("fo|o\nbar", "w"), "foo\n|bar");
        assert_eq!(run("fo|o\n\nbar", "w"), "foo\n|\nbar");
        assert_eq!(run("foo\n|\nbar", "w"), "foo\n\n|bar");
        assert_eq!(run("foo\n|bar", "b"), "|foo\nbar");
        assert_eq!(run("foo\n\n|bar", "b"), "foo\n|\nbar");
    }

    #[test]
    fn line_motions() {
        assert_eq!(run("  fo|o bar", "0"), "|  foo bar");
        assert_eq!(run("  foo b|ar", "^"), "  |foo bar");
        assert_eq!(run("|foo bar\nx", "$"), "foo ba|r\nx");
        assert_eq!(run("|ab\ncd\nef", "2$"), "ab\nc|d\nef");
        assert_eq!(run("|", "$"), "|");
    }

    #[test]
    fn gg_and_g_jump_by_line() {
        assert_eq!(run("a\nb\n  |c", "gg"), "|a\nb\n  c");
        assert_eq!(run("|a\nb\n  c", "G"), "a\nb\n  |c");
        assert_eq!(run("|a\nb\n  c", "2G"), "a\n|b\n  c");
        assert_eq!(run("a\n|b\nc", "3gg"), "a\nb\n|c");
        assert_eq!(run("|a\nb\nc", "99G"), "a\nb\n|c");
    }

    #[test]
    fn find_motions_and_their_repeats() {
        assert_eq!(run("|a,b,c,d", "f,"), "a|,b,c,d");
        assert_eq!(run("|a,b,c,d", "2f,"), "a,b|,c,d");
        assert_eq!(run("|a,b,c,d", "t,"), "|a,b,c,d");
        assert_eq!(run("|ab,cd", "t,"), "a|b,cd");
        assert_eq!(run("a,b,c,|d", "F,"), "a,b,c|,d");
        assert_eq!(run("a,b,c,|d", "T,"), "a,b,c,|d");
        assert_eq!(run("|a,b,c,d", "f,;"), "a,b|,c,d");
        assert_eq!(run("|a,b,c,d", "f,;,"), "a|,b,c,d");
        assert_eq!(run("|a,b,c,d", "f,;;"), "a,b,c|,d");
        assert_eq!(run("|ab,cd,ef", "t,;"), "ab,c|d,ef");
        assert_eq!(run("|abc", "fz"), "|abc");
    }

    #[test]
    fn percent_jumps_between_brackets() {
        assert_eq!(run("|(a [b] c)", "%"), "(a [b] c|)");
        assert_eq!(run("(a [b] c|)", "%"), "|(a [b] c)");
        assert_eq!(run("(a |[b] c)", "%"), "(a [b|] c)");
        assert_eq!(run("a |b (c)", "%"), "a b (c|)");
        assert_eq!(run("|{\n  x\n}", "%"), "{\n  x\n|}");
        assert_eq!(run("|abc", "%"), "|abc");
    }

    #[test]
    fn paragraph_motions() {
        assert_eq!(run("|a\nb\n\nc\n\nd", "}"), "a\nb\n|\nc\n\nd");
        assert_eq!(run("a\nb\n|\nc\n\nd", "}"), "a\nb\n\nc\n|\nd");
        assert_eq!(run("|a\nb", "}"), "a\n|b");
        assert_eq!(run("a\nb\n\nc|", "{"), "a\nb\n|\nc");
        assert_eq!(run("a\n|b", "{"), "|a\nb");
        assert_eq!(run("|a\n\nb\n\nc", "2}"), "a\n\nb\n|\nc");
    }

    #[test]
    fn x_deletes_characters_and_fills_the_register() {
        let (mut doc, mut vim) = setup("a|bcd");
        feed(&mut doc, &mut vim, "x");
        assert_eq!(shown(&doc, &vim), "a|cd");
        assert_eq!(vim.register(), &Register { text: "b".into(), linewise: false });
        feed(&mut doc, &mut vim, "2x");
        assert_eq!(shown(&doc, &vim), "|a");
        assert_eq!(run("a|bc\nd", "5x"), "|a\nd");
        assert_eq!(run("abc|d", "X"), "ab|d");
        assert_eq!(run("abcd|", "x"), "ab|c");
        assert_eq!(run("a|bcd", "2X"), "|bcd");
    }

    #[test]
    fn dw_deletes_to_the_next_word() {
        assert_eq!(run("|foo bar baz", "dw"), "|bar baz");
        assert_eq!(run("|foo bar baz", "2dw"), "|baz");
        assert_eq!(run("|foo bar baz", "d2w"), "|baz");
        assert_eq!(run("foo |bar", "dw"), "foo| ");
    }

    #[test]
    fn dw_on_the_last_word_keeps_the_newline() {
        assert_eq!(run("foo |bar\nbaz", "dw"), "foo| \nbaz");
        assert_eq!(run("|foo\nbaz", "dw"), "|\nbaz");
    }

    #[test]
    fn d_with_other_motions() {
        assert_eq!(run("foo b|ar", "db"), "foo |ar");
        assert_eq!(run("|foo bar", "de"), "| bar");
        assert_eq!(run("foo| bar", "d0"), "| bar");
        assert_eq!(run("  fo|o bar", "d^"), "  |o bar");
        assert_eq!(run("fo|o bar", "d$"), "f|o");
        assert_eq!(run("|a,b,c", "df,"), "|b,c");
        assert_eq!(run("|a,b,c", "dt,"), "|,b,c");
        assert_eq!(run("a,b,|c", "dF,"), "a,b|c");
        assert_eq!(run("|(a b) c", "d%"), "| c");
        assert_eq!(run("a|bc", "dl"), "a|c");
        assert_eq!(run("ab|c", "dh"), "a|c");
    }

    #[test]
    fn d_with_linewise_motions() {
        assert_eq!(run("a\n|b\nc\nd", "dj"), "a\n|d");
        assert_eq!(run("a\nb\n|c\nd", "dk"), "a\n|d");
        assert_eq!(run("a\n|b\nc\nd", "dG"), "|a");
        assert_eq!(run("a\nb\n|c\nd", "dgg"), "|d");
        assert_eq!(run("a\n|b\nc", "d2G"), "a\n|c");
        assert_eq!(run("|a\nb\nc", "d}"), "|");
    }

    #[test]
    fn d_brace_stops_before_the_blank_line() {
        assert_eq!(run("|a\nb\n\nc", "d}"), "|\nc");
    }

    #[test]
    fn dd_deletes_lines_and_registers_them() {
        let (mut doc, mut vim) = setup("a\n|b\nc");
        feed(&mut doc, &mut vim, "dd");
        assert_eq!(shown(&doc, &vim), "a\n|c");
        assert_eq!(vim.register(), &Register { text: "b\n".into(), linewise: true });
        assert_eq!(run("a\nb\n|c", "dd"), "a\n|b");
        assert_eq!(run("|a", "dd"), "|");
        assert_eq!(run("|a\nb\nc\nd", "2dd"), "|c\nd");
        assert_eq!(run("a\n|b\nc\nd", "d2d"), "a\n|d");
        assert_eq!(run("|a\nb", "9dd"), "|");
        assert_eq!(run("a\n  |b\n  c", "dd"), "a\n  |c");
    }

    #[test]
    fn d_and_c_to_the_end_of_line() {
        assert_eq!(run("ab|cd\nx", "D"), "a|b\nx");
        assert_eq!(run("ab|cd\nxy\nz", "2D"), "a|b\nz");
        assert_eq!(run("ab|cd\nx", "Cxy<Esc>"), "abx|y\nx");
        assert_eq!(run("|abc", "D"), "|");
    }

    #[test]
    fn cw_changes_to_the_end_of_the_word() {
        assert_eq!(run("|foo bar", "cwxy<Esc>"), "x|y bar");
        assert_eq!(run("f|oo bar", "cwxy<Esc>"), "fx|y bar");
        assert_eq!(run("fo|o bar", "cwxy<Esc>"), "fox|y bar");
        assert_eq!(run("|foo bar baz", "2cwX<Esc>"), "|X baz");
        assert_eq!(run("|foo bar", "cWX<Esc>"), "|X bar");
        assert_eq!(run("foo| bar", "cwX<Esc>"), "foo|Xbar");
    }

    #[test]
    fn cc_clears_the_line_and_inserts() {
        assert_eq!(run("a\n|bcd\ne", "ccx<Esc>"), "a\n|x\ne");
        assert_eq!(run("a\nb|cd", "cc"), "a\n|");
        assert_eq!(run("|ab\ncd\nef", "2ccX<Esc>"), "|X\nef");
    }

    #[test]
    fn c_with_motions_stays_in_insert_mode() {
        let (mut doc, mut vim) = setup("|foo bar");
        feed(&mut doc, &mut vim, "ce");
        assert_eq!(vim.mode(), Mode::Insert);
        assert_eq!(doc.source(), " bar");
        feed(&mut doc, &mut vim, "hi<Esc>");
        assert_eq!(vim.mode(), Mode::Normal);
        assert_eq!(shown(&doc, &vim), "h|i bar");
        assert_eq!(run("a|bc d", "c$X<Esc>"), "a|X");
    }

    #[test]
    fn s_substitutes_characters() {
        assert_eq!(run("a|bcd", "sX<Esc>"), "a|Xcd");
        assert_eq!(run("a|bcd", "2sX<Esc>"), "a|Xd");
    }

    #[test]
    fn yank_and_put_charwise() {
        assert_eq!(run("|foo bar", "ywP"), "foo| foo bar");
        assert_eq!(run("|foo bar", "yw$p"), "foo barfoo| ");
        assert_eq!(run("|ab cd", "dwP"), "ab| cd");
        assert_eq!(run("|ab", "xp"), "b|a");
        assert_eq!(run("|ab", "xP"), "|ab");
        assert_eq!(run("|ab", "x3p"), "baa|a");
    }

    #[test]
    fn the_cursor_after_charwise_p_is_on_the_last_pasted_character() {
        assert_eq!(run("|abc", "ylp"), "a|abc");
        assert_eq!(run("|abc", "yl2p"), "aa|abc");
    }

    #[test]
    fn yank_puts_the_cursor_at_the_start_of_the_range() {
        assert_eq!(run("foo b|ar", "yb"), "foo |bar");
        assert_eq!(run("a\n|b", "yk"), "|a\nb");
        assert_eq!(run("|a\nb", "yj"), "|a\nb");
    }

    #[test]
    fn linewise_put() {
        assert_eq!(run("|a\nb", "yyp"), "a\n|a\nb");
        assert_eq!(run("a\n|b", "yyP"), "a\n|b\nb");
        assert_eq!(run("|a\nb", "yy2p"), "a\n|a\na\nb");
        assert_eq!(run("a\n|b", "yyp"), "a\nb\n|b");
        assert_eq!(run("|a\nb", "ddp"), "b\n|a");
        assert_eq!(run("|a\n  b", "jddP"), "  |b\na");
        assert_eq!(run("a\n|b\nc", "yjjp"), "a\nb\nc\n|b\nc");
        assert_eq!(run("|a", "yyp"), "a\n|a");
    }

    #[test]
    fn put_with_an_empty_register_does_nothing() {
        assert_eq!(run("|ab", "p"), "|ab");
    }

    #[test]
    fn r_replaces_characters() {
        assert_eq!(run("|abc", "rx"), "|xbc");
        assert_eq!(run("|abc", "2rx"), "x|xc");
        assert_eq!(run("|abc", "4rx"), "|abc");
        assert_eq!(run("a|bc", "r<CR>"), "a\n|c");
        assert_eq!(run("|abc", "r<Esc>"), "|abc");
    }

    #[test]
    fn j_joins_lines_with_one_space() {
        assert_eq!(run("|a\nb", "J"), "a| b");
        assert_eq!(run("|a\n   b", "J"), "a| b");
        assert_eq!(run("|a\nb\nc", "3J"), "a b| c");
        assert_eq!(run("|a\nb\nc", "J"), "a| b\nc");
        assert_eq!(run("|a \nb", "J"), "a |b");
        assert_eq!(run("|a(\n)", "J"), "a(|)");
        assert_eq!(run("|a\n\nb", "J"), "|a\nb");
        assert_eq!(run("a|", "J"), "|a");
    }

    #[test]
    fn tilde_swaps_case_and_advances() {
        assert_eq!(run("|aBc", "~"), "A|Bc");
        assert_eq!(run("|aBc", "3~"), "Ab|C");
        assert_eq!(run("a|b", "~"), "a|B");
    }

    #[test]
    fn insert_commands_place_the_caret() {
        assert_eq!(run("a|bc", "iX<Esc>"), "a|Xbc");
        assert_eq!(run("a|bc", "aX<Esc>"), "ab|Xc");
        assert_eq!(run("  a|bc", "IX<Esc>"), "  |Xabc");
        assert_eq!(run("a|bc", "AX<Esc>"), "abc|X");
        assert_eq!(run("a|b", "a"), "ab|");
    }

    #[test]
    fn typing_in_insert_mode_is_not_consumed() {
        let (mut doc, mut vim) = setup("|abc");
        assert!(vim.key(&mut doc, Key::Char('i')).consumed);
        assert_eq!(vim.mode(), Mode::Insert);
        assert!(!vim.key(&mut doc, Key::Char('x')).consumed);
        assert!(!vim.key(&mut doc, Key::Enter).consumed);
        assert!(!vim.key(&mut doc, Key::Backspace).consumed);
        assert!(!vim.key(&mut doc, Key::Left).consumed);
        assert!(vim.key(&mut doc, Key::Escape).consumed);
        assert_eq!(vim.mode(), Mode::Normal);
    }

    #[test]
    fn escape_leaves_insert_one_to_the_left() {
        assert_eq!(run("|ab", "iXY<Esc>"), "X|Yab");
        assert_eq!(run("|ab", "A<Esc>"), "a|b");
        assert_eq!(run("|", "ix<Esc>"), "|x");
        assert_eq!(run("ab|c", "i<C-[>"), "a|bc");
    }

    #[test]
    fn o_opens_a_line_below_and_o_above() {
        assert_eq!(run("a|b\nc", "oX<Esc>"), "ab\n|X\nc");
        assert_eq!(run("a|b\nc", "OX<Esc>"), "|X\nab\nc");
        assert_eq!(run("a|b", "o"), "ab\n|");
        assert_eq!(run("a\nb|", "ox<Esc>"), "a\nb\n|x");
    }

    #[test]
    fn o_continues_a_list() {
        assert_eq!(run("- a|b", "ox<Esc>"), "- ab\n- |x");
    }

    #[test]
    fn counts_repeat_inserted_text() {
        assert_eq!(run("|", "3ix<Esc>"), "xx|x");
        assert_eq!(run("a|b", "2aXY<Esc>"), "abXYX|Y");
        assert_eq!(run("|a", "2ox<Esc>"), "a\nx\n|x");
    }

    #[test]
    fn undo_and_redo_are_whole_commands() {
        let (mut doc, mut vim) = setup("|foo bar baz");
        feed(&mut doc, &mut vim, "dwdw");
        assert_eq!(doc.source(), "baz");
        feed(&mut doc, &mut vim, "u");
        assert_eq!(shown(&doc, &vim), "|bar baz");
        feed(&mut doc, &mut vim, "u");
        assert_eq!(shown(&doc, &vim), "|foo bar baz");
        feed(&mut doc, &mut vim, "<C-r>");
        assert_eq!(shown(&doc, &vim), "|bar baz");
        feed(&mut doc, &mut vim, "<C-r>");
        assert_eq!(shown(&doc, &vim), "|baz");
        feed(&mut doc, &mut vim, "<C-r>");
        assert_eq!(shown(&doc, &vim), "|baz");
        feed(&mut doc, &mut vim, "2u");
        assert_eq!(shown(&doc, &vim), "|foo bar baz");
    }

    #[test]
    fn undo_restores_a_deleted_line() {
        assert_eq!(run("a\n|b\nc", "ddu"), "a\n|b\nc");
        assert_eq!(run("a|bc", "xu"), "a|bc");
    }

    #[test]
    fn typed_text_is_undone_as_one_step() {
        assert_eq!(run("|", "ihello<Esc>u"), "|");
    }

    #[test]
    fn indent_and_outdent_lines() {
        assert_eq!(run("a|b\nc", ">>"), "  |ab\nc");
        assert_eq!(run("|a\nb\nc", "2>>"), "  |a\n  b\nc");
        assert_eq!(run("|a\nb\nc", ">j"), "  |a\n  b\nc");
        assert_eq!(run("  |a\n   b", "<<"), "|a\n   b");
        assert_eq!(run("|a\n\nb", ">2j"), "  |a\n\n  b");
        assert_eq!(run("    |a", "<<"), "  |a");
        assert_eq!(run("|a", "<<"), "|a");
        assert_eq!(run("\t|a", "<<"), "|a");
    }

    #[test]
    fn dot_repeats_the_last_change() {
        assert_eq!(run("|a b c d", "dw."), "|c d");
        assert_eq!(run("|a b c d e", "d2w."), "|e");
        assert_eq!(run("|a b c d e f", "dw3."), "|e f");
        assert_eq!(run("|abcd", "x."), "|cd");
        assert_eq!(run("|a\nb\nc", "dd."), "|c");
        assert_eq!(run("|a\nb\nc", "yy."), "|a\nb\nc");
        assert_eq!(run("|abc", "rx."), "|xbc");
        assert_eq!(run("|abc", "rxl."), "x|xc");
    }

    #[test]
    fn dot_repeats_inserts() {
        assert_eq!(run("|a b", "cwX<Esc>w."), "X |X");
        assert_eq!(run("|", "ihi<Esc>."), "hh|ii");
        assert_eq!(run("|ab", "Ax<Esc>."), "abx|x");
        assert_eq!(run("|a", "ox<Esc>."), "a\nx\n|x");
        assert_eq!(run("|a", "3ix<Esc>."), "xxxx|xxa");
    }

    #[test]
    fn dot_repeat_is_one_undo_step() {
        assert_eq!(run("|a b c", "dw.u"), "|b c");
    }

    #[test]
    fn dot_does_nothing_before_any_change() {
        assert_eq!(run("a|b", "."), "a|b");
        assert_eq!(run("a|b", "l."), "a|b");
    }

    #[test]
    fn dot_with_find_remembers_its_character() {
        assert_eq!(run("|a,b,c,d", "df,."), "|c,d");
    }

    #[test]
    fn visual_mode_selects_inclusively() {
        let (mut doc, mut vim) = setup("|abcd");
        feed(&mut doc, &mut vim, "vl");
        assert_eq!(vim.mode(), Mode::Visual);
        assert_eq!(doc.selection().range(), 0..2);
        assert_eq!(vim.block_cursor(&doc), Some(1));
        feed(&mut doc, &mut vim, "<Esc>");
        assert_eq!(vim.mode(), Mode::Normal);
        assert_eq!(shown(&doc, &vim), "a|bcd");
    }

    #[test]
    fn visual_delete_yank_and_change() {
        assert_eq!(run("|abcd", "vld"), "|cd");
        assert_eq!(run("|abcd", "vy$p"), "abcd|a");
        assert_eq!(run("|abc def", "vwd"), "|ef");
        assert_eq!(run("a|bcd", "vlcX<Esc>"), "a|Xd");
        assert_eq!(run("ab|cd", "vhd"), "a|d");
        assert_eq!(run("|abcd", "vx"), "|bcd");
        assert_eq!(run("|abcd", "vsX<Esc>"), "|Xbcd");
        assert_eq!(run("a|b\ncd", "vjd"), "|a");
    }

    #[test]
    fn visual_with_counts_and_text_objects() {
        assert_eq!(run("|abcdef", "v3ld"), "|ef");
        assert_eq!(run("foo b|ar baz", "viwd"), "foo | baz");
        assert_eq!(run("foo b|ar baz", "vawd"), "foo |baz");
        assert_eq!(run("f(a|b)g", "vi(d"), "f(|)g");
    }

    #[test]
    fn visual_o_swaps_the_ends() {
        let (mut doc, mut vim) = setup("a|bcd");
        feed(&mut doc, &mut vim, "vlo");
        assert_eq!(vim.block_cursor(&doc), Some(1));
        feed(&mut doc, &mut vim, "h");
        assert_eq!(doc.selection().range(), 0..3);
        feed(&mut doc, &mut vim, "d");
        assert_eq!(doc.source(), "d");
    }

    #[test]
    fn visual_line_operates_on_whole_lines() {
        assert_eq!(run("a\n|b\nc\nd", "Vjd"), "a\n|d");
        assert_eq!(run("a\n|b\nc", "Vd"), "a\n|c");
        assert_eq!(run("|a\nb\nc", "Vjy"), "|a\nb\nc");
        assert_eq!(run("|a\nb\nc", "Vjyjp"), "a\nb\n|a\nb\nc");
        assert_eq!(run("a\n|b\nc", "Vcx<Esc>"), "a\n|x\nc");
        assert_eq!(run("|a\nb\nc", "Vj>"), "  |a\n  b\nc");
        assert_eq!(run("|a\nb", "Vj<Esc>"), "a\n|b");
    }

    #[test]
    fn visual_line_highlights_whole_lines() {
        let (mut doc, mut vim) = setup("a|b\ncd\nef");
        feed(&mut doc, &mut vim, "Vj");
        assert_eq!(vim.mode(), Mode::VisualLine);
        assert_eq!(doc.selection().range(), 0..5);
    }

    #[test]
    fn v_and_capital_v_switch_and_toggle() {
        let (mut doc, mut vim) = setup("|ab\ncd");
        feed(&mut doc, &mut vim, "vV");
        assert_eq!(vim.mode(), Mode::VisualLine);
        feed(&mut doc, &mut vim, "v");
        assert_eq!(vim.mode(), Mode::Visual);
        feed(&mut doc, &mut vim, "v");
        assert_eq!(vim.mode(), Mode::Normal);
    }

    #[test]
    fn visual_case_commands() {
        assert_eq!(run("|aBc", "vl~"), "|Abc");
        assert_eq!(run("|abc", "vlU"), "|ABc");
        assert_eq!(run("|ABC", "vllu"), "|abc");
        assert_eq!(run("|ab", "V~"), "|AB");
    }

    #[test]
    fn visual_r_replaces_every_character() {
        assert_eq!(run("|abcd", "vllrx"), "|xxxd");
    }

    #[test]
    fn visual_j_joins_the_selected_lines() {
        assert_eq!(run("|a\nb\nc\nd", "VjjJ"), "a b| c\nd");
    }

    #[test]
    fn visual_p_replaces_the_selection_and_keeps_the_old_text() {
        let (mut doc, mut vim) = setup("|foo bar");
        feed(&mut doc, &mut vim, "yiww");
        feed(&mut doc, &mut vim, "viwp");
        assert_eq!(doc.source(), "foo foo");
        assert_eq!(vim.register().text, "bar");
        feed(&mut doc, &mut vim, "0viwp");
        assert_eq!(doc.source(), "bar foo");
    }

    #[test]
    fn visual_line_p_swaps_lines() {
        assert_eq!(run("|a\nb", "yyjVp"), "a\n|a");
    }

    #[test]
    fn operators_work_with_text_objects() {
        assert_eq!(run("foo b|ar baz", "diw"), "foo | baz");
        assert_eq!(run("foo b|ar baz", "daw"), "foo |baz");
        assert_eq!(run("foo bar b|az", "daw"), "foo ba|r");
        assert_eq!(run("foo b|ar baz", "ciwX<Esc>"), "foo |X baz");
        assert_eq!(run("f(a b|)g", "di("), "f(|)g");
        assert_eq!(run("f(a b|)g", "da("), "f|g");
        assert_eq!(run("f(a (b) |c)g", "di("), "f(|)g");
        assert_eq!(run("say \"h|i\" ok", "di\""), "say \"|\" ok");
        assert_eq!(run("say \"h|i\" ok", "da\""), "say |ok");
        assert_eq!(run("a |\"b\" c", "ci\"X<Esc>"), "a \"|X\" c");
        assert_eq!(run("x [a|b] y", "yi[P"), "x [a|bab] y");
        assert_eq!(run("x {a|b} y", "di{"), "x {|} y");
    }

    #[test]
    fn a_missing_text_object_does_nothing() {
        assert_eq!(run("a|b", "di("), "a|b");
        assert_eq!(run("a|b", "di\""), "a|b");
    }

    #[test]
    fn escape_cancels_a_pending_operator() {
        assert_eq!(run("|ab cd", "d<Esc>w"), "ab |cd");
        assert_eq!(run("|ab cd", "3<Esc>w"), "ab |cd");
    }

    #[test]
    fn escape_in_normal_mode_is_left_to_the_host() {
        let (mut doc, mut vim) = setup("|ab");
        assert!(!vim.key(&mut doc, Key::Escape).consumed);
        vim.key(&mut doc, Key::Char('d'));
        assert!(vim.key(&mut doc, Key::Escape).consumed);
        assert!(!vim.key(&mut doc, Key::Escape).consumed);
        vim.key(&mut doc, Key::Char('v'));
        assert!(vim.key(&mut doc, Key::Escape).consumed);
    }

    #[test]
    fn unknown_keys_are_swallowed_in_normal_mode() {
        let (mut doc, mut vim) = setup("|ab");
        let out = vim.key(&mut doc, Key::Char('Z'));
        assert!(out.consumed);
        assert!(!out.changed);
        assert_eq!(doc.source(), "ab");
        assert!(!vim.key(&mut doc, Key::Ctrl('c')).consumed);
    }

    #[test]
    fn changed_reports_edits() {
        let (mut doc, mut vim) = setup("|ab");
        assert!(!vim.key(&mut doc, Key::Char('l')).changed);
        assert!(vim.key(&mut doc, Key::Char('x')).changed);
    }

    #[test]
    fn a_failed_motion_cancels_the_operator() {
        assert_eq!(run("a|b", "dfzx"), "|a");
        assert_eq!(run("ab\nc|d", "dj"), "ab\nc|d");
    }

    #[test]
    fn mode_labels() {
        assert_eq!(Mode::Normal.label(), "NORMAL");
        assert_eq!(Mode::VisualLine.label(), "VISUAL LINE");
    }

    #[test]
    fn a_mouse_selection_becomes_visual_mode() {
        let (mut doc, mut vim) = setup("|abcd");
        doc.perform(Action::Select(0..3));
        vim.sync(&mut doc);
        assert_eq!(vim.mode(), Mode::Visual);
        assert_eq!(vim.block_cursor(&doc), Some(2));
        feed(&mut doc, &mut vim, "d");
        assert_eq!(doc.source(), "d");
    }

    #[test]
    fn a_click_inside_visual_mode_returns_to_normal() {
        let (mut doc, mut vim) = setup("|abcd");
        feed(&mut doc, &mut vim, "vl");
        doc.perform(Action::SetCaret { offset: 3, extend: false });
        feed(&mut doc, &mut vim, "x");
        assert_eq!(shown(&doc, &vim), "ab|c");
    }

    #[test]
    fn a_caret_at_the_line_end_is_pulled_back() {
        let (mut doc, mut vim) = setup("ab|\ncd");
        vim.sync(&mut doc);
        assert_eq!(doc.selection().head, 1);
    }

    #[test]
    fn the_pending_command_is_shown() {
        let (mut doc, mut vim) = setup("|abc");
        feed(&mut doc, &mut vim, "2d");
        assert_eq!(vim.pending(), "2d");
        feed(&mut doc, &mut vim, "<Esc>");
        assert_eq!(vim.pending(), "");
    }

    #[test]
    fn ctrl_d_and_ctrl_u_move_by_ten_lines() {
        let text: String = (0..30).map(|i| format!("l{i}\n")).collect();
        let mut doc = Document::new(&text);
        doc.perform(Action::SetCaret { offset: 0, extend: false });
        let mut vim = Vim::new();
        vim.key(&mut doc, Key::Ctrl('d'));
        assert_eq!(line_no(doc.source(), doc.selection().head), 10);
        vim.key(&mut doc, Key::Ctrl('u'));
        assert_eq!(line_no(doc.source(), doc.selection().head), 0);
    }

    #[test]
    fn multibyte_text_is_handled_by_character() {
        assert_eq!(run("|héllo wörld", "w"), "héllo |wörld");
        assert_eq!(run("|héllo", "lx"), "h|llo");
        assert_eq!(run("|héllo", "fl"), "hé|llo");
        assert_eq!(run("|aé", "$x"), "|a");
        assert_eq!(run("é|x", "rü"), "é|ü");
    }

    #[test]
    fn an_empty_document_does_not_panic() {
        for keys in ["x", "dd", "p", "J", "w", "b", "e", "$", "0", "G", "gg", "%", "}", "{", "~", "u", ".", "vd", "Vd", "cc<Esc>", "o<Esc>"] {
            let _ = run("|", keys);
        }
    }

    #[test]
    fn a_hundred_random_keys_never_panic() {
        let alphabet: Vec<Key> = "hjklwbeWBE0^$gGfFtT;,%{}dcyxXsrJpPuiaIAoOvV><~.12 \u{e9}\n".chars().map(Key::Char).chain([Key::Escape, Key::Ctrl('r'), Key::Enter, Key::Backspace]).collect();
        let mut seed = 12345u64;
        for round in 0..40 {
            let (mut doc, mut vim) = setup("Hello (world), \"quoted\" text.\n\n  - item one\n- item two\n\nlast para\n|");
            for _ in 0..100 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let key = alphabet[(seed >> 33) as usize % alphabet.len()];
                let out = vim.key(&mut doc, key);
                if !out.consumed {
                    match key {
                        Key::Char(c) => {
                            doc.perform(Action::Insert(c.to_string()));
                        }
                        Key::Enter => {
                            doc.perform(Action::Newline);
                        }
                        Key::Backspace => {
                            doc.perform(Action::Backspace);
                        }
                        _ => {}
                    }
                }
            }
            let _ = round;
        }
    }
}
