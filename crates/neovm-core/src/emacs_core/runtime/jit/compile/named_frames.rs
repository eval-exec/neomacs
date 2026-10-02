//! Per-call named inline entry checks and frame emission.
//!
//! Threading: emission reads the executing mutator's Context and obarray.
//! There is no baked Lisp cell address, cross-mutator cache or publication.
//! The existing obarray mutation protocol owns synchronization of its cells.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, InstBuilder, MemFlagsData, Value as ClifValue, types};
use cranelift_frontend::FunctionBuilder;

use super::jit_layout::{
    CONTEXT_OBARRAY_OFFSET, LISP_SYMBOL_FUNCTION_OFFSET, LISP_SYMBOL_SIZE, OBARRAY_CHUNK_BITS,
    OBARRAY_CHUNK_SLOTS, OBARRAY_JIT_LEN_OFFSET, OBARRAY_JIT_SPINE_OFFSET,
};
use super::lowering::{RtCtx, emit_guard};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;

/// GNU Bcall reads the function cell at this call boundary. This fallback
/// follows the executing mutator's obarray spine and never couples unrelated
/// definitions to the guard. A changed running callee remains rooted by its
/// leaf; only a subsequent entry checks the changed cell.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_identity_guard(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    symbol: SymId,
    designator: ClifValue,
    expected_bits: u64,
    deopt: Block,
) {
    let flags = MemFlagsData::trusted();
    let called_symbol = fb
        .ins()
        .iconst(types::I64, Value::from_sym_id(symbol).bits() as i64);
    let same = fb.ins().icmp(IntCC::Equal, designator, called_symbol);
    emit_guard(fb, deopt, same);
    let ctx = fb.use_var(rt.vmctx_var);
    let len = fb.ins().load(
        types::I64,
        flags,
        ctx,
        (CONTEXT_OBARRAY_OFFSET + OBARRAY_JIT_LEN_OFFSET) as i32,
    );
    let exists = fb
        .ins()
        .icmp_imm_u(IntCC::UnsignedGreaterThan, len, i64::from(symbol.0));
    emit_guard(fb, deopt, exists);
    let spine = fb.ins().load(
        types::I64,
        flags,
        ctx,
        (CONTEXT_OBARRAY_OFFSET + OBARRAY_JIT_SPINE_OFFSET) as i32,
    );
    let chunk = fb.ins().load(
        types::I64,
        flags,
        spine,
        ((symbol.0 as usize >> OBARRAY_CHUNK_BITS) * 8) as i32,
    );
    let function = fb.ins().load(
        types::I64,
        flags,
        chunk,
        (((symbol.0 as usize & (OBARRAY_CHUNK_SLOTS - 1)) * LISP_SYMBOL_SIZE)
            + LISP_SYMBOL_FUNCTION_OFFSET) as i32,
    );
    let expected = fb.ins().iconst(types::I64, expected_bits as i64);
    let same = fb.ins().icmp(IntCC::Equal, function, expected);
    emit_guard(fb, deopt, same);
}
