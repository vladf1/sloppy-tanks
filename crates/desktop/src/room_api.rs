//! The room Battle Setup chose and how to take a seat in it, and the name to play
//! under (`RoomSelection` and `preferredPlayerName` of the page).

use sloppy_core::net::player_names::{is_player_name, player_name};
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

/// A name to play under: the saved one when it is a drawn name, else one `random` draws.
/// A name typed before names were drawn, or a bot's, gives way, since a room would seat
/// it under a drawn one anyway.
pub fn preferred_player_name(saved: &str, random: u64) -> String {
    if is_player_name(saved) {
        saved.to_owned()
    } else {
        player_name(random)
    }
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
    fn a_typed_or_bot_name_gives_way_to_a_drawn_one() {
        for saved in [
            "",
            " ",
            "Vlad",
            " Swift Falcon ",
            "Swift Falcon 2",
            BOT_NAMES[0],
        ] {
            let name = preferred_player_name(saved, 12345);
            assert_eq!(name, player_name(12345), "{saved:?}");
            assert!(is_player_name(&name));
        }
    }
}
