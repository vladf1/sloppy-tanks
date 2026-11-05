//! Cottages built of parts: a shell breaks the part it strikes and leaves a real gap,
//! an HE round bursts a fuse length inside, an armour-piercing round goes through,
//! the roof comes down where its support is gone (not after a count of hits), and the
//! fallen parts clear away.

use crate::{net_support, support};

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, weapon};
use sloppy_core::sim::house_layout::{HousePart, RoofForm};
use sloppy_core::sim::house_physics::{FUSE_DISTANCE, part_box};
use sloppy_core::sim::house_structure::{CHIMNEY_PART, HouseStructure, PartKind, PartSet};
use sloppy_core::sim::math::{Point3, Vec2};
use sloppy_core::sim::{CoverKind, SimEventType, Simulation, Team, Weapon};
use support::{clear_arena, idle, place_tank};

/// The arena's cottage at (24, 0): 5 m across x, 7 m along z, its door facing -x.
fn lone_cottage() -> (Simulation, usize) {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -40.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let house = s.add_cover(&CoverDef::house(24.0, 0.0, 5.0, 7.0, 4.9, 0xb87b4c));
    s.nav.rebuild(&s.covers, None);
    (s, house)
}

fn gone(s: &Simulation, house: usize) -> PartSet {
    s.covers[house].structure.as_ref().expect("a cottage").gone
}

fn kind(s: &Simulation, house: usize, part: usize) -> PartKind {
    s.covers[house]
        .structure
        .as_ref()
        .expect("a cottage")
        .layout
        .parts[part]
        .kind
}

/// A shell of `weapon` at combat height from `from` toward `to`.
fn fire(s: &mut Simulation, from: Vec2, to: Vec2, weapon_kind: Weapon) -> u32 {
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let length = dx.hypot(dz);
    let stats = weapon(weapon_kind);
    let id = s.allocate_id();
    s.shots.push(sloppy_core::sim::Shot {
        id,
        x: from.x,
        z: from.z,
        y: Some(1.0),
        vx: dx / length * stats.speed,
        vz: dz / length * stats.speed,
        team: Team::Blue,
        owner: s.tanks[0].id,
        weapon: weapon_kind,
        damage: stats.damage,
        piercing: u32::from(weapon_kind == Weapon::Piercing),
        life: 3.0,
        ..Default::default()
    });
    id
}

fn run(s: &mut Simulation, seconds: f64) {
    for _ in 0..(seconds / STEP).round() as usize {
        idle(s);
    }
}

/// The standing parts whose boxes the line at combat height along z through `x`
/// crosses, nearest +z first.
fn parts_on_line(s: &Simulation, house: usize, x: f64) -> Vec<usize> {
    let state = s.covers[house].structure.as_ref().expect("a cottage");
    let mut parts: Vec<(f64, usize)> = (0..state.layout.parts.len())
        .filter(|&index| state.standing(index))
        .filter_map(|index| {
            let (min, max) = part_box(&s.covers[house], index)?;
            (min.x < x && max.x > x && min.y < 1.0 && max.y > 1.0).then_some((-max.z, index))
        })
        .collect();
    parts.sort_by(|a, b| a.0.total_cmp(&b.0));
    parts.into_iter().map(|(_, index)| index).collect()
}

/// A line along z through the cottage that crosses only its two gable-end walls (no
/// partition, posts or openings in between), as x.
fn clear_line(s: &Simulation, house: usize) -> f64 {
    let c = &s.covers[house];
    // Nearest the middle first, clear of the end walls a burst inside would reach.
    (0..50)
        .map(|k| c.x + f64::from(if k % 2 == 0 { k / 2 } else { -(k + 1) / 2 }) * 0.05)
        .find(|&x| {
            let parts = parts_on_line(s, house, x);
            parts.len() == 2 && parts.iter().all(|&p| kind(s, house, p) == PartKind::Panel)
        })
        .expect("a line through two wall panels")
}

#[test]
fn a_shell_breaks_the_panel_it_strikes_and_the_gap_lets_the_next_through() {
    let (mut s, house) = lone_cottage();
    let x = clear_line(&s, house);
    let [front, back] = parts_on_line(&s, house, x)[..] else {
        unreachable!()
    };
    let rng = s.rng.state;
    let (near, far) = (Vec2::new(x, 20.0), Vec2::new(x, -20.0));
    // Only the struck panel, though the burst inside may blow out the glass over it.
    let only = |s: &Simulation, struck: &[usize]| {
        let gone = gone(s, house);
        struck.iter().all(|&part| gone.contains(part))
            && gone
                .iter()
                .all(|part| struck.contains(&part) || kind(s, house, part) == PartKind::Window)
    };
    fire(&mut s, near, far, Weapon::Standard);
    run(&mut s, 1.5);
    assert!(only(&s, &[front]), "{:?}", gone(&s, house));
    // From the other side: its own panel.
    fire(&mut s, far, near, Weapon::Standard);
    run(&mut s, 1.5);
    assert!(only(&s, &[front, back]), "{:?}", gone(&s, house));
    // Now a shell in through one hole and out through the other touches nothing.
    let opened = gone(&s, house);
    let before = s.covers[house].structure.as_ref().unwrap().damage.clone();
    let id = fire(&mut s, near, far, Weapon::Standard);
    let mut out = false;
    for _ in 0..90 {
        idle(&mut s);
        if let Some(shot) = s.shots.iter().find(|shot| shot.id == id)
            && shot.z < s.covers[house].z - s.covers[house].d / 2.0 - 1.0
        {
            out = true;
        }
    }
    assert!(out, "the shell flew on out of the far hole");
    assert_eq!(gone(&s, house), opened);
    assert_eq!(
        s.covers[house].structure.as_ref().unwrap().damage,
        before,
        "nothing was struck"
    );
    assert_eq!(s.rng.state, rng, "a cottage's damage draws nothing");
}

#[test]
fn an_he_shell_bursts_a_fuse_length_inside_and_breaks_what_is_there() {
    let (mut s, house) = lone_cottage();
    // A partition panel by an outer wall, and a line along it just clear of its face.
    let state = s.covers[house].structure.as_ref().unwrap();
    let partition = (0..state.layout.parts.len())
        .filter(|&i| state.layout.parts[i].kind == PartKind::Partition)
        .map(|i| (i, part_box(&s.covers[house], i).unwrap()))
        .max_by(|a, b| (a.1.1.z.max(-a.1.0.z)).total_cmp(&(b.1.1.z.max(-b.1.0.z))))
        .expect("a partition");
    let (index, (min, max)) = partition;
    let c = s.covers[house].clone();
    // The partition runs across z here (the ridge runs along x), so the shell comes in
    // along z through the nearer long wall, beside the panel.
    let reaches_z = (max.z - min.z) > (max.x - min.x);
    assert!(reaches_z, "the partition spans z: {min:?} {max:?}");
    let x = max.x + 0.25;
    let (start, face) = if max.z > c.z {
        (Vec2::new(x, c.z + 20.0), c.z + c.d / 2.0)
    } else {
        (Vec2::new(x, c.z - 20.0), c.z - c.d / 2.0)
    };
    fire(&mut s, start, Vec2::new(x, c.z), Weapon::Standard);
    let mut burst = None;
    for _ in 0..90 {
        s.events.clear();
        idle(&mut s);
        if let Some(event) = s
            .events
            .iter()
            .find(|event| event.kind == SimEventType::Explosion)
        {
            burst = Some(Point3::new(event.x, event.height.unwrap_or(1.0), event.z));
            break;
        }
    }
    let burst = burst.expect("the shell burst");
    let inside = (burst.z - face).abs();
    assert!(
        (inside - FUSE_DISTANCE).abs() < 0.1,
        "burst {inside} m in from the face"
    );
    assert!(
        gone(&s, house).contains(index),
        "the partition by the burst broke"
    );
    assert!(
        gone(&s, house).len() >= 2,
        "the struck wall panel and the partition"
    );
}

#[test]
fn an_armour_piercing_round_goes_through_breaking_only_what_it_touches() {
    let (mut s, house) = lone_cottage();
    let x = clear_line(&s, house);
    let crossed = parts_on_line(&s, house, x);
    let id = fire(
        &mut s,
        Vec2::new(x, 20.0),
        Vec2::new(x, -20.0),
        Weapon::Piercing,
    );
    let mut explosions = 0;
    let mut beyond = false;
    for _ in 0..90 {
        s.events.clear();
        idle(&mut s);
        explosions += s
            .events
            .iter()
            .filter(|event| event.kind == SimEventType::Explosion)
            .count();
        beyond |= s.shots.iter().any(|shot| shot.id == id && shot.z < -4.5);
    }
    assert_eq!(explosions, 0, "no burst");
    assert!(beyond, "the round flew on through");
    // What it touched, and only the windows that stood on it.
    let gone = gone(&s, house);
    assert!(crossed.iter().all(|&part| gone.contains(part)));
    assert!(
        gone.iter()
            .all(|part| crossed.contains(&part) || kind(&s, house, part) == PartKind::Window),
        "{gone:?}"
    );
}

#[test]
fn shells_through_open_gaps_never_level_a_cottage() {
    let (mut s, house) = lone_cottage();
    let x = clear_line(&s, house);
    let (near, far) = (Vec2::new(x, 20.0), Vec2::new(x, -20.0));
    fire(&mut s, near, far, Weapon::Standard);
    run(&mut s, 1.0);
    fire(&mut s, far, near, Weapon::Standard);
    run(&mut s, 1.0);
    let opened = gone(&s, house);
    // Forty more shells through the same holes: no count of hits fells it.
    for k in 0..40 {
        let (a, b) = if k % 2 == 0 { (near, far) } else { (far, near) };
        fire(&mut s, a, b, Weapon::Standard);
        run(&mut s, 0.9);
    }
    assert_eq!(gone(&s, house), opened);
    assert!(s.covers[house].alive);
}

#[test]
fn the_roof_comes_down_where_the_wall_under_it_is_gone() {
    let (mut s, house) = lone_cottage();
    let rng = s.rng.state;
    let bodies = s.world.bodies.len();
    let state = s.covers[house].structure.as_ref().unwrap();
    let layout = state.layout.clone();
    // The long wall on +z (an eave wall: the ridge runs along x) and its corner posts.
    let wall: Vec<usize> = (0..layout.parts.len())
        .filter(|&i| {
            let (min, max) = part_box(&s.covers[house], i).unwrap();
            min.z > s.covers[house].z + s.covers[house].d / 2.0 - 0.35
                && max.y < 3.5
                && matches!(
                    layout.parts[i].kind,
                    PartKind::Panel
                        | PartKind::Window
                        | PartKind::Door
                        | PartKind::Post
                        | PartKind::Plate
                )
        })
        .collect();
    assert!(wall.len() > 10, "{wall:?}");
    let sheets_over = |s: &Simulation| -> Vec<usize> {
        (0..layout.parts.len())
            .filter(|&i| layout.parts[i].kind == PartKind::Sheet)
            .filter(|&i| part_box(&s.covers[house], i).unwrap().0.z > s.covers[house].z + 0.5)
            .collect()
    };
    let over = sheets_over(&s);
    assert!(!over.is_empty());
    // Most of the wall goes; until its plates do, the roof stands.
    let (plates, rest): (Vec<usize>, Vec<usize>) = wall
        .iter()
        .partition(|&&i| layout.parts[i].kind == PartKind::Plate);
    s.break_house_parts(house, PartSet::from_indices(rest.clone()), None, None);
    let standing_sheets = over
        .iter()
        .filter(|&&i| !gone(&s, house).contains(i))
        .count();
    let falling: usize = s
        .fragments
        .iter()
        .filter_map(|f| f.house_parts)
        .map(|parts| parts.len())
        .sum();
    // Bare plates spanning a whole wall do not carry it: they and the roof on them come
    // down; or they hold for now as beams between what still stands.
    if standing_sheets == over.len() {
        assert_eq!(falling, 0);
        s.break_house_parts(house, PartSet::from_indices(plates), None, None);
    }
    for &sheet in &over {
        assert!(gone(&s, house).contains(sheet), "sheet {sheet} came down");
    }
    let fallen: PartSet = s
        .fragments
        .iter()
        .filter_map(|f| f.house_parts)
        .fold(PartSet::EMPTY, |all, parts| all.union(&parts));
    assert!(
        over.iter().all(|&sheet| fallen.contains(sheet)),
        "falling as bodies"
    );
    // The far slope still stands on its own wall.
    let far: Vec<usize> = (0..layout.parts.len())
        .filter(|&i| layout.parts[i].kind == PartKind::Sheet)
        .filter(|&i| part_box(&s.covers[house], i).unwrap().1.z < s.covers[house].z - 1.0)
        .collect();
    assert!(
        far.iter().any(|&i| !gone(&s, house).contains(i)),
        "the far slope stands"
    );
    // The fallen parts land, lie a while and sink away: none is left, and no body.
    for _ in 0..40 {
        run(&mut s, 1.0);
        assert!(s.fragments.len() <= s.max_fragments);
    }
    assert!(
        s.fragments
            .iter()
            .all(|f| f.house_id != Some(s.covers[house].id))
    );
    assert_eq!(s.world.bodies.len(), bodies, "every fallen body is gone");
    assert_eq!(s.rng.state, rng, "a collapse draws nothing");
}

#[test]
fn a_rebuilt_cottage_stands_whole_again() {
    let (mut s, house) = lone_cottage();
    let colliders = s.world.colliders.len();
    let all = s.covers[house].structure.as_ref().unwrap().layout.all();
    s.break_house_parts(house, all, None, None);
    assert!(!s.covers[house].alive);
    s.restore_cover(house);
    assert!(s.covers[house].alive);
    assert!(gone(&s, house).is_empty());
    assert_eq!(s.world.colliders.len(), colliders);
    assert_eq!(s.covers[house].kind, CoverKind::House);
}

#[test]
fn broken_parts_and_falling_bodies_reach_room_clients_and_late_joiners() {
    use net_support::{apply_batch, baseline, batch};
    use sloppy_core::net::multiplayer_simulation::{
        MultiplayerOptions, create_multiplayer_simulation,
    };
    use sloppy_core::net::replication::{StateMirror, StateStream};
    use sloppy_core::net::scene_codec::{MirrorScene, Scene};
    use sloppy_core::sim::map_options::MapId;
    use sloppy_core::sim::types::{PlayerAssignment, VehicleKind};
    let players = [PlayerAssignment {
        player_id: "one".into(),
        name: "One".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    }];
    let mut s = create_multiplayer_simulation(
        4242.0,
        &players,
        MultiplayerOptions {
            map_mode: Some(MapId::Village),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    s.start();
    let house = (0..s.covers.len())
        .find(|&i| s.covers[i].kind == CoverKind::House)
        .expect("a cottage");
    let id = s.covers[house].id;
    let viewer = s.human().id;
    let mut stream = StateStream::new("room", 1);
    let mut mirror = StateMirror::default();
    let scene = Scene::capture(&s);
    mirror
        .apply_full(
            &baseline(&stream.full(0, 0, [], || scene.clone())),
            "room",
            1,
        )
        .unwrap();
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    // A whole wall's lower courses and posts, one part a tick, then its plates.
    let wall: Vec<usize> = (0..layout.parts.len())
        .filter(|&i| layout.parts[i].wall == Some(2) || layout.parts[i].kind == PartKind::Post)
        .collect();
    for (tick, &part) in wall.iter().enumerate() {
        s.break_house_parts(house, PartSet::from_indices([part]), None, None);
        let mut next = Scene::capture(&s);
        let body = stream.snapshot(&mut next, tick as u64 + 1, &[], &[]);
        apply_batch(
            &mut mirror,
            &batch(1, 0, stream.seq, &[(tick as u64 + 1, body)]),
        );
        let shown = mirror.render(viewer).unwrap();
        let cover = shown.covers.iter().find(|cover| cover.id == id).unwrap();
        assert_eq!(cover.broken, gone(&s, house), "tick {tick}");
        for fragment in s.fragments.iter().filter(|f| f.house_id == Some(id)) {
            let seen = shown
                .fragments
                .iter()
                .find(|shown| shown.id == fragment.id)
                .expect("each falling body reaches the client");
            assert_eq!(seen.house_parts, fragment.house_parts);
        }
    }
    assert!(
        s.fragments.iter().any(|f| f.house_id == Some(id)),
        "something fell"
    );
    // A client joining now sees the cottage as it stands and what is falling.
    let scene = Scene::capture(&s);
    let late = MirrorScene::from_scene(scene)
        .unwrap()
        .render(viewer)
        .unwrap();
    let cover = late.covers.iter().find(|cover| cover.id == id).unwrap();
    assert_eq!(cover.broken, gone(&s, house));
    let falling: Vec<_> = s
        .fragments
        .iter()
        .filter(|f| f.house_id == Some(id))
        .map(|f| (f.id, f.house_parts))
        .collect();
    let seen: Vec<_> = late
        .fragments
        .iter()
        .filter(|f| f.house_id == Some(id))
        .map(|f| (f.id, f.house_parts))
        .collect();
    assert_eq!(seen, falling);
}

#[test]
fn tanks_find_a_way_through_a_gutted_cottage() {
    let (mut s, house) = lone_cottage();
    let c = s.covers[house].clone();
    let through = |s: &mut Simulation| {
        let from = Vec2::new(c.x, c.z + 12.0);
        let to = Vec2::new(c.x, c.z - 12.0);
        let path = s.nav.find(from, to);
        !path.is_empty() && path.iter().all(|p| (p.x - c.x).abs() < c.w / 2.0 + 0.5)
    };
    assert!(!through(&mut s), "a whole cottage is in the way");
    // Its two long walls broken open over their middle (lower courses, posts kept).
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    let open: Vec<usize> = (0..layout.parts.len())
        .filter(|&i| {
            let (min, max) = part_box(&s.covers[house], i).unwrap();
            layout.parts[i].blocks_ground()
                && layout.parts[i].kind != PartKind::Post
                && (min.x + max.x) / 2.0 > c.x - 1.9
                && (min.x + max.x) / 2.0 < c.x + 1.9
        })
        .collect();
    s.break_house_parts(house, PartSet::from_indices(open), None, None);
    assert!(through(&mut s), "a path runs through the gap");
}

#[test]
fn a_shot_out_corner_brings_down_its_eave_and_the_roof_over_it() {
    let (mut s, house) = lone_cottage();
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    let c = s.covers[house].clone();
    // The +x/+z corner's post and the wall round it within 2.2 m.
    let corner = Point3::new(c.x + c.w / 2.0, 0.0, c.z + c.d / 2.0);
    let near = |i: usize| {
        let (min, max) = part_box(&s.covers[house], i).unwrap();
        let x = corner.x.clamp(min.x, max.x) - corner.x;
        let z = corner.z.clamp(min.z, max.z) - corner.z;
        x.hypot(z) < 2.2
    };
    let shot: Vec<usize> = (0..layout.parts.len())
        .filter(|&i| {
            layout.parts[i].kind.outer_wall() && layout.parts[i].kind != PartKind::Gable && near(i)
        })
        .collect();
    assert!(shot.iter().any(|&i| layout.parts[i].kind == PartKind::Post));
    s.break_house_parts(house, PartSet::from_indices(shot.clone()), None, None);
    let fallen: PartSet = s
        .fragments
        .iter()
        .filter_map(|f| f.house_parts)
        .fold(PartSet::EMPTY, |all, parts| all.union(&parts));
    // The roof sheet over the corner comes down with what is left of its eave.
    let over_corner = (0..layout.parts.len()).find(|&i| {
        let (min, max) = part_box(&s.covers[house], i).unwrap();
        layout.parts[i].kind == PartKind::Sheet
            && min.x <= corner.x + 0.5
            && max.x >= corner.x - 0.5
            && max.z >= corner.z - 0.2
    });
    let sheet = over_corner.expect("a sheet over the corner");
    assert!(
        fallen.contains(sheet),
        "the sheet over the corner fell: {fallen:?}"
    );
    // The far corner's roof still stands.
    assert!(s.covers[house].alive);
}

#[test]
fn a_roof_coming_down_never_lies_up_on_the_remains_and_breaks_nothing_it_did_not_strike() {
    let (mut s, house) = lone_cottage();
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    // An eave wall shot out above its lowest course: the slope over it comes down on
    // the gables and the stubs left standing.
    let eave = 2;
    let upper: Vec<usize> = (0..layout.parts.len())
        .filter(|&index| layout.parts[index].wall == Some(eave) && !layout.parts[index].grounded)
        .collect();
    let gables: Vec<usize> = (0..layout.parts.len())
        .filter(|&index| layout.parts[index].kind == PartKind::Gable)
        .collect();
    let rng = s.rng.state;
    s.break_house_parts(house, PartSet::from_indices(upper), None, None);
    assert!(
        s.fragments
            .iter()
            .any(|fragment| fragment.house_parts.is_some()),
        "the slope fell"
    );
    let gables_before = gables
        .iter()
        .filter(|&&g| !gone(&s, house).contains(g))
        .count();
    run(&mut s, 4.0);
    // Nothing lies up on what stands, like a lid.
    for fragment in s
        .fragments
        .iter()
        .filter(|fragment| fragment.house_parts.is_some())
    {
        let body = &s.world.bodies[fragment.body];
        assert!(
            body.center_of_mass().y < 1.5 || body.linvel().length() > 0.2,
            "a loose part rests up at {:?}",
            body.center_of_mass()
        );
    }
    // The gables it slid off stand.
    let gables_after = gables
        .iter()
        .filter(|&&g| !gone(&s, house).contains(g))
        .count();
    assert!(
        gables_after > 0 && gables_after + 1 >= gables_before,
        "{gables_before} -> {gables_after}"
    );
    assert_eq!(s.rng.state, rng, "no random draw");
}

#[test]
fn a_falling_part_striking_a_standing_one_hard_breaks_it() {
    use sloppy_core::sim::house_physics::crush_house_parts;
    use sloppy_core::sim::physics::ContactForce;
    let (mut s, house) = lone_cottage();
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    // A slope comes loose; one of its bodies strikes a wall stub.
    let eave = 2;
    let upper: Vec<usize> = (0..layout.parts.len())
        .filter(|&index| layout.parts[index].wall == Some(eave) && !layout.parts[index].grounded)
        .collect();
    s.break_house_parts(house, PartSet::from_indices(upper), None, None);
    let falling = s
        .fragments
        .iter()
        .find(|fragment| fragment.house_parts.is_some())
        .map(|fragment| s.world.bodies[fragment.body].colliders()[0])
        .expect("a falling body");
    let stub = (0..layout.parts.len())
        .find(|&index| {
            layout.parts[index].wall == Some(eave)
                && layout.parts[index].grounded
                && !gone(&s, house).contains(index)
        })
        .expect("a stub");
    let struck = s.covers[house].structure.as_ref().unwrap().colliders[stub].expect("standing");
    let contact = |force: f32| ContactForce {
        collider1: falling,
        collider2: struck,
        total_force_magnitude: force,
    };
    // Its weight: nothing.
    crush_house_parts(&mut s, &[contact(60.0)]);
    assert!(
        !gone(&s, house).contains(stub),
        "resting weight breaks nothing"
    );
    // A hard landing: the stub breaks.
    let body = s.world.colliders[falling].parent().unwrap();
    for fragment in s.fragments.iter_mut().filter(|f| f.body == body) {
        fragment.house_velocity = Some(Point3::new(0.0, -12.0, 0.0));
    }
    crush_house_parts(&mut s, &[contact(1200.0)]);
    assert!(gone(&s, house).contains(stub), "a hard strike breaks it");
}

/// Every cottage the maps place (the village's gable and hip roofs, its two-storey
/// houses, the Stress Grid's and the Scrap Yard's), as their simulations build them.
fn every_placed_cottage() -> Vec<std::sync::Arc<HouseStructure>> {
    use sloppy_core::sim::SimulationSetup;
    use sloppy_core::sim::level_rules::single_player_rules;
    use sloppy_core::sim::stress_test_level::stress_test_level;
    use sloppy_core::sim::superstress_level::superstress_level;
    let sims = [
        Simulation::with_seed(731.0),
        Simulation::new(
            731.0,
            single_player_rules(stress_test_level()).merged(SimulationSetup::default()),
        ),
        Simulation::new(
            731.0,
            single_player_rules(superstress_level()).merged(SimulationSetup::default()),
        ),
    ];
    sims.iter()
        .flat_map(|sim| sim.covers.iter())
        .filter_map(|cover| cover.structure.as_ref().map(|state| state.layout.clone()))
        .collect()
}

#[test]
fn every_placed_cottage_stands_and_drops_a_slope_whose_eave_wall_is_gone() {
    let cottages = every_placed_cottage();
    assert!(
        cottages.iter().any(|c| c.roof.form == RoofForm::Hip),
        "a hip roof"
    );
    assert!(
        cottages.iter().any(|c| c.roof.form == RoofForm::Gable),
        "a gable roof"
    );
    assert!(
        cottages.iter().any(|c| c.design.storeys() == 2),
        "a two-storey house"
    );
    for layout in &cottages {
        let what = format!(
            "{:?} at {:?}",
            layout.roof.form,
            (layout.design.w, layout.design.d, layout.design.h)
        );
        assert!(
            layout.unsupported(&PartSet::EMPTY).is_empty(),
            "{what} stands whole"
        );
        for (side, slope) in [(2, HousePart::RightRoof), (3, HousePart::LeftRoof)] {
            let gone = PartSet::from_indices(
                (0..layout.parts.len()).filter(|&index| layout.parts[index].wall == Some(side)),
            );
            let falling = layout.unsupported(&gone);
            for (index, part) in layout.parts.iter().enumerate() {
                if part.slope.is_some_and(|(over, _)| over == slope) {
                    assert!(
                        falling.contains(index),
                        "{what}: sheet {index} over wall {side} stays up"
                    );
                }
            }
            assert!(
                (0..4).all(|post| !falling.contains(post)),
                "{what}: posts stand"
            );
        }
        // With the roof down nothing above the walls is left floating.
        let roof = PartSet::from_indices(
            (0..layout.parts.len()).filter(|&index| layout.parts[index].kind.roof()),
        );
        let down = roof.union(&layout.unsupported(&roof));
        assert!(
            down.contains(CHIMNEY_PART),
            "{what}: the chimney comes down with the roof"
        );
    }
}

/// The cottage's loose bodies: (fragment id, parts, middle, speed).
fn loose_bodies(s: &Simulation, house: usize) -> Vec<(u32, PartSet, Point3, f64)> {
    let id = s.covers[house].id;
    s.fragments
        .iter()
        .filter(|f| f.house_id == Some(id) && f.house_parts.is_some())
        .map(|f| {
            let body = &s.world.bodies[f.body];
            let at = body.center_of_mass();
            (
                f.id,
                f.house_parts.unwrap(),
                Point3::new(at.x.into(), at.y.into(), at.z.into()),
                body.linvel().length().into(),
            )
        })
        .collect()
}

/// An eave wall shot out above its footing: the slope over it comes down, lands and
/// comes to rest. Returns the biggest loose body's fragment id.
fn fallen_roof(s: &mut Simulation, house: usize) -> u32 {
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    let wall: Vec<usize> = (0..layout.parts.len())
        .filter(|&index| layout.parts[index].wall == Some(2))
        .collect();
    s.break_house_parts(house, PartSet::from_indices(wall), None, None);
    run(s, 8.0);
    let bodies = loose_bodies(s, house);
    let (id, parts, at, speed) = *bodies
        .iter()
        .max_by_key(|(id, parts, _, _)| (parts.len(), std::cmp::Reverse(*id)))
        .expect("the slope fell");
    assert!(
        parts.len() >= 2,
        "a section of several parts lies there: {bodies:?}"
    );
    assert!(speed < 0.1, "it has come to rest: {speed}");
    assert!(at.y < 1.5, "on the ground: {at:?}");
    // No slope lies whole: each broke into pieces when it came down.
    let slope_sheets = (0..layout.parts.len())
        .filter(|&index| {
            layout.parts[index]
                .slope
                .is_some_and(|(slope, _)| slope == HousePart::RightRoof)
        })
        .count();
    for (_, parts, _, _) in &bodies {
        let sheets = parts
            .iter()
            .filter(|&part| layout.parts[part].kind == PartKind::Sheet)
            .count();
        assert!(
            sheets < slope_sheets,
            "a slope lies whole: {sheets} of {slope_sheets}"
        );
    }
    id
}

/// A standard shell from 15 m off toward `at`, along whichever axis is clear of the
/// cottage's remains.
fn fire_at(s: &mut Simulation, house: usize, at: Point3, weapon_kind: Weapon) {
    let c = &s.covers[house];
    let away = Vec2::new(at.x - c.x, at.z - c.z);
    let length = away.x.hypot(away.z).max(1e-6);
    let from = Vec2::new(at.x + away.x / length * 15.0, at.z + away.z / length * 15.0);
    fire(s, from, Vec2::new(at.x, at.z), weapon_kind);
}

#[test]
fn a_fallen_roof_section_breaks_under_shells_down_to_debris_that_clears() {
    let (mut s, house) = lone_cottage();
    let rng = s.rng.state;
    let section = fallen_roof(&mut s, house);
    let (_, parts, at, _) = loose_bodies(&s, house)
        .into_iter()
        .find(|body| body.0 == section)
        .unwrap();
    let body = s.fragments.iter().find(|f| f.id == section).unwrap().body;
    assert!(
        s.world.bodies[body].is_sleeping() || s.world.bodies[body].linvel().length() < 0.05,
        "it lies still"
    );
    // Shells at it (any other piece in their way breaks first): the struck part
    // shatters, the rest breaks into a few pieces that are thrown a little.
    let mut burst = false;
    for _ in 0..6 {
        if s.fragments.iter().all(|f| f.id != section) {
            break;
        }
        fire_at(&mut s, house, at, Weapon::Standard);
        for _ in 0..90 {
            s.events.clear();
            idle(&mut s);
            burst |= s.events.iter().any(|e| e.kind == SimEventType::Explosion);
        }
    }
    assert!(burst, "the shells burst");
    assert!(
        s.fragments.iter().all(|f| f.id != section),
        "the section broke up: {parts:?} at {at:?} among {:?}",
        loose_bodies(&s, house)
    );
    let pieces: Vec<_> = loose_bodies(&s, house)
        .into_iter()
        .filter(|(_, p, _, _)| p.iter().all(|part| parts.contains(part)))
        .collect();
    let left: usize = pieces.iter().map(|(_, p, _, _)| p.len()).sum();
    assert!(left < parts.len(), "the struck part shattered");
    if parts.len() > 2 {
        assert!(pieces.len() >= 2, "into pieces: {pieces:?}");
        assert!(
            pieces.iter().all(|(_, p, _, _)| p.len() < parts.len()),
            "smaller ones"
        );
    }
    // Shell after shell at what is left of it: each breaks it down further, until
    // nothing of it is left.
    let mut remaining = left;
    for _ in 0..parts.len() * 3 {
        let Some((_, _, at, _)) = loose_bodies(&s, house)
            .into_iter()
            .filter(|(_, p, _, _)| p.iter().all(|part| parts.contains(part)))
            .max_by_key(|(id, p, _, _)| (p.len(), std::cmp::Reverse(*id)))
        else {
            break;
        };
        fire_at(&mut s, house, at, Weapon::Standard);
        run(&mut s, 1.5);
        let now: usize = loose_bodies(&s, house)
            .iter()
            .filter(|(_, p, _, _)| p.iter().all(|part| parts.contains(part)))
            .map(|(_, p, _, _)| p.len())
            .sum();
        assert!(now <= remaining, "nothing grows back: {remaining} -> {now}");
        remaining = now;
    }
    assert_eq!(remaining, 0, "the section was shot to splinters");
    assert_eq!(s.rng.state, rng, "no random draw");
}

#[test]
fn an_armour_piercing_round_breaks_a_fallen_section_and_flies_on() {
    let (mut s, house) = lone_cottage();
    let section = fallen_roof(&mut s, house);
    let (_, parts, at, _) = loose_bodies(&s, house)
        .into_iter()
        .find(|body| body.0 == section)
        .unwrap();
    fire_at(&mut s, house, at, Weapon::Piercing);
    let mut explosions = 0;
    for _ in 0..90 {
        s.events.clear();
        idle(&mut s);
        explosions += s
            .events
            .iter()
            .filter(|e| e.kind == SimEventType::Explosion)
            .count();
    }
    assert_eq!(explosions, 0, "no burst");
    assert!(s.fragments.iter().all(|f| f.id != section), "it broke");
    let left: usize = loose_bodies(&s, house)
        .iter()
        .filter(|(_, p, _, _)| p.iter().all(|part| parts.contains(part)))
        .map(|(_, p, _, _)| p.len())
        .sum();
    assert!(
        left < parts.len(),
        "it broke where it was struck: {left} of {}",
        parts.len()
    );
}

#[test]
fn a_levelled_cottage_leaves_no_loose_debris_after_twenty_seconds() {
    use sloppy_core::sim::SimulationSetup;
    use sloppy_core::sim::level_rules::single_player_rules;
    use sloppy_core::sim::stress_test_level::stress_test_level;
    let village = lone_cottage();
    let mut stress = Simulation::new(
        731.0,
        single_player_rules(stress_test_level()).merged(SimulationSetup::default()),
    );
    stress.start();
    let human = stress.human_index().expect("a human tank");
    let c = stress
        .covers
        .iter()
        .find(|cover| cover.structure.is_some())
        .expect("a Stress Grid cottage")
        .clone();
    clear_arena(&mut stress, &[human]);
    place_tank(&mut stress, 0, -40.0, -40.0, None);
    stress.tanks[0].protection = 999.0;
    let cottage = stress.add_cover(&CoverDef::house(c.x, c.z, c.w, c.d, c.h, c.color));
    for (mut s, house) in [village, (stress, cottage)] {
        let baseline = s.fragments.len();
        // Levelled from within by a heavy blast at each wall, then shot at as it lies.
        s.damage_every_house_part(house, 1000.0, None);
        run(&mut s, 3.0);
        let mut most = 0;
        for k in 0..3 {
            if let Some((_, _, at, _)) = loose_bodies(&s, house).get(k).copied() {
                fire_at(&mut s, house, at, Weapon::Standard);
            }
            run(&mut s, 1.0);
            most = most.max(s.fragments.len());
        }
        assert!(most > baseline, "pieces lay there");
        run(&mut s, 20.0 - 6.0);
        assert!(
            loose_bodies(&s, house).is_empty(),
            "every loose piece sank away"
        );
        assert_eq!(s.fragments.len(), baseline, "and nothing else is left");
    }
}

#[test]
fn shells_through_a_gable_window_break_the_gables_not_the_roof() {
    let (mut s, house) = lone_cottage();
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    let c = s.covers[house].clone();
    // The capture's window shots: four shells at the +x gable's window at 1 m, which
    // go on through the gaps they open to the far gable.
    for _ in 0..4 {
        let z = c.z + 1.89;
        fire(
            &mut s,
            Vec2::new(c.x + c.w / 2.0 + 26.0, z),
            Vec2::new(c.x + c.w / 2.0, z),
            Weapon::Standard,
        );
        run(&mut s, 2.5);
    }
    let gone = gone(&s, house);
    assert!(
        gone.iter()
            .any(|part| layout.parts[part].kind == PartKind::Window),
        "the window went"
    );
    let roof: Vec<usize> = gone
        .iter()
        .filter(|&part| layout.parts[part].kind.roof())
        .collect();
    assert!(
        roof.is_empty(),
        "the roof stands on its eave walls: {roof:?}"
    );
}

#[test]
fn a_fallen_piece_left_on_its_edge_is_laid_down() {
    use rapier3d::prelude::{Pose, Rotation, Vector};
    let (mut s, house) = lone_cottage();
    fallen_roof(&mut s, house);
    let c = s.covers[house].clone();
    // A single sheet, stood on its edge in the open beside the cottage.
    let (id, body) = s
        .fragments
        .iter()
        .find(|f| {
            f.house_id == Some(c.id)
                && f.house_parts.is_some_and(|parts| {
                    parts.len() <= 2
                        && parts
                            .iter()
                            .all(|part| kind(&s, house, part) == PartKind::Sheet)
                })
        })
        .map(|f| (f.id, f.body))
        .expect("a fallen sheet");
    let height = |s: &Simulation| {
        let (low, high) = s.world.bodies[body].colliders().iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(low, high), &h| {
                // From the body's pose now, not the colliders' as of the last step.
                let collider = &s.world.colliders[h];
                let pose = s.world.bodies[body].position()
                    * collider.position_wrt_parent().copied().unwrap_or_default();
                let aabb = collider.shape().compute_aabb(&pose);
                (low.min(aabb.mins.y), high.max(aabb.maxs.y))
            },
        );
        (low, high - low)
    };
    // Stood on its edge in the open 12 m off the cottage, held from toppling (as loose
    // boards round it would hold it), as a slope comes to rest on its eave.
    let pitch = s.covers[house]
        .structure
        .as_ref()
        .unwrap()
        .layout
        .roof
        .pitch
        .atan() as f32;
    let place = |s: &mut Simulation, angle: f32| {
        let turn = Rotation::from_axis_angle(Vector::X, angle);
        let rigid = &mut s.world.bodies[body];
        rigid.set_position(Pose::from_parts(Vector::ZERO, turn), true);
        let middle = rigid.center_of_mass();
        let target = Vector::new(c.x as f32, 3.0, (c.z + 12.0) as f32);
        rigid.set_position(Pose::from_parts(target - middle, turn), true);
    };
    let half = std::f32::consts::FRAC_PI_2;
    let upright = [half - pitch, -half - pitch, half + pitch, -half + pitch]
        .into_iter()
        .max_by(|&a, &b| {
            place(&mut s, a);
            let ha = height(&s).1;
            place(&mut s, b);
            let hb = height(&s).1;
            ha.total_cmp(&hb)
        })
        .unwrap();
    place(&mut s, upright);
    let (low, tall) = height(&s);
    assert!(tall > 1.0, "stood on its edge: {tall}");
    let rigid = &mut s.world.bodies[body];
    let at = rigid.translation();
    rigid.set_translation(at - Vector::new(0.0, low, 0.0), false);
    rigid.set_linvel(Vector::ZERO, false);
    rigid.set_angvel(Vector::ZERO, false);
    rigid.lock_rotations(true, true);
    run(&mut s, 3.0);
    assert!(s.fragments.iter().any(|f| f.id == id), "it lies on");
    // Turned down to within about 22 degrees of level (held, it lies tilted there).
    let (_, flat) = height(&s);
    assert!(
        flat < tall * 0.45,
        "laid down: {flat} m tall, {tall} on edge"
    );
}

#[test]
fn a_blown_out_cottage_falls_rather_than_flying_up() {
    // The hip-roofed cottage the capture levels: six shells across its +x wall, then
    // heavy bursts along that wall at 1 and 2.6 m and, two ticks later, the far one.
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().unwrap();
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -10.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let house = s.add_cover(&CoverDef::house(-45.0, 13.0, 5.0, 6.0, 4.3, 0xb87b4c));
    s.nav.rebuild(&s.covers, None);
    let c = s.covers[house].clone();
    assert_eq!(
        c.structure.as_ref().unwrap().layout.roof.form,
        RoofForm::Hip
    );
    for k in 0..6 {
        let z = c.z + [-2.6, -0.52, 1.56][k % 3];
        fire(
            &mut s,
            Vec2::new(c.x + c.w / 2.0 + 9.0, z),
            Vec2::new(c.x + c.w / 2.0, z),
            Weapon::Standard,
        );
        run(&mut s, 1.5);
    }
    let me = (s.tanks[0].id, s.tanks[0].team);
    let mut first: std::collections::HashMap<u32, f64> = Default::default();
    let mut worst = (0.0f64, 0u32);
    let mut watch = |s: &Simulation| {
        for f in s.fragments.iter().filter(|f| f.house_parts.is_some()) {
            let y = f64::from(s.world.bodies[f.body].center_of_mass().y);
            let start = *first.entry(f.id).or_insert(y);
            if y - start > worst.0 {
                worst = (y - start, f.id);
            }
        }
    };
    for face in [1.0, -1.0] {
        for i in 0..5 {
            for y in [1.0, 2.6] {
                let at = Point3::new(c.x + face * c.w / 2.0, y, c.z - 2.6 + 1.3 * f64::from(i));
                s.damage_cover(house, 150.0, me.0, me.1, None, Some(at));
            }
        }
        for _ in 0..2 {
            idle(&mut s);
            watch(&s);
        }
    }
    for _ in 0..120 {
        idle(&mut s);
        watch(&s);
    }
    assert!(!s.covers[house].alive, "it came down");
    // A piece is thrown up at most about a metre; nothing is flung over the roofs.
    assert!(worst.0 < 1.5, "piece {} rose {:.2} m", worst.1, worst.0);
}

#[test]
fn a_levelled_two_storey_house_lies_in_pieces_not_one_propped_lump() {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().unwrap();
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -10.0, -40.0, None);
    let house = s.add_cover(&CoverDef::house(45.0, -39.0, 5.0, 6.0, 6.8, 0xb87b4c));
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    assert_eq!(layout.design.storeys(), 2);
    // Its ground floor's walls blown out round it: all above them comes down.
    let c = s.covers[house].clone();
    let me = (s.tanks[0].id, s.tanks[0].team);
    for (dx, dz, along_x) in [
        (1.0, 0.0, false),
        (-1.0, 0.0, false),
        (0.0, 1.0, true),
        (0.0, -1.0, true),
    ] {
        for i in 0..5 {
            let u = -2.0 + f64::from(i);
            let (x, z) = if along_x {
                (c.x + u, c.z + dz * c.d / 2.0)
            } else {
                (c.x + dx * c.w / 2.0, c.z + u)
            };
            for y in [1.0, 2.6] {
                s.damage_cover(house, 150.0, me.0, me.1, None, Some(Point3::new(x, y, z)));
            }
        }
    }
    run(&mut s, 5.0);
    for f in s.fragments.iter().filter(|f| f.house_parts.is_some()) {
        let parts = f.house_parts.unwrap();
        let floors = parts
            .iter()
            .any(|p| layout.parts[p].kind == PartKind::Floor);
        let walls = parts.iter().any(|p| layout.parts[p].wall.is_some());
        assert!(
            !(floors && walls),
            "a wall and a floor lie fused: {parts:?}"
        );
        let body = &s.world.bodies[f.body];
        assert!(
            f64::from(body.center_of_mass().y) < 2.0,
            "a piece stands up {:?}",
            body.center_of_mass()
        );
    }
}

#[test]
fn a_cottage_garden_bears_its_fallen_parts_but_not_tanks_or_shells() {
    use rapier3d::prelude::{QueryFilter, Vector};
    use sloppy_core::sim::data::group;
    use sloppy_core::sim::house_layout::HouseDesign;
    use sloppy_core::sim::physics::interaction_groups;
    // A cottage that keeps a garden.
    let x = (0..60)
        .map(|k| 20.0 + f64::from(k) * 0.5)
        .find(|&x| {
            HouseDesign::of(x, 0.0, 5.0, 7.0, 4.9)
                .garden_bed()
                .is_some()
        })
        .expect("a cottage with a garden");
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().unwrap();
    clear_arena(&mut s, &[human]);
    let house = s.add_cover(&CoverDef::house(x, 0.0, 5.0, 7.0, 4.9, 0xb87b4c));
    s.world.step();
    let design = s.covers[house].structure.as_ref().unwrap().layout.design;
    let (bed, _) = design.garden_bed().unwrap();
    let (a, b) = bed.bounds(&design, 0.3);
    let middle = design.to_world((a + b) / 2.0);
    let at = Vector::new((x + middle.x) as f32, 0.3, middle.z as f32);
    let meets = |s: &Simulation, groups: u32| {
        s.world
            .intersect_point(
                at,
                QueryFilter::default().groups(interaction_groups(groups)),
            )
            .next()
            .is_some()
    };
    assert!(
        meets(&s, group::HOUSE_SHARD),
        "a fallen part rests on the bed"
    );
    assert!(!meets(&s, group::TANK), "a tank drives over it");
    assert!(!meets(&s, group::COVER_QUERY), "a shell flies over it");
}
