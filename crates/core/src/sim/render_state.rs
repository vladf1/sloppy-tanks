//! What presentation draws: plain values copied from the simulation, with no physics
//! handles. The local renderer fills it from a `Simulation`; the network client fills the
//! same type from replicated snapshots. Ownership, damage and contact bookkeeping stay
//! authoritative in the simulation.

use serde::{Deserialize, Serialize};

use super::debris_physics::DebrisMaterial;
use super::map_options::MapId;
use super::maps::GroundKind;
use super::math::{Point3, Quat4, Vec2};
use super::simulation::Simulation;
use super::simulation_rules::SIMULATION_RULES;
use super::timber_layout::{TimberHit, TimberJoin, TimberPart};
use super::tower_layout::TowerPiece;
use super::types::{
    AmmoInventory, CoverKind, CoverMotion, Fragment, FragmentShape, Match, Mine, Pickup, Shot,
    Tank, Team, VehicleKind, Weapon, WreckPart,
};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderTank {
    pub id: u32,
    pub name: String,
    pub kind: VehicleKind,
    pub team: Team,
    pub human: bool,
    pub alive: bool,
    /// Planar position at the start of the tick; a dead tank's rest pose.
    pub previous: Vec2,
    /// Hull yaw, radians.
    pub heading: f64,
    /// Turret yaw, radians.
    pub aim: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub xp: f64,
    /// Remaining shield seconds, and the damage it can still absorb.
    pub shield: f64,
    pub shield_points: f64,
    /// Remaining spawn-protection seconds.
    pub protection: f64,
    /// Remaining laser-defense seconds.
    pub laser: f64,
    /// Gun recoil, 1 just after firing and recovering to 0.
    pub recoil: f64,
    pub cooldown: f64,
    pub mine_cooldown: f64,
    /// Seconds until a dead tank respawns.
    pub respawn: f64,
    pub rapid: f64,
    pub speed: f64,
    pub selected_ammo: Weapon,
    pub ammo: AmmoInventory,
    pub kills: u32,
    pub deaths: u32,
    pub last_combat: f64,
    /// Life generation; changes on death or seat reassignment.
    pub life: u32,
    /// Body position (a dead tank rests at `previous`, 0.65 m up).
    pub position: Point3,
    pub velocity: Point3,
    /// The move input the tank last drove with (x, z); zero for a wreck. Client
    /// prediction holds it for the tanks it cannot steer.
    pub drive: Vec2,
}

// Manual so a state refilled in place keeps each tank's name allocation.
impl Clone for RenderTank {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            ..*self
        }
    }

    fn clone_from(&mut self, source: &Self) {
        let mut name = std::mem::take(&mut self.name);
        name.clone_from(&source.name);
        *self = Self { name, ..*source };
    }
}

/// Original footprint of movable cover; its navigation bookkeeping stays in the simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderCoverMotion {
    pub origin_x: f64,
    pub origin_z: f64,
    pub w: f64,
    pub d: f64,
}

impl From<CoverMotion> for RenderCoverMotion {
    fn from(motion: CoverMotion) -> Self {
        Self {
            origin_x: motion.origin_x,
            origin_z: motion.origin_z,
            w: motion.w,
            d: motion.d,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderCover {
    pub id: u32,
    pub kind: CoverKind,
    /// Footprint centre and full dimensions (movable cover: the current nav footprint).
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    /// Infinite for indestructible cover.
    pub hp: f64,
    pub max_hp: f64,
    pub alive: bool,
    pub destructible: bool,
    pub color: u32,
    pub debris_seed: Option<f64>,
    pub timber_hits: Vec<TimberHit>,
    pub timber_join: Option<TimberJoin>,
    pub motion: Option<RenderCoverMotion>,
    /// Body pose; destroyed cover without a body stays at its footprint.
    pub position: Point3,
    pub rotation: Quat4,
}

// Manual so a state refilled in place keeps each cover's timber hits allocation.
impl Clone for RenderCover {
    fn clone(&self) -> Self {
        Self {
            timber_hits: self.timber_hits.clone(),
            ..*self
        }
    }

    fn clone_from(&mut self, source: &Self) {
        let mut timber_hits = std::mem::take(&mut self.timber_hits);
        timber_hits.clone_from(&source.timber_hits);
        *self = Self {
            timber_hits,
            ..*source
        };
    }
}

impl Default for RenderCover {
    fn default() -> Self {
        Self {
            id: 0,
            kind: CoverKind::Boundary,
            x: 0.0,
            z: 0.0,
            w: 0.0,
            h: 0.0,
            d: 0.0,
            hp: 0.0,
            max_hp: 0.0,
            alive: false,
            destructible: false,
            color: 0,
            debris_seed: None,
            timber_hits: Vec::new(),
            timber_join: None,
            motion: None,
            position: Point3::ZERO,
            rotation: Quat4::IDENTITY,
        }
    }
}

/// A projectile as presentation sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderShot {
    pub id: u32,
    pub x: f64,
    pub z: f64,
    /// Combat lane height.
    pub y: Option<f64>,
    /// Render launch height.
    pub visual_y: Option<f64>,
    pub vx: f64,
    pub vz: f64,
    pub weapon: Weapon,
    pub team: Team,
}

impl From<&Shot> for RenderShot {
    fn from(shot: &Shot) -> Self {
        Self {
            id: shot.id,
            x: shot.x,
            z: shot.z,
            y: shot.y,
            visual_y: shot.visual_y,
            vx: shot.vx,
            vz: shot.vz,
            weapon: shot.weapon,
            team: shot.team,
        }
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFragment {
    pub id: u32,
    /// Remaining seconds; the last `DEBRIS_CLEANUP_SECONDS` sink and fade.
    pub life: f64,
    pub size: f64,
    pub color: u32,
    pub shape: Option<FragmentShape>,
    /// Full box dimensions of authored scenery pieces.
    pub dimensions: Option<Point3>,
    pub material: Option<DebrisMaterial>,
    pub source_kind: Option<CoverKind>,
    pub timber_part: Option<TimberPart>,
    pub tower_piece: Option<TowerPiece>,
    pub tree_cover_id: Option<u32>,
    pub tree_center_y: Option<f64>,
    pub created_at: Option<f64>,
    pub expires_at: Option<f64>,
    pub wreck: Option<VehicleKind>,
    pub part: Option<WreckPart>,
    pub team: Option<Team>,
    pub position: Point3,
    pub rotation: Quat4,
}

// Manual so a state refilled in place keeps each timber part's marks allocation.
impl Clone for RenderFragment {
    fn clone(&self) -> Self {
        Self {
            timber_part: self.timber_part.clone(),
            ..*self
        }
    }

    fn clone_from(&mut self, source: &Self) {
        let mut timber_part = self.timber_part.take();
        timber_part.clone_from(&source.timber_part);
        *self = Self {
            timber_part,
            ..*source
        };
    }
}

/// Presentation reads values only. Network implementations contain no physics world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderState {
    /// The tank the camera follows.
    pub viewer_id: u32,
    pub tanks: Vec<RenderTank>,
    pub covers: Vec<RenderCover>,
    pub fragments: Vec<RenderFragment>,
    pub shots: Vec<RenderShot>,
    pub mines: Vec<Mine>,
    pub pickups: Vec<Pickup>,
    /// Simulation seconds.
    pub elapsed: f64,
    #[serde(rename = "match")]
    pub match_state: Match,
    /// Renderer theme: `village`, `harbor`, `quarry`, or an extra level's id.
    pub map_theme: String,
    pub map_floor: Option<GroundKind>,
    pub map_outer_floor: Option<GroundKind>,
    pub map_outer_floor_extent: Option<f64>,
    /// Compact maps' size relative to the standard arena.
    pub map_scale: f64,
    /// The extra level whose own map is in play, if any.
    pub custom_map: Option<MapId>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            viewer_id: 0,
            tanks: Vec::new(),
            covers: Vec::new(),
            fragments: Vec::new(),
            shots: Vec::new(),
            mines: Vec::new(),
            pickups: Vec::new(),
            elapsed: 0.0,
            match_state: super::match_state::new_match(1),
            map_theme: String::new(),
            map_floor: None,
            map_outer_floor: None,
            map_outer_floor_extent: None,
            map_scale: 1.0,
            custom_map: None,
        }
    }
}

impl RenderState {
    /// The followed tank, if it is present.
    pub fn viewer(&self) -> Option<&RenderTank> {
        self.tanks.iter().find(|tank| tank.id == self.viewer_id)
    }
}

/// Resize `items` to `sources.len()`, keeping existing allocations, and update each entry
/// in place.
pub(crate) fn fill_each<T: Default, S>(
    items: &mut Vec<T>,
    sources: &[S],
    mut update: impl FnMut(&mut T, &S),
) {
    items.truncate(sources.len());
    while items.len() < sources.len() {
        items.push(T::default());
    }
    for (item, source) in items.iter_mut().zip(sources) {
        update(item, source);
    }
}

impl Simulation {
    /// A dead tank rests at its last planar position.
    pub fn tank_position(&self, tank: &Tank) -> Point3 {
        if tank.alive {
            self.body_translation(tank.body)
        } else {
            Point3::new(
                tank.previous.x,
                SIMULATION_RULES.tank_body_height,
                tank.previous.z,
            )
        }
    }

    pub fn tank_velocity(&self, tank: &Tank) -> Point3 {
        if tank.alive {
            self.body_linvel(tank.body)
        } else {
            Point3::ZERO
        }
    }

    /// Fill `state` for the tank `viewer` (the first human by default). Reuses the state's
    /// allocations, so steady-state frames do not allocate.
    pub fn fill_render_state(&self, state: &mut RenderState, viewer: Option<u32>) {
        state.viewer_id =
            viewer.unwrap_or_else(|| self.human_index().map_or(0, |i| self.tanks[i].id));
        fill_each(&mut state.tanks, &self.tanks, |view, tank| {
            self.fill_tank(view, tank)
        });
        fill_each(&mut state.covers, &self.covers, |view, cover| {
            let has_body = self.world.bodies.contains(cover.body);
            view.id = cover.id;
            view.kind = cover.kind;
            view.x = cover.x;
            view.z = cover.z;
            view.w = cover.w;
            view.h = cover.h;
            view.d = cover.d;
            view.hp = cover.hp;
            view.max_hp = cover.max_hp;
            view.alive = cover.alive;
            view.destructible = cover.destructible;
            view.color = cover.color;
            view.debris_seed = cover.debris_seed;
            view.timber_hits.clone_from(&cover.timber_hits);
            view.timber_join = cover.timber_join;
            view.motion = cover.motion.map(RenderCoverMotion::from);
            view.position = if has_body {
                self.body_translation(cover.body)
            } else {
                Point3::new(cover.x, 0.0, cover.z)
            };
            view.rotation = if has_body {
                self.body_rotation(cover.body)
            } else {
                Quat4::IDENTITY
            };
        });
        fill_each(&mut state.fragments, &self.fragments, |view, fragment| {
            self.fill_fragment(view, fragment)
        });
        fill_each(&mut state.shots, &self.shots, |view, shot| {
            *view = RenderShot::from(shot)
        });
        state.mines.clone_from(&self.mines);
        state.pickups.clone_from(&self.pickups);
        state.elapsed = self.elapsed;
        state.match_state.clone_from(&self.match_state);
        let theme = self.map_theme();
        if state.map_theme != theme {
            state.map_theme.clear();
            state.map_theme.push_str(theme);
        }
        state.map_floor = self.map_floor();
        state.map_outer_floor = self.map_outer_floor();
        state.map_outer_floor_extent = self.map_outer_floor_extent();
        state.map_scale = self.map_scale();
        state.custom_map = self.custom_map.map(|map| map.id);
    }

    /// A new render state for `viewer` (the first human by default).
    pub fn render_state(&self, viewer: Option<u32>) -> RenderState {
        let mut state = RenderState::default();
        self.fill_render_state(&mut state, viewer);
        state
    }

    fn fill_tank(&self, view: &mut RenderTank, tank: &Tank) {
        view.id = tank.id;
        view.name.clone_from(&tank.name);
        view.kind = tank.kind;
        view.team = tank.team;
        view.human = tank.human;
        view.alive = tank.alive;
        view.previous = tank.previous;
        view.heading = tank.heading;
        view.aim = tank.aim;
        view.hp = tank.hp;
        view.max_hp = self.max_health(tank);
        view.xp = tank.xp;
        view.shield = tank.shield;
        view.shield_points = tank.shield_points;
        view.protection = tank.protection;
        view.laser = tank.laser;
        view.recoil = tank.recoil;
        view.cooldown = tank.cooldown;
        view.mine_cooldown = tank.mine_cooldown;
        view.respawn = tank.respawn;
        view.rapid = tank.rapid;
        view.speed = tank.speed;
        view.selected_ammo = tank.selected_ammo;
        view.ammo = tank.ammo;
        view.kills = tank.kills;
        view.deaths = tank.deaths;
        view.last_combat = tank.last_combat;
        view.life = tank.life;
        view.position = self.tank_position(tank);
        view.drive = if tank.alive {
            Vec2::new(tank.command.move_x, tank.command.move_z)
        } else {
            Vec2::ZERO
        };
        view.velocity = self.tank_velocity(tank);
    }

    fn fill_fragment(&self, view: &mut RenderFragment, fragment: &Fragment) {
        view.id = fragment.id;
        view.life = fragment.life;
        view.size = fragment.size;
        view.color = fragment.color;
        view.shape = fragment.shape;
        view.dimensions = fragment.dimensions;
        view.material = fragment.material;
        view.source_kind = fragment.source_kind;
        view.timber_part.clone_from(&fragment.timber_part);
        view.tower_piece = fragment.tower_piece;
        view.tree_cover_id = fragment.tree_cover_id;
        view.tree_center_y = fragment.tree_center_y;
        view.created_at = fragment.created_at;
        view.expires_at = fragment.expires_at;
        view.wreck = fragment.wreck;
        view.part = fragment.part;
        view.team = fragment.team;
        view.position = self.body_translation(fragment.body);
        view.rotation = self.body_rotation(fragment.body);
    }
}
