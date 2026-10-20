//! Icons: SVG files in `assets/icons`, built into the program and drawn in the colour of a
//! secondary button's label, so they sit in a button the way its text would.
//!
//! The files are plain strokes on a 24 unit square; the colour in them is replaced when drawn.

use iced::widget::{button, container, svg, text, tooltip};
use iced::{Color, Element, Length, Theme};

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
    /// The times something happened.
    History,
    /// Say it aloud.
    Speaker,
    /// Fill the window with the text alone.
    Expand,
    /// Give the window its tools back.
    Collapse,
    /// A page with a margin either side, shown / not shown.
    MarginsOn,
    MarginsOff,
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
            Icon::History => include_bytes!("../assets/icons/history.svg"),
            Icon::Speaker => include_bytes!("../assets/icons/speaker.svg"),
            Icon::Expand => include_bytes!("../assets/icons/expand.svg"),
            Icon::Collapse => include_bytes!("../assets/icons/collapse.svg"),
            Icon::MarginsOn => include_bytes!("../assets/icons/margins-on.svg"),
            Icon::MarginsOff => include_bytes!("../assets/icons/margins-off.svg"),
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

    /// The icon `size` pixels square, in the dropdowns' text color toward the page's color: all of the way at `alpha` 1, none at 0.
    /// (Mixed to a solid grey, not made see-through: a picture's color takes no part of its opacity, so a faint white
    /// would be plain white.)
    pub fn view_text<'a, M: 'a>(self, size: f32, alpha: f32) -> Element<'a, M> {
        svg(svg::Handle::from_memory(self.source()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(move |theme: &Theme, _status| {
                let p = theme.extended_palette();
                let (text, page) = (p.background.weak.text, p.background.base.color);
                let mix = |t: f32, b: f32| b + (t - b) * alpha;
                svg::Style { color: Some(Color { r: mix(text.r, page.r), g: mix(text.g, page.g), b: mix(text.b, page.b), a: 1.0 }) }
            })
            .into()
    }

    /// A history button (back, forward), dressed like the dropdowns beside it. When there is nowhere
    /// to go it is plainly off: no fill, only a faint outline, and the arrow faint, so it does not
    /// look like the ones that work.
    pub fn arrow_button<'a, M: Clone + 'a>(self, on: bool, tip: impl Into<String>, message: M) -> Element<'a, M> {
        with_tip(self.arrow_button_bare(on, message), tip, tooltip::Position::Bottom)
    }

    /// The square, secondary-styled button of a tab's bar (the menu, search) holding this icon, with the tip it must have.
    pub fn square_button<'a, M: Clone + 'a>(self, side: impl Into<Length>, tip: impl Into<String>, message: M) -> Element<'a, M> {
        let side = side.into();
        let b = button(container(self.view(18.0)).center(Length::Fill)).width(side).height(side).padding(0).style(button::secondary).on_press(message);
        with_tip(b, tip, tooltip::Position::Bottom)
    }

    fn arrow_button_bare<'a, M: Clone + 'a>(self, on: bool, message: M) -> Element<'a, M> {
        let size = |b: button::Button<'a, M>| b.width(31).height(31).padding(0);
        if on {
            return size(button(container(self.view_text(18.0, 1.0)).center(Length::Fill)))
                .style(|theme: &Theme, status| {
                    let p = theme.extended_palette();
                    let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some(p.background.weak.color.into()),
                        text_color: p.background.weak.text,
                        border: iced::Border {
                            color: if lit { p.primary.strong.color } else { p.background.strong.color },
                            width: 1.0,
                            radius: 2.0.into(),
                        },
                        ..button::Style::default()
                    }
                })
                .on_press(message)
                .into();
        }
        size(button(container(self.view_text(18.0, 0.28)).center(Length::Fill)))
            .style(|theme: &Theme, _status| {
                let line = theme.extended_palette().background.weak.text;
                button::Style {
                    background: None,
                    border: iced::Border { color: Color { a: 0.18, ..line }, width: 1.0, radius: 2.0.into() },
                    ..button::Style::default()
                }
            })
            .into()
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

/// A button (or anything) with the app's tooltip over it: the action it does, and its shortcut where it has one
/// (see `commands::tip`). Every button that is only a picture goes through this or a constructor above that takes the tip.
pub fn with_tip<'a, M: 'a>(content: impl Into<Element<'a, M>>, tip: impl Into<String>, position: tooltip::Position) -> Element<'a, M> {
    tooltip(content, container(text(tip.into()).size(12)).padding(4).style(container::rounded_box), position).into()
}

/// `with_tip` below the thing, as most are.
pub fn tip<'a, M: 'a>(content: impl Into<Element<'a, M>>, tip: impl Into<String>) -> Element<'a, M> {
    with_tip(content, tip, tooltip::Position::Bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tipped_button_is_a_tooltip_and_the_square_one_asks_for_its_tip() {
        let _: Element<'_, ()> = with_tip(button(text("x")), "Close tab", tooltip::Position::Bottom);
        let _: Element<'_, ()> = Icon::Menu.square_button(31, "Open menu", ());
        let _: Element<'_, ()> = Icon::Back.arrow_button(false, "Back", ());
    }

    const ALL: [Icon; 22] = [
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
        Icon::History,
        Icon::Speaker,
        Icon::Expand,
        Icon::Collapse,
        Icon::MarginsOn,
        Icon::MarginsOff,
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
