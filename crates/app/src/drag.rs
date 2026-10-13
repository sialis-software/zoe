//! Dragging: a tab, or a link, carried across the window and dropped on a pane.
//!
//! Dropping on the middle of a pane puts the tab (or a new tab for the link) in that pane;
//! dropping on the edge of a pane splits it, with the tab in the new half on that side. A tab
//! dropped on another tab goes in front of it.

use crate::opening::Spec;
use crate::panes::{Axis, PaneId};
use crate::shared::TabId;
use iced::Point;

/// What is being carried.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Tab(TabId),
    Link(Spec),
}

/// Where on a pane something is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    /// For an edge, how the pane is split and whether the new half is before (left of, above)
    /// the old.
    pub fn split(self) -> Option<(Axis, bool)> {
        match self {
            Zone::Center => None,
            Zone::Left => Some((Axis::Row, true)),
            Zone::Right => Some((Axis::Row, false)),
            Zone::Top => Some((Axis::Column, true)),
            Zone::Bottom => Some((Axis::Column, false)),
        }
    }
}

/// What the pointer is over while dragging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    /// A part of a pane's body.
    Pane(PaneId, Zone),
    /// A tab in a bar.
    Chip(TabId),
    /// The empty part of a pane's tab bar.
    Bar(PaneId),
}

#[derive(Clone, Debug)]
pub struct Drag {
    pub payload: Payload,
    pub at: Point,
    pub over: Option<Over>,
}

/// A tab pressed, which becomes a drag if the pointer moves far enough before it comes up.
#[derive(Clone, Copy, Debug)]
pub struct Pressed {
    pub tab: TabId,
    /// Where the pointer first was after the press.
    pub origin: Option<Point>,
}

/// How far the pointer must travel with a tab pressed for it to be a drag and not a click.
pub const THRESHOLD: f32 = 6.0;

pub fn far_enough(from: Point, to: Point) -> bool {
    from.distance(to) > THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_split_the_way_they_point_and_the_middle_does_not() {
        assert_eq!(Zone::Center.split(), None);
        assert_eq!(Zone::Left.split(), Some((Axis::Row, true)));
        assert_eq!(Zone::Right.split(), Some((Axis::Row, false)));
        assert_eq!(Zone::Top.split(), Some((Axis::Column, true)));
        assert_eq!(Zone::Bottom.split(), Some((Axis::Column, false)));
    }

    #[test]
    fn a_small_wobble_is_not_a_drag() {
        let a = Point::new(10.0, 10.0);
        assert!(!far_enough(a, Point::new(13.0, 12.0)));
        assert!(far_enough(a, Point::new(20.0, 10.0)));
    }
}
