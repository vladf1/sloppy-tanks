//! Players' names in a room: two words from fixed lists, "Swift Falcon", or on the web
//! a name the player typed. A page or app draws one with [`player_name`] and rerolls it
//! on request. The room host accepts a join's drawn name ([`is_player_name`]), and a
//! typed one ([`typed_player_name`]) only where it seats [`NameRule::Typed`]; otherwise
//! it draws one, so a modified client cannot show other players a name of its own. The
//! App Store apps play on servers of their own that seat only drawn names
//! ([`NameRule::DrawnOnly`]), so they show no user-written text to moderate.
//!
//! The lists are curated as pairs, not only as words: every adjective must read as
//! harmless before every noun. Keep them free of colours that name people, body or
//! innuendo words, nationalities and anything the bots are called (their names are
//! one capitalised word or two, `BOT_NAMES`), so a name alone never passes for a bot.

use crate::sim::bot_personalities::BOT_NAMES;

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

/// The longest name a player may type: as long as the longest drawn one, so a drawn
/// name edited a little still plays, and with a room's " 99" suffix within
/// `MAX_PLAYER_NAME_LENGTH`.
pub const MAX_TYPED_NAME: usize = 2 * MAX_WORD + 1;

/// Which names a room host seats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameRule {
    /// Drawn names and names players typed (the web's servers).
    Typed,
    /// Drawn names only: a typed name gives way to a drawn one (the App Store apps').
    DrawnOnly,
}

/// Whether a player may type `character` in a name: ASCII letters and digits, spaces and
/// a little punctuation. Every face of the interface draws them, and no invisible,
/// combining or right-to-left character can disguise a name.
pub fn typed_name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.' | '\'')
}

/// The name `text` gives, trimmed and with its spaces single, when a player may play
/// under it: at most [`MAX_TYPED_NAME`] characters a player may type, at least one a
/// letter or digit, and not a bot's name, which would pass the player off as a bot.
pub fn typed_player_name(text: &str) -> Option<String> {
    let mut name = String::with_capacity(text.len());
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(word);
    }
    let fits = (1..=MAX_TYPED_NAME).contains(&name.len())
        && name.chars().all(typed_name_character)
        && name.chars().any(|character| character.is_ascii_alphanumeric());
    let bot = BOT_NAMES
        .iter()
        .any(|bot| bot.eq_ignore_ascii_case(&name));
    (fits && !bot).then_some(name)
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

    #[test]
    fn a_typed_name_is_trimmed_and_kept_within_its_characters() {
        assert_eq!(typed_player_name("  Vlad   the  Tank "), Some("Vlad the Tank".into()));
        assert_eq!(typed_player_name("O'Neil-2_x."), Some("O'Neil-2_x.".into()));
        assert_eq!(typed_player_name("Swift Falcon"), Some("Swift Falcon".into()));
        let longest = "A".repeat(MAX_TYPED_NAME);
        assert_eq!(typed_player_name(&longest), Some(longest.clone()));
        assert!(longest.len() + " 99".len() <= MAX_PLAYER_NAME_LENGTH);
        for name in [
            "",
            "   ",
            "...",
            "- _ '",
            &"A".repeat(MAX_TYPED_NAME + 1),
            "<b>bold</b>",
            "Tab\there",
            "Vlad\u{200b}",
            "Zoë",
            "\u{202e}reversed",
        ] {
            assert_eq!(typed_player_name(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_typed_name_never_takes_a_bots() {
        for name in BOT_NAMES {
            assert_eq!(typed_player_name(name), None, "{name}");
            assert_eq!(typed_player_name(&name.to_ascii_lowercase()), None, "{name}");
        }
    }
}
