//! Hash maps and sets keyed by a fixed hash (FxHash's multiply-rotate), for the
//! presentation's per-entity views. std's maps seed every map at random natively (and
//! by creation order in Wasm), so the order they visit and release entities, and with
//! it the renderer slots and draw classes those entities reuse, differed from run to
//! run: with a fixed hash a seeded match draws the same frames every time
//! (`render_frame_benchmark` compares them). It also hashes the entity ids the views
//! look up every frame in a multiply rather than SipHash's rounds. The keys are the
//! game's own ids, never untrusted input.

use std::hash::{BuildHasherDefault, Hasher};

/// Maps and sets with [`FixedHasher`]; make them with `default()`.
pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FixedHasher>>;
pub type HashSet<K> = std::collections::HashSet<K, BuildHasherDefault<FixedHasher>>;

/// FxHash (rustc's hasher): each word is mixed in with a rotate, xor and multiply.
#[derive(Clone, Copy, Default)]
pub struct FixedHasher(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FixedHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FixedHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
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
    fn maps_visit_entries_in_the_same_order_every_time() {
        let fill = || {
            let mut map = HashMap::default();
            for id in (0..200u32).rev() {
                map.insert(id * 7, id);
            }
            map.retain(|id, _| id % 3 != 0);
            map.into_iter().collect::<Vec<_>>()
        };
        assert_eq!(fill(), fill());
    }
}
