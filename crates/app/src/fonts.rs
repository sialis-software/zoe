//! The interface font.
//!
//! iced's `Font::DEFAULT` is the generic "sans-serif", which the font database maps to a
//! family that may not be installed ("Open Sans" here). Text then falls back glyph by glyph,
//! and the bold weight can land in a different face (a monospace one). Naming a real family
//! keeps regular and bold in the same face. Where the family is missing the old fallback
//! applies, so nothing breaks.

use iced::Font;

#[cfg(target_os = "macos")]
const FAMILY: &str = "SF Pro Text";
#[cfg(target_os = "windows")]
const FAMILY: &str = "Segoe UI";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const FAMILY: &str = "Noto Sans";

pub const UI: Font = Font::with_name(FAMILY);

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::graphics::text::cosmic_text::{Family, FontSystem, fontdb};

    #[test]
    fn regular_and_bold_come_from_the_same_family_when_it_is_installed() {
        let fs = FontSystem::new();
        let db = fs.db();
        let face = |weight: u16| {
            let query = fontdb::Query {
                families: &[Family::Name(FAMILY)],
                weight: fontdb::Weight(weight),
                stretch: fontdb::Stretch::Normal,
                style: fontdb::Style::Normal,
            };
            db.query(&query).and_then(|id| db.face(id)).map(|f| (f.families[0].0.clone(), f.weight.0))
        };
        let (Some(regular), Some(bold)) = (face(400), face(700)) else {
            return eprintln!("skipped: {FAMILY} is not installed");
        };
        assert_eq!(regular.0, bold.0);
        assert!(bold.1 >= 600, "a genuinely bold face, not the regular one: {bold:?}");
    }

    /// The word that starts with a marker (`**bold` while the markers show) must be shaped
    /// with the bold face like the rest of the bold text: cosmic-text 0.15 used the face of
    /// the word's first character, which is why the workspace patches it (vendor/cosmic-text).
    #[test]
    fn a_word_starting_with_an_unbold_marker_is_still_bold_after_it() {
        use iced::advanced::graphics::text::cosmic_text::{Attrs, Buffer, Metrics, Shaping, Weight};
        let mut fs = FontSystem::new();
        // A family whose regular and bold are separate faces, so the face id tells them apart.
        let family = ["Helvetica Neue", "Arial", "DejaVu Sans", "Noto Sans", "Segoe UI"].into_iter().find(|name| {
            let face = |w: u16| {
                let q = fontdb::Query {
                    families: &[Family::Name(name)],
                    weight: fontdb::Weight(w),
                    stretch: fontdb::Stretch::Normal,
                    style: fontdb::Style::Normal,
                };
                fs.db().query(&q)
            };
            matches!((face(400), face(700)), (Some(a), Some(b)) if a != b)
        });
        let Some(family) = family else { return eprintln!("skipped: no family with separate regular and bold faces") };
        let plain = Attrs::new().family(Family::Name(family));
        let bold = plain.clone().weight(Weight::BOLD);
        let mut buffer = Buffer::new(&mut fs, Metrics::new(17.0, 22.0));
        buffer.set_size(&mut fs, Some(600.0), None);
        buffer.set_rich_text(
            &mut fs,
            [("**", plain.clone()), ("bold words", bold.clone()), ("**", plain.clone())],
            &plain,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fs, false);
        let text = "**bold words**";
        let run = buffer.layout_runs().next().expect("a laid-out line");
        let face_of = |piece: &str| {
            run.glyphs.iter().find(|g| &text[g.start..g.end] == piece).map(|g| g.font_id).unwrap_or_else(|| panic!("no glyph {piece:?}"))
        };
        let regular = face_of("*");
        assert_ne!(face_of("b"), regular, "the first letter after the marker is bold");
        assert_eq!(face_of("b"), face_of("w"), "and the same face as the words after it");
    }
}
