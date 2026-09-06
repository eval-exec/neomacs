use std::fs;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::Command;

use super::{
    Frontend, MetricName, PerfError, PerfHarness, RunRequest, RunVerdict, ScenarioId,
    collect_editor_provenance, configure_benchmark_environment, validate_harness_build,
    validate_harness_revision,
};

#[test]
fn invalid_scenario_output_is_persisted_but_never_accepted_as_a_sample() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(3).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 2,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 4,
              "lsp_diagnostic_count": 4,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    assert!(matches!(
        report.artifact.verdict,
        RunVerdict::CorrectnessMismatch { .. }
    ));
    assert!(!report.artifact.verdict.is_valid());
    assert!(
        report
            .artifact_path
            .starts_with(workspace.path().join("tmp/perf"))
    );
    assert!(report.artifact_path.is_file());

    let persisted = fs::read_to_string(&report.artifact_path).expect("read artifact");
    assert!(persisted.contains("correctness-mismatch"));
    assert!(persisted.contains("iterations"));
}

#[test]
fn fixture_overlay_count_is_checked_against_the_harness_oracle() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-oracle-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(3).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 3,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 3,
              "lsp_diagnostic_count": 4,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    match report.artifact.verdict {
        RunVerdict::CorrectnessMismatch { mismatches } => {
            assert!(
                mismatches
                    .iter()
                    .any(|mismatch| mismatch.invariant == "overlay-count")
            );
        }
        verdict => panic!("fixture-owned expectation was accepted: {verdict:?}"),
    }
}

#[test]
fn mx_tab_result_is_valid_only_when_the_real_completion_window_lifecycle_completed() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-mx-tab-result-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::MxTabCompletion,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(2).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Tui {
        rows: 40,
        columns: 120,
    });

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "mx-tab-completion",
              "status": "ok",
              "iterations": 2,
              "elapsed_us": 1000,
              "completion_help_calls": 2,
              "completion_visible": true,
              "completion_mode_correct": true,
              "known_commands_present": true,
              "completion_candidate_count": 1024,
              "candidate_count_stable": true,
              "completion_hidden_after_exit": true,
              "minibuffer_depth_restored": true,
              "selected_buffer_restored": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::Valid { measurements } = report.artifact.verdict else {
        panic!("correct M-x TAB lifecycle was rejected")
    };
    let per_completion = measurements
        .iter()
        .find(|measurement| measurement.name == super::MetricName::PerCompletionCpuTime)
        .expect("per-completion metric");
    assert_eq!(per_completion.value, 500.0);
    assert_eq!(
        per_completion.unit,
        super::MetricUnit::MicrosecondsPerCompletion
    );
}

#[test]
fn mx_tab_result_cannot_treat_a_missing_completion_window_as_a_fast_sample() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-mx-tab-mismatch-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::MxTabCompletion,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(1).expect("non-zero iterations"),
    );

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "mx-tab-completion",
              "status": "ok",
              "iterations": 1,
              "elapsed_us": 1,
              "completion_help_calls": 1,
              "completion_visible": false,
              "completion_mode_correct": true,
              "known_commands_present": true,
              "completion_candidate_count": 1023,
              "candidate_count_stable": false,
              "completion_hidden_after_exit": true,
              "minibuffer_depth_restored": true,
              "selected_buffer_restored": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
        panic!("missing completion window was accepted")
    };
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "completion-window-visible")
    );
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "completion-candidate-count-stable")
    );
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "completion-candidate-count")
    );
}

#[test]
fn bytecode_call_loop_accepts_only_the_full_interpreted_call_count() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-bytecode-call-result-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::BytecodeCallLoop,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(20).expect("non-zero iterations"),
    );

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "bytecode-call-loop",
              "status": "ok",
              "iterations": 20,
              "elapsed_us": 10,
              "bytecode_calls": 20,
              "result": 1,
              "expected_result": 1,
              "bytecode_functions_compiled": true,
              "interpreter_requested": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::Valid { measurements } = report.artifact.verdict else {
        panic!("correct bytecode call loop was rejected")
    };
    let per_call = measurements
        .iter()
        .find(|measurement| measurement.name == super::MetricName::PerBytecodeCallCpuTime)
        .expect("per-bytecode-call metric");
    assert_eq!(per_call.value, 0.5);
    assert_eq!(
        per_call.unit,
        super::MetricUnit::MicrosecondsPerBytecodeCall
    );
}

#[test]
fn bytecode_call_loop_rejects_a_short_or_wrong_result() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-bytecode-call-mismatch-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::BytecodeCallLoop,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(20).expect("non-zero iterations"),
    );

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "bytecode-call-loop",
              "status": "ok",
              "iterations": 20,
              "elapsed_us": 1,
              "bytecode_calls": 19,
              "result": 20,
              "expected_result": 1,
              "bytecode_functions_compiled": true,
              "interpreter_requested": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
        panic!("incomplete bytecode call loop was accepted")
    };
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "bytecode-call-count")
    );
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "bytecode-result")
    );
}

#[test]
fn scenario_result_requires_every_schema_field() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-schema-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(1).expect("non-zero iterations"),
    );

    let error = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 1,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 4,
              "lsp_diagnostic_count": 4
            }"##,
        )
        .expect_err("the required `error` field is absent");

    assert!(matches!(error, PerfError::InvalidScenarioResult { .. }));
    assert!(error.to_string().contains("missing field `error`"));
}

#[test]
fn scenario_result_rejects_a_success_status_with_an_error() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-outcome-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(1).expect("non-zero iterations"),
    );

    let error = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 1,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 4,
              "lsp_diagnostic_count": 4,
              "error": "workload failed"
            }"##,
        )
        .expect_err("success and an error message are contradictory");

    assert!(matches!(error, PerfError::InvalidScenarioResult { .. }));
    assert!(
        error
            .to_string()
            .contains("status `ok` requires a null error")
    );
}

#[test]
fn editing_simulation_requires_every_promoted_phase_measurement() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let workspace_tmp = workspace_root.join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-editor-workload-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let request = RunRequest::new(
        ScenarioId::EditingSimulation,
        "/unused/editor",
        NonZeroU32::new(1).expect("non-zero literal"),
    );
    let raw_result = r#"{
      "schema_version": 1,
      "scenario": "editing-simulation",
      "status": "ok",
      "iterations": 1,
      "elapsed_us": 100,
      "elapsed_wall_us": 120,
      "operation_count": 1,
      "initial_checksum": "same",
      "final_checksum": "same",
      "point_restored": true,
      "expected_major_mode": "emacs-lisp-mode",
      "actual_major_mode": "emacs-lisp-mode",
      "type_phase_us": 1,
      "comment_phase_us": 1,
      "kill_yank_phase_us": 1,
      "indent_phase_us": 1,
      "regex_phase_us": 1,
      "latency_samples_us": [],
      "mode_phase_us": 1,
      "fontify_phase_us": 1,
      "replace_phase_us": 1,
      "undo_redo_phase_us": 1,
      "isearch_phase_us": 1,
      "buffer_switch_phase_us": 1,
      "how_many_phase_us": 1,
      "motion_phase_us": 0,
      "error": null
    }"#;

    let report = PerfHarness::new(scratch.path())
        .record_fixture_result(&request, raw_result)
        .expect("persist invalid promoted workload result");
    let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
        panic!("a missing promoted phase must reject the sample")
    };
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "motion-phase-time")
    );
}

#[test]
fn sustained_editing_reports_insert_and_delete_as_two_edits() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let workspace_tmp = workspace_root.join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-sustained-editing-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let request = RunRequest::new(
        ScenarioId::SustainedEditing,
        "/unused/editor",
        NonZeroU32::new(2).expect("non-zero literal"),
    );
    let raw_result = r#"{
      "schema_version": 1,
      "scenario": "sustained-editing",
      "status": "ok",
      "iterations": 2,
      "elapsed_us": 100,
      "elapsed_wall_us": 120,
      "operation_count": 2,
      "initial_checksum": "same",
      "final_checksum": "same",
      "point_restored": true,
      "expected_major_mode": "emacs-lisp-mode",
      "actual_major_mode": "emacs-lisp-mode",
      "type_phase_us": 100,
      "comment_phase_us": 0,
      "kill_yank_phase_us": 0,
      "indent_phase_us": 0,
      "regex_phase_us": 0,
      "latency_samples_us": [],
      "mode_phase_us": 0,
      "fontify_phase_us": 0,
      "replace_phase_us": 0,
      "undo_redo_phase_us": 0,
      "isearch_phase_us": 0,
      "buffer_switch_phase_us": 0,
      "how_many_phase_us": 0,
      "motion_phase_us": 0,
      "error": null
    }"#;

    let report = PerfHarness::new(scratch.path())
        .record_fixture_result(&request, raw_result)
        .expect("persist sustained workload result");
    let measurements = report
        .artifact
        .verdict
        .measurements()
        .expect("valid sustained workload measurements");
    let edits = measurements
        .iter()
        .find(|measurement| measurement.name == MetricName::Edits)
        .expect("typed edit count");
    let per_edit = measurements
        .iter()
        .find(|measurement| measurement.name == MetricName::PerEditCpuTime)
        .expect("typed per-edit duration");
    let per_edit_wall = measurements
        .iter()
        .find(|measurement| measurement.name == MetricName::PerEditWallTime)
        .expect("typed wall-clock per-edit duration");
    assert_eq!(edits.value, 4.0);
    assert_eq!(per_edit.value, 25.0);
    assert_eq!(per_edit_wall.value, 30.0);
}

#[test]
fn gui_input_latency_counts_keystrokes_over_the_one_millisecond_budget() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let workspace_tmp = workspace_root.join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-gui-latency-budget-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let request = RunRequest::new(
        ScenarioId::GuiInputLatency,
        "/unused/editor",
        NonZeroU32::new(5).expect("non-zero literal"),
    );
    // Exactly 1000 us is within budget; the count is of samples strictly over it.
    let raw_result = r#"{
      "schema_version": 1,
      "scenario": "gui-input-latency",
      "status": "ok",
      "iterations": 5,
      "elapsed_us": 100,
      "elapsed_wall_us": 120,
      "operation_count": 5,
      "initial_checksum": "same",
      "final_checksum": "same",
      "point_restored": true,
      "expected_major_mode": "emacs-lisp-mode",
      "actual_major_mode": "emacs-lisp-mode",
      "type_phase_us": 0,
      "comment_phase_us": 0,
      "kill_yank_phase_us": 0,
      "indent_phase_us": 0,
      "regex_phase_us": 0,
      "latency_samples_us": [500, 1500, 999, 1000, 2000],
      "mode_phase_us": 0,
      "fontify_phase_us": 0,
      "replace_phase_us": 0,
      "undo_redo_phase_us": 0,
      "isearch_phase_us": 0,
      "buffer_switch_phase_us": 0,
      "how_many_phase_us": 0,
      "motion_phase_us": 0,
      "error": null
    }"#;

    let report = PerfHarness::new(scratch.path())
        .record_fixture_result(&request, raw_result)
        .expect("persist gui latency result");
    let measurements = report
        .artifact
        .verdict
        .measurements()
        .expect("valid gui latency measurements");
    let metric = |name: MetricName| {
        measurements
            .iter()
            .find(|measurement| measurement.name == name)
            .unwrap_or_else(|| panic!("{name:?} measurement"))
            .value
    };
    assert_eq!(metric(MetricName::InputLatencyOverBudgetCount), 2.0);
    assert_eq!(metric(MetricName::P50InputToRedisplayLatency), 1000.0);
    assert_eq!(metric(MetricName::P99InputToRedisplayLatency), 2000.0);
}

#[test]
fn sustained_native_video_promotes_pacing_gpu_pool_and_memory_metrics() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let workspace_tmp = workspace_root.join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-native-video-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let request = RunRequest::new(
        ScenarioId::SustainedNativeVideo,
        "/unused/editor",
        NonZeroU32::new(300).expect("non-zero literal"),
    );
    let raw_result = r#"{
      "schema_version": 4,
      "scenario": "sustained-native-video",
      "status": "ok",
      "iterations": 300,
      "elapsed_cpu_us": 6000000,
      "elapsed_wall_us": 30000000,
      "presentation_width_pixels": 1920,
      "presentation_height_pixels": 1080,
      "viewport_width_pixels": 3440,
      "viewport_height_pixels": 1880,
      "backend": "gstreamer",
      "decode_residency": "hardware-decoder-reports-renderer-device",
      "decoder_factory": "vah264dec",
      "decoder_plugin": "va",
      "decoder_kind": "hardware",
      "gpu_adapter_name": "AMD Radeon RX 7900 XTX",
      "gpu_vendor": 4098,
      "gpu_device": 29631,
      "gpu_device_type": "DiscreteGpu",
      "graphics_backend": "vulkan",
      "gpu_driver": "radv",
      "gpu_driver_info": "Mesa test driver",
      "drm_render_node": "/dev/dri/renderD128",
      "display_refresh_hz": 60,
      "frame_format": "nv12",
      "compositor_import": "borrowed-native-surface",
      "presentation": "wgpu-composited",
      "decoded_frames": 1800,
      "replaced_frames": 1,
      "late_dropped_frames": 2,
      "imported_frames": 1797,
      "backpressured_frames": 3,
      "borrowed_native_frames": 1797,
      "gpu_blit_frames": 0,
      "cpu_upload_frames": 0,
      "submitted_frames": 1797,
      "presented_frames": 1795,
      "interval_samples": 1794,
      "interval_p50_us": 16667,
      "interval_p95_us": 17000,
      "interval_p99_us": 18500,
      "interval_max_us": 22000,
      "gpu_timing_status": "enabled",
      "gpu_pass_samples": 1790,
      "gpu_pass_total_us": 895000,
      "gpu_pass_min_us": 350,
      "gpu_pass_max_us": 900,
      "gpu_memory_bytes": 24883200,
      "pool_capacity": 64,
      "pool_allocations": 0,
      "pool_reuses": 1793,
      "pool_backpressured_acquires": 0,
      "pool_in_flight_high_water": 3,
      "error": null
    }"#;

    let report = PerfHarness::new(scratch.path())
        .record_fixture_result(&request, raw_result)
        .expect("persist valid native-video result");
    let measurements = report
        .artifact
        .verdict
        .measurements()
        .expect("valid native-video measurements");
    let metric = |name| {
        measurements
            .iter()
            .find(|measurement| measurement.name == name)
            .expect("promoted typed metric")
            .value
    };
    assert_eq!(
        metric(MetricName::VideoPresentationFramesPerSecond),
        1795.0 / 30.0
    );
    assert_eq!(metric(MetricName::P99VideoPresentationInterval), 18_500.0);
    assert_eq!(metric(MetricName::AverageVideoGpuPassTime), 500.0);
    assert_eq!(metric(MetricName::VideoSurfacePoolReuses), 1793.0);
    assert_eq!(metric(MetricName::VideoGpuMemoryBytes), 24_883_200.0);

    let undersized_viewport = raw_result.replace(
        r#""viewport_width_pixels": 3440"#,
        r#""viewport_width_pixels": 1280"#,
    );
    let rejected = PerfHarness::new(scratch.path())
        .record_fixture_result(&request, &undersized_viewport)
        .expect("persist geometrically invalid native-video result");
    let RunVerdict::CorrectnessMismatch { mismatches } = rejected.artifact.verdict else {
        panic!("an undersized viewport must not produce a benchmark sample")
    };
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "presentation-fits-viewport")
    );
}

#[test]
fn sustained_native_video_rejects_a_cpu_upload_fallback() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let workspace_tmp = workspace_root.join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-native-video-fallback-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let request = RunRequest::new(
        ScenarioId::SustainedNativeVideo,
        "/unused/editor",
        NonZeroU32::new(1).expect("non-zero literal"),
    );
    let raw_result = r#"{
      "schema_version": 4, "scenario": "sustained-native-video", "status": "ok",
      "iterations": 1, "elapsed_cpu_us": 1, "elapsed_wall_us": 100000,
      "presentation_width_pixels": 1920, "presentation_height_pixels": 1080,
      "viewport_width_pixels": 3440, "viewport_height_pixels": 1880,
      "backend": "gstreamer", "decode_residency": "software",
      "decoder_factory": "avdec_h264", "decoder_plugin": "libav",
      "decoder_kind": "software", "gpu_adapter_name": "AMD Radeon RX 7900 XTX",
      "gpu_vendor": 4098, "gpu_device": 29631, "gpu_device_type": "DiscreteGpu",
      "graphics_backend": "vulkan", "gpu_driver": "radv",
      "gpu_driver_info": "Mesa test driver", "drm_render_node": "/dev/dri/renderD128",
      "display_refresh_hz": 60, "frame_format": "rgba8",
      "compositor_import": "cpu-upload", "presentation": "wgpu-composited",
      "decoded_frames": 2, "replaced_frames": 0, "late_dropped_frames": 0,
      "imported_frames": 1, "backpressured_frames": 0,
      "borrowed_native_frames": 0, "gpu_blit_frames": 0, "cpu_upload_frames": 1,
      "submitted_frames": 1, "presented_frames": 1, "interval_samples": 1,
      "interval_p50_us": 16667, "interval_p95_us": 16667,
      "interval_p99_us": 16667, "interval_max_us": 16667,
      "gpu_timing_status": "enabled", "gpu_pass_samples": 1,
      "gpu_pass_total_us": 500, "gpu_pass_min_us": 500, "gpu_pass_max_us": 500,
      "gpu_memory_bytes": 1, "pool_capacity": 1, "pool_allocations": 1,
      "pool_reuses": 0, "pool_backpressured_acquires": 0,
      "pool_in_flight_high_water": 1, "error": null
    }"#;

    let report = PerfHarness::new(scratch.path())
        .record_fixture_result(&request, raw_result)
        .expect("persist rejected native-video result");
    let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
        panic!("CPU upload fallback must reject a native-video sample")
    };
    assert!(mismatches.iter().any(|mismatch| {
        mismatch.invariant == "compositor-import"
            && mismatch.expected == "borrowed-native-surface"
            && mismatch.actual == "cpu-upload"
    }));
}

#[test]
fn run_persists_a_missing_editor_as_an_infrastructure_failure() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-run-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let missing_editor = workspace.path().join("missing-neomacs");
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        &missing_editor,
        NonZeroU32::new(1).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .run(&request)
        .expect("infrastructure failures are persisted reports");

    match &report.artifact.verdict {
        RunVerdict::InfrastructureFailure { message } => {
            assert!(message.contains("missing editor executable"));
            assert!(message.contains("missing-neomacs"));
        }
        verdict => panic!("expected infrastructure failure, got {verdict:?}"),
    }
    assert!(report.artifact.verdict.measurements().is_none());
    assert!(report.artifact_path.is_file());
}

#[test]
fn non_video_scenario_rejects_a_video_file_before_launching_editor() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-input-contract-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        "/missing/editor",
        NonZeroU32::new(1).expect("non-zero literal"),
    )
    .with_video_file(Some(PathBuf::from("unexpected.mp4")));

    let report = PerfHarness::new(workspace.path())
        .run(&request)
        .expect("input contract failure is persisted");
    let RunVerdict::InfrastructureFailure { message } = report.artifact.verdict else {
        panic!("unexpected video input must not launch an unrelated scenario")
    };
    assert!(message.contains("does not accept native-video input"));
}

#[cfg(unix)]
#[test]
fn pty_runner_publishes_the_raw_terminal_byte_stream() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch_root = workspace_root.join("tmp");
    fs::create_dir_all(&scratch_root).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-pty-test-")
        .tempdir_in(&scratch_root)
        .expect("create workspace-local PTY test directory");
    let sentinel = scratch.path().join("done");
    let terminal_bytes = scratch.path().join("terminal.ansi");

    let output = Command::new("python3")
        .arg(workspace_root.join("tools/bench/pty-run.py"))
        .args([
            "sh",
            "-c",
            r#"printf '\033[31mred\033[0m\n'; : > "$SENTINEL""#,
        ])
        .current_dir(&workspace_root)
        .env("SENTINEL", &sentinel)
        .env("PTY_OUTPUT", &terminal_bytes)
        .env("PTY_TIMEOUT", "5")
        .output()
        .expect("run deterministic PTY adapter");

    assert!(
        output.status.success(),
        "PTY adapter failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(&terminal_bytes).expect("read raw terminal artifact"),
        b"\x1b[31mred\x1b[0m\r\n"
    );
}

#[cfg(unix)]
#[test]
fn pty_runner_allows_a_profile_wrapper_to_finalize_after_the_sentinel() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch_root = workspace_root.join("tmp");
    fs::create_dir_all(&scratch_root).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-pty-finalize-test-")
        .tempdir_in(&scratch_root)
        .expect("create workspace-local PTY test directory");
    let sentinel = scratch.path().join("done");
    let finalized = scratch.path().join("profile-finalized");

    let output = Command::new("python3")
        .arg(workspace_root.join("tools/bench/pty-run.py"))
        .args([
            "sh",
            "-c",
            r##": > "$SENTINEL"; sleep 1; : > "$FINALIZED""##,
        ])
        .current_dir(&workspace_root)
        .env("SENTINEL", &sentinel)
        .env("FINALIZED", &finalized)
        .env("PTY_TIMEOUT", "5")
        .output()
        .expect("run PTY adapter around a finalizing wrapper");

    assert!(
        output.status.success(),
        "PTY adapter failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        finalized.is_file(),
        "PTY adapter killed the wrapper before profile finalization"
    );
}

#[test]
fn gui_runner_keeps_logs_workspace_local_and_owns_its_viewport_size() {
    let runner = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/tools/bench/gui-run.sh"
    ));
    assert!(!runner.contains(">/tmp/"));
    assert!(runner.contains("GUI_WESTON_LOG"));
    assert!(runner.contains("GUI_APP_LOG"));
    assert!(runner.contains("GUI_WIDTH"));
    assert!(runner.contains("GUI_HEIGHT"));
    assert!(runner.contains("--xwayland"));
    assert!(runner.contains("DISPLAY=\"$XWAYLAND_DISPLAY\""));
}

#[test]
fn mx_tab_fixture_uses_a_controlled_cross_editor_candidate_namespace() {
    let fixture = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/crates/neomacs-perf/fixtures/mx-tab-completion.el"
    ));
    assert!(fixture.contains("neomacs-perf-mx-tab--candidate-total 1024"));
    assert!(fixture.contains("neomacs-perf-command-%04d"));
    assert!(fixture.contains("neomacs-perf-command-0000"));
    assert!(fixture.contains("neomacs-perf-command-1023"));
}

#[test]
fn benchmark_environment_does_not_inherit_logging_or_allocator_controls() {
    let sandbox = neomacs_melpa_test_support::MelpaSandbox::new("perf-environment")
        .expect("create package sandbox");
    let mut command = Command::new("env");
    command
        .env("RUST_LOG", "debug")
        .env("MIMALLOC_ARENA_EAGER_COMMIT", "2")
        .env("NEOMACS_DUMP_FRAME_GLYPHS", "1");

    configure_benchmark_environment(&mut command, &sandbox);
    let output = command.output().expect("print benchmark environment");
    let environment = String::from_utf8(output.stdout).expect("environment is UTF-8");

    assert!(!environment.contains("RUST_LOG="));
    assert!(!environment.contains("MIMALLOC_ARENA_EAGER_COMMIT="));
    assert!(!environment.contains("NEOMACS_DUMP_FRAME_GLYPHS="));
    assert!(environment.contains("NEOMACS_TEST_SANDBOX_ROOT="));
}

#[cfg(unix)]
#[test]
fn editor_provenance_uses_content_and_pdump_fingerprints() {
    use std::os::unix::fs::PermissionsExt;

    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-editor-provenance-")
        .tempdir_in(workspace_root.join("tmp"))
        .expect("create workspace-local provenance scratch directory");
    let editor = scratch.path().join("fake-neomacs");
    fs::write(
        &editor,
        r##"#!/bin/sh
case "$1" in
  --fingerprint) printf '%s\n' 'PDUMP-FINGERPRINT' ;;
  --version) printf '%s\n' 'Neomacs test-build' ;;
  --batch) printf '%s' '0,1,1,1,0,1' ;;
  *) exit 64 ;;
esac
"##,
    )
    .expect("write fake editor");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let sandbox = neomacs_melpa_test_support::MelpaSandbox::new("editor-provenance")
        .expect("create package sandbox");

    let before = collect_editor_provenance(&editor, &sandbox).expect("collect editor identity");
    assert_eq!(before.pdump_fingerprint, "PDUMP-FINGERPRINT");
    assert_eq!(before.version, "Neomacs test-build");
    assert_eq!(before.kind, crate::EditorKind::Neomacs);
    assert_eq!(
        before.capabilities,
        crate::EditorCapabilities {
            native_compilation: false,
            tree_sitter: true,
            dynamic_modules: true,
            video_playback: true,
            webview: false,
            embedded_terminal: true,
        }
    );
    assert_eq!(before.executable_sha256.len(), 64);
    assert!(
        before
            .executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );

    let mut changed = fs::read(&editor).expect("read fake editor");
    changed.extend_from_slice(b"# changed build\n");
    fs::write(&editor, changed).expect("change fake editor contents");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755))
        .expect("keep fake editor executable");
    let after = collect_editor_provenance(&editor, &sandbox).expect("collect changed identity");
    assert_ne!(before.executable_sha256, after.executable_sha256);
}

#[test]
fn stale_harness_revision_cannot_be_attributed_to_the_current_checkout() {
    assert!(validate_harness_revision("same", "same").is_ok());
    let error = validate_harness_revision("old-commit", "new-commit")
        .expect_err("stale harness must be rejected");
    assert!(error.contains("old-commit"));
    assert!(error.contains("new-commit"));
    assert!(error.contains("rebuild"));
}

#[test]
fn harness_built_from_dirty_tracked_sources_cannot_be_acceptance_evidence() {
    assert!(validate_harness_build("same", "same", false).is_ok());
    let error = validate_harness_build("same", "same", true)
        .expect_err("dirty-built harness must be rejected after the checkout is restored");
    assert!(error.contains("dirty tracked harness inputs"));
    assert!(error.contains("rebuild"));
}

/// The editor child gets the allowlisted host variables plus any operator-set
/// `NEOVM_JIT_*` diagnostic knob, and nothing else.
#[test]
fn benchmark_environment_forwards_the_allowlist_and_jit_knobs_only() {
    use std::ffi::OsString;
    let os = |s: &str| OsString::from(s);
    let vars = vec![
        (os("PATH"), os("/bin")),
        (os("HOME"), os("/home/nobody")),
        (os("NEOVM_JIT_PROFILE"), os("/tmp/census.csv")),
        (os("NEOVM_JIT_THRESHOLD"), os("1")),
        (os("NEOVM_GC_TRACE"), os("1")),
        (os("RUST_LOG"), os("debug")),
    ];
    let mut forwarded: Vec<String> = super::harness::passthrough_from(vars)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    forwarded.sort();
    assert_eq!(
        forwarded,
        ["NEOVM_JIT_PROFILE", "NEOVM_JIT_THRESHOLD", "PATH"]
    );
}

#[test]
fn org_journal_open_result_is_valid_when_every_journal_invariant_holds() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-org-journal-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::OrgJournalOpen,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(3).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "org-journal-open",
              "status": "ok",
              "iterations": 3,
              "elapsed_us": 9000000,
              "elapsed_wall_us": 9100000,
              "operation_count": 3,
              "open_phase_us": 4000000,
              "fontify_phase_us": 3000000,
              "settle_phase_us": 2000000,
              "expected_major_mode": "org-journal-mode",
              "actual_major_mode": "org-journal-mode",
              "org_superstar_active": true,
              "git_gutter_active": true,
              "overlay_count_min": 2697,
              "overlay_count_final": 2697,
              "stable_checksum": true,
              "entry_created": true,
              "journal_line_count": 4340,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::Valid { measurements } = &report.artifact.verdict else {
        panic!("a complete journal-open result must be valid");
    };
    assert!(measurements
        .iter()
        .any(|measurement| measurement.name == MetricName::PerOperationWallTime));
    assert!(measurements
        .iter()
        .any(|measurement| measurement.name == MetricName::OverlayCount
            && measurement.value > 0.0));
}

#[test]
fn org_journal_open_rejects_a_journal_that_never_created_overlays_or_an_entry() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-org-journal-reject-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::OrgJournalOpen,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(3).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "org-journal-open",
              "status": "ok",
              "iterations": 3,
              "elapsed_us": 9000000,
              "elapsed_wall_us": 9100000,
              "operation_count": 3,
              "open_phase_us": 4000000,
              "fontify_phase_us": 3000000,
              "settle_phase_us": 2000000,
              "expected_major_mode": "org-journal-mode",
              "actual_major_mode": "org-mode",
              "org_superstar_active": true,
              "git_gutter_active": true,
              "overlay_count_min": 0,
              "overlay_count_final": 0,
              "stable_checksum": false,
              "entry_created": false,
              "journal_line_count": 4340,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::CorrectnessMismatch { mismatches } = &report.artifact.verdict else {
        panic!("a degenerate journal-open result must not be valid");
    };
    let invariants: Vec<&str> = mismatches
        .iter()
        .map(|mismatch| mismatch.invariant.as_str())
        .collect();
    for expected in [
        "major-mode",
        "stable-checksum",
        "entry-created",
        "overlay-count",
    ] {
        assert!(
            invariants.contains(&expected),
            "expected invariant {expected} to be rejected, got {invariants:?}"
        );
    }
}

#[test]
fn org_journal_open_relaxes_creation_invariants_for_an_external_journal() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-org-journal-external-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::OrgJournalOpen,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(2).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch)
    .with_journal_file(Some(workspace.path().join("2026-bug.org")));

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "org-journal-open",
              "status": "ok",
              "iterations": 2,
              "elapsed_us": 6000000,
              "elapsed_wall_us": 6100000,
              "operation_count": 2,
              "open_phase_us": 2500000,
              "fontify_phase_us": 2000000,
              "settle_phase_us": 1500000,
              "expected_major_mode": "org-journal-mode",
              "actual_major_mode": "org-journal-mode",
              "org_superstar_active": true,
              "git_gutter_active": true,
              "overlay_count_min": 0,
              "overlay_count_final": 0,
              "stable_checksum": true,
              "entry_created": false,
              "journal_line_count": 4340,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    assert!(
        report.artifact.verdict.is_valid(),
        "an external journal that already has today's entry is not a correctness failure"
    );
}

#[test]
fn synthetic_journal_generator_is_deterministic_and_heavier_than_the_real_workload() {
    use super::harness::{civil_from_days, days_from_civil, generate_synthetic_journal};

    // Cross-machine reproduction hinges on the constant seed: the same
    // elapsed-day count must always produce a byte-identical journal.
    let (first, entries_first) = generate_synthetic_journal(2026, 249);
    let (second, entries_second) = generate_synthetic_journal(2026, 249);
    assert_eq!(first, second);
    assert_eq!(entries_first, entries_second);

    // Heavier than the ~330 KB / ~4,330-line real workload this scenario
    // guards, so the font-lock, overlay, and marker costs are unmistakable
    // on any machine.
    let lines = first.lines().count() as u64;
    assert!(
        (5_000..8_000).contains(&lines),
        "journal should stay around ~5.5k-7k lines at 249 days, got {lines}"
    );
    assert!(
        (450_000..700_000).contains(&first.len()),
        "journal should stay around ~0.5-0.7 MB at 249 days, got {}",
        first.len()
    );
    assert!(
        (500..800).contains(&entries_first),
        "journal should carry ~600 timed entries at 249 days, got {entries_first}"
    );
    assert!(first.starts_with("* 2026-01-01, Thursday\n"));
    // org-journal's timed entry shape must be present for font-lock.
    assert!(first.contains("\n** "));
}

#[test]
fn civil_date_helpers_round_trip_across_the_scenario_year() {
    use super::harness::{civil_from_days, days_from_civil};

    assert_eq!(civil_from_days(0), (1970, 1, 1));
    for days in [
        days_from_civil(2026, 1, 1),
        days_from_civil(2026, 9, 6),
        days_from_civil(2024, 2, 29),
        days_from_civil(2025, 12, 31),
    ] {
        let (year, month, mday) = civil_from_days(days);
        assert_eq!(days_from_civil(year, month, mday), days);
    }
}
