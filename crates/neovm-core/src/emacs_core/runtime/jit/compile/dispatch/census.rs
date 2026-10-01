//! The exact spec-shim census entry, selected by generated code only when
//! `NEOVM_JIT_CALL_CENSUS` is enabled. The ordinary shim and this entry
//! expand the same protocol directly; no wrapper invocation or new Rust
//! inlining boundary is inserted into the ordinary call path.
//!
//! Threading: each invocation reads its caller's mutator-owned Context and
//! existing atomic spec slot. The census owns no Lisp state; its shared
//! counts use relaxed atomic increments in `call_census`.

use super::*;

/// [`neovm_jit_call_spec`] with exact accepted-fast-entry counts. Raw
/// memory/register entries increment the denominator; the contained
/// framed entry also increments the numerator, including AOT-sidecar-only
/// leaves. Revalidation, attention, debugger and depth misses do not count.
///
/// SAFETY: the identical seven-argument ABI and live slot/context contract
/// of [`neovm_jit_call_spec`]. The shared macro preserves all existing
/// containment, frame, marshal, deopt and exit behavior.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw pointers follow the documented caller contract.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_call_spec_census(
    ctx: *mut u8,
    sym_bits: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    spec_call_body!(true, ctx, sym_bits, expected, slot, args_ptr, nargs, out)
}
