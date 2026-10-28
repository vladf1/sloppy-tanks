//! The room Battle Setup chose and how to take a seat in it, and the name to play
//! under (`RoomSelection` and `preferredPlayerName` of the page).

use sloppy_core::net::protocol::{JoinChoice, MAX_PLAYER_NAME_LENGTH};
use sloppy_core::sim::bot_personalities::BOT_NAMES;

/// The room Battle Setup chose and how to take a seat in it (`RoomSelection`).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomSelection {
    pub room: String,
    /// The WebSocket address of the server that hosts it.
    pub server: String,
    /// The name, tank and team to join with, and the rules of a room this join creates.
    pub choice: JoinChoice,
}

/// `name` trimmed and cut to the longest a room takes, in UTF-16 units as the page's
/// `slice` and the room's reader count it.
pub fn player_name(name: &str) -> String {
    let mut units = 0;
    name.trim()
        .chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= MAX_PLAYER_NAME_LENGTH
        })
        .collect()
}

/// A name to play under: the saved one, cut to the longest a room takes, or a random
/// bot name (`preferredPlayerName`).
pub fn preferred_player_name(saved: &str, random: u64) -> String {
    let saved = player_name(saved);
    if saved.is_empty() {
        BOT_NAMES[(random % BOT_NAMES.len() as u64) as usize].to_owned()
    } else {
        saved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_name_wins_over_a_random_one() {
        assert_eq!(preferred_player_name("  Vlad ", 3), "Vlad");
        assert_eq!(preferred_player_name(&"x".repeat(30), 3).len(), 24);
        assert!(BOT_NAMES.contains(&preferred_player_name(" ", 12345).as_str()));
    }

    #[test]
    fn a_name_is_cut_where_the_room_counts_it_full() {
        // A character beyond the Basic Multilingual Plane takes two UTF-16 units, as
        // the room's reader counts.
        let shield = '\u{1F6E1}';
        let name = player_name(&shield.to_string().repeat(20));
        assert_eq!(name.chars().count(), 12);
        assert_eq!(
            player_name(&format!("{}{shield}", "a".repeat(23))),
            "a".repeat(23)
        );
    }
}
