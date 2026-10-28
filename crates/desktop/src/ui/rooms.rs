//! Battle Setup's Multiplayer tab (`src/net/room-browser.ts`, the room panel of
//! `battle-setup.html`): the open rooms the server directory lists, polled every few
//! seconds while the tab shows, a new room's rules, the player's name and team, and
//! CREATE ROOM or JOIN ROOM. A room code typed in stands in for the browser's invite link.

use std::time::{Duration, Instant};

use egui::RichText;

use super::setup::{EXTRA_MAPS, MAPS};
use super::theme::{self, GOLD, RED, TEXT_DIM, display_font};
use super::{Action, View};
use crate::directory::{
    self, DEFAULT_DIRECTORY, DirectoryRequest, ROOM_SEATS, RoomListing, ServerList,
};
use crate::preferences::Preferences;
use crate::room_api::{RoomRules, RoomSelection, preferred_player_name};
use sloppy_core::net::protocol::{DEFAULT_ROUND_MINUTES, MAX_PLAYER_NAME_LENGTH};

/// The list refreshes this often while the tab shows.
const REFRESH: Duration = Duration::from_secs(5);
/// Minutes one press of the length stepper moves.
const LENGTH_STEP: i32 = 5;

/// Where the room list stands.
#[derive(Clone, Debug, Default, PartialEq)]
enum ListState {
    #[default]
    Loading,
    Ready,
    /// The directory did not answer; the next try is at `retry`.
    Offline {
        reason: String,
        retry: Instant,
    },
}

/// The tab's choices and the directory's latest answer. It outlives a room, so Battle
/// Setup comes back with the room selected and the reason it ended.
pub struct RoomsState {
    /// The Multiplayer tab is the one showing.
    pub multiplayer: bool,
    /// The directory's base address (`--directory`, else the build's).
    pub directory: String,
    request: Option<DirectoryRequest>,
    /// When to ask again; `None`: now.
    next_poll: Option<Instant>,
    list: ServerList,
    state: ListState,
    /// The open room chosen, or empty for the new room.
    selected: String,
    /// `None` until first shown, then the name typed (saved on joining).
    name: Option<String>,
    /// `None`: the side with fewer humans.
    team: Option<u8>,
    round_minutes: u32,
    humans_only: bool,
    /// The room code typed in.
    code: String,
    /// A room to select once the directory lists it: one just left, or a typed code.
    link: Option<String>,
    /// Shown instead of the usual prompt until the player picks a room.
    notice: String,
}

impl Default for RoomsState {
    fn default() -> Self {
        Self {
            multiplayer: false,
            directory: DEFAULT_DIRECTORY.to_owned(),
            request: None,
            next_poll: None,
            list: ServerList::default(),
            state: ListState::Loading,
            selected: String::new(),
            name: None,
            team: None,
            round_minutes: DEFAULT_ROUND_MINUTES,
            humans_only: true,
            code: String::new(),
            link: None,
            notice: String::new(),
        }
    }
}

impl RoomsState {
    /// Come back from `room` with `notice` saying why: the tab shows with the room
    /// selected once the directory lists it.
    pub fn returned_from(&mut self, room: &str, notice: &str) {
        self.multiplayer = true;
        self.link = (!room.is_empty()).then(|| room.to_owned());
        self.notice = notice.to_owned();
        self.next_poll = None;
    }

    /// Ask the directory when it is time, and take its answer once it comes.
    fn poll(&mut self, extra_levels: bool) {
        let now = Instant::now();
        if let Some(answer) = self.request.as_ref().and_then(DirectoryRequest::answer) {
            self.request = None;
            self.next_poll = Some(now + REFRESH);
            match answer {
                Ok(mut list) => {
                    let link = self.link.clone();
                    list.rooms.retain(|room| {
                        extra_levels
                            || !EXTRA_MAPS.iter().any(|(id, _)| *id == room.map_mode)
                            || link.as_deref() == Some(room.room.as_str())
                    });
                    self.list = list;
                    self.state = ListState::Ready;
                    self.follow_link();
                }
                Err(reason) => {
                    self.list = ServerList::default();
                    self.state = ListState::Offline {
                        reason,
                        retry: now + REFRESH,
                    };
                }
            }
            if !self
                .list
                .rooms
                .iter()
                .any(|room| room.room == self.selected && room.joinable())
            {
                self.selected.clear();
            }
        }
        if self.request.is_none() && self.next_poll.is_none_or(|at| now >= at) {
            self.request = Some(DirectoryRequest::start(&self.directory));
            if self.state != ListState::Ready {
                self.state = ListState::Loading;
            }
        }
    }

    /// Select the linked room if it can take a player; otherwise say why not. A room
    /// made moments ago reaches the directory a little later, so the link keeps
    /// watching until its room shows up or the player chooses.
    fn follow_link(&mut self) {
        let Some(link) = self.link.clone() else {
            return;
        };
        let listing = self.list.rooms.iter().find(|room| room.room == link);
        if listing.is_some() {
            self.link = None;
        }
        let reason = match listing {
            Some(room) if room.joinable() => {
                self.selected = link.clone();
                None
            }
            Some(_) => Some(format!("Room {link} can't take another player right now.")),
            None => Some(format!(
                "Room {link} isn't open. Choose another room or create one."
            )),
        };
        if let Some(reason) = reason {
            self.notice = if self.notice.is_empty() || self.notice.starts_with("Room ") {
                reason
            } else {
                format!("{}\n{reason}", self.notice)
            };
        }
    }

    fn choose(&mut self, room: &str) {
        self.link = None;
        self.notice.clear();
        self.selected = room.to_owned();
    }

    /// The status line and its hint: what the action button will do, or what is wrong.
    fn prompt(&self) -> (String, String) {
        match &self.state {
            ListState::Loading if self.list.servers.is_empty() => (
                "Looking for rooms…".into(),
                "Pick your tank and map meanwhile.".into(),
            ),
            ListState::Offline { reason, retry } => {
                let seconds = retry
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64()
                    .ceil()
                    .max(1.0);
                (
                    "Reconnecting to the game server".into(),
                    format!("{reason} · next try in {seconds} s"),
                )
            }
            _ if !self.notice.is_empty() => (self.notice.clone(), String::new()),
            _ if !self.selected.is_empty() => (
                format!("Ready to join room {}", self.selected),
                "Hit JOIN ROOM to play.".into(),
            ),
            _ if self.list.new_room_server().is_none() => (
                "Update Sloppy Tanks to play online".into(),
                "The game server runs another version.".into(),
            ),
            _ => (
                "Ready to create a room".into(),
                if self.list.rooms.is_empty() {
                    "Copy its code in the room to invite friends.".into()
                } else {
                    "Or pick an open room to join.".into()
                },
            ),
        }
    }
}

/// The map's name for a map id.
pub fn map_name(id: &str) -> &str {
    MAPS.iter()
        .chain(&EXTRA_MAPS)
        .find(|(map, _)| *map == id)
        .map_or(id, |(_, name)| name)
}

fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::bold_font(13.0))
            .color(theme::TEXT_LABEL),
    );
}

/// One open room's lines (`RoomBrowser.render`).
fn room_lines(room: &RoomListing) -> (String, String) {
    let title = format!(
        "{}{} · {}/{ROOM_SEATS} players",
        map_name(&room.map_mode),
        if EXTRA_MAPS.iter().any(|(id, _)| *id == room.map_mode) {
            " (EXTRA)"
        } else {
            ""
        },
        room.players
    );
    let seconds = room.time.ceil() as u64;
    let phase = match room.phase.as_str() {
        "playing" => format!(
            "{}:{:02} left · {}",
            seconds / 60,
            seconds % 60,
            room.scores
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join("–")
        ),
        "lobby" => "In lobby".into(),
        _ => "Between rounds".into(),
    };
    let bots = if room.humans_only {
        "Humans only".to_owned()
    } else {
        format!("Bots: {}", room.difficulty)
    };
    let mut detail = format!(
        "{} · {bots} · {} min · {phase}",
        room.room, room.round_minutes
    );
    if room.reserved > room.players {
        detail += &format!(" · {} reconnecting", room.reserved - room.players);
    }
    if room.content_version != sloppy_core::net::protocol::CONTENT_VERSION {
        detail += " · Update the game to join";
    } else if room.reserved >= ROOM_SEATS {
        detail += " · Full";
    }
    (title, detail)
}

/// The Multiplayer tab below the tank and map choices, in place of single player's
/// battle format, difficulty and GO.
pub fn multiplayer(
    ui: &mut egui::Ui,
    state: &mut RoomsState,
    view: &View,
    prefs: &Preferences,
    actions: &mut Vec<Action>,
) {
    state.poll(view.debug);
    let name = state
        .name
        .get_or_insert_with(|| {
            preferred_player_name(
                &prefs.player_name,
                u64::from_le_bytes(directory::random_bytes()),
            )
        })
        .clone();
    ui.horizontal(|ui| {
        label(ui, &format!("ROOMS · {}", state.list.rooms.len()));
        let busy = state.request.is_some();
        if ui
            .add_enabled(!busy, egui::Button::new("⟳").small())
            .on_hover_text("Refresh rooms")
            .clicked()
        {
            state.next_poll = None;
        }
    });
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let new_room = format!("New room on {}", map_name(&prefs.map));
        if ui
            .radio(state.selected.is_empty(), RichText::new(new_room).strong())
            .clicked()
        {
            state.choose("");
        }
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new("Length").color(TEXT_DIM));
            let mut minutes = state.round_minutes;
            if ui.small_button("−").clicked() {
                minutes = directory::stepped_minutes(minutes, -LENGTH_STEP);
            }
            ui.label(RichText::new(format!("{minutes} min")).strong());
            if ui.small_button("+").clicked() {
                minutes = directory::stepped_minutes(minutes, LENGTH_STEP);
            }
            ui.add_space(12.0);
            let mut humans_only = state.humans_only;
            ui.checkbox(&mut humans_only, "Humans only")
                .on_hover_text("No bots fill the teams; empty seats stay empty");
            if minutes != state.round_minutes || humans_only != state.humans_only {
                state.round_minutes = minutes;
                state.humans_only = humans_only;
                // Editing the new room's rules chooses the new room.
                state.choose("");
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(150.0)
            .auto_shrink([false, true])
            .show(ui, |ui| room_rows(ui, state));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Join by code").color(TEXT_DIM));
        let field = ui.add(
            egui::TextEdit::singleline(&mut state.code)
                .char_limit(8)
                .desired_width(110.0)
                .hint_text("ABCD2345"),
        );
        let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if ui.button("FIND").clicked() || entered {
            match directory::read_room_code(&state.code) {
                Some(code) => {
                    state.code.clone_from(&code);
                    state.notice.clear();
                    state.link = Some(code);
                    state.follow_link();
                    if state.link.is_some() {
                        // Not listed yet: look again now.
                        state.next_poll = None;
                    }
                }
                None => state.notice = "A room code is 8 letters and digits.".into(),
            }
        }
    });
    ui.horizontal(|ui| {
        label(ui, "YOUR NAME");
        let mut typed = name.clone();
        ui.add(
            egui::TextEdit::singleline(&mut typed)
                .char_limit(MAX_PLAYER_NAME_LENGTH)
                .desired_width(180.0)
                .hint_text("Tank driver"),
        );
        state.name = Some(typed);
        ui.add_space(8.0);
        label(ui, "TEAM");
        for (team, text) in [(None, "Auto"), (Some(0), "Blue"), (Some(1), "Red")] {
            let response = ui.selectable_label(state.team == team, text);
            let response = if team.is_none() {
                response.on_hover_text("Joins the side with fewer players")
            } else {
                response
            };
            if response.clicked() {
                state.team = team;
            }
        }
    });
    ui.add_space(6.0);
    let (message, hint) = state.prompt();
    ui.vertical_centered(|ui| {
        let offline = matches!(state.state, ListState::Offline { .. });
        ui.label(RichText::new(message).color(if offline { RED } else { theme::TEXT }));
        if !hint.is_empty() {
            ui.label(RichText::new(hint).size(12.0).color(TEXT_DIM));
        }
        ui.add_space(4.0);
        let joining = !state.selected.is_empty();
        let label = if joining { "JOIN ROOM" } else { "CREATE ROOM" };
        let enabled = if joining {
            true
        } else {
            state.list.new_room_server().is_some()
        };
        let button = egui::Button::new(
            RichText::new(label)
                .font(display_font(28.0))
                .color(theme::INK_0),
        )
        .fill(GOLD)
        .min_size([220.0, 46.0].into());
        if ui.add_enabled(enabled, button).clicked()
            && let Some(selection) = selection(state, prefs)
        {
            actions.push(Action::JoinRoom(Box::new(selection)));
        }
    });
}

fn room_rows(ui: &mut egui::Ui, state: &mut RoomsState) {
    if state.list.rooms.is_empty() {
        ui.vertical_centered(|ui| match &state.state {
            ListState::Loading => {
                ui.add(egui::Spinner::new());
                ui.label(RichText::new("Scanning for open rooms…").color(TEXT_DIM));
            }
            ListState::Ready => {
                ui.label(RichText::new("No open rooms right now").strong());
                ui.label(
                    RichText::new("Be the first: create a room and invite friends.")
                        .size(12.0)
                        .color(TEXT_DIM),
                );
            }
            ListState::Offline { .. } => {
                ui.label(RichText::new("Can't reach the game server").strong());
                ui.label(
                    RichText::new("Rooms list here once it answers.")
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                ui.horizontal(|ui| {
                    if ui.button("BATTLE THE BOTS").clicked() {
                        state.multiplayer = false;
                    }
                    if ui.button("RETRY NOW").clicked() {
                        state.next_poll = None;
                    }
                });
            }
        });
        return;
    }
    let rooms = state.list.rooms.clone();
    for room in &rooms {
        let (title, detail) = room_lines(room);
        ui.add_enabled_ui(room.joinable(), |ui| {
            if ui
                .radio(state.selected == room.room, RichText::new(title).strong())
                .clicked()
            {
                state.choose(&room.room);
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new(detail).size(12.0).color(TEXT_DIM));
        });
    }
}

/// What CREATE ROOM or JOIN ROOM asks for, or `None` when the choice cannot play.
fn selection(state: &mut RoomsState, prefs: &Preferences) -> Option<RoomSelection> {
    let name = state.name.clone().unwrap_or_default().trim().to_owned();
    if name.is_empty() {
        state.notice = "Enter a name to play.".into();
        return None;
    }
    let (room, server, create) = if state.selected.is_empty() {
        let server = state.list.new_room_server()?.to_owned();
        let create = RoomRules {
            map_mode: prefs.map.clone(),
            difficulty: "normal".into(),
            humans_only: state.humans_only,
            round_minutes: state.round_minutes,
        };
        (
            directory::room_code(directory::random_bytes()),
            server,
            Some(create),
        )
    } else {
        let listing = state
            .list
            .rooms
            .iter()
            .find(|room| room.room == state.selected && room.joinable())?;
        (listing.room.clone(), listing.server.clone(), None)
    };
    Some(RoomSelection {
        room,
        server,
        name,
        kind: prefs.tank.clone(),
        team: state.team,
        create,
    })
}
