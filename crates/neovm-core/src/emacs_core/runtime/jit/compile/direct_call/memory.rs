//! Direct entry with the existing memory ABI, gated by
//! `NEOVM_JIT_DIRECT_MEMORY`. The parent owns all GNU frame/depth/guard and
//! exit protocols; only argument/result transport differs here.
//! Threading: emits code for one compiler's leaf, using only that native
//! activation's stack slots. Adds no shared state or Lisp-state caches.

use super::super::lowering::RtCtx;
use super::*;

/// Call an exact pass-through frameless JIT memory entry. The original
/// arguments are already recorded in the parent site's backtrace frame;
/// calls wider than two also stored them into `call_args_slot` for that
/// frame. One/two-word frames store their arguments inline, so spill those
/// words here. The memory body writes the result slot on `STATUS_OK` only;
/// initialize it to nil so exceptional returns never load undefined data.
pub(super) fn emit_raw_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    entry: ClifValue,
    vmctx: ClifValue,
    aux: ClifValue,
    args: &[ClifValue],
) -> (ClifValue, ClifValue) {
    if args.len() <= 2 {
        for (i, &arg) in args.iter().enumerate() {
            fb.ins()
                .stack_store(rt.ptr_ty, arg, rt.call_args_slot, (i * 8) as i32);
        }
    }
    let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
    let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
    let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
    fb.ins().stack_store(rt.ptr_ty, nil, rt.call_result_slot, 0);
    let sig = fb.import_signature(LeafAbi::Memory.signature(rt.refs.call_conv, rt.ptr_ty));
    let call = fb
        .ins()
        .call_indirect(sig, entry, &[vmctx, args_addr, out_addr, aux]);
    let status = fb.inst_results(call)[0];
    let value = fb
        .ins()
        .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
    (value, status)
}
