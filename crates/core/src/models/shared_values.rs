//! Where the model builders keep their shared meshes and materials, so a model read
//! back from a page's geometry cache (`scene_codec`) shares them with what the
//! process builds before and after it, exactly as a built one does.
//!
//! Builders share values through process-wide stores ([`Cache`](super::model_primitives),
//! [`SharedOnce`] and the batching's vertex materials): a cover built after the
//! scenery takes the scenery's `material(..)` and `box_part(..)` `Arc`s, and the
//! renderer uploads, batches and interns by `Arc` identity. A cache file records, for
//! each mesh and material it holds, the store and key it was shared under; reading
//! takes the store's value when the process holds one already ([`adopt`]), and
//! otherwise leaves the read value for the store to hand out when a builder first
//! asks for that key ([`take`]), instead of building it.
//!
//! A store is named in its source (`"model_primitives::MATERIALS"`) and a key by the
//! hash of its value, so a record means the same in every build of the same sources
//! and compiler: the page keys its files by the content version, which covers both.
//! A store keyed by addresses (which mean nothing in another run) is private and
//! never listed.

use std::any::Any;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

/// A process-wide store of shared values.
pub(crate) trait SharedStore: Sync {
    fn name(&self) -> &'static str;
    /// Each held value's key hash and `Arc` address.
    fn each(&self, visit: &mut dyn FnMut(u64, usize));
    /// The value under a key hash, as a boxed `Arc<V>`.
    fn find(&self, key: u64) -> Option<Box<dyn Any>>;
}

static STORES: Mutex<Vec<&'static dyn SharedStore>> = Mutex::new(Vec::new());

/// Values read back for a store's key no builder has asked for yet, by store name.
type Waiting = HashMap<String, HashMap<u64, Box<dyn Any + Send>>>;
static WAITING: Mutex<Option<Waiting>> = Mutex::new(None);

/// The store of the batching's shared vertex-color materials (`batching.rs`), which
/// is a list rather than a [`SharedStore`].
pub(crate) const VERTEX_MATERIALS: &str = "batching::VERTEX_MATERIALS";

/// Record a store as it first fills. Store names are unique: a record names one.
pub(crate) fn register(store: &'static dyn SharedStore) {
    let mut stores = STORES.lock().unwrap_or_else(|poison| poison.into_inner());
    assert!(
        stores.iter().all(|held| held.name() != store.name()),
        "two shared stores are named {}",
        store.name()
    );
    stores.push(store);
}

/// The hash a store's key is recorded under. `DefaultHasher::new` has fixed keys, so
/// the hash is the same in every run of one compiler.
pub(crate) fn key_hash<K: Hash>(key: &K) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// A value read back for `store`'s `key`, to hand out instead of building it.
pub(crate) fn take<V: Send + Sync + 'static>(store: &str, key: u64) -> Option<Arc<V>> {
    let mut waiting = WAITING.lock().unwrap_or_else(|poison| poison.into_inner());
    let value = waiting.as_mut()?.get_mut(store)?.remove(&key)?;
    value.downcast::<Arc<V>>().ok().map(|value| *value)
}

/// Drop what was read back for `store` and never asked for.
pub(crate) fn forget(store: &str) {
    if let Some(waiting) = WAITING
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_mut()
    {
        waiting.remove(store);
    }
}

/// Every listed shared value's place, by `Arc` address: its store and key hash.
pub fn places() -> HashMap<usize, (&'static str, u64)> {
    let mut places = HashMap::new();
    for &store in STORES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
    {
        let name = store.name();
        store.each(&mut |key, address| {
            places.entry(address).or_insert((name, key));
        });
    }
    super::batching::each_vertex_material(&mut |address| {
        places.entry(address).or_insert((VERTEX_MATERIALS, 0));
    });
    places
}

/// The value the process uses from now on for a read-back `value` that was shared
/// under `store`'s `key`: the one the store or an earlier file already holds, or else
/// `value`, kept for the store to hand out when its builder first asks.
pub fn adopt<V: Send + Sync + 'static>(store: &str, key: u64, value: Arc<V>) -> Arc<V> {
    if store == VERTEX_MATERIALS {
        return adopt_vertex_material(value);
    }
    let held = STORES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .find(|held| held.name() == store)
        .and_then(|held| held.find(key));
    if let Some(held) = held.and_then(|held| held.downcast::<Arc<V>>().ok()) {
        return *held;
    }
    let mut waiting = WAITING.lock().unwrap_or_else(|poison| poison.into_inner());
    let keys = waiting
        .get_or_insert_with(HashMap::new)
        .entry(store.to_owned())
        .or_default();
    // Two files may bring one key's value (the vehicles and the scenery share paint):
    // the first one read stands for both.
    if let Some(earlier) = keys
        .get(&key)
        .and_then(|earlier| earlier.downcast_ref::<Arc<V>>())
    {
        return earlier.clone();
    }
    keys.insert(key, Box::new(value.clone()));
    value
}

fn adopt_vertex_material<V: Send + Sync + 'static>(value: Arc<V>) -> Arc<V> {
    let any: Box<dyn Any> = Box::new(value.clone());
    let Ok(material) = any.downcast::<Arc<crate::scene::Material>>() else {
        return value;
    };
    let shared: Box<dyn Any> = Box::new(super::batching::adopt_vertex_material(*material));
    shared.downcast::<Arc<V>>().map_or(value, |shared| *shared)
}

/// A `OnceLock<Arc<T>>` the geometry cache can find and refill: one shared value.
pub(crate) struct SharedOnce<T> {
    name: &'static str,
    value: OnceLock<Arc<T>>,
}

impl<T: Send + Sync + 'static> SharedOnce<T> {
    pub(crate) const fn new(name: &'static str) -> Self {
        Self {
            name,
            value: OnceLock::new(),
        }
    }

    pub(crate) fn get_or_init(&'static self, build: impl FnOnce() -> Arc<T>) -> &'static Arc<T> {
        self.value.get_or_init(|| {
            register(self);
            take(self.name, 0).unwrap_or_else(build)
        })
    }
}

impl<T: Send + Sync + 'static> SharedStore for SharedOnce<T> {
    fn name(&self) -> &'static str {
        self.name
    }

    fn each(&self, visit: &mut dyn FnMut(u64, usize)) {
        if let Some(value) = self.value.get() {
            visit(0, Arc::as_ptr(value) as usize);
        }
    }

    fn find(&self, key: u64) -> Option<Box<dyn Any>> {
        (key == 0)
            .then(|| self.value.get())
            .flatten()
            .map(|value| Box::new(value.clone()) as Box<dyn Any>)
    }
}
