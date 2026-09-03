pub mod buffer;
mod code_conversion_workspace;
pub mod display_evaluation;
pub mod emacs_core;
pub mod encoding;
pub mod face;
mod frontend_events;
#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub mod fuzz_support;
pub mod gc_trace;
pub mod heap_types;
pub mod host;
pub mod keyboard;
mod keyboard_input;
pub mod logging;
pub mod tagged;
#[cfg(test)]
pub mod test_utils;
pub mod window;

// Curated facade: the front door for consumers of the Lisp engine. The
// full module tree stays reachable for specialized needs; these are the
// types nearly every embedder touches.
pub use emacs_core::error::{EvalError, Flow};
pub use emacs_core::eval::Context;
pub use emacs_core::value::{Value, ValueKind};

pub const CORE_BACKEND: &str = "rust";

// GNU seeds emacs-version from configure.ac's PACKAGE_VERSION; version.el
// derives its numeric components from that same value. Keep the bare evaluator
// and loaded Lisp equally consistent, including for GNU's MAJOR.MINOR.MICRO
// development versions. Integer constants make invalid components a compile
// error, and concat! keeps the public string allocation-free.
macro_rules! gnu_emacs_version {
    ($major:literal, $minor:literal $(, $micro:literal)?) => {
        /// The GNU Emacs version whose Lisp tree and compatibility surface we track.
        pub const GNU_EMACS_VERSION: &str =
            concat!(stringify!($major), ".", stringify!($minor) $(, ".", stringify!($micro))?);
        pub(crate) const GNU_EMACS_MAJOR_VERSION: u32 = $major;
        pub(crate) const GNU_EMACS_MINOR_VERSION: u32 = $minor;
        $(const _: u32 = $micro;)?
    };
}

gnu_emacs_version!(31, 1);

use neovm_host_abi::{LispValue, SelectOp, SelectResult, Signal, TaskError, TaskOptions};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
}

/// Contract between the engine and a host task runtime (implemented by
/// neovm-worker): spawn Lisp forms as tasks, await/cancel them, and
/// multiplex channel operations.
pub trait TaskScheduler {
    fn spawn_task(&self, form: LispValue, opts: TaskOptions) -> Result<TaskHandle, Signal>;

    fn task_cancel(&self, handle: TaskHandle) -> bool;

    fn task_status(&self, handle: TaskHandle) -> Option<TaskStatus>;

    fn task_await(
        &self,
        handle: TaskHandle,
        timeout: Option<Duration>,
    ) -> Result<LispValue, TaskError>;

    fn select(&self, ops: &[SelectOp], timeout: Option<Duration>) -> SelectResult;
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
