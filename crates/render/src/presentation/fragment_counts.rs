//! Per-frame debris counts, with one stack slot per shape.

use sloppy_core::sim::FragmentShape;

#[derive(Default)]
pub(super) struct FragmentCounts([usize; 10]);

impl FragmentCounts {
    pub(super) fn add(&mut self, shape: FragmentShape) -> usize {
        let index = match shape {
            FragmentShape::Armor => 0,
            FragmentShape::Wheel => 1,
            FragmentShape::Track => 2,
            FragmentShape::Shard => 3,
            FragmentShape::Wood => 4,
            FragmentShape::Panel => 5,
            FragmentShape::Beam => 6,
            FragmentShape::Log => 7,
            FragmentShape::DrumShell => 8,
            FragmentShape::DrumLid => 9,
        };
        self.0[index] += 1;
        self.0[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::simulation_rules::FRAGMENT_CAPACITY;
    use std::collections::HashMap;

    const SHAPES: [FragmentShape; 10] = [
        FragmentShape::Armor,
        FragmentShape::Wheel,
        FragmentShape::Track,
        FragmentShape::Shard,
        FragmentShape::Wood,
        FragmentShape::Panel,
        FragmentShape::Beam,
        FragmentShape::Log,
        FragmentShape::DrumShell,
        FragmentShape::DrumLid,
    ];

    #[test]
    fn mixed_fragments_keep_the_same_visibility_order_and_per_shape_cap() {
        for offset in 0..SHAPES.len() {
            let mut counts = FragmentCounts::default();
            let mut expected = HashMap::<FragmentShape, usize>::new();
            for ordinal in 0..=FRAGMENT_CAPACITY {
                for index in 0..SHAPES.len() {
                    let shape = SHAPES[(index + offset) % SHAPES.len()];
                    let old_count = expected.entry(shape).or_default();
                    *old_count += 1;
                    let count = counts.add(shape);
                    assert_eq!(count, *old_count);
                    assert_eq!(count <= FRAGMENT_CAPACITY, ordinal < FRAGMENT_CAPACITY);
                }
            }
        }
    }

    #[test]
    fn each_frame_starts_with_an_unused_budget_for_every_shape() {
        for _ in 0..3 {
            let mut counts = FragmentCounts::default();
            for shape in SHAPES {
                assert_eq!(counts.add(shape), 1);
                assert_eq!(counts.add(shape), 2);
            }
        }
    }
}
