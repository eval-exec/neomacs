//! Real absent-startup Coverage policy through the existing warmed Sync producer.
//! Each observation exclusively owns its Context and returns copied complete
//! frame/face/source observations, callback strings and numeric witnesses. The
//! default scope is !Send/!Sync, owns only its test-thread counters/Still policy,
//! and never forces Coverage or mutates the environment/process OnceLock.
use super::*;

fn observed_actual_startup() -> (
    FrameObservation,
    String,
    LayoutStats,
    sync_probes::Counts,
    coverage::Counts,
) {
    let _witnesses = CoverageGuard::observe_default();
    let observed = observed_sync(true);
    (
        observed.0,
        observed.1,
        observed.2,
        observed.4,
        coverage::counts(),
    )
}

#[test]
fn coverage_absent_startup_elides_fontify_points_after_complete_outputs() {
    assert!(
        std::env::var_os("NEOMACS_EDIT_SYNC_FONTIFY_COVERAGE").is_none(),
        "fixture requires absent process-startup Coverage selector"
    );
    assert_eq!(coverage::forced(), None, "Coverage must start unforced");
    // Initialize/observe the real absent process policy before any explicit
    // reference override. The existing producer also checks a fresh full walk.
    let actual_default = observed_actual_startup();
    assert_eq!(coverage::forced(), None);
    let off = observed_coverage(false);
    let on = observed_coverage(true);
    assert_eq!(
        actual_default.0, off.0,
        "complete default/OFF frame and faces"
    );
    assert_eq!(
        actual_default.0, on.0,
        "complete default/ON frame and faces"
    );
    assert_eq!(
        actual_default.1, off.1,
        "default/OFF callbacks and observers"
    );
    assert_eq!(actual_default.1, on.1, "default/ON callbacks and observers");
    for (label, stats, sync, counts) in [
        (
            "default",
            &actual_default.2,
            &actual_default.3,
            &actual_default.4,
        ),
        ("OFF", &off.2, &off.3, &off.4),
        ("ON", &on.2, &on.3, &on.4),
    ] {
        assert_eq!(stats.edit_windows, 1, "{label}: real edit producer");
        assert!(stats.reused_rows > 0, "{label}: retained rows must be used");
        assert!(sync.sync_admissions > 0, "{label}: real Sync admission");
        assert!(
            counts.completed_sync_installs > 0,
            "{label}: must reach real Finished Sync and install"
        );
        assert_eq!(counts.sync_queries, 0, "{label}: no old uncovered query");
    }
    assert!(
        off.4.sync_iterators > 0 && off.4.sync_points > 0,
        "actual original OFF iterator construction and visits: {:?}",
        off.4
    );
    assert_eq!(
        on.4.sync_iterators, 0,
        "explicit ON reference must avoid old iterator"
    );
    assert_eq!(on.4.sync_points, 0);
    assert!(on.4.shortcuts > 0);
    // Only this final counter is the intended absence-policy RED. All complete
    // canonical/output/callback, actual finish and reference checks precede it.
    assert_eq!(
        actual_default.4.sync_iterators, 0,
        "absent Coverage selector still constructed fontification point iterators"
    );
    assert_eq!(actual_default.4.sync_points, 0);
    assert!(actual_default.4.shortcuts > 0);
    assert_eq!(
        coverage::forced(),
        None,
        "references restore absent Coverage policy"
    );
}
