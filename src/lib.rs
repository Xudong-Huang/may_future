#![feature(async_await)]

mod background;
mod builder;
mod enter;
mod park;
mod task_executor;

pub use self::builder::Builder;
pub use self::task_executor::TaskExecutor;

use self::background::Background;

use tracing_core as trace;

use std::cell::Cell;
use std::future::Future;
use std::io;

lazy_static::lazy_static! {
    /// global may_future runtime
    pub static ref RT: Runtime = Runtime::new().expect("failed to build may_future runtime");
}

thread_local! {
    /// Tracks if the runtime start, each thread would keep a never return co
    /// to init the tokio run time, this is a hack to work around the TLS context
    static CURRENT_RUNTIME: Cell<bool> = Cell::new(false)
}

/// Handle to the may_future runtime.
///
/// The may_future runtime includes a reactor as well as an executor for running
/// tasks.
///
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
/// [`new`]: #method.new
/// [`Builder`]: struct.Builder.html
#[derive(Debug)]
pub struct Runtime {
    inner: Option<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Task execution pool.
    pool: tokio_threadpool::ThreadPool,

    /// Tracing dispatcher
    trace: trace::Dispatch,

    /// Maintains a reactor and timer that are always running on a background
    /// thread. This is to support `runtime.block_on` w/o requiring the future
    /// to be `Send`.
    ///
    /// A dedicated background thread is required as the threadpool threads
    /// might not be running. However, this is a temporary work around.
    ///
    /// TODO: Delete this
    background: Background,
}

// ===== impl Runtime =====

impl Runtime {
    /// Create a new runtime instance with default configuration values.
    ///
    /// This results in a reactor, thread pool, and timer being initialized. The
    /// thread pool will not spawn any worker threads until it needs to, i.e.
    /// tasks are scheduled to run.
    ///
    /// Most users will not need to call this function directly, instead they
    /// will use [`tokio::run`](fn.run.html).
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// Creating a new `Runtime` with default configuration values.
    ///
    /// ```
    /// use may_future::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_now();
    /// ```
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new().build()
    }

    /// Return a handle to the runtime's executor.
    ///
    /// The returned handle can be used to spawn tasks that run on this runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use may_future::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// let executor_handle = rt.executor();
    ///
    /// // use `executor_handle`
    /// ```
    pub fn executor(&self) -> TaskExecutor {
        let inner = self.inner().pool.sender().clone();
        TaskExecutor { inner }
    }

    /// Spawn a future onto the may_future runtime.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```rust
    /// #![feature(async_await)]
    /// use may_future::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn(async {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    /// # pub fn main() {}
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F) -> &Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.inner().pool.spawn(future);
        self
    }

    /// Run a future to completion on the may_future runtime.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let blocker = co_waiter::Waiter::new();

        let blocker_ref: &'static co_waiter::Waiter<_> = unsafe { std::mem::transmute(&blocker) };

        self.spawn(async move {
            let res = future.await;
            blocker_ref.set_rsp(res);
        });

        blocker.wait_rsp(None).expect("failed to wait result")
    }

    /// Run a future to completion on local thread/coroutine.
    ///
    /// This runs the given future on the local thead/coroutine,
    /// blocking until it is complete, and yielding its resolved
    /// result. Any tasks or timers which the future spawns
    /// internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// This method is slower than the `block_on` method, it evolves
    /// more works for the coroutine scheduling. But this method doesn't
    /// requre the futrue be `Send + 'static`
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on_local<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        CURRENT_RUNTIME.with(|b_init| {
            if !b_init.get() {
                let builder = may::coroutine::Builder::new().stack_size(0x400);
                let clo = || {
                    let bg = &RT.inner().background;
                    let trace = &RT.inner().trace;
                    tokio_executor::with_default(&mut RT.inner().pool.sender(), || {
                        tokio_reactor::with_default(bg.reactor(), || {
                            tokio_timer::with_default(bg.timer(), || {
                                trace::dispatcher::with_default(trace, || {
                                    enter::block_on(async {
                                        // this future would never return
                                        // and all default runtime are set
                                        // properly for `enter::block_on`
                                        let (_tx, rx) = tokio_sync::oneshot::channel::<()>();
                                        rx.await.ok();
                                    })
                                })
                            })
                        })
                    });
                };
                unsafe {
                    // here we use the spawn_local to run
                    // the coroutine in current thread.
                    builder.spawn_local(clo).ok();
                }
                b_init.set(true);
            }
        });

        enter::block_on(future)
    }

    /// Signals the runtime to shutdown once it becomes idle.
    ///
    /// Returns a future that completes once the shutdown operation has
    /// completed.
    ///
    /// This function can be used to perform a graceful shutdown of the runtime.
    ///
    /// The runtime enters an idle state once **all** of the following occur.
    ///
    /// * The thread pool has no tasks to execute, i.e., all tasks that were
    ///   spawned have completed.
    /// * The reactor is not managing any I/O resources.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use may_future::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// let _fut = rt.shutdown_on_idle();
    /// ```
    ///
    /// [mod]: index.html
    pub async fn shutdown_on_idle(mut self) {
        let inner = self.inner.take().unwrap();
        let inner = inner.pool.shutdown_on_idle();

        inner.await;
    }

    /// Signals the runtime to shutdown immediately.
    ///
    /// Returns a future that completes once the shutdown operation has
    /// completed.
    ///
    /// This function will forcibly shutdown the runtime, causing any
    /// in-progress work to become canceled. The shutdown steps are:
    ///
    /// * Drain any scheduled work queues.
    /// * Drop any futures that have not yet completed.
    /// * Drop the reactor.
    ///
    /// Once the reactor has dropped, any outstanding I/O resources bound to
    /// that reactor will no longer function. Calling any method on them will
    /// result in an error.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use may_future::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_now();
    /// ```
    ///
    /// [mod]: index.html
    pub async fn shutdown_now(mut self) {
        let inner = self.inner.take().unwrap();
        inner.pool.shutdown_now().await;
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }
}
