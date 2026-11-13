//! Native integer operations selected by the representation pass. Threading:
//! operands, proofs and deopt snapshots belong to one compilation; no Lisp
//! object is inspected or cached across workers or mutators.

use super::*;
use crate::emacs_core::jit::opt::mem::{AliasClass, Effects};

/// Decode only an actual immutable literal, never an environment template or
/// a refined value. Threading: opaque constant bits and proofs are owned by the
/// compilation; this reads no Lisp heap or mutator state.
fn static_fixnum_payload(ctx: &EmitContext, value: ir::Value) -> Option<i64> {
    if jit_opt_passes().licm {
        let (_, bits) =
            crate::emacs_core::jit::opt::passes::static_fix::static_fix_origin(ctx.func, value)?;
        return Some((bits.0 as i64) >> FIXNUM_SHIFT);
    }
    let data = &ctx.func.values[value.index()];
    if data.ty.is_bottom() || !data.ty.is_subset(TypeSet::FIXNUM) {
        return None;
    }
    let ir::ValueDef::Inst(definition) = data.def else {
        return None;
    };
    let ir::Opcode::Const(index) = ctx.func.insts[definition.index()].op else {
        return None;
    };
    if (index as usize) < ctx.dynamic_prefix {
        return None;
    }
    let bits = ctx.func.consts.get(index as usize)?.0;
    if bits & FIXNUM_CHECK_MASK as u64 != FIXNUM_CHECK_VALUE as u64 {
        return None;
    }
    Some((bits as i64) >> FIXNUM_SHIFT)
}

fn fixnum_operand(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    value: ir::Value,
    expected: ir::Rep,
) -> Result<ClifValue, CompileError> {
    let value = canonical(ctx.func, value);
    let data = &ctx.func.values[value.index()];
    if data.rep != expected || data.ty.is_bottom() || !data.ty.is_subset(TypeSet::FIXNUM) {
        return Err(CompileError::UnsupportedOp("opt-emit:integer-proof"));
    }
    let (word, rep) = ctx.values.read(ctx.fb, ctx.func, local, value);
    let expected = if expected == ir::Rep::RawInt {
        SlotRep::RawFixnum
    } else {
        SlotRep::Tagged
    };
    if rep != expected || ctx.fb.func.dfg.value_type(word) != types::I64 {
        return Err(CompileError::UnsupportedOp("opt-emit:integer-operand"));
    }
    Ok(word)
}

/// Every failed speculation resumes the original operation with its complete
/// GNU stack. Threading: the queued exit and its scalar views are compiler-local.
fn precise_exit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    let frame = inst
        .frame
        .ok_or(CompileError::UnsupportedOp("opt-emit:integer-frame"))?;
    set_snapshot(ctx, local, frame)?;
    let exact = snapshot(ctx, local, frame)?;
    let state = &ctx.func.frames[frame.index()];
    let first = deopts.len();
    let site = deopt_site(
        ctx.fb,
        state.pc as usize,
        state.handlers as usize,
        &exact.0,
        &exact.1,
        deopts,
    );
    override_deopts(ctx.func, frame, &exact, &mut deopts[first..]);
    ctx.fb.set_cold_block(site);
    Ok(site)
}

fn no_overflow(ctx: &mut EmitContext, site: Block, overflow: ClifValue) {
    let valid = lowering::icmp_imm_p(ctx.fb, IntCC::Equal, overflow, 0);
    lowering::emit_guard(ctx.fb, site, valid);
}

/// Emit only supported selected integer operations. The caller dispatches
/// these opcodes after verification; unproved unchecked operations remain a
/// native refusal until a later pass supplies a range proof. Threading: all
/// state is borrowed from the owning compiler invocation.
pub(super) fn emit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Option<RuntimeValue>, CompileError> {
    if !jit_opt_passes().reps {
        return Err(CompileError::UnsupportedOp("opt-emit:integer-pass-off"));
    }
    if let Some(runtime) = emit_range_proved_unchecked(ctx, local, inst)? {
        return Ok(Some(runtime));
    }
    let runtime = match inst.op {
        ir::Opcode::UntagFix => {
            let input = canonical(ctx.func, inst.args[0]);
            let rep = ctx.func.values[input.index()].rep;
            if !rep.is_tagged() {
                return Err(CompileError::UnsupportedOp("opt-emit:untag-representation"));
            }
            let payload = if let Some(payload) = static_fixnum_payload(ctx, input) {
                ctx.fb.ins().iconst(types::I64, payload)
            } else {
                let word = fixnum_operand(ctx, local, input, rep)?;
                lowering::sshr_imm_p(ctx.fb, word, FIXNUM_SHIFT as i64)
            };
            (payload, SlotRep::RawFixnum)
        }
        ir::Opcode::TagFix => {
            let word = fixnum_operand(ctx, local, inst.args[0], ir::Rep::RawInt)?;
            (retag_fixnum(ctx.fb, word), SlotRep::Tagged)
        }
        ir::Opcode::FixCmp(cmp) => {
            let input = canonical(ctx.func, inst.args[0]);
            let rep = ctx.func.values[input.index()].rep;
            if !matches!(rep, ir::Rep::TaggedFix | ir::Rep::RawInt) {
                return Err(CompileError::UnsupportedOp(
                    "opt-emit:compare-representation",
                ));
            }
            let a = fixnum_operand(ctx, local, inst.args[0], rep)?;
            let b = fixnum_operand(ctx, local, inst.args[1], rep)?;
            let cc = match cmp {
                ir::Cmp::Eq => IntCC::Equal,
                ir::Cmp::Ne => IntCC::NotEqual,
                ir::Cmp::Lt => IntCC::SignedLessThan,
                ir::Cmp::Le => IntCC::SignedLessThanOrEqual,
                ir::Cmp::Gt => IntCC::SignedGreaterThan,
                ir::Cmp::Ge => IntCC::SignedGreaterThanOrEqual,
            };
            // A tagged fixnum's signed word is monotone in its payload, so a
            // comparison needs no untagging. The unselected Bool path keeps
            // the existing compiler-only 0/1 transport until BoolToLisp.
            (
                ctx.fb.ins().icmp(cc, a, b),
                if jit_opt_passes().bool_rep {
                    SlotRep::Bool
                } else {
                    SlotRep::Tagged
                },
            )
        }
        ir::Opcode::FixAdd { checked: true }
        | ir::Opcode::FixSub { checked: true }
        | ir::Opcode::FixMul { checked: true } => {
            let result = inst
                .result
                .ok_or(CompileError::UnsupportedOp("opt-emit:integer-result"))?;
            let rep = ctx.func.values[result.index()].rep;
            if !matches!(rep, ir::Rep::TaggedFix | ir::Rep::RawInt) {
                return Err(CompileError::UnsupportedOp(
                    "opt-emit:integer-representation",
                ));
            }
            let a = fixnum_operand(ctx, local, inst.args[0], rep)?;
            let b = fixnum_operand(ctx, local, inst.args[1], rep)?;
            let site = precise_exit(ctx, local, inst, deopts)?;
            let value = if rep == ir::Rep::RawInt {
                match inst.op {
                    ir::Opcode::FixAdd { .. } => {
                        lowering::raw_fixnum_addsub(ctx.fb, site, false, a, b)
                    }
                    ir::Opcode::FixSub { .. } => {
                        lowering::raw_fixnum_addsub(ctx.fb, site, true, a, b)
                    }
                    _ => {
                        // A and B fit the fixnum domain. B << FIXNUM_SHIFT
                        // therefore fits i64 exactly; a signed overflow of the
                        // scaled product is precisely fixnum-domain overflow.
                        let scaled = lowering::ishl_imm_p(ctx.fb, b, FIXNUM_SHIFT as i64);
                        let (product, overflow) = ctx.fb.ins().smul_overflow(a, scaled);
                        no_overflow(ctx, site, overflow);
                        lowering::sshr_imm_p(ctx.fb, product, FIXNUM_SHIFT as i64)
                    }
                }
            } else {
                // Tagged V(n) = (n << FIXNUM_SHIFT) + FIXNUM_CHECK_VALUE.
                // Removing B's bias yields its exact scaled payload, including
                // the most-negative fixnum, without an extra range check.
                let scaled = lowering::iadd_imm_p(ctx.fb, b, -(FIXNUM_CHECK_VALUE as i64));
                match inst.op {
                    ir::Opcode::FixAdd { .. } => {
                        let (sum, overflow) = ctx.fb.ins().sadd_overflow(a, scaled);
                        no_overflow(ctx, site, overflow);
                        sum
                    }
                    ir::Opcode::FixSub { .. } => {
                        let (difference, overflow) = ctx.fb.ins().ssub_overflow(a, scaled);
                        no_overflow(ctx, site, overflow);
                        difference
                    }
                    _ => {
                        let raw = lowering::sshr_imm_p(ctx.fb, a, FIXNUM_SHIFT as i64);
                        let (product, overflow) = ctx.fb.ins().smul_overflow(raw, scaled);
                        no_overflow(ctx, site, overflow);
                        lowering::iadd_imm_p(ctx.fb, product, FIXNUM_CHECK_VALUE as i64)
                    }
                }
            };
            (
                value,
                if rep == ir::Rep::RawInt {
                    SlotRep::RawFixnum
                } else {
                    SlotRep::Tagged
                },
            )
        }
        _ => return Err(CompileError::UnsupportedOp("opt-emit:integer-opcode")),
    };
    Ok(Some(runtime))
}

/// Optional new branch inside the existing selected native integer emitter.
/// Neither source PCs nor full recovery frames are rewritten here. Independent
/// endpoint validation refuses bad/unsupported IR rather than emitting wrapping
/// Lisp arithmetic. The trusted Range producer still owns branch-fact validity.
/// Threading: exclusively borrowed compiler values, no retained Lisp state.
fn emit_range_proved_unchecked(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
) -> Result<Option<RuntimeValue>, CompileError> {
    if !matches!(
        inst.op,
        ir::Opcode::FixAdd { checked: false }
            | ir::Opcode::FixSub { checked: false }
            | ir::Opcode::FixMul { checked: false }
    ) {
        return Ok(None);
    }
    if !jit_opt_passes().range
        || !jit_opt_passes().reps
        || inst.eff != Effects::PURE
        || inst.mem != AliasClass::None
        || inst.args.len() != 2
    {
        return Err(CompileError::UnsupportedOp(
            "opt-emit:range-unchecked-contract",
        ));
    }
    let result = inst.result.ok_or(CompileError::BadOperand)?;
    let result_data = &ctx.func.values[result.index()];
    let rep = result_data.rep;
    if !matches!(rep, ir::Rep::TaggedFix | ir::Rep::RawInt)
        || result_data.ty.is_bottom()
        || !result_data.ty.is_subset(TypeSet::FIXNUM)
    {
        return Err(CompileError::UnsupportedOp("opt-emit:range-result-proof"));
    }
    let intervals = inst
        .args
        .iter()
        .map(|&input| {
            let input = canonical(ctx.func, input);
            let data = &ctx.func.values[input.index()];
            if data.rep != rep || data.ty.is_bottom() || !data.ty.is_subset(TypeSet::FIXNUM) {
                return None;
            }
            data.ty.range()
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(CompileError::UnsupportedOp("opt-emit:range-operand-proof"))?;
    if !crate::emacs_core::jit::opt::passes::range::arithmetic_result_fits(
        &inst.op,
        intervals[0],
        intervals[1],
        result_data.ty,
    ) {
        return Err(CompileError::UnsupportedOp(
            "opt-emit:range-declaration-proof",
        ));
    }
    let a = fixnum_operand(ctx, local, inst.args[0], rep)?;
    let b = fixnum_operand(ctx, local, inst.args[1], rep)?;
    // No snapshot, precise_exit, overflow flag or guard on proved arithmetic.
    // Current checked emit logic remains byte-for-byte below this new branch.
    let value = if rep == ir::Rep::RawInt {
        match inst.op {
            ir::Opcode::FixAdd { .. } => ctx.fb.ins().iadd(a, b),
            ir::Opcode::FixSub { .. } => ctx.fb.ins().isub(a, b),
            _ => ctx.fb.ins().imul(a, b),
        }
    } else {
        let scaled = lowering::iadd_imm_p(ctx.fb, b, -(FIXNUM_CHECK_VALUE as i64));
        match inst.op {
            ir::Opcode::FixAdd { .. } => ctx.fb.ins().iadd(a, scaled),
            ir::Opcode::FixSub { .. } => ctx.fb.ins().isub(a, scaled),
            _ => {
                let raw = lowering::sshr_imm_p(ctx.fb, a, FIXNUM_SHIFT as i64);
                let product = ctx.fb.ins().imul(raw, scaled);
                lowering::iadd_imm_p(ctx.fb, product, FIXNUM_CHECK_VALUE as i64)
            }
        }
    };
    Ok(Some((
        value,
        if rep == ir::Rep::RawInt {
            SlotRep::RawFixnum
        } else {
            SlotRep::Tagged
        },
    )))
}
