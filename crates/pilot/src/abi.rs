//! The Wasm exports the bots Worker calls (`bots/pilot-engine.ts`): the [`Fleet`] behind
//! plain numbers, with text and bytes passed through one staging buffer each way. The
//! module imports nothing, so it needs no generated glue.
//!
//! To pass text or bytes in, JavaScript asks [`pilot_input`] for a buffer of their length,
//! writes them there and makes the call. Calls that answer with text leave it in the
//! output buffer and return its length; [`pilot_output`] is where it starts. Memory may
//! grow during any call, so JavaScript takes a fresh view of it after each. A panic
//! aborts the call (a trap in JavaScript); [`pilot_panic`] then answers with its message,
//! and the Worker starts a new instance.

use std::cell::RefCell;

use super::fleet::{Fleet, versions};

#[derive(Default)]
struct Engine {
    fleet: Fleet,
    input: Vec<u8>,
    output: Vec<u8>,
    panic: String,
}

thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::default());
}

/// Runs `call` with the engine, and keeps any text it answers in the output buffer.
fn answer(call: impl FnOnce(&mut Engine) -> String) -> usize {
    ENGINE.with_borrow_mut(|engine| {
        let text = call(engine);
        engine.output = text.into_bytes();
        engine.output.len()
    })
}

fn input_text(engine: &Engine, length: usize) -> String {
    String::from_utf8_lossy(&engine.input[..length.min(engine.input.len())]).into_owned()
}

/// A buffer of `length` bytes for the next call's text or bytes.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_input(length: usize) -> *mut u8 {
    ENGINE.with_borrow_mut(|engine| {
        engine.input.resize(length, 0);
        engine.input.as_mut_ptr()
    })
}

/// Where the last answer starts.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_output() -> *const u8 {
    ENGINE.with_borrow(|engine| engine.output.as_ptr())
}

/// The last answer's length in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_output_length() -> usize {
    ENGINE.with_borrow(|engine| engine.output.len())
}

/// Installs the panic hook; call once after instantiating. Answers [`versions`].
#[unsafe(no_mangle)]
pub extern "C" fn pilot_start() -> usize {
    std::panic::set_hook(Box::new(|info| {
        let message = info.to_string();
        // The engine may be borrowed by the call that panicked.
        let _ = ENGINE.try_with(|engine| {
            if let Ok(mut engine) = engine.try_borrow_mut() {
                engine.panic = message;
            }
        });
    }));
    answer(|_| versions())
}

/// The message of the panic that trapped the last call.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_panic() -> usize {
    answer(|engine| engine.panic.clone())
}

/// Creates a pilot from the setup JSON in the input buffer. Returns its handle, or 0 with
/// the reason as the answer ([`pilot_output_length`] long).
#[unsafe(no_mangle)]
pub extern "C" fn pilot_create(length: usize, now_ms: f64) -> u32 {
    ENGINE.with_borrow_mut(|engine| {
        let setup = input_text(engine, length);
        match engine.fleet.create(&setup, now_ms) {
            Ok(handle) => handle,
            Err(error) => {
                engine.output = error.into_bytes();
                0
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn pilot_opened(handle: u32, socket: u32, now_ms: f64) {
    ENGINE.with_borrow_mut(|engine| engine.fleet.opened(handle, socket, now_ms));
}

/// A text frame of `length` bytes, in the input buffer.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_text(handle: u32, socket: u32, length: usize, now_ms: f64) {
    ENGINE.with_borrow_mut(|engine| {
        let text = input_text(engine, length);
        engine.fleet.text(handle, socket, &text, now_ms);
    });
}

/// A binary frame of `length` bytes, in the input buffer.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_binary(handle: u32, socket: u32, length: usize, now_ms: f64) {
    ENGINE.with_borrow_mut(|engine| {
        let Engine { fleet, input, .. } = engine;
        let bytes = &input[..length.min(input.len())];
        fleet.binary(handle, socket, bytes, now_ms);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn pilot_closed(handle: u32, socket: u32, code: u32, now_ms: f64) {
    let code = u16::try_from(code).unwrap_or(1006);
    ENGINE.with_borrow_mut(|engine| engine.fleet.closed(handle, socket, code, now_ms));
}

/// One frame; answers the actions to perform.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_tick(handle: u32, now_ms: f64) -> usize {
    answer(|engine| engine.fleet.tick(handle, now_ms))
}

/// Gives the seat up; answers the actions to perform.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_leave(handle: u32, now_ms: f64) -> usize {
    answer(|engine| engine.fleet.leave(handle, now_ms))
}

/// Answers the pilot's report.
#[unsafe(no_mangle)]
pub extern "C" fn pilot_report(handle: u32, now_ms: f64) -> usize {
    answer(|engine| engine.fleet.report(handle, now_ms))
}

#[unsafe(no_mangle)]
pub extern "C" fn pilot_remove(handle: u32) {
    ENGINE.with_borrow_mut(|engine| engine.fleet.remove(handle));
}
