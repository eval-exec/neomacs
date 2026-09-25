//! CLIF intrinsics for leaf builtins (design `p1-2-builtin-intrinsics` §2.7):
//! I6 `symbol-value`.
//!
//! Each is a PREFIX of its opcode site: the common shape is answered inline,
//! and every other shape falls through -- never to a deopt -- into the code
//! the site emits anyway (the leaf trampoline, the value shim or the table
//! shim), which answers or signals exactly as before. A miss therefore costs
//! only the tests it failed; a data-dependent shape cannot start a deopt
//! loop. The inline paths read the heap and never write it, allocate, signal
//! or call, so they need no roots, and a site stays what it was for the
//! root-window record ([`finish`] meets the two paths' records).
//!
//! `NEOVM_JIT_INTRINSICS` selects them (default off, read at compile time:
//! off emits nothing, so the site's CLIF is the former exactly). JIT only:
//! the offsets are baked. Every emitted site counts, per intrinsic, in the
//! `[neovm-jit-final-builtin-leaves]` census line (and, in tests, per
//! thread), because a correctness test alone passes just as well when
//! nothing was emitted.

use super::lowering::{
    RtCtx, band_imm_p, binary_value_and_iconst, icmp_imm_p, iconst_bits, ishl_imm_p,
    rootwin_carry_meet, rootwin_carry_snapshot, ushr_imm_p,
};
use super::*;
use cranelift_codegen::ir::{Block, InstBuilder, MemFlagsData, types};

/// One intrinsic, as the census and the knob name it.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intrinsic {
    /// I6: `Op::SymbolValue`.
    SymbolValue,
}

impl Intrinsic {
    pub(crate) const COUNT: usize = Intrinsic::SymbolValue as usize + 1;
    pub(crate) const ALL: [Intrinsic; Intrinsic::COUNT] = [Intrinsic::SymbolValue];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Intrinsic::SymbolValue => "symbol-value",
        }
    }

    /// The intrinsic an opcode site may emit.
    pub(crate) fn of(op: &Op) -> Option<Intrinsic> {
        Some(match op {
            Op::SymbolValue => Intrinsic::SymbolValue,
            _ => return None,
        })
    }

    /// Whether `knob` turns this intrinsic on.
    pub(crate) const fn enabled(self, knob: super::IntrinsicKnob) -> bool {
        match self {
            Intrinsic::SymbolValue => knob.symbol_value,
        }
    }
}

/// Inline sites emitted, per [`Intrinsic`] (compile time; the census).
pub(crate) static INTRINSIC_SITES: [AtomicU64; Intrinsic::COUNT] =
    [const { AtomicU64::new(0) }; Intrinsic::COUNT];

#[cfg(test)]
thread_local! {
    /// Test hook: inline sites emitted on this thread, per intrinsic.
    static THREAD_SITES: [std::cell::Cell<u64>; Intrinsic::COUNT] =
        const { [const { std::cell::Cell::new(0) }; Intrinsic::COUNT] };
}

/// Test hook: `which`'s inline sites emitted on this thread so far.
#[cfg(test)]
pub(crate) fn intrinsic_sites_for_test(which: Intrinsic) -> u64 {
    THREAD_SITES.with(|c| c[which as usize].get())
}

fn note_site(which: Intrinsic) {
    INTRINSIC_SITES[which as usize].fetch_add(1, Ordering::Relaxed);
    #[cfg(test)]
    THREAD_SITES.with(|c| {
        let cell = &c[which as usize];
        cell.set(cell.get() + 1);
    });
}

/// The census entries: every intrinsic with an emitted site.
pub(crate) fn render_intrinsic_stats() -> Vec<String> {
    Intrinsic::ALL
        .iter()
        .filter_map(|&which| {
            let sites = INTRINSIC_SITES[which as usize].load(Ordering::Relaxed);
            (sites > 0).then(|| format!("intrinsic-{}:inline_sites={sites}", which.name()))
        })
        .collect()
}

/// An emitted prefix: its hits define `res` and jump to `merge`; the builder
/// is left in the (sealed) miss block, where the site's call follows and
/// hands its result to [`finish`].
pub(crate) struct InlinePrefix {
    merge: Block,
    res: Variable,
    /// The root-window record on the inline path (it stores nothing).
    carry: Vec<Option<ClifValue>>,
}

/// Emit the intrinsic of `op` over `operands` (tagged; popped by the caller)
/// if `NEOVM_JIT_INTRINSICS` selects it and its shape applies; `None`
/// emits nothing.
pub(crate) fn emit_prefix(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    op: &Op,
    operands: &[ClifValue],
    aot: bool,
) -> Option<InlinePrefix> {
    if aot {
        return None;
    }
    let which = Intrinsic::of(op)?;
    if !which.enabled(super::jit_intrinsic_knob()) {
        return None;
    }
    let carry = rootwin_carry_snapshot();
    let merge = fb.create_block();
    let res = fb.declare_var(types::I64);
    let miss = fb.create_block();
    let emitted = match which {
        Intrinsic::SymbolValue => emit_symbol_value(fb, rt, operands[0], res, merge, miss),
    };
    if !emitted {
        // Nothing was emitted into the current block: the two fresh blocks
        // stay unreachable and empty, which Cranelift drops. (Every
        // emitter decides before its first instruction.)
        return None;
    }
    fb.switch_to_block(miss);
    fb.seal_block(miss);
    note_site(which);
    Some(InlinePrefix { merge, res, carry })
}

/// Close a site: with a prefix, the site's own result (computed in the miss
/// path, current block) joins the inline hits at the merge; without one it
/// is the result.
pub(crate) fn finish(
    fb: &mut FunctionBuilder,
    prefix: Option<InlinePrefix>,
    slow: ClifValue,
) -> ClifValue {
    let Some(prefix) = prefix else {
        return slow;
    };
    fb.def_var(prefix.res, slow);
    fb.ins().jump(prefix.merge, &[]);
    fb.switch_to_block(prefix.merge);
    fb.seal_block(prefix.merge);
    rootwin_carry_meet(&prefix.carry);
    fb.use_var(prefix.res)
}

/// The tagged bits `v` holds at every execution, when the builder can tell:
/// a baked `iconst`, or a raw constant retagged (`(k << 2) | 2`, what the
/// MIR tier's operands become). Heap constants are loaded (relocs), so they
/// never qualify.
fn constant_bits(fb: &FunctionBuilder, v: ClifValue) -> Option<i64> {
    use cranelift_codegen::ir::Opcode;
    if let Some(bits) = iconst_bits(fb, v) {
        return Some(bits);
    }
    let (shifted, tag) = binary_value_and_iconst(fb, v, Opcode::Bor)?;
    let (raw, shift) = binary_value_and_iconst(fb, shifted, Opcode::Ishl)?;
    if tag != FIXNUM_CHECK_VALUE as i64 || shift != i64::from(FIXNUM_SHIFT) {
        return None;
    }
    Some(Value::fixnum(iconst_bits(fb, raw)?).bits() as i64)
}

/// `x & TAG_MASK == tag`.
fn has_tag(fb: &mut FunctionBuilder, x: ClifValue, tag: usize) -> ClifValue {
    let t = band_imm_p(fb, x, TAG_MASK as i64);
    icmp_imm_p(fb, IntCC::Equal, t, tag as i64)
}

fn hit(fb: &mut FunctionBuilder, res: Variable, merge: Block, value: ClifValue) {
    fb.def_var(res, value);
    fb.ins().jump(merge, &[]);
}

// ---------------------------------------------------------------------------
// I6: symbol-value.
// ---------------------------------------------------------------------------

/// `Bsymbol_value` of a bare symbol whose value cell is plain and holds a
/// value: the read `Op::VarRef` makes inline ([`emit_symbol_cell_read`], the
/// same emitter). A constant symbol refuses a nil value only when it is a
/// dedicated buffer-local (whose nil cell means "read the buffer"), as
/// `VarRef` does; a dynamic one must refuse every nil value, since which
/// symbol it is is not known. Everything else -- a symbol with position, a
/// non-symbol, a buffer-local, forwarded or aliased variable, a void or
/// refused value -- goes to `miss`, where the site's call (the `symbol-value`
/// leaf, which reads the cached tiers, or the table shim) answers or signals.
fn emit_symbol_value(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    symbol: ClifValue,
    res: Variable,
    merge: Block,
    miss: Block,
) -> bool {
    let (sym_v, refuse_nil) = match constant_bits(fb, symbol) {
        Some(bits) => {
            let v = Value::from_bits(bits as usize);
            let Some(sym) = v.as_symbol_id() else {
                // A constant non-symbol: always the call's (it signals).
                return false;
            };
            (
                fb.ins().iconst(types::I64, i64::from(sym.0)),
                crate::buffer::buffer::DedicatedBufferLocal::from_sym_id(sym).is_some(),
            )
        }
        None => {
            let is_symbol = has_tag(fb, symbol, TAG_SYMBOL);
            let bare = fb.create_block();
            fb.ins().brif(is_symbol, bare, &[], miss, &[]);
            fb.switch_to_block(bare);
            fb.seal_block(bare);
            (ushr_imm_p(fb, symbol, TAG_BITS as i64), true)
        }
    };
    let value = emit_symbol_cell_read(fb, rt, sym_v, refuse_nil, miss);
    hit(fb, res, merge, value);
    true
}

/// The inline read of a symbol's value cell (GNU `Bvarref`'s: a plain
/// `SYMBOL_VAL` that is bound): the symbol id `sym_v` in range of the
/// obarray's spine, its cell's redirect `Plainval`, its value not unbound --
/// and, with `refuse_nil`, not nil. Branches to `slow` when a test fails and
/// leaves the builder in a fresh sealed block, returning the value. A
/// constant `sym_v` folds the chunk and cell offsets. Shared by
/// `Op::VarRef`'s lowering and I6, instruction for instruction.
pub(crate) fn emit_symbol_cell_read(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    sym_v: ClifValue,
    refuse_nil: bool,
    slow: Block,
) -> ClifValue {
    use super::jit_layout::{
        LISP_SYMBOL_FLAGS_OFFSET, LISP_SYMBOL_SIZE, LISP_SYMBOL_VAL_OFFSET, OBARRAY_CHUNK_BITS,
        OBARRAY_CHUNK_SLOTS, OBARRAY_JIT_LEN_OFFSET, OBARRAY_JIT_SPINE_OFFSET,
        SYMBOL_FLAGS_REDIRECT_MASK,
    };
    let ob = super::jit_layout::CONTEXT_OBARRAY_OFFSET;
    let vmctx = fb.use_var(rt.vmctx_var);
    let len = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        vmctx,
        (ob + OBARRAY_JIT_LEN_OFFSET) as i32,
    );
    let in_range = fb.ins().icmp(IntCC::UnsignedLessThan, sym_v, len);
    let cell_blk = fb.create_block();
    fb.ins().brif(in_range, cell_blk, &[], slow, &[]);
    fb.switch_to_block(cell_blk);
    fb.seal_block(cell_blk);
    let spine = fb.ins().load(
        rt.ptr_ty,
        MemFlagsData::trusted(),
        vmctx,
        (ob + OBARRAY_JIT_SPINE_OFFSET) as i32,
    );
    // Same-session JIT symbol identities are constants. AOT may
    // load a relocated identity, so fold only proven constants.
    let offsets = iconst_bits(fb, sym_v)
        .and_then(|sym| u32::try_from(sym).ok())
        .and_then(|sym| {
            let cell = (sym as usize & (OBARRAY_CHUNK_SLOTS - 1)).checked_mul(LISP_SYMBOL_SIZE)?;
            Some((
                i64::from(sym >> OBARRAY_CHUNK_BITS) * 8,
                i64::try_from(cell).ok()?,
            ))
        });
    let chunk_off = if let Some((chunk, _)) = offsets {
        fb.ins().iconst(types::I64, chunk)
    } else {
        let chunk_index = ushr_imm_p(fb, sym_v, OBARRAY_CHUNK_BITS as i64);
        ishl_imm_p(fb, chunk_index, 3)
    };
    let chunk_slot = fb.ins().iadd(spine, chunk_off);
    let chunk = fb
        .ins()
        .load(rt.ptr_ty, MemFlagsData::trusted(), chunk_slot, 0);
    let cell_off = if let Some((_, cell)) = offsets {
        fb.ins().iconst(types::I64, cell)
    } else {
        let slot_index = band_imm_p(fb, sym_v, (OBARRAY_CHUNK_SLOTS - 1) as i64);
        fb.ins().imul_imm_u(slot_index, LISP_SYMBOL_SIZE as i64)
    };
    let cell = fb.ins().iadd(chunk, cell_off);
    let flags = fb.ins().uload8(
        types::I64,
        MemFlagsData::trusted(),
        cell,
        LISP_SYMBOL_FLAGS_OFFSET as i32,
    );
    let redirect = band_imm_p(fb, flags, SYMBOL_FLAGS_REDIRECT_MASK as i64);
    let plain = icmp_imm_p(fb, IntCC::Equal, redirect, 0);
    let val_blk = fb.create_block();
    fb.ins().brif(plain, val_blk, &[], slow, &[]);
    fb.switch_to_block(val_blk);
    fb.seal_block(val_blk);
    let val = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        cell,
        LISP_SYMBOL_VAL_OFFSET as i32,
    );
    let unbound = icmp_imm_p(fb, IntCC::Equal, val, Value::UNBOUND.bits() as i64);
    let refused = if refuse_nil {
        let nil = icmp_imm_p(fb, IntCC::Equal, val, Value::NIL.bits() as i64);
        fb.ins().bor(unbound, nil)
    } else {
        unbound
    };
    let fast_blk = fb.create_block();
    fb.ins().brif(refused, slow, &[], fast_blk, &[]);
    fb.switch_to_block(fast_blk);
    fb.seal_block(fast_blk);
    val
}

#[cfg(test)]
#[path = "../tests/intrinsics.rs"]
mod intrinsics_tests;
