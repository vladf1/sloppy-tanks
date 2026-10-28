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
pub mod touch;

use crate::engine_api::{EngineEvent, HudState, Phase, ScoreboardRow};
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

/// One line of the kill feed (`FeedRow` in `hud-feedback.ts`).
#[derive(Clone, Debug)]
pub struct KillFeedEntry {
    /// The names shown: "YOU" for the viewer, "YARD" for nobody's ordnance.
    pub killer: String,
    pub victim: String,
    /// Each name's side: a team, or neutral for the yard.
    pub killer_team: Option<u8>,
    pub victim_team: Option<u8>,
    /// The damage cause, for the weapon icon between the names.
    pub cause: Option<String>,
    /// The viewer destroyed someone else.
    pub own_kill: bool,
    /// The viewer's own wreck.
    pub own_death: bool,
    /// Seconds it has shown.
    pub age: f64,
}

/// What wrecked the viewer's tank, for the respawn card (`DeathReport`).
#[derive(Clone, Debug, Default)]
pub struct DeathReport {
    /// The killer's name; `None` for the yard or a departed player.
    pub killer: Option<String>,
    /// The viewer's own ordnance.
    pub own_goal: bool,
    pub cause: Option<String>,
    /// The weapon's label, such as "Rocket blast".
    pub weapon: Option<String>,
    pub killer_vehicle: Option<String>,
    pub killer_team: Option<u8>,
    pub killer_kills: Option<f64>,
    /// The sentence the toast and a solo report say.
    pub sentence: String,
}

/// The tone a toast takes: its rule lines' and text's colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Pickup,
    Promotion,
    Death,
}

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
    pub hud: Option<HudState>,
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
    /// The arena for the chosen options is ready.
    pub ready: bool,
    /// GO was pressed and the round is about to start.
    pub starting: bool,
    pub show_nerd_stats: bool,
    /// The engine's latest `stats_json`, while the nerd stats show.
    pub stats: Option<serde_json::Value>,
    /// A room's last network reading, which its update rate is measured from.
    pub network_sample: Option<nerd_stats::NetworkSample>,
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
    /// The longest time seen on each timed effect, so its ring drains from full.
    pub effect_longest: [f64; 5],
    pub pause_tab: PauseTab,
    /// Settings were opened from the battle, which carries on when they close.
    pub resume_after_settings: bool,
    /// The Settings dialog showed last frame.
    pub settings_were_open: bool,
    /// The round the feedback belongs to.
    pub round: u32,
    /// The icons and tank renders, made on the first frame.
    pub art: Option<icons::Art>,
    /// The touch controls this frame, while the player can act on a touch screen.
    pub touch: Option<touch::TouchView>,
    /// Seconds the touch controls have shown since they last appeared, for the fire
    /// hint.
    pub touch_shown: f64,
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

/// The damage labels of the respawn card and the death sentence (`DAMAGE_LABELS`).
pub fn damage_label(cause: &str) -> Option<&'static str> {
    Some(match cause {
        "standard" => "Standard shell",
        "spread" => "Spread shot",
        "rocket" => "Rocket blast",
        "tow" => "TOW missile",
        "ricochet" => "Ricochet shell",
        "piercing" => "Piercing shell",
        "mine" => "Mine explosion",
        "drum" => "Exploding barrel",
        "interception" => "Shell collision blast",
        "explosion" => "Explosion",
        _ => return None,
    })
}

/// The vehicles' names, as Battle Setup and the HUD show them.
pub fn vehicle_name(kind: &str) -> &'static str {
    match kind {
        "scout" => "SKIPPER",
        "heavy" => "BIG RIG",
        "humvee" => "HUNTER",
        _ => "BRUISER",
    }
}

/// What wrecked the viewer, from its death event (`deathReport` and `deathCause`).
pub fn death_report(event: &EngineEvent, scoreboard: &[ScoreboardRow]) -> DeathReport {
    let own_goal = event.owner.is_some() && event.owner == event.id;
    let killer = if own_goal {
        None
    } else {
        scoreboard.iter().find(|row| Some(row.id) == event.owner)
    };
    let cause = event
        .damage_source
        .as_ref()
        .map(|source| source.cause.clone());
    let weapon = cause.as_deref().and_then(damage_label);
    let with = weapon.map(|label| {
        let article = if label.starts_with(['A', 'E', 'I', 'O', 'U']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {}", label.to_lowercase())
    });
    let sentence = match (own_goal, killer, &with) {
        (true, _, Some(with)) => format!("You destroyed yourself with {with}."),
        (true, _, None) => "You destroyed yourself.".to_owned(),
        (false, Some(killer), Some(with)) => format!("{} destroyed you with {with}.", killer.name),
        (false, Some(killer), None) => format!("{} destroyed you.", killer.name),
        (false, None, Some(with)) => format!("You were destroyed by {with}."),
        (false, None, None) => "You were destroyed.".to_owned(),
    };
    DeathReport {
        killer: killer.map(|row| row.name.clone()),
        own_goal,
        cause,
        weapon: weapon.map(str::to_owned),
        killer_vehicle: killer.map(|row| vehicle_name(&row.kind).to_owned()),
        killer_team: killer.map(|row| row.team),
        killer_kills: killer.map(|row| row.kills),
        sentence,
    }
}

/// The share of a timed effect left, against the longest time seen since it began
/// (`effectShare`), which [`UiState::tick`] keeps.
pub fn effect_share(longest: f64, seconds: f64) -> f64 {
    let full = longest.max(seconds);
    if full > 0.0 {
        (seconds / full).min(1.0)
    } else {
        1.0
    }
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
        self.touch_shown = if self.touch.is_some() {
            self.touch_shown + dt
        } else {
            0.0
        };
        if let Some(hud) = &self.hud {
            let human = &hud.human;
            if !human.alive {
                self.respawn_total = self.respawn_total.max(human.respawn);
            }
            if human.mine_cooldown > 0.0 {
                self.mine_total = self.mine_total.max(human.mine_cooldown);
            }
            // An effect that ended forgets its length, so the next pickup starts full.
            let timed = [
                human.protection,
                human.shield,
                human.rapid,
                human.speed,
                human.laser,
            ];
            for (longest, seconds) in self.effect_longest.iter_mut().zip(timed) {
                *longest = if seconds > 0.0 {
                    longest.max(seconds)
                } else {
                    0.0
                };
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

    /// Show an engine event's feedback (`UI.event` in `ui.ts`): kill feed lines, toasts,
    /// the ammunition notice, the hit flash, the damage ring and the death report.
    pub fn event(&mut self, event: &EngineEvent) {
        let Some(hud) = &self.hud else { return };
        let viewer = hud.human.id;
        let alive = hud.human.alive;
        if self.round != hud.match_state.round {
            self.round = hud.match_state.round;
            self.reset_feedback();
        }
        let hud = self.hud.as_ref().expect("checked above");
        let mine = event.own || event.id == Some(viewer);
        if mine {
            match event.kind.as_str() {
                "hurt" => self.hit_flash = Some(0.0),
                "notice" if alive => {
                    if let Some(label) = &event.label {
                        self.ammo_notice = Some((label.clone(), 0.0));
                    }
                }
                _ => {}
            }
            if matches!(event.kind.as_str(), "hurt" | "death")
                && let Some(angle) = event.damage_angle
            {
                self.damage_ring = Some((angle, 0.0));
            }
            match event.kind.as_str() {
                "death" => {
                    let report = death_report(event, &hud.scoreboard);
                    self.toast = Some(Toast {
                        text: report.sentence.clone(),
                        tone: ToastTone::Death,
                        age: 0.0,
                        duration: TOAST_SECONDS,
                    });
                    self.death = Some(report);
                    self.respawn_total = 0.0;
                    self.ammo_notice = None;
                }
                "respawn" => {
                    self.death = None;
                    self.damage_ring = None;
                }
                "pickup" | "promotion" => {
                    let pickup = event.kind == "pickup";
                    self.toast = Some(Toast {
                        text: event.label.clone().unwrap_or_default(),
                        tone: if pickup {
                            ToastTone::Pickup
                        } else {
                            ToastTone::Promotion
                        },
                        age: 0.0,
                        duration: if pickup {
                            PICKUP_TOAST_SECONDS
                        } else {
                            TOAST_SECONDS
                        },
                    });
                }
                _ => {}
            }
        }
        if event.kind == "death" {
            let name = |id: Option<u32>| {
                if id == Some(viewer) {
                    "YOU".to_owned()
                } else {
                    hud.scoreboard
                        .iter()
                        .find(|row| Some(row.id) == id)
                        .map_or_else(|| "YARD".to_owned(), |row| row.name.clone())
                }
            };
            // The victim's team comes with the event; allies take no hull damage from
            // each other, so the killer is on the other side unless the tank destroyed
            // itself or the yard did it (`killFeedSides`).
            let victim_team = event.team;
            let killer_team = match event.owner {
                None => None,
                Some(owner) if Some(owner) == event.id => victim_team,
                Some(_) => victim_team.map(|team| 1 - team.min(1)),
            };
            self.kill_feed.insert(
                0,
                KillFeedEntry {
                    killer: name(event.owner),
                    victim: name(event.id),
                    killer_team,
                    victim_team,
                    cause: event
                        .damage_source
                        .as_ref()
                        .map(|source| source.cause.clone()),
                    own_kill: event.owner == Some(viewer) && event.id != Some(viewer),
                    own_death: mine,
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
        self.effect_longest = [0.0; 5];
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
            Phase::Paused => menus::pause(context, state, &art, view, &mut actions),
            Phase::Results => recap::results(context, state, &art, view, &mut actions),
            Phase::Playing | Phase::Ready => hud::hud(context, state, &art, view, &mut actions),
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
        if view.in_round && view.phase == Phase::Paused {
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
