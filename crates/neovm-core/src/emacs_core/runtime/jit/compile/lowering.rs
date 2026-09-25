//! Bytecode-to-CLIF lowering: fixnum guard/retag helpers, the fixnum/predicate/car-cdr op lowerings, MIR lowering (lower_mir_with_plan) and its runtime-context and deopt emission, the ISA config, and the per-op baseline lowering (lower_simple_op).
//!
//! Moved out of `compile.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// Emit a speculation guard.
///
/// If `cond` (an `i8` boolean from `icmp`) is false, branch to the shared deopt
/// block — created lazily on first use; otherwise fall through into a fresh,
/// sealed continuation block. On return, the builder is positioned in the
/// continuation so lowering continues on the success path.
pub(crate) fn emit_guard(fb: &mut FunctionBuilder, deopt: Block, cond: ClifValue) {
    GUARD_COUNT.with(|c| c.set(c.get() + 1));
    // J0 verification harness: force every guard to fail so the deopt path is
    // always taken (see `jit_force_deopt`). A constant-false condition makes
    // `brif` unconditionally branch to `deopt`.
    let cond = if jit_force_deopt() {
        let ty = fb.func.dfg.value_type(cond);
        fb.ins().iconst(ty, 0)
    } else {
        cond
    };
    let cont = fb.create_block();
    fb.ins().brif(cond, cont, &[], deopt, &[]);
    fb.switch_to_block(cont);
    // `cont`'s only predecessor is the guard branch just emitted.
    fb.seal_block(cont);
}

thread_local! {
    /// Guards emitted by the function being lowered (every `emit_guard`,
    /// tag and range alike): the MIR tier's guard census, printed in its
    /// `NEOVM_JIT_DUMP_CLIF` header and asserted by its tests.
    static GUARD_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Untags (`mir_as_raw`'s `sshr`) and retags (`retag_fixnum`) emitted by
    /// the function being lowered — the other two halves of what the MIR
    /// tier's unboxing claims, pinned by the same tests as the guards.
    static UNTAG_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static RETAG_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Start the guard/untag/retag census for a new function.
pub(crate) fn guards_emitted_reset() {
    GUARD_COUNT.with(|c| c.set(0));
    UNTAG_COUNT.with(|c| c.set(0));
    RETAG_COUNT.with(|c| c.set(0));
    flonum_census_reset();
}

/// Guards emitted since the last [`guards_emitted_reset`].
pub(crate) fn guards_emitted() -> u32 {
    GUARD_COUNT.with(|c| c.get())
}

/// MIR untags emitted since the last [`guards_emitted_reset`].
pub(crate) fn untags_emitted() -> u32 {
    UNTAG_COUNT.with(|c| c.get())
}

/// Retags emitted since the last [`guards_emitted_reset`].
pub(crate) fn retags_emitted() -> u32 {
    RETAG_COUNT.with(|c| c.get())
}

/// Return the bits of `v` when it is a Cranelift integer constant.
///
/// Since Cranelift 0.134, immediate convenience builders materialize an
/// `iconst` operand instead of preserving a distinct immediate instruction
/// shape. Keep that representation knowledge at this single inspection seam.
pub(crate) fn iconst_bits(fb: &FunctionBuilder, v: ClifValue) -> Option<i64> {
    use cranelift_codegen::ir::{InstructionData, Opcode, ValueDef};
    let ValueDef::Result(inst, _) = fb.func.dfg.value_def(v) else {
        return None;
    };
    match fb.func.dfg.insts[inst] {
        InstructionData::UnaryImm {
            opcode: Opcode::Iconst,
            imm,
        } => Some(imm.bits()),
        _ => None,
    }
}

/// Return `(value, immediate)` for a binary instruction whose right operand is
/// an `iconst`. This is the 0.134 IR shape produced by helpers such as
/// `bor_imm_u` and `ishl_imm_u`.
pub(crate) fn binary_value_and_iconst(
    fb: &FunctionBuilder,
    v: ClifValue,
    expected_opcode: cranelift_codegen::ir::Opcode,
) -> Option<(ClifValue, i64)> {
    use cranelift_codegen::ir::{InstructionData, ValueDef};
    let ValueDef::Result(inst, _) = fb.func.dfg.value_def(v) else {
        return None;
    };
    let InstructionData::Binary { opcode, args } = fb.func.dfg.insts[inst] else {
        return None;
    };
    if opcode != expected_opcode {
        return None;
    }
    Some((args[0], iconst_bits(fb, args[1])?))
}

/// True if `v` is a compile-time fixnum constant — an `iconst` whose immediate
/// already carries the fixnum tag bits. A runtime fixnum guard on such a value
/// is provably unnecessary (it is the same fixnum on every path), so
/// [`guard_fixnum`] can skip it. This is the safe, dataflow-free subset of
/// redundant-guard elimination: constant operands of arithmetic/comparison are
/// pervasive (`(+ i 1)`, `(< i n)`, `(1+ i)`), and a fixnum `iconst` dominates
/// every use, so eliding its guard cannot change any result or deopt.
pub(crate) fn is_fixnum_const(fb: &FunctionBuilder, v: ClifValue) -> bool {
    iconst_bits(fb, v)
        .is_some_and(|bits| (bits & FIXNUM_CHECK_MASK as i64) == FIXNUM_CHECK_VALUE as i64)
}

/// True if `v` is a compile-time constant (`iconst`) whose bits are a NON-HEAP
/// immediate — a fixnum, or a symbol (`nil`/`t`/keywords/interned names are all
/// symbol-tagged). Such a value provably never needs operand-stack GC rooting, so
/// a residual push can be skipped ENTIRELY at compile time — the baseline (no-MIR)
/// analogue of [`LispType::never_needs_gc_root`]. The predicate is exactly the one
/// [`emit_conditional_gc_push`] inlines at run time, so it can never mis-skip a
/// heap value; and heap constants are `iconst`-immune in the baseline anyway (R1a
/// routes them through the reloc load, never a baked pointer). Because the
/// baseline runs Cranelift at `opt_level="none"` (no constant folding), skipping
/// here removes a dead tag-test the optimizer would otherwise leave in.
pub(crate) fn is_nonheap_const(fb: &FunctionBuilder, v: ClifValue) -> bool {
    if let Some(bits) = iconst_bits(fb, v) {
        let bits = bits as usize;
        return (bits & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE || (bits & TAG_MASK) == TAG_SYMBOL;
    }
    false
}

/// True if `v` PROVABLY holds the tagged bits of symbol `sym` at this point:
/// an `iconst` of exactly those bits (the JIT bake of a symbol constant, see
/// the `Op::Constant` lowering) or a load of the symbol's slot from the
/// per-leaf reloc vector (the AOT shape — same emission, reloc'd). This is
/// the SSA soundness gate for `Op::Call` speculation: `find_spec_sites`'
/// abstract stack tracking SELECTS the sites, but the spec shim call the
/// lowering emits IGNORES the runtime callee slot in favor of the baked
/// symbol — so the lowering only takes the spec path when this independent
/// proof holds, and any divergence in the tracking degrades to the generic
/// call instead of a wrong-callee mis-speculation. Copies made by
/// `Dup`/`StackRef`/`StackSet` reuse the same SSA value, so straight-line
/// propagation keeps the proof; values that crossed a block boundary are
/// variables (not an iconst/load result) and correctly fail it.
pub(crate) fn callee_is_symbol_const(
    fb: &FunctionBuilder,
    v: ClifValue,
    sym: u32,
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
) -> bool {
    use cranelift_codegen::ir::immediates::Offset32;
    use cranelift_codegen::ir::{InstructionData, Opcode, ValueDef};
    let expected_bits = Value::from_sym_id(crate::emacs_core::intern::SymId(sym)).bits();
    let ValueDef::Result(inst, _) = fb.func.dfg.value_def(v) else {
        return false;
    };
    match fb.func.dfg.insts[inst] {
        InstructionData::UnaryImm {
            opcode: Opcode::Iconst,
            imm,
        } => imm.bits() == expected_bits as i64,
        InstructionData::Load {
            opcode: Opcode::Load,
            arg,
            offset,
            ..
        } => {
            let Some(base) = reloc_base else {
                return false;
            };
            let Some(&idx) = reloc_index.get(&expected_bits) else {
                return false;
            };
            arg == base && offset == Offset32::new((idx * 8) as i32)
        }
        _ => false,
    }
}

/// True if `v` is provably a fixnum at this point — a fixnum constant
/// ([`is_fixnum_const`]) OR the output of [`retag_fixnum`], i.e.
/// `bor_imm(ishl_imm(_, k>=FIXNUM_SHIFT), FIXNUM_CHECK_VALUE)`, whose low tag
/// bits are exactly `0b10`. In either case a fixnum guard on `v` would always
/// pass, so it can be elided. The retag case extends redundant-guard elimination
/// to chained arithmetic WITHIN a block: the range-checked, retagged inner result
/// of `(+ (+ a b) c)` / `(< (1+ i) n)` is re-guarded for nothing. (Sound even if
/// some non-retag op produced the same bit pattern — any value with low bits
/// `0b10` passes the guard. opt_level=none keeps the instruction sequence stable.)
pub(crate) fn is_known_fixnum(fb: &FunctionBuilder, v: ClifValue) -> bool {
    use cranelift_codegen::ir::Opcode;
    if is_fixnum_const(fb, v) {
        return true;
    }
    let Some((shifted, tag)) = binary_value_and_iconst(fb, v, Opcode::Bor) else {
        return false;
    };
    if tag != FIXNUM_CHECK_VALUE as i64 {
        return false;
    }
    // The bor operand must clear the low FIXNUM_SHIFT bits (a left shift by at
    // least FIXNUM_SHIFT), so `v`'s low two bits are exactly the fixnum tag.
    binary_value_and_iconst(fb, shifted, Opcode::Ishl)
        .is_some_and(|(_, shift)| shift >= FIXNUM_SHIFT as i64)
}

/// Guard that `v` is a fixnum (`(v & 0b11) == 0b10`), deopting otherwise.
/// Branch to `deopt` unless `v` is a tagged float.
///
/// The float twin of [`guard_fixnum`], and the reason a float operand no
/// longer has to bail the whole compiled body.
pub(crate) fn guard_float(fb: &mut FunctionBuilder, deopt: Block, v: ClifValue) {
    let tag = fb.ins().band_imm_u(v, TAG_MASK as i64);
    let is_float = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, tag, crate::tagged::value::TAG_FLOAT as i64);
    let cont = fb.create_block();
    fb.ins().brif(is_float, cont, &[], deopt, &[]);
    fb.switch_to_block(cont);
    fb.seal_block(cont);
}

/// The `f64` payload of a value already proved to be a float.
///
/// Reads at [`FLOAT_VALUE_OFFSET`], the same offset the runtime writes through
/// `FloatObj`, so the two can never drift.
pub(crate) fn unbox_float(fb: &mut FunctionBuilder, v: ClifValue) -> ClifValue {
    let ptr = fb.ins().band_imm_u(v, !(TAG_MASK as i64));
    fb.ins().load(
        types::F64,
        MemFlagsData::trusted(),
        ptr,
        crate::tagged::header::FLOAT_VALUE_OFFSET,
    )
}

/// Model-stack slot `k` as an `f64` at a FLOAT site, accepting a fixnum too:
/// a raw slot is a proven fixnum and PROMOTES (GNU promotes a fixnum operand
/// against a float); a tagged slot is tag-tested at run time — fixnum ->
/// untag + promote, float -> unbox, anything else -> deopt. Its own
/// diamond defining ONLY the f64 view: the compare helper below also
/// converts the float to an integer for GNU's tie-break, and that
/// `fcvt_to_sint_sat` would be dead here (~6 x86 instructions per site).
///
/// A flonum is its `f64`, which is always the value's double view (a
/// fixnum tag word's promotion included): no test, no load.
pub(crate) fn stack_as_f64_or_promote(
    fb: &mut FunctionBuilder,
    deopt: Block,
    stack: &[ClifValue],
    reps: &[SlotRep],
    k: usize,
) -> ClifValue {
    match reps[k] {
        SlotRep::RawFixnum => return fb.ins().fcvt_from_sint(types::F64, stack[k]),
        SlotRep::Flonum { f64, .. } => return f64,
        SlotRep::Tagged => {}
    }
    let v = stack[k];
    let out = fb.declare_var(types::F64);
    let fix_b = fb.create_block();
    let flt_b = fb.create_block();
    let merge = fb.create_block();
    let is_fix = fixnum_tag_test(fb, v);
    fb.ins().brif(is_fix, fix_b, &[], flt_b, &[]);

    fb.switch_to_block(fix_b);
    fb.seal_block(fix_b);
    let n = sshr_imm_p(fb, v, FIXNUM_SHIFT as i64);
    let promoted = fb.ins().fcvt_from_sint(types::F64, n);
    fb.def_var(out, promoted);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(flt_b);
    fb.seal_block(flt_b);
    guard_float(fb, deopt, v);
    let unboxed = unbox_float(fb, v);
    fb.def_var(out, unboxed);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    fb.use_var(out)
}

/// [`stack_as_f64_or_promote`] that ALSO returns an exact integer view of the
/// slot: the fixnum itself when it is one, else the float's value converted
/// with `fcvt_to_sint_sat` (saturating, NaN -> 0, so it is safe to evaluate
/// unconditionally). GNU's `arithcompare` needs it: when a fixnum and a float
/// tie as doubles it re-decides on integers, because promoting a fixnum
/// above 2^53 rounds — `(> 9007199254740993 9007199254740992.0)` is t and
/// `(= most-positive-fixnum (float most-positive-fixnum))` is nil.
///
/// A flonum's double view is its `f64`; its integer view is the float's
/// conversion when the tag word is the float sentinel, else the fixnum the
/// tag word holds (exact, where the `f64` may have rounded).
pub(crate) fn stack_as_f64_and_int(
    fb: &mut FunctionBuilder,
    deopt: Block,
    stack: &[ClifValue],
    reps: &[SlotRep],
    k: usize,
) -> (ClifValue, ClifValue) {
    match reps[k] {
        SlotRep::RawFixnum => {
            let f = fb.ins().fcvt_from_sint(types::F64, stack[k]);
            return (f, stack[k]);
        }
        SlotRep::Flonum {
            f64,
            kind: FlonumKind::Float,
        } => {
            let i = fb.ins().fcvt_to_sint_sat(types::I64, f64);
            return (f64, i);
        }
        SlotRep::Flonum {
            f64,
            kind: FlonumKind::FloatOrFixnum,
        } => {
            let tag = stack[k];
            let as_int = fb.ins().fcvt_to_sint_sat(types::I64, f64);
            let fixnum = sshr_imm_p(fb, tag, FIXNUM_SHIFT as i64);
            let is_float = icmp_imm_p(fb, IntCC::Equal, tag, UNBOXED_FLOAT_TAG_WORD);
            let i = fb.ins().select(is_float, as_int, fixnum);
            return (f64, i);
        }
        SlotRep::Tagged => {}
    }
    let v = stack[k];
    let out_f = fb.declare_var(types::F64);
    let out_i = fb.declare_var(types::I64);
    let fix_b = fb.create_block();
    let flt_b = fb.create_block();
    let merge = fb.create_block();
    let is_fix = fixnum_tag_test(fb, v);
    fb.ins().brif(is_fix, fix_b, &[], flt_b, &[]);

    fb.switch_to_block(fix_b);
    fb.seal_block(fix_b);
    let n = sshr_imm_p(fb, v, FIXNUM_SHIFT as i64);
    let promoted = fb.ins().fcvt_from_sint(types::F64, n);
    fb.def_var(out_f, promoted);
    fb.def_var(out_i, n);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(flt_b);
    fb.seal_block(flt_b);
    guard_float(fb, deopt, v);
    let unboxed = unbox_float(fb, v);
    let as_int = fb.ins().fcvt_to_sint_sat(types::I64, unboxed);
    fb.def_var(out_f, unboxed);
    fb.def_var(out_i, as_int);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    (fb.use_var(out_f), fb.use_var(out_i))
}

/// `(v & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE` as an i8 condition.
pub(crate) fn fixnum_tag_test(fb: &mut FunctionBuilder, v: ClifValue) -> ClifValue {
    let tag = band_imm_p(fb, v, FIXNUM_CHECK_MASK as i64);
    fb.ins()
        .icmp_imm_u(IntCC::Equal, tag, FIXNUM_CHECK_VALUE as i64)
}

/// The f64 `+ - * /` for a Float site. `/` gets the negative default NaN
/// only for an INVALID op (`0.0/0.0`, `inf/inf`) — a NaN INPUT propagates
/// with its own sign, as GNU's `(/ 0.0e+NaN 1)` = `0.0e+NaN` shows
/// (`builtin_div` applies the same rule).
pub(crate) fn emit_float_arith(
    fb: &mut FunctionBuilder,
    op: &Op,
    fa_val: ClifValue,
    fb_val: ClifValue,
) -> ClifValue {
    match op {
        Op::Add => fb.ins().fadd(fa_val, fb_val),
        Op::Sub => fb.ins().fsub(fa_val, fb_val),
        Op::Mul => fb.ins().fmul(fa_val, fb_val),
        _ => {
            let q = fb.ins().fdiv(fa_val, fb_val);
            let q_nan = fb
                .ins()
                .fcmp(cranelift_codegen::ir::condcodes::FloatCC::NotEqual, q, q);
            let a_ord = fb.ins().fcmp(
                cranelift_codegen::ir::condcodes::FloatCC::Equal,
                fa_val,
                fa_val,
            );
            let b_ord = fb.ins().fcmp(
                cranelift_codegen::ir::condcodes::FloatCC::Equal,
                fb_val,
                fb_val,
            );
            let ordered = fb.ins().band(a_ord, b_ord);
            let invalid = fb.ins().band(q_nan, ordered);
            let neg_nan = fb
                .ins()
                .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(
                    f64::NAN.to_bits() | (1_u64 << 63),
                ));
            fb.ins().select(invalid, neg_nan, q)
        }
    }
}

/// Run-time "both operands are floats" for model-stack slots `i` and `j`; a
/// raw slot is a proven fixnum, so the test is constant false. Fused like
/// [`both_fixnum_test`]. A flonum is tested on its tag word (the float
/// sentinel passes the float tag test), and a statically-float flonum
/// contributes no test at all.
fn both_float_test(
    fb: &mut FunctionBuilder,
    stack: &[ClifValue],
    reps: &[SlotRep],
    i: usize,
    j: usize,
) -> ClifValue {
    if reps[i] == SlotRep::RawFixnum || reps[j] == SlotRep::RawFixnum {
        return fb.ins().iconst(types::I8, 0);
    }
    match (reps[i].is_static_float(), reps[j].is_static_float()) {
        (true, true) => return fb.ins().iconst(types::I8, 1),
        (true, false) => return float_tag_test(fb, stack[j]),
        (false, true) => return float_tag_test(fb, stack[i]),
        (false, false) => {}
    }
    both_tag_test(
        fb,
        stack[i],
        stack[j],
        TAG_MASK as i64,
        crate::tagged::value::TAG_FLOAT as i64,
    )
}

/// Run-time "both operands are fixnums" for model-stack slots `i` and `j`;
/// a raw slot is a proven fixnum and contributes no test. Two tagged slots
/// use ONE fused test, `((a ^ 2) | (b ^ 2)) & 3 == 0`, which lowers to
/// xor/xor/or/test/jz — a `band` of two `icmp` results materialises both
/// through `setcc` first (11 instructions instead of 5).
///
/// A `FloatOrFixnum` flonum is tested on its tag word; a statically-float
/// one never reaches here (its site has no fixnum arm).
fn both_fixnum_test(
    fb: &mut FunctionBuilder,
    stack: &[ClifValue],
    reps: &[SlotRep],
    i: usize,
    j: usize,
) -> ClifValue {
    debug_assert!(!reps[i].is_static_float() && !reps[j].is_static_float());
    match (reps[i] == SlotRep::RawFixnum, reps[j] == SlotRep::RawFixnum) {
        (true, true) => fb.ins().iconst(types::I8, 1),
        (true, false) => fixnum_tag_test(fb, stack[j]),
        (false, true) => fixnum_tag_test(fb, stack[i]),
        (false, false) => both_tag_test(
            fb,
            stack[i],
            stack[j],
            FIXNUM_CHECK_MASK as i64,
            FIXNUM_CHECK_VALUE as i64,
        ),
    }
}

/// `(v & 7) == TAG_FLOAT` as an i8 condition.
fn float_tag_test(fb: &mut FunctionBuilder, v: ClifValue) -> ClifValue {
    let tag = band_imm_p(fb, v, TAG_MASK as i64);
    icmp_imm_p(
        fb,
        IntCC::Equal,
        tag,
        crate::tagged::value::TAG_FLOAT as i64,
    )
}

/// Model-stack slot `k` as an `f64` in a float site's both-floats arm: a
/// flonum's `f64`, else the boxed float's payload. (A raw slot makes the
/// arm dead — its test is constant false — and unboxes it anyway, as the
/// arm always has.)
fn float_payload(
    fb: &mut FunctionBuilder,
    stack: &[ClifValue],
    reps: &[SlotRep],
    k: usize,
) -> ClifValue {
    match reps[k] {
        SlotRep::Flonum { f64, .. } => f64,
        SlotRep::Tagged | SlotRep::RawFixnum => unbox_float(fb, stack[k]),
    }
}

/// Model-stack slot `k` as an untagged i64 in a float site's both-fixnums
/// arm: a raw slot as is, else the tag word (a `FloatOrFixnum` flonum's
/// included) untagged.
fn fixnum_payload(
    fb: &mut FunctionBuilder,
    stack: &[ClifValue],
    reps: &[SlotRep],
    k: usize,
) -> ClifValue {
    if reps[k] == SlotRep::RawFixnum {
        stack[k]
    } else {
        sshr_imm_p(fb, stack[k], FIXNUM_SHIFT as i64)
    }
}

/// `((a ^ tag) | (b ^ tag)) & mask == 0`: both values carry `tag` under
/// `mask`, as one branch-fusable i8 condition.
fn both_tag_test(
    fb: &mut FunctionBuilder,
    a: ClifValue,
    b: ClifValue,
    mask: i64,
    tag: i64,
) -> ClifValue {
    let xa = fb.ins().bxor_imm_u(a, tag);
    let xb = fb.ins().bxor_imm_u(b, tag);
    let either = fb.ins().bor(xa, xb);
    let masked = band_imm_p(fb, either, mask);
    fb.ins().icmp_imm_u(IntCC::Equal, masked, 0)
}

/// Box an `f64` result back into a tagged Lisp float.
pub(crate) fn box_float(fb: &mut FunctionBuilder, rt: &RtCtx, v: ClifValue) -> ClifValue {
    let call = fb.ins().call(rt.refs.make_float, &[v]);
    fb.inst_results(call)[0]
}

pub(crate) fn guard_fixnum(
    fb: &mut FunctionBuilder,
    deopt: Block,
    v: ClifValue,
    known: &HashSet<ClifValue>,
) {
    // Redundant-guard elimination: a value provably a fixnum needs no runtime
    // guard. Within-block: a fixnum constant or range-checked+retagged arithmetic
    // result ([`is_known_fixnum`]). Cross-block: an operand the dataflow analysis
    // proved fixnum at this block's entry ([`compute_known_fixnum_slots`], seeded
    // into `known` by `lower_leaf_full`).
    if is_known_fixnum(fb, v) || known.contains(&v) {
        return;
    }
    let tag = band_imm_p(fb, v, FIXNUM_CHECK_MASK as i64);
    let is_fix = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, tag, FIXNUM_CHECK_VALUE as i64);
    emit_guard(fb, deopt, is_fix);
}

/// Retag an untagged i64 `n` as a fixnum `Value`: `(n << 2) | 2`.
pub(crate) fn retag_fixnum(fb: &mut FunctionBuilder, n: ClifValue) -> ClifValue {
    RETAG_COUNT.with(|c| c.set(c.get() + 1));
    let shifted = ishl_imm_p(fb, n, FIXNUM_SHIFT as i64);
    bor_imm_p(fb, shifted, FIXNUM_CHECK_VALUE as i64)
}

/// Lower a fixnum-fast-path binary op (`Add`/`Sub`) with the exact parity the
/// interpreter uses (`vm.rs` `Op::Add`): require both operands be fixnums and
/// the result be in fixnum range, else deopt. Returns the tagged-fixnum result.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lower_fixnum_binop(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_sub: bool,
    a: ClifValue,
    b: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    guard_fixnum(fb, deopt, b, known);

    // Untag (arithmetic shift right by 2 == GNU XFIXNUM), compute, range-check.
    let av = sshr_imm_p(fb, a, FIXNUM_SHIFT as i64);
    let bv = sshr_imm_p(fb, b, FIXNUM_SHIFT as i64);
    // Operands are <= 61-bit, so the i64 result cannot overflow; a fixnum-range
    // check is sufficient and matches the interpreter exactly.
    let res = if is_sub {
        fb.ins().isub(av, bv)
    } else {
        fb.ins().iadd(av, bv)
    };

    // Guard: MOST_NEGATIVE_FIXNUM <= res <= MOST_POSITIVE_FIXNUM.
    let ge_lo = icmp_imm_p(
        fb,
        IntCC::SignedGreaterThanOrEqual,
        res,
        Value::MOST_NEGATIVE_FIXNUM,
    );
    let le_hi = icmp_imm_p(
        fb,
        IntCC::SignedLessThanOrEqual,
        res,
        Value::MOST_POSITIVE_FIXNUM,
    );
    let in_range = fb.ins().band(ge_lo, le_hi);
    emit_guard(fb, deopt, in_range);

    retag_fixnum(fb, res)
}

/// A fixnum-fast-path unary opcode.
#[derive(Clone, Copy)]
pub(crate) enum UnaryKind {
    /// `1+`: n -> n + 1.
    Add1,
    /// `1-`: n -> n - 1.
    Sub1,
    /// unary `-`: n -> -n.
    Negate,
}

/// Lower a fixnum-fast-path unary op with exact interpreter parity (`vm.rs`
/// `Op::Add1`/`Op::Sub1`/`Op::Negate`): require a fixnum operand whose result
/// stays in range, else deopt. The single out-of-range input per op is the
/// boundary fixnum, so the interpreter's `n != BOUND` guard is reproduced
/// exactly rather than a post-compute range check.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lower_fixnum_unop(
    fb: &mut FunctionBuilder,
    deopt: Block,
    kind: UnaryKind,
    a: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    let n = sshr_imm_p(fb, a, FIXNUM_SHIFT as i64);

    // The only input that leaves fixnum range is the op's boundary value.
    let bound = match kind {
        UnaryKind::Add1 => Value::MOST_POSITIVE_FIXNUM,
        UnaryKind::Sub1 | UnaryKind::Negate => Value::MOST_NEGATIVE_FIXNUM,
    };
    let in_range = icmp_imm_p(fb, IntCC::NotEqual, n, bound);
    emit_guard(fb, deopt, in_range);

    let res = match kind {
        UnaryKind::Add1 => iadd_imm_p(fb, n, 1),
        UnaryKind::Sub1 => iadd_imm_p(fb, n, -1),
        UnaryKind::Negate => fb.ins().ineg(n),
    };
    retag_fixnum(fb, res)
}

/// Lower a fixnum multiply with exact interpreter parity (`vm.rs` `Op::Mul`):
/// both operands fixnums and the exact product in fixnum range, else deopt.
///
/// Operands are <= 61-bit so the product is <= 122-bit; widening to `i128` makes
/// it exact, then a single range check covers both i64 overflow and
/// fixnum-range overflow at once.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lower_fixnum_mul(
    fb: &mut FunctionBuilder,
    deopt: Block,
    a: ClifValue,
    b: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    guard_fixnum(fb, deopt, b, known);
    let av = sshr_imm_p(fb, a, FIXNUM_SHIFT as i64);
    let bv = sshr_imm_p(fb, b, FIXNUM_SHIFT as i64);

    let a128 = fb.ins().sextend(types::I128, av);
    let b128 = fb.ins().sextend(types::I128, bv);
    let prod = fb.ins().imul(a128, b128);

    let lo = fb.ins().iconst(types::I64, Value::MOST_NEGATIVE_FIXNUM);
    let hi = fb.ins().iconst(types::I64, Value::MOST_POSITIVE_FIXNUM);
    let lo128 = fb.ins().sextend(types::I128, lo);
    let hi128 = fb.ins().sextend(types::I128, hi);
    let ge = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, prod, lo128);
    let le = fb.ins().icmp(IntCC::SignedLessThanOrEqual, prod, hi128);
    let in_range = fb.ins().band(ge, le);
    emit_guard(fb, deopt, in_range);

    let res = fb.ins().ireduce(types::I64, prod);
    retag_fixnum(fb, res)
}

/// Lower fixnum `/` or `%` with exact interpreter parity (`vm.rs`
/// `Op::Div`/`Op::Rem`): both operands fixnums and the divisor nonzero, else
/// deopt (the interpreter's `/` builtin signals arith-error on zero). Rust and
/// CLIF `sdiv`/`srem` both truncate toward zero, matching the interpreter; the
/// operands are <= 61-bit so the i64 ops cannot trap.
///
/// STALE PARITY (dead code): the interpreter's `Op::Div` fast path now
/// range-checks and routes `MOST_NEGATIVE_FIXNUM / -1` to the `/` builtin
/// (bignum promotion, like GNU), so wiring this lowering up would require the
/// same range guard the unboxed MIR analogue `raw_fixnum_divrem` already has
/// (it deopts on the overflow rather than wrapping).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lower_fixnum_divrem(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_rem: bool,
    a: ClifValue,
    b: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    guard_fixnum(fb, deopt, b, known);
    let bv = sshr_imm_p(fb, b, FIXNUM_SHIFT as i64);
    let nonzero = icmp_imm_p(fb, IntCC::NotEqual, bv, 0);
    emit_guard(fb, deopt, nonzero);
    let av = sshr_imm_p(fb, a, FIXNUM_SHIFT as i64);
    let res = if is_rem {
        fb.ins().srem(av, bv)
    } else {
        fb.ins().sdiv(av, bv)
    };
    retag_fixnum(fb, res)
}

/// A non-allocating unary type/nil predicate. Inspects only the tagged bits;
/// never dereferences the value, allocates, or deopts.
#[derive(Clone, Copy)]
pub(crate) enum PredKind {
    /// `null`/`not`: value is nil.
    Null,
    /// `consp`: value is a cons.
    Consp,
    /// `stringp`: value is a string.
    Stringp,
    /// `listp`: value is nil or a cons.
    Listp,
}

/// Lower a type/nil predicate to `t`/`nil` via `select` (no branch, no deopt —
/// it matches the interpreter for any value by inspecting the tag bits).
pub(crate) fn lower_predicate(fb: &mut FunctionBuilder, kind: PredKind, a: ClifValue) -> ClifValue {
    let cond = match kind {
        PredKind::Null => fb
            .ins()
            .icmp_imm_u(IntCC::Equal, a, Value::NIL.bits() as i64),
        PredKind::Consp => {
            let tag = band_imm_p(fb, a, TAG_MASK as i64);
            icmp_imm_p(fb, IntCC::Equal, tag, TAG_CONS as i64)
        }
        PredKind::Stringp => {
            let tag = band_imm_p(fb, a, TAG_MASK as i64);
            icmp_imm_p(fb, IntCC::Equal, tag, TAG_STRING as i64)
        }
        PredKind::Listp => {
            let is_nil = fb
                .ins()
                .icmp_imm_u(IntCC::Equal, a, Value::NIL.bits() as i64);
            let tag = band_imm_p(fb, a, TAG_MASK as i64);
            let is_cons = icmp_imm_p(fb, IntCC::Equal, tag, TAG_CONS as i64);
            fb.ins().bor(is_nil, is_cons)
        }
    };
    let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
    let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
    fb.ins().select(cond, t, nil)
}

/// Lower `car`/`cdr` (and the `-safe` variants) with exact interpreter parity:
/// a cons yields the loaded field; otherwise plain car/cdr yields nil for nil
/// and deopts for anything else (the interpreter signals
/// `wrong-type-argument`), while car-safe/cdr-safe yield nil for ANY non-cons
/// (total, no deopt). Non-allocating; reading a cons field needs no SATB
/// barrier (the barrier is on writes), and there is no GC safepoint here.
pub(crate) fn lower_car_cdr(
    fb: &mut FunctionBuilder,
    deopt: Option<Block>,
    is_cdr: bool,
    safe: bool,
    a: ClifValue,
) -> ClifValue {
    let tag = band_imm_p(fb, a, TAG_MASK as i64);
    let is_cons = icmp_imm_p(fb, IntCC::Equal, tag, TAG_CONS as i64);
    if !safe {
        let is_nil = fb
            .ins()
            .icmp_imm_u(IntCC::Equal, a, Value::NIL.bits() as i64);
        let valid = fb.ins().bor(is_cons, is_nil);
        emit_guard(
            fb,
            deopt.expect("guarded car/cdr lowers with a deopt site"),
            valid,
        );
    }

    // Branch: cons -> load the field; nil -> nil. The result flows through a
    // fresh SSA variable (Cranelift inserts the phi at the merge).
    let res = fb.declare_var(types::I64);
    let cons_blk = fb.create_block();
    let nil_blk = fb.create_block();
    let merge = fb.create_block();
    fb.ins().brif(is_cons, cons_blk, &[], nil_blk, &[]);

    fb.switch_to_block(cons_blk);
    let ptr = band_imm_p(fb, a, !(TAG_MASK as i64));
    let offset = if is_cdr {
        core::mem::offset_of!(ConsCell, cdr_or_next)
    } else {
        core::mem::offset_of!(ConsCell, car)
    };
    let field = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), ptr, offset as i32);
    fb.def_var(res, field);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(nil_blk);
    if safe {
        // -safe variants: ANY non-cons yields nil.
        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
        fb.def_var(res, nil);
    } else {
        fb.def_var(res, a); // nil -> nil (a already holds nil, guarded above)
    }
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.use_var(res)
}

/// `aref` of a plain vector or record, read inline — GNU `Baref`'s
/// in-bytecode path. Emits the checks `neovm_jit_aref`'s fast path makes
/// (a vector or record, a fixnum index in range, not a tagged char-table or
/// bool-vector vector), branching to `slow` when any fails; on success defines
/// `res` as the slot and jumps to `merge`. The shim was ~60 instructions per
/// `aref` with the call: 63M arefs in elb nbody.
///
/// Returns `false`, emitting nothing, when this build's vector storage does
/// not keep its element pointer and length at offsets every storage kind
/// shares (`LispValueVec::jit_slice_offsets`).
fn emit_inline_aref(
    fb: &mut FunctionBuilder,
    array: ClifValue,
    index: ClifValue,
    slow: Block,
    res: Variable,
    merge: Block,
) -> bool {
    use crate::tagged::header::{LispValueVec, VecLikeHeader, VecLikeType, VectorObj};
    let Some((ptr_off, len_off)) = LispValueVec::jit_slice_offsets() else {
        return false;
    };
    const_assert_vector_record_share_layout();
    let data_off = core::mem::offset_of!(VectorObj, data);
    let type_off = core::mem::offset_of!(VecLikeHeader, type_tag);
    let (char_table_tag, bool_vector_tag, char_table_min_len) =
        crate::emacs_core::chartable::inline_vector_tag_shape();

    let tag = band_imm_p(fb, array, TAG_MASK as i64);
    let is_veclike = icmp_imm_p(
        fb,
        IntCC::Equal,
        tag,
        crate::tagged::value::TAG_VECLIKE as i64,
    );
    let index_tag = band_imm_p(fb, index, FIXNUM_CHECK_MASK as i64);
    let is_fixnum = icmp_imm_p(fb, IntCC::Equal, index_tag, FIXNUM_CHECK_VALUE as i64);
    let shapes = fb.ins().band(is_veclike, is_fixnum);
    let typed = fb.create_block();
    fb.ins().brif(shapes, typed, &[], slow, &[]);
    fb.switch_to_block(typed);
    fb.seal_block(typed);
    let object = band_imm_p(fb, array, !(TAG_MASK as i64));
    let type_tag = fb
        .ins()
        .load(types::I8, MemFlagsData::trusted(), object, type_off as i32);
    let is_vector = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, type_tag, VecLikeType::Vector as u8 as i64);
    let is_record = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, type_tag, VecLikeType::Record as u8 as i64);
    let either = fb.ins().bor(is_vector, is_record);
    let ranged = fb.create_block();
    fb.ins().brif(either, ranged, &[], slow, &[]);
    fb.switch_to_block(ranged);
    fb.seal_block(ranged);
    let len = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        object,
        (data_off + len_off) as i32,
    );
    let slots = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        object,
        (data_off + ptr_off) as i32,
    );
    let i = sshr_imm_p(fb, index, FIXNUM_SHIFT as i64);
    // Unsigned: a negative index is out of range too.
    let in_range = fb.ins().icmp(IntCC::UnsignedLessThan, i, len);
    let plain = fb.create_block();
    fb.ins().brif(in_range, plain, &[], slow, &[]);
    fb.switch_to_block(plain);
    fb.seal_block(plain);
    // `classify_vector_slots`: a vector of two or more slots whose slot 0 is
    // the bool-vector tag, or of `char_table_min_len` or more whose slot 0 is
    // the char-table tag (vectors only), is not plain. `i < len` put slot 0
    // in bounds.
    let first = fb.ins().load(types::I64, MemFlagsData::trusted(), slots, 0);
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
    let two_or_more = icmp_imm_p(fb, IntCC::UnsignedGreaterThanOrEqual, len, 2);
    let not_plain = fb.ins().band(tagged, two_or_more);
    let load = fb.create_block();
    fb.ins().brif(not_plain, slow, &[], load, &[]);
    fb.switch_to_block(load);
    fb.seal_block(load);
    let byte_off = ishl_imm_p(fb, i, 3);
    let slot = fb.ins().iadd(slots, byte_off);
    let element = fb.ins().load(types::I64, MemFlagsData::trusted(), slot, 0);
    fb.def_var(res, element);
    fb.ins().jump(merge, &[]);
    true
}

/// `type-of` / `cl-type-of` of a record, answered inline at an armed spec
/// site: `record_type_of`, which is slot 0, or slot 1 of slot 0 when slot 0
/// is itself a record of two or more slots (an EIEIO object's class). The
/// arming test is `subr_spec_armed`'s fast answer — the site's epoch is the
/// obarray's, no compiler overrides — plus `debug-on-next-call` down (its
/// memoized cell resolved and false) and `maybe_quit_hot_ok` (no quit, signal,
/// profiler tick or bound `throw-on-input`: those take the shim, which polls
/// first, as GNU's `Bcall` does). Anything else, a non-record or a record of
/// no slots included, continues in the block this leaves current, where the
/// caller emits the shim call. Every length is read only after its object's
/// type tag says record. Returns the merge block and the result variable the
/// hit path defined; the caller defines it on its own path and jumps to the
/// merge. `None`, emitting nothing, when a layout probe fails.
///
/// Every `cl-defstruct` accessor and predicate asks `(type-of x)`; through
/// `neovm_jit_pred_spec` that was ~100 instructions a call, 2,508 per
/// elb-eieio repeat.
/// Lower a Tier-A buffer read INLINE -- the load itself, in place of a call to
/// `neovm_jit_cbsym_read` that performs it on the caller's behalf.
///
/// GNU answers each of these with a single bytecode opcode reading a field of
/// `current_buffer` (`Bpoint`, `Bpoint_min`); the shim's own prologue and
/// epilogue cost more than the load, which is why `(point-min)` measured 78
/// instructions against GNU's 2.
///
/// Returns a STATUS value with the shim's own meaning -- `STATUS_OK` with the
/// answer already in `call_result_slot`, or `STATUS_NEED_GENERIC` -- so the
/// call site's existing merge handles both paths unchanged. Returns `None` to
/// emit nothing at all and keep the shim: for any `which` not handled here,
/// and for any layout probe that could not prove what it found.
///
/// Everything that is not a plain field read bounces to the generic path: no
/// current buffer, an id outside the slot vector, a freed slot, or the symbol
/// no longer being a plain builtin.
///
/// SOUNDNESS: the loads are `MemFlagsData::trusted()`, which is `notrap` and
/// `aligned` and deliberately NOT `readonly`. `dispatch.rs`'s ANTI-REQUIREMENT
/// says buffer state must never be value-numbered across a `CallBuiltinSym`
/// op, and that still holds with the read inlined -- not because the call is
/// opaque any more, but because cranelift gives `Opcode::Call` memory-fence
/// semantics (`inst_predicates.rs`), so a load after a call cannot forward
/// from one before it, and only a `readonly` load is pure enough for the
/// egraph to hoist. Marking these `readonly` would break exactly that.
/// How many Tier-A sites were lowered inline, so a test can tell "the inline
/// path answered" from "the emitter declined and the shim answered" -- which
/// a correctness assertion alone cannot distinguish.
#[cfg(debug_assertions)]
pub(crate) static INLINE_CBSYM_READ_EMITTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn emit_inline_cbsym_read(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    which: u8,
    sym: u32,
) -> Option<ClifValue> {
    use crate::buffer::buffer::{Buffer, jit_layout};
    use crate::emacs_core::eval::runtime_projection::CONTEXT_BUFFERS_OFFSET;

    // Two shapes. The position reads answer a 1-based Lisp CHARACTER position;
    // `bobp`/`eobp` compare point against an accessible bound in the BYTE
    // coordinate and answer t/nil -- GNU's `Fbobp' is `PT == BEGV'. Reading
    // the character coordinate for those would compare the wrong pair of
    // coordinates and still typecheck, which is why both sets of offsets
    // exist.
    let (field_off, bound_off) = match which {
        CBSYM_A_POINT => (Buffer::POINT_CHAR_POS_OFFSET, None),
        CBSYM_A_POINT_MIN => (Buffer::BEGV_CHAR_POS_OFFSET, None),
        CBSYM_A_POINT_MAX => (Buffer::ZV_CHAR_POS_OFFSET, None),
        CBSYM_A_BOBP => (
            Buffer::POINT_EMACS_BYTE_POS_OFFSET,
            Some(Buffer::BEGV_EMACS_BYTE_POS_OFFSET),
        ),
        CBSYM_A_EOBP => (
            Buffer::POINT_EMACS_BYTE_POS_OFFSET,
            Some(Buffer::ZV_EMACS_BYTE_POS_OFFSET),
        ),
        _ => return None,
    };
    let Some((bits_addr, bits_mask)) =
        crate::emacs_core::eval::builtin_sym_bit_probe(crate::emacs_core::intern::SymId(sym))
    else {
        return None;
    };
    let Some((tag_off, payload_off, none_tag)) = jit_layout::current_option_layout() else {
        return None;
    };
    let Some((ptr_off, len_off)) = jit_layout::slots_vec_offsets() else {
        return None;
    };

    let flags = MemFlagsData::trusted();
    let manager = CONTEXT_BUFFERS_OFFSET;
    let current = manager + jit_layout::BUFFER_MANAGER_CURRENT_OFFSET;
    let slots =
        manager + jit_layout::BUFFER_MANAGER_BUFFERS_OFFSET + jit_layout::LIVE_BUFFERS_SLOTS_OFFSET;

    let miss = fb.create_block();
    let done = fb.create_block();
    let status = fb.declare_var(types::I64);

    // Arming, and the ONLY dynamic test the shim makes: is the symbol still a
    // plain builtin? (Its arity check and the harness override are both
    // compile-time here.) A bitmap word and a mask, both baked.
    let bits_ptr = fb.ins().iconst(rt.ptr_ty, bits_addr as i64);
    let word = fb.ins().load(types::I64, flags, bits_ptr, 0);
    let armed = band_imm_p(fb, word, bits_mask as i64);
    let has_buffer = fb.create_block();
    fb.ins().brif(armed, has_buffer, &[], miss, &[]);

    // Is there a current buffer at all?
    fb.switch_to_block(has_buffer);
    fb.seal_block(has_buffer);
    let vmctx = fb.use_var(rt.vmctx_var);
    let tag = fb
        .ins()
        .load(types::I64, flags, vmctx, (current + tag_off) as i32);
    let present = icmp_imm_p(fb, IntCC::NotEqual, tag, none_tag as i64);
    let in_range = fb.create_block();
    fb.ins().brif(present, in_range, &[], miss, &[]);

    // Its id must index the slot vector. `LiveBuffers::len` is the live COUNT,
    // not the vector's length, so the bound has to come from the probed word.
    fb.switch_to_block(in_range);
    fb.seal_block(in_range);
    let id = fb
        .ins()
        .load(types::I64, flags, vmctx, (current + payload_off) as i32);
    let len = fb
        .ins()
        .load(types::I64, flags, vmctx, (slots + len_off) as i32);
    let within = fb.ins().icmp(IntCC::UnsignedLessThan, id, len);
    let live = fb.create_block();
    fb.ins().brif(within, live, &[], miss, &[]);

    // Each slot is one niche-optimised pointer word, so a freed slot is null.
    fb.switch_to_block(live);
    fb.seal_block(live);
    let base = fb
        .ins()
        .load(rt.ptr_ty, flags, vmctx, (slots + ptr_off) as i32);
    let byte_index = fb.ins().imul_imm(id, 8);
    let slot_addr = fb.ins().iadd(base, byte_index);
    let buffer = fb.ins().load(rt.ptr_ty, flags, slot_addr, 0);
    let read = fb.create_block();
    let occupied = icmp_imm_p(fb, IntCC::NotEqual, buffer, 0);
    fb.ins().brif(occupied, read, &[], miss, &[]);

    // The read itself.
    fb.switch_to_block(read);
    fb.seal_block(read);
    let field = fb.ins().load(types::I64, flags, buffer, field_off as i32);
    let tagged = match bound_off {
        // A position: GNU's 1-based Lisp coordinate, tagged.
        None => {
            let lisp = fb.ins().iadd_imm(field, 1);
            retag_fixnum(fb, lisp)
        }
        // A bound test: `PT == BEGV` / `PT == ZV`, answered as t or nil.
        Some(bound_off) => {
            let bound = fb.ins().load(types::I64, flags, buffer, bound_off as i32);
            let at_bound = fb.ins().icmp(IntCC::Equal, field, bound);
            let yes = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            let no = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            fb.ins().select(at_bound, yes, no)
        }
    };
    fb.ins()
        .stack_store(rt.ptr_ty, tagged, rt.call_result_slot, 0);
    let ok = fb.ins().iconst(types::I64, STATUS_OK);
    fb.def_var(status, ok);
    fb.ins().jump(done, &[]);

    fb.switch_to_block(miss);
    fb.seal_block(miss);
    let need_generic = fb.ins().iconst(types::I64, STATUS_NEED_GENERIC);
    fb.def_var(status, need_generic);
    fb.ins().jump(done, &[]);

    fb.switch_to_block(done);
    fb.seal_block(done);
    #[cfg(debug_assertions)]
    INLINE_CBSYM_READ_EMITTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Some(fb.use_var(status))
}

fn emit_inline_record_type_of(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    slot: ClifValue,
    arg: ClifValue,
) -> Option<(Block, Variable)> {
    use crate::emacs_core::eval::runtime_projection::{
        CONTEXT_COMPILER_OVERRIDES_ACTIVE_OFFSET, CONTEXT_QUIT_FLAG_OFFSET,
        CONTEXT_QUIT_REQUESTED_OFFSET, CONTEXT_THROW_ON_INPUT_OFFSET, arc_atomic_bool_data_offset,
    };
    use crate::emacs_core::forward::LISP_BOOL_FWD_VALUE_OFFSET;
    use crate::emacs_core::symbol::{
        OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET, OBARRAY_FUNCTION_EPOCH_OFFSET,
    };
    use crate::tagged::header::{LispValueVec, VecLikeHeader, VecLikeType, VectorObj};
    let (ptr_off, len_off) = LispValueVec::jit_slice_offsets()?;
    let quit_requested_off = arc_atomic_bool_data_offset()?;
    const_assert_vector_record_share_layout();
    let data_off = core::mem::offset_of!(VectorObj, data);
    let type_off = core::mem::offset_of!(VecLikeHeader, type_tag);
    let ob = core::mem::offset_of!(Context, obarray);
    let slot_epoch_off = core::mem::offset_of!(super::SpecSlot, epoch);
    let flags = MemFlagsData::trusted();
    debug_assert_eq!(Value::NIL.bits(), 0, "the clear test ORs nil words");
    let record_tag = VecLikeType::Record as u8 as i64;

    let miss = fb.create_block();
    let merge = fb.create_block();
    let res = fb.declare_var(types::I64);
    // Stage 1: a veclike argument, the one test a miss usually fails.
    let tag = band_imm_p(fb, arg, TAG_MASK as i64);
    let veclike = icmp_imm_p(
        fb,
        IntCC::Equal,
        tag,
        crate::tagged::value::TAG_VECLIKE as i64,
    );
    let stage2 = fb.create_block();
    fb.ins().brif(veclike, stage2, &[], miss, &[]);
    // Stage 2: arming, the quit poll's fast test, and a record. Every flag
    // and word that must be clear is ORed into one test (nil is zero).
    fb.switch_to_block(stage2);
    fb.seal_block(stage2);
    let vmctx = fb.use_var(rt.vmctx_var);
    let epoch = fb.ins().load(
        types::I64,
        flags,
        vmctx,
        (ob + OBARRAY_FUNCTION_EPOCH_OFFSET) as i32,
    );
    let armed_epoch = fb
        .ins()
        .load(types::I64, flags, slot, slot_epoch_off as i32);
    let epoch_ok = fb.ins().icmp(IntCC::Equal, epoch, armed_epoch);
    let overrides = fb.ins().uload8(
        types::I64,
        flags,
        vmctx,
        CONTEXT_COMPILER_OVERRIDES_ACTIVE_OFFSET as i32,
    );
    let quit_flag = fb
        .ins()
        .load(types::I64, flags, vmctx, CONTEXT_QUIT_FLAG_OFFSET as i32);
    let throw_on_input = fb.ins().load(
        types::I64,
        flags,
        vmctx,
        CONTEXT_THROW_ON_INPUT_OFFSET as i32,
    );
    let requested_arc = fb.ins().load(
        rt.ptr_ty,
        flags,
        vmctx,
        CONTEXT_QUIT_REQUESTED_OFFSET as i32,
    );
    let requested = fb
        .ins()
        .uload8(types::I64, flags, requested_arc, quit_requested_off as i32);
    let signal_addr = fb.ins().iconst(
        rt.ptr_ty,
        crate::emacs_core::os_signal::pending_flag_addr() as i64,
    );
    let signal = fb.ins().uload8(types::I64, flags, signal_addr, 0);
    let tick_addr = fb.ins().iconst(
        rt.ptr_ty,
        crate::emacs_core::profiler::profiler_sample_due_addr() as i64,
    );
    let tick = fb.ins().uload8(types::I64, flags, tick_addr, 0);
    // An unresolved `debug-on-next-call` cell reads through a stand-in that
    // is always armed.
    let fwd = fb.ins().load(
        rt.ptr_ty,
        flags,
        vmctx,
        (ob + OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET) as i32,
    );
    let always_armed = fb.ins().iconst(
        rt.ptr_ty,
        std::ptr::from_ref(&crate::emacs_core::forward::JIT_ALWAYS_TRUE_BOOL_FWD) as i64,
    );
    let fwd_resolved = icmp_imm_p(fb, IntCC::NotEqual, fwd, 0);
    let cell = fb.ins().select(fwd_resolved, fwd, always_armed);
    let debug = fb
        .ins()
        .uload8(types::I64, flags, cell, LISP_BOOL_FWD_VALUE_OFFSET as i32);
    let object = band_imm_p(fb, arg, !(TAG_MASK as i64));
    let type_tag = fb.ins().uload8(types::I64, flags, object, type_off as i32);
    let not_record = fb.ins().bxor_imm_u(type_tag, record_tag);
    let set = fb.ins().bor(overrides, quit_flag);
    let set = fb.ins().bor(set, throw_on_input);
    let set = fb.ins().bor(set, requested);
    let set = fb.ins().bor(set, signal);
    let set = fb.ins().bor(set, tick);
    let set = fb.ins().bor(set, debug);
    let set = fb.ins().bor(set, not_record);
    let clear = icmp_imm_p(fb, IntCC::Equal, set, 0);
    let stage2_ok = fb.ins().band(epoch_ok, clear);
    let sized = fb.create_block();
    fb.ins().brif(stage2_ok, sized, &[], miss, &[]);
    // Stage 3: a record of one or more slots.
    fb.switch_to_block(sized);
    fb.seal_block(sized);
    let len = fb
        .ins()
        .load(types::I64, flags, object, (data_off + len_off) as i32);
    let has_slot = icmp_imm_p(fb, IntCC::NotEqual, len, 0);
    let hit = fb.create_block();
    fb.ins().brif(has_slot, hit, &[], miss, &[]);
    fb.switch_to_block(miss);
    // All three edges into `miss` exist now.
    fb.seal_block(miss);
    // The hit: slot 0, which is the answer unless it is a class record.
    fb.switch_to_block(hit);
    fb.seal_block(hit);
    let slots = fb
        .ins()
        .load(types::I64, flags, object, (data_off + ptr_off) as i32);
    let tag_slot = fb.ins().load(types::I64, flags, slots, 0);
    fb.def_var(res, tag_slot);
    let tag_slot_tag = band_imm_p(fb, tag_slot, TAG_MASK as i64);
    let tag_slot_veclike = icmp_imm_p(
        fb,
        IntCC::Equal,
        tag_slot_tag,
        crate::tagged::value::TAG_VECLIKE as i64,
    );
    let class = fb.create_block();
    fb.ins().brif(tag_slot_veclike, class, &[], merge, &[]);
    fb.switch_to_block(class);
    fb.seal_block(class);
    let class_object = band_imm_p(fb, tag_slot, !(TAG_MASK as i64));
    let class_type = fb
        .ins()
        .uload8(types::I64, flags, class_object, type_off as i32);
    let class_is_record = icmp_imm_p(fb, IntCC::Equal, class_type, record_tag);
    let class_sized = fb.create_block();
    fb.ins().brif(class_is_record, class_sized, &[], merge, &[]);
    fb.switch_to_block(class_sized);
    fb.seal_block(class_sized);
    let class_len = fb
        .ins()
        .load(types::I64, flags, class_object, (data_off + len_off) as i32);
    let two_or_more = icmp_imm_p(fb, IntCC::UnsignedGreaterThan, class_len, 1);
    let name = fb.create_block();
    fb.ins().brif(two_or_more, name, &[], merge, &[]);
    fb.switch_to_block(name);
    fb.seal_block(name);
    let class_slots = fb
        .ins()
        .load(types::I64, flags, class_object, (data_off + ptr_off) as i32);
    let class_name = fb.ins().load(types::I64, flags, class_slots, 8);
    fb.def_var(res, class_name);
    fb.ins().jump(merge, &[]);
    fb.switch_to_block(miss);
    Some((merge, res))
}

/// `emit_inline_aref` reads a record through `VectorObj`'s offsets.
const fn const_assert_vector_record_share_layout() {
    use crate::tagged::header::{RecordObj, VectorObj};
    assert!(core::mem::offset_of!(VectorObj, data) == core::mem::offset_of!(RecordObj, data));
}

/// Lower a no-argument straight-line leaf body. Thin wrapper over [`lower_leaf`]
/// kept for the existing call sites/tests.
pub fn lower_nullary_leaf(ops: &[Op], constants: &[Value]) -> Result<CompiledLeaf, CompileError> {
    lower_leaf(ops, constants, 0)
}

/// Get MIR value `v` as a RAW (untagged) fixnum i64 for arithmetic. If `cval_raw`
/// marks it already raw (a prior fixnum arithmetic result or fixnum constant in
/// this block), use it directly — no re-guard, no re-untag (the unboxing fast
/// path: chained fixnum arithmetic stays raw). Otherwise a value is guarded
/// and untagged AT MOST ONCE per block: the guard is skipped when the type
/// fixpoint proved the value a fixnum (`proven_fixnum`, a block param the
/// loop carries as a `Bin`/`Unary` result on every edge — the baseline's
/// cross-block `known` set, per value rather than per slot because every
/// `MirValue` is block-local), and the untagged form is memoized in
/// `raw_twin`, so a second use (`(+ acc i)` then `(1+ i)`) reuses it.
///
/// The twin is NEVER written back into `cval`/`cval_raw`: the terminators
/// force-tag every edge arg, so an untagged param would be retagged on every
/// edge; the tagged form stays in `cval` and flows through an edge unchanged.
/// (`guard_fixnum`'s own `is_known_fixnum` still covers a retagged result.)
pub(crate) fn mir_as_raw(
    fb: &mut FunctionBuilder,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    raw_twin: &mut [Option<ClifValue>],
    proven_fixnum: &[bool],
    v: mir::MirValue,
    deopt: Block,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        return Ok(cv);
    }
    if let Some(t) = raw_twin[i] {
        return Ok(t);
    }
    if !proven_fixnum[i] {
        guard_fixnum(fb, deopt, cv, &HashSet::new());
    }
    UNTAG_COUNT.with(|c| c.set(c.get() + 1));
    let t = sshr_imm_p(fb, cv, FIXNUM_SHIFT as i64);
    raw_twin[i] = Some(t);
    Ok(t)
}

/// Get MIR value `v` as a TAGGED `Value` (for boundaries: returns, predicates,
/// car/cdr, cross-block block args). Retags a raw fixnum; passes a tagged value
/// through unchanged.
pub(crate) fn mir_as_tagged(
    fb: &mut FunctionBuilder,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    v: mir::MirValue,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        Ok(retag_fixnum(fb, cv))
    } else {
        Ok(cv)
    }
}

/// Force MIR value `v` to its TAGGED form IN PLACE (mutating `cval`/`cval_raw`),
/// returning the tagged value. Use before a call, an allocation, an edge or a
/// deopt snapshot: those are the points where a value reaches a GC-VISIBLE
/// STORE (the call-args slot, the root window, a cons cell, the spill buffer)
/// or a block boundary, and a raw i64 stored there would be traced as a
/// tagged pointer (a raw `3` has bits `0b011` == TAG_CONS -> a bogus rooted
/// cons -> UAF). The collector uses exact roots only, so a raw i64 that stays
/// in a register across a shim is invisible to it and needs nothing. Unlike
/// [`mir_as_tagged`] (which retags WITHOUT writing back), this clears the raw
/// mask so every LATER use and every deopt-framestate snapshot sees the
/// tagged form — no stale raw alias survives the safepoint. The MIR analogue
/// of the baseline's `stack_force_tagged`.
pub(crate) fn mir_force_tagged(
    fb: &mut FunctionBuilder,
    cval: &mut [Option<ClifValue>],
    cval_raw: &mut [bool],
    v: mir::MirValue,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        let tagged = retag_fixnum(fb, cv);
        cval[i] = Some(tagged);
        cval_raw[i] = false;
        Ok(tagged)
    } else {
        Ok(cv)
    }
}

/// Root one live residual `v` (already tagged) across a GC safepoint, inlining
/// the `is_heap_object` tag test so a non-heap value (fixnum or symbol) skips
/// the `neovm_jit_gc_push` shim CALL entirely at run time.
///
/// Used for residuals whose MIR type is `Unknown`/`Any` — not provably
/// immediate (those skip the push at *compile* time, [`LispType::never_needs_gc_root`])
/// and not provably heap (those get an unconditional push,
/// [`LispType::provably_heap`]). Empirically most such residuals resolve to
/// fixnum accumulators or symbol arguments at run time, so the branch is
/// overwhelmingly not-taken and predicts well.
///
/// CORRECTNESS: the emitted test skips the push ONLY for values the tag layout
/// guarantees are non-heap. Every non-heap `Value` is either a fixnum
/// (`bits & FIXNUM_CHECK_MASK == FIXNUM_CHECK_VALUE`) or a symbol
/// (`bits & TAG_MASK == TAG_SYMBOL`) — nil/t are symbols, chars are fixnums —
/// while every heap tag (cons/string/veclike/float) satisfies NEITHER predicate.
/// So `!(is_fixnum | is_symbol)` is an exact, layout-anchored `is_heap_object`
/// and can never drop a live heap root (which under GC would be a
/// use-after-free). The shim additionally re-checks `is_heap_object`, so even
/// the unused tag `0b001` (never produced) would be handled safely if pushed.
/// Is lever 1 (the inlined `is_heap_object` residual-rooting tag test) enabled?
/// Default yes; `NEOVM_JIT_LEVER1=off` reverts residual rooting to the pre-lever-1
/// behavior — an UNCONDITIONAL `neovm_jit_gc_push` per residual, with no
/// compile-time non-heap-constant skip — so lever 1's per-call effect can be
/// A/B-measured against the old code in a SINGLE build (pair with a call-heavy
/// bench like `jit_bench_fib`, whose recursive-call residual is a fixnum that
/// lever 1 skips at run time). Cached once per process.
pub(crate) fn jit_lever1_on() -> bool {
    use std::sync::OnceLock;
    pub(crate) static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_LEVER1").as_deref() != Ok("off"))
}

/// Bracketing state for residual-root windows: `base == None` means the
/// site emitted no rooting at all (statically empty to-root set); otherwise
/// the frame-base `jit_root_stack_top` value loaded before the stores, which
/// the post-call helper writes back.
#[derive(Clone, Copy)]
pub(crate) struct CondRoots {
    pub(crate) base: Option<ClifValue>,
}

impl CondRoots {
    pub(crate) const NONE: Self = Self { base: None };
}

/// Compile-time byte offsets of the [`Context`] JIT root-window mirror fields
/// generated code reads/writes ((ptr, top, cap)).
pub(crate) fn ctx_rootwin_offsets() -> (i32, i32, i32) {
    (
        core::mem::offset_of!(Context, jit_root_stack_ptr) as i32,
        core::mem::offset_of!(Context, jit_root_stack_top) as i32,
        core::mem::offset_of!(Context, jit_root_stack_cap) as i32,
    )
}

/// Store `to_root` into the ctx residual-root window at `[top..top+N)` and
/// bump `top`, returning the saved frame base for the post-call restore.
///
/// This replaces the gc_save / gc_push_many / gc_restore shim trio (~3 calls
/// plus per-value pushes, measured at ~123 Ir/call with heap residuals): on
/// the non-grow path it is two field loads, one compare, N+1 stores and no
/// calls. `top` is invariant between sites (every site restores it), so the
/// fresh load here always sees the frame base; nested calls stack naturally.
/// Slots below the stack's length always hold valid tagged Values
/// (NIL-initialized, only ever overwritten by these tagged stores), so the
/// tracer's `0..top` walk never sees garbage and stale slots merely
/// over-retain, exactly like interpreter operand-stack residue.
pub(crate) fn emit_root_window_stores(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    to_root: &[ClifValue],
) -> ClifValue {
    debug_assert!(
        to_root
            .iter()
            .all(|&v| iconst_bits(fb, v) != Some(UNBOXED_FLOAT_TAG_WORD)),
        "an unboxed-float tag word must never be published as a root"
    );
    let (off_ptr, off_top, off_cap) = ctx_rootwin_offsets();
    let vmctx = fb.use_var(rt.vmctx_var);
    if let Some(h) = rt.rootwin {
        // Hoisted form (see `HoistedRootWin`): the capacity was checked at
        // entry for the largest window; only the buffer pointer can have
        // moved since (a nested grow), so reload it and store.
        if cfg!(debug_assertions) {
            // The invariant the hoist rests on: `top` is back at the frame
            // base whenever a site runs. Debug builds trap if it is not.
            let top = fb
                .ins()
                .load(types::I64, MemFlagsData::trusted(), vmctx, off_top);
            let moved = fb.ins().icmp(IntCC::NotEqual, top, h.base);
            fb.ins()
                .trapnz(moved, cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
        // Which slots already hold their value (see `RootWinCarry`).
        let (ptr, slot0) = ROOTWIN_CARRY.with(|c| {
            let mut c = c.borrow_mut();
            let needs_store = to_root
                .iter()
                .enumerate()
                .any(|(i, &v)| c.stored.get(i).copied().flatten() != Some(v));
            let addr = needs_store.then(|| {
                let ptr = fb
                    .ins()
                    .load(rt.ptr_ty, MemFlagsData::trusted(), vmctx, off_ptr);
                (ptr, fb.ins().iadd(ptr, h.byte_off))
            });
            for (i, &v) in to_root.iter().enumerate() {
                if c.stored.get(i).copied().flatten() == Some(v) {
                    c.elided += 1;
                    continue;
                }
                let (_, slot0) = addr.expect("a differing slot implies the address");
                fb.ins()
                    .store(MemFlagsData::trusted(), v, slot0, (i * 8) as i32);
                c.emitted += 1;
                if c.stored.len() <= i {
                    c.stored.resize(i + 1, None);
                }
                c.stored[i] = Some(v);
            }
            // Slots at or above this site's count may be clobbered by the
            // nested activation during the call.
            c.stored.truncate(to_root.len());
            addr.unwrap_or((vmctx, vmctx))
        });
        let _ = (ptr, slot0);
        let need = iadd_imm_p(fb, h.base, to_root.len() as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), need, vmctx, off_top);
        return h.base;
    }
    let base = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), vmctx, off_top);
    let need = iadd_imm_p(fb, base, to_root.len() as i64);
    let cap = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), vmctx, off_cap);
    let fits = fb.ins().icmp(IntCC::UnsignedLessThanOrEqual, need, cap);
    let grow_blk = fb.create_block();
    let store_blk = fb.create_block();
    fb.ins().brif(fits, store_blk, &[], grow_blk, &[]);
    fb.switch_to_block(grow_blk);
    fb.seal_block(grow_blk);
    fb.ins().call(rt.refs.rootwin_grow, &[vmctx, need]);
    fb.ins().jump(store_blk, &[]);
    fb.switch_to_block(store_blk);
    fb.seal_block(store_blk);
    // Re-load the (possibly regrown) buffer pointer AFTER the capacity gate.
    let ptr = fb
        .ins()
        .load(rt.ptr_ty, MemFlagsData::trusted(), vmctx, off_ptr);
    let byte_off = ishl_imm_p(fb, base, 3);
    let slot0 = fb.ins().iadd(ptr, byte_off);
    for (i, &v) in to_root.iter().enumerate() {
        fb.ins()
            .store(MemFlagsData::trusted(), v, slot0, (i * 8) as i32);
    }
    fb.ins()
        .store(MemFlagsData::trusted(), need, vmctx, off_top);
    base
}

pub(crate) fn emit_cond_residual_roots_pre(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    values: &[ClifValue],
) -> CondRoots {
    let on = jit_lever1_on();
    let mut to_root: Vec<ClifValue> = Vec::with_capacity(values.len());
    for &v in values {
        // A provably non-heap immediate needs no root: a constant, or a value
        // the SSA shows is `(x << >=2) | 0b10` — a tagged fixnum, never a
        // pointer (`is_known_fixnum`, the same proof the guard elision uses).
        if on && (is_nonheap_const(fb, v) || is_known_fixnum(fb, v)) {
            continue;
        }
        to_root.push(v);
    }
    if to_root.is_empty() {
        // This site stores nothing, yet its shim may run a nested activation
        // with `top` at the frame base, which can overwrite EVERY slot: the
        // record must be truncated to this site's count, zero (`RootWinCarry`).
        rootwin_carry_reset();
        return CondRoots::NONE;
    }
    CondRoots {
        base: Some(emit_root_window_stores(fb, rt, &to_root)),
    }
}

pub(crate) fn emit_cond_residual_roots_post(fb: &mut FunctionBuilder, rt: &RtCtx, cr: CondRoots) {
    let Some(base) = cr.base else {
        return;
    };
    // Pop the site's residual window: top back to the frame base.
    let (_, off_top, _) = ctx_rootwin_offsets();
    let vmctx = fb.use_var(rt.vmctx_var);
    fb.ins()
        .store(MemFlagsData::trusted(), base, vmctx, off_top);
}

/// The deopt landing block for a guard-emitting MIR inst. In a CALL-BEARING body
/// (`precise`), every guard gets a fresh PER-SITE STATUS_DEOPT_AT block capturing
/// the inst's pre-op operand stack from `inst.pre_stack` (snapshotted EAGERLY
/// through `cval`/`cval_raw`, because a later call force-tags residual slots and
/// would otherwise corrupt a pre-call guard's raw mask) — NEVER rerun-from-start,
/// which would re-execute a call's side effect (the loop-back-edge hole the
/// adversarial critique caught). In a pure body it is the shared rerun-from-start
/// block (STATUS_DEOPT), created lazily.
/// `NEOVM_JIT_MIR_OPAQUE=0`/`off`: refuse every shim-lowered op in the MIR
/// tier (the pre-adapter behaviour — the single-build A/B for the port).
pub(crate) fn mir_opaque_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_MIR_OPAQUE").as_deref(),
            Ok("0") | Ok("off")
        )
    })
}

/// The variant name of a bytecode op (`VarRef(3)` -> `VarRef`), the key the
/// MIR bail census groups by.
fn op_variant_name(op: &Op) -> String {
    let name = format!("{op:?}");
    name.split(['(', '{', ' '])
        .next()
        .unwrap_or("?")
        .to_string()
}

/// Lower one shim-using MIR instruction through the BASELINE's per-op emitter
/// (`lower_simple_op`): the adapter that lets an `Opaque` op — a variable
/// read/write, a builtin, a list op — stay in the MIR tier instead of bailing
/// the whole body to the baseline.
///
/// The emitters work on an ad-hoc operand stack of tagged CLIF values:
///
/// * A SAFEPOINT op (`MirOp::Opaque`) sees the full pre-op stack
///   (`inst.pre_stack`, the accurate framestate), force-tagged with write-back
///   so no raw alias survives the shim call; the emitter pops its operands off
///   the top (which `build_mir` guarantees are the op's `args`, checked here),
///   roots the residual below them across the call, and its status branch
///   lands in the shared `signal_exit`. `precise` is on for such a body, so a
///   later guard deopts at its own pc — never rerun-from-start past the op's
///   side effect.
/// * A NON-safepoint op (`Eq`, `symbolp`/`integerp`/`numberp`: context-free
///   shims that never allocate, GC or signal) sees ONLY its operands, as
///   tagged copies without write-back, so a raw accumulator stays raw and an
///   INLINED predicate (whose `pre_stack` is the call site's) reads the right
///   operand.
///
/// No adapter-reachable arm queues a deopt or a handler dispatch (asserted).
/// Ordinary call sites share the baseline's guarded shims; inline arithmetic
/// remains outside this adapter. A MIR leaf has no handlers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_mir_inst_via_baseline(
    fb: &mut FunctionBuilder,
    inst: &mir::MirInst,
    op: &Op,
    m: &mir::MirFunction,
    cval: &mut [Option<ClifValue>],
    cval_raw: &mut [bool],
    cons_repl: &[Option<(mir::MirValue, mir::MirValue)>],
    rt: &RtCtx,
    pending: &mut Vec<PendingDeopt>,
    signal_exit: &mut Option<Block>,
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
    aot: bool,
    max_call_args: usize,
    spec: Option<(u32, u64, i64, usize, SpecCalleeKind)>,
) -> Result<(), CompileError> {
    use mir::MirOp;
    let bail = |key: String| {
        super::super::stats::record_mir_bail(key);
        Err(CompileError::UnsupportedOp("mir-pure-shim-op"))
    };
    if !mir_opaque_enabled() {
        return bail(format!("opaque:{}", op_variant_name(op)));
    }
    if spec.is_some_and(|(_, _, _, _, kind)| {
        aot || matches!(kind, SpecCalleeKind::ArithIntrinsic { .. })
    }) {
        return bail("adapter:unqualified-call-spec".to_string());
    }
    // A bind/unwind frame needs `has_binds` on the leaf and the native entry's
    // bind-frame arming (`invoke_native`); the MIR leaf has neither.
    if matches!(
        op,
        Op::VarBind(_)
            | Op::Unbind(_)
            | Op::SaveCurrentBuffer
            | Op::SaveExcursion
            | Op::SaveRestriction
            | Op::UnwindProtectPop
    ) {
        return bail(format!("opaque-bind:{}", op_variant_name(op)));
    }
    // An op-symbol operand bakes a session SymId; the AOT reloc collector walks
    // only `MirOp::Const`, so such a body is not cross-session portable.
    if aot
        && matches!(
            op,
            Op::VarRef(_)
                | Op::VarSet(_)
                | Op::VarBind(_)
                | Op::CallBuiltin(..)
                | Op::CallBuiltinSym(..)
        )
    {
        return bail(format!("aot-opsym:{}", op_variant_name(op)));
    }
    let (needs, delta) = super::simple_effect(op)?;
    let produces = needs as i64 + delta;
    debug_assert!((0..=1).contains(&produces), "one result at most: {op:?}");
    // The MIR tier rejects Float-site bodies (`gate:float-site`), and the
    // adapter never lowers arithmetic, so no flonum is ever produced here.
    debug_assert!(
        !matches!(op, Op::Add | Op::Sub | Op::Mul | Op::Div),
        "the adapter is never handed an arithmetic op: {op:?}"
    );
    let args: Vec<mir::MirValue> = match &inst.op {
        MirOp::Opaque { args, .. } => args.clone(),
        MirOp::Eq(a, b) => vec![*a, *b],
        MirOp::Pred(_, a) => vec![*a],
        other => unreachable!("not an adapter op: {other:?}"),
    };
    debug_assert_eq!(
        args.len(),
        needs,
        "operand count follows simple_effect: {op:?}"
    );
    let safepoint = matches!(inst.op, MirOp::Opaque { .. });
    debug_assert!(
        !safepoint || needs <= max_call_args,
        "the call-args slot ({max_call_args} words) holds this op's {needs} operands: {op:?}"
    );
    let mut stack: Vec<ClifValue> = if safepoint {
        let base = inst.pre_stack.len();
        if base < needs || inst.pre_stack[base - needs..] != args[..] {
            // Only an inlined Opaque could reach here (its pre_stack is the
            // call site's), and `callee_inlinable` admits none: a tripwire.
            return bail("adapter:operand-mismatch".to_string());
        }
        let mut v = Vec::with_capacity(base);
        for &pv in &inst.pre_stack {
            debug_assert!(
                cons_repl[pv.0 as usize].is_none(),
                "no elided cons in an Opaque body's framestate"
            );
            v.push(mir_force_tagged(fb, cval, cval_raw, pv)?);
        }
        v
    } else {
        let mut v = Vec::with_capacity(needs);
        for &a in &args {
            v.push(mir_as_tagged(fb, cval, cval_raw, a)?);
        }
        v
    };
    let base_len = stack.len();
    // Everything is tagged already, so the emitter's own retag pass is a no-op.
    let mut reps: Vec<SlotRep> = vec![SlotRep::Tagged; base_len];
    let mut dispatch: Vec<PendingDispatch> = Vec::new();
    let deopts_before = pending.len();
    lower_simple_op(
        fb,
        inst.pc,
        pending,
        signal_exit,
        &m.constants,
        &mut stack,
        &mut reps,
        Some(rt),
        &[],
        &mut dispatch,
        spec.or_else(|| {
            super::named_builtin_call(op).map(|call| {
                // CBSym uses only the kind and the op's symbol. No epoch slot or
                // expected callee is needed, identically to the baseline/AOT path.
                (0, 0, 0, 0, call.kind)
            })
        }),
        op,
        &HashSet::new(),
        reloc_base,
        reloc_index,
        aot,
        None,
        None,
        0,
        None,
    )?;
    debug_assert!(
        dispatch.is_empty(),
        "a MIR leaf has no handlers to dispatch to"
    );
    debug_assert_eq!(
        pending.len(),
        deopts_before,
        "no adapter-reachable arm queues a deopt"
    );
    debug_assert_eq!(
        stack.len() as i64,
        base_len as i64 - needs as i64 + produces,
        "the emitter's stack effect is simple_effect's: {op:?}"
    );
    let r = inst.result.0 as usize;
    if produces == 1 {
        cval[r] = stack.pop();
        cval_raw[r] = false;
        debug_assert!(
            matches!(reps.pop(), None | Some(SlotRep::Tagged)),
            "results are tagged"
        );
    }
    Ok(())
}

/// An inlined call in a reentrant body must observe state changed by the
/// preceding callback or service poll. These loads are deliberately mutable:
/// they cannot be forwarded across a call. Failure resumes the original call,
/// where the interpreter handles redefinition, debugging, quit and depth.
fn emit_mir_inline_entry_guard(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    epoch: u64,
    deopt: Block,
) -> Result<(), CompileError> {
    use crate::emacs_core::eval::runtime_projection::{
        CONTEXT_COMPILER_OVERRIDES_ACTIVE_OFFSET, CONTEXT_QUIT_FLAG_OFFSET,
        CONTEXT_QUIT_REQUESTED_OFFSET, CONTEXT_THROW_ON_INPUT_OFFSET, arc_atomic_bool_data_offset,
    };
    use crate::emacs_core::forward::LISP_BOOL_FWD_VALUE_OFFSET;
    use crate::emacs_core::symbol::{
        OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET, OBARRAY_FUNCTION_EPOCH_OFFSET,
    };
    let requested_off =
        arc_atomic_bool_data_offset().ok_or(CompileError::UnsupportedOp("inline-quit-layout"))?;
    let ob = core::mem::offset_of!(Context, obarray);
    let flags = MemFlagsData::trusted();
    let vmctx = fb.use_var(rt.vmctx_var);
    let current = fb.ins().load(
        types::I64,
        flags,
        vmctx,
        (ob + OBARRAY_FUNCTION_EPOCH_OFFSET) as i32,
    );
    let epoch_ok = icmp_imm_p(fb, IntCC::Equal, current, epoch as i64);
    let overrides = fb.ins().uload8(
        types::I64,
        flags,
        vmctx,
        CONTEXT_COMPILER_OVERRIDES_ACTIVE_OFFSET as i32,
    );
    let quit = fb
        .ins()
        .load(types::I64, flags, vmctx, CONTEXT_QUIT_FLAG_OFFSET as i32);
    let throw = fb.ins().load(
        types::I64,
        flags,
        vmctx,
        CONTEXT_THROW_ON_INPUT_OFFSET as i32,
    );
    let requested_arc = fb.ins().load(
        rt.ptr_ty,
        flags,
        vmctx,
        CONTEXT_QUIT_REQUESTED_OFFSET as i32,
    );
    let requested = fb
        .ins()
        .uload8(types::I64, flags, requested_arc, requested_off as i32);
    let signal_addr = fb.ins().iconst(
        rt.ptr_ty,
        crate::emacs_core::os_signal::pending_flag_addr() as i64,
    );
    let signal = fb.ins().uload8(types::I64, flags, signal_addr, 0);
    let tick_addr = fb.ins().iconst(
        rt.ptr_ty,
        crate::emacs_core::profiler::profiler_sample_due_addr() as i64,
    );
    let tick = fb.ins().uload8(types::I64, flags, tick_addr, 0);
    let fwd = fb.ins().load(
        rt.ptr_ty,
        flags,
        vmctx,
        (ob + OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET) as i32,
    );
    let armed = fb.ins().iconst(
        rt.ptr_ty,
        std::ptr::from_ref(&crate::emacs_core::forward::JIT_ALWAYS_TRUE_BOOL_FWD) as i64,
    );
    let resolved = icmp_imm_p(fb, IntCC::NotEqual, fwd, 0);
    let cell = fb.ins().select(resolved, fwd, armed);
    let debug = fb
        .ins()
        .uload8(types::I64, flags, cell, LISP_BOOL_FWD_VALUE_OFFSET as i32);
    let depth = fb.ins().load(
        rt.ptr_ty,
        flags,
        vmctx,
        core::mem::offset_of!(Context, depth) as i32,
    );
    let max_depth = fb.ins().load(
        rt.ptr_ty,
        flags,
        vmctx,
        core::mem::offset_of!(Context, max_depth) as i32,
    );
    let depth_ok = fb.ins().icmp(IntCC::UnsignedLessThan, depth, max_depth);
    debug_assert_eq!(Value::NIL.bits(), 0);
    let set = fb.ins().bor(overrides, quit);
    let set = fb.ins().bor(set, throw);
    let set = fb.ins().bor(set, requested);
    let set = fb.ins().bor(set, signal);
    let set = fb.ins().bor(set, tick);
    let set = fb.ins().bor(set, debug);
    let clear = icmp_imm_p(fb, IntCC::Equal, set, 0);
    let valid = fb.ins().band(epoch_ok, clear);
    let valid = fb.ins().band(valid, depth_ok);
    emit_guard(fb, deopt, valid);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn mir_deopt_block(
    fb: &mut FunctionBuilder,
    precise: bool,
    inst: &mir::MirInst,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    cons_repl: &[Option<(mir::MirValue, mir::MirValue)>],
    rt: Option<&RtCtx>,
    shared: &mut Option<Block>,
    pending: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    if precise {
        let mut stack = Vec::with_capacity(inst.pre_stack.len());
        let mut reps = Vec::with_capacity(inst.pre_stack.len());
        let mut cons_rebuilds: Vec<ConsReconstruction> = Vec::new();
        let mut rebuilt: Option<HashMap<mir::MirValue, usize>> = None;
        for (slot, &v) in inst.pre_stack.iter().enumerate() {
            if let Some((car, cdr)) = cons_repl[v.0 as usize] {
                let value = |v: mir::MirValue| -> Result<(ClifValue, bool), CompileError> {
                    Ok((
                        cval[v.0 as usize].ok_or(CompileError::BadOperand)?,
                        cval_raw[v.0 as usize],
                    ))
                };
                let car = value(car)?;
                let cdr = value(cdr)?;
                // This placeholder is replaced before spilling. Repeated
                // references to the same virtual cons must get ONE object.
                stack.push(car.0);
                reps.push(SlotRep::Tagged);
                let rebuilt = rebuilt.get_or_insert_with(HashMap::new);
                if let Some(&i) = rebuilt.get(&v) {
                    cons_rebuilds[i].slots.push(slot);
                } else {
                    rebuilt.insert(v, cons_rebuilds.len());
                    cons_rebuilds.push(ConsReconstruction {
                        slots: vec![slot],
                        car,
                        cdr,
                        allocator: rt
                            .ok_or(CompileError::UnsupportedOp("mir-cons-deopt-no-rt"))?
                            .refs
                            .cons,
                    });
                }
            } else {
                stack.push(cval[v.0 as usize].ok_or(CompileError::BadOperand)?);
                reps.push(SlotRep::raw_if(cval_raw[v.0 as usize]));
            }
        }
        let block = deopt_site(fb, inst.pc, 0, &stack, &reps, pending);
        let site = pending.last_mut().expect("deopt_site queues one site");
        debug_assert!(site.region.is_none(), "MIR carries its own call framestate");
        site.cons_rebuilds = cons_rebuilds;
        Ok(block)
    } else {
        Ok(*shared.get_or_insert_with(|| fb.create_block()))
    }
}

/// Raw fixnum add/sub: operands and result are untagged i64 (no untag/retag), with
/// the interpreter's fixnum-range check (deopt on overflow). The unboxed analogue
/// of [`lower_fixnum_binop`].
pub(crate) fn raw_fixnum_addsub(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_sub: bool,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let res = if is_sub {
        fb.ins().isub(av, bv)
    } else {
        fb.ins().iadd(av, bv)
    };
    // A signed 62-bit payload fits iff its upper three bits are all equal:
    // arithmetic res >> 61 is -1 or 0. Biasing by one makes that a single
    // unsigned comparison, without loading both fixnum bounds. The result
    // stays raw for subsequent arithmetic and the existing deopt snapshot.
    let high = sshr_imm_p(fb, res, (i64::BITS - FIXNUM_SHIFT - 1) as i64);
    let biased = iadd_imm_p(fb, high, 1);
    let in_range = icmp_imm_p(fb, IntCC::UnsignedLessThanOrEqual, biased, 1);
    emit_guard(fb, deopt, in_range);
    res
}

/// Raw fixnum 1+/1-/negate: untagged in, untagged out, with the interpreter's
/// boundary check (deopt on the single out-of-range input). Unboxed analogue of
/// [`lower_fixnum_unop`].
pub(crate) fn raw_fixnum_unop(
    fb: &mut FunctionBuilder,
    deopt: Block,
    kind: UnaryKind,
    av: ClifValue,
) -> ClifValue {
    let bound = match kind {
        UnaryKind::Add1 => Value::MOST_POSITIVE_FIXNUM,
        UnaryKind::Sub1 | UnaryKind::Negate => Value::MOST_NEGATIVE_FIXNUM,
    };
    let in_range = icmp_imm_p(fb, IntCC::NotEqual, av, bound);
    emit_guard(fb, deopt, in_range);
    match kind {
        UnaryKind::Add1 => iadd_imm_p(fb, av, 1),
        UnaryKind::Sub1 => iadd_imm_p(fb, av, -1),
        UnaryKind::Negate => fb.ins().ineg(av),
    }
}

/// Raw fixnum `*`: untagged in/out, widen to i128 for the product + the
/// interpreter's fixnum-range check (deopt on overflow). Unboxed analogue of
/// [`lower_fixnum_mul`].
pub(crate) fn raw_fixnum_mul(
    fb: &mut FunctionBuilder,
    deopt: Block,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let a128 = fb.ins().sextend(types::I128, av);
    let b128 = fb.ins().sextend(types::I128, bv);
    let prod = fb.ins().imul(a128, b128);
    let lo = fb.ins().iconst(types::I64, Value::MOST_NEGATIVE_FIXNUM);
    let hi = fb.ins().iconst(types::I64, Value::MOST_POSITIVE_FIXNUM);
    let lo128 = fb.ins().sextend(types::I128, lo);
    let hi128 = fb.ins().sextend(types::I128, hi);
    let ge = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, prod, lo128);
    let le = fb.ins().icmp(IntCC::SignedLessThanOrEqual, prod, hi128);
    let in_range = fb.ins().band(ge, le);
    emit_guard(fb, deopt, in_range);
    fb.ins().ireduce(types::I64, prod)
}

/// Raw fixnum `/`/`%`: untagged in/out. Deopts on a zero divisor (interpreter
/// signals arith-error). Operands are <= 61-bit so `sdiv`/`srem` cannot trap.
/// For `/`, the only out-of-fixnum-range result is MOST_NEGATIVE_FIXNUM / -1 (a
/// wrap in the interpreter); deopt on it for parity rather than keep an
/// out-of-range raw value (`%` is always in range). Unboxed analogue of
/// [`lower_fixnum_divrem`].
pub(crate) fn raw_fixnum_divrem(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_rem: bool,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let nonzero = icmp_imm_p(fb, IntCC::NotEqual, bv, 0);
    emit_guard(fb, deopt, nonzero);
    if is_rem {
        fb.ins().srem(av, bv)
    } else {
        let res = fb.ins().sdiv(av, bv);
        let ge = icmp_imm_p(
            fb,
            IntCC::SignedGreaterThanOrEqual,
            res,
            Value::MOST_NEGATIVE_FIXNUM,
        );
        let le = icmp_imm_p(
            fb,
            IntCC::SignedLessThanOrEqual,
            res,
            Value::MOST_POSITIVE_FIXNUM,
        );
        let in_range = fb.ins().band(ge, le);
        emit_guard(fb, deopt, in_range);
        res
    }
}

/// Raw fixnum `max`/`min`: untagged in/out, a branchless `select` of the two
/// already-range-valid operands (no overflow, no deopt of its own).
pub(crate) fn raw_fixnum_maxmin(
    fb: &mut FunctionBuilder,
    is_min: bool,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let cc = if is_min {
        IntCC::SignedLessThan
    } else {
        IntCC::SignedGreaterThan
    };
    let cond = fb.ins().icmp(cc, av, bv);
    fb.ins().select(cond, av, bv)
}

/// What a MIR leaf needs from the lowering, decided once from the MIR and
/// shared by the JIT wrapper ([`lower_mir_with_plan`]), the AOT wrapper
/// (`aot::define_leaf_into_module`) and the tier gate — one set of facts, so
/// the descriptor a `.so` carries, the deopt buffers a leaf is given and the
/// code emitted for it can never disagree.
pub(crate) struct MirLeafPlan {
    /// Any [`mir::MirOp::Opaque`] — an op lowered through the baseline's
    /// shim-calling emitters (a call, a variable op, a builtin, ...).
    #[cfg(test)]
    pub(crate) has_opaque: bool,
    /// A call without a qualified shared specialization. These bodies retain
    /// the baseline tier; Apply and inline arithmetic remain unqualified.
    pub(crate) has_generic_call: bool,
    /// An opaque op without a shared ordinary-call or read-only named-builtin
    /// fast path. Other adapter families retain conservative loop admission.
    /// Ordinary calls and all generic fallbacks retain their full effects.
    pub(crate) has_unqualified_adapter: bool,
    pub(crate) has_named_builtin: bool,
    /// Stable JIT-only call state. The plan owns slots while code is emitted;
    /// the leaf takes ownership before any baked pointer can be executed.
    spec_sites: HashMap<usize, super::SpecSite>,
    spec_slots: Box<[super::SpecSlot]>,
    /// A loop (an edge to a block at or before its source).
    pub(crate) has_backedge: bool,
    /// Every guard deopts PRECISELY (`STATUS_DEOPT_AT`), never rerun-from-
    /// start: the baseline's own rule for any body with a status-shim site.
    /// `= has_opaque || has_backedge`: a loop poll can run post-gc-hook, so
    /// even an otherwise pure loop must not replay its work after a poll.
    pub(crate) precise: bool,
    /// The body needs vmctx + the shim scaffolding (`RtCtx`).
    pub(crate) needs_rt: bool,
    /// Block-local cons scalar replacement. Precise exits reconstruct virtual
    /// pairs; opaque safepoints and outgoing edges retain real stack values.
    pub(crate) cons_repl: Vec<Option<(mir::MirValue, mir::MirValue)>>,
    /// Words the call-args scratch slot must hold: the widest operand set any
    /// `Opaque` marshals (the baseline's arms store `needs` words at `i*8`).
    pub(crate) max_call_args: usize,
    /// The deepest pre-op operand stack: the precise-deopt spill size, and
    /// the widest residual any rooting site can need.
    pub(crate) max_depth: usize,
    /// `Opaque` ops whose emitter roots a residual in the Context root window
    /// (the baseline's `is_rooting_site_op`): calls, variable ops, builtins,
    /// list ops. A `Cons` is not one (its shim is context-free), so a
    /// cons-only body may still run with a null vmctx.
    pub(crate) rooting_sites: usize,
}

/// Which `Op::Call`/`Apply`/`CallBuiltinSym` a body still has after inlining
/// (`gate:generic-call`'s census key), or `""`.
pub(crate) fn mir_generic_call_kinds(m: &mir::MirFunction) -> String {
    use mir::MirOp;
    let mut kinds: Vec<&str> = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .filter_map(|i| match &i.op {
            MirOp::Opaque {
                op: Op::Call(_), ..
            } => Some("call"),
            MirOp::Opaque {
                op: Op::Apply(_), ..
            } => Some("apply"),
            MirOp::Opaque {
                op: Op::CallBuiltinSym(..),
                ..
            } => Some("cbsym"),
            _ => None,
        })
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds.join("+")
}

/// The adapter op that keeps a looping body out of the MIR tier
/// (`gate:loop-opaque`'s census key): the first one in a block a back edge
/// can reach, by variant name.
pub(crate) fn mir_loop_adapter_op(m: &mir::MirFunction) -> String {
    use mir::MirOp;
    let loop_head = m
        .blocks
        .iter()
        .filter_map(|b| {
            mir::successor_edges(&b.term)
                .map(|(t, _)| m.blocks[t.0 as usize].bytecode_pc)
                .filter(|&pc| pc <= b.bytecode_pc)
                .min()
        })
        .min();
    let Some(head) = loop_head else {
        return String::new();
    };
    m.blocks
        .iter()
        .filter(|b| b.bytecode_pc >= head)
        .flat_map(|b| b.insts.iter())
        .find_map(|i| match &i.op {
            MirOp::Opaque { op, .. } => Some(op_variant_name(op)),
            MirOp::Eq(..) => Some("Eq".to_string()),
            MirOp::Pred(
                mir::PredKind::Symbolp | mir::PredKind::Integerp | mir::PredKind::Numberp,
                _,
            ) => Some("Pred".to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Decide a MIR leaf's [`MirLeafPlan`].
pub(crate) fn plan_mir_leaf(m: &mir::MirFunction) -> MirLeafPlan {
    plan_mir_leaf_with_spec(m, HashMap::new(), Box::from([]))
}

/// Reuse the baseline's site selection on the original bytecode. Inlining
/// only splices pure callees, so every remaining Opaque Call keeps its original
/// pc. The emitter still independently proves/checks the actual callee value.
pub(super) fn plan_mir_leaf_for_jit(
    m: &mir::MirFunction,
    ops: &[Op],
    obarray: Option<&Obarray>,
) -> MirLeafPlan {
    let calls: HashSet<usize> = m
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|i| {
            matches!(
                i.op,
                mir::MirOp::Opaque {
                    op: Op::Call(_),
                    ..
                }
            )
            .then_some(i.pc)
        })
        .collect();
    let Some(ob) = obarray.filter(|_| !calls.is_empty()) else {
        return plan_mir_leaf(m);
    };
    // Sharing dispatch alone does not make a call-dominated wrapper profitable.
    // Preserve the existing deferral unless MIR already eliminated a callee.
    if m.inline_epoch.is_none() && !super::body_is_jit_profitable(ops, &m.constants) {
        return plan_mir_leaf(m);
    }
    let leaders: Vec<_> = m.blocks.iter().map(|b| b.bytecode_pc).collect();
    let mut sites = super::find_spec_sites(ops, &m.constants, &leaders, ob, true);
    sites.retain(|pc, site| {
        calls.contains(pc) && !matches!(site.kind, SpecCalleeKind::ArithIntrinsic { .. })
    });
    // Filtering out inlined calls, named sites and arithmetic leaves holes in
    // the baseline's slot numbering. Assign dense indices before taking addresses.
    for (slot, site) in sites.values_mut().enumerate() {
        site.slot = slot;
    }
    let slots = (0..sites.len())
        .map(|_| super::SpecSlot::at_epoch(ob.function_epoch()))
        .collect();
    plan_mir_leaf_with_spec(m, sites, slots)
}

fn plan_mir_leaf_with_spec(
    m: &mir::MirFunction,
    spec_sites: HashMap<usize, super::SpecSite>,
    spec_slots: Box<[super::SpecSlot]>,
) -> MirLeafPlan {
    use mir::{MirOp, PredKind as MP};
    let insts = || m.blocks.iter().flat_map(|b| b.insts.iter());
    let has_opaque = insts().any(|i| matches!(i.op, MirOp::Opaque { .. }));
    let has_generic_call = insts().any(|i| match &i.op {
        MirOp::Opaque {
            op: Op::Call(_), ..
        } => !spec_sites.contains_key(&i.pc),
        MirOp::Opaque {
            op: Op::Apply(_), ..
        } => true,
        MirOp::Opaque {
            op: op @ Op::CallBuiltinSym(..),
            ..
        } => super::named_builtin_call(op).is_none(),
        _ => false,
    });
    let has_named_builtin = insts().any(|i| {
        matches!(
            &i.op, MirOp::Opaque { op, .. } if super::named_builtin_call(op).is_some()
        )
    });
    let has_unqualified_adapter = insts().any(|i| {
        matches!(
            &i.op, MirOp::Opaque { op, .. } if !spec_sites.contains_key(&i.pc) && !super::named_builtin_call(op)
                .is_some_and(|call| call.fast_effects.is_read_only())
        )
    });
    // Any op that goes through a runtime shim: an `Opaque`, an `Eq` (the
    // symbols-with-position slow path), a `symbolp`/`integerp`/`numberp`.
    let has_adapter_site = insts().any(|i| {
        matches!(
            i.op,
            MirOp::Opaque { .. }
                | MirOp::Eq(..)
                | MirOp::Pred(MP::Symbolp | MP::Integerp | MP::Numberp, _)
        )
    });
    let has_backedge = m.blocks.iter().any(|b| {
        mir::successor_edges(&b.term)
            .any(|(t, _)| m.blocks[t.0 as usize].bytecode_pc <= b.bytecode_pc)
    });
    let precise = has_opaque || has_backedge;
    let cons_repl = mir::cons_scalar_repl_targets(m);
    let has_escaping_cons = insts()
        .any(|i| matches!(i.op, MirOp::Cons(..)) && cons_repl[i.result.0 as usize].is_none());
    let max_call_args = insts()
        .filter_map(|i| match &i.op {
            MirOp::Opaque { op, .. } => super::simple_effect(op).ok().map(|(needs, _)| needs),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let max_depth = insts()
        .map(|i| i.pre_stack.len())
        .chain(m.blocks.iter().map(|b| b.params.len()))
        .chain(
            m.blocks
                .iter()
                .flat_map(|b| mir::successor_edges(&b.term).map(|(_, args)| args.len())),
        )
        .max()
        .unwrap_or(0);
    let rooting_sites = insts()
        .filter(|i| matches!(&i.op, MirOp::Opaque { op, .. } if super::is_rooting_site_op(op)))
        .count();
    MirLeafPlan {
        #[cfg(test)]
        has_opaque,
        has_generic_call,
        has_unqualified_adapter,
        has_named_builtin,
        spec_sites,
        spec_slots,
        has_backedge,
        precise,
        needs_rt: has_adapter_site || has_escaping_cons || has_backedge,
        cons_repl,
        max_call_args,
        max_depth,
        rooting_sites,
    }
}

/// **MIR Tier-2 lowering.** Lower a [`mir::MirFunction`] to a [`CompiledLeaf`] by
/// driving CLIF emission from the MIR instead of a bytecode walk. Wired into
/// `compile_bytecode_function_inner` as the live optimizing tier. A *pure* body
/// (arithmetic / comparisons / type predicates / car-cdr / stack — no shim-using
/// ops) needs no vmctx and reruns the interpreter from the start on a failing
/// guard (sound: no side effect precedes any guard). A looping or call-bearing
/// body threads vmctx + runtime shims and uses precise deopt: a back-edge poll
/// can collect and run Lisp hooks, so restarting across it is not sound.
///
/// Uses CLIF **block parameters** as the SSA phis — each MIR block becomes a
/// CLIF block whose params are its entry operand stack, and terminator edges
/// pass the live stack as block arguments. Validated by differential tests
/// against the interpreter and the force-deopt gate.
#[cfg(test)]
pub(crate) fn lower_mir_pure(m: &mir::MirFunction) -> Result<CompiledLeaf, CompileError> {
    lower_mir_with_plan(m, plan_mir_leaf(m))
}

pub(super) fn lower_mir_with_plan(
    m: &mir::MirFunction,
    plan: MirLeafPlan,
) -> Result<CompiledLeaf, CompileError> {
    use mir::MirOp;

    // Loops and shim-lowered ops route EVERY guard to STATUS_DEOPT_AT, so a
    // deopt never replays work across a call or a poll's Lisp hooks. Virtual
    // conses are reconstructed on cold exits, preserving frame aliases. They
    // cannot cross a safepoint or outgoing edge. Cons allocation itself never
    // collects, so reconstruction needs no intermediate GC root publication.

    // --- JIT-only module prologue (the wrapper). ----------------------------
    // The three ObjectModule-incompatible seams that stay here (and out of the
    // generic build fn `build_mir_leaf_fn`): `builder.symbol(...)` bakes the shim
    // host addresses (AOT replaces this with `Linkage::Import` + dlopen);
    // `JITModule::new` (AOT: `ObjectModule::new`); `finalize_definitions` +
    // `get_finalized_function` below (AOT: `ObjectModule::finish()` + `dlsym`).
    let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
    if plan.needs_rt {
        // The shims the calls-slice + cons allocation reference; declare_rt_refs
        // declares the full import set but Cranelift resolves only referenced ones.
        // Every shim, from the one table (see `super::shims::JIT_SHIM_TABLE`).
        super::shims::register_shims(&mut builder);
    }
    let mut module = JITModule::new(builder);

    // Precise-deopt spill buffer + cells, sized to the deepest pre-op operand stack
    // (the framestate a post-call guard spills). Empty/inert for pure bodies (which
    // keep the rerun-from-start STATUS_DEOPT path).
    let deopt_spill: Box<[core::cell::Cell<i64>]> = if plan.precise {
        (0..plan.max_depth)
            .map(|_| core::cell::Cell::new(0))
            .collect()
    } else {
        Box::from([])
    };
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });

    // R1a: per-leaf heap-constant reloc vector — collect the DISTINCT heap-object
    // constants (deduped by tagged bits) so generated code loads each from
    // reloc_data[idx] instead of baking its heap pointer as an immediate (untraced
    // by the GC, unportable to an AOT .so). Fixnums + non-heap immediates (nil/t)
    // stay baked. Allocated here (before the FunctionBuilder) so reloc_data.as_ptr()
    // is stable when the loads bake its base address (same pattern as deopt_spill).
    let mut reloc_vals: Vec<Value> = Vec::new();
    let mut reloc_index: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for blk in &m.blocks {
        for inst in &blk.insts {
            if let MirOp::Const(v) = &inst.op {
                let bits = v.bits();
                if (bits & FIXNUM_CHECK_MASK) != FIXNUM_CHECK_VALUE
                    && v.is_heap_object()
                    && !reloc_index.contains_key(&bits)
                {
                    reloc_index.insert(bits, reloc_vals.len() as u32);
                    reloc_vals.push(*v);
                }
            }
        }
    }
    let reloc_data: Box<[Value]> = reloc_vals.into_boxed_slice();

    // The entry's declared name (see `lower_leaf_full_osr`).
    let label =
        super::super::stats::perf_map::active_label(super::super::stats::perf_map::LabelTier::Mir);
    let entry_name = label.as_deref().unwrap_or("__neovm_mir_leaf");
    #[cfg(test)]
    super::super::stats::perf_map::record_entry_name_for_test(entry_name);
    // Before the build: with entry counting on, the prologue bakes the
    // address of `obs.entries`.
    let mut obs = LeafObs::new(super::super::stats::entry_counting_enabled());

    // Build + define the leaf into the module via the module-generic seam
    // (`build_mir_leaf_fn`). The buffers are owned here and threaded in by
    // reference so their addresses (baked into the generated loads) stay stable
    // and so the wrapper can move them into the returned `CompiledLeaf`.
    let fid = build_mir_leaf_fn(
        &mut module,
        m,
        &deopt_spill,
        &deopt_meta,
        &reloc_data,
        &reloc_index,
        &plan,
        entry_name,
        Linkage::Local,
        /*aot=*/ false,
        obs.entry_counter(),
    )?;

    // --- JIT-only module epilogue (the wrapper). ----------------------------
    module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
    let entry = module.get_finalized_function(fid);
    super::super::stats::asm_dump::flush(&super::super::stats::asm_dump::AsmLeafInfo {
        tier: super::super::stats::perf_map::LabelTier::Mir,
        entry_name,
        entry,
        regalloc: active_regalloc_choice().name(),
        clif_insts: super::clif_size_now().0,
    });
    obs.label = label.map(String::into_boxed_str);

    Ok(CompiledLeaf {
        tier: LeafTier::Mir,
        regalloc: active_regalloc_choice(),
        profit_gate_bypassed: super::profit_gate_bypassed_now(),
        call_heavy: super::call_heavy_now(),
        clif_insts: super::clif_size_now().0,
        clif_blocks: super::clif_size_now().1,
        arity: m.arity,
        required: m.arity,
        has_rest: false,
        has_binds: false,
        has_handlers: false,
        // Set by compile_bytecode_function_inner after a successful inline pass.
        inline_epoch: None,
        // A shim-bearing body runs a side effect ahead of its (precise) deopts, so
        // it must never rerun-from-start (the refuse-to-rerun guard).
        has_side_effects: plan.precise,
        // Baseline default; compile_bytecode_function_inner overrides with the
        // actual inlined-callee SymIds after the inline pass.
        inline_deps: Box::from([]),
        spec_slots: plan.spec_slots,
        spec_expected: Box::from([]),
        deopt_spill,
        deopt_meta,
        reloc_data,
        // JIT bakes its bases as iconst; the 4th entry arg is ignored.
        sidecar: None,
        // MIR leaves are only built for unpatched sources (see
        // compile_bytecode_function_inner).
        dynamic_prefix: 0,
        obs,
        entry,
        _backing: LeafBacking::Jit(module),
    })
}

/// The host ISA for JIT modules, with cranelift-jit's own flag defaults
/// (`use_colocated_libcalls=false`, `is_pic=false` — mirrored by the AOT
/// module builder, which flips only `is_pic`) plus ONE deliberate change:
/// the Cranelift IR **verifier** runs only in debug builds. Cranelift enables
/// it by default and `JITBuilder::new` inherited that, so every production
/// tier-up paid a full IR verification pass (~12% of the compile Ir on the
/// fontify sim's 352-op font-lock body). The verifier exists to catch
/// lowering bugs, which debug/test builds still do; release compiles are
/// trusted the same way a shipped compiler's are.
pub(crate) fn jit_isa()
-> Result<std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>, CompileError> {
    use cranelift_codegen::settings::{self, Configurable};
    let init_err = |e: String| CompileError::Backend(BackendError::ModuleInit(e));
    let mut flags = settings::builder();
    flags
        .set("use_colocated_libcalls", "false")
        .map_err(|e| init_err(e.to_string()))?;
    flags
        .set("is_pic", "false")
        .map_err(|e| init_err(e.to_string()))?;
    flags
        .set(
            "regalloc_algorithm",
            active_regalloc_choice().cranelift_setting(),
        )
        .map_err(|e| init_err(e.to_string()))?;
    if regalloc_checker_enabled() {
        flags
            .set("regalloc_checker", "true")
            .map_err(|e| init_err(e.to_string()))?;
    }
    flags
        .set(
            "enable_verifier",
            if cfg!(debug_assertions) {
                "true"
            } else {
                "false"
            },
        )
        .map_err(|e| init_err(e.to_string()))?;
    cranelift_native::builder()
        .map_err(|e| init_err(e.to_string()))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| init_err(e.to_string()))
}

/// Which regalloc2 allocator a Cranelift compile runs.
///
/// Measured on the same binary (2026-09-05, `tmp/rr/wf2/ab-regalloc.sh`):
/// `Fast` halves compile time (mean 399 → 209 µs, max 1.46 → 0.43 ms per
/// leaf — a keystroke-time stall) and cuts a compile-bound fixture by 11%,
/// but its code runs ~10% more instructions on the call-heavy benchmark. So
/// it is a per-compile choice (`choose_regalloc`), never a global one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegallocChoice {
    /// regalloc2's single-pass allocator (`fastalloc`): compiles fast.
    Fast,
    /// regalloc2's backtracking allocator (`ion`, Cranelift's default): the
    /// better code.
    Full,
}

impl RegallocChoice {
    /// The value of Cranelift's `regalloc_algorithm` setting.
    pub(crate) fn cranelift_setting(self) -> &'static str {
        match self {
            RegallocChoice::Fast => "single_pass",
            RegallocChoice::Full => "backtracking",
        }
    }

    /// The short name the JIT reports print (`fast`/`full`).
    pub(crate) fn name(self) -> &'static str {
        match self {
            RegallocChoice::Fast => "fast",
            RegallocChoice::Full => "full",
        }
    }
}

/// Why a compile is happening, which decides its allocator (see
/// [`choose_regalloc`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegallocPolicy {
    /// An entry tier-up: the body's shape decides.
    Auto,
    /// The full allocator regardless of shape: an OSR entry (a loop that is
    /// already running), or a re-tier of a leaf that proved hot.
    Full,
}

thread_local! {
    /// The allocator of the compile in progress on this thread. `Full`
    /// outside any [`RegallocScope`], so every path that does not opt in
    /// compiles exactly as before.
    static ACTIVE_REGALLOC: std::cell::Cell<RegallocChoice> =
        const { std::cell::Cell::new(RegallocChoice::Full) };
}

/// The allocator of the compile in progress (see [`ACTIVE_REGALLOC`]).
pub(crate) fn active_regalloc_choice() -> RegallocChoice {
    ACTIVE_REGALLOC.with(|c| c.get())
}

/// RAII scope: every ISA built inside it uses `choice`; the previous choice
/// is restored on drop (a nested compile restores its parent's).
pub(crate) struct RegallocScope(RegallocChoice);

impl RegallocScope {
    pub(crate) fn enter(choice: RegallocChoice) -> Self {
        Self(ACTIVE_REGALLOC.with(|c| c.replace(choice)))
    }
}

impl Drop for RegallocScope {
    fn drop(&mut self) {
        ACTIVE_REGALLOC.with(|c| c.set(self.0));
    }
}

/// `NEOVM_JIT_REGALLOC`: `single_pass` (`fast`) or `backtracking` (`full`)
/// forces one allocator for every JIT compile — the A/B knob. Unset (or any
/// other value) means the policy in [`choose_regalloc`].
pub(crate) fn forced_regalloc() -> Option<RegallocChoice> {
    use std::sync::OnceLock;
    static FORCED: OnceLock<Option<RegallocChoice>> = OnceLock::new();
    *FORCED
        .get_or_init(|| parse_regalloc_choice(std::env::var("NEOVM_JIT_REGALLOC").ok().as_deref()))
}

/// The accepted spellings of `NEOVM_JIT_REGALLOC`; anything else = policy.
pub(crate) fn parse_regalloc_choice(value: Option<&str>) -> Option<RegallocChoice> {
    match value.map(str::trim) {
        Some("single_pass" | "single-pass" | "fastalloc" | "fast") => Some(RegallocChoice::Fast),
        Some("backtracking" | "ion" | "full") => Some(RegallocChoice::Full),
        _ => None,
    }
}

/// The allocator policy: a forced choice wins; otherwise the full allocator
/// for a `Full` policy or a body that can run unboundedly per entry -- a back
/// edge, or a self-recursive call (see `regalloc_for_shape`) -- so its code
/// quality is worth the compile, and the fast one for
/// a straight-line or branchy body (bounded work per entry — and it re-tiers
/// to `Full` if the interpreter keeps entering it; see `retier_heat`).
pub(crate) fn choose_regalloc(
    forced: Option<RegallocChoice>,
    policy: RegallocPolicy,
    has_back_edge: bool,
    call_heavy: bool,
) -> RegallocChoice {
    if let Some(forced) = forced {
        return forced;
    }
    // A call-heavy body's runtime is its shim calls, whatever it does around
    // them; the full allocator would spend ~90% of a ~38M-instruction compile
    // (regalloc2 ion, org editing probe 2026-09-05) improving code that is
    // not where the time goes. Fast even when it loops, and never re-tiered.
    if call_heavy {
        return RegallocChoice::Fast;
    }
    if policy == RegallocPolicy::Full || has_back_edge {
        RegallocChoice::Full
    } else {
        RegallocChoice::Fast
    }
}

/// `NEOVM_JIT_REGALLOC_CHECKER=1`: run regalloc2's checker after every
/// allocation (a verification harness for the allocator choice; slow).
fn regalloc_checker_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_REGALLOC_CHECKER").as_deref() == Ok("1"))
}

/// Whether a constant `Value` must be routed through the per-leaf reloc vector
/// when lowering for **AOT** (`true`) instead of being baked as an `iconst`.
///
/// A baked immediate is only valid in the SESSION that emitted it. Two kinds of
/// constant carry session-specific bits and so cannot be baked into a
/// cross-session `.so`:
///   * HEAP OBJECTS (string/cons/vector/float) — the bits are a heap pointer
///     (already routed through reloc by R1a, via `is_heap_object()`).
///   * SYMBOLS other than `nil`/`t` — the bits encode a `SymId`, which is
///     INTERN-ORDER dependent (`intern.rs`: `SymId(symbols.len())`), so the same
///     name interns to a different id in a different session. `nil`/`t` are
///     pre-seeded at fixed ids 0/1, so they ARE session-stable and stay baked.
///
/// Everything else — fixnums (chars are fixnums in `[0, MAX_CHAR]`), `nil`, `t`
/// — is a universal immediate with session-stable bits and is baked in both
/// tiers. For the JIT (`aot=false`) only heap objects reloc (symbols bake, which
/// is correct same-session and keeps the JIT byte-identical); the broader symbol
/// reloc applies ONLY to AOT. (Audit #16.)
pub(crate) fn const_relocs_for_aot(v: Value) -> bool {
    v.is_heap_object() || (v.is_symbol() && v != Value::NIL && v != Value::T)
}

/// Module-generic build seam for [`lower_mir_with_plan`]: sets up the leaf ABI
/// signature, lowers the MIR through a `FunctionBuilder`, then declares +
/// defines the function into `module`, returning its `FuncId`. CLIF output is
/// byte-identical to the previous in-line lowering — this is a pure extraction.
///
/// Generic over `M: Module` so the same lowering drives the `JITModule` JIT
/// path today and an `ObjectModule` AOT path later, unchanged. The buffers
/// (`deopt_spill`/`deopt_meta`/`reloc_data`) are borrowed: their stable
/// addresses are baked into the generated code, and the caller retains
/// ownership to move them into the `CompiledLeaf`.
///
/// This fn deliberately contains NONE of the three ObjectModule-incompatible
/// JIT seams, which stay in the [`lower_mir_with_plan`] wrapper:
///   * `builder.symbol(...)`    — AOT: `Linkage::Import` resolved via dlopen.
///   * `finalize_definitions()` — AOT: `ObjectModule::finish()`.
///   * `get_finalized_function` — AOT: `dlsym` of the exported entry symbol.
///
/// `pub(crate)` so the AOT path (`jit::aot`) can drive it with `M = ObjectModule`.
///
/// `entry_name` / `entry_linkage` parameterize ONLY the entry's symbol-table
/// declaration (not the CLIF body, which stays byte-identical): the JIT wrapper
/// passes `("__neovm_mir_leaf", Linkage::Local)` exactly as before, while the AOT
/// path passes a unique `("__neovm_aot_{hash}_{tag}", Linkage::Export)` so the
/// `.o` exports a symbol the loader can `dlsym`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_mir_leaf_fn<M: Module>(
    module: &mut M,
    m: &mir::MirFunction,
    deopt_spill: &[core::cell::Cell<i64>],
    deopt_meta: &DeoptCells,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    plan: &MirLeafPlan,
    entry_name: &str,
    entry_linkage: Linkage,
    // R1c-sidecar: false → JIT (bases baked as `iconst` from the passed-in buffer
    // addresses, unchanged/fast); true → AOT (bases loaded from the 4th entry arg,
    // the per-thread `LeafSidecar`, since the addresses are session-specific). The
    // CLIF body is otherwise identical — same RESULTS either way.
    aot: bool,
    // JIT only, and only under entry counting: see `build_leaf_fn`.
    entry_counter: Option<&core::cell::Cell<u64>>,
) -> Result<cranelift_module::FuncId, CompileError> {
    imm_pool_reset();
    guards_emitted_reset();
    rootwin_counters_reset();
    use mir::{BinKind, CmpKind, MirOp, MirTerm, PredKind as MP, UnaryKind as MU};

    // The block-parameter type fixpoint (the MIR twin of the baseline's
    // `compute_known_fixnum_slots`): `types[v]` is what `v` provably is at
    // run time. A `Fixnum`-typed value skips its tag guard in `mir_as_raw`
    // and its GC root at a call — so this ONE table must feed both.
    let types = mir::infer_value_types(m);
    let n_fixnum_params = m
        .blocks
        .iter()
        .flat_map(|b| b.params.iter())
        .filter(|p| types[p.0 as usize] == mir::LispType::Fixnum)
        .count();

    // Phase-0 fix: this reset + the post-finalize set below used to exist only
    // in the baseline `build_leaf_fn`, so a Tier-2 compile's trace line
    // reported the PREVIOUS baseline compile's IR stats.
    LAST_IR_STATS.with(|c| c.set((0, 0, 0, 0)));

    let frontend_config = module.target_config();
    let call_conv = frontend_config.default_call_conv;
    let ptr_ty = frontend_config.pointer_type();

    // Unified 4-param entry ABI: fn(vmctx, args, out, sidecar) -> status. The
    // `sidecar` param is the per-(thread,leaf) base block (LeafSidecar). AOT code
    // reads its bases from it (`aot=true`); JIT code declares it but never reads
    // it (`aot=false`, bases stay `iconst`), so the dispatch passes null.
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty)); // vmctx (unused for pure)
    sig.params.push(AbiParam::new(ptr_ty)); // args
    sig.params.push(AbiParam::new(ptr_ty)); // out
    sig.params.push(AbiParam::new(ptr_ty)); // sidecar (*const LeafSidecar)
    sig.returns.push(AbiParam::new(types::I64));

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);

        // One CLIF block per MIR block, params = the MIR block's params.
        let clif_blocks: Vec<Block> = m
            .blocks
            .iter()
            .map(|blk| {
                let cb = fb.create_block();
                for _ in &blk.params {
                    fb.append_block_param(cb, types::I64);
                }
                cb
            })
            .collect();

        // Runtime context for polls, calls and allocation. declare_rt_refs
        // declares the full import set; only referenced shims are resolved.
        let mut rt = if plan.needs_rt {
            // Qualified ordinary calls and named builtins share the baseline
            // emitter. AOT plans have no ordinary call slots yet.
            let refs = declare_rt_refs(
                &mut *module,
                fb.func,
                call_conv,
                ptr_ty,
                plan.spec_sites.values().any(|s| s.kind.is_round1_subr()),
                plan.has_named_builtin,
            )?;
            let vmctx_var = fb.declare_var(ptr_ty);
            let call_args_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (plan.max_call_args.max(1) * 8) as u32,
                3,
            ));
            let call_result_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            Some(RtCtx {
                refs,
                vmctx_var,
                ptr_ty,
                call_args_slot,
                call_result_slot,
                rootwin: None,
            })
        } else {
            None
        };
        let backedge_counter = plan.has_backedge.then(|| {
            fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3))
        });
        // The deopt-buffer base addresses (for the JIT `iconst` path). The CLIF
        // `DeoptRefs` (iconst or sidecar-load) is materialized in the entry block
        // below, once it is populated and the sidecar param is available.
        let spill_base_addr = deopt_spill.as_ptr() as i64;
        let meta_pc_addr = &deopt_meta.pc as *const core::cell::Cell<i64> as i64;
        let meta_depth_addr = &deopt_meta.depth as *const core::cell::Cell<i64> as i64;
        let meta_handlers_addr = &deopt_meta.handlers as *const core::cell::Cell<i64> as i64;
        // ALL-PRECISE deopt for shim-bearing bodies (see mir_deopt_block): never
        // rerun-from-start after a side effect. Pure bodies keep the shared
        // rerun block.
        let precise = plan.precise;
        debug_assert!(
            !precise || deopt_spill.len() >= plan.max_depth,
            "the precise-deopt spill buffer holds the deepest framestate"
        );
        let cons_repl = &plan.cons_repl;
        let mut pending: Vec<PendingDeopt> = Vec::new();
        // Shared signal-propagation block (returns STATUS_SIGNAL), created lazily by
        // the first call lowering.
        let mut signal_exit: Option<Block> = None;

        // Map every MIR value to its CLIF value (filled in dominance order: a
        // single forward pass works because the MIR is SSA and block params
        // carry all cross-block values).
        let mut cval: Vec<Option<ClifValue>> = vec![None; m.value_types.len()];
        // Per-value form: true if `cval` holds an UNTAGGED raw fixnum (unboxing).
        // Fixnum arithmetic results + fixnum constants stay raw WITHIN a block (no
        // intermediate retag/untag/re-guard); boundaries (returns, predicates,
        // car/cdr, cross-block args) retag. Block params/args + non-fixnum values
        // are tagged (false) — no raw phis (the simpler, sound scope). A param
        // the fixpoint proved `Fixnum` skips its tag guard, and the untagged
        // form of any tagged value is memoized per block in `raw_twin`, never
        // written back (see `mir_as_raw`).
        let mut cval_raw: Vec<bool> = vec![false; m.value_types.len()];
        let mut proven_fixnum: Vec<bool> =
            types.iter().map(|t| *t == mir::LispType::Fixnum).collect();
        let mut raw_twin: Vec<Option<ClifValue>> = vec![None; m.value_types.len()];
        // Every `MirValue` is defined and used within ONE block (edge args are
        // the source block's values; the target sees only its own params), which
        // is what lets `raw_twin` skip a per-block reset and lets `proven_fixnum`
        // stand in for the baseline's per-block-entry set. Checked in debug.
        let owner: Vec<u32> = {
            let mut o = vec![u32::MAX; m.value_types.len()];
            for (bi, blk) in m.blocks.iter().enumerate() {
                for p in &blk.params {
                    o[p.0 as usize] = bi as u32;
                }
                for inst in &blk.insts {
                    o[inst.result.0 as usize] = bi as u32;
                }
            }
            o
        };

        // Shared deopt landing block: pure bodies rerun the interpreter from the
        // start (STATUS_DEOPT), created lazily on the first guard.
        let mut deopt: Option<Block> = None;

        // Function-entry block: stash the out pointer + load args, jump into MIR
        // block 0 passing the args as block params.
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        if let Some(counter) = entry_counter {
            debug_assert!(!aot, "AOT code never counts entries");
            emit_entry_count(&mut fb, ptr_ty, counter);
        }
        let vmctx_param = fb.block_params(entry)[0];
        if let Some(slot) = backedge_counter {
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().stack_store(ptr_ty, one, slot, 0);
        }
        if let Some(rt) = rt.as_mut() {
            fb.def_var(rt.vmctx_var, vmctx_param);
            // Root-window base + capacity check once per activation instead
            // of once per site (`HoistedRootWin`), on the baseline's rule:
            // only a body with rooting sites (those dereference the vmctx
            // anyway, so it is a real Context — a site-free body may be
            // entered with a null one), and only when a site can run more
            // than once per activation (two sites, or one in a loop). The
            // un-hoisted per-site sequence was the whole MIR-vs-baseline
            // gap on shim-heavy bodies: a 47-op dhrystone body lowered to
            // 593 CLIF instructions here against the baseline's 382.
            let sites = plan.rooting_sites;
            if plan.max_depth > 0 && (sites >= 2 || (sites == 1 && plan.has_backedge)) {
                emit_hoisted_root_window_prologue(&mut fb, rt, vmctx_param, plan.max_depth);
            }
        }
        let args_ptr = fb.block_params(entry)[1];
        let out_ptr = fb.block_params(entry)[2];
        // R1c-sidecar: the 4th entry param (the per-thread `*const LeafSidecar`).
        // Read only in AOT mode; JIT ignores it. The entry block dominates every
        // block, so a base materialized here is valid in any (incl. cold) block.
        let sidecar_param = aot.then(|| fb.block_params(entry)[3]);
        // R1a: base address of the heap-constant reloc vector, materialized once
        // near entry. JIT bakes the Box address as `iconst`; AOT loads it from the
        // sidecar (session-specific). `None` when the body references no heap
        // constants (then nothing loads off it).
        let reloc_base = if reloc_data.is_empty() {
            None
        } else if aot {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            Some(fb.ins().load(
                ptr_ty,
                MemFlagsData::trusted(),
                sc,
                LeafSidecar::OFF_RELOC_BASE,
            ))
        } else {
            Some(fb.ins().iconst(ptr_ty, reloc_data.as_ptr() as i64))
        };
        // The deopt-buffer bases (iconst or sidecar-load), materialized in the
        // entry block so they dominate the cold precise-deopt blocks.
        let deopt_refs = materialize_deopt_refs(
            &mut fb,
            ptr_ty,
            aot,
            /*has_precise_deopt=*/ precise,
            sidecar_param,
            spill_base_addr,
            meta_pc_addr,
            meta_depth_addr,
            meta_handlers_addr,
        );
        let arg_vals: Vec<BlockArg> = (0..m.arity)
            .map(|i| {
                let v = fb.ins().load(
                    types::I64,
                    MemFlagsData::trusted(),
                    args_ptr,
                    (i * 8) as i32,
                );
                BlockArg::Value(v)
            })
            .collect();
        fb.ins().jump(clif_blocks[0], &arg_vals);

        for (bi, blk) in m.blocks.iter().enumerate() {
            let cb = clif_blocks[bi];
            fb.switch_to_block(cb);
            // A block head can be reached from more than one predecessor, so
            // the record of which root-window slots already hold their value
            // (`RootWinCarry`) is dropped here, as at a bytecode leader.
            rootwin_carry_reset();
            // Bind this block's params to the CLIF block params.
            let bp = fb.block_params(cb).to_vec();
            for (p, &cv) in blk.params.iter().zip(bp.iter()) {
                cval[p.0 as usize] = Some(cv);
            }

            for inst in &blk.insts {
                let r = inst.result.0 as usize;
                debug_assert!(
                    mir::op_operands(&inst.op).all(|v| owner[v.0 as usize] == bi as u32),
                    "MIR operands are block-local (block {bi}, {:?})",
                    inst.op
                );
                match &inst.op {
                    MirOp::Arg(_) => {
                        // The param already holds the argument (bound above).
                    }
                    MirOp::InlineEntry => {
                        if precise && let Some(epoch) = m.inline_epoch {
                            if aot {
                                return Err(CompileError::UnsupportedOp("aot-inline-epoch"));
                            }
                            let d = mir_deopt_block(
                                &mut fb,
                                precise,
                                inst,
                                &cval,
                                &cval_raw,
                                cons_repl,
                                rt.as_ref(),
                                &mut deopt,
                                &mut pending,
                            )?;
                            emit_mir_inline_entry_guard(
                                &mut fb,
                                rt.as_ref().expect("precise inline runtime"),
                                epoch,
                                d,
                            )?;
                        }
                    }
                    MirOp::Const(v) => {
                        // Which non-fixnum consts route through the reloc vector vs
                        // bake: JIT relocs heap objects only (symbols bake — valid
                        // same-session, keeps the JIT byte-identical); AOT also
                        // relocs non-nil/t symbols, whose baked SymId would be
                        // session-specific in a cross-session `.so` (audit #16).
                        let needs_reloc = if aot {
                            const_relocs_for_aot(*v)
                        } else {
                            v.is_heap_object()
                        };
                        if (v.bits() & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE {
                            // Fixnum constant (incl chars) -> keep raw (untagged integer).
                            cval[r] = Some(
                                fb.ins()
                                    .iconst(types::I64, (v.bits() as i64) >> FIXNUM_SHIFT),
                            );
                            cval_raw[r] = true;
                        } else if !needs_reloc {
                            // Session-stable immediate (nil/t/char/...): no
                            // session-specific bits, so bake the tagged bits directly.
                            cval[r] = Some(fb.ins().iconst(types::I64, v.bits() as i64));
                        } else {
                            // Session-specific const (heap object always; under AOT
                            // also a non-nil/t symbol): load from the per-leaf reloc
                            // vector (R1a) — never bake session-specific bits, so the
                            // code is GC-pointer-free AND cross-session AOT-portable.
                            let idx = reloc_index[&v.bits()];
                            let base = reloc_base.expect("reloc_base set when reloc nonempty");
                            cval[r] = Some(fb.ins().load(
                                types::I64,
                                MemFlagsData::trusted(),
                                base,
                                (idx * 8) as i32,
                            ));
                        }
                    }
                    MirOp::Bin(kind, a, b) => {
                        let d = mir_deopt_block(
                            &mut fb,
                            precise,
                            inst,
                            &cval,
                            &cval_raw,
                            cons_repl,
                            rt.as_ref(),
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(
                            &mut fb,
                            &cval,
                            &cval_raw,
                            &mut raw_twin,
                            &proven_fixnum,
                            *a,
                            d,
                        )?;
                        let bv = mir_as_raw(
                            &mut fb,
                            &cval,
                            &cval_raw,
                            &mut raw_twin,
                            &proven_fixnum,
                            *b,
                            d,
                        )?;
                        let res = match kind {
                            BinKind::Add => raw_fixnum_addsub(&mut fb, d, false, av, bv),
                            BinKind::Sub => raw_fixnum_addsub(&mut fb, d, true, av, bv),
                            BinKind::Mul => raw_fixnum_mul(&mut fb, d, av, bv),
                            BinKind::Div => raw_fixnum_divrem(&mut fb, d, false, av, bv),
                            BinKind::Rem => raw_fixnum_divrem(&mut fb, d, true, av, bv),
                            BinKind::Max => raw_fixnum_maxmin(&mut fb, false, av, bv),
                            BinKind::Min => raw_fixnum_maxmin(&mut fb, true, av, bv),
                        };
                        cval[r] = Some(res);
                        cval_raw[r] = true;
                    }
                    MirOp::Unary(kind, a) => {
                        let k = match kind {
                            MU::Add1 => UnaryKind::Add1,
                            MU::Sub1 => UnaryKind::Sub1,
                            MU::Negate => UnaryKind::Negate,
                        };
                        let d = mir_deopt_block(
                            &mut fb,
                            precise,
                            inst,
                            &cval,
                            &cval_raw,
                            cons_repl,
                            rt.as_ref(),
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(
                            &mut fb,
                            &cval,
                            &cval_raw,
                            &mut raw_twin,
                            &proven_fixnum,
                            *a,
                            d,
                        )?;
                        cval[r] = Some(raw_fixnum_unop(&mut fb, d, k, av));
                        cval_raw[r] = true;
                    }
                    MirOp::Cmp(kind, a, b) => {
                        let cc = match kind {
                            CmpKind::NumEq => IntCC::Equal,
                            CmpKind::Lt => IntCC::SignedLessThan,
                            CmpKind::Gt => IntCC::SignedGreaterThan,
                            CmpKind::Le => IntCC::SignedLessThanOrEqual,
                            CmpKind::Ge => IntCC::SignedGreaterThanOrEqual,
                        };
                        let d = mir_deopt_block(
                            &mut fb,
                            precise,
                            inst,
                            &cval,
                            &cval_raw,
                            cons_repl,
                            rt.as_ref(),
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(
                            &mut fb,
                            &cval,
                            &cval_raw,
                            &mut raw_twin,
                            &proven_fixnum,
                            *a,
                            d,
                        )?;
                        let bv = mir_as_raw(
                            &mut fb,
                            &cval,
                            &cval_raw,
                            &mut raw_twin,
                            &proven_fixnum,
                            *b,
                            d,
                        )?;
                        let cond = fb.ins().icmp(cc, av, bv);
                        let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
                        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                        cval[r] = Some(fb.ins().select(cond, t, nil));
                    }
                    MirOp::Pred(kind, a) => {
                        let k = match kind {
                            MP::Null | MP::Not => PredKind::Null,
                            MP::Consp => PredKind::Consp,
                            MP::Stringp => PredKind::Stringp,
                            MP::Listp => PredKind::Listp,
                            // The symbols-with-position / bignum slow paths
                            // are shims: the baseline's arm, via the adapter.
                            MP::Symbolp | MP::Integerp | MP::Numberp => {
                                let bop = match kind {
                                    MP::Symbolp => Op::Symbolp,
                                    MP::Integerp => Op::Integerp,
                                    _ => Op::Numberp,
                                };
                                let rt = rt
                                    .as_ref()
                                    .ok_or(CompileError::UnsupportedOp("mir-adapter-no-rt"))?;
                                lower_mir_inst_via_baseline(
                                    &mut fb,
                                    inst,
                                    &bop,
                                    m,
                                    &mut cval,
                                    &mut cval_raw,
                                    cons_repl,
                                    rt,
                                    &mut pending,
                                    &mut signal_exit,
                                    reloc_base,
                                    reloc_index,
                                    aot,
                                    plan.max_call_args,
                                    None,
                                )?;
                                continue;
                            }
                        };
                        let a = mir_as_tagged(&mut fb, &cval, &cval_raw, *a)?;
                        cval[r] = Some(lower_predicate(&mut fb, k, a));
                    }
                    MirOp::CarCdr { cdr, safe, arg } => {
                        // If `arg` is a scalar-replaced (eliminated) cons, forward the
                        // read directly to its car/cdr operand SSA value — no consp
                        // guard, no allocation, no lower_car_cdr. Carry cval_raw so a
                        // raw fixnum stays raw across the elided cons. (Checked BEFORE
                        // mir_deopt_block — a forwarded read never deopts.)
                        if let Some((car_v, cdr_v)) = cons_repl[arg.0 as usize] {
                            let src = if *cdr { cdr_v } else { car_v };
                            cval[r] = cval[src.0 as usize];
                            cval_raw[r] = cval_raw[src.0 as usize];
                            // The read IS the operand, so its proof and its
                            // untagged twin carry over too (the builder types
                            // a CarCdr `Any`; the fixpoint does not see
                            // through an elided cons). The rooting skip still
                            // reads `types[r]` — `Any` — so it stays rooted.
                            proven_fixnum[r] = proven_fixnum[src.0 as usize];
                            raw_twin[r] = raw_twin[src.0 as usize];
                        } else {
                            let d = if *safe {
                                None
                            } else {
                                Some(mir_deopt_block(
                                    &mut fb,
                                    precise,
                                    inst,
                                    &cval,
                                    &cval_raw,
                                    cons_repl,
                                    rt.as_ref(),
                                    &mut deopt,
                                    &mut pending,
                                )?)
                            };
                            let a = mir_as_tagged(&mut fb, &cval, &cval_raw, *arg)?;
                            cval[r] = Some(lower_car_cdr(&mut fb, d, *cdr, *safe, a));
                        }
                    }
                    // A CALL: a GC safepoint + a side effect. Force-tag every value
                    // that survives it (a raw fixnum cannot cross the safepoint —
                    // the GC would trace the untagged i64 as a pointer), root the
                    // live-across-call residual, dispatch the generic shim for an
                    // unclassified or AOT call, propagate a signal, and on STATUS_OK
                    // push the tagged result. The body's guards are all precise
                    // (`precise == has_call`), so no rerun-from-start re-runs this.
                    MirOp::Opaque { op, args }
                        if matches!(op, Op::Call(_) | Op::Apply(_))
                            && !plan.spec_sites.contains_key(&inst.pc) =>
                    {
                        let rt = rt
                            .as_ref()
                            .ok_or(CompileError::UnsupportedOp("mir-call-no-rt"))?;
                        let n = match op {
                            Op::Call(n) | Op::Apply(n) => *n as usize,
                            _ => unreachable!("guarded to Call/Apply"),
                        };
                        let is_apply = matches!(op, Op::Apply(_));
                        if args.len() != n + 1 {
                            return Err(CompileError::UnsupportedOp("mir-call-arity"));
                        }
                        // Marshal the n args (args[1..]) tagged into the call buffer.
                        for (i, a) in args[1..].iter().enumerate() {
                            let v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *a)?;
                            fb.ins()
                                .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                        }
                        let func_val =
                            mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, args[0])?;
                        // Residual = operand-stack values live ACROSS the call (the
                        // pre-op stack below func+args). Root them (force-tagged) so a
                        // GC inside the callee can trace them.
                        let residual_len = inst.pre_stack.len().saturating_sub(n + 1);
                        // Gather the to-root residuals (force-tag ALL so
                        // downstream `cval` state never moves; skip provably-
                        // immediate MIR types when the opt is on).
                        let on = jit_lever1_on();
                        let mut to_root: Vec<ClifValue> = Vec::with_capacity(residual_len);
                        for k in 0..residual_len {
                            let rv = inst.pre_stack[k];
                            let v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, rv)?;
                            // The SAME inferred table the guard elision reads:
                            // a wrong `Fixnum` here is a missing root (a UAF),
                            // not a deopt, so debug builds trap on a skipped
                            // residual whose tag is not the fixnum tag.
                            let ty = types[rv.0 as usize];
                            if on && ty.never_needs_gc_root() {
                                if cfg!(debug_assertions) && ty == mir::LispType::Fixnum {
                                    let tag = band_imm_p(&mut fb, v, FIXNUM_CHECK_MASK as i64);
                                    let not_fix = fb.ins().icmp_imm_u(
                                        IntCC::NotEqual,
                                        tag,
                                        FIXNUM_CHECK_VALUE as i64,
                                    );
                                    fb.ins().trapnz(
                                        not_fix,
                                        cranelift_codegen::ir::TrapCode::unwrap_user(4),
                                    );
                                }
                                continue;
                            }
                            to_root.push(v);
                        }
                        // CONDITIONAL ROOTING: residuals here are typed
                        // Unknown/Any but empirically resolve to immediates
                        // (fixnum accumulators, symbols) on the hot paths, so
                        // test the tags INLINE and branch around all three
                        // rooting shims (save + push_many + restore) when
                        // nothing is heap. `!(is_fixnum | is_symbol)` is the
                        // exact layout-anchored `is_heap_object` (see the
                        // lever-1 correctness note); the shim's own re-test
                        // keeps any over-approximation harmless.
                        let saved = if to_root.is_empty() {
                            // Live residuals, all skipped by type: the callee
                            // runs with `top` at the base and may overwrite
                            // every slot (`RootWinCarry` rule 1). This arm
                            // skips by MIR TYPE while the adapter's arms skip
                            // only literal immediates, so a value skipped here
                            // can be one an adapter site stored and recorded.
                            rootwin_carry_reset();
                            CondRoots::NONE
                        } else {
                            CondRoots {
                                base: Some(emit_root_window_stores(&mut fb, rt, &to_root)),
                            }
                        };
                        let vmctx = fb.use_var(rt.vmctx_var);
                        let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
                        let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
                        let n_val = fb.ins().iconst(types::I64, n as i64);
                        let shim = if is_apply {
                            rt.refs.apply
                        } else {
                            rt.refs.call
                        };
                        let call = fb
                            .ins()
                            .call(shim, &[vmctx, func_val, args_addr, n_val, out_addr]);
                        let status = fb.inst_results(call)[0];
                        emit_cond_residual_roots_post(&mut fb, rt, saved);
                        // STATUS_OK -> continue; anything else is STATUS_SIGNAL.
                        let se = *signal_exit.get_or_insert_with(|| fb.create_block());
                        let cont = fb.create_block();
                        let ok = icmp_imm_p(&mut fb, IntCC::Equal, status, STATUS_OK);
                        fb.ins().brif(ok, cont, &[], se, &[]);
                        fb.switch_to_block(cont);
                        fb.seal_block(cont);
                        let result =
                            fb.ins()
                                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
                        cval[r] = Some(result);
                        cval_raw[r] = false;
                    }
                    // A non-escaping cons (escape analysis) is ELIDED: emit nothing,
                    // leave cval[r]=None — every use is a CarCdr that forwards to the
                    // operands.
                    MirOp::Cons(..) if cons_repl[r].is_some() => {}
                    // An escaping cons uses the non-collecting neovm_jit_cons shim.
                    // A pure body may rerun from the start: a fresh unshared object
                    // is not observable yet. A precise body spills the real Value
                    // normally. Tag both fields before storing them in the heap;
                    // no root publication is needed across this allocation.
                    MirOp::Cons(car, cdr) => {
                        let rt = rt
                            .as_ref()
                            .ok_or(CompileError::UnsupportedOp("mir-cons-no-rt"))?;
                        let car_v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *car)?;
                        let cdr_v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *cdr)?;
                        // No residual rooting: the cons shim is pure
                        // allocation and never reaches a GC safe point (see
                        // `neovm_jit_cons`), so nothing live across it can be
                        // collected. Infallible + context-free (no status, no
                        // vmctx) — no STATUS branch / signal exit.
                        let call = fb.ins().call(rt.refs.cons, &[car_v, cdr_v]);
                        let result = fb.inst_results(call)[0];
                        cval[r] = Some(result);
                        cval_raw[r] = false;
                    }
                    // Every other shim-using op — `eq` (the symbols-with-
                    // position slow path), a variable op, a builtin, a list
                    // op — is the baseline's arm, through the adapter.
                    MirOp::Eq(..) | MirOp::Opaque { .. } => {
                        let bop = match &inst.op {
                            MirOp::Opaque { op, .. } => op,
                            _ => &Op::Eq,
                        };
                        let rt = rt
                            .as_ref()
                            .ok_or(CompileError::UnsupportedOp("mir-adapter-no-rt"))?;
                        lower_mir_inst_via_baseline(
                            &mut fb,
                            inst,
                            bop,
                            m,
                            &mut cval,
                            &mut cval_raw,
                            cons_repl,
                            rt,
                            &mut pending,
                            &mut signal_exit,
                            reloc_base,
                            reloc_index,
                            aot,
                            plan.max_call_args,
                            plan.spec_sites.get(&inst.pc).map(|site| {
                                (
                                    site.sym,
                                    site.expected_bits,
                                    &plan.spec_slots[site.slot] as *const super::SpecSlot as i64,
                                    site.slot,
                                    site.kind,
                                )
                            }),
                        )?;
                    }
                }
            }

            // Terminator.
            debug_assert!(
                mir::term_operands(&blk.term).all(|v| owner[v.0 as usize] == bi as u32),
                "MIR terminator operands are block-local (block {bi})"
            );
            match &blk.term {
                MirTerm::Return(v) => {
                    let rv = mir_as_tagged(&mut fb, &cval, &cval_raw, *v)?;
                    let out = out_ptr;
                    fb.ins().store(MemFlagsData::trusted(), rv, out, 0);
                    let ok = fb.ins().iconst(types::I64, STATUS_OK);
                    fb.ins().return_(&[ok]);
                }
                MirTerm::Goto { target, args } => {
                    // Cross-block args are tagged (block params are tagged).
                    // Force-tag (write-back) rather than plain retag: the
                    // block ends here, and a value repeated in the arg list
                    // then retags ONCE instead of per mention.
                    let mut values = Vec::with_capacity(args.len());
                    for v in args {
                        values.push(mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *v)?);
                    }
                    let a: Vec<BlockArg> = values.iter().copied().map(BlockArg::Value).collect();
                    if m.blocks[target.0 as usize].bytecode_pc <= blk.bytecode_pc {
                        emit_backedge_jump_with_args(
                            &mut fb,
                            rt.as_ref().expect("backedge implies rt"),
                            backedge_counter.expect("backedge implies counter"),
                            &mut signal_exit,
                            &values,
                            None,
                            clif_blocks[target.0 as usize],
                            &a,
                            &[],
                            &mut Vec::new(),
                        );
                    } else {
                        fb.ins().jump(clif_blocks[target.0 as usize], &a);
                    }
                }
                MirTerm::Branch {
                    cond,
                    on_nil,
                    taken,
                    taken_args,
                    fallthrough,
                    fallthrough_args,
                    ..
                } => {
                    // Force-tag (write-back): both edges' arg sequences are
                    // emitted straight-line before the brif, so a raw value
                    // present on BOTH edges (the common shape — a loop
                    // accumulator) used to be retagged TWICE per boundary
                    // (mir_as_tagged has no memo). The attack pass measured
                    // ~2 duplicate shl+or pairs per branch chunk on the
                    // big-body bench. The block ends at the terminator, so
                    // clearing the raw mask costs nothing.
                    let c = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *cond)?;
                    let is_nil = fb
                        .ins()
                        .icmp_imm_u(IntCC::Equal, c, Value::NIL.bits() as i64);
                    let mut ta: Vec<BlockArg> = Vec::with_capacity(taken_args.len());
                    for v in taken_args {
                        ta.push(BlockArg::Value(mir_force_tagged(
                            &mut fb,
                            &mut cval,
                            &mut cval_raw,
                            *v,
                        )?));
                    }
                    let mut fa: Vec<BlockArg> = Vec::with_capacity(fallthrough_args.len());
                    for v in fallthrough_args {
                        fa.push(BlockArg::Value(mir_force_tagged(
                            &mut fb,
                            &mut cval,
                            &mut cval_raw,
                            *v,
                        )?));
                    }
                    // Only a taken backward edge counts toward the poll. A
                    // trampoline carries exactly that edge's tagged stack,
                    // including ElsePop's retained condition when applicable.
                    let mut trampolines = Vec::new();
                    let mut edge_target = |target: mir::MirBlockId| {
                        let dest = &m.blocks[target.0 as usize];
                        let cb = clif_blocks[target.0 as usize];
                        if dest.bytecode_pc > blk.bytecode_pc {
                            return cb;
                        }
                        let tramp = fb.create_block();
                        for _ in &dest.params {
                            fb.append_block_param(tramp, types::I64);
                        }
                        trampolines.push((tramp, cb));
                        tramp
                    };
                    let tb = edge_target(*taken);
                    let fbk = edge_target(*fallthrough);
                    // brif takes the `then` block when the condition is true.
                    if *on_nil {
                        fb.ins().brif(is_nil, tb, &ta, fbk, &fa);
                    } else {
                        fb.ins().brif(is_nil, fbk, &fa, tb, &ta);
                    }
                    for (tramp, target) in trampolines {
                        fb.switch_to_block(tramp);
                        fb.seal_block(tramp);
                        let values = fb.block_params(tramp).to_vec();
                        let args: Vec<BlockArg> =
                            values.iter().copied().map(BlockArg::Value).collect();
                        emit_backedge_jump_with_args(
                            &mut fb,
                            rt.as_ref().expect("backedge implies rt"),
                            backedge_counter.expect("backedge implies counter"),
                            &mut signal_exit,
                            &values,
                            None,
                            target,
                            &args,
                            &[],
                            &mut Vec::new(),
                        );
                    }
                }
            }
        }

        if let Some(db) = deopt {
            fb.switch_to_block(db);
            let code = fb.ins().iconst(types::I64, STATUS_DEOPT);
            fb.ins().return_(&[code]);
        }

        // Per-site precise-deopt blocks (call-bearing bodies): spill the captured
        // framestate (retagging raw slots in the cold block) + return STATUS_DEOPT_AT.
        // No-op for pure bodies (pending is empty).
        // The MIR tier has no float sites (`gate:float-site`), so no flonums.
        emit_pending_deopts(&mut fb, deopt_refs, &mut pending, None);

        // Signal propagation from a call: return STATUS_SIGNAL (the Flow is stashed
        // in the Context by the shim). No binds/handlers to unwind — build_mir bails
        // on those, so a MIR leaf never registers any.
        if let Some(se) = signal_exit {
            fb.switch_to_block(se);
            fb.seal_block(se);
            let code = fb.ins().iconst(types::I64, STATUS_SIGNAL);
            fb.ins().return_(&[code]);
        }

        fb.seal_all_blocks();
        fb.finalize(frontend_config);
    }
    LAST_IR_STATS.with(|c| {
        let (_, _, sites, slots) = c.get();
        c.set((
            func.dfg.num_insts() as u32,
            func.layout.blocks().count() as u32,
            sites,
            slots,
        ));
    });
    let (rw_stores, rw_elided) = rootwin_counters();
    dump_clif(
        &func,
        &format!(
            "mir blocks={} fixnum_params={n_fixnum_params} guards={} untags={} retags={} rw_stores={rw_stores} rw_elided={rw_elided} entry={entry_name}",
            m.blocks.len(),
            guards_emitted(),
            untags_emitted(),
            retags_emitted()
        ),
    );

    let fid = module
        .declare_function(entry_name, entry_linkage, &sig)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    super::note_clif_size(&ctx.func);
    // NEOVM_JIT_DUMP_ASM: Cranelift renders its disassembly only when asked.
    let disasm = super::super::stats::asm_dump::want_disasm(aot);
    if disasm {
        ctx.set_disasm(true);
    }
    module
        .define_function(fid, &mut ctx)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    if disasm {
        super::super::stats::asm_dump::stash(&ctx);
    }
    module.clear_context(&mut ctx);

    Ok(fid)
}

/// Per-function runtime-call machinery: shim references plus the vmctx variable
/// and the scratch stack slots `Call` spills through. Present only when the body
/// re-enters the runtime (`Cons` / `Call`).
pub(crate) struct RtCtx {
    pub(crate) refs: RtRefs,
    /// The `*mut Context` function parameter, carried in an SSA variable so any
    /// block can read it.
    pub(crate) vmctx_var: Variable,
    /// Pointer type of the target (for `stack_addr`).
    pub(crate) ptr_ty: Type,
    /// Spill buffer for outgoing call arguments (max `Call` nargs in the body).
    pub(crate) call_args_slot: StackSlot,
    /// 8-byte result slot the call shim writes through.
    pub(crate) call_result_slot: StackSlot,
    /// Root-window state hoisted to the function entry (baseline leaves;
    /// `None` keeps the per-site sequence): see [`HoistedRootWin`].
    pub(crate) rootwin: Option<HoistedRootWin>,
}

/// The residual root window's frame base, loaded once at entry, with its
/// capacity checked once for the largest window any site can need (the
/// body's max operand-stack depth). A site then emits one pointer load, its
/// stores and one `top` update instead of two loads, a compare, a branch, a
/// grow block and a pointer reload: on the org editing probe the per-site
/// sequence was ~40% of all CLIF instructions and ~36% of all blocks.
///
/// Sound because `top` is invariant between sites within one activation
/// (every site restores it; a nested activation works above it), so the
/// base loaded at entry is every site's base, and because capacity only
/// ever grows, so a nested activation's grow cannot invalidate the entry
/// check. The buffer POINTER can move on a nested grow, so a site reloads it.
#[derive(Clone, Copy)]
pub(crate) struct HoistedRootWin {
    /// The activation's frame base (`top` at entry).
    pub(crate) base: ClifValue,
    /// `base * 8`: the byte offset of slot 0 from the buffer pointer.
    pub(crate) byte_off: ClifValue,
}

thread_local! {
    /// Which root-window slots the hoisted sites of the function being
    /// lowered have already stored (see [`RootWinCarry`]). Reset per
    /// function and at every bytecode basic-block leader.
    static ROOTWIN_CARRY: std::cell::RefCell<RootWinCarry> =
        const { std::cell::RefCell::new(RootWinCarry { stored: Vec::new(), emitted: 0, elided: 0 }) };
}

/// The per-window-index SSA value the last hoisted site stored.
///
/// The invariant: whenever control reaches a site, window slot `i` holds
/// `stored[i]` for every recorded `i`. Eliding a store whose recorded value
/// matches is then exact. Three rules keep it true:
///
/// 1. **Truncate at every site that can run a nested activation.** Between
///    two sites the slots below the first site's count are written by nobody
///    — our sites are the only writers of this frame's slots, a nested
///    activation works above `top`, and the collector only reads. Slots at or
///    above a site's count may be clobbered by the nested activation during
///    its call, so the record is truncated to that count — INCLUDING a count
///    of zero: a site that reaches its shim with live residuals but stores
///    none (every residual skipped as provably immediate) runs the callee
///    with `top` at the base, and the record must be emptied. A site whose
///    residual is EMPTY may skip this: every value it recorded is dead (all
///    live operand values are in a site's residual), so none can match again.
/// 2. **Merge at a conditional fallback.** An arm whose fast path stores
///    nothing (a GC-free read shim that cannot run a nested activation) and
///    whose fallback block roots for a general call has two store histories
///    meeting at its continuation. The record there is the MEET of the two
///    (`rootwin_carry_snapshot` / `rootwin_carry_meet`): an entry survives
///    only if both paths leave the same value in that slot.
/// 3. **Reset at a join.** Every other internal block an op's lowering
///    creates (status checks, guards) has all its predecessors inside that
///    op and runs the same stores on every path, so the only edges that can
///    reach a site with a different store history are jumps to a bytecode
///    leader (loop heads, branch targets, handlers, the OSR entry), a MIR
///    block head, a handler-dispatch block (emitted at the end of its
///    bytecode block but entered from one earlier site's signal edge), and a
///    back-edge poll block (a Switch compare chain emits one per backward
///    target, as sibling paths) — the record is dropped there.
struct RootWinCarry {
    stored: Vec<Option<ClifValue>>,
    /// Diagnostics: root-window stores emitted / elided in this function.
    emitted: u32,
    elided: u32,
}

#[cfg(test)]
thread_local! {
    static FORCE_SPEC_GUARD_MISS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Compile every cross-block spec site's callee check so it never matches
/// (tests only): exercises the generic call behind it.
#[cfg(test)]
pub(crate) fn force_spec_guard_miss_for_test(on: bool) {
    FORCE_SPEC_GUARD_MISS.with(|c| c.set(on));
}

/// Forget the carried record: a new function, a bytecode leader, a MIR block
/// head, or a site that runs a nested activation without storing anything.
pub(crate) fn rootwin_carry_reset() {
    ROOTWIN_CARRY.with(|c| c.borrow_mut().stored.clear());
}

/// The carried record as it stands — taken just before a conditional
/// fallback roots, i.e. the record on the path that skips the fallback.
pub(crate) fn rootwin_carry_snapshot() -> Vec<Option<ClifValue>> {
    ROOTWIN_CARRY.with(|c| c.borrow().stored.clone())
}

/// Merge a conditional fallback's store history with the path that skipped
/// it (`snapshot`): keep an entry only where both paths leave the same value
/// in the slot (rule 2 on [`RootWinCarry`]).
pub(crate) fn rootwin_carry_meet(snapshot: &[Option<ClifValue>]) {
    ROOTWIN_CARRY.with(|c| {
        let mut c = c.borrow_mut();
        let n = c.stored.len().min(snapshot.len());
        c.stored.truncate(n);
        for (slot, &other) in c.stored.iter_mut().zip(snapshot) {
            if *slot != other {
                *slot = None;
            }
        }
    });
}

/// Zero the per-function elision counters (at the hoisted prologue).
pub(crate) fn rootwin_counters_reset() {
    ROOTWIN_CARRY.with(|c| {
        let mut c = c.borrow_mut();
        c.stored.clear();
        c.emitted = 0;
        c.elided = 0;
    });
}

/// `(root-window stores emitted, elided)` for the function just lowered.
pub(crate) fn rootwin_counters() -> (u32, u32) {
    ROOTWIN_CARRY.with(|c| {
        let c = c.borrow();
        (c.emitted, c.elided)
    })
}

thread_local! {
    /// Per-function pool of I64 immediates materialized once in the entry
    /// block (see [`imm_pool_define`]); empty for functions that do not opt
    /// in, so [`imm64`] falls back to a fresh `iconst`.
    static IMM_POOL: std::cell::RefCell<std::collections::HashMap<i64, ClifValue>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The immediates a baseline leaf pools at entry: the org editing probe's
/// CLIF census (2026-09-05) had 8,137 `iconst`s in 62 bodies, 842 distinct
/// values, and these few small ones were two thirds of them (1, 0, 3, 2, 4,
/// 7, 8, -8, ... — tag masks, slot strides, small counts).
pub(crate) const POOLED_IMMEDIATES: &[i64] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 24, 32, -1, -8,
];

/// Forget the pool (a new function).
pub(crate) fn imm_pool_reset() {
    IMM_POOL.with(|p| p.borrow_mut().clear());
}

/// Materialize `values` in the CURRENT block — which must be the entry
/// block, so every later use is dominated — and pool them.
///
/// Why this is safe for code quality: every pooled use below goes through an
/// ALU op whose x64 lowering folds an `iconst` operand into the instruction
/// (`cmp r, imm`, `and r, imm`, `add r, imm`, shifts by immediate), matched
/// on the operand's DEFINITION regardless of how many uses it has; a pooled
/// constant all of whose uses fold is never emitted and has no live range.
/// Constants that feed stores, calls or selects are deliberately NOT pooled
/// (Cranelift 0.134 does not rematerialize, so they would become long-lived
/// registers). The saving is IR the frontend and lowering never walk: one
/// definition per value per function instead of one per use.
pub(crate) fn imm_pool_define(fb: &mut FunctionBuilder, values: &[i64]) {
    IMM_POOL.with(|p| {
        let mut p = p.borrow_mut();
        for &k in values {
            let v = fb.ins().iconst(types::I64, k);
            p.insert(k, v);
        }
    });
}

/// An I64 immediate: the pooled entry-block value when `k` is pooled, else a
/// fresh `iconst`.
pub(crate) fn imm64(fb: &mut FunctionBuilder, k: i64) -> ClifValue {
    if let Some(v) = IMM_POOL.with(|p| p.borrow().get(&k).copied()) {
        return v;
    }
    fb.ins().iconst(types::I64, k)
}

fn is_i64(fb: &FunctionBuilder, x: ClifValue) -> bool {
    fb.func.dfg.value_type(x) == types::I64
}

/// `icmp_imm` through the pool for I64 operands (Cranelift's `icmp_imm_u`
/// materializes a fresh `iconst` per call).
pub(crate) fn icmp_imm_p(fb: &mut FunctionBuilder, cc: IntCC, x: ClifValue, k: i64) -> ClifValue {
    if is_i64(fb, x) {
        let c = imm64(fb, k);
        fb.ins().icmp(cc, x, c)
    } else {
        fb.ins().icmp_imm_u(cc, x, k)
    }
}

macro_rules! pooled_binop {
    ($name:ident, $op:ident, $imm:ident) => {
        /// Immediate-form ALU op through the pool for I64 operands.
        pub(crate) fn $name(fb: &mut FunctionBuilder, x: ClifValue, k: i64) -> ClifValue {
            if is_i64(fb, x) {
                let c = imm64(fb, k);
                fb.ins().$op(x, c)
            } else {
                fb.ins().$imm(x, k)
            }
        }
    };
}
pooled_binop!(iadd_imm_p, iadd, iadd_imm_u);
pooled_binop!(band_imm_p, band, band_imm_u);
pooled_binop!(bor_imm_p, bor, bor_imm_u);
pooled_binop!(sshr_imm_p, sshr, sshr_imm_u);
pooled_binop!(ushr_imm_p, ushr, ushr_imm_u);
pooled_binop!(ishl_imm_p, ishl, ishl_imm_u);

/// `*counter += 1` through a baked address (a load, an add and a store): the
/// instructions a leaf's entry block starts with when entry counting was on
/// at compile time
/// (`stats::entry_counting_enabled`). Never emitted otherwise, so default
/// code is unchanged. A Rust `Cell` in the leaf's `LeafObs`, not the Lisp
/// heap: no barrier, no safepoint, and it runs before any guard.
pub(crate) fn emit_entry_count(
    fb: &mut FunctionBuilder,
    ptr_ty: Type,
    counter: &core::cell::Cell<u64>,
) {
    let addr = fb.ins().iconst(ptr_ty, counter.as_ptr() as i64);
    let n = fb.ins().load(types::I64, MemFlagsData::trusted(), addr, 0);
    let n1 = fb.ins().iadd_imm_s(n, 1);
    fb.ins().store(MemFlagsData::trusted(), n1, addr, 0);
}

/// `NEOVM_JIT_DUMP_CLIF=<path>`: append every lowered function's CLIF (with
/// an `;; ops=N` header) to `path` — the IR-composition census.
///
/// Each record is formatted first and appended with ONE write, so processes
/// sharing the file (a parallel test run) interleave whole records, never
/// fragments of them: a before/after comparison can then sort the records.
pub(crate) fn dump_clif(func: &cranelift_codegen::ir::Function, header: &str) {
    use std::sync::OnceLock;
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let Some(path) = PATH.get_or_init(|| std::env::var("NEOVM_JIT_DUMP_CLIF").ok()) else {
        return;
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let record = format!(";; {header}\n{}\n\n", func.display());
        let _ = f.write_all(record.as_bytes());
    }
}

/// Emit the hoisted root-window prologue into the current (entry) block:
/// load the frame base, grow the buffer once if `max_slots` more slots may
/// not fit, and record the base for every site (`RtCtx::rootwin`).
pub(crate) fn emit_hoisted_root_window_prologue(
    fb: &mut FunctionBuilder,
    rt: &mut RtCtx,
    vmctx: ClifValue,
    max_slots: usize,
) {
    rootwin_counters_reset();
    let (_off_ptr, off_top, off_cap) = ctx_rootwin_offsets();
    let base = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), vmctx, off_top);
    let need_max = iadd_imm_p(fb, base, max_slots as i64);
    let cap = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), vmctx, off_cap);
    let fits = fb.ins().icmp(IntCC::UnsignedLessThanOrEqual, need_max, cap);
    let grow_blk = fb.create_block();
    let cont_blk = fb.create_block();
    fb.ins().brif(fits, cont_blk, &[], grow_blk, &[]);
    fb.switch_to_block(grow_blk);
    fb.seal_block(grow_blk);
    fb.ins().call(rt.refs.rootwin_grow, &[vmctx, need_max]);
    fb.ins().jump(cont_blk, &[]);
    fb.switch_to_block(cont_blk);
    fb.seal_block(cont_blk);
    let byte_off = ishl_imm_p(fb, base, 3);
    rt.rootwin = Some(HoistedRootWin { base, byte_off });
}

/// Callable references to every runtime shim, declared into one function.
pub(crate) struct RtRefs {
    pub(crate) rootwin_grow: FuncRef,
    pub(crate) cons: FuncRef,
    /// Boxes an `f64` computed in a register (`neovm_jit_make_float`).
    pub(crate) make_float: FuncRef,
    /// The generic fallback of an arithmetic site whose feedback says the
    /// fixnum path misses (`neovm_jit_arith_generic`).
    pub(crate) arith_generic: FuncRef,
    pub(crate) call: FuncRef,
    pub(crate) apply: FuncRef,
    pub(crate) eq_slow: FuncRef,
    pub(crate) symbolp_slow: FuncRef,
    pub(crate) varref: FuncRef,
    pub(crate) varset: FuncRef,
    pub(crate) varbind: FuncRef,
    pub(crate) unbind: FuncRef,
    pub(crate) backedge: FuncRef,
    pub(crate) save_current_buffer: FuncRef,
    pub(crate) save_excursion: FuncRef,
    pub(crate) save_restriction: FuncRef,
    pub(crate) unwind_protect: FuncRef,
    pub(crate) throw_flow: FuncRef,
    pub(crate) integerp_slow: FuncRef,
    pub(crate) numberp_slow: FuncRef,
    pub(crate) builtin1: FuncRef,
    pub(crate) builtin2: FuncRef,
    pub(crate) builtin3: FuncRef,
    /// `Op::Aref` (`neovm_jit_aref`): the element's bits or `VALUE_SHIM_SIGNAL`.
    pub(crate) aref: FuncRef,
    /// `Op::Aset` (`neovm_jit_aset`): the value's bits or a `VALUE_SHIM_*` word.
    pub(crate) aset: FuncRef,
    /// `Op::Memq` (`neovm_jit_memq`): the tail's bits or `VALUE_SHIM_SIGNAL`.
    pub(crate) memq: FuncRef,
    /// `Op::Assq` (`neovm_jit_assq`): the entry's bits or `VALUE_SHIM_SIGNAL`.
    pub(crate) assq: FuncRef,
    /// `Op::Setcar` (`neovm_jit_setcar`): the new car's bits or
    /// `VALUE_SHIM_SIGNAL`.
    pub(crate) setcar: FuncRef,
    /// `Op::Setcdr` (`neovm_jit_setcdr`): the new cdr's bits or
    /// `VALUE_SHIM_SIGNAL`.
    pub(crate) setcdr: FuncRef,
    pub(crate) push_cc: FuncRef,
    pub(crate) push_cc_raw: FuncRef,
    pub(crate) push_catch: FuncRef,
    pub(crate) pop_handler: FuncRef,
    pub(crate) match_handler: FuncRef,
    pub(crate) switch_lookup: FuncRef,
    pub(crate) switch_stale: FuncRef,
    pub(crate) list: FuncRef,
    pub(crate) builtin_slice: FuncRef,
    pub(crate) named_builtin: FuncRef,
    pub(crate) save_window_excursion: FuncRef,
    pub(crate) call_spec: FuncRef,
    /// The three subr-speculation shims (Gap 1), declared ONLY when the body
    /// actually has subr-kind spec sites (`declare_rt_refs`' `subr_spec`
    /// flag). `None` otherwise — in particular for EVERY AOT build
    /// (`build_baseline_leaf_object` compiles with an empty site map), so an
    /// AOT object can never acquire an import of these JIT-only shims (they
    /// are deliberately NOT in `shim_names.rs`, and
    /// `assert_aot_imports_exported` refuses foreign imports at emit time).
    pub(crate) call_subr_spec: Option<FuncRef>,
    pub(crate) pred_spec: Option<FuncRef>,
    pub(crate) eq_incl_props_spec: Option<FuncRef>,
    /// `neovm_jit_arith_spec` (logand/logior/logxor intrinsic). Declared under the
    /// same `subr_spec` flag as the round-1 subr shims (see `is_round1_subr`).
    pub(crate) arith_spec: Option<FuncRef>,
    /// The R2 CallBuiltinSym intrinsic shims — Tier-B dispatch-skip
    /// ([`neovm_jit_cbsym_spec`]) + Tier-A GC-free read
    /// ([`neovm_jit_cbsym_read`]), declared ONLY when the body has a CBSym-kind
    /// spec site (`declare_rt_refs`' `cbsym_spec` flag). UNLIKE the round-1 subr
    /// shims (still `Some(obarray)`-gated), CBSym classification is obarray-free,
    /// so these ARE declared for AOT baseline leaves too (increment A) — both are
    /// exported (`shim_names.rs`) and bind against the host at `dlopen`. `None`
    /// when the body has no CBSym-kind site.
    pub(crate) cbsym_spec: Option<FuncRef>,
    pub(crate) cbsym_read: Option<FuncRef>,
}

/// Declare the runtime-shim imports into `module`/`func` and return the callable
/// refs. The matching addresses are registered on the `JITBuilder` in
/// [`lower_leaf`] via `builder.symbol(...)` (the JIT seam); under AOT the same
/// `Linkage::Import` declarations resolve via the dynamic loader instead.
///
/// Generic over the module type (`M: Module`) so it serves both the `JITModule`
/// JIT path and the future `ObjectModule` AOT path with no change — it only
/// calls `Module::declare_function`, a trait method available on both.
///
/// `subr_spec`: declare the three round-1 subr-speculation shims (Gap 1) — still
/// JIT-only (their `find_spec_sites` pass requires `Some(obarray)`, i.e. never
/// AOT — increment B), so those names (absent from `shim_names.rs`) are never
/// DECLARED into an `ObjectModule`, independent of whether unreferenced
/// declarations would reach the emitted object.
/// `cbsym_spec`: declare the R2 CallBuiltinSym intrinsic shims (Tier-A read +
/// Tier-B dispatch-skip). CBSym classification is obarray-free, so as of increment
/// A this flag is TRUE for AOT baseline leaves too — the shims ARE in
/// `shim_names.rs` (exported + salted) and resolve at `dlopen`.
/// Both flags are set by `build_leaf_fn` from the body's actual spec sites.
pub(crate) fn declare_rt_refs<M: Module>(
    module: &mut M,
    func: &mut Function,
    call_conv: cranelift_codegen::isa::CallConv,
    ptr_ty: Type,
    subr_spec: bool,
    cbsym_spec: bool,
) -> Result<RtRefs, CompileError> {
    let i64t = types::I64;
    let mut sig_rootwin_grow = Signature::new(call_conv); // (vmctx, need) -> ()
    sig_rootwin_grow.params.push(AbiParam::new(ptr_ty));
    sig_rootwin_grow.params.push(AbiParam::new(i64t));
    let mut sig_cons = Signature::new(call_conv); // (i64, i64) -> i64
    sig_cons.params.push(AbiParam::new(i64t));
    sig_cons.params.push(AbiParam::new(i64t));
    sig_cons.returns.push(AbiParam::new(i64t));
    // (vmctx, func_bits, args_ptr, nargs, out_ptr) -> status
    let mut sig_call = Signature::new(call_conv);
    sig_call.params.push(AbiParam::new(ptr_ty));
    sig_call.params.push(AbiParam::new(i64t));
    sig_call.params.push(AbiParam::new(ptr_ty));
    sig_call.params.push(AbiParam::new(i64t));
    sig_call.params.push(AbiParam::new(ptr_ty));
    sig_call.returns.push(AbiParam::new(i64t));
    // (vmctx, a, b) -> t/nil bits
    let mut sig_eq = Signature::new(call_conv);
    sig_eq.params.push(AbiParam::new(ptr_ty));
    sig_eq.params.push(AbiParam::new(i64t));
    sig_eq.params.push(AbiParam::new(i64t));
    sig_eq.returns.push(AbiParam::new(i64t));
    // (vmctx, v) -> t/nil bits
    let mut sig_symp = Signature::new(call_conv);
    sig_symp.params.push(AbiParam::new(ptr_ty));
    sig_symp.params.push(AbiParam::new(i64t));
    sig_symp.returns.push(AbiParam::new(i64t));

    let declare = |module: &mut M, name: &str, sig: &Signature| {
        module
            .declare_function(name, Linkage::Import, sig)
            .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))
    };

    let rootwin_grow_id = declare(module, "neovm_jit_rootwin_grow", &sig_rootwin_grow)?;
    let cons_id = declare(module, "neovm_jit_cons", &sig_cons)?;
    let mut sig_make_float = Signature::new(call_conv); // (f64) -> i64
    sig_make_float.params.push(AbiParam::new(types::F64));
    sig_make_float.returns.push(AbiParam::new(i64t));
    let make_float_id = declare(module, "neovm_jit_make_float", &sig_make_float)?;
    // (vmctx, kind, a, b, out_ptr) -> status
    let mut sig_arith_generic = Signature::new(call_conv);
    sig_arith_generic.params.push(AbiParam::new(ptr_ty));
    sig_arith_generic.params.push(AbiParam::new(i64t));
    sig_arith_generic.params.push(AbiParam::new(i64t));
    sig_arith_generic.params.push(AbiParam::new(i64t));
    sig_arith_generic.params.push(AbiParam::new(ptr_ty));
    sig_arith_generic.returns.push(AbiParam::new(i64t));
    let arith_generic_id = declare(module, "neovm_jit_arith_generic", &sig_arith_generic)?;
    let call_id = declare(module, "neovm_jit_call", &sig_call)?;
    let apply_id = declare(module, "neovm_jit_apply", &sig_call)?;
    let eq_id = declare(module, "neovm_jit_eq_slow", &sig_eq)?;
    let symp_id = declare(module, "neovm_jit_symbolp_slow", &sig_symp)?;
    // (vmctx, sym_id, out_ptr) -> status
    let mut sig_varref = Signature::new(call_conv);
    sig_varref.params.push(AbiParam::new(ptr_ty));
    sig_varref.params.push(AbiParam::new(i64t));
    sig_varref.params.push(AbiParam::new(ptr_ty));
    sig_varref.returns.push(AbiParam::new(i64t));
    // (vmctx, sym_id, val) -> status
    let mut sig_varset = Signature::new(call_conv);
    sig_varset.params.push(AbiParam::new(ptr_ty));
    sig_varset.params.push(AbiParam::new(i64t));
    sig_varset.params.push(AbiParam::new(i64t));
    sig_varset.returns.push(AbiParam::new(i64t));
    let varref_id = declare(module, "neovm_jit_varref", &sig_varref)?;
    let varset_id = declare(module, "neovm_jit_varset", &sig_varset)?;
    // (vmctx, sym_id, val) -> status
    let mut sig_varbind = Signature::new(call_conv);
    sig_varbind.params.push(AbiParam::new(ptr_ty));
    sig_varbind.params.push(AbiParam::new(i64t));
    sig_varbind.params.push(AbiParam::new(i64t));
    sig_varbind.returns.push(AbiParam::new(i64t));
    // (vmctx, n) -> status
    let mut sig_unbind = Signature::new(call_conv);
    sig_unbind.params.push(AbiParam::new(ptr_ty));
    sig_unbind.params.push(AbiParam::new(i64t));
    sig_unbind.returns.push(AbiParam::new(i64t));
    let varbind_id = declare(module, "neovm_jit_varbind", &sig_varbind)?;
    let unbind_id = declare(module, "neovm_jit_unbind", &sig_unbind)?;
    // (vmctx) -> status
    let mut sig_backedge = Signature::new(call_conv);
    sig_backedge.params.push(AbiParam::new(ptr_ty));
    sig_backedge.returns.push(AbiParam::new(i64t));
    let backedge_id = declare(module, "neovm_jit_backedge", &sig_backedge)?;
    // (vmctx) -> ()  — the infallible Save* records.
    let mut sig_save = Signature::new(call_conv);
    sig_save.params.push(AbiParam::new(ptr_ty));
    let scb_id = declare(module, "neovm_jit_save_current_buffer", &sig_save)?;
    let sexc_id = declare(module, "neovm_jit_save_excursion", &sig_save)?;
    let sres_id = declare(module, "neovm_jit_save_restriction", &sig_save)?;
    // (vmctx, forms) -> ()  — unwind-protect record (infallible). Keep this
    // distinct from the now-fallible unbind ABI above.
    let mut sig_unwind_protect = Signature::new(call_conv);
    sig_unwind_protect.params.push(AbiParam::new(ptr_ty));
    sig_unwind_protect.params.push(AbiParam::new(i64t));
    let up_id = declare(module, "neovm_jit_unwind_protect", &sig_unwind_protect)?;
    // (tag, value) -> ()  — context-free Flow stash.
    let mut sig_throw = Signature::new(call_conv);
    sig_throw.params.push(AbiParam::new(i64t));
    sig_throw.params.push(AbiParam::new(i64t));
    let throw_id = declare(module, "neovm_jit_throw", &sig_throw)?;
    // (v) -> t/nil bits  — context-free predicates.
    let mut sig_pred1 = Signature::new(call_conv);
    sig_pred1.params.push(AbiParam::new(i64t));
    sig_pred1.returns.push(AbiParam::new(i64t));
    let intp_id = declare(module, "neovm_jit_integerp_slow", &sig_pred1)?;
    let nump_id = declare(module, "neovm_jit_numberp_slow", &sig_pred1)?;
    // (vmctx, idx, a[, b[, c]], out_ptr) -> status — generic direct builtins.
    let mut sig_b1 = Signature::new(call_conv);
    sig_b1.params.push(AbiParam::new(ptr_ty));
    sig_b1.params.push(AbiParam::new(i64t));
    sig_b1.params.push(AbiParam::new(i64t));
    sig_b1.params.push(AbiParam::new(ptr_ty));
    sig_b1.returns.push(AbiParam::new(i64t));
    let mut sig_b2 = sig_b1.clone();
    sig_b2.params.insert(3, AbiParam::new(i64t));
    let mut sig_b3 = sig_b2.clone();
    sig_b3.params.insert(4, AbiParam::new(i64t));
    let b1_id = declare(module, "neovm_jit_builtin1", &sig_b1)?;
    let b2_id = declare(module, "neovm_jit_builtin2", &sig_b2)?;
    let b3_id = declare(module, "neovm_jit_builtin3", &sig_b3)?;
    // (vmctx, array, index) -> element bits | VALUE_SHIM_SIGNAL
    let mut sig_aref = Signature::new(call_conv);
    sig_aref.params.push(AbiParam::new(ptr_ty));
    sig_aref.params.push(AbiParam::new(i64t));
    sig_aref.params.push(AbiParam::new(i64t));
    sig_aref.returns.push(AbiParam::new(i64t));
    // (vmctx, array, index, value) -> value bits | VALUE_SHIM_*
    let mut sig_aset = sig_aref.clone();
    sig_aset.params.push(AbiParam::new(i64t));
    let aref_id = declare(module, "neovm_jit_aref", &sig_aref)?;
    let aset_id = declare(module, "neovm_jit_aset", &sig_aset)?;
    let memq_id = declare(module, "neovm_jit_memq", &sig_aref)?;
    let assq_id = declare(module, "neovm_jit_assq", &sig_aref)?;
    let setcar_id = declare(module, "neovm_jit_setcar", &sig_aref)?;
    let setcdr_id = declare(module, "neovm_jit_setcdr", &sig_aref)?;
    // (vmctx, target, stack_len) -> ()  — condition-case push (infallible).
    let mut sig_pcc = Signature::new(call_conv);
    sig_pcc.params.push(AbiParam::new(ptr_ty));
    sig_pcc.params.push(AbiParam::new(i64t));
    sig_pcc.params.push(AbiParam::new(i64t));
    // (vmctx, target, stack_len, conditions/tag) -> ()
    let mut sig_pcc_raw = sig_pcc.clone();
    sig_pcc_raw.params.push(AbiParam::new(i64t));
    let pcc_id = declare(module, "neovm_jit_push_cc", &sig_pcc)?;
    let pcc_raw_id = declare(module, "neovm_jit_push_cc_raw", &sig_pcc_raw)?;
    let pcatch_id = declare(module, "neovm_jit_push_catch", &sig_pcc_raw)?;
    let pop_handler_id = declare(module, "neovm_jit_pop_handler", &sig_save)?;
    // (vmctx, ours, out_ptr) -> matched ordinal or -1.
    let match_id = declare(module, "neovm_jit_match_handler", &sig_varref)?;
    // (vmctx, dispatch, table) -> raw target addr / miss / stale.
    let switch_id = declare(module, "neovm_jit_switch", &sig_eq)?;
    // () -> ()  — stash the stale-table signal.
    let sig_void = Signature::new(call_conv);
    let switch_stale_id = declare(module, "neovm_jit_switch_stale", &sig_void)?;
    // (args_ptr, nargs) -> list bits  — infallible n-ary list builder.
    let mut sig_list = Signature::new(call_conv);
    sig_list.params.push(AbiParam::new(ptr_ty));
    sig_list.params.push(AbiParam::new(i64t));
    sig_list.returns.push(AbiParam::new(i64t));
    let list_id = declare(module, "neovm_jit_list", &sig_list)?;
    // (idx, args_ptr, nargs, out_ptr) -> status  — slice-shaped builtins.
    let mut sig_slice = Signature::new(call_conv);
    sig_slice.params.push(AbiParam::new(i64t));
    sig_slice.params.push(AbiParam::new(ptr_ty));
    sig_slice.params.push(AbiParam::new(i64t));
    sig_slice.params.push(AbiParam::new(ptr_ty));
    sig_slice.returns.push(AbiParam::new(i64t));
    let slice_id = declare(module, "neovm_jit_builtin_slice", &sig_slice)?;
    // (vmctx, variant, sym, args_ptr, nargs, out_ptr) -> status.
    let mut sig_named = Signature::new(call_conv);
    sig_named.params.push(AbiParam::new(ptr_ty));
    sig_named.params.push(AbiParam::new(i64t));
    sig_named.params.push(AbiParam::new(i64t));
    sig_named.params.push(AbiParam::new(ptr_ty));
    sig_named.params.push(AbiParam::new(i64t));
    sig_named.params.push(AbiParam::new(ptr_ty));
    sig_named.returns.push(AbiParam::new(i64t));
    let named_id = declare(module, "neovm_jit_named_builtin", &sig_named)?;
    // (vmctx, body, out_ptr) -> status.
    let swe_id = declare(module, "neovm_jit_save_window_excursion", &sig_varref)?;
    // (vmctx, sym, expected, slot_ptr, args_ptr, nargs, out_ptr) -> status.
    let mut sig_spec = Signature::new(call_conv);
    sig_spec.params.push(AbiParam::new(ptr_ty));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(ptr_ty));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(ptr_ty));
    sig_spec.returns.push(AbiParam::new(i64t));
    let call_spec_id = declare(module, "neovm_jit_call_spec", &sig_spec)?;
    // Gap 1: the subr-speculation shims, JIT-only (see the `subr_spec` doc).
    // call_subr_spec shares sig_spec's shape; pred/eq share one 7-param shape:
    // (vmctx, k1, k2, k3, k4, k5, out_ptr) -> status
    //   pred: (vmctx, kind, sym, expected, slot_ptr, a, out_ptr)
    //   eq:   (vmctx, sym, expected, slot_ptr, a, b, out_ptr)
    // arith adds one word for the second arg (kind + 2 args):
    //   arith: (vmctx, kind, sym, expected, slot_ptr, a, b, out_ptr)
    let subr_spec_refs = if subr_spec {
        let mut sig_pred = Signature::new(call_conv);
        sig_pred.params.push(AbiParam::new(ptr_ty));
        for _ in 0..5 {
            sig_pred.params.push(AbiParam::new(i64t));
        }
        sig_pred.params.push(AbiParam::new(ptr_ty));
        sig_pred.returns.push(AbiParam::new(i64t));
        let mut sig_arith = Signature::new(call_conv);
        sig_arith.params.push(AbiParam::new(ptr_ty)); // vmctx
        for _ in 0..6 {
            // kind, sym, expected, slot_ptr, a, b
            sig_arith.params.push(AbiParam::new(i64t));
        }
        sig_arith.params.push(AbiParam::new(ptr_ty)); // out
        sig_arith.returns.push(AbiParam::new(i64t));
        let subr_id = declare(module, "neovm_jit_call_subr_spec", &sig_spec)?;
        let pred_id = declare(module, "neovm_jit_pred_spec", &sig_pred)?;
        let eq_id = declare(module, "neovm_jit_eq_incl_props_spec", &sig_pred)?;
        let arith_id = declare(module, "neovm_jit_arith_spec", &sig_arith)?;
        Some((subr_id, pred_id, eq_id, arith_id))
    } else {
        None
    };
    // R2 CallBuiltinSym intrinsic shims (JIT-only, see doc). Tier-B dispatch-skip
    // shares `sig_call`'s shape (vmctx, sym, args_ptr, nargs, out_ptr) -> status;
    // Tier-A read adds a leading `which` discriminant
    // (vmctx, which, sym, args_ptr, nargs, out_ptr) -> status.
    let (cbsym_spec_id, cbsym_read_id) = if cbsym_spec {
        let mut sig_read = Signature::new(call_conv);
        sig_read.params.push(AbiParam::new(ptr_ty)); // vmctx
        sig_read.params.push(AbiParam::new(i64t)); // which
        sig_read.params.push(AbiParam::new(i64t)); // sym
        sig_read.params.push(AbiParam::new(ptr_ty)); // args_ptr
        sig_read.params.push(AbiParam::new(i64t)); // nargs
        sig_read.params.push(AbiParam::new(ptr_ty)); // out
        sig_read.returns.push(AbiParam::new(i64t));
        (
            Some(declare(module, "neovm_jit_cbsym_spec", &sig_call)?),
            Some(declare(module, "neovm_jit_cbsym_read", &sig_read)?),
        )
    } else {
        (None, None)
    };

    Ok(RtRefs {
        rootwin_grow: module.declare_func_in_func(rootwin_grow_id, func),
        cons: module.declare_func_in_func(cons_id, func),
        make_float: module.declare_func_in_func(make_float_id, func),
        arith_generic: module.declare_func_in_func(arith_generic_id, func),
        call: module.declare_func_in_func(call_id, func),
        apply: module.declare_func_in_func(apply_id, func),
        eq_slow: module.declare_func_in_func(eq_id, func),
        symbolp_slow: module.declare_func_in_func(symp_id, func),
        varref: module.declare_func_in_func(varref_id, func),
        varset: module.declare_func_in_func(varset_id, func),
        varbind: module.declare_func_in_func(varbind_id, func),
        unbind: module.declare_func_in_func(unbind_id, func),
        backedge: module.declare_func_in_func(backedge_id, func),
        save_current_buffer: module.declare_func_in_func(scb_id, func),
        save_excursion: module.declare_func_in_func(sexc_id, func),
        save_restriction: module.declare_func_in_func(sres_id, func),
        unwind_protect: module.declare_func_in_func(up_id, func),
        throw_flow: module.declare_func_in_func(throw_id, func),
        integerp_slow: module.declare_func_in_func(intp_id, func),
        numberp_slow: module.declare_func_in_func(nump_id, func),
        builtin1: module.declare_func_in_func(b1_id, func),
        builtin2: module.declare_func_in_func(b2_id, func),
        builtin3: module.declare_func_in_func(b3_id, func),
        aref: module.declare_func_in_func(aref_id, func),
        aset: module.declare_func_in_func(aset_id, func),
        memq: module.declare_func_in_func(memq_id, func),
        assq: module.declare_func_in_func(assq_id, func),
        setcar: module.declare_func_in_func(setcar_id, func),
        setcdr: module.declare_func_in_func(setcdr_id, func),
        push_cc: module.declare_func_in_func(pcc_id, func),
        push_cc_raw: module.declare_func_in_func(pcc_raw_id, func),
        push_catch: module.declare_func_in_func(pcatch_id, func),
        pop_handler: module.declare_func_in_func(pop_handler_id, func),
        match_handler: module.declare_func_in_func(match_id, func),
        switch_lookup: module.declare_func_in_func(switch_id, func),
        switch_stale: module.declare_func_in_func(switch_stale_id, func),
        list: module.declare_func_in_func(list_id, func),
        builtin_slice: module.declare_func_in_func(slice_id, func),
        named_builtin: module.declare_func_in_func(named_id, func),
        save_window_excursion: module.declare_func_in_func(swe_id, func),
        call_spec: module.declare_func_in_func(call_spec_id, func),
        call_subr_spec: subr_spec_refs.map(|(id, _, _, _)| module.declare_func_in_func(id, func)),
        pred_spec: subr_spec_refs.map(|(_, id, _, _)| module.declare_func_in_func(id, func)),
        eq_incl_props_spec: subr_spec_refs
            .map(|(_, _, id, _)| module.declare_func_in_func(id, func)),
        arith_spec: subr_spec_refs.map(|(_, _, _, id)| module.declare_func_in_func(id, func)),
        cbsym_spec: cbsym_spec_id.map(|id| module.declare_func_in_func(id, func)),
        cbsym_read: cbsym_read_id.map(|id| module.declare_func_in_func(id, func)),
    })
}

/// The per-leaf cells a precise-deopt exit writes through before returning
/// [`STATUS_DEOPT_AT`]: the failing op's bytecode index, the live operand
/// stack depth (the values themselves go to the spill buffer), and the number
/// of condition frames this frame had registered at that point. `Cell` makes
/// the native interior writes legal; the mutator is single-threaded and the
/// values are consumed immediately after the native call returns.
pub(crate) struct DeoptCells {
    pub(crate) pc: core::cell::Cell<i64>,
    pub(crate) depth: core::cell::Cell<i64>,
    pub(crate) handlers: core::cell::Cell<i64>,
}

/// A block-local virtual cons, reconstructed only on a precise deopt exit.
/// The allocator does not collect; all operands and previously reconstructed
/// objects remain valid until the complete interpreter stack is installed.
pub(crate) struct ConsReconstruction {
    slots: Vec<usize>,
    car: (ClifValue, bool),
    cdr: (ClifValue, bool),
    allocator: FuncRef,
}

/// A precise-deopt exit block queued at a guard-emitting op: created (and
/// targeted by that op's guards) during lowering, filled after the bytecode
/// block terminates. Captures the op's index and the operand stack snapshot
/// from BEFORE the op popped its operands — the interpreter reruns the
/// failing op itself.
pub(crate) struct PendingDeopt {
    cons_rebuilds: Vec<ConsReconstruction>,
    pub(crate) block: Block,
    pub(crate) pc: usize,
    pub(crate) handlers_len: usize,
    /// The framestate of the INLINED region this site sits in, if any: a
    /// deopt there resumes the interpreter at the call the region replaced,
    /// with the caller's pre-call stack, and re-runs the whole call.
    pub(crate) region: Option<RegionDeopt>,
    pub(crate) stack: Vec<ClifValue>,
    /// Per-slot representation snapshot: a [`SlotRep::RawFixnum`] slot holds
    /// an untagged i64 and is retagged in the cold deopt block before the
    /// framestate spill, since `run_resumed_frame` reads them back as tagged
    /// `Value`s.
    pub(crate) reps: Vec<SlotRep>,
}

impl PendingDeopt {
    /// Whether the snapshot holds an unboxed float, which its cold block
    /// will box.
    pub(crate) fn holds_flonum(&self) -> bool {
        self.reps.iter().any(|rep| rep.is_flonum())
    }
}

/// What a deopt inside an inlined region resumes with: the caller's operand
/// stack as it stood before the call (its residual, the callee object, and
/// the arguments), and the caller pc of that call. Captured once at region
/// entry — the region's own code never touches those slots, and re-reading
/// the argument slots later would be wrong anyway, since a callee may
/// `setq` its own parameter.
#[derive(Clone)]
pub(crate) struct RegionDeopt {
    pub(crate) call_site_pc: usize,
    pub(crate) stack: Vec<ClifValue>,
    pub(crate) reps: Vec<SlotRep>,
}

thread_local! {
    /// The region the lowering is inside, for every `deopt_site` it makes.
    static ACTIVE_REGION: std::cell::RefCell<Option<RegionDeopt>> =
        const { std::cell::RefCell::new(None) };
}

/// Enter (or leave) an inlined region for the ops lowered next.
pub(crate) fn set_active_region(region: Option<RegionDeopt>) {
    ACTIVE_REGION.with(|r| *r.borrow_mut() = region);
}

/// The call site of the region the lowering is inside, if any.
pub(crate) fn active_region_call_site() -> Option<usize> {
    ACTIVE_REGION.with(|r| r.borrow().as_ref().map(|region| region.call_site_pc))
}

/// Check, at a spliced region's first op, that the callee slot still holds the
/// object whose body was spliced — and deopt to the call if it does not.
///
/// The constant-propagation that picked the site is a SELECTION heuristic, not
/// a proof: it does not model jump-table edges, so a block reachable only
/// through a `Switch` can inherit a callee tag from an unrelated predecessor.
/// The baseline's own `Op::Call` speculation answers this the same way — bake
/// the expected callee, re-check it at run time — and here the deopt it needs
/// already exists, since a region deopt replays the whole call.
///
/// Must be called with `set_active_region` already set for this region, so the
/// site it queues carries the region's framestate.
pub(crate) fn emit_region_entry_guard(
    fb: &mut FunctionBuilder,
    region: &super::super::inline::InlineRegion,
    handlers_len: usize,
    stack: &[ClifValue],
    reps: &[SlotRep],
    pending: &mut Vec<PendingDeopt>,
) -> Result<(), CompileError> {
    // `frame_base` came from the UNFUSED caller's depths and `stack` is the
    // FUSED lowering's; they agree only because every region is exactly
    // stack-neutral. If they ever disagree the slot below names something other
    // than the callee, so refuse to compile rather than guard the wrong slot —
    // or, worse, emit no guard at all.
    if region.frame_base == 0 || stack.len() != region.frame_base + region.nargs {
        return Err(CompileError::UnsupportedOp("inline-region-depth"));
    }
    // The callee object sits one slot below the first argument.
    let slot = region.frame_base - 1;
    let (v, raw) = (stack[slot], reps[slot] == SlotRep::RawFixnum);
    let dsite = deopt_site(fb, region.call_site_pc, handlers_len, stack, reps, pending);
    // A raw slot holds an untagged fixnum, which is never a bytecode object:
    // retagging makes the comparison false, so such a site simply deopts.
    let v = if raw { retag_fixnum(fb, v) } else { v };
    let expected = fb.ins().iconst(types::I64, region.callee_bits as i64);
    let same = fb.ins().icmp(IntCC::Equal, v, expected);
    emit_guard(fb, dsite, same);
    Ok(())
}

/// Queue (and return) the precise-deopt block for the guard-emitting op at
/// bytecode index `pc`, capturing the pre-op operand stack + its slot
/// representations.
pub(crate) fn deopt_site(
    fb: &mut FunctionBuilder,
    pc: usize,
    handlers_len: usize,
    stack: &[ClifValue],
    reps: &[SlotRep],
    pending: &mut Vec<PendingDeopt>,
) -> Block {
    let block = fb.create_block();
    let region = ACTIVE_REGION.with(|r| r.borrow().clone());
    // `pc` indexes the ops being LOWERED, which after inlining is the fused
    // body — but a deopt resumes the interpreter in the original one, so an
    // unmapped fused index would be a wrong-instruction (or out-of-range)
    // resume. Inside a region the region supplies the call's own pc instead.
    let pc = match (&region, super::super::inline::active_fused()) {
        (Some(_), _) | (None, None) => pc,
        // Total over every lowered pc: `fuse_calls` refuses a body whose
        // inverse map does not cover its ops.
        (None, Some(fused)) => {
            debug_assert!(
                fused.caller_pc(pc).is_some(),
                "fused pc {pc} has no caller pc"
            );
            fused.caller_pc(pc).unwrap_or(pc)
        }
    };
    pending.push(PendingDeopt {
        cons_rebuilds: Vec::new(),
        block,
        pc,
        handlers_len,
        region,
        stack: stack.to_vec(),
        reps: reps.to_vec(),
    });
    block
}

/// How the precise-deopt blocks reach the leaf's deopt cells + spill buffer.
///
/// JIT (`Baked`): the four base addresses are stable Box pointers baked as
/// `iconst` LAZILY inside each cold deopt block (zero hot-path cost — the
/// pre-sidecar behavior). AOT (`Sidecar`): the bases are session-specific, so
/// they are LOADED from the per-thread `LeafSidecar` ONCE in the entry block
/// (which dominates the cold blocks) and shared as CLIF values. Splitting the
/// two keeps the JIT's CLIF unchanged (audit: hoisting the iconsts to the entry
/// block was a minor hot-path regression for JIT leaves with deopt sites).
#[derive(Clone, Copy)]
pub(crate) enum DeoptRefs {
    /// JIT: raw Box addresses, iconst'd lazily in each cold deopt block.
    Baked {
        spill_base: i64,
        meta_pc: i64,
        meta_depth: i64,
        meta_handlers: i64,
    },
    /// AOT: entry-block CLIF values loaded from the sidecar (dominate the cold
    /// blocks, so reused directly).
    Sidecar {
        spill_base: ClifValue,
        meta_pc: ClifValue,
        meta_depth: ClifValue,
        meta_handlers: ClifValue,
    },
}

thread_local! {
    /// IR-size facts of the most recent baseline compile on this thread —
    /// `(clif insts, blocks, deopt sites, deopt snapshot slots)` — read by
    /// `stats::record_compile` for its per-compile trace line. Diagnostic only.
    pub(crate) static LAST_IR_STATS: core::cell::Cell<(u32, u32, u32, u32)> =
        const { core::cell::Cell::new((0, 0, 0, 0)) };
}

/// Fill the precise-deopt blocks queued within one bytecode block: spill the
/// captured live stack, record pc/depth/handler-count, and return
/// [`STATUS_DEOPT_AT`]. For `Baked` (JIT) the base addresses are iconst'd HERE in
/// the cold block (off the hot path); for `Sidecar` (AOT) they are the
/// entry-block loaded values.
///
/// A flonum in a snapshot is boxed HERE, in the cold block, with the
/// representation known at compile time (`float_boxer` is the body's
/// `make_float` ref, present whenever a float site exists), before the spill
/// write — so the runtime framestate stays all-tagged, and nothing allocates
/// between the spill write and its readback. Aliases of one result share one
/// box. A snapshot at op `i` is entered only at op `i`: a box made at a later
/// op has not run yet, and one made earlier already turned the aliases
/// `Tagged` before the snapshot was taken, so a cold box never duplicates an
/// identity that escaped.
pub(crate) fn emit_pending_deopts(
    fb: &mut FunctionBuilder,
    refs: DeoptRefs,
    pending: &mut Vec<PendingDeopt>,
    float_boxer: Option<FuncRef>,
) {
    LAST_IR_STATS.with(|c| {
        let (i, b, sites, slots) = c.get();
        let add_slots: usize = pending.iter().map(|pd| pd.stack.len()).sum();
        c.set((
            i,
            b,
            sites.saturating_add(pending.len() as u32),
            slots.saturating_add(add_slots as u32),
        ));
    });
    for pd in pending.drain(..) {
        fb.switch_to_block(pd.block);
        fb.seal_block(pd.block);
        // Materialize the four bases. For Baked, the iconsts live in THIS cold
        // block (the original JIT placement); for Sidecar they are entry values.
        let (spill_base, meta_pc, meta_depth, meta_handlers) = match refs {
            DeoptRefs::Baked {
                spill_base,
                meta_pc,
                meta_depth,
                meta_handlers,
            } => (
                fb.ins().iconst(types::I64, spill_base),
                fb.ins().iconst(types::I64, meta_pc),
                fb.ins().iconst(types::I64, meta_depth),
                fb.ins().iconst(types::I64, meta_handlers),
            ),
            DeoptRefs::Sidecar {
                spill_base,
                meta_pc,
                meta_depth,
                meta_handlers,
            } => (spill_base, meta_pc, meta_depth, meta_handlers),
        };
        // Inside an inlined region the interpreter must resume at the CALL,
        // with the stack the caller had before it — the region's own operand
        // stack means nothing to it.
        let (pc, stack, reps) = match &pd.region {
            Some(region) => (
                region.call_site_pc,
                region.stack.as_slice(),
                region.reps.as_slice(),
            ),
            None => (pd.pc, pd.stack.as_slice(), pd.reps.as_slice()),
        };
        // Baseline sites borrow their snapshots unchanged. Only a virtual
        // cons needs a mutable copy; its aliases share one reconstructed cell.
        let mut stack = std::borrow::Cow::Borrowed(stack);
        for cons in &pd.cons_rebuilds {
            let tag = |fb: &mut FunctionBuilder, (v, raw)| {
                if raw { retag_fixnum(fb, v) } else { v }
            };
            let car = tag(fb, cons.car);
            let cdr = tag(fb, cons.cdr);
            let call = fb.ins().call(cons.allocator, &[car, cdr]);
            let value = fb.inst_results(call)[0];
            for &slot in &cons.slots {
                debug_assert_eq!(reps[slot], SlotRep::Tagged);
                stack.to_mut()[slot] = value;
            }
        }
        debug_assert!(
            pd.region.is_none() || !reps.iter().any(|rep| rep.is_flonum()),
            "a region's framestate is materialized at its entry"
        );
        let mut boxed: SmallVec<[(ClifValue, ClifValue); 8]> = SmallVec::new();
        for (j, &v) in stack.iter().enumerate() {
            // Retag raw fixnum slots (and box flonums) in the COLD deopt
            // block (zero hot-path cost): the framestate is read back as
            // tagged Values by run_resumed_frame.
            let tagged = snapshot_slot_tagged(fb, float_boxer, &mut boxed, v, reps[j], true);
            fb.ins()
                .store(MemFlagsData::trusted(), tagged, spill_base, (j * 8) as i32);
        }
        let pc_v = fb.ins().iconst(types::I64, pc as i64);
        fb.ins().store(MemFlagsData::trusted(), pc_v, meta_pc, 0);
        let depth_v = fb.ins().iconst(types::I64, stack.len() as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), depth_v, meta_depth, 0);
        let h_v = fb.ins().iconst(types::I64, pd.handlers_len as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), h_v, meta_handlers, 0);
        let code = fb.ins().iconst(types::I64, STATUS_DEOPT_AT);
        fb.ins().return_(&[code]);
    }
}

/// Build the [`DeoptRefs`] for this leaf.
///
/// JIT (`aot=false`) → `DeoptRefs::Baked` with the raw Box addresses: NOTHING is
/// emitted in the entry block; `emit_pending_deopts` iconst's them lazily inside
/// each cold deopt block (the original placement — no hot-path cost).
///
/// AOT (`aot=true`) → `DeoptRefs::Sidecar` with the bases LOADED from the
/// per-thread `LeafSidecar` in the ENTRY block (so they dominate the cold blocks
/// and are shared). MUST be called with `fb` in the entry block. Gated on
/// `has_precise_deopt`: a body with no precise-deopt site never reaches
/// `emit_pending_deopts`, and the `sidecar` may even be null (the raw-entry
/// pure-leaf path), so emit nothing/zero placeholders and dereference no sidecar.
#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_deopt_refs(
    fb: &mut FunctionBuilder,
    ptr_ty: Type,
    aot: bool,
    has_precise_deopt: bool,
    sidecar: Option<ClifValue>,
    spill_base_addr: i64,
    meta_pc_addr: i64,
    meta_depth_addr: i64,
    meta_handlers_addr: i64,
) -> DeoptRefs {
    if !aot {
        // JIT: defer the address iconsts to the cold deopt blocks. No entry-block
        // codegen here at all — keeps the hot path byte-identical to pre-sidecar.
        return DeoptRefs::Baked {
            spill_base: spill_base_addr,
            meta_pc: meta_pc_addr,
            meta_depth: meta_depth_addr,
            meta_handlers: meta_handlers_addr,
        };
    }
    if has_precise_deopt {
        let sc = sidecar.expect("AOT precise-deopt lowering requires the sidecar param");
        let load = |fb: &mut FunctionBuilder, off: i32| {
            fb.ins().load(ptr_ty, MemFlagsData::trusted(), sc, off)
        };
        DeoptRefs::Sidecar {
            spill_base: load(fb, LeafSidecar::OFF_SPILL_BASE),
            meta_pc: load(fb, LeafSidecar::OFF_META_PC),
            meta_depth: load(fb, LeafSidecar::OFF_META_DEPTH),
            meta_handlers: load(fb, LeafSidecar::OFF_META_HANDLERS),
        }
    } else {
        // AOT, no precise deopt: bases unused (pending is empty). Zero
        // placeholders, never a sidecar deref (sidecar may be null here).
        let z = fb.ins().iconst(ptr_ty, 0);
        DeoptRefs::Sidecar {
            spill_base: z,
            meta_pc: z,
            meta_depth: z,
            meta_handlers: z,
        }
    }
}

/// A handler-dispatch block queued at a `STATUS_SIGNAL` site inside a
/// protected extent: created (and branched to) at the site, filled after the
/// current bytecode block terminates by [`emit_pending_dispatches`]. Carries
/// the static handler list active at the site and the live operand-stack
/// snapshot (the site's SSA values dominate the dispatch block — it is their
/// only successor on the signal edge).
pub(crate) struct PendingDispatch {
    pub(crate) block: Block,
    pub(crate) handlers: Vec<HandlerStatic>,
    pub(crate) stack: Vec<ClifValue>,
    /// `stack`'s slot representations: never [`SlotRep::RawFixnum`] (every
    /// signal site runs after the raw retag).
    pub(crate) reps: Vec<SlotRep>,
}

/// Where a `STATUS_SIGNAL` site should branch: with no active handlers, the
/// shared signal-exit block (today's behavior); inside a protected extent, a
/// per-site dispatch block that will call the match shim.
pub(crate) fn signal_target_for_site(
    fb: &mut FunctionBuilder,
    signal_exit: &mut Option<Block>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    stack: &[ClifValue],
    reps: &[SlotRep],
) -> Block {
    if handlers.is_empty() {
        return *signal_exit.get_or_insert_with(|| fb.create_block());
    }
    let block = fb.create_block();
    let reps = &reps[..stack.len()];
    debug_assert!(
        !reps.contains(&SlotRep::RawFixnum),
        "signal snapshots are taken after the raw retag"
    );
    pending.push(PendingDispatch {
        block,
        handlers: handlers.to_vec(),
        stack: stack.to_vec(),
        reps: reps.to_vec(),
    });
    block
}

/// Fill the dispatch blocks queued by [`signal_target_for_site`] within one
/// bytecode block (called after its terminator, when the builder can switch
/// blocks). Each dispatch: root the live operand stack (the match shim can run
/// lisp — unwind-protect cleanups, handler-bind handlers, signal hooks — and
/// GC), call the match shim, and map the returned ordinal (`m` misses from the
/// innermost handler; -1 = propagate) onto the statically known handler
/// targets: re-materialize the handler's entry stack (the current model values
/// below its push depth + the error value the shim wrote through the result
/// slot) and jump to its block.
///
/// A flonum in the snapshot is not rooted (it holds no reference) and is
/// boxed in each hit block, AFTER the match shim — which may run Lisp and
/// collect — so no box crosses a safe point; aliases share one box.
pub(crate) fn emit_pending_dispatches(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    signal_exit: &mut Option<Block>,
    vars: &[Variable],
    block_for: &HashMap<usize, Block>,
    pending: &mut Vec<PendingDispatch>,
) -> Result<(), CompileError> {
    for pd in pending.drain(..) {
        fb.switch_to_block(pd.block);
        fb.seal_block(pd.block);
        // Emitted after the whole bytecode block, but ENTERED from the signal
        // edge of one earlier site: the store record as it stands now belongs
        // to the block's end, not to that site (a fast path that stored
        // nothing, a later site that did). A join with a different store
        // history — rule 3 on `RootWinCarry`. Cold code; nothing to save.
        rootwin_carry_reset();
        let saved = if pd.stack.is_empty() {
            CondRoots::NONE
        } else {
            emit_model_roots_pre(fb, rt, &pd.stack, &pd.reps)
        };
        let vmctx = fb.use_var(rt.vmctx_var);
        let ours = fb.ins().iconst(types::I64, pd.handlers.len() as i64);
        let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
        let call = fb
            .ins()
            .call(rt.refs.match_handler, &[vmctx, ours, out_addr]);
        let idx = fb.inst_results(call)[0];
        emit_cond_residual_roots_post(fb, rt, saved);
        // Compare chain over the (small) static handler list: shim ordinal
        // m counts misses from the top, so m maps to handlers[len-1-m].
        let k = pd.handlers.len();
        for m in 0..k {
            let (target, push_depth) = pd.handlers[k - 1 - m];
            if push_depth > pd.stack.len() {
                // The byte-compiler keeps the operand stack at or above the
                // protected base inside the extent; anything else is exotic —
                // bail to the interpreter.
                return Err(CompileError::UnsupportedOp("handler-depth"));
            }
            let hit = fb.create_block();
            let next = fb.create_block();
            let is_m = icmp_imm_p(fb, IntCC::Equal, idx, m as i64);
            fb.ins().brif(is_m, hit, &[], next, &[]);
            fb.switch_to_block(hit);
            fb.seal_block(hit);
            let mut boxed: SmallVec<[(ClifValue, ClifValue); 8]> = SmallVec::new();
            for (j, &v) in pd.stack.iter().take(push_depth).enumerate() {
                let v = snapshot_slot_tagged(
                    fb,
                    Some(rt.refs.make_float),
                    &mut boxed,
                    v,
                    pd.reps[j],
                    false,
                );
                fb.def_var(vars[j], v);
            }
            let err = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            fb.def_var(vars[push_depth], err);
            fb.ins().jump(block_for[&target], &[]);
            fb.switch_to_block(next);
            fb.seal_block(next);
        }
        let se = *signal_exit.get_or_insert_with(|| fb.create_block());
        fb.ins().jump(se, &[]);
    }
    Ok(())
}

/// How baseline model-stack slot `k` holds its value: the lowering keeps one
/// per `stack` slot, in lockstep. Lowering-time only — nothing here reaches
/// runtime metadata (deopt spills stay all-tagged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotRep {
    /// `stack[k]` is a tagged Lisp `Value`.
    Tagged,
    /// `stack[k]` is an untagged, proven fixnum (cross-op fixnum unboxing).
    RawFixnum,
    /// A float-site result that is not boxed yet (a "flonum"). `stack[k]` is
    /// its TAG WORD: [`UNBOXED_FLOAT_TAG_WORD`] when the value is the float
    /// `f64`, otherwise the tagged fixnum the site's both-fixnum arm
    /// produced. `f64` is ALWAYS the value's double view (for a fixnum, GNU's
    /// `(double) XFIXNUM`). The tag word is never a heap reference: it is
    /// never rooted, stored, returned or passed as a `Value`, and boxing
    /// ([`box_flonum_slot`]) is the only way out.
    ///
    /// `f64` is also the value's IDENTITY: every result is a fresh SSA value
    /// (a new `fadd`, a new merge parameter) and every copy of one result
    /// (`Dup`, `StackRef`, `StackSet`) copies this rep, so two slots with
    /// equal reps are the same Lisp object and must share one box.
    Flonum { f64: ClifValue, kind: FlonumKind },
}

/// What a [`SlotRep::Flonum`] can hold at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlonumKind {
    /// Statically a float (an operand was statically a float, so GNU's
    /// contagion makes the result a float on every path): the tag word is
    /// the sentinel on every path, so no run-time test is ever emitted.
    Float,
    /// A float or a fixnum, decided by the tag word at run time: a
    /// `Float`-feedback site whose operands were both fixnums computes a
    /// fixnum (`(+ 2 3)` is 5, not 5.0).
    FloatOrFixnum,
}

/// `TAG_FLOAT` with a null pointer: the tag word of a [`SlotRep::Flonum`]
/// that holds a float. Never a valid `Value` (arena float pointers are
/// non-null). It passes the float tag test `(w & 7) == 7`, so the fused
/// `both_tag_test` classifies a flonum exactly like a boxed float, and fails
/// the fixnum test `(w & 3) == 2`. A word that must never be published:
/// root-window stores, the GC's window seeding and the deopt readback
/// assert it in debug builds.
pub(crate) const UNBOXED_FLOAT_TAG_WORD: i64 = crate::tagged::value::TAG_FLOAT as i64;

impl SlotRep {
    /// [`SlotRep::RawFixnum`] when `raw`, else [`SlotRep::Tagged`].
    pub(crate) fn raw_if(raw: bool) -> Self {
        if raw { Self::RawFixnum } else { Self::Tagged }
    }

    /// An unboxed float result (either kind).
    pub(crate) fn is_flonum(self) -> bool {
        matches!(self, Self::Flonum { .. })
    }

    /// A flonum that is a float on every path ([`FlonumKind::Float`]).
    pub(crate) fn is_static_float(self) -> bool {
        matches!(
            self,
            Self::Flonum {
                kind: FlonumKind::Float,
                ..
            }
        )
    }
}

/// What the flonum lowering did in the function being lowered: the census
/// its tests assert and its `compile` trace event reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FlonumCensus {
    /// Float-site results left unboxed.
    pub(crate) results: u32,
    /// Boxes made on the ordinary path, where a flonum escapes (one per
    /// result, however many aliases it has).
    pub(crate) escape_boxes: u32,
    /// Boxes made in deopt exits and handler-dispatch blocks.
    pub(crate) cold_boxes: u32,
}

thread_local! {
    static FLONUM_CENSUS: std::cell::Cell<FlonumCensus> =
        const { std::cell::Cell::new(FlonumCensus { results: 0, escape_boxes: 0, cold_boxes: 0 }) };
}

/// Start the flonum census for a new function.
pub(crate) fn flonum_census_reset() {
    FLONUM_CENSUS.with(|c| c.set(FlonumCensus::default()));
}

/// The flonum census since the last [`flonum_census_reset`].
pub(crate) fn flonum_census() -> FlonumCensus {
    FLONUM_CENSUS.with(|c| c.get())
}

fn flonum_census_note(update: impl FnOnce(&mut FlonumCensus)) {
    FLONUM_CENSUS.with(|c| {
        let mut census = c.get();
        update(&mut census);
        c.set(census);
    });
}

/// Box one unboxed float result: [`FlonumKind::Float`] is a straight
/// `neovm_jit_make_float` call; [`FlonumKind::FloatOrFixnum`] is the diamond
/// `tag == SENTINEL ? make_float(f64) : tag` (a fixnum result is its own tag
/// word). GNU made exactly one object for the result (`data.c`
/// `float_arith_driver` ends in `make_float`), so this runs once per result.
///
/// `make_float` never reaches a GC safe point, so earlier boxes may wait in
/// registers while this one is made, and it stores nothing in the root
/// window: the `RootWinCarry` record holds on both sides of the diamond.
/// `cold` marks the diamond's blocks cold (a deopt exit).
pub(crate) fn box_flonum_value(
    fb: &mut FunctionBuilder,
    make_float: FuncRef,
    tag: ClifValue,
    f64: ClifValue,
    kind: FlonumKind,
    cold: bool,
) -> ClifValue {
    match kind {
        FlonumKind::Float => {
            let call = fb.ins().call(make_float, &[f64]);
            fb.inst_results(call)[0]
        }
        FlonumKind::FloatOrFixnum => {
            let out = fb.declare_var(types::I64);
            let box_b = fb.create_block();
            let merge = fb.create_block();
            if cold {
                fb.set_cold_block(box_b);
                fb.set_cold_block(merge);
            }
            fb.def_var(out, tag);
            let is_float = icmp_imm_p(fb, IntCC::Equal, tag, UNBOXED_FLOAT_TAG_WORD);
            fb.ins().brif(is_float, box_b, &[], merge, &[]);
            fb.switch_to_block(box_b);
            fb.seal_block(box_b);
            let call = fb.ins().call(make_float, &[f64]);
            let boxed = fb.inst_results(call)[0];
            fb.def_var(out, boxed);
            fb.ins().jump(merge, &[]);
            fb.switch_to_block(merge);
            fb.seal_block(merge);
            fb.use_var(out)
        }
    }
}

/// Box flonum slot `k` where it escapes, and rewrite EVERY model-stack alias
/// of it (same rep, see [`SlotRep::Flonum`]) to the box: GNU made one object
/// for this result, all its copies must stay `eq`, and a later escape of an
/// alias must reuse the box. A no-op on a slot that is not a flonum.
///
/// Must run on the op's main line (it dominates the rest of the block),
/// never inside one arm of an op's own branching.
pub(crate) fn box_flonum_slot(
    fb: &mut FunctionBuilder,
    make_float: FuncRef,
    stack: &mut [ClifValue],
    reps: &mut [SlotRep],
    k: usize,
) -> ClifValue {
    let key = reps[k];
    let SlotRep::Flonum { f64, kind } = key else {
        return stack[k];
    };
    let boxed = box_flonum_value(fb, make_float, stack[k], f64, kind, false);
    for s in 0..stack.len() {
        if reps[s] == key {
            stack[s] = boxed;
            reps[s] = SlotRep::Tagged;
        }
    }
    flonum_census_note(|c| c.escape_boxes += 1);
    boxed
}

/// Force every raw fixnum slot in the model stack back to a tagged `Value`
/// (flonums stay unboxed).
pub(crate) fn retag_raw_fixnums(
    fb: &mut FunctionBuilder,
    stack: &mut [ClifValue],
    reps: &mut [SlotRep],
) {
    for k in 0..stack.len() {
        stack_force_tagged(fb, stack, reps, k);
    }
}

/// Box every flonum in the model stack (alias-shared), leaving raw fixnums
/// raw: the flonum half of [`materialize_model_stack`], for the block edges,
/// which keep a raw fixnum raw when its successor variable is raw.
pub(crate) fn box_all_flonums(
    fb: &mut FunctionBuilder,
    rt: Option<&RtCtx>,
    stack: &mut [ClifValue],
    reps: &mut [SlotRep],
) {
    for k in 0..stack.len() {
        if reps[k].is_flonum() {
            let rt = rt.expect("a flonum implies the runtime refs (float sites declare them)");
            box_flonum_slot(fb, rt.refs.make_float, stack, reps, k);
        }
    }
}

/// Force the whole model stack to tagged `Value`s: retag every raw fixnum
/// and box every flonum (alias-shared). Before an op or terminator that
/// roots, snapshots or passes the stack as `Value`s.
pub(crate) fn materialize_model_stack(
    fb: &mut FunctionBuilder,
    rt: Option<&RtCtx>,
    stack: &mut [ClifValue],
    reps: &mut [SlotRep],
) {
    for k in 0..stack.len() {
        match reps[k] {
            SlotRep::Tagged => {}
            SlotRep::RawFixnum => stack_force_tagged(fb, stack, reps, k),
            SlotRep::Flonum { .. } => {
                let rt = rt.expect("a flonum implies the runtime refs (float sites declare them)");
                box_flonum_slot(fb, rt.refs.make_float, stack, reps, k);
            }
        }
    }
}

/// Residual roots for a baseline site: its `Tagged` slots only. A flonum
/// holds no reference (a raw f64 cannot go stale: Lisp floats are
/// immutable), and its tag word must never reach the GC. An empty set
/// resets the root-window record, as [`emit_cond_residual_roots_pre`] does.
pub(crate) fn emit_model_roots_pre(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    stack: &[ClifValue],
    reps: &[SlotRep],
) -> CondRoots {
    let reps = &reps[..stack.len()];
    if !reps.iter().any(|rep| rep.is_flonum()) {
        debug_assert!(
            !reps.contains(&SlotRep::RawFixnum),
            "rooting sites run after the raw retag"
        );
        return emit_cond_residual_roots_pre(fb, rt, stack);
    }
    let tagged: SmallVec<[ClifValue; 16]> = stack
        .iter()
        .zip(reps)
        .filter_map(|(&v, &rep)| match rep {
            SlotRep::Tagged => Some(v),
            // Not a reference either; never reached (the retag runs first).
            SlotRep::RawFixnum => {
                debug_assert!(false, "rooting sites run after the raw retag");
                None
            }
            SlotRep::Flonum { .. } => None,
        })
        .collect();
    emit_cond_residual_roots_pre(fb, rt, &tagged)
}

/// The tagged value of snapshot slot `slot` in a cold exit (a deopt spill,
/// a handler entry): a raw fixnum is retagged, and a flonum is boxed once
/// per snapshot — `boxed` maps each flonum's `f64` to its box, so aliases
/// share one object (the pattern of the MIR virtual-cons rebuild).
fn snapshot_slot_tagged(
    fb: &mut FunctionBuilder,
    make_float: Option<FuncRef>,
    boxed: &mut SmallVec<[(ClifValue, ClifValue); 8]>,
    v: ClifValue,
    rep: SlotRep,
    cold: bool,
) -> ClifValue {
    match rep {
        SlotRep::Tagged => v,
        SlotRep::RawFixnum => retag_fixnum(fb, v),
        SlotRep::Flonum { f64, kind } => {
            if let Some(&(_, b)) = boxed.iter().find(|&&(key, _)| key == f64) {
                return b;
            }
            let make_float =
                make_float.expect("a flonum implies the runtime refs (float sites declare them)");
            let b = box_flonum_value(fb, make_float, v, f64, kind, cold);
            boxed.push((f64, b));
            flonum_census_note(|c| c.cold_boxes += 1);
            b
        }
    }
}

/// Get model-stack slot `k` as a RAW (untagged) fixnum i64 for arithmetic. If the
/// slot is already raw (a prior fixnum arithmetic result in this block), return it
/// directly — the cross-op fast path: no re-guard, no re-untag. Otherwise guard it
/// is a fixnum (deopt else, honoring the cross-block `known` elision) and untag once.
///
/// A flonum is guarded on its tag word, ALWAYS — never elided through `known`
/// or [`is_known_fixnum`] (the 2026-09-14 known-fixnum miscompile): a fixnum
/// tag word is the value, the float sentinel deopts, and the deopt exit boxes
/// the float so the interpreter reruns the op on a real one.
pub(crate) fn stack_as_raw(
    fb: &mut FunctionBuilder,
    deopt: Block,
    stack: &[ClifValue],
    reps: &[SlotRep],
    k: usize,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    match reps[k] {
        SlotRep::RawFixnum => stack[k],
        SlotRep::Tagged => {
            guard_fixnum(fb, deopt, stack[k], known);
            sshr_imm_p(fb, stack[k], FIXNUM_SHIFT as i64)
        }
        SlotRep::Flonum { .. } => {
            let is_fix = fixnum_tag_test(fb, stack[k]);
            emit_guard(fb, deopt, is_fix);
            sshr_imm_p(fb, stack[k], FIXNUM_SHIFT as i64)
        }
    }
}

/// Retag model-stack slot `k` to a tagged `Value` if it currently holds a raw
/// fixnum, clearing its raw flag. Used at every boundary where a value escapes the
/// in-flight arithmetic (returns, predicates, car/cdr, calls/gc roots, cross-block
/// edges, deopt/signal snapshots).
pub(crate) fn stack_force_tagged(
    fb: &mut FunctionBuilder,
    stack: &mut [ClifValue],
    reps: &mut [SlotRep],
    k: usize,
) {
    if reps[k] == SlotRep::RawFixnum {
        stack[k] = retag_fixnum(fb, stack[k]);
        reps[k] = SlotRep::Tagged;
    }
}

/// Ops that participate in cross-op fixnum unboxing: they maintain `reps`
/// themselves (arithmetic produces raw results, comparisons consume raw operands,
/// stack shuffles move the raw flags, VarRef tags a separate slow-path snapshot).
/// EVERY OTHER op force-tags the stack first
/// (so its gc_push / signal snapshot / shim args never observe a raw slot) and has
/// its mask re-synced by the caller.
pub(crate) fn op_preserves_raw(op: &Op) -> bool {
    matches!(
        op,
        Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq
            | Op::Constant(_)
            | Op::Nil
            | Op::True
            | Op::Pop
            | Op::Dup
            | Op::StackRef(_)
            | Op::StackSet(_)
            | Op::DiscardN(_)
            | Op::VarRef(_)
    )
}

/// An arithmetic or comparison site whose feedback says its fixnum guard
/// fails there (`compile::arith_site_takes_generic`): an inline fixnum fast
/// path whose every miss — a non-fixnum operand, a result out of fixnum range,
/// a zero divisor — calls the interpreter's own builtin through
/// `neovm_jit_arith_generic`, instead of deopting to the interpreter for the
/// rest of the activation.
///
/// * The whole model stack is tagged first: the fallback roots the residual
///   and a signal re-materializes it for a handler, and neither may see a
///   raw fixnum.
/// * The fast path stores no root and runs no nested activation; the
///   fallback roots the residual around its call. The continuation is the
///   meet of the two store histories (`RootWinCarry` rule 2).
/// * The result is TAGGED on both paths (the fallback's may be a bignum or a
///   float), so it is pushed as a tagged value — and the known-fixnum
///   analysis, reading the same predicate, does not call it a fixnum.
#[allow(clippy::too_many_arguments)]
fn lower_generic_arith_site(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    op: &Op,
    stack: &mut Vec<ClifValue>,
    reps: &mut Vec<SlotRep>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    signal_exit: &mut Option<Block>,
) -> Result<(), CompileError> {
    let (kind, nargs) = crate::emacs_core::bytecode::Vm::arith_generic_kind(op)
        .ok_or(CompileError::UnsupportedOp("arith-generic"))?;
    if stack.len() < nargs {
        return Err(CompileError::StackUnderflow);
    }
    materialize_model_stack(fb, Some(rt), stack, reps);
    let at = stack.len() - nargs;
    let operands: Vec<ClifValue> = stack[at..].to_vec();
    stack.truncate(at);
    reps.truncate(at);

    let res_var = fb.declare_var(types::I64);
    let fix_b = fb.create_block();
    let gen_b = fb.create_block();
    let merge = fb.create_block();

    // Fast path: fixnum operands, computed inline.
    let is_fix = if nargs == 2 {
        both_tag_test(
            fb,
            operands[0],
            operands[1],
            FIXNUM_CHECK_MASK as i64,
            FIXNUM_CHECK_VALUE as i64,
        )
    } else {
        fixnum_tag_test(fb, operands[0])
    };
    fb.ins().brif(is_fix, fix_b, &[], gen_b, &[]);
    fb.switch_to_block(fix_b);
    fb.seal_block(fix_b);
    let raw = |fb: &mut FunctionBuilder, v: ClifValue| sshr_imm_p(fb, v, FIXNUM_SHIFT as i64);
    let fast = match op {
        Op::Add | Op::Sub => {
            let (a, b) = (raw(fb, operands[0]), raw(fb, operands[1]));
            let r = raw_fixnum_addsub(fb, gen_b, matches!(op, Op::Sub), a, b);
            retag_fixnum(fb, r)
        }
        Op::Mul => {
            let (a, b) = (raw(fb, operands[0]), raw(fb, operands[1]));
            let r = raw_fixnum_mul(fb, gen_b, a, b);
            retag_fixnum(fb, r)
        }
        Op::Div | Op::Rem => {
            let (a, b) = (raw(fb, operands[0]), raw(fb, operands[1]));
            let r = raw_fixnum_divrem(fb, gen_b, matches!(op, Op::Rem), a, b);
            retag_fixnum(fb, r)
        }
        Op::Max | Op::Min => {
            // Tagging is monotonic, so selecting on the tagged words picks the
            // same operand.
            let cc = if matches!(op, Op::Min) {
                IntCC::SignedLessThan
            } else {
                IntCC::SignedGreaterThan
            };
            let cond = fb.ins().icmp(cc, operands[0], operands[1]);
            fb.ins().select(cond, operands[0], operands[1])
        }
        Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => {
            let cc = match op {
                Op::Eqlsign => IntCC::Equal,
                Op::Lss => IntCC::SignedLessThan,
                Op::Gtr => IntCC::SignedGreaterThan,
                Op::Leq => IntCC::SignedLessThanOrEqual,
                _ => IntCC::SignedGreaterThanOrEqual,
            };
            let cond = fb.ins().icmp(cc, operands[0], operands[1]);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            fb.ins().select(cond, t, nil)
        }
        Op::Add1 | Op::Sub1 | Op::Negate => {
            let a = raw(fb, operands[0]);
            let unary = match op {
                Op::Add1 => UnaryKind::Add1,
                Op::Sub1 => UnaryKind::Sub1,
                _ => UnaryKind::Negate,
            };
            let r = raw_fixnum_unop(fb, gen_b, unary, a);
            retag_fixnum(fb, r)
        }
        _ => unreachable!("arith_generic_kind admitted {op:?}"),
    };
    fb.def_var(res_var, fast);
    fb.ins().jump(merge, &[]);

    // Fallback: every miss above branches here (the tag test, and the range
    // and divisor guards, which target this block instead of a deopt).
    fb.switch_to_block(gen_b);
    fb.seal_block(gen_b);
    let carry_fast = rootwin_carry_snapshot();
    let saved = if stack.is_empty() {
        CondRoots::NONE
    } else {
        emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
    };
    // Operands in registers: `call_args_slot` is sized for the body's
    // CALL sites only, so it must not carry them.
    let vmctx = fb.use_var(rt.vmctx_var);
    let kind_v = fb.ins().iconst(types::I64, kind);
    let second = if nargs == 2 { operands[1] } else { operands[0] };
    let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
    let call = fb.ins().call(
        rt.refs.arith_generic,
        &[vmctx, kind_v, operands[0], second, out_addr],
    );
    let status = fb.inst_results(call)[0];
    emit_cond_residual_roots_post(fb, rt, saved);
    rootwin_carry_meet(&carry_fast);
    let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
    let ok_b = fb.create_block();
    let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(ok, ok_b, &[], se, &[]);
    fb.switch_to_block(ok_b);
    fb.seal_block(ok_b);
    let slow = fb
        .ins()
        .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
    fb.def_var(res_var, slow);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    stack.push(fb.use_var(res_var));
    reps.push(SlotRep::Tagged);
    Ok(())
}

/// A `Float`-feedback `+ - * /` site under [`FlonumMode::Off`]: the result
/// is boxed at the site (`neovm_jit_make_float`), as before unboxed floats.
fn lower_float_site_arith_boxed(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    op: &Op,
    dsite: Block,
    stack: &mut Vec<ClifValue>,
    reps: &mut Vec<SlotRep>,
) {
    let n = stack.len();
    let res_var = fb.declare_var(types::I64);
    let ff_b = fb.create_block();
    let slow_b = fb.create_block();
    let fix_b = fb.create_block();
    let mix_b = fb.create_block();
    let merge = fb.create_block();
    let both_float = both_float_test(fb, stack, reps, n - 2, n - 1);
    fb.ins().brif(both_float, ff_b, &[], slow_b, &[]);

    // Both floats (the predicted case): unbox and compute.
    fb.switch_to_block(ff_b);
    fb.seal_block(ff_b);
    let fa_val = unbox_float(fb, stack[n - 2]);
    let fb_val = unbox_float(fb, stack[n - 1]);
    let res = emit_float_arith(fb, op, fa_val, fb_val);
    let boxed = box_float(fb, rt, res);
    fb.def_var(res_var, boxed);
    fb.ins().jump(merge, &[]);

    // Not both floats: both fixnums, or a mix, or a non-number.
    fb.switch_to_block(slow_b);
    fb.seal_block(slow_b);
    let both_fix = both_fixnum_test(fb, stack, reps, n - 2, n - 1);
    fb.ins().brif(both_fix, fix_b, &[], mix_b, &[]);

    fb.switch_to_block(fix_b);
    fb.seal_block(fix_b);
    let a = if reps[n - 2] == SlotRep::RawFixnum {
        stack[n - 2]
    } else {
        sshr_imm_p(fb, stack[n - 2], FIXNUM_SHIFT as i64)
    };
    let b = if reps[n - 1] == SlotRep::RawFixnum {
        stack[n - 1]
    } else {
        sshr_imm_p(fb, stack[n - 1], FIXNUM_SHIFT as i64)
    };
    let raw = match op {
        Op::Add | Op::Sub => raw_fixnum_addsub(fb, dsite, matches!(op, Op::Sub), a, b),
        Op::Mul => raw_fixnum_mul(fb, dsite, a, b),
        _ => raw_fixnum_divrem(fb, dsite, false, a, b),
    };
    let tagged = retag_fixnum(fb, raw);
    fb.def_var(res_var, tagged);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(mix_b);
    fb.seal_block(mix_b);
    let fb_val = stack_as_f64_or_promote(fb, dsite, stack, reps, n - 1);
    let fa_val = stack_as_f64_or_promote(fb, dsite, stack, reps, n - 2);
    let res = emit_float_arith(fb, op, fa_val, fb_val);
    let boxed = box_float(fb, rt, res);
    fb.def_var(res_var, boxed);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    let out = fb.use_var(res_var);
    stack.truncate(n - 2);
    reps.truncate(n - 2);
    stack.push(out);
    reps.push(SlotRep::Tagged);
}

/// A `Float`-feedback `+ - * /` site whose result stays UNBOXED: a
/// [`SlotRep::Flonum`] the next float op or compare reads directly, boxed
/// only where it escapes ([`box_flonum_slot`] and the snapshot helpers).
///
/// The dispatch is the boxed lowering's — both floats (the predicted case),
/// both fixnums, or a mix, with a non-number deopting — but no arm allocates:
///
/// - both floats and the mix compute an `f64`, and the tag word is the
///   float sentinel;
/// - both fixnums keep GNU's fixnum result (`(+ 2 3)` is 5): the tag word is
///   the tagged fixnum and the `f64` its promotion, so a later float op
///   reads the right double and a later escape boxes nothing;
/// - an operand that is statically a float (GNU contagion) makes the result
///   one on every path: the fixnum arm is not emitted and the result is a
///   [`FlonumKind::Float`], whose tag word no consumer needs to test.
///
/// A flonum operand is read without boxing (its `f64`, or its tag word for
/// the tag tests and the fixnum arm); the deopt snapshot `dsite` boxes it in
/// the cold exit.
fn lower_float_site_arith(
    fb: &mut FunctionBuilder,
    op: &Op,
    dsite: Block,
    stack: &mut Vec<ClifValue>,
    reps: &mut Vec<SlotRep>,
) {
    let n = stack.len();
    let (ia, ib) = (n - 2, n - 1);
    let statically_float = reps[ia].is_static_float() || reps[ib].is_static_float();
    let tag_v = fb.declare_var(types::I64);
    let f_v = fb.declare_var(types::F64);
    let ff_b = fb.create_block();
    let slow_b = fb.create_block();
    let mix_b = fb.create_block();
    let merge = fb.create_block();
    // The float result of the ff and mix arms. The sentinel is a LOCAL
    // `iconst`, never the pooled 7: it feeds the merge's block parameter.
    let def_float = |fb: &mut FunctionBuilder, r: ClifValue| {
        if !statically_float {
            let sentinel = fb.ins().iconst(types::I64, UNBOXED_FLOAT_TAG_WORD);
            fb.def_var(tag_v, sentinel);
        }
        fb.def_var(f_v, r);
        fb.ins().jump(merge, &[]);
    };
    let both_float = both_float_test(fb, stack, reps, ia, ib);
    fb.ins().brif(both_float, ff_b, &[], slow_b, &[]);

    // Both floats (the predicted case): compute.
    fb.switch_to_block(ff_b);
    fb.seal_block(ff_b);
    let fa_val = float_payload(fb, stack, reps, ia);
    let fb_val = float_payload(fb, stack, reps, ib);
    let res = emit_float_arith(fb, op, fa_val, fb_val);
    def_float(fb, res);

    // Not both floats: both fixnums (unless an operand is statically a
    // float), or a mix, or a non-number.
    fb.switch_to_block(slow_b);
    fb.seal_block(slow_b);
    if statically_float {
        fb.ins().jump(mix_b, &[]);
    } else {
        let fix_b = fb.create_block();
        let both_fix = both_fixnum_test(fb, stack, reps, ia, ib);
        fb.ins().brif(both_fix, fix_b, &[], mix_b, &[]);
        fb.switch_to_block(fix_b);
        fb.seal_block(fix_b);
        let a = fixnum_payload(fb, stack, reps, ia);
        let b = fixnum_payload(fb, stack, reps, ib);
        let raw = match op {
            Op::Add | Op::Sub => raw_fixnum_addsub(fb, dsite, matches!(op, Op::Sub), a, b),
            Op::Mul => raw_fixnum_mul(fb, dsite, a, b),
            _ => raw_fixnum_divrem(fb, dsite, false, a, b),
        };
        let tagged = retag_fixnum(fb, raw);
        let promoted = fb.ins().fcvt_from_sint(types::F64, raw);
        fb.def_var(tag_v, tagged);
        fb.def_var(f_v, promoted);
        fb.ins().jump(merge, &[]);
    }

    fb.switch_to_block(mix_b);
    fb.seal_block(mix_b);
    let fb_val = stack_as_f64_or_promote(fb, dsite, stack, reps, ib);
    let fa_val = stack_as_f64_or_promote(fb, dsite, stack, reps, ia);
    let res = emit_float_arith(fb, op, fa_val, fb_val);
    def_float(fb, res);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    stack.truncate(n - 2);
    reps.truncate(n - 2);
    let f64 = fb.use_var(f_v);
    if statically_float {
        // Never tested (see `FlonumKind::Float`); dead unless a fixnum-only
        // consumer guards it, and then it deopts, as a float must.
        stack.push(fb.ins().iconst(types::I64, UNBOXED_FLOAT_TAG_WORD));
        reps.push(SlotRep::Flonum {
            f64,
            kind: FlonumKind::Float,
        });
    } else {
        stack.push(fb.use_var(tag_v));
        reps.push(SlotRep::Flonum {
            f64,
            kind: FlonumKind::FloatOrFixnum,
        });
    }
    flonum_census_note(|c| c.results += 1);
}

/// Lower one non-control-flow opcode, updating the compile-time operand `stack`
/// (the live CLIF SSA values within the current basic block). Terminators
/// (`Return`/`Goto`/`GotoIf*`) are handled by the block lowerer before this.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_simple_op(
    fb: &mut FunctionBuilder,
    pc: usize,
    deopt_sites: &mut Vec<PendingDeopt>,
    signal_exit: &mut Option<Block>,
    constants: &[Value],
    stack: &mut Vec<ClifValue>,
    // Per-slot representations (cross-op unboxing), kept in lockstep with
    // `stack`.
    reps: &mut Vec<SlotRep>,
    rt: Option<&RtCtx>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    // R2 increment B2: an `Op::Call` spec site carries `(sym, expected, slot_ptr,
    // slot_idx, kind)`. `slot_ptr` is the baked `SpecSlot*` (JIT); `slot_idx` indexes
    // the AOT sidecar's `spec_slot_base`/`spec_expected_base` arrays.
    spec: Option<(u32, u64, i64, usize, SpecCalleeKind)>,
    op: &Op,
    // Cross-block known-fixnum operand values at this block (seeded by
    // `lower_leaf_full` from `compute_known_fixnum_slots`); `guard_fixnum` elides
    // guards for members.
    known: &HashSet<ClifValue>,
    // R1a: heap-constant reloc vector base (baked in entry) + bits->index map, so
    // `Op::Constant` loads a heap object from reloc_base[idx] instead of baking it.
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
    // R2 increment B2: false → JIT (spec `expected`/`slot` baked as `iconst`,
    // byte-identical); true → AOT (loaded from the sidecar's `spec_expected_base`/
    // `spec_slot_base` at `slot_idx`). The two bases are `Some` only in AOT mode at a
    // body with an `Op::Call` spec site (loaded once in the entry block).
    aot: bool,
    spec_slot_base: Option<ClifValue>,
    spec_expected_base: Option<ClifValue>,
    // `make-closure` patched prefix + the callee constant base bound in the entry
    // block (JIT only, `None` when the prefix is 0): `Op::Constant(idx)` with
    // `idx < dynamic_prefix` loads `consts_base[idx]` instead of baking.
    dynamic_prefix: usize,
    consts_base: Option<ClifValue>,
) -> Result<(), CompileError> {
    // Non-unboxing ops must see only tagged Values: force-tag the whole stack so
    // their gc_push / signal snapshot / shim args never observe a raw slot (closes
    // the GC-root + dispatch-snapshot soundness holes in one place). A
    // resident flonum below an audited op's operands is the one exception:
    // every reader of those slots is representation-aware.
    let residual = if op_preserves_raw(op) {
        None
    } else {
        Some(prepare_op_operands(fb, rt, op, stack, reps)?)
    };
    #[cfg(debug_assertions)]
    let residual_before: Option<Vec<ClifValue>> = residual.map(|keep| stack[..keep].to_vec());
    lower_simple_op_arms(
        fb,
        pc,
        deopt_sites,
        signal_exit,
        constants,
        stack,
        reps,
        rt,
        handlers,
        pending,
        spec,
        op,
        known,
        reloc_base,
        reloc_index,
        aot,
        spec_slot_base,
        spec_expected_base,
        dynamic_prefix,
        consts_base,
    )?;
    // Re-sync the reps after a non-unboxing op: the slots below `keep` are
    // untouched (the audited-op invariant, checked in debug builds), and its
    // result is tagged. Unboxing ops keep `reps` in lockstep themselves.
    if let Some(keep) = residual {
        #[cfg(debug_assertions)]
        debug_assert!(
            residual_before.as_deref() == stack.get(..keep),
            "{op:?} rewrote the model stack below its operands"
        );
        reps.truncate(keep);
        reps.resize(stack.len(), SlotRep::Tagged);
    }
    debug_assert_eq!(stack.len(), reps.len(), "{op:?} left reps desynced");
    Ok(())
}

/// Ops whose lowering reads nothing below its operands except through
/// [`deopt_site`], [`signal_target_for_site`] and [`emit_model_roots_pre`] —
/// all representation-aware — so under [`FlonumMode::Resident`] a flonum
/// below its operands stays unboxed across it (an audited list: an op that
/// reads a deeper slot any other way must not be here; the resync's debug
/// check catches one that rewrites them).
pub(crate) fn op_keeps_residual_flonums(op: &Op) -> bool {
    matches!(
        op,
        Op::Call(_)
            | Op::Apply(_)
            | Op::Aset
            | Op::CallBuiltin(..)
            | Op::CallBuiltinSym(..)
            | Op::List(_)
            | Op::Cons
            | Op::Car
            | Op::Cdr
            | Op::CarSafe
            | Op::CdrSafe
            | Op::Eq
            | Op::Null
            | Op::Not
            | Op::Consp
            | Op::Stringp
            | Op::Listp
            | Op::Symbolp
            | Op::Integerp
            | Op::Numberp
            | Op::VarSet(_)
    ) || direct_builtin_spec(op).is_some()
        || slice_builtin_spec(op).is_some()
}

/// Before a non-unboxing op: tag what it may observe, and return `keep`, the
/// depth below which its lowering leaves the model stack untouched. Raw
/// fixnums are always retagged. Under [`FlonumMode::Resident`] an audited op
/// ([`op_keeps_residual_flonums`]) boxes only its own operands (and their
/// aliases, which then share the box); every other op boxes every flonum.
fn prepare_op_operands(
    fb: &mut FunctionBuilder,
    rt: Option<&RtCtx>,
    op: &Op,
    stack: &mut [ClifValue],
    reps: &mut [SlotRep],
) -> Result<usize, CompileError> {
    if super::jit_flonum_mode() == super::FlonumMode::Resident
        && op_keeps_residual_flonums(op)
        && let Ok((needs, _)) = super::simple_effect(op)
    {
        let at = stack
            .len()
            .checked_sub(needs)
            .ok_or(CompileError::StackUnderflow)?;
        retag_raw_fixnums(fb, stack, reps);
        for k in at..stack.len() {
            if reps[k].is_flonum() {
                let rt = rt.expect("a flonum implies the runtime refs (float sites declare them)");
                box_flonum_slot(fb, rt.refs.make_float, stack, reps, k);
            }
        }
        return Ok(at);
    }
    materialize_model_stack(fb, rt, stack, reps);
    // Everything is tagged now, so the resync owes nothing to the residual.
    Ok(0)
}

/// The per-op arms of [`lower_simple_op`], after its operand preparation.
#[allow(clippy::too_many_arguments)]
fn lower_simple_op_arms(
    fb: &mut FunctionBuilder,
    pc: usize,
    deopt_sites: &mut Vec<PendingDeopt>,
    signal_exit: &mut Option<Block>,
    constants: &[Value],
    stack: &mut Vec<ClifValue>,
    // Per-slot representations (cross-op unboxing), kept in lockstep with
    // `stack`.
    reps: &mut Vec<SlotRep>,
    rt: Option<&RtCtx>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    // R2 increment B2: an `Op::Call` spec site carries `(sym, expected, slot_ptr,
    // slot_idx, kind)`. `slot_ptr` is the baked `SpecSlot*` (JIT); `slot_idx` indexes
    // the AOT sidecar's `spec_slot_base`/`spec_expected_base` arrays.
    spec: Option<(u32, u64, i64, usize, SpecCalleeKind)>,
    op: &Op,
    // Cross-block known-fixnum operand values at this block (seeded by
    // `lower_leaf_full` from `compute_known_fixnum_slots`); `guard_fixnum` elides
    // guards for members.
    known: &HashSet<ClifValue>,
    // R1a: heap-constant reloc vector base (baked in entry) + bits->index map, so
    // `Op::Constant` loads a heap object from reloc_base[idx] instead of baking it.
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
    // R2 increment B2: false → JIT (spec `expected`/`slot` baked as `iconst`,
    // byte-identical); true → AOT (loaded from the sidecar's `spec_expected_base`/
    // `spec_slot_base` at `slot_idx`). The two bases are `Some` only in AOT mode at a
    // body with an `Op::Call` spec site (loaded once in the entry block).
    aot: bool,
    spec_slot_base: Option<ClifValue>,
    spec_expected_base: Option<ClifValue>,
    // `make-closure` patched prefix + the callee constant base bound in the entry
    // block (JIT only, `None` when the prefix is 0): `Op::Constant(idx)` with
    // `idx < dynamic_prefix` loads `consts_base[idx]` instead of baking.
    dynamic_prefix: usize,
    consts_base: Option<ClifValue>,
) -> Result<(), CompileError> {
    if let Some(rt) = rt
        && !aot
        && super::arith_site_takes_generic(op, pc)
    {
        return lower_generic_arith_site(fb, rt, op, stack, reps, handlers, pending, signal_exit);
    }
    match op {
        // A `make-closure`-patched slot: per-instance, so load it through the
        // executing callee's constant vector (live, exactly the interpreter's
        // read) instead of baking the compile-time instance's value.
        Op::Constant(idx) if (*idx as usize) < dynamic_prefix => {
            let base = consts_base.expect("consts_base bound for a dynamic-prefix leaf");
            let off = i32::try_from(*idx as usize * 8).map_err(|_| CompileError::BadOperand)?;
            let cv = fb
                .ins()
                .load(types::I64, MemFlagsData::trusted(), base, off);
            stack.push(cv);
            reps.push(SlotRep::Tagged);
        }
        Op::Constant(idx) => {
            let v = constants
                .get(*idx as usize)
                .ok_or(CompileError::BadOperand)?;
            // Reloc-load when this const's bits are in the per-leaf reloc vector,
            // else bake. Keyed on `reloc_index` PRESENCE (not `is_heap_object`):
            //  - heap objects are ALWAYS collected (both JIT + AOT) → present → load
            //    (never bake a heap pointer; GC-pointer-free + AOT-portable, R1a);
            //  - a non-nil/t SYMBOL const is collected ONLY under AOT
            //    (collect_baseline_aot_relocs, const_relocs_for_aot) → present under
            //    AOT → loads its session-stable reloc; absent under JIT → bakes. This
            //    closes the audit CRITICAL #1: a quoted/arg symbol const took the
            //    iconst else-branch and baked its SESSION SymId (silent cross-session
            //    corruption). JIT stays byte-identical (its reloc_index never holds an
            //    op-symbol), exactly as the CallBuiltinSym site below.
            let cv = if reloc_index.contains_key(&v.bits()) {
                let i = reloc_index[&v.bits()];
                let base = reloc_base.expect("reloc_base set when a const is reloc'd");
                fb.ins()
                    .load(types::I64, MemFlagsData::trusted(), base, (i * 8) as i32)
            } else {
                // Fixnum / nil / t / char (immediate, session-stable): bake the bits.
                fb.ins().iconst(types::I64, v.bits() as i64)
            };
            stack.push(cv);
            reps.push(SlotRep::Tagged);
        }
        Op::Nil => {
            stack.push(fb.ins().iconst(types::I64, Value::NIL.bits() as i64));
            reps.push(SlotRep::Tagged);
        }
        Op::True => {
            stack.push(fb.ins().iconst(types::I64, Value::T.bits() as i64));
            reps.push(SlotRep::Tagged);
        }
        Op::Pop => {
            stack.pop().ok_or(CompileError::StackUnderflow)?;
            reps.pop();
        }
        Op::Dup => {
            let top = *stack.last().ok_or(CompileError::StackUnderflow)?;
            let top_raw = *reps.last().ok_or(CompileError::StackUnderflow)?;
            stack.push(top);
            reps.push(top_raw);
        }
        Op::StackRef(n) => {
            // 0 = top of stack, 1 = one below, ...
            let n = *n as usize;
            let idx = stack
                .len()
                .checked_sub(1 + n)
                .ok_or(CompileError::StackUnderflow)?;
            stack.push(stack[idx]);
            reps.push(reps[idx]);
        }
        Op::StackSet(n) => {
            // Assign TOS into the slot N below TOS, then pop TOS (N = 0 == pop).
            let n = *n as usize;
            let top = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let top_raw = reps.pop().ok_or(CompileError::StackUnderflow)?;
            if n != 0 {
                let idx = stack
                    .len()
                    .checked_sub(n)
                    .ok_or(CompileError::StackUnderflow)?;
                stack[idx] = top;
                reps[idx] = top_raw;
            }
        }
        Op::DiscardN(raw) => {
            // Low 7 bits: count to discard. High bit: keep TOS in the last kept
            // slot before discarding. Pure operand-stack manipulation.
            let preserve_tos = (*raw & 0x80) != 0;
            let n = (*raw & 0x7F) as usize;
            if n != 0 {
                let len = stack.len();
                if preserve_tos {
                    let target = len.checked_sub(1 + n).ok_or(CompileError::StackUnderflow)?;
                    stack[target] = stack[len - 1];
                    reps[target] = reps[len - 1];
                } else if n > len {
                    return Err(CompileError::StackUnderflow);
                }
                stack.truncate(len - n);
                reps.truncate(len - n);
            }
        }
        Op::Add | Op::Sub | Op::Mul | Op::Div => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
            // FLOAT SITE: the interpreter has seen floats here, so predict
            // floats — ONE combined tag test and branch, then unbox and
            // compute; that is no more than the old float-only arm paid, and
            // `nbody` (26 such sites in one body) is the benchmark that
            // measures it. But feedback is sticky from one float pair and the
            // fixnum fast arm records nothing, so the site may be fixnum-hot:
            // on the not-both-floats path dispatch again — both fixnums -> the
            // fixnum arm (GNU: fixnum op fixnum is a FIXNUM, `(+ 2 3)` is 5
            // not 5.0); one of each -> promote the fixnum and compute in f64;
            // a non-number -> deopt. Both PROVEN fixnums skip all of it.
            if let Some(rt) = rt
                && matches!(
                    super::active_numeric_feedback(pc),
                    crate::emacs_core::jit::NumericFeedback::Float
                )
                && !(reps[n - 1] == SlotRep::RawFixnum && reps[n - 2] == SlotRep::RawFixnum)
            {
                if !aot && super::jit_flonum_mode() != super::FlonumMode::Off {
                    lower_float_site_arith(fb, op, dsite, stack, reps);
                } else {
                    lower_float_site_arith_boxed(fb, rt, op, dsite, stack, reps);
                }
                return Ok(());
            }
            let b = stack_as_raw(fb, dsite, stack, reps, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, reps, n - 2, known);
            stack.truncate(n - 2);
            reps.truncate(n - 2);
            let res = match op {
                Op::Add | Op::Sub => raw_fixnum_addsub(fb, dsite, matches!(op, Op::Sub), a, b),
                Op::Mul => raw_fixnum_mul(fb, dsite, a, b),
                _ => raw_fixnum_divrem(fb, dsite, false, a, b),
            };
            stack.push(res);
            reps.push(SlotRep::RawFixnum);
        }
        Op::Rem => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, reps, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, reps, n - 2, known);
            stack.truncate(n - 2);
            reps.truncate(n - 2);
            stack.push(raw_fixnum_divrem(fb, dsite, true, a, b));
            reps.push(SlotRep::RawFixnum);
        }
        Op::Eq => {
            // Bit-equal -> t natively; differing bits -> the read-only slow-path
            // shim (only symbols-with-pos can make differing bits eq).
            let rt = rt.ok_or(CompileError::UnsupportedOp("eq"))?;
            let b = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let res = fb.declare_var(types::I64);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let same = fb.ins().icmp(IntCC::Equal, a, b);
            fb.ins().brif(same, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let vmctx = fb.use_var(rt.vmctx_var);
            let call = fb.ins().call(rt.refs.eq_slow, &[vmctx, a, b]);
            let slow_res = fb.inst_results(call)[0];
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::Symbolp => {
            // Symbol tag -> t natively (nil/t are symbols); otherwise the
            // read-only slow-path shim (symbol-with-pos while enabled).
            let rt = rt.ok_or(CompileError::UnsupportedOp("symbolp"))?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let res = fb.declare_var(types::I64);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let tag = band_imm_p(fb, a, TAG_MASK as i64);
            let is_sym = icmp_imm_p(fb, IntCC::Equal, tag, TAG_SYMBOL as i64);
            fb.ins().brif(is_sym, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let vmctx = fb.use_var(rt.vmctx_var);
            let call = fb.ins().call(rt.refs.symbolp_slow, &[vmctx, a]);
            let slow_res = fb.inst_results(call)[0];
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::Add1 | Op::Sub1 | Op::Negate => {
            let n = stack.len();
            if n < 1 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
            let a = stack_as_raw(fb, dsite, stack, reps, n - 1, known);
            stack.truncate(n - 1);
            reps.truncate(n - 1);
            let kind = match op {
                Op::Add1 => UnaryKind::Add1,
                Op::Sub1 => UnaryKind::Sub1,
                Op::Negate => UnaryKind::Negate,
                _ => unreachable!("matched Add1/Sub1/Negate above"),
            };
            stack.push(raw_fixnum_unop(fb, dsite, kind, a));
            reps.push(SlotRep::RawFixnum);
        }
        Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
            // FLOAT SITE — same float-first dispatch as the arithmetic ops.
            // Both floats: a plain IEEE `fcmp` (ordered: every relation is
            // false against NaN, as the builtins are). Both fixnums: integer
            // compare. One of each: GNU `arithcompare` — promote, and if the
            // doubles TIE re-decide on the exact integers, because a fixnum
            // above 2^53 rounds when promoted. Result is t/nil either way.
            // Flonum operands are read unboxed (their `f64`, and their tag
            // word for the tag tests and the fixnum arm); a statically-float
            // one leaves no fixnum arm.
            if matches!(
                super::active_numeric_feedback(pc),
                crate::emacs_core::jit::NumericFeedback::Float
            ) && !(reps[n - 1] == SlotRep::RawFixnum && reps[n - 2] == SlotRep::RawFixnum)
            {
                let statically_float =
                    reps[n - 2].is_static_float() || reps[n - 1].is_static_float();
                let res_var = fb.declare_var(types::I64);
                let ff_b = fb.create_block();
                let slow_b = fb.create_block();
                let fix_b = (!statically_float).then(|| fb.create_block());
                let mix_b = fb.create_block();
                let merge = fb.create_block();
                let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
                let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                let cc = match op {
                    Op::Eqlsign => IntCC::Equal,
                    Op::Lss => IntCC::SignedLessThan,
                    Op::Gtr => IntCC::SignedGreaterThan,
                    Op::Leq => IntCC::SignedLessThanOrEqual,
                    _ => IntCC::SignedGreaterThanOrEqual,
                };
                let fcc: cranelift_codegen::ir::condcodes::FloatCC = match op {
                    Op::Eqlsign => cranelift_codegen::ir::condcodes::FloatCC::Equal,
                    Op::Lss => cranelift_codegen::ir::condcodes::FloatCC::LessThan,
                    Op::Gtr => cranelift_codegen::ir::condcodes::FloatCC::GreaterThan,
                    Op::Leq => cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual,
                    _ => cranelift_codegen::ir::condcodes::FloatCC::GreaterThanOrEqual,
                };
                let both_float = both_float_test(fb, stack, reps, n - 2, n - 1);
                fb.ins().brif(both_float, ff_b, &[], slow_b, &[]);

                fb.switch_to_block(ff_b);
                fb.seal_block(ff_b);
                let fa_val = float_payload(fb, stack, reps, n - 2);
                let fb_val = float_payload(fb, stack, reps, n - 1);
                let cond = fb.ins().fcmp(fcc, fa_val, fb_val);
                let r = fb.ins().select(cond, t, nil);
                fb.def_var(res_var, r);
                fb.ins().jump(merge, &[]);

                fb.switch_to_block(slow_b);
                fb.seal_block(slow_b);
                if let Some(fix_b) = fix_b {
                    let both_fix = both_fixnum_test(fb, stack, reps, n - 2, n - 1);
                    fb.ins().brif(both_fix, fix_b, &[], mix_b, &[]);

                    fb.switch_to_block(fix_b);
                    fb.seal_block(fix_b);
                    let a = fixnum_payload(fb, stack, reps, n - 2);
                    let b = fixnum_payload(fb, stack, reps, n - 1);
                    let cond = fb.ins().icmp(cc, a, b);
                    let r = fb.ins().select(cond, t, nil);
                    fb.def_var(res_var, r);
                    fb.ins().jump(merge, &[]);
                } else {
                    fb.ins().jump(mix_b, &[]);
                }

                fb.switch_to_block(mix_b);
                fb.seal_block(mix_b);
                let (fb_val, ib) = stack_as_f64_and_int(fb, dsite, stack, reps, n - 1);
                let (fa_val, ia) = stack_as_f64_and_int(fb, dsite, stack, reps, n - 2);
                let cf = fb.ins().fcmp(fcc, fa_val, fb_val);
                let tie = fb.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::Equal,
                    fa_val,
                    fb_val,
                );
                let ci = fb.ins().icmp(cc, ia, ib);
                let cond = fb.ins().select(tie, ci, cf);
                let r = fb.ins().select(cond, t, nil);
                fb.def_var(res_var, r);
                fb.ins().jump(merge, &[]);

                fb.switch_to_block(merge);
                fb.seal_block(merge);
                let out = fb.use_var(res_var);
                stack.truncate(n - 2);
                reps.truncate(n - 2);
                stack.push(out);
                reps.push(SlotRep::Tagged);
                return Ok(());
            }
            let (a, b) = if reps[n - 2] == SlotRep::Tagged && reps[n - 1] == SlotRep::Tagged {
                // Fixnum tagging (4*x + 2) preserves signed integer order.
                // Keep tagged operands when neither came from raw arithmetic;
                // otherwise retain the existing cross-op unboxed path.
                let (a, b) = (stack[n - 2], stack[n - 1]);
                guard_fixnum(fb, dsite, b, known);
                guard_fixnum(fb, dsite, a, known);
                (a, b)
            } else {
                let b = stack_as_raw(fb, dsite, stack, reps, n - 1, known);
                let a = stack_as_raw(fb, dsite, stack, reps, n - 2, known);
                (a, b)
            };
            stack.truncate(n - 2);
            reps.truncate(n - 2);
            let cc = match op {
                Op::Eqlsign => IntCC::Equal,
                Op::Lss => IntCC::SignedLessThan,
                Op::Gtr => IntCC::SignedGreaterThan,
                Op::Leq => IntCC::SignedLessThanOrEqual,
                Op::Geq => IntCC::SignedGreaterThanOrEqual,
                _ => unreachable!("matched comparison ops above"),
            };
            // Operands share a representation; the result is tagged t/nil.
            let cond = fb.ins().icmp(cc, a, b);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            stack.push(fb.ins().select(cond, t, nil));
            reps.push(SlotRep::Tagged);
        }
        Op::Null | Op::Not | Op::Consp | Op::Stringp | Op::Listp => {
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let kind = match op {
                Op::Null | Op::Not => PredKind::Null,
                Op::Consp => PredKind::Consp,
                Op::Stringp => PredKind::Stringp,
                Op::Listp => PredKind::Listp,
                _ => unreachable!("matched predicate ops above"),
            };
            stack.push(lower_predicate(fb, kind, a));
        }
        Op::Car | Op::Cdr => {
            // Non-raw: the top-of-fn materialization tagged the stack; the
            // deopt snapshot's mask is all-false (cold retag is a no-op here).
            let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let is_cdr = matches!(op, Op::Cdr);
            stack.push(lower_car_cdr(fb, Some(dsite), is_cdr, false, a));
        }
        Op::CarSafe | Op::CdrSafe => {
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let is_cdr = matches!(op, Op::CdrSafe);
            stack.push(lower_car_cdr(fb, None, is_cdr, true, a));
        }
        Op::Max | Op::Min => {
            // Both fixnum -> select the larger/smaller RAW operand (one of the two
            // valid raw inputs, so the result stays raw); otherwise deopt to the
            // interpreter's number-coercing builtin.
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, reps, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, reps, n - 2, known);
            stack.truncate(n - 2);
            reps.truncate(n - 2);
            stack.push(raw_fixnum_maxmin(fb, matches!(op, Op::Min), a, b));
            reps.push(SlotRep::RawFixnum);
        }
        Op::Integerp | Op::Numberp => {
            // Fixnum tag -> t natively; anything else (bignum/float/non-number)
            // through the context-free slow shim.
            let rt = rt.ok_or(CompileError::UnsupportedOp("predicate"))?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let shim = if matches!(op, Op::Integerp) {
                rt.refs.integerp_slow
            } else {
                rt.refs.numberp_slow
            };
            let res = fb.declare_var(types::I64);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let tagbits = band_imm_p(fb, a, FIXNUM_CHECK_MASK as i64);
            let is_fix = fb
                .ins()
                .icmp_imm_u(IntCC::Equal, tagbits, FIXNUM_CHECK_VALUE as i64);
            fb.ins().brif(is_fix, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let call = fb.ins().call(shim, &[a]);
            let slow_res = fb.inst_results(call)[0];
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::VarRef(idx) => {
            // Read through the runtime's variable machinery (buffer-locals,
            // redirects); can signal void-variable. Reads are idempotent, so
            // this neither poisons nor guards.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            // GNU's `Bvarref` reads a plain symbol's value cell inline; so does
            // this: the answer `neovm_jit_varref`'s first branch gives, without
            // the call — a slot in range whose redirect is `Plainval` and whose
            // value is bound (an empty slot reads unbound). `nil` answers here
            // too unless the symbol is a dedicated buffer-local, whose `nil`
            // cell means "read the buffer" (decided now: the set is fixed by
            // symbol identity). Everything else takes the shim.
            let res = fb.declare_var(types::I64);
            let slow = fb.create_block();
            let cont = fb.create_block();
            {
                use crate::emacs_core::symbol::{
                    LISP_SYMBOL_FLAGS_OFFSET, LISP_SYMBOL_SIZE, LISP_SYMBOL_VAL_OFFSET,
                    OBARRAY_CHUNK_BITS, OBARRAY_CHUNK_SLOTS, OBARRAY_JIT_LEN_OFFSET,
                    OBARRAY_JIT_SPINE_OFFSET, SYMBOL_FLAGS_REDIRECT_MASK,
                };
                let ob = core::mem::offset_of!(Context, obarray);
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
                        let cell = (sym as usize & (OBARRAY_CHUNK_SLOTS - 1))
                            .checked_mul(LISP_SYMBOL_SIZE)?;
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
                let refused = if crate::buffer::buffer::DedicatedBufferLocal::from_sym_id(
                    crate::emacs_core::intern::SymId(sym),
                )
                .is_some()
                {
                    let nil = icmp_imm_p(fb, IntCC::Equal, val, Value::NIL.bits() as i64);
                    fb.ins().bor(unbound, nil)
                } else {
                    unbound
                };
                let fast_blk = fb.create_block();
                fb.ins().brif(refused, slow, &[], fast_blk, &[]);
                fb.switch_to_block(fast_blk);
                fb.seal_block(fast_blk);
                fb.def_var(res, val);
                fb.ins().jump(cont, &[]);
            }
            // The shim: root live stack values (variable access may allocate).
            // The inline read stored nothing, so the continuation meets both
            // store records.
            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let carry_fast = rootwin_carry_snapshot();
            // Reading a variable leaves the residual stack unchanged. Keep
            // its integers raw and its floats unboxed across the inline read
            // and the continuation; only the fallback needs tagged roots
            // (a flonum is no reference: it is skipped) and a signal
            // snapshot (whose handler entry boxes a flonum).
            let mut tagged = std::borrow::Cow::Borrowed(stack.as_slice());
            let mut tagged_reps = std::borrow::Cow::Borrowed(reps.as_slice());
            for (i, &rep) in reps.iter().enumerate() {
                if rep == SlotRep::RawFixnum {
                    tagged.to_mut()[i] = retag_fixnum(fb, stack[i]);
                    tagged_reps.to_mut()[i] = SlotRep::Tagged;
                }
            }
            let saved = if tagged.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, &tagged, &tagged_reps)
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call = fb.ins().call(rt.refs.varref, &[vmctx, sym_v, out_addr]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            rootwin_carry_meet(&carry_fast);
            let se =
                signal_target_for_site(fb, signal_exit, handlers, pending, &tagged, &tagged_reps);
            let slow_ok = fb.create_block();
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, slow_ok, &[], se, &[]);
            fb.switch_to_block(slow_ok);
            fb.seal_block(slow_ok);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            fb.def_var(res, result);
            fb.ins().jump(cont, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            stack.push(fb.use_var(res));
            reps.push(SlotRep::Tagged);
        }
        Op::VarSet(idx) => {
            // Assign through the runtime (may run variable watchers — arbitrary
            // lisp — and signal). A side effect: poisons later guards.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            let val = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            let call = fb.ins().call(rt.refs.varset, &[vmctx, sym_v, val]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let cont = fb.create_block();
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
        }
        Op::Call(n) | Op::Apply(n) => {
            // `rt` is always present here (`needs_rt` includes Call/Apply).
            // Stack: [func a1 .. aN] -> [result], mirroring the interpreter's
            // Op::Call / Op::Apply; the two differ only in which shim runs
            // (apply spreads its last argument inside the runtime).
            let rt = rt.ok_or(CompileError::UnsupportedOp("call"))?;
            let shim = if matches!(op, Op::Apply(_)) {
                rt.refs.apply
            } else {
                rt.refs.call
            };
            let n = *n as usize;
            if stack.len() < n + 1 {
                return Err(CompileError::StackUnderflow);
            }
            let args_at = stack.len() - n;
            // Speculated direct call: the callee slot holds a constant symbol
            // whose compile-time binding was a bytecode object or a fixed-arity
            // builtin subr (Apply never speculates).
            let spec = spec.filter(|_| matches!(op, Op::Call(_)));
            // SSA soundness gate: emit the spec shim ONLY when the callee
            // slot's value is provably the site's symbol constant (iconst /
            // reloc-load of its tagged bits). find_spec_sites' abstract stack
            // tracking selected this site; if the lowering cannot re-prove it
            // here, the site silently degrades to the generic call below —
            // never a wrong-callee speculation.
            // A JIT site whose callee crossed a block boundary (its value is a
            // block parameter the proof cannot see through) speculates behind a
            // run-time check of the callee's bits instead (`guarded_sym`): a
            // mismatch takes the plain generic call. Not for AOT (the symbol's
            // bits are relocated there) or inline arithmetic (which deopts).
            let mut guarded_sym: Option<u32> = None;
            let spec = spec.filter(|&(sym, _, _, _, kind)| {
                if callee_is_symbol_const(fb, stack[args_at - 1], sym, reloc_base, reloc_index) {
                    return true;
                }
                if !aot && !matches!(kind, SpecCalleeKind::ArithIntrinsic { .. }) {
                    guarded_sym = Some(sym);
                    return true;
                }
                tracing::debug!(
                    target: "neovm_jit",
                    sym,
                    "spec site dropped: callee slot not provably the tracked symbol"
                );
                false
            });
            // LEVEL-B (JIT only): inline logand/logior/logxor/lognot as native ops
            // on the TAGGED fixnum bits, guarded by a fixnum check that DEOPTS —
            // instead of the armed shim's 8-arg call. The fixnum tag is 2
            // (`retag_fixnum = (n<<2)|2`), so `a & b` / `a | b` keep it (2&2=2,
            // 2|2=2), `a ^ b` clears it (2^2=0 → restore with `| 2`), and negating
            // a tagged fixnum yields `lognot` exactly (-a == retag(~n)). A non-fixnum
            // arg deopts to the precise-deopt block, where the interpreter re-runs
            // the REAL call from `pc` (graceful for the odd bignum; a fully mixed
            // loop stays interpreted). Redefinition is caught by the leaf's
            // inline_epoch eviction (set in compile_bytecode_function_inner). AOT
            // (`aot`) keeps the shim — its loader owns arm/disarm.
            if jit_inline_arith_on()
                && !aot
                && let Some((_, _, _, _, SpecCalleeKind::ArithIntrinsic { op })) = spec
                && arith_op_inlines(op)
            {
                let dsite = deopt_site(fb, pc, handlers.len(), stack, reps, deopt_sites);
                let sp = stack.len();
                let is_lognot = op == ARITH_KIND_LOGNOT as u8;
                let a = stack[sp - if is_lognot { 1 } else { 2 }];
                guard_fixnum(fb, dsite, a, known);
                let res = if is_lognot {
                    // lognot(n) == ~n == -n-1; on the tagged bits: -a == retag(~n).
                    fb.ins().ineg(a)
                } else {
                    let b = stack[sp - 1];
                    guard_fixnum(fb, dsite, b, known);
                    match op {
                        x if x == ARITH_KIND_LOGAND as u8 => fb.ins().band(a, b),
                        x if x == ARITH_KIND_LOGIOR as u8 => fb.ins().bor(a, b),
                        x if x == ARITH_KIND_MOD as u8 => {
                            // GNU Fmod integer branch on the untagged values:
                            // truncated srem, then pull a nonzero result onto
                            // the divisor's side of zero. Zero divisor deopts
                            // (the interpreter re-runs the real call, which
                            // signals arith-error). The result magnitude stays
                            // below |b|, so the retag never overflows.
                            let av = sshr_imm_p(fb, a, FIXNUM_SHIFT as i64);
                            let bv = sshr_imm_p(fb, b, FIXNUM_SHIFT as i64);
                            let nonzero = icmp_imm_p(fb, IntCC::NotEqual, bv, 0);
                            emit_guard(fb, dsite, nonzero);
                            let m = fb.ins().srem(av, bv);
                            let signs = fb.ins().bxor(m, bv);
                            let differ = icmp_imm_p(fb, IntCC::SignedLessThan, signs, 0);
                            let m_nonzero = icmp_imm_p(fb, IntCC::NotEqual, m, 0);
                            let need_fix = fb.ins().band(differ, m_nonzero);
                            let fixed = fb.ins().iadd(m, bv);
                            let floored = fb.ins().select(need_fix, fixed, m);
                            retag_fixnum(fb, floored)
                        }
                        _ => {
                            debug_assert_eq!(op, ARITH_KIND_LOGXOR as u8);
                            // XOR clears the tag bit (2^2=0); restore it.
                            let x = fb.ins().bxor(a, b);
                            bor_imm_p(fb, x, FIXNUM_CHECK_VALUE as i64)
                        }
                    }
                };
                // Drop callee + args, push the tagged fixnum result.
                stack.truncate(args_at - 1);
                reps.truncate(args_at - 1);
                stack.push(res);
                reps.push(SlotRep::Tagged);
                return Ok(());
            }
            // Pred/EqIncl sites pass their 1–2 args in REGISTERS on the direct
            // path (no spill; their fallback block spills for itself). Every
            // other shape spills the args into the call buffer for its shim.
            let reg_args: Option<SmallVec<[ClifValue; 2]>> = match spec {
                Some((_, _, _, _, kind)) if kind.is_reg_args() => {
                    Some(stack[args_at..].iter().copied().collect())
                }
                _ => {
                    for (i, &v) in stack[args_at..].iter().enumerate() {
                        fb.ins()
                            .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                    }
                    None
                }
            };
            let func_val = stack[args_at - 1];
            stack.truncate(args_at - 1);
            // What the generic call uses must dominate the guard's edge into
            // it, so a guarded site defines the call buffers first.
            let guarded_buffers = guarded_sym.map(|_| {
                (
                    fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0),
                    fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0),
                    fb.ins().iconst(types::I64, n as i64),
                )
            });
            // The guarded site's check, before anything the speculated path
            // stores: a mismatch enters the generic-fallback block below.
            let guard_generic: Option<Block> = guarded_sym.map(|sym| {
                let bits = Value::from_sym_id(crate::emacs_core::intern::SymId(sym)).bits();
                #[cfg(test)]
                let bits = if FORCE_SPEC_GUARD_MISS.with(|c| c.get()) {
                    !bits
                } else {
                    bits
                };
                let is_sym = icmp_imm_p(fb, IntCC::Equal, func_val, bits as i64);
                let speculated = fb.create_block();
                let generic = fb.create_block();
                fb.ins().brif(is_sym, speculated, &[], generic, &[]);
                fb.switch_to_block(speculated);
                fb.seal_block(speculated);
                generic
            });
            // Root every value that stays live across the call (the callee +
            // args are rooted by the shim; the constants are rooted by the
            // dispatch seam via the executing function). Pred/EqIncl direct
            // paths SKIP this: their shims are GC-free by contract (they bounce
            // to the fallback block rather than run anything that could
            // allocate), and the fallback block roots for itself.
            let saved = if stack.is_empty() || reg_args.is_some() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let (args_addr, out_addr, n_val) = guarded_buffers.unwrap_or_else(|| {
                (
                    fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0),
                    fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0),
                    fb.ins().iconst(types::I64, n as i64),
                )
            });
            // A JIT `type-of` / `cl-type-of` site answers a record inline and
            // leaves everything else to the shim below (see
            // `emit_inline_record_type_of`). The inline path stores nothing,
            // so the merge meets the shim path's store record with this one.
            let inline_type_of = match (spec, &reg_args) {
                (
                    Some((
                        _,
                        _,
                        slot_ptr,
                        _,
                        SpecCalleeKind::PredTypeOf | SpecCalleeKind::PredClTypeOf,
                    )),
                    Some(args),
                ) if !aot && !jit_force_slow_spec() && jit_inline_type_of_on() => {
                    let carry = rootwin_carry_snapshot();
                    let slot_v = fb.ins().iconst(types::I64, slot_ptr);
                    emit_inline_record_type_of(fb, rt, slot_v, args[0])
                        .map(|(merge, res)| (merge, res, carry))
                }
                _ => None,
            };
            // Subr-kind sites can return STATUS_NEED_GENERIC, which routes to a
            // fallback block that re-does this site as the ORIGINAL generic
            // call; bytecode-kind sites keep their everything-inside-the-shim
            // protocol. `None` = no NEED_GENERIC possible.
            let mut generic_fallback: Option<Block> = guard_generic;
            let call = match spec {
                Some((sym, expected, slot_ptr, slot_idx, SpecCalleeKind::Bytecode)) => {
                    let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
                    let exp_v =
                        materialize_spec_expected(fb, aot, spec_expected_base, expected, slot_idx);
                    let slot_v = materialize_spec_slot(fb, aot, spec_slot_base, slot_ptr, slot_idx);
                    fb.ins().call(
                        rt.refs.call_spec,
                        &[vmctx, sym_v, exp_v, slot_v, args_addr, n_val, out_addr],
                    )
                }
                Some((sym, expected, slot_ptr, slot_idx, kind)) => {
                    // PRESERVE emission order: create the generic-fallback block
                    // FIRST (byte-identical to before B2), then the operands.
                    // A guarded site already made it (both edges enter it).
                    if generic_fallback.is_none() {
                        generic_fallback = Some(fb.create_block());
                    }
                    let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
                    let exp_v =
                        materialize_spec_expected(fb, aot, spec_expected_base, expected, slot_idx);
                    let slot_v = materialize_spec_slot(fb, aot, spec_slot_base, slot_ptr, slot_idx);
                    // The refs are Some whenever a subr-kind site exists (the
                    // declare is keyed on exactly that condition).
                    match (kind, &reg_args) {
                        (SpecCalleeKind::SubrGeneral, _) => {
                            let f = rt
                                .refs
                                .call_subr_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            fb.ins().call(
                                f,
                                &[vmctx, sym_v, exp_v, slot_v, args_addr, n_val, out_addr],
                            )
                        }
                        (
                            SpecCalleeKind::PredRecordp
                            | SpecCalleeKind::PredSymbolWithPos
                            | SpecCalleeKind::PredTypeOf
                            | SpecCalleeKind::PredClTypeOf
                            | SpecCalleeKind::PredFboundp
                            | SpecCalleeKind::PredAutoloadDoLoad,
                            Some(args),
                        ) => {
                            let f = rt
                                .refs
                                .pred_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            let kind_v = fb.ins().iconst(
                                types::I64,
                                match kind {
                                    SpecCalleeKind::PredRecordp => PRED_KIND_RECORDP,
                                    SpecCalleeKind::PredSymbolWithPos => {
                                        PRED_KIND_SYMBOL_WITH_POS_P
                                    }
                                    SpecCalleeKind::PredTypeOf => PRED_KIND_TYPE_OF,
                                    SpecCalleeKind::PredFboundp => PRED_KIND_FBOUNDP,
                                    SpecCalleeKind::PredAutoloadDoLoad => {
                                        PRED_KIND_AUTOLOAD_DO_LOAD
                                    }
                                    _ => PRED_KIND_CL_TYPE_OF,
                                },
                            );
                            fb.ins()
                                .call(f, &[vmctx, kind_v, sym_v, exp_v, slot_v, args[0], out_addr])
                        }
                        (SpecCalleeKind::EqInclProps, Some(args)) => {
                            let f = rt
                                .refs
                                .eq_incl_props_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            fb.ins().call(
                                f,
                                &[vmctx, sym_v, exp_v, slot_v, args[0], args[1], out_addr],
                            )
                        }
                        (SpecCalleeKind::ArithIntrinsic { op }, Some(args)) => {
                            let f = rt
                                .refs
                                .arith_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            let kind_v = fb.ins().iconst(types::I64, op as i64);
                            // lognot is 1-arg: pass a dummy `b` (the shim ignores it
                            // for LOGNOT). The 2-arg ops collected both.
                            let b_v = args.get(1).copied().unwrap_or_else(|| {
                                fb.ins().iconst(types::I64, Value::NIL.bits() as i64)
                            });
                            fb.ins().call(
                                f,
                                &[vmctx, kind_v, sym_v, exp_v, slot_v, args[0], b_v, out_addr],
                            )
                        }
                        // Reg-arg kinds always collected their args above.
                        _ => return Err(CompileError::UnsupportedOp("subr-spec-shape")),
                    }
                }
                None => fb
                    .ins()
                    .call(shim, &[vmctx, func_val, args_addr, n_val, out_addr]),
            };
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            // STATUS_OK -> continue with the result; STATUS_NEED_GENERIC (subr
            // spec sites only) -> the generic fallback block; anything else is
            // STATUS_SIGNAL -> propagate via the handler-aware signal target.
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let cont = fb.create_block();
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            if let Some(gen_block) = generic_fallback {
                let check = fb.create_block();
                fb.ins().brif(ok, cont, &[], check, &[]);
                fb.switch_to_block(check);
                fb.seal_block(check);
                let need_gen = fb
                    .ins()
                    .icmp_imm_u(IntCC::Equal, status, STATUS_NEED_GENERIC);
                fb.ins().brif(need_gen, gen_block, &[], se, &[]);
                // Fallback: the ORIGINAL generic Op::Call lowering for this
                // site — spill the register args (if any), root the residual
                // stack, call the plain generic shim on the constant SYMBOL
                // (which resolves the live binding: fset/advice/overrides all
                // take effect), same OK/signal branching.
                fb.switch_to_block(gen_block);
                fb.seal_block(gen_block);
                if let Some(args) = &reg_args {
                    for (i, &v) in args.iter().enumerate() {
                        fb.ins()
                            .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                    }
                }
                // The fast path may have stored nothing (a reg-args site);
                // the continuation merges both store histories.
                let carry_fast = rootwin_carry_snapshot();
                if guard_generic.is_some() {
                    // The guard's edge skips every store the speculated path
                    // made, so this path may elide none of them.
                    rootwin_carry_reset();
                }
                let saved_gen = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_model_roots_pre(fb, rt, stack, reps)
                };
                let vmctx_gen = fb.use_var(rt.vmctx_var);
                let call_gen = fb
                    .ins()
                    .call(shim, &[vmctx_gen, func_val, args_addr, n_val, out_addr]);
                let status_gen = fb.inst_results(call_gen)[0];
                emit_cond_residual_roots_post(fb, rt, saved_gen);
                rootwin_carry_meet(&carry_fast);
                let se_gen =
                    signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
                let ok_gen = icmp_imm_p(fb, IntCC::Equal, status_gen, STATUS_OK);
                fb.ins().brif(ok_gen, cont, &[], se_gen, &[]);
            } else {
                fb.ins().brif(ok, cont, &[], se, &[]);
            }
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            match inline_type_of {
                Some((merge, res, carry)) => {
                    fb.def_var(res, result);
                    fb.ins().jump(merge, &[]);
                    fb.switch_to_block(merge);
                    fb.seal_block(merge);
                    rootwin_carry_meet(&carry);
                    stack.push(fb.use_var(res));
                }
                None => stack.push(result),
            }
        }
        Op::Cons => {
            // `rt` is always present here: analyze_cfg accepts Cons only when the
            // function declares the shims (see `needs_rt` in lower_leaf).
            let rt = rt.ok_or(CompileError::UnsupportedOp("cons"))?;
            let cdr = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let car = stack.pop().ok_or(CompileError::StackUnderflow)?;
            // No rooting at all: the cons shim is pure allocation and never
            // reaches a GC safe point (see `neovm_jit_cons`), so neither
            // car/cdr nor the residual operand stack can be collected under it.
            let call = fb.ins().call(rt.refs.cons, &[car, cdr]);
            let result = fb.inst_results(call)[0];
            stack.push(result);
        }
        Op::VarBind(idx) => {
            // GNU Bvarbind: specbind(sym, POP). A typed per-buffer forwarder can
            // signal, so branch through the same handler-aware signal target as
            // VarSet. The shim records a bind depth only after a successful
            // store.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            let val = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            // The shim runs variable watchers (arbitrary lisp -> GC). `val` is
            // rooted by `specbind` inside the shim, but the remaining operand
            // stack lives only in Cranelift registers — root it across the call
            // (mirrors VarRef/VarSet). This is an exact-root GC: a live Value
            // unrooted across a GC-capable call is a use-after-free.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let call = fb.ins().call(rt.refs.varbind, &[vmctx, sym_v, val]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let cont = fb.create_block();
            let signal =
                signal_target_for_site(fb, signal_exit, handlers, pending, stack.as_slice(), reps);
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], signal, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
        }
        Op::Unbind(n) => {
            // Unbind the N most recent dynamic bindings. Static analysis
            // guarantees balance, but cleanup Lisp/watchers can still exit.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let vmctx = fb.use_var(rt.vmctx_var);
            let n_v = fb.ins().iconst(types::I64, *n as i64);
            // The shim runs unwind-protect cleanups (arbitrary lisp -> GC); root
            // the whole live operand stack across the call.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let call = fb.ins().call(rt.refs.unbind, &[vmctx, n_v]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let cont = fb.create_block();
            let signal =
                signal_target_for_site(fb, signal_exit, handlers, pending, stack.as_slice(), reps);
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], signal, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
        }
        Op::SaveCurrentBuffer | Op::SaveExcursion | Op::SaveRestriction => {
            // Infallible specpdl records (the interpreter arms mirrored in the
            // shims); restored by the matching Unbind or the frame unwind.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let shim = match op {
                Op::SaveCurrentBuffer => rt.refs.save_current_buffer,
                Op::SaveExcursion => rt.refs.save_excursion,
                Op::SaveRestriction => rt.refs.save_restriction,
                _ => unreachable!("matched Save* above"),
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            fb.ins().call(shim, &[vmctx]);
        }
        Op::UnwindProtectPop => {
            // Pop the cleanup form and register the unwind-protect record
            // (infallible; the cleanup runs via the shared unbind machinery).
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let forms = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let vmctx = fb.use_var(rt.vmctx_var);
            fb.ins().call(rt.refs.unwind_protect, &[vmctx, forms]);
        }
        Op::SaveWindowExcursion => {
            // Evaluate the popped body under a window-configuration
            // save/restore via the shim (interpreter arm parity).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let body = stack.pop().ok_or(CompileError::StackUnderflow)?;
            // Root remaining live values: the body runs arbitrary lisp.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call = fb
                .ins()
                .call(rt.refs.save_window_excursion, &[vmctx, body, out_addr]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let cont = fb.create_block();
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
        Op::Aset => {
            // `neovm_jit_aset` answers VALUE's bits for every `aset` it can run
            // itself — the vector, record and same-width string stores, and
            // `builtin_aset_args` for the other shapes — or a `VALUE_SHIM_*`
            // word (tag 0b001, never a Lisp value). None of that reaches a safe
            // point, so the call roots nothing. A redefined or advised `aset`
            // runs Lisp: the shim answers NEED_GENERIC before storing anything
            // and the site takes the rooted general call (named builtin,
            // variant 2).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            if stack.len() < 3 {
                return Err(CompileError::StackUnderflow);
            }
            let at = stack.len() - 3;
            let operands = [stack[at], stack[at + 1], stack[at + 2]];
            stack.truncate(at);
            let vmctx = fb.use_var(rt.vmctx_var);
            let call = fb.ins().call(
                rt.refs.aset,
                &[vmctx, operands[0], operands[1], operands[2]],
            );
            let word = fb.inst_results(call)[0];
            let res = fb.declare_var(types::I64);
            fb.def_var(res, word);
            let cont = fb.create_block();
            let sentinel = fb.create_block();
            let tag = band_imm_p(fb, word, TAG_MASK as i64);
            let is_sentinel = icmp_imm_p(
                fb,
                IntCC::Equal,
                tag,
                dispatch::VALUE_SHIM_SIGNAL & TAG_MASK as i64,
            );
            fb.ins().brif(is_sentinel, sentinel, &[], cont, &[]);

            fb.switch_to_block(sentinel);
            fb.seal_block(sentinel);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let gen_block = fb.create_block();
            let need_gen = icmp_imm_p(fb, IntCC::Equal, word, dispatch::VALUE_SHIM_NEED_GENERIC);
            fb.ins().brif(need_gen, gen_block, &[], se, &[]);

            // The general call. The shim stored nothing on this edge, so the
            // continuation meets the fast path's store record with this one.
            fb.switch_to_block(gen_block);
            fb.seal_block(gen_block);
            let carry_fast = rootwin_carry_snapshot();
            for (i, &v) in operands.iter().enumerate() {
                fb.ins()
                    .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
            }
            let saved_gen = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let vmctx_gen = fb.use_var(rt.vmctx_var);
            let variant_gen = fb.ins().iconst(types::I64, 2);
            let sym_gen = fb.ins().iconst(types::I64, 0);
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let n_val = fb.ins().iconst(types::I64, 3);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call_gen = fb.ins().call(
                rt.refs.named_builtin,
                &[vmctx_gen, variant_gen, sym_gen, args_addr, n_val, out_addr],
            );
            let status_gen = fb.inst_results(call_gen)[0];
            emit_cond_residual_roots_post(fb, rt, saved_gen);
            rootwin_carry_meet(&carry_fast);
            let se_gen = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let gen_ok = fb.create_block();
            let ok_gen = icmp_imm_p(fb, IntCC::Equal, status_gen, STATUS_OK);
            fb.ins().brif(ok_gen, gen_ok, &[], se_gen, &[]);
            fb.switch_to_block(gen_ok);
            fb.seal_block(gen_ok);
            let gen_result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            fb.def_var(res, gen_result);
            fb.ins().jump(cont, &[]);

            fb.switch_to_block(cont);
            fb.seal_block(cont);
            stack.push(fb.use_var(res));
        }
        Op::CallBuiltin(..) | Op::CallBuiltinSym(..) => {
            // Named-builtin escape hatch: route through the Vm::*_for_jit
            // helpers mirroring the interpreter arms (override-aware /
            // advice-bypassing / writeback / quit poll).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let (variant, sym, nargs): (i64, u32, usize) = match op {
                Op::CallBuiltin(name_idx, n) => {
                    (0, const_sym_id(constants, *name_idx)?, *n as usize)
                }
                Op::CallBuiltinSym(sym, n) => (1, sym.0, *n as usize),
                _ => unreachable!("matched named-builtin ops above"),
            };
            if stack.len() < nargs {
                return Err(CompileError::StackUnderflow);
            }
            // R2: a Tier-B CallBuiltinSym spec site takes the dispatch-skip fast
            // path (`neovm_jit_cbsym_spec`) with a NEED_GENERIC fallback to THIS
            // op's original general lowering; Tier-A sites (COMMIT 5) take the
            // GC-free read shim; every other named-builtin op keeps the general
            // lowering. As of increment A BOTH JIT and AOT baseline emit take the
            // fast path (CBSym classification is obarray-free) — the `sym`
            // materialize below is AOT-reloc-aware, so the baked shim call reloads
            // the SymId by name under AOT and iconsts it under JIT (byte-identical).
            let cbsym_spec_b = matches!(op, Op::CallBuiltinSym(..))
                && matches!(spec, Some((_, _, _, _, SpecCalleeKind::CbsymTierB)));
            // Tier-A GC-free read (`neovm_jit_cbsym_read`): its OK path returns an
            // IMMEDIATE and never allocates, so the fast path skips residual-stack
            // rooting entirely (like the round-1 predicate shims). `which` is the
            // baked builtin discriminant. NEED_GENERIC still routes to the general
            // fallback (which DOES root, since it can allocate).
            let cbsym_a_which: Option<u8> = if matches!(op, Op::CallBuiltinSym(..)) {
                match spec {
                    Some((_, _, _, _, SpecCalleeKind::CbsymTierA { which })) => Some(which),
                    _ => None,
                }
            } else {
                None
            };
            let at = stack.len() - nargs;
            for (i, &v) in stack[at..].iter().enumerate() {
                fb.ins()
                    .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
            }
            stack.truncate(at);
            // Root remaining live values (arbitrary lisp may run; the shim roots
            // the operands themselves). The Tier-A read shim is GC-free by
            // contract, so its fast path needs NO residual rooting; its
            // NEED_GENERIC fallback block re-roots for the general call.
            let saved = if stack.is_empty() || cbsym_a_which.is_some() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            // R2-E (must-nail #2): the named-builtin callee SymId is session-specific.
            // The JIT bakes it (`iconst(sym)`); AOT must RELOC it BY NAME — the op's
            // symbol Value was collected into the per-leaf reloc vector, so load its
            // bits from reloc_base[idx] and recover the SymId (`bits >> TAG_BITS`,
            // TAG_SYMBOL==0). Keyed on `reloc_index` presence: the JIT reloc set never
            // contains op-symbols (only heap consts), so JIT always bakes → byte-
            // identical.
            // Shared by the fast-shim call, the direct general call, AND the
            // fallback (all JIT-only when a CBSym spec site exists).
            let sym_v = match reloc_index.get(&((sym as usize) << TAG_BITS | TAG_SYMBOL)) {
                Some(&idx) => {
                    let base = reloc_base.expect("reloc_base set when an op-symbol is reloc'd");
                    let sym_bits =
                        fb.ins()
                            .load(types::I64, MemFlagsData::trusted(), base, (idx * 8) as i32);
                    ushr_imm_p(fb, sym_bits, TAG_BITS as i64)
                }
                None => fb.ins().iconst(types::I64, sym as i64),
            };
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let n_val = fb.ins().iconst(types::I64, nargs as i64);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            // Fast path: the Tier-A GC-free read shim or the Tier-B dispatch-skip
            // shim; NEED_GENERIC routes to `generic_fallback` (the ORIGINAL
            // named-builtin call). Everything else keeps the general lowering.
            let mut generic_fallback: Option<Block> = None;
            // A Tier-A read lowers INLINE where the host layout could be
            // proved, performing the load itself instead of calling a shim to
            // perform it -- GNU answers these with one bytecode opcode. The
            // inline sequence yields the same STATUS the shim would, so the
            // OK / NEED_GENERIC merge below is unchanged.
            //
            // JIT ONLY: it bakes host field offsets and a host static's
            // address, neither of which means anything in an AOT object
            // loaded into a different build. `force_cbsym_generic` and the
            // arity are compile-time facts here, so they gate emission rather
            // than costing anything at run time.
            let mut inline_status: Option<ClifValue> = None;
            if let Some(which) = cbsym_a_which
                && !aot
                && !super::force_cbsym_generic()
                && nargs == super::dispatch::cbsym_read_expected_nargs(which)
            {
                inline_status = emit_inline_cbsym_read(fb, rt, which, sym);
                if inline_status.is_some() {
                    generic_fallback = Some(fb.create_block());
                }
            }
            let call = if inline_status.is_some() {
                None
            } else if let Some(which) = cbsym_a_which {
                generic_fallback = Some(fb.create_block());
                let f = rt
                    .refs
                    .cbsym_read
                    .ok_or(CompileError::UnsupportedOp("cbsym-read-refs"))?;
                let which_v = fb.ins().iconst(types::I64, which as i64);
                Some(
                    fb.ins()
                        .call(f, &[vmctx, which_v, sym_v, args_addr, n_val, out_addr]),
                )
            } else if cbsym_spec_b {
                generic_fallback = Some(fb.create_block());
                let f = rt
                    .refs
                    .cbsym_spec
                    .ok_or(CompileError::UnsupportedOp("cbsym-spec-refs"))?;
                Some(
                    fb.ins()
                        .call(f, &[vmctx, sym_v, args_addr, n_val, out_addr]),
                )
            } else {
                let variant_v = fb.ins().iconst(types::I64, variant);
                Some(fb.ins().call(
                    rt.refs.named_builtin,
                    &[vmctx, variant_v, sym_v, args_addr, n_val, out_addr],
                ))
            };
            let status = match (inline_status, call) {
                (Some(status), _) => status,
                (None, Some(call)) => fb.inst_results(call)[0],
                (None, None) => unreachable!("a call is emitted unless the read inlined"),
            };
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let cont = fb.create_block();
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            if let Some(gen_block) = generic_fallback {
                // STATUS_OK -> cont; STATUS_NEED_GENERIC -> the general CBSym
                // lowering; anything else -> STATUS_SIGNAL via the signal target.
                let check = fb.create_block();
                fb.ins().brif(ok, cont, &[], check, &[]);
                fb.switch_to_block(check);
                fb.seal_block(check);
                let need_gen = fb
                    .ins()
                    .icmp_imm_u(IntCC::Equal, status, STATUS_NEED_GENERIC);
                fb.ins().brif(need_gen, gen_block, &[], se, &[]);
                // Fallback: the ORIGINAL general CBSym lowering (variant 1 ->
                // `Vm::callbuiltinsym_for_jit`). The fast shim left the args in
                // `call_args_slot` untouched, so reuse `args_addr`; the residual
                // stack was restored above, so re-root it around this call.
                fb.switch_to_block(gen_block);
                fb.seal_block(gen_block);
                // A Tier-A fast path stored nothing; the continuation merges
                // both store histories.
                let carry_fast = rootwin_carry_snapshot();
                let saved_gen = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_model_roots_pre(fb, rt, stack, reps)
                };
                let vmctx_gen = fb.use_var(rt.vmctx_var);
                let variant_gen = fb.ins().iconst(types::I64, variant);
                let call_gen = fb.ins().call(
                    rt.refs.named_builtin,
                    &[vmctx_gen, variant_gen, sym_v, args_addr, n_val, out_addr],
                );
                let status_gen = fb.inst_results(call_gen)[0];
                emit_cond_residual_roots_post(fb, rt, saved_gen);
                rootwin_carry_meet(&carry_fast);
                let se_gen =
                    signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
                let ok_gen = icmp_imm_p(fb, IntCC::Equal, status_gen, STATUS_OK);
                fb.ins().brif(ok_gen, cont, &[], se_gen, &[]);
            } else {
                fb.ins().brif(ok, cont, &[], se, &[]);
            }
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
        Op::List(n) => {
            // N-ary list builder — infallible allocation through the shim
            // (the interpreter's Value::list_from_slice on the stack slice).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let n = *n as usize;
            if stack.len() < n {
                return Err(CompileError::StackUnderflow);
            }
            let at = stack.len() - n;
            for (i, &v) in stack[at..].iter().enumerate() {
                fb.ins()
                    .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
            }
            stack.truncate(at);
            // Root remaining live values (the allocation may GC; the shim
            // roots the operands themselves).
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let n_val = fb.ins().iconst(types::I64, n as i64);
            let call = fb.ins().call(rt.refs.list, &[args_addr, n_val]);
            let result = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            stack.push(result);
        }
        other => {
            // Slice-shaped builtins (nconc/concat/substring): spill the
            // operands and call the generic slice shim with the table index
            // baked in — the SAME builtins::*_slice function the interpreter
            // arm calls.
            if let Some((nargs, idx)) = slice_builtin_spec(other) {
                let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
                if stack.len() < nargs {
                    return Err(CompileError::StackUnderflow);
                }
                let at = stack.len() - nargs;
                for (i, &v) in stack[at..].iter().enumerate() {
                    fb.ins()
                        .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                }
                stack.truncate(at);
                let saved = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_model_roots_pre(fb, rt, stack, reps)
                };
                let idx_v = fb.ins().iconst(types::I64, idx as i64);
                let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
                let n_val = fb.ins().iconst(types::I64, nargs as i64);
                let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
                let call = fb
                    .ins()
                    .call(rt.refs.builtin_slice, &[idx_v, args_addr, n_val, out_addr]);
                let status = fb.inst_results(call)[0];
                emit_cond_residual_roots_post(fb, rt, saved);
                let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
                let cont = fb.create_block();
                let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
                fb.ins().brif(ok, cont, &[], se, &[]);
                fb.switch_to_block(cont);
                fb.seal_block(cont);
                let result = fb
                    .ins()
                    .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
                stack.push(result);
                return Ok(());
            }
            // Direct-builtin ops: pop the operands, root the rest of the live
            // frame, and call the arity-shaped generic shim with the table
            // index baked in — the shim invokes the SAME builtins::* function
            // the interpreter arm calls.
            let Some((arity, idx)) = direct_builtin_spec(other) else {
                return Err(CompileError::UnsupportedOp(op_category(other)));
            };
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let arity = arity as usize;
            if stack.len() < arity {
                return Err(CompileError::StackUnderflow);
            }
            let at = stack.len() - arity;
            let operands: Vec<ClifValue> = stack[at..].to_vec();
            stack.truncate(at);
            let value_shim = match other {
                Op::Aref => Some(rt.refs.aref),
                Op::Memq => Some(rt.refs.memq),
                Op::Assq => Some(rt.refs.assq),
                Op::Setcar => Some(rt.refs.setcar),
                Op::Setcdr => Some(rt.refs.setcdr),
                _ => None,
            };
            if let Some(value_shim) = value_shim {
                // `neovm_jit_aref`/`_memq`/`_assq`/`_setcar`/`_setcdr` answer
                // the result's bits or VALUE_SHIM_SIGNAL (tag 0b001, never a
                // Lisp value). GC-free like the pure table entries they stand
                // in for: no roots.
                //
                // JIT `aref` of a plain vector or record at an in-range fixnum
                // index reads the slot inline and calls the shim for anything
                // else (see `emit_inline_aref`).
                let inline_aref = if matches!(other, Op::Aref) && !aot && jit_inline_aref_on() {
                    let merge = fb.create_block();
                    let slow = fb.create_block();
                    let res = fb.declare_var(types::I64);
                    emit_inline_aref(fb, operands[0], operands[1], slow, res, merge).then(|| {
                        fb.switch_to_block(slow);
                        fb.seal_block(slow);
                        (merge, res)
                    })
                } else {
                    None
                };
                let vmctx = fb.use_var(rt.vmctx_var);
                let call = fb
                    .ins()
                    .call(value_shim, &[vmctx, operands[0], operands[1]]);
                let word = fb.inst_results(call)[0];
                let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
                let cont = fb.create_block();
                let tag = band_imm_p(fb, word, TAG_MASK as i64);
                let is_signal = icmp_imm_p(
                    fb,
                    IntCC::Equal,
                    tag,
                    dispatch::VALUE_SHIM_SIGNAL & TAG_MASK as i64,
                );
                fb.ins().brif(is_signal, se, &[], cont, &[]);
                fb.switch_to_block(cont);
                fb.seal_block(cont);
                match inline_aref {
                    Some((merge, res)) => {
                        fb.def_var(res, word);
                        fb.ins().jump(merge, &[]);
                        fb.switch_to_block(merge);
                        fb.seal_block(merge);
                        stack.push(fb.use_var(res));
                    }
                    None => stack.push(word),
                }
                return Ok(());
            }
            // Root remaining live values (the builtin may allocate/GC; the
            // shim roots the operands themselves) — unless the builtin cannot
            // collect at all (`dispatch::JitBuiltin2Pure`). Such a site stores
            // nothing and cannot start a nested activation, so the root-window
            // store record stays exact across it (`RootWinCarry` rule 1 applies
            // only to sites that can).
            let gc_free = match arity {
                1 => dispatch::JIT_BUILTIN1_PURE[idx].is_some(),
                2 => dispatch::JIT_BUILTIN2_PURE[idx].is_some(),
                _ => false,
            };
            let saved = if stack.is_empty() || gc_free {
                CondRoots::NONE
            } else {
                emit_model_roots_pre(fb, rt, stack, reps)
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let idx_v = fb.ins().iconst(types::I64, idx as i64);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let shim = match arity {
                1 => rt.refs.builtin1,
                2 => rt.refs.builtin2,
                _ => rt.refs.builtin3,
            };
            let mut call_args = vec![vmctx, idx_v];
            call_args.extend(operands);
            call_args.push(out_addr);
            let call = fb.ins().call(shim, &call_args);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
            let cont = fb.create_block();
            let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
    }
    Ok(())
}
