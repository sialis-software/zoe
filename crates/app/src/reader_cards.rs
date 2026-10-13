//! Margin-card placement (docs/05 §4): where cards go beside the text, given where their
//! marks are. Pure, so it is easy to test.

/// A card to place: where its mark sits, how tall the card is, and whether it should stay
/// level with its mark if at all possible (e.g. the card being edited).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub anchor: f32,
    pub height: f32,
    pub pinned: bool,
}

/// Top `y` for each slot (same order as `slots`). Cards keep to the order of their anchors,
/// sit level with their marks when there is room, and otherwise stack with `gap` between
/// them. Pinned cards hold their anchor and the others make way, upwards as well as down.
pub fn place_column(slots: &[Slot], gap: f32) -> Vec<f32> {
    let n = slots.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| slots[a].anchor.total_cmp(&slots[b].anchor).then(a.cmp(&b)));

    let mut y = vec![0.0_f32; n];
    let forward = |y: &mut Vec<f32>| {
        let mut floor = 0.0_f32;
        for &i in &order {
            y[i] = y[i].max(floor);
            floor = y[i] + slots[i].height + gap;
        }
    };

    for &i in &order {
        y[i] = slots[i].anchor;
    }
    forward(&mut y);

    for (pos, &p) in order.iter().enumerate() {
        if !slots[p].pinned {
            continue;
        }
        y[p] = slots[p].anchor;
        for k in (0..pos).rev() {
            let (i, next) = (order[k], order[k + 1]);
            y[i] = y[i].min(y[next] - gap - slots[i].height);
        }
        for &i in &order {
            y[i] = y[i].max(0.0).min(y[i]);
        }
        let mut floor = 0.0_f32;
        for &i in &order {
            y[i] = y[i].max(floor);
            floor = y[i] + slots[i].height + gap;
        }
    }
    y
}

/// Which lane each connector runs in. A connector goes from its mark across a gap to its card,
/// with a vertical stretch `(top, bottom)` between the two heights. Connectors whose stretches
/// overlap would draw on top of each other, so they get different lanes where there are enough
/// (`lanes`); the rest share the least crowded lane. Returns a lane per input, in input order.
pub fn assign_lanes(stretches: &[(f32, f32)], lanes: usize) -> Vec<usize> {
    const PAD: f32 = 3.0;
    let lanes = lanes.max(1);
    let mut order: Vec<usize> = (0..stretches.len()).collect();
    order.sort_by(|&a, &b| stretches[a].0.total_cmp(&stretches[b].0).then(a.cmp(&b)));

    let overlaps = |a: (f32, f32), b: (f32, f32)| a.0 - PAD < b.1 && b.0 - PAD < a.1;
    let mut chosen: Vec<Option<usize>> = vec![None; stretches.len()];
    for (rank, &i) in order.iter().enumerate() {
        let crowd = |lane: usize| {
            order[..rank]
                .iter()
                .filter(|&&j| chosen[j] == Some(lane) && overlaps(stretches[i], stretches[j]))
                .count()
        };
        // The first lane nobody overlapping us uses; failing that, the one with fewest.
        let lane = (0..lanes).find(|&l| crowd(l) == 0).unwrap_or_else(|| {
            (0..lanes).min_by_key(|&l| (crowd(l), l)).unwrap_or(0)
        });
        chosen[i] = Some(lane);
    }
    chosen.into_iter().map(|l| l.unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(anchor: f32, height: f32) -> Slot {
        Slot { anchor, height, pinned: false }
    }

    fn no_overlap(slots: &[Slot], ys: &[f32], gap: f32) {
        let mut order: Vec<usize> = (0..slots.len()).collect();
        order.sort_by(|&a, &b| ys[a].total_cmp(&ys[b]));
        for pair in order.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(ys[a] + slots[a].height + gap <= ys[b] + 0.001, "{a} and {b} overlap: {ys:?}");
        }
        assert!(ys.iter().all(|&y| y >= 0.0), "negative position: {ys:?}");
    }

    #[test]
    fn separated_cards_sit_level_with_their_marks() {
        let slots = [slot(100.0, 50.0), slot(300.0, 80.0), slot(500.0, 40.0)];
        assert_eq!(place_column(&slots, 8.0), [100.0, 300.0, 500.0]);
    }

    #[test]
    fn colliding_cards_stack_in_anchor_order_with_a_gap() {
        // Two marks on the same line, and a third just below them.
        let slots = [slot(200.0, 60.0), slot(200.0, 60.0), slot(230.0, 60.0)];
        let ys = place_column(&slots, 8.0);
        assert_eq!(ys, [200.0, 268.0, 336.0]);
        no_overlap(&slots, &ys, 8.0);
    }

    #[test]
    fn order_follows_the_anchors_not_the_input_order() {
        let slots = [slot(400.0, 50.0), slot(100.0, 50.0), slot(250.0, 50.0)];
        let ys = place_column(&slots, 8.0);
        assert!(ys[1] < ys[2] && ys[2] < ys[0]);
        assert_eq!(ys, [400.0, 100.0, 250.0]);
    }

    #[test]
    fn a_card_near_the_top_cannot_go_above_zero() {
        let slots = [slot(5.0, 50.0), slot(10.0, 50.0)];
        let ys = place_column(&slots, 8.0);
        assert_eq!(ys, [5.0, 63.0]);
    }

    #[test]
    fn a_pinned_card_holds_its_anchor_and_the_others_make_way() {
        let mut slots = [slot(200.0, 60.0), slot(210.0, 60.0), slot(220.0, 60.0)];
        slots[1].pinned = true;
        let ys = place_column(&slots, 8.0);
        assert_eq!(ys[1], 210.0, "pinned card level with its mark");
        assert!(ys[0] + 60.0 + 8.0 <= ys[1], "the card before it moved up: {ys:?}");
        assert!(ys[2] >= ys[1] + 60.0 + 8.0, "the card after it moved down: {ys:?}");
        no_overlap(&slots, &ys, 8.0);
    }

    #[test]
    fn pinning_near_the_top_pushes_the_rest_down_instead_of_off_screen() {
        let mut slots = [slot(10.0, 100.0), slot(20.0, 100.0)];
        slots[1].pinned = true;
        let ys = place_column(&slots, 8.0);
        no_overlap(&slots, &ys, 8.0);
        assert!(ys[0] >= 0.0);
    }

    #[test]
    fn many_random_layouts_never_overlap() {
        // Deterministic pseudo-random sweep.
        let mut seed = 12345_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1 << 24) as f32
        };
        for _ in 0..200 {
            let n = 1 + (next() * 12.0) as usize;
            let mut slots: Vec<Slot> =
                (0..n).map(|_| slot(next() * 1500.0, 30.0 + next() * 120.0)).collect();
            if n > 2 && next() > 0.5 {
                let p = (next() * n as f32) as usize % n;
                slots[p].pinned = true;
            }
            let ys = place_column(&slots, 8.0);
            no_overlap(&slots, &ys, 8.0);
        }
    }

    #[test]
    fn no_cards_no_positions() {
        assert!(place_column(&[], 8.0).is_empty());
    }
    #[test]
    fn separate_connectors_share_the_first_lane() {
        let stretches = [(0.0, 10.0), (100.0, 150.0), (300.0, 300.0)];
        assert_eq!(assign_lanes(&stretches, 4), [0, 0, 0]);
    }

    #[test]
    fn overlapping_connectors_get_their_own_lanes() {
        // The three stacked connectors of a verse with three marks.
        let stretches = [(100.0, 140.0), (120.0, 220.0), (130.0, 300.0)];
        let lanes = assign_lanes(&stretches, 4);
        assert_eq!(lanes, [0, 1, 2]);
    }

    #[test]
    fn a_lane_is_reused_once_its_connector_has_finished() {
        let stretches = [(100.0, 200.0), (150.0, 400.0), (250.0, 300.0)];
        // The third overlaps only the second, so it can take the first lane again.
        assert_eq!(assign_lanes(&stretches, 4), [0, 1, 0]);
    }

    #[test]
    fn running_out_of_lanes_still_gives_valid_lanes() {
        let stretches: Vec<(f32, f32)> = (0..9).map(|i| (i as f32, 500.0)).collect();
        let lanes = assign_lanes(&stretches, 3);
        assert!(lanes.iter().all(|&l| l < 3));
        // Spread out rather than piled into one lane.
        for lane in 0..3 {
            assert_eq!(lanes.iter().filter(|&&l| l == lane).count(), 3);
        }
    }

    #[test]
    fn input_order_does_not_matter_to_the_result_for_each_connector() {
        let a = [(100.0, 140.0), (120.0, 220.0)];
        let b = [(120.0, 220.0), (100.0, 140.0)];
        let (la, lb) = (assign_lanes(&a, 4), assign_lanes(&b, 4));
        assert_eq!((la[0], la[1]), (lb[1], lb[0]));
        assert_eq!(assign_lanes(&[], 4), Vec::<usize>::new());
    }
}
