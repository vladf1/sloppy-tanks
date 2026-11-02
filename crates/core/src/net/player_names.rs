//! Players' names in a room: two words from fixed lists, "Swift Falcon". Nobody types
//! a name other players read, so a room shows no user-written text to moderate. A page
//! or app draws one with [`player_name`] and rerolls it on request; the room host
//! accepts a join's name only if [`is_player_name`] holds, and draws one otherwise, so
//! a modified client cannot show other players a name of its own.
//!
//! The lists are curated as pairs, not only as words: every adjective must read as
//! harmless before every noun. Keep them free of colours that name people, body or
//! innuendo words, nationalities and anything the bots are called (their names are
//! one capitalised word or two, `BOT_NAMES`), so a name alone never passes for a bot.

/// First words. Each at most [`MAX_WORD`] characters.
pub const ADJECTIVES: [&str; 64] = [
    "Amber", "Arctic", "Armored", "Autumn", "Bold", "Brave", "Bronze", "Canyon", "Chrome",
    "Cobalt", "Coastal", "Daring", "Desert", "Dusty", "Eager", "Emerald", "Fearless",
    "Fierce", "Foggy", "Frosty", "Gallant", "Golden", "Granite", "Grizzled", "Hasty",
    "Highland", "Humble", "Icy", "Jade", "Jolly", "Keen", "Lucky", "Lunar", "Meadow",
    "Mighty", "Misty", "Muddy", "Nimble", "Noble", "Northern", "Olive", "Plucky", "Polar",
    "Prairie", "Quick", "Quiet", "Rapid", "Rocky", "Rowdy", "Rugged", "Sandy", "Silent",
    "Silver", "Snowy", "Speedy", "Steady", "Steel", "Stormy", "Sturdy", "Swift", "Tidal",
    "Valiant", "Wild", "Windy",
];

/// Second words. Each at most [`MAX_WORD`] characters.
pub const NOUNS: [&str; 64] = [
    "Anvil", "Avalanche", "Bison", "Blizzard", "Bobcat", "Boulder", "Bulldog", "Captain",
    "Cheetah", "Comet", "Condor", "Convoy", "Corporal", "Puma", "Cyclone", "Eagle",
    "Elk", "Ember", "Falcon", "Ferret", "Fox", "Gecko", "Glacier", "Grizzly", "Gunner",
    "Hawk", "Heron", "Hornet", "Husky", "Jaguar", "Kestrel", "Leopard", "Lynx", "Marlin",
    "Marshal", "Meteor", "Mongoose", "Moose", "Mustang", "Nomad", "Orca", "Osprey",
    "Beagle", "Panther", "Pioneer", "Piston", "Ranger", "Raven", "Rhino", "Rover",
    "Scorpion", "Sentry", "Sergeant", "Shark", "Sprocket", "Coyote", "Summit", "Tornado",
    "Tortoise", "Typhoon", "Voyager", "Walrus", "Warden", "Wolverine",
];

/// The longest word in either list, so a name and a room's " 2" suffix stay within
/// `MAX_PLAYER_NAME_LENGTH`.
pub const MAX_WORD: usize = 10;

/// The name `random` picks: its low bits choose the adjective, the next ones the noun.
pub fn player_name(random: u64) -> String {
    let adjective = ADJECTIVES[(random % ADJECTIVES.len() as u64) as usize];
    let noun = NOUNS[(random / ADJECTIVES.len() as u64 % NOUNS.len() as u64) as usize];
    format!("{adjective} {noun}")
}

/// Whether `name` is one [`player_name`] can draw: an adjective, one space and a noun.
pub fn is_player_name(name: &str) -> bool {
    name.split_once(' ').is_some_and(|(adjective, noun)| {
        ADJECTIVES.contains(&adjective) && NOUNS.contains(&noun)
    })
}

/// A hash of `text` for drawing a name from a random token (FNV-1a).
pub fn name_seed(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::MAX_PLAYER_NAME_LENGTH;
    use crate::sim::bot_personalities::BOT_NAMES;
    use std::collections::BTreeSet;

    #[test]
    fn every_pair_is_a_name_and_fits_a_room_with_its_suffix() {
        for random in 0..(ADJECTIVES.len() * NOUNS.len()) as u64 {
            let name = player_name(random);
            assert!(is_player_name(&name), "{name}");
            assert!(name.len() + " 99".len() <= MAX_PLAYER_NAME_LENGTH, "{name}");
        }
        for word in ADJECTIVES.iter().chain(&NOUNS) {
            assert!(word.len() <= MAX_WORD, "{word}");
            assert!(word.chars().all(|c| c.is_ascii_alphabetic()), "{word}");
        }
    }

    #[test]
    fn every_pair_is_distinct_and_reachable() {
        let names: BTreeSet<String> = (0..(ADJECTIVES.len() * NOUNS.len()) as u64)
            .map(player_name)
            .collect();
        assert_eq!(names.len(), ADJECTIVES.len() * NOUNS.len());
        let words: BTreeSet<&str> = ADJECTIVES.iter().chain(&NOUNS).copied().collect();
        assert_eq!(words.len(), ADJECTIVES.len() + NOUNS.len(), "no word repeats");
    }

    #[test]
    fn no_player_name_shares_a_word_with_a_bot() {
        let bot_words: BTreeSet<String> = BOT_NAMES
            .iter()
            .flat_map(|name| name.split(' '))
            .map(str::to_ascii_uppercase)
            .collect();
        for word in ADJECTIVES.iter().chain(&NOUNS) {
            assert!(!bot_words.contains(&word.to_ascii_uppercase()), "{word}");
        }
        for name in BOT_NAMES {
            assert!(!is_player_name(name), "{name}");
        }
    }

    #[test]
    fn only_exact_names_pass() {
        assert!(is_player_name("Swift Falcon"));
        for name in [
            "swift falcon",
            "Swift  Falcon",
            " Swift Falcon",
            "Swift Falcon ",
            "Swift Falcon 2",
            "Falcon Swift",
            "Swift",
            "",
            "Swift Falcon\u{200b}",
        ] {
            assert!(!is_player_name(name), "{name:?}");
        }
    }
}
