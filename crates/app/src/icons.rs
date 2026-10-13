//! Icons: SVG files in `assets/icons`, built into the program and drawn in the colour of a
//! secondary button's label, so they sit in a button the way its text would.
//!
//! The files are plain strokes on a 24 unit square; the colour in them is replaced when drawn.

use iced::widget::svg;
use iced::{Element, Length, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Link,
    Quote,
    Trash,
    Pencil,
    Plus,
    /// The notes that link to something.
    Backlinks,
    /// The hamburger of a menu.
    Menu,
    Back,
    Forward,
    /// A pane that is one of several.
    Pane,
    /// A pane filling the window.
    PaneFull,
    /// Two panes side by side.
    PanesAcross,
    /// Two panes one above the other.
    PanesDown,
    ChevronDown,
    ChevronUp,
    Search,
}

impl Icon {
    fn source(self) -> &'static [u8] {
        match self {
            Icon::Link => include_bytes!("../assets/icons/link.svg"),
            Icon::Quote => include_bytes!("../assets/icons/quote.svg"),
            Icon::Trash => include_bytes!("../assets/icons/trash.svg"),
            Icon::Pencil => include_bytes!("../assets/icons/pencil.svg"),
            Icon::Plus => include_bytes!("../assets/icons/plus.svg"),
            Icon::Backlinks => include_bytes!("../assets/icons/backlinks.svg"),
            Icon::Menu => include_bytes!("../assets/icons/menu.svg"),
            Icon::Back => include_bytes!("../assets/icons/arrow-left.svg"),
            Icon::Forward => include_bytes!("../assets/icons/arrow-right.svg"),
            Icon::Pane => include_bytes!("../assets/icons/pane-one.svg"),
            Icon::PaneFull => include_bytes!("../assets/icons/pane-full.svg"),
            Icon::PanesAcross => include_bytes!("../assets/icons/pane-across.svg"),
            Icon::PanesDown => include_bytes!("../assets/icons/pane-down.svg"),
            Icon::ChevronDown => include_bytes!("../assets/icons/chevron-down.svg"),
            Icon::ChevronUp => include_bytes!("../assets/icons/chevron-up.svg"),
            Icon::Search => include_bytes!("../assets/icons/search.svg"),
        }
    }

    /// The pane button's icon: outlined, or filled while the pane fills the window.
    pub fn from_zoom(zoomed: bool) -> Icon {
        if zoomed { Icon::PaneFull } else { Icon::Pane }
    }

    /// The selector's arrow: up while its panel is open.
    pub fn chevron(open: bool) -> Icon {
        if open { Icon::ChevronUp } else { Icon::ChevronDown }
    }

    /// The icon `size` pixels square.
    pub fn view<'a, M: 'a>(self, size: f32) -> Element<'a, M> {
        svg(svg::Handle::from_memory(self.source()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(|theme: &Theme, _status| svg::Style { color: Some(theme.extended_palette().secondary.base.text) })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Icon; 16] = [
        Icon::Link,
        Icon::Quote,
        Icon::Trash,
        Icon::Pencil,
        Icon::Plus,
        Icon::Backlinks,
        Icon::Menu,
        Icon::Back,
        Icon::Forward,
        Icon::Pane,
        Icon::PaneFull,
        Icon::PanesAcross,
        Icon::PanesDown,
        Icon::ChevronDown,
        Icon::ChevronUp,
        Icon::Search,
    ];

    #[test]
    fn every_icon_is_a_well_formed_svg_with_the_same_square() {
        for icon in ALL {
            let text = std::str::from_utf8(icon.source()).unwrap();
            assert!(text.starts_with("<svg") && text.trim_end().ends_with("</svg>"), "{icon:?}");
            assert!(text.contains(r#"viewBox="0 0 24 24""#), "{icon:?} uses the common 24 unit square");
        }
    }
}
