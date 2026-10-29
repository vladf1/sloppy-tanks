//! Work whose answer a later frame picks up, one way on every platform. Natively an
//! engine starts on the main thread before the call returns and other work runs on a
//! thread of its own; in a page both run on the browser's event loop between frames,
//! which has no thread to block.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// What main-thread work made, once it is done.
pub struct Task<T>(Rc<RefCell<Option<T>>>);

impl<T: 'static> Task<T> {
    /// Run `future` on the main thread (an engine and its GPU device): natively to the
    /// end before this returns, in a page while frames go on.
    pub fn on_main_thread(future: impl Future<Output = T> + 'static) -> Self {
        let answer = Rc::new(RefCell::new(None));
        #[cfg(not(target_arch = "wasm32"))]
        {
            *answer.borrow_mut() = Some(pollster::block_on(future));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let slot = answer.clone();
            wasm_bindgen_futures::spawn_local(async move {
                *slot.borrow_mut() = Some(future.await);
            });
        }
        Self(answer)
    }

    /// The answer, once it has come.
    pub fn take(&self) -> Option<T> {
        self.0.borrow_mut().take()
    }
}

/// What background work made, once it is done.
pub struct Background<T>(Receiver<T>);

impl<T: Send + 'static> Background<T> {
    /// Run `future` on a thread of its own, where it may block (a request).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn(future: impl Future<Output = T> + Send + 'static) -> Self {
        let (sender, answer) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(pollster::block_on(future));
        });
        Self(answer)
    }

    /// Run `future` between the page's frames.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn(future: impl Future<Output = T> + 'static) -> Self {
        let (sender, answer) = channel();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sender.send(future.await);
        });
        Self(answer)
    }

    /// The answer once it has come; disconnected when the work ended without one.
    pub fn answer(&self) -> Result<T, TryRecvError> {
        self.0.try_recv()
    }
}
