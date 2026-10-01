use super::{
    editor_workload::{EditorWorkloadResult, valid_editor_workload_measurements},
    rust_lsp::{RustLspTypingResult, valid_rust_lsp_typing_measurements},
};
use crate::{MetricName, MetricUnit};
use serde_json::{Value, json};

#[test]
fn typing_result_gc_boundaries_accept_legacy_and_validate_new_fields() {
    let rust = json!({
        "schema_version": 1, "scenario": "rust-lsp-typing", "status": "ok",
        "iterations": 2, "elapsed_us": 100, "major_mode": "rust-ts-mode",
        "lsp_mode_loaded": true, "treesit_parser_language": "rust",
        "text_unchanged": true, "point_unchanged": true, "overlay_count": 4,
        "lsp_diagnostic_count": 4, "error": null
    });
    let sustained = json!({
        "schema_version": 1, "scenario": "sustained-editing", "status": "ok",
        "iterations": 2, "elapsed_us": 100, "elapsed_wall_us": 120,
        "operation_count": 2, "initial_checksum": "same", "final_checksum": "same",
        "point_restored": true, "expected_major_mode": "emacs-lisp-mode",
        "actual_major_mode": "emacs-lisp-mode", "type_phase_us": 100,
        "comment_phase_us": 0, "kill_yank_phase_us": 0, "indent_phase_us": 0,
        "regex_phase_us": 0, "latency_samples_us": [], "mode_phase_us": 0,
        "fontify_phase_us": 0, "replace_phase_us": 0, "undo_redo_phase_us": 0,
        "isearch_phase_us": 0, "buffer_switch_phase_us": 0, "how_many_phase_us": 0,
        "motion_phase_us": 0, "error": null
    });
    let boundaries = json!({
        "gcs_done_start": 12, "gcs_done_end": 15, "gcs_done_delta": 3,
        "gc_elapsed_us_start": 100, "gc_elapsed_us_end": 180,
        "gc_elapsed_us_delta": 80
    });
    for (base, rust_schema) in [(rust, true), (sustained, false)] {
        let parse = |value: Value| {
            if rust_schema {
                serde_json::from_value::<RustLspTypingResult>(value)
                    .map(|result| valid_rust_lsp_typing_measurements(&result, 1))
            } else {
                serde_json::from_value::<EditorWorkloadResult>(value)
                    .map(|result| valid_editor_workload_measurements(&result, 1))
            }
        };
        assert!(
            parse(base.clone()).is_ok(),
            "legacy artifact remains accepted"
        );
        let names = [
            MetricName::GcsDoneStart,
            MetricName::GcsDoneEnd,
            MetricName::GcsDoneDelta,
            MetricName::GcElapsedStart,
            MetricName::GcElapsedEnd,
            MetricName::GcElapsedDelta,
        ];
        assert!(
            parse(base.clone())
                .unwrap()
                .iter()
                .all(|m| !names.contains(&m.name)),
            "legacy results omit GC measurements"
        );
        let mut complete = base.clone();
        complete
            .as_object_mut()
            .unwrap()
            .extend(boundaries.as_object().unwrap().clone());
        assert!(
            parse(complete.clone()).is_ok(),
            "consistent boundaries are accepted"
        );
        let measurements = parse(complete.clone()).unwrap();
        for (index, (name, value)) in names
            .into_iter()
            .zip([12.0, 15.0, 3.0, 100.0, 180.0, 80.0])
            .enumerate()
        {
            let found: Vec<_> = measurements.iter().filter(|m| m.name == name).collect();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].value, value);
            assert_eq!(
                found[0].unit,
                if index < 3 {
                    MetricUnit::Count
                } else {
                    MetricUnit::Microseconds
                }
            );
        }
        let mut no_collections = complete.clone();
        no_collections["gcs_done_end"] = json!(12);
        no_collections["gcs_done_delta"] = json!(0);
        no_collections["gc_elapsed_us_end"] = json!(100);
        no_collections["gc_elapsed_us_delta"] = json!(0);
        let zeros = parse(no_collections).unwrap();
        for name in [MetricName::GcsDoneDelta, MetricName::GcElapsedDelta] {
            assert!(
                zeros.iter().any(|m| m.name == name && m.value == 0.0),
                "zero deltas are measurements, not omitted fields"
            );
        }
        complete["gcs_done_delta"] = json!(2);
        assert!(
            parse(complete.clone()).is_err(),
            "wrong count delta is rejected"
        );
        complete["gcs_done_delta"] = json!(3);
        complete["gc_elapsed_us_end"] = json!(99);
        assert!(
            parse(complete).is_err(),
            "backward time boundary is rejected"
        );
        let mut extreme = base.clone();
        extreme
            .as_object_mut()
            .unwrap()
            .extend(boundaries.as_object().unwrap().clone());
        extreme["gcs_done_start"] = json!(u64::MAX - 2);
        extreme["gcs_done_end"] = json!(u64::MAX);
        extreme["gcs_done_delta"] = json!(2);
        assert!(
            parse(extreme.clone()).is_ok(),
            "unsigned boundary maximum is safe"
        );
        extreme["gcs_done_end"] = json!(0);
        assert!(
            parse(extreme).is_err(),
            "count underflow is rejected without wrapping"
        );
        let mut wrong_type = base.clone();
        wrong_type
            .as_object_mut()
            .unwrap()
            .extend(boundaries.as_object().unwrap().clone());
        wrong_type["gcs_done_start"] = json!(-1);
        assert!(
            parse(wrong_type.clone()).is_err(),
            "negative counts are rejected"
        );
        wrong_type["gcs_done_start"] = Value::Null;
        assert!(
            parse(wrong_type).is_err(),
            "present null is not a legacy field"
        );
        let mut null_boundaries = base.clone();
        for key in boundaries.as_object().unwrap().keys() {
            null_boundaries[key] = Value::Null;
        }
        assert!(
            parse(null_boundaries).is_err(),
            "all explicit nulls are not a legacy record"
        );
        let mut incomplete = base;
        incomplete["gcs_done_start"] = json!(12);
        assert!(
            parse(incomplete).is_err(),
            "partial boundary record is rejected"
        );
    }
}
