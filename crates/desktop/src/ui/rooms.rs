//! Battle Setup's Multiplayer tab (`src/net/room-browser.ts`, the room panel of
//! `battle-setup.html`): the open rooms the server directory lists, polled every few
//! seconds while the tab shows, a new room's rules, the player's name and team, and
//! CREATE ROOM or JOIN ROOM. A room code typed in finds its room as an invite link
//! (`?room=`, `links.rs`) does.

use std::time::Duration;

use web_time::Instant;

use egui::{
    Align2, Color32, Id, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};

use super::icons::Art;
use super::paint::{self, ButtonStyle, fade, hex, hexa, mix};
use super::theme::{GOLD, bold_font, display_font};
use super::{Action, View};
use crate::directory::{self, DirectoryRequest};
use crate::links::{self, Place};
use crate::preferences::Preferences;
use crate::room_api::{RoomSelection, preferred_player_name};
use sloppy_core::net::player_names::player_name;
use sloppy_core::net::protocol::{
    DEFAULT_ROUND_MINUTES, JoinChoice, RoomPhase, RoomSettings, read_room_code, room_code,
    stepped_minutes,
};
use sloppy_core::net::room_list::{HostedRoom, ROOM_SEATS, ServerList};
use sloppy_core::sim::Team;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::{MapId, is_extra_level, map_option_for};
use sloppy_web::battle_report::duration;

/// The list refreshes this often while the tab shows.
const REFRESH: Duration = Duration::from_secs(5);
/// The name well's dice button, a square at its right end, and its inset there.
const DICE_BUTTON: f32 = 30.0;
const DICE_INSET: f32 = 6.0;
/// Minutes one press of the length stepper moves.
const LENGTH_STEP: i32 = 5;
/// The width of a stood-down action's stripes.
const DISABLED_STRIPE: f32 = 8.0;
/// CREATE ROOM and JOIN ROOM's type (`.room-action .primary`).
const ROOM_ACTION_LETTERING: paint::Lettering = paint::Lettering::Display {
    size: 26.0,
    spacing: 2.0,
};

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
    /// `None` until first shown, then the drawn name to play under (saved on joining).
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

    /// The name to play under: the saved one, or a drawn one the first time the tab
    /// shows.
    fn player_name(&mut self, prefs: &Preferences) -> &str {
        self.name.get_or_insert_with(|| {
            preferred_player_name(
                &prefs.player_name,
                u64::from_le_bytes(directory::random_bytes()),
            )
        })
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

/// Refresh (the page's `#refresh-rooms` icon, 15 px of its 24-unit view): a ring open
/// at its top right, where an arrowhead turns it on, `turn` radians round while busy.
fn refresh_icon(painter: &Painter, center: Pos2, turn: f32, ink: Color32) {
    const UNIT: f32 = 15.0 / 24.0;
    let stroke = Stroke::new(2.4 * UNIT, ink);
    // The ring from its right end clockwise round to its top right.
    paint::arc(
        painter,
        center,
        7.5 * UNIT,
        std::f32::consts::FRAC_PI_2 + turn,
        std::f32::consts::PI * 1.75,
        stroke,
    );
    let (sin, cos) = turn.sin_cos();
    let at = |x: f32, y: f32| center + vec2(x * cos - y * sin, x * sin + y * cos) * UNIT;
    painter.add(Shape::line(
        vec![at(7.5, -7.5), at(7.5, -3.5), at(3.5, -3.5)],
        stroke,
    ));
}

/// The name well's dice (a 24-unit die drawn in `rect`): a rounded face outlined in
/// `ink` with its five pips.
fn dice_icon(painter: &Painter, rect: Rect, ink: Color32) {
    let unit = rect.width() / 24.0;
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * unit;
    painter.rect_stroke(
        Rect::from_min_max(at(3.5, 3.5), at(20.5, 20.5)),
        4.0 * unit,
        Stroke::new(2.0 * unit, ink),
        StrokeKind::Middle,
    );
    for (x, y) in [
        (8.0, 8.0),
        (16.0, 8.0),
        (12.0, 12.0),
        (8.0, 16.0),
        (16.0, 16.0),
    ] {
        painter.circle_filled(at(x, y), 1.7 * unit, ink);
    }
}

/// A text field in a sunken well, like the page's inputs. Its id is its hint's, not
/// egui's count of the widgets before it: the room list above it changes as the
/// directory answers, and a field whose id moved would drop the click that focuses it
/// and its cursor.
fn field(
    ui: &mut egui::Ui,
    rect: Rect,
    text: &mut String,
    limit: usize,
    hint: &str,
    size: f32,
) -> egui::Response {
    well(ui.painter(), rect, 9);
    let inner = rect.shrink2(vec2(12.0, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child
        .centered_and_justified(|ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(Id::unique(("text field", hint)))
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

/// A section heading's row over its well (`.setup-label` and its margin).
pub const HEADING_ROW: f32 = 27.0;
/// The name field's height.
pub const NAME_FIELD_HEIGHT: f32 = 42.0;
/// The wells' padding and the gap between the choices in them.
const WELL_PADDING: f32 = 5.0;
const WELL_GAP: f32 = 6.0;
/// The new room's card with its rules on a line of their own, as the garage's narrower
/// room list has it.
const NEW_ROOM_WRAPPED_HEIGHT: f32 = 89.0;
/// A line of the new room's rules, where HUMANS ONLY takes one under LENGTH.
const RULE_ROW: f32 = 34.0;
/// The widest the rules (LENGTH, its stepper, HUMANS ONLY) take at a card's right end.
const ROOM_RULES_WIDTH: f32 = 334.0;
/// The room the new room's name and line keep beside its rules before the rules take a
/// line of their own.
const NEW_ROOM_COPY_MIN: f32 = 180.0;
/// The new room's text: its name's line and each line of what it holds (`.room-row`).
const NEW_ROOM_NAME_LINE: f32 = 19.0;
const NEW_ROOM_LINE: f32 = 16.8;
/// A team choice's name line and each line of what it means, and its padding.
const TEAM_NAME_LINE: f32 = 18.0;
const TEAM_LINE: f32 = 15.6;
const TEAM_PADDING: f32 = 5.0;
/// From a team choice's left edge to its name, past the swatch.
const TEAM_COPY_LEFT: f32 = 44.0;
/// An empty list's message: the radar, its title and its line, and the offline state's
/// two buttons under them (`.room-empty`).
const EMPTY_ROOMS_HEIGHT: f32 = 150.0;
const OFFLINE_ROOMS_HEIGHT: f32 = 180.0;
/// The offline state's two keys at their widest, and the gap between them.
const OFFLINE_KEY_WIDTH: f32 = 156.0;
const OFFLINE_KEY_GAP: f32 = 12.0;

/// Where the Multiplayer tab's parts go: the ROOMS heading over the room list and the
/// status well under it; YOUR NAME and TEAM beside or under them, over the action.
#[derive(Clone, Copy)]
pub struct MultiplayerLayout {
    pub rooms_heading: Pos2,
    pub room_list: Rect,
    /// The new room's rules take a line under its name rather than sharing its row.
    pub rules_below: bool,
    pub status_well: Rect,
    pub name_field: Rect,
    pub team_well: Rect,
    pub action: Rect,
}

impl MultiplayerLayout {
    /// The garage's panel (laid out as the page at 1280 × 800): the room list beside the
    /// name and team. Widening the sheet adds room without scaling fonts or controls.
    pub fn garage(bounds: Rect) -> Self {
        let right_width = 302.0 + (bounds.width() - 842.0) * 1.22 / 3.22;
        let right_left = bounds.right() - right_width;
        let room_list = Rect::from_min_max(
            bounds.min + vec2(0.0, HEADING_ROW),
            pos2(right_left - 23.0, bounds.bottom() - 76.0),
        );
        Self {
            rooms_heading: bounds.min,
            room_list,
            rules_below: true,
            status_well: Rect::from_min_max(
                pos2(bounds.left(), bounds.bottom() - 62.0),
                pos2(room_list.right(), bounds.bottom()),
            ),
            name_field: Rect::from_min_size(
                pos2(right_left, bounds.top() + HEADING_ROW),
                vec2(right_width, NAME_FIELD_HEIGHT),
            ),
            team_well: Rect::from_min_max(
                pos2(right_left, bounds.top() + 110.0),
                pos2(bounds.right(), bounds.bottom() - 75.0),
            ),
            action: Rect::from_min_size(
                pos2(right_left, bounds.bottom() - 62.0),
                vec2(right_width, 62.0),
            ),
        }
    }
}

/// Whether the new room's rules leave its name and line too little room in a card
/// `width` wide, and take a line of their own.
pub fn rules_below(width: f32) -> bool {
    width - 38.0 - ROOM_RULES_WIDTH - 12.0 < NEW_ROOM_COPY_MIN
}

/// Whether a card `width` wide is too narrow for its rules on one line under its name,
/// and HUMANS ONLY takes a line under LENGTH (a narrow window's one column).
fn rules_split(width: f32) -> bool {
    width < 38.0 + ROOM_RULES_WIDTH + 12.0
}

/// The new room's card height in a card `width` wide.
fn new_room_height(painter: &Painter, width: f32, rules_below: bool) -> f32 {
    if rules_below {
        return NEW_ROOM_WRAPPED_HEIGHT + if rules_split(width) { RULE_ROW } else { 0.0 };
    }
    let lines = new_room_line(painter, width - 38.0 - ROOM_RULES_WIDTH - 12.0)
        .rows
        .len();
    12.0 + NEW_ROOM_NAME_LINE + 2.0 + lines as f32 * NEW_ROOM_LINE
}

/// What a new room holds, wrapped at `width`.
fn new_room_line(painter: &Painter, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        format!("Up to {ROOM_SEATS} players · {}", WORDS.invite_card),
        egui::TextFormat {
            font_id: egui::FontId::proportional(12.0),
            color: hex(SMALL_INK),
            line_height: Some(NEW_ROOM_LINE),
            ..Default::default()
        },
    );
    job.wrap.max_width =
        paint::wrap_width(painter, &job.text, egui::FontId::proportional(12.0), width);
    painter.layout_job(job)
}

/// The room list's least height in a one-column layout, where it shows all it holds: the
/// new room's card and the open rooms or why there are none.
pub fn room_list_height(painter: &Painter, state: &RoomsState, width: f32) -> f32 {
    let card = width - 2.0 * WELL_PADDING;
    let rows = if state.list.rooms.is_empty() {
        if matches!(state.state, ListState::Offline { .. }) {
            OFFLINE_ROOMS_HEIGHT
        } else {
            EMPTY_ROOMS_HEIGHT
        }
    } else {
        state.list.rooms.len() as f32 * (ROOM_ROW_HEIGHT + 2.0)
    };
    2.0 * WELL_PADDING + new_room_height(painter, card, rules_below(card)) + WELL_GAP + rows
}

/// What a team choice says it means, wrapped to a choice `width` wide.
fn team_line(painter: &Painter, line: &str, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        line.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(12.0),
            color: hex(0xd0e4f3),
            line_height: Some(TEAM_LINE),
            ..Default::default()
        },
    );
    job.wrap.max_width = paint::wrap_width(
        painter,
        line,
        egui::FontId::proportional(12.0),
        width - TEAM_COPY_LEFT - 14.0,
    );
    painter.layout_job(job)
}

/// Each team choice's least height in a well `width` wide.
fn team_heights(painter: &Painter, width: f32) -> [f32; 3] {
    TEAMS.map(|(_, _, line, _)| {
        let lines = team_line(painter, line, width - 2.0 * WELL_PADDING)
            .rows
            .len();
        2.0 * TEAM_PADDING + TEAM_NAME_LINE + lines as f32 * TEAM_LINE
    })
}

/// The TEAM well's least height at `width`.
pub fn team_well_height(painter: &Painter, width: f32) -> f32 {
    2.0 * WELL_PADDING + 2.0 * WELL_GAP + team_heights(painter, width).iter().sum::<f32>()
}

/// The Multiplayer tab under the map row, where single player shows its battle format,
/// difficulty and GO (`#multiplayer-panel`), its parts where `layout` puts them.
pub fn multiplayer(
    ui: &mut egui::Ui,
    art: &Art,
    state: &mut RoomsState,
    view: &View,
    prefs: &Preferences,
    layout: &MultiplayerLayout,
    actions: &mut Vec<Action>,
) {
    let MultiplayerLayout {
        rooms_heading: rooms_head,
        room_list,
        rules_below,
        status_well,
        name_field,
        team_well,
        action,
    } = *layout;
    let new_room = Rect::from_min_size(
        room_list.min + vec2(WELL_PADDING, WELL_PADDING),
        vec2(
            room_list.width() - 2.0 * WELL_PADDING,
            new_room_height(
                ui.painter(),
                room_list.width() - 2.0 * WELL_PADDING,
                rules_below,
            ),
        ),
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
            let turn = if busy { time as f32 * 6.0 } else { 0.0 };
            refresh_icon(painter, rect.center(), turn, ink);
        },
    ) {
        state.next_poll = None;
    }
    let code_field = Rect::from_min_size(
        pos2(room_list.right() - 169.0, rooms_head.y - 3.0),
        vec2(102.0, 26.0),
    );
    let typed = field(ui, code_field, &mut state.code, 8, "Room code", 13.0);
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
    new_room_card(ui, state, new_room_map, new_room, rules_below);
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

    // YOUR NAME: the drawn name in a well like the room code's field, which nobody
    // types in, and the dice that draws another at its right end.
    paint::section_label(
        &painter,
        pos2(name_field.left(), name_field.top() - HEADING_ROW),
        "YOUR NAME",
    );
    well(&painter, name_field, 9);
    painter.rect_stroke(
        name_field,
        9,
        Stroke::new(0.5, hexa(0x92b4cf, 0.19)),
        StrokeKind::Inside,
    );
    let dice = Rect::from_min_size(
        pos2(
            name_field.right() - DICE_INSET - DICE_BUTTON,
            name_field.center().y - DICE_BUTTON / 2.0,
        ),
        Vec2::splat(DICE_BUTTON),
    );
    paint::text_fit(
        &painter,
        pos2(name_field.left() + 12.0, name_field.center().y),
        Align2::LEFT_CENTER,
        state.player_name(prefs),
        bold_font(15.0),
        hex(0xf5faff),
        0.0,
        dice.left() - name_field.left() - 18.0,
    );
    if icon_button(
        ui,
        dice,
        "dice",
        true,
        "Draw another name",
        |painter, rect, ink| {
            dice_icon(
                painter,
                Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
                ink,
            );
        },
    ) {
        state.name = Some(player_name(u64::from_le_bytes(directory::random_bytes())));
    }

    // TEAM: each choice its least height and an equal share of what the well has more.
    paint::section_label(
        &painter,
        pos2(team_well.left(), team_well.top() - HEADING_ROW),
        "TEAM",
    );
    well(&painter, team_well, 14);
    let heights = team_heights(&painter, team_well.width());
    let spare = (team_well.height() - team_well_height(&painter, team_well.width())).max(0.0);
    let mut top = team_well.top() + WELL_PADDING;
    for (index, (team, name, line, colors)) in TEAMS.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(team_well.left() + WELL_PADDING, top),
            vec2(
                team_well.width() - 2.0 * WELL_PADDING,
                heights[index] + spare / 3.0,
            ),
        );
        top = rect.bottom() + WELL_GAP;
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
        // The name over what it means, centred in the choice however tall it stands.
        let meaning = team_line(&painter, line, rect.width());
        let copy_top = rect.center().y - (TEAM_NAME_LINE + meaning.size().y) / 2.0;
        paint::text(
            &painter,
            pos2(
                rect.left() + TEAM_COPY_LEFT,
                copy_top + TEAM_NAME_LINE / 2.0,
            ),
            Align2::LEFT_CENTER,
            name,
            bold_font(15.0),
            Color32::WHITE,
            0.0,
        );
        painter.galley(
            pos2(rect.left() + TEAM_COPY_LEFT, copy_top + TEAM_NAME_LINE),
            meaning,
            Color32::WHITE,
        );
        super::setup::choice_ring(&painter, rect, chosen);
        if response.clicked() {
            state.team = team;
        }
    }

    // CREATE ROOM or JOIN ROOM.
    if room_action(ui, art, action, state)
        && let Some(selection) = selection(state, prefs, new_room_map)
    {
        actions.push(Action::JoinRoom(Box::new(selection)));
    }
}

/// CREATE ROOM, or JOIN ROOM with an open room chosen, in `rect`; true when pressed.
/// With no server to create on it stands down (`.room-action .primary:disabled`): dark
/// glass and grey type, and offline a lock and why under its name.
fn room_action(ui: &mut egui::Ui, art: &Art, rect: Rect, state: &RoomsState) -> bool {
    let joining = !state.selected.is_empty();
    let label = if joining { "JOIN ROOM" } else { "CREATE ROOM" };
    if joining || state.list.new_room_server().is_some() {
        return paint::action_button(ui, art, rect, label, ROOM_ACTION_LETTERING, true).clicked();
    }
    let response = ui.interact(rect, Id::unique("room action"), Sense::hover());
    paint::labelled(&response, false, label);
    let painter = ui.painter();
    let offline = matches!(state.state, ListState::Offline { .. });
    let (fill, ring, ink) = if offline {
        (hex(0x081428), hexa(0xb7d7ed, 0.12), hex(0x8ea4ba))
    } else {
        (hex(0x0a1830), hexa(0xb7d7ed, 0.18), hex(0xa3b9ce))
    };
    painter.rect_filled(rect, 12, fill);
    // Faint diagonal stripes, as a key that cannot be pressed.
    let stripes = painter.with_clip_rect(rect.shrink(1.0));
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        stripes.add(Shape::convex_polygon(
            vec![
                pos2(x, rect.bottom()),
                pos2(x + DISABLED_STRIPE, rect.bottom()),
                pos2(x + DISABLED_STRIPE + rect.height(), rect.top()),
                pos2(x + rect.height(), rect.top()),
            ],
            hexa(0xffffff, 0.024),
            Stroke::NONE,
        ));
        x += 2.0 * DISABLED_STRIPE;
    }
    painter.rect_stroke(rect, 12, Stroke::new(1.0, ring), StrokeKind::Inside);
    let font = display_font(26.0);
    if !offline {
        paint::text(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            font,
            ink,
            2.0,
        );
        return false;
    }
    let reason = "Waits for the game server";
    let reason_font = bold_font(12.0);
    let copy = paint::text_width(painter, label, font.clone(), 2.0).max(paint::text_width(
        painter,
        reason,
        reason_font.clone(),
        0.6,
    ));
    let left = rect.center().x - (20.0 + 12.0 + copy) / 2.0;
    art.icon(
        painter,
        "lock",
        Rect::from_center_size(pos2(left + 10.0, rect.center().y), vec2(20.0, 20.0)),
        ink,
    );
    paint::text(
        painter,
        pos2(left + 32.0, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        label,
        font,
        ink,
        2.0,
    );
    paint::text(
        painter,
        pos2(left + 32.0, rect.center().y + 14.0),
        Align2::LEFT_CENTER,
        reason,
        reason_font,
        hex(0xb9cde0),
        0.6,
    );
    false
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
    if room_action(ui, art, action, state)
        && let Some(selection) = selection(state, prefs, new_room_map)
    {
        actions.push(Action::JoinRoom(Box::new(selection)));
    }
}

/// How far the new room's rules fade while an open room is chosen
/// (`.new-room:not(:has(#new-room:checked)) .room-rules`).
const UNCHOSEN_RULES_OPACITY: f32 = 0.55;

/// The new room's card: its map, what it holds, and its length and humans-only rules,
/// at the card's right end or, `rules_below`, on a line of their own under its name.
fn new_room_card(
    ui: &mut egui::Ui,
    state: &mut RoomsState,
    map: MapId,
    new_room: Rect,
    rules_below: bool,
) {
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
    let copy_left = new_room.left() + 38.0;
    let (radio_y, name_y, line_width, rules_left, rule_y) = if rules_below {
        (
            new_room.top() + 25.0,
            new_room.top() + 16.0,
            f32::INFINITY,
            copy_left,
            new_room.top() + 67.0,
        )
    } else {
        let line_width = new_room.right() - 12.0 - ROOM_RULES_WIDTH - copy_left;
        let block = NEW_ROOM_NAME_LINE + 2.0 + new_room_line(&painter, line_width).size().y;
        (
            new_room.center().y,
            new_room.center().y - block / 2.0 + NEW_ROOM_NAME_LINE / 2.0,
            line_width,
            new_room.right() - 12.0 - ROOM_RULES_WIDTH,
            new_room.center().y,
        )
    };
    radio(&painter, pos2(new_room.left() + 19.0, radio_y), chosen);
    let title = paint::text(
        &painter,
        pos2(copy_left, name_y),
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
    painter.galley(
        pos2(copy_left, name_y + NEW_ROOM_NAME_LINE / 2.0 + 2.0),
        new_room_line(&painter, line_width),
        Color32::WHITE,
    );
    // The rules fade while an open room is chosen.
    let mut rules = painter.clone();
    if !chosen {
        rules.multiply_opacity(UNCHOSEN_RULES_OPACITY);
    }
    // LENGTH: a stepper in its well.
    paint::text(
        &rules,
        pos2(rules_left, rule_y),
        Align2::LEFT_CENTER,
        "LENGTH",
        bold_font(12.0),
        hex(SMALL_INK),
        0.8,
    );
    let stepper = Rect::from_min_size(pos2(rules_left + 65.0, rule_y - 17.0), vec2(124.0, 34.0));
    well(&rules, stepper, 9);
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
        rules.rect_filled(
            rect,
            7,
            hexa(0xffffff, if response.hovered() { 0.12 } else { 0.06 }),
        );
        paint::text(
            &rules,
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
        &rules,
        pos2(stepper.left() + 47.0, rule_y),
        Align2::CENTER_CENTER,
        &minutes.to_string(),
        bold_font(15.0),
        hex(0xf5faff),
        0.0,
    );
    paint::text(
        &rules,
        pos2(stepper.left() + 63.0, rule_y + 1.0),
        Align2::LEFT_CENTER,
        "min",
        egui::FontId::proportional(12.0),
        hex(SMALL_INK),
        0.0,
    );
    // HUMANS ONLY: a switch after the stepper, or under LENGTH in a narrow card.
    let (switch_left, switch_y) = if rules_below && rules_split(new_room.width()) {
        (rules_left, rule_y + RULE_ROW)
    } else {
        (rules_left + 203.0, rule_y)
    };
    let switch = Rect::from_min_size(pos2(switch_left, switch_y - 10.0), vec2(34.0, 20.0));
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
        rules.rect_filled(switch, 10, GOLD);
        rules.circle_filled(
            pos2(switch.right() - 10.0, switch.center().y),
            7.0,
            Color32::WHITE,
        );
    } else {
        rules.rect_filled(switch, 10, hex(0x050d1c));
        rules.rect_stroke(
            switch,
            10,
            Stroke::new(1.0, hexa(0x8bb9dc, 0.4)),
            StrokeKind::Inside,
        );
        rules.circle_filled(
            pos2(switch.left() + 10.0, switch.center().y),
            7.0,
            hex(0x8ea7bd),
        );
    }
    paint::text(
        &rules,
        pos2(switch.right() + 7.0, switch_y),
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
        // The two keys side by side, narrower where a narrow list leaves less room.
        let key_width = OFFLINE_KEY_WIDTH.min((area.width() - OFFLINE_KEY_GAP) / 2.0);
        for (index, label) in ["BATTLE THE BOTS", "RETRY NOW"].into_iter().enumerate() {
            let rect = Rect::from_center_size(
                pos2(
                    area.center().x + (index as f32 - 0.5) * (key_width + OFFLINE_KEY_GAP),
                    area.top() + 162.0,
                ),
                vec2(key_width, 30.0),
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
        // A listed room shows its choice by its radio alone, with no fill or ring
        // (`.start #room-list .room-row` outranks the chosen card's rule).
        if joinable && response.hovered() {
            painter.rect_filled(rect, 10, hex(0x1b3966));
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
        name: Some("Swift Falcon".into()),
        ..RoomsState::default()
    }
}

/// What CREATE ROOM or JOIN ROOM asks for, or `None` when the choice cannot play. A new
/// room opens on `map`.
fn selection(state: &mut RoomsState, prefs: &Preferences, map: MapId) -> Option<RoomSelection> {
    let name = state.player_name(prefs).to_owned();
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
            bot: false,
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
