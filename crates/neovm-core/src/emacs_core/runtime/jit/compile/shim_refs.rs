//! The runtime shims a leaf can call, declared once per module and imported
//! into a function on first use (P2.4 B3 = P0.6 L4-C2).
//!
//! Before this table every leaf that re-entered the runtime declared all 41
//! base shims into its module and imported every one into its function, so
//! a body calling `cons` alone still carried 41 signatures, 41 external
//! functions and 41 `SigSet` ABI computations through Cranelift. Now:
//!
//! - [`Shim`] names every shim with its symbol and signature, in the order
//!   the old `declare_rt_refs` declared them;
//! - [`ShimIds`] holds a module's `FuncId`s, declared once per module (a
//!   per-leaf module declares the same set the old code did; the persistent
//!   per-thread module declares them once for its whole life);
//! - [`RtRefs`] imports a shim into the function being built the first time
//!   the lowering asks for it ([`RtRefs::get`]).
//!
//! `NEOVM_JIT_LAZY_SHIMS=off` imports every declared shim up front in the old
//! order, which reproduces the old CLIF exactly (the single-build A/B arm).
//! Either way the machine code is the same: an import that is never called
//! emits nothing.
//!
//! The shim NAMES and SIGNATURES are unchanged (`ABI_TAG_VERSION` stays): an
//! AOT object imports exactly the shims its code calls, all of which were
//! already in its import set.

use std::cell::Cell;

use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, FuncRef, Function, Signature, Type, UserExternalName,
    types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{FuncId, Linkage, Module};
use strum::{EnumCount, IntoEnumIterator};

use super::CompileError;
use crate::emacs_core::jit::backend::BackendError;

/// Which declaration group a shim belongs to: the base set every
/// runtime-entering leaf declares, the round-1 subr-speculation shims (JIT
/// only: never declared into an AOT object), and the CallBuiltinSym
/// intrinsics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShimGroup {
    Base,
    SubrSpec,
    CbsymSpec,
}

/// Every runtime shim generated code calls, in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter)]
pub(crate) enum Shim {
    RootwinGrow,
    Cons,
    /// Boxes an `f64` computed in a register (`neovm_jit_make_float`).
    MakeFloat,
    /// The generic fallback of an arithmetic site whose feedback says the
    /// fixnum path misses (`neovm_jit_arith_generic`).
    ArithGeneric,
    Call,
    Apply,
    EqSlow,
    SymbolpSlow,
    Varref,
    Varset,
    Varbind,
    Unbind,
    Backedge,
    SaveCurrentBuffer,
    SaveExcursion,
    SaveRestriction,
    UnwindProtect,
    ThrowFlow,
    IntegerpSlow,
    NumberpSlow,
    Builtin1,
    Builtin2,
    Builtin3,
    /// `Op::Aref`: the element's bits or `VALUE_SHIM_SIGNAL`.
    Aref,
    /// `Op::Aset`: the value's bits or a `VALUE_SHIM_*` word.
    Aset,
    /// `Op::Memq`: the tail's bits or `VALUE_SHIM_SIGNAL`.
    Memq,
    /// `Op::Assq`: the entry's bits or `VALUE_SHIM_SIGNAL`.
    Assq,
    /// `Op::Setcar`: the new car's bits or `VALUE_SHIM_SIGNAL`.
    Setcar,
    /// `Op::Setcdr`: the new cdr's bits or `VALUE_SHIM_SIGNAL`.
    Setcdr,
    PushCc,
    PushCcRaw,
    PushCatch,
    PopHandler,
    MatchHandler,
    SwitchLookup,
    SwitchStale,
    List,
    BuiltinSlice,
    NamedBuiltin,
    SaveWindowExcursion,
    CallSpec,
    /// The subr-speculation shims (Gap 1): declared only when the body has
    /// subr-kind spec sites, and so never for an AOT object (its baseline
    /// emit classifies only CallBuiltinSym sites; these names are
    /// deliberately absent from `shim_names.rs`).
    CallSubrSpec,
    PredSpec,
    EqInclPropsSpec,
    /// `neovm_jit_arith_spec` (the logand/logior/logxor intrinsic).
    ArithSpec,
    /// The CallBuiltinSym intrinsics: Tier-B dispatch-skip and Tier-A
    /// GC-free read. Declared when the body has a CallBuiltinSym-kind site
    /// (AOT baseline leaves included: both are exported).
    CbsymSpec,
    CbsymRead,
}

/// The parameter shapes of the shim signatures.
#[derive(Clone, Copy)]
enum P {
    Ptr,
    I64,
    F64,
}

impl Shim {
    /// The shim's exported symbol.
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Shim::RootwinGrow => "neovm_jit_rootwin_grow",
            Shim::Cons => "neovm_jit_cons",
            Shim::MakeFloat => "neovm_jit_make_float",
            Shim::ArithGeneric => "neovm_jit_arith_generic",
            Shim::Call => "neovm_jit_call",
            Shim::Apply => "neovm_jit_apply",
            Shim::EqSlow => "neovm_jit_eq_slow",
            Shim::SymbolpSlow => "neovm_jit_symbolp_slow",
            Shim::Varref => "neovm_jit_varref",
            Shim::Varset => "neovm_jit_varset",
            Shim::Varbind => "neovm_jit_varbind",
            Shim::Unbind => "neovm_jit_unbind",
            Shim::Backedge => "neovm_jit_backedge",
            Shim::SaveCurrentBuffer => "neovm_jit_save_current_buffer",
            Shim::SaveExcursion => "neovm_jit_save_excursion",
            Shim::SaveRestriction => "neovm_jit_save_restriction",
            Shim::UnwindProtect => "neovm_jit_unwind_protect",
            Shim::ThrowFlow => "neovm_jit_throw",
            Shim::IntegerpSlow => "neovm_jit_integerp_slow",
            Shim::NumberpSlow => "neovm_jit_numberp_slow",
            Shim::Builtin1 => "neovm_jit_builtin1",
            Shim::Builtin2 => "neovm_jit_builtin2",
            Shim::Builtin3 => "neovm_jit_builtin3",
            Shim::Aref => "neovm_jit_aref",
            Shim::Aset => "neovm_jit_aset",
            Shim::Memq => "neovm_jit_memq",
            Shim::Assq => "neovm_jit_assq",
            Shim::Setcar => "neovm_jit_setcar",
            Shim::Setcdr => "neovm_jit_setcdr",
            Shim::PushCc => "neovm_jit_push_cc",
            Shim::PushCcRaw => "neovm_jit_push_cc_raw",
            Shim::PushCatch => "neovm_jit_push_catch",
            Shim::PopHandler => "neovm_jit_pop_handler",
            Shim::MatchHandler => "neovm_jit_match_handler",
            Shim::SwitchLookup => "neovm_jit_switch",
            Shim::SwitchStale => "neovm_jit_switch_stale",
            Shim::List => "neovm_jit_list",
            Shim::BuiltinSlice => "neovm_jit_builtin_slice",
            Shim::NamedBuiltin => "neovm_jit_named_builtin",
            Shim::SaveWindowExcursion => "neovm_jit_save_window_excursion",
            Shim::CallSpec => "neovm_jit_call_spec",
            Shim::CallSubrSpec => "neovm_jit_call_subr_spec",
            Shim::PredSpec => "neovm_jit_pred_spec",
            Shim::EqInclPropsSpec => "neovm_jit_eq_incl_props_spec",
            Shim::ArithSpec => "neovm_jit_arith_spec",
            Shim::CbsymSpec => "neovm_jit_cbsym_spec",
            Shim::CbsymRead => "neovm_jit_cbsym_read",
        }
    }

    /// The declaration group (see [`ShimGroup`]).
    pub(crate) fn group(self) -> ShimGroup {
        match self {
            Shim::CallSubrSpec | Shim::PredSpec | Shim::EqInclPropsSpec | Shim::ArithSpec => {
                ShimGroup::SubrSpec
            }
            Shim::CbsymSpec | Shim::CbsymRead => ShimGroup::CbsymSpec,
            Shim::RootwinGrow
            | Shim::Cons
            | Shim::MakeFloat
            | Shim::ArithGeneric
            | Shim::Call
            | Shim::Apply
            | Shim::EqSlow
            | Shim::SymbolpSlow
            | Shim::Varref
            | Shim::Varset
            | Shim::Varbind
            | Shim::Unbind
            | Shim::Backedge
            | Shim::SaveCurrentBuffer
            | Shim::SaveExcursion
            | Shim::SaveRestriction
            | Shim::UnwindProtect
            | Shim::ThrowFlow
            | Shim::IntegerpSlow
            | Shim::NumberpSlow
            | Shim::Builtin1
            | Shim::Builtin2
            | Shim::Builtin3
            | Shim::Aref
            | Shim::Aset
            | Shim::Memq
            | Shim::Assq
            | Shim::Setcar
            | Shim::Setcdr
            | Shim::PushCc
            | Shim::PushCcRaw
            | Shim::PushCatch
            | Shim::PopHandler
            | Shim::MatchHandler
            | Shim::SwitchLookup
            | Shim::SwitchStale
            | Shim::List
            | Shim::BuiltinSlice
            | Shim::NamedBuiltin
            | Shim::SaveWindowExcursion
            | Shim::CallSpec => ShimGroup::Base,
        }
    }

    /// `(params, returns an i64 status/value word)`.
    fn shape(self) -> (&'static [P], bool) {
        use P::{F64, I64, Ptr};
        match self {
            // (vmctx, need) -> ()
            Shim::RootwinGrow => (&[Ptr, I64], false),
            // (car, cdr) -> cons bits
            Shim::Cons => (&[I64, I64], true),
            // (f64) -> float bits
            Shim::MakeFloat => (&[F64], true),
            // (vmctx, kind, a, b, out_ptr) -> status
            Shim::ArithGeneric => (&[Ptr, I64, I64, I64, Ptr], true),
            // (vmctx, func_bits, args_ptr, nargs, out_ptr) -> status
            Shim::Call | Shim::Apply | Shim::CbsymSpec => (&[Ptr, I64, Ptr, I64, Ptr], true),
            // (vmctx, a, b) -> t/nil bits; (vmctx, dispatch, table) -> target
            Shim::EqSlow | Shim::SwitchLookup => (&[Ptr, I64, I64], true),
            // (vmctx, v) -> t/nil bits
            Shim::SymbolpSlow => (&[Ptr, I64], true),
            // (vmctx, sym_id, out_ptr) -> status; (vmctx, ours, out_ptr) ->
            // ordinal; (vmctx, body, out_ptr) -> status
            Shim::Varref | Shim::MatchHandler | Shim::SaveWindowExcursion => {
                (&[Ptr, I64, Ptr], true)
            }
            // (vmctx, sym_id, val) -> status
            Shim::Varset | Shim::Varbind => (&[Ptr, I64, I64], true),
            // (vmctx, n) -> status
            Shim::Unbind => (&[Ptr, I64], true),
            // (vmctx) -> status
            Shim::Backedge => (&[Ptr], true),
            // (vmctx) -> (): the infallible Save* records and pop-handler
            Shim::SaveCurrentBuffer
            | Shim::SaveExcursion
            | Shim::SaveRestriction
            | Shim::PopHandler => (&[Ptr], false),
            // (vmctx, forms) -> ()
            Shim::UnwindProtect => (&[Ptr, I64], false),
            // (tag, value) -> ()
            Shim::ThrowFlow => (&[I64, I64], false),
            // (v) -> t/nil bits
            Shim::IntegerpSlow | Shim::NumberpSlow => (&[I64], true),
            // (vmctx, idx, a[, b[, c]], out_ptr) -> status
            Shim::Builtin1 => (&[Ptr, I64, I64, Ptr], true),
            Shim::Builtin2 => (&[Ptr, I64, I64, I64, Ptr], true),
            Shim::Builtin3 => (&[Ptr, I64, I64, I64, I64, Ptr], true),
            // (vmctx, array, index) -> bits | VALUE_SHIM_SIGNAL
            Shim::Aref | Shim::Memq | Shim::Assq | Shim::Setcar | Shim::Setcdr => {
                (&[Ptr, I64, I64], true)
            }
            // (vmctx, array, index, value) -> bits | VALUE_SHIM_*
            Shim::Aset => (&[Ptr, I64, I64, I64], true),
            // (vmctx, target, stack_len) -> ()
            Shim::PushCc => (&[Ptr, I64, I64], false),
            // (vmctx, target, stack_len, conditions/tag) -> ()
            Shim::PushCcRaw | Shim::PushCatch => (&[Ptr, I64, I64, I64], false),
            // () -> ()
            Shim::SwitchStale => (&[], false),
            // (args_ptr, nargs) -> list bits
            Shim::List => (&[Ptr, I64], true),
            // (idx, args_ptr, nargs, out_ptr) -> status
            Shim::BuiltinSlice => (&[I64, Ptr, I64, Ptr], true),
            // (vmctx, variant, sym, args_ptr, nargs, out_ptr) -> status;
            // (vmctx, which, sym, args_ptr, nargs, out_ptr) -> status
            Shim::NamedBuiltin | Shim::CbsymRead => (&[Ptr, I64, I64, Ptr, I64, Ptr], true),
            // (vmctx, sym, expected, slot_ptr, args_ptr, nargs, out_ptr) -> status
            Shim::CallSpec | Shim::CallSubrSpec => (&[Ptr, I64, I64, I64, Ptr, I64, Ptr], true),
            // pred: (vmctx, kind, sym, expected, slot_ptr, a, out_ptr)
            // eq:   (vmctx, sym, expected, slot_ptr, a, b, out_ptr)
            Shim::PredSpec | Shim::EqInclPropsSpec => (&[Ptr, I64, I64, I64, I64, I64, Ptr], true),
            // (vmctx, kind, sym, expected, slot_ptr, a, b, out_ptr)
            Shim::ArithSpec => (&[Ptr, I64, I64, I64, I64, I64, I64, Ptr], true),
        }
    }

    /// The shim's Cranelift signature for this target.
    pub(crate) fn signature(self, call_conv: CallConv, ptr_ty: Type) -> Signature {
        let (params, returns) = self.shape();
        let mut sig = Signature::new(call_conv);
        for p in params {
            sig.params.push(AbiParam::new(match p {
                P::Ptr => ptr_ty,
                P::I64 => types::I64,
                P::F64 => types::F64,
            }));
        }
        if returns {
            sig.returns.push(AbiParam::new(types::I64));
        }
        sig
    }
}

/// The groups a leaf declares: the base set always, the two optional groups
/// when the body has sites that call them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShimGroups {
    pub(crate) subr_spec: bool,
    pub(crate) cbsym_spec: bool,
}

impl ShimGroups {
    pub(crate) fn contains(self, group: ShimGroup) -> bool {
        match group {
            ShimGroup::Base => true,
            ShimGroup::SubrSpec => self.subr_spec,
            ShimGroup::CbsymSpec => self.cbsym_spec,
        }
    }
}

/// A module's `FuncId` for each declared shim.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShimIds([Option<FuncId>; Shim::COUNT]);

impl ShimIds {
    /// Declare the shims of `groups` into `module` as imports, in
    /// [`Shim`] order (the order the per-leaf declaration always used).
    /// Idempotent per module: a repeated declaration returns the same ids.
    pub(crate) fn declare<M: Module>(
        module: &mut M,
        call_conv: CallConv,
        ptr_ty: Type,
        groups: ShimGroups,
    ) -> Result<ShimIds, CompileError> {
        let mut ids = [None; Shim::COUNT];
        for shim in Shim::iter() {
            if !groups.contains(shim.group()) {
                continue;
            }
            let id = module
                .declare_function(
                    shim.symbol(),
                    Linkage::Import,
                    &shim.signature(call_conv, ptr_ty),
                )
                .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
            ids[shim as usize] = Some(id);
        }
        Ok(ShimIds(ids))
    }

    /// The module id of `shim`, if its group was declared.
    pub(crate) fn get(&self, shim: Shim) -> Option<FuncId> {
        self.0[shim as usize]
    }
}

/// Callable references to the runtime shims, for ONE function under
/// construction: each shim is imported the first time [`Self::get`] asks for
/// it (or all up front under `NEOVM_JIT_LAZY_SHIMS=off`).
pub(crate) struct RtRefs {
    ids: ShimIds,
    /// The groups this leaf may call. A shim outside them is refused even
    /// when the module declared it (the persistent module declares every
    /// group), so the lowering's "refs exist iff the site kind exists" rule
    /// is independent of the module.
    groups: ShimGroups,
    /// The calling convention of every shim: a site that calls a leaf
    /// builtin's trampoline by address (`call_indirect`) builds its
    /// signature with it.
    pub(crate) call_conv: CallConv,
    ptr_ty: Type,
    imported: [Cell<Option<FuncRef>>; Shim::COUNT],
}

impl RtRefs {
    /// The refs of a function about to be lowered into a module whose shims
    /// are `ids`. Under `NEOVM_JIT_LAZY_SHIMS=off` every shim of `groups` is
    /// imported now, in declaration order.
    pub(crate) fn new(
        ids: ShimIds,
        groups: ShimGroups,
        func: &mut Function,
        call_conv: CallConv,
        ptr_ty: Type,
    ) -> RtRefs {
        let refs = RtRefs {
            ids,
            groups,
            call_conv,
            ptr_ty,
            imported: std::array::from_fn(|_| Cell::new(None)),
        };
        if !lazy_shims_enabled() {
            for shim in Shim::iter() {
                if groups.contains(shim.group()) {
                    refs.try_get(func, shim);
                }
            }
        }
        refs
    }

    /// The callable ref of a base shim (always declared).
    pub(crate) fn get(&self, func: &mut Function, shim: Shim) -> FuncRef {
        debug_assert_eq!(shim.group(), ShimGroup::Base, "{shim:?}: use try_get");
        self.try_get(func, shim)
            .unwrap_or_else(|| panic!("base shim {shim:?} is always declared"))
    }

    /// The callable ref of `shim`, or `None` when its group is not one this
    /// leaf declared (the optional speculation groups).
    pub(crate) fn try_get(&self, func: &mut Function, shim: Shim) -> Option<FuncRef> {
        let cell = &self.imported[shim as usize];
        if let Some(r) = cell.get() {
            return Some(r);
        }
        if !self.groups.contains(shim.group()) {
            return None;
        }
        let id = self.ids.get(shim)?;
        // `Module::declare_func_in_func`, without needing the module: an
        // import is its signature plus the module-level name.
        let signature = func.import_signature(shim.signature(self.call_conv, self.ptr_ty));
        let name = func.declare_imported_user_function(UserExternalName {
            namespace: 0,
            index: id.as_u32(),
        });
        let r = func.import_function(ExtFuncData {
            name: ExternalName::user(name),
            signature,
            // An import is never final (`Linkage::Import.is_final()`).
            colocated: false,
            patchable: false,
        });
        cell.set(Some(r));
        Some(r)
    }
}

/// `NEOVM_JIT_LAZY_SHIMS=off`: import every declared shim into every
/// runtime-entering function (the pre-table behaviour, CLIF-identical). Read
/// once, at compile time only.
pub(crate) fn lazy_shims_enabled() -> bool {
    #[cfg(test)]
    if let Some(on) = LAZY_SHIMS_TEST_OVERRIDE.with(Cell::get) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_LAZY_SHIMS").as_deref() != Ok("off"))
}

#[cfg(test)]
thread_local! {
    static LAZY_SHIMS_TEST_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Force lazy shim import on or off for this thread (tests only).
#[cfg(test)]
pub(crate) fn force_lazy_shims_for_test(on: bool) {
    LAZY_SHIMS_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}
