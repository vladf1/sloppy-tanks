//! What Battle Setup, the Settings dialog and the HUD offer, as display data keyed by
//! the core ids: the map and tank cards, the ammunition, the battle formats, the
//! difficulties and the graphics settings. Names, hit points, speeds, labels and colours
//! the engine owns come from `sloppy-core`; this adds only the words and art around
//! them.
//!
//! The page keeps its own copies (`src/game/map-options.ts`, `ammo-options.ts`, the
//! cards and key strips of `battle-setup.html`, the controls lists of `ui-markup.ts`,
//! `GRAPHICS_SETTINGS`), since it shows Battle Setup before the engine loads; the values
//! here are the ones it shows, and the page catalog pins its copies to them.

use sloppy_core::sim::GameMode;
use sloppy_core::sim::ammunition::AMMO_ORDER;
use sloppy_core::sim::data::{vehicle, weapon};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::{MAP_OPTIONS, MapId, map_option_for};
use sloppy_core::sim::{VehicleKind, Weapon};
use sloppy_render::quality::{GraphicsSetting, Tier};

/// Kilometres per hour in a metre per second.
const KM_PER_HOUR: f64 = 3.6;

/// A map card (`MAP_OPTIONS` in `map-options.ts`); its name and whether it is an extra
/// level come from [`map_option_for`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapCard {
    pub id: MapId,
    /// The card's line.
    pub description: &'static str,
    /// A shorter line for the desktop app's cards, which the page's longer extra-level
    /// lines overflow: five cards share Battle Setup's map row there.
    pub short_description: Option<&'static str>,
    /// The symbol an extra level's card shows (the standard maps show their art).
    pub icon: &'static str,
    /// The colour the card is washed in.
    pub tint: u32,
    /// Tanks per team on an extra level, which brings its own roster.
    pub team_tanks: Option<u32>,
}

impl MapCard {
    pub fn name(&self) -> &'static str {
        map_option_for(self.id).name
    }

    pub fn extra(&self) -> bool {
        map_option_for(self.id).extra
    }
}

/// Every map's card, in `MAP_OPTIONS` order.
pub const MAP_CARDS: [MapCard; MAP_OPTIONS.len()] = [
    MapCard {
        id: MapId::Village,
        description: "Quiet lanes. Loud tanks.",
        short_description: None,
        icon: "village",
        tint: 0x2f7d4f,
        team_tanks: None,
    },
    MapCard {
        id: MapId::Harbor,
        description: "Salt air. Hot steel.",
        short_description: None,
        icon: "harbor",
        tint: 0x2c6fa8,
        team_tanks: None,
    },
    MapCard {
        id: MapId::Quarry,
        description: "Open pits. Dig a shortcut.",
        short_description: None,
        icon: "quarry",
        tint: 0xa8733a,
        team_tanks: None,
    },
    MapCard {
        id: MapId::StressTest,
        description: "30 tanks · 75 destructibles · permanent buildings and barriers",
        short_description: Some("30 tanks · 75 destructibles"),
        icon: "stress",
        tint: 0x6a5ea8,
        team_tanks: Some(15),
    },
    MapCard {
        id: MapId::Superstress,
        description: "Compact yard · 30 tanks · cover rebuilds and debris lingers",
        short_description: Some("Compact yard · cover rebuilds"),
        icon: "yard",
        tint: 0x8a5a44,
        team_tanks: Some(15),
    },
];

pub fn map_card(id: MapId) -> &'static MapCard {
    MAP_CARDS
        .iter()
        .find(|card| card.id == id)
        .expect("every map has a card")
}

/// A tank card of Battle Setup and the respawn card (the `.vehicle` buttons of
/// `battle-setup.html`): its class, the class's colour and the tank's size in the card.
/// The name, hit points and speed are the vehicle's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TankCard {
    pub kind: VehicleKind,
    /// The class under the name, as the page writes it (it shows it in capitals).
    pub class: &'static str,
    /// The class in one word, where the card is small.
    pub short_class: &'static str,
    /// The class tag's dot.
    pub color: u32,
    /// The tank's size in its card: a scout sits small and a heavy fills it.
    pub scale: f32,
}

impl TankCard {
    pub fn name(&self) -> &'static str {
        vehicle(self.kind).name
    }

    pub fn hit_points(&self) -> u32 {
        vehicle(self.kind).health as u32
    }

    /// Top speed, rounded to whole kilometres per hour.
    pub fn km_per_hour(&self) -> u32 {
        (vehicle(self.kind).speed * KM_PER_HOUR).round() as u32
    }

    /// The hit points bar: a share of the toughest tank's.
    pub fn hit_points_share(&self) -> f32 {
        self.share_of_most(TankCard::hit_points)
    }

    /// The speed bar: a share of the fastest tank's shown speed.
    pub fn speed_share(&self) -> f32 {
        self.share_of_most(TankCard::km_per_hour)
    }

    /// This tank's `stat` as a share of the most any tank has, to two places as the page
    /// writes it.
    fn share_of_most(&self, stat: fn(&TankCard) -> u32) -> f32 {
        let most = TANK_CARDS.iter().map(stat).max().unwrap_or(1);
        let share = f64::from(stat(self)) / f64::from(most);
        ((share * 100.0).round() / 100.0) as f32
    }
}

/// The tanks a player drives, lightest first.
pub const TANK_CARDS: [TankCard; VehicleKind::PLAYABLE.len()] = [
    TankCard {
        kind: VehicleKind::Scout,
        class: "Light scout",
        short_class: "Scout",
        color: 0x5fd8ff,
        scale: 0.8,
    },
    TankCard {
        kind: VehicleKind::Balanced,
        class: "Balanced tank",
        short_class: "Balanced",
        color: 0xffcf38,
        scale: 0.94,
    },
    TankCard {
        kind: VehicleKind::Heavy,
        class: "Heavy tank",
        short_class: "Heavy",
        color: 0xff8a4c,
        scale: 1.1,
    },
];

/// An ammunition the HUD's rack offers (`AMMO_OPTIONS` in `ammo-options.ts`); its label
/// and colour are the weapon's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmoOption {
    pub weapon: Weapon,
    pub name: &'static str,
    /// What it does, for the slot's tooltip.
    pub help: &'static str,
}

impl AmmoOption {
    pub fn label(&self) -> &'static str {
        weapon(self.weapon).label
    }

    pub fn color(&self) -> u32 {
        weapon(self.weapon).color
    }
}

/// The rack, in selection order (`AMMO_ORDER`, keys 1–5).
pub const AMMO_OPTIONS: [AmmoOption; AMMO_ORDER.len()] = [
    AmmoOption {
        weapon: Weapon::Standard,
        name: "Standard shells",
        help: "Unlimited shells · stops at walls",
    },
    AmmoOption {
        weapon: Weapon::Spread,
        name: "Spread shot",
        help: "Three shells per volley · best up close",
    },
    AmmoOption {
        weapon: Weapon::Rocket,
        name: "Breaching rockets",
        help: "Accelerates in flight · explosive blast · can hurt you",
    },
    AmmoOption {
        weapon: Weapon::Ricochet,
        name: "Ricochet shells",
        help: "High damage · bounces up to three times",
    },
    AmmoOption {
        weapon: Weapon::Piercing,
        name: "Piercing shells",
        help: "Passes through one enemy shell · stops at tanks and cover",
    },
];

/// A battle format card of single player's Battle Setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BattleFormat {
    pub mode: GameMode,
    pub name: &'static str,
    /// The line under the name on a standard map.
    pub detail: &'static str,
}

pub const BATTLE_FORMATS: [BattleFormat; GameMode::ALL.len()] = [
    BattleFormat {
        mode: GameMode::Team,
        name: "Team Battle",
        detail: "6 vs 6 · Respawns · First to 100",
    },
    BattleFormat {
        mode: GameMode::Solo,
        name: "Solo Assault",
        detail: "Survive endless enemies",
    },
];

impl BattleFormat {
    /// The line under the name with `map` chosen: an extra level's team battle names
    /// its roster (`showBattleFormat`).
    pub fn detail_on(&self, map: MapId) -> String {
        match map_card(map).team_tanks {
            Some(tanks) if self.mode == GameMode::Team => {
                format!("{tanks} vs {tanks} · Endless respawns and scoring")
            }
            _ => self.detail.to_owned(),
        }
    }

    /// Why the format cannot be chosen with `map`: an extra level plays only its own
    /// team battle.
    pub fn unavailable_on(&self, map: MapId) -> Option<String> {
        (self.mode == GameMode::Solo && map_card(map).team_tanks.is_some())
            .then(|| format!("Not available on {}", map_option_for(map).name))
    }
}

/// A difficulty of Battle Setup: its name and what it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DifficultyOption {
    pub difficulty: Difficulty,
    pub name: &'static str,
    pub detail: &'static str,
}

pub const DIFFICULTY_OPTIONS: [DifficultyOption; Difficulty::ALL.len()] = [
    DifficultyOption {
        difficulty: Difficulty::Easy,
        name: "Easy",
        detail: "Slower, less accurate bots · you take 10% less damage",
    },
    DifficultyOption {
        difficulty: Difficulty::Normal,
        name: "Normal",
        detail: "Bots aim well and react fast · the intended fight",
    },
    DifficultyOption {
        difficulty: Difficulty::Hard,
        name: "Hard",
        detail: "Sharp, fast-reloading bots · you take 15% more damage",
    },
];

pub fn difficulty_name(difficulty: Difficulty) -> &'static str {
    DIFFICULTY_OPTIONS
        .iter()
        .find(|option| option.difficulty == difficulty)
        .map_or("Normal", |option| option.name)
}

/// The Settings dialog's Graphics choices and their names (`GRAPHICS_SETTINGS`): Auto
/// adapts the quality tier and render scale to the frame rate; the others fix the tier
/// at full resolution.
pub const GRAPHICS_OPTIONS: [(GraphicsSetting, &str); 4] = [
    (GraphicsSetting::Auto, "Auto"),
    (GraphicsSetting::Fixed(Tier::High), "High"),
    (GraphicsSetting::Fixed(Tier::Medium), "Medium"),
    (GraphicsSetting::Fixed(Tier::Low), "Low"),
];

/// The sound volume the Settings dialog starts at and marks on its slider, and the step
/// its sliders move in (volume and the battle speeds, whose range is core's
/// `MIN_SPEED_SCALE` to `MAX_SPEED_SCALE`).
pub const DEFAULT_VOLUME: f64 = 0.6;
pub const SLIDER_STEP: f64 = 0.05;

/// One line of the controls lists (`ControlLine` in `ui-markup.ts`): what it does, its
/// keys, and a note after them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlLine {
    pub action: &'static str,
    pub keys: &'static [&'static str],
    pub note: &'static str,
}

const fn line(action: &'static str, keys: &'static [&'static str]) -> ControlLine {
    ControlLine {
        action,
        keys,
        note: "",
    }
}

const fn noted(
    action: &'static str,
    keys: &'static [&'static str],
    note: &'static str,
) -> ControlLine {
    ControlLine { action, keys, note }
}

/// Keyboard and mouse, as the pause menu and the room menu list them.
pub const KEYBOARD_CONTROLS: [ControlLine; 8] = [
    line("Drive", &["W", "A", "S", "D"]),
    line("Aim", &["Mouse"]),
    noted("Fire", &["Left click"], "hold"),
    line("Drop mine", &["Right click"]),
    noted("Ammo", &["Q", "E", "1–5"], "or wheel"),
    line("Zoom", &["Shift", "Wheel"]),
    noted("First person", &["V"], "turret follows mouse"),
    line("Pause", &["Esc"]),
];

/// The same moves on a touch screen.
pub const TOUCH_CONTROLS: [ControlLine; 6] = [
    line("Drive", &["Left stick"]),
    line("Aim and fire", &["Touch the arena"]),
    line("Drop mine", &["Mine button"]),
    line("Ammo", &["Tap a round"]),
    line("Zoom", &["−", "+"]),
    noted("First person", &["Eye button"], "stick sideways turns"),
];

/// Battle Setup's line of essentials under its dialog (`.key-strip` in
/// `battle-setup.html`), keys and what they do; the pause menu lists every control.
pub const KEY_STRIP: [(&[&str], &str); 6] = [
    (&["W", "A", "S", "D"], "Drive"),
    (&["Mouse"], "Aim"),
    (&["Left click"], "Fire"),
    (&["Right click"], "Mine"),
    (&["Q", "E"], "Ammo"),
    (&["Esc"], "All controls"),
];

/// The same line on a touch screen (`.touch-strip`).
pub const TOUCH_STRIP: [(&[&str], &str); 4] = [
    (&["Left stick"], "Drive"),
    (&["Touch the arena"], "Aim & fire"),
    (&["Mine button"], "Mine"),
    (&["Eye button"], "First person"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_map_and_playable_tank_has_one_card_in_the_engines_order() {
        let maps: Vec<MapId> = MAP_CARDS.iter().map(|card| card.id).collect();
        let options: Vec<MapId> = MAP_OPTIONS.iter().map(|option| option.id).collect();
        assert_eq!(maps, options);
        for card in &MAP_CARDS {
            assert_eq!(card.team_tanks.is_some(), card.extra(), "{:?}", card.id);
            assert_eq!(card.short_description.is_some(), card.extra());
        }
        let kinds: Vec<VehicleKind> = TANK_CARDS.iter().map(|card| card.kind).collect();
        assert_eq!(kinds, VehicleKind::PLAYABLE);
        let weapons: Vec<Weapon> = AMMO_OPTIONS.iter().map(|option| option.weapon).collect();
        assert_eq!(weapons, AMMO_ORDER);
    }

    /// The tank cards' numbers and bars as `battle-setup.html` writes them.
    #[test]
    fn tank_cards_show_the_pages_numbers_and_bars() {
        let shown: Vec<_> = TANK_CARDS
            .iter()
            .map(|card| {
                (
                    card.name(),
                    card.hit_points(),
                    card.km_per_hour(),
                    card.hit_points_share(),
                    card.speed_share(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("SKIPPER", 80, 40, 0.57, 1.0),
                ("BRUISER", 100, 32, 0.71, 0.8),
                ("BIG RIG", 140, 24, 1.0, 0.6),
            ]
        );
    }

    #[test]
    fn ammunition_takes_the_weapons_label_and_colour() {
        let rocket = &AMMO_OPTIONS[2];
        assert_eq!((rocket.label(), rocket.color()), ("ROCKET", 0xff591c));
        assert_eq!(AMMO_OPTIONS[4].label(), "PIERCING");
    }

    #[test]
    fn an_extra_level_names_its_roster_and_offers_no_solo_assault() {
        let [team, solo] = BATTLE_FORMATS;
        assert_eq!(
            team.detail_on(MapId::Superstress),
            "15 vs 15 · Endless respawns and scoring"
        );
        assert_eq!(team.detail_on(MapId::Harbor), team.detail);
        assert_eq!(
            solo.unavailable_on(MapId::StressTest).as_deref(),
            Some("Not available on Stress Grid")
        );
        assert_eq!(solo.unavailable_on(MapId::Village), None);
        assert_eq!(team.unavailable_on(MapId::StressTest), None);
        assert_eq!(solo.detail_on(MapId::StressTest), solo.detail);
    }

    #[test]
    fn graphics_options_name_the_settings_the_engine_reads() {
        for (setting, _) in GRAPHICS_OPTIONS {
            assert_eq!(GraphicsSetting::parse(setting.name()), setting);
        }
        assert_eq!(difficulty_name(Difficulty::Hard), "Hard");
    }
}
