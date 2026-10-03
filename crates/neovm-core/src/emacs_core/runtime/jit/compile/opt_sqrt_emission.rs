//! Checked selected native sqrt payload emission, child of opt_emission.
//! A successful arm returns only an F64 payload. The root-owned Float sink
//! supplies the fresh semantic identity and its one optional box; equal
//! payloads must never merge identities. One selected GC-free internal guard
//! shim is needed; existing Lisp Call ABI/runtime structures are unchanged.

use super::super::numeric_carrier::{self, NumericCarrier};
use super::super::sqrt_snapshot::SqrtCallWitness;
use super::*;
use cranelift_codegen::ir::condcodes::FloatCC;
use cranelift_codegen::ir::immediates::Ieee64;

/// An actual compiler view of the current argument. FloatPayload is supplied
/// only by the owning Float web when its verified semantic value is a float;
/// Runtime uses existing native slot transport. This is invocation-local CLIF
/// metadata, not a shared object, cache, runtime ABI or future recipe stand-in.
#[derive(Clone, Copy)]
pub(super) enum SqrtArgument {
    Runtime(RuntimeValue),
    FloatPayload(ClifValue),
    /// Actual root-verified tuple; Borrow resolves only under this Call exit.
    Carrier(NumericCarrier),
}

/// Full original Call recovery: no partial numeric stack or rerun-from-entry.
/// The owning snapshot implementation must materialize a future virtual Float
/// through its original identity/optional-box recipe in this cold exit only.
fn precise_call_exit(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    let frame = inst
        .frame
        .ok_or(CompileError::UnsupportedOp("opt-sqrt:frame"))?;
    let state = &ctx.func.frames[frame.index()];
    if state.pc != inst.pc || state.stack.len() < 2 {
        return Err(CompileError::UnsupportedOp("opt-sqrt:call-frame"));
    }
    let (pc, handlers) = (state.pc, state.handlers);
    let exact = precise_snapshot(ctx, local, frame)?;
    let first = deopts.len();
    let site = deopt_site(
        ctx.fb,
        pc as usize,
        handlers as usize,
        &exact.0,
        &exact.1,
        deopts,
    );
    override_deopts(ctx.func, frame, &exact, &mut deopts[first..]);
    ctx.fb.set_cold_block(site);
    Ok(site)
}

/// Lower the original retained one-argument Call through a front-captured
/// actual Subr witness. The argument callback lets a future verified Float web
/// expose its actual F64 payload without first materializing a tagged box.
/// The callback emits no Lisp call/GC/poll and cannot change the source frame.
/// All views and guards are compiler-local; no worker dereferences a Subr.
pub(super) fn emit_call_payload(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    witness: &SqrtCallWitness,
    deopts: &mut Vec<PendingDeopt>,
    argument: impl FnOnce(
        &mut EmitContext,
        &mut LocalValues,
        ir::Value,
    ) -> Result<SqrtArgument, CompileError>,
) -> Result<ClifValue, CompileError> {
    if ctx.aot
        || jit_opt_mode() != OptMode::Opt
        || !jit_opt_passes().sink
        || inst.op != ir::Opcode::Opaque(Op::Call(1))
        || inst.args.len() != 2
        || inst.result.is_none()
        || witness.pc != inst.pc as usize
        || witness.expected_subr_bits == 0
    {
        return Err(CompileError::UnsupportedOp("opt-sqrt:call-proof"));
    }
    let rt = ctx
        .rt
        .ok_or(CompileError::UnsupportedOp("opt-sqrt:runtime"))?;
    let site = precise_call_exit(ctx, local, inst, deopts)?;
    // Same existing mutable entry guard as MIR inlining: current Context epoch,
    // attention + asynchronous sources, debugger cell and depth. Never hoist
    // these loads or reuse their success across an earlier callback/Poll.
    lowering::emit_mir_inline_entry_guard(ctx.fb, rt, witness.epoch, site)?;
    let (callee, callee_rep) = ctx.values.read(ctx.fb, ctx.func, local, inst.args[0]);
    let callee = match callee_rep {
        SlotRep::Tagged => callee,
        SlotRep::RawFixnum => retag_fixnum(ctx.fb, callee),
        SlotRep::Bool => super::super::boolean::tagged_bool_view(ctx.fb, callee),
        SlotRep::Flonum { .. } => {
            return Err(CompileError::UnsupportedOp("opt-sqrt:callee-float"));
        }
    };
    let same = lowering::icmp_imm_p(ctx.fb, IntCC::Equal, callee, witness.symbol_bits as i64);
    lowering::emit_guard(ctx.fb, site, same);
    // A Context epoch can match by accident across owners, and a Subr's bits
    // can remain stable while another same-thread Context rewrites its entry.
    // Fresh current-owner function-cell + actual typed A1 entry validation is
    // mandatory on EVERY call; never reuse a mutable SpecSlot epoch hit.
    let binding_ref = rt
        .refs
        .try_get(ctx.fb.func, super::super::Shim::SqrtBindingValid)
        .ok_or(CompileError::UnsupportedOp("opt-sqrt:binding-ref"))?;
    let vmctx = ctx.fb.use_var(rt.vmctx_var);
    let expected = ctx
        .fb
        .ins()
        .iconst(types::I64, witness.expected_subr_bits as i64);
    let epoch = ctx.fb.ins().iconst(types::I64, witness.epoch as i64);
    let call = ctx
        .fb
        .ins()
        .call(binding_ref, &[vmctx, callee, expected, epoch]);
    let valid = ctx.fb.inst_results(call)[0];
    let valid = lowering::icmp_imm_p(ctx.fb, IntCC::NotEqual, valid, 0);
    lowering::emit_guard(ctx.fb, site, valid);
    let payload = match argument(ctx, local, inst.args[1])? {
        SqrtArgument::Runtime((word, rep)) => {
            lowering::stack_as_f64_or_promote(ctx.fb, site, &[word], &[rep], 0)
        }
        SqrtArgument::Carrier(carrier) => {
            let carrier = numeric_carrier::resolve_operand(ctx.fb, site, carrier)?;
            // FIX payload is permitted to be dummy. Derive/promote exact word;
            // only normalized Float consumes its supplied F64 payload.
            numeric_carrier::ready_as_f64(ctx.fb, carrier)?
        }
        SqrtArgument::FloatPayload(payload) => {
            if ctx.fb.func.dfg.value_type(payload) != types::F64 {
                return Err(CompileError::UnsupportedOp("opt-sqrt:payload-type"));
            }
            payload
        }
    };
    // Strictly positive and finite first arm. Both signed zeros, NaNs, infinities,
    // negatives, bignums, markers and wrong types replay the original Call.
    let zero = ctx.fb.ins().f64const(Ieee64::with_bits(0.0f64.to_bits()));
    let infinity = ctx
        .fb
        .ins()
        .f64const(Ieee64::with_bits(f64::INFINITY.to_bits()));
    let positive = ctx.fb.ins().fcmp(FloatCC::GreaterThan, payload, zero);
    let finite = ctx.fb.ins().fcmp(FloatCC::LessThan, payload, infinity);
    let valid = ctx.fb.ins().band(positive, finite);
    lowering::emit_guard(ctx.fb, site, valid);
    Ok(ctx.fb.ins().sqrt(payload))
}
