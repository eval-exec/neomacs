//! JIT dispatch and control-flow shims: the direct-builtin dispatch tables, the spec-call C-ABI shims (call_spec, call_subr_spec, pred/arith/cbsym spec, named builtin), and the control-flow shims (throw, save-excursion/restriction, unwind-protect, catch/handler, switch, backedge).
//!
//! Moved out of `compile.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;
use crate::emacs_core::error::FlowKind;
use crate::emacs_core::eval::AttentionMask;
#[cfg(feature = "jit")]
use crate::tagged::header::{SubrFn0, SubrFn1, SubrFn2, SubrFn3, SubrFn4, SubrFn5};

pub(crate) type JitBuiltin1 = fn(&mut Context, Value) -> Result<Value, Flow>;
pub(crate) type JitBuiltin2 = fn(&mut Context, Value, Value) -> Result<Value, Flow>;
pub(crate) type JitBuiltin3 = fn(&mut Context, Value, Value, Value) -> Result<Value, Flow>;

pub(crate) use crate::emacs_core::builtins as b;

pub(crate) static JIT_BUILTIN1: [JitBuiltin1; 4] = [
    b::builtin_length_1,          // 0
    b::builtin_symbol_value_1,    // 1
    b::builtin_symbol_function_1, // 2
    b::builtin_nreverse_1,        // 3
];

pub(crate) static JIT_BUILTIN2: [JitBuiltin2; 15] = [
    b::builtin_nth_2,          // 0
    b::builtin_nthcdr_2,       // 1
    b::builtin_elt_2,          // 2
    b::builtin_member_2,       // 3
    b::builtin_memq_2,         // 4
    b::builtin_assq_2,         // 5
    b::builtin_equal_2,        // 6
    b::builtin_setcar_2,       // 7
    b::builtin_setcdr_2,       // 8
    b::builtin_aref_2,         // 9
    b::builtin_set_2,          // 10
    b::builtin_fset_2,         // 11
    b::builtin_get_2,          // 12
    b::builtin_string_equal_2, // 13
    b::builtin_string_lessp_2, // 14
];

/// A direct builtin that needs only a SHARED borrow of the evaluator.
///
/// Holding `&Context` rather than `&mut Context` is the whole contract: a GC
/// safe point (`maybe_gc`), a quit poll (`maybe_quit`, which can sample the
/// profiler and drain OS signals) and any call back into Lisp all require
/// `&mut Context`, so a function of this type cannot collect, cannot run
/// Lisp and cannot start a nested native activation — checked by the
/// compiler, not by a comment. On its success path it only reads and mutates
/// heap objects it was handed (`setcar`'s store keeps its write barrier), and
/// its error payload is an in-flight `Flow` that roots itself.
///
/// So the shim calls such an entry with no scratch roots, and the call site
/// roots no residual stack (a value live across the call cannot be moved or
/// freed, because no collection can run).
pub(crate) type JitBuiltin2Pure = fn(&Context, Value, Value) -> Result<Value, Flow>;

fn pure_nth(_: &Context, n: Value, list: Value) -> Result<Value, Flow> {
    b::bytecode_nth_values(n, list)
        .map_err(|flow| flow.with_bytecode_error_backtrace(&Op::Nth, &[n, list]))
}
fn pure_nthcdr(_: &Context, n: Value, list: Value) -> Result<Value, Flow> {
    b::builtin_nthcdr_values(n, list)
}
fn pure_elt(context: &Context, sequence: Value, n: Value) -> Result<Value, Flow> {
    b::bytecode_elt_values_in_state(context, sequence, n)
        .map_err(|flow| flow.with_bytecode_error_backtrace(&Op::Elt, &[sequence, n]))
}
fn pure_member(ctx: &Context, target: Value, list: Value) -> Result<Value, Flow> {
    b::builtin_member_values(target, list, ctx.symbols_with_pos_enabled)
}
fn pure_memq(ctx: &Context, target: Value, list: Value) -> Result<Value, Flow> {
    b::builtin_memq_values(target, list, ctx.symbols_with_pos_enabled)
}
fn pure_assq(ctx: &Context, key: Value, list: Value) -> Result<Value, Flow> {
    b::builtin_assq_values(key, list, ctx.symbols_with_pos_enabled)
}
fn pure_equal(ctx: &Context, a: Value, b_: Value) -> Result<Value, Flow> {
    Ok(Value::bool_val(
        crate::emacs_core::value::try_equal_value_swp(&a, &b_, 0, ctx.symbols_with_pos_enabled)?,
    ))
}
fn pure_setcar(_: &Context, cons: Value, new_car: Value) -> Result<Value, Flow> {
    b::builtin_setcar_values(cons, new_car)
        .map_err(|flow| flow.with_bytecode_error_backtrace(&Op::Setcar, &[cons, new_car]))
}
fn pure_setcdr(_: &Context, cons: Value, new_cdr: Value) -> Result<Value, Flow> {
    b::builtin_setcdr_values(cons, new_cdr)
        .map_err(|flow| flow.with_bytecode_error_backtrace(&Op::Setcdr, &[cons, new_cdr]))
}
fn pure_aref(ctx: &Context, array: Value, index: Value) -> Result<Value, Flow> {
    b::builtin_aref_values_in_state(ctx, array, index)
        .map_err(|flow| flow.with_bytecode_error_backtrace(&Op::Aref, &[array, index]))
}
fn pure_get(ctx: &Context, symbol: Value, prop: Value) -> Result<Value, Flow> {
    Ok(b::symbol_property_get(ctx, symbol, prop)?
        .1
        .unwrap_or(Value::NIL))
}

/// The unary twin of [`JitBuiltin2Pure`]: same contract.
pub(crate) type JitBuiltin1Pure = fn(&Context, Value) -> Result<Value, Flow>;

fn pure_length(_: &Context, sequence: Value) -> Result<Value, Flow> {
    b::builtin_length_value(sequence)
}
fn pure_symbol_value(ctx: &Context, symbol_value: Value) -> Result<Value, Flow> {
    // `builtin_symbol_value_1`: the void-variable error reports the argument
    // as given (a symbol-with-pos keeps its wrapper).
    let symbol = b::expect_symbol_id_checked(&symbol_value, ctx.symbols_with_pos_enabled)?;
    match ctx.visible_runtime_variable_value_by_id(symbol)? {
        Some(value) => Ok(value),
        None => Err(signal(
            crate::emacs_core::error::LispCondition::VoidVariable,
            vec![symbol_value],
        )),
    }
}
fn pure_symbol_function(ctx: &Context, symbol_value: Value) -> Result<Value, Flow> {
    let symbol = b::expect_symbol_id_checked(&symbol_value, ctx.symbols_with_pos_enabled)?;
    b::symbol_function_by_id(ctx.obarray(), symbol)
}
fn pure_nreverse(_: &Context, arg: Value) -> Result<Value, Flow> {
    b::nreverse_value(arg)
}

/// [`JIT_BUILTIN1`] entries that are [`JitBuiltin1Pure`], index for index.
/// `jit_builtin1_pure_matches_the_table` holds every entry to the rooted
/// builtin's answers.
pub(crate) static JIT_BUILTIN1_PURE: [Option<JitBuiltin1Pure>; 4] = [
    Some(pure_length),          // 0
    Some(pure_symbol_value),    // 1
    Some(pure_symbol_function), // 2
    Some(pure_nreverse),        // 3
];

/// [`JIT_BUILTIN2`] entries that are [`JitBuiltin2Pure`], index for index.
/// `set` runs variable watchers and `fset` redefines functions, so both keep
/// the rooted path; `string=`/`string<` go through `typed_subr!`'s argument
/// conversion and are not claimed here. `jit_builtin2_pure_matches_the_table`
/// holds every entry to the rooted builtin's answers.
pub(crate) static JIT_BUILTIN2_PURE: [Option<JitBuiltin2Pure>; 15] = [
    Some(pure_nth),    // 0
    Some(pure_nthcdr), // 1
    Some(pure_elt),    // 2
    Some(pure_member), // 3
    Some(pure_memq),   // 4
    Some(pure_assq),   // 5
    Some(pure_equal),  // 6
    Some(pure_setcar), // 7
    Some(pure_setcdr), // 8
    Some(pure_aref),   // 9
    None,              // 10 set
    None,              // 11 fset
    Some(pure_get),    // 12
    None,              // 13 string=
    None,              // 14 string<
];

pub(crate) static JIT_BUILTIN3: [JitBuiltin3; 1] = [
    b::builtin_put_3, // 0
];

/// Slice-shaped builtins (`fn(&[Value]) -> EvalResult`, no Context) — the
/// exact functions the interpreter's `Nconc`/`Concat`/`Substring` arms call.
pub(crate) type JitBuiltinSlice = fn(&[Value]) -> Result<Value, Flow>;

pub(crate) static JIT_BUILTIN_SLICE: [JitBuiltinSlice; 3] = [
    b::builtin_nconc_slice_values, // 0
    b::builtin_concat_slice,       // 1
    b::builtin_substring_slice,    // 2
];

/// `(nargs, table_index)` for ops lowered through the slice-builtin shim.
/// `Concat`'s arity rides in the opcode; `Nconc`/`Substring` are fixed.
pub(crate) fn slice_builtin_spec(op: &Op) -> Option<(usize, usize)> {
    Some(match op {
        Op::Nconc => (2, 0),
        Op::Concat(n) => (*n as usize, 1),
        Op::Substring => (3, 2),
        _ => return None,
    })
}

/// `(table_arity, table_index)` for ops lowered through the generic
/// direct-builtin shims. (There is no longer a per-op "mutates" flag: every op
/// that needs runtime re-entry already sets `needs_rt`, and these ops always
/// route through the precise-deopt path, so there is nothing to poison.)
pub(crate) fn direct_builtin_spec(op: &Op) -> Option<(u8, usize)> {
    Some(match op {
        Op::Length => (1, 0),
        Op::SymbolValue => (1, 1),
        Op::SymbolFunction => (1, 2),
        Op::Nreverse => (1, 3),
        Op::Nth => (2, 0),
        Op::Nthcdr => (2, 1),
        Op::Elt => (2, 2),
        Op::Member => (2, 3),
        Op::Memq => (2, 4),
        Op::Assq => (2, 5),
        Op::Equal => (2, 6),
        Op::Setcar => (2, 7),
        Op::Setcdr => (2, 8),
        Op::Aref => (2, 9),
        Op::Set => (2, 10),
        Op::Fset => (2, 11),
        Op::Get => (2, 12),
        Op::StringEqual => (2, 13),
        Op::StringLessp => (2, 14),
        Op::Put => (3, 0),
        _ => return None,
    })
}

/// Call a unary direct builtin (`JIT_BUILTIN1[idx]`) — the identical function
/// the interpreter arm calls. Roots the argument across the call (builtins may
/// GC); the generated code rooted the rest of its frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin1(ctx: *mut u8, idx: i64, a: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        if let Some(pure) = JIT_BUILTIN1_PURE[idx as usize] {
            // No collection can run (see `JitBuiltin2Pure`): no scratch roots.
            // SAFETY: see neovm_jit_call's function-level contract.
            let ctx = unsafe { &*(ctx as *const Context) };
            return match pure(ctx, a) {
                Ok(value) => {
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = value.bits() as i64 };
                    STATUS_OK
                }
                Err(flow) => {
                    stash_pending_flow(flow);
                    STATUS_SIGNAL
                }
            };
        }
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN1[idx as usize](ctx, a) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Binary variant of [`neovm_jit_builtin1`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin2(ctx: *mut u8, idx: i64, a: i64, b: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let b = Value::from_bits(b as usize);
        if let Some(pure) = JIT_BUILTIN2_PURE[idx as usize] {
            // No collection can run (see `JitBuiltin2Pure`): no scratch roots.
            // SAFETY: see neovm_jit_call's function-level contract.
            let ctx = unsafe { &*(ctx as *const Context) };
            return match pure(ctx, a, b) {
                Ok(value) => {
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = value.bits() as i64 };
                    STATUS_OK
                }
                Err(flow) => {
                    stash_pending_flow(flow);
                    STATUS_SIGNAL
                }
            };
        }
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        push_scratch_gc_root(b);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN2[idx as usize](ctx, a, b) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// A return word of the value-returning array shims ([`neovm_jit_aref`],
/// [`neovm_jit_aset`]): a non-local exit was stashed with
/// [`stash_pending_flow`], so take the site's signal edge. Every other word
/// such a shim returns is the result's own bits, and no Lisp value carries
/// tag `0b001`, so one tag test tells the two apart.
pub const VALUE_SHIM_SIGNAL: i64 = 0b0001;
const _: () = {
    use crate::tagged::value::{TAG_FLOAT, TAG_VECLIKE};
    let tag = VALUE_SHIM_SIGNAL as usize & TAG_MASK;
    assert!(tag & FIXNUM_CHECK_MASK != FIXNUM_CHECK_VALUE);
    assert!(tag != TAG_SYMBOL && tag != TAG_CONS && tag != TAG_STRING);
    assert!(tag != TAG_VECLIKE && tag != TAG_FLOAT);
};

// `(aref ARRAY INDEX)` on the shapes a loop indexes: the one fast path both
// `Baref` entries share (`builtins::aref_fast`; run_loop reaches it through
// `builtin_aref_values_in_vm_context_slot`). `None` for everything else,
// which `builtin_aref_values` then answers (or signals) exactly as before.
use b::aref_fast;

#[cfg(test)]
#[path = "tests/array_projection_capture_test.rs"]
mod array_projection_capture;

/// `Op::Aref` (GNU `Baref`) from compiled code: the element's bits, or
/// [`VALUE_SHIM_SIGNAL`]. `builtin2`'s table dispatch, `Result` and result
/// slot cost ~50 instructions on top of the builtin's own decode — dhrystone
/// indexes 90M times. GC-free on every path, like the pure table entry it
/// replaces at the site, so the site roots nothing.
/// SAFETY: `ctx` is only handed to the panic containment (vmctx contract of
/// [`neovm_jit_call`]).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_aref(ctx: *mut u8, array: i64, index: i64) -> i64 {
    #[cfg(test)]
    AREF_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let array = Value::from_bits(array as usize);
    let index = Value::from_bits(index as usize);
    match aref_fast(array, index) {
        Some(value) => value.bits() as i64,
        None => aref_slow(ctx, array, index),
    }
}

#[cfg(test)]
thread_local! {
    /// Test hook: how many times compiled code called `neovm_jit_aref`.
    pub(crate) static AREF_SHIM_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Test hook: how many times compiled code called `neovm_jit_aset`.
    pub(crate) static ASET_SHIM_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// Test hook: how many times compiled code called `neovm_jit_memq` or
    /// `neovm_jit_assq` (a search its inline path left to the shim).
    pub(crate) static LIST_SEARCH_SHIM_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// Test hook: how many times compiled code called `neovm_jit_setcar` or
    /// `neovm_jit_setcdr` (a store its inline path left to the shim).
    pub(crate) static LIST_STORE_SHIM_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// Test hook: exact observation queries after a compiled Cons window hit.
    pub(crate) static CONS_OBSERVATION_QUERIES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// Test hook: how many times an array shim fell past its fast path.
    pub(crate) static ARRAY_SHIM_SLOW_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cold]
#[inline(never)]
fn aref_slow(ctx: *mut u8, array: Value, index: Value) -> i64 {
    #[cfg(test)]
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(c.get() + 1));
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        // SAFETY: live dormant Context supplied by the JIT shim contract.
        let context = unsafe { &*(ctx as *const Context) };
        match b::builtin_aref_values_in_state(context, array, index) {
            Ok(value) => value.bits() as i64,
            Err(flow) => {
                stash_pending_flow(flow.with_bytecode_error_backtrace(&Op::Aref, &[array, index]));
                VALUE_SHIM_SIGNAL
            }
        }
    })
}

/// How many conses the list shims' fast walks visit before handing the rest
/// of a long list to the builtin (which then walks it from the start, with
/// its cycle check).
const LIST_SHIM_FAST_STEPS: usize = 64;

// These bounded shims never invoke Lisp or start a capture while walking.
#[inline(always)]
fn scan_car<const OBSERVE: bool>(cell: Value) -> Value {
    if OBSERVE {
        cell.cons_car()
    } else {
        cell.cons_car_unobserved()
    }
}

#[inline(always)]
fn scan_cdr<const OBSERVE: bool>(cell: Value) -> Value {
    if OBSERVE {
        cell.cons_cdr()
    } else {
        cell.cons_cdr_unobserved()
    }
}

/// A bounded `memq` walk's answer: `Ok` is the answer (a matching tail or
/// `NIL`), `Err` is the tail where the walk stopped -- an improper cell or
/// the cell after [`LIST_SHIM_FAST_STEPS`] checked ones -- and the builtin's
/// scan continues there instead of re-walking the prefix. Both variants are
/// one `Value`, so the walk returns in two registers; a three-word struct
/// would go through the stack and tax every match.
type MemqWalk = Result<Value, Value>;

/// `(memq ELT LIST)` for a short list: bit identity, and -- while
/// `symbols-with-pos-enabled` (`eq` then looks through positions) -- GNU
/// `slow_eq`'s positioned-symbol rule, the same one the builtin applies.
/// `answer` is `None` when the list is improper before a match or longer
/// than [`LIST_SHIM_FAST_STEPS`]; the builtin answers those, continuing from
/// `tail`. A match is found before the builtin's cycle check could fire:
/// that check fires only after every distinct cons has been visited.
#[inline(always)]
fn memq_fast(ctx: &Context, elt: Value, list: Value) -> MemqWalk {
    if !crate::tagged::collection_reads::reads_need_observation() {
        memq_fast_walk::<false>(ctx, elt, list)
    } else {
        memq_observed_walk(ctx, elt, list)
    }
}

/// The observed instantiation answers only while a capture scope is open.
/// Keep it out of line so the shim's hot body holds just the unobserved
/// walks and keeps its former prologue; the observation calls themselves
/// are unchanged.
#[inline(never)]
fn memq_observed_walk(ctx: &Context, elt: Value, list: Value) -> MemqWalk {
    memq_fast_walk::<true>(ctx, elt, list)
}

/// The flag selects which comparison `eq` means. The flag-off body here must
/// stay the exact bit-identity scan it was: it answers nearly every compiled
/// `memq`, and a per-cell flag test or a stack frame would tax them all (the
/// observed instantiation is out of line for the same reason).
#[inline(always)]
fn memq_fast_walk<const OBSERVE: bool>(ctx: &Context, elt: Value, list: Value) -> MemqWalk {
    if ctx.symbols_with_pos_enabled {
        return memq_swp_walk::<OBSERVE>(elt, list);
    }
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return if tail.is_nil() {
                Ok(Value::NIL)
            } else {
                Err(tail)
            };
        }
        if scan_car::<OBSERVE>(tail).bits() == elt.bits() {
            return Ok(tail);
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    Err(tail)
}

/// GNU `BASE_EQ`: a short-list scan with the flag off (also the shape the
/// flag-on walk uses when the target is no symbol).
#[inline(always)]
fn memq_bits_walk<const OBSERVE: bool>(elt: Value, list: Value) -> MemqWalk {
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return if tail.is_nil() {
                Ok(Value::NIL)
            } else {
                Err(tail)
            };
        }
        if scan_car::<OBSERVE>(tail).bits() == elt.bits() {
            return Ok(tail);
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    Err(tail)
}

/// GNU `EQ` while the flag is on (`lisp.h` `EQ` -> `data.c` `slow_eq`): bit
/// identity, else each side unwraps only if it is itself a symbol with
/// position. Hoist the target's unwrap out of the loop. A target that is no
/// symbol after unwrapping can only match by identity (a positioned symbol
/// unwraps to a symbol), exactly as the builtin's scan decides.
///
/// A bare-symbol target (94% of the byte compiler's calls) is the hot shape:
/// its identity test just ran, so a positioned element only has to unwrap to
/// the same symbol. The positioned-target shape, which must also compare
/// the element's bits with the bare symbol, is out of line.
#[inline(always)]
fn memq_swp_walk<const OBSERVE: bool>(elt: Value, list: Value) -> MemqWalk {
    let elt_bare = b::positioned_symbol_scan::<OBSERVE>(elt).unwrap_or(elt);
    if !elt_bare.is_bare_symbol() {
        return memq_bits_walk::<OBSERVE>(elt, list);
    }
    if elt_bare.bits() != elt.bits() {
        return memq_positioned_target_walk::<OBSERVE>(elt, elt_bare, list);
    }
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return if tail.is_nil() {
                Ok(Value::NIL)
            } else {
                Err(tail)
            };
        }
        let element = scan_car::<OBSERVE>(tail);
        if element.bits() == elt.bits()
            || (element.is_veclike()
                && b::positioned_symbol_scan::<OBSERVE>(element)
                    .is_some_and(|sym| sym.bits() == elt.bits()))
        {
            return Ok(tail);
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    Err(tail)
}

/// [`memq_swp_walk`] with a symbol-with-position target: the element must be
/// the target itself, its bare symbol, or a positioned symbol of that
/// symbol.
#[inline(never)]
fn memq_positioned_target_walk<const OBSERVE: bool>(
    elt: Value,
    elt_bare: Value,
    list: Value,
) -> MemqWalk {
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return if tail.is_nil() {
                Ok(Value::NIL)
            } else {
                Err(tail)
            };
        }
        let element = scan_car::<OBSERVE>(tail);
        if element.bits() == elt.bits() || b::eq_bare_symbol_swp_scan::<OBSERVE>(element, elt_bare)
        {
            return Ok(tail);
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    Err(tail)
}

/// `(assq KEY LIST)` for a short list, as [`memq_fast`]: the first element
/// that is a cons whose car is `eq` to KEY.
#[inline(always)]
fn assq_fast(ctx: &Context, key: Value, list: Value) -> Option<Value> {
    if !crate::tagged::collection_reads::reads_need_observation() {
        assq_fast_walk::<false>(ctx, key, list)
    } else {
        assq_observed_walk(ctx, key, list)
    }
}

/// [`memq_observed_walk`]'s twin for `assq`.
#[inline(never)]
fn assq_observed_walk(ctx: &Context, key: Value, list: Value) -> Option<Value> {
    assq_fast_walk::<true>(ctx, key, list)
}

/// The same two rules as [`memq_fast_walk`], on the entry's key.
#[inline(always)]
fn assq_fast_walk<const OBSERVE: bool>(ctx: &Context, key: Value, list: Value) -> Option<Value> {
    if ctx.symbols_with_pos_enabled {
        return assq_swp_walk::<OBSERVE>(key, list);
    }
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return tail.is_nil().then_some(Value::NIL);
        }
        let entry = scan_car::<OBSERVE>(tail);
        if entry.is_cons() && scan_car::<OBSERVE>(entry).bits() == key.bits() {
            return Some(entry);
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    None
}

/// GNU `BASE_EQ`: the flag-off walk, unchanged.
#[inline(always)]
fn assq_bits_walk<const OBSERVE: bool>(key: Value, list: Value) -> Option<Value> {
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return tail.is_nil().then_some(Value::NIL);
        }
        let entry = scan_car::<OBSERVE>(tail);
        if entry.is_cons() && scan_car::<OBSERVE>(entry).bits() == key.bits() {
            return Some(entry);
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    None
}

/// GNU `EQ` with the flag on, as [`memq_swp_walk`]: the first entry that is
/// a cons whose key is `eq` to KEY. The bare-key shape (53% of the byte
/// compiler's calls) is `memq_swp_walk`'s hot shape; the positioned-key
/// shape is out of line.
#[inline(always)]
fn assq_swp_walk<const OBSERVE: bool>(key: Value, list: Value) -> Option<Value> {
    let key_bare = b::positioned_symbol_scan::<OBSERVE>(key).unwrap_or(key);
    if !key_bare.is_bare_symbol() {
        return assq_bits_walk::<OBSERVE>(key, list);
    }
    if key_bare.bits() != key.bits() {
        return assq_positioned_key_walk::<OBSERVE>(key, key_bare, list);
    }
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return tail.is_nil().then_some(Value::NIL);
        }
        let entry = scan_car::<OBSERVE>(tail);
        if entry.is_cons() {
            let entry_key = scan_car::<OBSERVE>(entry);
            if entry_key.bits() == key.bits()
                || (entry_key.is_veclike()
                    && b::positioned_symbol_scan::<OBSERVE>(entry_key)
                        .is_some_and(|sym| sym.bits() == key.bits()))
            {
                return Some(entry);
            }
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    None
}

/// [`assq_swp_walk`] with a symbol-with-position key.
#[inline(never)]
fn assq_positioned_key_walk<const OBSERVE: bool>(
    key: Value,
    key_bare: Value,
    list: Value,
) -> Option<Value> {
    let mut tail = list;
    for _ in 0..LIST_SHIM_FAST_STEPS {
        if !tail.is_cons() {
            return tail.is_nil().then_some(Value::NIL);
        }
        let entry = scan_car::<OBSERVE>(tail);
        if entry.is_cons() {
            let entry_key = scan_car::<OBSERVE>(entry);
            if entry_key.bits() == key.bits()
                || b::eq_bare_symbol_swp_scan::<OBSERVE>(entry_key, key_bare)
            {
                return Some(entry);
            }
        }
        tail = scan_cdr::<OBSERVE>(tail);
    }
    None
}

/// `Op::Memq` (GNU `Bmemq`) from compiled code: the tail's bits or
/// [`VALUE_SHIM_SIGNAL`]. GC-free on every path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; only read here.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_memq(ctx: *mut u8, elt: i64, list: i64) -> i64 {
    #[cfg(test)]
    LIST_SEARCH_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let elt = Value::from_bits(elt as usize);
    let list = Value::from_bits(list as usize);
    // SAFETY: seam-provided dormant Context; read-only access.
    let ctx_ref = unsafe { &*(ctx as *const Context) };
    match memq_fast(ctx_ref, elt, list) {
        Ok(value) => value.bits() as i64,
        Err(tail) => list_slow_memq_from(ctx, elt, list, tail),
    }
}

/// `Op::Assq` (GNU `Bassq`) from compiled code: the entry's bits or
/// [`VALUE_SHIM_SIGNAL`]. GC-free on every path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; only read here.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_assq(ctx: *mut u8, key: i64, list: i64) -> i64 {
    #[cfg(test)]
    LIST_SEARCH_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let key = Value::from_bits(key as usize);
    let list = Value::from_bits(list as usize);
    // SAFETY: seam-provided dormant Context; read-only access.
    let ctx_ref = unsafe { &*(ctx as *const Context) };
    match assq_fast(ctx_ref, key, list) {
        Some(value) => value.bits() as i64,
        None => list_slow(ctx, pure_assq, key, list),
    }
}

/// `Op::Setcar` (GNU `Bsetcar`) from compiled code: NEWCAR's bits, or
/// [`VALUE_SHIM_SIGNAL`] when CELL is not a cons. GC-free on every path.
///
/// Through `neovm_jit_builtin2`'s table and `pure_setcar` a list loop's
/// `setcar` cost 93 instructions, over half of elb inclist; here a cons
/// takes the store and its barrier in one frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; only read here.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_setcar_collection(ctx: *mut u8, cell: i64, new_car: i64) -> i64 {
    #[cfg(test)]
    LIST_STORE_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let cell = Value::from_bits(cell as usize);
    let new_car = Value::from_bits(new_car as usize);
    if compiled_cons_store(cell, new_car, false) {
        return new_car.bits() as i64;
    }
    list_slow(ctx, pure_setcar, cell, new_car)
}

// The legacy host entry retains the original full setter without a compiled
// observation-policy check. Enabled leaves import its specialized twin.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_setcar(ctx: *mut u8, cell: i64, new_car: i64) -> i64 {
    #[cfg(test)]
    LIST_STORE_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let cell = Value::from_bits(cell as usize);
    let new_car = Value::from_bits(new_car as usize);
    if crate::tagged::mutate::set_cons_car(cell, new_car) {
        return new_car.bits() as i64;
    }
    list_slow(ctx, pure_setcar, cell, new_car)
}

/// `Op::Setcdr` (GNU `Bsetcdr`): [`neovm_jit_setcar`]'s twin.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; only read here.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_setcdr_collection(ctx: *mut u8, cell: i64, new_cdr: i64) -> i64 {
    #[cfg(test)]
    LIST_STORE_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let cell = Value::from_bits(cell as usize);
    let new_cdr = Value::from_bits(new_cdr as usize);
    if compiled_cons_store(cell, new_cdr, true) {
        return new_cdr.bits() as i64;
    }
    list_slow(ctx, pure_setcdr, cell, new_cdr)
}

// The legacy host entry retains the original full setter without a compiled
// observation-policy check. Enabled leaves import its specialized twin.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_setcdr(ctx: *mut u8, cell: i64, new_cdr: i64) -> i64 {
    #[cfg(test)]
    LIST_STORE_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let cell = Value::from_bits(cell as usize);
    let new_cdr = Value::from_bits(new_cdr as usize);
    if crate::tagged::mutate::set_cons_cdr(cell, new_cdr) {
        return new_cdr.bits() as i64;
    }
    list_slow(ctx, pure_setcdr, cell, new_cdr)
}

/// A window hit can be an unobserved hole in the observation envelope.
/// Keep the GC barrier on every accepted store, but journal only an exact
/// observed owner in GEN0 observed mode unless explicit write tracking requires
/// the interpreter's full setter bookkeeping. No safe point intervenes.
#[inline]
fn compiled_cons_store(cell: Value, value: Value, cdr: bool) -> bool {
    use crate::tagged::collection_reads::{
        CompiledJournalMode, compiled_journal_mode, is_observed,
    };
    // GEN1 shares this observed-owner journal decision; its remembered-set
    // barrier is note_heap_slot_write below, exactly as in the full setter.
    if compiled_journal_mode() != CompiledJournalMode::Observed
        || crate::tagged::gc::current_write_tracking_enabled()
    {
        return if cdr {
            crate::tagged::mutate::set_cons_cdr(cell, value)
        } else {
            crate::tagged::mutate::set_cons_car(cell, value)
        };
    }
    if !cell.is_cons() {
        return false;
    }
    let address = cell.bits() & !TAG_MASK;
    let (lo, hi) = crate::tagged::collection_reads::compiled_observation_window();
    let observed = if lo <= address && address < hi {
        #[cfg(test)]
        CONS_OBSERVATION_QUERIES.with(|count| count.set(count.get() + 1));
        is_observed(cell.bits())
    } else {
        false
    };
    if !observed {
        // Refine the native gap before entering the shared mutation body.
        crate::tagged::gc::neovm_jit_unobserved_collection_owner(cell.bits() as i64);
    }
    crate::tagged::gc::with_store_state(|store| {
        if observed {
            crate::tagged::collection_reads::record_projected_write_in_store(
                store,
                cell,
                crate::tagged::collection_reads::WriteProjection::Cons,
            );
        }
        // SAFETY: the preceding Cons tag check validated the live owner.
        unsafe { crate::tagged::mutate::finish_cons_store_in_store(store, cell, value, cdr) }
    })
}

#[cold]
#[inline(never)]
fn list_slow(ctx: *mut u8, builtin: JitBuiltin2Pure, a: Value, b_: Value) -> i64 {
    #[cfg(test)]
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(c.get() + 1));
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        // SAFETY: seam-provided dormant Context; read-only access.
        let ctx_ref = unsafe { &*(ctx as *const Context) };
        match builtin(ctx_ref, a, b_) {
            Ok(value) => value.bits() as i64,
            Err(flow) => {
                stash_pending_flow(flow);
                VALUE_SHIM_SIGNAL
            }
        }
    })
}

/// [`list_slow`] for `memq`: the builtin's budgeted scan continues at `tail`
/// (where [`memq_fast`] stopped) instead of re-visiting the prefix that walk
/// already checked. `list` stays the original head, which only the exact
/// fallback (improper, circular, or budget-exhausted) reads, so every error
/// and its data are unchanged.
#[cold]
#[inline(never)]
fn list_slow_memq_from(ctx: *mut u8, elt: Value, list: Value, tail: Value) -> i64 {
    #[cfg(test)]
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(c.get() + 1));
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        // SAFETY: seam-provided dormant Context; read-only access.
        let ctx_ref = unsafe { &*(ctx as *const Context) };
        match b::builtin_memq_values_from(elt, list, tail, ctx_ref.symbols_with_pos_enabled) {
            Ok(value) => value.bits() as i64,
            Err(flow) => {
                stash_pending_flow(flow);
                VALUE_SHIM_SIGNAL
            }
        }
    })
}

/// `(aset ARRAY INDEX VALUE)` into a plain vector or record slot, or a byte
/// of a string that cannot change width: any byte of a unibyte string, or
/// ASCII into an all-ASCII multibyte one. `false` for everything else, which
/// [`builtin_aset_args`](b::builtin_aset_args) then handles (or signals).
/// Must agree with it wherever it stores.
#[inline(always)]
fn aset_collection_fast(array: Value, index: Value, value: Value) -> bool {
    use crate::tagged::collection_reads::{CompiledJournalMode, compiled_journal_mode};
    let Some(idx) = index.as_fixnum().and_then(|i| usize::try_from(i).ok()) else {
        return false;
    };
    // No callback or safe point can change this process-immutable policy.
    // Reuse it in the outlined store instead of reading the OnceLock twice.
    let journal = compiled_journal_mode();
    if journal != CompiledJournalMode::Off
        && let Some(stored) = aset_journaled_vector(array, idx, value, journal)
    {
        return stored;
    }
    aset_fast(array, index, value)
}

// Exact legacy setter tier: Off calls the original host entry below.
#[inline(always)]
fn aset_fast(array: Value, index: Value, value: Value) -> bool {
    let Some(idx) = index.as_fixnum().and_then(|i| usize::try_from(i).ok()) else {
        return false;
    };
    if array.veclike_type() == Some(crate::tagged::header::VecLikeType::Vector) {
        // One pass over what `set_vector_slot` would check again: owned
        // storage (a mapped pdump vector copies itself on the slow path),
        // bounds, a plain vector, the barrier, the store.
        // veclike_type already performed the original projection read.
        let obj = (array.bits() & !TAG_MASK) as *mut crate::tagged::header::VectorObj;
        let data = unsafe { std::ptr::addr_of_mut!((*obj).data) };
        // End the preparation loan before any journal/barrier/preimage work.
        if !unsafe { &*data }.is_owned() || idx >= unsafe { &*data }.len() {
            return false;
        }
        return crate::tagged::gc::with_store_state(|store| {
            // SAFETY: owned backing and bounds were validated without a
            // callback or safe point. Off retains its unjournaled policy.
            unsafe {
                crate::tagged::mutate::finish_slot_store_in_store(
                    store,
                    array,
                    crate::tagged::gc::HeapWriteKind::VectorSlot,
                    idx,
                    value,
                    data,
                )
            }
        });
    }
    if array.is_veclike() {
        let (items, record) = match array.veclike_type() {
            Some(crate::tagged::header::VecLikeType::Vector) => match array.as_vector_data() {
                Some(items) => (items, false),
                None => return false,
            },
            Some(crate::tagged::header::VecLikeType::Record) => match array.as_record_data() {
                Some(items) => (items, true),
                None => return false,
            },
            _ => return false,
        };
        if idx >= items.len() {
            return false;
        }
        // Both setters run the heap write barrier.
        return if record {
            array.set_record_slot(idx, value)
        } else {
            array.set_vector_slot(idx, value)
        };
    }
    if array.is_string() {
        let (Some(code), Some(string)) = (value.as_fixnum(), array.as_lisp_string()) else {
            return false;
        };
        if idx >= string.schars() {
            return false;
        }
        let fits = if string.is_multibyte() {
            string.has_one_byte_characters() && (0..=0x7f).contains(&code)
        } else {
            (0..=0xff).contains(&code)
        };
        return fits && array.set_string_byte_same_char_count(idx, code as u8);
    }
    false
}

/// The native vector setter's projected write must precede its observed
/// pointer projection. Inspect the immutable header and backing guards
/// without observing them, then reuse the interpreter's fused recorder.
/// A failed guard leaves all effects to the existing generic path.
///
/// Threading: no state is retained here. The caller's mutator owns the
/// existing collection journal and heap barrier; the slot store remains
/// atomic for the concurrent collector, as in the ordinary fast path.
#[inline]
fn aset_journaled_vector(
    array: Value,
    idx: usize,
    value: Value,
    journal: crate::tagged::collection_reads::CompiledJournalMode,
) -> Option<bool> {
    if !array.is_veclike() {
        return None;
    }
    let header = (array.bits() & !TAG_MASK) as *const crate::tagged::header::VecLikeHeader;
    // SAFETY: a live veclike, with an immutable subtype header. There is
    // no callback or GC safe point between the guards and this mutation.
    let (data, kind) = match unsafe { (*header).type_tag } {
        crate::tagged::header::VecLikeType::Vector => (
            unsafe {
                std::ptr::addr_of_mut!((*(header as *mut crate::tagged::header::VectorObj)).data)
            },
            crate::tagged::gc::HeapWriteKind::VectorSlot,
        ),
        crate::tagged::header::VecLikeType::Record => (
            unsafe {
                std::ptr::addr_of_mut!((*(header as *mut crate::tagged::header::RecordObj)).data)
            },
            crate::tagged::gc::HeapWriteKind::RecordSlot,
        ),
        _ => return None,
    };
    if !unsafe { &*data }.is_owned() || idx >= unsafe { &*data }.len() {
        return Some(false);
    }
    // GEN0 and GEN1 share the observed-owner decision. The remembered-set
    // barrier is note_heap_slot_write below and never depends on history.
    let record = journal == crate::tagged::collection_reads::CompiledJournalMode::Eager
        || crate::tagged::gc::current_write_tracking_enabled()
        || unsafe { (*header).gc.collection_observed() };
    Some(crate::tagged::gc::with_store_state(|store| {
        if record {
            crate::tagged::collection_reads::record_projected_write_in_store(
                store,
                array,
                crate::tagged::collection_reads::WriteProjection::VecLike,
            );
        }
        // SAFETY: subtype, owned backing and bounds were validated above;
        // no payload loan spans recording or the barrier's preimage reads.
        unsafe {
            crate::tagged::mutate::finish_slot_store_in_store(store, array, kind, idx, value, data)
        }
    }))
}

/// `Op::Aset` (GNU `Baset`) from compiled code: VALUE's bits, or
/// [`VALUE_SHIM_SIGNAL`]. Like GNU's inline store / `Faset` call, this never
/// consults `aset`'s function cell. Nothing here reaches a safe point: the
/// stores and [`builtin_aset_args`](b::builtin_aset_args) run no Lisp and never
/// collect, so the site roots nothing.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_aset_collection(
    ctx: *mut u8,
    array: i64,
    index: i64,
    value: i64,
) -> i64 {
    #[cfg(test)]
    ASET_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let array = Value::from_bits(array as usize);
    let index = Value::from_bits(index as usize);
    let value = Value::from_bits(value as usize);
    if aset_collection_fast(array, index, value) {
        return value.bits() as i64;
    }
    aset_slow(ctx, array, index, value)
}

/// The original primitive array setter host entry. Knob-off generated code
/// imports this name; collection-aware code selects its separate export.
/// Like GNU Baset, neither entry consults the aset function cell.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_aset(ctx: *mut u8, array: i64, index: i64, value: i64) -> i64 {
    #[cfg(test)]
    ASET_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    let array = Value::from_bits(array as usize);
    let index = Value::from_bits(index as usize);
    let value = Value::from_bits(value as usize);
    if aset_fast(array, index, value) {
        return value.bits() as i64;
    }
    aset_slow(ctx, array, index, value)
}

#[cold]
#[inline(never)]
fn aset_slow(ctx: *mut u8, array: Value, index: Value, value: Value) -> i64 {
    #[cfg(test)]
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(c.get() + 1));
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        // SAFETY: live dormant Context supplied by the JIT shim contract.
        let context = unsafe { &*(ctx as *const Context) };
        match b::builtin_aset_args_in_context(context, &[array, index, value]) {
            Ok(value) => value.bits() as i64,
            Err(flow) => {
                stash_pending_flow(
                    flow.with_bytecode_error_backtrace(&Op::Aset, &[array, index, value]),
                );
                VALUE_SHIM_SIGNAL
            }
        }
    })
}

/// Ternary variant of [`neovm_jit_builtin1`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin3(
    ctx: *mut u8,
    idx: i64,
    a: i64,
    b: i64,
    c: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let b = Value::from_bits(b as usize);
        let c = Value::from_bits(c as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        push_scratch_gc_root(b);
        push_scratch_gc_root(c);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN3[idx as usize](ctx, a, b, c) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// `Op::List`: build a list from `n` operand words, GNU `Flist (n, &TOP)` on
/// the live stack slice. Nothing here can collect or run Lisp (cons
/// allocation is GC-atomic, see `TaggedHeap::alloc_cons`), so the operand
/// words need no scratch roots -- the same contract `neovm_jit_cons`
/// documents. Infallible, context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_list(args_ptr: *const i64, nargs: i64) -> i64 {
    // SAFETY: the generated code stored exactly `nargs` words at `args_ptr`
    // (its call-args stack slot) immediately before this call, and `Value` is
    // `#[repr(transparent)]` over a word, so the slot IS a `&[Value]` for the
    // duration of the call; nothing below writes to generated-code memory.
    let args = unsafe { std::slice::from_raw_parts(args_ptr.cast::<Value>(), nargs as usize) };
    Value::list_from_slice(args).bits() as i64
}

/// Call a slice-shaped direct builtin (`JIT_BUILTIN_SLICE[idx]`) — the
/// identical function the interpreter arm calls (`nconc`/`concat`/
/// `substring`). Roots the operands across the call (they may allocate);
/// context-free like the interpreter's slice calls.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin_slice(
    idx: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(no_ctx, STATUS_SIGNAL, {
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        // The builtin reads the call-args slot in place (`Value` is the
        // word the generated code stored) and the slot is rooted in ONE
        // batch: building a SmallVec and rooting each word on its own was
        // ~120 of this shim's ~165 instructions per `concat`.
        // SAFETY: see neovm_jit_list — the same spill-slot contract; the
        // slot outlives the call (the generated code is suspended in it).
        let args: &[Value] =
            unsafe { core::slice::from_raw_parts(args_ptr as *const Value, nargs) };
        crate::emacs_core::eval::push_scratch_gc_roots(args);
        let status = match JIT_BUILTIN_SLICE[idx as usize](args) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Primitive opcode dispatch for `Op::CallBuiltin`/`Op::CallBuiltinSym`
/// through the dedicated `Vm::*_for_jit` helpers. Both execute the primitive
/// directly, matching GNU bytecode, with mutating-first-arg string writeback
/// and the trailing quit poll. Genuine `Bcall` sites use the call shims.
/// `variant`: 0 = CallBuiltin, 1 = CallBuiltinSym.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_named_builtin(
    ctx: *mut u8,
    variant: i64,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        // SAFETY: the generated code stored exactly `nargs` words in its
        // call-args stack slot immediately before this call.
        debug_assert!(matches!(variant, 0 | 1), "primitive opcode shim variant");
        let mut args = LispArgVec::new();
        for i in 0..nargs {
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            push_scratch_gc_root(v);
            args.push(v);
        }
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let mut vm = Vm::from_context(ctx);
        let result = match variant {
            0 => vm.callbuiltin_for_jit(SymId(sym as u32), args),
            _ => vm.callbuiltinsym_for_jit(SymId(sym as u32), args),
        };
        let status = match result {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// `Op::SaveWindowExcursion` (GNU bytecode.c Bsave_window_excursion): pop the
/// body form list, evaluate `(progn . body)` inside a real
/// window-configuration save/restore — the interpreter arm 1:1, including
/// error precedence (a failed restore wins over the body's flow). The body
/// runs arbitrary lisp: everything live is rooted here, the generated code
/// rooted the rest of its frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_window_excursion(ctx: *mut u8, body: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let body = Value::from_bits(body as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let root_scope = save_scratch_gc_roots();
        push_scratch_gc_root(body);
        let progn_form = Value::cons(Value::symbol("progn"), body);
        push_scratch_gc_root(progn_form);
        let status = (|| {
            let saved = match crate::emacs_core::builtins::SavedWindowConfiguration::capture(
                ctx,
                Value::NIL,
            ) {
                Ok(saved) => saved,
                Err(flow) => {
                    stash_pending_flow(flow);
                    return STATUS_SIGNAL;
                }
            };
            let body_result = ctx.with_unwind_scope(|ctx| {
                ctx.record_native_unwind(
                    crate::emacs_core::eval::NativeUnwindAction::RestoreWindowConfiguration {
                        configuration: saved,
                        options:
                            crate::emacs_core::builtins::WindowConfigurationRestoreOptions::default(
                            ),
                    },
                );
                ctx.eval_sub(progn_form)
            });
            match body_result {
                Ok(result) => {
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = result.bits() as i64 };
                    STATUS_OK
                }
                Err(flow) => {
                    stash_pending_flow(flow);
                    STATUS_SIGNAL
                }
            }
        })();
        restore_scratch_gc_roots(root_scope);
        status
    })
}

/// Sentinel `SpecSlot::epoch` value marking a site the AOT LOADER left DISARMED
/// because its live-obarray re-classification disagreed with the kind baked into
/// the generated code (see `CompiledLeaf::from_aot`'s arming). A disarmed site
/// must NEVER re-arm: the shims' fast paths are keyed on the BAKED kind (e.g.
/// `neovm_jit_pred_spec`'s hardcoded `is_record` tag test), so re-arming against a
/// now-different binding would run the WRONG op. Both `neovm_jit_call_spec` and
/// `subr_spec_armed` short-circuit to their not-armed (generic / strict-symbol)
/// path on this value BEFORE any re-validate/re-arm. `u64::MAX` is RESERVED: the
/// live obarray `function_epoch` skips it on wrap (`next_function_epoch`), so a
/// legitimately-armed slot never collides with it. JIT leaves never store it, so
/// the extra compare is a perfectly-predicted never-taken branch there (~0 tax).
pub(crate) const SPEC_EPOCH_DISARMED: u64 = u64::MAX;

/// The strict call the spec shim falls back to (callee not natively
/// runnable, or the site not armed): the args go onto the operand stack once
/// and the call takes the stack-span path — `call_for_jit_stack` →
/// `call_function_from_stack_args` → `execute_bytecode_call_from_stack` —
/// like the generic shim, so a compiled callee reaches the leaf-slot entry
/// and an interpreted one enters the interpreter without a second copy.
/// `target` is scratch-rooted across the call; the args are rooted by the
/// operand stack. Out of line, as both strict calls: inlined, they cost the
/// shim's slow half its frame size and register assignment.
#[cfg(feature = "jit")]
#[inline(never)]
fn call_for_jit_from_native(
    ctx: &mut Context,
    target: Value,
    args_ptr: *const i64,
    nargs: usize,
) -> crate::emacs_core::error::EvalResult {
    stack_call_from_native(ctx, target, args_ptr, nargs, |vm, args_start| {
        vm.call_for_jit_stack(target, args_start, nargs)
    })
}

/// [`call_for_jit_from_native`] for an armed spec site whose callee the leaf
/// fast path declined (not compiled, or an arity the strict path signals):
/// the epoch proof says the symbol `called` names the bytecode object
/// `callee`, so the stack call runs `callee` without resolving the symbol
/// again (`Vm::call_resolved_bytecode_for_jit_stack`), and its frame records
/// `called`, as GNU's `Bcall` does.
#[cfg(feature = "jit")]
#[inline(never)]
fn call_resolved_for_jit_from_native(
    ctx: &mut Context,
    called: Value,
    callee: Value,
    args_ptr: *const i64,
    nargs: usize,
) -> crate::emacs_core::error::EvalResult {
    stack_call_from_native(ctx, callee, args_ptr, nargs, |vm, args_start| {
        vm.call_resolved_bytecode_for_jit_stack(called, callee, args_start, nargs)
    })
}

/// The operand-stack setup of the strict calls above: `root` scratch-rooted
/// and the caller's `nargs` argument words pushed onto `bc_buf` (GC-traced)
/// for the duration of `call`, which gets their start index.
#[cfg(feature = "jit")]
#[inline(always)]
fn stack_call_from_native(
    ctx: &mut Context,
    root: Value,
    args_ptr: *const i64,
    nargs: usize,
    call: impl FnOnce(&mut Vm<'_>, usize) -> crate::emacs_core::error::EvalResult,
) -> crate::emacs_core::error::EvalResult {
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(root);
    let args_start = ctx.bc_buf.len();
    // SAFETY: the generated code stored exactly `nargs` argument words at
    // `args_ptr` (its call-args slot) immediately before this call.
    ctx.bc_buf
        .extend((0..nargs).map(|i| Value::from_bits(unsafe { *args_ptr.add(i) } as usize)));
    let res = call(&mut Vm::from_context(ctx), args_start);
    ctx.bc_buf.truncate(args_start);
    restore_scratch_gc_roots(saved);
    res
}

/// Expand the spec-call protocol into its ordinary and census entry points.
/// The census flag is a compile-time literal: the ordinary entry has no
/// counter, runtime knob branch or additional invocation/inlining boundary.
/// Function argument identifiers are explicit to preserve macro hygiene.
///
/// Threading: both expansions use the caller's mutator-owned Context and
/// existing atomic spec slot; no Lisp state is cached. Census increments
/// are process-wide relaxed atomics and never change the call's state.
macro_rules! spec_call_body {
    ($census:expr, $ctx:ident, $sym_bits:ident, $expected:ident, $slot:ident,
     $args_ptr:ident, $nargs:ident, $out:ident) => {{
        // Evidence that speculation actually engages, including in release
        // unit tests. Non-test release builds carry no counter.
        #[cfg(any(test, debug_assertions))]
        SPEC_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract; `slot` points
        // into the executing leaf's spec_slots.
        let ctx_ref = unsafe { &mut *($ctx as *mut Context) };
        let slot_ref = unsafe { &*($slot as *const SpecSlot) };
        let direct_consts = slot_ref.direct_consts.load(Ordering::Relaxed);
        if direct_consts != 0
            && ctx_ref.attention_clear(AttentionMask::SPEC_CALL)
            && {
                let mut projection = ctx_ref.dispatch_projection_exclusive();
                slot_ref.epoch.load(Ordering::Relaxed) == projection.function_epoch()
                    && !projection.debug_on_next_call_armed_fast()
            }
            && ctx_ref.depth < ctx_ref.max_depth
        {
            #[cfg(any(test, debug_assertions))]
            {
                SPEC_FAST_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
                SPEC_SHIM_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            // SAFETY: a non-null slot leaf names a live or retired cache leaf
            // (`resolve_compiled_leaf_ptr`'s invariant), and `direct_consts` is
            // the constant base of the object the slot was armed for, which the
            // epoch proof keeps alive (the slot is cleared on every re-arm).
            let leaf = unsafe { &*slot_ref.leaf_ptr() };
            let callee = Value::from_bits($expected as usize);
            let $nargs = $nargs as usize;
            let bt_count = ctx_ref.specpdl.len();
            // SAFETY: args_ptr addresses `nargs` valid tagged words (the caller's
            // call-args slot). The frame records the called SYMBOL, as GNU's
            // `Bcall` does (src/bytecode.c:792-796); `callee` stays alive through
            // the symbol's function cell, or past a redefinition through the
            // frame (`cache::pin_redefined_function`).
            unsafe {
                ctx_ref.push_backtrace_frame_from_native_args(
                    Value::from_bits($sym_bits as usize),
                    $args_ptr,
                    $nargs,
                )
            };
            ctx_ref.depth += 1;
            // The callee's frame: the call as laid out when the count is the
            // leaf's arity, otherwise the given words followed by nil for each
            // missing `&optional` slot, in a buffer of this frame (the arming
            // condition bounds the arity; the backtrace entry above still
            // records the call's own arguments). The nils need no rooting.
            let mut padded = core::mem::MaybeUninit::<[i64; FAST_PATH_MAX_ARITY]>::uninit();
            let frame_args: *const i64 = if direct_consts & SpecSlot::KEY_SHORT_CALL == 0 {
                $args_ptr
            } else {
                let arity = leaf.arity;
                let buf = padded.as_mut_ptr() as *mut i64;
                if leaf.has_rest {
                    // SAFETY: arity <= FAST_PATH_MAX_ARITY (the arming
                    // condition), and args_ptr addresses `nargs` words.
                    unsafe { spec_rest_frame(ctx_ref, $args_ptr, $nargs, arity - 1, buf) };
                } else {
                    // SAFETY: nargs < arity <= FAST_PATH_MAX_ARITY (the arming
                    // condition), and args_ptr addresses `nargs` words.
                    unsafe {
                        core::ptr::copy_nonoverlapping($args_ptr, buf, $nargs);
                        for i in $nargs..arity {
                            *buf.add(i) = Value::NIL.bits() as i64;
                        }
                    }
                }
                buf as *const i64
            };
            // One test picks the memory-ABI raw entry (the flags the slot was
            // armed with say the leaf is frameless with the memory ABI); the
            // register-ABI raw entry and the framed run are the other two.
            let run = if direct_consts & (SpecSlot::KEY_FRAMED | SpecSlot::KEY_REGISTER) == 0 {
                if $census {
                    $crate::emacs_core::jit::compile::call_census::record_spec_fast_for_census(
                        false,
                    );
                }
                // The constant base is the key without its flags. Worked out
                // here, for this call alone: the other two take the key and
                // mask it themselves, so the base's register is this call's.
                let consts = (direct_consts & !SpecSlot::KEY_FLAGS) as usize as *const Value;
                let mut bits: i64 = 0;
                // SAFETY: `frame_args` addresses `arity` live words for the
                // call (the slot or this frame's buffer) of a frameless
                // memory-ABI leaf; `ctx` is the dormant seam Context.
                let status =
                    unsafe { leaf.entry_call_raw_memory($ctx, consts, frame_args, &mut bits) };
                if status == STATUS_OK {
                    #[cfg(any(test, debug_assertions))]
                    crate::emacs_core::jit::cache::NATIVE_OK_COUNT.fetch_add(1, Ordering::Relaxed);
                    if ctx_ref.pop_native_backtrace_frame(bt_count) {
                        ctx_ref.depth -= 1;
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *$out = bits };
                        return STATUS_OK;
                    }
                    FastRun::Done(Value::from_bits(bits as usize))
                } else {
                    FastRun::Raw(status)
                }
            } else if direct_consts & SpecSlot::KEY_FRAMED == 0 {
                if $census {
                    $crate::emacs_core::jit::compile::call_census::record_spec_fast_for_census(
                        false,
                    );
                }
                // SAFETY: as above, for a frameless register-ABI leaf.
                let ret = unsafe { call_spec_register_run($ctx, frame_args, leaf, direct_consts) };
                if ret.status == STATUS_OK {
                    #[cfg(any(test, debug_assertions))]
                    crate::emacs_core::jit::cache::NATIVE_OK_COUNT.fetch_add(1, Ordering::Relaxed);
                    if ctx_ref.pop_native_backtrace_frame(bt_count) {
                        ctx_ref.depth -= 1;
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *$out = ret.value };
                        return STATUS_OK;
                    }
                    FastRun::Done(Value::from_bits(ret.value as usize))
                } else {
                    FastRun::Raw(ret.status)
                }
            } else {
                if $census {
                    $crate::emacs_core::jit::compile::call_census::record_spec_fast_for_census(
                        true,
                    );
                }
                match call_spec_framed_run($ctx, leaf, direct_consts, frame_args) {
                    NativeRun::Ok(bits) => {
                        #[cfg(any(test, debug_assertions))]
                        crate::emacs_core::jit::cache::NATIVE_OK_COUNT
                            .fetch_add(1, Ordering::Relaxed);
                        if ctx_ref.pop_native_backtrace_frame(bt_count) {
                            ctx_ref.depth -= 1;
                            // SAFETY: as above.
                            unsafe { *$out = bits as i64 };
                            return STATUS_OK;
                        }
                        FastRun::Done(Value::from_bits(bits))
                    }
                    // A panic contained in the framed entry: leave its marker
                    // and the residue -- this frame's backtrace entry, the
                    // depth count -- to the caller's healing points, exactly
                    // as the single-frame containment did. Folding it here
                    // would consume the marker before `cold_frame_exit` or the
                    // match shim could restore the caller's boundary.
                    // The residue keeps this frame, but not its reads of the
                    // caller's slot, which dies when the caller leaf exits.
                    NativeRun::Signal if shim_panic_pending() => {
                        // SAFETY: `args_ptr` is the caller's live call-args slot.
                        unsafe { ctx_ref.detach_native_frames_into($args_ptr) };
                        return STATUS_SIGNAL;
                    }
                    other => FastRun::Framed(other),
                }
            };
            return call_spec_finish($ctx, callee, leaf, $args_ptr, $nargs, $out, bt_count, run);
        }
        call_spec_slow(
            $ctx,
            $sym_bits,
            $expected,
            slot_ref,
            $args_ptr,
            $nargs as usize,
            $out,
        )
    }};
}

/// Speculated direct call (`Op::Call` whose callee slot provably holds a
/// constant symbol that was fbound to a bytecode object at compile time).
/// Quit poll FIRST (the interpreter's Op::Call order — quit processing can run
/// lisp, including fset), then the validity check: if `ctx.obarray`'s
/// function_epoch still equals this site's armed epoch, NO function binding
/// anywhere has changed since the binding was observed equal to `expected`,
/// so the callee object is still reachable through the obarray and calling it
/// directly is exactly equivalent to resolving the symbol — minus the
/// resolution. On an epoch move, re-validate THIS binding: unchanged -> re-arm
/// the slot and proceed direct; changed -> strict symbol call (fset/advice
/// take effect immediately, GNU default-settings parity).
///
/// Two halves. The armed fast path runs in this frame: the site's cached
/// leaf takes the call's arguments exactly as the generated code laid them
/// out (`SpecSlot::direct_consts`), nothing needs the quit poll's attention,
/// the debugger is not armed and the call fits under `max-lisp-eval-depth`.
/// Every step of it is a load, a compare, the backtrace push (a bounded
/// write; growing the specpdl aborts rather than unwinds), the raw native
/// entry of a handler-free body and the balanced pop -- none can unwind in
/// a release build, so the path runs OUTSIDE the panic-containment frame,
/// whose register saves and landing pad were 21 of the shim's
/// instructions. (A framed body's entry can run Lisp on its way out and
/// keeps a containment frame of its own, [`call_spec_framed_run`].) It
/// re-classifies
/// nothing: the callee, the leaf and the argument shape were classified
/// once, when the slot was armed (`Vm::call_armed_callee_native`), another
/// 20 instructions a call. A short call or a `&rest` callee has its frame
/// built in a buffer here ([`spec_rest_frame`] conses the tail). Everything
/// else -- the first call through a site, a re-validation, a callee wider
/// than that buffer, the debugger, the depth floor, a non-OK native exit --
/// takes the contained slow half,
/// [`call_spec_slow`] / [`call_spec_finish`], which is the reference
/// protocol unchanged.
///
/// `sym_bits` is the called symbol as its tagged `Value` bits: the callee's
/// backtrace frame records it, as GNU's `Bcall` records `call_fun`
/// (src/bytecode.c:792-796), and the site's constant is already in that
/// form, so the hot path records it with no conversion.
///
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `slot` points into the
/// owning CompiledLeaf's spec_slots (alive whenever its code runs).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call_spec(
    ctx: *mut u8,
    sym_bits: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    spec_call_body!(false, ctx, sym_bits, expected, slot, args_ptr, nargs, out)
}

#[path = "dispatch/census.rs"]
mod census;
pub(crate) use census::neovm_jit_call_spec_census;

/// The spec fast path's frame for a `&rest` callee of `nonrest + 1` slots:
/// the given words, nil for each missing `&optional` slot, and the words
/// past `nonrest` as a fresh list in the last slot -- what the slow half's
/// `marshal_and_run` builds, and the interpreter's `run_frame` (GNU
/// `setup_frame`'s `Flist`, src/bytecode.c:545-546). Every `cl-generic`
/// dispatcher takes its arguments this way (`(arg &rest args)`), and
/// before this each of its calls took the whole contained slow half.
///
/// The list needs no root: allocation never collects, and nothing between
/// it and the callee's entry reading its frame reaches a safe point; the
/// elements are the call's arguments, which its backtrace frame roots.
/// Allocation cannot unwind either (the generated code's own `cons` shim
/// allocates uncontained), so the fast path stays outside a containment
/// frame. Out of line: the pure pass-through is the path that must stay
/// small.
///
/// SAFETY: `args_ptr` addresses `nargs` valid words and `buf` has room for
/// `nonrest + 1` words.
#[inline(never)]
unsafe fn spec_rest_frame(
    ctx: &mut Context,
    args_ptr: *const i64,
    nargs: usize,
    nonrest: usize,
    buf: *mut i64,
) {
    let fixed = nargs.min(nonrest);
    // SAFETY: per the contract, `fixed <= nargs` words are readable and
    // `nonrest + 1` writable.
    unsafe {
        core::ptr::copy_nonoverlapping(args_ptr, buf, fixed);
        for i in fixed..nonrest {
            *buf.add(i) = Value::NIL.bits() as i64;
        }
        let rest = if nargs > nonrest {
            let tail =
                core::slice::from_raw_parts(args_ptr.add(nonrest) as *const Value, nargs - nonrest);
            ctx.tagged_heap.list_from_slice(tail)
        } else {
            Value::NIL
        };
        *buf.add(nonrest) = rest.bits() as i64;
    }
}

/// The fast path's framed entry, under a containment frame of its own: a
/// body with bindings or handler frames exits through `invoke_native`'s
/// parity unwinds, which run Lisp -- `unwind-protect` cleanups, variable
/// watchers, the debugger's exit hook -- and Lisp can reach a Rust panic
/// that must become a Lisp error, not an abort at the shim's C boundary.
/// The raw entry of a handler-free body has no such exit and stays in the
/// shim's frame. A contained panic answers `Signal` with the pending-panic
/// marker set; the shim returns STATUS_SIGNAL at once and leaves the
/// healing to the caller leaf's exit (see the fast path).
/// `key` is the slot's fast-path key, the constant base with its flags.
#[inline(never)]
pub(super) fn call_spec_framed_run(
    ctx: *mut u8,
    leaf: &CompiledLeaf,
    key: u64,
    args_ptr: *const i64,
) -> NativeRun {
    let consts = (key & !SpecSlot::KEY_FLAGS) as usize as *const Value;
    jit_shim_contain!(ctx, NativeRun::Signal, {
        leaf.call_premarshaled_consts(ctx, consts, args_ptr)
    })
}

/// The fast path's raw entry of a frameless register-ABI leaf
/// (`SpecSlot::KEY_REGISTER`), out of the shim's frame: the memory-ABI raw
/// entry is the one the shim calls in line. `key` is the slot's fast-path
/// key, the constant base with its flags.
///
/// SAFETY: as `CompiledLeaf::entry_call_raw_register`.
#[inline(never)]
unsafe fn call_spec_register_run(
    ctx: *mut u8,
    args_ptr: *const i64,
    leaf: &CompiledLeaf,
    key: u64,
) -> super::reg_abi::NativeRet {
    // Keep the leaf pointer in this outlined helper's argument list. ThinLTO
    // otherwise promotes its entry and thunk loads into the spec shim, adding
    // two loads there and changing the framed arm's argument register. The
    // opacity stays inside this register-only helper, off the memory path.
    let leaf = std::hint::black_box(leaf);
    let consts = (key & !SpecSlot::KEY_FLAGS) as usize as *const Value;
    // SAFETY: the caller's contract.
    unsafe { leaf.entry_call_raw_register(ctx, consts, args_ptr) }
}

/// How a fast-path native run ended when it did not end in a balanced OK.
pub(super) enum FastRun {
    /// OK, but the callee's backtrace frame is no longer the plain entry the
    /// push made (the debugger flagged it): the general pop must run.
    Done(Value),
    /// A raw (direct) entry returned this non-OK status.
    Raw(i64),
    /// A framed run ended other than OK.
    Framed(NativeRun),
}

/// The fast path's cold exit: everything `run_leaf_native_to_native` does
/// after the native run for a non-OK outcome -- signal fold, precise-deopt
/// resume, interpreter rerun, the depth and frame pops -- under the
/// panic-containment frame those steps need. Runs with the callee's depth
/// still counted and its frame still pushed.
#[cold]
#[inline(never)]
#[allow(clippy::too_many_arguments)] // the fast path's live state, handed over whole
pub(super) fn call_spec_finish(
    ctx: *mut u8,
    callee: Value,
    leaf: &CompiledLeaf,
    args_ptr: *const i64,
    nargs: usize,
    out: *mut i64,
    bt_count: usize,
    run: FastRun,
) -> i64 {
    jit_shim_contain!(detach args_ptr, ctx, STATUS_SIGNAL, {
        use crate::emacs_core::jit::cache::{self, NativeCallOutcome};
        let ctx_ptr = ctx as *mut Context;
        // SAFETY: the dormant seam Context (the shim contract).
        let ctx = unsafe { &mut *ctx_ptr };
        // SAFETY: the slot's leaf was resolved from this object's data
        // (`call_armed_callee_native`), which materialized it.
        let bc = unsafe { callee.bytecode_data_materialized_by_caller() };
        let outcome = match run {
            FastRun::Done(value) => NativeCallOutcome::Value(value),
            FastRun::Raw(status) => {
                #[cfg(any(test, debug_assertions))]
                cache::count_native_status(status);
                cache::direct_call_cold(ctx_ptr, bc, callee, leaf, status)
            }
            FastRun::Framed(outcome) => {
                cache::finish_framed_run(ctx_ptr, bc, callee, leaf, outcome)
            }
        };
        let outcome = match outcome {
            NativeCallOutcome::Fallback => {
                Vm::interp_fallback_native(ctx_ptr, bc, callee, args_ptr, nargs)
            }
            o => o,
        };
        ctx.depth -= 1;
        // A contained panic (its marker still set, see `direct_call_cold`):
        // this frame's own entry goes if it is still on top, the rest of
        // the residue and the marker are the caller's healing exit's; the
        // general unwinder below would fold the marker first.
        if shim_panic_pending() {
            // SAFETY: `args_ptr` is this shim's caller's live call-args slot.
            unsafe { ctx.pop_or_detach_native_frame(bt_count, args_ptr) };
            return STATUS_SIGNAL;
        }
        let outcome = if ctx.pop_native_backtrace_frame(bt_count) {
            outcome
        } else {
            // Imbalanced (rare): materialize the EvalResult the general
            // unwinder needs, then re-compact.
            let res = match outcome {
                NativeCallOutcome::Value(v) => Ok(v),
                NativeCallOutcome::FlowStashed => {
                    Err(take_pending_flow().expect("FlowStashed implies a pending flow"))
                }
                NativeCallOutcome::Fallback => unreachable!("Fallback resolved before the pop"),
            };
            NativeCallOutcome::from_result(
                ctx.pop_bytecode_backtrace_frame_with_result(bt_count, res),
            )
        };
        match outcome {
            NativeCallOutcome::Value(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            NativeCallOutcome::FlowStashed => STATUS_SIGNAL,
            NativeCallOutcome::Fallback => {
                unreachable!("Fallback outcome at the spec-shim boundary")
            }
        }
    })
}

/// The reference protocol of [`neovm_jit_call_spec`]: the quit poll, the
/// epoch check and re-validation, and the call through
/// `Vm::call_armed_callee_native` (which arms the slot's fast-path key) or
/// the strict path.
#[inline(never)]
fn call_spec_slow(
    ctx: *mut u8,
    sym_bits: i64,
    expected: i64,
    slot: &SpecSlot,
    args_ptr: *const i64,
    nargs: usize,
    out: *mut i64,
) -> i64 {
    // The site passes the called symbol as its tagged bits: what the
    // callee's frame records, handed down as they are. Its id is only for
    // the obarray reads below (the lowering bakes a symbol here; a tag test
    // and a panic path on every slow call were measured, so debug-only).
    let called = Value::from_bits(sym_bits as usize);
    debug_assert!(
        called.is_bare_symbol(),
        "a speculated call site's callee is a symbol"
    );
    let sym = called.xbare_symbol_id();
    jit_shim_contain!(detach args_ptr, ctx, STATUS_SIGNAL, {
        // Build a rooted LispArgVec from the caller's call-args slot — used only by
        // the strict-call fallback paths (call_for_jit), inside their own
        // scratch-root scope. The native-to-native fast path passes `args_ptr`
        // straight through and touches no scratch-root state at all.
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        // Same split as the interpreter's Op::Call poll: the loads-only fast
        // condition runs first; the full poll only when it has work.
        let quit = if ctx.maybe_quit_hot_ok() {
            Ok(())
        } else {
            ctx.maybe_quit()
        };
        match quit {
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
            Ok(()) => {
                let slot_epoch = slot.epoch.load(Ordering::Relaxed);
                // A loader-DISARMED site (epoch == SPEC_EPOCH_DISARMED) is
                // permanently not-armed: its baked callee/kind disagreed with the
                // live obarray at load, so re-validating could re-arm the WRONG
                // binding. Check it FIRST (before the epoch load / re-validate) and
                // fall to the strict-symbol-call path. JIT never sets DISARMED, so
                // this compare is perfectly-predicted never-taken there.
                let armed = if slot_epoch == SPEC_EPOCH_DISARMED {
                    false
                } else {
                    let epoch = ctx.dispatch_projection().function_epoch();
                    // NEOVM_JIT_FORCE_SLOW_SPEC pretends the armed epoch is stale so
                    // every call exercises the re-validate/re-arm branch.
                    (!jit_force_slow_spec() && slot_epoch == epoch) || {
                        use crate::emacs_core::jit::stats::epoch::{
                            SpecRevalidation, note_spec_revalidation,
                        };
                        let cur = ctx.obarray.symbol_function_id(sym);
                        if cur.is_some_and(|v| v.bits() as i64 == expected) {
                            note_spec_revalidation(SpecRevalidation::Rearmed);
                            // Equal bits re-arm the site, but they do not
                            // prove the cached leaf still belongs to the
                            // binding: a bytecode object that was unbound,
                            // collected and whose arena slot the next
                            // definition of this symbol landed on has the
                            // same bits and different code (two raw `fset`s
                            // with a collection between; `defun` roots the
                            // old definition through `function-history`).
                            // And the redefinition that moved the epoch may
                            // be a bit-op's, which retired a leaf that
                            // inlined it. So the leaf is dropped on every
                            // re-arm and the next call resolves it again
                            // from the function the binding holds now --
                            // one cache lookup per site per epoch move.
                            slot.clear_leaf();
                            slot.epoch.store(epoch, Ordering::Relaxed);
                            true
                        } else {
                            note_spec_revalidation(SpecRevalidation::BindingChanged);
                            // The binding changed: drop any cached callee leaf so
                            // a later re-arm can't reuse a stale callee.
                            slot.clear_leaf();
                            false
                        }
                    }
                };
                // Armed: the symbol still names the compile-time bytecode object.
                // Try the fast path (cached leaf, native-to-native pass-through when
                // the callee is a pure fixed-arity match — no arg marshaling at
                // all). Fall back to the strict call on the VALUE if it can't be
                // fast-pathed (arity / not compilable). Not armed: strict call on
                // the SYMBOL (resolves the new binding — fset/advice take effect
                // immediately).
                use crate::emacs_core::jit::cache::NativeCallOutcome;
                let outcome = if armed {
                    let target = Value::from_bits(expected as usize);
                    // About to resolve (and maybe compile) the callee's leaf
                    // before its backtrace frame exists: hand its symbol to
                    // the leaf's perf-map label. Naming-only, cold.
                    if slot.leaf_ptr().is_null() && crate::emacs_core::jit::stats::naming_enabled()
                    {
                        crate::emacs_core::jit::stats::perf_map::set_pending_callee(sym);
                    }
                    // No scratch rooting and no Vm construction on the armed
                    // fast path: the target is the symbol's function (the
                    // epoch proof), which keeps it alive -- and once that is
                    // redefined, the callee's frame recording the symbol
                    // does (`cache::pin_redefined_function`) -- and
                    // `Vm::from_context`'s eager cache zero-fill was a
                    // measured per-call tax.
                    match Vm::call_armed_callee_native(ctx, called, target, slot, args_ptr, nargs)
                    {
                        Some(o) => o,
                        // The strict call runs `target`, which the epoch
                        // proof says the symbol names, so it resolves
                        // nothing again; its frame records the SYMBOL, as
                        // GNU's `Bcall` does and as the unarmed call below.
                        None => NativeCallOutcome::from_result(call_resolved_for_jit_from_native(
                            ctx, called, target, args_ptr, nargs,
                        )),
                    }
                } else {
                    NativeCallOutcome::from_result(call_for_jit_from_native(
                        ctx, called, args_ptr, nargs,
                    ))
                };
                match outcome {
                    NativeCallOutcome::Value(value) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    NativeCallOutcome::FlowStashed => STATUS_SIGNAL,
                    // from_result never produces Fallback, and
                    // call_armed_callee_native resolves its Fallbacks itself.
                    NativeCallOutcome::Fallback => {
                        unreachable!("Fallback outcome at the spec-shim boundary")
                    }
                }
            }
        }
    })
}

/// Predicate discriminator for [`neovm_jit_pred_spec`] (baked as an iconst by
/// the lowering): `recordp`.
pub(crate) const PRED_KIND_RECORDP: i64 = 0;
/// Predicate discriminator for [`neovm_jit_pred_spec`]: `symbol-with-pos-p`.
pub(crate) const PRED_KIND_SYMBOL_WITH_POS_P: i64 = 1;
/// `type-of`: the builtin's answer (see [`SpecCalleeKind::PredTypeOf`]).
pub(crate) const PRED_KIND_TYPE_OF: i64 = 2;
/// `cl-type-of`.
pub(crate) const PRED_KIND_CL_TYPE_OF: i64 = 3;
/// `fboundp`: whether the symbol's function cell is non-nil (see
/// [`SpecCalleeKind::PredFboundp`]).
pub(crate) const PRED_KIND_FBOUNDP: i64 = 4;
/// `autoload-do-load`: FUNDEF itself unless it is an autoload object (see
/// [`SpecCalleeKind::PredAutoloadDoLoad`]).
pub(crate) const PRED_KIND_AUTOLOAD_DO_LOAD: i64 = 5;

/// Op discriminators for [`neovm_jit_arith_spec`] (baked as an iconst by the
/// lowering, and — offset by 5 — the [`SpecCalleeKind::to_spec_disc`] value):
/// `logand`.
pub(crate) const ARITH_KIND_LOGAND: i64 = 0;
/// Op discriminator for [`neovm_jit_arith_spec`]: `logior`.
pub(crate) const ARITH_KIND_LOGIOR: i64 = 1;
/// Op discriminator for [`neovm_jit_arith_spec`]: `logxor`.
pub(crate) const ARITH_KIND_LOGXOR: i64 = 2;
/// Op discriminator for [`neovm_jit_arith_spec`]: `ash` (2-arg arithmetic shift).
/// LEFT shifts can overflow fixnum range → the shim bounces to generic (bignum).
pub(crate) const ARITH_KIND_ASH: i64 = 3;
/// Op discriminator for [`neovm_jit_arith_spec`]: `lognot` (1-arg bitwise NOT).
pub(crate) const ARITH_KIND_LOGNOT: i64 = 4;
/// Op discriminator for [`neovm_jit_arith_spec`]: `mod` (2-arg floor-modulo).
/// The fixnum fast path is GC-free: `|a % b| < |b|` and the sign-fixup add
/// stays within the divisor's magnitude, so the result is always a fixnum; a
/// zero divisor bounces to generic (arith-error), non-fixnums (floats,
/// bignums, markers) bounce to generic.
pub(crate) const ARITH_KIND_MOD: i64 = 5;

/// Classify a callee SYMBOL name + arity as a bitwise-arithmetic intrinsic op,
/// or `None`. Shared by [`subr_spec_kind`] (which additionally proves the live
/// binding is the real subr) and the profitability gate's arith-intrinsic scan
/// (name-only). The intrinsifiable forms are the ones whose fixnum fast path is
/// GC-free (or bounces to generic when it would allocate):
///
/// * `logand`/`logior`/`logxor` (2 args) — bitwise of two fixnums is always a
///   fixnum, never overflows (these are `ManySlice` → full generic dispatch
///   today, the biggest win).
/// * `ash` (2 args) — the interpreter has NO fixnum fast path (it always
///   materializes a bignum), so a fixnum shift is a real win; LEFT-shift overflow
///   bounces to generic.
/// * `lognot` (1 arg) — `!n` of a fixnum is always a fixnum.
/// * `mod` (2 args) — floor-modulo of two fixnums is always a fixnum
///   (`|a % b| < |b|`, and the sign-fixup add stays below the divisor's
///   magnitude); a zero divisor bounces to generic (arith-error).
///
/// Other arities stay on the generic path (0 → identity const, ≥3 → reduction).
/// `lsh` is deliberately absent: it is an elisp `defun` (subr.el), not a subr,
/// so it takes the Bytecode spec path and its `ash` call intrinsifies transitively.
pub(crate) fn arith_intrinsic_op_by_name(name: &str, nargs: usize) -> Option<u8> {
    let op = match (name, nargs) {
        ("logand", 2) => ARITH_KIND_LOGAND,
        ("logior", 2) => ARITH_KIND_LOGIOR,
        ("logxor", 2) => ARITH_KIND_LOGXOR,
        ("ash", 2) => ARITH_KIND_ASH,
        ("lognot", 1) => ARITH_KIND_LOGNOT,
        ("mod", 2) => ARITH_KIND_MOD,
        _ => return None,
    };
    Some(op as u8)
}

/// [`subr_spec_armed_reference`]'s answer, with its common case inline: no
/// compiler overrides and no force harness (the two conditions besides a
/// stale epoch that leave its compare, both bits of the attention word) and
/// an epoch equal to the obarray's. The reference then answers `true` as
/// well: a DISARMED slot never equals a live epoch (`next_function_epoch`
/// skips `u64::MAX`), so it would pass its DISARMED test, find no overrides,
/// no force, and an equal epoch. Every other state runs the reference body
/// unchanged.
#[inline(always)]
pub(crate) fn subr_spec_armed(ctx: &mut Context, sym: i64, expected: i64, slot: &SpecSlot) -> bool {
    if ctx.attention_word_clear(AttentionMask::SUBR_ARMING)
        && slot.epoch.load(Ordering::Relaxed)
            == ctx.dispatch_projection_exclusive().function_epoch()
    {
        return true;
    }
    subr_spec_armed_reference(ctx, sym, expected, slot)
}

/// Shared arming check for the three subr spec shims: TRUE iff the site's
/// direct fast path is still valid — the symbol's function cell (validated via
/// the per-site epoch, re-validated on any epoch move exactly like
/// [`neovm_jit_call_spec`]) still holds the compile-time subr VALUE, and no
/// compiler function overrides are active. The overrides check has no
/// bytecode-spec counterpart but is REQUIRED here for interpreter parity: the
/// generic path's `direct_subr_call_target` refuses direct subr dispatch for a
/// SYMBOL callee while overrides are active (they shadow function cells in
/// `resolve_named_call_target_by_id` and live in a VARIABLE, invisible to
/// `function_epoch`), so an armed site must bounce to the generic block then
/// too. Never allocates, never runs lisp — callers rely on this being GC-free.
///
/// This is the reference body; [`subr_spec_armed`] answers the common case
/// (nothing in [`AttentionMask::SUBR_ARMING`], an equal epoch) without it.
/// `#[inline(never)]`, not `#[cold]`: epoch re-validation is routine while
/// `.el` files load.
#[inline(never)]
pub(crate) fn subr_spec_armed_reference(
    ctx: &Context,
    sym: i64,
    expected: i64,
    slot: &SpecSlot,
) -> bool {
    let slot_epoch = slot.epoch.load(Ordering::Relaxed);
    // A loader-DISARMED site never re-arms (see SPEC_EPOCH_DISARMED /
    // neovm_jit_call_spec): the pred/eq shims' fast paths are keyed on the BAKED
    // kind, so re-arming a now-different binding would run the WRONG op. Check it
    // FIRST; JIT never sets DISARMED (perfectly-predicted here).
    if slot_epoch == SPEC_EPOCH_DISARMED {
        return false;
    }
    if ctx.compiler_function_overrides_active() {
        return false;
    }
    let epoch = ctx.dispatch_projection().function_epoch();
    // NEOVM_JIT_FORCE_SLOW_SPEC pretends the armed epoch is stale so every
    // call exercises the re-validate/re-arm branch.
    (!jit_force_slow_spec() && slot_epoch == epoch) || {
        let cur = ctx.obarray.symbol_function_id(SymId(sym as u32));
        if cur.is_some_and(|v| v.bits() as i64 == expected) {
            slot.epoch.store(epoch, Ordering::Relaxed);
            // Drop the screened-entry cache
            // (`SpecSlot::cache_screened_subr_entry`): the epoch move may
            // have followed an IN-PLACE static-subr rewrite, which keeps the
            // VALUE bits this re-arm just re-validated but changes the fn
            // pointer/arity/kind the cache captured. (The subr shims never
            // populate `slot.leaf`; the cache is the only word to clear.)
            slot.clear_screened_subr_entry();
            true
        } else {
            // The subr shims never populate `slot.leaf`; nothing to clear.
            false
        }
    }
}

/// A fixed-arity builtin the armed subr spec shim calls directly: the
/// [`SubrFn`](crate::tagged::header::SubrFn) entry's code address and its
/// slot class (A0..A5 => 0..=5).
#[cfg(feature = "jit")]
#[derive(Clone, Copy)]
struct FixedBuiltinEntry {
    fn_ptr: usize,
    slots: usize,
}

/// The cached screened entry's code address, viewed as the
/// [`SubrFn`](crate::tagged::header::SubrFn) variant its slot class names:
/// written through `addr`, read through the class's fn-pointer field. A
/// union rather than a `transmute` — the architecture transmute guard counts
/// production sites and may only decrease. The validity requirement is the
/// same: the address is one the screen read from a real `SubrFn::AN`, and
/// the class says which N it is.
#[cfg(feature = "jit")]
#[repr(C)]
union ScreenedFn {
    addr: usize,
    a0: SubrFn0,
    a1: SubrFn1,
    a2: SubrFn2,
    a3: SubrFn3,
    a4: SubrFn4,
    a5: SubrFn5,
}

/// Screen `target` as the fixed-arity `Builtin` entry the spec shim may call
/// directly. `None` = take the stack path (not a builtin subr, a Many/slice
/// signature, or an arity the fixed entry does not accept — the stack path
/// signals that one).
///
/// This re-reads the static subr object's entry on EVERY call, as GNU's
/// `funcall_subr` does: `update_static_subr_object_entry` rewrites the fn
/// pointer, arity bounds and dispatch kind IN PLACE keeping the VALUE bits
/// identical, so the armed epoch/bits check alone does not prove the screen.
/// The armed fast path's screened-entry cache
/// (`SpecSlot::cache_screened_subr_entry`) exists precisely because those
/// rewrites always bump `function_epoch`, and the shim's gate compares its
/// slot's epoch with it before trusting the cache.
#[cfg(feature = "jit")]
#[inline]
fn screen_fixed_builtin(target: Value, nargs: usize) -> Option<FixedBuiltinEntry> {
    use crate::tagged::header::{SubrFn, SubrObj, VecLikeType};
    let ptr = target.as_veclike_ptr()?;
    // SAFETY: a veclike pointer from a live Value; the type tag is checked
    // before the SubrObj view is used.
    let subr = unsafe {
        if (*ptr).type_tag != VecLikeType::Subr {
            return None;
        }
        &*(ptr as *const SubrObj)
    };
    if subr.dispatch_kind != SubrDispatchKind::Builtin
        || nargs < usize::from(subr.min_args)
        || subr.max_args.is_some_and(|max| nargs > usize::from(max))
    {
        return None;
    }
    // Only the fixed entries whose slot count covers `nargs` (missing
    // optionals are nil, as the stack dispatcher fills them).
    //
    // Read the entry point ONCE into a local. The arity screen and the call
    // below both discriminate on it, and re-reading it through `subr` made the
    // compiler load and branch on the same field twice on every builtin call
    // out of JIT-compiled code.
    let entry = match subr.function? {
        SubrFn::A0(f) => FixedBuiltinEntry {
            fn_ptr: f as usize,
            slots: 0,
        },
        SubrFn::A1(f) => FixedBuiltinEntry {
            fn_ptr: f as usize,
            slots: 1,
        },
        SubrFn::A2(f) => FixedBuiltinEntry {
            fn_ptr: f as usize,
            slots: 2,
        },
        SubrFn::A3(f) => FixedBuiltinEntry {
            fn_ptr: f as usize,
            slots: 3,
        },
        SubrFn::A4(f) => FixedBuiltinEntry {
            fn_ptr: f as usize,
            slots: 4,
        },
        SubrFn::A5(f) => FixedBuiltinEntry {
            fn_ptr: f as usize,
            slots: 5,
        },
        _ => return None,
    };
    if nargs > entry.slots {
        return None;
    }
    Some(entry)
}

/// Run a screened fixed-arity builtin. Same observable protocol as
/// `Context::call_spec_subr_from_bc_stack`: eval-depth accounting, a
/// backtrace frame recording the SYMBOL (over the native args slot, as the
/// bytecode direct path does), the signal hook, the frame pop.
///
/// `entry` must be one [`screen_fixed_builtin`] produced for this `nargs`
/// (the armed fast path's cached form comes with the same proof, see
/// `SpecSlot::cached_subr_entry`).
///
/// `#[inline(always)]` — measured, not assumed: as an out-of-line 0x338-byte
/// helper it spent its own 6-push prologue and 6-pop epilogue (~14 Ir) on
/// every armed builtin call (1.14M per elb-bytecomp run). The callers already
/// hold every value the body needs (Context, the args slot, nargs, the
/// patched frame index), and the body's own callee-saved set is a subset of
/// the shim's, so the inline form only adds a class-dispatch jump table and
/// the builtin call site to the caller. See the turn-2 report for the
/// measured hit-path Ir and the shim's census size.
#[cfg(feature = "jit")]
#[inline(always)]
fn run_screened_fixed_builtin(
    ctx: &mut Context,
    sym_id: SymId,
    entry: FixedBuiltinEntry,
    args_ptr: *const i64,
    nargs: usize,
) -> crate::emacs_core::error::EvalResult {
    // SAFETY: the generated code stored exactly `nargs` argument words at
    // `args_ptr` (its call-args slot) immediately before this call.
    let arg = |i: usize| {
        if i < nargs {
            unsafe { Value::from_bits(*args_ptr.add(i) as usize) }
        } else {
            Value::NIL
        }
    };
    let bt_count = ctx.specpdl.len();
    // SAFETY: `args_ptr` stays valid for the whole call (the caller's own
    // stack slot); the frame is popped below before returning.
    unsafe {
        ctx.push_backtrace_frame_from_native_args(Value::from_sym_id(sym_id), args_ptr, nargs)
    };
    ctx.depth += 1;
    if ctx.depth > ctx.max_depth
        && let Err(flow) = Vm::from_context(ctx).bytecode_depth_exceeded()
    {
        // GNU checks depth before recording this rejected callee's frame.
        // Dispatch at the counted depth, with the caller's native roots live.
        let flow = ctx
            .pop_bytecode_backtrace_frame_with_result(bt_count, Err(flow))
            .expect_err("a rejected callee has no cleanup or result");
        return Err(ctx.finish_lisp_depth_overflow(flow));
    }
    // SAFETY: `entry` is the `SubrFn` variant its slot class was screened
    // from, and the call's arguments follow that variant's arity; the union
    // view reads exactly that variant's field.
    let fns = ScreenedFn { addr: entry.fn_ptr };
    let result = unsafe {
        match entry.slots {
            0 => (fns.a0)(ctx),
            1 => (fns.a1)(ctx, arg(0)),
            2 => (fns.a2)(ctx, arg(0), arg(1)),
            3 => (fns.a3)(ctx, arg(0), arg(1), arg(2)),
            4 => (fns.a4)(ctx, arg(0), arg(1), arg(2), arg(3)),
            5 => (fns.a5)(ctx, arg(0), arg(1), arg(2), arg(3), arg(4)),
            _ => unreachable!("screened classes are the A0..A5 entries"),
        }
    };
    ctx.depth -= 1;
    // Extract the value before cleanup. Passing the whole Result through
    // the signal/unwind joins made LLVM copy its 16-byte aggregate with wide
    // loads after the builtin's narrow stores, even on balanced Ok returns.
    match result {
        Ok(value) => {
            if ctx.pop_native_backtrace_frame(bt_count) {
                return Ok(value);
            }
            finish_fixed_builtin_from_native(ctx, bt_count, Ok(value))
        }
        Err(flow) => finish_fixed_builtin_from_native(ctx, bt_count, Err(flow)),
    }
}

/// The spec shim's direct fixed-arity builtin call: `target` is the armed
/// `#<subr>` value. `None` = take the stack path (see
/// [`screen_fixed_builtin`]). On a successful screen the entry is cached in
/// `slot` for the armed fast path's next calls
/// (`SpecSlot::cache_screened_subr_entry`).
#[cfg(feature = "jit")]
fn call_fixed_builtin_from_native(
    ctx: &mut Context,
    sym_id: SymId,
    target: Value,
    slot: &SpecSlot,
    args_ptr: *const i64,
    nargs: usize,
) -> Option<crate::emacs_core::error::EvalResult> {
    let entry = screen_fixed_builtin(target, nargs)?;
    if let Some(encoded) = SpecSlot::pack_screened_subr_entry(entry.fn_ptr, entry.slots) {
        slot.cache_screened_subr_entry(encoded);
    }
    Some(run_screened_fixed_builtin(
        ctx, sym_id, entry, args_ptr, nargs,
    ))
}

/// Preserve signal-hook ordering and the general unwinder for non-local
/// returns, debugger exit flags, or bindings left above the native frame.
/// Keeping these aggregate Result moves out of line lets the common return
/// carry only the successful Value through its frame pop.
#[cfg(feature = "jit")]
#[cold]
#[inline(never)]
fn finish_fixed_builtin_from_native(
    ctx: &mut Context,
    bt_count: usize,
    result: crate::emacs_core::error::EvalResult,
) -> crate::emacs_core::error::EvalResult {
    let result = ctx.dispatch_signal_result_if_needed(result);
    ctx.pop_bytecode_backtrace_frame_with_result(bt_count, result)
}

/// GNU `Bcall`'s frame for a leaf builtin that signalled from an `Op::Call`
/// site (design `p1-2-builtin-intrinsics` §2.6): record the call as
/// `record_in_backtrace` would have (`bytecode.c:795`) -- the site's SYMBOL
/// over the caller's argument words and the call's own count, exactly the
/// frame [`call_fixed_builtin_from_native`] pushes -- then dispatch the
/// leaf's stashed, undispatched signal with it in place (signal hook,
/// `handler-bind`, the debugger) and pop it: the same sequence that
/// function runs after its builtin returns `Err`, at the same depth (the
/// caller's). The leaf's success path pushes no frame at all; only Lisp
/// running during the call could see one, and the only Lisp that can run
/// is this dispatch.
///
/// The generated code calls this by address (JIT only), from the leaf
/// site's cold signal block, with the residual operand stack rooted and the
/// arguments spilled to its call-args slot (which outlives the frame: it is
/// popped here).
///
/// A panic contained in the leaf leaves its marker for the caller's healing
/// points (the seam-diet-2 review rule): this shim must not take the flow
/// then, and answers STATUS_SIGNAL straight away.
///
/// SAFETY: the vmctx contract of [`neovm_jit_call`]; `args_ptr` addresses
/// `nargs` argument words; `out` is the site's result slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_leaf_signal_frame(
    ctx: *mut u8,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        if shim_panic_pending() {
            return STATUS_SIGNAL;
        }
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        // The raw take: no panic materialization (checked above).
        let Some(flow) = PENDING_FLOW.with(|p| p.borrow_mut().take()) else {
            unreachable!("a leaf answers LEAF_SIGNAL only after stashing its flow")
        };
        let bt_count = ctx.specpdl.len();
        // SAFETY: `args_ptr` is the site's call-args slot, valid until this
        // shim returns; the frame is popped below before that.
        unsafe {
            ctx.push_backtrace_frame_from_native_args(
                Value::from_sym_id(SymId(sym as u32)),
                args_ptr,
                nargs as usize,
            )
        };
        match finish_fixed_builtin_from_native(ctx, bt_count, Err(flow)) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}

/// Speculated direct SUBR call (`Op::Call` whose callee slot provably holds a
/// constant symbol fbound at compile time to a fixed-arity builtin subr — see
/// `find_spec_sites`' subr classification). Quit poll FIRST (interpreter
/// `Op::Call` order), then the validity check ([`subr_spec_armed`]):
///
/// * ARMED — the cell still holds the compile-time `#<subr>` VALUE. Dispatch
///   it directly via `Vm::call_spec_subr_stack`, which replicates the generic
///   path's subr protocol exactly (depth guard, backtrace frame recording the
///   SYMBOL, arity signal against the FRESH entry, A0..A8 stack-args dispatch,
///   debugger hook) minus the symbol resolution. The `SubrEntry` is re-read
///   from the subr object on EVERY call: `update_static_subr_object_entry`
///   rewrites entries IN PLACE keeping the value bits identical, so the fn
///   pointer must never be cached across calls (only the stable VALUE bits are
///   baked).
/// * NOT ARMED — return [`STATUS_NEED_GENERIC`] with no side effects; the
///   generated fallback block re-does this site as a plain generic call on the
///   SYMBOL (fset/advice/overrides take effect immediately, GNU parity).
///
/// GC rooting: the args are pushed onto the GC-traced `bc_buf` (rooted across
/// the subr call, which can allocate/GC/signal) — the same rooting scheme as
/// [`neovm_jit_call`]; the generated code rooted its residual operand stack
/// before this call. The recursion-depth guard is applied inside
/// `call_spec_subr_stack` via `with_bytecode_call_depth`, exactly where
/// `call_for_jit_stack` applies it on the generic path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `slot` points into the
/// owning CompiledLeaf's spec_slots (alive whenever its code runs).
///
/// AOT-importable (increment B2): the `Op::Call` subr spec sites are now emitted
/// by the AOT baseline tier too (`build_baseline_leaf_object` with `Some(obarray)`),
/// so an AOT `.so` may import this symbol. It is therefore host-exported
/// (`#[unsafe(no_mangle)] pub`) + in `shim_names.rs`/`MIR_SHIM_NAMES` (salted into
/// `ABI_TAG`) + anchored by `JIT_SHIM_TABLE`. Cross-session soundness rides on the
/// per-site epoch guard + the loader's DISARM sentinel (`SPEC_EPOCH_DISARMED`), not
/// on any baked address — the shim re-reads the live entry every call.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call_subr_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(detach args_ptr, ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        // One gate for the common case: nothing for the quit poll, no
        // compiler overrides, no force harness (the attention words), an
        // armed epoch, the debugger down. Then the reference sequence below
        // would poll and find nothing, arm (`subr_spec_armed`'s own fast
        // answer), and take the direct fixed-arity call -- so this does.
        // Every other state runs that sequence verbatim.
        if ctx.attention_clear(AttentionMask::SPEC_SUBR)
            && {
                let mut projection = ctx.dispatch_projection_exclusive();
                slot.epoch.load(Ordering::Relaxed) == projection.function_epoch()
                    && !projection.debug_on_next_call_armed_fast()
            }
        {
            #[cfg(debug_assertions)]
            SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
            let sym_id = SymId(sym as u32);
            // The slot's screened-entry cache
            // (`SpecSlot::cache_screened_subr_entry`): a hit means the epoch
            // equals the one the entry was screened under, so the tag,
            // dispatch-kind and arity screens and the `SubrFn` class match
            // are already proven — call the builtin body directly. The
            // site's `nargs` is its compile-time arity, so the cached class
            // covers every call through this slot; the compare stays as a
            // guard on the shim's ABI contract.
            let cached = slot.cached_subr_entry();
            let screened = if cached != 0 {
                let (fn_ptr, slots) = SpecSlot::decode_screened_subr_entry(cached);
                if nargs <= slots {
                    #[cfg(debug_assertions)]
                    SUBR_SPEC_CACHED_COUNT.fetch_add(1, Ordering::Relaxed);
                    Some(FixedBuiltinEntry { fn_ptr, slots })
                } else {
                    None
                }
            } else {
                None
            };
            let res = match screened {
                Some(entry) => Some(run_screened_fixed_builtin(
                    ctx, sym_id, entry, args_ptr, nargs,
                )),
                None => {
                    let target = Value::from_bits(expected as usize);
                    call_fixed_builtin_from_native(ctx, sym_id, target, slot, args_ptr, nargs)
                }
            };
            if let Some(res) = res {
                return match res {
                    Ok(value) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    Err(flow) => {
                        stash_pending_flow(flow);
                        STATUS_SIGNAL
                    }
                };
            }
            return call_stack_builtin_from_native(
                ctx,
                sym_id,
                Value::from_bits(expected as usize),
                args_ptr,
                nargs,
                out,
            );
        }
        subr_spec_call_reference(ctx, sym, expected, slot, args_ptr, nargs, out)
    })
}

/// [`neovm_jit_call_subr_spec`]'s reference sequence: the quit poll first
/// (interpreter `Op::Call` order), then the arming check, then the direct
/// fixed-arity call or the stack path. Runs inside the shim's containment
/// frame. `#[inline(never)]`, not `#[cold]`: re-validation after an epoch
/// move is routine while `.el` files load.
#[inline(never)]
fn subr_spec_call_reference(
    ctx: &mut Context,
    sym: i64,
    expected: i64,
    slot: &SpecSlot,
    args_ptr: *const i64,
    nargs: usize,
    out: *mut i64,
) -> i64 {
    if let Err(flow) = ctx.maybe_quit() {
        stash_pending_flow(flow);
        return STATUS_SIGNAL;
    }
    if !subr_spec_armed(ctx, sym, expected, slot) {
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        return STATUS_NEED_GENERIC;
    }
    #[cfg(debug_assertions)]
    SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
    let target = Value::from_bits(expected as usize);
    // Direct fixed-arity call (the common case): the subr object is re-read
    // on EVERY call (its entry is rewritten in place at registration, so the
    // fn pointer is never cached), the frame records the SYMBOL over the
    // native args slot, and the builtin is called by pointer with the args
    // in registers — no operand-stack copy, no span frame, no dispatch hop.
    // Everything else (Many/slice subrs, an arity the fixed entry rejects,
    // a debugger armed) takes the stack path below, the reference protocol.
    if !ctx.debug_on_next_call_is_armed()
        && let Some(res) =
            call_fixed_builtin_from_native(ctx, SymId(sym as u32), target, slot, args_ptr, nargs)
    {
        return match res {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
    }
    call_stack_builtin_from_native(ctx, SymId(sym as u32), target, args_ptr, nargs, out)
}

/// Stack-argument fallback for an armed native subr call. Keep its argument
/// staging and bytecode-frame temporaries out of the fixed-arity shim's stack
/// layout. This still runs inside that shim's panic-containment boundary.
#[inline(never)]
fn call_stack_builtin_from_native(
    ctx: &mut Context,
    sym_id: SymId,
    target: Value,
    args_ptr: *const i64,
    nargs: usize,
    out: *mut i64,
) -> i64 {
    let saved = save_scratch_gc_roots();
    // The args are GC-rooted on bc_buf and dispatched directly on Context.
    // Static subr objects are leaked, so the armed target needs no extra root.
    let args_start = ctx.bc_buf.len();
    // SAFETY: the calling shim's generated caller stored exactly `nargs`
    // tagged words at `args_ptr`; that native slot outlives this call.
    ctx.bc_buf
        .extend((0..nargs).map(|i| Value::from_bits(unsafe { *args_ptr.add(i) } as usize)));
    let res = ctx.call_spec_subr_from_bc_stack(sym_id, target, args_start, nargs);
    ctx.bc_buf.truncate(args_start);
    let status = match res {
        Ok(value) => {
            // SAFETY: `out` is the generated caller's result stack slot.
            unsafe { *out = value.bits() as i64 };
            STATUS_OK
        }
        Err(flow) => {
            stash_pending_flow(flow);
            STATUS_SIGNAL
        }
    };
    restore_scratch_gc_roots(saved);
    status
}

/// Speculated PREDICATE call: `recordp` / `symbol-with-pos-p` sites collapse
/// to a hardcoded tag test when armed. Both predicates are pure single-tag
/// checks, verified exact against their builtins (`builtin_recordp_1` =
/// `Value::is_record`, `builtin_symbol_with_pos_p_1` =
/// `Value::is_symbol_with_pos`; both independent of
/// `symbols-with-pos-enabled` — which is why `keywordp`/`symbolp` are General
/// only). NOT ARMED → [`STATUS_NEED_GENERIC`]; the generated fallback block
/// runs the plain generic call (full rooting, override/redefinition parity).
///
/// GC-FREE INVARIANT (load-bearing): the generated code does NOT root its
/// residual operand stack around this call, so no path here may allocate on
/// the lisp heap or run lisp code. That holds: `maybe_quit`'s quit/throw Flow
/// construction is pure Rust-heap (`signal(LispCondition::Quit, vec![])` — no lisp
/// allocation), `subr_spec_armed` only reads, and the armed test is a tag
/// check on an immediate/heap header. The skipped backtrace frame is
/// unobservable: the armed path cannot signal, GC, or run lisp between frame
/// push and pop.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_pred_spec(
    ctx: *mut u8,
    kind: i64,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        // GNU `Ffuncall` runs the debugger on entry to ANY function while
        // `debug-on-next-call` is set, a subr included: that call takes the
        // generic path, as `neovm_jit_call_subr_spec`'s does.
        // GNU's `Bcall` counts a subr callee against `max-lisp-eval-depth`
        // (`++lisp_eval_depth > max_lisp_eval_depth` before `funcall_subr`),
        // and so does the generic path here (`call_fixed_builtin_from_native`).
        // A call at the limit takes that path, which raises the floor and
        // signals exactly as the interpreter would; the intrinsic never
        // nests, so anywhere below the limit the count is unobservable.
        if ctx.debug_on_next_call_is_armed()
            || ctx.depth >= ctx.max_depth
            || !subr_spec_armed(ctx, sym, expected, slot)
        {
            #[cfg(debug_assertions)]
            SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let v = Value::from_bits(a as usize);
        let result = match kind {
            // `is_record`/`is_symbol_with_pos` = tag check BEFORE any header
            // deref (`veclike_type` guards on `is_veclike` first) — safe on
            // immediates.
            PRED_KIND_RECORDP => Value::bool_val(v.is_record()),
            PRED_KIND_SYMBOL_WITH_POS_P => Value::bool_val(v.is_symbol_with_pos()),
            // `builtin_fboundp_1`: a symbol's function cell, read off the
            // obarray with no allocation, no Lisp and no safe point (a
            // symbol-with-pos is a symbol here exactly when the builtin says
            // so). Anything else bounces to the generic call, which signals
            // `(wrong-type-argument symbolp V)` from a real frame.
            // `plan_autoload_do_load_in_state`: a FUNDEF that is not an
            // `(autoload ...)` cons is returned as it is, whatever FUNNAME and
            // MACRO-ONLY say (they are not read on that path; the site passes
            // only FUNDEF here). An autoload object bounces to the generic
            // call, which does the load with its frame and roots.
            PRED_KIND_AUTOLOAD_DO_LOAD => {
                if crate::emacs_core::autoload::is_autoload_value(&v, ctx.symbol_mode()) {
                    return STATUS_NEED_GENERIC;
                }
                v
            }
            PRED_KIND_FBOUNDP => {
                let Some(id) = crate::emacs_core::builtins::symbols::symbol_id_checked(
                    &v,
                    ctx.symbols_with_pos_enabled,
                ) else {
                    return STATUS_NEED_GENERIC;
                };
                Value::bool_val(
                    crate::emacs_core::builtins::symbols::function_cell_is_bound(ctx.obarray(), id),
                )
            }
            // The registered `type-of`/`cl-type-of` bodies, which read the
            // value and return an existing symbol or a record's type slot:
            // no allocation, no Lisp, no safe point. An error (none is
            // reachable at arity 1) bounces to the generic call to raise it.
            // A record answers the same for both (a `cl-defstruct`
            // predicate is `(memq (type-of x) TAGS)`): no decode, no Result.
            _ if let Some(type_symbol) = crate::emacs_core::builtins::types::record_type_of(v) => {
                type_symbol
            }
            _ => match pred_spec_type_of_non_record(ctx, kind, v) {
                Some(value) => value,
                None => return STATUS_NEED_GENERIC,
            },
        };
        // SAFETY: `out` is the generated code's result stack slot.
        unsafe { *out = result.bits() as i64 };
        STATUS_OK
    })
}

/// Speculated `fboundp` call: the dedicated 4-argument shim. The sequence is
/// exactly [`neovm_jit_pred_spec`]'s `PRED_KIND_FBOUNDP` arm — the quit poll,
/// the debugger/depth gates, the armed-epoch check and the symbol's function
/// cell read — without the `kind` argument, the site's `sym`/`expected` words
/// and the arm dispatch. Those two words are read back from the slot
/// (`SpecSlot::for_site` binds a `PredFboundp` slot like a `SubrGeneral` one)
/// so the arming test is the full [`subr_spec_armed`]: a stale epoch whose
/// live binding bits still match RE-ARMS the site and keeps the fast path,
/// and only a changed binding (or an armed debugger, the depth limit, a
/// pending overrides state) takes [`STATUS_NEED_GENERIC`] and the site's
/// generic fallback — which then resolves the live binding, so `fset`/advice
/// and the generic dispatch's Bcall depth accounting stay exact. Falling
/// straight to the generic call on every epoch move would make one `fset`
/// anywhere permanently slow every `fboundp` site.
///
/// GC-FREE INVARIANT (load-bearing): the generated caller roots nothing
/// around this call, so no path here may allocate on the lisp heap or run
/// lisp. `maybe_quit`'s Flow is Rust-heap only, the arming check only reads,
/// and the cell read is a symbol-table lookup with no allocation or safe
/// point. The skipped backtrace frame is unobservable: an armed hit cannot
/// signal, GC or run lisp between push and pop.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_fboundp_spec(ctx: *mut u8, slot: i64, a: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        {
            SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
            FBOUNDP_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        debug_assert!(
            slot.holds_subr_binding(),
            "an fboundp site's slot carries its binding words"
        );
        let (sym, expected) = slot.subr_binding();
        if ctx.debug_on_next_call_is_armed()
            || ctx.depth >= ctx.max_depth
            || !subr_spec_armed(ctx, sym.0 as i64, expected as i64, slot)
        {
            #[cfg(debug_assertions)]
            SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let v = Value::from_bits(a as usize);
        // `builtin_fboundp_1`: a symbol's function cell, read off the obarray
        // with no allocation, no Lisp and no safe point (a symbol-with-pos is
        // a symbol here exactly when the builtin says so, i.e. only while
        // `symbols-with-pos-enabled` is on). Anything else bounces to the
        // generic call, which signals `(wrong-type-argument symbolp V)` from a
        // real frame.
        let Some(id) = crate::emacs_core::builtins::symbols::symbol_id_checked(
            &v,
            ctx.symbols_with_pos_enabled,
        ) else {
            return STATUS_NEED_GENERIC;
        };
        let result = Value::bool_val(
            crate::emacs_core::builtins::symbols::function_cell_is_bound(ctx.obarray(), id),
        );
        // SAFETY: `out` is the generated code's result stack slot.
        unsafe { *out = result.bits() as i64 };
        STATUS_OK
    })
}

/// The `type-of`/`cl-type-of` arm of [`neovm_jit_pred_spec`] for a value that
/// is not a record: the registered builtin bodies, which classify by tag and
/// header and return an existing symbol (no allocation, no Lisp, no safe
/// point). `None` = the builtin signalled (unreachable at arity 1; the site
/// bounces to the generic call, which raises it from a real frame). Out of
/// line: the two classifiers are large, and the shim's other arms are a few
/// loads each -- keeping them in the shim body cost every predicate call
/// their register spills.
#[cold]
#[inline(never)]
fn pred_spec_type_of_non_record(ctx: &mut Context, kind: i64, v: Value) -> Option<Value> {
    let answer = if kind == PRED_KIND_TYPE_OF {
        crate::emacs_core::builtins::types::builtin_type_of_with_ctx_1(ctx, v)
    } else {
        debug_assert_eq!(kind, PRED_KIND_CL_TYPE_OF);
        crate::emacs_core::builtins::types::builtin_cl_type_of(&[v])
    };
    answer.ok()
}

/// Speculated `equal-including-properties` call (2 args): when armed and the
/// arguments are BITWISE equal, the answer is `t` with zero dispatch —
/// bit-equal ⟹ same object ⟹ equal-including-properties, which is literally
/// the builtin's own first check (`try_equal_value_inner`'s
/// `left.bits() == right.bits()`), so this covers same-object strings/conses,
/// identical NaN boxes, fixnums, and interned symbols alike.
///
/// On a bitwise MISS the shim returns [`STATUS_NEED_GENERIC`] instead of
/// invoking the builtin here: the deep comparison can allocate (its
/// depth-overflow arm signals with a freshly allocated lisp string), and this
/// site's direct path deliberately skips the residual-stack rooting — calling
/// the builtin from here could GC with the caller's live registers unrooted.
/// The generic fallback block does the fully-rooted call instead; the miss
/// cost is one epoch check + bit compare on top of a deep structural
/// comparison, i.e. noise. This keeps the same GC-FREE INVARIANT as
/// [`neovm_jit_pred_spec`] (no allocation / no lisp on ANY path here).
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_eq_incl_props_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if subr_spec_armed(ctx, sym, expected, slot) && a == b {
            #[cfg(debug_assertions)]
            SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
            // SAFETY: `out` is the generated code's result stack slot.
            unsafe { *out = Value::T.bits() as i64 };
            return STATUS_OK;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        STATUS_NEED_GENERIC
    })
}

/// Fixnum fast path for `(ash value count)` — mirrors GNU `Fash` /
/// `builtin_ash_slice` for the both-fixnum case, or `None` when the result would
/// leave fixnum range (so the shim bounces to the allocating generic path):
///
/// * `count == 0` → `value` unchanged.
/// * `count > 0` (left shift) → `value << count`, but ONLY when it is exactly
///   reversible (`(sh >> count) == value` rejects any bit lost to i64 overflow)
///   AND lands in fixnum range; otherwise `None` (the real result is a bignum).
///   `count >= 64` is rejected up front (an i64 shift by ≥64 is undefined).
/// * `count < 0` (arithmetic right shift, floor toward −∞ = `mpz_fdiv_q_2exp`) →
///   always a fixnum (magnitude only shrinks); huge shifts clamp to a 63-bit
///   sign fill (−1 for negative `value`, 0 otherwise), matching GNU.
pub(crate) fn ash_fixnum_fast(value: i64, count: i64) -> Option<i64> {
    if count == 0 {
        Some(value)
    } else if count > 0 {
        if count >= 64 {
            return None;
        }
        let sh = value.checked_shl(count as u32)?;
        if (sh >> count) == value
            && (Value::MOST_NEGATIVE_FIXNUM..=Value::MOST_POSITIVE_FIXNUM).contains(&sh)
        {
            Some(sh)
        } else {
            None
        }
    } else {
        // count < 0. `-count` is safe (count ≥ MOST_NEGATIVE_FIXNUM > i64::MIN).
        let bits = if count <= -63 { 63 } else { (-count) as u32 };
        Some(value >> bits)
    }
}

/// The generic fallback of an arithmetic/comparison site that feedback says
/// misses its fixnum fast path (`compile::arith_site_takes_generic`): the
/// interpreter's own slow arm (`Vm::call_arith_builtin_on_context` — the
/// static builtin, no backtrace frame, the debugger check on a signal) on the
/// operands `a` and (for a binary kind) `b`, passed in registers. `kind` is
/// an `ArithGenericKind` discriminant; a unary kind ignores `b`.
///
/// The operands go onto the GC-traced `bc_buf` for the call, as in
/// [`neovm_jit_call_subr_spec`]; the generated code rooted its residual stack
/// around this call. No `maybe_quit`: the opcode arms poll none either.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_arith_generic(
    ctx: *mut u8,
    kind: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        use crate::emacs_core::bytecode::{ArithGenericKind, Vm};
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let Some(kind) = ArithGenericKind::from_raw(kind) else {
            stash_pending_flow(signal(
                crate::emacs_core::error::LispCondition::InvalidFunction,
                vec![Value::fixnum(kind)],
            ));
            return STATUS_SIGNAL;
        };
        let nargs = kind.arity();
        // The all-integer answer first, straight from the registers: it
        // cannot reach a safe point, so `a` and `b` need no staging.
        let operands = [Value::from_bits(a as usize), Value::from_bits(b as usize)];
        if let Some(value) = Vm::arith_integer_fast(kind, &operands[..nargs]) {
            // SAFETY: `out` is the generated code's result stack slot.
            unsafe { *out = value.bits() as i64 };
            return STATUS_OK;
        }
        let args_start = ctx.bc_buf.len();
        ctx.bc_buf.push(operands[0]);
        if nargs == 2 {
            ctx.bc_buf.push(operands[1]);
        }
        let res = Vm::call_arith_builtin_slow_on_context(ctx, kind, args_start, nargs);
        ctx.bc_buf.truncate(args_start);
        // Value first: an `unwrap_or_else` on the `Option<EvalResult>` copied
        // it 16 bytes wide over the callee's narrow stores.
        match res {
            Some(Ok(value)) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Some(Err(flow)) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
            None => {
                stash_pending_flow(signal(
                    crate::emacs_core::error::LispCondition::VoidFunction,
                    vec![Value::from_sym_id(kind.builtin_id())],
                ));
                STATUS_SIGNAL
            }
        }
    })
}

/// Speculated bitwise-arithmetic call: `logand`/`logior`/`logxor`/`ash` (2 args)
/// and `lognot` (1 arg). When armed AND the argument(s) are fixnums, computes the
/// native op with zero dispatch — EXACTLY the interpreter's fixnum semantics:
/// `&`/`|`/`^` (`builtin_logand_slice` et al., always a fixnum, never overflows),
/// `!n` (`builtin_lognot`, always a fixnum), and the fixnum `ash` fast path
/// ([`ash_fixnum_fast`], which the interpreter LACKS — it always materializes a
/// bignum). `Value::fixnum` never overflows on these results and the shim never
/// allocates.
///
/// Anything else — NOT armed (callee redefined), an arg not a fixnum (marker /
/// bignum / wrong-type), or an `ash` LEFT-shift whose result leaves fixnum range
/// — returns [`STATUS_NEED_GENERIC`], and the generated fallback runs the plain
/// generic call reaching the SAME builtin body (bignum via GMP, marker→position,
/// wrong-type signal, `ash` overflow→bignum/overflow-error). The
/// deep/allocating cases therefore only run on the fully-rooted fallback path,
/// keeping the GC-FREE INVARIANT of [`neovm_jit_eq_incl_props_spec`] (no
/// allocation / no lisp on ANY path here; `maybe_quit` is Rust-heap only).
/// `op` is baked as an iconst (`ARITH_KIND_*`); each op also has a distinct
/// [`SpecCalleeKind::to_spec_disc`], so an AOT site whose baked op no longer
/// matches the live callee is disarmed by the loader (never run with a wrong op).
/// For `lognot` (1 arg) the generated code passes a dummy `b`; the shim ignores it.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_arith_spec(
    ctx: *mut u8,
    kind: i64,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if subr_spec_armed(ctx, sym, expected, slot) {
            // Single `match kind` (jump table) so the hot and/or/xor path has no
            // extra dispatch. `None` on any non-fixnum arg or an `ash` overflow ->
            // the generic bounce below. lognot is 1-arg (dummy `b`, ignored); the
            // 2-arg ops read both operands only inside their own arm.
            let fix2 = || match (
                Value::from_bits(a as usize).as_fixnum(),
                Value::from_bits(b as usize).as_fixnum(),
            ) {
                (Some(l), Some(r)) => Some((l, r)),
                _ => None,
            };
            let res: Option<i64> = match kind {
                ARITH_KIND_LOGAND => fix2().map(|(l, r)| l & r),
                ARITH_KIND_LOGIOR => fix2().map(|(l, r)| l | r),
                ARITH_KIND_LOGXOR => fix2().map(|(l, r)| l ^ r),
                ARITH_KIND_ASH => fix2().and_then(|(l, r)| ash_fixnum_fast(l, r)),
                // GNU Fmod integer branch: truncated rem, then pull the
                // result onto the divisor's side of zero.
                ARITH_KIND_MOD => fix2().and_then(|(l, r)| {
                    if r == 0 {
                        return None;
                    }
                    let m = l % r;
                    Some(if m != 0 && ((m < 0) != (r < 0)) {
                        m + r
                    } else {
                        m
                    })
                }),
                _ => {
                    debug_assert_eq!(kind, ARITH_KIND_LOGNOT);
                    Value::from_bits(a as usize).as_fixnum().map(|n| !n)
                }
            };
            if let Some(res) = res {
                #[cfg(debug_assertions)]
                SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = Value::fixnum(res).bits() as i64 };
                return STATUS_OK;
            }
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        STATUS_NEED_GENERIC
    })
}

/// R2 Tier-B CallBuiltinSym intrinsic (dispatch-skip): reproduce the
/// `Op::CallBuiltinSym` interpreter arm EXACTLY, skipping only the
/// `resolve_sym` → `builtin_name_id` → `dispatch_vm_builtin_unrooted`
/// special-name string-switch round trip. CallBuiltinSym name-dispatches the
/// STATIC subr table (`subr_from_sym_id(builtin_name_id(resolve_sym(sym)))`,
/// vm.rs) and is advice/fset/override IMMUNE, so there is NO epoch guard and NO
/// `compiler_function_overrides_active` gate: the shim RE-READS the live static
/// entry each call (`lookup_global_subr_entry`, safe against in-place rewrites)
/// and either
///
/// * `Some` + `dispatch_kind == Builtin` — dispatches via `funcall_general` on
///   the name-canonical SUBR value (`subr_from_sym_id(sym)`; canonicalizes a
///   name-alias, `Box::leak` process-stable). `funcall_general` IS the arm's
///   own dispatch (vm.rs → `dispatch_vm_builtin_unrooted` → `funcall_general`),
///   so this is byte-identical: a SUBR-value backtrace frame, the arity check
///   vs the FRESH entry signalling `wrong-number-of-arguments` with the SUBR
///   payload, exact-slice args (`Many` gets `into_vec`; fixed arity nil-pads
///   after the arity gate), the debugger `dispatch_signal_result_if_needed`,
///   and `unbind_to_with_result` — WITHOUT `with_bytecode_call_depth` (open-
///   coded ops add no lisp-eval-depth level). The `maybe_quit` poll runs AFTER
///   the op (GNU `Op::CallBuiltinSym` order), only on the Ok path. The R2 ship
///   set excludes `aset`/`fillarray`, so the arm's mutating-first-arg writeback
///   never applies and is correctly absent here.
/// * anything else — [`STATUS_NEED_GENERIC`]. The per-site generated fallback
///   re-runs the ORIGINAL general CBSym lowering (`neovm_jit_named_builtin`
///   variant 1 → `Vm::callbuiltinsym_for_jit`), which reproduces
///   void-function / invalid-function / the special-name arm exactly. Deferring
///   every edge case there keeps THIS shim's correctness obligation to the
///   clean `Some`+`Builtin` case only.
///
/// GC rooting: args are pushed onto the GC-traced `bc_buf` (rooted across the
/// subr, which can allocate/GC/signal) — the [`neovm_jit_call_subr_spec`]
/// scheme; the generated code rooted its residual operand stack via
/// gc_save/gc_push before this call, and `funcall_general` additionally roots
/// the args in its backtrace frame. `subr_from_sym_id`'s canonical subr object
/// is `Box::leak`'d, so the callee value needs no root.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
///
/// R2 increment A (CBSym-in-AOT): EXPORTED (`#[unsafe(no_mangle)] pub` + listed
/// in `shim_names.rs`/`MIR_SHIM_NAMES` + anchored by `JIT_SHIM_TABLE`). The
/// CBSym classification (`cbsym_spec_kind`) is name-canonical + obarray-free
/// (static subr table + name resolution only), so the AOT baseline emit
/// (`build_baseline_leaf_object`, `obarray=None`) now classifies CBSym sites too —
/// an AOT `.so` may import this shim and binds it against the host at `dlopen`.
/// (The round-1 `Op::Call` subr-spec shims stay JIT-only: their `find_spec_sites`
/// pass still requires `Some(obarray)` — that's increment B.)
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cbsym_spec(
    ctx: *mut u8,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let sym_id = SymId(sym as u32);
        // Advice/fset/override-immune: NO epoch guard. Re-read the live static entry
        // (in-place-rewrite safe). Not a plain builtin now → bounce so the general
        // shim reproduces void/invalid-function exactly. FORCE_CBSYM_GENERIC forces
        // every classified site down this bounce (harness).
        // GNU's inline opcodes (bytecode.c `Bpoint`..`Bwiden`,
        // `Bset_marker`..`Bdowncase`) call the C primitive directly: no
        // funcall, no backtrace frame, no arity check. Dispatch the registered
        // builtin straight off the argument slot the same way the interpreter's
        // `Op::CallBuiltinSym` arm does; the VM-owned specials and anything not
        // registered as a plain builtin bounce to the generic shim.
        let inline = crate::emacs_core::eval::inline_subr(sym_id);
        let inline_kind = inline.kind;
        let function = if force_cbsym_generic() {
            None
        } else {
            inline.function
        };
        let Some(function) = function else {
            #[cfg(debug_assertions)]
            CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        };
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let saved = save_scratch_gc_roots();
        let args_start = ctx.bc_buf.len();
        for i in 0..nargs {
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            ctx.bc_buf.push(v);
        }
        // The operand-stack copy above is what roots the arguments while the
        // builtin runs; for a small fixed-arity subr the call itself skips the
        // by-symbol dispatch layers (same shape as the interpreter's arm).
        let res = if inline_kind == crate::emacs_core::eval::InlineSubrKind::Direct {
            Vm::call_fixed_builtin_direct(ctx, Some(function), sym_id, args_start, nargs)
        } else {
            Vm::call_inline_builtin_from_stack(ctx, function, args_start, nargs)
        };
        ctx.bc_buf.truncate(args_start);
        let status = match res {
            Ok(value) => {
                // Quit poll AFTER the op (GNU Op::CallBuiltinSym order; the arm polls
                // maybe_quit only once the result is computed, and never on the
                // error path).
                match ctx.maybe_quit() {
                    Ok(()) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    Err(flow) => {
                        stash_pending_flow(flow);
                        STATUS_SIGNAL
                    }
                }
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// The fixed arity of a Tier-A CallBuiltinSym read (all 0-arg except
/// match-beginning/match-end, which take the group number). The shim bounces to
/// STATUS_NEED_GENERIC on an arity mismatch so the general path signals
/// wrong-number-of-arguments identically to the interpreter.
#[inline]
pub(crate) fn cbsym_read_expected_nargs(which: u8) -> usize {
    match which {
        CBSYM_A_MATCH_BEGINNING | CBSYM_A_MATCH_END => 1,
        _ => 0,
    }
}

/// R2 Tier-A CallBuiltinSym intrinsic (GC-free read): DELEGATE to the REAL
/// builtin body for `which`, never reimplement — a byte→char conversion
/// (match-beginning) or a three-case boundary (bolp) can then never drift. Every
/// OK path returns an IMMEDIATE (fixnum / t / nil / a pre-materialized buffer
/// value), so the shim needs NO residual-stack rooting (the generated code does
/// not root it — like the round-1 predicate shims); an Err returns STATUS_SIGNAL,
/// which DISCARDS the unrooted residual stack (no UAF even if the body's error
/// path allocates). NO epoch guard (CallBuiltinSym name-dispatches the static
/// table; advice/fset/override immune): re-read the live entry and bounce to
/// STATUS_NEED_GENERIC when it is no longer a plain builtin, when the arity is
/// unexpected, or under the FORCE_CBSYM_GENERIC harness — the general shim
/// reproduces void/invalid-function / wrong-number-of-arguments exactly.
///
/// `current-buffer` is the MISSED-HAZARD case: `builtin_current_buffer` calls
/// `make_buffer` (ALLOCATES → would corrupt the unrooted residual stack), so this
/// shim reads the ALREADY-materialized buffer value from the heap's buffer
/// registry (non-allocating) and bounces to STATUS_NEED_GENERIC when it was never
/// materialized (the general path materializes it, with a rooted residual stack).
///
/// ANTI-REQUIREMENT (upheld by the lowering): the JIT must NEVER value-number or
/// cache buffer state (current buffer / point / BEGV / ZV) across a CallBuiltinSym
/// op — `set-buffer`/`insert`/`goto-char`/`widen` change it — so every CBSym op
/// stays an opaque shim call, re-reading state each time.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
///
/// R2 increment A (CBSym-in-AOT): EXPORTED (`#[unsafe(no_mangle)] pub` + listed
/// in `shim_names.rs`/`MIR_SHIM_NAMES` + anchored by `JIT_SHIM_TABLE`). Tier-A
/// classification is name-canonical + obarray-free, so the AOT baseline emit
/// (`obarray=None`) now emits Tier-A sites too — an AOT `.so` may import this shim
/// and binds it against the host at `dlopen`.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cbsym_read(
    ctx: *mut u8,
    which: i64,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx_ref = unsafe { &mut *(ctx as *mut Context) };
    let which = which as u8;
    // Advice/fset/override-immune: NO epoch guard. Bounce (general path) when
    // the static entry is no longer a plain builtin, the arity is unexpected,
    // or the harness forces it. Decided once, for both paths below.
    if force_cbsym_generic()
        || nargs as usize != cbsym_read_expected_nargs(which)
        || !crate::emacs_core::eval::global_subr_is_builtin(SymId(sym as u32))
    {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        return STATUS_NEED_GENERIC;
    }
    // The reads that only load already-published state answer here, outside
    // the panic containment, as `neovm_jit_aref`'s fast path does: they walk
    // no Lisp structure, allocate nothing and cannot signal. GNU answers each
    // of these with one bytecode opcode (`Bpoint`, `Bpoint_min`, `Bbolp`,
    // `Bmatch_beginning'), which is why 34,000 of them land here per org
    // font-lock operation. Anything else -- the character reads,
    // `current-buffer', or an argument these arms decline -- goes out of line
    // to the contained path, which keeps this frame small enough for the
    // bodies below to inline into it.
    use crate::emacs_core::navigation;
    let answered = match which {
        CBSYM_A_POINT | CBSYM_A_POINT_MIN | CBSYM_A_POINT_MAX => {
            match ctx_ref.buffers.current_buffer() {
                Some(buf) => {
                    let pos = match which {
                        CBSYM_A_POINT => buf.point_lisp_char_pos(),
                        CBSYM_A_POINT_MIN => buf.point_min_lisp_char_pos(),
                        _ => buf.point_max_lisp_char_pos(),
                    };
                    Some(Value::fixnum(pos.as_i64()))
                }
                None => return STATUS_NEED_GENERIC,
            }
        }
        CBSYM_A_MATCH_BEGINNING | CBSYM_A_MATCH_END => {
            // SAFETY: the generated code stored exactly one argument word at
            // `args_ptr` immediately before this call.
            let group = Value::from_bits(unsafe { *args_ptr } as usize);
            match group.as_fixnum().filter(|group| *group >= 0) {
                Some(index) => Some(
                    match ctx_ref
                        .match_data
                        .as_ref()
                        .and_then(|md| md.group(index as usize))
                    {
                        Some(group) if which == CBSYM_A_MATCH_BEGINNING => {
                            Value::fixnum(group.start() as i64)
                        }
                        Some(group) => Value::fixnum(group.end() as i64),
                        None => Value::NIL,
                    },
                ),
                None => return STATUS_NEED_GENERIC,
            }
        }
        CBSYM_A_BOLP => navigation::builtin_bolp_0(ctx_ref).ok(),
        CBSYM_A_EOLP => navigation::builtin_eolp_0(ctx_ref).ok(),
        CBSYM_A_BOBP => navigation::builtin_bobp_0(ctx_ref).ok(),
        CBSYM_A_EOBP => navigation::builtin_eobp_0(ctx_ref).ok(),
        _ => None,
    };
    if let Some(value) = answered {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `out` is the generated code's result stack slot.
        unsafe { *out = value.bits() as i64 };
        return STATUS_OK;
    }
    cbsym_read_contained(ctx, which, args_ptr, out)
}

/// The Tier-A reads the fast path above does not answer: the two character
/// reads, `current-buffer', and any arm that declined. Out of line and panic
/// contained, as [`neovm_jit_aref`]'s slow path is.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; the caller has already
/// checked that the site is armed.
#[cold]
#[inline(never)]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
fn cbsym_read_contained(ctx: *mut u8, which: u8, args_ptr: *const i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        // current-buffer: NEVER allocate. Read the already-materialized buffer value;
        // bounce when it was never made (general path -> make_buffer, rooted).
        if which == CBSYM_A_CURRENT_BUFFER {
            let Some(buf_id) = ctx.buffers.current_buffer().map(|b| b.id) else {
                #[cfg(debug_assertions)]
                CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
                return STATUS_NEED_GENERIC;
            };
            return match crate::tagged::gc::with_tagged_heap(|h| h.buffer_value(buf_id)) {
                Some(v) => {
                    #[cfg(debug_assertions)]
                    CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = v.bits() as i64 };
                    STATUS_OK
                }
                None => {
                    #[cfg(debug_assertions)]
                    CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
                    STATUS_NEED_GENERIC
                }
            };
        }
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        // DELEGATE to the builtin body (GC-free OK path). These are GNU's
        // inline opcodes: like the interpreter's inline tier (`Vm::op_point`,
        // `call_fixed_builtin_direct`) they neither poll for quit nor push a
        // frame.
        use crate::emacs_core::{editfns, navigation};
        let res = match which {
            CBSYM_A_BOLP => navigation::builtin_bolp_0(ctx),
            CBSYM_A_EOLP => navigation::builtin_eolp_0(ctx),
            CBSYM_A_BOBP => navigation::builtin_bobp_0(ctx),
            CBSYM_A_EOBP => navigation::builtin_eobp_0(ctx),
            CBSYM_A_FOLLOWING_CHAR => editfns::builtin_following_char_0(ctx),
            CBSYM_A_PRECEDING_CHAR => editfns::builtin_preceding_char(ctx, Vec::new()),
            CBSYM_A_CHAR_AFTER => crate::emacs_core::buffer::builtin_char_after_1(ctx, Value::NIL),
            _ => return STATUS_NEED_GENERIC,
        };
        match res {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => match ctx.dispatch_signal_result_if_needed(Err(flow)) {
                Ok(value) => {
                    unsafe { *out = value.bits() as i64 };
                    STATUS_OK
                }
                Err(flow) => {
                    stash_pending_flow(flow);
                    STATUS_SIGNAL
                }
            },
        }
    })
}

/// Legacy bare-policy C shim; generated `Op::Throw` uses the Context shim below.
/// Compiled bodies have no local handlers (handler opcodes bail), so a throw
/// always propagates out — exactly the interpreter's `resume_nonlocal` once no
/// local handler matches. Context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_throw(tag: i64, value: i64) {
    jit_shim_contain!(no_ctx, (), {
        stash_pending_flow(Flow::throw(
            Value::from_bits(tag as usize),
            Value::from_bits(value as usize),
        ));
    })
}

/// Generated `Op::Throw`: capture the active policy before any unbinding.
/// SAFETY: `ctx` is the dormant Context supplied by the generated leaf.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: generated leaf vmctx contract.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_throw_with_symbols(ctx: *mut u8, tag: i64, value: i64) {
    jit_shim_contain!(ctx, (), {
        // SAFETY: this read cannot collect or call Lisp while the generated
        // leaf's operand slots are unrooted.
        let symbol_mode = unsafe { &*(ctx as *const Context) }.symbol_mode();
        stash_pending_flow(Flow::throw_with_symbols(
            Value::from_bits(tag as usize),
            Value::from_bits(value as usize),
            symbol_mode,
        ));
    })
}

/// Slow path for `integerp` when the value isn't a fixnum: bignums are
/// veclikes, so delegate to the value layer's own predicate. Context-free,
/// pure, never allocates or signals.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_integerp_slow(v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    (if v.is_integer() {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// Slow path for `numberp` when the value isn't a fixnum (floats, bignums).
/// Context-free, pure, never allocates or signals.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_numberp_slow(v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    (if v.is_number() {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// `Op::SaveCurrentBuffer`: record the current buffer on the specpdl + the
/// bind stack, exactly like the interpreter arm (conditional + infallible).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_current_buffer(ctx: *mut u8) {
    use crate::emacs_core::eval::SpecBinding;
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(buffer_id) = ctx.buffers.current_buffer().map(|buffer| buffer.id) {
        ctx.jit_bind_stack.push(ctx.specpdl.len());
        ctx.specpdl
            .push(SpecBinding::SaveCurrentBuffer { buffer_id });
    }
}

/// `Op::SaveExcursion`: record point/mark/buffer via the same Context helper
/// the interpreter uses (`record_save_excursion` pushes the specpdl record and
/// returns the pre-push depth for the bind stack).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_excursion(ctx: *mut u8) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(count) = ctx.record_save_excursion() {
        ctx.jit_bind_stack.push(count);
    }
}

/// `Op::SaveRestriction`: record the narrowing state, exactly like the
/// interpreter arm (conditional + infallible).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_restriction(ctx: *mut u8) {
    use crate::emacs_core::eval::SpecBinding;
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(saved) = ctx.buffers.save_current_restriction_state() {
        ctx.jit_bind_stack.push(ctx.specpdl.len());
        ctx.specpdl.push(SpecBinding::save_restriction(saved));
    }
}

/// `Op::UnwindProtectPop`: register an unwind-protect cleanup form as a
/// specpdl record (the interpreter arm mirrored 1:1 — same `SpecBinding`
/// entry, same captured lexenv). The cleanup runs whenever `unbind_to` crosses
/// it: the matching `Unbind`, or the frame unwind on any exit — shared
/// machinery with the interpreter, including the signal path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_unwind_protect(ctx: *mut u8, forms: i64) {
    use crate::emacs_core::eval::SpecBinding;
    let forms = Value::from_bits(forms as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    ctx.jit_bind_stack.push(ctx.specpdl.len());
    let lexenv = ctx.lexenv;
    ctx.specpdl
        .push(SpecBinding::UnwindProtect { forms, lexenv });
}

/// `Op::PushConditionCase`: register a `condition-case` handler frame on the
/// ctx-level condition stack, mirroring the interpreter arm exactly — implicit
/// `error` conditions, a `VmConditionCase` resume carrying the bytecode target,
/// the static operand-stack depth at the push, the current specpdl depth, and
/// the current JIT bind-stack length (this frame's analogue of the
/// interpreter's frame-local `bind_stack`). Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_cc(ctx: *mut u8, target: i64, stack_len: i64) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = ctx.jit_bind_stack.len();
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::ConditionCase {
        conditions: Value::symbol("error"),
        resume: ResumeTarget::VmConditionCase {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PushConditionCaseRaw`: like [`neovm_jit_push_cc`] but the handler
/// pattern (conditions) was popped from the operand stack by the generated
/// code and is passed in. Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_cc_raw(
    ctx: *mut u8,
    target: i64,
    stack_len: i64,
    conditions: i64,
) {
    let conditions = Value::from_bits(conditions as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = ctx.jit_bind_stack.len();
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::ConditionCase {
        conditions,
        resume: ResumeTarget::VmConditionCase {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PushCatch`: register a `catch` frame (tag popped by the generated
/// code), mirroring the interpreter arm. Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_catch(ctx: *mut u8, target: i64, stack_len: i64, tag: i64) {
    let tag = Value::from_bits(tag as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = ctx.jit_bind_stack.len();
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag,
        resume: ResumeTarget::VmCatch {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PopHandler`: drop the innermost handler frame (normal exit from a
/// protected extent). The static handler-depth analysis guarantees balance.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_pop_handler(ctx: *mut u8) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    ctx.pop_condition_frame();
}

/// Handler-match dispatch: called on the cold path after a runtime call inside
/// a protected extent returned [`STATUS_SIGNAL`], with `ours` = the number of
/// condition frames this *native frame* has active at the site (static). The
/// per-frame invariant "callees pop their own frames on every exit" means the
/// top `ours` frames of `ctx.condition_stack` are exactly ours.
///
/// Mirrors `Vm::resume_nonlocal` 1:1:
/// - `Throw`: select via `matching_catch_resume` (whole-stack scan, like the
///   interpreter); pop our frames innermost-first looking for the selected
///   resume. Found -> unwind (`unbind_to` to the frame's spec depth, truncate
///   the JIT bind stack), write the thrown value through `out`, and return the
///   0-based miss count `m` (0 = innermost handler matched). Selected-but-outer
///   -> all ours popped, rethrow (-1). No catch anywhere -> `no-catch` signal.
/// - `Signal`: `kill-emacs` propagates untouched (frames left for the frame
///   unwind, like the interpreter's early return). Otherwise run
///   `dispatch_signal_if_needed` (signal hooks + handler-bind — may run lisp,
///   GC, or itself raise: loop on the new flow, the interpreter's recursion),
///   then unwind to `selected_resume` among our frames; on a match the error
///   object (`make_signal_binding_value`) goes through `out`.
///
/// The generated code keeps its live operand-stack values rooted across this
/// call (the lisp run by cleanups/hooks can collect) and maps the returned
/// ordinal back to the statically known handler target.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `out` is the generated
/// code's result stack slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_match_handler(ctx: *mut u8, ours: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, -1, {
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let ours = ours as usize;
        // A contained panic skipped the panicked extent's cleanup; heal its
        // residue against the leaf-entry bases BEFORE taking/matching, so
        // the count-based pops below and the innermost-match scan operate on
        // exactly this leaf's own frames.
        if shim_panic_pending() {
            heal_shim_panic_residue_before_match(ctx, ours);
        }
        let mut flow = take_pending_flow().expect("match shim runs only after STATUS_SIGNAL");
        let mut popped_ordinal_base = 0usize;
        'resume: loop {
            // Frames still ours: `ours` less those popped by earlier passes.
            let remaining = ours - popped_ordinal_base;
            match flow.into_kind() {
                kind @ (FlowKind::ThreadBlocked(_) | FlowKind::Shutdown(_)) => {
                    stash_pending_flow(Flow::from_kind(kind));
                    return -1;
                }
                FlowKind::Throw(thrown) => {
                    let (tag, value) = (thrown.tag, thrown.value);
                    let Some(selected) =
                        ctx.matching_catch_resume_with_symbols(&tag, thrown.symbol_mode())
                    else {
                        // No matching catch anywhere: unwind all our frames and
                        // propagate `no-catch` (resume_nonlocal parity).
                        for _ in 0..remaining {
                            ctx.pop_condition_frame();
                        }
                        stash_pending_flow(signal(LispCondition::NoCatch, vec![tag, value]));
                        return -1;
                    };
                    for m in 0..remaining {
                        let frame = ctx
                            .pop_condition_frame()
                            .expect("JIT handler frames missing from condition stack");
                        let resume = condition_frame_resume(frame);
                        if resume == selected {
                            let ResumeTarget::VmCatch {
                                spec_depth,
                                bind_stack_len,
                                ..
                            } = resume
                            else {
                                unreachable!("JIT catch frame carries a VmCatch resume");
                            };
                            // unbind_to may run unwind-protect cleanups (lisp ->
                            // GC); keep the carried values alive across it.
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(tag);
                            push_scratch_gc_root(value);
                            let unwind = ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                            ctx.jit_bind_stack.truncate(bind_stack_len);
                            restore_scratch_gc_roots(saved);
                            if let Err(next) = unwind {
                                popped_ordinal_base += m + 1;
                                flow = next;
                                continue 'resume;
                            }
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = value.bits() as i64 };
                            return (popped_ordinal_base + m) as i64;
                        }
                    }
                    // The selected catch belongs to an outer frame: ours are all
                    // popped; rethrow for the frame unwind + outer handlers.
                    // The outer catch was selected under the original throwing policy.
                    stash_pending_flow(Flow::from_kind(FlowKind::Throw(thrown)));
                    return -1;
                }
                FlowKind::Signal(sig) => {
                    if sig.symbol == intern("kill-emacs") {
                        // Interpreter parity: propagate immediately, frames left
                        // to the frame-exit truncation.
                        stash_pending_flow(Flow::signal_boxed(sig));
                        return -1;
                    }
                    // Signal hooks / handler-bind handlers may run lisp and GC;
                    // root the signal payload across the dispatch.
                    let saved = save_scratch_gc_roots();
                    push_scratch_gc_root(Value::from_sym_id(sig.symbol));
                    for v in sig.data.iter().copied() {
                        push_scratch_gc_root(v);
                    }
                    if let Some(raw) = sig.raw_data {
                        push_scratch_gc_root(raw);
                    }
                    let dispatched = ctx.dispatch_signal_if_needed(sig);
                    restore_scratch_gc_roots(saved);
                    let sig = match dispatched {
                        Ok(sig) => sig,
                        // A hook/handler raised: restart matching on the new flow
                        // (resume_nonlocal recurses here).
                        Err(next) => {
                            flow = next;
                            continue;
                        }
                    };
                    let Some(selected) = sig.selected_resume.clone() else {
                        for _ in 0..remaining {
                            ctx.pop_condition_frame();
                        }
                        stash_pending_flow(Flow::signal_boxed(sig));
                        return -1;
                    };
                    for m in 0..remaining {
                        let frame = ctx
                            .pop_condition_frame()
                            .expect("JIT handler frames missing from condition stack");
                        let resume = condition_frame_resume(frame);
                        if resume == selected {
                            let ResumeTarget::VmConditionCase {
                                spec_depth,
                                bind_stack_len,
                                ..
                            } = resume
                            else {
                                unreachable!(
                                    "JIT condition-case frame carries a VmConditionCase resume"
                                );
                            };
                            // unbind_to runs cleanups and the error object below
                            // allocates: root the signal payload throughout.
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(Value::from_sym_id(sig.symbol));
                            for v in sig.data.iter().copied() {
                                push_scratch_gc_root(v);
                            }
                            if let Some(raw) = sig.raw_data {
                                push_scratch_gc_root(raw);
                            }
                            let unwind = ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                            ctx.jit_bind_stack.truncate(bind_stack_len);
                            if let Err(next) = unwind {
                                restore_scratch_gc_roots(saved);
                                popped_ordinal_base += m + 1;
                                flow = next;
                                continue 'resume;
                            }
                            let binding = make_signal_binding_value(&sig);
                            restore_scratch_gc_roots(saved);
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = binding.bits() as i64 };
                            return (popped_ordinal_base + m) as i64;
                        }
                    }
                    stash_pending_flow(Flow::signal_boxed(sig));
                    return -1;
                }
            }
        }
    })
}

/// `Op::Switch` lookup result: the dispatch value is not in the jump table —
/// fall through (interpreter parity).
pub(crate) const JIT_SWITCH_MISS: i64 = -1;
/// `Op::Switch` lookup result: the table no longer matches what was compiled
/// (a value mutated to a non-fixnum); the shim stashed a signal.
pub(crate) const JIT_SWITCH_STALE: i64 = -2;

/// `Op::Switch`: look the dispatch value up in the (statically verified
/// compile-time constant) hash-table jump table, with the interpreter's exact
/// key semantics: `LispHashTable::switch_target`, the table's switch plan,
/// which answers exactly as the hashed lookup under the table's own test.
/// Returns the raw fixnum target address on a hit
/// ([`JIT_SWITCH_MISS`]/[`JIT_SWITCH_STALE`] otherwise); the generated code
/// maps raw addresses onto the statically resolved target blocks. No Lisp
/// allocation and no Lisp calls (a Rust-side plan is built on the table's
/// second dispatch).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_switch(ctx: *mut u8, dispatch: i64, table: i64) -> i64 {
    jit_shim_contain!(ctx, JIT_SWITCH_STALE, {
        let table = Value::from_bits(table as usize);
        let dispatch = Value::from_bits(dispatch as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let Some(ht) = table.as_hash_table() else {
            // Statically verified a hash table; only runtime mutation of the
            // constant pool itself could change that.
            stash_pending_flow(signal(
                "error",
                vec![Value::string("jit: switch jump table mutated at runtime")],
            ));
            return JIT_SWITCH_STALE;
        };
        // Answer from the table's switch plan (a dense index, a bit scan or
        // key programs walked with the value) rather than hashing the value on
        // every dispatch. `neovm_jit_switch` is 215K calls per org edit
        // iteration.
        match ht.switch_target(dispatch, ctx.symbols_with_pos_enabled) {
            Some(v) => match v.kind() {
                ValueKind::Fixnum(addr) if addr >= 0 => addr,
                _ => {
                    stash_pending_flow(signal(
                        "error",
                        vec![Value::string("jit: switch jump table mutated at runtime")],
                    ));
                    JIT_SWITCH_STALE
                }
            },
            None => JIT_SWITCH_MISS,
        }
    })
}

/// Cold path for a switch hit whose raw address is not in the statically
/// compiled target set (the jump table was mutated after compilation — code
/// the byte-compiler never produces). Stash a loud signal; the generated code
/// routes to its signal path. Context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_switch_stale() {
    jit_shim_contain!(no_ctx, (), {
        stash_pending_flow(signal(
            "error",
            vec![Value::string("jit: switch jump table mutated at runtime")],
        ));
    })
}

/// Back-edge service poll: GC safepoint + `maybe_quit`, via the same shared
/// Context helper the interpreter's `branch_to!` wrap path uses
/// (`bytecode_branch_maybe_gc_and_quit`). Generated code calls this every 255
/// backward jumps (the interpreter's u8 `quitcounter` cadence), with its live
/// operand-stack values rooted by the caller — the poll may collect.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_backedge(ctx: *mut u8) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        match ctx.bytecode_branch_maybe_gc_and_quit() {
            Ok(()) => STATUS_OK,
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}

#[cfg(test)]
#[path = "../tests/list_search_swp_test.rs"]
mod list_search_swp_tests;
