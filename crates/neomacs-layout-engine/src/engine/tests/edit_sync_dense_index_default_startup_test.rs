//! Real absent-startup Dense policy through the existing warmed Sync producer.
//! Each call exclusively owns its Context and returns copied frame/face/source
//! observations, strings and numeric counters. The default scope owns only
//! thread-local numeric witnesses/Still policy and never forces Dense; it cannot
//! migrate threads and restores every prior value on unwind. No environment
//! mutation, Lisp cache, heap handle or shared fixture state is introduced.
use super::*;

fn observed_actual_startup() -> (
    FrameObservation,
    String,
    LayoutStats,
    sync_probes::Counts,
    dense::Counts,
) {
    let _witnesses = DenseGuard::observe_default();
    let observed = observed_sync(true);
    (
        observed.0,
        observed.1,
        observed.2,
        observed.4,
        dense::counts(),
    )
}

#[test]
fn dense_index_absent_startup_elides_numeric_hashes_after_complete_outputs() {
    assert!(
        std::env::var_os("NEOMACS_EDIT_SYNC_DENSE_INDEX").is_none(),
        "fixture requires absent process-startup Dense selector"
    );
    assert_eq!(dense::forced(), None, "Dense policy must start unforced");
    // Observe actual startup first; explicit OFF/ON references do not initialize
    // or replace the production policy used by this default observation.
    let actual_default = observed_actual_startup();
    assert_eq!(dense::forced(), None);
    let off = observed_dense(false);
    let on = observed_dense(true);
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
    for (label, stats, sync) in [
        ("default", &actual_default.2, &actual_default.3),
        ("OFF", &off.2, &off.3),
        ("ON", &on.2, &on.3),
    ] {
        assert_eq!(stats.edit_windows, 1, "{label}: actual edit producer");
        assert!(stats.reused_rows > 0, "{label}: must retain real rows");
        assert!(sync.sync_admissions > 0, "{label}: must admit real Sync");
    }
    assert!(
        off.4.plan_insertions > 0 && off.4.install_insertions > 0,
        "actual original OFF hash sites: {:?}",
        off.4
    );
    assert_eq!(
        on.4.plan_insertions + on.4.install_insertions,
        0,
        "explicit ON reference must avoid legacy hash indexes"
    );
    assert!(on.4.dense_plans > 0 && on.4.dense_installs > 0);
    // Intended absence-policy counter RED occurs only after complete canonical
    // output, callback and real Sync witnesses plus explicit controls above.
    assert_eq!(
        actual_default.4.plan_insertions + actual_default.4.install_insertions,
        0,
        "absent Dense selector still constructed numeric hash indexes"
    );
    assert!(actual_default.4.dense_plans > 0 && actual_default.4.dense_installs > 0);
    assert_eq!(
        dense::forced(),
        None,
        "references restore absent Dense policy"
    );
}
