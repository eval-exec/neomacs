use super::*;

#[test]
fn dispatch_counts_and_plans_interpret() {
    // Threshold-aware so the test also holds under a NEOVM_JIT_THRESHOLD
    // override (e.g. the =1 every-function soak).
    let threshold = hot_threshold();
    let rt = Runtime::new();
    assert_eq!(rt.heat(), 0);
    assert!(!rt.is_hot());
    for i in 1..=5u32 {
        let plan = rt.dispatch();
        if i >= threshold {
            assert!(
                matches!(plan, Plan::Compiled),
                "hot at {i} (>= {threshold})"
            );
        } else {
            assert!(
                matches!(plan, Plan::Interpret),
                "cold at {i} (< {threshold})"
            );
        }
        assert_eq!(rt.heat(), i);
    }
    assert_eq!(rt.is_hot(), 5 >= threshold);
}

#[test]
fn becomes_hot_at_threshold() {
    let rt = Runtime::new();
    for _ in 0..Runtime::HOT_THRESHOLD {
        let _ = rt.dispatch();
    }
    assert!(rt.is_hot());
}

#[test]
fn heat_saturates_without_wrapping() {
    let rt = Runtime::new();
    // Seed near the ceiling, then bump past it; must clamp, not wrap to cold.
    for _ in 0..3 {
        rt.heat
            .store(u32::MAX - 1, std::sync::atomic::Ordering::Relaxed);
        let _ = rt.dispatch();
        assert_eq!(rt.heat(), u32::MAX);
        let _ = rt.dispatch();
        assert_eq!(rt.heat(), u32::MAX);
    }
}

#[test]
fn loop_work_tiers_up_a_rarely_called_body() {
    // A body CALLED only a few times (dispatch < threshold) but running a
    // hot INNER LOOP must still tier up: note_loop_work credits the loop.
    let credit = loop_heat_per_wrap();
    if credit == 0 || !jit_runtime_enabled() {
        return; // loop heat disabled in this env — nothing to assert
    }
    let threshold = hot_threshold();
    if threshold <= 5 {
        // NEOVM_JIT_THRESHOLD=1 (the every-function soak) makes the
        // "5 calls stay cold" premise false by construction — skip,
        // like the loop-heat/kill-switch env guards above.
        return;
    }
    let rt = Runtime::new();
    // Called a handful of times: nowhere near hot on call count alone.
    for _ in 0..5 {
        assert!(matches!(rt.dispatch(), Plan::Interpret));
    }
    let after_calls = rt.heat();
    assert!(!rt.is_hot(), "5 calls must be cold (threshold {threshold})");
    // Now credit loop work (each wrap = 256 iterations); cross the threshold.
    let wraps_needed = threshold.div_ceil(credit) + 1;
    for _ in 0..wraps_needed {
        rt.note_loop_work();
    }
    assert!(rt.is_hot(), "hot after {wraps_needed} loop wraps");
    assert!(matches!(rt.dispatch(), Plan::Compiled));
    assert!(rt.heat() > after_calls);
}

#[test]
fn loop_work_saturates_without_wrapping() {
    if loop_heat_per_wrap() == 0 || !jit_runtime_enabled() {
        return;
    }
    let rt = Runtime::new();
    rt.heat
        .store(u32::MAX - 1, std::sync::atomic::Ordering::Relaxed);
    rt.note_loop_work();
    assert_eq!(rt.heat(), u32::MAX);
    rt.note_loop_work();
    assert_eq!(rt.heat(), u32::MAX);
}

/// Size-scaled tier-up budget: bodies up to one `size_unit()` of ops tier
/// at the flat threshold; a body of `k` units needs `k` times as many calls.
#[test]
fn dispatch_sized_scales_threshold_by_body_size() {
    let threshold = hot_threshold();
    let unit = size_unit() as usize;
    if unit == 0 {
        return; // scaling disabled by NEOVM_JIT_SIZE_UNIT=0
    }
    // Small body: flat threshold.
    let small = Runtime::new();
    for _ in 0..threshold.saturating_sub(1) {
        assert!(matches!(small.dispatch_sized(unit), Plan::Interpret));
    }
    assert!(matches!(small.dispatch_sized(unit), Plan::Compiled));
    // Three-unit body: three times the calls, and the extra calls between
    // the flat and the scaled threshold still interpret.
    let big = Runtime::new();
    let scaled = threshold.saturating_mul(3);
    for _ in 0..scaled.saturating_sub(1) {
        assert!(matches!(big.dispatch_sized(3 * unit), Plan::Interpret));
    }
    assert!(matches!(big.dispatch_sized(3 * unit), Plan::Compiled));
    assert_eq!(big.heat(), scaled);
    // Above the size cap: never tiers, however hot.
    let cap = max_tier_ops() as usize;
    if cap != 0 {
        let huge = Runtime::new();
        huge.set_hot_for_test();
        let cap_calls = threshold.saturating_mul((cap / unit) as u32);
        for _ in 0..cap_calls {
            assert!(matches!(huge.dispatch_sized(cap + 1), Plan::Interpret));
        }
        assert!(matches!(huge.dispatch_sized(cap), Plan::Compiled));
    }
}

/// A `NotCompilable` verdict is remembered on the runtime: a hot body the
/// JIT rejected answers `Interpret` without a cache probe, until the cache
/// forgets its verdicts (`clear`, a heap change) or a grown `make-closure`
/// prefix evicts the id — then it re-consults exactly as before.
#[cfg(feature = "jit")]
#[test]
fn rejected_body_interprets_without_a_cache_probe_until_the_cache_clears() {
    let rt = Runtime::new();
    rt.set_hot_for_test();
    assert!(matches!(rt.dispatch(), Plan::Compiled));
    assert!(matches!(rt.dispatch_sized(1), Plan::Compiled));
    rt.mark_native_rejected(super::cache::rejection_epoch());
    assert!(matches!(rt.dispatch(), Plan::Interpret));
    assert!(matches!(rt.dispatch_sized(1), Plan::Interpret));
    // Heat still accrues (OSR's `is_hot` is unaffected by the verdict).
    assert!(rt.is_hot());
    // The cache forgets: so must we.
    super::cache::clear();
    assert!(matches!(rt.dispatch(), Plan::Compiled));
    assert!(matches!(rt.dispatch_sized(1), Plan::Compiled));
    // A grown patched prefix evicts the leaf and the verdict with it.
    rt.mark_native_rejected(super::cache::rejection_epoch());
    assert!(matches!(rt.dispatch(), Plan::Interpret));
    let _ = rt.compiled_id_or_assign();
    assert!(rt.note_patched_prefix(1).is_some());
    assert!(matches!(rt.dispatch(), Plan::Compiled));
    // A prewarmed leaf is never rejected; the prewarm wins.
    rt.mark_native_rejected(super::cache::rejection_epoch());
    rt.mark_aot_prewarmed();
    assert!(matches!(rt.dispatch(), Plan::Compiled));
}

/// A profitability deferral holds the dispatcher at `Interpret` (no cache
/// probe) until the deferral heat, then reports itself expired so the
/// next compile bypasses the gate; `0` (the veto) never defers.
#[test]
fn deferred_body_interprets_until_the_deferral_heat() {
    let rt = Runtime::new();
    rt.set_hot_for_test();
    assert!(matches!(rt.dispatch(), Plan::Compiled));
    let at = hot_threshold().saturating_mul(4);
    rt.defer_tier_up(at);
    assert!(!rt.profit_deferral_expired());
    assert!(matches!(rt.dispatch(), Plan::Interpret));
    assert!(matches!(rt.dispatch_sized(1), Plan::Interpret));
    rt.set_heat_for_test(at.saturating_sub(1));
    assert!(
        matches!(rt.dispatch(), Plan::Compiled),
        "the bump reaches the deferral heat"
    );
    assert!(rt.profit_deferral_expired());
    assert!(matches!(rt.dispatch_sized(1), Plan::Compiled));
}

/// Cite-and-overturn of the former `clone_starts_cold` pin: a clone (what
/// `make-closure` produces per instantiation) SHARES the source's tiering
/// state — heat accumulated through any instance is the source's heat, and
/// the compiled id is one per source (see the `Runtime` docs).
#[test]
fn clone_shares_heat_and_compiled_id() {
    let rt = Runtime::new();
    for _ in 0..100 {
        let _ = rt.dispatch();
    }
    assert_eq!(rt.heat(), 100);
    let instance = rt.clone();
    assert_eq!(
        instance.heat(),
        100,
        "an instance inherits the source's heat"
    );
    for _ in 0..10 {
        let _ = instance.dispatch();
    }
    assert_eq!(rt.heat(), 110, "an instance's calls heat the source");
    let id = instance.compiled_id_or_assign();
    assert_eq!(rt.compiled_id(), Some(id), "one compiled id per source");
    // A fresh Runtime (a NEW source) is still cold and unshared.
    assert_eq!(Runtime::new().heat(), 0);
}

/// `make-closure` widening: the patched prefix is monotone, shared, and
/// reports the compiled id to evict only when it GROWS after a compile.
#[test]
fn note_patched_prefix_is_monotone_and_reports_stale_leaf() {
    let rt = Runtime::new();
    assert_eq!(rt.patched_prefix(), 0);
    // No compiled id yet: widening records but has nothing to evict.
    assert_eq!(rt.note_patched_prefix(2), None);
    assert_eq!(rt.patched_prefix(), 2);
    let instance = rt.clone();
    assert_eq!(instance.patched_prefix(), 2, "the record is shared");
    let id = rt.compiled_id_or_assign();
    // Same or narrower width: nothing changed, no eviction.
    assert_eq!(instance.note_patched_prefix(2), None);
    assert_eq!(instance.note_patched_prefix(1), None);
    assert_eq!(rt.patched_prefix(), 2);
    // Wider after a compile: the leaf assumed the narrower prefix.
    assert_eq!(instance.note_patched_prefix(3), Some(id));
    assert_eq!(rt.patched_prefix(), 3);
}

#[test]
fn call_feedback_packs_and_unpacks() {
    for fb in [
        CallFeedback::Uninit,
        CallFeedback::Monomorphic(SymId(0)),
        CallFeedback::Monomorphic(SymId(1)),
        CallFeedback::Monomorphic(SymId(u32::MAX)),
        CallFeedback::Megamorphic,
    ] {
        assert_eq!(CallFeedback::unpack(fb.pack()), fb);
    }
    // Uninit and Monomorphic(0) must be distinct despite the zero SymId.
    assert_ne!(
        CallFeedback::Uninit.pack(),
        CallFeedback::Monomorphic(SymId(0)).pack()
    );
}

#[test]
fn feedback_lattice_uninit_mono_mega() {
    let rt = Runtime::new();
    let ops_len = 8;
    let pc = 3;
    assert_eq!(rt.call_feedback(pc), CallFeedback::Uninit);

    // First observation -> Monomorphic.
    rt.record_call(pc, ops_len, SymId(42));
    assert_eq!(rt.call_feedback(pc), CallFeedback::Monomorphic(SymId(42)));

    // Same target -> still Monomorphic.
    rt.record_call(pc, ops_len, SymId(42));
    assert_eq!(rt.call_feedback(pc), CallFeedback::Monomorphic(SymId(42)));

    // Different target -> Megamorphic, and it sticks.
    rt.record_call(pc, ops_len, SymId(7));
    assert_eq!(rt.call_feedback(pc), CallFeedback::Megamorphic);
    rt.record_call(pc, ops_len, SymId(7));
    assert_eq!(rt.call_feedback(pc), CallFeedback::Megamorphic);
}

#[test]
fn feedback_is_per_site() {
    let rt = Runtime::new();
    rt.record_call(1, 8, SymId(10));
    rt.record_call(5, 8, SymId(20));
    assert_eq!(rt.call_feedback(0), CallFeedback::Uninit);
    assert_eq!(rt.call_feedback(1), CallFeedback::Monomorphic(SymId(10)));
    assert_eq!(rt.call_feedback(5), CallFeedback::Monomorphic(SymId(20)));
    // Out-of-range pc is Uninit, never a panic.
    assert_eq!(rt.call_feedback(99), CallFeedback::Uninit);
}

#[test]
fn out_of_range_record_is_ignored() {
    let rt = Runtime::new();
    rt.record_call(100, 8, SymId(1)); // pc >= ops_len: no-op, no panic
    assert_eq!(rt.call_feedback(100), CallFeedback::Uninit);
}

/// Cite-and-overturn of the former `clone_clears_feedback` pin: call-site
/// feedback is a property of the source's code, so instances share it.
#[test]
fn clone_shares_feedback() {
    let rt = Runtime::new();
    rt.record_call(2, 8, SymId(5));
    assert_eq!(rt.call_feedback(2), CallFeedback::Monomorphic(SymId(5)));
    let instance = rt.clone();
    assert_eq!(
        instance.call_feedback(2),
        CallFeedback::Monomorphic(SymId(5))
    );
    instance.record_call(2, 8, SymId(6));
    assert_eq!(
        rt.call_feedback(2),
        CallFeedback::Megamorphic,
        "recorded through the instance"
    );
}

/// `record_numeric` joins up the `FixnumOnly -> Float -> Other` lattice and
/// reports whether the slot moved; `FixnumOnly` never downgrades a slot.
#[test]
fn record_numeric_reports_change() {
    let rt = Runtime::new();
    assert!(!rt.record_numeric(2, 8, NumericFeedback::FixnumOnly));
    assert_eq!(rt.numeric_feedback(2), NumericFeedback::FixnumOnly);
    assert!(rt.record_numeric(2, 8, NumericFeedback::Float));
    assert!(
        !rt.record_numeric(2, 8, NumericFeedback::Float),
        "already Float"
    );
    assert!(
        !rt.record_numeric(2, 8, NumericFeedback::FixnumOnly),
        "FixnumOnly never downgrades"
    );
    assert_eq!(rt.numeric_feedback(2), NumericFeedback::Float);
    assert!(rt.record_numeric(2, 8, NumericFeedback::Other));
    assert!(
        !rt.record_numeric(2, 8, NumericFeedback::Float),
        "Other is sticky"
    );
    assert_eq!(rt.numeric_feedback(2), NumericFeedback::Other);
    assert!(
        !rt.record_numeric(99, 8, NumericFeedback::Other),
        "out of range"
    );
}

/// The operand classification the interpreter's slow arm and the deopt
/// classifier share.
#[test]
fn numeric_feedback_of_operands_classifies() {
    use crate::emacs_core::value::Value;
    let fix = Value::make_int(3);
    let flo = Value::make_float(1.5);
    assert_eq!(
        NumericFeedback::of_operands(&[fix, fix]),
        NumericFeedback::FixnumOnly
    );
    assert_eq!(
        NumericFeedback::of_operands(&[]),
        NumericFeedback::FixnumOnly
    );
    assert_eq!(
        NumericFeedback::of_operands(&[fix, flo]),
        NumericFeedback::Float
    );
    assert_eq!(
        NumericFeedback::of_operands(&[flo, flo]),
        NumericFeedback::Float
    );
    assert_eq!(
        NumericFeedback::of_operands(&[flo, Value::NIL]),
        NumericFeedback::Other
    );
    assert_eq!(
        NumericFeedback::of_operands(&[Value::string("x")]),
        NumericFeedback::Other
    );
}

/// The backoff ladder: each invalidation takes at least its cause's floor;
/// past `max_reopts` each one climbs one level; the level never falls, and
/// `Interpreter` is reached after at most `max_reopts + 4` invalidations.
#[test]
fn note_reopt_is_monotone_and_bounded() {
    let rt = Runtime::new();
    assert_eq!(rt.reopt_level(), ReoptLevel::Speculative);
    assert_eq!(rt.reopt_count(), 0);
    // Within the budget the level only follows the floor.
    for i in 1..=4u8 {
        assert_eq!(
            rt.note_reopt(ReoptLevel::Speculative, 4),
            ReoptLevel::Speculative
        );
        assert_eq!(rt.reopt_count(), i);
    }
    // A cause's floor applies at once, and never lowers the level.
    let rt2 = Runtime::new();
    assert_eq!(
        rt2.note_reopt(ReoptLevel::BaselineOnly, 4),
        ReoptLevel::BaselineOnly
    );
    assert_eq!(
        rt2.note_reopt(ReoptLevel::Speculative, 4),
        ReoptLevel::BaselineOnly
    );
    // Past the budget each invalidation climbs one level.
    let want = [
        ReoptLevel::NoInline,
        ReoptLevel::BaselineOnly,
        ReoptLevel::Generic,
        ReoptLevel::Interpreter,
        ReoptLevel::Interpreter,
    ];
    for level in want {
        assert_eq!(rt.note_reopt(ReoptLevel::Speculative, 4), level);
        assert_eq!(rt.reopt_level(), level);
    }
    assert_eq!(rt.reopt_count(), 9);
    // A clone (a `make-closure` instance) shares the level.
    assert_eq!(rt.clone().reopt_level(), ReoptLevel::Interpreter);
    // The count saturates.
    let rt3 = Runtime::new();
    for _ in 0..300 {
        rt3.note_reopt(ReoptLevel::Speculative, u8::MAX);
    }
    assert_eq!(rt3.reopt_count(), u8::MAX);
    assert_eq!(ReoptLevel::Interpreter.next(), ReoptLevel::Interpreter);
    assert_eq!(ReoptLevel::from_u8(200), ReoptLevel::Interpreter);
}

/// The per-pc no-inline bits: set per call site, allocated on first use,
/// out-of-range pcs ignored.
#[test]
fn call_site_no_inline_bitset() {
    let rt = Runtime::new();
    assert!(!rt.call_site_no_inline(3), "nothing allocated yet");
    rt.mark_call_site_no_inline(3, 130);
    rt.mark_call_site_no_inline(64, 130);
    rt.mark_call_site_no_inline(129, 130);
    rt.mark_call_site_no_inline(500, 130); // out of range: ignored
    for pc in 0..140 {
        assert_eq!(
            rt.call_site_no_inline(pc),
            matches!(pc, 3 | 64 | 129),
            "pc {pc}"
        );
    }
    assert!(!rt.call_site_no_inline(500));
}

/// The small reopt setters: reopening numeric feedback, the leaf-slot
/// disarm and the AOT-prewarm clear.
#[test]
fn reopt_setters_reopen_disarm_and_clear() {
    let rt = Runtime::new();
    rt.note_numeric_feedback_consumed();
    assert!(!rt.wants_numeric_feedback());
    rt.reopen_numeric_feedback();
    assert!(rt.wants_numeric_feedback());
    assert!(rt.widen_numeric(1, 4, NumericFeedback::Float));
    assert!(!rt.widen_numeric(1, 4, NumericFeedback::Float));
    assert_eq!(rt.numeric_feedback(1), NumericFeedback::Float);
    rt.mark_aot_prewarmed();
    rt.set_hot_for_test();
    assert!(matches!(rt.dispatch(), Plan::Compiled));
    rt.clear_aot_prewarmed();
    rt.defer_tier_up(u32::MAX);
    assert!(
        matches!(rt.dispatch(), Plan::Interpret),
        "no longer prewarmed: the deferral holds"
    );
}
