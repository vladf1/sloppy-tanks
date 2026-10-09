//! Projectile trajectories: a room replicates each shell as the leg of flight it is on,
//! sent once, rather than its position in every frame (Quake 3's missile trajectories).
//!
//! A [`ShotPath`] holds a shell's start tick, position and velocity, and the launch data
//! that never changes (team, weapon, heights, a rocket's thrust). Clients evaluate it at
//! their display tick with [`ShotPath::at`]. The host's [`ShotPathRecorder`] follows every
//! projectile sweep and starts a new path only when the drawn shell would stray more than
//! [`PATH_TOLERANCE`] from the simulated one, or would point or move wrongly: at a bounce,
//! a guided missile's turn, an interception's survivor. It ends a path when the shell is
//! gone, at the end of its last sweep, so impacts land where the shell is drawn. Hits stay
//! with the host's simulation; paths are presentation only.
//!
//! On the wire ([`PathEntry::write_binary`]) a path is its shell id, its start tick back
//! from the frame's in thousandths, and its position and velocity in millimetres; a launch
//! adds the team, the weapon, a mask of the heights and thrust it has, and those values.

use super::json::{self, ObjectWriter, POSITION_SCALE};
use super::protocol::read_team;
use super::scene_codec::WEAPONS;
use super::schema::{ReadResult, Record, choice, field, id32, number, number_in, optional};
use super::wire::{WireReader, put_signed, put_varint, units};
use crate::sim::data::STEP;
use crate::sim::math::angle_delta;
use crate::sim::projectiles::RocketThrust;
use crate::sim::render_state::RenderShot;
use crate::sim::simulation::Simulation;
use crate::sim::types::{Shot, Team, Weapon};

/// Farthest, in metres, a drawn shell may be from its simulated position.
pub const PATH_TOLERANCE: f64 = 0.05;
/// A drawn shell's heading (radians) and relative speed may differ this much from the
/// simulated one before a new path starts, so a bounce starts one at the wall.
const HEADING_TOLERANCE: f64 = 0.05;
const SPEED_TOLERANCE: f64 = 0.02;
/// Most shells a room tracks; the simulation's shells are far fewer.
pub const MAX_LIVE_PATHS: usize = 512;
/// Latest tick a path may name.
const MAX_TICK: f64 = 1e9;

/// What a shell keeps for its whole flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotLaunch {
    pub team: Team,
    pub weapon: Weapon,
    /// Combat lane height.
    pub y: Option<f64>,
    /// Render launch height.
    pub visual_y: Option<f64>,
    pub thrust: Option<RocketThrust>,
}

/// One leg of a shell's flight from `tick` (fractional) on, rounded as the wire sends it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotPath {
    pub id: u32,
    pub tick: f64,
    pub x: f64,
    pub z: f64,
    pub vx: f64,
    pub vz: f64,
    pub launch: ShotLaunch,
}

impl ShotPath {
    /// The shell `tick` (fractional) into the simulation, on this leg.
    pub fn at(&self, tick: f64) -> RenderShot {
        let seconds = (tick - self.tick).max(0.0) * STEP;
        let speed = self.vx.hypot(self.vz);
        let (distance, now) = match self.launch.thrust {
            Some(thrust) if speed > 0.0 && speed < thrust.top_speed => {
                thrust_flight(self.tick, speed, thrust, seconds)
            }
            _ => (speed * seconds, speed),
        };
        let (x, z, vx, vz) = if speed > 0.0 {
            let (dx, dz) = (self.vx / speed, self.vz / speed);
            (
                self.x + dx * distance,
                self.z + dz * distance,
                dx * now,
                dz * now,
            )
        } else {
            (self.x, self.z, 0.0, 0.0)
        };
        RenderShot {
            id: self.id,
            x,
            z,
            y: self.launch.y,
            visual_y: self.launch.visual_y,
            vx,
            vz,
            weapon: self.launch.weapon,
            team: self.launch.team,
        }
    }

    /// `{"id":..,"tick":..,"x":..,"z":..,"vx":..,"vz":..}`, followed for a launch by the
    /// shell's constant fields.
    pub fn write(&self, out: &mut String, launch: bool) {
        let mut writer = ObjectWriter::new(out);
        writer
            .int("id", u64::from(self.id))
            .number("tick", self.tick)
            .number("x", self.x)
            .number("z", self.z)
            .number("vx", self.vx)
            .number("vz", self.vz);
        if launch {
            let launch = &self.launch;
            writer
                .int("team", launch.team.index() as u64)
                .string("weapon", launch.weapon.as_str());
            if let Some(y) = launch.y {
                writer.number("y", y);
            }
            if let Some(visual) = launch.visual_y {
                writer.number("visualY", visual);
            }
            if let Some(thrust) = launch.thrust {
                writer
                    .number("thrust", thrust.acceleration)
                    .number("topSpeed", thrust.top_speed);
            }
        }
        writer.finish();
    }

    /// A path record; launch fields come from `launch` when given, else from the record.
    pub fn read(source: &Record, launch: Option<ShotLaunch>) -> ReadResult<Self> {
        let launch = match launch {
            Some(launch) => launch,
            None => ShotLaunch {
                team: field(source, "team", read_team)?,
                weapon: field(source, "weapon", |v| choice(v, &WEAPONS))?,
                y: field(source, "y", |v| optional(v, number))?,
                visual_y: field(source, "visualY", |v| optional(v, number))?,
                thrust: match source.get("thrust") {
                    None => None,
                    Some(_) => Some(RocketThrust {
                        acceleration: field(source, "thrust", |v| number_in(v, 0.0, 1e6, false))?,
                        top_speed: field(source, "topSpeed", |v| number_in(v, 0.0, 1e6, false))?,
                    }),
                },
            },
        };
        Ok(Self {
            id: field(source, "id", id32)?,
            tick: field(source, "tick", |v| number_in(v, 0.0, MAX_TICK, false))?,
            x: field(source, "x", number)?,
            z: field(source, "z", number)?,
            vx: field(source, "vx", number)?,
            vz: field(source, "vz", number)?,
            launch,
        })
    }

    /// The path a shell starting a sweep at `tick` would be drawn on, rounded for the wire.
    fn starting(shot: &Shot, tick: f64, x: f64, z: f64, thrust: Option<RocketThrust>) -> Self {
        Self {
            id: shot.id,
            tick: json::position(tick),
            x: json::position(x),
            z: json::position(z),
            vx: json::position(shot.vx),
            vz: json::position(shot.vz),
            launch: ShotLaunch {
                team: shot.team,
                weapon: shot.weapon,
                y: shot.y.map(json::position),
                visual_y: shot.visual_y.map(json::position),
                thrust: thrust.map(|thrust| RocketThrust {
                    acceleration: json::position(thrust.acceleration),
                    top_speed: json::position(thrust.top_speed),
                }),
            },
        }
    }
}

/// Distance flown and speed `seconds` after a leg starting at `tick` with `speed`: the
/// speed holds until the next whole tick, then grows by one step of thrust at each
/// (`step_projectiles` accelerates once per tick, before its sweeps) up to the top.
fn thrust_flight(tick: f64, speed: f64, thrust: RocketThrust, seconds: f64) -> (f64, f64) {
    let first = (tick.floor() + 1.0 - tick) * STEP;
    if seconds <= first {
        return (speed * seconds, speed);
    }
    let gain = thrust.acceleration * STEP;
    let rest = seconds - first;
    let whole = (rest / STEP).floor();
    let partial = rest - whole * STEP;
    // Ticks after the first boundary that still fly below the top speed.
    let climbing = if gain > 0.0 {
        ((thrust.top_speed - speed) / gain).ceil() - 1.0
    } else {
        f64::INFINITY
    }
    .max(0.0);
    let speed_after = |ticks: f64| {
        if ticks <= climbing {
            speed + gain * ticks
        } else {
            thrust.top_speed
        }
    };
    let rising = whole.min(climbing);
    let sum = rising * speed
        + gain * rising * (rising + 1.0) / 2.0
        + (whole - rising).max(0.0) * thrust.top_speed;
    let now = speed_after(whole + 1.0);
    (speed * first + sum * STEP + now * partial, now)
}

/// One change to a room's projectiles, in the order the host recorded it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathEntry {
    /// A shell's first path, with its launch fields.
    Launch(ShotPath),
    /// A flying shell's new path.
    Change(ShotPath),
    /// The shell is gone from `tick` (fractional) on.
    End { id: u32, tick: f64 },
}

impl PathEntry {
    pub fn id(&self) -> u32 {
        match self {
            PathEntry::Launch(path) | PathEntry::Change(path) => path.id,
            PathEntry::End { id, .. } => *id,
        }
    }

    pub fn write(&self, out: &mut String) {
        match self {
            PathEntry::Launch(path) => path.write(out, true),
            PathEntry::Change(path) => path.write(out, false),
            PathEntry::End { id, tick } => {
                let mut writer = ObjectWriter::new(out);
                writer.int("id", u64::from(*id)).number("end", *tick);
                writer.finish();
            }
        }
    }
}

/// The shells in flight on a client, as their current paths. Entries apply in order; a
/// change or an end needs its shell, a launch a new id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LivePaths {
    pub paths: Vec<ShotPath>,
}

impl LivePaths {
    fn position(&self, id: u32) -> Option<usize> {
        self.paths.iter().position(|path| path.id == id)
    }

    /// A baseline's paths ([`write_baseline`]): every shell's current path in launch form.
    pub fn read_binary(reader: &mut WireReader<'_>, tick: u64) -> ReadResult<Self> {
        let count = reader.varint()?;
        if count > MAX_LIVE_PATHS as u64 {
            return Err("Invalid list".into());
        }
        let mut live = Self::default();
        for _ in 0..count {
            let path = ShotPath::read_binary(reader, None, tick)?;
            if live.position(path.id).is_some() {
                return Err("Invalid projectile path".into());
            }
            live.paths.push(path);
        }
        Ok(live)
    }

    /// Writes every shell's path in launch form, for a baseline at `tick`.
    pub fn write_baseline<'a>(
        paths: impl IntoIterator<Item = &'a ShotPath>,
        tick: u64,
        out: &mut Vec<u8>,
    ) {
        let paths: Vec<&ShotPath> = paths.into_iter().collect();
        put_varint(out, paths.len() as u64);
        for path in paths {
            path.write_binary(out, true, tick);
        }
    }

    /// Reads `count` of a frame's path entries against these shells and applies them,
    /// returning each entry with its launch fields filled in. Fails, leaving `self` partly
    /// changed, on an entry that names an unknown or duplicate shell, goes back in time or
    /// passes `tick`.
    pub fn apply_binary(
        &mut self,
        reader: &mut WireReader<'_>,
        count: u64,
        tick: u64,
    ) -> ReadResult<Vec<PathEntry>> {
        let mut entries = Vec::with_capacity(count.min(MAX_LIVE_PATHS as u64) as usize);
        for _ in 0..count {
            let kind = reader.varint()?;
            let entry = match kind {
                LAUNCH_ENTRY => {
                    let path = ShotPath::read_binary(reader, None, tick)?;
                    if self.position(path.id).is_some() {
                        return Err("Duplicate projectile".into());
                    }
                    if self.paths.len() >= MAX_LIVE_PATHS {
                        return Err("Too many projectiles".into());
                    }
                    self.paths.push(path);
                    PathEntry::Launch(path)
                }
                CHANGE_ENTRY | END_ENTRY => {
                    let id = reader.varint32()?;
                    let index = self
                        .position(id)
                        .ok_or_else(|| "Unknown projectile".to_string())?;
                    if kind == END_ENTRY {
                        let end = tick_back(reader, tick)?;
                        if end < self.paths[index].tick {
                            return Err("Invalid projectile timeline".into());
                        }
                        self.paths.remove(index);
                        PathEntry::End { id, tick: end }
                    } else {
                        let mut path = ShotPath::read_motion(reader, id, tick)?;
                        path.launch = self.paths[index].launch;
                        if path.tick < self.paths[index].tick {
                            return Err("Invalid projectile timeline".into());
                        }
                        self.paths[index] = path;
                        PathEntry::Change(path)
                    }
                }
                _ => return Err("Invalid projectile path".into()),
            };
            entries.push(entry);
        }
        Ok(entries)
    }

    /// The shells at `tick`.
    pub fn fill(&self, shots: &mut Vec<RenderShot>, tick: f64) {
        shots.clear();
        shots.extend(self.paths.iter().map(|path| path.at(tick)));
    }
}

const LAUNCH_ENTRY: u64 = 0;
const CHANGE_ENTRY: u64 = 1;
const END_ENTRY: u64 = 2;
const HAS_Y: u64 = 1;
const HAS_VISUAL_Y: u64 = 2;
const HAS_THRUST: u64 = 4;

fn thousandths(value: f64) -> i64 {
    units(value, POSITION_SCALE)
}

fn read_thousandths(reader: &mut WireReader<'_>) -> ReadResult<f64> {
    let value = reader.signed()? as f64 / POSITION_SCALE;
    if value.abs() <= MAX_TICK {
        Ok(value)
    } else {
        Err("Invalid number".into())
    }
}

/// A tick written back from the frame's `tick`: never after it, never before zero.
fn tick_back(reader: &mut WireReader<'_>, tick: u64) -> ReadResult<f64> {
    let back = reader.signed()?;
    let at = thousandths(tick as f64) - back;
    if back < 0 || at < 0 {
        return Err("Invalid projectile timeline".into());
    }
    Ok(at as f64 / POSITION_SCALE)
}

impl ShotPath {
    /// The path in binary (see the module docs), its start written back from `tick`.
    pub fn write_binary(&self, out: &mut Vec<u8>, launch: bool, tick: u64) {
        put_varint(out, u64::from(self.id));
        put_signed(out, thousandths(tick as f64) - thousandths(self.tick));
        for value in [self.x, self.z, self.vx, self.vz] {
            put_signed(out, thousandths(value));
        }
        if !launch {
            return;
        }
        let launch = &self.launch;
        put_varint(out, launch.team.index() as u64);
        let weapon = WEAPONS
            .iter()
            .position(|(_, weapon)| *weapon == launch.weapon)
            .expect("every weapon has a wire name");
        put_varint(out, weapon as u64);
        let mut has = 0;
        for (present, flag) in [
            (launch.y.is_some(), HAS_Y),
            (launch.visual_y.is_some(), HAS_VISUAL_Y),
            (launch.thrust.is_some(), HAS_THRUST),
        ] {
            if present {
                has |= flag;
            }
        }
        put_varint(out, has);
        for value in [launch.y, launch.visual_y].into_iter().flatten() {
            put_signed(out, thousandths(value));
        }
        if let Some(thrust) = launch.thrust {
            put_signed(out, thousandths(thrust.acceleration));
            put_signed(out, thousandths(thrust.top_speed));
        }
    }

    /// A path's start and motion after its id; its launch fields are the caller's.
    fn read_motion(reader: &mut WireReader<'_>, id: u32, tick: u64) -> ReadResult<Self> {
        Ok(Self {
            id,
            tick: tick_back(reader, tick)?,
            x: read_thousandths(reader)?,
            z: read_thousandths(reader)?,
            vx: read_thousandths(reader)?,
            vz: read_thousandths(reader)?,
            launch: ShotLaunch {
                team: Team::Blue,
                weapon: Weapon::Standard,
                y: None,
                visual_y: None,
                thrust: None,
            },
        })
    }

    /// A path in launch form, or with `launch` when given.
    fn read_binary(
        reader: &mut WireReader<'_>,
        launch: Option<ShotLaunch>,
        tick: u64,
    ) -> ReadResult<Self> {
        let id = reader.varint32()?;
        let mut path = Self::read_motion(reader, id, tick)?;
        path.launch = match launch {
            Some(launch) => launch,
            None => {
                let team = match reader.varint()? {
                    0 => Team::Blue,
                    1 => Team::Red,
                    _ => return Err("team: Invalid choice".into()),
                };
                let weapon = WEAPONS
                    .get(reader.varint()? as usize)
                    .map(|(_, weapon)| *weapon)
                    .ok_or_else(|| "weapon: Invalid choice".to_string())?;
                let has = reader.varint()?;
                if has & !(HAS_Y | HAS_VISUAL_Y | HAS_THRUST) != 0 {
                    return Err("Invalid projectile path".into());
                }
                let mut optional = |flag| -> ReadResult<Option<f64>> {
                    if has & flag != 0 {
                        read_thousandths(reader).map(Some)
                    } else {
                        Ok(None)
                    }
                };
                let y = optional(HAS_Y)?;
                let visual_y = optional(HAS_VISUAL_Y)?;
                let thrust = match (optional(HAS_THRUST)?, optional(HAS_THRUST)?) {
                    (Some(acceleration), Some(top_speed))
                        if (0.0..=1e6).contains(&acceleration)
                            && (0.0..=1e6).contains(&top_speed) =>
                    {
                        Some(RocketThrust {
                            acceleration,
                            top_speed,
                        })
                    }
                    (None, None) => None,
                    _ => return Err("thrust: Invalid number".into()),
                };
                ShotLaunch {
                    team,
                    weapon,
                    y,
                    visual_y,
                    thrust,
                }
            }
        };
        Ok(path)
    }
}

impl PathEntry {
    /// The entry in binary: its kind, then a launch's whole path, a change's motion or an
    /// end's tick, ticks written back from the frame's `tick`.
    pub fn write_binary(&self, out: &mut Vec<u8>, tick: u64) {
        match self {
            PathEntry::Launch(path) => {
                put_varint(out, LAUNCH_ENTRY);
                path.write_binary(out, true, tick);
            }
            PathEntry::Change(path) => {
                put_varint(out, CHANGE_ENTRY);
                path.write_binary(out, false, tick);
            }
            PathEntry::End { id, tick: end } => {
                put_varint(out, END_ENTRY);
                put_varint(out, u64::from(*id));
                put_signed(out, thousandths(tick as f64) - thousandths(*end));
            }
        }
    }
}

/// A shell the host is following: the path clients draw and where its last sweep ended.
#[derive(Clone, Debug)]
struct Followed {
    path: ShotPath,
    swept_to: f64,
}

/// The host's side: turns projectile sweeps into path entries for the next frame.
#[derive(Clone, Debug, Default)]
pub struct ShotPathRecorder {
    /// In id order: shells are launched with increasing ids.
    followed: Vec<Followed>,
    entries: Vec<PathEntry>,
    flying: Vec<u32>,
}

impl ShotPathRecorder {
    /// Forgets every shell, for a new round.
    pub fn clear(&mut self) {
        self.followed.clear();
        self.entries.clear();
    }

    /// Follows the sweeps `simulation` recorded in its `projectile_moves` during tick
    /// `tick`, then ends the paths of shells that are gone. Keeps the capture's capacity.
    pub fn follow(&mut self, simulation: &mut Simulation, tick: u64) {
        let Some(mut moves) = simulation.projectile_moves.take() else {
            return;
        };
        let thrust = RocketThrust::of(simulation);
        for sweep in moves.drain(..) {
            let shot = &sweep.shot;
            self.sweep(
                tick,
                shot,
                sweep.seconds,
                sweep.offset,
                (shot.weapon == Weapon::Rocket).then_some(thrust),
            );
        }
        self.retire(&simulation.shots);
        simulation.projectile_moves = Some(moves);
    }

    /// Follows one sweep of `shot` during simulation tick `tick`: `seconds` long, from
    /// `offset` seconds into the tick, ending at the shell's current position.
    pub fn sweep(
        &mut self,
        tick: u64,
        shot: &Shot,
        seconds: f64,
        offset: f64,
        thrust: Option<RocketThrust>,
    ) {
        if seconds <= 0.0 {
            return;
        }
        let start = tick as f64 - 1.0 + offset / STEP;
        let end = start + seconds / STEP;
        let (start_x, start_z) = (shot.x - shot.vx * seconds, shot.z - shot.vz * seconds);
        let index = self
            .followed
            .binary_search_by_key(&shot.id, |followed| followed.path.id);
        match index {
            Ok(index) => {
                let followed = &mut self.followed[index];
                followed.swept_to = end;
                if !strays(&followed.path, shot, start, end, start_x, start_z) {
                    return;
                }
                let path =
                    ShotPath::starting(shot, start, start_x, start_z, followed.path.launch.thrust);
                followed.path = path;
                self.entries.push(PathEntry::Change(path));
            }
            Err(index) => {
                let path = ShotPath::starting(shot, start, start_x, start_z, thrust);
                self.followed.insert(
                    index,
                    Followed {
                        path,
                        swept_to: end,
                    },
                );
                self.entries.push(PathEntry::Launch(path));
            }
        }
    }

    /// Ends every path where its shell last flew to, for a round that stops stepping with
    /// shells still in flight: clients would otherwise carry them on past their final pose.
    pub fn end_all(&mut self) {
        for followed in self.followed.drain(..) {
            self.entries.push(PathEntry::End {
                id: followed.path.id,
                tick: json::position(followed.swept_to),
            });
        }
    }

    /// Ends the path of every followed shell that is no longer flying in `shots`.
    pub fn retire(&mut self, shots: &[Shot]) {
        self.flying.clear();
        self.flying.extend(
            shots
                .iter()
                .filter(|shot| shot.life > 0.0)
                .map(|shot| shot.id),
        );
        self.flying.sort_unstable();
        let (flying, entries) = (&self.flying, &mut self.entries);
        self.followed.retain(|followed| {
            let alive = flying.binary_search(&followed.path.id).is_ok();
            if !alive {
                entries.push(PathEntry::End {
                    id: followed.path.id,
                    tick: json::position(followed.swept_to),
                });
            }
            alive
        });
    }

    /// Entries recorded since the last [`Self::clear_entries`].
    pub fn entries(&self) -> &[PathEntry] {
        &self.entries
    }

    pub fn clear_entries(&mut self) {
        self.entries.clear();
    }

    /// Every followed shell's current path, for a baseline.
    pub fn paths(&self) -> impl Iterator<Item = &ShotPath> {
        self.followed.iter().map(|followed| &followed.path)
    }
}

/// Whether a sweep from `start` to `end` (fractional ticks) leaves `path`: the drawn shell
/// would be too far away at either end, or head or move differently.
fn strays(path: &ShotPath, shot: &Shot, start: f64, end: f64, x: f64, z: f64) -> bool {
    let from = path.at(start);
    let to = path.at(end);
    if (from.x - x).hypot(from.z - z) > PATH_TOLERANCE
        || (to.x - shot.x).hypot(to.z - shot.z) > PATH_TOLERANCE
    {
        return true;
    }
    let drawn = path.at((start + end) / 2.0);
    let drawn_speed = drawn.vx.hypot(drawn.vz);
    let speed = shot.vx.hypot(shot.vz);
    if (drawn_speed - speed).abs() > SPEED_TOLERANCE * speed {
        return true;
    }
    speed > 0.0
        && drawn_speed > 0.0
        && angle_delta(drawn.vx.atan2(drawn.vz), shot.vx.atan2(shot.vz)).abs() > HEADING_TOLERANCE
}
