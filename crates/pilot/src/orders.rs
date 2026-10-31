//! What a pilot is told to do: its mission (fight, hunt a player, escort a player), how
//! well it plays and what it drives. The command line and the bots Worker both write
//! these as words (`hunt:vlad`, `hard`, `heavy`), read here.

use serde::{Deserialize, Serialize};
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::types::VehicleKind;

/// Who a pilot fights for and against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mission {
    /// Plays like a fill bot on whichever team the room gives it.
    #[default]
    Skirmish,
    /// Takes the other team from the named player and goes after them first wherever they
    /// are, like a bot that keeps hearing their shells; with no name, the nearest player
    /// (not fill bot) on the other team.
    Hunt(Option<String>),
    /// Takes the named player's team, follows them, fires on what they fire on and turns
    /// on whoever hits them.
    Escort(String),
}

impl Mission {
    /// `skirmish`, `hunt`, `hunt:NAME` or `escort:NAME` (`ally:NAME` too).
    pub fn parse(text: &str) -> Result<Self, String> {
        let (word, name) = match text.split_once(':') {
            Some((word, name)) => (word.trim(), Some(name.trim())),
            None => (text.trim(), None),
        };
        let name = name.filter(|name| !name.is_empty()).map(str::to_string);
        match (word.to_ascii_lowercase().as_str(), name) {
            ("skirmish" | "", None) => Ok(Mission::Skirmish),
            ("hunt", name) => Ok(Mission::Hunt(name)),
            ("escort" | "ally", Some(name)) => Ok(Mission::Escort(name)),
            ("escort" | "ally", None) => Err("escort needs a player name: escort:NAME".into()),
            _ => Err(format!(
                "Unknown mission {text:?}: skirmish, hunt, hunt:NAME or escort:NAME"
            )),
        }
    }

    /// The player this mission is about, if it names one.
    pub fn player(&self) -> Option<&str> {
        match self {
            Mission::Skirmish | Mission::Hunt(None) => None,
            Mission::Hunt(Some(name)) | Mission::Escort(name) => Some(name),
        }
    }

    /// The words [`Mission::parse`] reads back.
    pub fn describe(&self) -> String {
        match self {
            Mission::Skirmish => "skirmish".into(),
            Mission::Hunt(None) => "hunt".into(),
            Mission::Hunt(Some(name)) => format!("hunt:{name}"),
            Mission::Escort(name) => format!("escort:{name}"),
        }
    }
}

impl Serialize for Mission {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.describe())
    }
}

impl<'de> Deserialize<'de> for Mission {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Mission::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A player name matches regardless of case and surrounding spaces, as people type it.
pub fn same_player(name: &str, wanted: &str) -> bool {
    name.trim().eq_ignore_ascii_case(wanted.trim())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Orders {
    pub mission: Mission,
    /// The bot brain's reaction, aim and reload tuning, as the room difficulty sets it for
    /// fill bots.
    pub skill: Difficulty,
    pub kind: VehicleKind,
    /// The bot role it fights as (range, sight, cadence, ammunition); by default the
    /// fill bots' role for its chassis, and support while escorting.
    pub personality: Option<BotPersonality>,
    /// Seconds a pilot that hosts the room waits in the lobby or results before starting
    /// the next battle, so a room it created never strands the people in it.
    pub host_start_seconds: f64,
}

impl Default for Orders {
    fn default() -> Self {
        Self {
            mission: Mission::Skirmish,
            skill: Difficulty::Normal,
            kind: VehicleKind::Balanced,
            personality: None,
            host_start_seconds: DEFAULT_HOST_START_SECONDS,
        }
    }
}

/// The traffic bots' lobby wait (`HOST_START_DELAY_MS` in `bots/bot-player.ts`).
pub const DEFAULT_HOST_START_SECONDS: f64 = 15.0;

impl Orders {
    /// The bot role the brain plays: the chosen one, or the one that fits the mission and
    /// chassis.
    pub fn role(&self) -> BotPersonality {
        if let Some(personality) = self.personality {
            return personality;
        }
        match (&self.mission, self.kind) {
            // Support bots escort the nearest ally; the pilot makes the leader that ally.
            (Mission::Escort(_), _) => BotPersonality::Support,
            (_, VehicleKind::Scout) => BotPersonality::Scout,
            (_, VehicleKind::Heavy) => BotPersonality::Heavy,
            (_, VehicleKind::Balanced | VehicleKind::Humvee) => BotPersonality::Guard,
        }
    }
}

pub fn parse_skill(text: &str) -> Result<Difficulty, String> {
    Difficulty::ALL
        .into_iter()
        .find(|skill| skill.as_str().eq_ignore_ascii_case(text.trim()))
        .ok_or_else(|| format!("Unknown skill {text:?}: easy, normal or hard"))
}

pub fn parse_kind(text: &str) -> Result<VehicleKind, String> {
    VehicleKind::PLAYABLE
        .into_iter()
        .find(|kind| kind.as_str().eq_ignore_ascii_case(text.trim()))
        .ok_or_else(|| format!("Unknown tank {text:?}: scout, balanced or heavy"))
}

pub fn parse_personality(text: &str) -> Result<BotPersonality, String> {
    BotPersonality::ALL
        .into_iter()
        .find(|role| role.as_str().eq_ignore_ascii_case(text.trim()))
        .ok_or_else(|| {
            let names: Vec<&str> = BotPersonality::ALL
                .iter()
                .map(|role| role.as_str())
                .collect();
            format!("Unknown style {text:?}: {}", names.join(", "))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missions_read_as_typed() {
        assert_eq!(Mission::parse("skirmish"), Ok(Mission::Skirmish));
        assert_eq!(Mission::parse(""), Ok(Mission::Skirmish));
        assert_eq!(Mission::parse("hunt"), Ok(Mission::Hunt(None)));
        assert_eq!(
            Mission::parse("Hunt: vlad "),
            Ok(Mission::Hunt(Some("vlad".into())))
        );
        assert_eq!(
            Mission::parse("ally:Vlad F"),
            Ok(Mission::Escort("Vlad F".into()))
        );
        assert_eq!(
            Mission::parse("escort:vlad"),
            Ok(Mission::Escort("vlad".into()))
        );
        assert!(Mission::parse("escort").is_err());
        assert!(Mission::parse("dance:vlad").is_err());
        for mission in [
            Mission::Skirmish,
            Mission::Hunt(None),
            Mission::Hunt(Some("vlad".into())),
            Mission::Escort("vlad".into()),
        ] {
            assert_eq!(Mission::parse(&mission.describe()), Ok(mission));
        }
    }

    #[test]
    fn orders_read_from_json_with_defaults() {
        let orders: Orders = serde_json::from_str(
            r#"{"mission":"escort:vlad","skill":"hard","kind":"heavy","personality":"sniper"}"#,
        )
        .unwrap();
        assert_eq!(orders.mission, Mission::Escort("vlad".into()));
        assert_eq!(orders.skill, Difficulty::Hard);
        assert_eq!(orders.kind, VehicleKind::Heavy);
        assert_eq!(orders.role(), BotPersonality::Sniper);
        assert_eq!(orders.host_start_seconds, DEFAULT_HOST_START_SECONDS);
        let plain: Orders = serde_json::from_str("{}").unwrap();
        assert_eq!(plain, Orders::default());
        assert!(serde_json::from_str::<Orders>(r#"{"mission":"dance"}"#).is_err());
    }

    #[test]
    fn roles_follow_the_mission_and_chassis() {
        let role = |mission, kind| {
            Orders {
                mission,
                kind,
                ..Orders::default()
            }
            .role()
        };
        assert_eq!(
            role(Mission::Escort("vlad".into()), VehicleKind::Heavy),
            BotPersonality::Support
        );
        assert_eq!(
            role(Mission::Hunt(None), VehicleKind::Scout),
            BotPersonality::Scout
        );
        assert_eq!(
            role(Mission::Skirmish, VehicleKind::Balanced),
            BotPersonality::Guard
        );
        assert_eq!(
            role(Mission::Skirmish, VehicleKind::Heavy),
            BotPersonality::Heavy
        );
    }

    #[test]
    fn words_parse_case_insensitively() {
        assert_eq!(parse_skill("HARD"), Ok(Difficulty::Hard));
        assert_eq!(parse_kind("Scout"), Ok(VehicleKind::Scout));
        assert!(parse_kind("humvee").is_err());
        assert_eq!(
            parse_personality("minelayer"),
            Ok(BotPersonality::Minelayer)
        );
        assert!(same_player(" Vlad ", "vlad"));
        assert!(!same_player("vladimir", "vlad"));
    }
}
