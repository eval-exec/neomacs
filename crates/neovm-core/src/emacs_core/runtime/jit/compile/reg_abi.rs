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
//!   `NEOVM_JIT_DIRECT_CALL=on`; a JIT, non-OSR, frameless, unpatched leaf
//!   of at most [`MAX_REG_ARGS`] required parameters, the bodies a direct
//!   call can enter: [`LeafAbi::for_build`]): `fn(vmctx, aux, a0, .., a{k-1}) ->
//!   (value, status)`. `aux` is the executing callee's constant base (read
//!   only by a `make-closure`-patched leaf, as the memory ABI's fourth word
//!   is); the arguments arrive in registers (the fifth and sixth on the
//!   stack) and the answer comes back in `rax:rdx`, the SysV return of a
//!   `#[repr(C)]` two-word struct ([`NativeRet`]). A compiled caller can
//!   then call the body with no memory round trip at all (a direct call).
//!
//! Rust callers need no adapter: the leaf's [`RegisterThunk`], picked for
//! its arity when it is built, loads the words and tail-calls the entry. The body's own code differs from
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
    /// The entry shape a build gives a body of `arity` argument words. The
    /// register ABI's one use is to be entered by a direct call
    /// (`direct_call`), so a body gets it only when the knob is on and a
    /// direct call could enter it:
    ///
    /// * a JIT build (`!aot`), not an OSR entry (whose "arguments" are an
    ///   operand-stack snapshot of any depth), and the words fit;
    /// * `frameless`: no dynamic bindings and no handler frames, the bodies
    ///   a direct call (and the spec shim's raw path) enters without a frame
    ///   of its own. So a framed entry is always a memory entry, and the
    ///   framed callers make no ABI test (`CompiledLeaf::call_premarshaled_consts`);
    /// * `dynamic_prefix == 0`: not a `make-closure`-patched source, whose
    ///   bodies are closures that `mapc`/`funcall` (Rust callers) run far
    ///   more often than a symbol's function cell does -- unless
    ///   `NEOVM_JIT_SPEC_SOURCES` is on, whose closure source sites
    ///   (`source_slots`) enter exactly those bodies directly;
    /// * a lambda list of required parameters only ([`exact_lambda_list`]):
    ///   a direct site calls with exactly that many arguments.
    ///
    /// Every other body keeps the memory ABI, and with it every Rust caller's
    /// entry as before the register ABI existed.
    pub(crate) fn for_build(
        aot: bool,
        osr: bool,
        arity: usize,
        frameless: bool,
        dynamic_prefix: usize,
    ) -> Self {
        if !aot
            && !osr
            && arity <= MAX_REG_ARGS
            && frameless
            && (dynamic_prefix == 0 || super::knobs::jit_spec_sources_on())
            && exact_lambda_list()
            && jit_register_abi_on()
        {
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

std::thread_local! {
    /// Whether the function being compiled on this thread takes required
    /// parameters only (see [`LambdaListScope`]); true outside any scope.
    static EXACT_LAMBDA_LIST: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

/// Whether the function being compiled takes required parameters only (no
/// `&optional`, no `&rest`): `compile_bytecode_function` says so for its
/// lowering ([`LambdaListScope`]); a lowering outside one (the tests'
/// direct builds) counts as exact.
pub(crate) fn exact_lambda_list() -> bool {
    EXACT_LAMBDA_LIST.with(core::cell::Cell::get)
}

/// For its lifetime, the lambda-list fact [`LeafAbi::for_build`] reads
/// ([`exact_lambda_list`]); the previous one is restored on drop.
pub(crate) struct LambdaListScope(bool);

impl LambdaListScope {
    pub(crate) fn enter(exact: bool) -> Self {
        Self(EXACT_LAMBDA_LIST.with(|c| c.replace(exact)))
    }
}

impl Drop for LambdaListScope {
    fn drop(&mut self) {
        EXACT_LAMBDA_LIST.with(|c| c.set(self.0));
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

/// Call a register-ABI entry with `arity` argument words read from `args`
/// (the tests' reference for the [`RegisterThunk`]s).
///
/// # Safety
///
/// `entry` is finalized native code with the register ABI for exactly
/// `arity` arguments ([`LeafAbi::Register`]), `args` addresses `arity` live
/// tagged words, and `vmctx`/`aux` meet the body's contract (see
/// `CompiledLeaf::invoke_native`).
#[cfg(test)]
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

/// A Rust caller's way into a register-ABI entry of one arity, chosen when
/// the leaf is built (`CompiledLeaf::register_thunk`) so no caller matches
/// on the arity per call: it loads the arity's words from `args` and calls
/// `entry` with `aux` stripped of the spec slot's key flags (the spec shim
/// passes its key as is; every other caller's base has none). With four
/// words or fewer the call is a tail call, so the body returns straight to
/// the thunk's caller.
pub(crate) type RegisterThunk = unsafe extern "C" fn(
    entry: *const u8,
    vmctx: *mut u8,
    aux: *const u8,
    args: *const i64,
) -> NativeRet;

/// `aux` without the key flags (`SpecSlot::KEY_FLAGS`).
#[inline(always)]
fn base_of(aux: *const u8) -> *const u8 {
    (aux as usize & !(SpecSlot::KEY_FLAGS as usize)) as *const u8
}

macro_rules! register_thunk {
    ($name:ident, $($i:literal),*) => {
        /// The [`RegisterThunk`] of its arity.
        ///
        /// SAFETY: `entry` has the register ABI for exactly this many
        /// words, which `args` addresses; `vmctx` and `aux` meet the body's
        /// contract.
        unsafe extern "C" fn $name(
            entry: *const u8,
            vmctx: *mut u8,
            aux: *const u8,
            args: *const i64,
        ) -> NativeRet {
            let _ = args;
            // SAFETY: the contract above.
            unsafe {
                let f: extern "C" fn(*mut u8, *const u8 $(, register_thunk!(@word $i))*) -> NativeRet =
                    core::mem::transmute(entry);
                f(vmctx, base_of(aux) $(, *args.add($i))*)
            }
        }
    };
    (@word $i:literal) => { i64 };
}

register_thunk!(register_thunk_0,);
register_thunk!(register_thunk_1, 0);
register_thunk!(register_thunk_2, 0, 1);
register_thunk!(register_thunk_3, 0, 1, 2);
register_thunk!(register_thunk_4, 0, 1, 2, 3);
register_thunk!(register_thunk_5, 0, 1, 2, 3, 4);
register_thunk!(register_thunk_6, 0, 1, 2, 3, 4, 5);

/// The thunk of a memory-ABI leaf, which has no register entry: never
/// called (every caller tests `EntryShape` first).
unsafe extern "C" fn no_register_entry(
    _entry: *const u8,
    _vmctx: *mut u8,
    _aux: *const u8,
    _args: *const i64,
) -> NativeRet {
    unreachable!("a memory-ABI leaf has no register entry")
}

/// The [`RegisterThunk`] a leaf of `abi` stores.
pub(crate) fn register_thunk_for(abi: LeafAbi) -> RegisterThunk {
    match abi {
        LeafAbi::Memory => no_register_entry,
        LeafAbi::Register { arity: 0 } => register_thunk_0,
        LeafAbi::Register { arity: 1 } => register_thunk_1,
        LeafAbi::Register { arity: 2 } => register_thunk_2,
        LeafAbi::Register { arity: 3 } => register_thunk_3,
        LeafAbi::Register { arity: 4 } => register_thunk_4,
        LeafAbi::Register { arity: 5 } => register_thunk_5,
        LeafAbi::Register { arity: 6 } => register_thunk_6,
        LeafAbi::Register { .. } => unreachable!("at most MAX_REG_ARGS register words"),
    }
}

#[cfg(test)]
#[path = "reg_abi/tests/reg_abi_test.rs"]
mod tests;
