//! A tab's history: the places it has been, with where it is now, like a browser's. Going to a new
//! place forgets whatever was ahead; going back and forward moves along what is kept.

/// The most places remembered; the oldest go first.
const LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub struct History<T> {
    entries: Vec<T>,
    at: usize,
}

impl<T: Clone> History<T> {
    pub fn new(first: T) -> Self {
        History { entries: vec![first], at: 0 }
    }

    pub fn current(&self) -> &T {
        &self.entries[self.at]
    }

    pub fn current_mut(&mut self) -> &mut T {
        &mut self.entries[self.at]
    }

    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.entries.len()
    }

    /// Record having gone to a new place, unless `same` says it is where we already are (then the
    /// entry is only brought up to date). What was ahead is forgotten.
    pub fn go(&mut self, entry: T, same: impl Fn(&T, &T) -> bool) {
        if same(self.current(), &entry) {
            *self.current_mut() = entry;
            return;
        }
        self.entries.truncate(self.at + 1);
        self.entries.push(entry);
        if self.entries.len() > LIMIT {
            self.entries.remove(0);
        }
        self.at = self.entries.len() - 1;
    }

    /// One place back, and where that is.
    pub fn back(&mut self) -> Option<&T> {
        self.can_back().then(|| {
            self.at -= 1;
            &self.entries[self.at]
        })
    }

    pub fn forward(&mut self) -> Option<&T> {
        self.can_forward().then(|| {
            self.at += 1;
            &self.entries[self.at]
        })
    }

    /// Move to place number `index` of the list (as [`History::entries`] has it).
    pub fn jump(&mut self, index: usize) -> Option<&T> {
        (index < self.entries.len()).then(|| {
            self.at = index;
            &self.entries[index]
        })
    }

    /// Everything kept, oldest first, and which is where we are.
    pub fn entries(&self) -> (&[T], usize) {
        (&self.entries, self.at)
    }

    /// Change every entry (a note was renamed, say).
    pub fn for_each_mut(&mut self, f: impl FnMut(&mut T)) {
        self.entries.iter_mut().for_each(f);
    }

    /// Drop the entries `keep` rejects, other than the one we are at. Places after dropped ones
    /// stay in order and where we are is still the same place.
    pub fn retain_others(&mut self, keep: impl Fn(&T) -> bool) {
        let here = self.at;
        let mut index = 0;
        let mut new_at = 0;
        self.entries.retain(|e| {
            let stays = index == here || keep(e);
            if stays && index < here {
                new_at += 1;
            }
            index += 1;
            stays
        });
        self.at = new_at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &i32, b: &i32) -> bool {
        a == b
    }

    #[test]
    fn going_back_and_forward_walks_what_was_visited() {
        let mut h = History::new(1);
        for n in [2, 3, 4] {
            h.go(n, same);
        }
        assert_eq!(*h.current(), 4);
        assert_eq!((h.can_back(), h.can_forward()), (true, false));
        assert_eq!(h.back().copied(), Some(3));
        assert_eq!(h.back().copied(), Some(2));
        assert_eq!(h.forward().copied(), Some(3));
        assert_eq!((h.can_back(), h.can_forward()), (true, true));
        assert_eq!(h.back().copied(), Some(2));
        assert_eq!(h.back().copied(), Some(1));
        assert_eq!(h.back(), None, "nowhere further back");
        assert_eq!(h.entries().1, 0);
    }

    #[test]
    fn a_new_place_forgets_what_was_ahead() {
        let mut h = History::new(1);
        for n in [2, 3] {
            h.go(n, same);
        }
        h.back();
        h.back();
        h.go(9, same);
        assert_eq!(h.entries(), (&[1, 9][..], 1));
        assert!(!h.can_forward());
    }

    #[test]
    fn going_where_we_already_are_adds_nothing_but_updates_the_entry() {
        #[derive(Clone, Debug, PartialEq)]
        struct E(u8, f32);
        let mut h = History::new(E(1, 0.0));
        h.go(E(1, 40.0), |a, b| a.0 == b.0);
        assert_eq!(h.entries().0.len(), 1);
        assert_eq!(h.current(), &E(1, 40.0), "the scroll it was left at is kept");
    }

    #[test]
    fn only_so_many_places_are_kept() {
        let mut h = History::new(0);
        for n in 1..=150 {
            h.go(n, same);
        }
        assert_eq!(h.entries().0.len(), LIMIT);
        assert_eq!(*h.current(), 150);
        assert_eq!(h.entries().0[0], 51, "the oldest went");
    }

    #[test]
    fn jumping_goes_straight_to_a_place() {
        let mut h = History::new(1);
        for n in [2, 3, 4] {
            h.go(n, same);
        }
        assert_eq!(h.jump(1).copied(), Some(2));
        assert_eq!(h.entries().1, 1);
        assert!(h.can_back() && h.can_forward());
        assert_eq!(h.jump(9), None);
    }

    #[test]
    fn dropping_entries_keeps_the_current_one_and_its_place_in_line() {
        let mut h = History::new(1);
        for n in [2, 3, 4, 5] {
            h.go(n, same);
        }
        h.jump(2); // at 3
        h.retain_others(|e| e % 2 == 1); // drop 2 and 4, and 5 stays (odd)
        assert_eq!(h.entries(), (&[1, 3, 5][..], 1));
        assert_eq!(*h.current(), 3);
        // The one we are at stays even when rejected.
        h.retain_others(|_| false);
        assert_eq!(h.entries(), (&[3][..], 0));
    }
}
