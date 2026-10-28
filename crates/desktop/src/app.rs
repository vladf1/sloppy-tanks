//! The window and its game (`src/main.ts` and `src/game.ts`): the arena prepares
//! behind Battle Setup, GO starts the round, and every redraw runs one engine frame
//! with the packed input, plays its events' sounds, lays out the HUD and menus and
//! shows the frame with them drawn over it.
//!
//! One engine owns the window's surface at a time: single player's `Game` behind Battle
//! Setup ([`Session`]), or a multiplayer room's `NetGame` ([`RoomSession`]). Joining a
//! room drops the game and its renderer first and keeps the rest of the shell
//! ([`Shell`]); leaving the room does the reverse.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use sloppy_render::gpu::RenderTarget;
use sloppy_render::wgpu;
use sloppy_web::{Game, frame_slot};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::audio::Audio;
use crate::bakes::{self, Bake};
use crate::engine_api::{EventBatch, HudState, Phase};
use crate::frame_log::{FrameLog, FrameParts};
use crate::input::{Controls, Shortcut, pack};
use crate::overlay::Overlay;
use crate::preferences::Preferences;
use crate::room::{POLL_MS, RoomSession, SavedSeats};
use crate::room_api::RoomSelection;
use crate::ui::rooms::RoomsState;
use crate::ui::{self, Action, UiState};

/// Pipelines created per preparation step; the menu redraws between steps.
const PREPARE_BUDGET: u32 = 4;
/// Frames between Stats for nerds readings.
const STATS_EVERY_FRAMES: u64 = 15;
/// The window's first size, in points.
const WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1280.0, 800.0);
const MIN_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1100.0, 720.0);
/// How often a window that takes no frames tries again.
const HIDDEN_RECHECK: Duration = Duration::from_millis(100);
/// Battle Setup's status once the arena is ready.
const READY_STATUS: &str = "Ready when you are.";

/// Command-line choices.
pub struct Options {
    /// `--debug`: extra levels in Battle Setup.
    pub debug: bool,
    /// `--frame-log`: frame rate and frame parts on stderr each second.
    pub frame_log: bool,
    /// `--autoplay`: the player's tank drives itself and the round starts once the
    /// arena is ready, for unattended frame-rate runs.
    pub autoplay: bool,
    /// `--directory <url>`: the server directory to list rooms from.
    pub directory: Option<String>,
}

/// Which engine has the window.
enum Mode {
    Single(Box<Session>),
    Room(Box<RoomSession>),
}

pub struct App {
    options: Options,
    mode: Option<Mode>,
}

impl App {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            mode: None,
        }
    }

    /// Hand the window to the other engine when the one playing asks: a room Battle
    /// Setup chose, or Battle Setup after a room. False when no engine could start.
    fn switch_engine(&mut self) -> bool {
        let wants_room =
            matches!(&self.mode, Some(Mode::Single(session)) if session.pending_room.is_some());
        let wants_setup = matches!(&self.mode, Some(Mode::Room(room)) if room.exit.is_some());
        if !wants_room && !wants_setup {
            return true;
        }
        let shell = match self.mode.take() {
            Some(Mode::Single(mut session)) => {
                let selection = session.pending_room.take();
                let shell = session.into_shell();
                crate::surface_layers::release(&shell.window);
                match selection.map(|selection| RoomSession::new(shell, selection)) {
                    Some(Ok(room)) => {
                        self.mode = Some(Mode::Room(Box::new(room)));
                        return true;
                    }
                    Some(Err(failed)) => {
                        let (mut shell, notice) = *failed;
                        crate::surface_layers::release(&shell.window);
                        shell.rooms.returned_from("", &notice);
                        shell
                    }
                    None => return false,
                }
            }
            Some(Mode::Room(mut room)) => {
                let notice = room.exit.take().unwrap_or_default();
                let code = room.room().to_owned();
                let mut shell = room.into_shell();
                crate::surface_layers::release(&shell.window);
                shell.rooms.returned_from(&code, &notice);
                shell
            }
            None => return false,
        };
        match Session::start(shell) {
            Ok(session) => {
                self.mode = Some(Mode::Single(Box::new(session)));
                true
            }
            Err(error) => {
                eprintln!("Sloppy Tanks could not start: {error}");
                false
            }
        }
    }
}

/// What outlives an engine on the window: the window, the choices, the sound, and
/// Battle Setup's Multiplayer tab with the seats this run holds.
pub struct Shell {
    pub window: Arc<Window>,
    pub prefs: Preferences,
    pub audio: Option<Audio>,
    pub debug: bool,
    pub build: String,
    pub rooms: RoomsState,
    pub seats: SavedSeats,
    /// `--frame-log` (see [`Options`]).
    pub frame_log: bool,
    /// `--autoplay`, for the first single-player round only.
    pub autoplay: bool,
}

/// The release line Battle Setup shows.
fn build_line() -> String {
    format!(
        "v{} · {}",
        option_env!("SLOPPY_RELEASE").unwrap_or(env!("CARGO_PKG_VERSION")),
        option_env!("SLOPPY_COMMIT").unwrap_or("dev")
    )
}

/// The engine's clock, in milliseconds (the origin `Game` and `NetGame` read too).
pub fn now_ms() -> f64 {
    sloppy_web::clock_start().elapsed().as_secs_f64() * 1000.0
}

/// The window, its game and everything around it.
struct Session {
    window: Arc<Window>,
    game: Game,
    overlay: Overlay,
    controls: Controls,
    modifiers: ModifiersState,
    prefs: Preferences,
    ui: UiState,
    audio: Option<Audio>,
    bake: Option<Bake>,
    debug: bool,
    build: String,
    /// The team the engine drew for the player (`initial_game_options`).
    human_team: u8,
    /// The options the engine's world was built with, and those wanted now.
    prepared_options: String,
    /// Preparation of the prepared options' arena is still running.
    preparing: bool,
    /// Battle Setup choices that have been on screen for a frame, ready to rebuild for.
    options_shown: Option<String>,
    compiled: u32,
    /// A round is on (the HUD shows; frames run).
    in_round: bool,
    phase: Phase,
    alive: bool,
    first_person: bool,
    cursor_grabbed: bool,
    /// The window is covered or minimized (macOS says so).
    occluded: bool,
    /// The window has no area: minimized on Windows, which reports no occlusion.
    minimized: bool,
    /// The window took the last frame.
    frame_shown: bool,
    frame_index: u64,
    last_frame: f64,
    /// Seconds the HUD's state has advanced since its last refresh.
    hud_dt: f64,
    /// The engine stopped (a GPU error); shown instead of the game.
    failure: Option<String>,
    frame_log: Option<FrameLog>,
    /// This frame's parts, for the frame log.
    parts: FrameParts,
    /// Seats rooms gave this run, kept for the next room.
    seats: SavedSeats,
    /// CREATE ROOM or JOIN ROOM: the app hands the window to the room.
    pending_room: Option<RoomSelection>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.mode.is_some() {
            return;
        }
        match Session::new(event_loop, &self.options) {
            Ok(session) => self.mode = Some(Mode::Single(Box::new(session))),
            Err(error) => {
                eprintln!("Sloppy Tanks could not start: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window: WindowId,
        event: WindowEvent,
    ) {
        let quit = match &mut self.mode {
            Some(Mode::Single(session)) => session.window_event(&event),
            Some(Mode::Room(room)) => room.window_event(&event),
            None => return,
        };
        if quit || !self.switch_engine() {
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device: DeviceId,
        event: DeviceEvent,
    ) {
        let DeviceEvent::MouseMotion { delta } = event else {
            return;
        };
        let (controls, grabbed) = match &mut self.mode {
            Some(Mode::Single(session)) => (&mut session.controls, session.cursor_grabbed),
            Some(Mode::Room(room)) => (&mut room.controls, room.cursor_grabbed),
            None => return,
        };
        if grabbed {
            // Like the page's `movementX`: macOS reports points, Windows raw counts.
            controls.look(delta.0);
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if !matches!(cause, winit::event::StartCause::ResumeTimeReached { .. }) {
            return;
        }
        match &self.mode {
            Some(Mode::Single(session)) => session.window.request_redraw(),
            Some(Mode::Room(room)) => room.window.request_redraw(),
            None => {}
        }
    }

    /// The app ends however it was asked to (the macOS menu's Quit included): a room
    /// says it is leaving first.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(Mode::Room(room)) = &mut self.mode {
            room.quit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match &mut self.mode {
            // A hidden window draws nothing: macOS does not pace a covered Metal layer
            // to the display, so it would spin hundreds of frames a second unseen.
            Some(Mode::Single(session)) if session.hidden() => {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            // A window that opened covered gets no occlusion event: while the window
            // takes no frames, look again a few times a second.
            Some(Mode::Single(session)) if !session.frame_shown => {
                event_loop
                    .set_control_flow(ControlFlow::WaitUntil(Instant::now() + HIDDEN_RECHECK));
            }
            Some(Mode::Single(session)) => {
                event_loop.set_control_flow(ControlFlow::Wait);
                session.window.request_redraw();
            }
            Some(Mode::Room(room)) => {
                // A hidden window draws no frames; the room's heartbeat runs on anyway.
                room.network();
                // A hidden window draws nothing; the heartbeat's timer probes it.
                if !room.hidden() {
                    room.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(POLL_MS as u64),
                ));
                if room.exit.is_some() && !self.switch_engine() {
                    event_loop.exit();
                }
            }
            None => {}
        }
    }
}

impl Session {
    fn new(event_loop: &ActiveEventLoop, options: &Options) -> Result<Self, String> {
        let prefs = Preferences::load();
        let mut attributes = Window::default_attributes()
            .with_title("Sloppy Tanks")
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(MIN_WINDOW_SIZE)
            .with_window_icon(window_icon());
        if prefs.fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| error.to_string())?,
        );
        let mut rooms = RoomsState::default();
        if let Some(directory) = &options.directory {
            rooms.directory.clone_from(directory);
        }
        Self::start(Shell {
            window,
            prefs,
            audio: None,
            debug: options.debug,
            build: build_line(),
            rooms,
            seats: SavedSeats::new(),
            frame_log: options.frame_log,
            autoplay: options.autoplay,
        })
    }

    /// Single player on the shell's window: the arena prepares behind Battle Setup.
    fn start(shell: Shell) -> Result<Self, String> {
        let Shell {
            window,
            prefs,
            audio,
            debug,
            build,
            rooms,
            seats,
            frame_log,
            autoplay,
        } = shell;
        let size = window.inner_size();
        let scale = window.scale_factor();
        let logical = size.to_logical::<f64>(scale);
        let target = RenderTarget {
            window: wgpu::SurfaceTarget::from(window.clone()),
            width: size.width,
            height: size.height,
        };
        let extra = !ui::setup::MAPS.iter().any(|(id, _)| *id == prefs.map);
        let config = json!({
            "assetBase": "",
            "map": prefs.map,
            "difficulty": prefs.difficulty,
            "extraLevels": debug || extra,
            "humanKind": prefs.tank,
            "gameMode": prefs.game_mode,
            "cssWidth": logical.width,
            "cssHeight": logical.height,
            "pixelRatio": scale,
            "firstPerson": prefs.first_person,
            "zoom": prefs.zoom,
            "graphics": prefs.graphics,
            "device": "desktop",
            "autoplay": autoplay,
        });
        let mut game = pollster::block_on(Game::create(target, &config.to_string()))?;
        let hud = HudState::parse(&game.hud_json()).unwrap_or_default();
        game.set_speed("tank-speed", prefs.tank_speed);
        game.set_speed("bullet-speed", prefs.bullet_speed);
        let overlay = Overlay::new(window.clone(), game.renderer());
        let mut session = Self {
            human_team: hud.human_team,
            window,
            game,
            overlay,
            controls: Controls::default(),
            modifiers: ModifiersState::default(),
            ui: UiState {
                hud: Some(hud),
                status: "Building the arena…".into(),
                rooms,
                ..UiState::default()
            },
            audio,
            bake: None,
            debug,
            build,
            prepared_options: String::new(),
            preparing: true,
            options_shown: None,
            compiled: 0,
            in_round: false,
            phase: Phase::Ready,
            alive: true,
            first_person: prefs.first_person,
            cursor_grabbed: false,
            occluded: false,
            minimized: false,
            frame_shown: true,
            frame_index: 0,
            last_frame: now_ms(),
            hud_dt: 0.0,
            failure: None,
            frame_log: frame_log.then(FrameLog::new),
            parts: FrameParts::default(),
            seats,
            pending_room: None,
            prefs,
        };
        session.ui.starting = autoplay;
        session.prepared_options = session.wanted_options();
        Ok(session)
    }

    /// Give the window up: the game, its renderer and the overlay go, the rest stays.
    fn into_shell(self) -> Shell {
        let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        self.window.set_cursor_visible(true);
        Shell {
            window: self.window,
            prefs: self.prefs,
            audio: self.audio,
            debug: self.debug,
            build: self.build,
            rooms: self.ui.rooms,
            seats: self.seats,
            frame_log: self.frame_log.is_some(),
            autoplay: false,
        }
    }

    /// Battle Setup's choices as `set_options` takes them.
    fn wanted_options(&self) -> String {
        json!({
            "humanKind": self.prefs.tank,
            "humanTeam": self.human_team,
            "gameMode": self.prefs.game_mode,
            "mapMode": self.prefs.map,
            "difficulty": self.prefs.difficulty,
        })
        .to_string()
    }

    /// Handle a window event; true when the app should quit.
    fn window_event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::RedrawRequested = event {
            return self.redraw();
        }
        let battle =
            self.in_round && self.phase == Phase::Playing && self.ui.settings_draft.is_none();
        let keys_to_menus = !battle || self.overlay.typing();
        let consumed = self
            .overlay
            .event(event, !self.cursor_grabbed, keys_to_menus);
        let playing = self.in_round && self.phase == Phase::Playing && self.alive;
        match event {
            WindowEvent::CloseRequested => return true,
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                self.hidden_changed();
                self.resize();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.resize(),
            WindowEvent::Occluded(occluded) => {
                self.occluded = *occluded;
                self.hidden_changed();
            }
            // Like the page's blur: held keys let go, the round plays on.
            WindowEvent::Focused(false) => {
                self.controls.clear();
                self.hold_pointer();
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
            // A press on a menu or the HUD's buttons is not a shot; a release always
            // reaches the controls, so fire never sticks.
            WindowEvent::MouseInput { state, button, .. }
                if *state == ElementState::Released
                    || (playing
                        && (self.cursor_grabbed
                            || !(consumed || self.overlay.wants_pointer()))) =>
            {
                self.controls.button(*button, *state);
            }
            WindowEvent::MouseWheel { delta, .. }
                if !self.overlay.wants_pointer() && self.in_round =>
            {
                self.controls
                    .wheel(*delta, self.modifiers.shift_key(), playing);
            }
            _ => {}
        }
        false
    }

    /// Whether the window can show nothing: covered, or minimized.
    fn hidden(&self) -> bool {
        self.occluded || self.minimized
    }

    /// Like a hidden page, a hidden window pauses the round and lets the cursor go.
    fn hidden_changed(&mut self) {
        if self.hidden() {
            self.controls.clear();
            if self.in_round && self.phase == Phase::Playing {
                self.apply(Action::Pause);
            }
        }
        self.hold_pointer();
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::Escape => {
                if self.ui.settings_draft.is_some() {
                    self.ui.settings_draft = None;
                } else if self.in_round && self.phase == Phase::Playing {
                    self.apply(Action::Pause);
                } else if self.in_round && self.phase == Phase::Paused {
                    self.apply(Action::Resume);
                }
            }
            Shortcut::ToggleView => {
                if self.in_round && self.phase == Phase::Playing {
                    self.apply(Action::ToggleView);
                }
            }
            Shortcut::NerdStats => {
                if self.debug || self.prefs.debug {
                    self.ui.show_nerd_stats = !self.ui.show_nerd_stats;
                }
            }
            Shortcut::Fullscreen => self.apply(Action::ToggleFullscreen),
        }
    }

    /// Render at the window's size in points times its scale (the engine caps it).
    fn resize(&mut self) {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor();
        let logical = size.to_logical::<f64>(scale);
        self.game
            .resize(logical.width, logical.height, scale, false);
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Start => {
                if self.in_round && self.phase == Phase::Results {
                    self.begin_round();
                } else {
                    self.ui.starting = true;
                    self.ui.status = "Preparing your arena…".into();
                }
            }
            Action::Resume => {
                self.controls.clear();
                self.game.resume();
                self.refresh_hud();
            }
            Action::Pause => {
                if self.phase == Phase::Playing {
                    self.game.pause();
                    self.controls.clear();
                    self.refresh_hud();
                }
            }
            Action::EndBattle => {
                self.game.end_battle();
                self.refresh_hud();
            }
            Action::BattleSetup => {
                self.controls.clear();
                self.game.restart();
                self.in_round = false;
                self.ui.reset_feedback();
                self.ui.starting = false;
                self.preparing = true;
                self.compiled = 0;
                self.refresh_hud();
            }
            Action::NextTank(kind) => {
                self.prefs.tank = kind;
                self.prefs.save();
                if let Err(error) = self.game.set_human_kind(&self.prefs.tank) {
                    eprintln!("{error}");
                }
                self.prepared_options = self.wanted_options();
            }
            Action::SelectAmmo(slot) => {
                if self.phase == Phase::Playing && self.alive {
                    self.controls.select_ammo(slot);
                }
            }
            Action::ToggleView => {
                self.first_person = self.game.toggle_first_person();
                self.save_camera();
            }
            Action::ToggleFullscreen => {
                let fullscreen = self.window.fullscreen().is_none();
                self.window
                    .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
                self.prefs.fullscreen = fullscreen;
                self.prefs.save();
            }
            Action::OpenSettings => {
                if self.in_round && self.phase == Phase::Playing {
                    self.apply(Action::Pause);
                }
                ui::menus::open_settings(&mut self.ui, &self.prefs);
            }
            Action::SettingsSaved(prefs) => self.save_settings(*prefs),
            Action::Quit => {}
            Action::JoinRoom(selection) => {
                self.prefs.player_name.clone_from(&selection.name);
                self.prefs.save();
                self.pending_room = Some(*selection);
            }
            // A room's own menu.
            Action::RoomStart
            | Action::RoomEnd
            | Action::RoomLeave
            | Action::RoomRejoin
            | Action::RoomSetup(_)
            | Action::RoomChoose { .. }
            | Action::RoomRules(_) => {}
        }
    }

    /// Apply and keep changed settings and Battle Setup choices.
    fn save_settings(&mut self, prefs: Preferences) {
        let old = std::mem::replace(&mut self.prefs, prefs);
        if old.graphics != self.prefs.graphics {
            self.game.set_graphics(&self.prefs.graphics);
        }
        if old.tank_speed != self.prefs.tank_speed {
            self.prefs.tank_speed = self.game.set_speed("tank-speed", self.prefs.tank_speed);
        }
        if old.bullet_speed != self.prefs.bullet_speed {
            self.prefs.bullet_speed = self.game.set_speed("bullet-speed", self.prefs.bullet_speed);
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

    fn save_camera(&mut self) {
        let camera = self.game.camera_preferences();
        self.prefs.first_person = camera.first().is_some_and(|&on| on == 1.0);
        self.prefs.zoom = camera.get(1).copied();
        self.prefs.save();
    }

    fn refresh_hud(&mut self) {
        if let Some(hud) = HudState::parse(&self.game.hud_json()) {
            self.phase = hud.match_state.phase;
            self.alive = hud.human.alive;
            self.ui.tick(self.hud_dt);
            self.hud_dt = 0.0;
            self.ui.hud = Some(hud);
        }
    }

    fn begin_round(&mut self) {
        self.in_round = true;
        self.ui.starting = false;
        self.ui.reset_feedback();
        self.controls.clear();
        self.hud_dt = 0.0;
        self.game.start();
        if self.audio.is_none() {
            self.audio = Audio::new();
            if let Some(audio) = &mut self.audio {
                audio.set_volume(self.prefs.volume);
            }
        }
        self.refresh_hud();
    }

    /// One step of preparing the wanted arena behind Battle Setup.
    fn prepare(&mut self) -> Result<(), String> {
        let wanted = self.wanted_options();
        if wanted != self.prepared_options {
            // The rebuild holds this thread for a moment: the player's new choice, or
            // GO's LOADING…, shows first, as the page waits for a paint.
            if self.options_shown.as_ref() != Some(&wanted) {
                self.options_shown = Some(wanted);
                return Ok(());
            }
            self.game.set_options(&wanted)?;
            self.prepared_options = wanted;
            self.preparing = true;
            self.compiled = 0;
            if let Some(hud) = HudState::parse(&self.game.hud_json()) {
                self.ui.hud = Some(hud);
            }
        }
        if !self.preparing {
            if self.ui.starting {
                self.begin_round();
            }
            return Ok(());
        }
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
            self.game.supply_texture(&key, &pixels)?;
        }
        let status = self.game.prepare_step(PREPARE_BUDGET)?;
        let [compiled, remaining, textures, done, gpu] =
            [0, 1, 2, 3, 4].map(|index| status.get(index).copied().unwrap_or(0.0));
        self.compiled += compiled as u32;
        if done == 1.0 {
            self.preparing = false;
            self.ui.ready = true;
            self.ui.status = READY_STATUS.into();
            return Ok(());
        }
        // A rebuild that reuses every pipeline and texture still waits for its warm-up
        // frames; that is not loading, so Battle Setup keeps showing it is ready rather
        // than flicker through the loading lines.
        if self.compiled == 0 && remaining == 0.0 && textures == 0.0 {
            return Ok(());
        }
        self.ui.ready = false;
        if remaining > 0.0 {
            self.ui.status = format!(
                "Shaders loaded: {} of {}",
                self.compiled,
                self.compiled + remaining as u32
            );
        } else if gpu == 1.0 {
            self.ui.status = "Compiling shaders…".into();
        } else if textures > 0.0 {
            self.ui.status = format!("Loading textures… {textures} left");
        }
        Ok(())
    }

    /// One engine frame of the round; its events and HUD reach the screens.
    fn battle_frame(&mut self, now: f64) -> Result<(), String> {
        let input = self.controls.take_input();
        let zoomed = input.zoom != 0.0;
        let result = self.game.frame(now, &pack(&input))?;
        self.parts.engine = now_ms() - now;
        self.parts.sim = f64::from(result.get(frame_slot::SIM_MS).copied().unwrap_or(0.0));
        self.parts.render = f64::from(result.get(frame_slot::RENDER_MS).copied().unwrap_or(0.0));
        if let Some(error) = self.game.error() {
            return Err(error);
        }
        if zoomed {
            self.save_camera();
        }
        let slot = |index: usize| result.get(index).copied().unwrap_or(0.0);
        self.phase = Phase::from_slot(slot(frame_slot::PHASE));
        self.alive = slot(frame_slot::HUMAN_ALIVE) == 1.0;
        self.first_person = slot(frame_slot::FIRST_PERSON) == 1.0;
        self.ui.cockpit = slot(frame_slot::COCKPIT) == 1.0;
        self.ui.hull_angle = f64::from(slot(frame_slot::HULL_ANGLE));
        let viewer = (slot(frame_slot::VIEWER_X), slot(frame_slot::VIEWER_Y));
        self.ui.viewer = (viewer.0.is_finite() && viewer.1.is_finite()).then_some(viewer);
        if slot(frame_slot::CLEAR_INPUT) == 1.0 {
            self.controls.clear();
        }
        if slot(frame_slot::EVENTS) > 0.0 {
            let batch = EventBatch::parse(&self.game.drain_events());
            if let Some(audio) = &mut self.audio {
                audio.play(&batch);
            }
            // A death names its killer, who may have joined since the last refresh.
            if batch.events.iter().any(|event| event.kind == "death") {
                self.refresh_hud();
            }
            for event in &batch.events {
                self.ui.event(event);
            }
        }
        let dt = f64::from(slot(frame_slot::DT));
        self.hud_dt += dt;
        if slot(frame_slot::HUD_DUE) == 1.0 {
            self.refresh_hud();
        }
        Ok(())
    }

    /// Grab the cursor while first person steers, so the mouse turns the view.
    fn hold_pointer(&mut self) {
        let wanted = self.in_round
            && self.first_person
            && self.phase == Phase::Playing
            && self.ui.settings_draft.is_none()
            && self.window.has_focus()
            && !self.hidden();
        grab_pointer(&self.window, wanted, &mut self.cursor_grabbed);
    }

    /// Run a frame and show it; true when the player chose to quit.
    fn redraw(&mut self) -> bool {
        let now = now_ms();
        let frame_dt = ((now - self.last_frame) / 1000.0).clamp(0.0, 0.1);
        self.last_frame = now;
        // A window that takes no frames only looks for its window again: engine frames
        // there would count as slow ones and lower Auto's graphics quality.
        if !self.frame_shown && self.in_round && self.phase == Phase::Playing {
            self.apply(Action::Pause);
        }
        if self.failure.is_none() && (self.frame_shown || !self.in_round) {
            let step = if self.in_round {
                self.battle_frame(now)
            } else {
                self.prepare()
            };
            if let Err(error) = step {
                eprintln!("The game stopped: {error}");
                self.failure = Some(error);
            }
        }
        self.frame_index += 1;
        if !self.in_round {
            self.ui.tick(frame_dt);
        } else if self.phase != Phase::Playing {
            self.ui.tick(0.0);
        }
        if self.ui.show_nerd_stats
            && (self.debug || self.prefs.debug)
            && self.frame_index.is_multiple_of(STATS_EVERY_FRAMES)
        {
            self.ui.stats = serde_json::from_str(&self.game.stats_json()).ok();
        }
        self.hold_pointer();
        let view = ui::View {
            in_round: self.in_round,
            phase: self.phase,
            prefs: &self.prefs,
            build: &self.build,
            debug: self.debug || self.prefs.debug,
            room: false,
        };
        let ui_state = &mut self.ui;
        let failure = self.failure.clone();
        let mut actions = Vec::new();
        let overlay_start = now_ms();
        let painted = self.overlay.run(|context| {
            if let Some(failure) = &failure {
                ui::menus::failure(context, failure);
            } else {
                actions = ui::draw(context, ui_state, &view);
            }
        });
        let target = self.game.renderer().size();
        let present_start = now_ms();
        self.parts.overlay = present_start - overlay_start;
        self.overlay.present(self.game.renderer(), painted);
        self.frame_shown = self.game.renderer().frame_shown();
        self.parts.present = now_ms() - present_start;
        if let Some(log) = &mut self.frame_log {
            let scene = self.game.renderer().scene_size();
            let gpu_ms = log
                .due()
                .then(|| serde_json::from_str::<serde_json::Value>(&self.game.stats_json()).ok())
                .flatten()
                .and_then(|stats| stats["quality"]["gpuMs"].as_f64());
            log.frame(&self.parts, target, scene, gpu_ms);
        }
        let mut quit = false;
        for action in actions {
            quit |= action == Action::Quit;
            self.apply(action);
        }
        quit
    }
}

/// Capture the cursor while first person steers (so the mouse turns the view), or
/// give it back.
pub fn grab_pointer(window: &Window, wanted: bool, grabbed: &mut bool) {
    if wanted == *grabbed {
        return;
    }
    if wanted {
        let captured = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        if captured.is_ok() {
            window.set_cursor_visible(false);
            *grabbed = true;
        }
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        *grabbed = false;
    }
}

/// The window's icon (Windows and Linux show it; macOS takes the bundle's).
fn window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(crate::assets::file("icons/icon-192.png")?).ok()?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
}
