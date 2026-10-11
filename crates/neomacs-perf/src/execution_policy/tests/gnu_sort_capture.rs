use crate::ExecutionOverride;
use std::ffi::OsString;

#[test]
fn removed_sort_capture_setting_cannot_change_benchmark_policy() {
    for setting in [
        "NEOVM_SORT_CAPTURE",
        "NEOVM_SORT_CAPTURE=off",
        "NEOVM_SORT_CAPTURE=on",
    ] {
        assert!(setting.parse::<ExecutionOverride>().is_err(), "{setting}");
        assert!(
            serde_json::from_value::<ExecutionOverride>(serde_json::json!(setting)).is_err(),
            "{setting}"
        );
    }
    assert!(
        crate::harness::passthrough_from([(
            OsString::from("NEOVM_SORT_CAPTURE"),
            OsString::from("off"),
        )])
        .is_empty()
    );
}
