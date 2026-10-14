//! Templates for new notes, written the way Obsidian's are: each is a note of its own, in the
//! vault's `templates/` folder, that a person can open and edit. A template is a *kind* of note:
//! adding a file adds a kind to the New note dialog.
//!
//! Notes are found by tag, never by folder, so every new note is filed in `notes/`.
//!
//! In a template's text, `{{title}}` is the note's title, `{{date}}` and `{{time}}` are now
//! (`{{date:dddd, MMMM D, YYYY}}` gives the format, in Obsidian's/moment's letters). Whatever
//! the app does differently for one kind is said in the template's own frontmatter, with
//! properties that start with `template_` (they are not copied to the notes made from it):
//!
//! - `template_description`: the line the New note dialog shows under the kind's name;
//! - `template_title`: the title a note gets when none is asked for or typed (it may use
//!   placeholders: a daily note is called `{{date:YYYY-MM-DD}}`);
//! - `template_ask_title`: `false` to make the note at once, without asking for a title;
//! - `template_unique`: `true` for one note under that title (asking again opens it).
//!
//! The other properties (`tags`, `type`, any of your own) are the properties the new note starts with.

use crate::frontmatter::{self, Frontmatter, Property, Value};
use chrono::{DateTime, Local};

/// Where the templates are kept in the vault.
pub const FOLDER: &str = "templates/";

/// Whether `path` is a template.
pub fn is_template(path: &str) -> bool {
    path.starts_with(FOLDER)
}

/// A kind of note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    /// Its file, in the vault.
    pub path: String,
    /// What it is called: the file's name.
    pub name: String,
    pub description: String,
    /// Whether a title is asked for.
    pub ask_title: bool,
    pub unique: bool,
    /// The title when there is none typed, if the template says (else its own name).
    default_title: Option<String>,
    /// The properties the new note starts with.
    tags: Vec<String>,
    kind: Option<String>,
    props: Vec<Property>,
    /// The note's text after the properties, as the template has it.
    body: String,
}

impl Template {
    pub fn parse(path: &str, text: &str) -> Template {
        let parsed = frontmatter::parse(text);
        let body = parsed.range.as_ref().map_or(text, |r| &text[r.end..]).trim_start_matches(['\n', '\r']).to_string();
        let name = path.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string();
        let (mut description, mut ask_title, mut unique, mut default_title) = (String::new(), true, false, None);
        let mut props = Vec::new();
        for prop in parsed.front.props {
            let words = |v: &Value| match v {
                Value::Text(t) | Value::Date(t) | Value::Number(t) => t.clone(),
                _ => String::new(),
            };
            match prop.key.as_str() {
                "template_description" => description = words(&prop.value),
                "template_title" => default_title = Some(words(&prop.value)).filter(|t| !t.trim().is_empty()),
                "template_ask_title" => ask_title = !matches!(prop.value, Value::Bool(false)),
                "template_unique" => unique = matches!(prop.value, Value::Bool(true)),
                key if key.starts_with("template_") => {}
                _ => props.push(prop),
            }
        }
        Template {
            path: path.to_string(),
            name,
            description,
            ask_title,
            unique,
            default_title,
            tags: parsed.front.tags,
            kind: parsed.front.kind,
            props,
            body,
        }
    }
}

/// The templates a new vault has, as (path, text): the kinds of note the app starts with.
pub fn builtin() -> Vec<(String, String)> {
    let file = |name: &str, text: &str| (format!("{FOLDER}{name}.md"), text.to_string());
    vec![
        file(
            "Prayer list",
            "---\ntemplate_description: Praise, requests to tick off, and answers.\ntags: [prayer]\n---\n# {{title}}\n\n## Praise\n\n- \n\n## Requests\n\n- [ ] \n\n## Answered\n\n- [x] \n",
        ),
        file(
            "Topic",
            "---\ntemplate_description: Passages, a summary and questions on one subject.\ntemplate_title: Untitled topic\ntags: [topic]\n---\n# {{title}}\n\n## Passages\n\n- \n\n## Summary\n\n\n## Questions\n\n- [ ] \n\n## Related\n\n- \n",
        ),
        file(
            "Daily",
            "---\ntemplate_description: \"Today's reading, reflection and prayer. One a day.\"\ntemplate_title: \"{{date:YYYY-MM-DD}}\"\ntemplate_ask_title: false\ntemplate_unique: true\ntags: [daily]\n---\n# {{date:dddd, MMMM D, YYYY}}\n\n## Reading\n\n- \n\n## Bookmarks\n\n- \n\n## Reflection\n\n\n## Prayer\n\n- [ ] \n\n## Tomorrow\n\n- [ ] \n",
        ),
        file("Blank", "---\ntemplate_description: An empty note.\ntemplate_title: Untitled\n---\n# {{title}}\n\n"),
    ]
}

/// The templates in `notes` (path to text), by name.
pub fn all<'a>(notes: impl Iterator<Item = (&'a String, &'a String)>) -> Vec<Template> {
    let mut found: Vec<Template> = notes.filter(|(path, _)| is_template(path)).map(|(path, text)| Template::parse(path, text)).collect();
    found.sort_by_key(|t| t.name.to_lowercase());
    found
}

/// `text` with the placeholders filled in.
pub fn expand(text: &str, title: &str, now: &DateTime<Local>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let (name, format) = match after[..end].split_once(':') {
            Some((n, f)) => (n.trim(), Some(f)),
            None => (after[..end].trim(), None),
        };
        match name {
            "title" => out.push_str(title),
            "date" => out.push_str(&now.format(&moment_to_strftime(format.unwrap_or("YYYY-MM-DD"))).to_string()),
            "time" => out.push_str(&now.format(&moment_to_strftime(format.unwrap_or("HH:mm"))).to_string()),
            // Not one of ours: left as written.
            _ => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// A date format in moment's letters (as Obsidian's templates use) as strftime's.
fn moment_to_strftime(format: &str) -> String {
    const TOKENS: [(&str, &str); 16] = [
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("MMMM", "%B"),
        ("MMM", "%b"),
        ("MM", "%m"),
        ("M", "%-m"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("DD", "%d"),
        ("D", "%-d"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("mm", "%M"),
        ("ss", "%S"),
        ("A", "%p"),
        ("a", "%P"),
    ];
    let mut out = String::new();
    let mut rest = format;
    'next: while let Some(c) = rest.chars().next() {
        for (token, strftime) in TOKENS {
            if let Some(after) = rest.strip_prefix(token) {
                out.push_str(strftime);
                rest = after;
                continue 'next;
            }
        }
        if c == '%' {
            out.push_str("%%");
        } else {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Today's note, if there is one: the note of a kind that is one a day (a unique one made without
/// asking its title) that exists under today's name.
pub fn todays_note(templates: &[Template], now: &DateTime<Local>, exists: impl Fn(&str) -> bool) -> Option<String> {
    templates
        .iter()
        .filter(|t| t.unique && !t.ask_title)
        .find_map(|t| Some(plan(t, now, None, &exists)).filter(|p| p.text.is_none()).map(|p| p.path))
}

/// What making a note from a template comes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub path: String,
    /// Its text; `None` when the note is already there (a unique one, such as today's daily
    /// note) and is to be opened as it is.
    pub text: Option<String>,
}

/// A title as a file name: what a file system will not have is taken out.
pub fn file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { ' ' } else { c })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches('.').trim().to_string()
}

/// The path and text for a new note from `template`, with the title the person gave (none, or
/// nothing but spaces, is the template's own: see `template_title`). `taken` says whether a
/// path is in use.
pub fn plan(template: &Template, now: &DateTime<Local>, title: Option<&str>, taken: impl Fn(&str) -> bool) -> Plan {
    let given = title.map(str::trim).filter(|t| template.ask_title && !file_name(t).is_empty());
    let own = template
        .default_title
        .as_deref()
        .map(|t| expand(t, &template.name, now))
        .filter(|t| !file_name(t).is_empty())
        .unwrap_or_else(|| template.name.clone());
    let base = file_name(given.unwrap_or(&own));
    let path_of = |n: usize| if n == 1 { format!("notes/{base}.md") } else { format!("notes/{base} {n}.md") };
    // One note under its own name: asking again for it shows the one there is.
    if template.unique && given.is_none() && taken(&path_of(1)) {
        return Plan { path: path_of(1), text: None };
    }
    let n = (1..).find(|&n| !taken(&path_of(n))).unwrap_or(1);
    let path = path_of(n);
    // The heading is what was typed (a number added to make the file name distinct is not part of it).
    let title = given
        .map(str::to_string)
        .unwrap_or_else(|| path.trim_end_matches(".md").rsplit('/').next().unwrap_or_default().to_string());
    let words = |text: &str| expand(text, &title, now);
    let front = Frontmatter {
        created: Some(frontmatter::now()),
        tags: template.tags.iter().map(|t| words(t)).collect(),
        kind: template.kind.as_deref().map(words),
        props: template
            .props
            .iter()
            .map(|p| Property {
                key: p.key.clone(),
                value: match &p.value {
                    Value::Text(t) => Value::Text(words(t)),
                    Value::List(items) => Value::List(items.iter().map(|i| words(i)).collect()),
                    other => other.clone(),
                },
            })
            .collect(),
        ..Frontmatter::default()
    };
    let body = words(&template.body);
    let text = if body.is_empty() { format!("{}\n", front.render()) } else { format!("{}\n{body}", front.render()) };
    Plan { path, text: Some(text) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 13, 9, 30, 0).unwrap()
    }

    fn kind(name: &str) -> Template {
        let (path, text) = builtin().into_iter().find(|(p, _)| p.ends_with(&format!("/{name}.md"))).unwrap();
        Template::parse(&path, &text)
    }

    #[test]
    fn the_builtin_kinds_read_back_with_what_their_frontmatter_says() {
        let names: Vec<String> = builtin().iter().map(|(p, t)| Template::parse(p, t).name).collect();
        assert_eq!(names, ["Prayer list", "Topic", "Daily", "Blank"]);
        let daily = kind("Daily");
        assert_eq!((daily.ask_title, daily.unique), (false, true), "the daily note is named by its date, once a day");
        assert_eq!(daily.description, "Today's reading, reflection and prayer. One a day.");
        let prayer = kind("Prayer list");
        assert_eq!((prayer.ask_title, prayer.unique), (true, false));
        assert!(prayer.description.starts_with("Praise"));
        assert!(prayer.props.is_empty(), "the template's own properties are not note properties");
        assert!(is_template(&prayer.path) && !is_template("notes/x.md"));
    }

    #[test]
    fn a_note_starts_with_the_templates_properties_and_text_with_the_placeholders_filled() {
        let made = plan(&kind("Prayer list"), &at(), Some("Family"), |_| false);
        assert_eq!(made.path, "notes/Family.md");
        let text = made.text.unwrap();
        let parsed = frontmatter::parse(&text);
        assert_eq!(parsed.front.tags, ["prayer"]);
        assert_eq!(parsed.front.title, None, "the file name and the heading are the title: no property says it again");
        assert!(parsed.front.created.is_some());
        assert!(parsed.front.props.is_empty(), "nothing of `template_…` is copied");
        assert!(text.contains("\n# Family\n\n## Praise"), "{text}");
        let daily = plan(&kind("Daily"), &at(), Some("ignored"), |_| false);
        assert_eq!(daily.path, "notes/2026-10-13.md", "it asks for no title, so a given one is not used");
        assert!(daily.text.unwrap().contains("\n# Tuesday, October 13, 2026\n"));
        let blank = plan(&kind("Blank"), &at(), None, |_| false);
        assert_eq!(blank.path, "notes/Untitled.md");
        assert!(blank.text.unwrap().contains("\n# Untitled\n"));
        assert_eq!(plan(&kind("Topic"), &at(), None, |_| false).path, "notes/Untitled topic.md");
    }

    #[test]
    fn a_person_can_write_a_template_with_their_own_properties_and_logic() {
        let text = "---\ntemplate_description: A sermon.\ntemplate_ask_title: true\ntemplate_unique: false\ntype: sermon\ntags: [sermon, \"{{date:YYYY}}\"]\npreacher: Me\nseries: \"{{title}} series\"\n---\n# {{title}}\n\nPreached {{date:MMM D}} at {{time:HH:mm}}.\n";
        let template = Template::parse("templates/Sermon.md", text);
        assert_eq!((template.name.as_str(), template.description.as_str()), ("Sermon", "A sermon."));
        let made = plan(&template, &at(), Some("Grace"), |_| false);
        let out = made.text.unwrap();
        let front = frontmatter::parse(&out).front;
        assert_eq!(front.kind.as_deref(), Some("sermon"));
        assert_eq!(front.tags, ["sermon", "2026"]);
        let value = |k: &str| front.props.iter().find(|p| p.key == k).map(|p| p.value.clone());
        assert_eq!(value("preacher"), Some(Value::Text("Me".into())));
        assert_eq!(value("series"), Some(Value::Text("Grace series".into())));
        assert!(out.contains("Preached Oct 13 at 09:30."), "{out}");
    }

    #[test]
    fn placeholders_use_moments_letters_and_unknown_ones_are_left_alone() {
        let now = at();
        assert_eq!(expand("{{date}} {{time}}", "T", &now), "2026-10-13 09:30");
        assert_eq!(expand("{{date:dddd, MMMM D, YYYY}}", "T", &now), "Tuesday, October 13, 2026");
        assert_eq!(expand("{{date:YY/M/D hh:mm A}}", "T", &now), "26/10/13 09:30 AM");
        assert_eq!(expand("{{ title }} and {{nothing}} and {{unclosed", "My title", &now), "My title and {{nothing}} and {{unclosed");
        assert_eq!(expand("100% {{date:D}}", "T", &now), "100% 13");
    }

    #[test]
    fn titles_become_file_names_and_names_are_made_distinct() {
        let none = |_: &str| false;
        let blank = kind("Blank");
        let odd = plan(&blank, &at(), Some("Is it 1/2: or *not*?"), none);
        assert_eq!(odd.path, "notes/Is it 1 2 or not.md");
        assert!(odd.text.unwrap().contains("\n# Is it 1/2: or *not*?\n"), "the heading keeps what was typed");
        let taken = |p: &str| p == "notes/Grace.md";
        let second = plan(&kind("Topic"), &at(), Some("Grace"), taken);
        assert_eq!(second.path, "notes/Grace 2.md");
        assert!(second.text.unwrap().contains("\n# Grace\n"), "a number in the name is not in the heading");
        assert_eq!(plan(&blank, &at(), Some("///"), none).path, "notes/Untitled.md");
        assert_eq!(file_name("..."), "");
    }

    #[test]
    fn a_unique_kind_opens_the_note_there_is() {
        let taken = |p: &str| p == "notes/2026-10-13.md";
        let again = plan(&kind("Daily"), &at(), None, taken);
        assert_eq!((again.path.as_str(), again.text), ("notes/2026-10-13.md", None));
        // A kind that is not unique makes another.
        let busy = |p: &str| p == "notes/Untitled.md";
        assert_eq!(plan(&kind("Blank"), &at(), None, busy).path, "notes/Untitled 2.md");
    }

    #[test]
    fn the_templates_in_a_vault_are_the_files_in_its_templates_folder_by_name() {
        let mut notes = std::collections::BTreeMap::new();
        for (p, t) in builtin() {
            notes.insert(p, t);
        }
        notes.insert("notes/Other.md".to_string(), "x".to_string());
        notes.insert("templates/Sermon.md".to_string(), "---\ntemplate_description: A sermon.\n---\n# {{title}}\n".to_string());
        let found = all(notes.iter());
        let names: Vec<&str> = found.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Blank", "Daily", "Prayer list", "Sermon", "Topic"], "adding a file adds a kind");
    }
}
