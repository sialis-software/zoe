//! The words typed in a Bible search, and what they mean: the terms to find, to leave out, the
//! Strong's numbers to have, and where to look. Nothing here reads a module: it makes the
//! expression the full-text index is asked, and finds the words a query matched in a verse (to
//! show them).
//!
//! The language: words (all must be there), `"a phrase"`, `a OR b`, `-leftout`, `pre*`, a Strong's
//! number (`G26`, `H430`) and `in:John`, `in:NT`, `in:Gen-Deut` for where.

use scripture::Book;

/// A thing to find in a verse's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    Word(String),
    /// Words that begin with this.
    Prefix(String),
    /// These words one after another.
    Phrase(Vec<String>),
}

/// What must be in a verse: one term, or any of several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unit {
    One(Term),
    Any(Vec<Term>),
}

/// Where to look: books `first` to `last`, in canon order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope {
    pub first: Book,
    pub last: Book,
}

impl Scope {
    pub const OLD: &'static str = "OT";
    pub const NEW: &'static str = "NT";

    pub fn old() -> Scope {
        Scope { first: Book::GENESIS, last: Book::from_osis("Mal").expect("Malachi") }
    }

    pub fn new_testament() -> Scope {
        Scope { first: Book::from_osis("Matt").expect("Matthew"), last: Book::from_osis("Rev").expect("Revelation") }
    }

    /// How the scope is written in a query (`in:` is not part of it).
    pub fn word(&self) -> String {
        if *self == Scope::old() {
            Scope::OLD.into()
        } else if *self == Scope::new_testament() {
            Scope::NEW.into()
        } else if self.first == self.last {
            self.first.osis().into()
        } else {
            format!("{}-{}", self.first.osis(), self.last.osis())
        }
    }

    /// Whether `book` is inside.
    pub fn contains(&self, book: Book) -> bool {
        let at = |b: Book| Book::all().position(|x| x == b).unwrap_or(0);
        (at(self.first)..=at(self.last)).contains(&at(book))
    }
}

/// A query, taken apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub units: Vec<Unit>,
    pub left_out: Vec<Term>,
    /// Strong's numbers, as `G26`: every one must be in the verse.
    pub strongs: Vec<String>,
    pub scope: Option<Scope>,
    /// Parts that could not be understood, to tell the person.
    pub problems: Vec<String>,
}

/// The words of `text` as they are indexed: runs of letters and digits, lowercased, with where each
/// starts and ends (byte offsets). An apostrophe ends a word, as it does in the index.
pub fn words(text: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((text[s..i].to_lowercase(), s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((text[s..].to_lowercase(), s, text.len()));
    }
    out
}

fn raw_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(text[s..i].to_string());
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(text[s..].to_string());
    }
    out
}

/// A Strong's number written as people do (`g26`, `H0430`), as `G26`.
pub fn strongs_number(word: &str) -> Option<String> {
    let mut chars = word.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let digits: String = chars.collect();
    if !matches!(letter, 'G' | 'H') || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().map(|n| format!("{letter}{n}"))
}

fn scope_of(word: &str) -> Result<Scope, String> {
    if word.eq_ignore_ascii_case(Scope::OLD) {
        return Ok(Scope::old());
    }
    if word.eq_ignore_ascii_case(Scope::NEW) {
        return Ok(Scope::new_testament());
    }
    let book = |name: &str| Book::parse(name).ok_or_else(|| format!("\u{201c}{name}\u{201d} is not a book"));
    match word.split_once('-') {
        Some((a, z)) => {
            let (first, last) = (book(a)?, book(z)?);
            let at = |b: Book| Book::all().position(|x| x == b).unwrap_or(0);
            if at(first) <= at(last) { Ok(Scope { first, last }) } else { Err(format!("{word}: the books are the wrong way round")) }
        }
        None => book(word).map(|b| Scope { first: b, last: b }),
    }
}

impl Query {
    pub fn parse(text: &str) -> Query {
        let mut q = Query::default();
        let mut rest = text.trim();
        // Pieces in order: a quoted phrase, or a run up to a space. `OR` joins its neighbours.
        let mut pieces: Vec<(bool, bool, String)> = Vec::new();
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('"') {
                let (inside, tail) = after.split_once('"').unwrap_or((after, ""));
                pieces.push((true, false, inside.to_string()));
                rest = tail.trim_start();
            } else if let Some(after) = rest.strip_prefix("-\"") {
                let (inside, tail) = after.split_once('"').unwrap_or((after, ""));
                pieces.push((true, true, inside.to_string()));
                rest = tail.trim_start();
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                pieces.push((false, false, rest[..end].to_string()));
                rest = rest[end..].trim_start();
            }
        }
        let mut or_next = false;
        for (quoted, quoted_negated, piece) in pieces {
            if !quoted && piece == "OR" {
                or_next = true;
                continue;
            }
            let (negated, body) = match piece.strip_prefix('-') {
                Some(b) if !quoted && !b.is_empty() => (true, b.to_string()),
                _ => (quoted_negated, piece.clone()),
            };
            if !quoted && !negated {
                if let Some(value) = body.strip_prefix("in:").or_else(|| body.strip_prefix("IN:")) {
                    match scope_of(value) {
                        Ok(scope) => q.scope = Some(scope),
                        Err(problem) => q.problems.push(problem),
                    }
                    continue;
                }
                if let Some(number) = strongs_number(&body) {
                    if !q.strongs.contains(&number) {
                        q.strongs.push(number);
                    }
                    continue;
                }
            }
            let prefix = !quoted && body.ends_with('*');
            let tokens = raw_words(&body);
            let term = match (tokens.len(), prefix) {
                (0, _) => continue,
                (1, true) => Term::Prefix(tokens[0].clone()),
                (1, false) => Term::Word(tokens[0].clone()),
                _ => Term::Phrase(tokens),
            };
            if negated {
                q.left_out.push(term);
                or_next = false;
                continue;
            }
            match (or_next, q.units.last_mut()) {
                (true, Some(Unit::One(prev))) => {
                    let prev = prev.clone();
                    *q.units.last_mut().expect("there is a last unit") = Unit::Any(vec![prev, term]);
                }
                (true, Some(Unit::Any(any))) => any.push(term),
                _ => q.units.push(Unit::One(term)),
            }
            or_next = false;
        }
        q
    }

    /// Whether there is anything to look for.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty() && self.strongs.is_empty()
    }

    /// The expression for the full-text index, if the query has words (not only Strong's numbers).
    pub fn fts(&self) -> Option<String> {
        let term = |t: &Term| match t {
            Term::Word(w) => format!("\"{}\"", w.to_lowercase()),
            Term::Prefix(w) => format!("\"{}\"*", w.to_lowercase()),
            Term::Phrase(ws) => format!("\"{}\"", ws.join(" ").to_lowercase()),
        };
        let units: Vec<String> = self
            .units
            .iter()
            .map(|u| match u {
                Unit::One(t) => term(t),
                Unit::Any(ts) => format!("({})", ts.iter().map(term).collect::<Vec<_>>().join(" OR ")),
            })
            .collect();
        if units.is_empty() {
            return None;
        }
        let mut out = units.join(" AND ");
        if !self.left_out.is_empty() {
            out = format!("({out}) NOT ({})", self.left_out.iter().map(term).collect::<Vec<_>>().join(" OR "));
        }
        Some(out)
    }

    /// Where in `text` the query's words are: byte spans, in order. `case` takes only words
    /// written as typed; `stem` takes other forms of a word (love, loved, loving).
    pub fn spans(&self, text: &str, case: bool, stem: bool) -> Vec<(usize, usize)> {
        let ws = words(text);
        let mut out = Vec::new();
        for unit in &self.units {
            match unit {
                Unit::One(t) => out.extend(find(t, text, &ws, case, stem)),
                Unit::Any(ts) => ts.iter().for_each(|t| out.extend(find(t, text, &ws, case, stem))),
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Whether `text` fits the query as written: every unit found and nothing left out, with
    /// `case` and `stem` as in [`Query::spans`]. (The index finds candidates; this is the final say
    /// for what it cannot tell, such as capitals.)
    pub fn fits(&self, text: &str, case: bool, stem: bool) -> bool {
        let ws = words(text);
        let found = |t: &Term| !find(t, text, &ws, case, stem).is_empty();
        self.units.iter().all(|u| match u {
            Unit::One(t) => found(t),
            Unit::Any(ts) => ts.iter().any(found),
        }) && !self.left_out.iter().any(found)
    }
}

/// A word as the index files it with `stem`: a short, rough stem (the index's own is Porter's).
fn stem_of(word: &str) -> String {
    let w = word.to_lowercase();
    // A last `e` is dropped from what is left, so that love and loved (lov) meet.
    let trim = |s: &str| s.strip_suffix('e').filter(|t| t.chars().count() >= 3).unwrap_or(s).to_string();
    for suffix in ["ingly", "edly", "ing", "ied", "ies", "ed", "es", "ly", "s", "d"] {
        if let Some(stem) = w.strip_suffix(suffix)
            && stem.chars().count() >= 3
        {
            return trim(stem);
        }
    }
    trim(&w)
}

fn find(term: &Term, text: &str, ws: &[(String, usize, usize)], case: bool, stem: bool) -> Vec<(usize, usize)> {
    let same = |typed: &str, at: &(String, usize, usize)| -> bool {
        if case {
            let shown = &text[at.1..at.2];
            return if stem { stem_of(shown) == stem_of(typed) && shown.chars().next() == typed.chars().next() } else { shown == typed };
        }
        if stem { stem_of(&at.0) == stem_of(typed) } else { at.0 == typed.to_lowercase() }
    };
    match term {
        Term::Word(w) => ws.iter().filter(|at| same(w, at)).map(|at| (at.1, at.2)).collect(),
        Term::Prefix(p) => ws
            .iter()
            .filter(|at| if case { text[at.1..at.2].starts_with(p.as_str()) } else { at.0.starts_with(&p.to_lowercase()) })
            .map(|at| (at.1, at.2))
            .collect(),
        Term::Phrase(parts) => {
            let mut out = Vec::new();
            for i in 0..ws.len().saturating_sub(parts.len() - 1) {
                if parts.iter().enumerate().all(|(k, p)| same(p, &ws[i + k])) {
                    out.push((ws[i].1, ws[i + parts.len() - 1].2));
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(w: &str) -> Term {
        Term::Word(w.into())
    }

    #[test]
    fn plain_words_phrases_alternatives_exclusions_and_prefixes() {
        let q = Query::parse("God loved \"only begotten\" grace OR mercy -wrath faith*");
        assert_eq!(
            q.units,
            vec![
                Unit::One(word("God")),
                Unit::One(word("loved")),
                Unit::One(Term::Phrase(vec!["only".into(), "begotten".into()])),
                Unit::Any(vec![word("grace"), word("mercy")]),
                Unit::One(Term::Prefix("faith".into())),
            ]
        );
        assert_eq!(q.left_out, vec![word("wrath")]);
        assert_eq!(
            q.fts().unwrap(),
            "(\"god\" AND \"loved\" AND \"only begotten\" AND (\"grace\" OR \"mercy\") AND \"faith\"*) NOT (\"wrath\")"
        );
    }

    #[test]
    fn alternatives_chain_and_a_word_with_an_apostrophe_is_a_phrase_as_the_index_has_it() {
        let q = Query::parse("a OR b OR c God's");
        assert_eq!(q.units[0], Unit::Any(vec![word("a"), word("b"), word("c")]));
        assert_eq!(q.units[1], Unit::One(Term::Phrase(vec!["God".into(), "s".into()])));
        assert_eq!(Query::parse("\"-quoted dash\"").left_out, vec![]);
        assert_eq!(Query::parse("-\"two words\" x").left_out, vec![Term::Phrase(vec!["two".into(), "words".into()])]);
    }

    #[test]
    fn strongs_numbers_and_scopes_are_not_words() {
        let q = Query::parse("love g0026 H430 in:John");
        assert_eq!(q.units, vec![Unit::One(word("love"))]);
        assert_eq!(q.strongs, ["G26", "H430"]);
        assert_eq!(q.scope, Some(Scope { first: Book::JOHN, last: Book::JOHN }));
        assert_eq!(Query::parse("in:NT").scope, Some(Scope::new_testament()));
        assert_eq!(Query::parse("in:ot x").scope, Some(Scope::old()));
        let range = Query::parse("x in:Gen-Deut").scope.unwrap();
        assert_eq!((range.first.osis(), range.last.osis()), ("Gen", "Deut"));
        assert!(range.contains(Book::from_osis("Exod").unwrap()) && !range.contains(Book::JOHN));
        let bad = Query::parse("x in:Narnia in:Deut-Gen");
        assert_eq!(bad.problems.len(), 2);
        assert!(Query::parse("G26").fts().is_none() && !Query::parse("G26").is_empty());
        assert!(Query::parse("  ").is_empty() && Query::parse("-x").fts().is_none());
    }

    #[test]
    fn the_words_found_in_a_verse_are_marked_where_they_are() {
        let text = "For God so loved the world, that he gave his only begotten Son";
        let q = Query::parse("loved \"only begotten\" gav*");
        let found: Vec<&str> = q.spans(text, false, false).iter().map(|&(a, z)| &text[a..z]).collect();
        assert_eq!(found, ["loved", "gave", "only begotten"]);
        assert!(q.fits(text, false, false));
        // Capitals, other forms of a word, and what is left out.
        assert!(Query::parse("god").fits(text, false, false) && !Query::parse("god").fits(text, true, false));
        assert!(Query::parse("God").fits(text, true, false));
        assert!(Query::parse("love").spans(text, false, false).is_empty());
        assert_eq!(Query::parse("love").spans(text, false, true), vec![(11, 16)]);
        assert!(!Query::parse("world -loved").fits(text, false, false));
        assert!(Query::parse("world OR zzz").fits(text, false, false) && !Query::parse("zzz OR qqq").fits(text, false, false));
    }
}
