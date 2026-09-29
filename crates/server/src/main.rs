//! The multiplayer server binary: settings from the environment, a multi-threaded Tokio
//! runtime, and a graceful reset of every room on SIGTERM or SIGINT.

use std::process::ExitCode;

use sloppy_server::config::Settings;
use sloppy_server::lobby_host::LobbyHost;
use sloppy_server::process_stats::CountingAllocator;
use sloppy_server::protocol::CONTENT_VERSION;
use sloppy_server::server::{MultiplayerServer, ServerOptions};
use tokio::signal::unix::{SignalKind, signal};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() -> ExitCode {
    let settings = match Settings::from_env(|name| std::env::var(name).ok()) {
        Ok(settings) => settings,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime");
    runtime.block_on(serve(settings))
}

async fn serve(settings: Settings) -> ExitCode {
    let mut options = ServerOptions::new(settings.allowed_origins, settings.trust_proxy);
    if let Some(max_rooms) = settings.max_rooms {
        options.max_rooms = max_rooms;
    }
    // Until the MatchHost port lands in sloppy-core, rooms host lobbies only.
    let server = match MultiplayerServer::listen(
        options,
        LobbyHost::new,
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
