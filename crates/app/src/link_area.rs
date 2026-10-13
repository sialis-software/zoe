//! A wrapper that gives something that is a link (a button, say) what a link in the text has:
//! a right click offers where to open it, and pulling it away carries it somewhere.
//!
//! Both report where the pointer is in the window. A click it does not turn into either goes on
//! to what it wraps, as it would have.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// How far the pointer goes with the button down before it is carrying the link.
const PULL: f32 = 3.0;

pub struct LinkArea<'a, Message> {
    content: Element<'a, Message>,
    on_menu: Box<dyn Fn(Point) -> Message + 'a>,
    on_pull: Box<dyn Fn(Point) -> Message + 'a>,
}

pub fn link_area<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_menu: impl Fn(Point) -> Message + 'a,
    on_pull: impl Fn(Point) -> Message + 'a,
) -> LinkArea<'a, Message> {
    LinkArea { content: content.into(), on_menu: Box::new(on_menu), on_pull: Box::new(on_pull) }
}

#[derive(Default)]
struct State {
    /// Where the button went down on it, while it is down and nothing has come of it.
    pressed: Option<Point>,
    /// The link was pulled away: the button coming up is not a click.
    pulled: bool,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for LinkArea<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
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
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if cursor.is_over(layout.bounds()) => {
                if let Some(at) = cursor.position() {
                    shell.publish((self.on_menu)(at));
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if cursor.is_over(layout.bounds()) => {
                state.pressed = cursor.position();
                state.pulled = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(from) = state.pressed
                    && (position.x - from.x).hypot(position.y - from.y) > PULL
                {
                    state.pressed = None;
                    state.pulled = true;
                    shell.publish((self.on_pull)(*position));
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.pressed = None;
                // Carried away: coming up is not a click on this.
                if std::mem::take(&mut state.pulled) {
                    shell.capture_event();
                    return;
                }
            }
            _ => {}
        }
        self.content.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
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
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, Message: 'a> From<LinkArea<'a, Message>> for Element<'a, Message> {
    fn from(area: LinkArea<'a, Message>) -> Self {
        Element::new(area)
    }
}
