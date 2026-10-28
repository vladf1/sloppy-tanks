//! The native precompiler: wgpu compiles every pipeline when it is created, so
//! nothing ever waits in the background and [`PipelineSpec::create`] always builds
//! its own.

use std::collections::HashSet;
use std::hash::Hash;

use super::PipelineSpec;
use crate::shader::StageSources;

/// Nothing is compiled ahead of wgpu natively.
pub type Compiled = ();

/// A fixed set of pipelines, ready at once.
pub struct Background;

impl Background {
    pub fn start(_device: &wgpu::Device, _jobs: &[(&PipelineSpec, &StageSources)]) -> Self {
        Self
    }

    pub fn done(&self) -> bool {
        true
    }

    pub fn compiled(&self, _index: usize) -> Option<Compiled> {
        None
    }
}

/// Records the keys asked for, each finished as soon as it is started.
pub struct Precompiler<K> {
    started: std::cell::RefCell<HashSet<K>>,
}

impl<K: Copy + Eq + Hash + 'static> Precompiler<K> {
    pub fn new(_device: &wgpu::Device) -> Self {
        Self {
            started: Default::default(),
        }
    }

    pub fn start(&self, key: K, _spec: &PipelineSpec, _sources: &StageSources) {
        self.started.borrow_mut().insert(key);
    }

    pub fn queued(&self, key: &K) -> bool {
        self.started.borrow().contains(key)
    }

    pub fn finished(&self, key: &K) -> bool {
        self.queued(key)
    }

    pub fn take(&self, _key: &K) -> Option<Compiled> {
        None
    }

    pub fn compiling(&self) -> u32 {
        0
    }
}
