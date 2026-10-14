//! A popover: something (a swatch, say) with a panel that floats over the window under it while it is open,
//! drawn on top of what is there instead of making room, and let go of by a click anywhere else or Esc.
//!
//! The panel is an overlay, so it is not clipped by what it is in (a scrolled strip, a boxed section) and
//! it keeps inside the window.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::keyboard::{self, key::Named};
use iced::widget::space;
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// The gap between the thing and its panel.
const GAP: f32 = 6.0;

pub struct Popover<'a, Message> {
    base: Element<'a, Message>,
    /// The panel, when it is open.
    panel: Option<Element<'a, Message>>,
    on_dismiss: Option<Message>,
}

/// `base`, with `panel` under it while it is `Some`; `on_dismiss` is sent when it is let go of.
pub fn popover<'a, Message>(base: impl Into<Element<'a, Message>>, panel: Option<Element<'a, Message>>, on_dismiss: Message) -> Popover<'a, Message> {
    Popover { base: base.into(), panel, on_dismiss: Some(on_dismiss) }
}

impl<Message> Popover<'_, Message> {
    /// The panel's own element in the tree: an empty one when it is closed, so the tree keeps its shape.
    fn closed() -> Element<'static, Message>
    where
        Message: 'static,
    {
        space::horizontal().width(0).height(0).into()
    }
}

impl<Message: Clone + 'static> Widget<Message, Theme, iced::Renderer> for Popover<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<()>()
    }

    fn state(&self) -> tree::State {
        tree::State::None
    }

    fn children(&self) -> Vec<Tree> {
        let closed = Self::closed();
        vec![Tree::new(&self.base), Tree::new(self.panel.as_ref().unwrap_or(&closed))]
    }

    fn diff(&self, tree: &mut Tree) {
        let closed = Self::closed();
        tree.diff_children(&[&self.base, self.panel.as_ref().unwrap_or(&closed)]);
    }

    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        self.base.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        self.base.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.base.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.base.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.base.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let (base_tree, panel_tree) = tree.children.split_at_mut(1);
        let nested = self.base.as_widget_mut().overlay(&mut base_tree[0], layout, renderer, viewport, translation);
        let panel = self.panel.as_mut().map(|panel| {
            overlay::Element::new(Box::new(Panel {
                at: layout.position() + translation,
                below: layout.bounds().height,
                panel,
                tree: &mut panel_tree[0],
                on_dismiss: self.on_dismiss.clone(),
            }))
        });
        if nested.is_some() || panel.is_some() {
            Some(overlay::Group::with_children(nested.into_iter().chain(panel).collect()).overlay())
        } else {
            None
        }
    }
}

struct Panel<'a, 'b, Message> {
    /// The top left of the thing the panel is under, in the window.
    at: Point,
    /// How tall the thing is.
    below: f32,
    panel: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    on_dismiss: Option<Message>,
}

impl<Message: Clone> overlay::Overlay<Message, Theme, iced::Renderer> for Panel<'_, '_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.panel.as_widget_mut().layout(self.tree, renderer, &layout::Limits::new(Size::ZERO, bounds));
        let size = node.size();
        // Under the thing, level with its left edge, and kept inside the window (above it when there is no room).
        let x = self.at.x.min(bounds.width - size.width).max(0.0);
        let below = self.at.y + self.below + GAP;
        let y = if below + size.height <= bounds.height { below } else { (self.at.y - GAP - size.height).max(0.0) };
        layout::Node::with_children(size, vec![node]).move_to(Point::new(x, y))
    }

    fn draw(&self, renderer: &mut iced::Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor) {
        if let Some(inner) = layout.children().next() {
            self.panel.as_widget().draw(self.tree, renderer, theme, style, inner, cursor, &Rectangle::with_size(Size::INFINITE));
        }
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        if let Some(inner) = layout.children().next() {
            self.panel.as_widget_mut().operate(self.tree, inner, renderer, operation);
        }
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let dismiss = |shell: &mut Shell<'_, Message>| {
            if let Some(message) = &self.on_dismiss {
                shell.publish(message.clone());
            }
            shell.capture_event();
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) if !cursor.is_over(layout.bounds()) => return dismiss(shell),
            Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(Named::Escape), .. }) => return dismiss(shell),
            _ => {}
        }
        if let Some(inner) = layout.children().next() {
            self.panel.as_widget_mut().update(self.tree, event, inner, cursor, renderer, clipboard, shell, &Rectangle::with_size(Size::INFINITE));
        }
    }

    fn mouse_interaction(&self, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &iced::Renderer) -> mouse::Interaction {
        match layout.children().next() {
            Some(inner) => self.panel.as_widget().mouse_interaction(self.tree, inner, cursor, &Rectangle::with_size(Size::INFINITE), renderer),
            None => mouse::Interaction::None,
        }
    }
}

impl<'a, Message: Clone + 'static> From<Popover<'a, Message>> for Element<'a, Message> {
    fn from(popover: Popover<'a, Message>) -> Self {
        Element::new(popover)
    }
}
