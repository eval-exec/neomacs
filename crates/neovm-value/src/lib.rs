//! Lower NeoVM runtime utilities shared with `neovm-core`.
//!
//! This crate is the value layer extracted from the core runtime.  Core
//! keeps the upper evaluator and re-exports these modules through its
//! facades.

pub mod alloc;
pub mod boolvec;
pub mod emacs_char;
pub mod error;
pub mod errors;
pub mod forward;
pub mod gc_trace;
pub mod hashtab;
pub mod heap_types;
pub mod intern;
pub mod symbol;
pub mod tagged;
pub mod tls_fingerprint;
pub mod tls_scope;
pub mod value;

// PC2 sealed hook installers (exact-chain; see architecture PC2 table)
pub use value::bytecode_semantics::{BytecodeSemantics, install_bytecode_semantics};
pub use value::hook_publication::{HookInstallError, HookInstallStatus};
pub use value::scratch_bridge::{ScratchRootHooks, install_scratch_root_hooks};
