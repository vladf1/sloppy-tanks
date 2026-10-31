//! A round's views, and the looks its preparation compiles, built a piece at a time
//! ([`Presentation::begin_build`], then [`Presentation::build_step`] until it says
//! done), so a page builds the arena behind Battle Setup between frames instead of
//! in one task of a few hundred milliseconds. [`Presentation::reset`] and
//! [`Presentation::begin_prepare`] take the same steps at once, in the same order, so
//! an arena comes out the same either way.

use sloppy_core::sim::render_state::RenderCover;
use sloppy_core::sim::timber_layout::{TimberWall, timber_parts};
use sloppy_core::sim::{CoverKind, PickupKind, RenderState, Team, VehicleKind};

use super::{
    CoverPlan, DEBRIS_SHAPES, HousePreparation, Lifetime, PendingScenery, Preparation,
    Presentation, SAMPLE_WRECK, SceneryModel, cover_damage_stage, model_catalog, timber_part_model,
};

/// What building the round's views has still to do, in order.
#[derive(Default)]
pub(super) enum BuildStage {
    /// Built, and the arena's pipelines asked for.
    #[default]
    Done,
    /// The theme's scenery model, unless a round of the theme built it before: the
    /// costliest piece, which only the core models can split further.
    SceneryModel,
    /// That model's static parts batched for upload.
    SceneryBatches(Box<(&'static str, SceneryModel)>),
    /// The scenery uploaded and kept for the theme.
    SceneryUpload(Box<PendingScenery>),
    /// The scenery's pipelines queued to compile while the rest builds.
    SceneryPipelines,
    /// The theme's scenery, water and light shown, and the effects cleared.
    Theme,
    /// The static covers of a look of their own, one model a step, then their
    /// combined model.
    CombinedCovers(Box<CoverPlan>),
    /// The other covers, one a step from this index of the plan's.
    OwnCovers(Vec<usize>, usize),
    /// The debris looks the covers can break into queued, and the covers' pipelines.
    CoverDebris,
    /// The round's tanks, one a step from this index.
    Tanks(usize),
    /// The supplies, and the camera placed on the viewer.
    Pickups,
    /// The views are built (where [`Presentation::reset`] stops). Then come the looks
    /// a round can show that it does not yet: the library's vehicles, bars, mines,
    /// the sample wreck, supplies and debris pieces.
    Library,
    /// One model of each damaged cover look and timber beam this round's covers can
    /// break into.
    CoverSamples,
    /// The arena's whole look is registered: the rest of its pipelines are queued to
    /// compile.
    Pipelines,
}

impl Presentation {
    /// Drop the old round's views and begin building `state`'s round
    /// ([`Self::build_step`]).
    pub fn begin_build(&mut self, state: &RenderState) {
        self.rig.snap_seat = true;
        if let Some(viewer) = state.viewer() {
            self.rig.first_person.yaw = viewer.aim;
        }
        // The old round's views go before its renderer resources: cover models hold
        // their source meshes, which `reset_round` can then free at once, before this
        // round and a new theme upload into the mesh pages they leave.
        self.tanks.clear();
        self.covers.clear();
        self.cover_models.clear();
        self.cover_looks.clear();
        self.fragments.clear();
        self.shared_debris.clear();
        self.house_pieces.clear();
        // Its models are the round's, which `reset_round` frees.
        self.house_prep = HousePreparation::default();
        // Round models go with the round.
        self.settled.clear();
        self.pickups.clear();
        self.supply_pieces.clear();
        self.mines.clear();
        self.branches.clear();
        self.sample_models.clear();
        self.renderer.reset_round();
        self.show_floors(state);
        self.build = BuildStage::SceneryModel;
    }

    /// Build the next piece of the round [`Self::begin_build`] began, each a few
    /// milliseconds but the scenery's; true once it is built and its preparation has
    /// begun ([`Self::prepare_step`] may run). `state` is the one the build began with.
    pub fn build_step(&mut self, state: &RenderState) -> bool {
        self.build = match std::mem::take(&mut self.build) {
            BuildStage::Done => BuildStage::Done,
            BuildStage::SceneryModel => match self.scenery_model(&state.map_theme) {
                Some(model) => BuildStage::SceneryBatches(Box::new(model)),
                None => BuildStage::Theme,
            },
            BuildStage::SceneryBatches(model) => {
                let (key, model) = *model;
                BuildStage::SceneryUpload(Box::new(self.prepare_scenery(key, model)))
            }
            BuildStage::SceneryUpload(pending) => {
                self.upload_scenery(*pending);
                BuildStage::SceneryPipelines
            }
            BuildStage::SceneryPipelines => {
                self.renderer.request_pipelines();
                BuildStage::Theme
            }
            BuildStage::Theme => {
                self.show_theme(state);
                self.rig.settle();
                self.hit_confirm_until = 0.0;
                self.player_was_alive = false;
                self.effects.reset(&mut self.renderer);
                BuildStage::CombinedCovers(Box::new(CoverPlan::of(&state.covers)))
            }
            BuildStage::CombinedCovers(mut plan) => {
                if self.build_combined_cover(&mut plan, &state.covers) {
                    BuildStage::CombinedCovers(plan)
                } else {
                    let own = std::mem::take(&mut plan.own);
                    self.add_combined_covers(*plan, &state.covers);
                    BuildStage::OwnCovers(own, 0)
                }
            }
            BuildStage::OwnCovers(own, next) => match own.get(next) {
                Some(&index) => {
                    self.add_cover(&state.covers[index]);
                    BuildStage::OwnCovers(own, next + 1)
                }
                None => BuildStage::CoverDebris,
            },
            BuildStage::CoverDebris => {
                self.queue_debris(&state.covers);
                self.renderer.request_pipelines();
                BuildStage::Tanks(0)
            }
            BuildStage::Tanks(index) => match state.tanks.get(index) {
                Some(tank) => {
                    self.add_tank(tank);
                    BuildStage::Tanks(index + 1)
                }
                None => BuildStage::Pickups,
            },
            BuildStage::Pickups => {
                for pickup in &state.pickups {
                    self.add_pickup(pickup.id, pickup.kind, pickup.x, pickup.z);
                }
                self.update_camera(state, 1.0, 0.0, false);
                self.renderer.request_pipelines();
                BuildStage::Library
            }
            BuildStage::Library => {
                self.preparation = Preparation::default();
                self.add_library();
                BuildStage::CoverSamples
            }
            BuildStage::CoverSamples => {
                self.add_cover_samples(&state.covers);
                BuildStage::Pipelines
            }
            BuildStage::Pipelines => {
                self.renderer.request_pipelines();
                BuildStage::Done
            }
        };
        !self.building()
    }

    /// A build [`Self::begin_build`] began is still under way.
    pub fn building(&self) -> bool {
        !matches!(self.build, BuildStage::Done)
    }

    /// Rebuild every entity view for a new round (`reset()`) at once.
    pub fn reset(&mut self, state: &RenderState) {
        self.begin_build(state);
        while !matches!(self.build, BuildStage::Library) {
            self.build_step(state);
        }
        self.build = BuildStage::Done;
    }

    /// Register one model per first-use look this round can show (the wrecks' looks,
    /// damaged cargo and timber stages, timber beams, mines, pickup glows, debris
    /// pieces), so `prepare_step` compiles their pipelines before combat instead of
    /// mid-fight, at once after [`Self::reset`]. Of the wrecks only the sample is built
    /// now (`wreck_library`); the page builds the rest once the arena is ready
    /// ([`Self::prebuild_step`]).
    pub fn begin_prepare(&mut self, state: &RenderState) {
        self.build = BuildStage::Library;
        while !self.build_step(state) {}
    }

    fn add_library(&mut self) {
        for kind in VehicleKind::ALL {
            for team in [Team::Blue, Team::Red] {
                self.library.tank(&mut self.renderer, kind, team);
                self.library.bar(&mut self.renderer, team);
                self.library.mine(&mut self.renderer, team);
            }
        }
        let (kind, team, part) = SAMPLE_WRECK;
        self.library.wreck(&mut self.renderer, kind, team, part);
        for kind in PickupKind::ALL {
            self.library.pickup(&mut self.renderer, kind);
        }
        for shape in DEBRIS_SHAPES {
            self.library.debris(&mut self.renderer, shape);
        }
        self.library.pickup_pad(&mut self.renderer);
    }

    fn add_cover_samples(&mut self, covers: &[RenderCover]) {
        let mut seen = std::collections::HashSet::new();
        for cover in covers {
            for health in [0.6, 0.3, 0.1] {
                let hp = cover.max_hp * health;
                let stage = cover_damage_stage(cover.kind, hp, cover.max_hp);
                if stage > 0 && seen.insert(format!("{:?}/{stage}", cover.kind)) {
                    let damaged = RenderCover {
                        hp,
                        ..cover.clone()
                    };
                    let model = model_catalog::cover_model(&damaged, stage);
                    self.sample_models
                        .push(self.renderer.add_model(&model.root, Lifetime::Round));
                }
            }
            if cover.kind == CoverKind::Timber && seen.insert("timber-parts".into()) {
                let wall = TimberWall {
                    x: cover.x,
                    z: cover.z,
                    w: cover.w,
                    h: cover.h,
                    d: cover.d,
                    color: cover.color,
                    hits: &cover.timber_hits,
                    join: cover.timber_join,
                };
                for part in timber_parts(&wall, 0) {
                    let node = timber_part_model(&part);
                    self.sample_models
                        .push(self.renderer.add_model(&node, Lifetime::Round));
                }
            }
            // Falling crowns and boughs reuse the live tree's materials, whose
            // faded variants every movable model registers up front.
        }
    }
}
