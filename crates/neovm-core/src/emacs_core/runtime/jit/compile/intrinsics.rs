//! CLIF intrinsics for leaf builtins (design `p1-2-builtin-intrinsics`
//! §2.7): I3 `length`, I4 `nth`/`nthcdr`/`elt` at a constant small index,
//! I5 `memq`/`assq`/`member` over a short list, I6 `symbol-value`.
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
    RtCtx, band_imm_p, binary_value_and_iconst, bor_imm_p, iadd_imm_p, icmp_imm_p, iconst_bits,
    ishl_imm_p, rootwin_carry_meet, rootwin_carry_snapshot, ushr_imm_p,
};
use super::*;
use cranelift_codegen::ir::{Block, InstBuilder, MemFlagsData, types};

/// One intrinsic, as the census and the knob name it.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intrinsic {
    /// I3: `Op::Length`.
    Length,
    /// I4: `Op::Nth` at a constant index.
    Nth,
    /// I4: `Op::Nthcdr` at a constant count.
    Nthcdr,
    /// I4: `Op::Elt` of a list at a constant index.
    Elt,
    /// I5: `Op::Memq`.
    Memq,
    /// I5: `Op::Assq`.
    Assq,
    /// I5: `Op::Member` with a fixnum or bare-symbol key.
    Member,
    /// I6: `Op::SymbolValue`.
    SymbolValue,
}

impl Intrinsic {
    pub(crate) const COUNT: usize = Intrinsic::SymbolValue as usize + 1;
    pub(crate) const ALL: [Intrinsic; Intrinsic::COUNT] = [
        Intrinsic::Length,
        Intrinsic::Nth,
        Intrinsic::Nthcdr,
        Intrinsic::Elt,
        Intrinsic::Memq,
        Intrinsic::Assq,
        Intrinsic::Member,
        Intrinsic::SymbolValue,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Intrinsic::Length => "length",
            Intrinsic::Nth => "nth",
            Intrinsic::Nthcdr => "nthcdr",
            Intrinsic::Elt => "elt",
            Intrinsic::Memq => "memq",
            Intrinsic::Assq => "assq",
            Intrinsic::Member => "member",
            Intrinsic::SymbolValue => "symbol-value",
        }
    }

    /// The intrinsic an opcode site may emit.
    pub(crate) fn of(op: &Op) -> Option<Intrinsic> {
        Some(match op {
            Op::Length => Intrinsic::Length,
            Op::Nth => Intrinsic::Nth,
            Op::Nthcdr => Intrinsic::Nthcdr,
            Op::Elt => Intrinsic::Elt,
            Op::Memq => Intrinsic::Memq,
            Op::Assq => Intrinsic::Assq,
            Op::Member => Intrinsic::Member,
            Op::SymbolValue => Intrinsic::SymbolValue,
            _ => return None,
        })
    }

    /// Whether `knob` turns this intrinsic on: the knob's parts group them
    /// as the design numbers them (I3 `length`, I4 `nth`, I5 `memq`, I6
    /// `symbol-value`).
    pub(crate) const fn enabled(self, knob: super::IntrinsicKnob) -> bool {
        match self {
            Intrinsic::Length => knob.length,
            Intrinsic::Nth | Intrinsic::Nthcdr | Intrinsic::Elt => knob.nth,
            Intrinsic::Memq | Intrinsic::Assq | Intrinsic::Member => knob.memq,
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

/// How many conses I3's walk counts before handing a longer list to the
/// site's call (which walks it from the start, with its cycle check). A
/// circular list never ends the walk, so it always reaches the call.
pub(crate) const LENGTH_INLINE_STEPS: i64 = 64;
/// The largest constant index I4 unrolls.
pub(crate) const NTH_INLINE_MAX: i64 = 4;
/// How many conses I5's walk visits before handing the list to the site's
/// call (the value shim walks up to 64 more, then the builtin), unless
/// `NEOVM_JIT_MEMQ_STEPS` says otherwise ([`memq_inline_steps`]). A list
/// longer than the bound is walked twice up to it: the bound trades the
/// matches it answers inline against that waste.
pub(crate) const MEMQ_INLINE_STEPS: i64 = 16;

/// I5's walk bound: `NEOVM_JIT_MEMQ_STEPS=<1..=64>` (a measurement knob,
/// read once), else [`MEMQ_INLINE_STEPS`].
pub(crate) fn memq_inline_steps() -> i64 {
    use std::sync::OnceLock;
    static STEPS: OnceLock<i64> = OnceLock::new();
    *STEPS.get_or_init(|| {
        std::env::var("NEOVM_JIT_MEMQ_STEPS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|n| (1..=64).contains(n))
            .unwrap_or(MEMQ_INLINE_STEPS)
    })
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
        Intrinsic::Length => emit_length(fb, operands[0], res, merge, miss),
        Intrinsic::Nth | Intrinsic::Nthcdr | Intrinsic::Elt => {
            emit_nth(fb, which, operands[0], operands[1], res, merge, miss)
        }
        Intrinsic::Memq | Intrinsic::Assq | Intrinsic::Member => {
            emit_memq(fb, rt, which, operands[0], operands[1], res, merge, miss)
        }
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
    Some(iconst_bits(fb, raw)?.wrapping_shl(FIXNUM_SHIFT) | FIXNUM_CHECK_VALUE as i64)
}

fn trusted() -> MemFlagsData {
    MemFlagsData::trusted()
}

/// `x & TAG_MASK == tag`.
fn has_tag(fb: &mut FunctionBuilder, x: ClifValue, tag: usize) -> ClifValue {
    let t = band_imm_p(fb, x, TAG_MASK as i64);
    icmp_imm_p(fb, IntCC::Equal, t, tag as i64)
}

/// A cons's car or cdr: one load, the tag folded into the offset.
fn cons_field(fb: &mut FunctionBuilder, cons: ClifValue, cdr: bool) -> ClifValue {
    let off = if cdr {
        core::mem::offset_of!(ConsCell, cdr_or_next)
    } else {
        core::mem::offset_of!(ConsCell, car)
    } as i64
        - TAG_CONS as i64;
    fb.ins().load(types::I64, trusted(), cons, off as i32)
}

/// `(n << 2) | 2`: a raw count as a fixnum.
fn fixnum_of(fb: &mut FunctionBuilder, n: ClifValue) -> ClifValue {
    let shifted = ishl_imm_p(fb, n, FIXNUM_SHIFT as i64);
    bor_imm_p(fb, shifted, FIXNUM_CHECK_VALUE as i64)
}

fn hit(fb: &mut FunctionBuilder, res: Variable, merge: Block, value: ClifValue) {
    fb.def_var(res, value);
    fb.ins().jump(merge, &[]);
}

// ---------------------------------------------------------------------------
// I3: length.
// ---------------------------------------------------------------------------

/// `Blength` (`builtin_length_value`) on the shapes it answers without a
/// cycle check: nil (0), a proper list of at most [`LENGTH_INLINE_STEPS`]
/// conses (a bounded walk), a string (its character count) and a plain
/// vector or record (its slot count: slot 0 is not a bool-vector's or a
/// char-table's tag, the classification `emit_plain_slot_address` makes).
/// An improper or longer list, a bool-vector, a char-table, a closure and
/// every non-sequence go to `miss`, where the call answers or signals.
fn emit_length(
    fb: &mut FunctionBuilder,
    x: ClifValue,
    res: Variable,
    merge: Block,
    miss: Block,
) -> bool {
    let nil = Value::NIL.bits() as i64;
    let walk = fb.create_block();
    fb.append_block_param(walk, types::I64); // a cons
    fb.append_block_param(walk, types::I64); // conses counted, it included
    let step = fb.create_block();
    let done = fb.create_block();
    fb.append_block_param(done, types::I64); // count
    let not_cons = fb.create_block();
    let not_nil = fb.create_block();
    let not_string = fb.create_block();
    let string = fb.create_block();

    // Dispatch on the tag: a cons first (the common case).
    let is_cons = has_tag(fb, x, TAG_CONS);
    let one = fb.ins().iconst(types::I64, 1);
    fb.ins().brif(
        is_cons,
        walk,
        &[BlockArg::Value(x), BlockArg::Value(one)],
        not_cons,
        &[],
    );

    // The walk: count each cdr that is a cons; stop at the bound.
    fb.switch_to_block(walk);
    let t = fb.block_params(walk)[0];
    let n = fb.block_params(walk)[1];
    let d = cons_field(fb, t, true);
    let d_cons = has_tag(fb, d, TAG_CONS);
    fb.ins()
        .brif(d_cons, step, &[], done, &[BlockArg::Value(n)]);
    fb.switch_to_block(step);
    fb.seal_block(step);
    let n2 = iadd_imm_p(fb, n, 1);
    let within = icmp_imm_p(fb, IntCC::SignedLessThanOrEqual, n2, LENGTH_INLINE_STEPS);
    fb.ins().brif(
        within,
        walk,
        &[BlockArg::Value(d), BlockArg::Value(n2)],
        miss,
        &[],
    );
    fb.seal_block(walk);
    // The last cdr: nil ends a proper list; anything else is improper.
    fb.switch_to_block(done);
    fb.seal_block(done);
    let count = fb.block_params(done)[0];
    let proper = icmp_imm_p(fb, IntCC::Equal, d, nil);
    let answer = fb.create_block();
    fb.ins().brif(proper, answer, &[], miss, &[]);
    fb.switch_to_block(answer);
    fb.seal_block(answer);
    let v = fixnum_of(fb, count);
    hit(fb, res, merge, v);

    // nil.
    fb.switch_to_block(not_cons);
    fb.seal_block(not_cons);
    let is_nil = icmp_imm_p(fb, IntCC::Equal, x, nil);
    let zero = fb.create_block();
    fb.ins().brif(is_nil, zero, &[], not_nil, &[]);
    fb.switch_to_block(zero);
    fb.seal_block(zero);
    let z = fb.ins().iconst(types::I64, Value::fixnum(0).bits() as i64);
    hit(fb, res, merge, z);

    // A string: its character count.
    fb.switch_to_block(not_nil);
    fb.seal_block(not_nil);
    let is_string = has_tag(fb, x, TAG_STRING);
    fb.ins().brif(is_string, string, &[], not_string, &[]);
    fb.switch_to_block(string);
    fb.seal_block(string);
    let size_off = core::mem::offset_of!(crate::tagged::header::StringObj, data)
        + crate::heap_types::LispString::JIT_SIZE_OFFSET;
    let size = fb.ins().load(
        types::I64,
        trusted(),
        x,
        (size_off as i64 - TAG_STRING as i64) as i32,
    );
    let v = fixnum_of(fb, size);
    hit(fb, res, merge, v);

    // A plain vector or record.
    fb.switch_to_block(not_string);
    fb.seal_block(not_string);
    match plain_vector_len(fb, x, miss) {
        Some(len) => {
            let v = fixnum_of(fb, len);
            hit(fb, res, merge, v);
        }
        None => {
            fb.ins().jump(miss, &[]);
        }
    }
    true
}

/// The slot count of a plain vector or record, emitted inline: branches to
/// `miss` for any other value (including a bool-vector or a char-table,
/// which are `Vector` objects tagged in slot 0) and leaves the builder in a
/// fresh sealed block. `None`, emitting nothing, when this build's vector
/// storage does not keep its length at an offset every storage kind shares.
fn plain_vector_len(fb: &mut FunctionBuilder, x: ClifValue, miss: Block) -> Option<ClifValue> {
    use crate::tagged::header::{VecLikeHeader, VecLikeType, VectorObj};
    let (ptr_off, len_off) = super::jit_layout::heap::value_vec_slice_offsets()?;
    let data_off = core::mem::offset_of!(VectorObj, data);
    let type_off = core::mem::offset_of!(VecLikeHeader, type_tag);
    let (char_table_tag, bool_vector_tag, char_table_min_len) =
        crate::emacs_core::chartable::inline_vector_tag_shape();
    let veclike = has_tag(fb, x, crate::tagged::value::TAG_VECLIKE);
    let typed = fb.create_block();
    fb.ins().brif(veclike, typed, &[], miss, &[]);
    fb.switch_to_block(typed);
    fb.seal_block(typed);
    let object = band_imm_p(fb, x, !(TAG_MASK as i64));
    let type_tag = fb.ins().load(types::I8, trusted(), object, type_off as i32);
    let is_vector = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, type_tag, VecLikeType::Vector as u8 as i64);
    let is_record = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, type_tag, VecLikeType::Record as u8 as i64);
    let either = fb.ins().bor(is_vector, is_record);
    let sized = fb.create_block();
    fb.ins().brif(either, sized, &[], miss, &[]);
    fb.switch_to_block(sized);
    fb.seal_block(sized);
    let len = fb
        .ins()
        .load(types::I64, trusted(), object, (data_off + len_off) as i32);
    // Slot 0 decides only for two or more slots (`classify_vector_slots`).
    let two_or_more = icmp_imm_p(fb, IntCC::UnsignedGreaterThanOrEqual, len, 2);
    let classify = fb.create_block();
    let plain = fb.create_block();
    fb.ins().brif(two_or_more, classify, &[], plain, &[]);
    fb.switch_to_block(classify);
    fb.seal_block(classify);
    let slots = fb
        .ins()
        .load(types::I64, trusted(), object, (data_off + ptr_off) as i32);
    let first = fb.ins().load(types::I64, trusted(), slots, 0);
    let is_bool_vector = icmp_imm_p(fb, IntCC::Equal, first, bool_vector_tag as i64);
    let is_char_table_tag = icmp_imm_p(fb, IntCC::Equal, first, char_table_tag as i64);
    let long_enough = icmp_imm_p(
        fb,
        IntCC::UnsignedGreaterThanOrEqual,
        len,
        char_table_min_len as i64,
    );
    let char_table = fb.ins().band(is_char_table_tag, long_enough);
    let char_table = fb.ins().band(char_table, is_vector);
    let tagged = fb.ins().bor(is_bool_vector, char_table);
    fb.ins().brif(tagged, miss, &[], plain, &[]);
    fb.switch_to_block(plain);
    fb.seal_block(plain);
    Some(len)
}

// ---------------------------------------------------------------------------
// I4: nth, nthcdr, elt at a constant index.
// ---------------------------------------------------------------------------

/// `Bnth`, `Bnthcdr` and `Belt` of a list at a constant index `k` in
/// `0..=NTH_INLINE_MAX`, as an unrolled cdr chain. Operands are `[n, list]`
/// for `nth`/`nthcdr` and `[sequence, n]` for `elt`. A dynamic or larger
/// index emits nothing.
///
/// - `nth` (`bytecode_nth_values`): walk while the count is positive and the
///   tail a cons; then the tail's car if a cons, nil if nil; any other tail
///   goes to `miss`, where the call signals (`listp TAIL`).
/// - `nthcdr` (`builtin_nthcdr_values`): each step takes the cdr of a cons,
///   answers nil at a nil tail, and leaves any other tail to `miss` (the
///   call signals); after `k` steps the tail itself, whatever it is.
/// - `elt` (`bytecode_elt_values`, GNU `Belt`): a cons sequence walks as
///   `nth`; nil is nil at any index; anything else (a vector, a string, a
///   non-sequence) goes to `miss`.
fn emit_nth(
    fb: &mut FunctionBuilder,
    which: Intrinsic,
    a: ClifValue,
    b: ClifValue,
    res: Variable,
    merge: Block,
    miss: Block,
) -> bool {
    let (index, list) = match which {
        Intrinsic::Elt => (b, a),
        _ => (a, b),
    };
    let Some(bits) = constant_bits(fb, index) else {
        return false;
    };
    let index = Value::from_bits(bits as usize);
    let Some(k) = index
        .as_fixnum()
        .filter(|k| (0..=NTH_INLINE_MAX).contains(k))
    else {
        return false;
    };
    let nil = Value::NIL.bits() as i64;
    let nil_v = fb.ins().iconst(types::I64, nil);
    if which == Intrinsic::Elt {
        // Only a cons takes `Belt`'s list walk; nil is nil at any index.
        let is_cons = has_tag(fb, list, TAG_CONS);
        let walk = fb.create_block();
        let not_cons = fb.create_block();
        fb.ins().brif(is_cons, walk, &[], not_cons, &[]);
        fb.switch_to_block(not_cons);
        fb.seal_block(not_cons);
        let is_nil = icmp_imm_p(fb, IntCC::Equal, list, nil);
        let nil_blk = fb.create_block();
        fb.ins().brif(is_nil, nil_blk, &[], miss, &[]);
        fb.switch_to_block(nil_blk);
        fb.seal_block(nil_blk);
        hit(fb, res, merge, nil_v);
        fb.switch_to_block(walk);
        fb.seal_block(walk);
    }
    if which == Intrinsic::Nthcdr {
        let mut t = list;
        for _ in 0..k {
            let is_cons = has_tag(fb, t, TAG_CONS);
            let next = fb.create_block();
            let not_cons = fb.create_block();
            fb.ins().brif(is_cons, next, &[], not_cons, &[]);
            fb.switch_to_block(not_cons);
            fb.seal_block(not_cons);
            let is_nil = icmp_imm_p(fb, IntCC::Equal, t, nil);
            let nil_blk = fb.create_block();
            fb.ins().brif(is_nil, nil_blk, &[], miss, &[]);
            fb.switch_to_block(nil_blk);
            fb.seal_block(nil_blk);
            hit(fb, res, merge, nil_v);
            fb.switch_to_block(next);
            fb.seal_block(next);
            t = cons_field(fb, t, true);
        }
        hit(fb, res, merge, t);
        return true;
    }
    // `nth` and a cons `elt`: stop at the first non-cons tail, then CAR.
    let last = fb.create_block();
    fb.append_block_param(last, types::I64);
    let mut t = list;
    for _ in 0..k {
        let is_cons = has_tag(fb, t, TAG_CONS);
        let next = fb.create_block();
        fb.ins()
            .brif(is_cons, next, &[], last, &[BlockArg::Value(t)]);
        fb.switch_to_block(next);
        fb.seal_block(next);
        t = cons_field(fb, t, true);
    }
    fb.ins().jump(last, &[BlockArg::Value(t)]);
    fb.switch_to_block(last);
    fb.seal_block(last);
    let tail = fb.block_params(last)[0];
    let is_cons = has_tag(fb, tail, TAG_CONS);
    let car_blk = fb.create_block();
    let not_cons = fb.create_block();
    fb.ins().brif(is_cons, car_blk, &[], not_cons, &[]);
    fb.switch_to_block(car_blk);
    fb.seal_block(car_blk);
    let car = cons_field(fb, tail, false);
    hit(fb, res, merge, car);
    fb.switch_to_block(not_cons);
    fb.seal_block(not_cons);
    let is_nil = icmp_imm_p(fb, IntCC::Equal, tail, nil);
    let nil_blk = fb.create_block();
    fb.ins().brif(is_nil, nil_blk, &[], miss, &[]);
    fb.switch_to_block(nil_blk);
    fb.seal_block(nil_blk);
    hit(fb, res, merge, nil_v);
    true
}

// ---------------------------------------------------------------------------
// I5: memq, assq, member.
// ---------------------------------------------------------------------------

/// `Bmemq`, `Bassq` and `Bmember` over the first [`memq_inline_steps`]
/// conses of a list, by bit identity -- exactly `memq_fast`/`assq_fast`'s
/// rule: only while `symbols-with-pos-enabled` is off (`eq` then looks
/// through positions), a match found before the builtin's cycle check
/// could fire, nil at a nil tail. `member` takes the same walk only for a
/// fixnum or bare-symbol key, for which `equal` decides as `eq` does (a
/// constant key is tested at compile time). An improper tail, a longer
/// list, any other key and the flag on go to `miss`.
#[allow(clippy::too_many_arguments)]
fn emit_memq(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    which: Intrinsic,
    key: ClifValue,
    list: ClifValue,
    res: Variable,
    merge: Block,
    miss: Block,
) -> bool {
    let key_const = constant_bits(fb, key).map(|bits| Value::from_bits(bits as usize));
    if which == Intrinsic::Member && key_const.is_some_and(|k| !(k.is_symbol() || k.is_fixnum())) {
        return false;
    }
    let nil = Value::NIL.bits() as i64;
    // The flag first: it is a context byte, off in all but byte-compiler runs.
    let vmctx = fb.use_var(rt.vmctx_var);
    let swp = fb.ins().load(
        types::I8,
        trusted(),
        vmctx,
        core::mem::offset_of!(Context, symbols_with_pos_enabled) as i32,
    );
    let flag_off = fb.create_block();
    fb.ins().brif(swp, miss, &[], flag_off, &[]);
    fb.switch_to_block(flag_off);
    fb.seal_block(flag_off);
    if which == Intrinsic::Member && key_const.is_none() {
        // A fixnum (`...10`) or a bare symbol (tag 000) key.
        let fix_tag = band_imm_p(fb, key, FIXNUM_CHECK_MASK as i64);
        let is_fixnum = icmp_imm_p(fb, IntCC::Equal, fix_tag, FIXNUM_CHECK_VALUE as i64);
        let is_symbol = has_tag(fb, key, TAG_SYMBOL);
        let eqable = fb.ins().bor(is_fixnum, is_symbol);
        let keyed = fb.create_block();
        fb.ins().brif(eqable, keyed, &[], miss, &[]);
        fb.switch_to_block(keyed);
        fb.seal_block(keyed);
    }
    let head = fb.create_block();
    fb.append_block_param(head, types::I64); // the tail
    fb.append_block_param(head, types::I64); // conses visited
    let body = fb.create_block();
    let next = fb.create_block();
    let end = fb.create_block();
    let zero = fb.ins().iconst(types::I64, 0);
    fb.ins()
        .jump(head, &[BlockArg::Value(list), BlockArg::Value(zero)]);

    fb.switch_to_block(head);
    let t = fb.block_params(head)[0];
    let n = fb.block_params(head)[1];
    let is_cons = has_tag(fb, t, TAG_CONS);
    fb.ins().brif(is_cons, body, &[], end, &[]);

    fb.switch_to_block(body);
    fb.seal_block(body);
    let element = cons_field(fb, t, false);
    match which {
        Intrinsic::Assq => {
            // The first element that is a cons whose car is KEY.
            let entry = fb.create_block();
            let is_entry = has_tag(fb, element, TAG_CONS);
            fb.ins().brif(is_entry, entry, &[], next, &[]);
            fb.switch_to_block(entry);
            fb.seal_block(entry);
            let entry_key = cons_field(fb, element, false);
            let found = fb.ins().icmp(IntCC::Equal, entry_key, key);
            let hit_blk = fb.create_block();
            fb.ins().brif(found, hit_blk, &[], next, &[]);
            fb.switch_to_block(hit_blk);
            fb.seal_block(hit_blk);
            hit(fb, res, merge, element);
        }
        _ => {
            // The first tail whose car is KEY.
            let found = fb.ins().icmp(IntCC::Equal, element, key);
            let hit_blk = fb.create_block();
            fb.ins().brif(found, hit_blk, &[], next, &[]);
            fb.switch_to_block(hit_blk);
            fb.seal_block(hit_blk);
            hit(fb, res, merge, t);
        }
    }

    fb.switch_to_block(next);
    fb.seal_block(next);
    let d = cons_field(fb, t, true);
    let n2 = iadd_imm_p(fb, n, 1);
    let within = icmp_imm_p(fb, IntCC::SignedLessThan, n2, memq_inline_steps());
    fb.ins().brif(
        within,
        head,
        &[BlockArg::Value(d), BlockArg::Value(n2)],
        miss,
        &[],
    );
    fb.seal_block(head);

    // The end of the list: nil answers nil; an improper tail is the call's.
    fb.switch_to_block(end);
    fb.seal_block(end);
    let is_nil = icmp_imm_p(fb, IntCC::Equal, t, nil);
    let nil_blk = fb.create_block();
    fb.ins().brif(is_nil, nil_blk, &[], miss, &[]);
    fb.switch_to_block(nil_blk);
    fb.seal_block(nil_blk);
    hit(fb, res, merge, t);
    true
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
#[path = "../tests/intrinsics_test.rs"]
mod intrinsics_tests;
