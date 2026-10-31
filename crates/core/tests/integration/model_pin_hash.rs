//! The pins of `model_pins`: hashes of everything the model builders produce, and
//! the builds they cover. Shared with `examples/model_build_wasm.rs`, which pins
//! the same builds in Wasm.

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
pub struct Pin(u64);

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

/// A theme's whole scenery.
pub fn scenery_pin(theme: MapTheme) -> u64 {
    let mut pin = Pin::new();
    pin.node(build_scenery(theme).root());
    pin.0
}

/// Every vehicle model, with and without its turret ring cut open, and every
/// wreck piece, for both teams.
pub fn vehicles_pin() -> u64 {
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
    pin.0
}

/// Every cover of a map at each damage stage it can show, batched like
/// presentation's `model_catalog::cover_model`.
pub fn covers_pin(map: MapId) -> u64 {
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
