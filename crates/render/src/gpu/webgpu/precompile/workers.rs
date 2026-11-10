//! Pipelines compiled by Web Workers before the page's device creates them, in Safari.
//!
//! Safari compiles one device's pipelines strictly one after another, async or not, and
//! the page draws no frame until they are done (about 180 ms each on an M-series Mac,
//! 140 ms on an iPhone, for a large fragment stage); then each new pipeline holds the
//! page again at its first draw (130 and 100 ms), surface, cutout and depth-only shadow
//! pipelines alike. Workers with devices of their own compile side by side while the
//! page keeps drawing at its full rate, and Safari keeps what any device compiled for
//! its whole GPU process: once a worker has compiled a pipeline and drawn with it, the
//! page's device creates the same pipeline (same stage texts, layout entries and
//! states) in 2-15 ms, and its first draw takes 1-2 ms. A worker that only compiles
//! saves the create but not the first draw, and setting the pipeline in a pass without
//! drawing saves neither; a draw that covers no pixel does, and what the worker built
//! stays after its device and the worker are gone. Chrome compiles one device's async
//! pipelines in parallel already and has no first-draw cost, so only Safari's page
//! offers the workers (`sloppyPipelineWorker` in `index.html`, `src/pipeline-worker.ts`).
//!
//! Every job resolves once a worker answers, compiled or not: the page's device then
//! compiles the pipeline as it would have, cheaply after a worker or at full cost after
//! a failure. A worker that cannot start a device, loses it, or keeps a job past
//! [`JOB_LIMIT_MS`] leaves, its job left to the page; once none is left, the page
//! compiles everything itself.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};

use js_sys::{Array, Function, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;

use super::browser::{RawDevice, object, set};

/// Fewest and most workers compiling at once, one per core the browser reports
/// (`navigator.hardwareConcurrency`: Safari says 8 on an M-series Mac, 4 on an iPhone).
/// An iPhone compiled 2x faster with 4 than on the page's device and gained nothing
/// from 8; a Mac gained 3.5x with 4 and almost 7x with 8. Each holds a GPU device,
/// which costs memory on a phone.
const MIN_WORKERS: u32 = 2;
const MAX_WORKERS: u32 = 8;

/// How long a worker may take over one job before it counts as broken (hung, or
/// suspended by the browser) and the page compiles that job itself. A cold Mac took up
/// to 0.8 s for one of the village's, side by side with three others; a slow phone may
/// take several times that, so only a worker that is really stuck reaches it.
const JOB_LIMIT_MS: i32 = 30_000;

/// How long the workers stay with nothing to compile before they and their devices go:
/// long enough for the pipelines that follow at once (an arena's after the renderer's
/// fixed ones), short enough to free them while the round plays.
const IDLE_MS: i32 = 1500;

#[wasm_bindgen]
extern "C" {
    /// A page's `Worker`.
    type PageWorker;

    #[wasm_bindgen(method, catch, js_name = postMessage)]
    fn post_message(this: &PageWorker, message: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    fn terminate(this: &PageWorker);
}

struct Worker {
    id: u32,
    worker: PageWorker,
    /// The job it compiles, if any: one at a time, since a device compiles one at a time.
    job: Option<u32>,
    _on_message: Closure<dyn FnMut(JsValue)>,
    _on_error: Closure<dyn FnMut(JsValue)>,
}

struct Pool {
    workers: Vec<Worker>,
    /// Jobs no worker has taken yet: their number and plain pipeline description.
    waiting: VecDeque<(u32, JsValue)>,
    /// Each job's promise's resolve, until a worker answers it.
    answers: HashMap<u32, Function>,
    next_job: u32,
    /// Counts work arriving, so a close planned while idle stands down.
    generation: u32,
}

thread_local! {
    static POOL: RefCell<Option<Pool>> = const { RefCell::new(None) };
    /// Set once no worker could start or every one broke: the page compiles itself.
    static GIVEN_UP: Cell<bool> = const { Cell::new(false) };
    static NEXT_WORKER: Cell<u32> = const { Cell::new(0) };
}

/// The page's worker factory, where it offers one (Safari's page, or `?pipeline-workers=on`).
fn factory() -> Option<Function> {
    let factory = Reflect::get(&js_sys::global(), &"sloppyPipelineWorker".into()).ok()?;
    factory.dyn_into().ok()
}

/// Whether the page has workers to compile pipelines (Safari's page, or
/// `?pipeline-workers=on`); asked before a job is described, so a page without them
/// does no more work than before.
pub(super) fn available() -> bool {
    if GIVEN_UP.get() {
        return false;
    }
    if factory().is_none() {
        GIVEN_UP.set(true);
        return false;
    }
    true
}

/// Have a worker compile and draw `plain` (a pipeline described with texts and layout
/// entries, `browser::Precompiler::plain_descriptor`) first; the promise resolves once
/// one has, or could not. `None` where the page has no workers: the page compiles alone.
pub(super) fn compile(device: &RawDevice, plain: JsValue) -> Option<Promise> {
    if !available() {
        return None;
    }
    let factory = factory()?;
    POOL.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Pool::start(&factory, device)?);
        }
        let pool = slot.as_mut()?;
        let job = pool.next_job;
        pool.next_job += 1;
        pool.generation += 1;
        let promise = Promise::new(&mut |resolve, _reject| {
            pool.answers.insert(job, resolve);
        });
        pool.waiting.push_back((job, plain));
        pool.dispatch();
        if pool.workers.is_empty() {
            *slot = None;
        }
        Some(promise)
    })
}

impl Pool {
    fn start(factory: &Function, device: &RawDevice) -> Option<Self> {
        let open = object(&[("open", device_wish(device).into())]);
        let count = worker_count();
        let mut workers = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let Some(worker) = start_worker(factory, &open) else {
                break;
            };
            workers.push(worker);
        }
        if workers.is_empty() {
            web_sys::console::warn_1(&"Pipeline workers did not start; the page compiles".into());
            GIVEN_UP.set(true);
            return None;
        }
        Some(Self {
            workers,
            waiting: VecDeque::new(),
            answers: HashMap::new(),
            next_job: 0,
            generation: 0,
        })
    }

    /// Hand waiting jobs to idle workers.
    fn dispatch(&mut self) {
        let mut broken = Vec::new();
        for worker in self
            .workers
            .iter_mut()
            .filter(|worker| worker.job.is_none())
        {
            let Some((job, plain)) = self.waiting.pop_front() else {
                break;
            };
            let message = object(&[("job", job.into()), ("pipeline", plain)]);
            match worker.worker.post_message(&message) {
                Ok(()) => {
                    worker.job = Some(job);
                    give_up_on_silence(worker.id, job);
                }
                Err(error) => {
                    resolve(&mut self.answers, job);
                    broken.push((worker.id, format!("{error:?}")));
                }
            }
        }
        for (id, reason) in broken {
            self.remove(id, &reason);
        }
    }

    fn idle(&self) -> bool {
        self.waiting.is_empty() && self.workers.iter().all(|worker| worker.job.is_none())
    }

    /// Drop worker `id` for good, its job left to the page.
    fn remove(&mut self, id: u32, reason: &str) {
        let Some(index) = self.workers.iter().position(|worker| worker.id == id) else {
            return;
        };
        let worker = self.workers.remove(index);
        worker.worker.terminate();
        if let Some(job) = worker.job {
            resolve(&mut self.answers, job);
        }
        web_sys::console::warn_1(&format!("A pipeline worker stopped: {reason}").into());
        if self.workers.is_empty() {
            GIVEN_UP.set(true);
            for (job, _) in std::mem::take(&mut self.waiting) {
                resolve(&mut self.answers, job);
            }
        } else if self.idle() {
            // The others may have nothing left: they go as after their last answer.
            close_if_still_idle(self.generation);
        }
    }

    /// Close every worker, each destroying its device first.
    fn close(self) {
        let close = object(&[("close", true.into())]);
        for worker in &self.workers {
            // Their listeners go with the pool.
            let target: &Object = worker.worker.unchecked_ref();
            set(target, "onmessage", JsValue::NULL);
            set(target, "onerror", JsValue::NULL);
            if worker.worker.post_message(&close).is_err() {
                worker.worker.terminate();
            }
        }
    }
}

fn start_worker(factory: &Function, open: &Object) -> Option<Worker> {
    let worker: PageWorker = factory.call0(&JsValue::NULL).ok()?.unchecked_into();
    let id = NEXT_WORKER.get();
    NEXT_WORKER.set(id + 1);
    let on_message = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let data = Reflect::get(&event, &"data".into()).unwrap_or(JsValue::UNDEFINED);
        answered(id, &data);
    });
    let on_error = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        // A script error is an `ErrorEvent`; a script that did not load, an `Event`.
        let message = Reflect::get(&event, &"message".into())
            .ok()
            .and_then(|message| message.as_string())
            .unwrap_or_else(|| "the worker did not load".into());
        with_pool(|pool| pool.remove(id, &message));
    });
    let target: &Object = worker.unchecked_ref();
    set(target, "onmessage", on_message.as_ref().clone());
    set(target, "onerror", on_error.as_ref().clone());
    if worker.post_message(open).is_err() {
        worker.terminate();
        return None;
    }
    Some(Worker {
        id,
        worker,
        job: None,
        _on_message: on_message,
        _on_error: on_error,
    })
}

/// Worker `id` answered: a job (`{ job, error? }`), or that it cannot go on
/// (`{ broken }`).
fn answered(id: u32, data: &JsValue) {
    let text = |name: &str| {
        Reflect::get(data, &name.into())
            .ok()
            .and_then(|value| value.as_string())
    };
    if let Some(reason) = text("broken") {
        with_pool(|pool| pool.remove(id, &reason));
        return;
    }
    let Some(job) = Reflect::get(data, &"job".into())
        .ok()
        .and_then(|job| job.as_f64())
    else {
        return;
    };
    with_pool(|pool| {
        // A worker removed meanwhile (one past its time limit) left its job to the page.
        let Some(worker) = pool.workers.iter_mut().find(|worker| worker.id == id) else {
            return;
        };
        worker.job = None;
        if let Some(error) = text("error") {
            web_sys::console::warn_1(
                &format!("A pipeline worker could not compile one; the page does: {error}").into(),
            );
        }
        resolve(&mut pool.answers, job as u32);
        pool.dispatch();
        if pool.idle() {
            close_if_still_idle(pool.generation);
        }
    });
}

fn with_pool(act: impl FnOnce(&mut Pool)) {
    POOL.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(pool) = slot.as_mut() {
            act(pool);
            if pool.workers.is_empty() {
                *slot = None;
            }
        }
    });
}

/// Remove worker `id` if it still has `job` after [`JOB_LIMIT_MS`].
fn give_up_on_silence(id: u32, job: u32) {
    let check = Closure::once_into_js(move || {
        with_pool(|pool| {
            let stuck = pool
                .workers
                .iter()
                .any(|worker| worker.id == id && worker.job == Some(job));
            if stuck {
                pool.remove(id, &format!("no answer in {JOB_LIMIT_MS} ms"));
            }
        });
    });
    set_timeout(&check, JOB_LIMIT_MS);
}

/// Close the workers after [`IDLE_MS`] unless work arrived meanwhile.
fn close_if_still_idle(generation: u32) {
    let check = Closure::once_into_js(move || {
        POOL.with(|cell| {
            let idle = cell
                .borrow()
                .as_ref()
                .is_some_and(|pool| pool.idle() && pool.generation == generation);
            if idle && let Some(pool) = cell.borrow_mut().take() {
                pool.close();
            }
        });
    });
    set_timeout(&check, IDLE_MS);
}

fn set_timeout(callback: &JsValue, ms: i32) {
    if let Some(window) = web_sys::window() {
        let _ = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), ms);
    }
}

/// Workers for this machine: one per core the browser reports, within bounds.
fn worker_count() -> u32 {
    let cores = Reflect::get(&js_sys::global(), &"navigator".into())
        .and_then(|navigator| Reflect::get(&navigator, &"hardwareConcurrency".into()))
        .ok()
        .and_then(|cores| cores.as_f64())
        .unwrap_or(0.0);
    (cores as u32).clamp(MIN_WORKERS, MAX_WORKERS)
}

fn resolve(answers: &mut HashMap<u32, Function>, job: u32) {
    if let Some(resolve) = answers.remove(&job) {
        // Resolving only queues the waiting code, so the pool stays borrowed safely.
        let _ = resolve.call0(&JsValue::NULL);
    }
}

/// The features and limits the page's device has, for the workers' devices: a
/// pipeline compiles for the device it is made on.
fn device_wish(device: &RawDevice) -> Object {
    let features = Reflect::get(device, &"features".into())
        .map(|features| Array::from(&features))
        .unwrap_or_default();
    let limits = Object::new();
    if let Ok(supported) = Reflect::get(device, &"limits".into()) {
        // `GPUSupportedLimits` has its limits as getters on its prototype.
        let prototype = Object::get_prototype_of(&supported);
        for name in Object::get_own_property_names(&prototype).iter() {
            if let Ok(value) = Reflect::get(&supported, &name)
                && value.as_f64().is_some()
            {
                let _ = Reflect::set(&limits, &name, &value);
            }
        }
    }
    object(&[("features", features.into()), ("limits", limits.into())])
}
