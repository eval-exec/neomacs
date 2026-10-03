//! Selected sink opcode emission through explicit physical SSA fields.
//! Threading: the verified Func and emitted SSA names are compilation-owned;
//! semantic box caches exist only as actual IR values, never mutable Rust state.
use super::*;
use crate::emacs_core::jit::opt::sink_recipes::{RecipeField, SinkOp};

fn precise_exit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    let frame = inst
        .frame
        .ok_or(CompileError::UnsupportedOp("opt-sink:source-frame"))?;
    let exact = precise_snapshot(ctx, local, frame)?;
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
    Ok(site)
}

pub(super) fn emit_view(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
) -> Result<(), CompileError> {
    if inst
        .result
        .is_some_and(|v| matches!(ctx.func.values[v.index()].rep, ir::Rep::Virtual(_)))
    {
        return Ok(());
    }
    let carrier = sink_snapshot::current_number(ctx, local, inst.args[0])?;
    ctx.sink_produced
        .insert(inst.result.ok_or(CompileError::BadOperand)?, carrier);
    Ok(())
}

pub(super) fn emit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    _id: ir::Inst,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Option<RuntimeValue>, CompileError> {
    let ir::Opcode::Sink(op) = &inst.op else {
        return Err(CompileError::BadOperand);
    };
    match op {
        SinkOp::BorrowNum => {
            let word = sink_snapshot::semantic_tagged(ctx, local, inst.args[0])?;
            let carrier = numeric_carrier::borrow_number(ctx.fb, word)?;
            ctx.sink_produced
                .insert(inst.result.ok_or(CompileError::BadOperand)?, carrier);
            Ok(None)
        }
        SinkOp::SourceNum(op) => {
            // Retain source admission independently of whether machine emission
            // requires its frame. The verified source/point/versions are unchanged.
            let frame = inst
                .frame
                .ok_or(CompileError::UnsupportedOp("opt-sink:source-frame"))?;
            let state = &ctx.func.frames[frame.index()];
            if state.parent.is_some() || state.handlers != 0 {
                return Err(CompileError::UnsupportedOp(
                    "opt-emit:frame-chain-or-handler",
                ));
            }
            let left = sink_snapshot::current_number(ctx, local, inst.args[0])?;
            let right = sink_snapshot::current_number(ctx, local, inst.args[1])?;
            let carrier = if numeric_carrier::binary_has_no_error_path(left, right) {
                // Preserve precise_snapshot's compiler region reset.
                lowering::set_active_region(None);
                numeric_carrier::emit_binary_ready_float(ctx.fb, op, left, right)?
            } else {
                let exit = precise_exit(ctx, local, inst, deopts)?;
                numeric_carrier::emit_binary(ctx.fb, op, exit, left, right)?
            };
            ctx.sink_produced
                .insert(inst.result.ok_or(CompileError::BadOperand)?, carrier);
            Ok(None)
        }
        SinkOp::SourceSqrt => {
            let witness = *ctx
                .sqrt_witnesses
                .get(&(inst.pc as usize))
                .ok_or(CompileError::UnsupportedOp("opt-sink:sqrt-witness"))?;
            let mut original = inst.clone();
            original.op = ir::Opcode::Opaque(Op::Call(1));
            let payload = sqrt_emission::emit_call_payload(
                ctx,
                local,
                &original,
                &witness,
                deopts,
                |ctx, local, value| {
                    Ok(sqrt_emission::SqrtArgument::Carrier(
                        sink_snapshot::current_number(ctx, local, value)?,
                    ))
                },
            )?;
            let carrier = numeric_carrier::fresh_float(ctx.fb, payload)?;
            ctx.sink_produced
                .insert(inst.result.ok_or(CompileError::BadOperand)?, carrier);
            Ok(None)
        }
        SinkOp::SourceCons(_) => Ok(None),
        SinkOp::RecipeField(field) => {
            let owner = canonical(ctx.func, inst.args[0]);
            if let Some(carrier) = ctx.sink_produced.get(&owner).copied() {
                let (word, rep) = match field {
                    RecipeField::Payload => (carrier.payload, SlotRep::Tagged),
                    RecipeField::Word => (carrier.word, SlotRep::Tagged),
                    RecipeField::Ready => (carrier.ready, SlotRep::Bool),
                    RecipeField::RealBox => (carrier.real_box, SlotRep::Tagged),
                    _ => return Err(CompileError::UnsupportedOp("opt-sink:number-projection")),
                };
                return Ok(Some((word, rep)));
            }
            if *field == RecipeField::RealBox
                && matches!(ctx.func.values[owner.index()].rep, ir::Rep::Virtual(_))
            {
                return Ok(Some((
                    ctx.fb.ins().iconst(types::I64, Value::NIL.bits() as i64),
                    SlotRep::Tagged,
                )));
            }
            Err(CompileError::UnsupportedOp(
                "opt-sink:missing-producer-projection",
            ))
        }
        SinkOp::MaterializeNum => {
            // The independent verifier already checks all four actual operand
            // fields against this exact current version before materializing.
            let carrier = sink_snapshot::current_number(ctx, local, inst.args[0])?;
            let rt = ctx
                .rt
                .ok_or(CompileError::UnsupportedOp("opt-sink:materialize-runtime"))?;
            let materialized =
                numeric_carrier::materialize(ctx.fb, &rt.refs, Some(rt), carrier, false)?;
            Ok(Some((materialized.tagged, SlotRep::Tagged)))
        }
        SinkOp::MaterializeCons => {
            let car = sink_snapshot::semantic_tagged(ctx, local, inst.args[1])?;
            let cdr = sink_snapshot::semantic_tagged(ctx, local, inst.args[2])?;
            let cache = ctx.values.read(ctx.fb, ctx.func, local, inst.args[3]).0;
            let rt = ctx
                .rt
                .ok_or(CompileError::UnsupportedOp("opt-sink:materialize-runtime"))?;
            let result = ctx.fb.declare_var(types::I64);
            let cached = ctx.fb.create_block();
            let allocate = ctx.fb.create_block();
            let merge = ctx.fb.create_block();
            let present =
                lowering::icmp_imm_p(ctx.fb, IntCC::NotEqual, cache, Value::NIL.bits() as i64);
            ctx.fb.ins().brif(present, cached, &[], allocate, &[]);
            ctx.fb.switch_to_block(cached);
            ctx.fb.seal_block(cached);
            ctx.fb.def_var(result, cache);
            ctx.fb.ins().jump(merge, &[]);
            ctx.fb.switch_to_block(allocate);
            ctx.fb.seal_block(allocate);
            let allocator = rt.refs.get(ctx.fb.func, Shim::Cons);
            let call = ctx.fb.ins().call(allocator, &[car, cdr]);
            let value = ctx.fb.inst_results(call)[0];
            ctx.fb.def_var(result, value);
            ctx.fb.ins().jump(merge, &[]);
            ctx.fb.switch_to_block(merge);
            ctx.fb.seal_block(merge);
            Ok(Some((ctx.fb.use_var(result), SlotRep::Tagged)))
        }
        SinkOp::CacheBoxAfter => Ok(Some(ctx.values.read(ctx.fb, ctx.func, local, inst.args[2]))),
    }
}
