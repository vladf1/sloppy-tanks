//! Shell hits on scenery (the simulation half of the former `tests/cover-hit-effects.test.ts`):
//! a survivable hit emits one cover impact, and a fatal hit destroys the cover once, without a
//! duplicate impact, and leaves physical debris. The particle counts (leaves and splinters)
//! the TS test also checked belong to the presentation's particle effects and are not ported.

mod support;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::{CoverKind, Shot, SimEventType, Simulation};
use support::event_count;

struct Hit {
    alive: bool,
    hp: f64,
    impacts: usize,
    destroys: usize,
    fragments: usize,
    tree_parts: usize,
}

/// Fire one standard shell into the cover's near face.
fn shoot(s: &mut Simulation, cover: usize, damage: f64) -> Hit {
    s.shots.clear();
    s.events.clear();
    let c = s.covers[cover].clone();
    let id = s.allocate_id();
    let (owner, team) = (s.human().id, s.human_team);
    s.shots.push(Shot {
        id,
        x: c.x,
        z: c.z - c.d / 2.0 - 0.5,
        vx: 0.0,
        vz: 40.0,
        owner,
        team,
        damage,
        life: 1.0,
        ..Shot::default()
    });
    step_projectiles(s, 0.05, false);
    let cover = &s.covers[cover];
    Hit {
        alive: cover.alive,
        hp: cover.hp,
        impacts: s
            .events
            .iter()
            .filter(|e| e.kind == SimEventType::Impact && e.cover_kind == Some(cover.kind))
            .count(),
        destroys: event_count(s, SimEventType::Destroy),
        fragments: s.fragments.len(),
        tree_parts: s
            .fragments
            .iter()
            .filter(|piece| piece.tree_cover_id == Some(cover.id))
            .count(),
    }
}

#[test]
fn shell_hits_chip_trees_timber_and_cargo_and_a_fatal_hit_bursts_once_and_leaves_debris() {
    let mut s = Simulation::with_seed(123.0);
    assert_eq!(s.map_mode, MapId::Village);
    let covers = [
        s.covers
            .iter()
            .position(|c| c.kind == CoverKind::Tree)
            .unwrap(),
        s.covers
            .iter()
            .position(|c| c.kind == CoverKind::Timber)
            .unwrap(),
        s.add_cover(&CoverDef::new(
            CoverKind::Cargo,
            0.0,
            30.0,
            4.0,
            0.9,
            1.5,
            80.0,
            0xb47a49,
        )),
    ];
    s.world.step();
    for cover in covers {
        let kind = s.covers[cover].kind;
        let hp = s.covers[cover].hp;
        let hit = shoot(&mut s, cover, 20.0);
        assert!(hit.alive, "{kind:?}");
        assert_eq!(hit.hp, hp - 20.0, "{kind:?}");
        assert_eq!(hit.impacts, 1, "{kind:?}");
        assert_eq!(hit.destroys, 0, "{kind:?}");
        let destroyed = shoot(&mut s, cover, 999.0);
        assert!(!destroyed.alive, "{kind:?}");
        assert_eq!(destroyed.destroys, 1, "{kind:?}");
        assert_eq!(
            destroyed.impacts, 0,
            "{kind:?}: fatal impacts must not double the burst"
        );
        assert!(
            destroyed.fragments > hit.fragments,
            "{kind:?}: destruction creates debris"
        );
        if kind == CoverKind::Tree {
            assert!(destroyed.tree_parts > 0);
        }
    }
}
