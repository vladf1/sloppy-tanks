//! The page's Battle Setup before its engine has arrived. The page's binary comes in two
//! halves (`scripts/build-wasm.mjs` splits it with binaryen's wasm-split): this menu,
//! which shows first, and the engine (the simulation, bots, maps and renderer), which
//! `index.html` downloads behind it. Battle Setup, Settings and the Multiplayer tab work
//! meanwhile; GO and JOIN wait for the engine. The menu opens the device and canvas the
//! renderer will draw on and draws egui on them; `split.rs` then hands those, the egui
//! context and the screens' state to the app ([`Carried`]), so the engine's first frame
//! carries on from the menu's last.
//!
//! Nothing here may reach the engine: the build keeps in the menu half only what a
//! menu-only build of this file reaches (`menu-only`), and a call into the engine half
//! before it has arrived throws (`scripts/split-check.mjs` plays the menu that way).

use std::sync::Arc;

use sloppy_core::sim::MatchPhase;
use sloppy_core::sim::game_options::initial_game_options;
use sloppy_render::gpu::GpuCanvas;
use sloppy_render::presentation::view_settings::CAMERA;
use sloppy_web::hud::Hud;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::app::{
    Carried, PREPARING_STATUS, Shell, css_size, now_ms, render_target, set_fullscreen,
};
use crate::automation;
use crate::input::Shortcut;
use crate::overlay::Overlay;
use crate::room_api::RoomSelection;
use crate::touch::{Play, Showing};
use crate::ui::{self, Action, UiState};
use crate::window_input::{Battle, Request, WindowInput};

/// Battle Setup's status while the engine downloads.
const DOWNLOADING_STATUS: &str = "Loading the game…";

/// Battle Setup on the page's canvas, with no engine.
pub struct Menu {
    shell: Shell,
    canvas: GpuCanvas,
    overlay: Overlay,
    input: WindowInput,
    ui: UiState,
    /// The first arena's seed, drawn here to show the team it gives the player.
    seed: f64,
    /// JOIN ROOM's or CREATE ROOM's room, joined once the engine arrives.
    room: Option<RoomSelection>,
    last_frame: f64,
}

impl Menu {
    /// Open the window's canvas and draw Battle Setup on it.
    pub async fn open(mut shell: Shell) -> Result<Self, String> {
        let canvas = GpuCanvas::open(render_target(&shell.window)?).await?;
        let overlay = Overlay::on_canvas(shell.window.clone(), &canvas)?;
        // The engine draws the player's team from the seed (`initial_game_options`); the
        // menu draws the seed, so its tanks show the team the arena will give.
        let seed = shell.seed.unwrap_or_else(sloppy_web::random_seed);
        let team = initial_game_options(seed, None, false, None).human_team;
        let ui = UiState {
            hud: Some(Hud {
                human_team: team,
                ..Hud::default()
            }),
            status: DOWNLOADING_STATUS.into(),
            progress: Some(0.0),
            rooms: std::mem::take(&mut shell.rooms),
            ..UiState::default()
        };
        // A page waits for no event: the first frame wakes it.
        shell.window.request_redraw();
        Ok(Self {
            shell,
            canvas,
            overlay,
            input: WindowInput::default(),
            ui,
            seed,
            room: None,
            last_frame: now_ms(),
        })
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.shell.window
    }

    /// The shell for the engine: the canvas, the egui context and the screens carried
    /// over, the seed the menu drew and the room it chose.
    pub fn into_shell(self) -> (Shell, Option<RoomSelection>) {
        let Self {
            mut shell,
            canvas,
            overlay,
            mut ui,
            seed,
            room,
            ..
        } = self;
        shell.rooms = std::mem::take(&mut ui.rooms);
        shell.seed = Some(seed);
        if !ui.starting {
            ui.status.clear();
            ui.progress = None;
        }
        shell.carried = Some(Carried {
            canvas,
            overlay,
            ui,
        });
        (shell, room)
    }

    pub fn window_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::RedrawRequested = event {
            self.redraw();
            return;
        }
        let battle = Battle {
            keys: false,
            playing: false,
            wheel: false,
            touch: Play::live(&self.overlay.context, Showing::Nothing, false, None),
        };
        match self
            .input
            .event(&self.shell.window, &mut self.overlay, event, &battle)
        {
            Some(Request::Shortcut(Shortcut::Escape)) => self.ui.settings_draft = None,
            Some(Request::Shortcut(Shortcut::Fullscreen)) => self.apply(Action::ToggleFullscreen),
            Some(Request::Shortcut(Shortcut::NerdStats)) => {
                if self.shell.debug || self.shell.prefs.debug {
                    self.ui.show_nerd_stats = !self.ui.show_nerd_stats;
                }
            }
            Some(Request::Shortcut(Shortcut::ToggleView) | Request::Quit) | None => {}
        }
        if let WindowEvent::Focused(false) = event {
            self.input.controls.clear();
        }
    }

    fn redraw(&mut self) {
        let now = automation::now();
        let dt = ((now - self.last_frame) / 1000.0).clamp(0.0, 0.1);
        self.last_frame = now;
        self.ui.tick(dt);
        if !self.ui.starting {
            self.ui.progress = Some(engine_progress());
        }
        // The drawing buffer as the renderer sizes it (`CanvasSize::canvas_ratio`).
        let (width, height, scale) = css_size(&self.shell.window);
        let ratio = scale.min(CAMERA.max_canvas_pixel_ratio);
        let pixels = |side: f64| (side * ratio).round().max(1.0) as u32;
        self.canvas.resize((pixels(width), pixels(height)));
        let prefs = &self.shell.prefs;
        let view = ui::View {
            in_round: false,
            phase: MatchPhase::Ready,
            prefs,
            build: &self.shell.build,
            debug: self.shell.debug || prefs.debug,
            room: false,
        };
        let ui_state = &mut self.ui;
        let mut actions = Vec::new();
        let painted = self
            .overlay
            .run(|context| actions = ui::draw_setup(context, ui_state, &view));
        self.overlay.present_on(&mut self.canvas, painted);
        if self.canvas.frame_shown() {
            crate::page::frame_presented();
            if automation::enabled() {
                automation::presented(self.automation_state(), |_| serde_json::Value::Null);
            }
        }
        for action in actions {
            self.apply(action);
        }
    }

    /// The hook's state: the menu, never ready, with the choices so far.
    fn automation_state(&self) -> automation::State {
        automation::State {
            engine: "menu",
            phase: automation::phase_name(MatchPhase::Ready),
            alive: true,
            multiplayer_tab: self.ui.rooms.multiplayer,
            status: self.ui.status.clone(),
            settings: self.ui.settings_draft.is_some(),
            touch_screen: crate::touch::screen(),
            touch_layout: crate::touch::screen()
                .then(|| (&crate::touch::TouchLayout::of(&self.overlay.context)).into()),
            error: self.canvas.error(),
            prefs: serde_json::to_value(&self.shell.prefs).unwrap_or_default(),
            team: self
                .ui
                .hud
                .as_ref()
                .map(|hud| automation::team_name(hud.human_team)),
            ..automation::State::default()
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            // The round starts once the engine has built the arena.
            Action::Start => {
                self.ui.starting = true;
                self.ui.status = PREPARING_STATUS.into();
                self.ui.progress = None;
            }
            Action::OpenSettings => ui::menus::open_settings(&mut self.ui, &self.shell.prefs),
            // The engine starts with the saved graphics, speeds and volume.
            Action::SettingsSaved(prefs) => {
                let fullscreen = prefs.fullscreen != self.shell.prefs.fullscreen;
                self.shell.prefs = *prefs;
                if fullscreen {
                    let wanted = self.shell.prefs.fullscreen;
                    set_fullscreen(&self.shell.window, &mut self.shell.prefs, wanted);
                }
                self.shell.prefs.save();
            }
            Action::ToggleFullscreen => {
                let fullscreen = self.shell.window.fullscreen().is_none();
                set_fullscreen(&self.shell.window, &mut self.shell.prefs, fullscreen);
            }
            Action::JoinRoom(selection) => {
                self.shell
                    .prefs
                    .player_name
                    .clone_from(&selection.choice.name);
                self.shell.prefs.save();
                self.room = Some(*selection);
            }
            // A round's and a room's actions have no screen here.
            _ => {}
        }
    }
}

/// The share of the engine half `index.html` has downloaded (`sloppyEngineProgress`).
fn engine_progress() -> f32 {
    js_sys::Reflect::get(&js_sys::global(), &"sloppyEngineProgress".into())
        .ok()
        .and_then(|value| value.as_f64())
        .map_or(0.0, |share| share.clamp(0.0, 1.0) as f32)
}
