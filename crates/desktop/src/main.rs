//! Sloppy Tanks on macOS, Windows and iPad: the game in a native window, drawn with
//! wgpu on Metal or Direct3D 12, its menus and HUD in egui. No browser: the shell
//! replaces the page's TypeScript (`src/`) and drives the same Rust `Game` the page
//! does (`crates/web`). The iPad plays it with touch controls (`touch.rs`).

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
mod preferences;
mod room;
mod room_api;
mod sockets;
mod surface_layers;
mod touch;
mod ui;

/// The iPad plays by touch (a hardware keyboard too), with no pointer lock, window
/// size or Quit; Macs and PCs play with keyboard and mouse.
pub const TOUCH: bool = cfg!(target_os = "ios");

fn main() {
    sloppy_render::gpu::set_asset_files(assets::file);
    if let Some(directory) = preferences::data_directory() {
        sloppy_web::set_records_file(directory.join("records.json"));
    }
    let arguments: Vec<String> = std::env::args().collect();
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
    let mut app = app::App::new(options);
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("Sloppy Tanks stopped: {error}");
    }
}
