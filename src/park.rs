use may::coroutine::ParkError;
use may::sync::Blocker;
use tokio_executor::park::{Park, Unpark};

use std::sync::Arc;
use std::task::{RawWaker, RawWakerVTable, Waker};
use std::time::Duration;

/// Blocks the current coroutine using coroutine blocker.
#[derive(Debug)]
pub struct Parker {
    blocker: Arc<Blocker>,
}

/// Unblocks a coroutine that was blocked by `Parker`.
#[derive(Clone, Debug)]
pub struct Unparker {
    inner: Arc<Blocker>,
}

// ===== impl Parker =====

impl Parker {
    /// Create a new `Parker` handle for the current coroutine.
    pub fn new() -> Parker {
        Parker {
            blocker: Blocker::current(),
        }
    }
}

impl Park for Parker {
    type Unpark = Unparker;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        let inner = self.blocker.clone();
        Unparker { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.blocker.park(None)
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.blocker.park(Some(duration))
    }
}

// ===== impl Unparker =====

impl Unpark for Unparker {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

impl Unparker {
    pub(crate) fn into_waker(self) -> Waker {
        unsafe {
            let raw = unparker_to_raw_waker(self);
            Waker::from_raw(raw)
        }
    }

    fn into_raw(this: Unparker) -> *const () {
        Arc::into_raw(this.inner) as *const ()
    }

    unsafe fn from_raw(ptr: *const ()) -> Unparker {
        Unparker {
            inner: Arc::from_raw(ptr as *const Blocker),
        }
    }
}

unsafe fn unparker_to_raw_waker(unparker: Unparker) -> RawWaker {
    RawWaker::new(Unparker::into_raw(unparker), &VTABLE)
}

unsafe fn clone(raw: *const ()) -> RawWaker {
    let unparker = Unparker::from_raw(raw);

    // Increment the ref count
    std::mem::forget(unparker.clone());

    unparker_to_raw_waker(unparker)
}

unsafe fn wake(raw: *const ()) {
    let unparker = Unparker::from_raw(raw);
    unparker.unpark();
}

unsafe fn wake_by_ref(raw: *const ()) {
    let unparker = Unparker::from_raw(raw);
    unparker.unpark();

    // We don't actually own a reference to the unparker
    std::mem::forget(unparker);
}

unsafe fn drop(raw: *const ()) {
    let _ = Unparker::from_raw(raw);
}
