//! Selected GC-free actual binding guard for a front-captured sqrt witness.
//! A GC-free selected native guard, never a dispatch or cached entry point.

use super::*;
use crate::emacs_core::symbol::FunctionCellSnapshot;
use crate::tagged::header::{SubrObj, VecLikeHeader, VecLikeType};

/// Check a live actual Subr entry on its owning FRONT or running MUTATOR.
/// Workers never call this helper: the compiler witness carries opaque scalar
/// bits only. The existing runtime owns/leaks static Subr objects. Selected
/// readers and the actual in-place registration writer share the process-wide
/// synchronization-only STATIC_SUBR_ENTRY_SYNC lock; plain metadata
/// fields are copied only under its read side. Busy/poisoned locks decline.
/// A current Context is exclusively used by its mutator while the helper runs;
/// no Lisp call, allocation, GC, collection-observation hook or new TLS/cache
/// access occurs here. Mutators inspect their own cells, never a shared stamp.
#[inline]
pub(super) fn is_actual_sqrt(binding: Value) -> bool {
    crate::tagged::value::with_static_subr_entry_read(|| is_actual_sqrt_locked(binding))
        .unwrap_or(false)
}

/// Called only while the paired registration/read lock protects metadata.
#[inline]
fn is_actual_sqrt_locked(binding: Value) -> bool {
    if !binding.is_veclike() {
        return false;
    }
    // Do not use as_veclike_ptr here: its optional collection-observation hook
    // can allocate/borrow TLS. This is the same tag-first actual-object view as
    // existing fixed Subr dispatch, on a live current binding (or the static
    // Subr witness just validated on the owning front).
    let ptr = (binding.bits() & !TAG_MASK) as *const VecLikeHeader;
    // SAFETY: binding is a live current function cell; the header check precedes
    // the SubrObj view. Native witness bits are only dereferenced after exact
    // equality with this live current cell, not on a Cranelift worker. The
    // active read lock excludes every published in-place metadata writer.
    let subr = unsafe {
        if (*ptr).type_tag != VecLikeType::Subr {
            return false;
        }
        &*(ptr as *const SubrObj)
    };
    if subr.dispatch_kind != SubrDispatchKind::Builtin
        || subr.min_args != 1
        || subr.max_args != Some(1)
    {
        return false;
    }
    let Some(SubrFn::A1(function)) = subr.function else {
        return false;
    };
    std::ptr::fn_addr_eq(
        function,
        crate::emacs_core::builtins::builtin_sqrt_1 as crate::tagged::header::SubrFn1,
    )
}

/// Validate the original immutable intrinsic witness against CURRENT owner
/// state, including an entry rewritten in place at unchanged object bits.
///
/// Acquire both publication clocks independently, just as an existing spec
/// refresh acquires its starting clock before reading the binding. No epoch
/// hit short-circuits the current function cell or actual entry proof: equal
/// integers from two Contexts are not ownership proof, and another same-thread
/// Context can rewrite the shared static object without bumping THIS clock.
/// This guard does not rearm or publish a slot. Its 0/1 result is a machine
/// predicate, never a Lisp value/STATUS/root; every miss replays original Call.
#[inline]
pub(super) fn binding_valid(
    ctx: &Context,
    symbol_bits: u64,
    expected_subr_bits: u64,
    expected_epoch: u64,
) -> bool {
    let before = ctx.obarray.function_epoch(); // Acquire.
    if before != expected_epoch {
        return false;
    }
    let Some(sym) = Value::from_bits(symbol_bits as usize).as_symbol_id() else {
        return false;
    };
    let FunctionCellSnapshot::Bound(binding) = ctx.obarray.function_cell_snapshot(sym) else {
        return false;
    };
    if binding.bits() as u64 != expected_subr_bits || !is_actual_sqrt(binding) {
        return false;
    }
    ctx.obarray.function_epoch() == before // Independent Acquire.
}

/// Selected internal JIT ABI only: (vmctx, symbol bits, expected Subr bits,
/// capture epoch) -> normalized i64 0/1. Never calls Lisp/GC or constructs an
/// error, and uses no RefCell/collection observer/profile hooks which could
/// panic or allocate. No saved Context/Subr pointer, global cache or new TLS.
///
/// SAFETY: generated code passes its currently running, dormant Context as
/// vmctx, under the same exclusive mutator contract as the existing read-only
/// dispatch shims. Null conservatively misses; arbitrary host pointers are
/// outside that existing ABI contract. Function cells and static object entry
/// metadata follow the paired registration/read lock; current function-cell
/// ownership and Release epoch publication follow existing dispatch contracts.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_sqrt_binding_valid(
    ctx: *mut u8,
    symbol_bits: i64,
    expected_subr_bits: i64,
    expected_epoch: i64,
) -> i64 {
    if ctx.is_null() {
        return 0;
    }
    // SAFETY: see the generated-call contract above; a shared read only.
    let ctx = unsafe { &*(ctx as *const Context) };
    i64::from(binding_valid(
        ctx,
        symbol_bits as u64,
        expected_subr_bits as u64,
        expected_epoch as u64,
    ))
}
