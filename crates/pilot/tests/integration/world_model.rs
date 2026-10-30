//! The pilot's model of a room's arena matches the host's: the same cover under the same
//! ids, the same pickups, and cover destroyed in the scene leaves the model's sight lines
//! and routes.

use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::types::{CoverKind, PlayerAssignment, Team, VehicleKind};
use sloppy_pilot::world_model::{Perspective, WorldModel, scene_map};

fn host_room(map: MapId) -> sloppy_core::sim::Simulation {
    let players = [PlayerAssignment {
        player_id: "p1".into(),
        name: "pilot".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    }];
    create_multiplayer_simulation(
        4242.0,
        &players,
        MultiplayerOptions {
            map_mode: Some(map),
            difficulty: Some(Difficulty::Normal),
            round: Some(3),
            humans_only: Some(false),
        },
    )
    .unwrap()
}

#[test]
fn the_model_has_the_hosts_cover_and_pickups_on_every_map() {
    for map in MapId::ALL {
        let host = host_room(map);
        let scene = host.render_state(None);
        assert_eq!(scene_map(&scene, None), map);
        let model = WorldModel::new(map, Difficulty::Normal, BotPersonality::Guard, 1.0);
        let ours: Vec<_> = model
            .simulation
            .covers
            .iter()
            .map(|cover| (cover.id, cover.kind, cover.x, cover.z, cover.w, cover.d))
            .collect();
        let theirs: Vec<_> = host
            .covers
            .iter()
            .map(|cover| (cover.id, cover.kind, cover.x, cover.z, cover.w, cover.d))
            .collect();
        assert_eq!(ours, theirs, "{map:?} cover");
        let pickups = |simulation: &sloppy_core::sim::Simulation| {
            simulation
                .pickups
                .iter()
                .map(|pickup| (pickup.id, pickup.kind, pickup.x, pickup.z))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pickups(&model.simulation),
            pickups(&host),
            "{map:?} pickups"
        );
        assert!(model.simulation.tanks.is_empty());
    }
}

#[test]
fn synced_tanks_stand_where_the_scene_draws_them() {
    let host = host_room(MapId::Village);
    let mut scene = host.render_state(None);
    let own = scene.tanks[0].id;
    scene.viewer_id = own;
    let mut model = WorldModel::new(MapId::Village, Difficulty::Hard, BotPersonality::Scout, 1.0);
    model.sync(&scene, Perspective { own, leader: None });
    assert_eq!(model.simulation.tanks.len(), scene.tanks.len());
    for drawn in &scene.tanks {
        let index = model.tank_index(drawn.id).unwrap();
        assert_eq!(model.simulation.tanks[index].alive, drawn.alive);
        if drawn.alive {
            let at = model.simulation.tank_planar(index);
            assert!((at.x - drawn.position.x).abs() < 1e-3);
            assert!((at.z - drawn.position.z).abs() < 1e-3);
        }
    }
    let index = model.tank_index(own).unwrap();
    let tank = &model.simulation.tanks[index];
    assert!(!tank.human, "the own tank plays as a bot");
    assert_eq!(tank.brain.personality, BotPersonality::Scout);
    // A tank that moves in the scene moves in the model, and its new life gets a new body.
    scene.tanks[0].position.x += 3.0;
    scene.tanks[0].life += 1;
    model.sync(&scene, Perspective { own, leader: None });
    let index = model.tank_index(own).unwrap();
    let at = model.simulation.tank_planar(index);
    assert!((at.x - scene.tanks[0].position.x).abs() < 1e-3);
    assert_eq!(model.simulation.tanks[index].life, scene.tanks[0].life);
    // A tank gone from the scene leaves the model with its body.
    let gone = scene.tanks.pop().unwrap();
    let bodies = model.simulation.world.bodies.len();
    model.sync(&scene, Perspective { own, leader: None });
    assert!(model.tank_index(gone.id).is_none());
    assert!(model.simulation.world.bodies.len() < bodies);
}

#[test]
fn fallen_cover_opens_sight_lines_and_routes_and_rebuilt_cover_closes_them() {
    let host = host_room(MapId::Village);
    let mut scene = host.render_state(None);
    let mut model = WorldModel::new(
        MapId::Village,
        Difficulty::Normal,
        BotPersonality::Guard,
        1.0,
    );
    // A destructible, non-tree cover big enough to block a sight line through its middle.
    let index = scene
        .covers
        .iter()
        .position(|cover| {
            cover.alive
                && cover.destructible
                && cover.kind != CoverKind::Tree
                && !cover.kind.movable()
                && cover.w.min(cover.d) > 1.0
        })
        .expect("the village has destructible cover");
    let cover = scene.covers[index].clone();
    let reach = cover.w.max(cover.d) / 2.0 + 2.0;
    let (a, b) = (
        Vec2::new(cover.x - reach, cover.z),
        Vec2::new(cover.x + reach, cover.z),
    );
    model.sync(&scene, Perspective::default());
    assert!(
        !model.simulation.visible(a, b),
        "standing cover blocks the view"
    );
    let middle = Vec2::new(cover.x, cover.z);
    assert!(model.simulation.nav.is_blocked(middle));
    scene.covers[index].alive = false;
    model.sync(&scene, Perspective::default());
    assert!(
        model.simulation.visible(a, b),
        "fallen cover no longer does"
    );
    assert!(!model.simulation.nav.is_blocked(middle));
    scene.covers[index].alive = true;
    model.sync(&scene, Perspective::default());
    assert!(
        !model.simulation.visible(a, b),
        "rebuilt cover blocks it again"
    );
    assert!(model.simulation.nav.is_blocked(middle));
}
