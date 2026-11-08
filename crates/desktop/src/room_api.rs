//! The room Battle Setup chose and how to take a seat in it, and the name to play
//! under (`RoomSelection` and `preferredPlayerName` of the page).

use sloppy_core::net::player_names::{is_player_name, player_name, typed_player_name};
use sloppy_core::net::protocol::JoinChoice;

/// The room Battle Setup chose and how to take a seat in it (`RoomSelection`).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomSelection {
    pub room: String,
    /// The WebSocket address of the server that hosts it.
    pub server: String,
    /// The name, tank and team to join with, and the rules of a room this join creates.
    pub choice: JoinChoice,
}

/// Whether a player may type the name they play under. The App Store apps offer only
/// drawn names: their servers seat no other (`NameRule::DrawnOnly`), so the apps show no
/// text a player wrote.
pub const TYPED_NAMES: bool = !cfg!(feature = "app-store");

/// A name to play under: `saved` when it is a drawn name or, where players type their
/// names, one they may type (trimmed); else one `random` draws. A bot's name, or an
/// empty or unplayable one, gives way, since a room would seat it under a drawn one
/// anyway.
pub fn preferred_player_name(saved: &str, random: u64) -> String {
    if is_player_name(saved) {
        return saved.to_owned();
    }
    TYPED_NAMES
        .then(|| typed_player_name(saved))
        .flatten()
        .unwrap_or_else(|| player_name(random))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::bot_personalities::BOT_NAMES;

    #[test]
    fn a_saved_drawn_name_wins_over_a_random_one() {
        assert_eq!(preferred_player_name("Swift Falcon", 3), "Swift Falcon");
        assert_ne!(player_name(3), "Swift Falcon");
    }

    #[test]
    fn an_empty_or_bot_name_gives_way_to_a_drawn_one() {
        for saved in ["", " ", "<b>", BOT_NAMES[0]] {
            let name = preferred_player_name(saved, 12345);
            assert_eq!(name, player_name(12345), "{saved:?}");
            assert!(is_player_name(&name));
        }
    }

    #[test]
    fn a_typed_name_stays_trimmed_unless_the_app_store_app_draws_names_only() {
        for (saved, typed) in [
            ("Vlad", "Vlad"),
            (" Swift  Falcon ", "Swift Falcon"),
            ("Swift Falcon 2", "Swift Falcon 2"),
        ] {
            let name = preferred_player_name(saved, 12345);
            if TYPED_NAMES {
                assert_eq!(name, typed, "{saved:?}");
            } else {
                assert_eq!(name, player_name(12345), "{saved:?}");
            }
        }
    }
}
