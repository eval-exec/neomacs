use super::*;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodePolicyExt;

fn sample_mixed_backtrace_frames(depth: usize) -> Vec<Option<String>> {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let base = ctx.specpdl.len();
    ctx.push_backtrace_frame(Value::symbol("profiler-general"), &[]);
    ctx.push_specpdl_root(Value::NIL);
    ctx.push_backtrace_frame(Value::symbol("profiler-one"), &[Value::fixnum(1)]);
    ctx.push_specpdl_root(Value::fixnum(2));
    ctx.push_backtrace_frame(
        Value::symbol("profiler-two"),
        &[Value::fixnum(3), Value::fixnum(4)],
    );
    let native_args =
        [Value::fixnum(5), Value::fixnum(6), Value::fixnum(7)].map(|value| value.bits() as i64);
    // SAFETY: this array contains valid tagged words and stays live until
    // unbind_to below removes the native frame. No pointer escapes the test.
    unsafe {
        ctx.push_backtrace_frame_from_native_args(
            Value::symbol("profiler-native"),
            native_args.as_ptr(),
            native_args.len(),
        );
    }
    assert!(matches!(
        ctx.specpdl.get(base),
        Some(SpecBinding::Backtrace { .. })
    ));
    assert!(matches!(
        ctx.specpdl.get(base + 2),
        Some(SpecBinding::Backtrace1 { .. })
    ));
    assert!(matches!(
        ctx.specpdl.get(base + 4),
        Some(SpecBinding::Backtrace2 { .. })
    ));
    assert!(matches!(
        ctx.specpdl.get(base + 5),
        Some(SpecBinding::BacktraceNative { .. })
    ));

    // CPU and memory profiling share this collector. A heap allocation gives
    // it a deterministic sample weight without clocks or a watchdog thread.
    ctx.profiler.memory_log = Some(ProfilerLog::new(depth, 10));
    ctx.profiler.memory_running = true;
    let before = ctx.tagged_heap.total_allocated_bytes();
    ctx.profiler.memory_last_allocated = before;
    let witness = ctx
        .tagged_heap
        .alloc_string(neovm_value::heap_types::LispString::from_utf8(
            "profiler allocation witness",
        ));
    ctx.push_specpdl_root(witness);
    let allocated = ctx.tagged_heap.total_allocated_bytes() - before;
    ctx.profiler_poll_sample();
    ctx.profiler.memory_running = false;
    ctx.unbind_to(base);
    std::hint::black_box(&native_args);

    assert!(allocated > 0);
    let log = ctx.profiler.memory_log.as_ref().unwrap();
    assert_eq!(log.entries.len(), 1);
    let sample = log.entries.values().next().unwrap();
    assert_eq!(sample.count, allocated);
    sample
        .frames
        .iter()
        .map(|frame| (!frame.is_nil()).then(|| frame_label(*frame)))
        .collect()
}

#[test]
fn profiler_poll_sample_preserves_all_backtrace_frame_kinds() {
    assert_eq!(
        sample_mixed_backtrace_frames(6),
        vec![
            Some("profiler-native".to_owned()),
            Some("profiler-two".to_owned()),
            Some("profiler-one".to_owned()),
            Some("profiler-general".to_owned()),
            None,
            None,
        ]
    );
}

#[test]
fn profiler_poll_sample_applies_depth_after_skipping_non_frame_bindings() {
    assert_eq!(
        sample_mixed_backtrace_frames(2),
        vec![
            Some("profiler-native".to_owned()),
            Some("profiler-two".to_owned())
        ]
    );
}

#[test]
fn cpu_folded_renders_recorded_stacks_with_names_and_sorts_by_count() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    // Not running, so the profiler_poll() inside profiler_cpu_folded is a
    // no-op and the log holds exactly what we record.
    assert!(!ctx.profiler.is_active());
    ctx.profiler.cpu_log = Some(ProfilerLog::new(16, 100));
    {
        let log = ctx.profiler.cpu_log.as_mut().unwrap();
        // Innermost-first, matching profiler_poll's rev() collection.
        log.record(&[Value::symbol("neo-leaf"), Value::symbol("neo-root")], 7);
        log.record(&[Value::symbol("neo-solo")], 3);
    }
    let folded = ctx.profiler_cpu_folded();
    // Root-first Brendan-Gregg order after the reverse; highest count first.
    assert!(
        folded.starts_with("neo-root;neo-leaf 7"),
        "folded was:\n{folded}"
    );
    assert!(folded.contains("neo-solo 3"), "folded was:\n{folded}");
}

#[test]
fn diagnostics_capture_resets_stale_log_and_does_not_hijack() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    assert!(!ctx.profiler.is_active());

    // Stale samples from a hypothetical prior capture.
    ctx.profiler.cpu_log = Some(ProfilerLog::new(16, 100));
    ctx.profiler
        .cpu_log
        .as_mut()
        .unwrap()
        .record(&[Value::symbol("stale-sym")], 99);

    // A fresh capture resets the log, so the stale sample cannot survive.
    assert!(ctx.diagnostics_cpu_profile_start(1_000_000));
    assert!(ctx.profiler_cpu_running());
    let folded = ctx.diagnostics_cpu_profile_stop_fold();
    assert!(
        !folded.contains("stale-sym"),
        "stale sample leaked: {folded}"
    );
    // Stopping clears the log so the next capture starts clean.
    assert!(!ctx.profiler_cpu_running());

    // A diagnostics capture reclaims its OWN orphaned session (a prior
    // diagnostics start whose stop never ran) instead of wedging.
    assert!(ctx.diagnostics_cpu_profile_start(1_000_000));
    assert!(
        ctx.diagnostics_cpu_profile_start(1_000_000),
        "should reclaim an orphaned diagnostics session"
    );
    ctx.diagnostics_cpu_profile_abort();
    assert!(!ctx.profiler_cpu_running());

    // But it must NOT hijack an interactive (non-diagnostics) session.
    assert!(ctx.profiler_cpu_start(1_000_000));
    assert!(
        !ctx.diagnostics_cpu_profile_start(1_000_000),
        "must not hijack an interactive profiler-start session"
    );
    assert!(ctx.profiler_cpu_running());
    ctx.profiler_cpu_stop();
}

#[test]
fn bounded_log_evicts_cold_samples_and_reports_discarded_weight() {
    let mut log = ProfilerLog::new(2, 2);
    log.record(&[Value::symbol("hot")], 10);
    log.record(&[Value::symbol("cold")], 1);
    log.record(&[Value::symbol("new")], 2);

    assert_eq!(log.entries.len(), 2);
    assert_eq!(log.discarded, 1);
}

#[test]
fn same_source_interpreted_closure_instances_do_not_merge_like_gnu_function_equal() {
    // GNU 31: two interpreted-function instances of the same lambda are NOT
    // `function-equal` (they merge EQ-only). Only compiled closures merge by
    // shared bytecode -- see the compiled-closure test below.
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let closures = ctx
        .eval_str(
            "(let ((make-closure (lambda () (lambda (value) value))))
               (list (funcall make-closure) (funcall make-closure)))",
        )
        .unwrap();
    let first = closures.cons_car();
    let second = closures.cons_cdr().cons_car();
    assert_ne!(first.bits(), second.bits());
    assert!(!first.function_equal(second));

    let mut log = ProfilerLog::new(1, 10);
    log.record(&[first], 2);
    log.record(&[second], 3);
    assert_eq!(log.entries.len(), 2);
}

#[test]
fn same_source_compiled_closure_instances_merge_like_gnu_function_equal() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let closures = ctx
        .eval_str(
            r#"(let* ((prototype (make-byte-code 0 "\300\207" [nil] 1))
                      (first (make-closure prototype 1))
                      (second (make-closure prototype 2)))
                 (list first second))"#,
        )
        .unwrap();
    let first = closures.cons_car();
    let second = closures.cons_cdr().cons_car();
    assert_ne!(first.bits(), second.bits());
    assert!(first.function_equal(second));

    let mut log = ProfilerLog::new(1, 10);
    log.record(&[first], 2);
    log.record(&[second], 3);
    assert_eq!(log.entries.len(), 1);
    assert_eq!(log.entries.values().next().unwrap().count, 5);
}

#[test]
fn structurally_equal_export_keys_preserve_all_counts() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let first = ctx.eval_str("(lambda (value) value)").unwrap();
    let second = ctx.eval_str("(lambda (value) value)").unwrap();
    assert_ne!(first.bits(), second.bits());
    assert!(!first.function_equal(second));

    let mut log = ProfilerLog::new(1, 10);
    log.record(&[first], 2);
    log.record(&[second], 3);
    assert_eq!(log.entries.len(), 2);

    let table = log.to_value();
    let table = table.as_hash_table().unwrap();
    assert_eq!(table.data.len(), 1);
    assert_eq!(table.data.values().next().unwrap().as_fixnum(), Some(5));
}

#[test]
fn zero_capacity_discards_samples_without_panicking() {
    let mut log = ProfilerLog::new(0, 0);
    log.record(&[Value::symbol("ignored")], 7);
    assert!(log.entries.is_empty());
    assert_eq!(log.discarded, 7);
}

#[test]
fn automatic_gc_samples_use_the_gnu_special_bucket() {
    let _harness = crate::test_utils::unit_harness();
    let mut log = ProfilerLog::new(4, 10);
    log.record_gc(9);
    let table = log.to_value();
    let table = table.as_hash_table().unwrap();
    let key = *table.key_snapshots().next().unwrap();
    let frames = key.as_vector_data().unwrap();
    assert_eq!(frames.len(), 2);
    assert!(frames[0].is_bare_symbol_named("Automatic GC"));
    assert_eq!(table.data.values().next().unwrap().as_fixnum(), Some(9));
}

#[test]
fn profiler_frames_remain_gc_roots_after_the_call_returns() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let closure = ctx.eval_str("(lambda (value) value)").unwrap();
    let mut log = ProfilerLog::new(1, 10);
    log.record(&[closure], 1);
    ctx.profiler.memory_log = Some(log);

    ctx.gc_collect_exact();

    let table = ctx.profiler_memory_log().unwrap();
    let key = *table
        .as_hash_table()
        .unwrap()
        .key_snapshots()
        .next()
        .unwrap();
    let frame = key.as_vector_data().unwrap()[0];
    assert!(frame.is_lambda());
}

/// The watchdog's tick is `AsyncSource::ProfilerTick` in the evaluator's
/// asynchronous-attention word: a raised tick sends the next safe point to
/// its slow path, which consumes it (one poll) and leaves the fast path clear;
/// disarming the watchdog lowers a tick nobody consumed.
#[test]
fn profiler_tick_bit_round_trip() {
    use crate::emacs_core::eval::{ASYNC_ATTENTION, AsyncSource};
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    assert!(ctx.maybe_quit_hot_ok());
    ASYNC_ATTENTION.raise(AsyncSource::ProfilerTick);
    assert!(
        !ctx.maybe_quit_hot_ok(),
        "a due tick is noticed at the next safe point"
    );
    ctx.maybe_quit().expect("a tick alone is not a quit");
    assert!(
        !ASYNC_ATTENTION.is_raised(AsyncSource::ProfilerTick),
        "the safe point consumed the tick"
    );
    assert!(ctx.maybe_quit_hot_ok());

    ASYNC_ATTENTION.raise(AsyncSource::ProfilerTick);
    disarm_profiler_sample_timer();
    assert!(
        !ASYNC_ATTENTION.is_raised(AsyncSource::ProfilerTick),
        "disarming drops an unconsumed tick"
    );
    assert!(ctx.maybe_quit_hot_ok());
}
