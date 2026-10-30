//! Battle Setup's Multiplayer tab (`src/net/room-browser.ts`, the room panel of
//! `battle-setup.html`): the open rooms the server directory lists, polled every few
//! seconds while the tab shows, a new room's rules, the player's name and team, and
//! CREATE ROOM or JOIN ROOM. A room code typed in finds its room as an invite link
//! (`?room=`, `links.rs`) does.

use std::time::Duration;

use web_time::Instant;

use egui::{
    Align2, Color32, Id, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, pos2,
    vec2,
};

use super::icons::Art;
use super::paint::{self, ButtonStyle, fade, hex, hexa, mix};
use super::theme::{GOLD, bold_font, display_font};
use super::{Action, View};
use crate::directory::{self, DirectoryRequest};
use crate::links::{self, Place};
use crate::preferences::Preferences;
use crate::room_api::{RoomSelection, player_name, preferred_player_name};
use sloppy_core::net::protocol::{
    DEFAULT_ROUND_MINUTES, JoinChoice, MAX_PLAYER_NAME_LENGTH, RoomPhase, RoomSettings,
    read_room_code, room_code, stepped_minutes,
};
use sloppy_core::net::room_list::{HostedRoom, ROOM_SEATS, ServerList};
use sloppy_core::sim::Team;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::{MapId, is_extra_level, map_option_for};
use sloppy_web::battle_report::duration;

/// The list refreshes this often while the tab shows.
/// Room the name field keeps at its right end for its pencil.
const PENCIL_ROOM: f32 = 18.0;
const REFRESH: Duration = Duration::from_secs(5);
/// Minutes one press of the length stepper moves.
const LENGTH_STEP: i32 = 5;

/// The tab's words where a page and the app differ: a page invites with its link and
/// takes a new game version by loading again (`battle-setup.html`, `room-browser.ts`); a
/// friend types the app's room code, and the app itself must be updated.
struct Words {
    invite_card: &'static str,
    invite_hint: &'static str,
    outdated_prompt: &'static str,
    outdated_row: &'static str,
}

const WORDS: Words = if cfg!(target_arch = "wasm32") {
    Words {
        invite_card: "Invite friends with its link",
        invite_hint: "Share its link to invite friends.",
        outdated_prompt: "Reload for updated game",
        outdated_row: "Reload for updated game",
    }
} else {
    Words {
        invite_card: "Invite friends with its room code",
        invite_hint: "Copy its code in the room to invite friends.",
        outdated_prompt: "Update Sloppy Tanks to play online",
        outdated_row: "Update the game to join",
    }
};

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
    team: Option<Team>,
    round_minutes: u32,
    humans_only: bool,
    /// The room code typed in.
    code: String,
    /// A room to select once the directory lists it: one just left, or a typed code.
    link: Option<String>,
    /// The room a code or a return named, which stays listed even on an extra level
    /// after the link is followed (the page's fixed `linkedRoom`).
    linked_room: Option<String>,
    /// Shown instead of the usual prompt until the player picks a room.
    notice: String,
    /// Why the player came back from a room: the notice's first line while the room's
    /// link waits, above what the directory says of the room (the page's `link.notice`).
    return_notice: String,
    /// The tab the page's address names: `None` until the player switches, as the address
    /// a link or a room left stays until then (`rememberPlayMode`).
    addressed: Option<bool>,
}

impl Default for RoomsState {
    fn default() -> Self {
        Self {
            multiplayer: false,
            directory: directory::default_directory(),
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
            linked_room: None,
            notice: String::new(),
            return_notice: String::new(),
            addressed: None,
        }
    }
}

impl RoomsState {
    #[cfg(test)]
    pub(super) fn snapshot_player_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[cfg(test)]
    pub(super) fn snapshot_rooms(&self) -> Vec<serde_json::Value> {
        self.list
            .rooms
            .iter()
            .map(|room| {
                serde_json::json!({
                    "room": room.room, "contentVersion": room.content_version,
                    "mapMode": room.map_mode, "difficulty": room.difficulty,
                    "humansOnly": room.humans_only, "roundMinutes": room.round_minutes,
                    "players": room.players, "reserved": room.reserved, "phase": room.phase,
                    "roundId": room.round_id, "time": room.time, "scores": room.scores,
                    "server": room.server,
                })
            })
            .collect()
    }

    /// Come back from `room` with `notice` saying why: the tab shows with the room
    /// selected once the directory lists it.
    pub fn returned_from(&mut self, room: &str, notice: &str) {
        self.multiplayer = true;
        self.link = (!room.is_empty()).then(|| room.to_owned());
        self.linked_room.clone_from(&self.link);
        self.notice = notice.to_owned();
        self.return_notice = notice.to_owned();
        self.next_poll = None;
    }

    /// Open on the tab as a link asks (`initialPlayMode`): `?multiplayer`, or an invite
    /// link's `room`, selected once the directory lists it (`linkedRoom`).
    pub fn open_link(&mut self, room: Option<&str>) {
        self.returned_from(room.unwrap_or_default(), "");
    }

    /// Keep the page's address on the tab showing, so that a reload opens it again.
    pub fn remember_tab(&mut self) {
        if *self.addressed.get_or_insert(self.multiplayer) != self.multiplayer {
            self.addressed = Some(self.multiplayer);
            links::show(if self.multiplayer {
                Place::Multiplayer
            } else {
                Place::Single
            });
        }
    }

    /// Select the room with `code` once the directory lists it (a typed code).
    fn find_room(&mut self, code: String) {
        self.notice.clear();
        self.return_notice.clear();
        self.link = Some(code);
        self.linked_room.clone_from(&self.link);
        self.follow_link();
        if self.link.is_some() {
            // Not listed yet: look again now.
            self.next_poll = None;
        }
    }

    /// Ask the directory when it is time, and take its answer once it comes.
    fn poll(&mut self, extra_levels: bool) {
        let now = Instant::now();
        if let Some(answer) = self.request.as_ref().and_then(DirectoryRequest::answer) {
            self.request = None;
            self.listed(answer, extra_levels, now);
        }
        if self.request.is_none() && self.next_poll.is_none_or(|at| now >= at) {
            self.request = Some(DirectoryRequest::start(&self.directory));
            if self.state != ListState::Ready {
                self.state = ListState::Loading;
            }
        }
    }

    /// Take the directory's answer at `now`. Rooms on extra levels show only with
    /// `extra_levels`, or when a code or a return named them.
    fn listed(&mut self, answer: Result<ServerList, String>, extra_levels: bool, now: Instant) {
        self.next_poll = Some(now + REFRESH);
        match answer {
            Ok(mut list) => {
                let linked = self.linked_room.clone();
                list.rooms.retain(|room| {
                    extra_levels
                        || !is_extra_level(room.map_mode)
                        || linked.as_deref() == Some(room.room.as_str())
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
        if crate::touch::phone() {
            self.pick_for_phone();
        }
    }

    /// A phone shows no room list (`autoPick`): each answer picks again, as rooms fill,
    /// empty and finish their battles, unless a room link chose the room.
    fn pick_for_phone(&mut self) {
        let linked = !self.selected.is_empty()
            && self.linked_room.as_deref() == Some(self.selected.as_str());
        if !linked {
            self.selected = busiest_open_room(&self.list.rooms)
                .map(|room| room.room.clone())
                .unwrap_or_default();
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
        let state = match listing {
            Some(room) if room.joinable() => {
                self.selected = link.clone();
                None
            }
            Some(_) => Some(format!("Room {link} can't take another player right now.")),
            None => Some(format!(
                "Room {link} isn't open. Choose another room or create one."
            )),
        };
        if let Some(state) = state {
            self.notice = if self.return_notice.is_empty() {
                state
            } else {
                format!("{}\n{state}", self.return_notice)
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
                WORDS.outdated_prompt.into(),
                "The game server runs another version.".into(),
            ),
            _ => (
                "Ready to create a room".into(),
                if self.list.rooms.is_empty() {
                    WORDS.invite_hint.into()
                } else {
                    "Or pick an open room to join.".into()
                },
            ),
        }
    }
}

/// One open room's lines (`RoomBrowser.render`).
fn room_lines(room: &HostedRoom) -> (String, String) {
    let title = format!(
        "{}{} · {}/{ROOM_SEATS} players",
        map_option_for(room.map_mode).name,
        if is_extra_level(room.map_mode) {
            " (EXTRA)"
        } else {
            ""
        },
        room.players
    );
    let phase = match room.phase {
        RoomPhase::Playing => format!(
            "{} left · {}",
            duration(room.time.ceil()),
            room.scores
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join("–")
        ),
        RoomPhase::Lobby => "In lobby".into(),
        RoomPhase::Results => "Between rounds".into(),
    };
    let bots = if room.humans_only {
        "Humans only".to_owned()
    } else {
        format!("Bots: {}", room.difficulty.as_str())
    };
    let mut detail = format!(
        "{} · {bots} · {} min · {phase}",
        room.room, room.round_minutes
    );
    if room.reserved > room.players {
        detail += &format!(" · {} reconnecting", room.reserved - room.players);
    }
    if room.content_version != sloppy_core::net::protocol::CONTENT_VERSION {
        detail += &format!(" · {}", WORDS.outdated_row);
    } else if room.reserved >= ROOM_SEATS {
        detail += " · Full";
    }
    (title, detail)
}

/// An open room's row.
const ROOM_ROW_HEIGHT: f32 = 34.0;
/// Light grey-blue of the page's small print in the room list.
const SMALL_INK: u32 = 0xb6cedf;

/// The teams: choice, name, line and the colours of its dot.
const TEAMS: [(Option<Team>, &str, &str, [u32; 2]); 3] = [
    (
        None,
        "Auto",
        "Joins the side with fewer players",
        [0x4ea3ff, 0xff6f5e],
    ),
    (
        Some(Team::Blue),
        "Blue",
        "Always the blue team",
        [0x4ea3ff, 0x4ea3ff],
    ),
    (
        Some(Team::Red),
        "Red",
        "Always the red team",
        [0xff6f5e, 0xff6f5e],
    ),
];

impl RoomsState {
    /// The status dot's colour: red offline, a pulsing gold while the first list loads,
    /// green once it answers.
    fn status_color(&self, time: f64) -> Color32 {
        match self.state {
            ListState::Offline { .. } => hex(0xff6f5e),
            ListState::Loading if self.list.servers.is_empty() => {
                mix(GOLD, hex(0xffe58a), (0.5 + 0.5 * (time * 4.0).sin()) as f32)
            }
            _ => hex(0x8ff0b0),
        }
    }

    /// The map of the open room chosen, which the map row shows instead of the player's.
    pub fn selected_map(&self) -> Option<MapId> {
        self.list
            .rooms
            .iter()
            .find(|room| room.room == self.selected)
            .map(|room| room.map_mode)
    }
}

/// A drawn radio: a dark socket that lights a gold dot when chosen.
fn radio(painter: &Painter, center: Pos2, chosen: bool) {
    painter.circle_filled(center, 9.0, hex(0x050d1c));
    if chosen {
        painter.circle_filled(center, 9.0, fade(GOLD, 0.25));
        painter.circle_filled(center, 5.0, GOLD);
        painter.circle_filled(center, 4.0, hex(0xffe58a));
        painter.circle_stroke(center, 8.25, Stroke::new(1.5, GOLD));
    } else {
        painter.circle_stroke(center, 8.25, Stroke::new(1.5, hexa(0x8bb9dc, 0.4)));
    }
}

/// A sunken well: the page's dark inset panels (`--well`).
fn well(painter: &Painter, rect: Rect, radius: u8) {
    painter.rect_filled(rect, radius, hexa(0x071328, 0.85));
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, hexa(0x000000, 0.35)),
        StrokeKind::Inside,
    );
}

/// A small square button with `glyph` drawn by `draw`; true when clicked.
fn icon_button(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &str,
    enabled: bool,
    tip: &str,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> bool {
    let response = ui
        .interact(rect, Id::unique(("rooms button", id)), Sense::click())
        .on_hover_text(tip);
    paint::labelled(&response, enabled, tip);
    let painter = ui.painter();
    let hovered = enabled && response.hovered();
    painter.rect_filled(rect, 7, hexa(0xb7d7ed, if hovered { 0.16 } else { 0.08 }));
    let ink = if !enabled {
        hexa(0xb6cedf, 0.4)
    } else if hovered {
        Color32::WHITE
    } else {
        hex(SMALL_INK)
    };
    draw(painter, rect, ink);
    enabled && response.clicked()
}

/// A text field in a sunken well, like the page's inputs, leaving `icon_room` at its
/// right end for an icon drawn over the well.
fn field(
    ui: &mut egui::Ui,
    rect: Rect,
    icon_room: f32,
    text: &mut String,
    limit: usize,
    hint: &str,
    size: f32,
) -> egui::Response {
    well(ui.painter(), rect, 9);
    let mut inner = rect.shrink2(vec2(12.0, 0.0));
    inner.max.x -= icon_room;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child
        .centered_and_justified(|ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .frame(egui::Frame::NONE)
                    .char_limit(limit)
                    .font(bold_font(size))
                    .text_color(hex(0xf5faff))
                    .hint_text(RichText::new(hint).color(hexa(0xb6cedf, 0.6)))
                    .align(egui::Align2::LEFT_CENTER),
            )
        })
        .inner
}

/// The Multiplayer tab under the map row, where single player shows its battle format,
/// difficulty and GO (`#multiplayer-panel`, laid out as the page at 1280 × 800).
pub fn multiplayer(
    ui: &mut egui::Ui,
    art: &Art,
    state: &mut RoomsState,
    view: &View,
    prefs: &Preferences,
    bounds: Rect,
    actions: &mut Vec<Action>,
) {
    // The two columns share Battle Setup's geometry; widening the sheet adds room
    // without scaling its fonts or its controls.
    let extra_height = bounds.height() - 363.0;
    let right_width = 302.0 + (bounds.width() - 842.0) * 1.22 / 3.22;
    let right_left = bounds.right() - right_width;
    let rooms_head = bounds.min;
    let room_list = Rect::from_min_max(
        bounds.min + vec2(0.0, 27.0),
        pos2(right_left - 23.0, bounds.bottom() - 76.0),
    );
    let new_room = Rect::from_min_size(
        room_list.min + vec2(5.0, 5.0),
        vec2(room_list.width() - 10.0, 89.0),
    );
    let status_well = Rect::from_min_max(
        pos2(bounds.left(), bounds.bottom() - 62.0),
        pos2(room_list.right(), bounds.bottom()),
    );
    let name_field = Rect::from_min_size(
        pos2(right_left, bounds.top() + 27.0),
        vec2(right_width, 42.0),
    );
    let team_well = Rect::from_min_max(
        pos2(right_left, bounds.top() + 110.0),
        pos2(bounds.right(), bounds.bottom() - 75.0),
    );
    let action = Rect::from_min_size(
        pos2(right_left, bounds.bottom() - 62.0),
        vec2(right_width, 62.0),
    );
    state.poll(view.debug);
    // A new room opens on the map single player would play: a saved extra level only
    // with the debug tools.
    let new_room_map = prefs.game_options(Team::Blue, view.debug).map_mode;
    let time = ui.input(|input| input.time);
    let painter = ui.painter().clone();

    // ROOMS, its count and refresh, and a room code typed in (the page's invite link).
    let heading = paint::section_label(&painter, rooms_head, "ROOMS");
    let count = state.list.rooms.len().to_string();
    let badge = Rect::from_min_size(
        pos2(heading.right() + 8.0, rooms_head.y + 2.0),
        vec2(
            paint::text_width(&painter, &count, bold_font(12.0), 0.0) + 10.0,
            17.0,
        ),
    );
    painter.rect_filled(badge, 4, hexa(0xb7d7ed, 0.11));
    paint::text(
        &painter,
        badge.center(),
        Align2::CENTER_CENTER,
        &count,
        bold_font(12.0),
        hex(0xc4dfef),
        0.0,
    );
    let refresh = Rect::from_min_size(
        pos2(badge.right() + 8.0, rooms_head.y - 2.0),
        vec2(24.0, 24.0),
    );
    let busy = state.request.is_some();
    if icon_button(
        ui,
        refresh,
        "refresh",
        !busy,
        "Refresh rooms",
        |painter, rect, ink| {
            let center = rect.center();
            let turn = if busy { time as f32 * 6.0 } else { 0.0 };
            paint::arc(
                painter,
                center,
                5.5,
                -2.4 + turn,
                4.6,
                Stroke::new(1.8, ink),
            );
            let tip = center + vec2((2.2 + turn).cos(), (2.2 + turn).sin()) * 5.5;
            painter.add(Shape::convex_polygon(
                vec![
                    tip + vec2(-3.0, -1.0),
                    tip + vec2(2.5, -2.5),
                    tip + vec2(0.5, 3.0),
                ],
                ink,
                Stroke::NONE,
            ));
        },
    ) {
        state.next_poll = None;
    }
    let code_field = Rect::from_min_size(
        pos2(room_list.right() - 169.0, rooms_head.y - 3.0),
        vec2(102.0, 26.0),
    );
    let typed = field(ui, code_field, 0.0, &mut state.code, 8, "Room code", 13.0);
    let entered = typed.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    let find = Rect::from_min_size(
        pos2(room_list.right() - 61.0, rooms_head.y - 3.0),
        vec2(61.0, 26.0),
    );
    let found = paint::button_at(
        ui,
        art,
        find,
        "FIND",
        None,
        ButtonStyle::Secondary,
        !state.code.trim().is_empty(),
    )
    .on_hover_text("Find the room with this code")
    .clicked();
    if found || entered {
        match read_room_code(&state.code) {
            Some(code) => {
                state.code.clone_from(&code);
                state.find_room(code);
            }
            None => state.notice = "A room code is 8 letters and digits.".into(),
        }
    }

    // The room list: the new room's card, then the open rooms or why there are none.
    well(&painter, room_list, 14);
    new_room_card(ui, state, new_room_map, new_room);
    let rows = Rect::from_min_max(
        pos2(new_room.left(), new_room.bottom() + 6.0),
        pos2(new_room.right(), room_list.bottom() - 5.0),
    );
    if state.list.rooms.is_empty() {
        empty_rooms(ui, art, rows, state, time);
    } else {
        let mut rows_ui = ui.new_child(egui::UiBuilder::new().max_rect(rows));
        egui::ScrollArea::vertical()
            .id_salt("open rooms")
            .max_height(rows.height())
            .auto_shrink([false, false])
            .show(&mut rows_ui, |ui| room_rows(ui, state));
    }

    // The status well: what the action will do, or what is wrong.
    let (message, hint) = state.prompt();
    painter.rect_filled(status_well, 10, hex(0x0a1a33));
    painter.rect_stroke(
        status_well,
        10,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.08)),
        StrokeKind::Inside,
    );
    let dot = pos2(status_well.left() + 22.0, status_well.center().y);
    let dot_color = state.status_color(time);
    painter.circle_filled(dot, 10.0, fade(dot_color, 0.2));
    painter.circle_filled(dot, 4.5, dot_color);
    let copy_left = status_well.left() + 42.0;
    let copy_width = status_well.right() - copy_left - 12.0;
    // Two rows at most: a notice's last two lines (why the player came back, then the
    // linked room's state, as the page's well wrapped them), or a line and its hint.
    let lines: Vec<&str> = message.lines().collect();
    let shown = &lines[lines
        .len()
        .saturating_sub(if hint.is_empty() { 2 } else { 1 })..];
    let first_y = if shown.len() + usize::from(!hint.is_empty()) > 1 {
        status_well.top() + 22.0
    } else {
        status_well.center().y
    };
    for (row, line) in shown.iter().enumerate() {
        paint::text_fit(
            &painter,
            pos2(copy_left, first_y + row as f32 * 17.0),
            Align2::LEFT_CENTER,
            line,
            egui::FontId::proportional(12.5),
            hex(0xe6f1fb),
            0.0,
            copy_width,
        );
    }
    if !hint.is_empty() {
        paint::text_fit(
            &painter,
            pos2(copy_left, status_well.top() + 39.0),
            Align2::LEFT_CENTER,
            &hint,
            egui::FontId::proportional(12.0),
            hex(0xa9c0d8),
            0.0,
            copy_width,
        );
    }

    // YOUR NAME.
    paint::section_label(&painter, pos2(name_field.left(), rooms_head.y), "YOUR NAME");
    let mut name = state
        .name
        .get_or_insert_with(|| {
            preferred_player_name(
                &prefs.player_name,
                u64::from_le_bytes(directory::random_bytes()),
            )
        })
        .clone();
    // The pencil sits inside the field, at its right end.
    field(
        ui,
        name_field,
        PENCIL_ROOM,
        &mut name,
        MAX_PLAYER_NAME_LENGTH,
        "Tank driver",
        15.0,
    );
    painter.rect_stroke(
        name_field,
        9,
        Stroke::new(0.5, hexa(0x92b4cf, 0.19)),
        StrokeKind::Inside,
    );
    // The pencil that says the name can be typed over.
    let pencil = pos2(name_field.right() - 18.0, name_field.center().y);
    let ink = Stroke::new(1.6, hex(SMALL_INK));
    painter.line_segment([pencil + vec2(-5.0, 5.0), pencil + vec2(4.0, -4.0)], ink);
    painter.line_segment([pencil + vec2(-5.0, 5.0), pencil + vec2(-6.0, 6.5)], ink);
    painter.line_segment([pencil + vec2(2.5, -5.5), pencil + vec2(5.5, -2.5)], ink);
    state.name = Some(name);

    // TEAM.
    paint::section_label(
        &painter,
        pos2(team_well.left(), team_well.top() - 27.0),
        "TEAM",
    );
    well(&painter, team_well, 14);
    for (index, (team, name, line, colors)) in TEAMS.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(
                team_well.left() + 5.0,
                team_well.top() + 5.0 + index as f32 * (58.0 + extra_height / 3.0),
            ),
            vec2(team_well.width() - 10.0, 52.0 + extra_height / 3.0),
        );
        let response = ui
            .interact(rect, Id::unique(("team", index)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let chosen = state.team == team;
        paint::labelled_choice(&response, true, chosen, name);
        super::setup::choice_frame(&painter, rect, chosen, response.hovered());
        let dot = pos2(rect.left() + 26.0, rect.center().y);
        let [left, right] = colors.map(hex);
        // A half-and-half dot for Auto.
        painter.circle_filled(dot, 8.0, right);
        if left != right {
            let half: Vec<Pos2> = (0..=16)
                .map(|step| {
                    let angle =
                        std::f32::consts::FRAC_PI_2 + step as f32 / 16.0 * std::f32::consts::PI;
                    dot + vec2(angle.cos(), angle.sin()) * 8.0
                })
                .collect();
            painter.add(Shape::convex_polygon(half, left, Stroke::NONE));
        }
        painter.circle_stroke(dot, 8.0, Stroke::new(1.0, hexa(0xffffff, 0.35)));
        paint::text(
            &painter,
            pos2(rect.left() + 43.0, rect.top() + 18.0),
            Align2::LEFT_CENTER,
            name,
            bold_font(15.0),
            Color32::WHITE,
            0.0,
        );
        paint::text(
            &painter,
            pos2(rect.left() + 43.0, rect.top() + 35.0),
            Align2::LEFT_CENTER,
            line,
            egui::FontId::proportional(12.0),
            hex(0xd0e4f3),
            0.0,
        );
        super::setup::choice_ring(&painter, rect, chosen);
        if response.clicked() {
            state.team = team;
        }
    }

    // CREATE ROOM or JOIN ROOM.
    let joining = !state.selected.is_empty();
    let enabled = joining || state.list.new_room_server().is_some();
    if paint::button_at(
        ui,
        art,
        action,
        if joining { "JOIN ROOM" } else { "CREATE ROOM" },
        None,
        ButtonStyle::Primary,
        enabled,
    )
    .clicked()
        && let Some(selection) = selection(state, prefs, new_room_map)
    {
        actions.push(Action::JoinRoom(Box::new(selection)));
    }
}

/// Of the rooms that can take a player, the one a phone joins (`busiestOpenRoom`): a
/// battle under way first, then a lobby, then a room between rounds; among those the
/// one with the most players, the directory's order deciding a tie.
fn busiest_open_room(rooms: &[HostedRoom]) -> Option<&HostedRoom> {
    let phase_order = |phase: RoomPhase| match phase {
        RoomPhase::Playing => 0,
        RoomPhase::Lobby => 1,
        RoomPhase::Results => 2,
    };
    rooms
        .iter()
        .filter(|room| room.joinable())
        .min_by_key(|room| (phase_order(room.phase), std::cmp::Reverse(room.players)))
}

/// A phone's Multiplayer tab: no room list, name or rules, only the status well in
/// `status` and one action in `action`, which joins the busiest open room the phone
/// picked or, with none open, creates a room on the chosen map. The new room waits for
/// the first list, which also says whether a room is open.
#[allow(clippy::too_many_arguments)]
pub fn phone_multiplayer(
    ui: &mut egui::Ui,
    art: &Art,
    state: &mut RoomsState,
    view: &View,
    prefs: &Preferences,
    status: Rect,
    action: Rect,
    actions: &mut Vec<Action>,
) {
    state.poll(view.debug);
    let new_room_map = prefs.game_options(Team::Blue, view.debug).map_mode;
    if state.name.is_none() {
        state.name = Some(preferred_player_name(
            &prefs.player_name,
            u64::from_le_bytes(directory::random_bytes()),
        ));
    }
    let time = ui.input(|input| input.time);
    let painter = ui.painter().clone();
    well(&painter, status, 12);
    let (message, hint) = state.prompt();
    let dot = pos2(status.left() + 20.0, status.center().y);
    let dot_color = state.status_color(time);
    painter.circle_filled(dot, 9.0, fade(dot_color, 0.2));
    painter.circle_filled(dot, 4.0, dot_color);
    let copy_left = status.left() + 38.0;
    let copy_width = status.right() - copy_left - 10.0;
    // The hints speak of the room list a phone has not got; offline, the reason and the
    // next try still show.
    let offline = matches!(state.state, ListState::Offline { .. });
    let two_lines = offline && status.height() >= 44.0;
    let message_y = if two_lines {
        status.center().y - 8.0
    } else {
        status.center().y
    };
    paint::text_fit(
        &painter,
        pos2(copy_left, message_y),
        Align2::LEFT_CENTER,
        message.lines().last().unwrap_or_default(),
        egui::FontId::proportional(12.5),
        hex(0xe6f1fb),
        0.0,
        copy_width,
    );
    if two_lines {
        paint::text_fit(
            &painter,
            pos2(copy_left, status.center().y + 9.0),
            Align2::LEFT_CENTER,
            &hint,
            egui::FontId::proportional(11.5),
            hex(0xa9c0d8),
            0.0,
            copy_width,
        );
    }
    let joining = !state.selected.is_empty();
    let enabled = joining || state.list.new_room_server().is_some();
    if paint::button_at(
        ui,
        art,
        action,
        if joining { "JOIN ROOM" } else { "CREATE ROOM" },
        None,
        ButtonStyle::Primary,
        enabled,
    )
    .clicked()
        && let Some(selection) = selection(state, prefs, new_room_map)
    {
        actions.push(Action::JoinRoom(Box::new(selection)));
    }
}

/// The new room's card: its map, what it holds, and its length and humans-only rules.
fn new_room_card(ui: &mut egui::Ui, state: &mut RoomsState, map: MapId, new_room: Rect) {
    let chosen = state.selected.is_empty();
    let response = ui
        .interact(new_room, Id::unique("new room"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    paint::labelled_choice(&response, true, chosen, "New room");
    let painter = ui.painter().clone();
    if chosen {
        painter.rect_filled(new_room.expand(3.0), 13, hexa(0xffcf38, 0.08));
        painter.rect_filled(new_room, 10, hex(0x22477a));
        painter.rect_stroke(new_room, 10, Stroke::new(2.0, GOLD), StrokeKind::Inside);
    } else {
        painter.rect_filled(
            new_room,
            10,
            if response.hovered() {
                hex(0x143060)
            } else {
                hex(0x10264a)
            },
        );
    }
    if response.clicked() {
        state.choose("");
    }
    radio(
        &painter,
        pos2(new_room.left() + 19.0, new_room.top() + 25.0),
        chosen,
    );
    let title = paint::text(
        &painter,
        pos2(new_room.left() + 38.0, new_room.top() + 16.0),
        Align2::LEFT_CENTER,
        "New room on ",
        bold_font(14.0),
        Color32::WHITE,
        0.0,
    );
    paint::text(
        &painter,
        pos2(title.right(), title.center().y),
        Align2::LEFT_CENTER,
        map_option_for(map).name,
        bold_font(14.0),
        GOLD,
        0.0,
    );
    paint::text(
        &painter,
        pos2(new_room.left() + 38.0, new_room.top() + 35.0),
        Align2::LEFT_CENTER,
        &format!("Up to {ROOM_SEATS} players · {}", WORDS.invite_card),
        egui::FontId::proportional(12.0),
        hex(SMALL_INK),
        0.0,
    );
    // LENGTH: a stepper in its well.
    let rule_y = new_room.top() + 67.0;
    paint::text(
        &painter,
        pos2(new_room.left() + 38.0, rule_y),
        Align2::LEFT_CENTER,
        "LENGTH",
        bold_font(12.0),
        hex(SMALL_INK),
        0.8,
    );
    let stepper = Rect::from_min_size(
        pos2(new_room.left() + 103.0, rule_y - 17.0),
        vec2(124.0, 34.0),
    );
    well(&painter, stepper, 9);
    let mut minutes = state.round_minutes;
    let step = |ui: &mut egui::Ui, left: f32, glyph: &str, id: &str| {
        let rect = Rect::from_min_size(pos2(left, stepper.top() + 2.0), vec2(28.0, 30.0));
        let response = ui.interact(rect, Id::unique(("length", id)), Sense::click());
        paint::labelled(
            &response,
            true,
            if id == "less" {
                "Fewer minutes"
            } else {
                "More minutes"
            },
        );
        ui.painter().rect_filled(
            rect,
            7,
            hexa(0xffffff, if response.hovered() { 0.12 } else { 0.06 }),
        );
        paint::text(
            ui.painter(),
            rect.center(),
            Align2::CENTER_CENTER,
            glyph,
            bold_font(17.0),
            hex(0xe6f1fb),
            0.0,
        );
        response.clicked()
    };
    if step(ui, stepper.left() + 2.0, "−", "less") {
        minutes = stepped_minutes(minutes, -LENGTH_STEP);
    }
    if step(ui, stepper.right() - 30.0, "+", "more") {
        minutes = stepped_minutes(minutes, LENGTH_STEP);
    }
    paint::text(
        &painter,
        pos2(stepper.left() + 47.0, rule_y),
        Align2::CENTER_CENTER,
        &minutes.to_string(),
        bold_font(15.0),
        hex(0xf5faff),
        0.0,
    );
    paint::text(
        &painter,
        pos2(stepper.left() + 63.0, rule_y + 1.0),
        Align2::LEFT_CENTER,
        "min",
        egui::FontId::proportional(12.0),
        hex(SMALL_INK),
        0.0,
    );
    // HUMANS ONLY: a switch.
    let switch = Rect::from_min_size(
        pos2(new_room.left() + 241.0, rule_y - 10.0),
        vec2(34.0, 20.0),
    );
    let switch_area = Rect::from_min_max(switch.min, pos2(switch.right() + 112.0, switch.bottom()));
    let response = ui
        .interact(switch_area, Id::unique("humans only"), Sense::click())
        .on_hover_text("No bots fill the teams; empty seats stay empty");
    paint::labelled_choice(&response, true, state.humans_only, "Humans only");
    let toggled = response.clicked();
    let mut humans_only = state.humans_only;
    if toggled {
        humans_only = !humans_only;
    }
    if humans_only {
        painter.rect_filled(switch, 10, GOLD);
        painter.circle_filled(
            pos2(switch.right() - 10.0, switch.center().y),
            7.0,
            Color32::WHITE,
        );
    } else {
        painter.rect_filled(switch, 10, hex(0x050d1c));
        painter.rect_stroke(
            switch,
            10,
            Stroke::new(1.0, hexa(0x8bb9dc, 0.4)),
            StrokeKind::Inside,
        );
        painter.circle_filled(
            pos2(switch.left() + 10.0, switch.center().y),
            7.0,
            hex(0x8ea7bd),
        );
    }
    paint::text(
        &painter,
        pos2(switch.right() + 7.0, rule_y),
        Align2::LEFT_CENTER,
        "HUMANS ONLY",
        bold_font(12.0),
        hex(SMALL_INK),
        0.8,
    );
    if minutes != state.round_minutes || humans_only != state.humans_only {
        state.round_minutes = minutes;
        state.humans_only = humans_only;
        // Editing the new room's rules chooses the new room.
        state.choose("");
    }
}

/// No open rooms: still looking, none open, or the server out of reach.
fn empty_rooms(ui: &mut egui::Ui, art: &Art, area: Rect, state: &mut RoomsState, time: f64) {
    let painter = ui.painter().clone();
    let ink = hex(0x8eaac4);
    let center = pos2(area.center().x, area.top() + 47.0);
    painter.circle_stroke(center, 21.75, Stroke::new(2.0, ink));
    painter.circle_stroke(center, 12.0, Stroke::new(2.0, fade(ink, 0.6)));
    let sweep = if matches!(state.state, ListState::Loading) {
        time as f32 * 3.0
    } else {
        -0.8
    };
    painter.line_segment(
        [center, center + vec2(sweep.cos(), sweep.sin()) * 21.0],
        Stroke::new(2.0, ink),
    );
    let (title, line) = match &state.state {
        ListState::Loading => (
            "SCANNING FOR OPEN ROOMS…",
            "Pick your tank and map meanwhile.",
        ),
        ListState::Ready => (
            "NO OPEN ROOMS RIGHT NOW",
            "Be the first: create a room and invite friends.",
        ),
        ListState::Offline { .. } => (
            "CAN'T REACH THE GAME SERVER",
            "Rooms list here once it answers.",
        ),
    };
    paint::shadowed(
        &painter,
        pos2(area.center().x, area.top() + 101.0),
        Align2::CENTER_CENTER,
        title,
        display_font(24.0),
        hex(0xf2f8fd),
        0.5,
    );
    paint::text(
        &painter,
        pos2(area.center().x, area.top() + 131.0),
        Align2::CENTER_CENTER,
        line,
        egui::FontId::proportional(14.0),
        hex(0xb9cde0),
        0.0,
    );
    if matches!(state.state, ListState::Offline { .. }) {
        for (index, label) in ["BATTLE THE BOTS", "RETRY NOW"].into_iter().enumerate() {
            let rect = Rect::from_center_size(
                pos2(
                    area.center().x + (index as f32 - 0.5) * 168.0,
                    area.top() + 162.0,
                ),
                vec2(156.0, 30.0),
            );
            let clicked =
                paint::button_at(ui, art, rect, label, None, ButtonStyle::Secondary, true)
                    .clicked();
            if clicked && index == 0 {
                state.multiplayer = false;
            } else if clicked {
                state.next_poll = None;
            }
        }
    }
}

/// The open rooms, one line each, chosen like the new room.
fn room_rows(ui: &mut egui::Ui, state: &mut RoomsState) {
    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
    let rooms = state.list.rooms.clone();
    for room in &rooms {
        let (title, detail) = room_lines(room);
        let joinable = room.joinable();
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), ROOM_ROW_HEIGHT),
            if joinable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let painter = ui.painter();
        let chosen = state.selected == room.room;
        paint::labelled_choice(&response, joinable, chosen, &room.room);
        if chosen {
            painter.rect_filled(rect, 10, hex(0x22477a));
            painter.rect_stroke(rect, 10, Stroke::new(2.0, GOLD), StrokeKind::Inside);
        } else if joinable && response.hovered() {
            painter.rect_filled(rect, 10, hex(0x143060));
        }
        let alpha = if joinable { 1.0 } else { 0.5 };
        radio(painter, pos2(rect.left() + 19.0, rect.center().y), chosen);
        let title_rect = paint::text(
            painter,
            pos2(rect.left() + 38.0, rect.center().y),
            Align2::LEFT_CENTER,
            &title,
            bold_font(14.0),
            fade(Color32::WHITE, alpha),
            0.0,
        );
        paint::text_fit(
            painter,
            pos2(title_rect.right() + 10.0, rect.center().y + 1.0),
            Align2::LEFT_CENTER,
            &detail,
            egui::FontId::proportional(12.0),
            fade(hex(SMALL_INK), alpha),
            0.0,
            (rect.right() - 10.0 - title_rect.right() - 10.0).max(0.0),
        );
        if joinable && response.clicked() {
            state.choose(&room.room);
        }
    }
}

/// The tab as the UI snapshots show it: the directory has answered with `rooms`.
#[cfg(test)]
pub fn answered(rooms: Vec<HostedRoom>) -> RoomsState {
    RoomsState {
        multiplayer: true,
        list: ServerList {
            servers: vec![sloppy_core::net::room_list::ServerListing {
                url: "wss://example.test".into(),
                content_version: sloppy_core::net::protocol::CONTENT_VERSION.into(),
            }],
            rooms,
        },
        state: ListState::Ready,
        next_poll: Some(Instant::now() + Duration::from_secs(3600)),
        name: Some("STEELTOE".into()),
        ..RoomsState::default()
    }
}

/// What CREATE ROOM or JOIN ROOM asks for, or `None` when the choice cannot play. A new
/// room opens on `map`.
fn selection(state: &mut RoomsState, prefs: &Preferences, map: MapId) -> Option<RoomSelection> {
    let name = player_name(state.name.as_deref().unwrap_or_default());
    if name.is_empty() {
        state.notice = "Enter a name to play.".into();
        return None;
    }
    let (room, server, create) = if state.selected.is_empty() {
        let server = state.list.new_room_server()?.to_owned();
        // A phone offers no room rules: its room fills its teams with bots, Easy as its
        // single player's.
        let phone = crate::touch::phone();
        let create = RoomSettings {
            map_mode: map,
            difficulty: if phone {
                Difficulty::Easy
            } else {
                Difficulty::Normal
            },
            humans_only: state.humans_only && !phone,
            round_minutes: state.round_minutes,
        };
        (room_code(directory::random_bytes()), server, Some(create))
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
        choice: JoinChoice {
            name,
            kind: prefs.tank,
            team: state.team,
            existing_room: create.is_none().then_some(true),
            create,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(code: &str, map: MapId) -> HostedRoom {
        HostedRoom {
            room: code.into(),
            content_version: sloppy_core::net::protocol::CONTENT_VERSION.into(),
            map_mode: map,
            difficulty: Difficulty::Normal,
            humans_only: false,
            round_minutes: 20,
            players: 2,
            reserved: 2,
            phase: RoomPhase::Playing,
            round_id: 1,
            time: 300.0,
            scores: vec![3, 4],
            server: "wss://example.test".into(),
        }
    }

    fn rooms(state: &RoomsState) -> Vec<&str> {
        state
            .list
            .rooms
            .iter()
            .map(|room| room.room.as_str())
            .collect()
    }

    /// A room on an extra level, picked by its code without the debug tools, stays
    /// listed and chosen after the refresh that found it; before, following the code
    /// forgot it, and the next refresh hid the room.
    #[test]
    fn a_room_found_by_code_stays_listed_on_an_extra_level() {
        let answer = || {
            Ok(ServerList {
                rooms: vec![
                    listing("ABCDEFGH", MapId::Superstress),
                    listing("JKLMNPQR", MapId::StressTest),
                    listing("STUVWXYZ", MapId::Harbor),
                ],
                ..answered(Vec::new()).list
            })
        };
        let mut state = answered(Vec::new());
        state.find_room("ABCDEFGH".into());
        assert!(state.notice.contains("isn't open"), "{}", state.notice);
        let now = Instant::now();
        for refresh in 0..3 {
            state.listed(answer(), false, now);
            assert_eq!(rooms(&state), ["ABCDEFGH", "STUVWXYZ"], "refresh {refresh}");
            assert_eq!(state.selected, "ABCDEFGH");
        }
        // The debug tools list every room.
        state.listed(answer(), true, now);
        assert_eq!(rooms(&state).len(), 3);
    }

    /// Back from a room that closed, the notice says why and then what the directory says
    /// of the room, on every refresh while its link waits; before, each refresh added the
    /// room's line again.
    #[test]
    fn a_closed_room_names_why_and_its_state_once_per_refresh() {
        let mut state = answered(Vec::new());
        state.returned_from("ABCDEFGH", "The room was closed.");
        let gone = "Room ABCDEFGH isn't open. Choose another room or create one.";
        for _ in 0..3 {
            state.listed(Ok(answered(Vec::new()).list), false, Instant::now());
            assert_eq!(state.notice, format!("The room was closed.\n{gone}"));
        }
        // A link from another page names the room alone.
        let mut linked = answered(Vec::new());
        linked.open_link(Some("ABCDEFGH"));
        assert!(linked.multiplayer);
        linked.listed(Ok(answered(Vec::new()).list), false, Instant::now());
        assert_eq!(linked.notice, gone);
        // A room that is listed but full says so under the reason.
        let mut full = answered(Vec::new());
        full.returned_from("ABCDEFGH", "This room is full.");
        let mut listed = listing("ABCDEFGH", MapId::Harbor);
        listed.reserved = ROOM_SEATS;
        full.listed(
            Ok(ServerList {
                rooms: vec![listed],
                ..answered(Vec::new()).list
            }),
            false,
            Instant::now(),
        );
        assert_eq!(
            full.notice,
            "This room is full.\nRoom ABCDEFGH can't take another player right now."
        );
    }

    #[test]
    fn a_room_left_stays_listed_when_it_plays_an_extra_level() {
        let mut state = answered(Vec::new());
        state.returned_from("ABCDEFGH", "");
        let list = ServerList {
            rooms: vec![listing("ABCDEFGH", MapId::StressTest)],
            ..answered(Vec::new()).list
        };
        for _ in 0..2 {
            state.listed(Ok(list.clone()), false, Instant::now());
            assert_eq!(rooms(&state), ["ABCDEFGH"]);
        }
    }

    /// A phone joins a battle under way before a lobby or a room between rounds, the
    /// fullest first, and never a full room or one of another version.
    #[test]
    fn a_phone_picks_the_busiest_open_room_preferring_a_battle_under_way() {
        let room = |code: &str, players, phase| HostedRoom {
            players,
            reserved: players,
            phase,
            ..listing(code, MapId::Village)
        };
        let full = room("FULLROOM", ROOM_SEATS, RoomPhase::Playing);
        let other_version = HostedRoom {
            content_version: "another".into(),
            ..room("OLDGAME1", ROOM_SEATS - 1, RoomPhase::Playing)
        };
        let between = room("RESULTS1", ROOM_SEATS - 1, RoomPhase::Results);
        let lobby = room("LOBBYONE", ROOM_SEATS - 2, RoomPhase::Lobby);
        let quiet = room("QUIETONE", 2, RoomPhase::Playing);
        let busy = room("BUSYONE1", 5, RoomPhase::Playing);
        let pick = |rooms: &[&HostedRoom]| {
            let rooms: Vec<HostedRoom> = rooms.iter().map(|&room| room.clone()).collect();
            busiest_open_room(&rooms).map(|room| room.room.clone())
        };
        assert_eq!(
            pick(&[&full, &other_version, &between, &lobby, &quiet, &busy]).as_deref(),
            Some("BUSYONE1")
        );
        assert_eq!(pick(&[&between, &lobby]).as_deref(), Some("LOBBYONE"));
        assert_eq!(pick(&[&between]).as_deref(), Some("RESULTS1"));
        assert_eq!(
            pick(&[&full, &other_version]),
            None,
            "none open: create one"
        );
        // A tie keeps the directory's order.
        let twin = room("TWINROOM", 5, RoomPhase::Playing);
        assert_eq!(pick(&[&twin, &busy]).as_deref(), Some("TWINROOM"));
    }

    /// Each refresh picks again as rooms change, but a room a link chose stays chosen.
    #[test]
    fn a_phone_picks_again_on_each_refresh_unless_a_link_chose_the_room() {
        let mut state = answered(vec![listing("QUIETONE", MapId::Harbor)]);
        state.pick_for_phone();
        assert_eq!(state.selected, "QUIETONE");
        state.list.rooms.insert(
            0,
            HostedRoom {
                players: 6,
                reserved: 6,
                ..listing("BUSYONE1", MapId::Village)
            },
        );
        state.pick_for_phone();
        assert_eq!(state.selected, "BUSYONE1");
        state.returned_from("QUIETONE", "");
        state.follow_link();
        state.pick_for_phone();
        assert_eq!(state.selected, "QUIETONE", "the link's room stays chosen");
    }
}
