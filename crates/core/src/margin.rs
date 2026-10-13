//! Margin marks (highlights, underlines, margin notes): data model, anchoring against the
//! canonical verse text, and the markdown file format (docs/05).

use scripture::{Book, RefRange};
use std::collections::BTreeMap;

/// Canonical text per verse, as returned by `Chapter::canonical_verses`.
pub type CanonicalText = BTreeMap<u16, String>;

/// Characters of context kept either side of the quote.
const CONTEXT: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkStyle {
    Highlight,
    Underline,
    /// A note anchored to the text without a highlight (drawn only as a faint line).
    None,
}

impl MarkStyle {
    fn as_str(self) -> &'static str {
        match self {
            MarkStyle::Highlight => "highlight",
            MarkStyle::Underline => "underline",
            MarkStyle::None => "none",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "highlight" => Some(MarkStyle::Highlight),
            "underline" => Some(MarkStyle::Underline),
            "none" => Some(MarkStyle::None),
            _ => None,
        }
    }
}

/// A byte range within one verse's canonical text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    pub verse: u16,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    /// Short id; in a vault it is the file name suffix.
    pub id: String,
    /// Verses the mark covers, in the reference versification.
    pub anchor: RefRange,
    /// Module the words were chosen in. Empty for verse-level marks (no quote), which are
    /// tied to the verse reference and so show in every module.
    pub module: String,
    /// The marked words. Empty means the whole verse(s) of `anchor`.
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub style: MarkStyle,
    /// `#RRGGBB`
    pub color: String,
    pub tags: Vec<String>,
    /// Optional markdown comment (a margin note); empty for a bare highlight.
    pub body: String,
    /// When it was made (RFC 3339), or empty for a mark from before marks were stamped.
    pub created: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchoring {
    /// Quote found (disambiguated by context if it occurs more than once).
    Exact,
    /// Quote not found in the displayed text.
    Orphaned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub state: Anchoring,
    pub slices: Vec<Slice>,
}

impl Mark {
    /// Whether the mark covers whole verses (tied to the reference, visible in any module)
    /// rather than quoted words in one module.
    pub fn is_verse_level(&self) -> bool {
        self.quote.is_empty()
    }
}

/// The anchor fields derived from a selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub range: RefRange,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
}

/// Verses `a..=z` joined by single spaces, with each verse's start offset in the result.
fn joined(canon: &CanonicalText, a: u16, z: u16) -> (String, Vec<(u16, usize)>) {
    let mut text = String::new();
    let mut bases = Vec::new();
    for (&verse, t) in canon.range(a..=z) {
        if !text.is_empty() {
            text.push(' ');
        }
        bases.push((verse, text.len()));
        text.push_str(t);
    }
    (text, bases)
}

/// Turn a selection (slices of canonical verse text, in verse order) into an anchor.
pub fn anchor_from_selection(
    book: Book,
    chapter: u16,
    canon: &CanonicalText,
    slices: &[Slice],
) -> Option<Anchor> {
    let (first, last) = (slices.first()?, slices.last()?);
    let (text, bases) = joined(canon, first.verse, last.verse);
    let base = |v: u16| bases.iter().find(|(x, _)| *x == v).map(|&(_, b)| b);
    let (start, end) = (base(first.verse)? + first.start, base(last.verse)? + last.end);
    let quote = text.get(start..end).filter(|q| !q.trim().is_empty())?;
    let prefix: String = text[..start].chars().rev().take(CONTEXT).collect::<Vec<_>>().into_iter().rev().collect();
    let suffix: String = text[end..].chars().take(CONTEXT).collect();
    Some(Anchor {
        range: RefRange { book, chapter, verses: Some((first.verse, last.verse)) },
        quote: quote.to_string(),
        prefix,
        suffix,
    })
}

/// An anchor for whole verses `a..=z`: just the reference, no quote.
pub fn anchor_for_verses(book: Book, chapter: u16, a: u16, z: u16) -> Anchor {
    Anchor {
        range: RefRange { book, chapter, verses: Some((a, z)) },
        quote: String::new(),
        prefix: String::new(),
        suffix: String::new(),
    }
}

/// If `slices` cover whole consecutive verses (each from its first byte to its last, with no
/// existing verse skipped), return the first and last verse.
pub fn whole_verses(canon: &CanonicalText, slices: &[Slice]) -> Option<(u16, u16)> {
    let (first, last) = (slices.first()?, slices.last()?);
    let full = slices.iter().all(|s| s.start == 0 && canon.get(&s.verse).is_some_and(|t| s.end == t.len()));
    let consecutive = canon.range(first.verse..=last.verse).map(|(&v, _)| v).eq(slices.iter().map(|s| s.verse));
    (full && consecutive).then_some((first.verse, last.verse))
}

/// Find where `mark` sits in `canon`, as per-verse slices.
pub fn resolve(mark: &Mark, canon: &CanonicalText) -> Resolved {
    let orphaned = Resolved { state: Anchoring::Orphaned, slices: Vec::new() };
    let (a, z) = mark.anchor.verses.unwrap_or((1, u16::MAX));
    if mark.is_verse_level() {
        let slices: Vec<Slice> = canon
            .range(a..=z)
            .map(|(&verse, t)| Slice { verse, start: 0, end: t.len() })
            .collect();
        return if slices.is_empty() { orphaned } else { Resolved { state: Anchoring::Exact, slices } };
    }
    let (text, bases) = joined(canon, a, z);

    // Among occurrences of the quote, prefer the one whose surroundings match best.
    let score = |at: usize| {
        usize::from(text[..at].ends_with(&mark.prefix))
            + usize::from(text[at + mark.quote.len()..].starts_with(&mark.suffix))
    };
    let Some(at) = text
        .match_indices(&mark.quote)
        .map(|(i, _)| i)
        .max_by_key(|&i| (score(i), std::cmp::Reverse(i)))
    else {
        return orphaned;
    };
    let end = at + mark.quote.len();

    let slices = bases
        .iter()
        .filter_map(|&(verse, base)| {
            let len = canon[&verse].len();
            let (lo, hi) = (at.max(base), end.min(base + len));
            (lo < hi).then(|| Slice { verse, start: lo - base, end: hi - base })
        })
        .collect();
    Resolved { state: Anchoring::Exact, slices }
}

// ---------------------------------------------------------------------------------------
// File format
// ---------------------------------------------------------------------------------------

fn yaml_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn yaml_unquote(s: &str) -> String {
    let s = s.trim();
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

impl Mark {
    /// Vault-relative path: `margins/John/03/16-17-highlight-k3f9.md`.
    pub fn path(&self) -> String {
        let verses = match self.anchor.verses {
            Some((a, z)) if a == z => format!("{a:02}"),
            Some((a, z)) => format!("{a:02}-{z:02}"),
            None => "all".to_string(),
        };
        format!(
            "margins/{}/{:02}/{verses}-{}-{}.md",
            self.anchor.book.osis(),
            self.anchor.chapter,
            if self.body.is_empty() { self.style.as_str() } else { "note" },
            self.id
        )
    }

    /// What this mark becomes when it is unlinked from the text: an ordinary note in
    /// `notes/` that keeps the quote, the comment and a reference link, with no highlight.
    pub fn unlinked_note(&self) -> (String, String) {
        let verses = match self.anchor.verses {
            Some((a, z)) if a == z => format!("{a}"),
            Some((a, z)) => format!("{a}-{z}"),
            None => "all".to_string(),
        };
        let path = format!("notes/{} {}.{verses} {}.md", self.anchor.book.name(), self.anchor.chapter, self.id);
        // The note is new, whenever the mark was made.
        let mut text = format!(
            "---\ncreated: {}\nrefs: [{}]\n---\n[[{}]]\n",
            crate::frontmatter::now(),
            self.anchor.osis(),
            self.anchor
        );
        if !self.quote.is_empty() {
            text.push_str(&format!("\n> {}\n", self.quote));
        }
        if !self.body.trim().is_empty() {
            text.push_str(&format!("\n{}\n", self.body));
        }
        (path, text)
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::from("---\ntype: margin\n");
        if !self.created.is_empty() {
            out.push_str(&format!("created: {}\n", self.created));
        }
        out.push_str(&format!("anchor: {}\n", self.anchor.osis()));
        if !self.is_verse_level() {
            out.push_str(&format!("module: {}\n", self.module));
            out.push_str(&format!("quote: {}\n", yaml_quote(&self.quote)));
            out.push_str(&format!("prefix: {}\n", yaml_quote(&self.prefix)));
            out.push_str(&format!("suffix: {}\n", yaml_quote(&self.suffix)));
        }
        out.push_str(&format!("style: {}\n", self.style.as_str()));
        out.push_str(&format!("color: {}\n", yaml_quote(&self.color)));
        out.push_str(&format!("tags: [{}]\n---\n", self.tags.join(", ")));
        // The file's last line ends with a newline; the body itself does not carry it.
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        out
    }

    /// Parse a margin file. `id` comes from the file name.
    pub fn from_markdown(id: &str, text: &str) -> Option<Mark> {
        let rest = text.strip_prefix("---\n")?;
        let (front, body) = rest.split_once("\n---\n").or_else(|| {
            rest.strip_suffix("\n---").map(|f| (f, ""))
        })?;
        let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
        for line in front.lines() {
            let (k, v) = line.split_once(':')?;
            fields.insert(k.trim(), v.trim());
        }
        if fields.get("type") != Some(&"margin") {
            return None;
        }
        let get = |k: &str| fields.get(k).copied();
        let tags = get("tags")
            .and_then(|t| t.strip_prefix('[')?.strip_suffix(']'))
            .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        Some(Mark {
            id: id.to_string(),
            anchor: RefRange::from_osis(get("anchor")?)?,
            module: get("module").unwrap_or_default().to_string(),
            quote: get("quote").map(yaml_unquote).unwrap_or_default(),
            prefix: get("prefix").map(yaml_unquote).unwrap_or_default(),
            suffix: get("suffix").map(yaml_unquote).unwrap_or_default(),
            style: MarkStyle::parse(get("style")?)?,
            color: yaml_unquote(get("color")?),
            tags,
            body: body.strip_suffix('\n').unwrap_or(body).to_string(),
            created: get("created").map(yaml_unquote).unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(verses: &[(u16, &str)]) -> CanonicalText {
        verses.iter().map(|&(v, t)| (v, t.to_string())).collect()
    }

    fn sample_canon() -> CanonicalText {
        canon(&[
            (16, "For God so loved the world, that he gave his only begotten Son"),
            (17, "For God sent not his Son into the world to condemn the world"),
        ])
    }

    fn mark(range: &str, quote: &str, prefix: &str, suffix: &str) -> Mark {
        Mark {
            id: "k3f9".into(),
            anchor: RefRange::parse(range).unwrap(),
            module: "KJV".into(),
            quote: quote.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            style: MarkStyle::Highlight,
            color: "#F5D90A".into(),
            tags: vec!["promise".into()],
            body: String::new(),
            created: String::new(),
        }
    }

    #[test]
    fn a_marks_created_time_is_kept_in_its_file_and_read_back() {
        let mut m = mark("John 3:16", "so loved", "", "");
        m.created = "2026-10-12T23:41:05-07:00".into();
        let text = m.to_markdown();
        assert!(text.contains("\ncreated: 2026-10-12T23:41:05-07:00\n"), "{text}");
        assert_eq!(Mark::from_markdown("k3f9", &text), Some(m));
        // A mark from before they were stamped has none, and writes none.
        let old = mark("John 3:16", "so loved", "", "");
        assert!(!old.to_markdown().contains("created"));
        assert_eq!(Mark::from_markdown("k3f9", &old.to_markdown()).unwrap().created, "");
    }

    #[test]
    fn selection_within_one_verse_makes_an_anchor_with_context() {
        let c = sample_canon();
        let slices = [Slice { verse: 16, start: 8, end: 20 }]; // "so loved the"
        let a = anchor_from_selection(Book::JOHN, 3, &c, &slices).unwrap();
        assert_eq!(a.quote, "so loved the");
        assert_eq!(a.prefix, "For God ");
        assert_eq!(a.suffix, " world, that");
        assert_eq!(a.range, RefRange::parse("John 3:16").unwrap());
    }

    #[test]
    fn selection_across_verses_joins_with_a_space() {
        let c = canon(&[(1, "In the beginning"), (2, "And the earth")]);
        let slices = [Slice { verse: 1, start: 7, end: 16 }, Slice { verse: 2, start: 0, end: 7 }];
        let a = anchor_from_selection(Book::JOHN, 1, &c, &slices).unwrap();
        assert_eq!(a.quote, "beginning And the");
        assert_eq!(a.range.verses, Some((1, 2)));
        let m = Mark { anchor: a.range, quote: a.quote, prefix: a.prefix, suffix: a.suffix, ..mark("John 1:1", "", "", "") };
        let r = resolve(&m, &c);
        assert_eq!(r.state, Anchoring::Exact);
        assert_eq!(r.slices, slices);
    }

    #[test]
    fn empty_or_whitespace_selection_is_rejected() {
        let c = sample_canon();
        assert!(anchor_from_selection(Book::JOHN, 3, &c, &[]).is_none());
        let space = [Slice { verse: 16, start: 3, end: 4 }];
        assert!(anchor_from_selection(Book::JOHN, 3, &c, &space).is_none());
    }

    #[test]
    fn repeated_quote_is_disambiguated_by_context() {
        let c = sample_canon();
        // "the world" occurs in verse 16, and twice in verse 17.
        let m = mark("John 3:16-17", "the world", "into ", " to condemn");
        let r = resolve(&m, &c);
        assert_eq!(r.state, Anchoring::Exact);
        assert_eq!(r.slices.len(), 1);
        assert_eq!(r.slices[0].verse, 17);
        let t = &c[&17];
        assert_eq!(&t[r.slices[0].start..r.slices[0].end], "the world");
        assert_eq!(&t[..r.slices[0].start], "For God sent not his Son into ");
        // the other, later occurrence is picked only when its context matches
        let m2 = mark("John 3:16-17", "the world", "condemn ", "");
        let r2 = resolve(&m2, &c);
        assert_eq!(r2.slices.len(), 1);
        assert!(c[&17][..r2.slices[0].start].ends_with("condemn "));
    }

    #[test]
    fn changed_text_orphans_the_mark_instead_of_guessing() {
        let c = canon(&[(16, "For God so cherished the world")]);
        let r = resolve(&mark("John 3:16", "so loved the", "For God ", " world"), &c);
        assert_eq!(r.state, Anchoring::Orphaned);
        assert!(r.slices.is_empty());
    }

    #[test]
    fn file_round_trip_keeps_awkward_characters() {
        let mut m = mark("John 3:16-17", "He said, \"Go\" \\ now: here", "a: b", "line\nbreak");
        m.body = "A **note** with [[John 3:16]].".into();
        m.tags = vec!["promise".into(), "topic/grace".into()];
        let text = m.to_markdown();
        assert!(text.starts_with("---\ntype: margin\nanchor: John.3.16-John.3.17\n"), "{text}");
        assert_eq!(Mark::from_markdown("k3f9", &text), Some(m));
    }

    #[test]
    fn the_files_final_newline_is_not_part_of_the_body() {
        let mut m = mark("John 3:16", "so loved", "For God ", " the world");
        m.body = "one\ntwo".into();
        let text = m.to_markdown();
        assert!(text.ends_with("two\n"));
        assert_eq!(Mark::from_markdown("x", &text).unwrap().body, "one\ntwo");
        assert_eq!(Mark::from_markdown("x", &Mark::from_markdown("x", &text).unwrap().to_markdown()).unwrap().body, "one\ntwo");
    }

    #[test]
    fn bare_highlight_has_no_body_and_a_readable_path() {
        let m = mark("John 3:16", "so loved", "", "");
        let text = m.to_markdown();
        assert!(text.ends_with("---\n"));
        assert_eq!(Mark::from_markdown("k3f9", &text).unwrap().body, "");
        assert_eq!(m.path(), "margins/John/03/16-highlight-k3f9.md");
        let range = mark("John 3:16-17", "x", "", "");
        assert_eq!(range.path(), "margins/John/03/16-17-highlight-k3f9.md");
    }

    #[test]
    fn whole_verse_selection_is_detected_only_when_complete_and_consecutive() {
        let c = canon(&[(1, "In the beginning"), (2, "And the earth"), (3, "Let there be light")]);
        let full = |verse, len| Slice { verse, start: 0, end: len };
        assert_eq!(whole_verses(&c, &[full(1, 16), full(2, 13)]), Some((1, 2)));
        assert_eq!(whole_verses(&c, &[full(2, 13)]), Some((2, 2)));
        // partial verse, a skipped verse, and nothing at all are not whole-verse selections
        assert_eq!(whole_verses(&c, &[Slice { verse: 1, start: 0, end: 5 }]), None);
        assert_eq!(whole_verses(&c, &[full(1, 16), full(3, 18)]), None);
        assert_eq!(whole_verses(&c, &[]), None);
    }

    #[test]
    fn verse_level_marks_cover_whole_verses_in_any_module() {
        let a = anchor_for_verses(Book::JOHN, 3, 16, 17);
        let m = Mark {
            module: String::new(),
            quote: a.quote,
            prefix: a.prefix,
            suffix: a.suffix,
            anchor: a.range,
            ..mark("John 3:16", "", "", "")
        };
        assert!(m.is_verse_level());
        // Resolves against whatever module's text is shown.
        let other = canon(&[(16, "God loved the world so much"), (17, "God did not send his Son")]);
        let r = resolve(&m, &other);
        assert_eq!(r.state, Anchoring::Exact);
        assert_eq!(
            r.slices,
            [Slice { verse: 16, start: 0, end: 27 }, Slice { verse: 17, start: 0, end: 24 }]
        );
        // The file has no quote or module, and round-trips.
        let text = m.to_markdown();
        assert!(!text.contains("quote:") && !text.contains("module:"), "{text}");
        assert_eq!(Mark::from_markdown("k3f9", &text), Some(m));
        // A verse range the module lacks is orphaned rather than invented.
        assert_eq!(resolve(&mark("John 4:1", "", "", ""), &other).state, Anchoring::Orphaned);
    }

    #[test]
    fn unlinking_keeps_the_words_and_the_comment_but_drops_the_mark() {
        let mut m = mark("John 3:16", "so loved the world", "For God ", ", that");
        m.body = "My note.".into();
        let (path, text) = m.unlinked_note();
        assert_eq!(path, "notes/John 3.16 k3f9.md");
        // The note is new, so it is stamped with when it was made; the rest is the mark's.
        let created = crate::frontmatter::parse(&text).front.created.expect("a created time");
        assert_eq!(
            text,
            format!("---\ncreated: {created}\nrefs: [John.3.16]\n---\n[[John 3:16]]\n\n> so loved the world\n\nMy note.\n")
        );
        assert!(Mark::from_markdown("k3f9", &text).is_none(), "no longer a mark");

        // A verse-level mark has no quote to keep.
        let a = anchor_for_verses(Book::JOHN, 3, 16, 17);
        let v = Mark { anchor: a.range, quote: String::new(), module: String::new(), ..m };
        let (path, text) = v.unlinked_note();
        assert_eq!(path, "notes/John 3.16-17 k3f9.md");
        assert!(text.contains("[[John 3:16-17]]") && !text.contains("> "), "{text}");
    }

    #[test]
    fn rejects_other_files() {
        assert!(Mark::from_markdown("x", "# just a note\n").is_none());
        assert!(Mark::from_markdown("x", "---\ntype: prayer\n---\n").is_none());
    }
}
