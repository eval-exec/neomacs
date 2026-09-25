//! The entry guard of compiled leaves that can re-enter Lisp: GNU's
//! "Bytecode stack overflow" (`setup_frame`, src/bytecode.c:514-515) before
//! the native stack runs out. The runtime half and the rationale are in
//! `eval::native_stack`.
//!
//! Only a body with an op that can run arbitrary Lisp ([`op_may_reenter_lisp`])
//! can take part in a recursion, and every recursion through compiled code
//! enters such a body once per level, so guarding those entries bounds it.
//! A call-free body's stack is its own frame plus the shims it calls; it is
//! not guarded (and may be entered with a null vmctx).
//!
//! The test compares the leaf's `out` pointer -- the result slot of its
//! caller, a word of the calling frame for every native caller -- against
//! `Context::jit_stack_limit`: a load, a compare and a branch, where reading
//! the stack pointer would add a move. The cold side calls
//! `neovm_jit_stack_check`, which measures the real stack: exhausted, the
//! error is stashed and the leaf returns `STATUS_SIGNAL`; otherwise the
//! limit names another segment (or `out` is not on this stack), and the
//! leaf runs.

use super::Shim;
use super::lowering::RtCtx;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use cranelift_codegen::ir::Value as ClifValue;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlagsData, types};
use cranelift_frontend::FunctionBuilder;

/// Whether `op` can run arbitrary Lisp, and so continue a recursion: a call,
/// a builtin (`funcall`, `mapcar`, a buffer change's hooks), a variable write
/// or binding (watchers), an unbind (watchers, an `unwind-protect` handler),
/// or a `save-window-excursion` body.
pub(crate) fn op_may_reenter_lisp(op: &Op) -> bool {
    matches!(
        op,
        Op::Call(_)
            | Op::Apply(_)
            | Op::CallBuiltin(..)
            | Op::CallBuiltinSym(..)
            | Op::VarSet(_)
            | Op::VarBind(_)
            | Op::Unbind(_)
            | Op::UnwindProtectPop
            | Op::Set
            | Op::SaveWindowExcursion
    )
}

/// Whether a body's entry needs the stack guard (see the module docs).
pub(crate) fn body_may_reenter_lisp(ops: &[Op]) -> bool {
    ops.iter().any(op_may_reenter_lisp)
}

/// Byte offset of [`Context::jit_stack_limit`], read by the guard.
pub(crate) fn ctx_stack_limit_offset() -> i32 {
    core::mem::offset_of!(Context, jit_stack_limit) as i32
}

/// Byte offset of [`Context::jit_stack_scratch`], where the guard's cold
/// side parks the entry parameters across its call.
pub(crate) fn ctx_stack_scratch_offset() -> i32 {
    core::mem::offset_of!(Context, jit_stack_scratch) as i32
}

/// Emit the guard as the first code of the entry block, whose parameters
/// are `params` (`vmctx, args, out, sidecar`), and leave the builder in the
/// body block that follows it. Returns the body block's parameters, the same
/// four values, for the rest of the entry code to use.
///
/// Below the limit the cold side parks `args, out, sidecar` in the
/// Context's scratch words, calls `neovm_jit_stack_check`, and on its
/// go-ahead (the vmctx back) reloads them into the body, so no value lives
/// across the call and the hot path keeps the entry registers; on null (the
/// error stashed) it returns `STATUS_SIGNAL`. The shim runs no Lisp and no
/// compiled code, so nothing else writes the scratch words meanwhile.
pub(crate) fn emit_entry_stack_guard(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    params: [ClifValue; 4],
) -> [ClifValue; 4] {
    #[cfg(any(test, debug_assertions))]
    STACK_GUARDS_EMITTED.with(|c| c.set(c.get() + 1));
    let [vmctx, _, out_ptr, _] = params;
    let ptr_ty = rt.ptr_ty;
    let trusted = MemFlagsData::trusted();
    let limit = fb
        .ins()
        .load(ptr_ty, trusted, vmctx, ctx_stack_limit_offset());
    let low = fb.ins().icmp(IntCC::UnsignedLessThan, out_ptr, limit);
    let check = fb.create_block();
    let body = fb.create_block();
    for _ in 0..params.len() {
        fb.append_block_param(body, ptr_ty);
    }
    let hot: Vec<BlockArg> = params.iter().map(|&v| BlockArg::Value(v)).collect();
    fb.set_cold_block(check);
    fb.ins().brif(low, check, &[], body, &hot);
    fb.switch_to_block(check);
    fb.seal_block(check);
    let scratch = ctx_stack_scratch_offset();
    for (i, &v) in params[1..].iter().enumerate() {
        fb.ins().store(trusted, v, vmctx, scratch + (i * 8) as i32);
    }
    let stack_check = rt.refs.get(fb.func, Shim::StackCheck);
    let call = fb.ins().call(stack_check, &[vmctx]);
    let resumed_ctx = fb.inst_results(call)[0];
    let resume = fb.create_block();
    let fail = fb.create_block();
    fb.set_cold_block(resume);
    fb.set_cold_block(fail);
    fb.ins().brif(resumed_ctx, resume, &[], fail, &[]);
    fb.switch_to_block(fail);
    fb.seal_block(fail);
    let signal = fb.ins().iconst(types::I64, super::STATUS_SIGNAL);
    fb.ins().return_(&[signal]);
    fb.switch_to_block(resume);
    fb.seal_block(resume);
    let mut cold: Vec<BlockArg> = vec![BlockArg::Value(resumed_ctx)];
    for i in 0..params.len() - 1 {
        let v = fb
            .ins()
            .load(ptr_ty, trusted, resumed_ctx, scratch + (i * 8) as i32);
        cold.push(BlockArg::Value(v));
    }
    fb.ins().jump(body, &cold);
    fb.switch_to_block(body);
    fb.seal_block(body);
    let p = fb.block_params(body);
    [p[0], p[1], p[2], p[3]]
}

#[cfg(any(test, debug_assertions))]
thread_local! {
    /// Entry guards emitted on this thread (engagement evidence for tests).
    pub(crate) static STACK_GUARDS_EMITTED: core::cell::Cell<usize> =
        const { core::cell::Cell::new(0) };
}

/// Entry guards emitted on this thread so far (tests).
#[cfg(test)]
pub(crate) fn stack_guards_emitted_for_test() -> usize {
    STACK_GUARDS_EMITTED.with(core::cell::Cell::get)
}
