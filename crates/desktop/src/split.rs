//! The page in two halves: Battle Setup (`menu.rs`) from the menu half of the binary
//! until `index.html` has instantiated the engine half (`window.sloppyEngine`), then the
//! app (`app.rs`) on the same window, canvas and screens.
//!
//! The menu half holds what a `menu-only` build of this file reaches, which leaves the
//! app out; the only calls into it are in [`engine`], kept out of line so the build's
//! split stops at them (`scripts/build-wasm.mjs`).

#[cfg(not(feature = "menu-only"))]
use std::mem::ManuallyDrop;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

#[cfg(not(feature = "menu-only"))]
use crate::app::App;
use crate::app::{Options, new_shell, startup_failed};
use crate::menu::Menu;
use crate::tasks::Task;

enum Stage {
    /// The window is not open yet.
    Closed,
    /// The menu is asking for its device.
    Opening(Task<Result<Menu, String>>),
    Menu(Box<Menu>),
    /// The app has the window for the rest of the page's life: no drop, whose glue would
    /// reach into the engine half from the menu's.
    #[cfg(not(feature = "menu-only"))]
    Engine(ManuallyDrop<App>),
    /// The menu could not start; the page says why.
    Failed,
}

pub struct Page {
    options: Option<Options>,
    stage: Stage,
}

impl Page {
    pub fn new(options: Options) -> Self {
        Self {
            options: Some(options),
            stage: Stage::Closed,
        }
    }

    /// Show the menu once its device is ready.
    fn opened(&mut self) {
        let Stage::Opening(task) = &self.stage else {
            return;
        };
        match task.take() {
            None => {}
            Some(Ok(menu)) => {
                menu.window().request_redraw();
                self.stage = Stage::Menu(Box::new(menu));
            }
            Some(Err(error)) => {
                startup_failed(&error);
                self.stage = Stage::Failed;
            }
        }
    }

    /// Hand the window to the app once the engine half has arrived.
    #[cfg(not(feature = "menu-only"))]
    fn hand_over(&mut self) {
        if !matches!(self.stage, Stage::Menu(_)) || !engine_arrived() {
            return;
        }
        let Stage::Menu(menu) = std::mem::replace(&mut self.stage, Stage::Failed) else {
            return;
        };
        let Some(options) = self.options.take() else {
            return;
        };
        let (shell, room) = menu.into_shell();
        self.stage = Stage::Engine(ManuallyDrop::new(engine::adopt(options, shell, room)));
    }

    #[cfg(feature = "menu-only")]
    fn hand_over(&mut self) {}
}

/// `index.html` has instantiated the engine half over the menu half's exports.
#[cfg(not(feature = "menu-only"))]
fn engine_arrived() -> bool {
    js_sys::Reflect::get(&js_sys::global(), &"sloppyEngine".into())
        .is_ok_and(|value| value.is_truthy())
}

impl ApplicationHandler for Page {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match &mut self.stage {
            Stage::Closed => {}
            #[cfg(not(feature = "menu-only"))]
            Stage::Engine(app) => return engine::resumed(app, event_loop),
            _ => return,
        }
        let Some(options) = &self.options else {
            return;
        };
        self.stage = match new_shell(event_loop, options) {
            Ok(shell) => {
                let window = shell.window.clone();
                Stage::Opening(Task::on_main_thread(async move {
                    let menu = Menu::open(shell).await;
                    // A page waits for no event: this one takes the answer, either way.
                    window.request_redraw();
                    menu
                }))
            }
            Err(error) => {
                startup_failed(&error);
                Stage::Failed
            }
        };
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window: WindowId, event: WindowEvent) {
        self.opened();
        match &mut self.stage {
            Stage::Menu(menu) => menu.window_event(&event),
            #[cfg(not(feature = "menu-only"))]
            Stage::Engine(app) => engine::window_event(app, event_loop, window, event),
            _ => {}
        }
        #[cfg(feature = "menu-only")]
        let _ = (event_loop, window);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, device: DeviceId, event: DeviceEvent) {
        #[cfg(not(feature = "menu-only"))]
        if let Stage::Engine(app) = &mut self.stage {
            engine::device_event(app, event_loop, device, event);
        }
        #[cfg(feature = "menu-only")]
        let _ = (event_loop, device, event);
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        #[cfg(not(feature = "menu-only"))]
        if let Stage::Engine(app) = &mut self.stage {
            engine::new_events(app, event_loop, cause);
        }
        #[cfg(feature = "menu-only")]
        let _ = (event_loop, cause);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(not(feature = "menu-only"))]
        if let Stage::Engine(app) = &mut self.stage {
            engine::suspended(app, event_loop);
        }
        #[cfg(feature = "menu-only")]
        let _ = event_loop;
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(not(feature = "menu-only"))]
        if let Stage::Engine(app) = &mut self.stage {
            engine::exiting(app, event_loop);
        }
        #[cfg(feature = "menu-only")]
        let _ = event_loop;
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.opened();
        self.hand_over();
        match &mut self.stage {
            // The backdrop moves: the menu draws every display frame.
            Stage::Menu(menu) => {
                event_loop.set_control_flow(ControlFlow::Wait);
                menu.window().request_redraw();
            }
            #[cfg(not(feature = "menu-only"))]
            Stage::Engine(app) => engine::about_to_wait(app, event_loop),
            _ => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// The only calls from the menu half into the app, each out of line so the split stops
/// at it (`scripts/build-wasm.mjs` keeps nothing past `split::engine::`). They run only
/// once the engine half has arrived.
#[cfg(not(feature = "menu-only"))]
mod engine {
    use super::*;
    use crate::app::Shell;
    use crate::room_api::RoomSelection;

    #[inline(never)]
    pub fn adopt(options: Options, shell: Shell, room: Option<RoomSelection>) -> App {
        App::adopt(options, shell, room)
    }

    #[inline(never)]
    pub fn resumed(app: &mut App, event_loop: &ActiveEventLoop) {
        app.resumed(event_loop);
    }

    #[inline(never)]
    pub fn window_event(
        app: &mut App,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        event: WindowEvent,
    ) {
        app.window_event(event_loop, window, event);
    }

    #[inline(never)]
    pub fn device_event(
        app: &mut App,
        event_loop: &ActiveEventLoop,
        device: DeviceId,
        event: DeviceEvent,
    ) {
        app.device_event(event_loop, device, event);
    }

    #[inline(never)]
    pub fn new_events(app: &mut App, event_loop: &ActiveEventLoop, cause: StartCause) {
        app.new_events(event_loop, cause);
    }

    #[inline(never)]
    pub fn suspended(app: &mut App, event_loop: &ActiveEventLoop) {
        app.suspended(event_loop);
    }

    #[inline(never)]
    pub fn exiting(app: &mut App, event_loop: &ActiveEventLoop) {
        app.exiting(event_loop);
    }

    #[inline(never)]
    pub fn about_to_wait(app: &mut App, event_loop: &ActiveEventLoop) {
        app.about_to_wait(event_loop);
    }
}
