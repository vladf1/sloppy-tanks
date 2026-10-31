//! The command line's options (`sloppy-pilot --help`), read here so the library's tests
//! cover them; `src/native/` runs them.

use sloppy_core::net::protocol::{
    DIFFICULTIES, MAP_MODES, MAX_ROUND_MINUTES, RoomSettings, read_room_code,
};

use crate::orders::{Mission, Orders, parse_kind, parse_personality, parse_skill};

/// The production directory (`DIRECTORY_URL` in `scripts/servers.mjs`).
pub const DEFAULT_DIRECTORY: &str = "https://directory.sloppytanks.com";

pub const USAGE: &str = "\
Usage: sloppy-pilot (--room CODE | --create MAP) [options]

Where:
  --room CODE            join this room
  --create MAP           create a room: village, harbor, quarry (or an extra level)
  --server URL           the game server (ws://127.0.0.1:8787, wss://…); without it the
                         directory names the room's server, or the one a new room opens on
  --directory URL        the server directory (default: production's)
  --environment NAME     the directory's environment (production, dev; default production)

Who:
  --pilots N             how many pilots (default 1); with --create the first makes the room
  --name NAME            the pilot's name, or with several pilots their prefix (default pilot)
  --mission MISSION      skirmish, hunt, hunt:NAME or escort:NAME (default skirmish)
  --hunt NAME            the same as --mission hunt:NAME
  --escort NAME          the same as --mission escort:NAME
  --skill SKILL          easy, normal or hard (default normal)
  --kind KIND            scout, balanced or heavy (default balanced)
  --style ROLE           the bot role: scout, guard, sniper, heavy, minelayer, support, artillery

A created room:
  --difficulty LEVEL     its fill bots' difficulty (default normal)
  --humans-only          no fill bots
  --round-minutes N      battle length (default 5)

Run:
  --minutes N            stop after this long (default 10)
  --fps N                pilot frames a second (default 30)
  --status-seconds N     seconds between status lines (default 5; 0 for none)
  --report FILE          also write the final reports to FILE as JSON
";

const DEFAULT_MINUTES: f64 = 10.0;
const DEFAULT_FPS: f64 = 30.0;
const DEFAULT_STATUS_SECONDS: f64 = 5.0;
const DEFAULT_ROUND_MINUTES: u32 = 5;

pub struct Options {
    pub room: Option<String>,
    pub create: Option<RoomSettings>,
    pub server: Option<String>,
    pub directory: String,
    pub environment: String,
    pub pilots: usize,
    pub name: String,
    pub orders: Orders,
    pub minutes: f64,
    pub fps: f64,
    pub status_seconds: f64,
    pub report: Option<String>,
}

/// The options in `args` (without the program name); an empty error asks for the usage.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        room: None,
        create: None,
        server: None,
        directory: DEFAULT_DIRECTORY.into(),
        environment: "production".into(),
        pilots: 1,
        name: "pilot".into(),
        orders: Orders::default(),
        minutes: DEFAULT_MINUTES,
        fps: DEFAULT_FPS,
        status_seconds: DEFAULT_STATUS_SECONDS,
        report: None,
    };
    let mut create_map = None;
    let mut difficulty = None;
    let mut humans_only = false;
    let mut round_minutes = DEFAULT_ROUND_MINUTES;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let number = |text: String, label: &str| {
            text.parse::<f64>()
                .ok()
                .filter(|number| number.is_finite() && *number > 0.0)
                .ok_or_else(|| format!("{label} must be a positive number"))
        };
        match flag.as_str() {
            "--room" => {
                let text = value()?;
                options.room = Some(
                    read_room_code(&text).ok_or_else(|| format!("{text:?} is not a room code"))?,
                );
            }
            "--create" => create_map = Some(value()?),
            "--server" => options.server = Some(value()?.trim_end_matches('/').to_string()),
            "--directory" => options.directory = value()?,
            "--environment" => options.environment = value()?,
            "--pilots" => options.pilots = number(value()?, "--pilots")? as usize,
            "--name" => options.name = value()?,
            "--mission" => options.orders.mission = Mission::parse(&value()?)?,
            "--hunt" => options.orders.mission = Mission::Hunt(Some(value()?)),
            "--escort" => options.orders.mission = Mission::Escort(value()?),
            "--skill" => options.orders.skill = parse_skill(&value()?)?,
            "--kind" => options.orders.kind = parse_kind(&value()?)?,
            "--style" => options.orders.personality = Some(parse_personality(&value()?)?),
            "--difficulty" => difficulty = Some(value()?),
            "--humans-only" => humans_only = true,
            "--round-minutes" => {
                round_minutes = number(value()?, "--round-minutes")? as u32;
            }
            "--minutes" => options.minutes = number(value()?, "--minutes")?,
            "--fps" => options.fps = number(value()?, "--fps")?.clamp(10.0, 120.0),
            "--status-seconds" => {
                let text = value()?;
                options.status_seconds = text
                    .parse()
                    .ok()
                    .filter(|seconds: &f64| *seconds >= 0.0)
                    .ok_or("--status-seconds must be zero or more")?;
            }
            "--report" => options.report = Some(value()?),
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("Unknown option {other}")),
        }
    }
    if let Some(map) = create_map {
        let map_mode = MAP_MODES
            .iter()
            .find(|(name, _)| *name == map)
            .map(|(_, id)| *id)
            .ok_or_else(|| format!("Unknown map {map:?}"))?;
        let difficulty = match difficulty {
            Some(text) => DIFFICULTIES
                .iter()
                .find(|(name, _)| *name == text)
                .map(|(_, level)| *level)
                .ok_or_else(|| format!("Unknown difficulty {text:?}"))?,
            None => Default::default(),
        };
        options.create = Some(RoomSettings {
            map_mode,
            difficulty,
            humans_only,
            round_minutes: round_minutes.clamp(1, MAX_ROUND_MINUTES),
        });
    }
    match (&options.room, &options.create) {
        (None, None) => return Err("Give --room CODE or --create MAP".into()),
        (Some(_), Some(_)) => return Err("Give --room or --create, not both".into()),
        _ => {}
    }
    if !(options.pilots >= 1 && options.pilots <= 8) {
        return Err("--pilots must be 1 to 8 (a room has eight seats)".into());
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn options_read_from_the_command_line() {
        let options = parse(&args(
            "--room abcd2345 --hunt vlad --pilots 3 --skill hard --kind scout --minutes 2",
        ))
        .unwrap();
        assert_eq!(options.room.as_deref(), Some("ABCD2345"));
        assert_eq!(options.orders.mission, Mission::Hunt(Some("vlad".into())));
        assert_eq!(options.pilots, 3);
        assert_eq!(options.minutes, 2.0);
        let created = parse(&args(
            "--create quarry --humans-only --round-minutes 3 --server ws://127.0.0.1:8787/",
        ))
        .unwrap();
        let settings = created.create.unwrap();
        assert_eq!(settings.map_mode.as_str(), "quarry");
        assert!(settings.humans_only);
        assert_eq!(settings.round_minutes, 3);
        assert_eq!(created.server.as_deref(), Some("ws://127.0.0.1:8787"));
        for bad in [
            "",
            "--room ABCD2345 --create village",
            "--room nope",
            "--create moon",
            "--room ABCD2345 --pilots 9",
            "--room ABCD2345 --mission dance",
            "--room ABCD2345 --bogus",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
    }
}
