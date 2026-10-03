//! Boolean result projection shared by baseline and opt lowering.
//!
//! These choices and representations belong to one compilation. They contain
//! no Lisp state, do not cross into runtime metadata, and require no shared
//! mutable state when compiler workers or Lisp mutators run concurrently.

use super::*;

/// The successful result view requested by an opcode's verified consumer.
/// This compilation-local choice changes no shim ABI or runtime hint; legacy
/// callers always select `TaggedLisp`, and opt selects `BoolFlag` only under
/// its explicit Boolean pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoolResultMode {
    TaggedLisp,
    BoolFlag,
}

impl BoolResultMode {
    /// Opcodes whose successful result is exactly GNU T or NIL. The shared
    /// numeric emitter retains its existing guards and fallback policy.
    pub(crate) fn supports(op: &Op) -> bool {
        matches!(
            op,
            Op::Null
                | Op::Not
                | Op::Consp
                | Op::Stringp
                | Op::Listp
                | Op::Symbolp
                | Op::Integerp
                | Op::Numberp
                | Op::Eq
                | Op::Eqlsign
                | Op::Lss
                | Op::Gtr
                | Op::Leq
                | Op::Geq
        )
    }

    pub(crate) fn ty(self) -> cranelift_codegen::ir::Type {
        match self {
            Self::TaggedLisp => types::I64,
            Self::BoolFlag => types::I8,
        }
    }

    pub(crate) fn rep(self) -> SlotRep {
        match self {
            Self::TaggedLisp => SlotRep::Tagged,
            Self::BoolFlag => SlotRep::Bool,
        }
    }

    pub(crate) fn constant(self, fb: &mut FunctionBuilder, truth: bool) -> ClifValue {
        let bits = match self {
            Self::TaggedLisp => (if truth { Value::T } else { Value::NIL }).bits() as i64,
            Self::BoolFlag => i64::from(truth),
        };
        fb.ins().iconst(self.ty(), bits)
    }

    /// Project an existing normalized condition. Tagged mode preserves the
    /// old T constant, NIL constant, select instruction order.
    pub(crate) fn condition(self, fb: &mut FunctionBuilder, flag: ClifValue) -> ClifValue {
        match self {
            Self::TaggedLisp => tagged_bool_view(fb, flag),
            Self::BoolFlag => flag,
        }
    }

    /// Preserve shared constant placement in a comparison's existing
    /// multi-arm dispatch. Bool mode has no tagged constants to project.
    pub(crate) fn shared_constants(
        self,
        fb: &mut FunctionBuilder,
    ) -> Option<(ClifValue, ClifValue)> {
        match self {
            Self::TaggedLisp => {
                let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
                let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                Some((t, nil))
            }
            Self::BoolFlag => None,
        }
    }

    pub(crate) fn condition_with_constants(
        self,
        fb: &mut FunctionBuilder,
        flag: ClifValue,
        constants: Option<(ClifValue, ClifValue)>,
    ) -> ClifValue {
        match self {
            Self::TaggedLisp => {
                let (t, nil) = constants.expect("tagged Boolean dispatch has T/NIL constants");
                fb.ins().select(flag, t, nil)
            }
            Self::BoolFlag => flag,
        }
    }

    /// The caller must already have established a successful T/NIL result.
    /// In particular, a generic arithmetic output is not read or normalized
    /// until its STATUS_OK edge has been taken.
    pub(crate) fn successful_tagged(
        self,
        fb: &mut FunctionBuilder,
        tagged: ClifValue,
    ) -> ClifValue {
        match self {
            Self::TaggedLisp => tagged,
            Self::BoolFlag => normalize_tagged_bool(fb, tagged),
        }
    }
}

/// A normalized I8 flag's exact GNU Lisp view. This pure, allocation-free view
/// has a distinct SSA identity: callers must not replace canonical Bool aliases
/// with it through float-box synchronization.
pub(crate) fn tagged_bool_view(fb: &mut FunctionBuilder, flag: ClifValue) -> ClifValue {
    debug_assert_eq!(fb.func.dfg.value_type(flag), types::I8);
    let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
    let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
    fb.ins().select(flag, t, nil)
}

/// Normalize an already successful tagged T/NIL answer; reducing or truncating
/// its word would confuse Lisp's T encoding with a normalized I8 condition.
pub(crate) fn normalize_tagged_bool(fb: &mut FunctionBuilder, tagged: ClifValue) -> ClifValue {
    fb.ins()
        .icmp_imm_u(IntCC::NotEqual, tagged, Value::NIL.bits() as i64)
}
