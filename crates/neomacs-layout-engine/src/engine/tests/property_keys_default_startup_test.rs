//! Real absence-only PropertyKeys policy; no environment mutation or override.
//! One test thread exclusively owns each Context and copied renderer observation.
//! Existing numeric guards select other producer policies without storing Lisp
//! state; PropertyKeys itself is genuinely absent at process startup.
use super::*;

#[test]
fn property_keys_absent_startup_inlines_warmed_sync_after_complete_outputs() {
    assert!(
        std::env::var_os("NEOMACS_LAYOUT_PROPERTY_KEYS_INLINE").is_none(),
        "fixture requires absent process-startup PropertyKeys selector"
    );
    assert_eq!(
        probes::forced(),
        None,
        "default selector must not be forced"
    );
    // Measure the actual default first, before any explicit reference override.
    let actual_default = observed_sync_with_policy(None);
    assert_eq!(probes::forced(), None);
    let off = observed_sync(false);
    let on = observed_sync(true);
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
        "default/OFF callback and position observers"
    );
    assert_eq!(
        actual_default.1, on.1,
        "default/ON callback and position observers"
    );
    for (label, stats, sync) in [
        ("default", &actual_default.2, &actual_default.4),
        ("OFF", &off.2, &off.4),
        ("ON", &on.2, &on.4),
    ] {
        assert_eq!(stats.edit_windows, 1, "{label}: actual edit producer");
        assert!(stats.reused_rows > 0, "{label}: retained rows must be used");
        assert!(sync.sync_admissions > 0, "{label}: must admit real Sync");
    }
    assert!(
        off.3.heap_materializations > 0,
        "actual OFF allocation-site control"
    );
    assert_eq!(on.3.heap_materializations, 0, "actual ON reference");
    assert!(on.3.inline_constructions > 0);
    // Intended default-policy counter RED occurs only after all complete outputs,
    // references, callbacks and real Sync admission have been proved above.
    assert_eq!(
        actual_default.3.heap_materializations, 0,
        "absent PropertyKeys selector still allocated canonical lookup keys"
    );
    assert!(actual_default.3.inline_constructions > 0);
    assert_eq!(
        probes::forced(),
        None,
        "references restore absent numeric policy"
    );
}
