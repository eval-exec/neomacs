//! Selected normal-T1 array profiling; all main operation arms stay exact.
//! Threading: builder state and scalar frontend requirements belong to one
//! compilation. The observer retains no Lisp/runtime/TLS state, and has no
//! allocation, safepoint or error status. Operand preparation runs once.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_simple_op_with_array_profile(
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
    lower_simple_op_with_cons_proof_array_profile(
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
        super::super::heap_inline::ConsStoreProof::Dynamic,
    )
}

#[allow(clippy::too_many_arguments)]
fn lower_simple_op_with_cons_proof_array_profile(
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
    // Main's Aref arm emits nothing before operand collection/truncation.
    // Observe the same prepared tagged operand here, before its first semantic
    // emission. The unchanged arms keep error order, roots and all fast paths.
    if matches!(op, Op::Aref) && stack.len() >= 2 {
        if let Some(rt) = rt {
            super::super::array_profile::emit(fb, rt, aot, pc, stack[stack.len() - 2]);
        }
    }
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
        cons_proof,
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
