use super::*;

#[test]
fn memory_telemetry_process_units_and_missing_fields_are_explicit() {
    let snapshot = ProcessSnapshot::parse(b"Name:\tneomacs\nVmHWM:\t17000 kB\nVmRSS:\t12345 kB\n");
    assert_eq!(snapshot.rss_bytes, Some(12345 * 1024));
    assert_eq!(snapshot.peak_rss_bytes, Some(17000 * 1024));
    let missing = ProcessSnapshot::parse(b"VmRSS:\t7 bytes\nVmHWM:\tunknown kB\n");
    assert!(missing.rss_bytes.is_none());
    assert!(missing.peak_rss_bytes.is_none());
}

#[test]
fn memory_telemetry_process_rejects_overflow_and_invalid_utf8() {
    let status = format!("VmRSS:\t{} kB\n", usize::MAX);
    assert!(
        ProcessSnapshot::parse(status.as_bytes())
            .rss_bytes
            .is_none()
    );
    assert!(
        ProcessSnapshot::parse(b"VmRSS:\t8 kB\n\xff")
            .rss_bytes
            .is_none()
    );
}

#[test]
fn memory_telemetry_unsupported_allocator_does_not_claim_zero_bytes() {
    let json = serde_json::to_value(AllocatorSnapshot::default()).unwrap();
    assert_eq!(json["available"], false);
    assert_eq!(json["stats_enabled"], false);
    assert!(json["resident_bytes"].is_null());
    assert!(json["allocated_bytes"].is_null());
}

#[test]
fn memory_telemetry_records_all_due_reasons_without_advancing_pacing() {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = true;
    heap.bootstrap_collected = true;
    heap.generational.old_bytes_after_major = 32 * 1024 * 1024;
    heap.generational.pacing_knobs = knobs::GenerationalPacingKnobs {
        major_growth_percent: 50,
        major_max_minors: 12,
        stress_major_every: 8,
    };
    heap.current_mutator_gc_mut().pacing = pacing::GenerationPacingCounters {
        promoted_since_major: 16 * 1024 * 1024,
        minors_since_major: 12,
        stress_cycles_since_major: 7,
    };
    let before = heap.current_mutator_gc().pacing;
    assert_eq!(
        selection_reasons(&heap, true, true),
        vec!["memory_full", "old_growth", "minor_limit", "stress_stride"]
    );
    assert_eq!(heap.current_mutator_gc().pacing, before);
    heap.generational.enabled = false;
    assert_eq!(
        selection_reasons(&heap, true, true),
        vec!["generational_disabled"]
    );
}
