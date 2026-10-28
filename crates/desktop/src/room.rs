//! A multiplayer room in the window (`src/net/client.ts`): the room's `NetGame` owns the
//! window's renderer while it plays, its sockets run on threads (`sockets`), and this
//! adapter performs the engine's socket and storage work, shows its notices and runs one
//! engine frame per redraw. Leaving hands the window back to single player through
//! [`Shell`].

use std::collections::HashMap;
use std::sync::Arc;

use sloppy_core::net::client::{ClientAction, POLL_MS, SavedSeat};
use sloppy_core::net::protocol::RoomPhase;
use sloppy_core::sim::data::{SCORE_LIMIT, TEAM_NAMES};
use sloppy_core::sim::map_options::map_option_for;
use sloppy_core::sim::simulation::SpeedTuning;
use sloppy_core::sim::{GameMode, Team};
use sloppy_render::gpu::RenderTarget;
use sloppy_render::wgpu;
use sloppy_web::hud::Hud;
use sloppy_web::room_notices::RoomNotice;
use sloppy_web::{NetConfig, NetGame, net_frame_slot as slot};
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window};

use crate::app::{Shell, grab_pointer, now_ms, view_config};
use crate::audio::Audio;
use crate::bakes::{self, Bake};
use crate::input::{Controls, Shortcut};
use crate::overlay::Overlay;
use crate::preferences::Preferences;
use crate::room_api::RoomSelection;
use crate::sockets::{SocketEvent, Sockets};
use crate::ui::room_menu::{self, RoomMenu};
use crate::ui::{self, Action, UiState};

/// Pipelines created per preparation step; the joining card redraws between steps.
const PREPARE_BUDGET: u32 = 4;
/// Frames between Stats for nerds readings.
const STATS_EVERY_FRAMES: u64 = 15;
/// Why a room that could not load sends the player back to Battle Setup.
const ARENA_FAILED: &str = "The arena could not load. Try again, or play single player.";

/// Seats the server issued, kept for this run by server and room (the page's
/// `sessionStorage`).
pub type SavedSeats = HashMap<String, SavedSeat>;

pub struct RoomSession {
    pub window: Arc<Window>,
    game: NetGame,
    overlay: Overlay,
    sockets: Sockets,
    pub controls: Controls,
    modifiers: ModifiersState,
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
    pub cursor_grabbed: bool,
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
    frame_index: u64,
    /// The renderer stopped; the room is given up.
    stopped: bool,
    /// Battle Setup is wanted, with this reason to show there.
    pub exit: Option<String>,
}

impl RoomSession {
    /// Join `selection`'s room on the window `shell` holds. The window's surface must be
    /// free (single player's renderer dropped). On failure the shell comes back with the
    /// reason to show on Battle Setup.
    pub fn new(shell: Shell, selection: RoomSelection) -> Result<Self, Box<(Shell, String)>> {
        let window = shell.window.clone();
        let size = window.inner_size();
        let config = NetConfig {
            server: selection.server.clone(),
            room: selection.room.clone(),
            saved_seat: shell.seats.get(&seat_key(&selection)).cloned(),
            ..NetConfig::default()
        };
        let target = RenderTarget {
            window: wgpu::SurfaceTarget::from(window.clone()),
            width: size.width,
            height: size.height,
        };
        let view = view_config(&window, &shell.prefs);
        let mut game = match pollster::block_on(NetGame::create_with(target, view, config)) {
            Ok(game) => game,
            Err(error) => {
                eprintln!("The room's renderer could not start: {error}");
                return Err(Box::new((shell, ARENA_FAILED.into())));
            }
        };
        let now = now_ms();
        game.join(selection.choice.clone(), now);
        let overlay = Overlay::new(window.clone(), game.renderer());
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
            controls: Controls::default(),
            modifiers: ModifiersState::default(),
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
            cursor_grabbed: false,
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
            frame_index: 0,
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
            self.controls.clear();
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
        grab_pointer(&self.window, wanted, &mut self.cursor_grabbed);
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
                self.seats.insert(
                    seat_key(&self.selection),
                    SavedSeat {
                        token: token.clone(),
                        room_epoch: room_epoch.clone(),
                    },
                );
            }
            ClientAction::ForgetSeat => {
                self.seats.remove(&seat_key(&self.selection));
            }
        }
    }

    fn handle(&mut self, notice: RoomNotice) {
        match notice {
            RoomNotice::Status { text, dialog } => self.menu.status(&text, dialog),
            RoomNotice::Notice(text) => self.menu.notice = text,
            RoomNotice::Ended(ending) => {
                self.controls.clear();
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
            RoomNotice::Result { outcome, team } => self.menu.outcome = Some((outcome, team)),
            RoomNotice::ResetFeedback => {
                self.ui.reset_feedback();
                self.menu.death_cause.clear();
            }
            RoomNotice::ClearInput => self.controls.clear(),
            RoomNotice::Reveal => {
                self.menu.joining = false;
                self.resize();
            }
            RoomNotice::Prepare => self.preparing = true,
            RoomNotice::ArenaFailed(error) => {
                eprintln!("Multiplayer graphics failed: {error}");
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
        let in_battle = self.menu.in_battle() && !self.menu.joining;
        let battle = in_battle && !self.menu.menu_open && self.ui.settings_draft.is_none();
        let keys_to_menus = !battle || self.overlay.typing();
        let consumed = self
            .overlay
            .event(event, !self.cursor_grabbed, keys_to_menus);
        let alive = self.ui.hud.as_ref().is_some_and(|hud| hud.human.alive);
        let playing = in_battle && alive;
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
                self.controls.clear();
                self.hold_pointer();
            }
            WindowEvent::Occluded(hidden) => {
                self.occluded = *hidden;
                self.hidden_changed();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                self.controls.set_command(
                    self.modifiers.super_key()
                        || self.modifiers.control_key()
                        || self.modifiers.alt_key(),
                );
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Typing goes to the field; a release still reaches the controls, so
                // a key held when a field took focus never sticks.
                if self.overlay.typing() && event.state == ElementState::Pressed {
                    return false;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::KeyQ && self.modifiers.super_key() {
                        self.quit();
                        return true;
                    }
                    let shortcut = self.controls.key(code, event.state, event.repeat, playing);
                    if let Some(shortcut) = shortcut {
                        self.shortcut(shortcut);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let size = self.window.inner_size();
                self.controls.aim_at(
                    (position.x, position.y),
                    (f64::from(size.width), f64::from(size.height)),
                );
            }
            WindowEvent::MouseInput { state, button, .. }
                if *state == ElementState::Released
                    || (playing
                        && (self.cursor_grabbed
                            || !(consumed || self.overlay.wants_pointer()))) =>
            {
                self.controls.button(*button, *state);
            }
            WindowEvent::MouseWheel { delta, .. } if !self.overlay.wants_pointer() && in_battle => {
                self.controls
                    .wheel(*delta, self.modifiers.shift_key(), playing);
            }
            _ => {}
        }
        false
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

    fn resize(&mut self) {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor();
        let logical = size.to_logical::<f64>(scale);
        self.game.resize(logical.width, logical.height, scale);
    }

    fn save_camera(&mut self) {
        let camera = self.game.camera();
        self.prefs.first_person = camera.first_person;
        self.prefs.zoom = Some(camera.zoom);
        self.prefs.save();
    }

    fn apply(&mut self, action: Action) -> bool {
        let now = now_ms();
        match action {
            Action::Pause => {
                if self.menu.in_battle() && self.game.pause(now) {
                    self.menu.menu_open = true;
                    self.menu.notice.clear();
                    self.controls.clear();
                }
            }
            Action::Resume => {
                self.controls.clear();
                self.menu.menu_open = self.game.resume(now);
            }
            Action::RoomStart => {
                self.controls.clear();
                self.game.start(now);
            }
            Action::RoomEnd => {
                self.controls.clear();
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
            Action::RoomChoose { team, kind } => match self.game.choose_seat(team, kind, now) {
                Ok(()) => {
                    self.prefs.tank = kind;
                    self.prefs.save();
                }
                Err(error) => eprintln!("Invalid room choice: {error}"),
            },
            Action::RoomRules(rules) => self.game.set_rules(rules, now),
            Action::SelectAmmo(slot) => {
                if self.menu.in_battle() {
                    self.controls.select_ammo(slot);
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
                self.window
                    .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
                self.prefs.fullscreen = fullscreen;
                self.prefs.save();
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
            self.game.set_graphics(self.prefs.graphics);
        }
        if let Some(audio) = &mut self.audio {
            audio.set_volume(self.prefs.volume);
        }
        if old.fullscreen != self.prefs.fullscreen {
            self.window.set_fullscreen(
                self.prefs
                    .fullscreen
                    .then_some(Fullscreen::Borderless(None)),
            );
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
        let settings = self.menu.lobby.as_ref().map(|lobby| lobby.settings);
        let hud = Hud {
            match_state: room.match_state.clone(),
            elapsed: 0.0,
            game_mode: GameMode::Team,
            endless_match: false,
            map_mode: settings
                .map(|settings| settings.map_mode)
                .unwrap_or_default(),
            map_name: settings
                .map(|settings| map_option_for(settings.map_mode).name.to_owned())
                .unwrap_or_default(),
            difficulty: settings
                .map(|settings| settings.difficulty)
                .unwrap_or_default(),
            human_team,
            score_limit: SCORE_LIMIT,
            team_names: TEAM_NAMES,
            active_enemies: 0,
            speed_tuning: SpeedTuning::default(),
            human: room.human,
            scoreboard: room.scoreboard.into_owned(),
            recap: None,
            score_pill: room.score_pill,
        };
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
            && let Some(pixels) = bake.finished()
        {
            let key = bake.key.clone();
            self.bake = None;
            if let Err(error) = self.game.supply_texture(&key, &pixels) {
                eprintln!("{error}");
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
        let input = self.controls.take_input();
        let camera_input = input.zoom != 0.0 || input.toggle_view;
        let result = self.game.frame(now, &input.pack())?;
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
        eprintln!("The room stopped: {error}");
        self.stopped = true;
        self.controls.clear();
        self.game.renderer_failed();
        self.pump();
    }

    /// Run a frame and show it; true when the player chose to quit.
    fn redraw(&mut self) -> bool {
        let now = now_ms();
        let frame_dt = ((now - self.last_frame) / 1000.0).clamp(0.0, 0.1);
        self.last_frame = now;
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
        self.frame_index += 1;
        if !self.menu.in_battle() {
            self.ui.tick(frame_dt);
        }
        if self.resume_after_settings && self.ui.settings_draft.is_none() {
            self.resume_after_settings = false;
            if self.menu.menu_open {
                self.apply(Action::Resume);
            }
        }
        if self.ui.show_nerd_stats
            && (self.debug || self.prefs.debug)
            && self.frame_index.is_multiple_of(STATS_EVERY_FRAMES)
        {
            self.ui.stats = Some(self.game.nerd_stats(now));
        }
        self.hold_pointer();
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
        self.hidden_changed();
        let mut quit = false;
        for action in actions {
            quit |= self.apply(action);
        }
        quit
    }
}

/// Where a seat is kept: one per server and room, as the page keys `sessionStorage`.
fn seat_key(selection: &RoomSelection) -> String {
    format!("{}:{}", selection.server, selection.room)
}
