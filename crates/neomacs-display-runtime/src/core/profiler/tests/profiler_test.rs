use super::*;

// -- CallStack tests --

#[test]
fn call_stack_new_and_depth() {
    let cs = CallStack::new(vec![1, 2, 3]);
    assert_eq!(cs.depth(), 3);
    assert_eq!(cs.frames, vec![1, 2, 3]);
}

#[test]
fn call_stack_empty() {
    let cs = CallStack::empty();
    assert_eq!(cs.depth(), 0);
    assert!(cs.frames.is_empty());
}

#[test]
fn call_stack_equality() {
    let a = CallStack::new(vec![10, 20, 30]);
    let b = CallStack::new(vec![10, 20, 30]);
    let c = CallStack::new(vec![10, 20, 31]);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn call_stack_hash_consistency() {
    let a = CallStack::new(vec![100, 200]);
    let b = CallStack::new(vec![100, 200]);
    assert_eq!(a.hash_value(), b.hash_value());
}

#[test]
fn call_stack_hash_differs_for_different_stacks() {
    let a = CallStack::new(vec![1, 2, 3]);
    let b = CallStack::new(vec![3, 2, 1]);
    // Not strictly guaranteed, but overwhelmingly likely.
    assert_ne!(a.hash_value(), b.hash_value());
}

#[test]
fn call_stack_from_slice() {
    let slice: &[u64] = &[7, 8, 9];
    let cs = CallStack::from(slice);
    assert_eq!(cs.frames, vec![7, 8, 9]);
}

#[test]
fn call_stack_from_vec() {
    let v = vec![42u64, 43];
    let cs = CallStack::from(v);
    assert_eq!(cs.frames, vec![42, 43]);
}

// -- ProfilerLog basic tests --

#[test]
fn profiler_log_new_is_empty() {
    let log = ProfilerLog::new(16);
    assert!(log.is_empty());
    assert_eq!(log.total_samples(), 0);
    assert_eq!(log.num_entries(), 0);
    assert_eq!(log.discarded(), 0);
    assert_eq!(log.gc_count(), 0);
    assert_eq!(log.max_stack_depth(), 16);
}

#[test]
fn profiler_log_record_and_count() {
    let mut log = ProfilerLog::new(16);
    let bt = [1u64, 2, 3];
    log.record(&bt);
    log.record(&bt);
    log.record(&bt);
    assert_eq!(log.count(&bt), 3);
    assert_eq!(log.total_samples(), 3);
    assert_eq!(log.num_entries(), 1);
}

#[test]
fn profiler_log_record_weighted() {
    let mut log = ProfilerLog::new(16);
    let bt = [10u64, 20];
    log.record_weighted(&bt, 100);
    log.record_weighted(&bt, 50);
    assert_eq!(log.count(&bt), 150);
    assert_eq!(log.total_samples(), 150);
}

#[test]
fn profiler_log_multiple_backtraces() {
    let mut log = ProfilerLog::new(16);
    let bt1 = [1u64, 2];
    let bt2 = [3u64, 4];
    log.record(&bt1);
    log.record(&bt1);
    log.record(&bt2);
    assert_eq!(log.count(&bt1), 2);
    assert_eq!(log.count(&bt2), 1);
    assert_eq!(log.num_entries(), 2);
    assert_eq!(log.total_samples(), 3);
}

#[test]
fn profiler_log_truncates_deep_backtrace() {
    let mut log = ProfilerLog::new(3);
    let deep_bt = [1u64, 2, 3, 4, 5, 6];
    log.record(&deep_bt);
    // Should be stored truncated to depth 3.
    assert_eq!(log.count(&[1, 2, 3]), 1);
    // The full backtrace should not match.
    assert_eq!(log.count(&deep_bt), 0);
}

#[test]
fn profiler_log_capacity_overflow_discards() {
    let mut log = ProfilerLog::with_capacity(16, 2);
    log.record(&[1u64, 2]);
    log.record(&[3u64, 4]);
    // Log is now full (2 distinct entries).
    log.record(&[5u64, 6]); // Should be discarded.
    assert_eq!(log.num_entries(), 2);
    assert_eq!(log.discarded(), 1);
    assert_eq!(log.count(&[5, 6]), 0);
}

#[test]
fn profiler_log_overflow_existing_entry_still_works() {
    let mut log = ProfilerLog::with_capacity(16, 2);
    log.record(&[1u64, 2]);
    log.record(&[3u64, 4]);
    // Log is full, but recording an existing backtrace should still work.
    log.record(&[1u64, 2]);
    assert_eq!(log.count(&[1, 2]), 2);
    assert_eq!(log.discarded(), 0);
}

#[test]
fn profiler_log_gc_count() {
    let mut log = ProfilerLog::new(16);
    log.record_gc(5);
    log.record_gc(3);
    assert_eq!(log.gc_count(), 8);
    assert_eq!(log.total_samples(), 8);
    // GC samples don't create entries.
    assert_eq!(log.num_entries(), 0);
    assert!(!log.is_empty());
}

#[test]
fn profiler_log_clear() {
    let mut log = ProfilerLog::new(16);
    log.record(&[1u64, 2]);
    log.record_gc(10);
    assert!(!log.is_empty());
    log.clear();
    assert!(log.is_empty());
    assert_eq!(log.total_samples(), 0);
    assert_eq!(log.gc_count(), 0);
    assert_eq!(log.discarded(), 0);
}

#[test]
fn profiler_log_count_nonexistent() {
    let log = ProfilerLog::new(16);
    assert_eq!(log.count(&[99, 100]), 0);
}

#[test]
fn profiler_log_saturating_add() {
    let mut log = ProfilerLog::new(16);
    let bt = [1u64];
    log.record_weighted(&bt, u64::MAX - 1);
    log.record_weighted(&bt, 10);
    // Should saturate to u64::MAX.
    assert_eq!(log.count(&bt), u64::MAX);
}

#[test]
fn profiler_log_iter() {
    let mut log = ProfilerLog::new(16);
    log.record(&[1u64]);
    log.record(&[2u64]);
    log.record(&[1u64]);
    let collected: HashMap<Vec<u64>, u64> =
        log.iter().map(|(k, &v)| (k.frames.clone(), v)).collect();
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[&vec![1u64]], 2);
    assert_eq!(collected[&vec![2u64]], 1);
}

// -- ProfilerState tests --

#[test]
fn profiler_state_lifecycle() {
    let mut state = ProfilerState::new(16, 1000);
    assert!(!state.is_running());

    // Start.
    assert!(state.start(ProfilerMode::Cpu, 1_000_000));
    assert!(state.is_running());
    assert_eq!(state.mode(), ProfilerMode::Cpu);
    assert_eq!(state.sample_interval_ns(), 1_000_000);

    // Record some samples.
    state.log_mut().record(&[1u64, 2, 3]);
    state.log_mut().record(&[1u64, 2, 3]);
    assert_eq!(state.log().total_samples(), 2);

    // Stop and get log.
    let log = state.stop();
    assert!(!state.is_running());
    assert_eq!(log.total_samples(), 2);
    assert_eq!(log.count(&[1, 2, 3]), 2);

    // Internal log should be fresh.
    assert!(state.log().is_empty());
}

#[test]
fn profiler_state_double_start_returns_false() {
    let mut state = ProfilerState::new(16, 1000);
    assert!(state.start(ProfilerMode::Cpu, 1_000_000));
    // Second start should fail.
    assert!(!state.start(ProfilerMode::Memory, 0));
    // Mode should remain CPU.
    assert_eq!(state.mode(), ProfilerMode::Cpu);
}

#[test]
fn profiler_state_stop_when_not_running() {
    let mut state = ProfilerState::new(16, 1000);
    let log = state.stop();
    assert!(log.is_empty());
}

#[test]
fn profiler_state_memory_mode() {
    let mut state = ProfilerState::new(16, 1000);
    assert!(state.start(ProfilerMode::Memory, 0));
    assert_eq!(state.mode(), ProfilerMode::Memory);
    state.log_mut().record_weighted(&[10u64, 20], 4096);
    let log = state.stop();
    assert_eq!(log.count(&[10, 20]), 4096);
}

// -- Report and merge tests --

#[test]
fn generate_report_basic() {
    let mut log = ProfilerLog::new(16);
    log.record_weighted(&[1u64, 2], 100);
    log.record_weighted(&[3u64, 4], 50);
    log.record_weighted(&[5u64, 6], 200);
    log.record_gc(10);

    let report = generate_report(&log);
    assert_eq!(report.total_samples, 360);
    assert_eq!(report.num_entries, 3);
    assert_eq!(report.gc_count, 10);
    assert_eq!(report.discarded, 0);
    // Top should be sorted descending.
    assert_eq!(report.top[0].1, 200);
    assert_eq!(report.top[1].1, 100);
    assert_eq!(report.top[2].1, 50);
}

#[test]
fn top_entries_limits_count() {
    let mut log = ProfilerLog::new(16);
    for i in 0..20u64 {
        log.record_weighted(&[i], i + 1);
    }
    let top = top_entries(&log, 5);
    assert_eq!(top.len(), 5);
    // Highest count should be 20 (from backtrace [19]).
    assert_eq!(top[0].1, 20);
    assert_eq!(top[4].1, 16);
}

#[test]
fn top_entries_more_than_available() {
    let mut log = ProfilerLog::new(16);
    log.record(&[1u64]);
    log.record(&[2u64]);
    let top = top_entries(&log, 100);
    assert_eq!(top.len(), 2);
}

#[test]
fn merge_logs_basic() {
    let mut log1 = ProfilerLog::new(16);
    log1.record_weighted(&[1u64, 2], 10);
    log1.record_weighted(&[3u64, 4], 5);
    log1.record_gc(2);

    let mut log2 = ProfilerLog::new(16);
    log2.record_weighted(&[1u64, 2], 20);
    log2.record_weighted(&[5u64, 6], 15);
    log2.record_gc(3);

    let merged = merge_logs(&[&log1, &log2]);
    assert_eq!(merged.count(&[1, 2]), 30);
    assert_eq!(merged.count(&[3, 4]), 5);
    assert_eq!(merged.count(&[5, 6]), 15);
    assert_eq!(merged.gc_count(), 5);
    assert_eq!(merged.num_entries(), 3);
}

#[test]
fn merge_logs_empty_input() {
    let merged = merge_logs(&[]);
    assert!(merged.is_empty());
}

#[test]
fn merge_logs_discarded_sums() {
    let mut log1 = ProfilerLog::with_capacity(16, 1);
    log1.record(&[1u64]);
    log1.record(&[2u64]); // Discarded.
    assert_eq!(log1.discarded(), 1);

    let mut log2 = ProfilerLog::with_capacity(16, 1);
    log2.record(&[3u64]);
    log2.record(&[4u64]); // Discarded.
    log2.record(&[5u64]); // Discarded.
    assert_eq!(log2.discarded(), 2);

    let merged = merge_logs(&[&log1, &log2]);
    assert_eq!(merged.discarded(), 3);
}

#[test]
fn profiler_log_with_capacity_accessors() {
    let log = ProfilerLog::with_capacity(8, 500);
    assert_eq!(log.max_stack_depth(), 8);
    assert_eq!(log.max_entries(), 500);
}
