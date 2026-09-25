//! The register entry ABI of JIT leaf bodies (design
//! `p1-1-direct-native-calls` §3.2, P1.0 S2.1a).
//!
//! A leaf body's entry has one of two shapes ([`LeafAbi`]):
//!
//! * **memory** (every AOT and OSR leaf, and every leaf while the knob is
//!   off): `fn(vmctx, args: *const i64, out: *mut i64, sidecar) -> status`.
//!   The arguments are read from the caller's words and the result written
//!   through `out`.
//! * **register** (`NEOVM_JIT_REG_ABI=on`, implied by
//!   `NEOVM_JIT_DIRECT_CALL=on`; a JIT, non-OSR leaf of at most
//!   [`MAX_REG_ARGS`] argument words): `fn(vmctx, aux, a0, .., a{k-1}) ->
//!   (value, status)`. `aux` is the executing callee's constant base (read
//!   only by a `make-closure`-patched leaf, as the memory ABI's fourth word
//!   is); the arguments arrive in registers (the fifth and sixth on the
//!   stack) and the answer comes back in `rax:rdx`, the SysV return of a
//!   `#[repr(C)]` two-word struct ([`NativeRet`]). A compiled caller can
//!   then call the body with no memory round trip at all (a direct call).
//!
//! Rust callers need no adapter: [`call_register_entry`] transmutes the
//! entry to the function type of its arity. The body's own code differs from
//! the memory shape only at its edges: the entry block takes the arguments
//! as parameters, and every exit returns two words instead of storing one
//! and returning the other ([`emit_leaf_return`]).

use super::*;
use cranelift_codegen::ir::Signature;

/// The most argument words a register-ABI body takes: with `vmctx` and
/// `aux`, eight parameters, the first six in registers.
pub(crate) const MAX_REG_ARGS: usize = 6;

/// A leaf body's entry shape (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafAbi {
    /// `fn(vmctx, args, out, sidecar) -> status`.
    Memory,
    /// `fn(vmctx, aux, a0, .., a{arity-1}) -> (value, status)`.
    Register { arity: u8 },
}

impl LeafAbi {
    /// The entry shape a JIT build gives a body of `arity` argument words:
    /// register when the knob is on, the build is JIT (`!aot`), not an OSR
    /// entry (whose "arguments" are an operand-stack snapshot of any
    /// depth), and the words fit.
    pub(crate) fn for_build(aot: bool, osr: bool, arity: usize) -> Self {
        if !aot && !osr && arity <= MAX_REG_ARGS && jit_register_abi_on() {
            LeafAbi::Register { arity: arity as u8 }
        } else {
            LeafAbi::Memory
        }
    }

    /// The Cranelift signature of the entry.
    pub(crate) fn signature(
        self,
        call_conv: cranelift_codegen::isa::CallConv,
        ptr_ty: types::Type,
    ) -> Signature {
        let mut sig = Signature::new(call_conv);
        match self {
            LeafAbi::Memory => {
                sig.params.push(AbiParam::new(ptr_ty)); // vmctx
                sig.params.push(AbiParam::new(ptr_ty)); // args
                sig.params.push(AbiParam::new(ptr_ty)); // out
                sig.params.push(AbiParam::new(ptr_ty)); // sidecar (*const LeafSidecar)
                sig.returns.push(AbiParam::new(types::I64));
            }
            LeafAbi::Register { arity } => {
                sig.params.push(AbiParam::new(ptr_ty)); // vmctx
                sig.params.push(AbiParam::new(ptr_ty)); // aux
                for _ in 0..arity {
                    sig.params.push(AbiParam::new(types::I64));
                }
                sig.returns.push(AbiParam::new(types::I64)); // value
                sig.returns.push(AbiParam::new(types::I64)); // status
            }
        }
        sig
    }
}

/// Emit a body exit: `status` (a `STATUS_*` constant) with `value` as the
/// result (`STATUS_OK`) or nothing (every other status). The memory ABI
/// stores `value` through `out` and returns the status; the register ABI
/// returns both words (a zero value for a non-OK exit).
pub(crate) fn emit_leaf_return(
    fb: &mut FunctionBuilder,
    abi: LeafAbi,
    out: Option<ClifValue>,
    value: Option<ClifValue>,
    status: i64,
) {
    match abi {
        LeafAbi::Memory => {
            if let Some(value) = value {
                let out = out.expect("a memory-ABI body has its out pointer");
                fb.ins().store(MemFlagsData::trusted(), value, out, 0);
            }
            let code = fb.ins().iconst(types::I64, status);
            fb.ins().return_(&[code]);
        }
        LeafAbi::Register { .. } => {
            let value = value.unwrap_or_else(|| fb.ins().iconst(types::I64, 0));
            let code = fb.ins().iconst(types::I64, status);
            fb.ins().return_(&[value, code]);
        }
    }
}

/// What a register-ABI entry returns: `rax:rdx` under SysV.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeRet {
    pub(crate) value: i64,
    pub(crate) status: i64,
}

/// Call a register-ABI entry with `arity` argument words read from `args`.
///
/// # Safety
///
/// `entry` is finalized native code with the register ABI for exactly
/// `arity` arguments ([`LeafAbi::Register`]), `args` addresses `arity` live
/// tagged words, and `vmctx`/`aux` meet the body's contract (see
/// `CompiledLeaf::invoke_native`).
#[inline(always)]
pub(crate) unsafe fn call_register_entry(
    entry: *const u8,
    arity: u8,
    vmctx: *mut u8,
    aux: *const u8,
    args: *const i64,
) -> NativeRet {
    type A = i64;
    // SAFETY: the caller's contract; each arm transmutes to the entry's
    // exact signature and reads exactly `arity` words.
    unsafe {
        let a = |i: usize| *args.add(i);
        match arity {
            0 => {
                let f: extern "C" fn(*mut u8, *const u8) -> NativeRet = core::mem::transmute(entry);
                f(vmctx, aux)
            }
            1 => {
                let f: extern "C" fn(*mut u8, *const u8, A) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, aux, a(0))
            }
            2 => {
                let f: extern "C" fn(*mut u8, *const u8, A, A) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, aux, a(0), a(1))
            }
            3 => {
                let f: extern "C" fn(*mut u8, *const u8, A, A, A) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, aux, a(0), a(1), a(2))
            }
            4 => {
                let f: extern "C" fn(*mut u8, *const u8, A, A, A, A) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, aux, a(0), a(1), a(2), a(3))
            }
            5 => {
                let f: extern "C" fn(*mut u8, *const u8, A, A, A, A, A) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, aux, a(0), a(1), a(2), a(3), a(4))
            }
            6 => {
                let f: extern "C" fn(*mut u8, *const u8, A, A, A, A, A, A) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, aux, a(0), a(1), a(2), a(3), a(4), a(5))
            }
            _ => unreachable!("a register-ABI body takes at most MAX_REG_ARGS words"),
        }
    }
}

const _: () = assert!(
    MAX_REG_ARGS == 6,
    "call_register_entry has one arm per arity"
);

#[cfg(test)]
#[path = "reg_abi/tests/reg_abi_test.rs"]
mod tests;
