//! Opt Boolean result lowering, apart from the literal baseline producer path.
//!
//! Threading: compiler-owned CLIF handles, representations and verified result
//! choices only. No runtime metadata, Lisp state, cached values or shim ABI.
//! Unsupported producers decline before altering the model stack. Every call,
//! heap-store and non-Boolean opcode continues through current-main arms.

use super::*;

pub(crate) fn lower_simple_op_with_cons_proof_and_result(
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
    cons_proof: super::super::heap_inline::ConsStoreProof,
    result_mode: BoolResultMode,
) -> Result<(), CompileError> {
    if result_mode == BoolResultMode::TaggedLisp && !reps.contains(&SlotRep::Bool) {
        return lower_simple_op_with_cons_proof(
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
            cons_proof,
        );
    }
    if result_mode == BoolResultMode::BoolFlag && !BoolResultMode::supports(op) {
        return Err(CompileError::UnsupportedOp("opt-bool-result"));
    }
    // Non-unboxing operands must be tagged Values. Representation-aware roots
    // and cold snapshots can retain untouched Bool slots; resident flonums
    // retain their existing audited residual policy.
    let residual = if op_preserves_raw(op) {
        None
    } else {
        Some(prepare_op_operands(fb, rt, op, stack, reps)?)
    };
    #[cfg(debug_assertions)]
    let residual_before: Option<Vec<ClifValue>> = residual.map(|keep| stack[..keep].to_vec());
    lower_simple_op_bool_arms(
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
        cons_proof,
        result_mode,
    )?;
    // Re-sync the reps after a non-unboxing op: the slots below `keep` are
    // untouched (the audited-op invariant, checked in debug builds), and its
    // result has its requested view. Unboxing ops keep `reps` in lockstep
    // themselves; flag mode is admitted only for single-result predicates.
    if let Some(keep) = residual {
        #[cfg(debug_assertions)]
        debug_assert!(
            residual_before.as_deref() == stack.get(..keep),
            "{op:?} rewrote the model stack below its operands"
        );
        reps.truncate(keep);
        reps.resize(stack.len(), SlotRep::Tagged);
        if result_mode == BoolResultMode::BoolFlag {
            *reps.last_mut().expect("single-result Boolean opcode") = SlotRep::Bool;
        }
    }
    debug_assert_eq!(stack.len(), reps.len(), "{op:?} left reps desynced");
    Ok(())
}

fn lower_simple_op_bool_arms(
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
    cons_proof: super::super::heap_inline::ConsStoreProof,
    result_mode: BoolResultMode,
) -> Result<(), CompileError> {
    if let Some(rt) = rt
        && !aot
        && super::super::arith_site_takes_generic(op, pc)
    {
        return lower_generic_arith_site_bool(
            fb,
            rt,
            op,
            stack,
            reps,
            handlers,
            pending,
            signal_exit,
            result_mode,
        );
    }
    if result_mode == BoolResultMode::TaggedLisp {
        return lower_simple_op_arms(
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
            cons_proof,
        );
    }
    match op {
        Op::Eq => {
            // Bit-equal -> t natively. Differing bits can still be `eq` only
            // through a symbol-with-pos (GNU `slow_eq` unwraps nothing
            // else), and a symbol-with-pos is a veclike: when neither
            // operand carries the veclike tag the answer is nil right here.
            // Only a veclike operand reaches the read-only slow-path shim
            // (cold), which tests `symbols-with-pos-enabled`.
            // `NEOVM_JIT_EQ_PREFILTER=off` emits the former shape (every
            // mismatch calls the shim) for a single-build A/B.
            let rt = rt.ok_or(CompileError::UnsupportedOp("eq"))?;
            let b = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let res = fb.declare_var(result_mode.ty());
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let check = super::super::jit_eq_prefilter_on().then(|| fb.create_block());
            let same = fb.ins().icmp(IntCC::Equal, a, b);
            fb.ins().brif(same, fast, &[], check.unwrap_or(slow), &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = result_mode.constant(fb, true);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            if let Some(check) = check {
                fb.switch_to_block(check);
                fb.seal_block(check);
                let nil = result_mode.constant(fb, false);
                fb.def_var(res, nil);
                let a_veclike = has_veclike_tag(fb, a);
                let b_veclike = has_veclike_tag(fb, b);
                let either_veclike = fb.ins().bor(a_veclike, b_veclike);
                fb.ins().brif(either_veclike, slow, &[], merge, &[]);
            }

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            if check.is_some() {
                fb.set_cold_block(slow);
            }
            let vmctx = fb.use_var(rt.vmctx_var);
            let eq_slow = rt.refs.get(fb.func, Shim::EqSlow);
            let call = fb.ins().call(eq_slow, &[vmctx, a, b]);
            let slow_res = fb.inst_results(call)[0];
            let slow_res = result_mode.successful_tagged(fb, slow_res);
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::Symbolp => {
            // Symbol tag -> t natively (nil/t are symbols). A value with any
            // other tag but veclike is nil natively; a veclike reaches the
            // read-only slow-path shim (cold), which answers t only for a
            // symbol-with-pos while `symbols-with-pos-enabled`.
            // `NEOVM_JIT_EQ_PREFILTER=off`: every non-symbol calls the shim.
            let rt = rt.ok_or(CompileError::UnsupportedOp("symbolp"))?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let res = fb.declare_var(result_mode.ty());
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let check = super::super::jit_eq_prefilter_on().then(|| fb.create_block());
            let tag = band_imm_p(fb, a, TAG_MASK as i64);
            let is_sym = icmp_imm_p(fb, IntCC::Equal, tag, TAG_SYMBOL as i64);
            fb.ins().brif(is_sym, fast, &[], check.unwrap_or(slow), &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = result_mode.constant(fb, true);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            if let Some(check) = check {
                fb.switch_to_block(check);
                fb.seal_block(check);
                let nil = result_mode.constant(fb, false);
                fb.def_var(res, nil);
                let is_veclike = icmp_imm_p(
                    fb,
                    IntCC::Equal,
                    tag,
                    crate::tagged::value::TAG_VECLIKE as i64,
                );
                fb.ins().brif(is_veclike, slow, &[], merge, &[]);
            }

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            if check.is_some() {
                fb.set_cold_block(slow);
            }
            let vmctx = fb.use_var(rt.vmctx_var);
            let symbolp_slow = rt.refs.get(fb.func, Shim::SymbolpSlow);
            let call = fb.ins().call(symbolp_slow, &[vmctx, a]);
            let slow_res = fb.inst_results(call)[0];
            let slow_res = result_mode.successful_tagged(fb, slow_res);
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
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
                super::super::active_numeric_feedback(pc),
                crate::emacs_core::jit::NumericFeedback::Float
            ) && !(reps[n - 1] == SlotRep::RawFixnum && reps[n - 2] == SlotRep::RawFixnum)
            {
                let statically_float =
                    reps[n - 2].is_static_float() || reps[n - 1].is_static_float();
                let res_var = fb.declare_var(result_mode.ty());
                let ff_b = fb.create_block();
                let slow_b = fb.create_block();
                let fix_b = (!statically_float).then(|| fb.create_block());
                let mix_b = fb.create_block();
                let merge = fb.create_block();
                let bool_constants = result_mode.shared_constants(fb);
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
                let r = result_mode.condition_with_constants(fb, cond, bool_constants);
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
                    let r = result_mode.condition_with_constants(fb, cond, bool_constants);
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
                let r = result_mode.condition_with_constants(fb, cond, bool_constants);
                fb.def_var(res_var, r);
                fb.ins().jump(merge, &[]);

                fb.switch_to_block(merge);
                fb.seal_block(merge);
                let out = fb.use_var(res_var);
                stack.truncate(n - 2);
                reps.truncate(n - 2);
                stack.push(out);
                reps.push(result_mode.rep());
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
            stack.push(result_mode.condition(fb, cond));
            reps.push(result_mode.rep());
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
            stack.push(match result_mode {
                BoolResultMode::TaggedLisp => lower_predicate(fb, kind, a),
                BoolResultMode::BoolFlag => lower_predicate_flag(fb, kind, a),
            });
        }
        Op::Integerp | Op::Numberp => {
            // Fixnum tag -> t natively; anything else (bignum/float/non-number)
            // through the context-free slow shim.
            let rt = rt.ok_or(CompileError::UnsupportedOp("predicate"))?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let shim = rt.refs.get(
                fb.func,
                if matches!(op, Op::Integerp) {
                    Shim::IntegerpSlow
                } else {
                    Shim::NumberpSlow
                },
            );
            let res = fb.declare_var(result_mode.ty());
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
            let t = result_mode.constant(fb, true);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let call = fb.ins().call(shim, &[a]);
            let slow_res = fb.inst_results(call)[0];
            let slow_res = result_mode.successful_tagged(fb, slow_res);
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        _ => return Err(CompileError::UnsupportedOp("opt-bool-result")),
    }
    Ok(())
}

fn lower_generic_arith_site_bool(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    op: &Op,
    stack: &mut Vec<ClifValue>,
    reps: &mut Vec<SlotRep>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    signal_exit: &mut Option<Block>,
    result_mode: BoolResultMode,
) -> Result<(), CompileError> {
    let kind = crate::emacs_core::bytecode::ArithGenericKind::from_op(op)
        .ok_or(CompileError::UnsupportedOp("arith-generic"))?;
    let nargs = kind.arity();
    if stack.len() < nargs {
        return Err(CompileError::StackUnderflow);
    }
    let at = stack.len() - nargs;
    if reps.contains(&SlotRep::Bool) {
        // The residual is observed only by representation-aware roots and
        // cold signal snapshots. Operands alone receive T/NIL views; float
        // alias boxing and raw retagging keep their existing whole-stack policy.
        for k in 0..stack.len() {
            match reps[k] {
                SlotRep::Bool if k >= at => {
                    stack[k] = tagged_bool_view(fb, stack[k]);
                    reps[k] = SlotRep::Tagged;
                }
                SlotRep::RawFixnum => stack_force_tagged(fb, stack, reps, k),
                SlotRep::Flonum { .. } => {
                    box_flonum_slot(fb, rt, stack, reps, k);
                }
                SlotRep::Tagged | SlotRep::Bool => {}
            }
        }
    } else {
        materialize_model_stack(fb, Some(rt), stack, reps);
    }
    let operands: Vec<ClifValue> = stack[at..].to_vec();
    stack.truncate(at);
    reps.truncate(at);

    let res_var = fb.declare_var(result_mode.ty());
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
            result_mode.condition(fb, cond)
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
        _ => unreachable!("ArithGenericKind::from_op admitted {op:?}"),
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
        emit_model_roots_pre(fb, rt, stack, reps)
    };
    // Operands in registers: `call_args_slot` is sized for the body's
    // CALL sites only, so it must not carry them.
    let vmctx = fb.use_var(rt.vmctx_var);
    // The discriminant is the shim's ABI (`ArithGenericKind` pins it).
    let kind_v = fb.ins().iconst(types::I64, kind as i64);
    let second = if nargs == 2 { operands[1] } else { operands[0] };
    let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
    let arith_generic = rt.refs.get(fb.func, Shim::ArithGeneric);
    let call = fb.ins().call(
        arith_generic,
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
    let slow = result_mode.successful_tagged(fb, slow);
    fb.def_var(res_var, slow);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    stack.push(fb.use_var(res_var));
    reps.push(result_mode.rep());
    Ok(())
}
