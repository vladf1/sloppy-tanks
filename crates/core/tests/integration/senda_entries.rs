//! Senda's offices and entries: one solid cover before every home, against its front wall,
//! clear of its driveway, the street's paving, its neighbours and every other cover; a tank
//! cannot path through one, the street before it stays open, and one brought down leaves
//! its house standing and its ground free.
//!
//! The layout is measured in its design metres (`senda_design_layout`); a simulation's
//! cover and navigation stand `SENDA_SCALE` times larger.

use glam::DVec2;
use sloppy_core::sim::arena::{CoverDef, cover_turn};
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::house_layout::{HouseDesign, REFERENCE_HOUSE};
use sloppy_core::sim::level_rules::standard_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Vec2, distance};
use sloppy_core::sim::senda_entries::{entry_place, is_entry};
use sloppy_core::sim::senda_layout::{
    CUL_DE_SAC_RADIUS, PARKWAY, SENDA_MAP, SENDA_PICKUPS, SENDA_SCALE, SIDEWALK, SendaHouse,
    Street, lamp_sites, senda_design_layout, senda_houses,
};
use sloppy_core::sim::senda_yards::{front_openings, senda_lots};
use sloppy_core::sim::{Cover, CoverKind, Simulation, Team};

/// How near (m) an entry's back must come to its house's front, and how far two boxes
/// may cross where they meet.
const TOUCH: f64 = 0.02;
/// A route ends in the navigation cell that contains its goal.
const ARRIVAL: f64 = 1.8;
/// Half a navigation cell (m): a cell's centre lies this near a point in it.
const HALF_CELL: f64 = 0.75;
/// Quarter-metre steps (design) from a sidewalk's middle across its outer half, the
/// parkway and into the street.
const STREET_STEPS: u8 = 12;

fn senda(seed: f64) -> Simulation {
    let streets = extra_level(MapId::Neighborhood).expect("the Neighborhood has its own streets");
    Simulation::new(seed, standard_rules().merged(streets))
}

/// A box on the ground: its centre, half extents along its own axes and its turn (as a
/// house's build frame turns into the world).
#[derive(Clone, Copy, Debug)]
struct Box2 {
    centre: DVec2,
    half: DVec2,
    turn: f64,
}

impl Box2 {
    fn of(
        kind: CoverKind,
        x: f64,
        z: f64,
        (w, d, h): (f64, f64, f64),
        plan: Option<sloppy_core::sim::house_layout::HousePlan>,
    ) -> Self {
        match plan {
            Some(plan) if kind == CoverKind::House => {
                let design = HouseDesign::planned(x, z, w, d, h, plan);
                Self {
                    centre: DVec2::new(x, z),
                    half: DVec2::new(design.w, design.d) / 2.0,
                    turn: design.yaw(),
                }
            }
            _ => Self {
                centre: DVec2::new(x, z),
                half: DVec2::new(w, d) / 2.0,
                turn: cover_turn(kind, plan),
            },
        }
    }

    fn cover(c: &CoverDef) -> Self {
        Self::of(c.kind, c.x, c.z, (c.w, c.d, c.h), c.house_plan)
    }

    fn of_cover(c: &Cover) -> Self {
        Self::of(c.kind, c.x, c.z, (c.w, c.d, c.h), c.house_plan)
    }

    /// Its own axes in the world.
    fn axes(&self) -> [DVec2; 2] {
        let (sin, cos) = self.turn.sin_cos();
        [DVec2::new(cos, -sin), DVec2::new(sin, cos)]
    }

    fn grown(self, by: f64) -> Self {
        Self {
            half: self.half + DVec2::splat(by),
            ..self
        }
    }

    fn corners(&self) -> [DVec2; 4] {
        let [a, b] = self.axes();
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(u, v)| self.centre + a * u * self.half.x + b * v * self.half.y)
    }

    /// `p` in its own frame.
    fn local(&self, p: DVec2) -> DVec2 {
        let [a, b] = self.axes();
        DVec2::new((p - self.centre).dot(a), (p - self.centre).dot(b))
    }

    fn contains(&self, p: DVec2) -> bool {
        let q = self.local(p);
        q.x.abs() < self.half.x && q.y.abs() < self.half.y
    }

    /// How far (m) `p` lies outside it (0 inside).
    fn distance(&self, p: DVec2) -> f64 {
        let q = self.local(p).abs() - self.half;
        q.max(DVec2::ZERO).length()
    }

    /// Whether the two boxes overlap (separating axes).
    fn overlaps(&self, other: &Box2) -> bool {
        let (mine, theirs) = (self.corners(), other.corners());
        self.axes().into_iter().chain(other.axes()).all(|axis| {
            let span = |points: &[DVec2; 4]| {
                points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                    (lo.min(p.dot(axis)), hi.max(p.dot(axis)))
                })
            };
            let (a, b) = (span(&mine), span(&theirs));
            a.0 < b.1 && b.0 < a.1
        })
    }
}

/// A house as it stands in the world.
fn standing(house: &SendaHouse) -> SendaHouse {
    SendaHouse {
        x: house.x * SENDA_SCALE,
        z: house.z * SENDA_SCALE,
        w: house.w * SENDA_SCALE,
        d: house.d * SENDA_SCALE,
        h: house.h * SENDA_SCALE,
        ..*house
    }
}

fn house_box(house: &SendaHouse) -> Box2 {
    Box2::of(
        CoverKind::House,
        house.x,
        house.z,
        (house.w, house.d, house.h),
        Some(house.plan),
    )
}

fn entries(layout: &[CoverDef]) -> Vec<&CoverDef> {
    layout
        .iter()
        .filter(|c| c.kind == CoverKind::Concrete && is_entry(c.color))
        .collect()
}

/// The entry cover `house` stands behind.
fn entry_of<'a>(layout: &'a [CoverDef], house: &SendaHouse) -> Vec<&'a CoverDef> {
    entries(layout)
        .into_iter()
        .filter(|c| {
            c.house_plan == Some(house.plan)
                && house_box(house).grown(0.1).overlaps(&Box2::cover(c))
        })
        .collect()
}

#[test]
fn every_home_has_one_entry_against_its_front_and_clear_of_its_drive_the_street_and_its_neighbours()
{
    let layout = senda_design_layout();
    let houses = senda_houses();
    assert_eq!(
        entries(&layout).len(),
        houses.len(),
        "one entry for every home"
    );
    for lot in senda_lots(&houses) {
        let house = lot.house;
        let found = entry_of(&layout, &house);
        assert_eq!(
            found.len(),
            1,
            "{:?} at {},{}: entries {found:?}",
            house.row,
            house.x,
            house.z
        );
        let cover = found[0];
        let place = entry_place(&house).expect("a placed entry");
        assert!(DVec2::new(cover.x, cover.z).distance(place.at) < 0.01);
        let entry = Box2::cover(cover);
        let own = house_box(&house);
        // Against the front wall, square to it, not into the house.
        assert!(
            own.grown(TOUCH).overlaps(&entry) && !own.grown(-TOUCH).overlaps(&entry),
            "{:?} at {},{}: the entry does not meet its front",
            house.row,
            house.x,
            house.z
        );
        let (door, garage) = front_openings(&lot).expect("a front door and a garage");
        let side = (door - garage).signum();
        let design = house.design();
        let drive = design.wall_plan(design.door_wall).garage_width() / 2.0;
        for corner in entry.corners() {
            let (u, v) = lot.local(corner);
            assert!(
                (u - garage) * side >= drive - TOUCH,
                "{:?} at {},{}: the entry reaches the driveway at {u}",
                house.row,
                house.x,
                house.z
            );
            // The streets' and the turning circles' paving, kerb to sidewalk.
            assert!(
                Street::ALL.into_iter().all(|street| {
                    street.line().distance(corner) >= street.half() + PARKWAY + SIDEWALK
                        && street.cul_de_sac().is_none_or(|middle| {
                            middle.distance(corner) >= CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK
                        })
                }),
                "{:?} at {},{}: the entry stands on the street's paving at ({u}, {v})",
                house.row,
                house.x,
                house.z,
            );
            assert!(
                lot.inside(corner) > 0.0,
                "{:?} at {},{}: the entry leaves its lot at ({u}, {v})",
                house.row,
                house.x,
                house.z
            );
        }
        // No other cover stands in it, its own house only touching.
        for other in layout.iter().filter(|c| !std::ptr::eq(*c, cover)) {
            let other_box = Box2::cover(other);
            let theirs = if other.kind == CoverKind::House {
                other_box.grown(-TOUCH)
            } else {
                other_box
            };
            assert!(
                !theirs.overlaps(&entry),
                "{:?} at {},{}: a {:?} at {},{} stands in the entry",
                house.row,
                house.x,
                house.z,
                other.kind,
                other.x,
                other.z
            );
        }
        for pickup in SENDA_PICKUPS {
            assert!(
                entry.distance(DVec2::new(pickup.x, pickup.z)) > 2.0,
                "a pickup by the entry at {},{}",
                cover.x,
                cover.z
            );
        }
        for (lamp, _) in lamp_sites() {
            assert!(
                !entry.grown(0.2).contains(lamp),
                "a lamp in the entry at {},{}",
                cover.x,
                cover.z
            );
        }
    }
}

#[test]
fn entries_block_tanks_and_leave_the_street_before_them_open() {
    let mut sim = senda(23.0);
    let houses = senda_houses();
    let layout = (SENDA_MAP.layout)();
    let boxes: Vec<Box2> = entries(&layout).into_iter().map(Box2::cover).collect();
    for entry in &boxes {
        // A tank's box cannot pass through: every cell inside the entry is blocked.
        let [a, b] = entry.axes();
        for (u, v) in [(0.0, 0.0), (-0.8, 0.0), (0.8, 0.0), (0.0, -0.8), (0.0, 0.8)] {
            let p = entry.centre + a * u * entry.half.x + b * v * entry.half.y;
            assert!(
                sim.nav.is_blocked(Vec2::new(p.x, p.y)),
                "the entry at {} lets a tank through at {p}",
                entry.centre
            );
        }
    }
    // Before every home a bot still finds its way past along the street's side wherever
    // it could with no entries standing (a few lots' sidewalks lie in pockets by the
    // level's wall either way).
    let mut open = senda(23.0);
    let tank = open.tanks[0].id;
    for index in 0..open.covers.len() {
        if open.covers[index].kind == CoverKind::Concrete && is_entry(open.covers[index].color) {
            open.damage_cover(index, 10_000.0, tank, Team::Blue, None, None);
        }
    }
    let way_past = |sim: &mut Simulation, lot: &sloppy_core::sim::senda_yards::Lot| {
        let v = lot.half_depth + lot.house.setback + SIDEWALK / 2.0;
        // From the sidewalk's middle at each lot line, out toward the street to the first
        // open cell (a cell on the house's side may be a pocket beside the entry).
        let ends = [-1.0, 1.0].map(|side| {
            let at = |out: f64| {
                let p = lot.at(side * lot.half_width, v + out) * SENDA_SCALE;
                Vec2::new(p.x, p.y)
            };
            (0..=STREET_STEPS)
                .map(|step| at(f64::from(step) * 0.25))
                .find(|&p| !sim.nav.is_blocked(p))
                .unwrap_or_else(|| sim.nav.point(sim.nav.nearest(sim.nav.index(at(0.0)))))
        });
        let path = sim.nav.find(ends[0], ends[1]);
        path.last()
            .is_some_and(|end| distance(*end, ends[1]) < ARRIVAL)
            .then_some(path)
    };
    let mut compared = 0;
    for lot in senda_lots(&houses) {
        if way_past(&mut open, &lot).is_none() {
            continue;
        }
        compared += 1;
        let path = way_past(&mut sim, &lot);
        assert!(
            path.is_some(),
            "no way past the home at {},{} with its entry standing",
            lot.house.x,
            lot.house.z,
        );
        for p in path.iter().flatten() {
            assert!(
                boxes
                    .iter()
                    .all(|entry| !entry.contains(DVec2::new(p.x, p.z))),
                "the way past {},{} runs through an entry at {p:?}",
                lot.house.x,
                lot.house.z
            );
        }
    }
    assert!(
        compared * 4 >= houses.len() * 3,
        "only {compared} homes compared"
    );
}

#[test]
fn an_entry_brought_down_leaves_its_house_standing_and_its_ground_free() {
    let mut sim = senda(31.0);
    let houses = senda_houses();
    let mut freed = 0;
    for house in &houses {
        let house = &standing(house);
        let house_index = sim
            .covers
            .iter()
            .position(|c| c.kind == CoverKind::House && c.x == house.x && c.z == house.z)
            .expect("the house's cover");
        let index = sim
            .covers
            .iter()
            .position(|c| {
                c.kind == CoverKind::Concrete
                    && is_entry(c.color)
                    && c.house_plan == Some(house.plan)
                    && house_box(house).grown(0.1).overlaps(&Box2::of_cover(c))
            })
            .expect("the house's entry");
        let entry = Box2::of_cover(&sim.covers[index]);
        // The cells only the entry blocks: within its clearance, well away from the rest.
        let others: Vec<Box2> = sim
            .covers
            .iter()
            .enumerate()
            .filter(|(k, c)| *k != index && (c.alive || c.kind == CoverKind::Tree))
            .map(|(_, c)| Box2::of_cover(c))
            .collect();
        let near = |p: DVec2| entry.distance(p) < 1.35 - HALF_CELL * 0.2;
        let alone = |p: DVec2| others.iter().all(|o| o.distance(p) > 1.35 + 0.5);
        let around: Vec<Vec2> = (-3..=3)
            .flat_map(|i| (-3..=3).map(move |j| (i, j)))
            .map(|(i, j)| {
                let p = entry.centre + DVec2::new(f64::from(i), f64::from(j)) * 2.0 * HALF_CELL;
                sim.nav.point(sim.nav.index(Vec2::new(p.x, p.y)))
            })
            .filter(|p| near(DVec2::new(p.x, p.z)) && alone(DVec2::new(p.x, p.z)))
            .collect();
        for p in &around {
            assert!(
                sim.nav.is_blocked(*p),
                "the entry at {} leaves {p:?} open",
                entry.centre
            );
        }
        let collider = sim.covers[index].collider;
        let tank = sim.tanks[0].id;
        sim.damage_cover(index, 10_000.0, tank, Team::Blue, None, None);
        let cover = &sim.covers[index];
        assert!(!cover.alive, "the entry at {} stands", entry.centre);
        assert!(
            !sim.cover_by_collider.contains_key(&collider),
            "its body stays"
        );
        let standing = &sim.covers[house_index];
        assert!(
            standing.alive
                && standing
                    .structure
                    .as_ref()
                    .is_some_and(|structure| structure.gone.is_empty()),
            "the house at {},{} broke with its entry",
            house.x,
            house.z
        );
        for p in &around {
            assert!(
                !sim.nav.is_blocked(*p),
                "the fallen entry at {} still blocks {p:?}",
                entry.centre
            );
        }
        freed += around.len();
    }
    assert!(freed >= houses.len() / 2, "only {freed} cells checked");
}

#[test]
fn a_shell_from_the_street_strikes_an_entry() {
    use sloppy_core::sim::data::{STEP, weapon};
    use sloppy_core::sim::projectiles::step_projectiles;
    use sloppy_core::sim::{Shot, Weapon};
    let mut sim = senda(5.0);
    sim.start();
    let houses = senda_houses();
    let house = houses
        .iter()
        .find(|house| house.plan.style == REFERENCE_HOUSE)
        .map(standing)
        .expect("the reference house");
    let house = &house;
    let index = sim
        .covers
        .iter()
        .position(|c| {
            c.kind == CoverKind::Concrete
                && is_entry(c.color)
                && c.house_plan == Some(house.plan)
                && house_box(house).grown(0.1).overlaps(&Box2::of_cover(c))
        })
        .expect("the house's entry");
    let entry = Box2::of_cover(&sim.covers[index]);
    // Out along the entry's own +z (away from the house), toward its middle.
    let [_, out] = entry.axes();
    let from = entry.centre + out * 5.0;
    let hp = sim.covers[index].hp;
    sim.shots.push(Shot {
        id: 999,
        x: from.x,
        z: from.y,
        vx: -out.x * 180.0,
        vz: -out.y * 180.0,
        owner: 999,
        team: Team::Blue,
        damage: weapon(Weapon::Standard).damage,
        bounces: 0,
        life: 2.0,
        piercing: 0,
        weapon: Weapon::Standard,
        ..Shot::default()
    });
    for _ in 0..10 {
        step_projectiles(&mut sim, STEP, false);
    }
    assert!(
        sim.covers[index].hp < hp,
        "the shell passed the entry at {}",
        entry.centre
    );
}
