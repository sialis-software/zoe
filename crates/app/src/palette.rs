//! Colors for text we style ourselves (reader spans, editor highlighting).
//! Widget chrome follows the iced `Theme`; this follows the same light/dark choice.

use iced::{Color, Theme};

#[derive(Clone, Copy)]
pub struct Palette {
    /// Words of Jesus.
    pub red: Color,
    pub muted: Color,
    pub link: Color,
    /// Highlight for drag-selected text (translucent, drawn under the glyphs).
    pub text_selection: Color,
    /// The margin beside the text: its background and its edge.
    pub margin_bg: Color,
    pub margin_rule: Color,
    /// A faint panel behind code, and the bar beside a quote.
    pub block_bg: Color,
    pub quote_bar: Color,
    pub heading: Color,
    pub code: Color,
    pub note_link: Color,
    pub module_link: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

impl Palette {
    pub fn for_theme(theme: &Theme) -> Self {
        if theme.extended_palette().is_dark { Self::DARK } else { Self::LIGHT }
    }

    const LIGHT: Palette = Palette {
        red: rgb(176, 41, 41),
        muted: rgb(115, 115, 128),
        link: rgb(26, 77, 179),
        text_selection: Color::from_rgba(0.25, 0.5, 0.95, 0.30),
        margin_bg: Color::from_rgba(0.0, 0.0, 0.0, 0.04),
        margin_rule: Color::from_rgba(0.0, 0.0, 0.0, 0.14),
        block_bg: Color::from_rgba(0.0, 0.0, 0.0, 0.05),
        quote_bar: Color::from_rgba(0.0, 0.0, 0.0, 0.30),
        heading: rgb(26, 51, 128),
        code: rgb(153, 77, 26),
        note_link: rgb(26, 128, 77),
        module_link: rgb(128, 51, 153),
    };

    // Softer than pure white-on-black: slightly desaturated, easier on the eyes.
    const DARK: Palette = Palette {
        red: rgb(240, 128, 120),
        muted: rgb(150, 154, 168),
        link: rgb(130, 177, 255),
        text_selection: Color::from_rgba(0.35, 0.58, 1.0, 0.40),
        margin_bg: Color::from_rgba(1.0, 1.0, 1.0, 0.035),
        margin_rule: Color::from_rgba(1.0, 1.0, 1.0, 0.14),
        block_bg: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
        quote_bar: Color::from_rgba(1.0, 1.0, 1.0, 0.35),
        heading: rgb(140, 175, 240),
        code: rgb(230, 175, 120),
        note_link: rgb(125, 215, 160),
        module_link: rgb(205, 165, 240),
    };
}
