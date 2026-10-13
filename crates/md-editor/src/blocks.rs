//! Splits markdown source into line blocks (docs/06 §6): one block per source line, each with
//! a kind that decides how it is drawn when rendered. The source stays the single truth;
//! blocks only describe byte ranges in it.

use std::ops::Range;
use study_core::markdown::find_links;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Byte range of the line, not including its trailing newline.
    pub range: Range<usize>,
    pub kind: Kind,
    /// Bytes at the start of the line that are syntax (`## `, `> `, `  - [ ] `), hidden when
    /// the block is rendered.
    pub prefix: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Blank,
    /// A line of the `---` frontmatter, including its fences.
    Frontmatter,
    Heading(u8),
    Quote,
    Item {
        /// Leading spaces.
        indent: usize,
        /// `Some("1.")` for an ordered item.
        ordered: Option<String>,
        /// `Some(checked)` for a task item.
        task: Option<bool>,
    },
    Rule,
    /// A line that is only `![[...]]`.
    Embed,
    /// A ``` or ~~~ line.
    Fence,
    /// A line inside a fenced code block.
    Code,
    Paragraph,
}

impl Block {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.range.clone()]
    }

    /// Where the content (everything after the syntax prefix) starts.
    pub fn content_start(&self) -> usize {
        self.range.start + self.prefix
    }
}

/// Parse every line of `source` into a block. A final empty line (after a trailing newline) is
/// a block too, so the caret has somewhere to be.
pub fn parse(source: &str) -> Vec<Block> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in source.split('\n') {
        lines.push((offset, line));
        offset += line.len() + 1;
    }

    // Frontmatter: a first line `---` and a later line `---`.
    let front_end = (lines.first().is_some_and(|(_, l)| l.trim_end() == "---"))
        .then(|| lines.iter().skip(1).position(|(_, l)| l.trim_end() == "---").map(|p| p + 1))
        .flatten();

    let mut blocks = Vec::with_capacity(lines.len());
    let mut in_fence = false;
    for (index, &(start, line)) in lines.iter().enumerate() {
        let range = start..start + line.len();
        if front_end.is_some_and(|end| index <= end) {
            blocks.push(Block { range, kind: Kind::Frontmatter, prefix: 0 });
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            blocks.push(Block { range, kind: Kind::Fence, prefix: 0 });
            continue;
        }
        if in_fence {
            blocks.push(Block { range, kind: Kind::Code, prefix: 0 });
            continue;
        }
        let (kind, prefix) = classify(line);
        blocks.push(Block { range, kind, prefix });
    }
    blocks
}

fn classify(line: &str) -> (Kind, usize) {
    if line.trim().is_empty() {
        return (Kind::Blank, 0);
    }
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    // Heading: 1-6 hashes then a space.
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        let spaces = trimmed[hashes..].bytes().take_while(|&b| b == b' ').count();
        return (Kind::Heading(hashes as u8), indent + hashes + spaces);
    }

    // Horizontal rule: three or more of one of - * _, nothing else.
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() >= 3 && ["-", "*", "_"].iter().any(|m| compact.chars().all(|c| m.starts_with(c))) {
        return (Kind::Rule, 0);
    }

    // Quote: leading `>` (nested allowed), each followed by an optional space.
    if trimmed.starts_with('>') {
        let mut rest = trimmed;
        let mut prefix = indent;
        while let Some(r) = rest.strip_prefix('>') {
            let r2 = r.strip_prefix(' ').unwrap_or(r);
            prefix += rest.len() - r2.len();
            rest = r2;
        }
        return (Kind::Quote, prefix);
    }

    // List item: `-`, `*`, `+` or `1.` / `1)`, then a space.
    if let Some((marker_len, ordered)) = list_marker(trimmed) {
        let after = &trimmed[marker_len..];
        let spaces = after.bytes().take_while(|&b| b == b' ').count();
        if spaces >= 1 {
            let mut prefix = indent + marker_len + spaces;
            let mut task = None;
            let body = &after[spaces..];
            for (marker, checked) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
                if body.starts_with(marker) && (body.len() == 3 || body[3..].starts_with(' ')) {
                    task = Some(checked);
                    prefix += 3 + usize::from(body.len() > 3);
                    break;
                }
            }
            return (Kind::Item { indent, ordered, task }, prefix);
        }
    }

    // A line that is only an embed.
    let links = find_links(trimmed);
    if links.len() == 1 && links[0].embed && links[0].span == (0..trimmed.trim_end().len()) {
        return (Kind::Embed, 0);
    }
    (Kind::Paragraph, 0)
}

/// Length of a list marker at the start of `s` (`- `, `12. `), and the number text if ordered.
fn list_marker(s: &str) -> Option<(usize, Option<String>)> {
    let first = s.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        return Some((1, None));
    }
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && digits <= 9 && matches!(s[digits..].chars().next(), Some('.') | Some(')')) {
        return Some((digits + 1, Some(s[..digits + 1].to_string())));
    }
    None
}

/// Index of the block that holds `offset` (the end of a line belongs to that line).
pub fn block_at(blocks: &[Block], offset: usize) -> usize {
    blocks
        .iter()
        .position(|b| offset <= b.range.end)
        .unwrap_or_else(|| blocks.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<Kind> {
        parse(source).into_iter().map(|b| b.kind).collect()
    }

    #[test]
    fn every_line_is_a_block_including_a_trailing_empty_one() {
        let blocks = parse("a\n\nb\n");
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].range, 0..1);
        assert_eq!(blocks[1].range, 2..2);
        assert_eq!(blocks[3].range, 5..5);
        assert_eq!(kinds(""), [Kind::Blank]);
    }

    #[test]
    fn headings_quotes_rules_and_paragraphs() {
        let blocks = parse("## Title\n> quoted\n---\n####### seven\nplain");
        assert_eq!(blocks[0].kind, Kind::Heading(2));
        assert_eq!(blocks[0].prefix, 3);
        assert_eq!(blocks[1].kind, Kind::Quote);
        assert_eq!(blocks[1].prefix, 2);
        assert_eq!(blocks[2].kind, Kind::Rule);
        assert_eq!(blocks[3].kind, Kind::Paragraph, "seven hashes is not a heading");
        assert_eq!(blocks[4].kind, Kind::Paragraph);
        assert_eq!(kinds("#nospace")[0], Kind::Paragraph, "a tag, not a heading");
    }

    #[test]
    fn list_items_tasks_and_indent() {
        let blocks = parse("- one\n  * two\n1. three\n- [ ] todo\n- [x] done\n-nospace");
        assert_eq!(blocks[0].kind, Kind::Item { indent: 0, ordered: None, task: None });
        assert_eq!(blocks[0].prefix, 2);
        assert_eq!(blocks[1].kind, Kind::Item { indent: 2, ordered: None, task: None });
        assert_eq!(blocks[1].prefix, 4);
        assert_eq!(blocks[2].kind, Kind::Item { indent: 0, ordered: Some("1.".into()), task: None });
        assert_eq!(blocks[3].kind, Kind::Item { indent: 0, ordered: None, task: Some(false) });
        assert_eq!(blocks[3].prefix, 6);
        assert_eq!(blocks[4].kind, Kind::Item { indent: 0, ordered: None, task: Some(true) });
        assert_eq!(blocks[5].kind, Kind::Paragraph);
    }

    #[test]
    fn frontmatter_and_fences() {
        let blocks = parse("---\ntitle: x\n---\n# H\n```rust\nlet x;\n```\nafter");
        assert!(blocks[..3].iter().all(|b| b.kind == Kind::Frontmatter));
        assert_eq!(blocks[3].kind, Kind::Heading(1));
        assert_eq!(blocks[4].kind, Kind::Fence);
        assert_eq!(blocks[5].kind, Kind::Code);
        assert_eq!(blocks[6].kind, Kind::Fence);
        assert_eq!(blocks[7].kind, Kind::Paragraph);
        // A lone `---` with no closing line is just a rule.
        assert_eq!(kinds("---\ntext")[0], Kind::Rule);
        // Inside a fence, markdown is not interpreted.
        assert_eq!(kinds("```\n# not a heading\n```")[1], Kind::Code);
    }

    #[test]
    fn an_embed_line_is_its_own_kind_but_an_embed_in_prose_is_not() {
        assert_eq!(kinds("![[John 3:16-17]]")[0], Kind::Embed);
        assert_eq!(kinds("  ![[John 3:16]]  ")[0], Kind::Embed);
        assert_eq!(kinds("see ![[John 3:16]] here")[0], Kind::Paragraph);
        assert_eq!(kinds("[[John 3:16]]")[0], Kind::Paragraph, "a plain link is not an embed");
    }

    #[test]
    fn block_at_treats_a_line_end_as_part_of_the_line() {
        let blocks = parse("ab\ncd");
        assert_eq!(block_at(&blocks, 0), 0);
        assert_eq!(block_at(&blocks, 2), 0, "end of the first line");
        assert_eq!(block_at(&blocks, 3), 1);
        assert_eq!(block_at(&blocks, 99), 1);
    }
}
