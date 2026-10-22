//! Grouping items by equal keys, for the mesh operations that weld corners or edges
//! by quantised position.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// Items grouped by equal keys, each group listing its items in ascending order, so
/// a sum over a group adds in item order: a counting sort over the keys, numbered as
/// they are first met. It does the work of a `HashMap<K, Vec<usize>>` without a
/// vector per group or SipHash, which made up most of welding a large mesh.
pub(crate) struct Buckets {
    /// The group of each item.
    group: Vec<usize>,
    /// Group `g` holds `items[starts[g]..starts[g + 1]]`.
    starts: Vec<usize>,
    items: Vec<usize>,
}

impl Buckets {
    pub(crate) fn new<K: Hash + Eq>(keys: impl Iterator<Item = K>) -> Self {
        let mut numbers: HashMap<K, usize, BuildHasherDefault<FxHasher>> = HashMap::default();
        let mut group = Vec::with_capacity(keys.size_hint().0);
        let mut counts: Vec<usize> = Vec::new();
        for key in keys {
            let next = numbers.len();
            let number = *numbers.entry(key).or_insert(next);
            if number == counts.len() {
                counts.push(0);
            }
            counts[number] += 1;
            group.push(number);
        }
        let mut starts = Vec::with_capacity(counts.len() + 1);
        let mut total = 0;
        for count in counts {
            starts.push(total);
            total += count;
        }
        starts.push(total);
        let mut next = starts.clone();
        let mut items = vec![0; total];
        for (item, &number) in group.iter().enumerate() {
            items[next[number]] = item;
            next[number] += 1;
        }
        Self {
            group,
            starts,
            items,
        }
    }

    /// The items whose key equals `item`'s, `item` included, in ascending order.
    pub(crate) fn with(&self, item: usize) -> &[usize] {
        let number = self.group[item];
        &self.items[self.starts[number]..self.starts[number + 1]]
    }

    /// Every group, in the order its key was first met.
    pub(crate) fn groups(&self) -> impl Iterator<Item = &[usize]> {
        self.starts
            .windows(2)
            .map(|range| &self.items[range[0]..range[1]])
    }
}

/// rustc's FxHash: the keys are a mesh's own quantised positions, where SipHash's
/// resistance to chosen keys buys nothing.
#[derive(Default)]
struct FxHasher(u64);

impl FxHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        for &byte in rest {
            self.add(u64::from(byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_items_by_key_in_item_order() {
        let keys = [
            [1, 2, 3],
            [4, 5, 6],
            [1, 2, 3],
            [7, 8, 9],
            [4, 5, 6],
            [1, 2, 3],
        ];
        let buckets = Buckets::new(keys.iter());
        assert_eq!(buckets.with(0), &[0, 2, 5]);
        assert_eq!(buckets.with(5), &[0, 2, 5]);
        assert_eq!(buckets.with(4), &[1, 4]);
        assert_eq!(buckets.with(3), &[3]);
        let groups: Vec<&[usize]> = buckets.groups().collect();
        assert_eq!(groups, [&[0, 2, 5][..], &[1, 4], &[3]]);
    }
}
