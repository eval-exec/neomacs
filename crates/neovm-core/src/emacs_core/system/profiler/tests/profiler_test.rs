use super::*;

#[test]
fn cpu_folded_renders_recorded_stacks_with_names_and_sorts_by_count() {
    let mut ctx = Context::new();
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
    let mut ctx = Context::new();
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
    let mut ctx = Context::new();
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
    let mut ctx = Context::new();
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
    let mut ctx = Context::new();
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
    let mut log = ProfilerLog::new(4, 10);
    log.record_gc(9);
    let table = log.to_value();
    let table = table.as_hash_table().unwrap();
    let key = *table.key_snapshots().next().unwrap();
    let frames = key.as_vector_data().unwrap();
    assert_eq!(frames.len(), 2);
    assert!(frames[0].is_symbol_named("Automatic GC"));
    assert_eq!(table.data.values().next().unwrap().as_fixnum(), Some(9));
}

#[test]
fn profiler_frames_remain_gc_roots_after_the_call_returns() {
    let mut ctx = Context::new();
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

#[test]
fn profiler_el_public_memory_workflow_builds_and_renders_a_report() {
    let mut ctx = crate::emacs_core::load::create_bootstrap_evaluator_cached().unwrap();
    let lisp_root = crate::test_utils::workspace_root()
        .as_path()
        .join("lisp")
        .canonicalize()
        .unwrap();
    let profiler_el = lisp_root.join("profiler.el");
    ctx.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![
            Value::string(lisp_root.to_string_lossy()),
            Value::string(lisp_root.join("emacs-lisp").to_string_lossy()),
        ]),
    );
    let load_result = ctx.eval_str(&format!(
        r#"(load {:?} nil t t)"#,
        profiler_el.to_string_lossy()
    ));
    assert_eq!(crate::emacs_core::format_eval_result(&load_result), "OK t");

    ctx.eval_str("(profiler-start 'mem)").unwrap();
    ctx.eval_str("(make-list 256 'profiled-value)").unwrap();
    ctx.eval_str("(profiler-stop)").unwrap();
    assert!(
        ctx.eval_str(
            "(and (hash-table-p profiler-memory-log) (> (hash-table-count profiler-memory-log) 0))"
        )
        .unwrap()
        .is_truthy()
    );
    assert!(
        ctx.eval_str("(profiler-calltree-p (profiler-calltree-build profiler-memory-log))")
            .unwrap()
            .is_truthy()
    );
    ctx.eval_str("(profiler-report)").unwrap();
}
