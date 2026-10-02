//! Entity records and the enums shared by simulation, presentation and the wire protocol.
//! Enum names serialize exactly as the TypeScript string unions did, because they appear in
//! JSON messages and HUD data. Timers are seconds; aim and heading are radians around Y.

use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::bot_personalities::BotPersonality;
use super::debris_physics::DebrisMaterial;
use super::humvee_tactics::HumveeTactics;
pub use super::math::{Point3, Quat4, Vec2};
use super::timber_layout::{TimberHit, TimberJoin, TimberPart};
use super::tower_layout::TowerPiece;

/// Serialized as 0 (blue) or 1 (red), like the TS `0 | 1` team.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Team {
    #[default]
    Blue = 0,
    Red = 1,
}

impl Team {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn from_index(index: usize) -> Team {
        if index.is_multiple_of(2) {
            Team::Blue
        } else {
            Team::Red
        }
    }

    pub const fn opponent(self) -> Team {
        match self {
            Team::Blue => Team::Red,
            Team::Red => Team::Blue,
        }
    }
}

impl Serialize for Team {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Team {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Team::Blue),
            1 => Ok(Team::Red),
            other => Err(serde::de::Error::custom(format!("invalid team {other}"))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VehicleKind {
    Scout,
    #[default]
    Balanced,
    Heavy,
    Humvee,
}

impl VehicleKind {
    pub const ALL: [VehicleKind; 4] = [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
        VehicleKind::Humvee,
    ];
    /// The chassis a player may drive (every kind but the bot-only Humvee).
    pub const PLAYABLE: [VehicleKind; 3] = [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            VehicleKind::Scout => "scout",
            VehicleKind::Balanced => "balanced",
            VehicleKind::Heavy => "heavy",
            VehicleKind::Humvee => "humvee",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weapon {
    #[default]
    Standard,
    Spread,
    Rocket,
    Ricochet,
    Piercing,
    Tow,
}

impl Weapon {
    pub const fn as_str(self) -> &'static str {
        match self {
            Weapon::Standard => "standard",
            Weapon::Spread => "spread",
            Weapon::Rocket => "rocket",
            Weapon::Ricochet => "ricochet",
            Weapon::Piercing => "piercing",
            Weapon::Tow => "tow",
        }
    }

    /// The limited ammunition this weapon draws from, if any.
    pub const fn special(self) -> Option<SpecialAmmo> {
        match self {
            Weapon::Spread => Some(SpecialAmmo::Spread),
            Weapon::Rocket => Some(SpecialAmmo::Rocket),
            Weapon::Ricochet => Some(SpecialAmmo::Ricochet),
            Weapon::Piercing => Some(SpecialAmmo::Piercing),
            Weapon::Standard | Weapon::Tow => None,
        }
    }
}

/// Ammunition that crates refill and tanks carry in limited amounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpecialAmmo {
    Spread,
    Rocket,
    Ricochet,
    Piercing,
}

impl SpecialAmmo {
    pub const ALL: [SpecialAmmo; 4] = [
        SpecialAmmo::Spread,
        SpecialAmmo::Rocket,
        SpecialAmmo::Ricochet,
        SpecialAmmo::Piercing,
    ];

    pub const fn weapon(self) -> Weapon {
        match self {
            SpecialAmmo::Spread => Weapon::Spread,
            SpecialAmmo::Rocket => Weapon::Rocket,
            SpecialAmmo::Ricochet => Weapon::Ricochet,
            SpecialAmmo::Piercing => Weapon::Piercing,
        }
    }
}

/// Rounds carried per special ammunition type, in `SpecialAmmo::ALL` order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AmmoInventory {
    pub spread: f64,
    pub rocket: f64,
    pub ricochet: f64,
    pub piercing: f64,
}

impl AmmoInventory {
    pub fn get(&self, kind: SpecialAmmo) -> f64 {
        match kind {
            SpecialAmmo::Spread => self.spread,
            SpecialAmmo::Rocket => self.rocket,
            SpecialAmmo::Ricochet => self.ricochet,
            SpecialAmmo::Piercing => self.piercing,
        }
    }

    pub fn get_mut(&mut self, kind: SpecialAmmo) -> &mut f64 {
        match kind {
            SpecialAmmo::Spread => &mut self.spread,
            SpecialAmmo::Rocket => &mut self.rocket,
            SpecialAmmo::Ricochet => &mut self.ricochet,
            SpecialAmmo::Piercing => &mut self.piercing,
        }
    }

    pub fn values(&self) -> [f64; 4] {
        [self.spread, self.rocket, self.ricochet, self.piercing]
    }
}

/// A named weapon, or a step through the ammunition order (-1 previous, 1 next).
/// Serialized as the weapon string or the number, like the TS `Weapon | -1 | 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmmoSelection {
    Weapon(Weapon),
    Step(i8),
}

impl Serialize for AmmoSelection {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            AmmoSelection::Weapon(weapon) => weapon.serialize(serializer),
            AmmoSelection::Step(step) => serializer.serialize_i8(*step),
        }
    }
}

impl<'de> Deserialize<'de> for AmmoSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Weapon(Weapon),
            Step(i8),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Weapon(weapon) => Ok(AmmoSelection::Weapon(weapon)),
            Raw::Step(step @ (-1 | 1)) => Ok(AmmoSelection::Step(step)),
            Raw::Step(other) => Err(serde::de::Error::custom(format!(
                "invalid ammo step {other}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickupKind {
    Spread,
    Rocket,
    Ricochet,
    Piercing,
    Rapid,
    Shield,
    Speed,
    Repair,
    Laser,
}

impl PickupKind {
    pub const ALL: [PickupKind; 9] = [
        PickupKind::Spread,
        PickupKind::Rocket,
        PickupKind::Ricochet,
        PickupKind::Piercing,
        PickupKind::Rapid,
        PickupKind::Shield,
        PickupKind::Speed,
        PickupKind::Repair,
        PickupKind::Laser,
    ];

    /// The TypeScript identifier (the serialized name).
    pub const fn as_str(self) -> &'static str {
        match self {
            PickupKind::Spread => "spread",
            PickupKind::Rocket => "rocket",
            PickupKind::Ricochet => "ricochet",
            PickupKind::Piercing => "piercing",
            PickupKind::Rapid => "rapid",
            PickupKind::Shield => "shield",
            PickupKind::Speed => "speed",
            PickupKind::Repair => "repair",
            PickupKind::Laser => "laser",
        }
    }

    pub const fn special_ammo(self) -> Option<SpecialAmmo> {
        match self {
            PickupKind::Spread => Some(SpecialAmmo::Spread),
            PickupKind::Rocket => Some(SpecialAmmo::Rocket),
            PickupKind::Ricochet => Some(SpecialAmmo::Ricochet),
            PickupKind::Piercing => Some(SpecialAmmo::Piercing),
            _ => None,
        }
    }

    /// The weapon whose ammunition this crate carries, if any.
    pub const fn weapon(self) -> Option<Weapon> {
        match self.special_ammo() {
            Some(ammo) => Some(ammo.weapon()),
            None => None,
        }
    }
}

/// The sole input boundary for humans, bots, recordings, and remote peers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VehicleCommand {
    pub move_x: f64,
    pub move_z: f64,
    pub aim: f64,
    pub fire: bool,
    pub mine: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammo_selection: Option<AmmoSelection>,
}

impl VehicleCommand {
    /// No movement, no fire, aim at north.
    pub const fn idle() -> Self {
        Self {
            move_x: 0.0,
            move_z: 0.0,
            aim: 0.0,
            fire: false,
            mine: false,
            ammo_selection: None,
        }
    }

    /// Idle, but keeping the given turret aim.
    pub const fn idle_aiming(aim: f64) -> Self {
        let mut command = Self::idle();
        command.aim = aim;
        command
    }
}

/// A multiplayer seat's tank assignment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerAssignment {
    pub player_id: String,
    pub name: String,
    pub team: Team,
    pub slot: usize,
    pub kind: VehicleKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    Human,
    #[default]
    Bot,
    Idle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BotMode {
    #[default]
    Advance,
    Fight,
    Retreat,
    Pickup,
    Escort,
}

/// Bot memory survives between decisions; steering and recovery update every fixed tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Brain {
    pub humvee: Option<HumveeTactics>,
    pub personality: BotPersonality,
    pub ultra_aggressive: bool,
    pub last_seen: Vec2,
    pub decision: f64,
    /// Target tank id; zero when there is none.
    pub target: u32,
    pub memory: f64,
    pub reaction: f64,
    pub fire_delay: f64,
    pub aim_error: f64,
    pub path: Vec<Vec2>,
    pub goal: Vec2,
    pub last: Vec2,
    pub stuck: f64,
    pub recovery: f64,
    pub recovery_goal: Vec2,
    pub recoveries: u32,
    pub avoidance: Vec2,
    pub avoidance_time: f64,
    /// Pickup id; zero when there is none.
    pub pickup_target: u32,
    pub nav_version: u32,
    pub mode: BotMode,
}

/// Persistent entity identity. The body and colliders are recreated for each life.
#[derive(Clone, Debug)]
pub struct Tank {
    pub id: u32,
    pub name: String,
    pub team: Team,
    pub human: bool,
    pub player_id: Option<String>,
    pub driver: Driver,
    /// Changes on death or reassignment, independently of the scoreboard.
    pub life: u32,
    pub kind: VehicleKind,
    pub body: RigidBodyHandle,
    /// The hull collider that touches cover and the ground (the first collider).
    pub collider: ColliderHandle,
    pub hp: f64,
    pub alive: bool,
    pub respawn: f64,
    pub protection: f64,
    pub selected_ammo: Weapon,
    pub ammo: AmmoInventory,
    pub shield: f64,
    pub shield_points: f64,
    pub rapid: f64,
    pub speed: f64,
    pub laser: f64,
    /// Seconds until the laser defense can zap again, counted from this tick's projectile sweep.
    pub laser_recharge: f64,
    pub cooldown: f64,
    pub mine_cooldown: f64,
    pub aim: f64,
    pub heading: f64,
    /// Planar position at the start of the current tick, and the rest pose of a dead tank.
    pub previous: Vec2,
    pub recoil: f64,
    pub kills: u32,
    pub damage_dealt: f64,
    pub life_kills: u32,
    pub best_life_kills: u32,
    pub highest_rank: usize,
    pub deaths: u32,
    pub xp: f64,
    pub last_combat: f64,
    pub command: VehicleCommand,
    pub brain: Brain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverKind {
    Rock,
    Teeth,
    Hedgehog,
    Container,
    Cargo,
    House,
    Tree,
    Timber,
    Concrete,
    Drum,
    Tower,
    Rubble,
    Boundary,
}

impl CoverKind {
    pub const ALL: [CoverKind; 13] = [
        CoverKind::Rock,
        CoverKind::Teeth,
        CoverKind::Hedgehog,
        CoverKind::Container,
        CoverKind::Cargo,
        CoverKind::House,
        CoverKind::Tree,
        CoverKind::Timber,
        CoverKind::Concrete,
        CoverKind::Drum,
        CoverKind::Tower,
        CoverKind::Rubble,
        CoverKind::Boundary,
    ];

    /// The TypeScript identifier (the serialized name).
    pub const fn as_str(self) -> &'static str {
        match self {
            CoverKind::Rock => "rock",
            CoverKind::Teeth => "teeth",
            CoverKind::Hedgehog => "hedgehog",
            CoverKind::Container => "container",
            CoverKind::Cargo => "cargo",
            CoverKind::House => "house",
            CoverKind::Tree => "tree",
            CoverKind::Timber => "timber",
            CoverKind::Concrete => "concrete",
            CoverKind::Drum => "drum",
            CoverKind::Tower => "tower",
            CoverKind::Rubble => "rubble",
            CoverKind::Boundary => "boundary",
        }
    }

    /// Drums, dragon's teeth and hedgehogs are dynamic bodies that blasts and shells push.
    pub const fn movable(self) -> bool {
        matches!(
            self,
            CoverKind::Drum | CoverKind::Teeth | CoverKind::Hedgehog
        )
    }
}

/// Original dimensions and last navigation footprint for movable cover.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverMotion {
    pub origin_x: f64,
    pub origin_z: f64,
    pub w: f64,
    pub d: f64,
    pub x: f64,
    pub z: f64,
    pub nav_w: f64,
    pub nav_d: f64,
    pub check_at: f64,
}

/// Axis-aligned cover footprint: w/d/h are full dimensions in world metres.
#[derive(Clone, Debug)]
pub struct Cover {
    pub id: u32,
    pub kind: CoverKind,
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
    /// Infinite for indestructible cover.
    pub hp: f64,
    pub max_hp: f64,
    pub alive: bool,
    pub destructible: bool,
    /// Invalid (removed from the world) once destroyed, except a felled tree's stump.
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    pub color: u32,
    /// Chosen at collapse so rubble stays stable when its model is rebuilt.
    pub debris_seed: Option<f64>,
    /// Bounded, persistent impact marks in local wall coordinates.
    pub timber_hits: Vec<TimberHit>,
    pub timber_join: Option<TimberJoin>,
    /// Direction of the last hit on a timber wall or the watchtower, which its
    /// pieces are thrown or topple along when it breaks.
    pub kick: Option<Vec2>,
    pub motion: Option<CoverMotion>,
    /// When level rules first saw this cover destroyed; the base game never reads it.
    pub fallen_at: Option<f64>,
}

/// Planar projectile state. vx/vz are metres per second; life is remaining seconds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shot {
    pub id: u32,
    pub x: f64,
    pub z: f64,
    /// Combat height at the muzzle; hit detection remains on the arena plane. Defaults to 1.
    pub y: Option<f64>,
    pub recap_hit: bool,
    /// Render height at the muzzle; may differ from the combat lane.
    pub visual_y: Option<f64>,
    /// Target selected when a bot-fired TOW is launched; guidance is render-independent.
    pub target_id: Option<u32>,
    pub target_life: Option<u32>,
    pub owner: u32,
    /// Owner's life generation when fired, to keep XP attached to that life.
    pub owner_life: Option<u32>,
    pub team: Team,
    pub vx: f64,
    pub vz: f64,
    pub damage: f64,
    pub bounces: u32,
    /// Set by the first bounce. Until then the shell passes through the tank that fired it,
    /// whose hull it starts beside; a reflected shell can strike its shooter.
    pub ricocheted: bool,
    pub life: f64,
    pub weapon: Weapon,
    /// One shell interception for a fresh piercing round, zero otherwise.
    pub piercing: u32,
    /// Two fresh piercing rounds pass through each other exactly once.
    pub pierced_shot: Option<u32>,
    /// A defense gets one chance per projectile, even after a miss or bounce.
    pub laser_checked_by: Vec<u32>,
}

impl Shot {
    /// The combat lane height, one metre when a test or probe leaves it unset.
    pub fn combat_y(&self) -> f64 {
        self.y.unwrap_or(1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mine {
    pub id: u32,
    pub owner: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_life: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<f64>,
    pub team: Team,
    pub x: f64,
    pub z: f64,
    pub arm: f64,
    pub life: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pickup {
    pub id: u32,
    pub kind: PickupKind,
    pub x: f64,
    pub z: f64,
    pub available: bool,
    pub cooldown: f64,
    /// Starting duration for the current cooldown, used by the refill indicator.
    pub cooldown_duration: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WreckPart {
    /// Hull and turret together.
    Intact,
    /// The hull alone, with the turret ring cut open.
    Hull,
    /// The turret without its gun.
    Turret,
    /// The turret with its gun.
    TurretBarrel,
    /// The gun alone.
    Barrel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FragmentShape {
    Armor,
    Wheel,
    Track,
    Shard,
    Wood,
    Panel,
    Beam,
    Log,
    DrumShell,
    DrumLid,
}

/// A physical debris piece; bounded by `Simulation::max_fragments`.
#[derive(Clone, Debug)]
pub struct Fragment {
    pub id: u32,
    pub body: RigidBodyHandle,
    pub life: f64,
    pub size: f64,
    pub color: u32,
    pub shape: Option<FragmentShape>,
    pub dimensions: Option<Point3>,
    pub material: Option<DebrisMaterial>,
    pub source_kind: Option<CoverKind>,
    pub timber_part: Option<TimberPart>,
    /// The watchtower part this piece is drawn as.
    pub tower_piece: Option<TowerPiece>,
    pub tree_cover_id: Option<u32>,
    pub tree_center_y: Option<f64>,
    pub expires_at: Option<f64>,
    /// Simulation timestamp for the 2.5-second tank wreck darkening.
    pub created_at: Option<f64>,
    pub wreck: Option<VehicleKind>,
    pub part: Option<WreckPart>,
    pub team: Option<Team>,
}

impl Fragment {
    /// A plain fragment record for `body`; callers set the optional fields they need.
    pub fn new(id: u32, body: RigidBodyHandle, life: f64, size: f64, color: u32) -> Self {
        Self {
            id,
            body,
            life,
            size,
            color,
            shape: None,
            dimensions: None,
            material: None,
            source_kind: None,
            timber_part: None,
            tower_piece: None,
            tree_cover_id: None,
            tree_center_y: None,
            expires_at: None,
            created_at: None,
            wreck: None,
            part: None,
            team: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DamageCause {
    Standard,
    Spread,
    Rocket,
    Ricochet,
    Piercing,
    Tow,
    Mine,
    Drum,
    Interception,
    Explosion,
}

impl From<Weapon> for DamageCause {
    fn from(weapon: Weapon) -> Self {
        match weapon {
            Weapon::Standard => DamageCause::Standard,
            Weapon::Spread => DamageCause::Spread,
            Weapon::Rocket => DamageCause::Rocket,
            Weapon::Ricochet => DamageCause::Ricochet,
            Weapon::Piercing => DamageCause::Piercing,
            Weapon::Tow => DamageCause::Tow,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageSource {
    pub cause: DamageCause,
    pub origin: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SimEventType {
    DebrisImpact,
    Notice,
    Shot,
    Impact,
    Explosion,
    Destroy,
    Death,
    Pickup,
    Respawn,
    Hurt,
    Ricochet,
    Laser,
    Promotion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeathStyle {
    Burnout,
}

/// A presentation/audio cue produced by one tick. Consumers drain `Simulation::events`;
/// the queue is capped at `SIMULATION_RULES.max_pending_events`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimEvent {
    #[serde(rename = "type")]
    pub kind: SimEventType,
    pub x: f64,
    pub z: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_life: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<Weapon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<Team>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    /// Laser beam origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Point3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_style: Option<DeathStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<DebrisMaterial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_source: Option<DamageSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_kind: Option<CoverKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

impl SimEvent {
    /// An event of `kind` at a planar position with every optional field unset.
    pub fn at(kind: SimEventType, x: f64, z: f64) -> Self {
        Self {
            kind,
            x,
            z,
            id: None,
            owner: None,
            owner_life: None,
            weapon: None,
            team: None,
            size: None,
            label: None,
            color: None,
            from: None,
            death_style: None,
            material: None,
            force: None,
            damage_source: None,
            cover_kind: None,
            height: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchPhase {
    #[default]
    Ready,
    Playing,
    Paused,
    Results,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub phase: MatchPhase,
    pub time: f64,
    pub scores: [u32; 2],
    pub overtime: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_early: Option<bool>,
    pub winner: Option<Team>,
    pub round: u32,
}
