//! The replicated scene (`src/net/scene-codec.ts`): what the host captures from the
//! simulation, what clients validate, and how a client projects it into the same
//! [`RenderState`] local play draws.
//!
//! Each entity, the match and the map is a [`WireRecord`] of its kind's field table, with
//! the precision the TypeScript `rounded` gave that field: positions and sizes in
//! millimetres, rotations and angles in ten-thousandths, timers and meters in hundredths.
//! Tables list the fields that change most often first, so a frame's field mask usually
//! fits one byte. The host compares records slot by slot to send only changed fields;
//! clients keep the same records and read them into typed values with the TypeScript
//! readers' limits.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::json::{self, POSITION_SCALE, ROTATION_SCALE, VALUE_SCALE};
use super::protocol::MAP_MODES;
use super::schema::{ReadResult, text_length};
use super::wire::{
    Field, FieldKind, Slot, WireReader, WireRecord, names, put_signed, put_varint, read_changes,
    wire_fields, write_changes,
};
use crate::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use crate::sim::debris_physics::DebrisMaterial;
use crate::sim::map_options::{MapId, is_extra_level};
use crate::sim::maps::GroundKind;
use crate::sim::math::{Point3, Quat4, Vec2};
use crate::sim::render_state::{
    RenderCover, RenderCoverMotion, RenderFragment, RenderState, RenderTank,
};
use crate::sim::simulation::Simulation;
use crate::sim::simulation_rules::FRAGMENT_CAPACITY;
use crate::sim::timber_layout::{
    TimberFace, TimberHit, TimberJoin, TimberMark, TimberPart, TimberPartKind,
};
use crate::sim::tower_layout::TowerPiece;
use crate::sim::types::{
    AmmoInventory, Cover, CoverKind, DamageCause, DamageSource, DeathStyle, Fragment,
    FragmentShape, Match, MatchPhase, Mine, Pickup, PickupKind, SimEvent, SimEventType, Tank, Team,
    VehicleKind, Weapon, WreckPart,
};

/// Entity kinds in wire order; the index of a kind in every per-kind array. Shells travel
/// as trajectories instead (`shot_paths`), not as per-frame records.
pub const ENTITY_TYPES: [&str; 5] = ["tanks", "covers", "fragments", "mines", "pickups"];
pub const TANKS: usize = 0;
pub const COVERS: usize = 1;
pub const FRAGMENTS: usize = 2;
pub const MINES: usize = 3;
pub const PICKUPS: usize = 4;
/// Most records of each kind a scene may hold. Standard rooms field 12 tanks; the extra
/// levels field 30.
pub const ENTITY_LIMITS: [usize; 5] = [32, 1024, FRAGMENT_CAPACITY, 256, 64];

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
pub const TOWER_PIECES: [(&str, TowerPiece); 12] = [
    ("west-bent", TowerPiece::WestBent),
    ("east-bent", TowerPiece::EastBent),
    ("back-bracing", TowerPiece::BackBracing),
    ("front-bracing", TowerPiece::FrontBracing),
    ("west-deck", TowerPiece::WestDeck),
    ("east-deck", TowerPiece::EastDeck),
    ("front-wall", TowerPiece::FrontWall),
    ("back-wall", TowerPiece::BackWall),
    ("west-wall", TowerPiece::WestWall),
    ("east-wall", TowerPiece::EastWall),
    ("roof", TowerPiece::Roof),
    ("ladder", TowerPiece::Ladder),
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
const DEATH_STYLES: [(&str, DeathStyle); 1] = [("burnout", DeathStyle::Burnout)];

/// The wire name of `value` in a name table.
pub fn name<T: PartialEq + Copy>(table: &[(&'static str, T)], value: T) -> &'static str {
    table[index_of(table, value)].0
}

fn index_of<T: PartialEq + Copy>(table: &[(&'static str, T)], value: T) -> usize {
    table
        .iter()
        .position(|(_, option)| *option == value)
        .expect("every enum value has a wire name")
}

// ---------------------------------------------------------------------------------------
// Field tables.
// ---------------------------------------------------------------------------------------

const POSITION: FieldKind = FieldKind::Fixed(POSITION_SCALE);
const ROTATION: FieldKind = FieldKind::Fixed(ROTATION_SCALE);
const VALUE: FieldKind = FieldKind::Fixed(VALUE_SCALE);
const COUNT: FieldKind = FieldKind::Count;
const FLAG: FieldKind = FieldKind::Flag;
/// Tank and player names are at most 64 UTF-16 units; four UTF-8 bytes each at most.
const NAME_BYTES: usize = 256;
/// Event labels (kill-feed notices) are at most 160 UTF-16 units.
const LABEL_BYTES: usize = 640;
const MAX_TIMBER_HITS: usize = 32;
const MAX_TIMBER_MARKS: usize = 32;
const TIMBER_HITS_BYTES: usize = MAX_TIMBER_HITS * 4 * 10 + 10;
const TIMBER_PART_BYTES: usize = 256 + MAX_TIMBER_MARKS * 50;
const SCORES_BYTES: usize = 20;

const VEHICLE_NAMES: [&str; 4] = names(&VEHICLE_KINDS);
const WEAPON_NAMES: [&str; 6] = names(&WEAPONS);
const COVER_NAMES: [&str; 13] = names(&COVER_KINDS);
const MATERIAL_NAMES: [&str; 3] = names(&MATERIALS);
const TOWER_NAMES: [&str; 12] = names(&TOWER_PIECES);
const SHAPE_NAMES: [&str; 10] = names(&FRAGMENT_SHAPES);
const WRECK_NAMES: [&str; 5] = names(&WRECK_PARTS);
const PICKUP_NAMES: [&str; 9] = names(&PICKUP_KINDS);
const PHASE_NAMES: [&str; 4] = names(&MATCH_PHASES);
const GROUND_NAMES: [&str; 2] = names(&GROUNDS);
const MAP_NAMES: [&str; 5] = names(&MAP_MODES);
const EVENT_NAMES: [&str; 13] = names(&EVENT_TYPES);
const CAUSE_NAMES: [&str; 10] = names(&DAMAGE_CAUSES);
const DEATH_STYLE_NAMES: [&str; 1] = names(&DEATH_STYLES);

wire_fields!(
    /// `tankReader`'s fields.
    TANK_FIELDS, tank {
        POSITION_X = Field::new("position.x", POSITION),
        POSITION_Z = Field::new("position.z", POSITION),
        HEADING = Field::new("heading", ROTATION),
        AIM = Field::new("aim", ROTATION),
        VELOCITY_X = Field::new("velocity.x", POSITION),
        VELOCITY_Z = Field::new("velocity.z", POSITION),
        DRIVE_X = Field::new("drive.x", VALUE),
        DRIVE_Z = Field::new("drive.z", VALUE),
        COOLDOWN = Field::new("cooldown", VALUE),
        RECOIL = Field::new("recoil", VALUE),
        POSITION_Y = Field::new("position.y", POSITION),
        VELOCITY_Y = Field::new("velocity.y", POSITION),
        LAST_COMBAT = Field::new("lastCombat", VALUE),
        SPEED = Field::new("speed", VALUE),
        MINE_COOLDOWN = Field::new("mineCooldown", VALUE),
        HP = Field::new("hp", VALUE),
        SHIELD = Field::new("shield", VALUE),
        SHIELD_POINTS = Field::new("shieldPoints", VALUE),
        PROTECTION = Field::new("protection", VALUE),
        LASER = Field::new("laser", VALUE),
        RESPAWN = Field::new("respawn", VALUE),
        RAPID = Field::new("rapid", VALUE),
        XP = Field::new("xp", VALUE),
        MAX_HP = Field::new("maxHp", VALUE),
        ALIVE = Field::new("alive", FLAG),
        LIFE = Field::new("life", COUNT),
        KILLS = Field::new("kills", COUNT),
        DEATHS = Field::new("deaths", COUNT),
        SELECTED_AMMO = Field::new("selectedAmmo", FieldKind::Choice(&WEAPON_NAMES)),
        AMMO_SPREAD = Field::new("ammo.spread", COUNT),
        AMMO_ROCKET = Field::new("ammo.rocket", COUNT),
        AMMO_RICOCHET = Field::new("ammo.ricochet", COUNT),
        AMMO_PIERCING = Field::new("ammo.piercing", COUNT),
        HUMAN = Field::new("human", FLAG),
        NAME = Field::new("name", FieldKind::Text(NAME_BYTES)),
        KIND = Field::new("kind", FieldKind::Choice(&VEHICLE_NAMES)),
        TEAM = Field::new("team", COUNT),
    }
);

wire_fields!(
    /// `coverReader`'s fields; `hp` and `maxHp` are absent (null) for indestructible cover.
    COVER_FIELDS, cover {
        POSITION_X = Field::new("position.x", POSITION),
        POSITION_Y = Field::new("position.y", POSITION),
        POSITION_Z = Field::new("position.z", POSITION),
        ROTATION_X = Field::new("rotation.x", ROTATION),
        ROTATION_Y = Field::new("rotation.y", ROTATION),
        ROTATION_Z = Field::new("rotation.z", ROTATION),
        ROTATION_W = Field::new("rotation.w", ROTATION),
        HP = Field::nullable("hp", VALUE),
        ALIVE = Field::new("alive", FLAG),
        TIMBER_HITS = Field::new(
            "timberHits",
            FieldKind::Blob(TIMBER_HITS_BYTES, timber_hits_json)
        ),
        MAX_HP = Field::nullable("maxHp", VALUE),
        X = Field::new("x", POSITION),
        Z = Field::new("z", POSITION),
        W = Field::new("w", POSITION),
        H = Field::new("h", POSITION),
        D = Field::new("d", POSITION),
        KIND = Field::new("kind", FieldKind::Choice(&COVER_NAMES)),
        DESTRUCTIBLE = Field::new("destructible", FLAG),
        COLOR = Field::new("color", COUNT),
        DEBRIS_SEED = Field::new("debrisSeed", COUNT),
        TIMBER_JOIN = Field::new("timberJoin", FieldKind::Blob(1, timber_join_json)),
        MOTION_ORIGIN_X = Field::new("motion.originX", POSITION),
        MOTION_ORIGIN_Z = Field::new("motion.originZ", POSITION),
        MOTION_W = Field::new("motion.w", POSITION),
        MOTION_D = Field::new("motion.d", POSITION),
    }
);

wire_fields!(
    /// `fragmentReader`'s fields.
    FRAGMENT_FIELDS, fragment {
        POSITION_X = Field::new("position.x", POSITION),
        POSITION_Y = Field::new("position.y", POSITION),
        POSITION_Z = Field::new("position.z", POSITION),
        ROTATION_X = Field::new("rotation.x", ROTATION),
        ROTATION_Y = Field::new("rotation.y", ROTATION),
        ROTATION_Z = Field::new("rotation.z", ROTATION),
        ROTATION_W = Field::new("rotation.w", ROTATION),
        LIFE = Field::new("life", VALUE),
        SIZE = Field::new("size", POSITION),
        COLOR = Field::new("color", COUNT),
        SHAPE = Field::new("shape", FieldKind::Choice(&SHAPE_NAMES)),
        DIMENSIONS_X = Field::new("dimensions.x", POSITION),
        DIMENSIONS_Y = Field::new("dimensions.y", POSITION),
        DIMENSIONS_Z = Field::new("dimensions.z", POSITION),
        MATERIAL = Field::new("material", FieldKind::Choice(&MATERIAL_NAMES)),
        SOURCE_KIND = Field::new("sourceKind", FieldKind::Choice(&COVER_NAMES)),
        TIMBER_PART = Field::new(
            "timberPart",
            FieldKind::Blob(TIMBER_PART_BYTES, timber_part_json)
        ),
        TOWER_PIECE = Field::new("towerPiece", FieldKind::Choice(&TOWER_NAMES)),
        TREE_COVER_ID = Field::new("treeCoverId", COUNT),
        TREE_CENTER_Y = Field::new("treeCenterY", POSITION),
        CREATED_AT = Field::new("createdAt", VALUE),
        EXPIRES_AT = Field::new("expiresAt", VALUE),
        WRECK = Field::new("wreck", FieldKind::Choice(&VEHICLE_NAMES)),
        PART = Field::new("part", FieldKind::Choice(&WRECK_NAMES)),
        TEAM = Field::new("team", COUNT),
    }
);

wire_fields!(
    /// `mineReader`'s fields.
    MINE_FIELDS, mine {
        ARM = Field::new("arm", VALUE),
        LIFE = Field::new("life", VALUE),
        X = Field::new("x", POSITION),
        Z = Field::new("z", POSITION),
        OWNER = Field::new("owner", COUNT),
        OWNER_LIFE = Field::new("ownerLife", COUNT),
        DAMAGE = Field::new("damage", POSITION),
        TEAM = Field::new("team", COUNT),
    }
);

wire_fields!(
    /// `pickupReader`'s fields.
    PICKUP_FIELDS, pickup {
        COOLDOWN = Field::new("cooldown", VALUE),
        AVAILABLE = Field::new("available", FLAG),
        X = Field::new("x", POSITION),
        Z = Field::new("z", POSITION),
        KIND = Field::new("kind", FieldKind::Choice(&PICKUP_NAMES)),
        COOLDOWN_DURATION = Field::new("cooldownDuration", VALUE),
    }
);

wire_fields!(
    /// `matchReader`'s fields; `winner` is absent (null) until a team wins.
    MATCH_FIELDS, match_record {
        TIME = Field::new("time", VALUE),
        PHASE = Field::new("phase", FieldKind::Choice(&PHASE_NAMES)),
        SCORES = Field::new("scores", FieldKind::Blob(SCORES_BYTES, scores_json)),
        OVERTIME = Field::new("overtime", FLAG),
        ENDED_EARLY = Field::new("endedEarly", FLAG),
        WINNER = Field::nullable("winner", COUNT),
        ROUND = Field::new("round", COUNT),
    }
);

wire_fields!(
    /// The scene's `map` object.
    MAP_FIELDS, map {
        THEME = Field::new("theme", FieldKind::Choice(&MAP_NAMES)),
        FLOOR = Field::new("floor", FieldKind::Choice(&GROUND_NAMES)),
        OUTER_FLOOR = Field::new("outerFloor", FieldKind::Choice(&GROUND_NAMES)),
        OUTER_FLOOR_EXTENT = Field::new("outerFloorExtent", POSITION),
        SCALE = Field::new("scale", POSITION),
    }
);

wire_fields!(
    /// `eventReader`'s fields.
    EVENT_FIELDS, event {
        TYPE = Field::new("type", FieldKind::Choice(&EVENT_NAMES)),
        X = Field::new("x", POSITION),
        Z = Field::new("z", POSITION),
        ID = Field::new("id", COUNT),
        OWNER = Field::new("owner", COUNT),
        OWNER_LIFE = Field::new("ownerLife", COUNT),
        WEAPON = Field::new("weapon", FieldKind::Choice(&WEAPON_NAMES)),
        TEAM = Field::new("team", COUNT),
        SIZE = Field::new("size", POSITION),
        LABEL = Field::new("label", FieldKind::Text(LABEL_BYTES)),
        COLOR = Field::new("color", COUNT),
        FROM_X = Field::new("from.x", POSITION),
        FROM_Y = Field::new("from.y", POSITION),
        FROM_Z = Field::new("from.z", POSITION),
        DEATH_STYLE = Field::new("deathStyle", FieldKind::Choice(&DEATH_STYLE_NAMES)),
        MATERIAL = Field::new("material", FieldKind::Choice(&MATERIAL_NAMES)),
        FORCE = Field::new("force", POSITION),
        COVER_KIND = Field::new("coverKind", FieldKind::Choice(&COVER_NAMES)),
        HEIGHT = Field::new("height", POSITION),
        DAMAGE_CAUSE = Field::new("damageSource.cause", FieldKind::Choice(&CAUSE_NAMES)),
        DAMAGE_ORIGIN_X = Field::new("damageSource.origin.x", POSITION),
        DAMAGE_ORIGIN_Z = Field::new("damageSource.origin.z", POSITION),
    }
);

/// Each entity kind's field table, by kind index.
pub const ENTITY_FIELDS: [&[Field]; 5] = [
    TANK_FIELDS,
    COVER_FIELDS,
    FRAGMENT_FIELDS,
    MINE_FIELDS,
    PICKUP_FIELDS,
];

// ---------------------------------------------------------------------------------------
// Nested structures, sent whole as blobs.
// ---------------------------------------------------------------------------------------

fn put_position(out: &mut Vec<u8>, value: f64) {
    put_signed(out, super::wire::units(value, POSITION_SCALE));
}

fn put_rotation(out: &mut Vec<u8>, value: f64) {
    put_signed(out, super::wire::units(value, ROTATION_SCALE));
}

fn read_position(reader: &mut WireReader<'_>) -> ReadResult<f64> {
    Ok(reader.signed()? as f64 / POSITION_SCALE)
}

fn read_rotation(reader: &mut WireReader<'_>) -> ReadResult<f64> {
    Ok(reader.signed()? as f64 / ROTATION_SCALE)
}

fn read_seed(reader: &mut WireReader<'_>) -> ReadResult<i32> {
    i32::try_from(reader.signed()?).map_err(|_| "Invalid number".into())
}

fn read_index<T: Copy>(reader: &mut WireReader<'_>, table: &[(&str, T)]) -> ReadResult<T> {
    table
        .get(reader.varint()? as usize)
        .map(|(_, value)| *value)
        .ok_or_else(|| "Invalid choice".into())
}

fn finished<T>(reader: &WireReader<'_>, value: T) -> ReadResult<T> {
    if reader.is_empty() {
        Ok(value)
    } else {
        Err("Invalid list".into())
    }
}

fn write_timber_hits(out: &mut Vec<u8>, hits: &[TimberHit]) {
    put_varint(out, hits.len() as u64);
    for hit in hits {
        put_position(out, hit.x);
        put_position(out, hit.y);
        put_position(out, hit.z);
        put_position(out, hit.size);
    }
}

fn read_timber_hits(bytes: &[u8]) -> ReadResult<Vec<TimberHit>> {
    let mut reader = WireReader::new(bytes);
    let count = reader.varint()?;
    if count > MAX_TIMBER_HITS as u64 {
        return Err("Invalid list".into());
    }
    let mut hits = Vec::with_capacity(count as usize);
    for _ in 0..count {
        hits.push(TimberHit {
            x: read_position(&mut reader)?,
            y: read_position(&mut reader)?,
            z: read_position(&mut reader)?,
            size: read_position(&mut reader)?,
        });
    }
    finished(&reader, hits)
}

fn timber_hits_json(bytes: &[u8]) -> ReadResult<Value> {
    Ok(Value::Array(
        read_timber_hits(bytes)?
            .iter()
            .map(|hit| {
                json!({
                    "x": json_number(hit.x), "y": json_number(hit.y),
                    "z": json_number(hit.z), "size": json_number(hit.size),
                })
            })
            .collect(),
    ))
}

const OPEN_MIN: u8 = 1;
const OPEN_MAX: u8 = 2;
const POST: u8 = 4;

fn read_timber_join(bytes: &[u8]) -> ReadResult<TimberJoin> {
    match bytes {
        [bits] if bits & !(OPEN_MIN | OPEN_MAX | POST) == 0 => Ok(TimberJoin {
            open_min: bits & OPEN_MIN != 0,
            open_max: bits & OPEN_MAX != 0,
            post: bits & POST != 0,
        }),
        _ => Err("Expected object".into()),
    }
}

/// Only set ends travel, as the JSON protocol sent them; a missing end reads as closed.
fn timber_join_json(bytes: &[u8]) -> ReadResult<Value> {
    let join = read_timber_join(bytes)?;
    let mut object = serde_json::Map::new();
    for (key, set) in [
        ("openMin", join.open_min),
        ("openMax", join.open_max),
        ("post", join.post),
    ] {
        if set {
            object.insert(key.into(), Value::Bool(true));
        }
    }
    Ok(Value::Object(object))
}

fn write_timber_part(out: &mut Vec<u8>, part: &TimberPart) {
    put_varint(out, index_of(&TIMBER_PART_KINDS, part.kind) as u64);
    put_varint(out, part.index as u64);
    for value in [part.x, part.y, part.z, part.w, part.h, part.d] {
        put_position(out, value);
    }
    put_rotation(out, part.yaw);
    put_rotation(out, part.lean);
    put_varint(out, u64::from(part.color));
    put_varint(out, u64::from(part.damage));
    put_signed(out, i64::from(part.damage_seed));
    put_varint(out, part.marks.len() as u64);
    for mark in &part.marks {
        put_position(out, mark.x);
        put_position(out, mark.y);
        put_varint(out, index_of(&TIMBER_FACES, mark.face) as u64);
        put_position(out, mark.size);
        put_signed(out, i64::from(mark.seed));
    }
}

fn read_timber_part(bytes: &[u8]) -> ReadResult<TimberPart> {
    let mut reader = WireReader::new(bytes);
    let reader = &mut reader;
    let kind = read_index(reader, &TIMBER_PART_KINDS)?;
    let index = reader.varint()? as usize;
    let mut part = TimberPart {
        kind,
        index,
        x: read_position(reader)?,
        y: read_position(reader)?,
        z: read_position(reader)?,
        w: read_position(reader)?,
        h: read_position(reader)?,
        d: read_position(reader)?,
        yaw: read_rotation(reader)?,
        lean: read_rotation(reader)?,
        color: reader.varint32()?,
        damage: reader.varint32()?,
        damage_seed: read_seed(reader)?,
        marks: Vec::new(),
    };
    let count = reader.varint()?;
    if count > MAX_TIMBER_MARKS as u64 {
        return Err("Invalid list".into());
    }
    for _ in 0..count {
        part.marks.push(TimberMark {
            x: read_position(reader)?,
            y: read_position(reader)?,
            face: read_index(reader, &TIMBER_FACES)?,
            size: read_position(reader)?,
            seed: read_seed(reader)?,
        });
    }
    finished(reader, part)
}

fn timber_part_json(bytes: &[u8]) -> ReadResult<Value> {
    let part = read_timber_part(bytes)?;
    Ok(json!({
        "kind": name(&TIMBER_PART_KINDS, part.kind),
        "index": part.index,
        "x": json_number(part.x), "y": json_number(part.y), "z": json_number(part.z),
        "w": json_number(part.w), "h": json_number(part.h), "d": json_number(part.d),
        "yaw": json_number(part.yaw), "lean": json_number(part.lean),
        "color": part.color,
        "damage": part.damage,
        "damageSeed": part.damage_seed,
        "marks": part.marks.iter().map(|mark| json!({
            "x": json_number(mark.x), "y": json_number(mark.y),
            "face": name(&TIMBER_FACES, mark.face),
            "size": json_number(mark.size), "seed": mark.seed,
        })).collect::<Vec<_>>(),
    }))
}

fn read_scores(bytes: &[u8]) -> ReadResult<[u32; 2]> {
    let mut reader = WireReader::new(bytes);
    let scores = [reader.varint32()?, reader.varint32()?];
    finished(&reader, scores)
}

fn scores_json(bytes: &[u8]) -> ReadResult<Value> {
    Ok(json!(read_scores(bytes)?))
}

/// A number as JavaScript would hold it: integers without a fraction.
pub(crate) fn json_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9e15 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

// ---------------------------------------------------------------------------------------
// Host side: wire records written from the simulation.
// ---------------------------------------------------------------------------------------

fn set_point(record: &mut WireRecord, first: usize, point: Point3) {
    record.set_fixed(first, point.x, POSITION_SCALE);
    record.set_fixed(first + 1, point.y, POSITION_SCALE);
    record.set_fixed(first + 2, point.z, POSITION_SCALE);
}

fn set_optional(record: &mut WireRecord, field: usize, value: Option<f64>, scale: f64) {
    if let Some(value) = value {
        record.set_fixed(field, value, scale);
    }
}

fn set_choice<T: PartialEq + Copy>(
    record: &mut WireRecord,
    field: usize,
    table: &[(&'static str, T)],
    value: Option<T>,
) {
    if let Some(value) = value {
        record.set_number(field, index_of(table, value) as i64);
    }
}

fn write_tank(record: &mut WireRecord, simulation: &Simulation, tank: &Tank) {
    use tank::*;
    record.reset(tank.id, TANK_FIELDS.len());
    let position = simulation.tank_position(tank);
    record.set_fixed(POSITION_X, position.x, POSITION_SCALE);
    record.set_fixed(POSITION_Y, position.y, POSITION_SCALE);
    record.set_fixed(POSITION_Z, position.z, POSITION_SCALE);
    let velocity = simulation.tank_velocity(tank);
    record.set_fixed(VELOCITY_X, velocity.x, POSITION_SCALE);
    record.set_fixed(VELOCITY_Y, velocity.y, POSITION_SCALE);
    record.set_fixed(VELOCITY_Z, velocity.z, POSITION_SCALE);
    record.set_fixed(HEADING, tank.heading, ROTATION_SCALE);
    record.set_fixed(AIM, tank.aim, ROTATION_SCALE);
    // The held move input, for client prediction to drive this tank with.
    let (move_x, move_z) = if tank.alive {
        (tank.command.move_x, tank.command.move_z)
    } else {
        (0.0, 0.0)
    };
    record.set_fixed(DRIVE_X, move_x, VALUE_SCALE);
    record.set_fixed(DRIVE_Z, move_z, VALUE_SCALE);
    for (field, value) in [
        (COOLDOWN, tank.cooldown),
        (RECOIL, tank.recoil),
        (LAST_COMBAT, tank.last_combat),
        (SPEED, tank.speed),
        (MINE_COOLDOWN, tank.mine_cooldown),
        (HP, tank.hp),
        (SHIELD, tank.shield),
        (SHIELD_POINTS, tank.shield_points),
        (PROTECTION, tank.protection),
        (LASER, tank.laser),
        (RESPAWN, tank.respawn),
        (RAPID, tank.rapid),
        (XP, tank.xp),
        (MAX_HP, simulation.max_health(tank)),
    ] {
        record.set_fixed(field, value, VALUE_SCALE);
    }
    record.set_flag(ALIVE, tank.alive);
    record.set_count(LIFE, u64::from(tank.life));
    record.set_count(KILLS, u64::from(tank.kills));
    record.set_count(DEATHS, u64::from(tank.deaths));
    set_choice(record, SELECTED_AMMO, &WEAPONS, Some(tank.selected_ammo));
    // Ammunition counts are whole rounds.
    record.set_count(AMMO_SPREAD, tank.ammo.spread as u64);
    record.set_count(AMMO_ROCKET, tank.ammo.rocket as u64);
    record.set_count(AMMO_RICOCHET, tank.ammo.ricochet as u64);
    record.set_count(AMMO_PIERCING, tank.ammo.piercing as u64);
    record.set_flag(HUMAN, tank.human);
    record.set_text(NAME, &tank.name);
    set_choice(record, KIND, &VEHICLE_KINDS, Some(tank.kind));
    record.set_count(TEAM, tank.team.index() as u64);
}

fn write_cover(record: &mut WireRecord, simulation: &Simulation, cover: &Cover) {
    use cover::*;
    record.reset(cover.id, COVER_FIELDS.len());
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
    set_point(record, POSITION_X, position);
    for (field, value) in [
        (ROTATION_X, rotation.x),
        (ROTATION_Y, rotation.y),
        (ROTATION_Z, rotation.z),
        (ROTATION_W, rotation.w),
    ] {
        record.set_fixed(field, value, ROTATION_SCALE);
    }
    // Indestructible cover has infinite health, which travels as null.
    set_optional(
        record,
        HP,
        Some(cover.hp).filter(|hp| hp.is_finite()),
        VALUE_SCALE,
    );
    set_optional(
        record,
        MAX_HP,
        Some(cover.max_hp).filter(|hp| hp.is_finite()),
        VALUE_SCALE,
    );
    record.set_flag(ALIVE, cover.alive);
    if cover.timber_hits.is_empty() {
        record.clear(TIMBER_HITS);
    } else {
        record.set_blob(TIMBER_HITS, |out| {
            write_timber_hits(out, &cover.timber_hits)
        });
    }
    for (field, value) in [
        (X, cover.x),
        (Z, cover.z),
        (W, cover.w),
        (H, cover.h),
        (D, cover.d),
    ] {
        record.set_fixed(field, value, POSITION_SCALE);
    }
    set_choice(record, KIND, &COVER_KINDS, Some(cover.kind));
    record.set_flag(DESTRUCTIBLE, cover.destructible);
    record.set_count(COLOR, u64::from(cover.color));
    // Debris seeds are whole numbers.
    if let Some(seed) = cover.debris_seed {
        record.set_count(DEBRIS_SEED, seed as u64);
    }
    match cover.timber_join {
        Some(join) => record.set_blob(TIMBER_JOIN, |out| {
            let mut bits = 0;
            for (set, bit) in [
                (join.open_min, OPEN_MIN),
                (join.open_max, OPEN_MAX),
                (join.post, POST),
            ] {
                if set {
                    bits |= bit;
                }
            }
            out.push(bits);
        }),
        None => record.clear(TIMBER_JOIN),
    }
    if let Some(motion) = cover.motion {
        record.set_fixed(MOTION_ORIGIN_X, motion.origin_x, POSITION_SCALE);
        record.set_fixed(MOTION_ORIGIN_Z, motion.origin_z, POSITION_SCALE);
        record.set_fixed(MOTION_W, motion.w, POSITION_SCALE);
        record.set_fixed(MOTION_D, motion.d, POSITION_SCALE);
    }
}

fn write_fragment(record: &mut WireRecord, simulation: &Simulation, fragment: &Fragment) {
    use fragment::*;
    record.reset(fragment.id, FRAGMENT_FIELDS.len());
    set_point(
        record,
        POSITION_X,
        simulation.body_translation(fragment.body),
    );
    let rotation = simulation.body_rotation(fragment.body);
    for (field, value) in [
        (ROTATION_X, rotation.x),
        (ROTATION_Y, rotation.y),
        (ROTATION_Z, rotation.z),
        (ROTATION_W, rotation.w),
    ] {
        record.set_fixed(field, value, ROTATION_SCALE);
    }
    // Clients read life only for the final fade, so a steady value until then keeps
    // every settled piece out of the per-frame deltas.
    record.set_fixed(LIFE, fragment.life.min(DEBRIS_CLEANUP_SECONDS), VALUE_SCALE);
    record.set_fixed(SIZE, fragment.size, POSITION_SCALE);
    record.set_count(COLOR, u64::from(fragment.color));
    set_choice(record, SHAPE, &FRAGMENT_SHAPES, fragment.shape);
    if let Some(dimensions) = fragment.dimensions {
        set_point(record, DIMENSIONS_X, dimensions);
    }
    set_choice(record, MATERIAL, &MATERIALS, fragment.material);
    set_choice(record, SOURCE_KIND, &COVER_KINDS, fragment.source_kind);
    match &fragment.timber_part {
        Some(part) => record.set_blob(TIMBER_PART, |out| write_timber_part(out, part)),
        None => record.clear(TIMBER_PART),
    }
    set_choice(record, TOWER_PIECE, &TOWER_PIECES, fragment.tower_piece);
    if let Some(tree) = fragment.tree_cover_id {
        record.set_count(TREE_COVER_ID, u64::from(tree));
    }
    set_optional(
        record,
        TREE_CENTER_Y,
        fragment.tree_center_y,
        POSITION_SCALE,
    );
    set_optional(record, CREATED_AT, fragment.created_at, VALUE_SCALE);
    set_optional(record, EXPIRES_AT, fragment.expires_at, VALUE_SCALE);
    set_choice(record, WRECK, &VEHICLE_KINDS, fragment.wreck);
    set_choice(record, PART, &WRECK_PARTS, fragment.part);
    if let Some(team) = fragment.team {
        record.set_count(TEAM, team.index() as u64);
    }
}

fn write_mine(record: &mut WireRecord, mine: &Mine) {
    use mine::*;
    record.reset(mine.id, MINE_FIELDS.len());
    record.set_fixed(ARM, mine.arm, VALUE_SCALE);
    record.set_fixed(LIFE, mine.life, VALUE_SCALE);
    record.set_fixed(X, mine.x, POSITION_SCALE);
    record.set_fixed(Z, mine.z, POSITION_SCALE);
    record.set_count(OWNER, u64::from(mine.owner));
    if let Some(life) = mine.owner_life {
        record.set_count(OWNER_LIFE, u64::from(life));
    }
    set_optional(record, DAMAGE, mine.damage, POSITION_SCALE);
    record.set_count(TEAM, mine.team.index() as u64);
}

fn write_pickup(record: &mut WireRecord, pickup: &Pickup) {
    use pickup::*;
    record.reset(pickup.id, PICKUP_FIELDS.len());
    record.set_fixed(COOLDOWN, pickup.cooldown, VALUE_SCALE);
    record.set_flag(AVAILABLE, pickup.available);
    record.set_fixed(X, pickup.x, POSITION_SCALE);
    record.set_fixed(Z, pickup.z, POSITION_SCALE);
    set_choice(record, KIND, &PICKUP_KINDS, Some(pickup.kind));
    record.set_fixed(COOLDOWN_DURATION, pickup.cooldown_duration, VALUE_SCALE);
}

fn write_match(record: &mut WireRecord, state: &Match) {
    use match_record::*;
    record.reset(0, MATCH_FIELDS.len());
    record.set_fixed(TIME, state.time, VALUE_SCALE);
    set_choice(record, PHASE, &MATCH_PHASES, Some(state.phase));
    record.set_blob(SCORES, |out| {
        put_varint(out, u64::from(state.scores[0]));
        put_varint(out, u64::from(state.scores[1]));
    });
    record.set_flag(OVERTIME, state.overtime);
    if let Some(early) = state.ended_early {
        record.set_flag(ENDED_EARLY, early);
    }
    if let Some(team) = state.winner {
        record.set_count(WINNER, team.index() as u64);
    }
    record.set_count(ROUND, u64::from(state.round));
}

fn write_map(record: &mut WireRecord, simulation: &Simulation) {
    use map::*;
    record.reset(0, MAP_FIELDS.len());
    let theme = MAP_MODES
        .iter()
        .position(|(name, _)| *name == simulation.map_theme())
        .expect("every map theme is a map mode");
    record.set_number(THEME, theme as i64);
    set_choice(record, FLOOR, &GROUNDS, simulation.map_floor());
    set_choice(record, OUTER_FLOOR, &GROUNDS, simulation.map_outer_floor());
    set_optional(
        record,
        OUTER_FLOOR_EXTENT,
        simulation.map_outer_floor_extent(),
        POSITION_SCALE,
    );
    set_optional(
        record,
        SCALE,
        Some(simulation.map_scale()).filter(|scale| *scale != 1.0),
        POSITION_SCALE,
    );
}

/// A simulation event as the wire sends it, rounded by field.
pub fn write_event(record: &mut WireRecord, event: &SimEvent) {
    use event::*;
    record.reset(0, EVENT_FIELDS.len());
    set_choice(record, TYPE, &EVENT_TYPES, Some(event.kind));
    record.set_fixed(X, event.x, POSITION_SCALE);
    record.set_fixed(Z, event.z, POSITION_SCALE);
    for (field, value) in [
        (ID, event.id),
        (OWNER, event.owner),
        (OWNER_LIFE, event.owner_life),
        (COLOR, event.color),
    ] {
        if let Some(value) = value {
            record.set_count(field, u64::from(value));
        }
    }
    set_choice(record, WEAPON, &WEAPONS, event.weapon);
    if let Some(team) = event.team {
        record.set_count(TEAM, team.index() as u64);
    }
    set_optional(record, SIZE, event.size, POSITION_SCALE);
    match &event.label {
        Some(label) => record.set_text(LABEL, label),
        None => record.clear(LABEL),
    }
    if let Some(from) = event.from {
        set_point(record, FROM_X, from);
    }
    set_choice(record, DEATH_STYLE, &DEATH_STYLES, event.death_style);
    set_choice(record, MATERIAL, &MATERIALS, event.material);
    set_optional(record, FORCE, event.force, POSITION_SCALE);
    set_choice(record, COVER_KIND, &COVER_KINDS, event.cover_kind);
    set_optional(record, HEIGHT, event.height, POSITION_SCALE);
    if let Some(source) = event.damage_source {
        set_choice(record, DAMAGE_CAUSE, &DAMAGE_CAUSES, Some(source.cause));
        record.set_fixed(DAMAGE_ORIGIN_X, source.origin.x, POSITION_SCALE);
        record.set_fixed(DAMAGE_ORIGIN_Z, source.origin.z, POSITION_SCALE);
    }
}

/// A captured scene: every entity's wire record by kind in simulation order, the match
/// and the map. Captures reuse the records' buffers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub entities: [Vec<WireRecord>; 5],
    pub elapsed: f64,
    pub match_record: WireRecord,
    pub map: WireRecord,
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
        let [tanks, covers, fragments, mines, pickups] = &mut self.entities;
        fill_records(tanks, &simulation.tanks, |record, tank| {
            write_tank(record, simulation, tank)
        });
        fill_records(covers, &simulation.covers, |record, cover| {
            write_cover(record, simulation, cover)
        });
        fill_records(fragments, &simulation.fragments, |record, fragment| {
            write_fragment(record, simulation, fragment)
        });
        fill_records(mines, &simulation.mines, write_mine);
        fill_records(pickups, &simulation.pickups, write_pickup);
        self.elapsed = json::position(simulation.elapsed);
        write_match(&mut self.match_record, &simulation.match_state);
        write_map(&mut self.map, simulation);
    }

    /// The scene as a baseline sends it: elapsed time, the match and the map, then each
    /// kind's record count and records (the id as a difference from the previous one's).
    pub fn write(&self, out: &mut Vec<u8>) {
        put_signed(out, super::wire::units(self.elapsed, POSITION_SCALE));
        write_record(MATCH_FIELDS, &self.match_record, out);
        write_record(MAP_FIELDS, &self.map, out);
        for (kind, records) in self.entities.iter().enumerate() {
            put_varint(out, records.len() as u64);
            let mut previous = 0;
            for record in records {
                put_signed(out, i64::from(record.id) - previous);
                previous = i64::from(record.id);
                write_record(ENTITY_FIELDS[kind], record, out);
            }
        }
    }

    /// The scene in the JSON protocol's shape, for tests and tools.
    pub fn to_json(&self) -> Value {
        let mut entities = serde_json::Map::new();
        for (kind, records) in self.entities.iter().enumerate() {
            let list = records
                .iter()
                .map(|record| super::wire_view::record_json(ENTITY_FIELDS[kind], record, true))
                .collect::<ReadResult<Vec<_>>>()
                .expect("captured records are valid");
            entities.insert(ENTITY_TYPES[kind].into(), Value::Array(list));
        }
        json!({
            "entities": entities,
            "elapsed": json_number(self.elapsed),
            "match": super::wire_view::record_json(MATCH_FIELDS, &self.match_record, false)
                .expect("valid match"),
            "map": super::wire_view::record_json(MAP_FIELDS, &self.map, false)
                .expect("valid map"),
        })
    }
}

/// A whole record, every present field; an empty record still writes its empty mask.
pub fn write_record(fields: &[Field], record: &WireRecord, out: &mut Vec<u8>) {
    if !write_changes(fields, None, record, out) {
        put_varint(out, 0);
    }
}

// ---------------------------------------------------------------------------------------
// Client side: readers and projection.
// ---------------------------------------------------------------------------------------

/// Typed reads of one record's fields, with errors that name the field.
struct Fields<'a> {
    table: &'static [Field],
    record: &'a WireRecord,
}

/// `number()`'s bound: any finite number within ±1e9.
const NUMBER_BOUND: f64 = 1e9;

impl<'a> Fields<'a> {
    fn new(table: &'static [Field], record: &'a WireRecord) -> Self {
        Self { table, record }
    }

    fn error<T>(&self, field: usize, message: &str) -> ReadResult<T> {
        Err(format!("{}: {message}", self.table[field].name))
    }

    fn number(&self, field: usize) -> Option<i64> {
        self.record.slots.get(field).and_then(Slot::number)
    }

    fn opt_fixed(&self, field: usize) -> ReadResult<Option<f64>> {
        let FieldKind::Fixed(scale) = self.table[field].kind else {
            unreachable!("not a fixed-point field");
        };
        match self.number(field) {
            None => Ok(None),
            Some(units) => {
                let value = units as f64 / scale;
                if value.abs() <= NUMBER_BOUND {
                    Ok(Some(value))
                } else {
                    self.error(field, "Invalid number")
                }
            }
        }
    }

    fn fixed(&self, field: usize) -> ReadResult<f64> {
        match self.opt_fixed(field)? {
            Some(value) => Ok(value),
            None => self.error(field, "Invalid number"),
        }
    }

    /// A quaternion component, which must lie in [-1, 1].
    fn unit(&self, field: usize) -> ReadResult<f64> {
        let value = self.fixed(field)?;
        if value.abs() <= 1.0 {
            Ok(value)
        } else {
            self.error(field, "Invalid number")
        }
    }

    fn opt_count32(&self, field: usize) -> ReadResult<Option<u32>> {
        match self.number(field) {
            None => Ok(None),
            Some(value) => match u32::try_from(value) {
                Ok(value) => Ok(Some(value)),
                Err(_) => self.error(field, "Invalid number"),
            },
        }
    }

    fn count32(&self, field: usize) -> ReadResult<u32> {
        match self.opt_count32(field)? {
            Some(value) => Ok(value),
            None => self.error(field, "Invalid number"),
        }
    }

    fn count(&self, field: usize) -> ReadResult<f64> {
        match self.number(field) {
            Some(value) => Ok(value as f64),
            None => self.error(field, "Invalid number"),
        }
    }

    fn opt_flag(&self, field: usize) -> Option<bool> {
        self.number(field).map(|value| value != 0)
    }

    fn flag(&self, field: usize) -> ReadResult<bool> {
        match self.opt_flag(field) {
            Some(value) => Ok(value),
            None => self.error(field, "Invalid boolean"),
        }
    }

    fn opt_choice<T: Copy>(&self, field: usize, table: &[(&str, T)]) -> ReadResult<Option<T>> {
        match self.number(field) {
            None => Ok(None),
            Some(index) => match table.get(index as usize) {
                Some((_, value)) => Ok(Some(*value)),
                None => self.error(field, "Invalid choice"),
            },
        }
    }

    fn choice<T: Copy>(&self, field: usize, table: &[(&str, T)]) -> ReadResult<T> {
        match self.opt_choice(field, table)? {
            Some(value) => Ok(value),
            None => self.error(field, "Invalid choice"),
        }
    }

    /// `team`: 0 or 1.
    fn opt_team(&self, field: usize) -> ReadResult<Option<Team>> {
        match self.number(field) {
            None => Ok(None),
            Some(0) => Ok(Some(Team::Blue)),
            Some(1) => Ok(Some(Team::Red)),
            Some(_) => self.error(field, "Invalid choice"),
        }
    }

    fn team(&self, field: usize) -> ReadResult<Team> {
        match self.opt_team(field)? {
            Some(team) => Ok(team),
            None => self.error(field, "Invalid choice"),
        }
    }

    /// Text of at most `max` UTF-16 units.
    fn opt_text(&self, field: usize, max: usize) -> ReadResult<Option<String>> {
        match self.record.slots.get(field) {
            Some(Slot::Text(text)) if text_length(text) <= max => Ok(Some(text.clone())),
            Some(Slot::Absent) | None => Ok(None),
            Some(_) => self.error(field, "Invalid text"),
        }
    }

    fn blob(&self, field: usize) -> Option<&'a [u8]> {
        match self.record.slots.get(field) {
            Some(Slot::Blob(bytes)) => Some(bytes),
            _ => None,
        }
    }

    fn opt_point(&self, first: usize) -> ReadResult<Option<Point3>> {
        let [x, y, z] = [
            self.opt_fixed(first)?,
            self.opt_fixed(first + 1)?,
            self.opt_fixed(first + 2)?,
        ];
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Ok(Some(Point3::new(x, y, z))),
            (None, None, None) => Ok(None),
            _ => self.error(first, "Expected object"),
        }
    }

    fn point(&self, first: usize) -> ReadResult<Point3> {
        match self.opt_point(first)? {
            Some(point) => Ok(point),
            None => self.error(first, "Expected object"),
        }
    }

    /// `projectScene`'s rotation check: a near-zero quaternion is rejected, others
    /// normalized.
    fn rotation(&self, first: usize) -> ReadResult<Quat4> {
        let q = Quat4 {
            x: self.unit(first)?,
            y: self.unit(first + 1)?,
            z: self.unit(first + 2)?,
            w: self.unit(first + 3)?,
        };
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
}

/// `tankReader`, projected with `previous` at its position.
pub fn read_tank(record: &WireRecord) -> ReadResult<RenderTank> {
    use tank::*;
    let fields = Fields::new(TANK_FIELDS, record);
    let position = fields.tank_position()?;
    Ok(RenderTank {
        id: record.id,
        life: fields.count32(LIFE)?,
        name: fields
            .opt_text(NAME, 64)?
            .ok_or_else(|| "name: Invalid text".to_string())?,
        kind: fields.choice(KIND, &VEHICLE_KINDS)?,
        team: fields.team(TEAM)?,
        human: fields.flag(HUMAN)?,
        alive: fields.flag(ALIVE)?,
        position,
        velocity: Point3::new(
            fields.fixed(VELOCITY_X)?,
            fields.fixed(VELOCITY_Y)?,
            fields.fixed(VELOCITY_Z)?,
        ),
        drive: Vec2::new(fields.fixed(DRIVE_X)?, fields.fixed(DRIVE_Z)?),
        heading: fields.fixed(HEADING)?,
        aim: fields.fixed(AIM)?,
        hp: fields.fixed(HP)?,
        max_hp: fields.fixed(MAX_HP)?,
        xp: fields.fixed(XP)?,
        shield: fields.fixed(SHIELD)?,
        shield_points: fields.fixed(SHIELD_POINTS)?,
        protection: fields.fixed(PROTECTION)?,
        laser: fields.fixed(LASER)?,
        recoil: fields.fixed(RECOIL)?,
        cooldown: fields.fixed(COOLDOWN)?,
        mine_cooldown: fields.fixed(MINE_COOLDOWN)?,
        respawn: fields.fixed(RESPAWN)?,
        rapid: fields.fixed(RAPID)?,
        speed: fields.fixed(SPEED)?,
        selected_ammo: fields.choice(SELECTED_AMMO, &WEAPONS)?,
        ammo: AmmoInventory {
            spread: fields.count(AMMO_SPREAD)?,
            rocket: fields.count(AMMO_ROCKET)?,
            ricochet: fields.count(AMMO_RICOCHET)?,
            piercing: fields.count(AMMO_PIERCING)?,
        },
        kills: fields.count32(KILLS)?,
        deaths: fields.count32(DEATHS)?,
        last_combat: fields.fixed(LAST_COMBAT)?,
        previous: Vec2::new(position.x, position.z),
    })
}

impl Fields<'_> {
    /// The tank table splits position across non-adjacent fields.
    fn tank_position(&self) -> ReadResult<Point3> {
        Ok(Point3::new(
            self.fixed(tank::POSITION_X)?,
            self.fixed(tank::POSITION_Y)?,
            self.fixed(tank::POSITION_Z)?,
        ))
    }
}

/// `coverReader`, projected: an indestructible cover's `null` hp is infinite.
pub fn read_cover(record: &WireRecord) -> ReadResult<RenderCover> {
    use cover::*;
    let fields = Fields::new(COVER_FIELDS, record);
    let motion = match (
        fields.opt_fixed(MOTION_ORIGIN_X)?,
        fields.opt_fixed(MOTION_ORIGIN_Z)?,
        fields.opt_fixed(MOTION_W)?,
        fields.opt_fixed(MOTION_D)?,
    ) {
        (Some(origin_x), Some(origin_z), Some(w), Some(d)) => Some(RenderCoverMotion {
            origin_x,
            origin_z,
            w,
            d,
        }),
        (None, None, None, None) => None,
        _ => return Err("motion: Expected object".into()),
    };
    Ok(RenderCover {
        id: record.id,
        kind: fields.choice(KIND, &COVER_KINDS)?,
        x: fields.fixed(X)?,
        z: fields.fixed(Z)?,
        w: fields.fixed(W)?,
        h: fields.fixed(H)?,
        d: fields.fixed(D)?,
        hp: fields.opt_fixed(HP)?.unwrap_or(f64::INFINITY),
        max_hp: fields.opt_fixed(MAX_HP)?.unwrap_or(f64::INFINITY),
        alive: fields.flag(ALIVE)?,
        destructible: fields.flag(DESTRUCTIBLE)?,
        color: fields.count32(COLOR)?,
        debris_seed: fields.number(DEBRIS_SEED).map(|seed| seed as f64),
        position: fields.point(POSITION_X)?,
        rotation: fields.rotation(ROTATION_X)?,
        timber_hits: match fields.blob(TIMBER_HITS) {
            Some(bytes) => read_timber_hits(bytes).map_err(|e| format!("timberHits: {e}"))?,
            None => Vec::new(),
        },
        timber_join: fields
            .blob(TIMBER_JOIN)
            .map(read_timber_join)
            .transpose()
            .map_err(|e| format!("timberJoin: {e}"))?,
        motion,
    })
}

/// `fragmentReader`, projected with a normalized rotation.
pub fn read_fragment(record: &WireRecord) -> ReadResult<RenderFragment> {
    use fragment::*;
    let fields = Fields::new(FRAGMENT_FIELDS, record);
    Ok(RenderFragment {
        id: record.id,
        life: fields.fixed(LIFE)?,
        size: fields.fixed(SIZE)?,
        color: fields.count32(COLOR)?,
        position: fields.point(POSITION_X)?,
        rotation: fields.rotation(ROTATION_X)?,
        shape: fields.opt_choice(SHAPE, &FRAGMENT_SHAPES)?,
        dimensions: fields.opt_point(DIMENSIONS_X)?,
        material: fields.opt_choice(MATERIAL, &MATERIALS)?,
        source_kind: fields.opt_choice(SOURCE_KIND, &COVER_KINDS)?,
        timber_part: fields
            .blob(TIMBER_PART)
            .map(read_timber_part)
            .transpose()
            .map_err(|e| format!("timberPart: {e}"))?,
        tower_piece: fields.opt_choice(TOWER_PIECE, &TOWER_PIECES)?,
        tree_cover_id: fields.opt_count32(TREE_COVER_ID)?,
        tree_center_y: fields.opt_fixed(TREE_CENTER_Y)?,
        created_at: fields.opt_fixed(CREATED_AT)?,
        expires_at: fields.opt_fixed(EXPIRES_AT)?,
        wreck: fields.opt_choice(WRECK, &VEHICLE_KINDS)?,
        part: fields.opt_choice(PART, &WRECK_PARTS)?,
        team: fields.opt_team(TEAM)?,
    })
}

/// `mineReader`.
pub fn read_mine(record: &WireRecord) -> ReadResult<Mine> {
    use mine::*;
    let fields = Fields::new(MINE_FIELDS, record);
    Ok(Mine {
        id: record.id,
        x: fields.fixed(X)?,
        z: fields.fixed(Z)?,
        owner: fields.count32(OWNER)?,
        owner_life: fields.opt_count32(OWNER_LIFE)?,
        damage: fields.opt_fixed(DAMAGE)?,
        team: fields.team(TEAM)?,
        arm: fields.fixed(ARM)?,
        life: fields.fixed(LIFE)?,
    })
}

/// `pickupReader`; a missing refill duration reads as zero.
pub fn read_pickup(record: &WireRecord) -> ReadResult<Pickup> {
    use pickup::*;
    let fields = Fields::new(PICKUP_FIELDS, record);
    Ok(Pickup {
        id: record.id,
        x: fields.fixed(X)?,
        z: fields.fixed(Z)?,
        kind: fields.choice(KIND, &PICKUP_KINDS)?,
        available: fields.flag(AVAILABLE)?,
        cooldown: fields.fixed(COOLDOWN)?,
        cooldown_duration: fields.opt_fixed(COOLDOWN_DURATION)?.unwrap_or(0.0),
    })
}

/// `matchReader`.
pub fn read_match(record: &WireRecord) -> ReadResult<Match> {
    use match_record::*;
    let fields = Fields::new(MATCH_FIELDS, record);
    Ok(Match {
        phase: fields.choice(PHASE, &MATCH_PHASES)?,
        time: fields.fixed(TIME)?,
        scores: match fields.blob(SCORES) {
            Some(bytes) => read_scores(bytes).map_err(|_| "Invalid scores".to_string())?,
            None => return Err("scores: Invalid list".into()),
        },
        overtime: fields.flag(OVERTIME)?,
        ended_early: fields.opt_flag(ENDED_EARLY),
        winner: fields.opt_team(WINNER)?,
        round: fields.count32(ROUND)?,
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

pub fn read_map(record: &WireRecord) -> ReadResult<SceneMap> {
    use map::*;
    let fields = Fields::new(MAP_FIELDS, record);
    let scale = fields.opt_fixed(SCALE)?;
    if scale.is_some_and(|scale| !(0.1..=1.0).contains(&scale)) {
        return Err("scale: Invalid number".into());
    }
    Ok(SceneMap {
        theme: fields.choice(THEME, &MAP_MODES)?,
        floor: fields.opt_choice(FLOOR, &GROUNDS)?,
        outer_floor: fields.opt_choice(OUTER_FLOOR, &GROUNDS)?,
        outer_floor_extent: fields.opt_fixed(OUTER_FLOOR_EXTENT)?,
        scale,
    })
}

/// `eventReader`.
pub fn read_event(record: &WireRecord) -> ReadResult<SimEvent> {
    use event::*;
    let fields = Fields::new(EVENT_FIELDS, record);
    let damage_source = match (
        fields.opt_choice(DAMAGE_CAUSE, &DAMAGE_CAUSES)?,
        fields.opt_fixed(DAMAGE_ORIGIN_X)?,
        fields.opt_fixed(DAMAGE_ORIGIN_Z)?,
    ) {
        (Some(cause), Some(x), Some(z)) => Some(DamageSource {
            cause,
            origin: Vec2::new(x, z),
        }),
        (None, None, None) => None,
        _ => return Err("damageSource: Expected object".into()),
    };
    Ok(SimEvent {
        kind: fields.choice(TYPE, &EVENT_TYPES)?,
        x: fields.fixed(X)?,
        z: fields.fixed(Z)?,
        id: fields.opt_count32(ID)?,
        owner: fields.opt_count32(OWNER)?,
        owner_life: fields.opt_count32(OWNER_LIFE)?,
        weapon: fields.opt_choice(WEAPON, &WEAPONS)?,
        team: fields.opt_team(TEAM)?,
        size: fields.opt_fixed(SIZE)?,
        label: fields.opt_text(LABEL, 160)?,
        color: fields.opt_count32(COLOR)?,
        from: fields.opt_point(FROM_X)?,
        death_style: fields.opt_choice(DEATH_STYLE, &DEATH_STYLES)?,
        material: fields.opt_choice(MATERIAL, &MATERIALS)?,
        force: fields.opt_fixed(FORCE)?,
        cover_kind: fields.opt_choice(COVER_KIND, &COVER_KINDS)?,
        height: fields.opt_fixed(HEIGHT)?,
        damage_source,
    })
}

/// One replicated entity: its wire record (for applying field changes) and the
/// validated, projected value.
#[derive(Clone, Debug)]
pub struct Stored<T> {
    pub id: u32,
    pub wire: WireRecord,
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

    /// The wire records in scene order.
    pub fn wires(&self) -> impl Iterator<Item = &WireRecord> {
        self.records.iter().map(|stored| &stored.wire)
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

/// A client's copy of the replicated scene: stored wire records and projected values.
#[derive(Clone, Debug)]
pub struct MirrorScene {
    pub tanks: EntityStore<RenderTank>,
    pub covers: EntityStore<RenderCover>,
    pub fragments: EntityStore<RenderFragment>,
    pub mines: EntityStore<Mine>,
    pub pickups: EntityStore<Pickup>,
    pub elapsed: f64,
    pub match_wire: WireRecord,
    pub match_state: Match,
    pub map_wire: WireRecord,
    pub map: SceneMap,
}

fn read_store<T>(
    records: impl IntoIterator<Item = WireRecord>,
    read: fn(&WireRecord) -> ReadResult<T>,
) -> ReadResult<EntityStore<T>> {
    let mut store = EntityStore::default();
    for wire in records {
        let value = read(&wire)?;
        store.push(Stored {
            id: wire.id,
            wire,
            value,
        })?;
    }
    Ok(store)
}

impl MirrorScene {
    /// Reads a baseline's scene (see [`Scene::write`]) with `projectScene`'s checks:
    /// every record valid, no duplicate ids, rotations usable and at least one tank to
    /// view.
    pub fn read(reader: &mut WireReader<'_>) -> ReadResult<MirrorScene> {
        let mut scene = Scene {
            elapsed: reader.signed()? as f64 / POSITION_SCALE,
            ..Scene::default()
        };
        read_changes(MATCH_FIELDS, &mut scene.match_record, reader)
            .map_err(|error| format!("match: {error}"))?;
        read_changes(MAP_FIELDS, &mut scene.map, reader)
            .map_err(|error| format!("map: {error}"))?;
        for (kind, records) in scene.entities.iter_mut().enumerate() {
            let count = reader.varint()?;
            if count > ENTITY_LIMITS[kind] as u64 {
                return Err(format!("entities: {}: Invalid list", ENTITY_TYPES[kind]));
            }
            let mut id = 0i64;
            for _ in 0..count {
                id += reader.signed()?;
                let mut record = WireRecord {
                    id: u32::try_from(id).map_err(|_| "Invalid entity id".to_string())?,
                    slots: Vec::new(),
                };
                read_changes(ENTITY_FIELDS[kind], &mut record, reader)
                    .map_err(|error| format!("{}: {error}", ENTITY_TYPES[kind]))?;
                records.push(record);
            }
        }
        Self::from_scene(scene)
    }

    /// Validates and projects a scene's records.
    pub fn from_scene(scene: Scene) -> ReadResult<MirrorScene> {
        if scene.elapsed.abs() > NUMBER_BOUND {
            return Err("elapsed: Invalid number".into());
        }
        let [tanks, covers, fragments, mines, pickups] = scene.entities;
        let label = |kind: usize| move |error: String| format!("{}: {error}", ENTITY_TYPES[kind]);
        let mirror = MirrorScene {
            tanks: read_store(tanks, read_tank).map_err(label(TANKS))?,
            covers: read_store(covers, read_cover).map_err(label(COVERS))?,
            fragments: read_store(fragments, read_fragment).map_err(label(FRAGMENTS))?,
            mines: read_store(mines, read_mine).map_err(label(MINES))?,
            pickups: read_store(pickups, read_pickup).map_err(label(PICKUPS))?,
            elapsed: scene.elapsed,
            match_state: read_match(&scene.match_record).map_err(|e| format!("match: {e}"))?,
            match_wire: scene.match_record,
            map: read_map(&scene.map).map_err(|e| format!("map: {e}"))?,
            map_wire: scene.map,
        };
        if mirror.tanks.is_empty() {
            return Err("Missing viewer".into());
        }
        Ok(mirror)
    }

    /// The mirrored records as a [`Scene`], for comparing a mirror with a capture.
    pub fn to_scene(&self) -> Scene {
        Scene {
            entities: [
                self.tanks.wires().cloned().collect(),
                self.covers.wires().cloned().collect(),
                self.fragments.wires().cloned().collect(),
                self.mines.wires().cloned().collect(),
                self.pickups.wires().cloned().collect(),
            ],
            elapsed: self.elapsed,
            match_record: self.match_wire.clone(),
            map: self.map_wire.clone(),
        }
    }

    /// The scene in the JSON protocol's shape, for tests and tools.
    pub fn to_value(&self) -> Value {
        self.to_scene().to_json()
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
