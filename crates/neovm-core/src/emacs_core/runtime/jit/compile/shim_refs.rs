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
//! Existing shim IDs and signatures keep their order. New JIT-only shape,
//! census and list-HOF shims are appended in optional groups; with their knobs
//! off they are never imported, including under eager imports. Their exported
//! names extend the ABI-salted name set, while AOT emission keeps these groups off.

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
    /// T1 profiling only; never selected by AOT or a knob-off leaf.
    Tier2Profile,
    /// Argument-normalizing direct calls (JIT only).
    DirectShapes,
    /// The call-shape measurement mode (JIT only).
    CallCensus,
    /// The contained framed direct call (JIT only, independent shape bit).
    DirectFramed,
    Hof,
    Tier2ArrayProfile,
    OptSink,
    /// String collection journaling, selected only by its compile-time knob.
    CollectionJournal,
    /// Cold GEN0 observed-window refinement; declared only in Observed JIT.
    CollectionObservationGate,
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
    /// `Op::Aset`: the value's bits or `VALUE_SHIM_SIGNAL`.
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
    /// The cold side of the entry stack guard (`compile::stack_guard`).
    StackCheck,
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
    // Appended to preserve every existing module declaration id/order.
    TierRequest,
    T2CallProf,
    T2CallSubrProf,
    T2CallFeedbackProf,
    T2CallFeedbackCensus,
    T2CallUseProf,
    T2ApplyUseProf,
    T2RecordCallUseTarget,
    /// Append new shims after existing IDs to preserve off-mode imports.
    /// `neovm_jit_direct_slow`: the reference call and shaped-entry arming.
    DirectSlow,
    /// `neovm_jit_call_census`: read-only call-shape measurement.
    CallCensus,
    /// Exact accepted spec-shim entries, selected only for a census build.
    CallSpecCensus,
    /// The contained framed direct-entry trampoline (JIT only).
    DirectFramed,
    HofLength,
    HofStart,
    HofStore,
    HofCursor,
    HofFinish,
    HofAbort,
    // Optional Opt shims follow every main identity.
    // Optional Opt shims follow every main identity.
    T2RecordArrayUse,
    SqrtBindingValid,
    /// A guarded string byte store; no Lisp allocation, callback or safe point.
    StringCollectionWrite,
    /// Cold exact-owner eligibility and empty-gap publication; no safe point.
    UnobservedCollectionOwner,
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
            Shim::SqrtBindingValid => "neovm_jit_sqrt_binding_valid",
            Shim::T2RecordArrayUse => "neovm_jit_t2_record_array_use",
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
            Shim::StackCheck => "neovm_jit_stack_check",
            Shim::CallSubrSpec => "neovm_jit_call_subr_spec",
            Shim::PredSpec => "neovm_jit_pred_spec",
            Shim::EqInclPropsSpec => "neovm_jit_eq_incl_props_spec",
            Shim::ArithSpec => "neovm_jit_arith_spec",
            Shim::CbsymSpec => "neovm_jit_cbsym_spec",
            Shim::CbsymRead => "neovm_jit_cbsym_read",
            Shim::TierRequest => "neovm_jit_tier_request",
            Shim::T2CallProf => "neovm_jit_t2_call_prof",
            Shim::T2CallSubrProf => "neovm_jit_t2_call_subr_prof",
            Shim::T2CallFeedbackProf => "neovm_jit_t2_call_feedback_prof",
            Shim::T2CallFeedbackCensus => "neovm_jit_t2_call_feedback_census",
            Shim::T2CallUseProf => "neovm_jit_t2_call_use_prof",
            Shim::T2ApplyUseProf => "neovm_jit_t2_apply_use_prof",
            Shim::T2RecordCallUseTarget => "neovm_jit_t2_record_call_use_target",
            Shim::DirectSlow => "neovm_jit_direct_slow",
            Shim::CallCensus => "neovm_jit_call_census",
            Shim::CallSpecCensus => "neovm_jit_call_spec_census",
            Shim::DirectFramed => "neovm_jit_direct_framed",
            Shim::HofLength => "neovm_jit_hof_length",
            Shim::HofStart => "neovm_jit_hof_start",
            Shim::HofStore => "neovm_jit_hof_store",
            Shim::HofCursor => "neovm_jit_hof_cursor",
            Shim::HofFinish => "neovm_jit_hof_finish",
            Shim::HofAbort => "neovm_jit_hof_abort",
            Shim::StringCollectionWrite => "neovm_jit_string_collection_write",
            Shim::UnobservedCollectionOwner => "neovm_jit_unobserved_collection_owner",
        }
    }

    /// The declaration group (see [`ShimGroup`]).
    pub(crate) fn group(self) -> ShimGroup {
        match self {
            Shim::SqrtBindingValid => ShimGroup::OptSink,
            Shim::T2RecordArrayUse => ShimGroup::Tier2ArrayProfile,
            Shim::StringCollectionWrite => ShimGroup::CollectionJournal,
            Shim::UnobservedCollectionOwner => ShimGroup::CollectionObservationGate,
            Shim::CallSubrSpec | Shim::PredSpec | Shim::EqInclPropsSpec | Shim::ArithSpec => {
                ShimGroup::SubrSpec
            }
            Shim::CbsymSpec | Shim::CbsymRead => ShimGroup::CbsymSpec,
            Shim::TierRequest
            | Shim::T2CallProf
            | Shim::T2CallSubrProf
            | Shim::T2CallFeedbackProf
            | Shim::T2CallFeedbackCensus
            | Shim::T2CallUseProf
            | Shim::T2ApplyUseProf
            | Shim::T2RecordCallUseTarget => ShimGroup::Tier2Profile,
            Shim::DirectSlow => ShimGroup::DirectShapes,
            Shim::DirectFramed => ShimGroup::DirectFramed,
            Shim::CallCensus | Shim::CallSpecCensus => ShimGroup::CallCensus,
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
            | Shim::CallSpec
            | Shim::StackCheck => ShimGroup::Base,
            Shim::HofLength
            | Shim::HofStart
            | Shim::HofStore
            | Shim::HofCursor
            | Shim::HofFinish
            | Shim::HofAbort => ShimGroup::Hof,
        }
    }

    /// `(params, returns a status/value word)`. The cold observation gate
    /// returns an I8 predicate; every other non-void result is I64.
    fn shape(self) -> (&'static [P], bool) {
        use P::{F64, I64, Ptr};
        match self {
            Shim::SqrtBindingValid => (&[Ptr, I64, I64, I64], true),
            Shim::T2RecordArrayUse => (&[Ptr, I64, Ptr], false),
            Shim::StringCollectionWrite => (&[I64], false),
            Shim::UnobservedCollectionOwner => (&[I64], true),
            // (leaf_obs) -> ()
            Shim::TierRequest => (&[Ptr], false),
            // Generic call's ABI plus the leaf's observation pointer.
            Shim::T2CallProf => (&[Ptr, I64, Ptr, I64, Ptr, Ptr], true),
            // Subr spec's ABI plus the leaf's observation pointer.
            Shim::T2CallSubrProf => (&[Ptr, I64, I64, I64, Ptr, I64, Ptr, Ptr], true),
            // Feedback call's ABI plus the leaf's observation pointer.
            Shim::T2CallFeedbackProf | Shim::T2CallFeedbackCensus => {
                (&[Ptr, I64, Ptr, I64, Ptr, Ptr, Ptr], true)
            }
            // Use T1: call ABI + site + obs + compile-time HOF credit.
            Shim::T2CallUseProf => (&[Ptr, I64, Ptr, I64, Ptr, Ptr, Ptr, I64], true),
            // Use T1: apply ABI + site + obs.
            Shim::T2ApplyUseProf => (&[Ptr, I64, Ptr, I64, Ptr, Ptr, Ptr], true),
            // Use T1: (site, callback, obs) -> ().
            Shim::T2RecordCallUseTarget => (&[Ptr, I64, Ptr], false),
            Shim::HofLength => (&[I64], true),
            Shim::HofStart => (&[Ptr, I64, Ptr, I64, I64, I64], true),
            Shim::HofStore | Shim::HofCursor => (&[Ptr, I64, I64], true),
            Shim::HofFinish => (&[Ptr, I64, I64, I64, I64, I64], true),
            Shim::HofAbort => (&[Ptr, I64], true),
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
            // (vmctx) -> status; (vmctx) -> vmctx or null
            Shim::Backedge | Shim::StackCheck => (&[Ptr], true),
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
            // (vmctx, array, index, value) -> bits | VALUE_SHIM_SIGNAL
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
            // (vmctx, sym, expected, slot_ptr, args_ptr, nargs, out_ptr) -> status;
            // CallSpec's `sym` is the symbol's tagged bits, CallSubrSpec's its id
            Shim::CallSpec | Shim::CallSubrSpec | Shim::CallSpecCensus => {
                (&[Ptr, I64, I64, I64, Ptr, I64, Ptr], true)
            }
            // pred: (vmctx, kind, sym, expected, slot_ptr, a, out_ptr)
            // eq:   (vmctx, sym, expected, slot_ptr, a, b, out_ptr)
            Shim::PredSpec | Shim::EqInclPropsSpec => (&[Ptr, I64, I64, I64, I64, I64, Ptr], true),
            // (vmctx, kind, sym, expected, slot_ptr, a, b, out_ptr)
            Shim::ArithSpec => (&[Ptr, I64, I64, I64, I64, I64, I64, Ptr], true),
            // (vmctx, sym, expected, slot, args, nargs, out, shape) -> status
            Shim::DirectSlow => (&[Ptr, I64, I64, I64, Ptr, I64, Ptr, I64], true),
            // (vmctx, callee, leaf, const_base, args, nargs, bt_count, out)
            Shim::DirectFramed => (&[Ptr, I64, I64, I64, Ptr, I64, I64, Ptr], true),
            // (vmctx, site, callee_or_slot, arg0, nargs) -> ()
            Shim::CallCensus => (&[Ptr, I64, I64, I64, I64], false),
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
            let result = if self == Shim::UnobservedCollectionOwner {
                types::I8
            } else {
                types::I64
            };
            sig.returns.push(AbiParam::new(result));
        }
        sig
    }
}

/// The groups a leaf declares: the base set always, speculation groups
/// when the body has their sites, and JIT-only shapes/census groups only
/// under their knobs. The frontend keeps optional groups off even when the
/// persistent backend declared them, preserving eager-import CLIF identity.
/// Threading: these immutable compile facts belong to one leaf build; the
/// table stores no Lisp state and each mutator owns its compilation module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShimGroups {
    pub(crate) subr_spec: bool,
    pub(crate) cbsym_spec: bool,
    pub(crate) tier2_profile: bool,
    pub(crate) direct_shapes: bool,
    pub(crate) call_census: bool,
    pub(crate) direct_framed: bool,
    pub(crate) hof: bool,
    pub(crate) collection_journal: bool,
    pub(crate) collection_observation_gate: bool,
}

/// Scalar frontend selection. Workers obtain this requirement from their
/// immutable imported-symbol payload; pure compilation never reads a heap.
/// GEN1 may declare the optional suffix but emits no refinement call.
pub(crate) fn collection_observation_gate_enabled() -> bool {
    crate::tagged::collection_reads::compiled_journal_mode()
        == crate::tagged::collection_reads::CompiledJournalMode::Observed
}

impl ShimGroups {
    pub(crate) fn contains(self, group: ShimGroup) -> bool {
        match group {
            ShimGroup::OptSink => false,
            ShimGroup::Tier2ArrayProfile => false,
            ShimGroup::Base => true,
            ShimGroup::SubrSpec => self.subr_spec,
            ShimGroup::CbsymSpec => self.cbsym_spec,
            ShimGroup::Tier2Profile => self.tier2_profile,
            ShimGroup::DirectShapes => self.direct_shapes,
            ShimGroup::CallCensus => self.call_census,
            ShimGroup::DirectFramed => self.direct_framed,
            ShimGroup::Hof => self.hof,
            ShimGroup::CollectionJournal => self.collection_journal,
            ShimGroup::CollectionObservationGate => self.collection_observation_gate,
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

    /// Compile-local group selection without importing a signature. This
    /// keeps AOT and scalar-policy-off emitters on their original paths.
    pub(crate) fn group_enabled(&self, group: ShimGroup) -> bool {
        self.groups.contains(group)
    }

    /// The callable ref of a base shim (always declared).
    pub(crate) fn get(&self, func: &mut Function, shim: Shim) -> FuncRef {
        debug_assert!(
            matches!(shim.group(), ShimGroup::Base | ShimGroup::Hof),
            "{shim:?}: use try_get"
        );
        let shim = if shim == Shim::CallSpec && self.groups.call_census {
            Shim::CallSpecCensus
        } else {
            shim
        };
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

// BEGIN T35 SELECTED SHIM BACKEND
/// Additional compiler-owned import requirements for one selected frontend.
/// Threading: immutable scalars belong to one compilation; no Lisp state,
/// runtime layout, mutator cache, or worker-side knob read is introduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectedShimGroups {
    pub(crate) main: ShimGroups,
    pub(crate) array_profile: bool,
    pub(crate) sink_versions: bool,
}

impl ShimIds {
    /// Declare the unchanged main prefix, then requested optional suffixes.
    pub(crate) fn declare_selected<M: Module>(
        module: &mut M,
        call_conv: CallConv,
        ptr_ty: Type,
        groups: SelectedShimGroups,
    ) -> Result<ShimIds, CompileError> {
        let ids = Self::declare(module, call_conv, ptr_ty, groups.main)?;
        Self::append_selected(module, call_conv, ptr_ty, ids, groups)
    }

    /// Recover every already-published optional ID and append absent requests.
    /// Module-name lookup preserves the union even if an intervening main-only
    /// profiling redeclaration returned a table containing just main IDs.
    pub(crate) fn append_selected<M: Module>(
        module: &mut M,
        call_conv: CallConv,
        ptr_ty: Type,
        mut ids: ShimIds,
        groups: SelectedShimGroups,
    ) -> Result<ShimIds, CompileError> {
        for (requested, shim) in [
            (groups.array_profile, Shim::T2RecordArrayUse),
            (groups.sink_versions, Shim::SqrtBindingValid),
        ] {
            let present = module.declarations().get_name(shim.symbol());
            let id = match present {
                Some(cranelift_module::FuncOrDataId::Func(id)) => Some(id),
                Some(cranelift_module::FuncOrDataId::Data(_)) => {
                    return Err(CompileError::Backend(BackendError::Define(format!(
                        "selected shim {} is declared as data",
                        shim.symbol()
                    ))));
                }
                None if requested => Some(
                    module
                        .declare_function(
                            shim.symbol(),
                            Linkage::Import,
                            &shim.signature(call_conv, ptr_ty),
                        )
                        .map_err(|error| {
                            CompileError::Backend(BackendError::Define(error.to_string()))
                        })?,
                ),
                None => None,
            };
            ids.0[shim as usize] = id;
        }
        Ok(ids)
    }
}

impl RtRefs {
    /// Preserve the main constructor and eagerly append only the selected
    /// optional refs. Their cached cells make the unchanged try_get accept
    /// them; unselected optional refs remain unavailable even in a module
    /// that previously published their IDs. Main lazy/eager behavior is exact.
    pub(crate) fn new_selected(
        ids: ShimIds,
        groups: SelectedShimGroups,
        func: &mut Function,
        call_conv: CallConv,
        ptr_ty: Type,
    ) -> RtRefs {
        let refs = Self::new(ids, groups.main, func, call_conv, ptr_ty);
        if groups.array_profile {
            refs.import_selected(func, Shim::T2RecordArrayUse);
        }
        if groups.sink_versions {
            refs.import_selected(func, Shim::SqrtBindingValid);
        }
        refs
    }

    fn import_selected(&self, func: &mut Function, shim: Shim) -> Option<FuncRef> {
        let cell = &self.imported[shim as usize];
        if let Some(r) = cell.get() {
            return Some(r);
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
// END T35 SELECTED SHIM BACKEND
