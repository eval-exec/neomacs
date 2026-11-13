//! Callback heat for `mapc`/`mapcar` callers.
//!
//! A caller's heat counts its entries, but the work it drives through a
//! mapping builtin runs in the callback, so a function that maps a long list
//! once per call stays cold while its callback runs millions of times. When
//! the caller has a site the default front's HOF-only path would select
//! (`compile::hof_caller_selectable`), the builtin credits the callbacks it
//! ran as caller heat, and the caller tiers up at its next entry with the
//! callback loop inlined.
//!
//! Threading: each mutator reads only its own specpdl; the credit is one
//! relaxed heat update per mapping call and the per-source bit is a relaxed
//! cached answer, so N mutators never contend on a per-element line.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use neovm_value::value::Value;

/// Callbacks worth one entry of caller heat.
const CALLBACKS_PER_HEAT: usize = 32;

/// Specpdl entries searched for the caller's backtrace frame.
const CALLER_SCAN: usize = 8;

/// After a `mapc`/`mapcar` call ran `callbacks` callbacks: credit them to
/// the calling bytecode function, if it can profit. A short mapping or a
/// disabled JIT returns before any lookup.
#[inline]
pub(crate) fn note_mapping(eval: &Context, callbacks: usize) {
    if callbacks >= CALLBACKS_PER_HEAT && super::jit_runtime_enabled() {
        credit_caller(eval, callbacks);
    }
}

#[cold]
#[inline(never)]
fn credit_caller(eval: &Context, callbacks: usize) {
    let mapping = |function: Value| {
        function
            .as_bare_symbol_id()
            .is_some_and(|id| matches!(neovm_value::intern::resolve_sym(id), "mapc" | "mapcar"))
    };
    let Some(function) =
        eval.innermost_backtrace_function_where(CALLER_SCAN, |function| !mapping(function))
    else {
        return;
    };
    let function = match function.as_bare_symbol_id() {
        Some(symbol) => match eval.obarray.symbol_function_id(symbol) {
            Some(cell) => cell,
            None => return,
        },
        None => function,
    };
    let Some(caller) = function.get_bytecode_data() else {
        return;
    };
    let runtime = caller.jit_runtime();
    // A caller already through a compile keeps its verdict; re-tiering it is
    // a separate decision. The per-source answer is cached, a negative one
    // too: it is computed only from inside the caller's own mapping, after
    // that callback ran at least `CALLBACKS_PER_HEAT` times, so the site's
    // callback is no longer cold, and its numeric feedback only narrows.
    if runtime.compiled_id().is_some()
        || !runtime.hof_caller(|| super::compile::hof_caller_selectable(caller))
    {
        return;
    }
    let cap = (super::hot_threshold() / 2).max(1);
    let credit = u32::try_from(callbacks / CALLBACKS_PER_HEAT)
        .unwrap_or(u32::MAX)
        .min(cap);
    runtime.note_callback_work(credit);
}
