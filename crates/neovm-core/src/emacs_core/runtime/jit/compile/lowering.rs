//! Bytecode-to-CLIF lowering: fixnum guard/retag helpers, the fixnum/predicate/car-cdr op lowerings, MIR lowering (lower_mir_pure) and its runtime-context and deopt emission, the ISA config, and the per-op baseline lowering (lower_simple_op).
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

/// Lower a no-argument straight-line leaf body. Thin wrapper over [`lower_leaf`]
/// kept for the existing call sites/tests.
pub fn lower_nullary_leaf(ops: &[Op], constants: &[Value]) -> Result<CompiledLeaf, CompileError> {
    lower_leaf(ops, constants, 0)
}

/// Get MIR value `v` as a RAW (untagged) fixnum i64 for arithmetic. If `cval_raw`
/// marks it already raw (a prior fixnum arithmetic result or fixnum constant in
/// this block), use it directly — no re-guard, no re-untag (the unboxing fast
/// path: chained fixnum arithmetic stays raw). Otherwise guard it is a fixnum
/// (deopt else) and untag.
pub(crate) fn mir_as_raw(
    fb: &mut FunctionBuilder,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    v: mir::MirValue,
    deopt: Block,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        Ok(cv)
    } else {
        guard_fixnum(fb, deopt, cv, &HashSet::new());
        Ok(sshr_imm_p(fb, cv, FIXNUM_SHIFT as i64))
    }
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
/// returning the tagged value. Wired by the calls-slice (next increment); kept
/// separate so the soundness-critical force-tag/deopt-routing logic lands and is
/// reviewable on its own. Use before a call (a GC SAFEPOINT): a raw
/// (untagged) fixnum must not be live across a call — the concurrent GC would
/// trace the bare i64 as a tagged pointer (a raw `3` has bits `0b011` == TAG_CONS
/// -> a bogus rooted cons -> UAF). Unlike [`mir_as_tagged`] (which retags WITHOUT
/// writing back), this clears the raw mask so every LATER use and every
/// deopt-framestate snapshot sees the tagged form — no stale raw alias survives
/// the safepoint. The MIR analogue of the baseline's `stack_force_tagged`.
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
        if on && is_nonheap_const(fb, v) {
            continue; // provably non-heap immediate: nothing to root.
        }
        to_root.push(v);
    }
    if to_root.is_empty() {
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
pub(crate) fn mir_deopt_block(
    fb: &mut FunctionBuilder,
    precise: bool,
    inst: &mir::MirInst,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    shared: &mut Option<Block>,
    pending: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    if precise {
        let mut stack = Vec::with_capacity(inst.pre_stack.len());
        let mut raw = Vec::with_capacity(inst.pre_stack.len());
        for v in &inst.pre_stack {
            stack.push(cval[v.0 as usize].ok_or(CompileError::BadOperand)?);
            raw.push(cval_raw[v.0 as usize]);
        }
        // handlers_len = 0: build_mir bails on handler/bind opcodes, so a MIR leaf
        // never has condition-case/catch frames to transfer on resume.
        Ok(deopt_site(fb, inst.pc, 0, &stack, &raw, pending))
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

/// **MIR Tier-2 lowering.** Lower a [`mir::MirFunction`] to a [`CompiledLeaf`] by
/// driving CLIF emission from the MIR instead of a bytecode walk. Wired into
/// `compile_bytecode_function_inner` as the live optimizing tier. A *pure* body
/// (arithmetic / comparisons / type predicates / car-cdr / stack — no shim-using
/// ops) needs no vmctx and reruns the interpreter from the start on a failing
/// guard (sound: no side effect precedes any guard). A call-bearing body threads
/// vmctx + the runtime shims and routes every guard to a per-site precise deopt
/// (see below).
///
/// Uses CLIF **block parameters** as the SSA phis — each MIR block becomes a
/// CLIF block whose params are its entry operand stack, and terminator edges
/// pass the live stack as block arguments. Validated by differential tests
/// against the interpreter and the force-deopt gate.
pub(crate) fn lower_mir_pure(m: &mir::MirFunction) -> Result<CompiledLeaf, CompileError> {
    use mir::MirOp;

    // The MIR tier handles a CALL (MirOp::Opaque{Call/Apply}) via PRECISE deopt:
    // such a body threads vmctx + the runtime shims and routes EVERY guard to a
    // per-site STATUS_DEOPT_AT (all-precise — a call-bearing body must never
    // rerun-from-start, which would re-execute the call's side effect).
    let has_call = m.blocks.iter().any(|b| {
        b.insts.iter().any(|i| {
            matches!(
                &i.op,
                MirOp::Opaque {
                    op: Op::Call(_) | Op::Apply(_),
                    ..
                }
            )
        })
    });

    // Escape analysis (hoisted — depends only on `m`). A NON-escaping cons is elided
    // (scalar-replaced, no allocation); an ESCAPING cons is heap-allocated via the
    // neovm_jit_cons shim so the body stays in the MIR tier. Both the calls-slice and
    // cons allocation need the runtime scaffolding (needs_rt: vmctx + shims), but a
    // cons allocation is a GC SAFEPOINT, NOT an observable side effect — so it does
    // NOT force precise deopt. precise (+ has_side_effects) stay = has_call:
    // rerun-from-start re-allocates a fresh (never-escaped) cons, which is sound.
    let cons_repl: Vec<Option<(mir::MirValue, mir::MirValue)>> = if has_call {
        vec![None; m.value_types.len()]
    } else {
        mir::cons_scalar_repl_targets(m)
    };
    let has_escaping_cons = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .any(|i| matches!(&i.op, MirOp::Cons(..)) && cons_repl[i.result.0 as usize].is_none());
    let needs_rt = has_call || has_escaping_cons;

    // --- JIT-only module prologue (the wrapper). ----------------------------
    // The three ObjectModule-incompatible seams that stay here (and out of the
    // generic build fn `build_mir_leaf_fn`): `builder.symbol(...)` bakes the shim
    // host addresses (AOT replaces this with `Linkage::Import` + dlopen);
    // `JITModule::new` (AOT: `ObjectModule::new`); `finalize_definitions` +
    // `get_finalized_function` below (AOT: `ObjectModule::finish()` + `dlsym`).
    let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
    if needs_rt {
        // The shims the calls-slice + cons allocation reference; declare_rt_refs
        // declares the full import set but Cranelift resolves only referenced ones.
        builder.symbol("neovm_jit_gc_save", neovm_jit_gc_save as *const u8);
        builder.symbol("neovm_jit_gc_push", neovm_jit_gc_push as *const u8);
        builder.symbol(
            "neovm_jit_gc_push_many",
            neovm_jit_gc_push_many as *const u8,
        );
        builder.symbol("neovm_jit_gc_restore", neovm_jit_gc_restore as *const u8);
        builder.symbol(
            "neovm_jit_rootwin_grow",
            neovm_jit_rootwin_grow as *const u8,
        );
        builder.symbol("neovm_jit_call", neovm_jit_call as *const u8);
        builder.symbol("neovm_jit_apply", neovm_jit_apply as *const u8);
        builder.symbol("neovm_jit_cons", neovm_jit_cons as *const u8);
    }
    let mut module = JITModule::new(builder);

    // Precise-deopt spill buffer + cells, sized to the deepest pre-op operand stack
    // (the framestate a post-call guard spills). Empty/inert for pure bodies (which
    // keep the rerun-from-start STATUS_DEOPT path).
    let max_depth = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .map(|i| i.pre_stack.len())
        .max()
        .unwrap_or(0);
    let deopt_spill: Box<[core::cell::Cell<i64>]> = if has_call {
        (0..max_depth).map(|_| core::cell::Cell::new(0)).collect()
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
        has_call,
        &cons_repl,
        needs_rt,
        "__neovm_mir_leaf",
        Linkage::Local,
        /*aot=*/ false,
    )?;

    // --- JIT-only module epilogue (the wrapper). ----------------------------
    module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
    let entry = module.get_finalized_function(fid);

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
        // A call-bearing body runs a side effect ahead of its (precise) deopts, so
        // it must never rerun-from-start (the refuse-to-rerun guard).
        has_side_effects: has_call,
        // Baseline default; compile_bytecode_function_inner overrides with the
        // actual inlined-callee SymIds after the inline pass.
        inline_deps: Box::from([]),
        spec_slots: Box::from([]),
        spec_expected: Box::from([]),
        deopt_spill,
        deopt_meta,
        reloc_data,
        // JIT bakes its bases as iconst; the 4th entry arg is ignored.
        sidecar: None,
        // MIR leaves are only built for unpatched sources (see
        // compile_bytecode_function_inner).
        dynamic_prefix: 0,
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
/// for a `Full` policy or a body with a back-edge (a loop can run unboundedly
/// per entry, so its code quality is worth the compile), and the fast one for
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

/// Module-generic build seam for [`lower_mir_pure`]: sets up the leaf ABI
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
/// JIT seams, which stay in the [`lower_mir_pure`] wrapper:
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
    has_call: bool,
    cons_repl: &[Option<(mir::MirValue, mir::MirValue)>],
    needs_rt: bool,
    entry_name: &str,
    entry_linkage: Linkage,
    // R1c-sidecar: false → JIT (bases baked as `iconst` from the passed-in buffer
    // addresses, unchanged/fast); true → AOT (bases loaded from the 4th entry arg,
    // the per-thread `LeafSidecar`, since the addresses are session-specific). The
    // CLIF body is otherwise identical — same RESULTS either way.
    aot: bool,
) -> Result<cranelift_module::FuncId, CompileError> {
    imm_pool_reset();
    use mir::{BinKind, CmpKind, MirOp, MirTerm, PredKind as MP, UnaryKind as MU};

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

        // Runtime context for calls (vmctx + shims + arg/result slots), built only
        // when the body has a call. declare_rt_refs declares the full import set;
        // only the referenced shims (call/apply/gc_*) are resolved at finalize.
        let rt = if needs_rt {
            // `module` is already `&mut M`; reborrow it for the call. The MIR
            // tier never emits subr-speculated or CBSym-intrinsic calls
            // (subr_spec=false, cbsym_spec=false).
            let refs = declare_rt_refs(&mut *module, fb.func, call_conv, ptr_ty, false, false)?;
            let vmctx_var = fb.declare_var(ptr_ty);
            let max_call_args = m
                .blocks
                .iter()
                .flat_map(|b| b.insts.iter())
                .filter_map(|i| match &i.op {
                    MirOp::Opaque {
                        op: Op::Call(n) | Op::Apply(n),
                        ..
                    } => Some(*n as usize),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let call_args_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_call_args.max(1) * 8) as u32,
                3,
            ));
            let call_result_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            // Lever 2: residual gather buffer, sized to the max pre-op operand-stack
            // depth (an upper bound on any site's residual count).
            let max_residual = m
                .blocks
                .iter()
                .flat_map(|b| b.insts.iter())
                .map(|i| i.pre_stack.len())
                .max()
                .unwrap_or(0);
            let residual_buf_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_residual.max(1) * 8) as u32,
                3,
            ));
            let gc_saved_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            Some(RtCtx {
                refs,
                vmctx_var,
                ptr_ty,
                call_args_slot,
                call_result_slot,
                residual_buf_slot,
                gc_saved_slot,
                rootwin: None,
            })
        } else {
            None
        };
        // The deopt-buffer base addresses (for the JIT `iconst` path). The CLIF
        // `DeoptRefs` (iconst or sidecar-load) is materialized in the entry block
        // below, once it is populated and the sidecar param is available.
        let spill_base_addr = deopt_spill.as_ptr() as i64;
        let meta_pc_addr = &deopt_meta.pc as *const core::cell::Cell<i64> as i64;
        let meta_depth_addr = &deopt_meta.depth as *const core::cell::Cell<i64> as i64;
        let meta_handlers_addr = &deopt_meta.handlers as *const core::cell::Cell<i64> as i64;
        // ALL-PRECISE deopt for call-bearing bodies (see mir_deopt_block): never
        // rerun-from-start after a call. Pure bodies keep the shared rerun block.
        let precise = has_call;
        // (cons_repl + needs_rt computed by the wrapper, threaded in as params.)
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
        // are tagged (false) — no raw phis (the simpler, sound scope).
        let mut cval_raw: Vec<bool> = vec![false; m.value_types.len()];

        // Shared deopt landing block: pure bodies rerun the interpreter from the
        // start (STATUS_DEOPT), created lazily on the first guard.
        let mut deopt: Option<Block> = None;

        // Function-entry block: stash the out pointer + load args, jump into MIR
        // block 0 passing the args as block params.
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        let vmctx_param = fb.block_params(entry)[0];
        if let Some(rt) = &rt {
            fb.def_var(rt.vmctx_var, vmctx_param);
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
            // Bind this block's params to the CLIF block params.
            let bp = fb.block_params(cb).to_vec();
            for (p, &cv) in blk.params.iter().zip(bp.iter()) {
                cval[p.0 as usize] = Some(cv);
            }

            for inst in &blk.insts {
                let r = inst.result.0 as usize;
                match &inst.op {
                    MirOp::Arg(_) => {
                        // The param already holds the argument (bound above).
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
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(&mut fb, &cval, &cval_raw, *a, d)?;
                        let bv = mir_as_raw(&mut fb, &cval, &cval_raw, *b, d)?;
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
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(&mut fb, &cval, &cval_raw, *a, d)?;
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
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(&mut fb, &cval, &cval_raw, *a, d)?;
                        let bv = mir_as_raw(&mut fb, &cval, &cval_raw, *b, d)?;
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
                            // Symbolp/Integerp/Numberp use shims; deferred.
                            _ => return Err(CompileError::UnsupportedOp("mir-pure-pred")),
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
                    // live-across-call residual, dispatch the GENERIC shim (no spec
                    // plumbing in the MIR tier), propagate a signal, and on STATUS_OK
                    // push the tagged result. The body's guards are all precise
                    // (`precise == has_call`), so no rerun-from-start re-runs this.
                    MirOp::Opaque { op, args } if matches!(op, Op::Call(_) | Op::Apply(_)) => {
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
                            if on && m.value_type(rv).never_needs_gc_root() {
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
                        // keeps any over-approximation harmless. The saved
                        // depth crosses the callee via `gc_saved_slot`, with
                        // -1 marking "nothing rooted".
                        let saved = if to_root.is_empty() {
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
                    // An ESCAPING cons is heap-allocated via the neovm_jit_cons shim —
                    // a GC SAFEPOINT, but NOT an observable side effect (a fresh
                    // unshared object), so it needs NO precise deopt: rerun-from-start
                    // (pure body) re-allocates a fresh cons the caller never saw, and a
                    // call-bearing body spills the allocated cons (a real Value) into
                    // its precise framestate normally. Force-tag car+cdr (no raw fixnum
                    // into the heap pair / across the safepoint; the shim self-roots
                    // them) + gc-root the live-across-allocation residual, like a call.
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
                    // Shim-using ops, deferred: `eq` needs the symbols-with-position
                    // slow-path shim (vmctx) so plain tagged-bits comparison would
                    // diverge when symbols-with-pos-enabled; other `opaque`
                    // (VarRef/builtins/...) not yet ported.
                    MirOp::Eq(..) | MirOp::Opaque { .. } => {
                        return Err(CompileError::UnsupportedOp("mir-pure-shim-op"));
                    }
                }
            }

            // Terminator.
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
                    let mut a: Vec<BlockArg> = Vec::with_capacity(args.len());
                    for v in args {
                        a.push(BlockArg::Value(mir_force_tagged(
                            &mut fb,
                            &mut cval,
                            &mut cval_raw,
                            *v,
                        )?));
                    }
                    fb.ins().jump(clif_blocks[target.0 as usize], &a);
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
                    let tb = clif_blocks[taken.0 as usize];
                    let fbk = clif_blocks[fallthrough.0 as usize];
                    // brif takes the `then` block when the condition is true.
                    if *on_nil {
                        fb.ins().brif(is_nil, tb, &ta, fbk, &fa);
                    } else {
                        fb.ins().brif(is_nil, fbk, &fa, tb, &ta);
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
        emit_pending_deopts(&mut fb, deopt_refs, &mut pending);

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

    let fid = module
        .declare_function(entry_name, entry_linkage, &sig)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    super::note_clif_size(&ctx.func);
    module
        .define_function(fid, &mut ctx)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
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
    /// Lever 2: gather buffer for BATCHED residual GC rooting. Sized to the body's
    /// max operand-stack depth (an upper bound on any site's residual count).
    /// Reused per residual-rooting site — `neovm_jit_gc_push_many` reads it
    /// synchronously, so the next site's stores may overwrite it.
    pub(crate) residual_buf_slot: StackSlot,
    /// Conditional-rooting saved scratch depth for the current call site:
    /// `-1` when the site's runtime tag tests found no heap residual (all
    /// three rooting shims skipped), else the `gc_save` result the post-call
    /// restore consumes. Written and read within one site — reusable.
    pub(crate) gc_saved_slot: StackSlot,
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
        std::cell::RefCell::new(RootWinCarry { stored: Vec::new(), emitted: 0, elided: 0 });
}

/// The per-window-index SSA value the last hoisted site stored.
///
/// Why eliding a store whose recorded value matches is exact: between two
/// sites of one activation the window slots below the first site's count
/// are written by nobody — our sites are the only writers of this frame's
/// slots, a nested activation works above `top`, and the collector only
/// reads — so a slot whose stored SSA value is unchanged still holds it.
/// Slots at or above a site's count may be clobbered by the nested
/// activation during its call, so the record is truncated to that count.
///
/// Why resetting only at bytecode leaders is exact: sites are the window's
/// only writers, and every internal block an op's lowering creates (status
/// checks, guards, fast/slow merges) has all its predecessors inside that
/// op, so within one bytecode basic block every path from the previous
/// site to the next runs the same stores. The only edges that can reach a
/// site with a different store history are jumps to a bytecode leader
/// (loop heads, branch targets, handlers, the OSR entry) — the record is
/// dropped there.
struct RootWinCarry {
    stored: Vec<Option<ClifValue>>,
    /// Diagnostics: root-window stores emitted / elided in this function.
    emitted: u32,
    elided: u32,
}

/// Forget the carried record: a new function, or a bytecode leader.
pub(crate) fn rootwin_carry_reset() {
    ROOTWIN_CARRY.with(|c| c.borrow_mut().stored.clear());
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

/// `NEOVM_JIT_DUMP_CLIF=<path>`: append every lowered function's CLIF (with
/// an `;; ops=N` header) to `path` — the IR-composition census.
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
        let _ = writeln!(f, ";; {header}\n{}\n", func.display());
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
    pub(crate) gc_save: FuncRef,
    pub(crate) gc_push: FuncRef,
    pub(crate) gc_push_many: FuncRef,
    pub(crate) gc_restore: FuncRef,
    pub(crate) cons: FuncRef,
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
    let mut sig_ret = Signature::new(call_conv); // () -> i64
    sig_ret.returns.push(AbiParam::new(i64t));
    let mut sig_arg = Signature::new(call_conv); // (i64) -> ()
    sig_arg.params.push(AbiParam::new(i64t));
    let mut sig_push_many = Signature::new(call_conv); // (ptr, i64) -> ()  (lever 2 batch)
    sig_push_many.params.push(AbiParam::new(ptr_ty));
    sig_push_many.params.push(AbiParam::new(i64t));
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

    let save_id = declare(module, "neovm_jit_gc_save", &sig_ret)?;
    let push_id = declare(module, "neovm_jit_gc_push", &sig_arg)?;
    let push_many_id = declare(module, "neovm_jit_gc_push_many", &sig_push_many)?;
    let restore_id = declare(module, "neovm_jit_gc_restore", &sig_arg)?;
    // (vmctx, need) -> (): same param shape as push_many.
    let rootwin_grow_id = declare(module, "neovm_jit_rootwin_grow", &sig_push_many)?;
    let cons_id = declare(module, "neovm_jit_cons", &sig_cons)?;
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
        gc_save: module.declare_func_in_func(save_id, func),
        gc_push: module.declare_func_in_func(push_id, func),
        gc_push_many: module.declare_func_in_func(push_many_id, func),
        gc_restore: module.declare_func_in_func(restore_id, func),
        cons: module.declare_func_in_func(cons_id, func),
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

/// A precise-deopt exit block queued at a guard-emitting op: created (and
/// targeted by that op's guards) during lowering, filled after the bytecode
/// block terminates. Captures the op's index and the operand stack snapshot
/// from BEFORE the op popped its operands — the interpreter reruns the
/// failing op itself.
pub(crate) struct PendingDeopt {
    pub(crate) block: Block,
    pub(crate) pc: usize,
    pub(crate) handlers_len: usize,
    pub(crate) stack: Vec<ClifValue>,
    /// Per-slot raw mask snapshot (cross-op unboxing): `true` slots hold an
    /// untagged i64 and must be retagged in the cold deopt block before the
    /// framestate spill, since `run_resumed_frame` reads them back as tagged
    /// `Value`s.
    pub(crate) stack_raw: Vec<bool>,
}

/// Queue (and return) the precise-deopt block for the guard-emitting op at
/// bytecode index `pc`, capturing the pre-op operand stack + its raw mask.
pub(crate) fn deopt_site(
    fb: &mut FunctionBuilder,
    pc: usize,
    handlers_len: usize,
    stack: &[ClifValue],
    stack_raw: &[bool],
    pending: &mut Vec<PendingDeopt>,
) -> Block {
    let block = fb.create_block();
    pending.push(PendingDeopt {
        block,
        pc,
        handlers_len,
        stack: stack.to_vec(),
        stack_raw: stack_raw.to_vec(),
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

/// Fill the precise-deopt blocks queued within one bytecode block: spill the
/// captured live stack, record pc/depth/handler-count, and return
/// [`STATUS_DEOPT_AT`]. For `Baked` (JIT) the base addresses are iconst'd HERE in
/// the cold block (off the hot path); for `Sidecar` (AOT) they are the
/// entry-block loaded values.
thread_local! {
    /// IR-size facts of the most recent baseline compile on this thread —
    /// `(clif insts, blocks, deopt sites, deopt snapshot slots)` — read by
    /// `stats::record_compile` for its per-compile trace line. Diagnostic only.
    pub(crate) static LAST_IR_STATS: core::cell::Cell<(u32, u32, u32, u32)> =
        const { core::cell::Cell::new((0, 0, 0, 0)) };
}

pub(crate) fn emit_pending_deopts(
    fb: &mut FunctionBuilder,
    refs: DeoptRefs,
    pending: &mut Vec<PendingDeopt>,
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
        for (j, &v) in pd.stack.iter().enumerate() {
            // Retag raw fixnum slots in the COLD deopt block (zero hot-path cost):
            // the framestate is read back as tagged Values by run_resumed_frame.
            let tagged = if pd.stack_raw[j] {
                retag_fixnum(fb, v)
            } else {
                v
            };
            fb.ins()
                .store(MemFlagsData::trusted(), tagged, spill_base, (j * 8) as i32);
        }
        let pc_v = fb.ins().iconst(types::I64, pd.pc as i64);
        fb.ins().store(MemFlagsData::trusted(), pc_v, meta_pc, 0);
        let depth_v = fb.ins().iconst(types::I64, pd.stack.len() as i64);
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
) -> Block {
    if handlers.is_empty() {
        return *signal_exit.get_or_insert_with(|| fb.create_block());
    }
    let block = fb.create_block();
    pending.push(PendingDispatch {
        block,
        handlers: handlers.to_vec(),
        stack: stack.to_vec(),
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
        let saved = if pd.stack.is_empty() {
            CondRoots::NONE
        } else {
            emit_cond_residual_roots_pre(fb, rt, &pd.stack)
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
            for (j, &v) in pd.stack.iter().take(push_depth).enumerate() {
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

/// Get model-stack slot `k` as a RAW (untagged) fixnum i64 for arithmetic. If the
/// slot is already raw (a prior fixnum arithmetic result in this block), return it
/// directly — the cross-op fast path: no re-guard, no re-untag. Otherwise guard it
/// is a fixnum (deopt else, honoring the cross-block `known` elision) and untag once.
pub(crate) fn stack_as_raw(
    fb: &mut FunctionBuilder,
    deopt: Block,
    stack: &[ClifValue],
    stack_raw: &[bool],
    k: usize,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    if stack_raw[k] {
        stack[k]
    } else {
        guard_fixnum(fb, deopt, stack[k], known);
        sshr_imm_p(fb, stack[k], FIXNUM_SHIFT as i64)
    }
}

/// Retag model-stack slot `k` to a tagged `Value` if it currently holds a raw
/// fixnum, clearing its raw flag. Used at every boundary where a value escapes the
/// in-flight arithmetic (returns, predicates, car/cdr, calls/gc roots, cross-block
/// edges, deopt/signal snapshots).
pub(crate) fn stack_force_tagged(
    fb: &mut FunctionBuilder,
    stack: &mut [ClifValue],
    stack_raw: &mut [bool],
    k: usize,
) {
    if stack_raw[k] {
        stack[k] = retag_fixnum(fb, stack[k]);
        stack_raw[k] = false;
    }
}

/// Force every raw slot in the model stack back to a tagged `Value`. Called before
/// any op/terminator that gc_pushes, calls a shim, snapshots the stack for signal
/// dispatch, or writes the stack to `vars` (cross-block) — so nothing raw ever
/// escapes the block or reaches the tracer.
pub(crate) fn retag_all_raw(
    fb: &mut FunctionBuilder,
    stack: &mut [ClifValue],
    stack_raw: &mut [bool],
) {
    for k in 0..stack.len() {
        stack_force_tagged(fb, stack, stack_raw, k);
    }
}

/// Ops that participate in cross-op fixnum unboxing: they maintain `stack_raw`
/// themselves (arithmetic produces raw results, comparisons consume raw operands,
/// stack shuffles move the raw flags). EVERY OTHER op force-tags the stack first
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
    )
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
    // Per-slot raw mask (cross-op unboxing), kept in lockstep with `stack`.
    stack_raw: &mut Vec<bool>,
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
    // the GC-root + dispatch-snapshot soundness holes in one place).
    if !op_preserves_raw(op) {
        retag_all_raw(fb, stack, stack_raw);
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
            stack_raw.push(false);
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
            stack_raw.push(false);
        }
        Op::Nil => {
            stack.push(fb.ins().iconst(types::I64, Value::NIL.bits() as i64));
            stack_raw.push(false);
        }
        Op::True => {
            stack.push(fb.ins().iconst(types::I64, Value::T.bits() as i64));
            stack_raw.push(false);
        }
        Op::Pop => {
            stack.pop().ok_or(CompileError::StackUnderflow)?;
            stack_raw.pop();
        }
        Op::Dup => {
            let top = *stack.last().ok_or(CompileError::StackUnderflow)?;
            let top_raw = *stack_raw.last().ok_or(CompileError::StackUnderflow)?;
            stack.push(top);
            stack_raw.push(top_raw);
        }
        Op::StackRef(n) => {
            // 0 = top of stack, 1 = one below, ...
            let n = *n as usize;
            let idx = stack
                .len()
                .checked_sub(1 + n)
                .ok_or(CompileError::StackUnderflow)?;
            stack.push(stack[idx]);
            stack_raw.push(stack_raw[idx]);
        }
        Op::StackSet(n) => {
            // Assign TOS into the slot N below TOS, then pop TOS (N = 0 == pop).
            let n = *n as usize;
            let top = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let top_raw = stack_raw.pop().ok_or(CompileError::StackUnderflow)?;
            if n != 0 {
                let idx = stack
                    .len()
                    .checked_sub(n)
                    .ok_or(CompileError::StackUnderflow)?;
                stack[idx] = top;
                stack_raw[idx] = top_raw;
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
                    stack_raw[target] = stack_raw[len - 1];
                } else if n > len {
                    return Err(CompileError::StackUnderflow);
                }
                stack.truncate(len - n);
                stack_raw.truncate(len - n);
            }
        }
        Op::Add | Op::Sub => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            let is_sub = matches!(op, Op::Sub);
            stack.push(raw_fixnum_addsub(fb, dsite, is_sub, a, b));
            stack_raw.push(true);
        }
        Op::Mul => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            stack.push(raw_fixnum_mul(fb, dsite, a, b));
            stack_raw.push(true);
        }
        Op::Div | Op::Rem => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            let is_rem = matches!(op, Op::Rem);
            stack.push(raw_fixnum_divrem(fb, dsite, is_rem, a, b));
            stack_raw.push(true);
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
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            stack.truncate(n - 1);
            stack_raw.truncate(n - 1);
            let kind = match op {
                Op::Add1 => UnaryKind::Add1,
                Op::Sub1 => UnaryKind::Sub1,
                Op::Negate => UnaryKind::Negate,
                _ => unreachable!("matched Add1/Sub1/Negate above"),
            };
            stack.push(raw_fixnum_unop(fb, dsite, kind, a));
            stack_raw.push(true);
        }
        Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            let cc = match op {
                Op::Eqlsign => IntCC::Equal,
                Op::Lss => IntCC::SignedLessThan,
                Op::Gtr => IntCC::SignedGreaterThan,
                Op::Leq => IntCC::SignedLessThanOrEqual,
                Op::Geq => IntCC::SignedGreaterThanOrEqual,
                _ => unreachable!("matched comparison ops above"),
            };
            // Operands raw, result is a tagged t/nil (a sink, not raw).
            let cond = fb.ins().icmp(cc, a, b);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            stack.push(fb.ins().select(cond, t, nil));
            stack_raw.push(false);
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
            // Non-raw: the top-of-fn retag_all_raw already tagged the stack; the
            // deopt snapshot's mask is all-false (cold retag is a no-op here).
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
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
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            stack.push(raw_fixnum_maxmin(fb, matches!(op, Op::Min), a, b));
            stack_raw.push(true);
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
            // Root live stack values: variable access may allocate.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call = fb.ins().call(rt.refs.varref, &[vmctx, sym_v, out_addr]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
        Op::VarSet(idx) => {
            // Assign through the runtime (may run variable watchers — arbitrary
            // lisp — and signal). A side effect: poisons later guards.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            let val = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            let call = fb.ins().call(rt.refs.varset, &[vmctx, sym_v, val]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
            let spec = spec.filter(|&(sym, ..)| {
                let proven =
                    callee_is_symbol_const(fb, stack[args_at - 1], sym, reloc_base, reloc_index);
                if !proven {
                    tracing::debug!(
                        target: "neovm_jit",
                        sym,
                        "spec site dropped: callee slot not provably the tracked symbol"
                    );
                }
                proven
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
                let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
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
                stack_raw.truncate(args_at - 1);
                stack.push(res);
                stack_raw.push(false);
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
            // Root every value that stays live across the call (the callee +
            // args are rooted by the shim; the constants are rooted by the
            // dispatch seam via the executing function). Pred/EqIncl direct
            // paths SKIP this: their shims are GC-free by contract (they bounce
            // to the fallback block rather than run anything that could
            // allocate), and the fallback block roots for itself.
            let saved = if stack.is_empty() || reg_args.is_some() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let n_val = fb.ins().iconst(types::I64, n as i64);
            // Subr-kind sites can return STATUS_NEED_GENERIC, which routes to a
            // fallback block that re-does this site as the ORIGINAL generic
            // call; bytecode-kind sites keep their everything-inside-the-shim
            // protocol. `None` = no NEED_GENERIC possible.
            let mut generic_fallback: Option<Block> = None;
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
                    generic_fallback = Some(fb.create_block());
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
                            SpecCalleeKind::PredRecordp | SpecCalleeKind::PredSymbolWithPos,
                            Some(args),
                        ) => {
                            let f = rt
                                .refs
                                .pred_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            let kind_v = fb.ins().iconst(
                                types::I64,
                                if kind == SpecCalleeKind::PredRecordp {
                                    PRED_KIND_RECORDP
                                } else {
                                    PRED_KIND_SYMBOL_WITH_POS_P
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
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
                let saved_gen = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
                };
                let vmctx_gen = fb.use_var(rt.vmctx_var);
                let call_gen = fb
                    .ins()
                    .call(shim, &[vmctx_gen, func_val, args_addr, n_val, out_addr]);
                let status_gen = fb.inst_results(call_gen)[0];
                emit_cond_residual_roots_post(fb, rt, saved_gen);
                let se_gen = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let call = fb.ins().call(rt.refs.varbind, &[vmctx, sym_v, val]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let cont = fb.create_block();
            let signal =
                signal_target_for_site(fb, signal_exit, handlers, pending, stack.as_slice());
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
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let call = fb.ins().call(rt.refs.unbind, &[vmctx, n_v]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let cont = fb.create_block();
            let signal =
                signal_target_for_site(fb, signal_exit, handlers, pending, stack.as_slice());
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
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call = fb
                .ins()
                .call(rt.refs.save_window_excursion, &[vmctx, body, out_addr]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
        Op::CallBuiltin(..) | Op::CallBuiltinSym(..) | Op::Aset => {
            // Named-builtin escape hatch + aset: route through the
            // Vm::*_for_jit helpers mirroring the interpreter arms
            // (override-aware / advice-bypassing / writeback / quit poll).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let (variant, sym, nargs): (i64, u32, usize) = match op {
                Op::CallBuiltin(name_idx, n) => {
                    (0, const_sym_id(constants, *name_idx)?, *n as usize)
                }
                Op::CallBuiltinSym(sym, n) => (1, sym.0, *n as usize),
                Op::Aset => (2, 0, 3),
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
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            // R2-E (must-nail #2): the named-builtin callee SymId is session-specific.
            // The JIT bakes it (`iconst(sym)`); AOT must RELOC it BY NAME — the op's
            // symbol Value was collected into the per-leaf reloc vector, so load its
            // bits from reloc_base[idx] and recover the SymId (`bits >> TAG_BITS`,
            // TAG_SYMBOL==0). Keyed on `reloc_index` presence: the JIT reloc set never
            // contains op-symbols (only heap consts), so JIT always bakes → byte-
            // identical. `Aset` (variant 2, sym==0) has no symbol → unchanged iconst.
            // Shared by the fast-shim call, the direct general call, AND the
            // fallback (all JIT-only when a CBSym spec site exists).
            let sym_v = match reloc_index
                .get(&((sym as usize) << TAG_BITS | TAG_SYMBOL))
                .filter(|_| variant != 2)
            {
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
            let call = if let Some(which) = cbsym_a_which {
                generic_fallback = Some(fb.create_block());
                let f = rt
                    .refs
                    .cbsym_read
                    .ok_or(CompileError::UnsupportedOp("cbsym-read-refs"))?;
                let which_v = fb.ins().iconst(types::I64, which as i64);
                fb.ins()
                    .call(f, &[vmctx, which_v, sym_v, args_addr, n_val, out_addr])
            } else if cbsym_spec_b {
                generic_fallback = Some(fb.create_block());
                let f = rt
                    .refs
                    .cbsym_spec
                    .ok_or(CompileError::UnsupportedOp("cbsym-spec-refs"))?;
                fb.ins()
                    .call(f, &[vmctx, sym_v, args_addr, n_val, out_addr])
            } else {
                let variant_v = fb.ins().iconst(types::I64, variant);
                fb.ins().call(
                    rt.refs.named_builtin,
                    &[vmctx, variant_v, sym_v, args_addr, n_val, out_addr],
                )
            };
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
                let saved_gen = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
                };
                let vmctx_gen = fb.use_var(rt.vmctx_var);
                let variant_gen = fb.ins().iconst(types::I64, variant);
                let call_gen = fb.ins().call(
                    rt.refs.named_builtin,
                    &[vmctx_gen, variant_gen, sym_v, args_addr, n_val, out_addr],
                );
                let status_gen = fb.inst_results(call_gen)[0];
                emit_cond_residual_roots_post(fb, rt, saved_gen);
                let se_gen = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
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
                    emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
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
                let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
            // Root remaining live values (the builtin may allocate/GC; the
            // shim roots the operands themselves).
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
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
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
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
