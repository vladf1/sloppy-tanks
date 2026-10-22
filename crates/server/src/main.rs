//! The multiplayer server binary: settings from the environment, the game's site bundle
//! when `SITE_DIR` names one, a multi-threaded Tokio runtime, and a graceful reset of
//! every room on SIGTERM (what `docker stop` sends) or SIGINT.

use std::process::ExitCode;
use std::sync::Arc;

use sloppy_server::config::Settings;
use sloppy_server::match_room::MatchRoom;
use sloppy_server::process_stats::CountingAllocator;
use sloppy_server::protocol::CONTENT_VERSION;
use sloppy_server::server::{MultiplayerServer, ServerOptions};
use sloppy_server::site::Site;
use tokio::signal::unix::{SignalKind, signal};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const MB: f64 = 1024.0 * 1024.0;

fn main() -> ExitCode {
    let settings = match Settings::from_env(|name| std::env::var(name).ok()) {
        Ok(settings) => settings,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    // Read before serving anything, so a bad bundle stops startup and the deploy's health
    // check rolls back.
    let site = match &settings.site_dir {
        None => None,
        Some(directory) => match Site::load(directory, &settings.build) {
            Ok(site) => {
                println!(
                    "Serving the game from {} ({} files, {:.1} MB in memory, site {})",
                    directory.display(),
                    site.file_count(),
                    site.stored_bytes() as f64 / MB,
                    site.build()
                );
                Some(Arc::new(site))
            }
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        },
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime");
    runtime.block_on(serve(settings, site))
}

async fn serve(settings: Settings, site: Option<Arc<Site>>) -> ExitCode {
    let mut options = ServerOptions::new(settings.trust_proxy);
    options.build = settings.build;
    options.site = site;
    if let Some(max_rooms) = settings.max_rooms {
        options.max_rooms = max_rooms;
    }
    let server = match MultiplayerServer::listen(
        options,
        MatchRoom::new,
        (settings.host.as_str(), settings.port),
    )
    .await
    {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "Cannot listen on {}:{}: {error}",
                settings.host, settings.port
            );
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Sloppy Tanks multiplayer listening on {} (content {CONTENT_VERSION})",
        server.local_addr()
    );
    let (mut terminate, mut interrupt) = match (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) {
        (Ok(terminate), Ok(interrupt)) => (terminate, interrupt),
        _ => {
            eprintln!("Cannot install signal handlers");
            return ExitCode::FAILURE;
        }
    };
    let name = tokio::select! {
        _ = terminate.recv() => "SIGTERM",
        _ = interrupt.recv() => "SIGINT",
    };
    println!("{name}: resetting {} room(s)", server.room_codes().len());
    server.close().await;
    ExitCode::SUCCESS
}
