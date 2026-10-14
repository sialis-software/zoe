//! Saying Greek and Hebrew words aloud with the voices the system already has: nothing is
//! downloaded and nothing leaves the machine. The system's voices say them as modern Greek and
//! modern Hebrew (there is no voice for Erasmian or Koine), which is what a person learning to
//! read the words will hear in the street, if not in the synagogue.
//!
//! - macOS: `say`, with a voice whose language is `el_*` (Melina) or `he_*` (Carmit) in the list
//!   `say -v ?` gives.
//! - Linux: `espeak-ng`, as Ancient Greek (`grc`, else `el`) or Hebrew (`he`), if it is installed.
//! - Elsewhere: nothing yet (the buttons that speak are not shown).

use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Greek,
    Hebrew,
}

/// The language the letters of `text` are in, going by which script there is most of.
pub fn language_of(text: &str) -> Option<Language> {
    let (mut greek, mut hebrew) = (0, 0);
    for c in text.chars() {
        match c as u32 {
            0x0370..=0x03FF | 0x1F00..=0x1FFF => greek += 1,
            0x0590..=0x05FF | 0xFB1D..=0xFB4F => hebrew += 1,
            _ => {}
        }
    }
    match (greek, hebrew) {
        (0, 0) => None,
        (g, h) if g >= h => Some(Language::Greek),
        _ => Some(Language::Hebrew),
    }
}

/// What is said for `text`: without the cantillation marks of the Hebrew text (they are not letters
/// or vowels, and voices do not know them), and with the words on one line.
pub fn spoken(text: &str) -> String {
    let kept: String = text.chars().filter(|c| !matches!(*c as u32, 0x0591..=0x05AF)).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A program and its arguments that say `text` in `voice`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Say {
    pub program: String,
    pub args: Vec<String>,
}

/// What to run to say `text` as `language` with the system's voice, if it has one.
pub fn command(language: Language, text: &str) -> Option<Say> {
    let text = spoken(text);
    if text.is_empty() {
        return None;
    }
    if cfg!(target_os = "macos") {
        let voice = mac_voice(language)?;
        // `--` so that a word never reads as an option.
        return Some(Say { program: "say".into(), args: vec!["-v".into(), voice, "--".into(), text] });
    }
    if cfg!(target_os = "linux") {
        let program = which("espeak-ng")?;
        let voice = match language {
            Language::Greek => "grc",
            Language::Hebrew => "he",
        };
        return Some(Say { program, args: vec!["-v".into(), voice.into(), "--".into(), text] });
    }
    None
}

/// Whether there is a voice for `language`.
pub fn available(language: Language) -> bool {
    command(language, "x").is_some()
}

/// Whether there is a voice for the words in `text`.
pub fn can_say(text: &str) -> bool {
    language_of(text).is_some_and(available)
}

fn which(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|p| p.is_file()).map(|p| p.display().to_string())
}

/// The names of the voices `say` knows, by language (`el_GR`, `he_IL`…), asked once.
fn mac_voices() -> &'static Vec<(String, String)> {
    static VOICES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    VOICES.get_or_init(|| {
        let Ok(output) = Command::new("say").args(["-v", "?"]).output() else { return Vec::new() };
        voices_from_listing(&String::from_utf8_lossy(&output.stdout))
    })
}

/// The voices in what `say -v ?` prints: `Melina              el_GR    # Χαίρετε! …`.
pub fn voices_from_listing(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter_map(|line| {
            let before = line.split('#').next()?.trim_end();
            let language = before.split_whitespace().last()?;
            let name = before[..before.len() - language.len()].trim();
            (!name.is_empty() && language.contains('_')).then(|| (name.to_string(), language.to_string()))
        })
        .collect()
}

fn mac_voice(language: Language) -> Option<String> {
    let prefix = match language {
        Language::Greek => "el_",
        Language::Hebrew => "he_",
    };
    mac_voices().iter().find(|(_, l)| l.starts_with(prefix)).map(|(n, _)| n.clone())
}

static CURRENT: Mutex<Option<Child>> = Mutex::new(None);

/// Stop what is being said.
pub fn stop() {
    if let Some(mut child) = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Say `text` now, in place of what was being said. Gives why not if it cannot.
pub fn speak(text: &str) -> Result<(), String> {
    let language = language_of(text).ok_or("There is no Greek or Hebrew to say")?;
    let say = command(language, text).ok_or_else(|| {
        format!("This system has no {} voice", if language == Language::Greek { "Greek" } else { "Hebrew" })
    })?;
    stop();
    let child = Command::new(&say.program)
        .args(&say.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not speak: {e}"))?;
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_of_a_word_is_the_script_it_is_mostly_in() {
        assert_eq!(language_of("ἀγάπη"), Some(Language::Greek));
        assert_eq!(language_of("ἀγαπάω ὁ λόγος"), Some(Language::Greek));
        assert_eq!(language_of("אֱלֹהִים"), Some(Language::Hebrew));
        assert_eq!(language_of("agape"), None);
        assert_eq!(language_of(""), None);
        assert_eq!(language_of("G25 ἀγαπάω"), Some(Language::Greek), "numbers and Latin letters do not count");
    }

    #[test]
    fn what_is_said_leaves_out_cantillation_marks_and_extra_spaces() {
        assert_eq!(spoken("  בְּרֵאשִׁ֖ית \n בָּרָ֣א "), "בְּרֵאשִׁית בָּרָא");
        assert_eq!(spoken("ὁ  λόγος"), "ὁ λόγος");
    }

    #[test]
    fn the_listing_of_voices_is_read_by_name_and_language() {
        let listing = "Carmit              he_IL    # שלום, שמי כרמית.\nMelina              el_GR    # Χαίρετε!\nGood News           en_US    # Hello\nBad line\n";
        let voices = voices_from_listing(listing);
        assert_eq!(voices[0], ("Carmit".into(), "he_IL".into()));
        assert_eq!(voices[1], ("Melina".into(), "el_GR".into()));
        assert_eq!(voices[2], ("Good News".into(), "en_US".into()), "a name with a space");
        assert_eq!(voices.len(), 3);
    }

    #[test]
    fn a_word_is_said_by_a_program_with_the_text_after_two_dashes_so_it_is_never_an_option() {
        let Some(say) = command(Language::Greek, "ἀγάπη") else { return eprintln!("skipped: no Greek voice here") };
        assert_eq!(say.args.last().map(String::as_str), Some("ἀγάπη"));
        assert_eq!(say.args[say.args.len() - 2], "--");
        assert!(command(Language::Greek, "   ").is_none(), "nothing to say");
        assert!(speak("hello").is_err(), "no Greek or Hebrew in it");
    }
}
