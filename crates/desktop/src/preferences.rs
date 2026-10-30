//! The player's choices between launches (`src/game/player-preferences.ts`): one JSON
//! file in the platform's settings folder, beside the personal bests; in a page, the
//! same JSON in its `localStorage`, the first time carried over from the keys the old
//! page kept there (its personal bests, under the same keys, need no carrying).

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::game_options::GameOptions;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{GameMode, Team, VehicleKind};
use sloppy_render::quality::GraphicsSetting;

/// The folder of the settings and records files: `~/Library/Application Support/Sloppy
/// Tanks` on macOS, `%APPDATA%\Sloppy Tanks` on Windows.
#[cfg(not(target_arch = "wasm32"))]
pub fn data_directory() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("Sloppy Tanks"))
}

/// Battle Setup's choices are the engine's own ids. One the file names wrongly (an old
/// or edited file) takes its default rather than reaching the engine or losing the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// A tank a player may drive.
    #[serde(deserialize_with = "playable_tank")]
    pub tank: VehicleKind,
    /// The format for the standard maps; an extra level plays its own team battle.
    #[serde(deserialize_with = "or_default")]
    pub game_mode: GameMode,
    #[serde(deserialize_with = "or_default")]
    pub map: MapId,
    #[serde(deserialize_with = "or_default")]
    pub difficulty: Difficulty,
    pub first_person: bool,
    pub zoom: Option<f64>,
    pub volume: f64,
    pub tank_speed: f64,
    pub bullet_speed: f64,
    #[serde(
        serialize_with = "graphics_name",
        deserialize_with = "graphics_setting"
    )]
    pub graphics: GraphicsSetting,
    pub player_name: String,
    /// Debug tools: the extra levels and Stats for nerds (N in battle), as `?debug`
    /// gives the page and `--debug` the app.
    pub debug: bool,
    pub fullscreen: bool,
    #[serde(deserialize_with = "or_default")]
    pub touch: TouchSetting,
}

/// Whether the touch controls show (`src/game/touch-mode.ts`): Auto once a finger
/// touches the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchSetting {
    #[default]
    Auto,
    On,
    Off,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            tank: VehicleKind::Balanced,
            game_mode: GameMode::Team,
            map: MapId::Village,
            difficulty: Difficulty::Normal,
            first_person: false,
            zoom: None,
            volume: sloppy_web::catalog::DEFAULT_VOLUME,
            tank_speed: 1.0,
            bullet_speed: 1.0,
            graphics: GraphicsSetting::Auto,
            player_name: String::new(),
            debug: false,
            fullscreen: false,
            touch: TouchSetting::Auto,
        }
    }
}

/// The saved settings' text.
#[cfg(not(target_arch = "wasm32"))]
fn settings_path() -> Option<PathBuf> {
    data_directory().map(|directory| directory.join("settings.json"))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_settings() -> Option<String> {
    std::fs::read_to_string(settings_path()?).ok()
}

/// Keep the settings' text; a folder that cannot be written keeps them for this run.
#[cfg(not(target_arch = "wasm32"))]
fn write_settings(text: &str) {
    let Some(path) = settings_path() else { return };
    if let Some(directory) = path.parent() {
        let _ = std::fs::create_dir_all(directory);
    }
    let _ = std::fs::write(path, text);
}

/// The page's `localStorage` key, apart from the old page's own choices.
#[cfg(any(target_arch = "wasm32", test))]
const SETTINGS_KEY: &str = "sloppy-shell-settings";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The settings' text, written once when it carries the old page's choices over.
#[cfg(target_arch = "wasm32")]
fn read_settings() -> Option<String> {
    let storage = local_storage()?;
    let (text, carried) = page_settings(|key| storage.get_item(key).ok().flatten())?;
    if carried {
        write_settings(&text);
    }
    Some(text)
}

/// The settings' text from a page's `localStorage` as `saved` reads it, and whether it
/// is new: the record when there is one, which always wins, else the old page's choices
/// carried over into one. The old keys stay as they are.
#[cfg(any(target_arch = "wasm32", test))]
fn page_settings(saved: impl Fn(&str) -> Option<String>) -> Option<(String, bool)> {
    if let Some(text) = saved(SETTINGS_KEY) {
        return Some((text, false));
    }
    let carried = Preferences::from_page_keys(saved)?;
    Some((serde_json::to_string_pretty(&carried).ok()?, true))
}

/// The old page's choices, each under `sloppy-` and its name
/// (`src/game/player-preferences.ts`).
#[cfg(any(target_arch = "wasm32", test))]
const PAGE_KEYS: [&str; 13] = [
    "tank",
    "game-mode",
    "map",
    "difficulty",
    "camera",
    "zoom",
    "volume",
    "tank-speed",
    "bullet-speed",
    "graphics",
    "player-name",
    "debug",
    "touch",
];

/// Keep the settings' text; private browsing or a full quota keeps them for this run.
#[cfg(target_arch = "wasm32")]
fn write_settings(text: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(SETTINGS_KEY, text);
    }
}

impl Preferences {
    /// The saved choices; malformed or missing ones take their defaults.
    pub fn load() -> Self {
        read_settings().map_or_else(Self::default, |text| Self::read(&text))
    }

    fn read(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// The old page's choices as `saved` reads them by key ([`PAGE_KEYS`]), or `None`
    /// when it kept none. A value the page could not have written takes its default, as
    /// the page read it; the app has no fullscreen choice there.
    #[cfg(any(target_arch = "wasm32", test))]
    fn from_page_keys(saved: impl Fn(&str) -> Option<String>) -> Option<Self> {
        use sloppy_core::net::protocol::{DIFFICULTIES, PLAYER_KINDS};
        use sloppy_core::sim::speed_tuning::{MAX_SPEED_SCALE, MIN_SPEED_SCALE};

        /// The choice `text` names in `table`.
        fn named<T: Copy>(text: Option<String>, table: &[(&str, T)]) -> Option<T> {
            let text = text?;
            table
                .iter()
                .find(|(id, _)| *id == text)
                .map(|(_, choice)| *choice)
        }

        let value = |name: &str| saved(&format!("sloppy-{name}"));
        if PAGE_KEYS.iter().all(|name| value(name).is_none()) {
            return None;
        }
        let is = |name: &str, text: &str| value(name).as_deref() == Some(text);
        let number = |name: &str| {
            value(name)
                .and_then(|text| text.trim().parse::<f64>().ok())
                .filter(|number| number.is_finite())
        };
        let speed = |name: &str| {
            number(name).map_or(1.0, |scale| scale.clamp(MIN_SPEED_SCALE, MAX_SPEED_SCALE))
        };
        let defaults = Self::default();
        Some(Self {
            tank: named(value("tank"), &PLAYER_KINDS).unwrap_or(defaults.tank),
            game_mode: if is("game-mode", "solo") {
                GameMode::Solo
            } else {
                GameMode::Team
            },
            map: value("map")
                .and_then(|id| MapId::parse(&id))
                .unwrap_or(defaults.map),
            difficulty: named(value("difficulty"), &DIFFICULTIES).unwrap_or(defaults.difficulty),
            first_person: is("camera", "first-person"),
            zoom: number("zoom"),
            volume: number("volume").map_or(defaults.volume, |volume| volume.clamp(0.0, 1.0)),
            tank_speed: speed("tank-speed"),
            bullet_speed: speed("bullet-speed"),
            graphics: GraphicsSetting::parse(&value("graphics").unwrap_or_default()),
            player_name: value("player-name").unwrap_or_default(),
            debug: is("debug", "on"),
            fullscreen: false,
            touch: match value("touch").as_deref() {
                Some("on") => TouchSetting::On,
                Some("off") => TouchSetting::Off,
                _ => TouchSetting::Auto,
            },
        })
    }

    /// Save the choices.
    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            write_settings(&text);
        }
    }

    /// Keep the view the player chose (`savedCameraPreferences`).
    pub fn save_camera(&mut self, camera: sloppy_web::CameraPreferences) {
        self.first_person = camera.first_person;
        self.zoom = Some(camera.zoom);
        self.save();
    }

    /// The choices as single player plays them, on `human_team`: an extra level only
    /// where extra levels are offered, and as its own team battle.
    pub fn game_options(&self, human_team: Team, extra_levels: bool) -> GameOptions {
        GameOptions {
            human_kind: self.tank,
            human_team,
            game_mode: self.game_mode,
            map_mode: self.map,
            difficulty: self.difficulty,
        }
        .offered(extra_levels)
    }
}

/// A saved choice, or its default when the file names one this build does not know.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}

fn playable_tank<'de, D: Deserializer<'de>>(deserializer: D) -> Result<VehicleKind, D::Error> {
    let kind: VehicleKind = or_default(deserializer)?;
    Ok(if VehicleKind::PLAYABLE.contains(&kind) {
        kind
    } else {
        VehicleKind::default()
    })
}

fn graphics_name<S: Serializer>(
    setting: &GraphicsSetting,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(setting.name())
}

/// "auto", "high", "medium" or "low"; anything else is Auto.
fn graphics_setting<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<GraphicsSetting, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(GraphicsSetting::parse(value.as_str().unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_choices_read_back_and_a_bad_one_takes_its_default_alone() {
        let prefs = Preferences {
            tank: VehicleKind::Scout,
            game_mode: GameMode::Solo,
            map: MapId::Quarry,
            difficulty: Difficulty::Hard,
            graphics: GraphicsSetting::parse("low"),
            volume: 0.25,
            ..Preferences::default()
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert!(text.contains(r#""map":"quarry""#) && text.contains(r#""graphics":"low""#));
        assert_eq!(Preferences::read(&text), prefs);

        let edited = Preferences::read(
            r#"{"tank": "humvee", "gameMode": "duel", "map": "atlantis", "difficulty": 3,
                "graphics": "ultra", "volume": 0.25, "playerName": "Vlad"}"#,
        );
        assert_eq!(
            edited,
            Preferences {
                volume: 0.25,
                player_name: "Vlad".into(),
                ..Preferences::default()
            }
        );
        assert_eq!(Preferences::read("not json"), Preferences::default());
    }

    /// A page's `localStorage`, as `page_settings` reads it.
    fn storage(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn settings(storage: &std::collections::HashMap<String, String>) -> Option<(String, bool)> {
        page_settings(|key| storage.get(key).cloned())
    }

    #[test]
    fn the_old_pages_choices_carry_over_once_and_stay_behind() {
        let mut saved = storage(&[
            ("sloppy-tank", "heavy"),
            ("sloppy-game-mode", "solo"),
            ("sloppy-map", "stress-test"),
            ("sloppy-difficulty", "hard"),
            ("sloppy-camera", "first-person"),
            ("sloppy-zoom", "41.5"),
            ("sloppy-volume", "0.25"),
            ("sloppy-tank-speed", "1.5"),
            ("sloppy-bullet-speed", "0.75"),
            ("sloppy-graphics", "medium"),
            ("sloppy-player-name", "Iron Jack"),
            ("sloppy-debug", "on"),
            ("sloppy-touch", "off"),
            ("sloppy-records-v1:team:Pine Village:normal", "{}"),
        ]);
        let (text, carried) = settings(&saved).expect("the old choices");
        assert!(carried);
        assert_eq!(
            Preferences::read(&text),
            Preferences {
                tank: VehicleKind::Heavy,
                game_mode: GameMode::Solo,
                map: MapId::StressTest,
                difficulty: Difficulty::Hard,
                first_person: true,
                zoom: Some(41.5),
                volume: 0.25,
                tank_speed: 1.5,
                bullet_speed: 0.75,
                graphics: GraphicsSetting::parse("medium"),
                player_name: "Iron Jack".into(),
                debug: true,
                fullscreen: false,
                touch: TouchSetting::Off,
            }
        );
        // Written once, the record is what every later visit reads, whatever the old
        // keys still say.
        saved.insert(SETTINGS_KEY.into(), text.clone());
        saved.insert("sloppy-tank".into(), "scout".into());
        assert_eq!(settings(&saved), Some((text, false)));
        assert!(saved.contains_key("sloppy-camera"));
    }

    #[test]
    fn a_newer_record_wins_even_over_old_keys_and_when_unreadable() {
        let saved = storage(&[(SETTINGS_KEY, "not json"), ("sloppy-tank", "scout")]);
        assert_eq!(settings(&saved), Some(("not json".into(), false)));
        assert_eq!(Preferences::read("not json"), Preferences::default());
    }

    #[test]
    fn a_page_that_kept_nothing_has_nothing_to_carry_over() {
        assert_eq!(settings(&storage(&[])), None);
        // Only the old page's choices count, not its records or seats.
        let unrelated = storage(&[("sloppy-records-v1:solo:Harbor Havoc:easy", "{}")]);
        assert_eq!(settings(&unrelated), None);
    }

    /// Some old choices, some the page could not have written: those take their
    /// defaults alone, as the page read them.
    #[test]
    fn partial_or_malformed_old_choices_take_their_defaults_alone() {
        let saved = storage(&[
            ("sloppy-tank", "humvee"),
            ("sloppy-game-mode", "duel"),
            ("sloppy-map", "atlantis"),
            ("sloppy-difficulty", "insane"),
            ("sloppy-camera", "overhead"),
            ("sloppy-zoom", " "),
            ("sloppy-volume", "loud"),
            ("sloppy-tank-speed", "9"),
            ("sloppy-bullet-speed", "NaN"),
            ("sloppy-graphics", "ultra"),
            ("sloppy-debug", "yes"),
            ("sloppy-touch", "maybe"),
        ]);
        let (text, _) = settings(&saved).expect("the old choices");
        assert_eq!(
            Preferences::read(&text),
            Preferences {
                tank_speed: sloppy_core::sim::speed_tuning::MAX_SPEED_SCALE,
                ..Preferences::default()
            }
        );
        let only_zoom = storage(&[("sloppy-zoom", "52")]);
        let (text, _) = settings(&only_zoom).expect("the old zoom");
        assert_eq!(
            Preferences::read(&text),
            Preferences {
                zoom: Some(52.0),
                ..Preferences::default()
            }
        );
    }

    /// Each saved choice lands in its own field and the debug tools decide what is
    /// offered; what `offered` makes of an extra level is core's rule, tested there.
    #[test]
    fn saved_choices_reach_the_battle_with_the_debug_tools_deciding_what_is_offered() {
        let prefs = Preferences {
            tank: VehicleKind::Scout,
            game_mode: GameMode::Solo,
            map: MapId::StressTest,
            difficulty: Difficulty::Hard,
            ..Preferences::default()
        };
        let saved = GameOptions {
            human_kind: VehicleKind::Scout,
            human_team: Team::Red,
            game_mode: GameMode::Solo,
            map_mode: MapId::StressTest,
            difficulty: Difficulty::Hard,
        };
        for extra_levels in [false, true] {
            assert_eq!(
                prefs.game_options(Team::Red, extra_levels),
                saved.offered(extra_levels),
                "extra levels offered: {extra_levels}"
            );
        }
    }
}
