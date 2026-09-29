//! Operating-system randomness for room epochs, seat tokens and match seeds.

/// A random version 4 UUID in its usual text form, like `crypto.randomUUID()`.
pub fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("operating system randomness");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut text = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            text.push('-');
        }
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// A seat token or player id: two UUIDs, as the TypeScript server makes them.
pub fn token() -> String {
    uuid_v4() + &uuid_v4()
}

pub fn seed() -> u32 {
    getrandom::u32().expect("operating system randomness")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_have_the_version_4_layout() {
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'));
        assert!(matches!(&parts[3][..1], "8" | "9" | "a" | "b"));
        assert_ne!(uuid_v4(), id);
        assert_eq!(token().len(), 72);
    }
}
