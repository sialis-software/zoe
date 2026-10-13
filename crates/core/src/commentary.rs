//! A commentary's notes as paragraphs of styled runs, from the markup modules keep them in
//! (ThML or OSIS): only what a reader needs, which is where paragraphs break, bold and italic,
//! and the passages the text points to.

/// A stretch of text in one style.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// The passage this text points to, as the module wrote it (`Joh 2:23,24`, `John.3.16`).
    pub passage: Option<String>,
}

pub type Paragraph = Vec<Span>;

/// What a `<hi>` made of its text.
#[derive(Clone, Copy)]
enum Hi {
    Bold,
    Italic,
    Other,
}

/// A note's paragraphs. A leading `3:16 ` (some modules begin each note with where it is) is left
/// off, and so are empty paragraphs.
pub fn paragraphs(raw: &str) -> Vec<Paragraph> {
    let raw = strip_place(raw);
    let mut out: Vec<Paragraph> = Vec::new();
    let mut current: Paragraph = Vec::new();
    let (mut bold, mut italic) = (0usize, 0usize);
    let mut passage: Vec<Option<String>> = Vec::new();
    let mut hi: Vec<Hi> = Vec::new();
    let mut rest = raw;
    let push_text = |current: &mut Paragraph, text: &str, bold: usize, italic: usize, passage: &[Option<String>]| {
        let text = decode(text);
        if text.is_empty() {
            return;
        }
        let passage = passage.iter().rev().find_map(|p| p.clone());
        match current.last_mut() {
            Some(last) if last.bold == (bold > 0) && last.italic == (italic > 0) && last.passage == passage => last.text.push_str(&text),
            _ => current.push(Span { text, bold: bold > 0, italic: italic > 0, passage }),
        }
    };
    let end_paragraph = |out: &mut Vec<Paragraph>, current: &mut Paragraph| {
        // Whitespace at the ends of a paragraph is not part of it.
        if let Some(first) = current.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = current.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        current.retain(|s| !s.text.is_empty());
        if !current.is_empty() {
            out.push(std::mem::take(current));
        }
    };
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            push_text(&mut current, rest, bold, italic, &passage);
            break;
        };
        push_text(&mut current, &rest[..open], bold, italic, &passage);
        let Some(close) = rest[open..].find('>').map(|c| open + c) else { break };
        let tag = &rest[open + 1..close];
        rest = &rest[close + 1..];
        let closing = tag.starts_with('/');
        let self_closing = tag.ends_with('/');
        let body = tag.trim_start_matches('/').trim_end_matches('/').trim();
        let name = body.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        match name.as_str() {
            // Where a paragraph ends (and the next begins), whether the tag opens or closes.
            "p" | "br" | "lb" | "div" | "list" | "item" | "h1" | "h2" | "h3" | "h4" => end_paragraph(&mut out, &mut current),
            // A title is a paragraph of its own, in bold.
            "title" => {
                end_paragraph(&mut out, &mut current);
                if closing {
                    bold = bold.saturating_sub(1);
                } else if !self_closing {
                    bold += 1;
                }
            }
            "b" | "strong" => {
                if closing { bold = bold.saturating_sub(1) } else if !self_closing { bold += 1 }
            }
            "i" | "em" => {
                if closing { italic = italic.saturating_sub(1) } else if !self_closing { italic += 1 }
            }
            "hi" if !self_closing => {
                // What opened is what closes: `hi` is told apart by its type.
                if closing {
                    match hi.pop() {
                        Some(Hi::Bold) => bold = bold.saturating_sub(1),
                        Some(Hi::Italic) => italic = italic.saturating_sub(1),
                        _ => {}
                    }
                } else {
                    let kind = match attribute(body, "type").or_else(|| attribute(body, "rend")).unwrap_or_default().as_str() {
                        "bold" => Hi::Bold,
                        "italic" => Hi::Italic,
                        _ => Hi::Other,
                    };
                    match kind {
                        Hi::Bold => bold += 1,
                        Hi::Italic => italic += 1,
                        Hi::Other => {}
                    }
                    hi.push(kind);
                }
            }
            "scripref" | "reference" => {
                if closing {
                    passage.pop();
                } else if !self_closing {
                    passage.push(attribute(body, "passage").or_else(|| attribute(body, "osisref")));
                }
            }
            _ => {}
        }
    }
    end_paragraph(&mut out, &mut current);
    out
}

/// The value of attribute `name` in the inside of a tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))? + name.len() + 1;
    let rest = tag.get(at..)?;
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &rest[1..];
    value.find(quote).map(|end| decode(&value[..end]))
}

/// Without a leading `3:16 ` and the space after it.
fn strip_place(raw: &str) -> &str {
    let raw = raw.trim_start();
    let digits = raw.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || !raw[digits..].starts_with(':') {
        return raw;
    }
    let verse = raw[digits + 1..].chars().take_while(char::is_ascii_digit).count();
    let end = digits + 1 + verse;
    if verse > 0 && raw[end..].starts_with(' ') { &raw[end + 1..] } else { raw }
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        match tail.find(';').filter(|&e| e <= 8) {
            Some(end) => {
                let entity = &tail[1..end];
                let decoded = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => entity
                        .strip_prefix('#')
                        .and_then(|n| match n.strip_prefix(['x', 'X']) {
                            Some(hex) => u32::from_str_radix(hex, 16).ok(),
                            None => n.parse().ok(),
                        })
                        .and_then(char::from_u32),
                };
                match decoded {
                    Some(c) => {
                        out.push(c);
                        rest = &tail[end + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &tail[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(p: &Paragraph) -> String {
        p.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn thml_notes_become_paragraphs_with_bold_and_passages() {
        let raw = "3:16 Ver. 16. <p /><b>For God so loved the world, &c.]</b><p /> The Persic reads &quot;men&quot;: see <scripRef passage=\"Joh 2:23,24\">Joh 2:23,24</scripRef>, and <i>more</i>.";
        let ps = paragraphs(raw);
        assert_eq!(ps.len(), 3);
        assert_eq!(text(&ps[0]), "Ver. 16.");
        assert!(ps[1][0].bold && text(&ps[1]) == "For God so loved the world, &c.]");
        assert_eq!(text(&ps[2]), "The Persic reads \"men\": see Joh 2:23,24, and more.");
        let link = ps[2].iter().find(|s| s.passage.is_some()).unwrap();
        assert_eq!((link.text.as_str(), link.passage.as_deref()), ("Joh 2:23,24", Some("Joh 2:23,24")));
        assert!(ps[2].iter().any(|s| s.italic && s.text == "more"));
    }

    #[test]
    fn osis_notes_have_titles_and_paragraph_divisions() {
        let raw = "<div annotateRef=\"John.3.14-John.3.17\" sID=\"a\" type=\"section\"/><div sID=\"b\" type=\"x-p\"/><title>God So Loved the World</title><div eID=\"b\" type=\"x-p\"/> <lb/><div sID=\"c\" type=\"x-p\"/>Once the Lord has spoken, <reference osisRef=\"John.3.16\">John 3:16</reference> follows.<div eID=\"c\" type=\"x-p\"/>";
        let ps = paragraphs(raw);
        assert_eq!(ps.len(), 2, "{ps:?}");
        assert!(ps[0][0].bold && text(&ps[0]) == "God So Loved the World");
        assert_eq!(text(&ps[1]), "Once the Lord has spoken, John 3:16 follows.");
        assert_eq!(ps[1].iter().find(|s| s.passage.is_some()).unwrap().passage.as_deref(), Some("John.3.16"));
    }

    #[test]
    fn a_title_or_a_highlight_does_not_leave_the_rest_in_bold() {
        let raw = "<title>Born Again</title> <div sID=\"p\" type=\"x-p\"/>He said <hi type=\"italic\">truly</hi> and <hi type=\"super\">2</hi> plain, then <hi type=\"bold\">strong</hi> words. <title type=\"sub\">Next</title> after.<list><item><label>I.</label> one</item><item>two</item></list>";
        let ps = paragraphs(raw);
        let bold: Vec<String> = ps.iter().flatten().filter(|s| s.bold).map(|s| s.text.clone()).collect();
        assert_eq!(bold, ["Born Again", "strong", "Next"], "{ps:?}");
        let after = ps.iter().find(|p| text(p).starts_with("after")).expect("the text after a title is a paragraph");
        assert!(after.iter().all(|s| !s.bold));
        assert!(ps.iter().any(|p| text(p) == "He said truly and 2 plain, then strong words."));
        assert!(ps.iter().any(|p| text(p) == "I. one") && ps.iter().any(|p| text(p) == "two"), "list items are paragraphs");
    }

    #[test]
    fn the_place_a_note_starts_with_is_left_off_and_nothing_is_nothing() {
        assert_eq!(text(&paragraphs("1:1 Ver. 1.")[0]), "Ver. 1.");
        assert_eq!(text(&paragraphs("12:7-9 not a place")[0]), "12:7-9 not a place");
        assert!(paragraphs("").is_empty() && paragraphs("<p /> <p />").is_empty());
        assert_eq!(text(&paragraphs("a &amp; b &#8217;s &unknown; c")[0]), "a & b \u{2019}s &unknown; c");
    }
}
