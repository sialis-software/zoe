//! The frontmatter of a note: a few fields the app knows and guides, written in a fixed order,
//! with anything else the person put there kept as it was (docs/02).
//!
//! ```text
//! ---
//! title: Notes on Nicodemus
//! created: 2026-10-12T23:41:05-07:00
//! tags: [john, new-birth]
//! type: prayer
//! ---
//! ```

use chrono::{DateTime, Local};
use std::ops::Range;

/// What kind of value a property holds. YAML has no declared types, so a value's type is what
/// it looks like: `true`, `3.5`, `2026-10-12`, `[a, b]`, or plain text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropType {
    Text,
    Number,
    Bool,
    Date,
    List,
}

impl std::fmt::Display for PropType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl PropType {
    pub const ALL: [PropType; 5] = [PropType::Text, PropType::Number, PropType::Bool, PropType::Date, PropType::List];

    pub fn label(self) -> &'static str {
        match self {
            PropType::Text => "Text",
            PropType::Number => "Number",
            PropType::Bool => "Yes / No",
            PropType::Date => "Date",
            PropType::List => "List",
        }
    }
}

/// The value of a property outside the ones the app knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Text(String),
    /// Kept as written, so a half-typed number is not lost.
    Number(String),
    Bool(bool),
    /// `2026-10-12`, or with a time.
    Date(String),
    List(Vec<String>),
    /// Something this editor does not model (a nested block, a multi-line value): its lines,
    /// the key line first, written back exactly as they were.
    Raw(Vec<String>),
}

impl Value {
    pub fn prop_type(&self) -> Option<PropType> {
        Some(match self {
            Value::Text(_) => PropType::Text,
            Value::Number(_) => PropType::Number,
            Value::Bool(_) => PropType::Bool,
            Value::Date(_) => PropType::Date,
            Value::List(_) => PropType::List,
            Value::Raw(_) => return None,
        })
    }

    /// A fresh, empty value of a type.
    pub fn empty(kind: PropType) -> Value {
        match kind {
            PropType::Text => Value::Text(String::new()),
            PropType::Number => Value::Number("0".into()),
            PropType::Bool => Value::Bool(false),
            PropType::Date => Value::Date(today()),
            PropType::List => Value::List(Vec::new()),
        }
    }

    /// Read what a value looks like.
    pub fn read(raw: &str) -> Value {
        let v = raw.trim();
        if v.starts_with('"') || v.starts_with('\'') {
            return Value::Text(unquote(v));
        }
        if v.starts_with('[') && v.ends_with(']') {
            return Value::List(tag_list(v));
        }
        match v {
            "true" | "false" => return Value::Bool(v == "true"),
            _ => {}
        }
        if !v.is_empty() && v.parse::<f64>().is_ok() && !v.eq_ignore_ascii_case("nan") && !v.to_lowercase().contains("inf") {
            return Value::Number(v.to_string());
        }
        if is_date(v) {
            return Value::Date(v.to_string());
        }
        Value::Text(v.to_string())
    }

    /// The single line a value is written as (after `key: `).
    fn write(&self) -> String {
        match self {
            // Text that would be read back as something else is quoted to stay text.
            Value::Text(t) => {
                let plain = scalar(t);
                if plain.starts_with('"') || matches!(Value::read(&plain), Value::Text(_)) { plain } else { quoted(t) }
            }
            Value::Number(n) | Value::Date(n) => n.clone(),
            Value::Bool(b) => b.to_string(),
            Value::List(items) => format!("[{}]", items.iter().map(|t| scalar(t)).collect::<Vec<_>>().join(", ")),
            Value::Raw(_) => String::new(),
        }
    }
}

/// `YYYY-MM-DD`, optionally followed by a time.
pub fn is_date(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && (b.len() == 10 || matches!(b[10], b'T' | b' '))
}

/// Today, as a date property's starting value.
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// One property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: Option<String>,
    /// When the note was made: an RFC 3339 time with its offset.
    pub created: Option<String>,
    pub tags: Vec<String>,
    /// What kind of note (`type:`), for filing it.
    pub kind: Option<String>,
    /// Every other property, in the order they were written.
    pub props: Vec<Property>,
}

/// A note's frontmatter and where it sits in the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub front: Frontmatter,
    /// The whole block, from its first `---` through the newline after its last, if there is one.
    pub range: Option<Range<usize>>,
}

/// A replacement of `range` in a note's text with `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// The kinds of note the app offers (`type:`). Any other value written by hand is kept and shown.
pub const KINDS: [&str; 4] = ["prayer", "sermon", "study", "journal"];

/// Names the app uses itself, so a property of the same name cannot be added.
pub const RESERVED_KEYS: [&str; 4] = ["title", "created", "tags", "type"];

/// The current time, as stored in `created:`.
pub fn now() -> String {
    Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// A stored time for people: "Oct 12, 2026 · 11:41 PM" in the time zone it was written in.
pub fn display(stamp: &str) -> String {
    match DateTime::parse_from_rfc3339(stamp.trim()) {
        Ok(t) => t.format("%b %-d, %Y · %-I:%M %p").to_string(),
        Err(_) => stamp.to_string(),
    }
}

pub fn parse(text: &str) -> Parsed {
    let Some(rest) = text.strip_prefix("---\n") else { return Parsed::default() };
    // The closing fence is a line that is exactly `---`.
    let mut at = 4;
    let mut end = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "---" {
            end = Some((at, at + line.len()));
            break;
        }
        at += line.len();
    }
    let Some((body_end, block_end)) = end else { return Parsed::default() };
    let mut front = Frontmatter::default();
    let mut lines = text[4..body_end].lines().peekable();
    while let Some(line) = lines.next() {
        let Some((key, value)) = line.split_once(':').filter(|(k, _)| !k.is_empty() && !k.starts_with([' ', '\t', '-', '#'])) else {
            // A comment, a stray line, or a continuation we did not expect: kept as it was.
            if !line.trim().is_empty() {
                front.props.push(Property { key: String::new(), value: Value::Raw(vec![line.to_string()]) });
            }
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        // A key with nothing after it owns the indented lines that follow (a list, a block).
        if value.is_empty() || value.starts_with(['|', '>']) {
            let mut raw = vec![line.to_string()];
            while let Some(next) = lines.peek() {
                if next.starts_with([' ', '\t']) || next.starts_with("- ") {
                    raw.push(lines.next().unwrap().to_string());
                } else {
                    break;
                }
            }
            front.props.push(Property { key: key.to_string(), value: Value::Raw(raw) });
            continue;
        }
        match key {
            "title" => front.title = Some(unquote(value)),
            "created" => front.created = Some(unquote(value)),
            "type" => front.kind = Some(unquote(value)),
            "tags" => front.tags = tag_list(value),
            _ => front.props.push(Property { key: key.to_string(), value: Value::read(value) }),
        }
    }
    Parsed { front, range: Some(0..block_end) }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    s.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(s).to_string()
}

fn tag_list(value: &str) -> Vec<String> {
    value
        .trim()
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(value)
        .split(',')
        .map(|t| unquote(t).trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A value that YAML would read as something else is quoted.
fn scalar(value: &str) -> String {
    let plain = !value.is_empty()
        && !value.starts_with(['"', '\'', '[', '{', '#', '&', '*', '!', '|', '>', '%', '@', '`', ' '])
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(' ');
    if plain { value.to_string() } else { quoted(value) }
}

impl Frontmatter {
    pub fn is_empty(&self) -> bool {
        *self == Frontmatter::default()
    }

    /// The block, in the app's order: title, created, tags, type, then the rest as they were.
    pub fn render(&self) -> String {
        let mut out = String::from("---\n");
        if let Some(t) = &self.title {
            out.push_str(&format!("title: {}\n", scalar(t)));
        }
        if let Some(c) = &self.created {
            out.push_str(&format!("created: {c}\n"));
        }
        if !self.tags.is_empty() {
            out.push_str(&format!("tags: [{}]\n", self.tags.iter().map(|t| scalar(t)).collect::<Vec<_>>().join(", ")));
        }
        if let Some(k) = &self.kind {
            out.push_str(&format!("type: {}\n", scalar(k)));
        }
        for prop in &self.props {
            match &prop.value {
                Value::Raw(lines) => lines.iter().for_each(|l| {
                    out.push_str(l);
                    out.push('\n');
                }),
                value => out.push_str(&format!("{}: {}\n", prop.key, value.write())),
            }
        }
        out.push_str("---\n");
        out
    }

    pub fn add_tag(&mut self, tag: &str) -> bool {
        let tag = tag.trim().trim_start_matches('#').trim();
        if tag.is_empty() || self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            return false;
        }
        self.tags.push(tag.to_string());
        true
    }
}

/// The edit that changes a note's frontmatter with `change` (adding the block if it has none).
/// Whatever `change` leaves alone stays as it was; the block is rewritten in the app's order.
pub fn update(text: &str, change: impl FnOnce(&mut Frontmatter)) -> Edit {
    let Parsed { mut front, range } = parse(text);
    change(&mut front);
    match range {
        Some(range) => Edit { range, text: front.render() },
        // A new block, with a blank line before whatever the note already says.
        None => Edit { range: 0..0, text: format!("{}{}", front.render(), if text.is_empty() { "" } else { "\n" }) },
    }
}

/// `text` with `edit` applied.
pub fn apply(text: &str, edit: &Edit) -> String {
    format!("{}{}{}", &text[..edit.range.start], edit.text, &text[edit.range.end..])
}

/// The text of a new note: its metadata, then room to write.
pub fn new_note(title: Option<&str>) -> String {
    let front = Frontmatter { title: title.map(str::to_string), created: Some(now()), ..Frontmatter::default() };
    format!("{}\n", front.render())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntitle: Notes on Nicodemus\ntags: [john, new-birth]\nowner: me\n---\n\n# Nicodemus\n";

    #[test]
    fn reads_known_fields_and_keeps_the_rest() {
        let p = parse(NOTE);
        assert_eq!(p.front.title.as_deref(), Some("Notes on Nicodemus"));
        assert_eq!(p.front.tags, ["john", "new-birth"]);
        assert_eq!(p.front.props, [Property { key: "owner".into(), value: Value::Text("me".into()) }]);
        let range = p.range.unwrap();
        assert_eq!(&NOTE[range.clone()], "---\ntitle: Notes on Nicodemus\ntags: [john, new-birth]\nowner: me\n---\n");
        assert!(NOTE[range.end..].starts_with("\n# Nicodemus"));
    }

    #[test]
    fn no_frontmatter_or_an_unclosed_block_is_none() {
        assert_eq!(parse("# Just text\n"), Parsed::default());
        assert_eq!(parse("---\ntitle: x\nno end\n"), Parsed::default());
        assert!(parse("---\n---\n").range.is_some(), "an empty block is a block");
    }

    #[test]
    fn rendering_is_in_a_fixed_order_and_round_trips() {
        let front = Frontmatter {
            kind: Some("prayer".into()),
            props: vec![Property { key: "owner".into(), value: Value::Text("me".into()) }],
            tags: vec!["a".into(), "b".into()],
            created: Some("2026-10-12T23:41:05-07:00".into()),
            title: Some("T".into()),
        };
        let text = front.render();
        assert_eq!(text, "---\ntitle: T\ncreated: 2026-10-12T23:41:05-07:00\ntags: [a, b]\ntype: prayer\nowner: me\n---\n");
        assert_eq!(parse(&text).front, front);
    }

    fn prop(key: &str, value: Value) -> Property {
        Property { key: key.into(), value }
    }

    #[test]
    fn other_properties_are_read_by_what_they_look_like() {
        let text = "---\nread: true\nchapters: 12\nrating: 4.5\ndue: 2026-10-12\nwhen: 2026-10-12T09:30:00-07:00\nwith: [Sam, Ann Lee]\nnote: just words\nquoted: \"42\"\n---\n";
        let props = parse(text).front.props;
        let get = |k: &str| props.iter().find(|p| p.key == k).unwrap().value.clone();
        assert_eq!(get("read"), Value::Bool(true));
        assert_eq!(get("chapters"), Value::Number("12".into()));
        assert_eq!(get("rating"), Value::Number("4.5".into()));
        assert_eq!(get("due"), Value::Date("2026-10-12".into()));
        assert_eq!(get("when"), Value::Date("2026-10-12T09:30:00-07:00".into()));
        assert_eq!(get("with"), Value::List(vec!["Sam".into(), "Ann Lee".into()]));
        assert_eq!(get("note"), Value::Text("just words".into()));
        assert_eq!(get("quoted"), Value::Text("42".into()), "quotes make it text");
        // And they are written back as they were.
        assert_eq!(parse(text).front.render(), text);
    }

    #[test]
    fn text_that_looks_like_something_else_stays_text() {
        for odd in ["42", "true", "2026-10-12", "[a]", "", " spaced "] {
            let front = Frontmatter { props: vec![prop("k", Value::Text(odd.into()))], ..Frontmatter::default() };
            assert_eq!(parse(&front.render()).front.props[0].value, Value::Text(odd.into()), "{odd:?}");
        }
    }

    #[test]
    fn what_the_editor_does_not_model_is_kept_verbatim() {
        let text = "---\ntitle: T\nauthors:\n  - Ann\n  - Sam\n# a comment\nnested:\n  inner: 1\nlast: ok\n---\n";
        let p = parse(text).front;
        assert_eq!(p.title.as_deref(), Some("T"));
        let raw: Vec<_> = p.props.iter().filter(|p| matches!(p.value, Value::Raw(_))).map(|p| p.key.as_str()).collect();
        assert_eq!(raw, ["authors", "", "nested"], "a list block, a comment, a nested block");
        assert_eq!(p.props.last().unwrap().value, Value::Text("ok".into()), "and ordinary ones after them");
        assert_eq!(p.render(), "---\ntitle: T\nauthors:\n  - Ann\n  - Sam\n# a comment\nnested:\n  inner: 1\nlast: ok\n---\n");
    }

    #[test]
    fn a_new_property_starts_empty_of_its_type() {
        assert_eq!(Value::empty(PropType::Bool), Value::Bool(false));
        assert_eq!(Value::empty(PropType::List), Value::List(vec![]));
        assert_eq!(Value::empty(PropType::Number), Value::Number("0".into()));
        assert!(matches!(Value::empty(PropType::Date), Value::Date(d) if is_date(&d)));
        assert_eq!(Value::empty(PropType::Text).prop_type(), Some(PropType::Text));
        assert_eq!(Value::Raw(vec![]).prop_type(), None);
    }

    #[test]
    fn awkward_values_are_quoted_and_survive() {
        let front = Frontmatter { title: Some("Why: a #question".into()), ..Frontmatter::default() };
        let text = front.render();
        assert!(text.contains("title: \"Why: a #question\""), "{text}");
        assert_eq!(parse(&text).front.title.as_deref(), Some("Why: a #question"));
    }

    #[test]
    fn updating_rewrites_only_the_block() {
        let edit = update(NOTE, |f| {
            f.add_tag("grace");
            f.created = Some("2026-10-12T23:41:05-07:00".into());
        });
        let out = apply(NOTE, &edit);
        assert!(out.starts_with("---\ntitle: Notes on Nicodemus\ncreated: 2026-10-12T23:41:05-07:00\ntags: [john, new-birth, grace]\nowner: me\n---\n\n# Nicodemus"), "{out}");
    }

    #[test]
    fn a_note_without_frontmatter_gets_a_block_above_its_text() {
        let edit = update("# Hello\n", |f| f.created = Some("T".into()));
        assert_eq!(apply("# Hello\n", &edit), "---\ncreated: T\n---\n\n# Hello\n");
        let empty = update("", |f| f.created = Some("T".into()));
        assert_eq!(apply("", &empty), "---\ncreated: T\n---\n");
    }

    #[test]
    fn tags_are_added_once_and_without_hashes() {
        let mut f = Frontmatter::default();
        assert!(f.add_tag("  #Grace "));
        assert!(!f.add_tag("grace"), "the same tag, any case");
        assert!(!f.add_tag("   "));
        assert_eq!(f.tags, ["Grace"]);
    }

    #[test]
    fn new_notes_carry_a_created_time_and_it_reads_back() {
        let text = new_note(Some("Untitled"));
        let p = parse(&text);
        assert_eq!(p.front.title.as_deref(), Some("Untitled"));
        let created = p.front.created.expect("a created time");
        assert!(DateTime::parse_from_rfc3339(&created).is_ok(), "{created}");
        assert!(text.ends_with("---\n\n"), "room to write below");
    }

    #[test]
    fn times_are_shown_for_people() {
        assert_eq!(display("2026-10-12T23:41:05-07:00"), "Oct 12, 2026 · 11:41 PM");
        assert_eq!(display("2026-01-05T08:05:00+00:00"), "Jan 5, 2026 · 8:05 AM");
        assert_eq!(display("not a time"), "not a time");
    }
}
