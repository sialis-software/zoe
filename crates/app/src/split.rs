//! A container for two children with a draggable divider between them.
//!
//! The divider is part of this widget, so a drag keeps working when the pointer strays over
//! the children: while dragging it takes every mouse event and reports the new fraction.

use crate::panes::{Axis, MAX_FRACTION, MIN_FRACTION};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, Widget, mouse, overlay};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// Thickness of the strip the pointer grabs; the line drawn in it is thinner.
const HANDLE: f32 = 7.0;

pub struct Split<'a, Message> {
    axis: Axis,
    fraction: f32,
    children: Vec<Element<'a, Message>>,
    on_resize: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<'a, Message> Split<'a, Message> {
    pub fn new(
        axis: Axis,
        fraction: f32,
        a: Element<'a, Message>,
        b: Element<'a, Message>,
        on_resize: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Split { axis, fraction, children: vec![a, b], on_resize: Box::new(on_resize) }
    }
}

#[derive(Default)]
struct State {
    dragging: bool,
}

/// The two sides and the divider between them, inside `bounds`.
fn geometry(bounds: Rectangle, axis: Axis, fraction: f32) -> [Rectangle; 3] {
    let (len, cross) = match axis {
        Axis::Row => (bounds.width, bounds.height),
        Axis::Column => (bounds.height, bounds.width),
    };
    let room = (len - HANDLE).max(0.0);
    let first = (room * fraction).round();
    let rect = |at: f32, size: f32| match axis {
        Axis::Row => Rectangle::new(Point::new(bounds.x + at, bounds.y), Size::new(size, cross)),
        Axis::Column => Rectangle::new(Point::new(bounds.x, bounds.y + at), Size::new(cross, size)),
    };
    [rect(0.0, first), rect(first, HANDLE), rect(first + HANDLE, room - first)]
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Split<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let [a, handle, b] = geometry(Rectangle::new(Point::ORIGIN, size), self.axis, self.fraction);
        let mut nodes = Vec::new();
        for (i, rect) in [a, b].into_iter().enumerate() {
            let fixed = layout::Limits::new(rect.size(), rect.size());
            let node = self.children[i].as_widget_mut().layout(&mut tree.children[i], renderer, &fixed);
            nodes.push(node.move_to(rect.position()));
        }
        // The divider has no widget; its place is recomputed from the bounds when needed.
        let _ = handle;
        layout::Node::with_children(size, nodes)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.children.iter_mut().zip(&mut tree.children).zip(layout.children()).for_each(
                |((child, state), layout)| child.as_widget_mut().operate(state, layout, renderer, operation),
            );
        });
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
        let bounds = layout.bounds();
        let [_, handle, _] = geometry(bounds, self.axis, self.fraction);
        let state = tree.state.downcast_mut::<State>();

        if state.dragging {
            match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    let (at, len, from) = match self.axis {
                        Axis::Row => (position.x, bounds.width, bounds.x),
                        Axis::Column => (position.y, bounds.height, bounds.y),
                    };
                    // The pointer is the middle of the divider.
                    let room = (len - HANDLE).max(1.0);
                    let fraction = ((at - from - HANDLE / 2.0) / room).clamp(MIN_FRACTION, MAX_FRACTION);
                    shell.publish((self.on_resize)(fraction));
                    shell.capture_event();
                    shell.request_redraw();
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    state.dragging = false;
                    shell.capture_event();
                    shell.request_redraw();
                }
                Event::Mouse(_) => shell.capture_event(),
                _ => {}
            }
            return;
        }

        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))) && cursor.is_over(handle) {
            state.dragging = true;
            shell.capture_event();
            shell.request_redraw();
            return;
        }

        for ((child, tree), layout) in self.children.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            child.as_widget_mut().update(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let [_, handle, _] = geometry(layout.bounds(), self.axis, self.fraction);
        if state.dragging || cursor.is_over(handle) {
            return match self.axis {
                Axis::Row => mouse::Interaction::ResizingHorizontally,
                Axis::Column => mouse::Interaction::ResizingVertically,
            };
        }
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, tree), layout)| child.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
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
        let bounds = layout.bounds();
        let Some(visible) = bounds.intersection(viewport) else { return };
        for ((child, tree), layout) in self.children.iter().zip(&tree.children).zip(layout.children()) {
            if layout.bounds().intersects(&visible) {
                child.as_widget().draw(tree, renderer, theme, style, layout, cursor, &visible);
            }
        }
        let state = tree.state.downcast_ref::<State>();
        let [_, handle, _] = geometry(bounds, self.axis, self.fraction);
        let hot = state.dragging || cursor.is_over(handle);
        let palette = theme.extended_palette();
        let (color, thickness) =
            if hot { (palette.primary.base.color, 3.0) } else { (palette.background.strong.color, 1.0) };
        let line = match self.axis {
            Axis::Row => Rectangle::new(
                Point::new(handle.x + (handle.width - thickness) / 2.0, handle.y),
                Size::new(thickness, handle.height),
            ),
            Axis::Column => Rectangle::new(
                Point::new(handle.x, handle.y + (handle.height - thickness) / 2.0),
                Size::new(handle.width, thickness),
            ),
        };
        renderer.fill_quad(Quad { bounds: line, ..Quad::default() }, color);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        overlay::from_children(&mut self.children, tree, layout, renderer, viewport, translation)
    }
}

impl<'a, Message: 'a> From<Split<'a, Message>> for Element<'a, Message> {
    fn from(split: Split<'a, Message>) -> Self {
        Element::new(split)
    }
}
