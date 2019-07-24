use super::park::Parker;
use tokio_executor::park::Park;

use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll::Ready;

/// Blocks the coroutine on the specified future, returning the value with
/// which that future completes.
pub fn block_on<F: Future>(mut f: F) -> F::Output {
    let mut park = Parker::new();
    let waker = park.unpark().into_waker();
    let mut cx = Context::from_waker(&waker);

    // `block_on` takes ownership of `f`. Once it is pinned here, the original `f` binding can
    // no longer be accessed, making the pinning safe.
    let mut f = unsafe { Pin::new_unchecked(&mut f) };

    loop {
        if let Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        if park.park().is_err() {
            may::coroutine::trigger_cancel_panic();
        }
    }
}
