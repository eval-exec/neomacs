//! Selected Sink deopt reconstruction; main's ordinary emitter is unchanged.
//!
//! Threading: all pending sites, SSA names and frozen recipe snapshots belong
//! to one compilation. At execution only the original tagged spill and stable
//! mutator-owned leaf cells are written. No Lisp cache, runtime layout, TLS
//! ownership state or AOT import is added. Existing no-GC materializers recover
//! all frame aliases before any spill; the runtime's return/readback interval
//! remains unchanged. Main physical chain frames never enter a Sink snapshot.

use super::*;

/// Opt-only caller seam. All-None batches invoke main's exact emitter, including
/// its declaration ids, cold tail layout, inline cause/chain stores and ABI.
/// Legacy/MIR/AOT/OSR-entry callers continue calling that emitter directly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_pending_deopts_with_sink(
    fb: &mut FunctionBuilder,
    refs: DeoptRefs,
    pending: &mut Vec<PendingDeopt>,
    float_boxer: Option<&RtRefs>,
    abi: super::super::LeafAbi,
) -> Result<(), CompileError> {
    if pending.iter().all(|site| site.sink_cold.is_none()) {
        emit_pending_deopts(fb, refs, pending, float_boxer, abi);
        return Ok(());
    }
    if jit_opt_mode() != OptMode::Opt || !jit_opt_passes().sink {
        return Err(CompileError::UnsupportedOp("opt-sink:cold-pass-off"));
    }
    if matches!(refs, DeoptRefs::Sidecar { .. }) {
        return Err(CompileError::UnsupportedOp("opt-sink:cold-aot"));
    }
    if float_boxer.is_none() {
        return Err(CompileError::UnsupportedOp("opt-sink:cold-runtime"));
    }
    // Main may attach an InlineDeoptWrite before the Opt snapshot override
    // clears region. Reject that independently; never drop main's physical
    // metadata or combine it with a single-frame virtual reconstruction.
    for site in pending.iter() {
        if let Some(sink) = &site.sink_cold {
            if site.region.is_some()
                || site.inline.is_some()
                || !site.cons_rebuilds.is_empty()
                || !sink.matches_frame(&site.stack, &site.reps)
            {
                return Err(CompileError::UnsupportedOp("opt-sink:cold-pending-frame"));
            }
        }
    }
    emit_selected_sink_deopts(fb, refs, pending, float_boxer, abi)
}

fn emit_selected_sink_deopts(
    fb: &mut FunctionBuilder,
    refs: DeoptRefs,
    pending: &mut Vec<PendingDeopt>,
    float_boxer: Option<&RtRefs>,
    abi: super::super::LeafAbi,
) -> Result<(), CompileError> {
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
    let cold = super::super::cold_exits::ColdSpan::begin(fb);
    // `NEOVM_JIT_COLD_EXITS=share`: the cell stores and the return live in
    // one tail per function. Created here, between filled blocks.
    let tail = if pending.is_empty() {
        None
    } else {
        super::super::cold_exits::shared_deopt_tail(fb, |fb| deopt_tail(fb, refs, abi))
    };
    for pd in pending.drain(..) {
        fb.switch_to_block(pd.block);
        fb.seal_block(pd.block);
        super::super::cold_exits::mark_exit_cold(
            fb,
            pd.block,
            super::super::cold_exits::ColdExit::Deopt,
        );
        if let Some(write) = &pd.inline {
            write.emit(fb);
        }
        // Materialize the four bases. For Baked, the iconsts live in THIS cold
        // block (the original JIT placement); for Sidecar they are entry values.
        // A site that jumps to the shared tail needs only the spill base.
        let (spill_base, metas) = match refs {
            DeoptRefs::Baked {
                spill_base,
                meta_pc,
                meta_depth,
                meta_handlers,
            } => (
                fb.ins().iconst(types::I64, spill_base),
                tail.is_none().then(|| {
                    (
                        fb.ins().iconst(types::I64, meta_pc),
                        fb.ins().iconst(types::I64, meta_depth),
                        fb.ins().iconst(types::I64, meta_handlers),
                    )
                }),
            ),
            DeoptRefs::Sidecar {
                spill_base,
                meta_pc,
                meta_depth,
                meta_handlers,
            } => (spill_base, Some((meta_pc, meta_depth, meta_handlers))),
        };
        // Inside an inlined region the interpreter must resume at the CALL,
        // with the stack the caller had before it — the region's own operand
        // stack means nothing to it.
        let (pc, stack, reps) = match &pd.region {
            Some(region) if region.chain.is_some() => {
                (pd.pc, pd.stack.as_slice(), pd.reps.as_slice())
            }
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
        let mut reps = std::borrow::Cow::Borrowed(reps);
        if let Some(sink) = &pd.sink_cold {
            // Entry-point validation rejects chain/region/MIR/AOT mixtures
            // before emission. Reconstruct the whole exact tagged frame before
            // its first spill, preserving main's no-GC return/readback interval.
            let runtime = float_boxer.expect("selected sink was checked before emission");
            fb.set_cold_block(pd.block);
            let tagged = super::super::sink_cold_snapshot::emit_cold_snapshot(fb, runtime, sink)?;
            reps = std::borrow::Cow::Owned(vec![SlotRep::Tagged; tagged.len()]);
            stack = std::borrow::Cow::Owned(tagged);
        }
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
        if let Some(tail) = tail {
            let pc_v = fb.ins().iconst(types::I64, pc as i64);
            let depth_v = fb.ins().iconst(types::I64, stack.len() as i64);
            let h_v = fb.ins().iconst(types::I64, pd.handlers_len as i64);
            fb.ins().jump(
                tail,
                &[
                    BlockArg::Value(pc_v),
                    BlockArg::Value(depth_v),
                    BlockArg::Value(h_v),
                ],
            );
            continue;
        }
        let (meta_pc, meta_depth, meta_handlers) =
            metas.expect("a site without the shared tail materializes the cells");
        let pc_v = fb.ins().iconst(types::I64, pc as i64);
        fb.ins().store(MemFlagsData::trusted(), pc_v, meta_pc, 0);
        let depth_v = fb.ins().iconst(types::I64, stack.len() as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), depth_v, meta_depth, 0);
        let h_v = fb.ins().iconst(types::I64, pd.handlers_len as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), h_v, meta_handlers, 0);
        super::super::reg_abi::emit_leaf_return(fb, abi, None, None, STATUS_DEOPT_AT);
    }
    cold.end(fb, None);
    Ok(())
}
