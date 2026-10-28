//! The player's choices between launches (`src/game/player-preferences.ts`): one JSON
//! file in the platform's settings folder, beside the personal bests.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The folder of the settings and records files: `~/Library/Application Support/Sloppy
/// Tanks` on macOS, `%APPDATA%\Sloppy Tanks` on Windows.
pub fn data_directory() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("Sloppy Tanks"))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// "scout", "balanced" or "heavy".
    pub tank: String,
    /// "team" or "solo".
    pub game_mode: String,
    pub map: String,
    /// "easy", "normal" or "hard".
    pub difficulty: String,
    pub first_person: bool,
    pub zoom: Option<f64>,
    pub volume: f64,
    pub tank_speed: f64,
    pub bullet_speed: f64,
    /// "auto", "high", "medium" or "low".
    pub graphics: String,
    pub player_name: String,
    /// Debug tools: the extra levels and Stats for nerds (N in battle), as `?debug`
    /// gives the page and `--debug` the app.
    pub debug: bool,
    pub fullscreen: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            tank: "balanced".into(),
            game_mode: "team".into(),
            map: "village".into(),
            difficulty: "normal".into(),
            first_person: false,
            zoom: None,
            volume: 0.6,
            tank_speed: 1.0,
            bullet_speed: 1.0,
            graphics: "auto".into(),
            player_name: String::new(),
            debug: false,
            fullscreen: false,
        }
    }
}

impl Preferences {
    fn path() -> Option<PathBuf> {
        data_directory().map(|directory| directory.join("settings.json"))
    }

    /// The saved choices; malformed or missing ones take their defaults.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Save the choices; a folder that cannot be written keeps them for this run.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(directory) = path.parent() {
            let _ = std::fs::create_dir_all(directory);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}
