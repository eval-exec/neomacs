//! Elisp interpreter module.
//!
//! Provides a full Elisp evaluator with:
//! - Value types: nil, t, int, float, string, symbol, keyword, char, cons, vector, hash-table
//! - Complete parser: strings, floats, chars, vectors, dotted pairs, quasiquote, reader macros
//! - Special forms: quote, function, let, let*, setq, if, and, or, cond, while, progn, prog1,
//!   lambda, defun, defvar, defconst, defmacro, funcall, catch, throw, unwind-protect,
//!   condition-case, when, unless
//! - 100+ built-in functions: arithmetic, comparisons, type predicates, list ops, string ops,
//!   vector ops, hash tables, higher-order functions, conversion, property lists

// This file is the stable module-path seam. Physical ownership is grouped by
// domain below while callers continue to use `crate::emacs_core::<module>`.

// Command input and invocation.
#[path = "commands/abbrev/mod.rs"]
pub mod abbrev;
#[path = "commands/command_observation/mod.rs"]
pub(crate) mod command_observation;
#[path = "commands/interactive/mod.rs"]
pub mod interactive;
#[path = "commands/kbd/mod.rs"]
pub mod kbd;
#[path = "commands/keyboard/mod.rs"]
pub mod keyboard;
#[path = "commands/keymap/mod.rs"]
pub mod keymap;
#[path = "commands/kmacro/mod.rs"]
pub mod kmacro;
#[path = "commands/minibuffer/mod.rs"]
pub mod minibuffer;
#[path = "commands/prefix/mod.rs"]
pub(crate) mod prefix;
#[path = "commands/register/mod.rs"]
pub mod register;

// Display model, redisplay, and graphical/terminal surfaces.
#[path = "display/chrome_dirty/mod.rs"]
pub mod chrome_dirty;
#[path = "display/display/mod.rs"]
pub mod display;
#[path = "display/display_host/mod.rs"]
pub mod display_host;
#[path = "display/display_spec/mod.rs"]
pub mod display_spec;
#[path = "display/dispnew/mod.rs"]
pub mod dispnew;
#[path = "display/effect_profile/mod.rs"]
pub mod effect_profile;
#[path = "display/font/mod.rs"]
pub mod font;
#[path = "display/fontset/mod.rs"]
pub mod fontset;
#[path = "display/frame/mod.rs"]
pub mod frame;
#[path = "display/frame_vars/mod.rs"]
pub mod frame_vars;
#[path = "display/hscroll/mod.rs"]
pub mod hscroll;
#[path = "display/image/mod.rs"]
pub mod image;
#[path = "display/image_catalog/mod.rs"]
pub mod image_catalog;
#[path = "display/image_path/mod.rs"]
pub mod image_path;
#[path = "display/invisibility/mod.rs"]
pub mod invisibility;
#[path = "display/neo/mod.rs"]
pub(crate) mod neo;
#[path = "display/shader_surface/mod.rs"]
pub mod shader_surface;
#[path = "display/sound/mod.rs"]
pub(crate) mod sound;
#[path = "display/terminal/mod.rs"]
pub mod terminal;
#[path = "display/video/mod.rs"]
pub mod video;
#[path = "display/window_cmds/mod.rs"]
pub mod window_cmds;
#[path = "display/window_edge_drag/mod.rs"]
pub mod window_edge_drag;
#[path = "display/xdisp/mod.rs"]
pub mod xdisp;
#[path = "display/xfaces/mod.rs"]
pub mod xfaces;
#[path = "display/xwidget/mod.rs"]
pub mod xwidget;

// Editor state and editing operations.
#[path = "editing/bookmark/mod.rs"]
pub mod bookmark;
#[path = "editing/buffer/mod.rs"]
pub mod buffer;
#[path = "editing/buffer_vars/mod.rs"]
pub mod buffer_vars;
#[path = "editing/dired/mod.rs"]
pub mod dired;
#[path = "editing/editfns/mod.rs"]
pub mod editfns;
#[path = "editing/indent/mod.rs"]
pub mod indent;
#[path = "editing/marker/mod.rs"]
pub mod marker;
#[path = "editing/mode/mod.rs"]
pub mod mode;
#[path = "editing/navigation/mod.rs"]
pub mod navigation;
#[path = "editing/rect/mod.rs"]
pub mod rect;
#[path = "editing/undo/mod.rs"]
pub mod undo;

// Lisp loading, documentation, and language-level facilities.
#[path = "lisp/advice/mod.rs"]
pub mod advice;
#[path = "lisp/autoload/mod.rs"]
pub mod autoload;
#[path = "lisp/cl_lib/mod.rs"]
pub mod cl_lib;
#[path = "lisp/custom/mod.rs"]
pub mod custom;
#[path = "lisp/doc/mod.rs"]
pub mod doc;
#[path = "lisp/hook_runtime/mod.rs"]
pub(crate) mod hook_runtime;
#[path = "lisp/load/mod.rs"]
pub mod load;
#[path = "lisp/lread/mod.rs"]
pub mod lread;
#[path = "lisp/print/mod.rs"]
pub mod print;
#[path = "lisp/provide_coupled_vars/mod.rs"]
pub mod provide_coupled_vars;
#[path = "lisp/reader/mod.rs"]
pub mod reader;

// Native Lisp subroutines and their metadata.
#[path = "lisp/native/builtins/mod.rs"]
pub mod builtins;
#[path = "lisp/native/builtins_extra/mod.rs"]
pub mod builtins_extra;
#[path = "lisp/native/floatfns/mod.rs"]
pub mod floatfns;
#[path = "lisp/native/fns/mod.rs"]
pub mod fns;
#[path = "lisp/native/misc/mod.rs"]
pub mod misc;
#[path = "lisp/native/subr/mod.rs"]
pub(crate) mod subr;
#[cfg(doctest)]
/// Compile-time contracts for native subroutine declarations.
///
/// This facade exists only while rustdoc compiles the examples below. Normal
/// builds keep the declaration machinery crate-private.
///
/// A two-slot constructor cannot accept a one-slot Rust entrypoint:
///
/// ```compile_fail,E0308
/// use neovm_core::{Context, Value};
/// use neovm_core::emacs_core::subr_compile_contract::{FixedMin2, SubrSpec};
/// use neovm_core::emacs_core::error::EvalResult;
///
/// fn one(_ctx: &mut Context, value: Value) -> EvalResult {
///     Ok(value)
/// }
///
/// const BAD: SubrSpec = SubrSpec::fixed2("bad", one, FixedMin2::One);
/// ```
///
/// A minimum for a different fixed-slot width is a different Rust type:
///
/// ```compile_fail,E0308
/// use neovm_core::{Context, Value};
/// use neovm_core::emacs_core::subr_compile_contract::{
///     FixedMin2, FixedMin3, SubrSpec,
/// };
/// use neovm_core::emacs_core::error::EvalResult;
///
/// fn two(_ctx: &mut Context, left: Value, _right: Value) -> EvalResult {
///     Ok(left)
/// }
///
/// const BAD: SubrSpec = SubrSpec::fixed2("bad", two, FixedMin3::Two);
/// ```
///
/// A localized declaration batch cannot be declared outside `subrs.rs`:
///
/// ```compile_fail,E0080
/// use neovm_core::{Context, Value};
/// use neovm_core::emacs_core::subr_compile_contract::{SubrBatch, SubrSpec};
/// use neovm_core::emacs_core::error::EvalResult;
///
/// fn zero(_ctx: &mut Context) -> EvalResult {
///     Ok(Value::NIL)
/// }
///
/// const BAD: SubrBatch = SubrBatch::new(
///     module_path!(),
///     &[SubrSpec::fixed0("bad", zero)],
/// );
/// ```
#[doc(hidden)]
pub mod subr_compile_contract {
    pub use super::subr::{FixedMin1, FixedMin2, FixedMin3, SubrBatch, SubrSpec};
}
#[path = "lisp/native/subr/info.rs"]
pub mod subr_info;

// Generated Lisp-facing documentation tables.
#[path = "lisp/docs/subr_docs/mod.rs"]
pub mod subr_docs;
#[path = "lisp/docs/var_docs/mod.rs"]
pub mod var_docs;

// Evaluator runtime, object model, VM, and persistence.
#[path = "runtime/alloc/mod.rs"]
pub mod alloc;
#[path = "runtime/bytecode/mod.rs"]
pub mod bytecode;
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
#[path = "runtime/error/mod.rs"]
pub mod error;
#[path = "runtime/errors/mod.rs"]
pub mod errors;
#[path = "runtime/eval/mod.rs"]
pub mod eval;
#[path = "runtime/forward/mod.rs"]
pub mod forward;
#[path = "runtime/gc_stats/mod.rs"]
pub mod gc_stats;
#[path = "runtime/hashtab/mod.rs"]
pub mod hashtab;
#[path = "runtime/intern/mod.rs"]
pub mod intern;
#[path = "runtime/jit/mod.rs"]
pub mod jit;
#[path = "runtime/pdump/mod.rs"]
pub mod pdump;
#[path = "runtime/plist/mod.rs"]
pub mod plist;
#[path = "runtime/position/mod.rs"]
pub(crate) mod position;
#[path = "runtime/runtime_identity/mod.rs"]
pub(crate) mod runtime_identity;
#[path = "runtime/symbol/mod.rs"]
pub mod symbol;
#[path = "runtime/threads/mod.rs"]
pub mod threads;
#[path = "runtime/value/mod.rs"]
pub mod value;
#[path = "runtime/value_reader/mod.rs"]
pub mod value_reader;

// Host integration, files, processes, networking, and platform support.
#[path = "system/platform/c_features/mod.rs"]
pub mod c_features;
#[path = "system/callproc/mod.rs"]
pub mod callproc;
#[path = "system/platform/cus_start_platform_vars/mod.rs"]
pub mod cus_start_platform_vars;
#[path = "system/dynamic_module/mod.rs"]
pub mod dynamic_module;
#[path = "system/environment/mod.rs"]
pub(crate) mod environment;
#[path = "system/fileio/mod.rs"]
pub mod fileio;
#[path = "system/filelock/mod.rs"]
pub mod filelock;
#[path = "system/network/mod.rs"]
pub mod network;
#[path = "system/os_signal/mod.rs"]
pub(crate) mod os_signal;
#[path = "system/path_exec/mod.rs"]
pub mod path_exec;
#[path = "system/perf_trace/mod.rs"]
pub mod perf_trace;
#[path = "system/post_image_init/mod.rs"]
pub(crate) mod post_image_init;
#[path = "system/process/mod.rs"]
pub mod process;
#[path = "system/profiler/mod.rs"]
pub(crate) mod profiler;
#[path = "system/shell_file_name/mod.rs"]
pub(crate) mod shell_file_name;
#[path = "system/sqlite/mod.rs"]
pub(crate) mod sqlite;
#[path = "system/timefns/mod.rs"]
pub mod timefns;
#[path = "system/timer/mod.rs"]
pub mod timer;
#[path = "system/tls/mod.rs"]
pub(crate) mod tls;
#[path = "system/platform/w32/mod.rs"]
pub(crate) mod w32;
#[path = "system/wait/mod.rs"]
pub(crate) mod wait;

// Character representation, coding, syntax, search, and structured text.
#[path = "text/casefiddle/mod.rs"]
pub mod casefiddle;
#[path = "text/casetab/mod.rs"]
pub mod casetab;
#[path = "text/category/mod.rs"]
pub mod category;
#[path = "text/ccl/mod.rs"]
pub mod ccl;
#[path = "text/character/mod.rs"]
pub mod character;
#[path = "text/charset/mod.rs"]
pub mod charset;
#[path = "text/chartable/mod.rs"]
pub mod chartable;
#[path = "text/coding/mod.rs"]
pub mod coding;
#[path = "text/composite/mod.rs"]
pub mod composite;
#[path = "text/emacs_char/mod.rs"]
pub mod emacs_char;
#[path = "text/format/mod.rs"]
pub mod format;
#[path = "text/json/mod.rs"]
pub mod json;
#[path = "text/regex/mod.rs"]
pub mod regex;
#[path = "text/regex/emacs.rs"]
pub mod regex_emacs;
#[path = "text/search/mod.rs"]
pub mod search;
#[path = "text/string_escape/mod.rs"]
pub(crate) mod string_escape;
#[path = "text/syntax/mod.rs"]
pub mod syntax;
#[path = "text/textprop/mod.rs"]
pub mod textprop;
#[path = "text/treesit/mod.rs"]
pub mod treesit;
#[path = "text/xml/mod.rs"]
pub mod xml;
#[path = "text/zlib/mod.rs"]
pub(crate) mod zlib;

// Cross-subsystem and externally-shaped regression suites retain the
// `emacs_core` lexical parent while living beside the subsystem they cover.
#[cfg(test)]
#[path = "system/platform/c_features/tests/surface.rs"]
mod c_features_test;
#[cfg(test)]
#[path = "tests/compat_regressions/mod.rs"]
pub mod compat_regressions;
#[cfg(test)]
#[path = "tests/build_support/compile_main_rule.rs"]
mod compile_main_rule_test;
#[cfg(test)]
#[path = "tests/build_support/generated_lisp.rs"]
mod generated_lisp_test;
#[cfg(test)]
#[path = "tests/gnu_surface/defvar_special.rs"]
mod gnu_defvar_special_test;
#[cfg(test)]
#[path = "tests/gnu_surface/subr.rs"]
mod gnu_subr_surface_test;
#[cfg(test)]
#[path = "editing/undo/tests/kill_ring.rs"]
mod kill_ring_test;
#[cfg(test)]
#[path = "tests/architecture/layout.rs"]
mod layout_test;
#[cfg(test)]
#[path = "lisp/provide_coupled_vars/tests/runtime_surface.rs"]
mod provide_coupled_vars_test;
#[cfg(test)]
#[path = "runtime/eval/tests/quit_regression.rs"]
mod quit_regression_test;
#[cfg(test)]
#[path = "tests/architecture/runtime_string_guard.rs"]
mod runtime_string_guard_test;
#[cfg(test)]
#[path = "display/shader_surface/tests/runtime.rs"]
mod shader_surface_test;
#[cfg(test)]
#[path = "lisp/load/tests/stale_bytecode.rs"]
mod stale_bytecode_test;
#[cfg(test)]
#[path = "runtime/symbol/tests/function_regression.rs"]
mod symbol_function_regression_test;
#[cfg(test)]
#[path = "runtime/symbol/tests/plist_regression.rs"]
mod symbol_plist_regression_test;
#[cfg(test)]
#[path = "runtime/symbol/tests/redirect_regression.rs"]
mod symbol_redirect_regression_test;
#[cfg(test)]
#[path = "text/syntax/tests/category_property.rs"]
mod syntax_category_property_test;
#[cfg(test)]
#[path = "text/syntax/tests/gnu_parity_regression.rs"]
mod syntax_gnu_parity_regression_test;
#[cfg(test)]
#[path = "system/tls/tests/runtime.rs"]
mod tls_test;
#[cfg(test)]
#[path = "display/video/tests/runtime.rs"]
mod video_test;
#[cfg(test)]
#[path = "display/window_cmds/tests/window_system_preload.rs"]
mod window_system_preload_test;
#[cfg(test)]
#[path = "display/xwidget/tests/runtime.rs"]
mod xwidget_test;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuBarPopupAnchor {
    pub(crate) frame_id: crate::window::FrameId,
    pub(crate) menu_key: Option<String>,
    pub(crate) menu_x: i64,
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: i64,
    pub(crate) height: i64,
}

// Re-export the main public API
pub use bytecode::{ByteCodeFunction, Vm as ByteVm};
pub use display_host::{DisplayHost, GraphicalFaceAttribute, SelectionOwner};
pub use error::{
    EvalError, format_eval_result, format_eval_result_bytes_with_eval,
    format_eval_result_with_eval, print_value_bytes_with_eval, print_value_with_eval,
};
pub use eval::{
    Context, GuiFrameHostRequest, MenuBarRebuildGeneration, PopupMenuEntry, PopupMenuRequest,
};
pub use intern::SymId;
pub use print::{print_value, print_value_bytes, print_value_with_buffers};
pub use symbol::Obarray;
pub use value::{LambdaData, LambdaParams, Value, ValueKind, VecLikeType};

/// Convenience: parse and evaluate source code, returning the last form's value.
pub fn eval_source(input: &str) -> Result<Value, EvalError> {
    let mut evaluator = Context::new();
    evaluator.eval_str(input)
}
