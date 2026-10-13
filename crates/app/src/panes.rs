//! The arrangement of panes: a binary tree of splits whose leaves are panes. Pure data, so
//! splitting, closing and resizing are easy to test; `split.rs` draws it.

pub type PaneId = u32;
pub type SplitId = u32;

/// Panes either side by side (`Row`) or one above the other (`Column`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

/// The least and most of a split the first side may take, so neither can be dragged shut.
pub const MIN_FRACTION: f32 = 0.12;
pub const MAX_FRACTION: f32 = 0.88;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Leaf(PaneId),
    Split { id: SplitId, axis: Axis, fraction: f32, a: Box<Node>, b: Box<Node> },
}

impl Node {
    /// The panes, first to last (left to right, top to bottom).
    #[cfg(test)]
    pub fn leaves(&self) -> Vec<PaneId> {
        match self {
            Node::Leaf(p) => vec![*p],
            Node::Split { a, b, .. } => a.leaves().into_iter().chain(b.leaves()).collect(),
        }
    }

    #[cfg(test)]
    pub fn contains(&self, pane: PaneId) -> bool {
        match self {
            Node::Leaf(p) => *p == pane,
            Node::Split { a, b, .. } => a.contains(pane) || b.contains(pane),
        }
    }

    /// Divide `target` in two along `axis`, keeping it first and putting `new` after it (to the
    /// right, or below). False if there is no `target`.
    pub fn split(&mut self, target: PaneId, new: PaneId, id: SplitId, axis: Axis) -> bool {
        match self {
            Node::Leaf(p) if *p == target => {
                *self = Node::Split {
                    id,
                    axis,
                    fraction: 0.5,
                    a: Box::new(Node::Leaf(target)),
                    b: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split(target, new, id, axis) || b.split(target, new, id, axis),
        }
    }

    /// Take `target` out; its neighbour takes its space. False if it is the only pane (or
    /// absent).
    pub fn remove(&mut self, target: PaneId) -> bool {
        let Node::Split { a, b, .. } = self else { return false };
        if matches!(**a, Node::Leaf(p) if p == target) {
            *self = (**b).clone();
            true
        } else if matches!(**b, Node::Leaf(p) if p == target) {
            *self = (**a).clone();
            true
        } else {
            a.remove(target) || b.remove(target)
        }
    }

    /// Move a divider: the first side takes `fraction` of the split, within limits.
    pub fn set_fraction(&mut self, split: SplitId, to: f32) -> bool {
        match self {
            Node::Leaf(_) => false,
            Node::Split { id, fraction, a, b, .. } => {
                if *id == split {
                    *fraction = to.clamp(MIN_FRACTION, MAX_FRACTION);
                    true
                } else {
                    a.set_fraction(split, to) || b.set_fraction(split, to)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_of(node: &Node) -> Option<Axis> {
        match node {
            Node::Split { axis, .. } => Some(*axis),
            Node::Leaf(_) => None,
        }
    }

    #[test]
    fn a_pane_splits_across_or_down_as_asked_and_the_new_pane_follows() {
        let mut n = Node::Leaf(1);
        assert!(n.split(1, 2, 10, Axis::Row));
        assert_eq!(n.leaves(), [1, 2]);
        assert_eq!(axis_of(&n), Some(Axis::Row));
        assert!(n.split(2, 3, 11, Axis::Column));
        assert!(n.split(1, 4, 12, Axis::Column));
        assert_eq!(n.leaves(), [1, 4, 2, 3]);
        let Node::Split { a, b, .. } = &n else { unreachable!() };
        assert_eq!((axis_of(a), axis_of(b)), (Some(Axis::Column), Some(Axis::Column)));
        // The same pane can go either way, whatever its neighbours did.
        let mut m = Node::Leaf(1);
        m.split(1, 2, 10, Axis::Row);
        m.split(2, 3, 11, Axis::Row);
        let Node::Split { b, .. } = &m else { unreachable!() };
        assert_eq!(axis_of(b), Some(Axis::Row));
    }

    #[test]
    fn splitting_a_missing_pane_changes_nothing() {
        let mut n = Node::Leaf(1);
        assert!(!n.split(9, 2, 10, Axis::Row));
        assert_eq!(n, Node::Leaf(1));
    }

    #[test]
    fn removing_a_pane_gives_its_space_to_its_neighbour() {
        let mut n = Node::Leaf(1);
        n.split(1, 2, 10, Axis::Row);
        n.split(2, 3, 11, Axis::Row);
        assert!(n.remove(2));
        assert_eq!(n.leaves(), [1, 3]);
        assert_eq!(axis_of(&n), Some(Axis::Row), "the stacked split collapsed away");
        assert!(n.remove(1));
        assert_eq!(n, Node::Leaf(3));
        assert!(!n.remove(3), "the last pane stays");
        assert!(!n.remove(7));
    }

    #[test]
    fn dividers_move_within_limits() {
        let mut n = Node::Leaf(1);
        n.split(1, 2, 10, Axis::Row);
        n.split(2, 3, 11, Axis::Row);
        assert!(n.set_fraction(10, 0.3));
        assert!(n.set_fraction(11, 0.99));
        let Node::Split { fraction, b, .. } = &n else { unreachable!() };
        assert_eq!(*fraction, 0.3);
        let Node::Split { fraction: inner, .. } = &**b else { unreachable!() };
        assert_eq!(*inner, MAX_FRACTION);
        assert!(!n.set_fraction(99, 0.5));
        n.set_fraction(10, 0.0);
        let Node::Split { fraction, .. } = &n else { unreachable!() };
        assert_eq!(*fraction, MIN_FRACTION);
    }

    #[test]
    fn contains_finds_panes_at_any_depth() {
        let mut n = Node::Leaf(1);
        n.split(1, 2, 10, Axis::Row);
        n.split(2, 3, 11, Axis::Row);
        assert!(n.contains(3) && n.contains(1) && !n.contains(4));
    }
}
