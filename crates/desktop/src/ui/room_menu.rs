//! A multiplayer room's screens (`src/net/network-ui.ts`, the join screen of
//! `src/game/join-screen.ts`): the joining card while the room loads, the room menu
//! (lobby, rules, roster, the host's controls, the results and the in-battle menu), the
//! dialog when the connection drops or ends, and the battle HUD's room parts (the
//! players list and the respawn card).

use egui::{Align2, RichText};
use sloppy_core::net::protocol::{Lobby, MAX_ROUND_MINUTES, Player, RoomPhase};
use sloppy_core::sim::types::Team;

use super::rooms::map_name;
use super::setup::{EXTRA_MAPS, MAPS, TANKS};
use super::theme::{GOLD, RED, TEXT_DIM, TEXT_LABEL, display_font, team_color};
use super::{Action, KillFeedEntry, UiState, View, hud, menus};
use crate::directory::stepped_minutes;
use crate::engine_api::{EngineEvent, MatchState, Phase};
use crate::room_api::RoomRules;

/// Seconds "Code copied" shows on its button.
const COPIED_SECONDS: f64 = 2.0;
/// Tanks per team, bots included, on a standard map; extra levels name their own.
const STANDARD_TEAM_TANKS: usize = 6;
/// Tanks per team on the extra levels (`teamTanks` in `map-options.ts`).
const EXTRA_TEAM_TANKS: usize = 15;
const TEAM_NAMES: [&str; 2] = ["BLUE", "RED"];
/// Minutes the host's length stepper moves.
const LENGTH_STEP: i32 = 5;

/// Whether the room's socket is live, being (re)connected, or has given up.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Live,
    Connecting {
        title: String,
        text: String,
    },
    /// `cause` is the client's `EndCause`.
    Ended {
        cause: String,
        text: String,
    },
}

/// The heading of each connection end, and the retry it offers besides Battle Setup.
fn ending(cause: &str) -> (&'static str, Option<&'static str>) {
    match cause {
        "lost" => ("CONNECTION LOST", Some("TRY AGAIN")),
        "rejected" => ("DISCONNECTED", Some("TRY AGAIN")),
        "seat-expired" => ("SEAT EXPIRED", Some("JOIN AGAIN")),
        "other-tab" => ("PLAYING ELSEWHERE", Some("PLAY HERE")),
        "room-ended" => ("ROOM CLOSED", None),
        "outdated" => ("GAME UPDATED", None),
        _ => ("RENDERER STOPPED", None),
    }
}

/// What the room's screens show besides the HUD's state.
pub struct RoomMenu {
    pub room: String,
    /// The room loads behind the joining card until the engine reveals it.
    pub joining: bool,
    pub creating: bool,
    /// The latest status line while joining or reconnecting.
    pub status: String,
    pub lobby: Option<Lobby>,
    pub player_id: String,
    pub link: Link,
    /// The finished round as the viewer's tank played it.
    pub outcome: Option<(MatchState, u8)>,
    /// The round this host ended early: its seat had the menu open, so no final state.
    pub ended_round: Option<u64>,
    /// The in-battle menu is open.
    pub menu_open: bool,
    editing_rules: bool,
    editing_choice: bool,
    /// A one-line answer in the room menu, such as a full team.
    pub notice: String,
    /// Seconds left on "Code copied".
    copied: f64,
    /// Who destroyed the viewer, while the respawn card shows.
    pub death_cause: String,
}

impl RoomMenu {
    pub fn new(room: &str, creating: bool) -> Self {
        Self {
            room: room.to_owned(),
            joining: true,
            creating,
            status: "Connecting to room…".into(),
            lobby: None,
            player_id: String::new(),
            link: Link::Connecting {
                title: "CONNECTING".into(),
                text: "Connecting to the room…".into(),
            },
            outcome: None,
            ended_round: None,
            menu_open: false,
            editing_rules: false,
            editing_choice: false,
            notice: String::new(),
            copied: 0.0,
            death_cause: String::new(),
        }
    }

    pub fn phase(&self) -> RoomPhase {
        self.lobby
            .as_ref()
            .map_or(RoomPhase::Lobby, |lobby| lobby.phase)
    }

    /// The battle is on and the player is in it: no menu, no dropped link.
    pub fn in_battle(&self) -> bool {
        self.phase() == RoomPhase::Playing && !self.menu_open && self.link == Link::Live
    }

    fn mine(&self) -> Option<&Player> {
        self.lobby
            .as_ref()?
            .players
            .iter()
            .find(|player| player.player_id == self.player_id)
    }

    /// The viewer's team, for the score bar.
    pub fn team(&self) -> Option<u8> {
        self.mine().map(|player| player.team.index() as u8)
    }

    /// Connection progress: live, or a dialog that says the socket is down.
    pub fn status(&mut self, text: &str, connected: bool) {
        self.status = text.to_owned();
        if connected {
            self.link = Link::Live;
        } else {
            let title = match &self.link {
                Link::Live => "CONNECTION LOST".to_owned(),
                Link::Connecting { title, .. } => title.clone(),
                Link::Ended { .. } => "RECONNECTING".to_owned(),
            };
            self.link = Link::Connecting {
                title,
                text: text.to_owned(),
            };
        }
    }

    pub fn ended(&mut self, cause: String, text: String) {
        self.link = Link::Ended { cause, text };
    }

    /// A new lobby: who joined or came back goes to the feed.
    pub fn lobby(&mut self, lobby: Lobby, player_id: String, ui: &mut UiState) {
        if let Some(previous) = self
            .lobby
            .as_ref()
            .filter(|previous| previous.room_epoch == lobby.room_epoch)
        {
            for player in &lobby.players {
                let before = previous
                    .players
                    .iter()
                    .find(|item| item.player_id == player.player_id);
                if player.player_id != player_id
                    && player.connected
                    && !before.is_some_and(|before| before.connected)
                {
                    let what = if before.is_some() {
                        " reconnected".to_owned()
                    } else {
                        format!(" joined {} team", team_word(player.team))
                    };
                    ui.kill_feed.push(KillFeedEntry {
                        killer: player.name.clone() + &what,
                        killer_team: Some(player.team.index() as u8),
                        victim: String::new(),
                        victim_team: None,
                        cause: None,
                        own_kill: false,
                        own_death: false,
                        age: 0.0,
                    });
                    if ui.kill_feed.len() > super::KILL_FEED_LINES {
                        ui.kill_feed.remove(0);
                    }
                }
            }
        }
        if lobby.phase != RoomPhase::Results {
            self.outcome = None;
        }
        if lobby.phase == RoomPhase::Playing {
            self.editing_rules = false;
            self.editing_choice = false;
        } else {
            self.menu_open = false;
        }
        self.lobby = Some(lobby);
        self.player_id = player_id;
    }

    /// An engine event's room feedback: the death cause on the respawn card.
    pub fn event(&mut self, event: &EngineEvent, ui: &UiState) {
        let Some(hud) = &ui.hud else { return };
        let viewer = hud.human.id;
        if event.id != Some(viewer) {
            return;
        }
        match event.kind.as_str() {
            "death" => {
                let weapon = event
                    .damage_source
                    .as_ref()
                    .map(|source| source.cause.to_lowercase())
                    .filter(|cause| !cause.is_empty())
                    .map(|cause| {
                        let article = if cause.starts_with(['a', 'e', 'i', 'o', 'u']) {
                            "an"
                        } else {
                            "a"
                        };
                        format!(" with {article} {cause}")
                    })
                    .unwrap_or_default();
                let killer = hud
                    .scoreboard
                    .iter()
                    .find(|row| Some(row.id) == event.owner)
                    .map(|row| row.name.clone());
                self.death_cause = if event.owner == Some(viewer) {
                    format!("You destroyed yourself{weapon}.")
                } else if let Some(killer) = killer {
                    format!("{killer} destroyed you{weapon}.")
                } else {
                    "You were destroyed.".into()
                };
            }
            "respawn" => self.death_cause.clear(),
            _ => {}
        }
    }

    /// Victory or defeat when the final state arrived; a neutral heading otherwise.
    fn results_title(&self) -> &'static str {
        match &self.outcome {
            None if self.ended_round.is_some()
                && self.ended_round == self.lobby.as_ref().map(|lobby| lobby.round_id) =>
            {
                "BATTLE ENDED"
            }
            None => "ROUND COMPLETE",
            Some((outcome, _)) if outcome.ended_early || outcome.winner.is_none() => "BATTLE ENDED",
            Some((outcome, team)) if outcome.winner == Some(*team) => "VICTORY",
            Some(_) => "DEFEAT",
        }
    }
}

fn team_word(team: Team) -> &'static str {
    if team == Team::Blue { "Blue" } else { "Red" }
}

fn tank_name(kind: &str) -> &str {
    TANKS
        .iter()
        .find(|tank| tank.id == kind)
        .map_or(kind, |tank| tank.name)
}

fn centered_window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
}

/// Draw the room's screens for this frame and return what the player chose.
pub fn draw(
    context: &egui::Context,
    menu: &mut RoomMenu,
    state: &mut UiState,
    view: &View,
    dt: f64,
) -> Vec<Action> {
    let mut actions = Vec::new();
    menu.copied = (menu.copied - dt).max(0.0);
    let art = state
        .art
        .take()
        .unwrap_or_else(|| super::icons::Art::new(context));
    if menu.joining {
        joining(context, menu, &mut actions);
    } else {
        if menu.in_battle() {
            hud::hud(context, state, &art, view, &mut actions);
            players(context, menu, state);
            if state.hud.as_ref().is_some_and(|hud| !hud.human.alive) {
                respawn(context, menu, state);
            }
        }
        match &menu.link {
            Link::Live if !menu.in_battle() => room_menu(context, menu, view, &mut actions),
            Link::Live => {}
            _ => connection(context, menu, &mut actions),
        }
    }
    if state.settings_draft.is_some() {
        menus::settings(context, state, &art, view, &mut actions);
    }
    state.art = Some(art);
    if state.show_nerd_stats && view.prefs.nerd_stats {
        super::nerd_stats::panel(context, state);
    }
    actions
}

/// Battle Setup's place while the room connects and compiles its arena.
fn joining(context: &egui::Context, menu: &RoomMenu, actions: &mut Vec<Action>) {
    centered_window("joining")
        .fixed_size([420.0, 0.0])
        .show(context, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(if menu.creating {
                        "CREATING ROOM"
                    } else {
                        "JOINING ROOM"
                    })
                    .font(display_font(30.0))
                    .color(GOLD),
                );
                ui.label(RichText::new(&menu.room).font(display_font(24.0)));
                ui.add_space(6.0);
                ui.add(egui::Spinner::new());
                ui.label(RichText::new(&menu.status).color(TEXT_DIM));
                ui.add_space(6.0);
                if ui.button("CANCEL").clicked() {
                    actions.push(Action::RoomLeave);
                }
            });
        });
}

/// The connection dropped (reconnecting by itself) or ended (the way back that fits).
fn connection(context: &egui::Context, menu: &RoomMenu, actions: &mut Vec<Action>) {
    let playing = menu.phase() == RoomPhase::Playing;
    let humans_only = menu
        .lobby
        .as_ref()
        .is_some_and(|lobby| lobby.settings.humans_only);
    centered_window("connection")
        .fixed_size([420.0, 0.0])
        .show(context, |ui| {
            ui.label(
                RichText::new(format!("ROOM {}", menu.room))
                    .size(12.0)
                    .color(TEXT_LABEL),
            );
            match &menu.link {
                Link::Connecting { title, text } => {
                    ui.label(RichText::new(title).font(display_font(30.0)).color(GOLD));
                    let mut text = text.clone();
                    if playing {
                        text += if humans_only {
                            " Your tank sits idle until you're back."
                        } else {
                            " A bot drives your tank until you're back."
                        };
                    }
                    ui.label(RichText::new(text).color(TEXT_DIM));
                    ui.add(egui::Spinner::new());
                    if ui.button("LEAVE ROOM").clicked() {
                        actions.push(Action::RoomLeave);
                    }
                }
                Link::Ended { cause, text } => {
                    let (title, retry) = ending(cause);
                    ui.label(RichText::new(title).font(display_font(30.0)).color(RED));
                    ui.label(RichText::new(text).color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        if let Some(retry) = retry
                            && ui
                                .button(RichText::new(retry).font(display_font(22.0)))
                                .clicked()
                        {
                            actions.push(Action::RoomRejoin);
                        }
                        if ui
                            .button(RichText::new("BATTLE SETUP").font(display_font(22.0)))
                            .clicked()
                        {
                            actions.push(Action::RoomSetup(text.clone()));
                        }
                    });
                }
                Link::Live => {}
            }
        });
}

/// The room menu: between battles the lobby or the results, during one the menu that
/// hands the tank to a bot while it is open.
fn room_menu(context: &egui::Context, menu: &mut RoomMenu, view: &View, actions: &mut Vec<Action>) {
    let Some(lobby) = menu.lobby.clone() else {
        return;
    };
    let host = lobby.host_id == menu.player_id;
    let playing = lobby.phase == RoomPhase::Playing;
    let alone = lobby.players.len() == 1;
    centered_window("room")
        .fixed_size([560.0, 0.0])
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("ROOM {}", menu.room))
                        .size(13.0)
                        .color(TEXT_LABEL),
                );
                let copy = if menu.copied > 0.0 {
                    "Code copied"
                } else {
                    "Copy room code"
                };
                if ui.small_button(copy).clicked() {
                    ui.ctx().copy_text(menu.room.clone());
                    menu.copied = COPIED_SECONDS;
                    menu.notice = "Room code copied. Friends enter it under Join by code.".into();
                }
            });
            ui.horizontal(|ui| {
                let title = match lobby.phase {
                    RoomPhase::Playing => "BATTLE IN PROGRESS",
                    RoomPhase::Results => menu.results_title(),
                    RoomPhase::Lobby => "LOBBY",
                };
                ui.label(RichText::new(title).font(display_font(36.0)).color(GOLD));
                if lobby.phase == RoomPhase::Results
                    && let Some((outcome, _)) = &menu.outcome
                {
                    ui.label(
                        RichText::new(outcome.scores[0].to_string())
                            .font(display_font(30.0))
                            .color(team_color(0)),
                    );
                    ui.label(RichText::new(":").font(display_font(30.0)));
                    ui.label(
                        RichText::new(outcome.scores[1].to_string())
                            .font(display_font(30.0))
                            .color(team_color(1)),
                    );
                }
            });
            ui.label(RichText::new(heading_hint(&lobby, host, alone)).color(TEXT_DIM));
            if !menu.notice.is_empty() {
                ui.label(RichText::new(&menu.notice).color(GOLD));
            }
            if !playing {
                ui.separator();
                next_battle(ui, menu, &lobby, host, view.debug, actions);
            }
            if lobby.phase == RoomPhase::Results {
                scoreboard(ui, &lobby, &menu.player_id);
            }
            ui.separator();
            roster(ui, &lobby, &menu.player_id, playing);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let big = |text: &str| RichText::new(text).font(display_font(24.0));
                if host && !playing {
                    let label = if lobby.phase == RoomPhase::Results {
                        "PLAY AGAIN"
                    } else {
                        "START BATTLE"
                    };
                    let start = egui::Button::new(big(label).color(super::theme::INK_0)).fill(GOLD);
                    if ui.add(start).clicked() {
                        actions.push(Action::RoomStart);
                    }
                }
                if playing && ui.button(big("RESUME")).clicked() {
                    actions.push(Action::Resume);
                }
                if host && playing && ui.button(big("END BATTLE").color(RED)).clicked() {
                    menu.ended_round = Some(lobby.round_id);
                    actions.push(Action::RoomEnd);
                }
                let leave = if playing {
                    "LEAVE ROOM"
                } else {
                    "BATTLE SETUP"
                };
                if ui.button(big(leave)).clicked() {
                    actions.push(Action::RoomLeave);
                }
                if ui.button("SETTINGS").clicked() {
                    actions.push(Action::OpenSettings);
                }
            });
            if alone {
                ui.label(
                    RichText::new("You're the last player here, so leaving closes the room.")
                        .size(12.0)
                        .color(TEXT_DIM),
                );
            }
            egui::CollapsingHeader::new("Controls")
                .id_salt("room controls")
                .show(ui, |ui| {
                    egui::Grid::new("room controls grid").show(ui, |ui| {
                        for (what, keys, note) in menus::CONTROLS {
                            ui.label(RichText::new(keys.join(" ")).strong());
                            ui.label(RichText::new(format!("{what} {note}")).color(TEXT_DIM));
                            ui.end_row();
                        }
                    });
                });
        });
}

fn heading_hint(lobby: &Lobby, host: bool, alone: bool) -> String {
    let next = if lobby.phase == RoomPhase::Results {
        "the next battle"
    } else {
        "the battle"
    };
    let host_name = lobby
        .players
        .iter()
        .find(|player| player.player_id == lobby.host_id)
        .map_or("the host", |player| player.name.as_str());
    match lobby.phase {
        RoomPhase::Playing if lobby.settings.humans_only => {
            "The battle keeps going. Your tank sits idle and vulnerable while this menu is open."
                .into()
        }
        RoomPhase::Playing => {
            "The battle keeps going. A bot drives your tank while this menu is open.".into()
        }
        _ if !host => {
            format!("You stay in this room with everyone. Waiting for {host_name} to start {next}.")
        }
        _ if alone => format!(
            "You're the only one here. Copy the room code to invite friends, or start {next} on your own."
        ),
        RoomPhase::Results => {
            "Everyone stays in this room. Play again to keep the teams together.".into()
        }
        RoomPhase::Lobby => {
            "Everyone stays in this room. Start the battle to keep the teams together.".into()
        }
    }
}

/// NEXT BATTLE: the rules as text, which the host unfolds to change, and the player's
/// own team and tank.
fn next_battle(
    ui: &mut egui::Ui,
    menu: &mut RoomMenu,
    lobby: &Lobby,
    host: bool,
    debug: bool,
    actions: &mut Vec<Action>,
) {
    let settings = &lobby.settings;
    let map_id = settings.map_mode.as_str();
    let extra = EXTRA_MAPS.iter().any(|(id, _)| *id == map_id);
    ui.label(RichText::new("NEXT BATTLE").size(12.0).color(TEXT_LABEL));
    ui.horizontal(|ui| {
        let bots = if settings.humans_only {
            "No bots".to_owned()
        } else {
            let difficulty = settings.difficulty.as_str();
            format!(
                "{}{} bots",
                difficulty[..1].to_uppercase(),
                &difficulty[1..]
            )
        };
        ui.label(
            RichText::new(format!(
                "{}{} · {bots} · {} min",
                map_name(map_id),
                if extra { " · Extra level" } else { "" },
                settings.round_minutes
            ))
            .strong(),
        );
        if host {
            let label = if menu.editing_rules {
                "Done"
            } else {
                "Change rules"
            };
            if ui.small_button(label).clicked() {
                menu.editing_rules = !menu.editing_rules;
            }
        }
    });
    if host && menu.editing_rules {
        let current = RoomRules {
            map_mode: map_id.to_owned(),
            difficulty: settings.difficulty.as_str().to_owned(),
            humans_only: settings.humans_only,
            round_minutes: settings.round_minutes,
        };
        let mut rules = current.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Map");
            let offered = if debug || extra { &EXTRA_MAPS[..] } else { &[] };
            for (id, name) in MAPS.iter().chain(offered) {
                if ui.selectable_label(rules.map_mode == *id, *name).clicked() {
                    rules.map_mode = (*id).into();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Bots");
            for (id, name) in [
                ("easy", "Easy"),
                ("normal", "Normal"),
                ("hard", "Hard"),
                ("none", "None"),
            ] {
                let chosen = if rules.humans_only {
                    id == "none"
                } else {
                    rules.difficulty == id
                };
                if ui.selectable_label(chosen, name).clicked() {
                    // "None" keeps the last difficulty for when bots come back.
                    rules.humans_only = id == "none";
                    if id != "none" {
                        rules.difficulty = id.into();
                    }
                }
            }
            ui.add_space(12.0);
            ui.label("Minutes");
            if ui.small_button("−").clicked() {
                rules.round_minutes = stepped_minutes(rules.round_minutes, -LENGTH_STEP);
            }
            ui.label(RichText::new(rules.round_minutes.to_string()).strong());
            if ui.small_button("+").clicked() {
                rules.round_minutes =
                    stepped_minutes(rules.round_minutes, LENGTH_STEP).min(MAX_ROUND_MINUTES);
            }
        });
        if rules != current {
            actions.push(Action::RoomRules(rules));
        }
    }
    if let Some(mine) = menu.mine().cloned() {
        ui.horizontal(|ui| {
            ui.label("You:");
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    team_word(mine.team),
                    tank_name(mine.kind.as_str())
                ))
                .strong()
                .color(team_color(mine.team.index() as u8)),
            );
            let label = if menu.editing_choice {
                "Done"
            } else {
                "Change team or tank"
            };
            if ui.small_button(label).clicked() {
                menu.editing_choice = !menu.editing_choice;
            }
        });
        if menu.editing_choice {
            let team = Some(mine.team.index() as u8);
            let kind = mine.kind.as_str();
            ui.horizontal(|ui| {
                ui.label("Team");
                for (choice, name) in [(None, "Auto"), (Some(0), "Blue"), (Some(1), "Red")] {
                    let response = ui.selectable_label(choice.is_some() && choice == team, name);
                    if response.clicked() {
                        actions.push(Action::RoomChoose {
                            team: choice,
                            kind: kind.to_owned(),
                        });
                    }
                }
                ui.add_space(12.0);
                ui.label("Tank");
                for tank in TANKS {
                    let (id, name) = (tank.id, tank.name);
                    if ui.selectable_label(kind == id, name).clicked() && kind != id {
                        actions.push(Action::RoomChoose {
                            team,
                            kind: id.to_owned(),
                        });
                    }
                }
            });
        }
    }
}

/// LAST ROUND: everyone's kills and deaths, the best first.
fn scoreboard(ui: &mut egui::Ui, lobby: &Lobby, player_id: &str) {
    let mut rows = lobby.scoreboard.clone();
    rows.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
    egui::Grid::new("room scoreboard")
        .striped(true)
        .num_columns(3)
        .show(ui, |ui| {
            for label in ["LAST ROUND", "KILLS", "DEATHS"] {
                ui.label(RichText::new(label).size(12.0).color(TEXT_LABEL));
            }
            ui.end_row();
            for row in &rows {
                let name = RichText::new(&row.name).color(team_color(row.team.index() as u8));
                ui.label(if row.player_id == player_id {
                    name.strong()
                } else {
                    name
                });
                ui.label(row.kills.to_string());
                ui.label(row.deaths.to_string());
                ui.end_row();
            }
        });
}

/// Both teams' players, with the seats bots or nobody fill.
fn roster(ui: &mut egui::Ui, lobby: &Lobby, player_id: &str, playing: bool) {
    let extra = EXTRA_MAPS
        .iter()
        .any(|(id, _)| *id == lobby.settings.map_mode.as_str());
    let team_tanks = if extra && !lobby.settings.humans_only {
        EXTRA_TEAM_TANKS
    } else {
        STANDARD_TEAM_TANKS
    };
    ui.columns(2, |columns| {
        for (side, column) in columns.iter_mut().enumerate() {
            let members: Vec<&Player> = lobby
                .players
                .iter()
                .filter(|player| player.team.index() == side)
                .collect();
            let open = team_tanks.saturating_sub(members.len());
            let fill = match (lobby.settings.humans_only, open) {
                (true, 1) => "1 open seat".to_owned(),
                (true, open) => format!("{open} open seats"),
                (false, 1) => "1 bot".to_owned(),
                (false, open) => format!("{open} bots"),
            };
            column.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} TEAM", TEAM_NAMES[side]))
                        .strong()
                        .color(team_color(side as u8)),
                );
                ui.label(RichText::new(fill).size(11.0).color(TEXT_DIM));
            });
            for player in members {
                let tags: Vec<&str> = [
                    (player.player_id == player_id).then_some("YOU"),
                    (player.player_id == lobby.host_id).then_some("HOST"),
                    (!player.connected).then_some("RECONNECTING"),
                ]
                .into_iter()
                .flatten()
                .collect();
                column.horizontal(|ui| {
                    let name = RichText::new(&player.name);
                    ui.label(if player.connected {
                        name
                    } else {
                        name.color(TEXT_DIM)
                    });
                    if !tags.is_empty() {
                        ui.label(RichText::new(tags.join(" · ")).size(10.0).color(GOLD));
                    }
                    if playing {
                        ui.label(RichText::new(player.kills.to_string()).strong());
                    }
                });
            }
        }
    });
}

/// The compact kill list beside the HUD during a battle.
fn players(context: &egui::Context, menu: &RoomMenu, state: &UiState) {
    let Some(lobby) = &menu.lobby else { return };
    let kills = |player: &Player| {
        player
            .tank_id
            .and_then(|tank| {
                state
                    .hud
                    .as_ref()?
                    .scoreboard
                    .iter()
                    .find(|row| row.id == tank)
                    .map(|row| row.kills as u32)
            })
            .unwrap_or(player.kills)
    };
    let mut players: Vec<&Player> = lobby.players.iter().collect();
    players.sort_by_key(|player| (player.team.index(), player.slot));
    egui::Area::new(egui::Id::unique("room players"))
        .anchor(Align2::LEFT_TOP, [12.0, 12.0])
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                egui::Grid::new("room players grid").show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("PLAYERS · {}", players.len()))
                            .size(11.0)
                            .color(TEXT_LABEL),
                    );
                    ui.label(RichText::new("KILLS").size(11.0).color(TEXT_LABEL));
                    ui.end_row();
                    for player in players {
                        let you = if player.player_id == menu.player_id {
                            " (you)"
                        } else {
                            ""
                        };
                        let name = RichText::new(format!("{}{you}", player.name))
                            .color(team_color(player.team.index() as u8));
                        ui.label(if player.connected { name } else { name.weak() });
                        ui.label(kills(player).to_string());
                        ui.end_row();
                    }
                });
            });
        });
}

/// The countdown to the next life and who ended the last one.
fn respawn(context: &egui::Context, menu: &RoomMenu, state: &UiState) {
    let Some(hud) = &state.hud else { return };
    egui::Area::new(egui::Id::unique("room respawn"))
        .anchor(Align2::CENTER_CENTER, [0.0, -40.0])
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(format!("Respawn in {:.0}", hud.human.respawn.ceil()))
                            .font(display_font(32.0))
                            .color(RED),
                    );
                    if !menu.death_cause.is_empty() {
                        ui.label(RichText::new(&menu.death_cause).color(TEXT_DIM));
                    }
                });
            });
        });
}

/// The phase the HUD and the pause logic see.
pub fn hud_phase(menu: &RoomMenu) -> Phase {
    match menu.phase() {
        RoomPhase::Playing if menu.in_battle() => Phase::Playing,
        RoomPhase::Playing => Phase::Paused,
        RoomPhase::Results => Phase::Results,
        RoomPhase::Lobby => Phase::Ready,
    }
}
