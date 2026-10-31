//! The room's WebSockets in a page: the browser's own `WebSocket` objects (those of
//! `src/net/client.ts`) behind the native sockets' interface (`sockets.rs`). Their
//! events queue for the room, which drains them every frame and poll.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use sloppy_core::net::client::MAX_BUFFERED_BYTES;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

/// The close code of a socket that never opened (no close frame).
const ABNORMAL_CLOSE: u16 = 1006;

/// What a socket reports, with the room's id for it.
#[derive(Debug, PartialEq, Eq)]
pub enum SocketEvent {
    Opened(u32),
    Text(u32, String),
    Binary(u32, Vec<u8>),
    /// The socket is gone, with its close code; the last event of a socket.
    Closed(u32, u16),
}

type Events = Rc<RefCell<VecDeque<SocketEvent>>>;

/// A socket and the handlers the browser calls, kept until its close arrives.
struct Socket {
    socket: WebSocket,
    _opened: Closure<dyn FnMut(Event)>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _closed: Closure<dyn FnMut(CloseEvent)>,
}

impl Socket {
    /// Close without hearing more: its handlers are about to go.
    fn silence(&self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        let _ = self.socket.close();
    }
}

/// The room's open sockets and their events.
#[derive(Default)]
pub struct Sockets {
    sockets: HashMap<u32, Socket>,
    /// Sockets the room closed, until the browser reports them closed.
    closing: HashMap<u32, Socket>,
    events: Events,
}

impl Sockets {
    /// Connect to `url`; its events carry `id`.
    pub fn open(&mut self, id: u32, url: String) {
        let Ok(socket) = WebSocket::new(&url) else {
            self.events
                .borrow_mut()
                .push_back(SocketEvent::Closed(id, ABNORMAL_CLOSE));
            return;
        };
        socket.set_binary_type(BinaryType::Arraybuffer);
        let events = self.events.clone();
        let opened = Closure::<dyn FnMut(Event)>::new(move |_: Event| {
            events.borrow_mut().push_back(SocketEvent::Opened(id));
        });
        let events = self.events.clone();
        let message = Closure::<dyn FnMut(MessageEvent)>::new(move |message: MessageEvent| {
            let data = message.data();
            let event = if let Some(text) = data.as_string() {
                SocketEvent::Text(id, text)
            } else if let Ok(buffer) = data.dyn_into::<js_sys::ArrayBuffer>() {
                SocketEvent::Binary(id, js_sys::Uint8Array::new(&buffer).to_vec())
            } else {
                return;
            };
            events.borrow_mut().push_back(event);
        });
        let events = self.events.clone();
        let closed = Closure::<dyn FnMut(CloseEvent)>::new(move |close: CloseEvent| {
            events
                .borrow_mut()
                .push_back(SocketEvent::Closed(id, close.code()));
        });
        socket.set_onopen(Some(opened.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
        socket.set_onclose(Some(closed.as_ref().unchecked_ref()));
        self.sockets.insert(
            id,
            Socket {
                socket,
                _opened: opened,
                _message: message,
                _closed: closed,
            },
        );
    }

    /// Send `text`, or close the socket when it is too far behind (the room reconnects).
    pub fn send(&mut self, id: u32, text: String) {
        let Some(socket) = self.sockets.get(&id) else {
            return;
        };
        if socket.socket.buffered_amount() as usize > MAX_BUFFERED_BYTES {
            self.close(id);
            return;
        }
        let _ = socket.socket.send_with_str(&text);
    }

    /// Close the socket after the sends queued before this.
    pub fn close(&mut self, id: u32) {
        if let Some(socket) = self.sockets.remove(&id) {
            let _ = socket.socket.close();
            self.closing.insert(id, socket);
        }
    }

    /// The next event of any socket, without waiting.
    pub fn next_event(&mut self) -> Option<SocketEvent> {
        let event = self.events.borrow_mut().pop_front()?;
        if let SocketEvent::Closed(id, _) = event {
            self.sockets.remove(&id);
            self.closing.remove(&id);
        }
        Some(event)
    }

    /// Close every socket; the browser sends what they hold, the room's leave among it.
    pub fn close_all_and_wait(&mut self) {
        let ids: Vec<u32> = self.sockets.keys().copied().collect();
        for id in ids {
            self.close(id);
        }
    }
}

impl Drop for Sockets {
    fn drop(&mut self) {
        for socket in self.sockets.values().chain(self.closing.values()) {
            socket.silence();
        }
    }
}
