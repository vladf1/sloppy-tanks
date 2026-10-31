//! The command line: pilots from this machine, on a game server named directly or found
//! through the server directory. It prints a line per pilot every few seconds and, when
//! the run ends (its time is up, every pilot's connection ended, or Ctrl-C), their reports
//! as JSON. Leaving sends `leave`, which frees the seats at once.

mod directory;
mod sockets;

use std::process::ExitCode;
use std::time::{Duration, Instant};

use sloppy_core::net::client::ClientAction;
use sloppy_core::net::protocol::{CONTENT_VERSION, room_code};
use sloppy_pilot::command_line::{Options, USAGE, parse};
use sloppy_pilot::pilot::{Pilot, PilotConfig};
use sloppy_pilot::report::PilotReport;

use directory::{list, tls_config};
use sockets::{SocketEvent, Sockets};

/// Leaving waits this long for the `leave` messages to go out.
const LEAVE_WAIT: Duration = Duration::from_millis(500);

pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("{error}\n");
            }
            eprint!("{USAGE}");
            return if error.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    match runtime.block_on(run(options)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Eight bytes from the standard library's per-process random hash keys and the time.
fn random_bytes() -> [u8; 8] {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos()),
    );
    hasher.finish().to_le_bytes()
}

/// The server and room the pilots play: as given, or as the directory lists them.
async fn destination(
    options: &Options,
    tls: std::sync::Arc<rustls::ClientConfig>,
) -> Result<(String, String), String> {
    let room = options
        .room
        .clone()
        .unwrap_or_else(|| room_code(random_bytes()));
    if let Some(server) = &options.server {
        return Ok((server.clone(), room));
    }
    let servers = list(&options.directory, &options.environment, tls).await?;
    let server = match &options.room {
        Some(code) => servers
            .rooms
            .iter()
            .find(|listed| &listed.room == code)
            .map(|listed| listed.server.clone())
            .ok_or_else(|| {
                format!(
                    "The {} directory lists no room {code}; give --server if it is unlisted",
                    options.environment
                )
            })?,
        None => servers
            .new_room_server()
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "No {} server plays this build's game (content version {}); \
                     build from the commit the server runs, or give --server",
                    options.environment, CONTENT_VERSION
                )
            })?,
    };
    Ok((server, room))
}

fn status_line(report: &PilotReport) -> String {
    let team = match report.team {
        Some(0) => "blue",
        Some(1) => "red",
        _ => "-",
    };
    let feel = &report.feel;
    let mut line = format!(
        "{:<14} {:<10} {:<4} K/D {}/{} hits {} (players {}) taken {} rtt {:.0} ms view {:.0} ms \
         flash {:.0} ms corr p95 {:.2} m phantom {}",
        report.name,
        report.phase,
        team,
        report.score_kills,
        report.score_deaths,
        report.tally.hits,
        report.tally.player_hits,
        report.tally.hits_taken,
        feel.rtt_ms,
        feel.view_delay_ms.mean,
        feel.fire_to_flash_ms.mean,
        feel.correction_p95_m,
        feel.phantom_strikes,
    );
    if let Some(text) = report.ended_text.as_ref().or(report.notice.as_ref()) {
        line.push_str(&format!(" | {text}"));
    }
    line
}

async fn run(options: Options) -> Result<(), String> {
    let tls = tls_config();
    let (server, room) = destination(&options, tls.clone()).await?;
    let (mut sockets, mut events) = Sockets::new(tls);
    let started = Instant::now();
    let now = || started.elapsed().as_secs_f64() * 1000.0;
    let mut seed = u64::from_le_bytes(random_bytes());
    let mut pilots: Vec<Pilot> = (0..options.pilots)
        .map(|index| {
            let name = if options.pilots == 1 {
                options.name.clone()
            } else {
                format!("{}-{}", options.name, index + 1)
            };
            seed = seed.rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15;
            Pilot::new(PilotConfig {
                server_url: server.clone(),
                room: room.clone(),
                name,
                orders: options.orders.clone(),
                create: options.create.filter(|_| index == 0),
                saved_seat: None,
                seed: seed as u32,
            })
        })
        .collect();
    println!(
        "{} pilot(s) {} room {room} on {server}: {}",
        pilots.len(),
        if options.create.is_some() {
            "creating"
        } else {
            "joining"
        },
        options.orders.mission.describe()
    );
    let mut started_pilots = 0;
    let deadline = options.minutes * 60_000.0;
    let mut frame = tokio::time::interval(Duration::from_secs_f64(1.0 / options.fps));
    frame.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut next_status = options.status_seconds * 1000.0;
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        tokio::select! {
            _ = frame.tick() => {
                let at = now();
                // The maker of a room goes first; the rest join once it is seated there.
                if started_pilots == 0 {
                    pilots[0].start(at);
                    started_pilots = 1;
                }
                if started_pilots < pilots.len()
                    && (options.create.is_none() || pilots[0].seated())
                {
                    for pilot in &mut pilots[started_pilots..] {
                        pilot.start(at);
                    }
                    started_pilots = pilots.len();
                }
                for (index, pilot) in pilots.iter_mut().enumerate().take(started_pilots) {
                    pilot.tick(at);
                    perform(&mut sockets, index, pilot.take_actions());
                }
                if options.status_seconds > 0.0 && at >= next_status {
                    next_status = at + options.status_seconds * 1000.0;
                    for pilot in &pilots {
                        println!("{}", status_line(&pilot.report(at)));
                    }
                }
                let all_ended = started_pilots == pilots.len()
                    && pilots.iter().all(|pilot| pilot.ended().is_some());
                if at >= deadline || all_ended {
                    break;
                }
            }
            Some(event) = events.recv() => {
                let at = now();
                let index = match &event {
                    SocketEvent::Opened(index, _)
                    | SocketEvent::Text(index, ..)
                    | SocketEvent::Binary(index, ..)
                    | SocketEvent::Closed(index, ..) => *index,
                };
                let pilot = &mut pilots[index];
                match event {
                    SocketEvent::Opened(_, socket) => pilot.socket_opened(socket, at),
                    SocketEvent::Text(_, socket, text) => pilot.socket_text(socket, &text, at),
                    SocketEvent::Binary(_, socket, bytes) => {
                        pilot.socket_binary(socket, &bytes, at);
                    }
                    SocketEvent::Closed(_, socket, code) => {
                        sockets.closed(index, socket);
                        pilot.socket_closed(socket, code, at);
                    }
                }
            }
            _ = &mut interrupt => break,
        }
    }
    let at = now();
    for (index, pilot) in pilots.iter_mut().enumerate().take(started_pilots) {
        if pilot.ended().is_none() {
            pilot.leave(at);
            perform(&mut sockets, index, pilot.take_actions());
        }
    }
    let reports: Vec<PilotReport> = pilots.iter().map(|pilot| pilot.report(at)).collect();
    // The leaves go out before the runtime stops.
    sockets.close_all(LEAVE_WAIT).await;
    for report in &reports {
        println!("{}", status_line(report));
    }
    let json = serde_json::to_string_pretty(&reports).expect("reports serialize");
    match &options.report {
        Some(path) => std::fs::write(path, json + "\n")
            .map_err(|error| format!("Cannot write {path}: {error}"))?,
        None => println!("{json}"),
    }
    Ok(())
}

fn perform(sockets: &mut Sockets, index: usize, actions: Vec<ClientAction>) {
    for action in actions {
        match action {
            ClientAction::OpenSocket { socket, url } => sockets.open(index, socket, url),
            ClientAction::Send { socket, text } => sockets.send(index, socket, text),
            ClientAction::CloseSocket { socket } => sockets.close(index, socket),
            // A run's seats end with it; a restarted run joins afresh.
            ClientAction::SaveSeat { .. } | ClientAction::ForgetSeat => {}
        }
    }
}
