use std::error::Error;
use std::fmt;
use std::future::Future;

/// Represents an executor context.
///
/// For more details, see [`enter` documentation](fn.enter.html)
#[derive(Debug)]
pub struct Enter;

/// An error returned by `enter` if an execution scope has already been
/// entered.
pub struct EnterError {
    _a: (),
}

impl fmt::Debug for EnterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnterError")
            .field("reason", &format!("{}", self))
            .finish()
    }
}

impl fmt::Display for EnterError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "attempted to run an executor while another executor is already running"
        )
    }
}

impl Error for EnterError {}

/// Marks the current coroutine as being within the dynamic extent of an
/// executor.
///
/// Executor implementations should call this function before blocking the
/// coroutine. If `None` is returned, the executor should fail by panicking or
/// taking some other action without blocking the current coroutine. This prevents
/// deadlocks due to multiple executors competing for the same coroutine.
///
/// # Error
///
/// Returns an error if the current coroutine is already marked
pub fn enter() -> Result<Enter, EnterError> {
    Ok(Enter)
}

impl Enter {
    /// Blocks the coroutine on the specified future, returning the value with
    /// which that future completes.
    pub fn block_on<F: Future>(&mut self, mut f: F) -> F::Output {
        use super::park::Parker;
        use tokio_executor::park::Park;

        use std::pin::Pin;
        use std::task::Context;
        use std::task::Poll::Ready;

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
}
