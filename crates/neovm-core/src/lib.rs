pub use neovm_display::display_evaluation;
pub use neovm_edit::buffer;
pub(crate) use neovm_text::code_conversion_workspace;
pub mod emacs_core;
pub use neovm_display::face;
pub use neovm_text::encoding;
// Frontend input vocabulary and state moved into the eval manifest (stage-4
// W2/E6b); the historical crate-root paths stay as re-exports for every
// in-crate and frontend-crate user.
pub(crate) use crate::emacs_core::eval::frontend_events;
#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub mod fuzz_support;
pub use neovm_value::gc_trace;
#[cfg(not(test))]
pub use neovm_value::heap_types;
#[cfg(test)]
#[path = "../core_test_facades/heap_types.rs"]
pub mod heap_types;
pub use crate::emacs_core::eval::keyboard;
pub(crate) use crate::emacs_core::eval::keyboard_input;
pub use crate::emacs_core::host_input;
#[cfg(test)]
pub(crate) use neovm_display::image_identity;
pub use neovm_eval::host_effect;
pub use neovm_value::logging;
#[cfg(test)]
#[path = "../core_test_facades/symbol.rs"]
pub mod symbol;
#[cfg(not(test))]
pub use neovm_value::tagged;
#[cfg(test)]
#[path = "../core_test_facades/keyboard_pure.rs"]
pub mod keyboard_pure;
#[cfg(test)]
#[path = "../core_test_facades/tagged.rs"]
pub mod tagged;
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
// The char-table container substrate (GNU chartab.c) moved below the
// evaluator with the stage-3 split (C7a): the edit layer reaches the pure
// lookups and `char-table-subtype` directly, below the text layer that
// keeps the evaluator-bound entries.
pub(crate) use neovm_value::chartab;
// The exact (TICKS . HZ) time-conversion core (GNU timefns.c) moved with
// the stage-3 split (C16): format's %N, fileio, buffer idle limits, dired,
// and the dynamic-module bridge read time values below the lisp layer.
pub(crate) use neovm_value::time_core;
// The native C error message template (GNU doprnt's translation surface)
// lives in the value crate: the edit layer's M3c host method takes it
// directly, below the coding layer that resolves the quoting style.
pub use neovm_display::window;
pub(crate) use neovm_value::errors;

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
// derives its numeric components from that same value. The constants moved
// with the evaluator (stage-4 W5/E13); the historical crate-root paths stay
// as re-exports.
pub use neovm_eval::GNU_EMACS_VERSION;
#[doc(hidden)]
pub(crate) use neovm_eval::{GNU_EMACS_MAJOR_VERSION, GNU_EMACS_MINOR_VERSION};

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
