//! Stable `emacs_core` module roots moved from the core crate.

#[path = "lisp/advice/mod.rs"]
pub mod advice;
#[path = "lisp/autoload/mod.rs"]
pub mod autoload;
#[path = "lisp/native/builtins/mod.rs"]
pub mod builtins;
#[path = "lisp/native/builtins_extra/mod.rs"]
pub mod builtins_extra;
#[path = "runtime/bytecode/mod.rs"]
pub mod bytecode;
#[path = "lisp/cl_lib/mod.rs"]
pub mod cl_lib;
#[path = "lisp/custom/mod.rs"]
pub mod custom;
#[path = "runtime/data/mod.rs"]
pub mod data;
#[path = "runtime/debug/mod.rs"]
pub mod debug;
#[path = "runtime/debug_on_call/mod.rs"]
pub mod debug_on_call;
#[path = "runtime/defvar_bool/mod.rs"]
pub mod defvar_bool;
#[path = "runtime/defvar_object/mod.rs"]
pub mod defvar_object;
#[path = "lisp/doc/mod.rs"]
pub mod doc;
#[path = "runtime/eval/mod.rs"]
pub mod eval;
#[path = "lisp/native/floatfns/mod.rs"]
pub mod floatfns;
#[path = "lisp/native/fns/mod.rs"]
pub mod fns;
#[path = "runtime/gc_stats/mod.rs"]
pub mod gc_stats;
#[path = "lisp/hook_runtime/mod.rs"]
pub mod hook_runtime;
#[path = "runtime/jit/mod.rs"]
pub mod jit;
#[path = "lisp/load/mod.rs"]
pub mod load;
#[path = "lisp/lread/mod.rs"]
pub mod lread;
#[path = "lisp/native/misc/mod.rs"]
pub mod misc;
#[path = "runtime/owned_roots/mod.rs"]
pub mod owned_roots;
#[path = "runtime/pdump/mod.rs"]
pub mod pdump;
#[path = "lisp/print/mod.rs"]
pub mod print;
#[path = "lisp/provide_coupled_vars/mod.rs"]
pub mod provide_coupled_vars;
#[path = "lisp/reader/mod.rs"]
pub mod reader;
#[path = "runtime/runtime_identity/mod.rs"]
pub mod runtime_identity;
#[path = "lisp/native/subr/mod.rs"]
pub mod subr;
#[path = "lisp/docs/subr_docs/mod.rs"]
pub mod subr_docs;
#[path = "lisp/native/subr/info.rs"]
pub mod subr_info;
#[path = "runtime/threads/mod.rs"]
pub mod threads;
#[path = "runtime/value_reader/mod.rs"]
pub mod value_reader;
#[path = "lisp/docs/var_docs/mod.rs"]
pub mod var_docs;

// The historical `crate::emacs_core::Name` roots the moved runtime tree
// names. Core's `emacs_core/mod.rs` used to provide these; the evaluator
// module tree is now their home.
pub use eval::Context;

// Scheduler lib tests still name the pre-split path
// `crate::emacs_core::process_hooks::select_handle_for_manager`. The function
// lives on the eval hook module; this alias is the route those call sites
// already use. cfg(test) only, so it is not a production surface.
#[cfg(test)]
pub(crate) mod process_hooks {
    pub(crate) use super::eval::process_hooks::select_handle_for_manager;
}
pub use print::{
    format_eval_result, format_eval_result_bytes_with_eval, format_eval_result_with_eval,
    print_value, print_value_bytes, print_value_bytes_with_eval, print_value_with_buffers,
    print_value_with_eval,
};

/// A menu-bar popup request's anchor, owned by the evaluator until the
/// display layer consumes it (moved down with the evaluator, stage-4 W5/E13).
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuBarPopupAnchor {
    pub request_id: Option<neomacs_display_protocol::menu::MenuBarRequestId>,
    pub frame_id: neovm_value::display_vocab::FrameId,
    pub menu_key: Option<String>,
    pub menu_x: i64,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}
