//! Watchtower base dimensions, shared by the intact model, surviving foundations and the
//! collision footprint.

pub struct TowerBase {
    pub offset: f64,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub rubble_height: f64,
    pub post_z: f64,
}

pub const TOWER_BASE: TowerBase = TowerBase {
    offset: 2.55,
    width: 1.3,
    depth: 3.0,
    height: 0.85,
    rubble_height: 1.25,
    post_z: 1.05,
};
