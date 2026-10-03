//! Native plain-array macros selected by Range. Threading: this child consumes
//! exclusively owned compiler operands and immutable final proof capabilities.
//! It retains no Lisp pointer, mutable length, or backing address across reads.

use super::*;

fn precise_exit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    let frame = inst
        .frame
        .ok_or(CompileError::UnsupportedOp("opt-array:frame"))?;
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
    ctx.fb.set_cold_block(site);
    Ok(site)
}

fn integer(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    value: ir::Value,
) -> Result<(RuntimeValue, ClifValue), CompileError> {
    let value = canonical(ctx.func, value);
    let data = &ctx.func.values[value.index()];
    if data.ty.is_bottom()
        || !data.ty.is_subset(TypeSet::FIXNUM)
        || !matches!(
            data.rep,
            ir::Rep::Tagged | ir::Rep::TaggedFix | ir::Rep::RawInt
        )
    {
        return Err(CompileError::UnsupportedOp("opt-array:integer-proof"));
    }
    let runtime = ctx.values.read(ctx.fb, ctx.func, local, value);
    if ctx.fb.func.dfg.value_type(runtime.0) != types::I64 {
        return Err(CompileError::UnsupportedOp("opt-array:integer-width"));
    }
    let raw = match runtime.1 {
        SlotRep::Tagged => lowering::sshr_imm_p(ctx.fb, runtime.0, FIXNUM_SHIFT as i64),
        SlotRep::RawFixnum => runtime.0,
        _ => return Err(CompileError::UnsupportedOp("opt-array:integer-word")),
    };
    Ok((runtime, raw))
}

/// The final capability is keyed by the unchanged original instruction owner,
/// not by a source pc or a declared type. A result is only its lookup route;
/// the owner/result/op consistency is checked before a capability is used.
pub(super) fn has_plain_read(ctx: &EmitContext, inst: &ir::InstData) -> bool {
    if ctx.aot || !jit_opt_passes().range || inst.op != ir::Opcode::Opaque(Op::Aref) {
        return false;
    }
    let Some(result) = inst.result else {
        return false;
    };
    let Some(data) = ctx.func.values.get(result.index()) else {
        return false;
    };
    let ir::ValueDef::Inst(owner) = data.def else {
        return false;
    };
    ctx.func.insts.get(owner.index()).is_some_and(|original| {
        original.result == Some(result)
            && original.op == ir::Opcode::Opaque(Op::Aref)
            && original.args == inst.args
            && original.frame == inst.frame
            && original.pc == inst.pc
    }) && ctx
        .verified_arrays
        .is_some_and(|caps| caps.reads.contains_key(&owner))
}

/// Called only inside shared_operation, AFTER its complete original frame,
/// semantic operand preparation, snapshot overrides, and alias bookkeeping
/// have been established. The canonical integer can still be raw even though
/// that semantic operand view was tagged. The read itself has no GC or failure
/// path, and always reloads the current owner backing before loading its slot.
pub(super) fn emit_plain_read(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
) -> Result<RuntimeValue, CompileError> {
    if !has_plain_read(ctx, inst) || inst.args.len() != 2 {
        return Err(CompileError::UnsupportedOp("opt-array:unverified-read"));
    }
    let result = inst
        .result
        .ok_or(CompileError::UnsupportedOp("opt-array:read-result"))?;
    let ir::ValueDef::Inst(owner) = ctx.func.values[result.index()].def else {
        return Err(CompileError::UnsupportedOp("opt-array:read-owner"));
    };
    let cap = ctx
        .verified_arrays
        .and_then(|caps| caps.reads.get(&owner))
        .ok_or(CompileError::UnsupportedOp("opt-array:read-capability"))?;
    if cap.index != inst.args[1] {
        return Err(CompileError::UnsupportedOp("opt-array:read-index"));
    }
    let base = ctx.values.read(ctx.fb, ctx.func, local, cap.base);
    let index = ctx.values.read(ctx.fb, ctx.func, local, cap.index);
    if base.1 != SlotRep::Tagged {
        return Err(CompileError::UnsupportedOp("opt-array:read-base-word"));
    }
    lowering::emit_verified_plain_aref(ctx.fb, base.0, index.0, index.1)
}

/// Actual ordered shape checks precede every first object dereference. Neither
/// slot zero nor a profitability hint classifies an array. All failures honor
/// FORCE_DEOPT and resume the original Aref with its complete pre-op frame.
fn shape_guard(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    target: TypeSet,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<RuntimeValue, CompileError> {
    let plain = TypeSet::VECTOR.join(TypeSet::RECORD);
    if target.is_bottom()
        || !target.is_subset(plain)
        || inst.args.len() != 1
        || inst.eff != crate::emacs_core::jit::opt::mem::Effects::MAY_DEOPT
        || inst.mem != crate::emacs_core::jit::opt::mem::AliasClass::None
    {
        return Err(CompileError::UnsupportedOp("opt-array:shape-contract"));
    }
    let input = canonical(ctx.func, inst.args[0]);
    if ctx.func.values[input.index()].rep != ir::Rep::Tagged {
        return Err(CompileError::UnsupportedOp(
            "opt-array:shape-representation",
        ));
    }
    let array = ctx.values.read(ctx.fb, ctx.func, local, input);
    if array.1 != SlotRep::Tagged || ctx.fb.func.dfg.value_type(array.0) != types::I64 {
        return Err(CompileError::UnsupportedOp("opt-array:shape-word"));
    }
    let layout = jit_layout::heap::plain_array_offsets()
        .ok_or(CompileError::UnsupportedOp("opt-array:layout"))?;
    let site = precise_exit(ctx, local, inst, deopts)?;
    let tag = lowering::band_imm_p(ctx.fb, array.0, TAG_MASK as i64);
    let veclike = lowering::icmp_imm_p(
        ctx.fb,
        IntCC::Equal,
        tag,
        crate::tagged::value::TAG_VECLIKE as i64,
    );
    lowering::emit_guard(ctx.fb, site, veclike);
    if let Some(bits) = target.singleton() {
        let identical = lowering::icmp_imm_p(ctx.fb, IntCC::Equal, array.0, bits.0 as i64);
        lowering::emit_guard(ctx.fb, site, identical);
    }
    let object = lowering::band_imm_p(ctx.fb, array.0, !(TAG_MASK as i64));
    let kind = ctx
        .fb
        .ins()
        .load(types::I8, MemFlagsData::trusted(), object, layout.type_tag);
    let accepted = if target.is_subset(TypeSet::VECTOR) {
        ctx.fb.ins().icmp_imm_u(
            IntCC::Equal,
            kind,
            crate::tagged::header::VecLikeType::Vector as u8 as i64,
        )
    } else if target.is_subset(TypeSet::RECORD) {
        ctx.fb.ins().icmp_imm_u(
            IntCC::Equal,
            kind,
            crate::tagged::header::VecLikeType::Record as u8 as i64,
        )
    } else {
        let vector = ctx.fb.ins().icmp_imm_u(
            IntCC::Equal,
            kind,
            crate::tagged::header::VecLikeType::Vector as u8 as i64,
        );
        let record = ctx.fb.ins().icmp_imm_u(
            IntCC::Equal,
            kind,
            crate::tagged::header::VecLikeType::Record as u8 as i64,
        );
        ctx.fb.ins().bor(vector, record)
    };
    lowering::emit_guard(ctx.fb, site, accepted);
    Ok(array)
}

/// The root dispatches only selected shape guards or capability-owned length /
/// bounds instructions here. Raw length facts describe actual valid storage;
/// no narrowed declared length or standalone synthetic pointer is accepted.
pub(super) fn emit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    id: ir::Inst,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Option<RuntimeValue>, CompileError> {
    if ctx.aot || !jit_opt_passes().range {
        return Err(CompileError::UnsupportedOp("opt-array:pass-off"));
    }
    let result = inst
        .result
        .ok_or(CompileError::UnsupportedOp("opt-array:result"))?;
    let output = &ctx.func.values[result.index()];
    let runtime = match inst.op {
        ir::Opcode::CheckType(target) => shape_guard(ctx, local, inst, target, deopts)?,
        ir::Opcode::LoadVecLen => {
            let caps = ctx
                .verified_arrays
                .ok_or(CompileError::UnsupportedOp("opt-array:no-capabilities"))?;
            let length_type = TypeSet::fixnum_range(crate::emacs_core::jit::opt::types::Range {
                lo: 0,
                hi: Value::MOST_POSITIVE_FIXNUM,
            });
            if !caps.length_reads.contains(&id)
                || inst.args.len() != 1
                || output.rep != ir::Rep::RawInt
                || output.ty != length_type
                || inst.eff != crate::emacs_core::jit::opt::mem::Effects::READ_HEAP
                || inst.mem != crate::emacs_core::jit::opt::mem::AliasClass::Unknown
            {
                return Err(CompileError::UnsupportedOp("opt-array:length-contract"));
            }
            let array = ctx.values.read(ctx.fb, ctx.func, local, inst.args[0]);
            if array.1 != SlotRep::Tagged || ctx.fb.func.dfg.value_type(array.0) != types::I64 {
                return Err(CompileError::UnsupportedOp("opt-array:length-word"));
            }
            let layout = jit_layout::heap::plain_array_offsets()
                .ok_or(CompileError::UnsupportedOp("opt-array:layout"))?;
            let object = lowering::band_imm_p(ctx.fb, array.0, !(TAG_MASK as i64));
            let length =
                ctx.fb
                    .ins()
                    .load(types::I64, MemFlagsData::trusted(), object, layout.length);
            (length, SlotRep::RawFixnum)
        }
        ir::Opcode::CheckBounds => {
            let caps = ctx
                .verified_arrays
                .ok_or(CompileError::UnsupportedOp("opt-array:no-capabilities"))?;
            if !caps.bounds.contains(&id) || inst.args.len() != 2 {
                return Err(CompileError::UnsupportedOp("opt-array:unverified-bounds"));
            }
            let (index_runtime, index) = integer(ctx, local, inst.args[0])?;
            let (_, length) = integer(ctx, local, inst.args[1])?;
            let site = precise_exit(ctx, local, inst, deopts)?;
            let valid = ctx.fb.ins().icmp(IntCC::UnsignedLessThan, index, length);
            lowering::emit_guard(ctx.fb, site, valid);
            match output.rep {
                ir::Rep::Tagged | ir::Rep::TaggedFix if index_runtime.1 == SlotRep::Tagged => {
                    index_runtime
                }
                ir::Rep::Tagged | ir::Rep::TaggedFix => {
                    (retag_fixnum(ctx.fb, index), SlotRep::Tagged)
                }
                ir::Rep::RawInt => (index, SlotRep::RawFixnum),
                _ => return Err(CompileError::UnsupportedOp("opt-array:bounds-result")),
            }
        }
        _ => return Err(CompileError::UnsupportedOp("opt-array:opcode")),
    };
    Ok(Some(runtime))
}

/// Raw transport under Range alone is admitted only for final capability-owned
/// current length/bounds or exact same-word views of those scalars. Threading:
/// owned compiler IDs only; declared raw arguments never authorize transport.
pub(super) fn proved_raw_value(ctx: &EmitContext, mut value: ir::Value) -> bool {
    let Some(caps) = ctx.verified_arrays else {
        return false;
    };
    for _ in 0..=ctx.func.values.len() {
        let Some(resolved) = ctx.func.resolve(value) else {
            return false;
        };
        value = resolved;
        let data = &ctx.func.values[value.index()];
        if data.rep != ir::Rep::RawInt || data.ty.is_bottom() || !data.ty.is_subset(TypeSet::FIXNUM)
        {
            return false;
        }
        let ir::ValueDef::Inst(id) = data.def else {
            return false;
        };
        let inst = &ctx.func.insts[id.index()];
        match inst.op {
            ir::Opcode::LoadVecLen => return caps.length_reads.contains(&id),
            ir::Opcode::CheckBounds => return caps.bounds.contains(&id),
            ir::Opcode::Refine(_) if inst.args.len() == 1 => value = inst.args[0],
            _ => return false,
        }
    }
    false
}
