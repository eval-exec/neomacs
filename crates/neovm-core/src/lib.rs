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
pub use neovm_value::gc_trace;
pub use neovm_value::heap_types;
pub mod host_effect;
pub mod host_input;
mod image_identity;
pub mod keyboard;
mod keyboard_input;
pub use neovm_value::logging;
pub use neovm_value::tagged;
#[cfg(test)]
#[path = "tests/test_utils_test.rs"]
pub mod test_utils;
#[cfg(debug_assertions)]
pub(crate) use neovm_value::tls_fingerprint;
// The thread-local restoration guard lives in the lower value crate. The
// re-export keeps the historical `crate::tls_scope` path for every core user.
pub(crate) use neovm_value::tls_scope;
// The process PRNG substrate (GNU sysdep.c) moved with the stage-3 split
// (C12): the buffer substrate generates unique names below the lisp layer.
pub(crate) use neovm_value::random;
// The list-scan substrate (GNU fns.c memq/assq + lisp.h FOR_EACH_TAIL) moved
// with the stage-3 split (C12): edit-layer text-property casing and the lisp
// subrs share the same scans below the evaluator.
pub(crate) use neovm_value::list_scan;
// The string-construction cores (GNU fns.c substring/concat) moved with the
// stage-3 split (C12): the subr wrappers keep the perf_trace probes in the
// lisp layer, while the buffer substrate reaches the cores directly.
pub(crate) use neovm_value::strings_core;
// The sequence-copy core (GNU fns.c copy-sequence) moved with the stage-3
// split (C12) once the char-table copy landed in chartab (C7a): the subr
// wrappers and Context-routed entries resolve the symbol policy first.
pub(crate) use neovm_value::sequence_copy;
// The exact (TICKS . HZ) time-conversion core (GNU timefns.c) moved with
// the stage-3 split (C16): format's %N, fileio, buffer idle limits, dired,
// and the dynamic-module bridge read time values below the lisp layer.
pub(crate) use neovm_value::time_core;
pub mod window;

// Curated facade: the front door for consumers of the Lisp engine. The
// full module tree stays reachable for specialized needs; these are the
// types nearly every embedder touches.
pub use emacs_core::error::{EvalError, Flow, FlowKind, FlowRef, FlowResultExt};
pub use emacs_core::eval::Context;
pub use emacs_core::value::{Value, ValueKind};

// Preserve canonical value-layer paths in unchanged relocated test bodies.
#[cfg(test)]
pub(crate) use emacs_core::{error, intern, value};

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
mod tests;
