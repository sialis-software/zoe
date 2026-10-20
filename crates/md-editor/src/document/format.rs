//! The formatting commands behind the bar above the editor: bold, headings, lists, links and the
//! rest. Each is a set of edits to the source made as one undo step, with the caret put where
//! the next keystroke is wanted. Character-level commands wrap the selection (with nothing
//! selected they put in a pair of markers and the caret between); line-level ones change the
//! lines the selection touches. Doing a command again takes it back (docs/14 §1).

use super::{Document, Edit, Selection, Wrap};
use crate::blocks::{Block, Kind, block_at};
use std::ops::Range;

/// The three kinds of list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Numbered,
    Task,
}

/// Which formats the selection is in, for the bar's buttons to show as pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Active {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    /// In a `[label](address)` link.
    pub link: bool,
    /// The heading level of the lines selected (0 for plain paragraphs), or `None` if they differ
    /// or are not text.
    pub heading: Option<u8>,
    /// The kind of list that all the lines selected are in.
    pub list: Option<ListKind>,
    pub quote: bool,
    pub code_block: bool,
}

/// Markers around some text on a line: where the opening and the closing ones are.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pair {
    open: Range<usize>,
    close: Range<usize>,
}

impl Pair {
    /// Whether `r` is inside the markers (touching them is inside), or is all of it, markers too.
    fn holds(&self, r: &Range<usize>) -> bool {
        (r.start >= self.open.end && r.end <= self.close.start) || (r.start == self.open.start && r.end == self.close.end)
    }
}

/// A `[label](address)` link on a line.
struct MdLink {
    whole: Range<usize>,
    label: Range<usize>,
}

/// An edit: the range of the old text, and what replaces it.
type Change = (Range<usize>, String);

/// Where `offset` goes when `edits` (in order, not overlapping, ranges in the old text) are made.
/// An offset right at an insertion goes after it if `after`.
fn shift(offset: usize, edits: &[Change], after: bool) -> usize {
    let mut delta = 0isize;
    for (r, t) in edits {
        let grow = t.len() as isize - r.len() as isize;
        if offset > r.end {
            delta += grow;
        } else if offset > r.start {
            return (r.start as isize + t.len() as isize + delta) as usize;
        } else if offset == r.start && r.is_empty() && after {
            delta += grow;
        }
    }
    (offset as isize + delta) as usize
}

fn leading_spaces(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b' ').count()
}

/// The runs of `c` in `line` (byte ranges); a run after a backslash starts after it.
fn runs(line: &str, c: u8) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != c {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < bytes.len() && bytes[end] == c {
            end += 1;
        }
        let start = if i > 0 && bytes[i - 1] == b'\\' { i + 1 } else { i };
        if start < end {
            found.push(start..end);
        }
        i = end;
    }
    found
}

/// The pairs of `wrap`'s markers on `line`, which starts at `base` in the source.
fn pairs(line: &str, base: usize, wrap: Wrap) -> Vec<Pair> {
    let at = |r: &Range<usize>| base + r.start..base + r.end;
    let mut found = Vec::new();
    match wrap {
        // A run of backticks closes at the next run of the same length; the same for `~~`.
        Wrap::Code | Wrap::Strike => {
            let c = if wrap == Wrap::Code { b'`' } else { b'~' };
            let mut open: Option<Range<usize>> = None;
            for run in runs(line, c).into_iter().filter(|r| wrap == Wrap::Code || r.len() == 2) {
                match open.take() {
                    Some(o) if o.len() == run.len() => found.push(Pair { open: at(&o), close: at(&run) }),
                    Some(_) | None => open = Some(run),
                }
            }
        }
        // Stars: a run opens when text follows it and closes when text comes before it, and
        // closes the nearest open run of the same length (1 is italic, 2 bold, 3 both).
        Wrap::Bold | Wrap::Italic => {
            let all = runs(line, b'*');
            let mut stack: Vec<&Range<usize>> = Vec::new();
            let wanted = if wrap == Wrap::Bold { 2 } else { 1 };
            for run in all.iter().filter(|r| r.len() <= 3) {
                let can_open = line[run.end..].chars().next().is_some_and(|c| !c.is_whitespace());
                let can_close = line[..run.start].chars().next_back().is_some_and(|c| !c.is_whitespace());
                if can_close && let Some(k) = stack.iter().rposition(|o| o.len() == run.len()) {
                    let open = stack[k].clone();
                    stack.truncate(k);
                    if run.len() == wanted || run.len() == 3 {
                        found.push(Pair { open: at(&open), close: at(run) });
                    }
                } else if can_open {
                    stack.push(run);
                }
            }
        }
    }
    found
}

/// The `[label](address)` links on `line`. `[[wikilinks]]` and `![images]` are not them.
fn md_links(line: &str, base: usize) -> Vec<MdLink> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let not_one = bytes[i] != b'[' || bytes.get(i + 1) == Some(&b'[') || (i > 0 && matches!(bytes[i - 1], b'[' | b'!' | b'\\'));
        if not_one {
            i += 1;
            continue;
        }
        let Some(close) = line[i + 1..].find(']').map(|k| i + 1 + k) else { break };
        if bytes.get(close + 1) == Some(&b'(')
            && let Some(end) = line[close + 2..].find(')').map(|k| close + 2 + k)
        {
            found.push(MdLink { whole: base + i..base + end + 1, label: base + i + 1..base + close });
            i = end + 1;
        } else {
            i = close + 1;
        }
    }
    found
}

impl Document {
    // ---- what the selection is in ----------------------------------------------------

    /// The text ranges that character-level commands act on: the selection (or the caret) on a
    /// line, or on each line of a selection of several, without the white space at their ends.
    fn inline_segments(&self) -> Vec<Range<usize>> {
        let r = self.selection.range();
        if r.is_empty() {
            return vec![r];
        }
        let (first, last) = self.touched(&r);
        let mut found = Vec::new();
        for block in &self.blocks[first..=last] {
            if matches!(block.kind, Kind::Blank | Kind::Frontmatter) {
                continue;
            }
            let (lo, hi) = (r.start.max(block.content_start()), r.end.min(block.range.end));
            if lo >= hi {
                continue;
            }
            let text = &self.source[lo..hi];
            let (start, end) = (lo + (text.len() - text.trim_start().len()), hi - (text.len() - text.trim_end().len()));
            if start < end {
                found.push(start..end);
            }
        }
        found
    }

    /// The word the caret is in or touching, if nothing is selected and it is in text a format can
    /// go round: what the character-level commands act on instead of an empty pair of markers.
    fn word_at_caret(&self) -> Option<Range<usize>> {
        let at = self.selection.head;
        let block = &self.blocks[block_at(&self.blocks, at)];
        if !self.selection.is_empty() || matches!(block.kind, Kind::Blank | Kind::Fence | Kind::Code | Kind::Frontmatter) {
            return None;
        }
        let (lo, hi) = (block.content_start(), block.range.end);
        if at < lo || at > hi {
            return None;
        }
        let in_word = |c: &char| c.is_alphanumeric();
        let before: usize = self.source[lo..at].chars().rev().take_while(in_word).map(char::len_utf8).sum();
        let after: usize = self.source[at..hi].chars().take_while(in_word).map(char::len_utf8).sum();
        (before + after > 0).then(|| at - before..at + after)
    }

    /// What bold, italic, code and strikethrough act on: the word at the caret when nothing is
    /// selected, else the same as `inline_segments`.
    fn wrap_segments(&self) -> Vec<Range<usize>> {
        self.word_at_caret().map(|w| vec![w]).unwrap_or_else(|| self.inline_segments())
    }

    /// The first and last block a selection touches: one that it only reaches the very start of
    /// does not count.
    fn touched(&self, r: &Range<usize>) -> (usize, usize) {
        let first = block_at(&self.blocks, r.start);
        let mut last = block_at(&self.blocks, r.end);
        if last > first && r.end == self.blocks[last].range.start {
            last -= 1;
        }
        (first, last)
    }

    /// The blocks the selection touches, without the frontmatter and the locked title.
    fn touched_blocks(&self) -> Vec<usize> {
        let (first, last) = self.touched(&self.selection.range());
        let front = self.front_end();
        (first..=last).filter(|&i| self.blocks[i].kind != Kind::Frontmatter && self.blocks[i].range.start >= front).collect()
    }

    /// The blocks that line-level commands change: of those touched, the ones that are text (not
    /// code or a rule), and, when there are several, not the blank ones.
    fn text_targets(&self) -> Vec<usize> {
        let mut ids = self.touched_blocks();
        if ids.len() > 1 {
            ids.retain(|&i| self.blocks[i].kind != Kind::Blank);
        }
        ids.retain(|&i| !matches!(self.blocks[i].kind, Kind::Fence | Kind::Code | Kind::Rule));
        ids
    }

    /// The pair of `wrap`'s markers that holds `seg`, if there is one.
    fn pair_around(&self, wrap: Wrap, seg: &Range<usize>) -> Option<Pair> {
        let block = &self.blocks[block_at(&self.blocks, seg.start)];
        if matches!(block.kind, Kind::Fence | Kind::Code | Kind::Frontmatter) {
            return None;
        }
        pairs(block.text(&self.source), block.range.start, wrap).into_iter().find(|p| p.holds(seg))
    }

    /// The markers when the caret is between a pair of them with nothing in it (`**|**`, which
    /// is a run of four stars, not a pair): what a second press takes out.
    fn empty_pair(&self, wrap: Wrap) -> Option<Range<usize>> {
        let (m, at) = (wrap.marker(), self.selection.head);
        let c = m.chars().next()?;
        let before = self.source[..at].chars().rev().take_while(|&x| x == c).count();
        let after = self.source[at..].chars().take_while(|&x| x == c).count();
        (self.selection.is_empty() && before == m.len() && after == m.len()).then(|| at - m.len()..at + m.len())
    }

    fn link_around(&self, seg: &Range<usize>) -> Option<MdLink> {
        let block = &self.blocks[block_at(&self.blocks, seg.start)];
        if matches!(block.kind, Kind::Fence | Kind::Code | Kind::Frontmatter) {
            return None;
        }
        md_links(block.text(&self.source), block.range.start).into_iter().find(|l| seg.start >= l.label.start && seg.end < l.whole.end)
    }

    fn list_kind(block: &Block) -> Option<ListKind> {
        match &block.kind {
            Kind::Item { task: Some(_), .. } => Some(ListKind::Task),
            Kind::Item { ordered: Some(_), .. } => Some(ListKind::Numbered),
            Kind::Item { .. } => Some(ListKind::Bullet),
            _ => None,
        }
    }

    /// The formats the selection is in (what the bar's buttons show as pressed).
    pub fn active_format(&self) -> Active {
        let segs = self.wrap_segments();
        let inline = |wrap| !segs.is_empty() && (self.empty_pair(wrap).is_some() || segs.iter().all(|s| self.pair_around(wrap, s).is_some()));
        let blocks: Vec<&Block> = self.text_targets().into_iter().map(|i| &self.blocks[i]).collect();
        let level = |b: &Block| match b.kind {
            Kind::Heading(n) => Some(n),
            Kind::Blank | Kind::Paragraph => Some(0),
            _ => None,
        };
        let heading = blocks.first().and_then(|b| level(b)).filter(|&n| blocks.iter().all(|b| level(b) == Some(n)));
        let list = blocks.first().and_then(|b| Self::list_kind(b)).filter(|&k| blocks.iter().all(|b| Self::list_kind(b) == Some(k)));
        let head = &self.blocks[block_at(&self.blocks, self.selection.head)];
        Active {
            bold: inline(Wrap::Bold),
            italic: inline(Wrap::Italic),
            code: inline(Wrap::Code),
            strike: inline(Wrap::Strike),
            link: !segs.is_empty() && segs.iter().all(|s| self.link_around(s).is_some()),
            heading,
            list,
            quote: !blocks.is_empty() && blocks.iter().all(|b| b.kind == Kind::Quote),
            code_block: matches!(head.kind, Kind::Fence | Kind::Code),
        }
    }

    // ---- making the edits ------------------------------------------------------------

    /// Make `edits` (in order, not overlapping) as one undo step, with `after` as the selection.
    /// Edits that change nothing are left out, and if none are left nothing is done.
    fn commit(&mut self, edits: Vec<Change>, after: Selection) {
        let edits: Vec<Change> = edits.into_iter().filter(|(r, t)| self.source[r.clone()] != **t).collect();
        if edits.is_empty() {
            return;
        }
        // Later text first, so that the places of the earlier edits are still right when they come.
        let steps = edits
            .into_iter()
            .rev()
            .map(|(r, t)| Edit { at: r.start, removed: self.source[r.clone()].to_string(), inserted: t })
            .collect();
        let before = self.selection;
        self.apply_step(steps, before, after, None);
    }

    /// The selection once `edits` are made: each end moves with the text.
    fn moved_selection(&self, edits: &[Change]) -> Selection {
        let sel = self.selection;
        Selection { anchor: shift(sel.anchor, edits, true), head: shift(sel.head, edits, true) }
    }

    // ---- character-level -------------------------------------------------------------

    /// Bold, italic, code or strikethrough: put the markers round the selection (round each line
    /// of it), or take them off if it is inside them already. With nothing selected the markers
    /// go in as a pair with the caret between.
    pub(super) fn toggle_wrap(&mut self, wrap: Wrap) {
        let m = wrap.marker();
        let segs = self.wrap_segments();
        if segs.is_empty() {
            return;
        }
        let found: Vec<Option<Pair>> = segs.iter().map(|s| self.pair_around(wrap, s)).collect();
        let sel = self.selection;
        let mut edits: Vec<Change> = Vec::new();
        if let Some(empty) = self.empty_pair(wrap) {
            return self.commit(vec![(empty, String::new())], Selection::caret(sel.head - m.len()));
        }
        if found.iter().all(Option::is_some) {
            // All of a run of backticks, but of a run of stars (`***` is both) only this format's.
            for pair in found.into_iter().flatten() {
                let take = if wrap == Wrap::Code { pair.open.len() } else { m.len() };
                edits.push((pair.open.start..pair.open.start + take, String::new()));
                edits.push((pair.close.end - take..pair.close.end, String::new()));
            }
        } else if sel.is_empty() && segs[0].is_empty() {
            return self.commit(vec![(sel.head..sel.head, format!("{m}{m}"))], Selection::caret(sel.head + m.len()));
        } else {
            for (seg, pair) in segs.iter().zip(&found) {
                if pair.is_none() {
                    edits.push((seg.start..seg.start, m.into()));
                    edits.push((seg.end..seg.end, m.into()));
                }
            }
        }
        // The ends of the selection stay on its text: the start after an opening marker put there,
        // the end before a closing one.
        let range = sel.range();
        let (lo, hi) = (shift(range.start, &edits, true), shift(range.end, &edits, false));
        let after = if sel.is_empty() {
            // The caret stays in its word, inside the markers: at its end, before the closing one.
            Selection::caret(shift(sel.head, &edits, sel.head != segs[0].end))
        } else if sel.anchor <= sel.head { Selection { anchor: lo, head: hi } } else { Selection { anchor: hi, head: lo } };
        self.commit(edits, after);
    }

    /// Link the selection: `[text]()` with the caret in the address, `[](address)` when what is
    /// selected is an address, or `[]()` with the caret in the label. In a link already, take the
    /// link off and leave its label.
    pub(super) fn toggle_link(&mut self) {
        let segs = self.inline_segments();
        if segs.is_empty() {
            return;
        }
        let links: Vec<Option<MdLink>> = segs.iter().map(|s| self.link_around(s)).collect();
        if links.iter().all(Option::is_some) {
            let mut edits = Vec::new();
            for link in links.into_iter().flatten() {
                edits.push((link.whole.start..link.label.start, String::new()));
                edits.push((link.label.end..link.whole.end, String::new()));
            }
            let after = self.moved_selection(&edits);
            return self.commit(edits, after);
        }
        // One line only: a link does not run over a line break.
        let [seg] = segs.as_slice() else { return };
        let text = &self.source[seg.clone()];
        let address = ["http://", "https://", "mailto:", "www."].iter().any(|p| text.starts_with(p));
        let (new, caret) = match (seg.is_empty(), address) {
            (true, _) => ("[]()".to_string(), seg.start + 1),
            (false, true) => (format!("[]({text})"), seg.start + 1),
            (false, false) => (format!("[{text}]()"), seg.start + text.len() + 3),
        };
        self.commit(vec![(seg.clone(), new)], Selection::caret(caret));
    }

    /// `[[` and `]]` round the selection, or an empty pair, for a link to a passage or a note.
    pub(super) fn wiki_link(&mut self) {
        let r = self.selection.range();
        let inner = self.source[r.clone()].to_string();
        let after = Selection { anchor: r.start + 2, head: r.start + 2 + inner.len() };
        self.commit(vec![(r, format!("[[{inner}]]"))], after);
    }

    // ---- line-level ------------------------------------------------------------------

    /// Make the lines of the selection headings of `level` (1 to 6), or paragraphs (0), whatever
    /// they were (a list item or quote becomes the heading).
    pub(super) fn set_heading(&mut self, level: u8) {
        let marker = if level == 0 { String::new() } else { format!("{} ", "#".repeat(level.min(6) as usize)) };
        let mut edits = Vec::new();
        for i in self.text_targets() {
            let b = &self.blocks[i];
            let replaced = match b.kind {
                Kind::Embed => continue,
                Kind::Heading(_) | Kind::Item { .. } | Kind::Quote => b.range.start..b.content_start(),
                _ => b.range.start..b.range.start,
            };
            edits.push((replaced, marker.clone()));
        }
        let after = self.moved_selection(&edits);
        self.commit(edits, after);
    }

    /// Make the lines of the selection a list of this kind, or, if they all are already, plain lines.
    pub(super) fn toggle_list(&mut self, kind: ListKind) {
        let ids = self.text_targets();
        let mut edits = Vec::new();
        if ids.iter().all(|&i| Self::list_kind(&self.blocks[i]) == Some(kind)) {
            for &i in &ids {
                let b = &self.blocks[i];
                edits.push((b.range.start..b.content_start(), String::new()));
            }
        } else {
            let mut number = 0;
            for &i in &ids {
                let b = &self.blocks[i];
                let line = b.text(&self.source);
                // An existing bullet keeps its character (`*` or `+`).
                let bullet = match &b.kind {
                    Kind::Item { indent, ordered: None, .. } => line[*indent..].chars().next().unwrap_or('-'),
                    _ => '-',
                };
                let marker = match kind {
                    ListKind::Bullet => format!("{bullet} "),
                    ListKind::Numbered => {
                        number += 1;
                        format!("{number}. ")
                    }
                    ListKind::Task => format!("{bullet} [ ] "),
                };
                let (replaced, indent) = match &b.kind {
                    Kind::Item { indent, .. } => (b.range.start..b.content_start(), " ".repeat(*indent)),
                    Kind::Heading(_) | Kind::Quote => (b.range.start..b.content_start(), String::new()),
                    _ => {
                        let at = b.range.start + leading_spaces(line);
                        (at..at, String::new())
                    }
                };
                edits.push((replaced, format!("{indent}{marker}")));
            }
        }
        let after = self.moved_selection(&edits);
        self.commit(edits, after);
    }

    /// Put `> ` before the lines of the selection, or take one level of it off if they all have it.
    pub(super) fn toggle_quote(&mut self) {
        let ids = self.text_targets();
        let mut edits = Vec::new();
        if !ids.is_empty() && ids.iter().all(|&i| self.blocks[i].kind == Kind::Quote) {
            for &i in &ids {
                let b = &self.blocks[i];
                let bar = b.range.start + leading_spaces(b.text(&self.source));
                let space = usize::from(self.source[bar + 1..].starts_with(' '));
                edits.push((bar..bar + 1 + space, String::new()));
            }
        } else {
            for &i in ids.iter().filter(|&&i| self.blocks[i].kind != Kind::Quote) {
                let at = self.blocks[i].range.start;
                edits.push((at..at, "> ".to_string()));
            }
        }
        let after = self.moved_selection(&edits);
        self.commit(edits, after);
    }

    /// A horizontal rule on a line of its own below the selection, with a blank line above it
    /// (directly under text, `---` would make that text a heading), and the caret below it.
    pub(super) fn divider(&mut self) {
        let ids = self.touched_blocks();
        let Some(&last) = ids.last() else { return };
        let block = &self.blocks[last];
        if matches!(block.kind, Kind::Fence | Kind::Code) {
            return;
        }
        let at_end = last + 1 == self.blocks.len();
        let (at, mut text) = if block.kind != Kind::Blank {
            (block.range.end, "\n\n---".to_string())
        } else if last > 0 && self.blocks[last - 1].kind != Kind::Blank {
            (block.range.start, "\n---".to_string())
        } else {
            (block.range.start, "---".to_string())
        };
        if at_end {
            text.push('\n');
        }
        // (Below the end of the text there is the line break that was there already.)
        let caret = at + text.len() + usize::from(!at_end);
        self.commit(vec![(at..at, text)], Selection::caret(caret));
    }

    /// The fences that hold the block `index`, if it is a fence or inside a fenced block (the
    /// second is `None` for a fence that is never closed).
    fn fences_around(&self, index: usize) -> Option<(usize, Option<usize>)> {
        let mut open: Option<usize> = None;
        for (i, b) in self.blocks.iter().enumerate() {
            if b.kind != Kind::Fence {
                continue;
            }
            match open.take() {
                Some(o) if (o..=i).contains(&index) => return Some((o, Some(i))),
                Some(_) => {}
                None => open = Some(i),
            }
        }
        open.filter(|&o| o <= index).map(|o| (o, None))
    }

    /// The range of block `index` with a line break, so that taking it out takes the line.
    fn line_with_break(&self, index: usize) -> Range<usize> {
        let r = self.blocks[index].range.clone();
        match self.blocks.get(index + 1) {
            Some(next) => r.start..next.range.start,
            None if r.start > 0 => r.start - 1..r.end,
            None => r,
        }
    }

    /// Fence the lines of the selection as code, or take the fences off the block the caret is in.
    pub(super) fn toggle_code_block(&mut self) {
        let head = block_at(&self.blocks, self.selection.head);
        if let Some((open, close)) = self.fences_around(head) {
            let first = self.line_with_break(open);
            let edits = match close.map(|c| self.line_with_break(c)) {
                None => vec![(first, String::new())],
                // Fences with nothing between them (and the last line's break before it).
                Some(second) if second.start < first.end => vec![(first.start..second.end, String::new())],
                Some(second) => vec![(first, String::new()), (second, String::new())],
            };
            let after = self.moved_selection(&edits);
            return self.commit(edits, after);
        }
        let ids = self.touched_blocks();
        let (Some(&first), Some(&last)) = (ids.first(), ids.last()) else { return };
        let (start, end) = (self.blocks[first].range.start, self.blocks[last].range.end);
        let sel = self.selection;
        if start == end {
            return self.commit(vec![(start..start, "```\n\n```".into())], Selection::caret(start + 4));
        }
        let to = |o: usize| if o < start { o } else if o > end { o + 8 } else { o + 4 };
        let edits = vec![(start..start, "```\n".to_string()), (end..end, "\n```".to_string())];
        self.commit(edits, Selection { anchor: to(sel.anchor), head: to(sel.head) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Action;

    /// A document from `text` where `|` marks the caret, or two of them the selection.
    fn doc(text: &str) -> Document {
        let marks: Vec<usize> = text.match_indices('|').map(|(i, _)| i).collect();
        let mut d = Document::new(&text.replace('|', ""));
        match marks.as_slice() {
            [a] => d.perform(Action::Select(*a..*a)),
            [a, b] => d.perform(Action::Select(*a..*b - 1)),
            _ => false,
        };
        d
    }

    /// The source with `|` put in at the selection's ends.
    fn shown(d: &Document) -> String {
        let (s, r) = (d.source(), d.selection().range());
        format!("{}|{}{}{}", &s[..r.start], &s[r.start..r.end], if r.is_empty() { "" } else { "|" }, &s[r.end..])
    }

    fn run(text: &str, action: Action) -> String {
        let mut d = doc(text);
        d.perform(action);
        shown(&d)
    }

    #[test]
    fn a_selection_is_wrapped_and_a_second_press_unwraps_it() {
        assert_eq!(run("make |bold| now", Action::Wrap(Wrap::Bold)), "make **|bold|** now");
        assert_eq!(run("make **|bold|** now", Action::Wrap(Wrap::Bold)), "make |bold| now");
        assert_eq!(run("a |b| c", Action::Wrap(Wrap::Italic)), "a *|b|* c");
        assert_eq!(run("a |b| c", Action::Wrap(Wrap::Code)), "a `|b|` c");
        assert_eq!(run("a |b| c", Action::Wrap(Wrap::Strike)), "a ~~|b|~~ c");
        assert_eq!(run("a ~~|b|~~ c", Action::Wrap(Wrap::Strike)), "a |b| c");
    }

    #[test]
    fn with_nothing_selected_the_markers_go_in_as_a_pair_and_the_caret_between() {
        assert_eq!(run("a | b", Action::Wrap(Wrap::Bold)), "a **|** b");
        assert_eq!(run("|", Action::Wrap(Wrap::Code)), "`|`");
        assert_eq!(run("**|**", Action::Wrap(Wrap::Bold)), "|", "pressed again between them, they come out");
        assert!(doc("**|**").active_format().bold);
        assert_eq!(run("**bo|ld**", Action::Wrap(Wrap::Bold)), "bo|ld", "the caret inside bold text un-bolds it");
    }

    #[test]
    fn with_nothing_selected_in_a_word_the_markers_go_round_the_word() {
        assert_eq!(run("make bo|ld now", Action::Wrap(Wrap::Bold)), "make **bo|ld** now");
        assert_eq!(run("make |bold now", Action::Wrap(Wrap::Italic)), "make *|bold* now");
        assert_eq!(run("make bold| now", Action::Wrap(Wrap::Strike)), "make ~~bold|~~ now");
        assert_eq!(run("make **bo|ld** now", Action::Wrap(Wrap::Bold)), "make bo|ld now");
        assert_eq!(run("# a ti|tle", Action::Wrap(Wrap::Bold)), "# a **ti|tle**");
        assert_eq!(run("- it|em", Action::Wrap(Wrap::Bold)), "- **it|em**");
        assert_eq!(run("na\u{ef}ve| x", Action::Wrap(Wrap::Bold)), "**na\u{ef}ve|** x");
        assert!(doc("make bo|ld").active_format().bold == false);
        assert!(doc("make **bo|ld**").active_format().bold);
        let mut d = doc("make bo|ld now");
        d.perform(Action::Wrap(Wrap::Bold));
        d.perform(Action::Undo);
        assert_eq!(shown(&d), "make bo|ld now");
    }

    #[test]
    fn the_whole_span_with_its_markers_selected_is_unwrapped_and_white_space_stays_outside() {
        assert_eq!(run("a |**b**| c", Action::Wrap(Wrap::Bold)), "a |b| c");
        assert_eq!(run("a| b |c", Action::Wrap(Wrap::Bold)), "a| **b** |c");
    }

    #[test]
    fn bold_and_italic_do_not_mistake_each_other() {
        assert_eq!(run("**|b|**", Action::Wrap(Wrap::Italic)), "***|b|***");
        assert_eq!(run("*|b|*", Action::Wrap(Wrap::Bold)), "***|b|***");
        // Taking one of the two off `***b***` leaves the other.
        assert_eq!(run("***|b|***", Action::Wrap(Wrap::Bold)), "*|b|*");
        assert_eq!(run("***|b|***", Action::Wrap(Wrap::Italic)), "**|b|**");
        assert_eq!(run("**a *|b|* c**", Action::Wrap(Wrap::Italic)), "**a |b| c**");
        let active = doc("**a *b|* c**").active_format();
        assert_eq!(active, Active { bold: true, italic: true, heading: Some(0), ..Active::default() });
    }

    #[test]
    fn a_selection_over_several_lines_is_wrapped_line_by_line() {
        assert_eq!(run("|one\n- two\n\nthree|", Action::Wrap(Wrap::Bold)), "**|one**\n- **two**\n\n**three|**");
        assert_eq!(run("|**one**\n- **two**|", Action::Wrap(Wrap::Bold)), "|one\n- two|");
        let mut d = doc("|one\ntwo|");
        d.perform(Action::Wrap(Wrap::Italic));
        d.perform(Action::Undo);
        assert_eq!(d.source(), "one\ntwo", "all the lines go back in one step");
    }

    #[test]
    fn each_command_is_one_undo_step_and_undo_puts_the_selection_back() {
        let actions = [
            Action::Wrap(Wrap::Bold),
            Action::Heading(2),
            Action::List(ListKind::Numbered),
            Action::Quote,
            Action::Divider,
            Action::CodeBlock,
            Action::Link,
            Action::WikiLink,
        ];
        for action in actions {
            let mut d = doc("one\n|two three|\nfour");
            let (source, selection) = (d.source().to_string(), d.selection());
            assert!(d.perform(action.clone()), "{action:?} changes the text");
            d.perform(Action::Undo);
            assert_eq!((d.source(), d.selection()), (source.as_str(), selection), "{action:?} undone in one step");
            d.perform(Action::Redo);
            assert_ne!(d.source(), source, "{action:?} redone");
        }
    }

    #[test]
    fn headings_are_set_not_toggled_and_replace_other_line_syntax() {
        assert_eq!(run("he|llo", Action::Heading(2)), "## he|llo");
        assert_eq!(run("# he|llo", Action::Heading(3)), "### he|llo");
        assert_eq!(run("## he|llo", Action::Heading(2)), "## he|llo");
        assert_eq!(run("## he|llo", Action::Heading(0)), "he|llo");
        assert_eq!(run("- he|llo", Action::Heading(1)), "# he|llo");
        assert_eq!(run("|", Action::Heading(1)), "# |");
        assert_eq!(run("|a\nb|", Action::Heading(2)), "## |a\n## b|");
        let mut d = doc("a\n\n|b");
        assert!(!d.perform(Action::Heading(0)), "a paragraph is already one");
    }

    #[test]
    fn lists_are_made_numbered_and_taken_off_again() {
        assert_eq!(run("on|e", Action::List(ListKind::Bullet)), "- on|e");
        assert_eq!(run("- on|e", Action::List(ListKind::Bullet)), "on|e");
        assert_eq!(run("on|e", Action::List(ListKind::Task)), "- [ ] on|e");
        assert_eq!(run("- [x] on|e", Action::List(ListKind::Task)), "on|e");
        assert_eq!(run("|a\n\nb\nc|", Action::List(ListKind::Numbered)), "1. |a\n\n2. b\n3. c|");
        assert_eq!(run("|1. a\n2. b|", Action::List(ListKind::Numbered)), "|a\nb|");
        // From one kind to another, keeping the indent and a bullet's character.
        assert_eq!(run("  * on|e", Action::List(ListKind::Task)), "  * [ ] on|e");
        assert_eq!(run("- [ ] on|e", Action::List(ListKind::Numbered)), "1. on|e");
        assert_eq!(run("> on|e", Action::List(ListKind::Bullet)), "- on|e");
        assert_eq!(run("|", Action::List(ListKind::Bullet)), "- |");
    }

    #[test]
    fn quotes_are_added_and_one_level_taken_off() {
        assert_eq!(run("on|e", Action::Quote), "> on|e");
        assert_eq!(run("> on|e", Action::Quote), "on|e");
        assert_eq!(run("> > on|e", Action::Quote), "> on|e");
        assert_eq!(run("|a\n> b|", Action::Quote), "> |a\n> b|", "the line that is quoted already is not quoted twice");
        assert_eq!(run("|> a\n> b|", Action::Quote), "|a\nb|");
        assert_eq!(run("- on|e", Action::Quote), "> - on|e");
    }

    #[test]
    fn a_divider_goes_below_with_a_blank_line_above_and_the_caret_below_it() {
        assert_eq!(run("on|e", Action::Divider), "one\n\n---\n|");
        assert_eq!(run("on|e\n\ntwo", Action::Divider), "one\n\n---\n|\ntwo");
        assert_eq!(run("one\n|\ntwo", Action::Divider), "one\n\n---\n|two");
        assert_eq!(run("|", Action::Divider), "---\n|");
        assert_eq!(run("one\n\n|", Action::Divider), "one\n\n---\n|");
        assert_eq!(run("|a\nb|\nc", Action::Divider), "a\nb\n\n---\n|c");
        let mut d = doc("one|");
        d.perform(Action::Divider);
        assert_eq!(d.blocks()[2].kind, Kind::Rule, "and it is a rule, not the underline of a heading");
    }

    #[test]
    fn a_code_block_fences_the_lines_and_the_fences_come_off_again() {
        assert_eq!(run("on|e", Action::CodeBlock), "```\non|e\n```");
        assert_eq!(run("|", Action::CodeBlock), "```\n|\n```");
        assert_eq!(run("|a\nb|\nc", Action::CodeBlock), "```\n|a\nb|\n```\nc");
        assert_eq!(run("x\n```\non|e\n```\ny", Action::CodeBlock), "x\non|e\ny");
        assert_eq!(run("```\non|e\n```", Action::CodeBlock), "on|e");
        assert_eq!(run("```\n|\n```", Action::CodeBlock), "|");
        assert_eq!(run("```|\n```", Action::CodeBlock), "|");
        assert_eq!(run("x\n```\non|e", Action::CodeBlock), "x\non|e", "a fence never closed loses its opening line");
        assert!(doc("```\non|e\n```").active_format().code_block);
    }

    #[test]
    fn a_link_wraps_the_text_an_address_or_nothing_and_is_taken_off_with_its_label_kept() {
        assert_eq!(run("see |this| now", Action::Link), "see [this](|) now");
        assert_eq!(run("see |https://a.org| now", Action::Link), "see [|](https://a.org) now");
        assert_eq!(run("see | now", Action::Link), "see [|]() now");
        assert_eq!(run("see [th|is](https://a.org) now", Action::Link), "see th|is now");
        assert_eq!(run("see [this](https://a.org|) now", Action::Link), "see this| now");
        assert!(doc("see [th|is](x)").active_format().link);
        assert!(!doc("see [[th|is]]").active_format().link, "a wikilink is not a link in this sense");
        assert_eq!(run("|a\nb|", Action::Link), "|a\nb|", "no link over a line break");
    }

    #[test]
    fn a_wikilink_goes_round_the_selection_or_in_empty() {
        assert_eq!(run("see | now", Action::WikiLink), "see [[|]] now");
        assert_eq!(run("see |John 3:16| now", Action::WikiLink), "see [[|John 3:16|]] now");
    }

    #[test]
    fn the_active_formats_follow_the_caret() {
        let a = |text: &str| doc(text).active_format();
        assert!(a("x **bo|ld** y").bold && !a("x **bo|ld** y").italic);
        assert!(!a("x **bold** y|").bold);
        assert!(a("x `co|de` y").code);
        assert!(a("x ~~go|ne~~ y").strike);
        assert!(a("x *it|* y").italic);
        assert!(!a("x * not italic* |").italic);
        assert_eq!(a("## he|llo").heading, Some(2));
        assert_eq!(a("he|llo").heading, Some(0));
        assert_eq!(a("|a\n# b|").heading, None, "mixed");
        assert_eq!(a("- [ ] t|ask").list, Some(ListKind::Task));
        assert_eq!(a("1. o|ne").list, Some(ListKind::Numbered));
        assert_eq!(a("- o|ne").list, Some(ListKind::Bullet));
        assert_eq!(a("o|ne").list, None);
        assert!(a("> qu|ote").quote);
        assert!(!a("qu|ote").quote);
        assert!(!a("|**a** **b**|").bold, "the space between is not bold");
        assert!(a("|**a b**|").bold);
    }

    #[test]
    fn the_title_and_frontmatter_of_a_locked_note_are_not_touched() {
        let mut d = Document::new("---\nid: 1\n---\n\n# Title\n\ntext").lock_title();
        d.perform(Action::SelectAll);
        d.perform(Action::Heading(3));
        assert_eq!(d.source(), "---\nid: 1\n---\n\n# Title\n\n### text");
        d.perform(Action::SelectAll);
        d.perform(Action::Quote);
        assert_eq!(d.source(), "---\nid: 1\n---\n\n# Title\n\n> ### text");
    }

    #[test]
    fn code_and_rules_are_left_alone_by_line_commands() {
        assert_eq!(run("```\nco|de\n```", Action::Heading(1)), "```\nco|de\n```");
        assert_eq!(run("--|-", Action::List(ListKind::Bullet)), "--|-");
        assert_eq!(run("```\nco|de\n```", Action::Quote), "```\nco|de\n```");
    }
}
