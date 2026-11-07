//! The ONE test-only thread-exit leak wrapper (peer ruling 11:5xZ G2).
//!
//! # Invariant
//!
//! A `thread_local!` fixture must never hold an OWNING `Context`, `World`
//! or `TaggedHeap`.  `Context::drop` needs live TLS by construction: it
//! clears the thread's `TAGGED_HEAP`, retires registries and joins the
//! heap's collector, whose `Collector::join` emits a tracing event.  At
//! thread exit the thread-local destructor runs inside TLS destruction,
//! where tracing's own thread-local may already be gone, so that event
//! panics with `cannot access a Thread Local Storage value during or after
//! destruction` (std AccessError).  Product code keeps no owning fixture in
//! a thread-local (the GC thread-locals are raw pointers); the architecture
//! guard `architecture_thread_local_owner_guard_test` enforces that.
//!
//! A test that genuinely needs one fixture per thread stores it in this
//! wrapper: the value is boxed and deliberately leaked when the thread-local
//! is destroyed (`ManuallyDrop`), so the destructor is a no-op and cannot
//! reach the collector join.  `nextest` runs each test in its own process,
//! so the leak is bounded there.  Under plain `cargo test` (many tests per
//! process) one leaked fixture accumulates per libtest thread until the
//! process exits; that is the accepted trade-off of this test-only route.

/// Owns `T` for the thread's lifetime and deliberately leaks it at thread
/// exit; see the module invariant.  Use [`Self::get`] / [`Self::get_mut`]
/// to reach the value; the wrapper is not `Sync`-special (the thread-local
/// itself provides the thread confinement).
pub struct ThreadExitLeak<T> {
    /// `ManuallyDrop`: dropping the box at thread exit is exactly the
    /// hazard this wrapper exists to prevent.
    value: std::mem::ManuallyDrop<Box<T>>,
}

impl<T> ThreadExitLeak<T> {
    /// Wrap `value`.  The value is never dropped by this wrapper.
    pub fn new(value: T) -> Self {
        Self {
            value: std::mem::ManuallyDrop::new(Box::new(value)),
        }
    }

    /// Shared access to the leaked value.
    pub fn get(&self) -> &T {
        &**self.value
    }

    /// Exclusive access to the leaked value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut **self.value
    }
}
