//! The replicated scene (`src/net/scene-codec.ts`): what the host captures from the
//! simulation, what clients validate, and how a client projects it into the same
//! [`RenderState`] local play draws.
//!
//! The host writes each wire record directly as JSON text, in its reader's field order and
//! with the precision the TypeScript `rounded` gave that field name: positions and sizes in
//! millimetres, rotations and angles in ten-thousandths, timers and meters in hundredths.
//! Each record remembers where every field's value sits in its text, so the replication
//! stream compares fields without parsing. Clients read records back through readers that
//! keep the TypeScript limits and error messages.

use std::collections::HashMap;

use serde_json::Value;

use super::json::{self, ObjectWriter, write_int, write_number, write_str};
use super::protocol::{MAP_MODES, read_team};
use super::schema::{
    ReadResult, Record, array, boolean, choice, field, id, id32, nested, nullable, number,
    number_in, optional, string,
};
use crate::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use crate::sim::debris_physics::DebrisMaterial;
use crate::sim::map_options::{MapId, is_extra_level};
use crate::sim::maps::GroundKind;
use crate::sim::math::{Point3, Quat4, Vec2, to_int32};
use crate::sim::render_state::{
    RenderCover, RenderCoverMotion, RenderFragment, RenderShot, RenderState, RenderTank,
};
use crate::sim::simulation::Simulation;
use crate::sim::simulation_rules::FRAGMENT_CAPACITY;
use crate::sim::timber_layout::{
    TimberFace, TimberHit, TimberJoin, TimberMark, TimberPart, TimberPartKind,
};
use crate::sim::types::{
    AmmoInventory, Cover, CoverKind, DamageCause, DamageSource, DeathStyle, Fragment,
    FragmentShape, Match, MatchPhase, Mine, Pickup, PickupKind, SimEvent, SimEventType, Tank, Team,
    VehicleKind, Weapon, WreckPart,
};

/// Entity kinds in wire order; the index of a kind in every per-kind array.
pub const ENTITY_TYPES: [&str; 6] = ["tanks", "covers", "fragments", "shots", "mines", "pickups"];
pub const TANKS: usize = 0;
pub const COVERS: usize = 1;
pub const FRAGMENTS: usize = 2;
pub const SHOTS: usize = 3;
pub const MINES: usize = 4;
pub const PICKUPS: usize = 5;
/// Most records of each kind a scene may hold. Standard rooms field 12 tanks; the extra
/// levels field 30.
pub const ENTITY_LIMITS: [usize; 6] = [32, 1024, FRAGMENT_CAPACITY, 512, 256, 64];

// Wire names of the simulation's enums, shared by writers and readers.
pub const VEHICLE_KINDS: [(&str, VehicleKind); 4] = [
    ("scout", VehicleKind::Scout),
    ("balanced", VehicleKind::Balanced),
    ("heavy", VehicleKind::Heavy),
    ("humvee", VehicleKind::Humvee),
];
pub const WEAPONS: [(&str, Weapon); 6] = [
    ("standard", Weapon::Standard),
    ("spread", Weapon::Spread),
    ("rocket", Weapon::Rocket),
    ("ricochet", Weapon::Ricochet),
    ("piercing", Weapon::Piercing),
    ("tow", Weapon::Tow),
];
pub const COVER_KINDS: [(&str, CoverKind); 13] = [
    ("rock", CoverKind::Rock),
    ("teeth", CoverKind::Teeth),
    ("hedgehog", CoverKind::Hedgehog),
    ("container", CoverKind::Container),
    ("cargo", CoverKind::Cargo),
    ("house", CoverKind::House),
    ("tree", CoverKind::Tree),
    ("timber", CoverKind::Timber),
    ("concrete", CoverKind::Concrete),
    ("drum", CoverKind::Drum),
    ("tower", CoverKind::Tower),
    ("rubble", CoverKind::Rubble),
    ("boundary", CoverKind::Boundary),
];
pub const MATERIALS: [(&str, DebrisMaterial); 3] = [
    ("wood", DebrisMaterial::Wood),
    ("metal", DebrisMaterial::Metal),
    ("concrete", DebrisMaterial::Concrete),
];
pub const FRAGMENT_SHAPES: [(&str, FragmentShape); 10] = [
    ("armor", FragmentShape::Armor),
    ("wheel", FragmentShape::Wheel),
    ("track", FragmentShape::Track),
    ("shard", FragmentShape::Shard),
    ("wood", FragmentShape::Wood),
    ("panel", FragmentShape::Panel),
    ("beam", FragmentShape::Beam),
    ("log", FragmentShape::Log),
    ("drum-shell", FragmentShape::DrumShell),
    ("drum-lid", FragmentShape::DrumLid),
];
pub const WRECK_PARTS: [(&str, WreckPart); 5] = [
    ("intact", WreckPart::Intact),
    ("hull", WreckPart::Hull),
    ("turret", WreckPart::Turret),
    ("turret-barrel", WreckPart::TurretBarrel),
    ("barrel", WreckPart::Barrel),
];
pub const PICKUP_KINDS: [(&str, PickupKind); 9] = [
    ("spread", PickupKind::Spread),
    ("rocket", PickupKind::Rocket),
    ("ricochet", PickupKind::Ricochet),
    ("piercing", PickupKind::Piercing),
    ("rapid", PickupKind::Rapid),
    ("shield", PickupKind::Shield),
    ("speed", PickupKind::Speed),
    ("repair", PickupKind::Repair),
    ("laser", PickupKind::Laser),
];
pub const MATCH_PHASES: [(&str, MatchPhase); 4] = [
    ("ready", MatchPhase::Ready),
    ("playing", MatchPhase::Playing),
    ("paused", MatchPhase::Paused),
    ("results", MatchPhase::Results),
];
pub const GROUNDS: [(&str, GroundKind); 2] = [
    ("dry-grass", GroundKind::DryGrass),
    ("packed-dirt", GroundKind::PackedDirt),
];
pub const TIMBER_FACES: [(&str, TimberFace); 6] = [
    ("front", TimberFace::Front),
    ("back", TimberFace::Back),
    ("left", TimberFace::Left),
    ("right", TimberFace::Right),
    ("top", TimberFace::Top),
    ("bottom", TimberFace::Bottom),
];
pub const TIMBER_PART_KINDS: [(&str, TimberPartKind); 2] = [
    ("beam", TimberPartKind::Beam),
    ("post", TimberPartKind::Post),
];
pub const EVENT_TYPES: [(&str, SimEventType); 13] = [
    ("debris-impact", SimEventType::DebrisImpact),
    ("notice", SimEventType::Notice),
    ("shot", SimEventType::Shot),
    ("impact", SimEventType::Impact),
    ("explosion", SimEventType::Explosion),
    ("destroy", SimEventType::Destroy),
    ("death", SimEventType::Death),
    ("pickup", SimEventType::Pickup),
    ("respawn", SimEventType::Respawn),
    ("hurt", SimEventType::Hurt),
    ("ricochet", SimEventType::Ricochet),
    ("laser", SimEventType::Laser),
    ("promotion", SimEventType::Promotion),
];
pub const DAMAGE_CAUSES: [(&str, DamageCause); 10] = [
    ("standard", DamageCause::Standard),
    ("spread", DamageCause::Spread),
    ("rocket", DamageCause::Rocket),
    ("ricochet", DamageCause::Ricochet),
    ("piercing", DamageCause::Piercing),
    ("tow", DamageCause::Tow),
    ("mine", DamageCause::Mine),
    ("drum", DamageCause::Drum),
    ("interception", DamageCause::Interception),
    ("explosion", DamageCause::Explosion),
];

/// The wire name of `value` in a name table.
pub fn name<T: PartialEq + Copy>(table: &[(&'static str, T)], value: T) -> &'static str {
    table
        .iter()
        .find(|(_, option)| *option == value)
        .map(|(name, _)| *name)
        .expect("every enum value has a wire name")
}

// ---------------------------------------------------------------------------------------
// Host side: wire records written from the simulation.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct FieldSpan {
    key: &'static str,
    start: u32,
    end: u32,
}

/// One JSON object record with the byte span of every field's value.
#[derive(Clone, Debug, Default)]
pub struct WireRecord {
    /// The entity id; zero for records without one (the match).
    pub id: u32,
    text: String,
    fields: Vec<FieldSpan>,
}

impl WireRecord {
    /// The whole record as JSON.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Field names and their JSON values, in order.
    pub fn fields(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.fields
            .iter()
            .map(|span| (span.key, &self.text[span.start as usize..span.end as usize]))
    }

    /// Search the existing field spans without materializing another field index.
    pub(super) fn find_field(&self, key: &str, start: usize) -> Option<(usize, &str)> {
        let index = start
            + self.fields[start..]
                .iter()
                .position(|span| span.key == key)?;
        let span = &self.fields[index];
        Some((index, &self.text[span.start as usize..span.end as usize]))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|span| span.key == key)
            .map(|span| &self.text[span.start as usize..span.end as usize])
    }

    fn begin(&mut self, id: u32) -> RecordWriter<'_> {
        self.id = id;
        self.text.clear();
        self.fields.clear();
        self.text.push('{');
        RecordWriter { record: self }
    }
}

struct RecordWriter<'a> {
    record: &'a mut WireRecord,
}

impl RecordWriter<'_> {
    fn value(&mut self, key: &'static str, write: impl FnOnce(&mut String)) -> &mut Self {
        let record = &mut *self.record;
        if !record.fields.is_empty() {
            record.text.push(',');
        }
        write_str(&mut record.text, key);
        record.text.push(':');
        let start = record.text.len() as u32;
        write(&mut record.text);
        let end = record.text.len() as u32;
        record.fields.push(FieldSpan { key, start, end });
        self
    }

    fn number(&mut self, key: &'static str, value: f64) -> &mut Self {
        self.value(key, |out| write_number(out, value))
    }

    fn int(&mut self, key: &'static str, value: u64) -> &mut Self {
        self.value(key, |out| write_int(out, value))
    }

    fn string(&mut self, key: &'static str, value: &str) -> &mut Self {
        self.value(key, |out| write_str(out, value))
    }

    fn boolean(&mut self, key: &'static str, value: bool) -> &mut Self {
        self.value(key, |out| {
            out.push_str(if value { "true" } else { "false" })
        })
    }

    fn end(self) {
        self.record.text.push('}');
    }
}

fn write_vector(out: &mut String, v: Point3) {
    let mut writer = ObjectWriter::new(out);
    writer
        .number("x", json::position(v.x))
        .number("y", json::position(v.y))
        .number("z", json::position(v.z));
    writer.finish();
}

fn write_quaternion(out: &mut String, q: Quat4) {
    let mut writer = ObjectWriter::new(out);
    writer
        .number("x", json::rotation(q.x))
        .number("y", json::rotation(q.y))
        .number("z", json::rotation(q.z))
        .number("w", json::rotation(q.w));
    writer.finish();
}

fn write_tank(record: &mut WireRecord, simulation: &Simulation, tank: &Tank) {
    let mut w = record.begin(tank.id);
    w.int("id", u64::from(tank.id))
        .int("life", u64::from(tank.life))
        .string("name", &tank.name)
        .string("kind", tank.kind.as_str())
        .int("team", tank.team.index() as u64)
        .boolean("human", tank.human)
        .boolean("alive", tank.alive)
        .value("position", |out| {
            write_vector(out, simulation.tank_position(tank))
        })
        .value("velocity", |out| {
            write_vector(out, simulation.tank_velocity(tank))
        })
        .number("heading", json::rotation(tank.heading))
        .number("aim", json::rotation(tank.aim))
        .number("hp", json::value(tank.hp))
        .number("maxHp", json::value(simulation.max_health(tank)))
        .number("xp", json::value(tank.xp))
        .number("shield", json::value(tank.shield))
        .number("shieldPoints", json::value(tank.shield_points))
        .number("protection", json::value(tank.protection))
        .number("laser", json::value(tank.laser))
        .number("recoil", json::value(tank.recoil))
        .number("cooldown", json::value(tank.cooldown))
        .number("mineCooldown", json::value(tank.mine_cooldown))
        .number("respawn", json::value(tank.respawn))
        .number("rapid", json::value(tank.rapid))
        .number("speed", json::value(tank.speed))
        .string("selectedAmmo", tank.selected_ammo.as_str())
        .value("ammo", |out| {
            let mut writer = ObjectWriter::new(out);
            writer
                .number("spread", tank.ammo.spread)
                .number("rocket", tank.ammo.rocket)
                .number("ricochet", tank.ammo.ricochet)
                .number("piercing", tank.ammo.piercing);
            writer.finish();
        })
        .int("kills", u64::from(tank.kills))
        .int("deaths", u64::from(tank.deaths))
        .number("lastCombat", json::value(tank.last_combat));
    w.end();
}

fn write_cover(record: &mut WireRecord, simulation: &Simulation, cover: &Cover) {
    let has_body = simulation.world.bodies.contains(cover.body);
    let position = if has_body {
        simulation.body_translation(cover.body)
    } else {
        Point3::new(cover.x, 0.0, cover.z)
    };
    let rotation = if has_body {
        simulation.body_rotation(cover.body)
    } else {
        Quat4::IDENTITY
    };
    let mut w = record.begin(cover.id);
    w.int("id", u64::from(cover.id))
        .string("kind", name(&COVER_KINDS, cover.kind))
        .number("x", json::position(cover.x))
        .number("z", json::position(cover.z))
        .number("w", json::position(cover.w))
        .number("h", json::position(cover.h))
        .number("d", json::position(cover.d));
    if cover.hp.is_finite() {
        w.number("hp", json::value(cover.hp));
    } else {
        w.value("hp", |out| out.push_str("null"));
    }
    if cover.max_hp.is_finite() {
        w.number("maxHp", json::value(cover.max_hp));
    } else {
        w.value("maxHp", |out| out.push_str("null"));
    }
    w.boolean("alive", cover.alive)
        .boolean("destructible", cover.destructible)
        .int("color", u64::from(cover.color));
    if let Some(seed) = cover.debris_seed {
        w.number("debrisSeed", seed);
    }
    w.value("position", |out| write_vector(out, position))
        .value("rotation", |out| write_quaternion(out, rotation));
    if !cover.timber_hits.is_empty() {
        w.value("timberHits", |out| {
            out.push('[');
            for (index, hit) in cover.timber_hits.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                let mut writer = ObjectWriter::new(out);
                writer
                    .number("x", json::position(hit.x))
                    .number("y", json::position(hit.y))
                    .number("z", json::position(hit.z))
                    .number("size", json::position(hit.size));
                writer.finish();
            }
            out.push(']');
        });
    }
    if let Some(join) = cover.timber_join {
        // The Rust layout keeps plain booleans, so only set ends are sent; a missing end
        // reads as closed, as `undefined` did.
        w.value("timberJoin", |out| {
            let mut writer = ObjectWriter::new(out);
            if join.open_min {
                writer.boolean("openMin", true);
            }
            if join.open_max {
                writer.boolean("openMax", true);
            }
            if join.post {
                writer.boolean("post", true);
            }
            writer.finish();
        });
    }
    if let Some(motion) = cover.motion {
        w.value("motion", |out| {
            let mut writer = ObjectWriter::new(out);
            writer
                .number("originX", json::position(motion.origin_x))
                .number("originZ", json::position(motion.origin_z))
                .number("w", json::position(motion.w))
                .number("d", json::position(motion.d));
            writer.finish();
        });
    }
    w.end();
}

fn write_timber_part(out: &mut String, part: &TimberPart) {
    let mut writer = ObjectWriter::new(out);
    writer
        .string("kind", name(&TIMBER_PART_KINDS, part.kind))
        .int("index", part.index as u64)
        .number("x", json::position(part.x))
        .number("y", json::position(part.y))
        .number("z", json::position(part.z))
        .number("w", json::position(part.w))
        .number("h", json::position(part.h))
        .number("d", json::position(part.d))
        .number("yaw", json::rotation(part.yaw))
        .number("lean", json::rotation(part.lean))
        .int("color", u64::from(part.color))
        .number("damage", json::position(f64::from(part.damage)))
        .number("damageSeed", f64::from(part.damage_seed));
    let marks = writer.key("marks");
    marks.push('[');
    for (index, mark) in part.marks.iter().enumerate() {
        if index > 0 {
            marks.push(',');
        }
        let mut mark_writer = ObjectWriter::new(marks);
        mark_writer
            .number("x", json::position(mark.x))
            .number("y", json::position(mark.y))
            .string("face", name(&TIMBER_FACES, mark.face))
            .number("size", json::position(mark.size))
            .number("seed", f64::from(mark.seed));
        mark_writer.finish();
    }
    marks.push(']');
    writer.finish();
}

fn write_fragment(record: &mut WireRecord, simulation: &Simulation, fragment: &Fragment) {
    let mut w = record.begin(fragment.id);
    w.int("id", u64::from(fragment.id))
        // Clients read life only for the final fade, so a steady value until then keeps
        // every settled piece out of the per-frame deltas.
        .number(
            "life",
            json::value(fragment.life.min(DEBRIS_CLEANUP_SECONDS)),
        )
        .number("size", json::position(fragment.size))
        .int("color", u64::from(fragment.color))
        .value("position", |out| {
            write_vector(out, simulation.body_translation(fragment.body))
        })
        .value("rotation", |out| {
            write_quaternion(out, simulation.body_rotation(fragment.body))
        });
    if let Some(shape) = fragment.shape {
        w.string("shape", name(&FRAGMENT_SHAPES, shape));
    }
    if let Some(dimensions) = fragment.dimensions {
        w.value("dimensions", |out| write_vector(out, dimensions));
    }
    if let Some(material) = fragment.material {
        w.string("material", name(&MATERIALS, material));
    }
    if let Some(kind) = fragment.source_kind {
        w.string("sourceKind", name(&COVER_KINDS, kind));
    }
    if let Some(part) = &fragment.timber_part {
        w.value("timberPart", |out| write_timber_part(out, part));
    }
    if let Some(tree) = fragment.tree_cover_id {
        w.int("treeCoverId", u64::from(tree));
    }
    if let Some(center) = fragment.tree_center_y {
        w.number("treeCenterY", json::position(center));
    }
    if let Some(created) = fragment.created_at {
        w.number("createdAt", json::value(created));
    }
    if let Some(expires) = fragment.expires_at {
        w.number("expiresAt", json::value(expires));
    }
    if let Some(wreck) = fragment.wreck {
        w.string("wreck", wreck.as_str());
    }
    if let Some(part) = fragment.part {
        w.string("part", name(&WRECK_PARTS, part));
    }
    if let Some(team) = fragment.team {
        w.int("team", team.index() as u64);
    }
    w.end();
}

/// A shell as the wire sends it, already rounded (`shotReader` then `rounded`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireShot {
    pub id: u32,
    pub x: f64,
    pub z: f64,
    pub y: Option<f64>,
    pub visual_y: Option<f64>,
    pub team: Team,
    pub vx: f64,
    pub vz: f64,
    pub weapon: Weapon,
}

impl WireShot {
    pub fn rounded(shot: RenderShot) -> Self {
        Self {
            id: shot.id,
            x: json::position(shot.x),
            z: json::position(shot.z),
            y: shot.y.map(json::position),
            visual_y: shot.visual_y.map(json::position),
            team: shot.team,
            vx: json::position(shot.vx),
            vz: json::position(shot.vz),
            weapon: shot.weapon,
        }
    }

    pub fn write(&self, out: &mut String) {
        let mut writer = ObjectWriter::new(out);
        writer
            .int("id", u64::from(self.id))
            .number("x", self.x)
            .number("z", self.z);
        if let Some(y) = self.y {
            writer.number("y", y);
        }
        if let Some(visual) = self.visual_y {
            writer.number("visualY", visual);
        }
        writer
            .int("team", self.team.index() as u64)
            .number("vx", self.vx)
            .number("vz", self.vz)
            .string("weapon", self.weapon.as_str());
        writer.finish();
    }
}

fn write_shot(record: &mut WireRecord, shot: &crate::sim::types::Shot) {
    let wire = WireShot::rounded(RenderShot::from(shot));
    let mut w = record.begin(wire.id);
    w.int("id", u64::from(wire.id))
        .number("x", wire.x)
        .number("z", wire.z);
    if let Some(y) = wire.y {
        w.number("y", y);
    }
    if let Some(visual) = wire.visual_y {
        w.number("visualY", visual);
    }
    w.int("team", wire.team.index() as u64)
        .number("vx", wire.vx)
        .number("vz", wire.vz)
        .string("weapon", wire.weapon.as_str());
    w.end();
}

fn write_mine(record: &mut WireRecord, mine: &Mine) {
    let mut w = record.begin(mine.id);
    w.int("id", u64::from(mine.id))
        .number("x", json::position(mine.x))
        .number("z", json::position(mine.z))
        .int("owner", u64::from(mine.owner));
    if let Some(life) = mine.owner_life {
        w.int("ownerLife", u64::from(life));
    }
    if let Some(damage) = mine.damage {
        w.number("damage", json::position(damage));
    }
    w.int("team", mine.team.index() as u64)
        .number("arm", json::value(mine.arm))
        .number("life", json::value(mine.life));
    w.end();
}

fn write_pickup(record: &mut WireRecord, pickup: &Pickup) {
    let mut w = record.begin(pickup.id);
    w.int("id", u64::from(pickup.id))
        .number("x", json::position(pickup.x))
        .number("z", json::position(pickup.z))
        .string("kind", name(&PICKUP_KINDS, pickup.kind))
        .boolean("available", pickup.available)
        .number("cooldown", json::value(pickup.cooldown))
        .number("cooldownDuration", json::value(pickup.cooldown_duration));
    w.end();
}

fn write_match(record: &mut WireRecord, state: &Match) {
    let mut w = record.begin(0);
    w.string("phase", name(&MATCH_PHASES, state.phase))
        .number("time", json::value(state.time))
        .value("scores", |out| {
            out.push('[');
            write_int(out, u64::from(state.scores[0]));
            out.push(',');
            write_int(out, u64::from(state.scores[1]));
            out.push(']');
        })
        .boolean("overtime", state.overtime);
    if let Some(early) = state.ended_early {
        w.boolean("endedEarly", early);
    }
    match state.winner {
        Some(team) => w.int("winner", team.index() as u64),
        None => w.value("winner", |out| out.push_str("null")),
    };
    w.int("round", u64::from(state.round));
    w.end();
}

fn write_map(out: &mut String, simulation: &Simulation) {
    out.clear();
    let mut writer = ObjectWriter::new(out);
    writer.string("theme", simulation.map_theme());
    if let Some(floor) = simulation.map_floor() {
        writer.string("floor", name(&GROUNDS, floor));
    }
    if let Some(outer) = simulation.map_outer_floor() {
        writer.string("outerFloor", name(&GROUNDS, outer));
    }
    if let Some(extent) = simulation.map_outer_floor_extent() {
        writer.number("outerFloorExtent", json::position(extent));
    }
    if simulation.map_scale() != 1.0 {
        writer.number("scale", json::position(simulation.map_scale()));
    }
    writer.finish();
}

/// A captured scene: every entity's wire record by kind in simulation order, the match
/// and the map. Captures reuse the records' buffers.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub entities: [Vec<WireRecord>; 6],
    pub elapsed: f64,
    pub match_record: WireRecord,
    /// The map object's JSON (`theme`, and `floor`, `outerFloor`, `outerFloorExtent`,
    /// `scale` when set).
    pub map: String,
}

fn fill_records<S>(
    records: &mut Vec<WireRecord>,
    sources: &[S],
    mut write: impl FnMut(&mut WireRecord, &S),
) {
    records.truncate(sources.len());
    records.resize_with(sources.len(), WireRecord::default);
    for (record, source) in records.iter_mut().zip(sources) {
        write(record, source);
    }
}

impl Scene {
    /// Reads the simulation's entities (`captureScene`).
    pub fn capture(simulation: &Simulation) -> Scene {
        let mut scene = Scene::default();
        scene.capture_from(simulation);
        scene
    }

    /// Overwrites this scene with the simulation's current state, reusing allocations.
    pub fn capture_from(&mut self, simulation: &Simulation) {
        let [tanks, covers, fragments, shots, mines, pickups] = &mut self.entities;
        fill_records(tanks, &simulation.tanks, |record, tank| {
            write_tank(record, simulation, tank)
        });
        fill_records(covers, &simulation.covers, |record, cover| {
            write_cover(record, simulation, cover)
        });
        fill_records(fragments, &simulation.fragments, |record, fragment| {
            write_fragment(record, simulation, fragment)
        });
        fill_records(shots, &simulation.shots, write_shot);
        fill_records(mines, &simulation.mines, write_mine);
        fill_records(pickups, &simulation.pickups, write_pickup);
        self.elapsed = json::position(simulation.elapsed);
        write_match(&mut self.match_record, &simulation.match_state);
        write_map(&mut self.map, simulation);
    }

    /// The scene object's JSON: `{"entities":{...},"elapsed":...,"match":{...},"map":{...}}`.
    pub fn write(&self, out: &mut String) {
        out.push_str("{\"entities\":{");
        for (kind, records) in self.entities.iter().enumerate() {
            if kind > 0 {
                out.push(',');
            }
            write_str(out, ENTITY_TYPES[kind]);
            out.push_str(":[");
            for (index, record) in records.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(record.text());
            }
            out.push(']');
        }
        out.push_str("},\"elapsed\":");
        write_number(out, self.elapsed);
        out.push_str(",\"match\":");
        out.push_str(self.match_record.text());
        out.push_str(",\"map\":");
        out.push_str(&self.map);
        out.push('}');
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}

/// A simulation event as the wire sends it: `eventReader`'s fields in its order, rounded
/// by field name.
pub fn write_event(out: &mut String, event: &SimEvent) {
    let mut writer = ObjectWriter::new(out);
    writer
        .string("type", name(&EVENT_TYPES, event.kind))
        .number("x", json::position(event.x))
        .number("z", json::position(event.z));
    if let Some(id) = event.id {
        writer.int("id", u64::from(id));
    }
    if let Some(owner) = event.owner {
        writer.int("owner", u64::from(owner));
    }
    if let Some(life) = event.owner_life {
        writer.int("ownerLife", u64::from(life));
    }
    if let Some(weapon) = event.weapon {
        writer.string("weapon", weapon.as_str());
    }
    if let Some(team) = event.team {
        writer.int("team", team.index() as u64);
    }
    if let Some(size) = event.size {
        writer.number("size", json::position(size));
    }
    if let Some(label) = &event.label {
        writer.string("label", label);
    }
    if let Some(color) = event.color {
        writer.int("color", u64::from(color));
    }
    if let Some(from) = event.from {
        write_vector(writer.key("from"), from);
    }
    if let Some(style) = event.death_style {
        writer.string(
            "deathStyle",
            match style {
                DeathStyle::Burnout => "burnout",
            },
        );
    }
    if let Some(material) = event.material {
        writer.string("material", name(&MATERIALS, material));
    }
    if let Some(force) = event.force {
        writer.number("force", json::position(force));
    }
    if let Some(kind) = event.cover_kind {
        writer.string("coverKind", name(&COVER_KINDS, kind));
    }
    if let Some(height) = event.height {
        writer.number("height", json::position(height));
    }
    if let Some(source) = event.damage_source {
        let out = writer.key("damageSource");
        let mut nested = ObjectWriter::new(out);
        nested.string("cause", name(&DAMAGE_CAUSES, source.cause));
        let origin = nested.key("origin");
        let mut point = ObjectWriter::new(origin);
        point
            .number("x", json::position(source.origin.x))
            .number("z", json::position(source.origin.z));
        point.finish();
        nested.finish();
    }
    writer.finish();
}

// ---------------------------------------------------------------------------------------
// Client side: readers and projection.
// ---------------------------------------------------------------------------------------

fn point(value: Option<&Value>) -> ReadResult<Point3> {
    nested(value, |source| {
        Ok(Point3::new(
            field(source, "x", number)?,
            field(source, "y", number)?,
            field(source, "z", number)?,
        ))
    })
}

fn unit(value: Option<&Value>) -> ReadResult<f64> {
    number_in(value, -1.0, 1.0, false)
}

fn quaternion(value: Option<&Value>) -> ReadResult<Quat4> {
    nested(value, |source| {
        Ok(Quat4 {
            x: field(source, "x", unit)?,
            y: field(source, "y", unit)?,
            z: field(source, "z", unit)?,
            w: field(source, "w", unit)?,
        })
    })
}

/// `projectScene`'s rotation check: a near-zero quaternion is rejected, others normalized.
fn normalized(q: Quat4) -> ReadResult<Quat4> {
    let length = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    if length < 0.5 {
        return Err("Invalid rotation".into());
    }
    Ok(Quat4 {
        x: q.x / length,
        y: q.y / length,
        z: q.z / length,
        w: q.w / length,
    })
}

fn seed(value: Option<&Value>) -> ReadResult<i32> {
    number_in(value, -2_147_483_648.0, 4_294_967_295.0, true).map(to_int32)
}

fn read_weapon(value: Option<&Value>) -> ReadResult<Weapon> {
    choice(value, &WEAPONS)
}

/// `tankReader`, projected with `previous` at its position.
pub fn read_tank(source: &Record) -> ReadResult<RenderTank> {
    let ammo = |value: Option<&Value>| {
        nested(value, |ammo| {
            Ok(AmmoInventory {
                spread: field(ammo, "spread", id)? as f64,
                rocket: field(ammo, "rocket", id)? as f64,
                ricochet: field(ammo, "ricochet", id)? as f64,
                piercing: field(ammo, "piercing", id)? as f64,
            })
        })
    };
    let mut tank = RenderTank {
        id: field(source, "id", id32)?,
        life: field(source, "life", id32)?,
        name: field(source, "name", |v| string(v, 64, 0))?,
        kind: field(source, "kind", |v| choice(v, &VEHICLE_KINDS))?,
        team: field(source, "team", read_team)?,
        human: field(source, "human", boolean)?,
        alive: field(source, "alive", boolean)?,
        position: field(source, "position", point)?,
        velocity: field(source, "velocity", point)?,
        heading: field(source, "heading", number)?,
        aim: field(source, "aim", number)?,
        hp: field(source, "hp", number)?,
        max_hp: field(source, "maxHp", number)?,
        xp: field(source, "xp", number)?,
        shield: field(source, "shield", number)?,
        shield_points: field(source, "shieldPoints", number)?,
        protection: field(source, "protection", number)?,
        laser: field(source, "laser", number)?,
        recoil: field(source, "recoil", number)?,
        cooldown: field(source, "cooldown", number)?,
        mine_cooldown: field(source, "mineCooldown", number)?,
        respawn: field(source, "respawn", number)?,
        rapid: field(source, "rapid", number)?,
        speed: field(source, "speed", number)?,
        selected_ammo: field(source, "selectedAmmo", read_weapon)?,
        ammo: field(source, "ammo", ammo)?,
        kills: field(source, "kills", id32)?,
        deaths: field(source, "deaths", id32)?,
        last_combat: field(source, "lastCombat", number)?,
        previous: Vec2::ZERO,
    };
    tank.previous = Vec2::new(tank.position.x, tank.position.z);
    Ok(tank)
}

/// `coverReader`, projected: an indestructible cover's `null` hp is infinite.
pub fn read_cover(source: &Record) -> ReadResult<RenderCover> {
    let hits = |value: Option<&Value>| {
        array(value, 32, |hit| {
            nested(Some(hit), |hit| {
                Ok(TimberHit {
                    x: field(hit, "x", number)?,
                    y: field(hit, "y", number)?,
                    z: field(hit, "z", number)?,
                    size: field(hit, "size", number)?,
                })
            })
        })
    };
    let join = |value: Option<&Value>| {
        nested(value, |join| {
            Ok(TimberJoin {
                open_min: field(join, "openMin", |v| optional(v, boolean))?.unwrap_or(false),
                open_max: field(join, "openMax", |v| optional(v, boolean))?.unwrap_or(false),
                post: field(join, "post", |v| optional(v, boolean))?.unwrap_or(false),
            })
        })
    };
    let motion = |value: Option<&Value>| {
        nested(value, |motion| {
            Ok(RenderCoverMotion {
                origin_x: field(motion, "originX", number)?,
                origin_z: field(motion, "originZ", number)?,
                w: field(motion, "w", number)?,
                d: field(motion, "d", number)?,
            })
        })
    };
    let cover = RenderCover {
        id: field(source, "id", id32)?,
        kind: field(source, "kind", |v| choice(v, &COVER_KINDS))?,
        x: field(source, "x", number)?,
        z: field(source, "z", number)?,
        w: field(source, "w", number)?,
        h: field(source, "h", number)?,
        d: field(source, "d", number)?,
        hp: field(source, "hp", |v| nullable(v, number))?.unwrap_or(f64::INFINITY),
        max_hp: field(source, "maxHp", |v| nullable(v, number))?.unwrap_or(f64::INFINITY),
        alive: field(source, "alive", boolean)?,
        destructible: field(source, "destructible", boolean)?,
        color: field(source, "color", id32)?,
        debris_seed: field(source, "debrisSeed", |v| optional(v, id))?.map(|seed| seed as f64),
        position: field(source, "position", point)?,
        rotation: field(source, "rotation", quaternion)?,
        timber_hits: field(source, "timberHits", |v| optional(v, hits))?.unwrap_or_default(),
        timber_join: field(source, "timberJoin", |v| optional(v, join))?,
        motion: field(source, "motion", |v| optional(v, motion))?,
    };
    Ok(RenderCover {
        rotation: normalized(cover.rotation)?,
        ..cover
    })
}

fn read_timber_part(value: Option<&Value>) -> ReadResult<TimberPart> {
    nested(value, |part| {
        let marks = |value: Option<&Value>| {
            array(value, 32, |mark| {
                nested(Some(mark), |mark| {
                    Ok(TimberMark {
                        x: field(mark, "x", number)?,
                        y: field(mark, "y", number)?,
                        face: field(mark, "face", |v| choice(v, &TIMBER_FACES))?,
                        size: field(mark, "size", number)?,
                        seed: field(mark, "seed", seed)?,
                    })
                })
            })
        };
        Ok(TimberPart {
            kind: field(part, "kind", |v| choice(v, &TIMBER_PART_KINDS))?,
            index: field(part, "index", id)? as usize,
            x: field(part, "x", number)?,
            y: field(part, "y", number)?,
            z: field(part, "z", number)?,
            w: field(part, "w", number)?,
            h: field(part, "h", number)?,
            d: field(part, "d", number)?,
            yaw: field(part, "yaw", number)?,
            lean: field(part, "lean", number)?,
            color: field(part, "color", id32)?,
            damage: field(part, "damage", number)?.max(0.0) as u32,
            damage_seed: field(part, "damageSeed", seed)?,
            marks: field(part, "marks", marks)?,
        })
    })
}

/// `fragmentReader`, projected with a normalized rotation.
pub fn read_fragment(source: &Record) -> ReadResult<RenderFragment> {
    let fragment = RenderFragment {
        id: field(source, "id", id32)?,
        life: field(source, "life", number)?,
        size: field(source, "size", number)?,
        color: field(source, "color", id32)?,
        position: field(source, "position", point)?,
        rotation: field(source, "rotation", quaternion)?,
        shape: field(source, "shape", |v| {
            optional(v, |v| choice(v, &FRAGMENT_SHAPES))
        })?,
        dimensions: field(source, "dimensions", |v| optional(v, point))?,
        material: field(source, "material", |v| {
            optional(v, |v| choice(v, &MATERIALS))
        })?,
        source_kind: field(source, "sourceKind", |v| {
            optional(v, |v| choice(v, &COVER_KINDS))
        })?,
        timber_part: field(source, "timberPart", |v| optional(v, read_timber_part))?,
        tree_cover_id: field(source, "treeCoverId", |v| optional(v, id32))?,
        tree_center_y: field(source, "treeCenterY", |v| optional(v, number))?,
        created_at: field(source, "createdAt", |v| optional(v, number))?,
        expires_at: field(source, "expiresAt", |v| optional(v, number))?,
        wreck: field(source, "wreck", |v| {
            optional(v, |v| choice(v, &VEHICLE_KINDS))
        })?,
        part: field(source, "part", |v| optional(v, |v| choice(v, &WRECK_PARTS)))?,
        team: field(source, "team", |v| optional(v, read_team))?,
    };
    Ok(RenderFragment {
        rotation: normalized(fragment.rotation)?,
        ..fragment
    })
}

/// `shotReader`.
pub fn read_shot(source: &Record) -> ReadResult<RenderShot> {
    Ok(RenderShot {
        id: field(source, "id", id32)?,
        x: field(source, "x", number)?,
        z: field(source, "z", number)?,
        y: field(source, "y", |v| optional(v, number))?,
        visual_y: field(source, "visualY", |v| optional(v, number))?,
        team: field(source, "team", read_team)?,
        vx: field(source, "vx", number)?,
        vz: field(source, "vz", number)?,
        weapon: field(source, "weapon", read_weapon)?,
    })
}

/// `mineReader`.
pub fn read_mine(source: &Record) -> ReadResult<Mine> {
    Ok(Mine {
        id: field(source, "id", id32)?,
        x: field(source, "x", number)?,
        z: field(source, "z", number)?,
        owner: field(source, "owner", id32)?,
        owner_life: field(source, "ownerLife", |v| optional(v, id32))?,
        damage: field(source, "damage", |v| optional(v, number))?,
        team: field(source, "team", read_team)?,
        arm: field(source, "arm", number)?,
        life: field(source, "life", number)?,
    })
}

/// `pickupReader`; a missing refill duration reads as zero.
pub fn read_pickup(source: &Record) -> ReadResult<Pickup> {
    Ok(Pickup {
        id: field(source, "id", id32)?,
        x: field(source, "x", number)?,
        z: field(source, "z", number)?,
        kind: field(source, "kind", |v| choice(v, &PICKUP_KINDS))?,
        available: field(source, "available", boolean)?,
        cooldown: field(source, "cooldown", number)?,
        cooldown_duration: field(source, "cooldownDuration", |v| optional(v, number))?
            .unwrap_or(0.0),
    })
}

/// `matchReader`.
pub fn read_match(source: &Record) -> ReadResult<Match> {
    Ok(Match {
        phase: field(source, "phase", |v| choice(v, &MATCH_PHASES))?,
        time: field(source, "time", number)?,
        scores: field(source, "scores", |v| {
            let scores = array(v, 2, |item| id32(Some(item)))?;
            if scores.len() != 2 {
                return Err("Invalid scores".into());
            }
            Ok([scores[0], scores[1]])
        })?,
        overtime: field(source, "overtime", boolean)?,
        ended_early: field(source, "endedEarly", |v| optional(v, boolean))?,
        winner: field(source, "winner", |v| nullable(v, read_team))?,
        round: field(source, "round", id32)?,
    })
}

/// The scene's `map` object: a themed map's theme is its id, and an extra level's yard is
/// named by its id.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneMap {
    pub theme: MapId,
    pub floor: Option<GroundKind>,
    pub outer_floor: Option<GroundKind>,
    pub outer_floor_extent: Option<f64>,
    pub scale: Option<f64>,
}

pub fn read_map(source: &Record) -> ReadResult<SceneMap> {
    let ground = |v: Option<&Value>| optional(v, |v| choice(v, &GROUNDS));
    Ok(SceneMap {
        theme: field(source, "theme", |v| choice(v, &MAP_MODES))?,
        floor: field(source, "floor", ground)?,
        outer_floor: field(source, "outerFloor", ground)?,
        outer_floor_extent: field(source, "outerFloorExtent", |v| optional(v, number))?,
        scale: field(source, "scale", |v| {
            optional(v, |v| number_in(v, 0.1, 1.0, false))
        })?,
    })
}

/// `eventReader`.
pub fn read_event(source: &Record) -> ReadResult<SimEvent> {
    let damage_source = |value: Option<&Value>| {
        nested(value, |damage| {
            Ok(DamageSource {
                cause: field(damage, "cause", |v| choice(v, &DAMAGE_CAUSES))?,
                origin: field(damage, "origin", |v| {
                    nested(v, |origin| {
                        Ok(Vec2::new(
                            field(origin, "x", number)?,
                            field(origin, "z", number)?,
                        ))
                    })
                })?,
            })
        })
    };
    Ok(SimEvent {
        kind: field(source, "type", |v| choice(v, &EVENT_TYPES))?,
        x: field(source, "x", number)?,
        z: field(source, "z", number)?,
        id: field(source, "id", |v| optional(v, id32))?,
        // The TypeScript host marked ownerless damage with -1; the Rust simulation uses 0.
        owner: field(source, "owner", |v| {
            optional(v, |v| {
                number_in(v, -1.0, super::schema::MAX_SAFE_INTEGER, true)
            })
        })?
        .map(|owner| {
            if owner < 0.0 {
                0
            } else {
                owner.min(f64::from(u32::MAX)) as u32
            }
        }),
        owner_life: field(source, "ownerLife", |v| optional(v, id32))?,
        weapon: field(source, "weapon", |v| optional(v, read_weapon))?,
        team: field(source, "team", |v| optional(v, read_team))?,
        size: field(source, "size", |v| optional(v, number))?,
        label: field(source, "label", |v| optional(v, |v| string(v, 160, 0)))?,
        color: field(source, "color", |v| optional(v, id32))?,
        from: field(source, "from", |v| optional(v, point))?,
        death_style: field(source, "deathStyle", |v| {
            optional(v, |v| choice(v, &[("burnout", DeathStyle::Burnout)]))
        })?,
        material: field(source, "material", |v| {
            optional(v, |v| choice(v, &MATERIALS))
        })?,
        force: field(source, "force", |v| optional(v, number))?,
        cover_kind: field(source, "coverKind", |v| {
            optional(v, |v| choice(v, &COVER_KINDS))
        })?,
        height: field(source, "height", |v| optional(v, number))?,
        damage_source: field(source, "damageSource", |v| optional(v, damage_source))?,
    })
}

/// One replicated entity: its merged wire fields (for applying field deltas) and the
/// validated, projected value.
#[derive(Clone, Debug)]
pub struct Stored<T> {
    pub id: u32,
    pub wire: Record,
    pub value: T,
}

/// Records of one kind in scene order, indexed by id.
#[derive(Clone, Debug)]
pub struct EntityStore<T> {
    pub records: Vec<Stored<T>>,
    index: HashMap<u32, usize>,
}

impl<T> Default for EntityStore<T> {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<T> EntityStore<T> {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, id: u32) -> Option<&Stored<T>> {
        self.index.get(&id).map(|&index| &self.records[index])
    }

    pub fn contains(&self, id: u32) -> bool {
        self.index.contains_key(&id)
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.records.iter().map(|stored| &stored.value)
    }

    fn push(&mut self, stored: Stored<T>) -> ReadResult<()> {
        if self.index.insert(stored.id, self.records.len()).is_some() {
            return Err("Duplicate entity id".into());
        }
        self.records.push(stored);
        Ok(())
    }

    /// Replaces a record in place, or appends a new one.
    pub(crate) fn upsert(&mut self, stored: Stored<T>) {
        match self.index.get(&stored.id) {
            Some(&index) => self.records[index] = stored,
            None => {
                self.index.insert(stored.id, self.records.len());
                self.records.push(stored);
            }
        }
    }

    /// Removes records, keeping the order of the rest.
    pub(crate) fn remove_all(&mut self, ids: &[u32]) {
        if ids.is_empty() {
            return;
        }
        self.records.retain(|stored| !ids.contains(&stored.id));
        self.index.clear();
        for (index, stored) in self.records.iter().enumerate() {
            self.index.insert(stored.id, index);
        }
    }
}

/// Fields each kind's reader declares; others are dropped like the TypeScript readers
/// drop unknown properties.
pub const ENTITY_FIELDS: [&[&str]; 6] = [
    &[
        "id",
        "life",
        "name",
        "kind",
        "team",
        "human",
        "alive",
        "position",
        "velocity",
        "heading",
        "aim",
        "hp",
        "maxHp",
        "xp",
        "shield",
        "shieldPoints",
        "protection",
        "laser",
        "recoil",
        "cooldown",
        "mineCooldown",
        "respawn",
        "rapid",
        "speed",
        "selectedAmmo",
        "ammo",
        "kills",
        "deaths",
        "lastCombat",
    ],
    &[
        "id",
        "kind",
        "x",
        "z",
        "w",
        "h",
        "d",
        "hp",
        "maxHp",
        "alive",
        "destructible",
        "color",
        "debrisSeed",
        "position",
        "rotation",
        "timberHits",
        "timberJoin",
        "motion",
    ],
    &[
        "id",
        "life",
        "size",
        "color",
        "position",
        "rotation",
        "shape",
        "dimensions",
        "material",
        "sourceKind",
        "timberPart",
        "treeCoverId",
        "treeCenterY",
        "createdAt",
        "expiresAt",
        "wreck",
        "part",
        "team",
    ],
    &["id", "x", "z", "y", "visualY", "team", "vx", "vz", "weapon"],
    &[
        "id",
        "x",
        "z",
        "owner",
        "ownerLife",
        "damage",
        "team",
        "arm",
        "life",
    ],
    &[
        "id",
        "x",
        "z",
        "kind",
        "available",
        "cooldown",
        "cooldownDuration",
    ],
];
pub const MATCH_FIELDS: [&str; 7] = [
    "phase",
    "time",
    "scores",
    "overtime",
    "endedEarly",
    "winner",
    "round",
];

/// A client's copy of the replicated scene: stored wire records and projected values.
#[derive(Clone, Debug)]
pub struct MirrorScene {
    pub tanks: EntityStore<RenderTank>,
    pub covers: EntityStore<RenderCover>,
    pub fragments: EntityStore<RenderFragment>,
    pub shots: EntityStore<RenderShot>,
    pub mines: EntityStore<Mine>,
    pub pickups: EntityStore<Pickup>,
    pub elapsed: f64,
    pub match_wire: Record,
    pub match_state: Match,
    pub map_wire: Record,
    pub map: SceneMap,
}

/// Validates one entity record of `kind` and keeps only its declared fields.
pub(crate) fn read_entity<T>(
    kind: usize,
    mut wire: Record,
    read: fn(&Record) -> ReadResult<T>,
    id_of: fn(&T) -> u32,
) -> ReadResult<Stored<T>> {
    let value = read(&wire)?;
    wire.retain(|key, _| ENTITY_FIELDS[kind].contains(&key.as_str()));
    Ok(Stored {
        id: id_of(&value),
        wire,
        value,
    })
}

fn read_store<T>(
    source: &Record,
    kind: usize,
    read: fn(&Record) -> ReadResult<T>,
    id_of: fn(&T) -> u32,
) -> ReadResult<EntityStore<T>> {
    field(source, ENTITY_TYPES[kind], |value| {
        let items = array(value, ENTITY_LIMITS[kind], |item| {
            nested(Some(item), |record| {
                read_entity(kind, record.clone(), read, id_of)
            })
        })?;
        let mut store = EntityStore::default();
        for stored in items {
            store.push(stored)?;
        }
        Ok(store)
    })
}

impl MirrorScene {
    /// `sceneReader` plus `indexEntities` and `projectScene`'s checks: every record valid,
    /// no duplicate ids, rotations usable and at least one tank to view.
    pub fn read(value: Option<&Value>) -> ReadResult<MirrorScene> {
        let scene = nested(value, |source| {
            let entities = field(source, "entities", |value| {
                nested(value, |entities| {
                    Ok((
                        read_store(entities, TANKS, read_tank, |t| t.id)?,
                        read_store(entities, COVERS, read_cover, |c| c.id)?,
                        read_store(entities, FRAGMENTS, read_fragment, |f| f.id)?,
                        read_store(entities, SHOTS, read_shot, |s| s.id)?,
                        read_store(entities, MINES, read_mine, |m| m.id)?,
                        read_store(entities, PICKUPS, read_pickup, |p| p.id)?,
                    ))
                })
            })?;
            let elapsed = field(source, "elapsed", number)?;
            let (match_wire, match_state) = field(source, "match", |v| {
                nested(v, |m| {
                    let state = read_match(m)?;
                    let mut wire = m.clone();
                    wire.retain(|key, _| MATCH_FIELDS.contains(&key.as_str()));
                    Ok((wire, state))
                })
            })?;
            let (map_wire, map) = field(source, "map", |v| {
                nested(v, |m| Ok((m.clone(), read_map(m)?)))
            })?;
            let (tanks, covers, fragments, shots, mines, pickups) = entities;
            Ok(MirrorScene {
                tanks,
                covers,
                fragments,
                shots,
                mines,
                pickups,
                elapsed,
                match_wire,
                match_state,
                map_wire,
                map,
            })
        })?;
        if scene.tanks.is_empty() {
            return Err("Missing viewer".into());
        }
        Ok(scene)
    }

    /// The scene as plain JSON, for comparing a mirror with a capture.
    pub fn to_value(&self) -> Value {
        fn records<T>(store: &EntityStore<T>) -> Value {
            Value::Array(
                store
                    .records
                    .iter()
                    .map(|stored| Value::Object(stored.wire.clone()))
                    .collect(),
            )
        }
        let mut entities = Record::new();
        entities.insert("tanks".into(), records(&self.tanks));
        entities.insert("covers".into(), records(&self.covers));
        entities.insert("fragments".into(), records(&self.fragments));
        entities.insert("shots".into(), records(&self.shots));
        entities.insert("mines".into(), records(&self.mines));
        entities.insert("pickups".into(), records(&self.pickups));
        let mut scene = Record::new();
        scene.insert("entities".into(), Value::Object(entities));
        scene.insert(
            "elapsed".into(),
            serde_json::from_str(&json_number(self.elapsed)).unwrap_or(Value::Null),
        );
        scene.insert("match".into(), Value::Object(self.match_wire.clone()));
        scene.insert("map".into(), Value::Object(self.map_wire.clone()));
        Value::Object(scene)
    }

    /// `projectScene`: the render state for the tank `viewer`.
    pub fn render(&self, viewer: u32) -> ReadResult<RenderState> {
        let mut state = RenderState::default();
        self.fill_render_state(&mut state, viewer)?;
        Ok(state)
    }

    /// Fills `state` for `viewer`, reusing its allocations.
    pub fn fill_render_state(&self, state: &mut RenderState, viewer: u32) -> ReadResult<()> {
        if !self.tanks.contains(viewer) {
            return Err("Missing viewer".into());
        }
        state.viewer_id = viewer;
        fill(&mut state.tanks, &self.tanks);
        fill(&mut state.covers, &self.covers);
        fill(&mut state.fragments, &self.fragments);
        fill(&mut state.shots, &self.shots);
        fill(&mut state.mines, &self.mines);
        fill(&mut state.pickups, &self.pickups);
        state.elapsed = self.elapsed;
        state.match_state.clone_from(&self.match_state);
        let theme = self.map.theme.as_str();
        if state.map_theme != theme {
            state.map_theme.clear();
            state.map_theme.push_str(theme);
        }
        state.map_floor = self.map.floor;
        state.map_outer_floor = self.map.outer_floor;
        state.map_outer_floor_extent = self.map.outer_floor_extent;
        state.map_scale = self.map.scale.unwrap_or(1.0);
        // Clients learn a level only from the replicated theme.
        state.custom_map = Some(self.map.theme).filter(|id| is_extra_level(*id));
        Ok(())
    }
}

fn fill<T: Clone>(items: &mut Vec<T>, store: &EntityStore<T>) {
    items.truncate(store.len());
    let existing = items.len();
    for (item, stored) in items.iter_mut().zip(&store.records) {
        item.clone_from(&stored.value);
    }
    items.extend(
        store.records[existing..]
            .iter()
            .map(|stored| stored.value.clone()),
    );
}

fn json_number(value: f64) -> String {
    let mut out = String::new();
    write_number(&mut out, value);
    out
}
