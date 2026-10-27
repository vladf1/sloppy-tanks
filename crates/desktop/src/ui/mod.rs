//! The desktop shell's screens, drawn with egui each frame from the engine's HUD
//! state (`src/game/ui.ts` and its parts). They never change the game themselves:
//! each returns the [`Action`]s the player chose, which the app applies.

pub mod hud;
pub mod menus;
pub mod nerd_stats;
pub mod room_menu;
pub mod rooms;
pub mod setup;
pub mod theme;

use crate::engine_api::{EngineEvent, HudState, Phase};
use crate::preferences::Preferences;
use crate::room_api::{RoomRules, RoomSelection};

/// What the player asked for this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// GO on Battle Setup, or PLAY AGAIN.
    Start,
    Resume,
    Pause,
    EndBattle,
    /// BATTLE SETUP from the results or the pause menu: a fresh world.
    BattleSetup,
    /// A Battle Setup choice changed (the preferences hold it).
    OptionsChanged,
    /// The respawn card picked the next life's tank.
    NextTank(String),
    SelectAmmo(u8),
    ToggleView,
    ToggleFullscreen,
    OpenSettings,
    /// Settings saved: the preferences changed.
    SettingsSaved(Box<Preferences>),
    Quit,
    /// CREATE ROOM or JOIN ROOM on Battle Setup's Multiplayer tab.
    JoinRoom(Box<RoomSelection>),
    /// The host's START BATTLE or PLAY AGAIN.
    RoomStart,
    /// The host's END BATTLE.
    RoomEnd,
    /// LEAVE ROOM, or BATTLE SETUP from the room between battles.
    RoomLeave,
    /// TRY AGAIN, JOIN AGAIN or PLAY HERE once the connection ended.
    RoomRejoin,
    /// BATTLE SETUP once the connection ended, with the reason to show there.
    RoomSetup(String),
    /// Team (`None`: the side with fewer humans) and tank for the next battle.
    RoomChoose {
        team: Option<u8>,
        kind: String,
    },
    /// The host's rules for the next battle.
    RoomRules(RoomRules),
}

/// One line of the kill feed.
#[derive(Clone, Debug)]
pub struct KillFeedEntry {
    pub killer: String,
    pub killer_team: Option<u8>,
    pub victim: String,
    pub victim_team: Option<u8>,
    pub weapon: String,
    /// The player's own kill or death.
    pub own: bool,
    /// Seconds it has shown.
    pub age: f64,
}

/// Seconds a kill feed line shows.
pub const KILL_FEED_SECONDS: f64 = 5.0;
/// Kill feed lines at most.
pub const KILL_FEED_LINES: usize = 3;
/// Seconds a toast shows.
pub const TOAST_SECONDS: f64 = 1.8;
/// Seconds the hit flash and the damage ring show.
pub const HIT_FLASH_SECONDS: f64 = 0.6;
pub const DAMAGE_RING_SECONDS: f64 = 1.2;

/// What the screens show besides the engine's HUD state: transient feedback the
/// events drive, and which dialog is open.
#[derive(Default)]
pub struct UiState {
    pub hud: Option<HudState>,
    pub kill_feed: Vec<KillFeedEntry>,
    pub toast: Option<(String, f64)>,
    /// Seconds since the player was hit, while the flash shows.
    pub hit_flash: Option<f64>,
    /// The clockwise screen angle of the latest damage and its age.
    pub damage_ring: Option<(f64, f64)>,
    /// The Settings dialog's draft, while it is open.
    pub settings_draft: Option<Preferences>,
    /// Battle Setup's status line (preparation progress, errors).
    pub status: String,
    /// The arena for the chosen options is ready.
    pub ready: bool,
    /// GO was pressed and the round is about to start.
    pub starting: bool,
    pub show_nerd_stats: bool,
    /// The engine's latest `stats_json`, while the nerd stats show.
    pub stats: Option<serde_json::Value>,
    /// First person is on (the cockpit compass shows).
    pub cockpit: bool,
    /// The hull's clockwise screen angle (the compass).
    pub hull_angle: f64,
    /// The player's tank on screen (NDC, y up), for the damage ring.
    pub viewer: Option<(f32, f32)>,
    /// Battle Setup's Multiplayer tab: the room list and the join choices.
    pub rooms: rooms::RoomsState,
}

/// What the app tells the screens about where things stand.
pub struct View<'a> {
    pub in_round: bool,
    pub phase: Phase,
    pub prefs: &'a Preferences,
    /// The release and commit line under Battle Setup.
    pub build: &'a str,
    /// Extra levels may be chosen (`--debug`).
    pub debug: bool,
    /// A multiplayer room: no single-player respawn choice.
    pub room: bool,
}

impl UiState {
    /// Advance the HUD's own timers by `dt` seconds.
    pub fn tick(&mut self, dt: f64) {
        for entry in &mut self.kill_feed {
            entry.age += dt;
        }
        self.kill_feed.retain(|entry| entry.age < KILL_FEED_SECONDS);
        if let Some((_, age)) = &mut self.toast {
            *age += dt;
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, age)| *age > TOAST_SECONDS)
        {
            self.toast = None;
        }
        if let Some(age) = &mut self.hit_flash {
            *age += dt;
        }
        if self.hit_flash.is_some_and(|age| age > HIT_FLASH_SECONDS) {
            self.hit_flash = None;
        }
        if let Some((_, age)) = &mut self.damage_ring {
            *age += dt;
        }
        if self
            .damage_ring
            .is_some_and(|(_, age)| age > DAMAGE_RING_SECONDS)
        {
            self.damage_ring = None;
        }
    }

    /// Show an engine event's feedback (`hud-feedback.ts`): kill feed lines, toasts
    /// and the hit flash.
    pub fn event(&mut self, event: &EngineEvent) {
        let name = |id: Option<u32>| {
            let row = self
                .hud
                .as_ref()
                .and_then(|hud| hud.scoreboard.iter().find(|row| Some(row.id) == id));
            (
                row.map_or_else(|| "Unknown".to_owned(), |row| row.name.clone()),
                row.map(|row| row.team),
            )
        };
        match event.kind.as_str() {
            "death" => {
                let (victim, victim_team) = name(event.id);
                let (killer, killer_team) = name(event.owner);
                let human = self.hud.as_ref().map(|hud| hud.human.id);
                self.kill_feed.push(KillFeedEntry {
                    killer,
                    killer_team,
                    victim,
                    victim_team,
                    weapon: event
                        .damage_source
                        .as_ref()
                        .map_or_else(String::new, |source| source.cause.clone()),
                    own: event.own || (human.is_some() && event.owner == human),
                    age: 0.0,
                });
                if self.kill_feed.len() > KILL_FEED_LINES {
                    self.kill_feed.remove(0);
                }
            }
            "pickup" | "promotion" | "notice" if event.own => {
                if let Some(label) = &event.label {
                    self.toast = Some((label.clone(), 0.0));
                }
            }
            "hurt" if event.own => {
                self.hit_flash = Some(0.0);
                if let Some(angle) = event.damage_angle {
                    self.damage_ring = Some((angle, 0.0));
                }
            }
            _ => {}
        }
    }

    /// Forget a round's feedback.
    pub fn reset_feedback(&mut self) {
        self.kill_feed.clear();
        self.toast = None;
        self.hit_flash = None;
        self.damage_ring = None;
    }
}

/// Draw every screen for this frame and return what the player chose.
pub fn draw(context: &egui::Context, state: &mut UiState, view: &View) -> Vec<Action> {
    let mut actions = Vec::new();
    if !view.in_round {
        setup::battle_setup(context, state, view, &mut actions);
    } else {
        hud::hud(context, state, view, &mut actions);
        match view.phase {
            Phase::Paused => menus::pause(context, state, view, &mut actions),
            Phase::Results => menus::results(context, state, view, &mut actions),
            Phase::Playing | Phase::Ready => {}
        }
    }
    if state.settings_draft.is_some() {
        menus::settings(context, state, view, &mut actions);
    }
    if state.show_nerd_stats && view.prefs.nerd_stats {
        nerd_stats::panel(context, state);
    }
    actions
}
