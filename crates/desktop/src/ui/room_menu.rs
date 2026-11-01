//! A multiplayer room's screens (`src/net/network-ui.ts`, the join screen of
//! `src/game/join-screen.ts`): the joining card while the room loads, the room menu
//! (lobby, rules, roster, the host's controls, the results and the in-battle menu), the
//! dialog when the connection drops or ends, and the battle HUD's room parts (the
//! players list and the respawn card).

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, FontId, Id, LayerId, Mesh, Order, Painter, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui, pos2, vec2,
};
use sloppy_core::net::multiplayer_simulation::TEAM_SLOTS;
use sloppy_core::net::protocol::{Lobby, MAX_ROUND_MINUTES, Player, RoomPhase, RoomSettings};
use sloppy_core::sim::data::{TEAM_NAMES, vehicle};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::{MAP_OPTIONS, map_option_for};
use sloppy_core::sim::{Match, MatchPhase, SimEventType, Team, VehicleKind};
use sloppy_web::battle_report::{count_line, title_case};
use sloppy_web::catalog::{DIFFICULTY_OPTIONS, TANK_CARDS, difficulty_name, map_card};
use sloppy_web::events::PendingEvent;
use sloppy_web::room_text::{Arrival, ConnectionDialog, Ending, MenuText, team_word};

use super::icons::Art;
use super::paint::{self, ButtonStyle, hex, hexa};
use super::theme::{GOLD, GOLD_HOT, TEXT, TEXT_DIM, TEXT_LABEL, bold_font, display_font};
use super::{Action, KillFeedEntry, UiState, View, hud, menus};
use crate::links;
use crate::tasks::Background;
use std::sync::mpsc::TryRecvError;

/// Seconds "Code copied" shows on its button.
const COPIED_SECONDS: f64 = 2.0;
/// What a join says while the room loads, in this order (`join-screen.ts`): the seat
/// first, then the arena until the room can draw it.
const CONNECTING_STATUS: &str = "Connecting to room…";
const PREPARING_STATUS: &str = "Preparing the arena…";
/// The gap between a room dialog's parts (`.network-menu`), between the lines of its
/// head (`.dialog-head`), between NEXT BATTLE's rows and its actions' rows.
const ROOM_GAP: f32 = 16.0;
const HEAD_GAP: f32 = 6.0;
const ROW_GAP: f32 = 10.0;
/// The head's eyebrow line, as tall as the invite button when that stands beside it;
/// the title's line (50 px at 0.95), and the lede's (13 px at 1.5).
const EYEBROW_LINE: f32 = 15.0;
const TEXT_BUTTON_HEIGHT: f32 = 28.0;
const TITLE_LINE: f32 = 47.5;
const LEDE_LINE: f32 = 19.5;
/// The room menu's text colour (`--text` in `.network-menu`), where the page names it.
const MENU_TEXT: u32 = 0xc4dfef;
/// The line under a dialog's title (`.dialog-lede`).
const LEDE: u32 = 0xc4dbee;
/// A form field's label line, its height, its inner padding, and the room a select keeps
/// at its right for the chevron (`.network-fields` select and input).
const FIELD_LABEL: f32 = 13.0;
const FIELD_HEIGHT: f32 = 42.0;
const FIELD_PADDING: f32 = 12.0;
const CHEVRON_ROOM: f32 = 32.0;
/// A team card's header down to the hairline over its first player, a player's row, and
/// what the card keeps under its last row (`.network-team`, `.roster-player`).
const TEAM_HEADER: f32 = 44.0;
const ROSTER_ROW: f32 = 30.0;
const TEAM_FOOT: f32 = 4.0;
/// A team card's coloured top border and its corner radius.
const TEAM_STRIPE: f32 = 3.0;
const TEAM_RADIUS: f32 = 10.0;
/// LAST ROUND's header line and each player's row (`#network-scoreboard`).
const SCORE_HEADER: f32 = 20.0;
const SCORE_ROW: f32 = 31.0;
/// The joining well (`.room-status`): its least height, padding, ring, and the sweep's
/// share of the track and its time across (`.startup-track`).
const STATUS_WELL: f32 = 48.0;
const STATUS_PADDING: f32 = 12.0;
const STATUS_RING: f32 = 9.0;
const SWEEP_SHARE: f32 = 0.38;
const SWEEP_SECONDS: f64 = 1.4;
/// Whether the room's socket is live, being (re)connected, or has given up, with the
/// engine's words for the connection dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Live,
    Connecting(ConnectionDialog),
    Ended(Ending),
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
    /// The engine's title, hint and rule chips for the lobby.
    pub text: MenuText,
    pub player_id: String,
    pub link: Link,
    /// The finished round as the viewer's tank played it, for its final score.
    pub outcome: Option<Match>,
    /// The in-battle menu is open.
    pub menu_open: bool,
    editing_rules: bool,
    editing_choice: bool,
    controls_open: bool,
    /// MINUTES as typed, while the field has the keyboard; it applies when the field
    /// lets go, as the page's input applies on `change`.
    minutes_draft: Option<String>,
    /// A one-line answer in the room menu, such as a full team.
    pub notice: String,
    /// Seconds left on "Code copied".
    copied: f64,
    /// The invite link on its way to the clipboard, which a page's browser may refuse.
    copying: Option<Background<bool>>,
    /// Who destroyed the viewer, while the respawn card shows.
    pub death_cause: String,
}

impl RoomMenu {
    pub fn new(room: &str, creating: bool) -> Self {
        Self {
            room: room.to_owned(),
            joining: true,
            creating,
            status: CONNECTING_STATUS.into(),
            lobby: None,
            text: MenuText::default(),
            player_id: String::new(),
            // The joining card covers the dialog until the engine reveals the room.
            link: Link::Connecting(ConnectionDialog {
                title: "CONNECTING",
                message: "Connecting to the room…".into(),
            }),
            outcome: None,
            menu_open: false,
            editing_rules: false,
            editing_choice: false,
            controls_open: false,
            minutes_draft: None,
            notice: String::new(),
            copied: 0.0,
            copying: None,
            death_cause: String::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn snapshot_fixture(&self) -> serde_json::Value {
        use serde_json::json;
        let link = match &self.link {
            Link::Live => json!({"kind": "live"}),
            Link::Connecting(dialog) => {
                json!({"kind": "connecting", "title": dialog.title, "text": dialog.message})
            }
            Link::Ended(ending) => json!({
                "kind": "ended",
                "text": ending.text,
                "title": ending.title,
                "retry": ending.retry,
                "setup": ending.setup,
            }),
        };
        json!({
            "room": self.room, "playerId": self.player_id, "joining": self.joining,
            "creating": self.creating, "status": self.status, "link": link,
            "lobby": self.lobby.as_ref().map(|lobby| serde_json::from_str::<serde_json::Value>(&lobby.to_json()).expect("a lobby JSON")),
            "menu": self.text,
            "outcome": self.outcome.as_ref().map(sloppy_web::hud::PageMatch::from),
            "menuOpen": self.menu_open, "editingRules": self.editing_rules, "editingChoice": self.editing_choice,
            "controlsOpen": self.controls_open, "notice": self.notice, "deathCause": self.death_cause,
        })
    }

    /// Say whether the invite link reached the clipboard, once that is known.
    fn copy_answered(&mut self) {
        let Some(answer) = self.copying.as_ref().map(Background::answer) else {
            return;
        };
        match answer {
            Err(TryRecvError::Empty) => return,
            Ok(true) => {
                self.copied = COPIED_SECONDS;
                self.notice = "Invite link copied. Send it to your friends.".into();
            }
            Ok(false) | Err(TryRecvError::Disconnected) => {
                self.notice = "Copy the address from your browser to invite friends.".into();
            }
        }
        self.copying = None;
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
    pub fn team(&self) -> Option<Team> {
        self.mine().map(|player| player.team)
    }

    /// Connection progress: live, or the dialog that says the socket is down. While the
    /// room still loads, a held seat means the arena is what is left: the client says
    /// "Connected" before the arena's state arrives and again once it compiled, and the
    /// joining card says the arena is preparing until the room can show it.
    pub fn status(&mut self, text: &str, dialog: Option<ConnectionDialog>) {
        self.status = if self.joining && dialog.is_none() {
            PREPARING_STATUS.to_owned()
        } else {
            text.to_owned()
        };
        self.link = dialog.map_or(Link::Live, Link::Connecting);
    }

    /// The connection ended, or the renderer stopped.
    pub fn ended(&mut self, ending: Ending) {
        self.link = Link::Ended(ending);
    }

    /// A new lobby with the engine's words for it: who joined or came back goes to the
    /// feed.
    pub fn lobby(
        &mut self,
        lobby: Lobby,
        player_id: String,
        text: MenuText,
        arrivals: Vec<Arrival>,
        ui: &mut UiState,
    ) {
        for arrival in arrivals {
            ui.push_feed(KillFeedEntry {
                killer: arrival.text,
                killer_team: Some(arrival.team),
                victim: String::new(),
                victim_team: None,
                cause: None,
                own_kill: false,
                own_death: false,
                age: 0.0,
            });
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
        self.text = text;
        self.player_id = player_id;
    }

    /// An engine event's room feedback: the engine's death sentence on the respawn card,
    /// until the viewer's tank respawns.
    pub fn event(&mut self, pending: &PendingEvent) {
        if let Some(report) = &pending.feedback.death_report {
            self.death_cause.clone_from(&report.sentence);
        } else if pending.own && pending.event.kind == SimEventType::Respawn {
            self.death_cause.clear();
        }
    }
}

/// The room's plate keeps the same width and padding as `multiplayer.css`; only
/// its content scrolls when a long roster or the host's fields need more space.
fn room_dialog(context: &egui::Context, id: &str, width: f32, build: impl FnOnce(&mut Ui)) {
    menus::dim_backdrop(context);
    menus::dialog_with_padding(
        context,
        id,
        width,
        [28.0, 24.0, 24.0],
        |ui, _, origin, width| {
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                origin,
                vec2(width, context.content_rect().height() - 80.0),
            )));
            body.spacing_mut().item_spacing = vec2(ROW_GAP, ROOM_GAP);
            body.style_mut().override_font_id = Some(FontId::proportional(14.0));
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(context.content_rect().height() - 80.0)
                .auto_shrink([false, true])
                .show(&mut body, |ui| {
                    ui.set_min_width(width);
                    build(ui);
                })
                .content_size
                .y
                .min(context.content_rect().height() - 80.0)
        },
    );
}

/// A centred word under the actions (`.network-note`).
fn note(ui: &mut Ui, text: &str) {
    let width = ui.available_width();
    let galley = paint::wrapped(
        ui.painter(),
        text,
        FontId::proportional(12.0),
        hex(0x9fbcd2),
        width,
    );
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    let left = rect.center().x - galley.size().x / 2.0;
    ui.painter()
        .galley(pos2(left, rect.top()), galley, hex(0x9fbcd2));
}

fn text_button(ui: &mut Ui, text: &str) -> egui::Response {
    let width = paint::text_width(ui.painter(), text, bold_font(12.0), 0.2) + 24.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    paint::labelled(&response, true, text);
    paint_text_button(ui.painter(), rect, text, response.hovered());
    response
}

fn paint_text_button(painter: &egui::Painter, rect: Rect, text: &str, hovered: bool) {
    painter.rect_filled(rect, 14, hexa(0xffffff, if hovered { 0.12 } else { 0.06 }));
    painter.rect_stroke(
        rect,
        14,
        Stroke::new(
            1.0,
            if hovered {
                hex(0xcfe8fa)
            } else {
                hexa(0x8bc2ec, 0.33)
            },
        ),
        StrokeKind::Inside,
    );
    paint::text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        bold_font(12.0),
        if hovered {
            Color32::WHITE
        } else {
            hex(0xd9eaf8)
        },
        0.2,
    );
}

fn text_button_at(ui: &mut Ui, right: f32, top: f32, text: &str, id: &str) -> bool {
    let width = paint::text_width(ui.painter(), text, bold_font(12.0), 0.2) + 24.0;
    let rect = Rect::from_min_size(pos2(right - width, top), vec2(width, 28.0));
    let response = ui.interact(rect, Id::unique(id), Sense::click());
    paint::labelled(&response, true, text);
    paint_text_button(ui.painter(), rect, text, response.hovered());
    response.clicked()
}

/// Where a room dialog's head put its eyebrow line and its title's line, for what
/// stands beside them: the invite button and the final score.
struct Head {
    eyebrow: Rect,
    title: Rect,
}

/// A room dialog's head (`.dialog-head`): ROOM and its code, the title, and the lines
/// under it, `HEAD_GAP` apart. `invite` makes the eyebrow line as tall as the invite
/// button that stands beside it (`.dialog-eyebrow` aligns their baselines).
fn room_head(
    ui: &mut Ui,
    room: &str,
    title: &str,
    invite: bool,
    lines: &[(&str, Color32)],
) -> Head {
    let width = ui.available_width();
    let painter = ui.painter().clone();
    let galleys: Vec<_> = lines
        .iter()
        .filter(|(text, _)| !text.is_empty())
        .map(|&(text, color)| lede(&painter, text, color, width))
        .collect();
    let eyebrow_line = if invite {
        TEXT_BUTTON_HEIGHT
    } else {
        EYEBROW_LINE
    };
    let height = eyebrow_line
        + HEAD_GAP
        + TITLE_LINE
        + galleys
            .iter()
            .map(|galley| HEAD_GAP + galley.size().y)
            .sum::<f32>();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let eyebrow = Rect::from_min_size(rect.min, vec2(width, eyebrow_line));
    room_eyebrow(&painter, pos2(eyebrow.left(), eyebrow.center().y), room);
    let title_line = Rect::from_min_size(
        pos2(rect.left(), eyebrow.bottom() + HEAD_GAP),
        vec2(width, TITLE_LINE),
    );
    // The display face's capitals sit a point under the middle of its line.
    paint::shadowed(
        &painter,
        pos2(title_line.left(), title_line.center().y + 1.0),
        Align2::LEFT_CENTER,
        title,
        display_font(50.0),
        Color32::WHITE,
        1.0,
    );
    let mut top = title_line.bottom();
    for galley in galleys {
        top += HEAD_GAP;
        let size = galley.size();
        painter.galley(pos2(rect.left(), top), galley, TEXT);
        top += size.y;
    }
    Head {
        eyebrow,
        title: title_line,
    }
}

/// ROOM in the label blue and the code in the menu's text colour (`.dialog-eyebrow b`),
/// on the line through `left_center`.
fn room_eyebrow(painter: &Painter, left_center: Pos2, room: &str) {
    // The code ends where it would in the line as one text: a laid-out text drops the
    // space after ROOM.
    let line = paint::text_width(painter, &format!("ROOM {room}"), bold_font(12.0), 2.4);
    let code = paint::text_width(painter, room, bold_font(12.0), 2.4);
    paint::text(
        painter,
        left_center,
        Align2::LEFT_CENTER,
        "ROOM",
        bold_font(12.0),
        TEXT_LABEL,
        2.4,
    );
    paint::text(
        painter,
        pos2(left_center.x + line - code, left_center.y),
        Align2::LEFT_CENTER,
        room,
        bold_font(12.0),
        hex(MENU_TEXT),
        2.4,
    );
}

/// A line of the head under the title (`.dialog-lede`: 13 px at 1.5), wrapped to `width`.
fn lede(painter: &Painter, text: &str, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: FontId::proportional(13.0),
            color,
            line_height: Some(LEDE_LINE),
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    painter.layout_job(job)
}

fn action(ui: &mut Ui, art: &Art, label: &str, style: ButtonStyle, width: f32) -> bool {
    paint::button(ui, art, label, None, style, vec2(width, 48.0), true).clicked()
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
    menu.copy_answered();
    let mut art = state
        .art
        .take()
        .unwrap_or_else(|| super::icons::Art::new(context));
    art.refresh(context);
    if menu.joining {
        joining(context, menu, &art, &mut actions);
    } else {
        if menu.in_battle() {
            hud::hud(context, state, &art, view, &mut actions);
            // A phone's arena keeps to its own few parts.
            if !crate::touch::phone() {
                players(context, menu, state);
            }
            if state.hud.as_ref().is_some_and(|hud| !hud.human.alive) {
                respawn(context, menu, state);
            }
        }
        match &menu.link {
            Link::Live if !menu.in_battle() && state.settings_draft.is_none() => {
                room_menu(context, menu, &art, view, &mut actions)
            }
            Link::Live => {}
            _ => connection(context, menu, &art, &mut actions),
        }
    }
    if state.settings_draft.is_some() {
        menus::settings(context, state, &art, view, &mut actions);
    }
    state.art = Some(art);
    if state.show_nerd_stats && view.debug {
        super::nerd_stats::panel(context, state);
    } else if view.debug {
        super::nerd_stats::link(context, state);
    }
    actions
}

/// Battle Setup's place while the room connects and compiles its arena: its backdrop, as
/// the arena stays out of sight until the room can show it, and a card with Battle
/// Setup's status well (`join-screen.ts`: "Joining room …", then what it waits for).
/// The card has its size from the first frame, so the room's first frame shows it.
fn joining(context: &egui::Context, menu: &RoomMenu, art: &Art, actions: &mut Vec<Action>) {
    const WIDTH: f32 = 480.0;
    const PADDING: [f32; 3] = [28.0, 24.0, 24.0];
    const BUTTON: f32 = 48.0;
    super::backdrop::backdrop(context, art);
    let [side, top, bottom] = PADDING;
    let width = menus::phone_width(context, WIDTH);
    let height = top
        + EYEBROW_LINE
        + HEAD_GAP
        + TITLE_LINE
        + ROOM_GAP
        + STATUS_WELL
        + ROOM_GAP
        + BUTTON
        + bottom;
    let card = Rect::from_center_size(context.content_rect().center(), vec2(width, height));
    let layer = LayerId::new(Order::Foreground, Id::unique("joining"));
    // An area new to egui lays out unseen for a frame and then fades in; this one's size
    // is known, so it starts sized and shows at once.
    if egui::AreaState::load(context, layer.id).is_none() {
        context.memory_mut(|memory| {
            memory.areas_mut().set_state(
                layer,
                egui::AreaState {
                    pivot_pos: Some(card.min),
                    pivot: Align2::LEFT_TOP,
                    size: Some(card.size()),
                    interactable: true,
                    last_became_visible_at: None,
                },
            );
        });
    }
    egui::Area::new(layer.id)
        .order(Order::Foreground)
        .fixed_pos(card.min)
        .constrain(false)
        .fade_in(false)
        .show(context, |ui| {
            ui.painter().extend(paint::plate_shapes(card, 14.0, None));
            let content =
                Rect::from_min_max(card.min + vec2(side, top), card.max - vec2(side, bottom));
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(content));
            body.spacing_mut().item_spacing = vec2(ROW_GAP, ROOM_GAP);
            let title = if menu.creating {
                "CREATING ROOM"
            } else {
                "JOINING ROOM"
            };
            room_head(&mut body, &menu.room, title, false, &[]);
            let (well, _) =
                body.allocate_exact_size(vec2(body.available_width(), STATUS_WELL), Sense::hover());
            let verb = if menu.creating { "Creating" } else { "Joining" };
            let time = body.input(|input| input.time);
            status_well(
                body.painter(),
                well,
                &format!("{verb} room {}", menu.room),
                &menu.status,
                time,
            );
            body.ctx().request_repaint();
            let width = body.available_width();
            if action(&mut body, art, "CANCEL", ButtonStyle::Secondary, width) {
                actions.push(Action::RoomLeave);
            }
            ui.allocate_rect(card, Sense::hover());
        });
}

/// Battle Setup's status well while a room loads (`.room-status[data-state="loading"]`):
/// a turning gold ring, what is happening over what it waits for, and a gold sweep
/// along its foot.
fn status_well(painter: &Painter, rect: Rect, message: &str, hint: &str, time: f64) {
    painter.rect_filled(rect, 10, hex(0x0a1a33));
    painter.rect_stroke(
        rect,
        10,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.08)),
        StrokeKind::Inside,
    );
    let center = pos2(rect.left() + STATUS_PADDING + 10.0, rect.center().y);
    let ring = Stroke::new(3.5, hexa(0xffcf38, 0.15));
    painter.circle_stroke(center, STATUS_RING - 1.75, ring);
    // A quarter of the ring, once round every 0.9 s (`room-scan`).
    let start = (time / 0.9).fract() as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let arc: Vec<Pos2> = (0..=12)
        .map(|step| {
            let angle = start + step as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
            center + vec2(angle.cos(), angle.sin()) * (STATUS_RING - 1.75)
        })
        .collect();
    painter.add(Shape::line(arc, Stroke::new(3.5, GOLD)));
    let left = center.x + 10.0 + 10.0;
    let copy_width = rect.right() - STATUS_PADDING - left;
    paint::text_fit(
        painter,
        pos2(left, rect.center().y - 1.0),
        Align2::LEFT_BOTTOM,
        message,
        FontId::proportional(12.5),
        hex(0xe6f1fb),
        0.0,
        copy_width,
    );
    paint::text_fit(
        painter,
        pos2(left, rect.center().y),
        Align2::LEFT_TOP,
        hint,
        FontId::proportional(12.0),
        TEXT_DIM,
        0.0,
        copy_width,
    );
    // The sweep: 38 % of the track, from just off its left end to past its right one,
    // easing in and out (`startup-sweep`), cut off at the track's ends.
    let track = Rect::from_min_max(
        pos2(rect.left() + STATUS_PADDING, rect.bottom() - 2.0),
        pos2(rect.right() - STATUS_PADDING, rect.bottom()),
    );
    let phase = ((time / SWEEP_SECONDS).fract()) as f32;
    let eased = phase * phase * (3.0 - 2.0 * phase);
    let sweep = track.width() * SWEEP_SHARE;
    let x = track.left() - sweep + eased * sweep * 4.65;
    let clipped = painter.with_clip_rect(track.intersect(painter.clip_rect()));
    clipped.add(paint::hgradient(
        Rect::from_min_size(pos2(x, track.top()), vec2(sweep, track.height())),
        &[(0.0, Color32::TRANSPARENT), (0.5, GOLD), (1.0, GOLD_HOT)],
    ));
}

/// The connection dropped (reconnecting by itself) or ended (the way back that fits).
fn connection(context: &egui::Context, menu: &RoomMenu, art: &Art, actions: &mut Vec<Action>) {
    room_dialog(context, "connection", 480.0, |ui| match &menu.link {
        Link::Connecting(dialog) => {
            room_head(
                ui,
                &menu.room,
                dialog.title,
                false,
                &[(&dialog.message, hex(LEDE))],
            );
            ui.add(egui::Spinner::new().color(GOLD));
            let width = ui.available_width();
            if action(ui, art, "LEAVE ROOM", ButtonStyle::Secondary, width) {
                actions.push(Action::RoomLeave);
            }
        }
        Link::Ended(ending) => {
            room_head(
                ui,
                &menu.room,
                ending.title,
                false,
                &[(&ending.text, hex(LEDE))],
            );
            let width = ui.available_width();
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            if let Some(retry) = ending.retry
                && action(ui, art, retry, ButtonStyle::Primary, width)
            {
                actions.push(Action::RoomRejoin);
            }
            if action(
                ui,
                art,
                ending.setup,
                if ending.retry.is_some() {
                    ButtonStyle::Secondary
                } else {
                    ButtonStyle::Primary
                },
                width,
            ) {
                actions.push(if ending.reload {
                    Action::Reload
                } else {
                    Action::RoomSetup(ending.text.clone())
                });
            }
        }
        Link::Live => {}
    });
}

/// The room menu: between battles the lobby or the results, during one the menu that
/// hands the tank to a bot while it is open.
fn room_menu(
    context: &egui::Context,
    menu: &mut RoomMenu,
    art: &Art,
    view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(lobby) = menu.lobby.clone() else {
        return;
    };
    let host = lobby.host_id == menu.player_id;
    let playing = lobby.phase == RoomPhase::Playing;
    let alone = lobby.players.len() == 1;
    let text = menu.text.clone();
    // A phone gets the short menu: the room, what is happening, the results and the
    // actions. Rules, team and tank changes and the controls need a larger screen (a
    // phone keeps the tank it chose in Battle Setup); the teams show only in the lobby,
    // where they say who the battle waits for.
    let phone = crate::touch::phone();
    room_dialog(context, "room", 560.0, |ui| {
        let head = room_head(
            ui,
            &menu.room,
            text.title,
            true,
            &[(&text.hint, hex(LEDE)), (&menu.notice, GOLD)],
        );
        let copy = if menu.copied > 0.0 {
            "Link copied"
        } else {
            "Copy invite link"
        };
        if text_button_at(
            ui,
            head.eyebrow.right(),
            head.eyebrow.top(),
            copy,
            "copy room invite",
        ) {
            let link = links::invite_link(&menu.room);
            menu.copying = Some(links::copy_text(ui.ctx(), link));
            menu.copy_answered();
        }
        // The final score stands at the end of the title's line, centred on it.
        if lobby.phase == RoomPhase::Results
            && let Some(outcome) = &menu.outcome
        {
            room_scores(ui.painter(), head.title.right_center(), outcome.scores);
        }
        if playing {
            summary(ui, &text.summary);
        }
        if lobby.phase == RoomPhase::Results {
            ui.scope(|ui| scoreboard(ui, &lobby, &menu.player_id));
        }
        if !phone || lobby.phase == RoomPhase::Lobby {
            roster(ui, &lobby, &menu.player_id, playing);
        }
        if !playing && !phone {
            ui.scope(|ui| next_battle(ui, menu, &lobby, host, view.debug, actions));
        }
        let width = ui.available_width();
        // The main action has a row to itself and the rest share the next, `ROW_GAP`
        // apart (`.network-actions`); the last player's note follows as closely.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            if host && !playing {
                let label = if lobby.phase == RoomPhase::Results {
                    "PLAY AGAIN"
                } else {
                    "START BATTLE"
                };
                if action(ui, art, label, ButtonStyle::Primary, width) {
                    actions.push(Action::RoomStart);
                }
            }
            if playing && action(ui, art, "RESUME", ButtonStyle::Primary, width) {
                actions.push(Action::Resume);
            }
            ui.horizontal(|ui| {
                let half = if host && playing {
                    (width - ROW_GAP) / 2.0
                } else {
                    width
                };
                if host && playing && action(ui, art, "END BATTLE", ButtonStyle::Danger, half) {
                    actions.push(Action::RoomEnd);
                }
                if action(
                    ui,
                    art,
                    if playing {
                        "LEAVE ROOM"
                    } else {
                        "BATTLE SETUP"
                    },
                    ButtonStyle::Secondary,
                    half,
                ) {
                    actions.push(Action::RoomLeave);
                }
            });
            if alone {
                note(
                    ui,
                    "You're the last player here, so leaving closes the room.",
                );
            }
        });
        ui.horizontal(|ui| {
            if text_button(ui, "Settings").clicked() {
                actions.push(Action::OpenSettings);
            }
            if phone {
                return;
            }
            // The fold arrow is painted: the page's fonts have no ▸ or ▾.
            const ARROW_ROOM: f32 = 13.0;
            let controls_width =
                ARROW_ROOM + paint::text_width(ui.painter(), "Controls", bold_font(12.0), 0.0);
            ui.add_space(((width - controls_width) / 2.0 - 85.0).max(0.0));
            let (rect, response) =
                ui.allocate_exact_size(vec2(controls_width, 22.0), Sense::click());
            paint::labelled(&response, true, "Controls");
            let color = hex(0x9fbcd2);
            let tip = pos2(rect.left() + 4.0, rect.center().y);
            let arrow = if menu.controls_open {
                vec![
                    tip + vec2(-3.5, -2.5),
                    tip + vec2(3.5, -2.5),
                    tip + vec2(0.0, 3.0),
                ]
            } else {
                vec![
                    tip + vec2(-2.0, -3.5),
                    tip + vec2(3.0, 0.0),
                    tip + vec2(-2.0, 3.5),
                ]
            };
            ui.painter()
                .add(egui::Shape::convex_polygon(arrow, color, Stroke::NONE));
            paint::text(
                ui.painter(),
                pos2(rect.left() + ARROW_ROOM, rect.center().y),
                Align2::LEFT_CENTER,
                "Controls",
                bold_font(12.0),
                color,
                0.0,
            );
            if response.clicked() {
                menu.controls_open = !menu.controls_open;
            }
        });
        if menu.controls_open {
            let origin = ui.cursor().min;
            let height = menus::controls_card(ui.painter(), origin, width, 1);
            ui.allocate_rect(
                Rect::from_min_size(origin, vec2(width, height)),
                Sense::hover(),
            );
        }
    });
}

/// Room results show only the final numbers; team labels belong to single player's score plates.
fn room_scores(painter: &egui::Painter, right_center: egui::Pos2, scores: [u32; 2]) {
    let font = display_font(34.0);
    let widths = scores.map(|score| {
        (paint::text_width(painter, &score.to_string(), font.clone(), 0.0) + 28.0).max(58.0)
    });
    let colon = 18.0;
    let left = right_center.x - widths[0] - widths[1] - colon;
    let top = right_center.y - 22.5;
    for index in 0..2 {
        let x = left + if index == 0 { 0.0 } else { widths[0] + colon };
        let rect = Rect::from_min_size(pos2(x, top), vec2(widths[index], 45.0));
        let polygon = if index == 0 {
            vec![
                rect.left_top() + vec2(10.0, 0.0),
                rect.right_top(),
                rect.right_bottom(),
                rect.left_bottom(),
            ]
        } else {
            vec![
                rect.left_top(),
                rect.right_top(),
                rect.right_bottom() - vec2(10.0, 0.0),
                rect.left_bottom(),
            ]
        };
        painter.add(paint::gradient_polygon(
            &polygon,
            if index == 0 {
                hex(0x2f8ff2)
            } else {
                hex(0xff5367)
            },
            if index == 0 {
                hex(0x1252b0)
            } else {
                hex(0xb01b33)
            },
        ));
        paint::shadowed(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            &scores[index].to_string(),
            font.clone(),
            Color32::WHITE,
            0.0,
        );
    }
    paint::text(
        painter,
        pos2(left + widths[0] + colon / 2.0, right_center.y),
        Align2::CENTER_CENTER,
        ":",
        display_font(30.0),
        hex(0x6f88a3),
        0.0,
    );
}

/// Read-only rule chips, in the room menu's own dark wells.
fn summary(ui: &mut Ui, chips: &[String]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    summary_at(ui.painter(), rect.min, chips);
}

fn summary_at(painter: &egui::Painter, origin: egui::Pos2, chips: &[String]) {
    let mut x = origin.x;
    for text in chips {
        let width = paint::text_width(painter, text, bold_font(12.0), 0.0) + 22.0;
        let rect = Rect::from_min_size(pos2(x, origin.y), vec2(width, 27.0));
        x = rect.right() + 6.0;
        painter.rect_filled(rect, 14, hexa(0x050d1c, 0.8));
        painter.rect_stroke(
            rect,
            14,
            Stroke::new(1.0, hexa(0x9fd0ff, 0.18)),
            StrokeKind::Inside,
        );
        paint::text(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            bold_font(12.0),
            hex(0xeef6fb),
            0.0,
        );
    }
}

/// NEXT BATTLE: the rules as text, which the host unfolds to change, and the player's
/// own team and tank, under a hairline and `ROW_GAP` apart (`.network-next`).
fn next_battle(
    ui: &mut egui::Ui,
    menu: &mut RoomMenu,
    lobby: &Lobby,
    host: bool,
    debug: bool,
    actions: &mut Vec<Action>,
) {
    /// The hairline over NEXT BATTLE and the space under it (`padding-top`).
    const RULE_ROOM: f32 = 15.0;
    const LABEL_LINE: f32 = 13.0;
    let settings = &lobby.settings;
    let extra = map_option_for(settings.map_mode).extra;
    let width = ui.available_width();
    let (rule, _) = ui.allocate_exact_size(vec2(width, RULE_ROOM), Sense::hover());
    ui.painter().line_segment(
        [rule.left_top(), rule.right_top()],
        Stroke::new(1.0, hexa(0x9fd0ff, 0.09)),
    );
    // The rule's padding stands in for the gap under it.
    ui.add_space(-ROOM_GAP);
    ui.spacing_mut().item_spacing.y = ROW_GAP;
    let (label, _) = ui.allocate_exact_size(vec2(width, LABEL_LINE), Sense::hover());
    paint::text(
        ui.painter(),
        pos2(label.left(), label.center().y),
        Align2::LEFT_CENTER,
        "NEXT BATTLE",
        bold_font(10.5),
        TEXT_LABEL,
        2.5,
    );
    let (rules_line, _) = ui.allocate_exact_size(vec2(width, TEXT_BUTTON_HEIGHT), Sense::hover());
    summary_at(ui.painter(), rules_line.min, &menu.text.summary);
    if host {
        let label = if menu.editing_rules {
            "Done"
        } else {
            "Change rules"
        };
        if text_button_at(
            ui,
            rules_line.right(),
            rules_line.top(),
            label,
            "change room rules",
        ) {
            // Done lets go of MINUTES as a blur would: hidden from this frame on, the
            // field could not apply a length typed into it.
            if menu.editing_rules
                && let Some(round_minutes) = take_minutes_draft(menu)
                && round_minutes != settings.round_minutes
            {
                actions.push(Action::RoomRules(RoomSettings {
                    round_minutes,
                    ..*settings
                }));
            }
            menu.editing_rules = !menu.editing_rules;
        }
    }
    if host && menu.editing_rules {
        let current: RoomSettings = *settings;
        let mut rules = current;
        let [map, bots, minutes] = field_columns::<3>(ui);
        // The extra levels with the debug tools, or while the room plays one.
        let maps = MAP_OPTIONS
            .iter()
            .filter(|map| debug || extra || !map.extra);
        select_field(ui, map, "MAP", map_option_for(rules.map_mode).name, |ui| {
            for option in maps {
                ui.selectable_value(&mut rules.map_mode, option.id, option.name);
            }
        });
        // `None`: humans only.
        let mut picked_bots: Option<Difficulty> = (!rules.humans_only).then_some(rules.difficulty);
        select_field(
            ui,
            bots,
            "BOTS",
            picked_bots.map_or("None", difficulty_name),
            |ui| {
                for option in &DIFFICULTY_OPTIONS {
                    ui.selectable_value(&mut picked_bots, Some(option.difficulty), option.name);
                }
                ui.selectable_value(&mut picked_bots, None, "None");
            },
        );
        rules.humans_only = picked_bots.is_none();
        if let Some(difficulty) = picked_bots {
            rules.difficulty = difficulty;
        }
        if let Some(round_minutes) = minutes_field(ui, minutes, menu, rules.round_minutes) {
            rules.round_minutes = round_minutes;
        }
        if rules != current {
            actions.push(Action::RoomRules(rules));
        }
    }

    if let Some(mine) = menu.mine().cloned() {
        let tank_name = title_case(vehicle(mine.kind).name);
        let (choice_line, _) =
            ui.allocate_exact_size(vec2(width, TEXT_BUTTON_HEIGHT), Sense::hover());
        let label = paint::text(
            ui.painter(),
            pos2(choice_line.left(), choice_line.center().y),
            Align2::LEFT_CENTER,
            "You:",
            FontId::proportional(13.0),
            hex(MENU_TEXT),
            0.0,
        );
        paint::text(
            ui.painter(),
            pos2(label.right() + 4.0, choice_line.center().y),
            Align2::LEFT_CENTER,
            &format!("{} · {tank_name}", team_word(mine.team)),
            bold_font(13.0),
            hex(0xf5faff),
            0.0,
        );
        let label = if menu.editing_choice {
            "Done"
        } else {
            "Change team or tank"
        };
        if text_button_at(
            ui,
            choice_line.right(),
            choice_line.top(),
            label,
            "change room player",
        ) {
            menu.editing_choice = !menu.editing_choice;
        }
        if menu.editing_choice {
            let team = Some(mine.team);
            let kind = mine.kind;
            let mut picked_team = team;
            let mut picked_kind = kind;
            let [team_column, tank_column] = field_columns::<2>(ui);
            select_field(ui, team_column, "TEAM", team_word(mine.team), |ui| {
                for (choice, label) in [
                    (None, "Auto · fewer humans"),
                    (Some(Team::Blue), "Blue"),
                    (Some(Team::Red), "Red"),
                ] {
                    ui.selectable_value(&mut picked_team, choice, label);
                }
            });
            select_field(ui, tank_column, "TANK", &tank_option(kind), |ui| {
                for tank in &TANK_CARDS {
                    ui.selectable_value(&mut picked_kind, tank.kind, tank_option(tank.kind));
                }
            });
            if picked_team != team || picked_kind != kind {
                actions.push(Action::RoomChoose {
                    team: picked_team,
                    kind: picked_kind,
                });
            }
        }
    }
}

/// A tank as the room menu's TANK field names it: its name and its class
/// (`network-ui.ts`: "Bruiser · balanced tank").
fn tank_option(kind: VehicleKind) -> String {
    let class = TANK_CARDS
        .iter()
        .find(|card| card.kind == kind)
        .map_or_else(String::new, |card| card.class.to_lowercase());
    format!("{} · {class}", title_case(vehicle(kind).name))
}

/// A row of `N` equal fields `ROW_GAP` apart (`.network-fields`), each a label over its
/// well; takes the row's room and returns each field's column.
fn field_columns<const N: usize>(ui: &mut Ui) -> [Rect; N] {
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(
        vec2(width, FIELD_LABEL + HEAD_GAP + FIELD_HEIGHT),
        Sense::hover(),
    );
    let column = (width - ROW_GAP * (N as f32 - 1.0)) / N as f32;
    std::array::from_fn(|index| {
        Rect::from_min_size(
            pos2(row.left() + index as f32 * (column + ROW_GAP), row.top()),
            vec2(column, row.height()),
        )
    })
}

/// A field's label at the top of its column; returns the well under it.
fn field_label(painter: &Painter, column: Rect, label: &str) -> Rect {
    paint::text(
        painter,
        pos2(column.left(), column.top() + FIELD_LABEL / 2.0),
        Align2::LEFT_CENTER,
        label,
        bold_font(10.5),
        TEXT_LABEL,
        2.5,
    );
    Rect::from_min_size(
        pos2(column.left(), column.bottom() - FIELD_HEIGHT),
        vec2(column.width(), FIELD_HEIGHT),
    )
}

/// A field's sunk well: the menu's dark glass, a hairline, and a shadow inside its top
/// edge (`box-shadow: inset 0 2px 6px`).
fn field_well(painter: &Painter, rect: Rect, hovered: bool) {
    painter.rect_filled(rect, 9, hexa(0x050d1c, 0.8));
    paint::rounded_vgradient(
        painter,
        Rect::from_min_size(rect.min, vec2(rect.width(), 8.0)),
        [9.0, 9.0, 0.0, 0.0],
        &[(0.0, hexa(0x000000, 0.45)), (1.0, Color32::TRANSPARENT)],
    );
    let edge = if hovered {
        hexa(0x9fd0ff, 0.45)
    } else {
        hexa(0x9fd0ff, 0.2)
    };
    painter.rect_stroke(rect, 9, Stroke::new(1.0, edge), StrokeKind::Inside);
}

/// A select (`.network-menu select`): the choice in bold on the left and a gold chevron
/// on the right; a click opens `options` under it.
fn select_field(
    ui: &mut Ui,
    column: Rect,
    label: &str,
    value: &str,
    options: impl FnOnce(&mut Ui),
) {
    let rect = field_label(ui.painter(), column, label);
    let response = ui
        .interact(rect, Id::unique(("room field", label)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, title_case(label));
        info.current_text_value = Some(value.to_owned());
        info
    });
    let painter = ui.painter();
    field_well(painter, rect, response.hovered());
    paint::text_fit(
        painter,
        pos2(rect.left() + FIELD_PADDING, rect.center().y),
        Align2::LEFT_CENTER,
        value,
        bold_font(14.0),
        hex(0xf5faff),
        0.0,
        rect.width() - FIELD_PADDING - CHEVRON_ROOM,
    );
    // The page's chevron: a 12 × 8 stroke whose right end is `FIELD_PADDING` in.
    let tip = pos2(rect.right() - FIELD_PADDING - 6.0, rect.center().y + 2.5);
    painter.add(Shape::line(
        vec![tip + vec2(-5.0, -5.0), tip, tip + vec2(5.0, -5.0)],
        Stroke::new(2.0, GOLD),
    ));
    egui::Popup::menu(&response).width(rect.width()).show(|ui| {
        ui.style_mut().override_font_id = Some(bold_font(14.0));
        options(ui);
    });
}

/// MINUTES (`.network-fields input`): typed in place and applied when the field lets
/// go, if it holds a whole number of minutes a room allows; `None` until then.
fn minutes_field(ui: &mut Ui, column: Rect, menu: &mut RoomMenu, minutes: u32) -> Option<u32> {
    let rect = field_label(ui.painter(), column, "MINUTES");
    let hovered = ui.rect_contains_pointer(rect);
    field_well(ui.painter(), rect, hovered);
    let text = menu
        .minutes_draft
        .get_or_insert_with(|| minutes.to_string());
    let inner = rect.shrink2(vec2(FIELD_PADDING, 0.0));
    let response = ui.put(
        inner,
        egui::TextEdit::singleline(text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(bold_font(14.0))
            .text_color(hex(0xf5faff))
            .align(Align2::LEFT_CENTER)
            .min_size(inner.size())
            .desired_width(inner.width()),
    );
    if response.lost_focus() {
        return take_minutes_draft(menu);
    }
    // Away from the keyboard the field shows the room's own length.
    if !response.has_focus() {
        menu.minutes_draft = None;
    }
    None
}

/// The length typed into MINUTES, if it is a whole number of minutes a room allows; the
/// field shows the room's own length again either way.
fn take_minutes_draft(menu: &mut RoomMenu) -> Option<u32> {
    let typed = menu.minutes_draft.take()?.trim().parse::<u32>().ok();
    typed.filter(|value| (1..=MAX_ROUND_MINUTES).contains(value))
}

/// LAST ROUND: everyone's kills and deaths, the best first, in a full-width table whose
/// columns share the width as the page's automatic table layout does: each as wide as
/// its widest content, and the spare width in proportion to that.
fn scoreboard(ui: &mut Ui, lobby: &Lobby, player_id: &str) {
    /// A row's name after its team's bar and padding (`td:first-child`).
    const NAME_INSET: f32 = 12.0;
    let mut rows = lobby.scoreboard.clone();
    rows.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
    let width = ui.available_width();
    let painter = ui.painter().clone();
    let head_width = |label: &str| paint::text_width(&painter, label, bold_font(10.0), 1.5);
    let number_width = |value: u32| {
        paint::text_width(
            &painter,
            &value.to_string(),
            FontId::proportional(13.0),
            0.0,
        )
    };
    let columns = [
        rows.iter()
            .map(|row| NAME_INSET + paint::text_width(&painter, &row.name, bold_font(13.0), 0.0))
            .fold(head_width("LAST ROUND"), f32::max),
        rows.iter()
            .map(|row| number_width(row.kills))
            .fold(head_width("KILLS"), f32::max),
        rows.iter()
            .map(|row| number_width(row.deaths))
            .fold(head_width("DEATHS"), f32::max),
    ];
    let share = width / columns.iter().sum::<f32>().max(1.0);
    let kills_right = (columns[0] + columns[1]) * share;
    // The header and the rows are one table.
    ui.spacing_mut().item_spacing.y = 0.0;
    let (head, _) = ui.allocate_exact_size(vec2(width, SCORE_HEADER), Sense::hover());
    // The labels sit on the header's text line, its padding under them.
    let label_y = head.top() + 7.0;
    for (label, x, align) in [
        ("LAST ROUND", head.left(), Align2::LEFT_CENTER),
        ("KILLS", head.left() + kills_right, Align2::RIGHT_CENTER),
        ("DEATHS", head.right(), Align2::RIGHT_CENTER),
    ] {
        paint::text(
            &painter,
            pos2(x, label_y),
            align,
            label,
            bold_font(10.0),
            hex(0x9fbcd2),
            1.5,
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("room scoreboard")
        .max_height(200.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &rows {
                let (rect, _) = ui.allocate_exact_size(vec2(width, SCORE_ROW), Sense::hover());
                let painter = ui.painter();
                painter.line_segment(
                    [rect.left_top(), rect.right_top()],
                    Stroke::new(1.0, hexa(0x9fd0ff, 0.09)),
                );
                painter.rect_filled(
                    Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                    0,
                    team_color(row.team),
                );
                let color = if row.player_id == player_id {
                    GOLD
                } else {
                    TEXT
                };
                let y = rect.center().y;
                paint::text_fit(
                    painter,
                    pos2(rect.left() + NAME_INSET, y),
                    Align2::LEFT_CENTER,
                    &row.name,
                    bold_font(13.0),
                    color,
                    0.0,
                    columns[0] * share - NAME_INSET,
                );
                for (value, x) in [
                    (row.kills, rect.left() + kills_right),
                    (row.deaths, rect.right()),
                ] {
                    paint::text(
                        painter,
                        pos2(x, y),
                        Align2::RIGHT_CENTER,
                        &value.to_string(),
                        FontId::proportional(13.0),
                        color,
                        0.0,
                    );
                }
            }
        });
}

/// A team's colour in the room menu (`--blue`, `--red`).
fn team_color(team: Team) -> Color32 {
    if team == Team::Blue {
        hex(0x74c4ff)
    } else {
        hex(0xff8592)
    }
}

/// A team card's top border (`border-top: 3px` under a 10 px radius): full width along
/// the top and thinning to nothing down each rounded corner, as the page's border bends
/// with the card's corners.
fn team_stripe(painter: &Painter, rect: Rect, color: Color32) {
    const ARC_STEPS: usize = 8;
    let radius = TEAM_RADIUS;
    let inner = radius - TEAM_STRIPE;
    // Both corners turn about the same centres: the outer edge on a circle, the inner
    // on an ellipse as tall as the radius less the border.
    let corner = |center: Pos2, angle: f32| {
        let (sin, cos) = angle.sin_cos();
        (
            center + vec2(cos, sin) * radius,
            center + vec2(cos * radius, sin * inner),
        )
    };
    let left = pos2(rect.left() + radius, rect.top() + radius);
    let right = pos2(rect.right() - radius, rect.top() + radius);
    let mut edges = Vec::with_capacity(2 * ARC_STEPS + 2);
    for step in 0..=ARC_STEPS {
        let t = step as f32 / ARC_STEPS as f32;
        edges.push(corner(left, std::f32::consts::PI * (1.0 + 0.5 * t)));
    }
    for step in 0..=ARC_STEPS {
        let t = step as f32 / ARC_STEPS as f32;
        edges.push(corner(right, std::f32::consts::PI * (1.5 + 0.5 * t)));
    }
    let mut mesh = Mesh::default();
    for (index, (outer, inner)) in edges.into_iter().enumerate() {
        mesh.colored_vertex(outer, color);
        mesh.colored_vertex(inner, color);
        if index > 0 {
            let base = (index as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Both teams' players, with the seats bots or nobody fill.
fn roster(ui: &mut Ui, lobby: &Lobby, player_id: &str, playing: bool) {
    // An extra level brings its own roster (`teamTanks`), unless only humans play.
    let team_tanks = map_card(lobby.settings.map_mode)
        .team_tanks
        .filter(|_| !lobby.settings.humans_only)
        .map_or(TEAM_SLOTS, |tanks| tanks as usize);
    let width = ui.available_width();
    let column_width = (width - ROW_GAP) / 2.0;
    let teams = [Team::Blue, Team::Red].map(|team| {
        lobby
            .players
            .iter()
            .filter(|player| player.team == team)
            .collect::<Vec<&Player>>()
    });
    let longest = teams.iter().map(Vec::len).max().unwrap_or(0);
    // Bots take the empty places, or they stay open seats when only humans play.
    let empty_seat = if lobby.settings.humans_only {
        "open seat"
    } else {
        "bot"
    };
    let height = TEAM_HEADER + longest as f32 * ROSTER_ROW + TEAM_FOOT;
    let (whole, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    for (side, (name, members)) in TEAM_NAMES.iter().zip(&teams).enumerate() {
        let open = team_tanks.saturating_sub(members.len()) as u32;
        let fill = count_line(open, empty_seat);
        let rect = Rect::from_min_size(
            whole.min + vec2(side as f32 * (column_width + ROW_GAP), 0.0),
            vec2(column_width, height),
        );
        let painter = ui.painter();
        let color = team_color([Team::Blue, Team::Red][side]);
        painter.rect_filled(rect, TEAM_RADIUS, hexa(0x050d1c, 0.8));
        paint::rounded_vgradient(
            painter,
            Rect::from_min_size(rect.min, vec2(rect.width(), 46.0)),
            [TEAM_RADIUS, TEAM_RADIUS, 0.0, 0.0],
            &[(0.0, paint::fade(color, 0.14)), (1.0, Color32::TRANSPARENT)],
        );
        team_stripe(painter, rect, color);
        // The header's name and its seats share a baseline (`align-items: baseline`).
        paint::text(
            painter,
            pos2(rect.left() + 12.0, rect.top() + 25.0),
            Align2::LEFT_CENTER,
            &format!("{name} TEAM"),
            display_font(20.0),
            color,
            1.0,
        );
        paint::text(
            painter,
            pos2(rect.right() - 12.0, rect.top() + 28.0),
            Align2::RIGHT_CENTER,
            &fill,
            FontId::proportional(12.0),
            hex(0x9fbcd2),
            0.0,
        );
        for (index, player) in members.iter().enumerate() {
            let top = rect.top() + TEAM_HEADER + index as f32 * ROSTER_ROW;
            painter.line_segment(
                [
                    pos2(rect.left() + 12.0, top),
                    pos2(rect.right() - 12.0, top),
                ],
                Stroke::new(1.0, hexa(0x9fd0ff, 0.09)),
            );
            let y = top + ROSTER_ROW / 2.0;
            let alpha = if player.connected { 1.0 } else { 0.55 };
            let tags: Vec<&str> = [
                (player.player_id == player_id).then_some("YOU"),
                (player.player_id == lobby.host_id).then_some("HOST"),
                (!player.connected).then_some("RECONNECTING"),
            ]
            .into_iter()
            .flatten()
            .collect();
            let tags = tags.join(" · ");
            let tag_width = paint::text_width(painter, &tags, bold_font(10.0), 1.0);
            let name_right = rect.right()
                - 12.0
                - if tags.is_empty() {
                    0.0
                } else {
                    tag_width + 8.0
                }
                - if playing { 28.0 } else { 0.0 };
            let name = paint::text_fit(
                painter,
                pos2(rect.left() + 12.0, y),
                Align2::LEFT_CENTER,
                &player.name,
                bold_font(14.0),
                paint::fade(TEXT, alpha),
                0.0,
                (name_right - rect.left() - 12.0).max(30.0),
            );
            // The smaller tags share the name's baseline, so they sit a little lower.
            paint::text(
                painter,
                pos2(name.right() + 8.0, y + 1.5),
                Align2::LEFT_CENTER,
                &tags,
                bold_font(10.0),
                paint::fade(
                    if player.player_id == player_id {
                        GOLD
                    } else {
                        hex(0x9fbcd2)
                    },
                    alpha,
                ),
                1.0,
            );
            if playing {
                paint::text(
                    painter,
                    pos2(rect.right() - 12.0, y),
                    Align2::RIGHT_CENTER,
                    &player.kills.to_string(),
                    bold_font(14.0),
                    paint::fade(TEXT, alpha),
                    0.0,
                );
            }
        }
    }
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
                    .row(tank)
                    .map(|row| row.kills)
            })
            .unwrap_or(player.kills)
    };
    let mut players: Vec<&Player> = lobby.players.iter().collect();
    players.sort_by_key(|player| (player.team.index(), player.slot));
    egui::Area::new(Id::unique("room players"))
        .anchor(Align2::LEFT_TOP, [16.0, 88.0])
        .interactable(false)
        .show(context, |ui| {
            let rect = Rect::from_min_size(
                ui.cursor().min,
                vec2(224.0, 44.0 + players.len() as f32 * 27.0),
            );
            let painter = ui.painter();
            paint::rounded_vgradient(
                painter,
                rect,
                [12.0; 4],
                &[(0.0, hexa(0x11264a, 0.94)), (1.0, hexa(0x08142a, 0.95))],
            );
            painter.rect_stroke(
                rect,
                12,
                Stroke::new(1.0, hexa(0x9fd0ff, 0.15)),
                StrokeKind::Inside,
            );
            paint::text(
                painter,
                rect.min + vec2(12.0, 12.0),
                Align2::LEFT_TOP,
                &format!("PLAYERS · {}", players.len()),
                bold_font(9.5),
                TEXT_LABEL,
                2.0,
            );
            paint::text(
                painter,
                pos2(rect.right() - 12.0, rect.top() + 12.0),
                Align2::RIGHT_TOP,
                "KILLS",
                bold_font(9.5),
                TEXT_LABEL,
                2.0,
            );
            for (index, player) in players.iter().enumerate() {
                let top = rect.top() + 32.0 + index as f32 * 27.0;
                let alpha = if player.connected { 1.0 } else { 0.55 };
                let team = if player.team == Team::Blue {
                    hex(0x74c4ff)
                } else {
                    hex(0xff8592)
                };
                painter.rect_filled(
                    Rect::from_min_size(pos2(rect.left() + 12.0, top), vec2(3.0, 27.0)),
                    0,
                    paint::fade(team, alpha),
                );
                let you = if player.player_id == menu.player_id {
                    " (you)"
                } else {
                    ""
                };
                paint::text_fit(
                    painter,
                    pos2(rect.left() + 24.0, top + 13.5),
                    Align2::LEFT_CENTER,
                    &format!("{}{you}", player.name),
                    bold_font(12.0),
                    paint::fade(hex(0xf5faff), alpha),
                    0.0,
                    155.0,
                );
                paint::text(
                    painter,
                    pos2(rect.right() - 12.0, top + 13.5),
                    Align2::RIGHT_CENTER,
                    &kills(player).to_string(),
                    bold_font(12.0),
                    paint::fade(hex(0xf5faff), alpha),
                    0.0,
                );
            }
            ui.allocate_rect(rect, Sense::hover());
        });
}

/// The countdown to the next life and who ended the last one.
fn respawn(context: &egui::Context, menu: &RoomMenu, state: &UiState) {
    let Some(hud) = &state.hud else { return };
    let countdown = format!("RESPAWN IN {:.0}", hud.human.respawn.ceil());
    egui::Area::new(Id::unique("room respawn"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -22.0])
        .interactable(false)
        .show(context, |ui| {
            let painter = ui.painter().clone();
            let text_width = paint::text_width(&painter, &countdown, display_font(34.0), 1.5);
            let width = (text_width + 56.0)
                .max(280.0)
                .min(context.content_rect().width() - 48.0);
            let cause = paint::wrapped(
                &painter,
                &menu.death_cause,
                bold_font(13.0),
                hex(0xffd6da),
                width - 56.0,
            );
            let height = 16.0
                + 10.5
                + 6.0
                + 40.0
                + if menu.death_cause.is_empty() {
                    0.0
                } else {
                    20.0 + cause.size().y
                }
                + 18.0;
            let rect = Rect::from_min_size(ui.cursor().min, vec2(width, height));
            paint::rounded_vgradient(
                &painter,
                rect,
                [16.0; 4],
                &[(0.0, hexa(0x172d4f, 0.94)), (1.0, hexa(0x0b1930, 0.96))],
            );
            painter.rect_stroke(
                rect,
                16,
                Stroke::new(1.0, hexa(0x9fd0ff, 0.15)),
                StrokeKind::Inside,
            );
            paint::text(
                &painter,
                pos2(rect.center().x, rect.top() + 16.0),
                Align2::CENTER_TOP,
                "TANK DESTROYED",
                bold_font(10.5),
                hex(0xff8592),
                3.0,
            );
            paint::shadowed(
                &painter,
                pos2(rect.center().x, rect.top() + 32.5),
                Align2::CENTER_TOP,
                &countdown,
                display_font(34.0),
                Color32::WHITE,
                1.5,
            );
            if !menu.death_cause.is_empty() {
                let y = rect.top() + 82.5;
                painter.line_segment(
                    [
                        pos2(rect.left() + 28.0, y - 10.0),
                        pos2(rect.right() - 28.0, y - 10.0),
                    ],
                    Stroke::new(1.0, hexa(0x9fd0ff, 0.1)),
                );
                painter.galley(
                    pos2(rect.center().x - cause.size().x / 2.0, y),
                    cause,
                    Color32::WHITE,
                );
            }
            ui.allocate_rect(rect, Sense::hover());
        });
}

/// The phase the HUD and the pause logic see.
pub fn hud_phase(menu: &RoomMenu) -> MatchPhase {
    match menu.phase() {
        RoomPhase::Playing if menu.in_battle() => MatchPhase::Playing,
        RoomPhase::Playing => MatchPhase::Paused,
        RoomPhase::Results => MatchPhase::Results,
        RoomPhase::Lobby => MatchPhase::Ready,
    }
}

/// The same room and seats in each UI capture, with no socket or running server; the
/// words are the engine's for that room.
#[cfg(test)]
pub(super) fn snapshot_menu(name: &str) -> RoomMenu {
    use sloppy_core::net::client::EndCause;
    use sloppy_web::Shell;
    use sloppy_web::room_text;
    let players: Vec<Player> = [
        (
            "viewer",
            "YOU",
            Team::Blue,
            VehicleKind::Balanced,
            true,
            7,
            3,
        ),
        (
            "friend",
            "Rusty",
            Team::Blue,
            VehicleKind::Heavy,
            true,
            9,
            2,
        ),
        ("rival", "Crank", Team::Red, VehicleKind::Scout, true, 8, 3),
        (
            "returning",
            "Tusk",
            Team::Red,
            VehicleKind::Balanced,
            false,
            4,
            3,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(index, (id, name, team, kind, connected, kills, deaths))| Player {
            player_id: id.into(),
            name: name.into(),
            kind,
            team,
            slot: (index % 2) as u32,
            connected,
            kills,
            deaths,
            tank_id: Some(index as u32 + 1),
        },
    )
    .collect();
    let mut menu = RoomMenu::new("KQ7M2XHP", false);
    menu.joining = false;
    menu.player_id = "viewer".into();
    menu.link = Link::Live;
    menu.lobby = Some(Lobby {
        room_epoch: "snapshot-room".into(),
        round_id: 1,
        phase: match name {
            "room-pause" | "room-respawn" | "room-reconnecting" | "room-nerd-stats"
            | "room-settings" => RoomPhase::Playing,
            "room-results" => RoomPhase::Results,
            _ => RoomPhase::Lobby,
        },
        host_id: if name == "room-lobby-guest" {
            "friend"
        } else {
            "viewer"
        }
        .into(),
        scoreboard: players.clone(),
        players,
        settings: RoomSettings::default(),
    });
    match name {
        "room-joining" => {
            menu.joining = true;
            menu.status = PREPARING_STATUS.into();
        }
        "room-reconnecting" => {
            menu.link = Link::Connecting(ConnectionDialog {
                title: "RECONNECTING",
                message: room_text::connecting_message(
                    "Connecting to the room…",
                    menu.lobby.as_ref(),
                ),
            })
        }
        "room-connection-ended" => {
            menu.link = Link::Ended(room_text::ending(
                Some(EndCause::Lost),
                "The connection to the room was lost. Try joining again.".into(),
                Shell::Desktop,
            ))
        }
        "room-pause" => menu.menu_open = true,
        "room-results" => {
            menu.outcome = Some(Match {
                phase: MatchPhase::Results,
                time: 0.0,
                scores: [42, 37],
                overtime: false,
                ended_early: None,
                winner: Some(Team::Blue),
                round: 1,
            })
        }
        "room-lobby-rules" => menu.editing_rules = true,
        "room-lobby-choice" => menu.editing_choice = true,
        "room-controls" => menu.controls_open = true,
        "room-respawn" => menu.death_cause = "Crank destroyed you with a rocket blast.".into(),
        _ => {}
    }
    if let Some(lobby) = &menu.lobby {
        // The viewer plays on blue.
        let outcome = menu.outcome.clone().map(|state| (state, Team::Blue));
        menu.text = room_text::menu_text(lobby, &menu.player_id, outcome.as_ref(), None);
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;

    struct Screen {
        context: egui::Context,
        menu: RoomMenu,
        state: UiState,
        prefs: Preferences,
        time: f64,
    }

    fn text_center(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_center(shape, label)),
            _ => None,
        }
    }

    impl Screen {
        fn new(scenario: &str) -> Self {
            let context = egui::Context::default();
            super::super::theme::install(&context);
            let mut screen = Self {
                context,
                menu: snapshot_menu(scenario),
                state: UiState::default(),
                prefs: Preferences::default(),
                time: 0.0,
            };
            for _ in 0..6 {
                screen.frame(Vec::new());
            }
            screen
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> (Vec<Action>, egui::FullOutput) {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let view = View {
                in_round: true,
                phase: hud_phase(&self.menu),
                prefs: &self.prefs,
                build: "test",
                debug: false,
                room: true,
            };
            let mut actions = Vec::new();
            let mut output = self.context.run_pass(input, |context| {
                actions = draw(context, &mut self.menu, &mut self.state, &view, 0.0);
            });
            // These interaction tests inspect shapes without uploading textures.
            output.textures_delta.clear();
            (actions, output)
        }

        /// Where a frame draws `label`.
        fn menu_text_center(&mut self, label: &str) -> egui::Pos2 {
            let (_, output) = self.frame(Vec::new());
            output
                .shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
                .unwrap_or_else(|| panic!("{label} is visible"))
        }

        /// Press and let go at `point`; the actions of both frames.
        fn tap(&mut self, point: egui::Pos2) -> Vec<Action> {
            let (mut actions, _) = self.frame(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ]);
            actions.extend(
                self.frame(vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                }])
                .0,
            );
            actions
        }

        fn click(&mut self, label: &str) -> Vec<Action> {
            let (_, output) = self.frame(Vec::new());
            let point = output
                .shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
                .unwrap_or_else(|| panic!("{label} is visible"));
            self.frame(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ]);
            self.frame(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }])
            .0
        }
    }

    #[test]
    fn a_join_says_the_arena_is_preparing_until_the_room_shows() {
        let mut menu = RoomMenu::new("KQ7M2XHP", false);
        assert_eq!(menu.status, CONNECTING_STATUS);
        let dialog = ConnectionDialog {
            title: "CONNECTING",
            message: "Connecting to the room…".into(),
        };
        menu.status("Connecting to the room…", Some(dialog));
        assert_eq!(menu.status, "Connecting to the room…");
        // The seat, the arena's compile and its end all read as the arena preparing.
        for text in ["Connected", "Preparing the arena…", "Connected"] {
            menu.status(text, None);
            assert_eq!(menu.status, PREPARING_STATUS, "after {text}");
            assert_eq!(menu.link, Link::Live);
        }
        menu.joining = false;
        menu.status("Connected", None);
        assert_eq!(menu.status, "Connected");
    }

    #[test]
    fn the_joining_card_shows_on_its_first_frame() {
        let mut screen = Screen::new("room-joining");
        screen.context = egui::Context::default();
        super::super::theme::install(&screen.context);
        let (_, output) = screen.frame(Vec::new());
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, "CANCEL").is_some()),
            "the first frame draws the card"
        );
        assert_eq!(screen.click("CANCEL"), [Action::RoomLeave]);
    }

    #[test]
    fn a_short_lobby_uses_its_content_height() {
        let screen = Screen::new("room-lobby");
        let rect = screen
            .context
            .memory(|memory| memory.area_rect(Id::unique("room")))
            .expect("the room plate");
        assert!(
            rect.height() < 700.0,
            "a short lobby must not fill the window: {rect:?}"
        );
        assert!((rect.width() - 560.0).abs() < 1.0);
        assert!((rect.center().y - 400.0).abs() < 1.0);
    }

    #[test]
    fn only_the_host_can_start_from_the_lobby() {
        let mut host = Screen::new("room-lobby");
        assert_eq!(host.click("START BATTLE"), [Action::RoomStart]);
        let mut guest = Screen::new("room-lobby-guest");
        let (_, output) = guest.frame(Vec::new());
        assert!(
            !output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, "START BATTLE").is_some())
        );
        assert_eq!(guest.click("BATTLE SETUP"), [Action::RoomLeave]);
    }

    #[test]
    fn room_controls_and_rules_only_change_their_own_panels() {
        let mut screen = Screen::new("room-lobby");
        assert!(screen.click("Change rules").is_empty());
        assert!(screen.menu.editing_rules);
        assert!(screen.click("Done").is_empty());
        assert!(!screen.menu.editing_rules);
        assert!(screen.click("Controls").is_empty());
        assert!(screen.menu.controls_open);
    }

    #[test]
    fn done_applies_a_round_length_still_being_typed() {
        let mut screen = Screen::new("room-lobby");
        screen.click("Change rules");
        let minutes = screen.menu_text_center("20");
        screen.tap(minutes);
        let mut typing = vec![egui::Event::Key {
            key: egui::Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }];
        for _ in 0..2 {
            typing.push(egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        }
        typing.push(egui::Event::Text("15".into()));
        screen.frame(typing);
        // Straight to Done, whose frames may or may not still show the field.
        let done = screen.menu_text_center("Done");
        let actions = screen.tap(done);
        let lengths: Vec<u32> = actions
            .iter()
            .filter_map(|action| match action {
                Action::RoomRules(rules) => Some(rules.round_minutes),
                _ => None,
            })
            .collect();
        assert_eq!(lengths, [15], "{actions:?}");
        assert!(!screen.menu.editing_rules);
    }

    #[test]
    fn room_pause_and_results_keep_their_distinct_actions() {
        let mut pause = Screen::new("room-pause");
        assert_eq!(pause.click("RESUME"), [Action::Resume]);
        assert_eq!(pause.click("END BATTLE"), [Action::RoomEnd]);
        assert_eq!(pause.click("LEAVE ROOM"), [Action::RoomLeave]);
        let mut results = Screen::new("room-results");
        assert_eq!(results.click("PLAY AGAIN"), [Action::RoomStart]);
    }

    #[test]
    fn room_settings_replaces_the_underlying_menu() {
        let mut screen = Screen::new("room-pause");
        assert_eq!(screen.click("Settings"), [Action::OpenSettings]);
        screen.state.settings_draft = Some(screen.prefs.clone());
        for _ in 0..3 {
            screen.frame(Vec::new());
        }
        let (_, output) = screen.frame(Vec::new());
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, "SETTINGS").is_some())
        );
        assert!(
            !output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, "RESUME").is_some())
        );
        assert!(
            !output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, "BATTLE IN PROGRESS").is_some())
        );
        assert_eq!(screen.click("CANCEL"), []);
        assert!(screen.state.settings_draft.is_none());
    }

    #[test]
    fn reconnect_actions_keep_the_connection_reason() {
        let mut screen = Screen::new("room-connection-ended");
        assert_eq!(screen.click("TRY AGAIN"), [Action::RoomRejoin]);
        assert_eq!(
            screen.click("BATTLE SETUP"),
            [Action::RoomSetup(
                "The connection to the room was lost. Try joining again.".into()
            )]
        );
    }
}
