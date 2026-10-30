//! The player's choices between launches (`src/game/player-preferences.ts`): one JSON
//! file in the platform's settings folder, beside the personal bests; in a page, the
//! same JSON in its `localStorage`.

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
    /// A page's touch controls (`touch-mode.ts`); see [`crate::touch::screen`].
    #[serde(deserialize_with = "or_default")]
    pub touch: TouchSetting,
}

/// Settings' Touch controls: Auto shows them on a touch screen (the browser says so, or
/// a finger touches the page); On and Off hold whatever the screen is.
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

/// The page's `localStorage` key, apart from the page shell's own choices.
#[cfg(target_arch = "wasm32")]
const SETTINGS_KEY: &str = "sloppy-shell-settings";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
fn read_settings() -> Option<String> {
    local_storage()?.get_item(SETTINGS_KEY).ok().flatten()
}

/// Keep the settings' text; private browsing or a full quota keeps them for this run.
#[cfg(target_arch = "wasm32")]
fn write_settings(text: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(SETTINGS_KEY, text);
    }
}

impl Preferences {
    /// The saved choices; malformed or missing ones take their defaults. The touch
    /// setting applies from here on.
    pub fn load() -> Self {
        let prefs = read_settings().map_or_else(Self::default, |text| Self::read(&text));
        crate::touch::set_setting(prefs.touch);
        prefs
    }

    fn read(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// Save the choices; a changed touch setting applies once kept, as the rest of
    /// Settings does.
    pub fn save(&self) {
        crate::touch::set_setting(self.touch);
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
    /// where extra levels are offered, and as its own team battle. A phone's bots are
    /// always Easy; the saved difficulty stays for a larger screen.
    pub fn game_options(&self, human_team: Team, extra_levels: bool) -> GameOptions {
        GameOptions {
            human_kind: self.tank,
            human_team,
            game_mode: self.game_mode,
            map_mode: self.map,
            difficulty: if crate::touch::phone() {
                Difficulty::Easy
            } else {
                self.difficulty
            },
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
            touch: TouchSetting::Off,
            ..Preferences::default()
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert!(text.contains(r#""map":"quarry""#) && text.contains(r#""graphics":"low""#));
        assert!(text.contains(r#""touch":"off""#));
        assert_eq!(Preferences::read(&text), prefs);

        let edited = Preferences::read(
            r#"{"tank": "humvee", "gameMode": "duel", "map": "atlantis", "difficulty": 3,
                "graphics": "ultra", "volume": 0.25, "playerName": "Vlad", "touch": "always"}"#,
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
