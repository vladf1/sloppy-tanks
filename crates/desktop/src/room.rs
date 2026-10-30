//! A multiplayer room in the window (`src/net/client.ts`): the room's `NetGame` owns the
//! window's renderer while it plays, its sockets run on threads (`sockets`), and this
//! adapter performs the engine's socket and storage work, shows its notices and runs one
//! engine frame per redraw. Leaving hands the window back to single player through
//! [`Shell`].

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
use std::sync::Arc;

use sloppy_core::net::client::{ClientAction, POLL_MS, SavedSeat};
use sloppy_core::net::protocol::RoomPhase;
use sloppy_core::sim::Team;
use sloppy_core::sim::map_options::map_option_for;
use sloppy_web::hud::Hud;
use sloppy_web::room_notices::RoomNotice;
use sloppy_web::{NetConfig, NetGame, net_frame_slot as slot};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::app::{
    Shell, css_size, engine_error, now_ms, render_target, set_fullscreen, view_config,
};
use crate::audio::Audio;
use crate::automation::{self, Command, Extra};
use crate::bakes::{self, Bake};
use crate::input::Shortcut;
use crate::overlay::Overlay;
use crate::preferences::Preferences;
use crate::room_api::RoomSelection;
use crate::sockets::{SocketEvent, Sockets};
use crate::touch::Play;
use crate::ui::room_menu::{self, RoomMenu};
use crate::ui::{self, Action, UiState};
use crate::window_input::{Battle, Request, WindowInput};

/// Pipelines created per preparation step; the joining card redraws between steps.
const PREPARE_BUDGET: u32 = 4;
/// Why a room that could not load sends the player back to Battle Setup.
const ARENA_FAILED: &str = "The arena could not load. Try again, or play single player.";

/// Seats the server issued, by server and room: natively for this run, in a page in its
/// `sessionStorage` under the old page's keys, so that a reload, which returns to Battle
/// Setup with the room selected, joins it again on the same seat.
#[derive(Default)]
pub struct SavedSeats {
    #[cfg(not(target_arch = "wasm32"))]
    seats: HashMap<String, SavedSeat>,
}

impl SavedSeats {
    pub fn get(&self, selection: &RoomSelection) -> Option<SavedSeat> {
        let key = seat_key(selection);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.seats.get(&key).cloned()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let text = session_storage()?.get_item(&key).ok()??;
            serde_json::from_str(&text).ok()
        }
    }

    /// Keep `seat`; a page without storage keeps it for no reload.
    fn save(&mut self, selection: &RoomSelection, seat: SavedSeat) {
        let key = seat_key(selection);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.seats.insert(key, seat);
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = session_storage() {
            let text = serde_json::json!({"token": seat.token, "roomEpoch": seat.room_epoch});
            let _ = storage.set_item(&key, &text.to_string());
        }
    }

    fn forget(&mut self, selection: &RoomSelection) {
        let key = seat_key(selection);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.seats.remove(&key);
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = session_storage() {
            let _ = storage.remove_item(&key);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn session_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}

/// The development network parameters (`?latency=&jitter=&stall=`, `?dodge=0`,
/// `?shooter=0`) every room a run joins takes, as `NetConfig` reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkTools {
    pub latency: Option<String>,
    pub jitter: Option<String>,
    pub stall: Option<String>,
    pub dodge: Option<String>,
    pub shooter: Option<String>,
}

pub struct RoomSession {
    pub window: Arc<Window>,
    game: NetGame,
    overlay: Overlay,
    sockets: Sockets,
    pub input: WindowInput,
    prefs: Preferences,
    audio: Option<Audio>,
    ui: UiState,
    menu: RoomMenu,
    selection: RoomSelection,
    seats: SavedSeats,
    debug: bool,
    build: String,
    /// The arena compiles between frames (`prepare` until done).
    preparing: bool,
    /// What preparation left for later (the wrecks) builds a step a frame.
    prebuilding: bool,
    bake: Option<Bake>,
    first_person: bool,
    /// The engine says the pointer is free: a menu, a drop or no battle.
    pointer_free: bool,
    /// The window took the last frame; a covered one takes none (see `app.rs`).
    frame_shown: bool,
    /// macOS reports the window covered, or it has no area (minimized on Windows).
    occluded: bool,
    minimized: bool,
    /// What the room was last told: hidden or shown.
    told_hidden: bool,
    /// Inside `pump`, which a notice can reach again.
    pumping: bool,
    /// `quit` ran.
    quitting: bool,
    /// Settings opened during the battle put the tank in a bot's hands until they close.
    resume_after_settings: bool,
    last_poll: f64,
    last_frame: f64,
    hud_dt: f64,
    /// The renderer stopped; the room is given up.
    stopped: bool,
    /// Battle Setup is wanted, with this reason to show there.
    pub exit: Option<String>,
}

impl RoomSession {
    /// Join `selection`'s room on the window `shell` holds, with `network`'s tools. The
    /// window's surface must be free (single player's renderer dropped). On failure the
    /// shell comes back with the reason to show on Battle Setup.
    pub async fn new(
        shell: Shell,
        selection: RoomSelection,
        network: NetworkTools,
    ) -> Result<Self, Box<(Shell, String)>> {
        let window = shell.window.clone();
        let config = NetConfig {
            server: selection.server.clone(),
            room: selection.room.clone(),
            saved_seat: shell.seats.get(&selection),
            latency: network.latency,
            jitter: network.jitter,
            stall: network.stall,
            dodge: network.dodge,
            shooter: network.shooter,
        };
        let view = view_config(&window, &shell.prefs);
        let created = match render_target(&window) {
            Ok(target) => NetGame::create_with(target, view, config)
                .await
                .map_err(engine_error),
            Err(error) => Err(error),
        };
        let mut game = match created {
            Ok(game) => game,
            Err(error) => {
                report!("The room's renderer could not start: {error}");
                return Err(Box::new((shell, ARENA_FAILED.into())));
            }
        };
        let overlay = match Overlay::new(window.clone(), game.renderer()) {
            Ok(overlay) => overlay,
            Err(error) => {
                report!("The room's menus could not draw: {error}");
                return Err(Box::new((shell, ARENA_FAILED.into())));
            }
        };
        let now = now_ms();
        game.join(selection.choice.clone(), now);
        let Shell {
            prefs,
            audio,
            debug,
            build,
            rooms,
            seats,
            ..
        } = shell;
        let mut session = Self {
            window,
            game,
            overlay,
            sockets: Sockets::default(),
            input: WindowInput::default(),
            first_person: prefs.first_person,
            prefs,
            audio,
            ui: UiState {
                rooms,
                ..UiState::default()
            },
            menu: RoomMenu::new(&selection.room, selection.choice.create.is_some()),
            selection,
            seats,
            debug,
            build,
            preparing: false,
            prebuilding: false,
            bake: None,
            pointer_free: true,
            frame_shown: true,
            occluded: false,
            minimized: false,
            told_hidden: false,
            pumping: false,
            quitting: false,
            resume_after_settings: false,
            last_poll: now,
            last_frame: now,
            hud_dt: 0.0,
            stopped: false,
            exit: None,
        };
        if session.audio.is_none() {
            session.audio = Audio::new();
        }
        if let Some(audio) = &mut session.audio {
            audio.set_volume(session.prefs.volume);
        }
        session.pump();
        Ok(session)
    }

    /// Whether the window shows nothing: covered, minimized, or taking no frames.
    pub fn hidden(&self) -> bool {
        self.occluded || self.minimized || !self.frame_shown
    }

    /// Tell the room once whenever the window hides or shows, as a page's
    /// visibility does; a hidden window lets the cursor go.
    fn hidden_changed(&mut self) {
        let hidden = self.hidden();
        if hidden != self.told_hidden {
            self.told_hidden = hidden;
            self.input.controls.clear();
            self.game.set_hidden(hidden, now_ms());
            self.pump();
        }
        self.hold_pointer();
    }

    /// Lock the cursor while first person steers the battle.
    fn hold_pointer(&mut self) {
        let wanted = self.first_person
            && !self.pointer_free
            && self.menu.in_battle()
            && self.ui.settings_draft.is_none()
            && self.window.has_focus()
            && !self.occluded
            && !self.minimized;
        self.input.hold_pointer(&self.window, wanted);
    }

    /// Give the window back: the room's renderer and overlay go, the rest stays.
    pub fn into_shell(self) -> Shell {
        let Self {
            window,
            prefs,
            audio,
            debug,
            build,
            ui,
            seats,
            ..
        } = self;
        Shell {
            window,
            prefs,
            audio,
            debug,
            build,
            rooms: ui.rooms,
            seats,
            frame_log: false,
            autoplay: false,
            seed: None,
        }
    }

    /// The room this session plays.
    pub fn room(&self) -> &str {
        &self.selection.room
    }

    /// Socket events, the engine's timers and the work they cause. Runs every redraw
    /// and between them, so a hidden window keeps its seat.
    pub fn network(&mut self) {
        let now = now_ms();
        while let Some(event) = self.sockets.next_event() {
            match event {
                SocketEvent::Opened(id) => self.game.socket_opened(id, now),
                SocketEvent::Text(id, text) => self.game.socket_message(id, &text, now),
                SocketEvent::Binary(id, bytes) => self.game.socket_binary(id, &bytes, now),
                SocketEvent::Closed(id, code) => self.game.socket_closed(id, code, now),
            }
            self.pump();
        }
        if now - self.last_poll >= POLL_MS {
            self.last_poll = now;
            self.game.poll(now);
            self.pump();
        }
    }

    /// Perform the engine's socket and storage work, then its notices, until both are
    /// empty; handling a notice may call back into the engine.
    fn pump(&mut self) {
        // A notice handled here can pump again (leaving to Battle Setup); the outer
        // loop takes what that would have, in order, as the page's `pumping` does.
        if self.pumping {
            return;
        }
        self.pumping = true;
        loop {
            let actions = self.game.take_room_actions();
            for action in &actions {
                self.perform(action);
            }
            let notices = self.game.take_room_notices();
            let done = actions.is_empty() && notices.is_empty();
            for notice in notices {
                self.handle(notice);
            }
            if done {
                break;
            }
        }
        self.pumping = false;
    }

    /// The app quits: leave the room and give the sockets a moment to say so, rather
    /// than leave the seat held until the server times it out.
    pub fn quit(&mut self) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        self.game.stop();
        self.pump();
        self.sockets.close_all_and_wait();
    }

    fn perform(&mut self, action: &ClientAction) {
        match action {
            ClientAction::OpenSocket { socket, url } => self.sockets.open(*socket, url.clone()),
            ClientAction::Send { socket, text } => self.sockets.send(*socket, text.clone()),
            ClientAction::CloseSocket { socket } => self.sockets.close(*socket),
            ClientAction::SaveSeat { token, room_epoch } => {
                let seat = SavedSeat {
                    token: token.clone(),
                    room_epoch: room_epoch.clone(),
                };
                self.seats.save(&self.selection, seat);
            }
            ClientAction::ForgetSeat => self.seats.forget(&self.selection),
        }
    }

    fn handle(&mut self, notice: RoomNotice) {
        match notice {
            RoomNotice::Status { text, dialog } => self.menu.status(&text, dialog),
            RoomNotice::Notice(text) => self.menu.notice = text,
            RoomNotice::Ended(ending) => {
                self.input.controls.clear();
                // A join still behind the joining card reports on Battle Setup.
                if self.menu.joining {
                    self.leave_to_setup(ending.text);
                } else {
                    self.menu.ended(ending);
                }
            }
            RoomNotice::Lobby {
                lobby,
                player_id,
                menu,
                arrivals,
            } => {
                self.menu
                    .lobby(*lobby, player_id, menu, arrivals, &mut self.ui);
            }
            RoomNotice::Result(outcome) => self.menu.outcome = Some(outcome),
            RoomNotice::ResetFeedback => {
                self.ui.reset_feedback();
                self.menu.death_cause.clear();
            }
            RoomNotice::ClearInput => self.input.controls.clear(),
            RoomNotice::Reveal => {
                self.menu.joining = false;
                self.resize();
            }
            RoomNotice::Prepare => self.preparing = true,
            RoomNotice::ArenaFailed(error) => {
                report!("Multiplayer graphics failed: {error}");
                self.leave_to_setup(ARENA_FAILED.into());
            }
            RoomNotice::BaselineShown => {}
        }
    }

    /// Stop the room's connection and ask for Battle Setup with `notice`.
    fn leave_to_setup(&mut self, notice: String) {
        self.game.stop();
        self.pump();
        self.exit = Some(notice);
    }

    /// Handle a window event; true when the app should quit.
    pub fn window_event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::RedrawRequested = event {
            return self.redraw();
        }
        let battle = self.battle();
        let request = self
            .input
            .event(&self.window, &mut self.overlay, event, &battle);
        if let Some(request) = request
            && self.request(request)
        {
            return true;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.quit();
                return true;
            }
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                self.hidden_changed();
                self.resize();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.resize(),
            WindowEvent::Focused(false) => {
                self.input.controls.clear();
                self.hold_pointer();
            }
            WindowEvent::Occluded(hidden) => {
                self.occluded = *hidden;
                self.hidden_changed();
            }
            _ => {}
        }
        false
    }

    /// Where the battle stands, for the window's input. Whether the player can act is
    /// the engine's to say (`activeInput`, as the page asks it).
    fn battle(&self) -> Battle {
        let in_battle = self.menu.in_battle() && !self.menu.joining;
        Battle {
            keys: in_battle && self.ui.settings_draft.is_none(),
            playing: self.game.active_input(),
            wheel: in_battle,
            touch: Play::live(
                &self.overlay.context,
                self.touch_live(),
                self.first_person,
                self.ui.hud.as_ref(),
            ),
        }
    }

    /// Whether the touch controls show: the player can act, with no dialog open.
    fn touch_live(&self) -> bool {
        self.game.active_input() && self.ui.settings_draft.is_none()
    }

    /// Do what input asked; true when the app should quit.
    fn request(&mut self, request: Request) -> bool {
        match request {
            Request::Quit => {
                self.quit();
                true
            }
            Request::Shortcut(shortcut) => {
                self.shortcut(shortcut);
                false
            }
        }
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::Escape => {
                if self.ui.settings_draft.is_some() {
                    self.ui.settings_draft = None;
                } else if self.menu.in_battle() {
                    self.apply(Action::Pause);
                } else if self.menu.phase() == RoomPhase::Playing && self.menu.menu_open {
                    self.apply(Action::Resume);
                }
            }
            Shortcut::ToggleView => {
                self.apply(Action::ToggleView);
            }
            Shortcut::NerdStats => {
                if self.debug || self.prefs.debug {
                    self.ui.show_nerd_stats = !self.ui.show_nerd_stats;
                }
            }
            Shortcut::Fullscreen => {
                self.apply(Action::ToggleFullscreen);
            }
        }
    }

    pub fn resize(&mut self) {
        let (width, height, scale) = css_size(&self.window);
        self.game.resize(width, height, scale);
    }

    fn save_camera(&mut self) {
        self.prefs.save_camera(self.game.camera());
    }

    fn apply(&mut self, action: Action) -> bool {
        let now = now_ms();
        match action {
            Action::Pause => {
                if self.menu.in_battle() && self.game.pause(now) {
                    self.menu.menu_open = true;
                    self.menu.notice.clear();
                    self.input.controls.clear();
                }
            }
            Action::Resume => {
                self.input.controls.clear();
                self.menu.menu_open = self.game.resume(now);
            }
            Action::RoomStart => {
                self.input.controls.clear();
                self.game.start(now);
            }
            Action::RoomEnd => {
                self.input.controls.clear();
                self.game.end(now);
            }
            Action::RoomLeave => {
                self.game.leave(now);
                self.pump();
                self.leave_to_setup(String::new());
            }
            // The engine's next status puts up RECONNECTING.
            Action::RoomRejoin => self.game.rejoin(now),
            Action::RoomSetup(notice) => self.leave_to_setup(notice),
            // The address still names the room: the new page opens Battle Setup with it.
            Action::Reload => crate::links::reload(),
            Action::RoomChoose { team, kind } => match self.game.choose_seat(team, kind, now) {
                Ok(()) => {
                    self.prefs.tank = kind;
                    self.prefs.save();
                }
                Err(error) => report!("Invalid room choice: {}", engine_error(error)),
            },
            Action::RoomRules(rules) => self.game.set_rules(rules, now),
            Action::SelectAmmo(slot) => {
                if self.menu.in_battle() {
                    self.input.controls.select_ammo(slot);
                }
            }
            Action::ToggleView => {
                if self.menu.in_battle() {
                    self.first_person = self.game.toggle_first_person();
                    self.save_camera();
                }
            }
            Action::ToggleFullscreen => {
                let fullscreen = self.window.fullscreen().is_none();
                set_fullscreen(&self.window, &mut self.prefs, fullscreen);
            }
            Action::OpenSettings => {
                if self.menu.in_battle() {
                    self.apply(Action::Pause);
                    self.resume_after_settings = self.menu.menu_open;
                }
                ui::menus::open_settings(&mut self.ui, &self.prefs);
            }
            Action::SettingsSaved(prefs) => self.save_settings(*prefs),
            Action::Quit => {
                self.game.stop();
                self.pump();
                return true;
            }
            // Single player's choices; a room decides them between battles.
            Action::Start
            | Action::EndBattle
            | Action::BattleSetup
            | Action::NextTank(_)
            | Action::JoinRoom(_) => {}
        }
        self.pump();
        false
    }

    /// Apply and keep changed settings; the speeds are the room's, not the player's.
    fn save_settings(&mut self, prefs: Preferences) {
        let old = std::mem::replace(&mut self.prefs, prefs);
        if old.graphics != self.prefs.graphics {
            self.game.set_graphics_setting(self.prefs.graphics);
        }
        if let Some(audio) = &mut self.audio {
            audio.set_volume(self.prefs.volume);
        }
        if old.fullscreen != self.prefs.fullscreen {
            let fullscreen = self.prefs.fullscreen;
            set_fullscreen(&self.window, &mut self.prefs, fullscreen);
        }
        self.prefs.save();
    }

    /// The engine's HUD with what a room's score bar needs besides: a room plays a team
    /// battle on the lobby's map and difficulty.
    fn refresh_hud(&mut self) {
        let Some(room) = self.game.hud() else {
            return;
        };
        let viewer = room.human.id;
        let human_team = room
            .scoreboard
            .row(viewer)
            .map(|row| row.team)
            .or(self.menu.team())
            .unwrap_or(Team::Blue);
        // The default HUD is a team battle with the standard score limit and team names.
        let mut hud = Hud {
            match_state: room.match_state.clone(),
            human_team,
            human: room.human,
            scoreboard: room.scoreboard.into_owned(),
            score_pill: room.score_pill,
            ..Hud::default()
        };
        if let Some(lobby) = &self.menu.lobby {
            hud.map_mode = lobby.settings.map_mode;
            hud.map_name = map_option_for(lobby.settings.map_mode).name.to_owned();
            hud.difficulty = lobby.settings.difficulty;
        }
        self.ui.tick(self.hud_dt);
        self.hud_dt = 0.0;
        self.ui.hud = Some(hud);
    }

    /// One step of compiling the arena the room asked for, between frames.
    fn prepare(&mut self, now: f64) {
        if self.bake.is_none()
            && let Some(key) = self.game.claim_texture_bake()
        {
            self.bake = bakes::start(key.clone());
            if self.bake.is_none() {
                self.game.release_texture_bake(&key);
            }
        }
        if let Some(bake) = &self.bake
            && let Some(baked) = bake.finished()
        {
            let key = bake.key.clone();
            self.bake = None;
            match baked {
                Ok(pixels) => {
                    if let Err(error) = self.game.supply_pixels(&key, &pixels) {
                        report!("{}", engine_error(error));
                    }
                }
                Err(_) => self.game.release_texture_bake(&key),
            }
        }
        let status = self.game.prepare(PREPARE_BUDGET, now);
        if status.ready {
            self.preparing = false;
            self.prebuilding = true;
        } else if self.menu.joining && (status.compiled > 0 || status.remaining > 0) {
            self.menu.status = "Preparing the arena…".into();
        }
        self.pump();
    }

    /// One engine frame: input in, the drawn scene, its events and HUD out.
    fn frame(&mut self, now: f64) -> Result<(), String> {
        let input = self.input.take_input(self.first_person);
        let camera_input = input.zoom != 0.0 || input.toggle_view;
        let result = self.game.frame(now, &input.pack()).map_err(engine_error)?;
        if let Some(error) = self.game.error() {
            return Err(error);
        }
        if camera_input {
            self.save_camera();
        }
        self.pump();
        let value = |index: usize| result.get(index).copied().unwrap_or(0.0);
        self.first_person = value(slot::FIRST_PERSON) == 1.0;
        self.pointer_free = value(slot::POINTER_FREE) == 1.0;
        if value(slot::DRAWN) == 1.0 {
            self.ui.cockpit = value(slot::COCKPIT) == 1.0;
            self.ui.hull_angle = f64::from(value(slot::HULL_ANGLE));
            let viewer = (value(slot::VIEWER_X), value(slot::VIEWER_Y));
            self.ui.viewer = (viewer.0.is_finite() && viewer.1.is_finite()).then_some(viewer);
            self.hud_dt += f64::from(value(slot::DT));
            let hud_due = value(slot::HUD_DUE) == 1.0;
            if hud_due {
                self.refresh_hud();
            }
            if value(slot::EVENTS) > 0.0 {
                let batch = self.game.take_events();
                if let Some(audio) = &mut self.audio {
                    audio.play(&batch);
                }
                for event in &batch.events {
                    self.ui.event(event);
                    self.menu.event(event);
                }
            }
        } else if value(slot::EVENTS) > 0.0 {
            self.game.take_events();
        }
        Ok(())
    }

    /// The renderer stopped: give up the connection, and the engine says so.
    fn fail(&mut self, error: &str) {
        report!("The room stopped: {error}");
        crate::page::failed(&format!("The renderer stopped: {error}"));
        self.stopped = true;
        self.input.controls.clear();
        self.game.renderer_failed();
        self.pump();
    }

    /// What a check reads of this frame (`automation.rs`).
    fn automation_state(&mut self, now: f64) -> automation::State {
        let menu = &self.menu;
        automation::State {
            engine: "room",
            in_round: true,
            phase: automation::phase_name(room_menu::hud_phase(menu)),
            alive: self.ui.hud.as_ref().is_none_or(|hud| hud.human.alive),
            first_person: self.first_person,
            settings: self.ui.settings_draft.is_some(),
            touch_controls: self.ui.touch.is_some(),
            touch_screen: crate::touch::screen(),
            failure: self.stopped.then(|| "The renderer stopped.".into()),
            error: self.game.error(),
            prefs: serde_json::to_value(&self.prefs).unwrap_or_default(),
            room: Some(serde_json::json!({
                "code": menu.room,
                "phase": format!("{:?}", menu.phase()),
                "link": format!("{:?}", menu.link),
                "menuOpen": menu.menu_open,
                "joining": menu.joining,
                "status": menu.status,
                "players": menu.lobby.as_ref().map(|lobby| lobby.players.len()),
                "polledAt": self.last_poll,
                "now": now,
            })),
            ..automation::State::default()
        }
    }

    /// Run a check's debug command: a room's engine takes only the network tools.
    fn command(&mut self, command: &Command) -> Result<serde_json::Value, String> {
        match *command {
            Command::Latency {
                latency,
                jitter,
                stall,
            } => {
                self.game.set_transport_delay(latency, jitter, stall);
                Ok(serde_json::Value::Null)
            }
            ref other => Err(format!("a room cannot run {other:?}")),
        }
    }

    /// Run a frame and show it; true when the player chose to quit.
    fn redraw(&mut self) -> bool {
        let now = automation::now();
        let frame_dt = ((now - self.last_frame) / 1000.0).clamp(0.0, 0.1);
        self.last_frame = now;
        for pending in automation::take_commands() {
            let answer = self.command(&pending.command);
            pending.answer(answer);
        }
        let playing = self.battle().playing;
        let mut quit = false;
        for request in self
            .input
            .hardware_keys(&self.window, &self.overlay, playing)
        {
            quit |= self.request(request);
        }
        if !self.stopped {
            self.network();
            if self.preparing {
                self.prepare(now);
            } else if self.prebuilding {
                self.prebuilding = self.game.prebuild_step();
            }
            // A window that takes no frames draws none, as a hidden page does; the
            // heartbeat above runs on.
            if !self.hidden()
                && let Err(error) = self.frame(now)
            {
                self.fail(&error);
            }
        }
        if !self.menu.in_battle() {
            self.ui.tick(frame_dt);
        }
        if self.resume_after_settings && self.ui.settings_draft.is_none() {
            self.resume_after_settings = false;
            if self.menu.menu_open {
                self.apply(Action::Resume);
            }
        }
        if self.ui.stats_due(now, self.debug || self.prefs.debug) {
            self.ui.stats = Some(self.game.nerd_stats(now));
        }
        self.hold_pointer();
        self.ui.touch = self.input.touch_view(self.touch_live(), self.first_person);
        let view = ui::View {
            in_round: true,
            phase: room_menu::hud_phase(&self.menu),
            prefs: &self.prefs,
            build: &self.build,
            debug: self.debug || self.prefs.debug,
            room: true,
        };
        let menu = &mut self.menu;
        let ui_state = &mut self.ui;
        let mut actions = Vec::new();
        let painted = self.overlay.run(|context| {
            actions = room_menu::draw(context, menu, ui_state, &view, frame_dt);
        });
        self.overlay.present(self.game.renderer(), painted);
        self.frame_shown = self.game.renderer().frame_shown();
        if self.frame_shown {
            crate::page::frame_presented();
            if automation::enabled() {
                let state = self.automation_state(now);
                let game = &mut self.game;
                automation::presented(state, |extra| {
                    let json = match extra {
                        Extra::Hud => game.hud_json(),
                        Extra::Stats => game.stats_json(now),
                        Extra::Nerd => game.nerd_stats_json(now),
                        Extra::Debug | Extra::Snapshot => game.debug_json(),
                    };
                    serde_json::from_str(&json).unwrap_or_default()
                });
            }
        }
        self.hidden_changed();
        for action in actions {
            quit |= self.apply(action);
        }
        quit
    }
}

/// Where a seat is kept: one per server and room, as the old page keyed `sessionStorage`.
fn seat_key(selection: &RoomSelection) -> String {
    format!("sloppy-seat:{}:{}", selection.server, selection.room)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(server: &str, room: &str) -> RoomSelection {
        RoomSelection {
            room: room.into(),
            server: server.into(),
            choice: sloppy_core::net::protocol::JoinChoice {
                name: "Driver".into(),
                kind: sloppy_core::sim::VehicleKind::Balanced,
                team: None,
                create: None,
                existing_room: Some(true),
            },
        }
    }

    /// A seat is the server's and the room's: the same code on another server is another
    /// room. The key is the old page's, so a tab that reloads into this page keeps it.
    #[test]
    fn a_seat_is_kept_per_server_and_room() {
        let mut seats = SavedSeats::default();
        let room = selection("wss://one.example", "ABCD2345");
        assert_eq!(seat_key(&room), "sloppy-seat:wss://one.example:ABCD2345");
        let seat = SavedSeat {
            token: "t".repeat(16),
            room_epoch: "e1".into(),
        };
        seats.save(&room, seat.clone());
        assert_eq!(seats.get(&room), Some(seat));
        assert_eq!(seats.get(&selection("wss://two.example", "ABCD2345")), None);
        seats.forget(&room);
        assert_eq!(seats.get(&room), None);
    }
}
