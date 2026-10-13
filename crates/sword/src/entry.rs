//! A dictionary entry's markup (TEI, ThML or plain text) to the parts we show.

use crate::osis::{Token, tokenize};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The word in its own script, when the entry has it.
    pub lemma: String,
    pub transliteration: String,
    pub definition: String,
}

/// Text of the first element called `name` for which `want` accepts the attributes' `type`.
fn first_text(tokens: &[Token], name: &str, want: impl Fn(Option<&str>) -> bool) -> Option<String> {
    let mut depth = 0;
    let mut out = String::new();
    let mut inside = false;
    for t in tokens {
        match t {
            Token::Tag(tag) if tag.name == name && !tag.closing && !tag.self_closing && !inside => {
                if want(tag.attr("type")) {
                    inside = true;
                    depth = 1;
                }
            }
            Token::Tag(tag) if inside && tag.name == name && !tag.self_closing => {
                if tag.closing {
                    depth -= 1;
                    if depth == 0 {
                        return Some(squash(&out));
                    }
                } else {
                    depth += 1;
                }
            }
            Token::Text(s) if inside => out.push_str(s),
            _ => {}
        }
    }
    None
}

fn all_text(tokens: &[Token]) -> String {
    let mut out = String::new();
    for t in tokens {
        match t {
            Token::Text(s) => out.push_str(s),
            Token::Tag(tag) if matches!(tag.name.as_str(), "p" | "lb" | "br") => out.push(' '),
            _ => {}
        }
    }
    out
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Drop "see GREEK for 5689": a cross-reference line the e-text adds for programs.
fn without_see_lines(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = ["see GREEK for", "see HEBREW for"].iter().filter_map(|m| rest.find(m)).min() {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let skip = after.find("for").map_or(0, |f| f + 3);
        let digits = after[skip..].trim_start();
        let used = skip + (after[skip..].len() - digits.len());
        let n = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
        rest = &after[used + n..];
    }
    out.push_str(rest);
    out
}

pub fn parse_entry(markup: &str) -> Entry {
    let tokens = tokenize(markup);
    let has = |name: &str| tokens.iter().any(|t| matches!(t, Token::Tag(tag) if tag.name == name && !tag.closing));
    if has("orth") {
        // TEI: <orth>lemma</orth> <orth type="trans">…</orth> <def>…</def>
        let lemma = first_text(&tokens, "orth", |ty| ty.is_none()).unwrap_or_default();
        let transliteration = first_text(&tokens, "orth", |ty| ty == Some("trans")).unwrap_or_default();
        let definition = first_text(&tokens, "def", |_| true).unwrap_or_else(|| squash(&all_text(&tokens)));
        return Entry { lemma, transliteration, definition: squash(&without_see_lines(&definition)) };
    }
    if has("grk") || has("heb") {
        // ThML: <p><grk>lemma</grk></p><p><b>translit</b>; etymology; <i>gloss</i> — usage</p>
        let lemma = first_text(&tokens, "grk", |_| true).or_else(|| first_text(&tokens, "heb", |_| true)).unwrap_or_default();
        let transliteration = first_text(&tokens, "b", |_| true).unwrap_or_default();
        let full = squash(&all_text(&tokens));
        let body = full.strip_prefix(&lemma).unwrap_or(&full).trim_start();
        let body = body.strip_prefix(&transliteration).unwrap_or(body);
        let definition = body.trim_start_matches([' ', ';']).to_string();
        return Entry { lemma, transliteration, definition };
    }
    // Plain: " 430  'elohiym  el-o-heem'" then the definition.
    let text = all_text(&tokens);
    let mut lines = text.lines().skip_while(|l| l.trim().is_empty());
    let head = lines.next().unwrap_or_default();
    let mut words = head.split_whitespace();
    let numbered = words.next().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()));
    if numbered {
        let transliteration = words.next().unwrap_or_default().to_string();
        let rest: Vec<&str> = lines.collect();
        let definition = squash(&without_see_lines(&rest.join(" ")));
        return Entry { lemma: String::new(), transliteration, definition };
    }
    Entry { lemma: String::new(), transliteration: String::new(), definition: squash(&text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tei_entries() {
        let e = parse_entry(
            "<entryFree n=\"25\"> <orth>ἀγαπάω</orth><lb/> <orth type=\"writing\">ajgapavw</orth> <orth rend=\"bold\" type=\"trans\">agapao</orth> <pron>{ag-ap-ah'-o}</pron> <def>\n perhaps from agan (much); to love (in a social or moral sense):--(be-)love(-ed). Compare 5368. <lb/> see GREEK for 5689 <lb/> see GREEK for 5368 </def>\n </entryFree>",
        );
        assert_eq!(e.lemma, "ἀγαπάω");
        assert_eq!(e.transliteration, "agapao");
        assert_eq!(e.definition, "perhaps from agan (much); to love (in a social or moral sense):--(be-)love(-ed). Compare 5368.");
    }

    #[test]
    fn thml_entries() {
        let e = parse_entry("<p><grk>ἀγαπάω</grk></p><p><b>agapaō</b>; of unc. or.; <i>to love, show love:</i> — beloved(8), love(76).</p>");
        assert_eq!(e.lemma, "ἀγαπάω");
        assert_eq!(e.transliteration, "agapaō");
        assert_eq!(e.definition, "of unc. or.; to love, show love: — beloved(8), love(76).");
        let h = parse_entry("<p><heb>אֱלֹהִים</heb></p><p><b>elohim</b> (43b); pl. of 433; <i>God, god:</i> — angels(1).</p>");
        assert_eq!((h.lemma.as_str(), h.transliteration.as_str()), ("אֱלֹהִים", "elohim"));
        assert!(h.definition.starts_with("(43b); pl. of 433"));
    }

    #[test]
    fn plain_entries() {
        let e = parse_entry(" 430  'elohiym  el-o-heem'\r\n\r\n\r\n plural of 433; gods in the ordinary sense:--God.\r\n see HEBREW for 0433\r\n");
        assert_eq!(e.transliteration, "'elohiym");
        assert_eq!(e.definition, "plural of 433; gods in the ordinary sense:--God.");
        assert_eq!(parse_entry("just some words").definition, "just some words");
    }
}
