//! The desktop shell's screens, drawn with egui each frame from the engine's HUD
//! state (`src/game/ui.ts` and its parts). They never change the game themselves:
//! each returns the [`Action`]s the player chose, which the app applies.

pub mod hud;
pub mod icons;
pub mod menus;
pub mod nerd_stats;
pub mod paint;
pub mod recap;
pub mod respawn;
pub mod room_menu;
pub mod rooms;
pub mod setup;
pub mod theme;

use sloppy_core::net::protocol::RoomSettings;
use sloppy_core::sim::{DamageCause, MatchPhase, SimEventType, Team, VehicleKind};
use sloppy_web::events::PendingEvent;
pub use sloppy_web::feedback::{DeathReport, ToastTone};
use sloppy_web::hud::Hud;

use crate::preferences::Preferences;
use crate::room_api::RoomSelection;

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
    /// The respawn card picked the next life's tank.
    NextTank(VehicleKind),
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
        team: Option<Team>,
        kind: VehicleKind,
    },
    /// The host's rules for the next battle.
    RoomRules(RoomSettings),
}

/// One line of the kill feed: the engine's row (`sloppy_web::feedback::FeedRow`), or a
/// room's notice in the killer's place.
#[derive(Clone, Debug)]
pub struct KillFeedEntry {
    /// The names shown: "YOU" for the viewer, "YARD" for nobody's ordnance.
    pub killer: String,
    pub victim: String,
    /// Each name's side: a team, or neutral for the yard.
    pub killer_team: Option<Team>,
    pub victim_team: Option<Team>,
    /// The damage cause, for the weapon icon between the names.
    pub cause: Option<DamageCause>,
    /// The viewer destroyed someone else.
    pub own_kill: bool,
    /// The viewer's own wreck.
    pub own_death: bool,
    /// Seconds it has shown.
    pub age: f64,
}

/// A notice over the field: the engine's toast, and how long it has shown.
#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub tone: ToastTone,
    /// Seconds it has shown, and how long it stays.
    pub age: f64,
    pub duration: f64,
}

/// Pause's two tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PauseTab {
    #[default]
    Battle,
    Controls,
}

/// Seconds a kill feed line shows.
pub const KILL_FEED_SECONDS: f64 = 5.0;
/// Kill feed lines at most.
pub const KILL_FEED_LINES: usize = 3;
/// Seconds a pickup's toast shows, and a promotion's or death's.
pub const PICKUP_TOAST_SECONDS: f64 = 1.5;
pub const TOAST_SECONDS: f64 = 2.0;
/// Seconds an ammunition notice shows above the tank panel.
pub const AMMO_NOTICE_SECONDS: f64 = 3.0;
/// Seconds the hit flash and the damage ring show.
pub const HIT_FLASH_SECONDS: f64 = 0.6;
pub const DAMAGE_RING_SECONDS: f64 = 1.2;
/// The hull bar's ghost waits this long after a hit, then trails down over the next.
const HULL_GHOST_DELAY: f64 = 0.35;
const HULL_GHOST_SECONDS: f64 = 0.55;

/// What the screens show besides the engine's HUD state: transient feedback the
/// events drive, and which dialog is open.
#[derive(Default)]
pub struct UiState {
    /// The engine's HUD as of its last refresh.
    pub hud: Option<Hud<'static>>,
    pub kill_feed: Vec<KillFeedEntry>,
    pub toast: Option<Toast>,
    /// The latest ammunition notice and its age.
    pub ammo_notice: Option<(String, f64)>,
    /// Seconds since the player was hit, while the flash shows.
    pub hit_flash: Option<f64>,
    /// The clockwise screen angle of the latest damage and its age.
    pub damage_ring: Option<(f64, f64)>,
    /// The Settings dialog's draft, while it is open.
    pub settings_draft: Option<Preferences>,
    /// Battle Setup's status line (preparation progress, errors).
    pub status: String,
    /// The share of the arena's shaders loaded while the status line counts them.
    pub progress: Option<f32>,
    /// The arena for the chosen options is ready.
    pub ready: bool,
    /// GO was pressed and the round is about to start.
    pub starting: bool,
    pub show_nerd_stats: bool,
    /// The engine's latest Stats for nerds rows, while the nerd stats show.
    pub stats: Option<Vec<sloppy_web::nerd_rows::StatsSection>>,
    /// Stats for nerds' sections the player opened or folded.
    pub nerd_sections: std::collections::BTreeMap<&'static str, bool>,
    /// First person is on (the cockpit compass shows).
    pub cockpit: bool,
    /// The hull's clockwise screen angle (the compass).
    pub hull_angle: f64,
    /// The player's tank on screen (NDC, y up), for the damage ring.
    pub viewer: Option<(f32, f32)>,
    /// Battle Setup's Multiplayer tab: the room list and the join choices.
    pub rooms: rooms::RoomsState,
    /// The viewer's latest death, until the tank respawns.
    pub death: Option<DeathReport>,
    /// The respawn countdown's length when it started, so its ring drains to empty.
    pub respawn_total: f64,
    /// The hull share the bar's ghost shows, and how long it has waited to trail.
    pub hull_ghost: f64,
    pub hull_ghost_wait: f64,
    /// The longest mine cooldown seen, so the mine dial fills from empty.
    pub mine_total: f64,
    pub pause_tab: PauseTab,
    /// Settings were opened from the battle, which carries on when they close.
    pub resume_after_settings: bool,
    /// The Settings dialog showed last frame.
    pub settings_were_open: bool,
    /// The round the feedback belongs to.
    pub round: u32,
    /// The icons and tank renders, made on the first frame.
    pub art: Option<icons::Art>,
}

/// What the app tells the screens about where things stand.
pub struct View<'a> {
    pub in_round: bool,
    pub phase: MatchPhase,
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
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| toast.age > toast.duration)
        {
            self.toast = None;
        }
        if let Some((_, age)) = &mut self.ammo_notice {
            *age += dt;
        }
        if self
            .ammo_notice
            .as_ref()
            .is_some_and(|(_, age)| *age > AMMO_NOTICE_SECONDS)
        {
            self.ammo_notice = None;
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
        self.trail_hull_ghost(dt);
        if let Some(hud) = &self.hud {
            let human = &hud.human;
            if !human.alive {
                self.respawn_total = self.respawn_total.max(human.respawn);
            }
            if human.mine_cooldown > 0.0 {
                self.mine_total = self.mine_total.max(human.mine_cooldown);
            }
        }
    }

    /// The ghost behind the hull bar trails it down after a hit; a repair moves it up
    /// at once (it sits under the bar, hidden).
    fn trail_hull_ghost(&mut self, dt: f64) {
        let Some(hud) = &self.hud else { return };
        let ratio = hud.human.health_ratio.clamp(0.0, 1.0);
        if ratio >= self.hull_ghost {
            self.hull_ghost = ratio;
            self.hull_ghost_wait = 0.0;
            return;
        }
        self.hull_ghost_wait += dt;
        if self.hull_ghost_wait > HULL_GHOST_DELAY {
            let step = dt / HULL_GHOST_SECONDS;
            self.hull_ghost = (self.hull_ghost - step).max(ratio);
        }
    }

    /// Show an engine event's feedback (`UI.event` in `ui.ts`): the kill feed row, toast
    /// and death report the engine attached, the ammunition notice, the hit flash and
    /// the damage ring.
    pub fn event(&mut self, pending: &PendingEvent) {
        let Some(hud) = &self.hud else { return };
        let alive = hud.human.alive;
        if self.round != hud.match_state.round {
            self.round = hud.match_state.round;
            self.reset_feedback();
        }
        let event = &pending.event;
        let feedback = &pending.feedback;
        if pending.own {
            match event.kind {
                SimEventType::Hurt => self.hit_flash = Some(0.0),
                SimEventType::Notice if alive => {
                    if let Some(label) = &event.label {
                        self.ammo_notice = Some((label.clone(), 0.0));
                    }
                }
                SimEventType::Respawn => {
                    self.death = None;
                    self.damage_ring = None;
                }
                _ => {}
            }
            if matches!(event.kind, SimEventType::Hurt | SimEventType::Death)
                && let Some(angle) = pending.damage_angle
            {
                self.damage_ring = Some((angle, 0.0));
            }
            if let Some(report) = &feedback.death_report {
                self.death = Some(report.clone());
                self.respawn_total = 0.0;
                self.ammo_notice = None;
            }
        }
        if let Some(toast) = &feedback.toast {
            self.toast = Some(Toast {
                text: toast.text.clone(),
                tone: toast.tone,
                age: 0.0,
                duration: if toast.tone == ToastTone::Pickup {
                    PICKUP_TOAST_SECONDS
                } else {
                    TOAST_SECONDS
                },
            });
        }
        if let Some(row) = &feedback.feed {
            let [killer, victim] = row.names.clone();
            self.kill_feed.insert(
                0,
                KillFeedEntry {
                    killer,
                    victim,
                    killer_team: row.sides[0].team(),
                    victim_team: row.sides[1].team(),
                    cause: row.cause,
                    own_kill: row.own_kill,
                    own_death: row.own_death,
                    age: 0.0,
                },
            );
            self.kill_feed.truncate(KILL_FEED_LINES);
        }
    }

    /// Forget a round's feedback.
    pub fn reset_feedback(&mut self) {
        self.kill_feed.clear();
        self.toast = None;
        self.ammo_notice = None;
        self.hit_flash = None;
        self.damage_ring = None;
        self.death = None;
        self.respawn_total = 0.0;
        self.mine_total = 0.0;
        self.hull_ghost = 1.0;
        self.hull_ghost_wait = 0.0;
        self.pause_tab = PauseTab::Battle;
    }
}

/// Draw every screen for this frame and return what the player chose.
pub fn draw(context: &egui::Context, state: &mut UiState, view: &View) -> Vec<Action> {
    let mut actions = Vec::new();
    let art = state.art.take().unwrap_or_else(|| icons::Art::new(context));
    if !view.in_round {
        setup::battle_setup(context, state, &art, view, &mut actions);
    } else {
        match view.phase {
            MatchPhase::Paused => menus::pause(context, state, &art, view, &mut actions),
            MatchPhase::Results => recap::results(context, state, &art, view, &mut actions),
            MatchPhase::Playing | MatchPhase::Ready => {
                hud::hud(context, state, &art, view, &mut actions);
            }
        }
    }
    if state.settings_draft.is_some() {
        menus::settings(context, state, &art, view, &mut actions);
    }
    state.art = Some(art);
    let settings_open = state.settings_draft.is_some();
    if state.settings_were_open && !settings_open && state.resume_after_settings {
        // Settings opened from the battle's toolbar paused it; closing them, however
        // they closed, carries it on.
        state.resume_after_settings = false;
        if view.in_round && view.phase == MatchPhase::Paused {
            actions.push(Action::Resume);
        }
    }
    state.settings_were_open = settings_open;
    if state.show_nerd_stats && view.debug {
        nerd_stats::panel(context, state);
    }
    actions
}

#[cfg(test)]
mod snapshots;
