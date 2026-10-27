//! Sloppy Tanks on macOS and Windows: the game in a native window, drawn with wgpu on
//! Metal, Direct3D 12 or Vulkan, its menus and HUD in egui. No browser: the shell
//! replaces the page's TypeScript (`src/`) and drives the same Rust `Game` the page
//! does (`crates/web`).

// A Windows release build opens no console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod audio;
mod bakes;
mod engine_api;
mod input;
mod overlay;
mod preferences;
mod ui;

fn main() {
    sloppy_render::gpu::set_asset_files(assets::file);
    if let Some(directory) = preferences::data_directory() {
        sloppy_web::set_records_file(directory.join("records.json"));
    }
    let options = app::Options {
        debug: std::env::args().any(|argument| argument == "--debug"),
    };
    let event_loop = winit::event_loop::EventLoop::new().expect("an event loop");
    let mut app = app::App::new(options);
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("Sloppy Tanks stopped: {error}");
    }
}
