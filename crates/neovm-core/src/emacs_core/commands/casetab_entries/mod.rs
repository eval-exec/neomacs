//! Case-table Lisp entries that stay in the core command layer (stage-3b R0).
//!
//! The current/standard case-table entries and the derivations core consults
//! (canon/upcase tables, the current-buffer override) bind evaluator state:
//! the raw obarray legacy write, the green standard-table setter, the cached
//! standard table, the tagged-heap identity and the variable store.  Under R0
//! they stay core-side; the case-table engine and its thread-local cache stay
//! in the text layer and are reached through its helpers (the cache stays
//! private to text: two narrow cache-write helpers are the only surface).

use crate::emacs_core::casetab::{
    CaseTableOverride, cache_standard_case_table_heap, cache_standard_case_table_object,
    case_table_extra, current_case_table_for_buffer_in_state_with_symbols,
    ensure_case_table_derived_slots_with_symbols,
    ensure_standard_case_table_object_in_state_with_symbols, is_case_table_with_symbols,
    set_current_case_table_for_buffer_in_state, standard_case_table_object_symbol_id,
};
use crate::tagged::SymbolMode;
use neovm_value::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use neovm_value::value::*;

/// `(current-case-table)` -- evaluator-backed current buffer case table object.
pub(crate) fn builtin_current_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = ctx.symbol_mode();
    expect_args("current-case-table", &args, 0)?;
    current_case_table_for_context(ctx, mode)
}

/// `(standard-case-table)` -- evaluator-backed standard case table object.
pub(crate) fn builtin_standard_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = ctx.symbol_mode();
    expect_args("standard-case-table", &args, 0)?;
    ensure_standard_case_table_for_context(ctx, mode)
}

/// `(set-case-table TABLE)` -- evaluator-backed current buffer case table set.
pub(crate) fn builtin_set_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = ctx.symbol_mode();
    expect_args("set-case-table", &args, 1)?;
    if !is_case_table_with_symbols(&args[0], mode) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), args[0]],
        ));
    }
    let table = args[0];
    let _ = ensure_standard_case_table_for_context(ctx, mode)?;
    ensure_case_table_derived_slots_with_symbols(table, mode)?;
    set_current_case_table_for_buffer_in_state(&mut ctx.edit.buffers, table)?;
    Ok(table)
}

/// `(set-standard-case-table TABLE)` -- evaluator-backed standard table set.
pub(crate) fn builtin_set_standard_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mode = ctx.symbol_mode();
    expect_args("set-standard-case-table", &args, 1)?;
    if !is_case_table_with_symbols(&args[0], mode) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), args[0]],
        ));
    }
    cache_standard_case_table_object(args[0]);
    ctx.cached_standard_case_table = Some(args[0]);
    ctx.green_set_standard_case_table(args[0]);
    cache_standard_case_table_heap(ctx.tagged_heap().tagged_heap_identity());
    let table = args[0];
    ensure_case_table_derived_slots_with_symbols(table, mode)?;
    ctx.store_variable(standard_case_table_object_symbol_id(), table);
    Ok(table)
}

#[inline(always)]
fn ensure_standard_case_table_for_context(
    ctx: &mut crate::emacs_core::eval::Context,
    mode: SymbolMode,
) -> EvalResult {
    // SAFETY: lazy standard-table creation only reads/stores a variable value;
    // it cannot call Lisp or publish canonical dispatch metadata or raw storage.
    unsafe {
        ctx.obarray_view_mut().with_legacy_value_write(|obarray| {
            ensure_standard_case_table_object_in_state_with_symbols(obarray, mode)
        })
    }
}

#[inline(always)]
fn current_case_table_for_context(
    ctx: &mut crate::emacs_core::eval::Context,
    mode: SymbolMode,
) -> Result<Value, Flow> {
    let (buffers, mut obarray_view) = ctx.buffers_and_obarray_view_mut();
    // SAFETY: this noncalling phase selects/seeds case-table Values and buffer
    // slots, without changing function/debugger metadata or escaping a loan.
    unsafe {
        obarray_view.with_legacy_value_write(|obarray| {
            current_case_table_for_buffer_in_state_with_symbols(obarray, buffers, mode)
        })
    }
}

pub(crate) fn sync_current_buffer_case_table_state(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<(), Flow> {
    let mode = ctx.symbol_mode();
    let _ = current_case_table_for_context(ctx, mode)?;
    Ok(())
}

pub(crate) fn current_case_canon_table(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<Value, Flow> {
    let mode = ctx.symbol_mode();
    let table = current_case_table_for_context(ctx, mode)?;
    ensure_case_table_derived_slots_with_symbols(table, mode)?;
    Ok(case_table_extra(table, 1))
}

/// Resolve GNU Fupcase's actual upcase table, including explicit changes to
/// the standard table. The caller keeps a call-local, read-only projection;
/// this adds no cache or Lisp state shared between independently owned Contexts.
pub(crate) fn current_case_upcase_table(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<Value, Flow> {
    let mode = ctx.symbol_mode();
    let table = current_case_table_for_context(ctx, mode)?;
    // GNU prepare_casing_context only rebuilds a modified table when the
    // canonicalize slot is nil, so ordinary comparisons do not write tables.
    if case_table_extra(table, 1).is_nil() {
        ensure_case_table_derived_slots_with_symbols(table, mode)?;
    }
    Ok(case_table_extra(table, 0))
}

/// Resolve the override for the current buffer, mirroring GNU's use of the
/// per-buffer downcase/upcase/canon char-tables (`buffer.h:1648-1663`,
/// `editfns.c:4440`).
pub(crate) fn case_table_override_for_current_buffer(
    ctx: &mut super::eval::Context,
) -> Result<CaseTableOverride, Flow> {
    let mode = ctx.symbol_mode();
    // Without a current buffer there is no buffer-local case table; behave
    // like GNU's default (the standard table) and use the hardwired path.
    if ctx.edit.buffers.current_buffer_id().is_none() {
        return Ok(CaseTableOverride::none());
    }
    let standard = ensure_standard_case_table_for_context(ctx, mode)?;
    let table = current_case_table_for_context(ctx, mode)?;
    // The standard table's explicit entries already match the hardwired
    // ASCII path; skip the override entirely so we stay byte-identical.
    if table.bits() == standard.bits() {
        return Ok(CaseTableOverride::Standard);
    }
    // A custom table is installed: make sure the up/canon subsidiary tables
    // exist (GNU `set_case_table` recomputes them lazily via extras[1] nil).
    ensure_case_table_derived_slots_with_symbols(table, mode)?;
    Ok(CaseTableOverride::from_installed_tables(
        table,
        case_table_extra(table, 0),
        case_table_extra(table, 1),
        mode,
    ))
}

/// The one canonical case-table table (peer-ruled PC2 shape). The authority
/// stays here because it binds the raw obarray legacy write, the green
/// standard-table setter, the cached standard table and the tagged-heap
/// identity (stage-3b R0/T5); eval reaches it through
/// `eval::case_tables`, installed at the two construction seams.
static CANONICAL_CASE_TABLE_HOOKS: crate::emacs_core::eval::case_tables::CaseTableHooks =
    crate::emacs_core::eval::case_tables::CaseTableHooks::new(
        current_case_canon_table,
        current_case_upcase_table,
        sync_current_buffer_case_table_state,
        standard_case_table_for_admission,
        case_table_override_for_current_buffer,
    );

/// Admission needs the standard table installed; the Lisp entry's error is
/// ignored exactly as before (this runs at admission, after construction).
fn standard_case_table_for_admission(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<Value, Flow> {
    builtin_standard_case_table(ctx, Vec::new())
}

/// The one canonical case-table descriptor (published only by the one core
/// boot entry, peer follow-up F2).
pub(crate) fn canonical_case_table_hooks()
-> &'static crate::emacs_core::eval::case_tables::CaseTableHooks {
    &CANONICAL_CASE_TABLE_HOOKS
}
