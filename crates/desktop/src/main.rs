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
mod automation;
mod bakes;
mod directory;
mod frame_log;
mod hardware_keys;
mod input;
#[cfg(target_os = "ios")]
mod ios;
mod links;
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

use sloppy_core::net::protocol::read_room_code;
use sloppy_core::sim::map_options::MapId;

use crate::room::NetworkTools;

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
    if arguments.iter().any(|argument| argument == "--automation") {
        automation::enable();
    }
    let options = options(&arguments, links::development());
    let event_loop = winit::event_loop::EventLoop::new().expect("an event loop");
    let app = app::App::new(options);
    run(event_loop, app);
}

/// The command line; in a page its query, `?debug&directory=<url>` reading as
/// `--debug --directory <url>` (`links::query_arguments`).
fn arguments() -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().skip(1).collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let query = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        links::query_arguments(query.trim_start_matches('?'))
    }
}

/// What the command line or a page's link asks for: the page's flags (`?debug`,
/// `?autoplay`, `?map=<id>`, `?multiplayer`, `?room=<CODE>`) and, with `development`
/// (`links::development`), `?directory=<url>` and the rooms' network tools
/// (`?latency=&jitter=&stall=` in milliseconds, `?dodge=0`, `?shooter=0`).
fn options(arguments: &[String], development: bool) -> app::Options {
    let flag = |name: &str| arguments.iter().any(|argument| argument == name);
    let value = |name: &str| {
        arguments
            .iter()
            .position(|argument| argument == name)
            .and_then(|index| arguments.get(index + 1))
            .filter(|value| !value.starts_with("--"))
            .cloned()
    };
    let tool = |name: &str| value(name).filter(|_| development);
    app::Options {
        debug: flag("--debug"),
        frame_log: flag("--frame-log"),
        autoplay: flag("--autoplay"),
        directory: tool("--directory"),
        map: value("--map").and_then(|id| MapId::parse(&id)),
        multiplayer: flag("--multiplayer"),
        // In any case; a code that cannot be one is still named in the room list's answer.
        room: value("--room")
            .map(|code| read_room_code(&code).unwrap_or_else(|| code.trim().to_uppercase())),
        network: NetworkTools {
            latency: tool("--latency"),
            jitter: tool("--jitter"),
            stall: tool("--stall"),
            dodge: tool("--dodge"),
            shooter: tool("--shooter"),
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn link(query: &str, development: bool) -> app::Options {
        options(&links::query_arguments(query), development)
    }

    #[test]
    fn a_link_opens_its_map_tab_or_room() {
        let invite = link("room=abcd2345&debug", false);
        assert_eq!(invite.room.as_deref(), Some("ABCD2345"));
        assert!(invite.debug && !invite.multiplayer);
        assert_eq!(link("room=%20x%20", false).room.as_deref(), Some("X"));
        assert_eq!(link("room=", false).room, None);

        let tab = link("multiplayer=&map=stress-test", false);
        assert!(tab.multiplayer && tab.room.is_none());
        assert_eq!(tab.map, Some(MapId::StressTest));
        assert_eq!(link("map=atlantis", false).map, None);
        assert!(link("autoplay", false).autoplay);
    }

    /// The directory and the network tools are a developer's: a production page's link
    /// cannot point its rooms elsewhere or slow them down.
    #[test]
    fn the_development_parameters_need_a_development_run() {
        let query = "directory=http%3A%2F%2F127.0.0.1%3A8784&latency=120&jitter=10&stall=200\
                     &dodge=0&shooter=0";
        let production = link(query, false);
        assert_eq!(production.directory, None);
        assert_eq!(production.network, NetworkTools::default());
        let development = link(query, true);
        assert_eq!(
            development.directory.as_deref(),
            Some("http://127.0.0.1:8784")
        );
        assert_eq!(
            development.network,
            NetworkTools {
                latency: Some("120".into()),
                jitter: Some("10".into()),
                stall: Some("200".into()),
                dodge: Some("0".into()),
                shooter: Some("0".into()),
            }
        );
        // A flag with no value takes none from the next one.
        assert_eq!(link("directory&debug", true).directory, None);
    }
}
