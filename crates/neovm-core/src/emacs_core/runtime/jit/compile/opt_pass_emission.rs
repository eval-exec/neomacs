//! Native operations introduced by the selected mid-end passes. Threading:
//! all facts and operands belong to one compiler; no Lisp object is inspected
//! on a worker or cached across mutators.

use super::*;

pub(super) fn emit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
) -> Result<Option<RuntimeValue>, CompileError> {
    let runtime = match &inst.op {
        ir::Opcode::BoolConst(value) => (
            ctx.fb.ins().iconst(types::I8, i64::from(*value)),
            SlotRep::Tagged,
        ),
        ir::Opcode::BoolToLisp => {
            let (flag, _) = ctx.values.read(ctx.fb, ctx.func, local, inst.args[0]);
            let flag = if ctx.fb.func.dfg.value_type(flag) == types::I8 {
                flag
            } else {
                lowering::icmp_imm_p(ctx.fb, IntCC::NotEqual, flag, 0)
            };
            let yes = ctx.fb.ins().iconst(types::I64, Value::T.bits() as i64);
            let no = ctx.fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            (ctx.fb.ins().select(flag, yes, no), SlotRep::Tagged)
        }
        ir::Opcode::TypeTest(ty) => {
            let (word, rep) = ctx.values.read(ctx.fb, ctx.func, local, inst.args[0]);
            if rep.is_flonum() {
                return Err(CompileError::UnsupportedOp("opt-emit:type-test-flonum"));
            }
            let word = if rep == SlotRep::RawFixnum {
                retag_fixnum(ctx.fb, word)
            } else {
                word
            };
            (guard_condition(ctx, *ty, word)?, SlotRep::Tagged)
        }
        ir::Opcode::Select => {
            let result_rep =
                ctx.func.values[inst.result.expect("verified Select result").index()].rep;
            if result_rep != ir::Rep::Bool && !result_rep.is_tagged() {
                return Err(CompileError::UnsupportedOp(
                    "opt-emit:select-representation",
                ));
            }
            let (flag, _) = ctx.values.read(ctx.fb, ctx.func, local, inst.args[0]);
            let (yes, yes_rep) = ctx.values.read(ctx.fb, ctx.func, local, inst.args[1]);
            let (no, no_rep) = ctx.values.read(ctx.fb, ctx.func, local, inst.args[2]);
            if yes_rep.is_flonum() || no_rep.is_flonum() {
                return Err(CompileError::UnsupportedOp("opt-emit:select-flonum"));
            }
            let flag = if ctx.fb.func.dfg.value_type(flag) == types::I8 {
                flag
            } else {
                lowering::icmp_imm_p(ctx.fb, IntCC::NotEqual, flag, 0)
            };
            let yes = if result_rep == ir::Rep::Bool && ctx.fb.func.dfg.value_type(yes) == types::I8
            {
                ctx.fb.ins().uextend(types::I64, yes)
            } else if yes_rep == SlotRep::RawFixnum {
                retag_fixnum(ctx.fb, yes)
            } else {
                yes
            };
            let no = if result_rep == ir::Rep::Bool && ctx.fb.func.dfg.value_type(no) == types::I8 {
                ctx.fb.ins().uextend(types::I64, no)
            } else if no_rep == SlotRep::RawFixnum {
                retag_fixnum(ctx.fb, no)
            } else {
                no
            };
            (ctx.fb.ins().select(flag, yes, no), SlotRep::Tagged)
        }
        ir::Opcode::LoadCar | ir::Opcode::LoadCdr => {
            let input = canonical(ctx.func, inst.args[0]);
            if !ctx.func.values[input.index()].ty.is_subset(TypeSet::CONS)
                || ctx.func.values[input.index()].ty.is_bottom()
            {
                return Err(CompileError::UnsupportedOp("opt-emit:cons-read-proof"));
            }
            let (word, rep) = ctx.values.read(ctx.fb, ctx.func, local, input);
            if rep != SlotRep::Tagged {
                return Err(CompileError::UnsupportedOp(
                    "opt-emit:cons-read-representation",
                ));
            }
            let ptr = lowering::band_imm_p(ctx.fb, word, !(crate::tagged::value::TAG_MASK as i64));
            let offset = if matches!(inst.op, ir::Opcode::LoadCdr) {
                jit_layout::CONS_CDR_OFFSET
            } else {
                jit_layout::CONS_CAR_OFFSET
            };
            (
                ctx.fb
                    .ins()
                    .load(types::I64, MemFlagsData::trusted(), ptr, offset as i32),
                SlotRep::Tagged,
            )
        }
        _ => return Err(CompileError::UnsupportedOp("opt-emit:pass-opcode")),
    };
    Ok(Some(runtime))
}
