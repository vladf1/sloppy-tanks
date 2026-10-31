//! The window and its game (`src/main.ts` and `src/game.ts`): the arena prepares
//! behind Battle Setup, GO starts the round, and every redraw runs one engine frame
//! with the packed input, plays its events' sounds, lays out the HUD and menus and
//! shows the frame with them drawn over it.
//!
//! One engine owns the window's surface at a time: single player's `Game` behind Battle
//! Setup ([`Session`]), or a multiplayer room's `NetGame` ([`RoomSession`]). Joining a
//! room drops the game and its renderer first and keeps the rest of the shell
//! ([`Shell`]); leaving the room does the reverse. An engine starts as a [`Task`]:
//! natively at once, in a page once the browser has given it a GPU device.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use sloppy_core::net::client::POLL_MS;
use sloppy_core::sim::game_options::GameOptions;
use sloppy_core::sim::map_options::{MapId, is_extra_level};
use sloppy_core::sim::speed_tuning::SpeedSetting;
use sloppy_core::sim::{MatchPhase, SimEventType, Team};
use sloppy_render::gpu::RenderTarget;
use sloppy_render::presentation::PrepareStatus;
use sloppy_render::quality::DeviceClass;
use sloppy_web::hud::Loading;
use sloppy_web::{Game, GameConfig, PageError, ViewConfig, frame_slot};
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use crate::audio::Audio;
use crate::automation::{self, Command, Extra};
use crate::bakes::{self, Bake};
use crate::frame_log::{FrameLog, FrameParts};
use crate::input::Shortcut;
use crate::links::{self, Place};
use crate::overlay::Overlay;
use crate::preferences::Preferences;
use crate::room::{NetworkTools, RoomSession, SavedSeats};
use crate::room_api::RoomSelection;
use crate::tasks::Task;
use crate::touch::{Play, Showing};
use crate::ui::rooms::RoomsState;
use crate::ui::{self, Action, UiState};
use crate::window_input::{Battle, Request, WindowInput};

/// Pipelines created per preparation step.
const PREPARE_BUDGET: u32 = 4;
/// Milliseconds of a redraw that preparation steps may take before the menu paints;
/// a step that is the arena's scenery or a native pipeline compile can overrun it.
const PREPARE_SLICE_MS: f64 = 8.0;
/// The window's first size, in points.
#[cfg(not(any(target_os = "ios", target_arch = "wasm32")))]
const WINDOW_SIZE: winit::dpi::LogicalSize<f64> = winit::dpi::LogicalSize::new(1280.0, 800.0);
#[cfg(not(any(target_os = "ios", target_arch = "wasm32")))]
const MIN_WINDOW_SIZE: winit::dpi::LogicalSize<f64> = winit::dpi::LogicalSize::new(1100.0, 720.0);
/// How often a window that takes no frames tries again.
const HIDDEN_RECHECK: Duration = Duration::from_millis(100);
/// Battle Setup's status once the arena is ready.
const READY_STATUS: &str = "Ready when you are.";

/// Command-line choices, a page's link's included (`main.rs`).
pub struct Options {
    /// `--debug`: extra levels in Battle Setup.
    pub debug: bool,
    /// `--frame-log`: frame rate and frame parts on stderr (a page's console) each second.
    pub frame_log: bool,
    /// `--autoplay`: the player's tank drives itself and the round starts once the
    /// arena is ready, for unattended frame-rate runs.
    pub autoplay: bool,
    /// `--directory <url>`: the server directory to list rooms from.
    pub directory: Option<String>,
    /// `--map <id>`: Battle Setup's map in place of the saved one, an extra level only
    /// where extra levels are offered (`initialGameOptions`).
    pub map: Option<MapId>,
    /// `--multiplayer`: Battle Setup opens on its Multiplayer tab.
    pub multiplayer: bool,
    /// `--room <CODE>`, an invite link: the Multiplayer tab, with the room selected once
    /// the directory lists it.
    pub room: Option<String>,
    /// The development network tools every room this run joins takes.
    pub network: NetworkTools,
    /// `--seed <n>`: the first single-player arena's seed in place of a random one, for
    /// measurements that compare builds on the same match.
    pub seed: Option<f64>,
}

/// Which engine has the window.
enum Mode {
    Single(Box<Session>),
    Room(Box<RoomSession>),
    /// The next engine is starting; none when it could not.
    Starting(Task<Option<Mode>>),
}

impl Mode {
    /// The window's size may have changed while the engine started.
    fn resize(&mut self) {
        match self {
            Mode::Single(session) => session.resize(),
            Mode::Room(room) => room.resize(),
            Mode::Starting(_) => {}
        }
    }
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
        match self.mode.take() {
            Some(Mode::Single(mut session)) => match session.pending_room.take() {
                Some(selection) => {
                    let shell = session.into_shell();
                    crate::surface_layers::release(&shell.window);
                    links::show(Place::Room(&selection.room));
                    self.launch(join_room(shell, selection, self.options.network.clone()))
                }
                None => {
                    self.mode = Some(Mode::Single(session));
                    true
                }
            },
            Some(Mode::Room(mut room)) => match room.exit.take() {
                Some(notice) => {
                    let code = room.room().to_owned();
                    let mut shell = room.into_shell();
                    crate::surface_layers::release(&shell.window);
                    // A room left on purpose leaves the Multiplayer tab; one that ended
                    // keeps its address, and a reload selects it again.
                    if notice.is_empty() {
                        links::show(Place::Multiplayer);
                    }
                    shell.rooms.returned_from(&code, &notice);
                    self.launch(single_player(shell))
                }
                None => {
                    self.mode = Some(Mode::Room(room));
                    true
                }
            },
            mode => {
                self.mode = mode;
                self.started()
            }
        }
    }

    /// Start the next engine on the window; false when none could start.
    fn launch(&mut self, engine: impl Future<Output = Option<Mode>> + 'static) -> bool {
        self.mode = Some(Mode::Starting(Task::on_main_thread(engine)));
        self.started()
    }

    /// Hand the window to the engine that was starting, once it has; false when none
    /// could start.
    fn started(&mut self) -> bool {
        let Some(Mode::Starting(task)) = &self.mode else {
            return true;
        };
        match task.take() {
            None => true,
            Some(None) => false,
            Some(Some(mut mode)) => {
                mode.resize();
                self.mode = Some(mode);
                true
            }
        }
    }
}

/// Single player on the shell's window; none when it could not start.
async fn single_player(shell: Shell) -> Option<Mode> {
    let window = shell.window.clone();
    let session = Session::start(shell).await;
    // A page waits for no event: the first frame wakes it.
    window.request_redraw();
    match session {
        Ok(session) => Some(Mode::Single(Box::new(session))),
        Err(error) => {
            startup_failed(&error);
            None
        }
    }
}

/// No engine could start: natively the app ends, saying why on stderr. A browser can
/// offer a WebGPU adapter and still fail to make the engine's device, so a page's WebGPU
/// build hands over to its WebGL2 one (`index.html` loads that for `?webgl`), as the
/// page's engine fell back before; otherwise the page's loading view says why.
fn startup_failed(error: &str) {
    report!("Sloppy Tanks could not start: {error}");
    #[cfg(target_arch = "wasm32")]
    {
        if cfg!(not(feature = "webgl"))
            && let Some(location) = web_sys::window().map(|window| window.location())
            && let Ok(url) = location.href().and_then(|href| web_sys::Url::new(&href))
            && !url.search_params().has("webgl")
        {
            url.search_params().append("webgl", "");
            if location.replace(&url.href()).is_ok() {
                return;
            }
        }
        crate::page::failed(error);
    }
}

/// The page's browser, by its user agent string.
#[cfg(target_arch = "wasm32")]
fn user_agent() -> String {
    web_sys::window()
        .and_then(|window| window.navigator().user_agent().ok())
        .unwrap_or_default()
}

/// The room Battle Setup chose, with `network`'s tools; Battle Setup again with the room
/// selected, saying why, when it fails.
async fn join_room(shell: Shell, selection: RoomSelection, network: NetworkTools) -> Option<Mode> {
    let window = shell.window.clone();
    let code = selection.room.clone();
    match RoomSession::new(shell, selection, network).await {
        Ok(room) => {
            window.request_redraw();
            Some(Mode::Room(Box::new(room)))
        }
        Err(failed) => {
            let (mut shell, notice) = *failed;
            crate::surface_layers::release(&shell.window);
            shell.rooms.returned_from(&code, &notice);
            single_player(shell).await
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
    /// `--seed`, for the first single-player arena only.
    pub seed: Option<f64>,
}

/// The release line Battle Setup shows.
fn build_line() -> String {
    format!(
        "v{} · {}",
        option_env!("SLOPPY_RELEASE").unwrap_or(env!("CARGO_PKG_VERSION")),
        option_env!("SLOPPY_COMMIT").unwrap_or("dev")
    )
}

/// The window's size in pixels as the renderer and egui draw it. On iOS winit's inner
/// size is only the safe area, while the view and its Metal layer cover the screen
/// (egui-winit lays out on the whole screen and keeps the insets apart).
pub fn drawable_size(window: &Window) -> winit::dpi::PhysicalSize<u32> {
    if cfg!(target_os = "ios") {
        window.outer_size()
    } else {
        window.inner_size()
    }
}

/// The window's size in points and its pixels per point, as the engines' `resize`
/// takes them (the page's CSS size and device pixel ratio). In a page it is the canvas's
/// CSS box: under an emulated pixel ratio (DevTools' devices, Playwright) the browser
/// reports the canvas's device pixels as CSS pixels, so winit's size is off by that ratio
/// while its pointer positions are not.
pub fn css_size(window: &Window) -> (f64, f64, f64) {
    let scale = window.scale_factor();
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        if let Some(canvas) = window.canvas() {
            let (width, height) = (canvas.client_width(), canvas.client_height());
            return (f64::from(width), f64::from(height), scale);
        }
    }
    let logical = drawable_size(window).to_logical::<f64>(scale);
    (logical.width, logical.height, scale)
}

/// Whether the player may choose fullscreen: not in the iPad app, which always fills the
/// screen, nor in the phone edition, whose top row keeps only its few keys.
pub fn offers_fullscreen() -> bool {
    !cfg!(target_os = "ios") && !crate::touch::phone()
}

/// Fill the screen or give the window back, as `prefs` say, and keep the choice.
pub fn set_fullscreen(window: &Window, prefs: &mut Preferences, fullscreen: bool) {
    if !offers_fullscreen() {
        return;
    }
    window.set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
    prefs.fullscreen = fullscreen;
    prefs.save();
}

/// The engine's clock, in milliseconds (the origin `Game` and `NetGame` read too).
pub fn now_ms() -> f64 {
    sloppy_web::now_ms()
}

/// An engine's error in words: natively its message, in a page a JS `Error`'s.
pub fn engine_error(error: PageError) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Error::from(error).message().into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        error
    }
}

/// What an engine draws into: the window natively, its canvas in a page.
pub fn render_target(window: &Arc<Window>) -> Result<RenderTarget, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let size = drawable_size(window);
        Ok(RenderTarget {
            window: sloppy_render::wgpu::SurfaceTarget::from(window.clone()),
            width: size.width,
            height: size.height,
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        window
            .canvas()
            .ok_or_else(|| "The page has no canvas".to_owned())
    }
}

/// A phone's overhead camera starts this far out until the player zooms
/// (`startingCamera`): its small screen shows more of the arena than the default's.
const PHONE_ZOOM: f64 = 40.0;

/// The view configuration both engines take for `window`, with the player's camera
/// and graphics choices.
pub fn view_config(window: &Window, prefs: &Preferences) -> ViewConfig {
    let (width, height, scale) = css_size(window);
    let device = crate::touch::device();
    ViewConfig {
        css_width: Some(width),
        css_height: Some(height),
        pixel_ratio: Some(scale),
        first_person: prefs.first_person,
        zoom: prefs
            .zoom
            .or((device == DeviceClass::Phone).then_some(PHONE_ZOOM)),
        // Touch devices draw no overhead reticle (`startingCamera`): a finger aims
        // where it is, so the mark sat under it and stuck where it lifted.
        hide_reticle: device != DeviceClass::Desktop,
        graphics: prefs.graphics,
        // Tablets and phones start a tier lower and draw single-sampled (`quality.rs`).
        device,
        // A page words what only a page does (RELOAD, "in this browser") as the page did.
        shell: if cfg!(target_arch = "wasm32") {
            sloppy_web::Shell::Page
        } else {
            sloppy_web::Shell::Desktop
        },
        // A page in WebKit holds the GPU's backlog to two frames itself.
        #[cfg(target_arch = "wasm32")]
        frames_in_flight: sloppy_web::frame_clock::browser_frames_in_flight(&user_agent()),
        ..ViewConfig::default()
    }
}

/// What preparing the arena behind Battle Setup has reached (`prepareArena` in
/// `game.ts`).
#[derive(Debug, PartialEq)]
enum Progress {
    /// The arena is compiled, warm and drawn.
    Ready,
    /// A rebuild that reuses every pipeline and texture still waits for its warm-up
    /// frames; that is not loading, so Battle Setup keeps showing it is ready rather
    /// than flicker through the loading lines.
    Warming,
    /// Loading, with the engine's status line.
    Loading(Loading),
}

/// Preparing the chosen arena, and building what it left for later.
#[derive(Debug, Default)]
struct Preparation {
    /// The prepared options' arena is still preparing.
    preparing: bool,
    /// The library's wrecks, which preparation leaves for later, build a step a redraw
    /// once the ready menu has painted, as the page builds them between tasks: built on
    /// first use instead, they would hitch the battle.
    prebuilding: bool,
}

impl Preparation {
    /// A new arena to prepare.
    fn restart(&mut self) {
        self.preparing = true;
    }

    /// Take one `prepare` step's status and the engine's line for it.
    fn advance(&mut self, status: &PrepareStatus, line: Option<Loading>) -> Progress {
        if status.ready {
            self.preparing = false;
            self.prebuilding = true;
            return Progress::Ready;
        }
        line.map_or(Progress::Warming, Progress::Loading)
    }

    /// Build the next of the wrecks with `step` while any remain; it says whether more
    /// do. Preparation reaches readiness in a redraw before that redraw paints the
    /// ready menu, so the first step waits for the next redraw.
    fn prebuild(&mut self, step: impl FnOnce() -> bool) {
        if self.prebuilding {
            self.prebuilding = step();
        }
    }
}

/// The window, its game and everything around it.
struct Session {
    window: Arc<Window>,
    game: Game,
    overlay: Overlay,
    input: WindowInput,
    prefs: Preferences,
    ui: UiState,
    audio: Option<Audio>,
    bake: Option<Bake>,
    debug: bool,
    build: String,
    /// The team the engine drew for the player (`initial_game_options`).
    human_team: Team,
    /// The options the engine's world was built with.
    prepared_options: GameOptions,
    preparation: Preparation,
    /// Battle Setup choices that have been on screen for a frame, ready to rebuild for.
    options_shown: Option<GameOptions>,
    /// A round is on (the HUD shows; frames run).
    in_round: bool,
    phase: MatchPhase,
    alive: bool,
    first_person: bool,
    /// The window is covered or minimized (macOS says so).
    occluded: bool,
    /// The window has no area: minimized on Windows, which reports no occlusion.
    minimized: bool,
    /// The window took the last frame.
    frame_shown: bool,
    /// Battle Setup has been presented once: the arena, which the engine leaves to its
    /// first preparation step, builds only after it, so the menu shows first.
    menu_shown: bool,
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
        let started = match Session::shell(event_loop, &self.options) {
            Ok(shell) => self.launch(single_player(shell)),
            Err(error) => {
                startup_failed(&error);
                false
            }
        };
        if !started {
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window: WindowId,
        event: WindowEvent,
    ) {
        if !self.started() {
            event_loop.exit();
            return;
        }
        let quit = match &mut self.mode {
            Some(Mode::Single(session)) => session.window_event(&event),
            Some(Mode::Room(room)) => room.window_event(&event),
            Some(Mode::Starting(_)) | None => return,
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
        let (input, window) = match &mut self.mode {
            Some(Mode::Single(session)) => (&mut session.input, &session.window),
            Some(Mode::Room(room)) => (&mut room.input, &room.window),
            Some(Mode::Starting(_)) | None => return,
        };
        if input.pointer_locked() {
            input
                .controls
                .look(crate::pointer_lock::look_travel(window, delta.0));
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if !matches!(cause, winit::event::StartCause::ResumeTimeReached { .. }) {
            return;
        }
        match &self.mode {
            Some(Mode::Single(session)) => session.window.request_redraw(),
            Some(Mode::Room(room)) => room.window.request_redraw(),
            Some(Mode::Starting(_)) | None => {}
        }
    }

    /// iOS takes the screen away for a moment (Control Center, the app switcher, a
    /// call): fingers let go and single player pauses, as a hidden window does.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        match &mut self.mode {
            Some(Mode::Single(session)) => {
                session.input.controls.clear();
                if session.in_round && session.phase == MatchPhase::Playing {
                    session.apply(Action::Pause);
                }
            }
            Some(Mode::Room(room)) => room.input.controls.clear(),
            Some(Mode::Starting(_)) | None => {}
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
        if !self.started() {
            event_loop.exit();
            return;
        }
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
            // The starting engine wakes the loop when it is ready.
            Some(Mode::Starting(_)) | None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

impl Session {
    /// The window and the shell around it, from the saved choices and `options`.
    fn shell(event_loop: &ActiveEventLoop, options: &Options) -> Result<Shell, String> {
        let mut prefs = Preferences::load();
        // A link's map wins over the saved one while it is offered.
        if let Some(map) = options.map
            && (options.debug || prefs.debug || !is_extra_level(map))
        {
            prefs.map = map;
        }
        let window = Arc::new(
            event_loop
                .create_window(window_attributes(&prefs))
                .map_err(|error| error.to_string())?,
        );
        #[cfg(target_os = "ios")]
        {
            crate::ios::attach_to_scene(&window);
            crate::ios::listen_for_keys(&window);
        }
        let mut rooms = RoomsState::default();
        if let Some(directory) = &options.directory {
            rooms.directory.clone_from(directory);
        }
        if options.multiplayer || options.room.is_some() {
            rooms.open_link(options.room.as_deref());
        }
        Ok(Shell {
            window,
            prefs,
            audio: None,
            debug: options.debug,
            build: build_line(),
            rooms,
            seats: SavedSeats::default(),
            frame_log: options.frame_log,
            autoplay: options.autoplay,
            seed: options.seed,
        })
    }

    /// Single player on the shell's window: the arena prepares behind Battle Setup.
    async fn start(shell: Shell) -> Result<Self, String> {
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
            seed,
        } = shell;
        let target = render_target(&window)?;
        // The team is the seed's to draw.
        let extra_levels = debug || prefs.debug;
        let options = prefs.game_options(Team::Blue, extra_levels);
        let config = GameConfig {
            map: Some(options.map_mode),
            difficulty: Some(options.difficulty),
            extra_levels,
            human_kind: Some(options.human_kind),
            human_team: None,
            game_mode: Some(options.game_mode),
            autoplay,
            defer_arena: true,
        };
        let view = ViewConfig {
            seed,
            ..view_config(&window, &prefs)
        };
        let mut game = Game::create_with(target, view, config)
            .await
            .map_err(engine_error)?;
        let hud = game.hud().into_owned();
        game.set_speed_scale(SpeedSetting::TankSpeed, prefs.tank_speed);
        game.set_speed_scale(SpeedSetting::BulletSpeed, prefs.bullet_speed);
        let overlay = Overlay::new(window.clone(), game.renderer())?;
        let human_team = hud.human_team;
        let mut session = Self {
            human_team,
            window,
            game,
            overlay,
            input: WindowInput::default(),
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
            prepared_options: prefs.game_options(human_team, extra_levels),
            preparation: Preparation::default(),
            options_shown: None,
            in_round: false,
            phase: MatchPhase::Ready,
            alive: true,
            first_person: prefs.first_person,
            occluded: false,
            minimized: false,
            frame_shown: true,
            menu_shown: false,
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
        session.preparation.restart();
        Ok(session)
    }

    /// Give the window up: the game, its renderer and the overlay go, the rest stays.
    fn into_shell(self) -> Shell {
        crate::pointer_lock::release(&self.window);
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
            seed: None,
        }
    }

    /// The debug tools (extra levels, Stats for nerds): `--debug`, or Settings.
    fn debug_tools(&self) -> bool {
        self.debug || self.prefs.debug
    }

    /// Battle Setup's choices as the engine plays them.
    fn wanted_options(&self) -> GameOptions {
        self.prefs.game_options(self.human_team, self.debug_tools())
    }

    /// Handle a window event; true when the app should quit.
    fn window_event(&mut self, event: &WindowEvent) -> bool {
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
                self.input.controls.clear();
                self.hold_pointer();
            }
            _ => {}
        }
        false
    }

    /// Where the battle stands, for the window's input.
    fn battle(&self) -> Battle {
        let playing = self.in_round && self.phase == MatchPhase::Playing;
        Battle {
            keys: playing && self.ui.settings_draft.is_none(),
            playing: playing && self.alive,
            wheel: self.in_round,
            touch: Play::live(
                &self.overlay.context,
                self.touch_showing(),
                self.first_person,
                self.ui.hud.as_ref(),
            ),
        }
    }

    /// Which touch controls show in a running battle with no dialog open: all of them
    /// while the player can act, the zoom pair alone while the tank is a wreck.
    fn touch_showing(&self) -> Showing {
        if !self.in_round || self.phase != MatchPhase::Playing || self.ui.settings_draft.is_some() {
            Showing::Nothing
        } else if self.alive {
            Showing::All
        } else {
            Showing::Zoom
        }
    }

    /// Do what input asked; true when the app should quit.
    fn request(&mut self, request: Request) -> bool {
        match request {
            Request::Quit => true,
            Request::Shortcut(shortcut) => {
                self.shortcut(shortcut);
                false
            }
        }
    }

    /// Whether the window can show nothing: covered, or minimized.
    fn hidden(&self) -> bool {
        self.occluded || self.minimized
    }

    /// Like a hidden page, a hidden window pauses the round and lets the cursor go.
    fn hidden_changed(&mut self) {
        if self.hidden() {
            self.input.controls.clear();
            if self.in_round && self.phase == MatchPhase::Playing {
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
                } else if self.in_round && self.phase == MatchPhase::Playing {
                    self.apply(Action::Pause);
                } else if self.in_round && self.phase == MatchPhase::Paused {
                    self.apply(Action::Resume);
                }
            }
            Shortcut::ToggleView => {
                if self.in_round && self.phase == MatchPhase::Playing {
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
    pub fn resize(&mut self) {
        let (width, height, scale) = css_size(&self.window);
        self.game.resize(width, height, scale, false);
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Start => {
                if self.in_round && self.phase == MatchPhase::Results {
                    self.begin_round();
                } else {
                    self.ui.starting = true;
                    self.ui.status = "Preparing your arena…".into();
                    self.ui.progress = None;
                }
            }
            Action::Resume => {
                self.input.controls.clear();
                self.game.resume();
                self.refresh_hud();
            }
            Action::Pause => {
                if self.phase == MatchPhase::Playing {
                    self.game.pause();
                    self.input.controls.clear();
                    self.refresh_hud();
                }
            }
            Action::EndBattle => {
                self.game.end_battle();
                self.refresh_hud();
            }
            Action::BattleSetup => {
                self.input.controls.clear();
                self.game.restart();
                self.in_round = false;
                self.ui.reset_feedback();
                self.ui.starting = false;
                self.preparation.restart();
                self.refresh_hud();
            }
            Action::NextTank(kind) => {
                self.prefs.tank = kind;
                self.prefs.save();
                if let Err(error) = self.game.respawn_in(kind) {
                    report!("{}", engine_error(error));
                }
                self.prepared_options = self.wanted_options();
            }
            Action::SelectAmmo(slot) => {
                if self.phase == MatchPhase::Playing && self.alive {
                    self.input.controls.select_ammo(slot);
                }
            }
            Action::ToggleView => {
                self.first_person = self.game.toggle_first_person();
                self.save_camera();
            }
            Action::ToggleFullscreen => {
                let fullscreen = self.window.fullscreen().is_none();
                set_fullscreen(&self.window, &mut self.prefs, fullscreen);
            }
            Action::OpenSettings => {
                if self.in_round && self.phase == MatchPhase::Playing {
                    self.apply(Action::Pause);
                }
                ui::menus::open_settings(&mut self.ui, &self.prefs);
            }
            Action::SettingsSaved(prefs) => self.save_settings(*prefs),
            Action::Quit => {}
            Action::JoinRoom(selection) => {
                self.prefs.player_name.clone_from(&selection.choice.name);
                self.prefs.save();
                self.pending_room = Some(*selection);
            }
            // A room's own menu.
            Action::RoomStart
            | Action::RoomEnd
            | Action::RoomLeave
            | Action::RoomRejoin
            | Action::RoomSetup(_)
            | Action::Reload
            | Action::RoomChoose { .. }
            | Action::RoomRules(_) => {}
        }
    }

    /// Apply and keep changed settings and Battle Setup choices.
    fn save_settings(&mut self, prefs: Preferences) {
        let old = std::mem::replace(&mut self.prefs, prefs);
        if old.graphics != self.prefs.graphics {
            self.game.set_graphics_setting(self.prefs.graphics);
        }
        if old.tank_speed != self.prefs.tank_speed {
            self.prefs.tank_speed = self
                .game
                .set_speed_scale(SpeedSetting::TankSpeed, self.prefs.tank_speed);
        }
        if old.bullet_speed != self.prefs.bullet_speed {
            self.prefs.bullet_speed = self
                .game
                .set_speed_scale(SpeedSetting::BulletSpeed, self.prefs.bullet_speed);
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

    fn save_camera(&mut self) {
        self.prefs.save_camera(self.game.camera());
    }

    fn refresh_hud(&mut self) {
        let hud = self.game.hud().into_owned();
        self.phase = hud.match_state.phase;
        self.alive = hud.human.alive;
        self.ui.tick(self.hud_dt);
        self.hud_dt = 0.0;
        self.ui.hud = Some(hud);
    }

    fn begin_round(&mut self) {
        self.in_round = true;
        self.ui.starting = false;
        self.ui.reset_feedback();
        self.input.controls.clear();
        self.hud_dt = 0.0;
        self.game.start();
        crate::page::round_started();
        if self.audio.is_none() {
            self.audio = Audio::new();
            if let Some(audio) = &mut self.audio {
                audio.set_volume(self.prefs.volume);
            }
        }
        self.refresh_hud();
    }

    /// A slice of a redraw spent preparing the wanted arena behind Battle Setup: steps
    /// that build it or create pipelines follow each other until the slice is spent;
    /// one that only waits (for textures, background compiles, the GPU) ends it.
    fn prepare(&mut self) -> Result<(), String> {
        if !self.menu_shown {
            return Ok(());
        }
        let wanted = self.wanted_options();
        if wanted != self.prepared_options {
            // The rebuild's first step holds this thread for a moment: the player's new
            // choice, or GO's LOADING…, shows first, as the page waits for a paint.
            if self.options_shown.as_ref() != Some(&wanted) {
                self.options_shown = Some(wanted);
                return Ok(());
            }
            self.game.set_game_options(&wanted);
            self.prepared_options = wanted;
            self.preparation.restart();
            self.ui.hud = Some(self.game.hud().into_owned());
        }
        if !self.preparation.preparing {
            if self.ui.starting {
                self.begin_round();
            }
            return Ok(());
        }
        let until = now_ms() + PREPARE_SLICE_MS;
        loop {
            let building = self.game.building_arena();
            let status = self.prepare_step()?;
            let progressed = building || status.compiled > 0;
            if status.ready || !progressed || now_ms() >= until {
                return Ok(());
            }
        }
    }

    /// One `prepare` step, with the page's bake of a generated texture started or its
    /// pixels handed over first.
    fn prepare_step(&mut self) -> Result<PrepareStatus, String> {
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
                    self.game
                        .supply_pixels(&key, &pixels)
                        .map_err(engine_error)?;
                }
                Err(_) => self.game.release_texture_bake(&key),
            }
        }
        let status = self.game.prepare(PREPARE_BUDGET).map_err(engine_error)?;
        match self.preparation.advance(&status, self.game.loading()) {
            Progress::Ready => {
                self.ui.ready = true;
                self.ui.status = READY_STATUS.into();
                self.ui.progress = None;
                crate::page::arena_ready();
            }
            Progress::Warming => {}
            Progress::Loading(line) => {
                self.ui.ready = false;
                self.ui.status = line.to_string();
                self.ui.progress = line.progress();
            }
        }
        Ok(status)
    }

    /// One engine frame of the round; its events and HUD reach the screens.
    fn battle_frame(&mut self, now: f64) -> Result<(), String> {
        let input = self.input.take_input(self.first_person);
        let zoomed = input.zoom != 0.0;
        // `now` is the game's clock, which a check may hold (`automation.rs`).
        let engine_start = now_ms();
        let result = self.game.frame(now, &input.pack()).map_err(engine_error)?;
        self.parts.engine = now_ms() - engine_start;
        self.parts.sim = f64::from(result.get(frame_slot::SIM_MS).copied().unwrap_or(0.0));
        self.parts.render = f64::from(result.get(frame_slot::RENDER_MS).copied().unwrap_or(0.0));
        if let Some(error) = self.game.error() {
            return Err(error);
        }
        if zoomed {
            self.save_camera();
        }
        let slot = |index: usize| result.get(index).copied().unwrap_or(0.0);
        self.phase = frame_slot::phase(slot(frame_slot::PHASE));
        self.alive = slot(frame_slot::HUMAN_ALIVE) == 1.0;
        self.first_person = slot(frame_slot::FIRST_PERSON) == 1.0;
        self.ui.cockpit = slot(frame_slot::COCKPIT) == 1.0;
        self.ui.hull_angle = f64::from(slot(frame_slot::HULL_ANGLE));
        let viewer = (slot(frame_slot::VIEWER_X), slot(frame_slot::VIEWER_Y));
        self.ui.viewer = (viewer.0.is_finite() && viewer.1.is_finite()).then_some(viewer);
        if slot(frame_slot::CLEAR_INPUT) == 1.0 {
            self.input.controls.clear();
        }
        if slot(frame_slot::EVENTS) > 0.0 {
            let batch = self.game.take_events();
            if let Some(audio) = &mut self.audio {
                audio.play(&batch);
            }
            // A death names its killer, who may have joined since the last refresh.
            if batch
                .events
                .iter()
                .any(|pending| pending.event.kind == SimEventType::Death)
            {
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

    /// A dialog dims the arena this frame (pause, the results, Settings during a round),
    /// which the page also blurred behind it. Battle Setup hides the arena.
    fn dialog_over_arena(&self) -> bool {
        self.in_round
            && (matches!(self.phase, MatchPhase::Paused | MatchPhase::Results)
                || self.ui.settings_draft.is_some())
    }

    /// Grab the cursor while first person steers, so the mouse turns the view; a death
    /// keeps it (`pointer_lock.rs`).
    fn hold_pointer(&mut self) {
        let wanted = self.in_round
            && self.first_person
            && self.phase == MatchPhase::Playing
            && self.ui.settings_draft.is_none()
            && self.window.has_focus()
            && !self.hidden();
        self.input
            .hold_pointer_steering(&self.window, wanted, self.alive);
        self.ui.aim_hint = self.input.aim_waits_for_click();
    }

    /// What a check reads of this frame (`automation.rs`).
    fn automation_state(&mut self) -> automation::State {
        let touch = self.input.controls.touch;
        automation::State {
            engine: "single",
            in_round: self.in_round,
            phase: automation::phase_name(self.phase),
            alive: self.alive,
            first_person: self.first_person,
            aim_hint: self.ui.aim_hint,
            multiplayer_tab: self.ui.rooms.multiplayer,
            ready: self.ui.ready,
            status: self.ui.status.clone(),
            settings: self.ui.settings_draft.is_some(),
            touch_controls: self.ui.touch.is_some_and(|touch| touch.acting),
            touch_screen: crate::touch::screen(),
            touch_layout: crate::touch::screen()
                .then(|| (&crate::touch::TouchLayout::of(&self.overlay.context)).into()),
            stick_push: touch.drive.map(|_| [touch.push.0, touch.push.1]),
            arena_finger: touch.arena.is_some(),
            failure: self.failure.clone(),
            error: self.game.error(),
            prefs: serde_json::to_value(&self.prefs).unwrap_or_default(),
            parts: Some(serde_json::json!({
                "engine": self.parts.engine,
                "sim": self.parts.sim,
                "render": self.parts.render,
                "overlay": self.parts.overlay,
                "present": self.parts.present,
            })),
            ..automation::State::default()
        }
    }

    /// Run a check's debug command on the engine, as the old page's `window.sloppy` did.
    fn command(&mut self, command: &Command) -> Result<serde_json::Value, String> {
        use serde_json::Value;
        if *command == Command::Start {
            self.apply(Action::Start);
            return Ok(Value::Null);
        }
        let game = &mut self.game;
        Ok(match *command {
            Command::Start => Value::Null,
            Command::Autoplay(on) => game.debug_set_autoplay(on).into(),
            Command::Overview(on) => {
                game.debug_set_overview(on);
                Value::Null
            }
            Command::AutoRounds(on) => game.debug_set_auto_rounds(on).into(),
            Command::Zoom(zoom) => game.debug_set_zoom(zoom).into(),
            Command::Reflections(strength) => {
                game.debug_set_reflections(strength);
                Value::Null
            }
            Command::Stress => {
                game.debug_stress();
                Value::Null
            }
            Command::Collapse => {
                game.debug_collapse();
                Value::Null
            }
            Command::GiveAmmo(count) => {
                game.debug_give_ammo(count);
                Value::Null
            }
            Command::KillHuman => {
                game.debug_kill_human();
                Value::Null
            }
            Command::Soak(seconds) => serde_json::from_str(&game.debug_soak(seconds))
                .map_err(|error| error.to_string())?,
            Command::Off(ref off) => {
                game.debug_set_off(off).map_err(engine_error)?;
                Value::Null
            }
            Command::Latency { .. } => return Err("single player has no connection".into()),
        })
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
        // A window that takes no frames only looks for its window again: engine frames
        // there would count as slow ones and lower Auto's graphics quality.
        if !self.frame_shown && self.in_round && self.phase == MatchPhase::Playing {
            self.apply(Action::Pause);
        }
        if self.failure.is_none() && (self.frame_shown || !self.in_round) {
            let game = &mut self.game;
            self.preparation.prebuild(|| game.prebuild_step());
            let step = if self.in_round {
                self.battle_frame(now)
            } else {
                self.prepare()
            };
            if let Err(error) = step {
                report!("The game stopped: {error}");
                // A page offers a reload over the shell's own words.
                crate::page::failed(&format!("The renderer stopped: {error}"));
                self.failure = Some(error);
            }
        }
        if !self.in_round {
            self.ui.tick(frame_dt);
        } else if self.phase != MatchPhase::Playing {
            self.ui.tick(0.0);
        }
        if self.ui.stats_due(now, self.debug_tools()) {
            self.ui.stats = Some(self.game.nerd_stats());
        }
        self.hold_pointer();
        self.ui.touch = self
            .input
            .touch_view(self.touch_showing(), self.first_person);
        let view = ui::View {
            in_round: self.in_round,
            phase: self.phase,
            prefs: &self.prefs,
            build: &self.build,
            debug: self.debug_tools(),
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
        self.game.set_backdrop_blur(self.dialog_over_arena());
        self.overlay.present(self.game.renderer(), painted);
        self.frame_shown = self.game.renderer().frame_shown();
        self.parts.present = now_ms() - present_start;
        if self.frame_shown {
            self.menu_shown = true;
            crate::page::frame_presented();
            if automation::enabled() {
                let state = self.automation_state();
                let game = &mut self.game;
                automation::presented(state, |extra| {
                    let json = match extra {
                        Extra::Hud => game.hud_json(),
                        Extra::Stats => game.stats_json(),
                        Extra::Nerd => game.nerd_stats_json(),
                        Extra::Debug => game.debug_json(),
                        Extra::Snapshot => game.debug_snapshot(),
                    };
                    serde_json::from_str(&json).unwrap_or_default()
                });
            }
        }
        if let Some(log) = &mut self.frame_log {
            let scene = self.game.renderer().scene_size();
            log.frame(&self.parts, target, scene, self.game.gpu_ms());
        }
        for action in actions {
            quit |= action == Action::Quit;
            self.apply(action);
        }
        quit
    }
}

/// A resizable window at its first size, or fullscreen as the player left it.
#[cfg(not(any(target_os = "ios", target_arch = "wasm32")))]
fn window_attributes(prefs: &Preferences) -> WindowAttributes {
    let attributes = Window::default_attributes()
        .with_title("Sloppy Tanks")
        .with_inner_size(WINDOW_SIZE)
        .with_min_inner_size(MIN_WINDOW_SIZE)
        .with_window_icon(window_icon());
    if prefs.fullscreen {
        attributes.with_fullscreen(Some(Fullscreen::Borderless(None)))
    } else {
        attributes
    }
}

/// The iPad's whole screen, held sideways (the menus and HUD need a landscape
/// window), with no status bar; the system's edge swipes wait for a second swipe, so a
/// thumb on the stick or the toolbar stays in the game.
#[cfg(target_os = "ios")]
fn window_attributes(_prefs: &Preferences) -> WindowAttributes {
    use winit::platform::ios::{ScreenEdge, ValidOrientations, WindowAttributesExtIOS};
    Window::default_attributes()
        .with_valid_orientations(ValidOrientations::Landscape)
        .with_prefers_status_bar_hidden(true)
        .with_prefers_home_indicator_hidden(true)
        .with_preferred_screen_edges_deferring_system_gestures(ScreenEdge::ALL)
}

/// A canvas filling the page (`index.html` sizes it). Fullscreen waits for the
/// player's click: a page may not take the screen by itself.
#[cfg(target_arch = "wasm32")]
fn window_attributes(_prefs: &Preferences) -> WindowAttributes {
    use winit::platform::web::WindowAttributesExtWebSys;
    Window::default_attributes()
        .with_title("Sloppy Tanks")
        .with_append(true)
}

/// The window's icon (Windows and Linux show it; macOS takes the bundle's).
#[cfg(not(any(target_os = "ios", target_arch = "wasm32")))]
fn window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(crate::assets::file("icons/icon-192.png")?).ok()?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(compiled: u32, remaining: u32, ready: bool) -> PrepareStatus {
        PrepareStatus {
            compiled,
            remaining,
            ready,
            ..PrepareStatus::default()
        }
    }

    /// Single player builds the library's wrecks once the arena is ready and its menu
    /// has painted, a step a redraw, as the page does; before, nothing built them until
    /// a battle needed one.
    #[test]
    fn the_wrecks_build_after_the_ready_arena_a_step_a_redraw() {
        let mut preparation = Preparation::default();
        preparation.restart();
        let steps = std::cell::Cell::new(0);
        let redraw = |preparation: &mut Preparation, more: bool| {
            preparation.prebuild(|| {
                steps.set(steps.get() + 1);
                more
            });
        };
        let shaders = Loading::Shaders {
            loaded: 4,
            total: 10,
        };
        assert_eq!(
            preparation.advance(&status(4, 6, false), Some(shaders)),
            Progress::Loading(shaders)
        );
        redraw(&mut preparation, true);
        assert_eq!(
            preparation.advance(&status(6, 0, true), None),
            Progress::Ready
        );
        assert!(!preparation.preparing);
        for more in [true, true, false, true] {
            redraw(&mut preparation, more);
        }
        assert_eq!(
            steps.get(),
            3,
            "three steps after the ready arena, then none"
        );
        // A new arena does not build them again.
        preparation.restart();
        redraw(&mut preparation, true);
        assert_eq!(steps.get(), 3);
    }

    #[test]
    fn a_rebuild_that_only_warms_up_keeps_battle_setup_ready() {
        let mut preparation = Preparation::default();
        preparation.restart();
        // The engine has no line for a step that only warms up.
        assert_eq!(
            preparation.advance(&status(0, 0, false), None),
            Progress::Warming
        );
        assert!(preparation.preparing);
    }
}
