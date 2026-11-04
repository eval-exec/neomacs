//! Lower NeoVM runtime utilities shared with `neovm-core`.
//!
//! This crate is the value layer extracted from the core runtime.  Core
//! keeps the upper evaluator and re-exports these modules through its
//! facades.

pub mod alloc;
pub mod boolvec;
pub mod builtin_frontend;
pub mod chartab;
pub mod cold_retained;
pub mod emacs_char;
pub mod error;
pub mod errors;
pub mod forward;
pub mod gc_trace;
pub mod hashtab;
pub mod heap_registry;
pub mod heap_types;
pub mod intern;
pub mod list_scan;
pub mod plist;
pub mod quit;
pub mod random;
pub mod runtime_root;
pub mod sequence_copy;
pub mod string_pos_cache;
pub mod strings_core;
pub mod symbol;
pub mod system_vocab;
pub mod tagged;
pub mod time_core;
pub mod tls_fingerprint;
pub mod tls_scope;
pub mod value;

pub mod logging;

/// Test-only read surface for core-owned tests of the moved modules.
#[cfg(feature = "test-support")]
pub mod test_support;

/// Upper-free test helpers shared with core's test build (peer-signed R7c).
#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;

// PC2 sealed hook installers (exact-chain; see architecture PC2 table)
pub use error::format_hook::{EvalErrorFormatter, install_eval_error_formatter};
pub use heap_types::install_string_bytes_changed_hook;
pub use symbol::function_cell_observer::{
    FunctionCellObserver, FunctionEpochMoveSink, install_function_cell_observer,
    install_function_epoch_move_sink,
};
pub use tagged::header::sqlite_finalizer::{SqliteObjFinalizer, install_sqlite_obj_finalizer};
pub use value::bytecode_semantics::{BytecodeSemantics, install_bytecode_semantics};
pub use value::hook_publication::{HookInstallError, HookInstallStatus};
pub use value::scratch_bridge::{ScratchRootHooks, install_scratch_root_hooks};
