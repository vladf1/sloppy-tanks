//! Pins every value the model builders produce: the three themes' scenery, the
//! vehicles and their wrecks, and each map's covers at every damage stage, batched
//! as presentation draws them. A hash covers each node's name, transform and
//! visibility, and each drawable's material, flags, instances and mesh (positions,
//! normals, UVs, colours, indices and custom attributes, bit for bit), so a
//! construction speed-up that moves one vertex, reorders one triangle or drops one
//! node fails here. Update a pin only for an intentional change to a model.

use glam::{DMat4, DQuat, DVec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::models::{
    CoverShape, MapTheme, TreeDetail, WreckPart, batch, build_scenery, cover_damage_stage,
    cover_model, tank_model_variant, wreck_model,
};
use sloppy_core::scene::Node;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{Team, VehicleKind};
use sloppy_core::sim::{RenderState, Simulation};

/// FNV-1a over 32-bit words: order-sensitive and cheap enough in a debug build
/// for the village's millions of vertex values.
struct Pin(u64);

impl Pin {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn word(&mut self, word: u32) {
        self.0 = (self.0 ^ u64::from(word)).wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn words(&mut self, words: impl IntoIterator<Item = u32>) {
        let mut count = 0u32;
        for word in words {
            self.word(word);
            count += 1;
        }
        self.word(count);
    }

    fn floats<'a>(&mut self, values: impl IntoIterator<Item = &'a f32>) {
        self.words(values.into_iter().map(|v| v.to_bits()));
    }

    fn double(&mut self, value: f64) {
        let bits = value.to_bits();
        self.word(bits as u32);
        self.word((bits >> 32) as u32);
    }

    fn vector(&mut self, v: DVec3) {
        for c in v.to_array() {
            self.double(c);
        }
    }

    fn quaternion(&mut self, q: DQuat) {
        for c in q.to_array() {
            self.double(c);
        }
    }

    fn matrix(&mut self, m: &DMat4) {
        for c in m.to_cols_array() {
            self.double(c);
        }
    }

    fn text(&mut self, text: &str) {
        self.words(text.bytes().map(u32::from));
    }

    fn mesh(&mut self, mesh: &Mesh) {
        self.floats(mesh.positions.as_flattened());
        self.floats(mesh.normals.as_flattened());
        self.floats(mesh.uvs.as_flattened());
        self.floats(mesh.colors.as_flattened());
        match &mesh.indices {
            Some(indices) => self.words(indices.iter().copied()),
            None => self.word(u32::MAX),
        }
        self.word(mesh.attributes.len() as u32);
        for attribute in &mesh.attributes {
            self.text(attribute.name);
            self.word(u32::from(attribute.item_size));
            self.word(u32::from(attribute.per_instance));
            self.floats(&attribute.data);
        }
    }

    fn node(&mut self, node: &Node) {
        self.text(&node.name);
        self.vector(node.position);
        self.quaternion(node.rotation);
        self.vector(node.scale);
        self.word(u32::from(node.visible));
        match &node.drawable {
            Some(drawable) => {
                self.word(1);
                self.mesh(&drawable.mesh);
                // Materials are small; their debug form prints every field, floats
                // exactly.
                self.text(&format!("{:?}", drawable.material));
                self.word(u32::from(drawable.cast_shadow));
                self.word(u32::from(drawable.receive_shadow));
                self.word(drawable.render_order as u32);
                self.word(u32::from(drawable.frustum_culled));
                match &drawable.instances {
                    Some(instances) => {
                        self.word(instances.len() as u32);
                        for instance in instances {
                            self.matrix(&instance.matrix);
                            match instance.color {
                                Some(color) => self.floats(&color),
                                None => self.word(u32::MAX),
                            }
                        }
                    }
                    None => self.word(u32::MAX),
                }
            }
            None => self.word(0),
        }
        self.word(node.children.len() as u32);
        for child in &node.children {
            self.node(child);
        }
    }
}

fn scenery_pin(theme: MapTheme) -> u64 {
    let mut pin = Pin::new();
    pin.node(build_scenery(theme).root());
    pin.0
}

fn assert_pin(what: &str, actual: u64, expected: u64) {
    assert_eq!(
        actual, expected,
        "{what} changed: pin {actual:#018x}, expected {expected:#018x}"
    );
}

#[test]
fn village_scenery_is_pinned() {
    assert_pin(
        "village scenery",
        scenery_pin(MapTheme::Village),
        0x0c52_66a4_1d6d_b740,
    );
}

#[test]
fn harbor_scenery_is_pinned() {
    assert_pin(
        "harbor scenery",
        scenery_pin(MapTheme::Harbor),
        0x2579_221d_91e9_5548,
    );
}

#[test]
fn quarry_scenery_is_pinned() {
    assert_pin(
        "quarry scenery",
        scenery_pin(MapTheme::Quarry),
        0x6f4e_b490_6e1a_2a5c,
    );
}

#[test]
fn vehicles_and_wrecks_are_pinned() {
    let mut pin = Pin::new();
    for kind in VehicleKind::ALL {
        for team in [Team::Blue, Team::Red] {
            for open_turret_ring in [false, true] {
                pin.node(&tank_model_variant(kind, team, open_turret_ring));
            }
            for part in [
                WreckPart::Intact,
                WreckPart::Hull,
                WreckPart::Turret,
                WreckPart::TurretBarrel,
                WreckPart::Barrel,
            ] {
                pin.node(&wreck_model(kind, team, part));
            }
        }
    }
    assert_pin("vehicles and wrecks", pin.0, 0x5eb7_8e3b_29af_69ca);
}

/// Every cover of a map at each damage stage it can show, batched like
/// presentation's `model_catalog::cover_model`.
fn covers_pin(map: MapId) -> u64 {
    let mut simulation = Simulation::with_seed(417.0);
    simulation.map_mode = map;
    simulation.reset(None);
    let mut view = RenderState::default();
    simulation.fill_render_state(&mut view, None);
    let mut pin = Pin::new();
    for cover in &view.covers {
        let shape = CoverShape::from(cover);
        let mut stages: Vec<u32> = [1.0, 0.9, 0.6, 0.3, 0.1]
            .map(|fraction| cover_damage_stage(cover.kind, cover.max_hp * fraction, cover.max_hp))
            .to_vec();
        stages.dedup();
        for stage in stages {
            let mut root = cover_model(&shape, TreeDetail::Full, stage);
            batch(&mut root);
            pin.word(stage);
            pin.node(&root);
        }
    }
    pin.0
}

#[test]
fn village_covers_are_pinned() {
    assert_pin(
        "village covers",
        covers_pin(MapId::Village),
        0xdf29_d15a_f4a0_ae39,
    );
}

#[test]
fn harbor_covers_are_pinned() {
    assert_pin(
        "harbor covers",
        covers_pin(MapId::Harbor),
        0xb197_b799_92b0_3f77,
    );
}

#[test]
fn quarry_covers_are_pinned() {
    assert_pin(
        "quarry covers",
        covers_pin(MapId::Quarry),
        0x5e0f_0aa7_0303_a172,
    );
}
