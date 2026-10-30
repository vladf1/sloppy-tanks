//! Sloppy Tanks on macOS, Windows, iPad and the web: the game's menus and HUD in egui
//! over the engine's `Game` and `NetGame` (`crates/web`), in a native window drawn with
//! wgpu on Metal or Direct3D 12, or built for `wasm32` (`pnpm run wasm`) as the web
//! version's page (`index.html`) on the browser's WebGPU. The iPad plays it with touch
//! controls (`touch.rs`).

// A Windows release build opens no console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod audio;
mod bakes;
mod directory;
mod frame_log;
mod hardware_keys;
mod input;
#[cfg(target_os = "ios")]
mod ios;
mod overlay;
mod page;
mod preferences;
mod room;
mod room_api;
#[cfg_attr(target_arch = "wasm32", path = "sockets_web.rs")]
mod sockets;
mod surface_layers;
mod tasks;
mod touch;
mod ui;
mod window_input;

/// The app may quit itself: not on an iPad, nor in a page.
pub const QUITS: bool = cfg!(not(any(target_os = "ios", target_arch = "wasm32")));

fn main() {
    // A page fetches the textures from its site and keeps records in `localStorage`.
    #[cfg(not(target_arch = "wasm32"))]
    {
        sloppy_render::gpu::set_asset_files(assets::file);
        if let Some(directory) = preferences::data_directory() {
            sloppy_web::set_records_file(directory.join("records.json"));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        // A bake worker runs this module too (`bakes.rs`), only to bake.
        if web_sys::window().is_none() {
            console_error_panic_hook::set_once();
            return;
        }
        // A panic stops the game (a release build aborts): the console has the details,
        // and the page says so in place of the game.
        std::panic::set_hook(Box::new(|info| {
            console_error_panic_hook::hook(info);
            page::failed(&format!("The game stopped: {info}"));
        }));
        touch::detect();
    }
    let arguments = arguments();
    let flag = |name: &str| arguments.iter().any(|argument| argument == name);
    let options = app::Options {
        debug: flag("--debug"),
        frame_log: flag("--frame-log"),
        autoplay: flag("--autoplay"),
        directory: arguments
            .iter()
            .position(|argument| argument == "--directory")
            .and_then(|index| arguments.get(index + 1).cloned()),
    };
    let event_loop = winit::event_loop::EventLoop::new().expect("an event loop");
    let app = app::App::new(options);
    run(event_loop, app);
}

/// The command line; in a page its query, `?debug&directory=<url>` reading as
/// `--debug --directory <url>`.
fn arguments() -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let query = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        let mut arguments = vec![String::new()];
        for pair in query.trim_start_matches('?').split('&') {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            if !name.is_empty() {
                arguments.push(format!("--{name}"));
            }
            if let Ok(value) = js_sys::decode_uri_component(value)
                && value.length() > 0
            {
                arguments.push(value.into());
            }
        }
        arguments
    }
}

/// Run the app until it quits.
#[cfg(not(target_arch = "wasm32"))]
fn run(event_loop: winit::event_loop::EventLoop<()>, mut app: app::App) {
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("Sloppy Tanks stopped: {error}");
    }
}

/// Hand the app to the page's event loop; `main` returns at once.
#[cfg(target_arch = "wasm32")]
fn run(event_loop: winit::event_loop::EventLoop<()>, app: app::App) {
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(app);
}
