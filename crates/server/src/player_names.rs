//! Offensive player names, masked before a room's host sees them.
//!
//! Strangers in a public room read each other's typed names in the lobby, the
//! scoreboard and the kill feed. A join whose name is profane, a slur or sexual,
//! leetspeak and spaced or stretched spellings included, keeps its seat under a
//! neutral callsign instead of being refused. The word lists (rustrict's) live only
//! here, so neither the page's Wasm nor the content version clients must match grows
//! with them. Bot names are generated and never pass through a join.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::sync::Once;

use rustrict::{Censor, Trie, Type};
use serde_json::Value;

/// A masked player's name before its number: `Tanker 3`. At most 17 characters
/// with any `u32`, well inside `MAX_PLAYER_NAME_LENGTH`.
const CALLSIGN_PREFIX: &str = "Tanker ";

/// Ordinary names that contain a listed word and that rustrict's own exceptions
/// (Scunthorpe, Dickens, cocktail and thousands more) miss.
const ORDINARY_NAMES: &[&str] = &[
    "hancock",
    "hitchcock",
    "shitake",
    "shiitake",
    "matsushita",
    "peniston",
    "fagan",
];

static ORDINARY_NAMES_ADDED: Once = Once::new();

/// Adds [`ORDINARY_NAMES`] to rustrict's word trie, building it (about 20 MB, 15 ms)
/// on the first call. The server calls this before it listens, so no room's first
/// join pays for it.
pub fn prepare() {
    ORDINARY_NAMES_ADDED.call_once(|| {
        // SAFETY: the trie is changed only here, inside `call_once`, and every reader
        // goes through `is_offensive`, which waits for this to finish first.
        let trie = unsafe { Trie::customize_default() };
        for name in ORDINARY_NAMES {
            trie.set(name, Type::NONE);
        }
    });
}

/// Whether a name may not be shown to other players: moderate or severe profanity,
/// slurs or sexual terms, or severe meanness ("kys"). Mild words (damn, idiot) pass,
/// as do identity words the lists mark mildly offensive, which ordinary names carry.
pub fn is_offensive(name: &str) -> bool {
    prepare();
    let offensive = (Type::PROFANE | Type::OFFENSIVE | Type::SEXUAL) & Type::MODERATE_OR_HIGHER
        | Type::MEAN & Type::SEVERE;
    Censor::from_str(name).analyze().is(offensive)
}

/// One room's masking: the callsign numbers its joins have used, so two masked
/// players, or a masked player and someone who typed a callsign, never share one.
#[derive(Debug, Default)]
pub struct Callsigns {
    taken: BTreeSet<u32>,
}

impl Callsigns {
    /// The message to hand the host: `text` itself, unless it is a join whose name is
    /// offensive, which comes back with a callsign in the name's place. Malformed
    /// messages pass unchanged for the host to refuse.
    pub fn mask_join<'a>(&mut self, text: &'a str) -> Cow<'a, str> {
        // A join spells "join" in its type, or escapes it; inputs, which are most
        // messages, do neither and skip the parse.
        if !text.contains("join") && !text.contains('\\') {
            return Cow::Borrowed(text);
        }
        let Ok(Value::Object(mut message)) = serde_json::from_str::<Value>(text) else {
            return Cow::Borrowed(text);
        };
        if message.get("type").and_then(Value::as_str) != Some("join") {
            return Cow::Borrowed(text);
        }
        let Some(name) = message.get("name").and_then(Value::as_str) else {
            return Cow::Borrowed(text);
        };
        if let Some(number) = callsign_number(name) {
            self.taken.insert(number);
        }
        if !is_offensive(name) {
            return Cow::Borrowed(text);
        }
        let mut number = 1;
        while !self.taken.insert(number) {
            number += 1;
        }
        message.insert(
            "name".into(),
            Value::String(format!("{CALLSIGN_PREFIX}{number}")),
        );
        Cow::Owned(Value::Object(message).to_string())
    }
}

/// The number of a name typed as a callsign, as the host will trim it.
fn callsign_number(name: &str) -> Option<u32> {
    name.trim().strip_prefix(CALLSIGN_PREFIX)?.parse().ok()
}
