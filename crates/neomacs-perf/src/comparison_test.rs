use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::PathBuf;

use super::{
    COMPARISON_ARTIFACT_SCHEMA_VERSION, ComparisonArtifact, ComparisonInput, ComparisonObservation,
    ComparisonRejection, ComparisonRun, ComparisonRunOutcome, ComparisonRunRole,
    ComparisonSampleCount, ComparisonVerdict, CorrectnessMismatch, EditorProvenance, Frontend,
    Measurement, MetricName, MetricUnit, NativeVideoComparisonIdentity, RunVerdict, ScenarioId,
    comparison_schedule, evaluate_comparison,
};
use crate::native_video::{
    NativeVideoDecoderKind, NativeVideoExecutionIdentity, NativeVideoFrameFormat,
    NativeVideoFrameRate, NativeVideoGpuTimingStatus, NativeVideoGraphicsBackend,
    NativeVideoMediaMetadata,
};

fn editor(role: ComparisonRunRole) -> PathBuf {
    match role {
        ComparisonRunRole::Baseline => PathBuf::from("target/release/neomacs"),
        ComparisonRunRole::Candidate => PathBuf::from("target/release-pgo/neomacs"),
    }
}

fn provenance(role: ComparisonRunRole) -> EditorProvenance {
    EditorProvenance {
        path: format!("/repo/{}", editor(role).display()),
        executable_sha256: format!("{role:?}-sha256"),
        executable_size_bytes: 42,
        pdump_fingerprint: format!("{role:?}-pdump"),
        version: "Neomacs test-build".to_string(),
        kind: crate::EditorKind::Neomacs,
        capabilities: crate::EditorCapabilities {
            native_compilation: false,
            tree_sitter: true,
            dynamic_modules: true,
            video_playback: true,
            webview: false,
            embedded_terminal: true,
        },
    }
}

fn valid_observation(
    role: ComparisonRunRole,
    sample_index: u32,
    value: f64,
) -> ComparisonObservation {
    ComparisonObservation {
        run: ComparisonRun {
            role,
            sample_index,
            run_id: format!("{role:?}-{sample_index}"),
            artifact_path: PathBuf::from(format!("{role:?}-{sample_index}/artifact.json")),
            scenario: ScenarioId::RustLspTyping,
            frontend: Frontend::Batch,
            editor: editor(role),
            iterations: 10,
            editor_provenance: Some(provenance(role)),
            native_video_input: None,
            native_video_execution: None,
            outcome: ComparisonRunOutcome::Valid,
        },
        verdict: RunVerdict::Valid {
            measurements: vec![Measurement {
                name: MetricName::PerEditCpuTime,
                value,
                unit: MetricUnit::MicrosecondsPerEdit,
            }],
        },
    }
}

fn native_video_identity(digest: &str) -> NativeVideoComparisonIdentity {
    NativeVideoComparisonIdentity {
        workload_fixture_sha256: "fixture-sha256".to_owned(),
        video_file_sha256: digest.to_owned(),
        video_file_size_bytes: 4096,
        media: NativeVideoMediaMetadata {
            width_pixels: 3840,
            height_pixels: 2160,
            frame_rate: NativeVideoFrameRate {
                numerator: 60,
                denominator: 1,
            },
            codec_caps: "video/x-h264".to_owned(),
        },
        presentation_width_pixels: 1920,
        presentation_height_pixels: 1080,
        display_environment: BTreeMap::from([("DISPLAY".to_owned(), ":0".to_owned())]),
        gstreamer_environment: BTreeMap::from([(
            "GST_PLUGIN_SYSTEM_PATH_1_0".to_owned(),
            "/nix/store/plugins".to_owned(),
        )]),
        gpu_frame_timing: "requested".to_owned(),
    }
}

fn native_video_execution() -> NativeVideoExecutionIdentity {
    NativeVideoExecutionIdentity {
        decoder_factory: "vah264dec".to_owned(),
        decoder_plugin: "va".to_owned(),
        decoder_kind: NativeVideoDecoderKind::Hardware,
        gpu_adapter_name: "AMD Radeon".to_owned(),
        gpu_vendor: 0x1002,
        gpu_device: 0x1234,
        gpu_device_type: "discrete-gpu".to_owned(),
        graphics_backend: NativeVideoGraphicsBackend::Vulkan,
        gpu_driver: "radv".to_owned(),
        gpu_driver_info: "Mesa".to_owned(),
        drm_render_node: Some("/dev/dri/renderD128".to_owned()),
        display_refresh_hz: Some(60),
        frame_format: NativeVideoFrameFormat::Nv12,
        gpu_timing_status: NativeVideoGpuTimingStatus::Enabled,
    }
}

fn native_video_comparison() -> (ComparisonInput, Vec<ComparisonObservation>) {
    let mut input = input();
    input.scenario = ScenarioId::SustainedNativeVideo;
    input.frontend = Frontend::Gui {
        width: 1920,
        height: 1080,
    };
    input.primary_metric = MetricName::VideoPresentationFramesPerSecond;
    input.video_file = Some(PathBuf::from("video.mp4"));
    let mut observations = valid_observations();
    for observation in &mut observations {
        observation.run.scenario = input.scenario;
        observation.run.frontend = input.frontend;
        observation.run.native_video_input = Some(native_video_identity("video-sha256"));
        observation.run.native_video_execution = Some(native_video_execution());
        let RunVerdict::Valid { measurements } = &mut observation.verdict else {
            unreachable!("fixture uses valid runs")
        };
        measurements[0] = Measurement {
            name: input.primary_metric,
            value: 60.0,
            unit: MetricUnit::FramesPerSecond,
        };
    }
    (input, observations)
}

#[test]
fn native_video_comparison_rejects_changed_execution_path() {
    for change in ["decoder", "render-node", "frame-format", "gpu-timing"] {
        let (input, mut observations) = native_video_comparison();
        let identity = observations[2]
            .run
            .native_video_execution
            .as_mut()
            .expect("fixture execution identity");
        match change {
            "decoder" => identity.decoder_factory = "nvh264dec".to_owned(),
            "render-node" => {
                identity.drm_render_node = Some("/dev/dri/renderD129".to_owned());
            }
            "frame-format" => identity.frame_format = NativeVideoFrameFormat::P010,
            "gpu-timing" => {
                identity.gpu_timing_status = NativeVideoGpuTimingStatus::Unsupported;
            }
            _ => unreachable!(),
        }

        let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations)
        else {
            panic!("native-video comparison accepted a changed {change}")
        };
        assert!(reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::NativeVideoExecutionMismatch { .. }
        )));
    }
}

fn sample_count(value: u32) -> ComparisonSampleCount {
    ComparisonSampleCount::new(value).expect("valid comparison sample count")
}

fn input() -> ComparisonInput {
    ComparisonInput {
        scenario: ScenarioId::RustLspTyping,
        frontend: Frontend::Batch,
        iterations: NonZeroU32::new(10).expect("non-zero iterations"),
        samples_per_side: sample_count(3),
        primary_metric: MetricName::PerEditCpuTime,
        baseline_editor: editor(ComparisonRunRole::Baseline),
        candidate_editor: editor(ComparisonRunRole::Candidate),
        machine: crate::MachinePolicy::default(),
        counters: None,
        video_file: None,
        journal_file: None,
    }
}

fn valid_observations() -> Vec<ComparisonObservation> {
    vec![
        valid_observation(ComparisonRunRole::Baseline, 0, 10.0),
        valid_observation(ComparisonRunRole::Candidate, 0, 9.0),
        valid_observation(ComparisonRunRole::Candidate, 1, 12.0),
        valid_observation(ComparisonRunRole::Baseline, 1, 30.0),
        valid_observation(ComparisonRunRole::Baseline, 2, 20.0),
        valid_observation(ComparisonRunRole::Candidate, 2, 15.0),
    ]
}

#[test]
fn comparison_artifact_round_trips_every_underlying_run_link() {
    let observations = valid_observations();
    let runs = observations
        .iter()
        .map(|observation| observation.run.clone())
        .collect();
    let artifact = ComparisonArtifact {
        schema_version: COMPARISON_ARTIFACT_SCHEMA_VERSION,
        comparison_id: "rust-lsp-typing-compare-42".to_string(),
        input: input(),
        started_unix_ms: 42,
        total_elapsed_us: 900,
        verdict: evaluate_comparison(&input(), &observations),
        runs,
    };

    let json = serde_json::to_string_pretty(&artifact).expect("serialize comparison artifact");
    let decoded: ComparisonArtifact =
        serde_json::from_str(&json).expect("deserialize comparison artifact");

    assert_eq!(decoded, artifact);
    assert!(json.contains(r##""candidate_to_baseline_ratio": 0.6"##));
    assert!(json.contains("Candidate-0/artifact.json"));
    assert!(!json.contains(r##""measurements""##));
}

#[test]
fn comparison_schedule_alternates_which_editor_runs_first() {
    assert_eq!(
        comparison_schedule(sample_count(3)),
        vec![
            (ComparisonRunRole::Baseline, 0),
            (ComparisonRunRole::Candidate, 0),
            (ComparisonRunRole::Candidate, 1),
            (ComparisonRunRole::Baseline, 1),
            (ComparisonRunRole::Baseline, 2),
            (ComparisonRunRole::Candidate, 2),
        ]
    );
}

#[test]
fn comparison_input_deserialization_rejects_fewer_than_three_samples() {
    let mut json = serde_json::to_value(input()).expect("serialize comparison input");
    json["samples_per_side"] = serde_json::json!(2);

    let error = serde_json::from_value::<ComparisonInput>(json)
        .expect_err("unchecked sample count crossed the artifact boundary");

    assert!(error.to_string().contains("at least 3"));
}

#[test]
fn valid_repetitions_produce_medians_and_candidate_ratio() {
    let verdict = evaluate_comparison(&input(), &valid_observations());

    assert_eq!(
        verdict,
        ComparisonVerdict::Valid {
            summary: super::ComparisonMetricSummary {
                metric: MetricName::PerEditCpuTime,
                unit: MetricUnit::MicrosecondsPerEdit,
                baseline_samples: vec![10.0, 20.0, 30.0],
                candidate_samples: vec![9.0, 12.0, 15.0],
                baseline_median: 20.0,
                candidate_median: 12.0,
                baseline_median_absolute_deviation: 10.0,
                candidate_median_absolute_deviation: 3.0,
                candidate_to_baseline_ratio: 0.6,
                percent_change: -40.0,
            }
        }
    );
}

#[test]
fn cross_editor_completion_candidate_count_mismatch_rejects_comparison() {
    let mut input = input();
    input.scenario = ScenarioId::MxTabCompletion;
    input.primary_metric = MetricName::PerCompletionCpuTime;
    let mut observations = valid_observations();
    for observation in &mut observations {
        observation.run.scenario = ScenarioId::MxTabCompletion;
        let RunVerdict::Valid { measurements } = &mut observation.verdict else {
            unreachable!("fixture uses valid runs")
        };
        measurements[0].name = MetricName::PerCompletionCpuTime;
        measurements[0].unit = MetricUnit::MicrosecondsPerCompletion;
        measurements.push(Measurement {
            name: MetricName::CompletionCandidateCount,
            value: match observation.run.role {
                ComparisonRunRole::Baseline => 3_114.0,
                ComparisonRunRole::Candidate => 3_118.0,
            },
            unit: MetricUnit::Count,
        });
    }

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations) else {
        panic!("cross-editor candidate divergence was accepted")
    };
    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::CrossEditorParityMismatch {
            metric: MetricName::CompletionCandidateCount,
            baseline_values,
            candidate_values,
        } if baseline_values == &["3114", "3114", "3114"]
            && candidate_values == &["3118", "3118", "3118"]
    )));
}

#[test]
fn one_correctness_mismatch_rejects_without_serializing_partial_measurements() {
    let mut observations = valid_observations();
    let mismatch = &mut observations[1];
    mismatch.run.outcome = ComparisonRunOutcome::CorrectnessMismatch;
    mismatch.verdict = RunVerdict::CorrectnessMismatch {
        mismatches: vec![CorrectnessMismatch {
            invariant: "final-buffer-text".to_string(),
            expected: "true".to_string(),
            actual: "false".to_string(),
        }],
    };

    let verdict = evaluate_comparison(&input(), &observations);
    assert!(matches!(verdict, ComparisonVerdict::Rejected { .. }));
    let artifact = ComparisonArtifact {
        schema_version: COMPARISON_ARTIFACT_SCHEMA_VERSION,
        comparison_id: "rejected".to_string(),
        input: input(),
        started_unix_ms: 42,
        total_elapsed_us: 900,
        verdict,
        runs: observations
            .into_iter()
            .map(|observation| observation.run)
            .collect(),
    };
    let json = serde_json::to_string(&artifact).expect("serialize rejected comparison");

    assert!(!json.contains("measurements"));
    assert!(!json.contains("baseline_samples"));
    assert!(!json.contains("candidate_samples"));
}

#[test]
fn canonical_metric_unit_is_required_even_when_every_sample_agrees() {
    let mut observations = valid_observations();
    for observation in &mut observations {
        let RunVerdict::Valid { measurements } = &mut observation.verdict else {
            unreachable!("fixture uses valid runs")
        };
        measurements[0].unit = MetricUnit::Count;
    }

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input(), &observations)
    else {
        panic!("non-canonical units were accepted")
    };

    assert!(reasons.iter().all(|reason| matches!(
        reason,
        ComparisonRejection::MetricUnitMismatch {
            expected: MetricUnit::MicrosecondsPerEdit,
            actual: MetricUnit::Count,
            ..
        }
    )));
}

#[test]
fn malformed_non_primary_measurements_reject_the_whole_comparison() {
    let mut observations = valid_observations();
    let RunVerdict::Valid { measurements } = &mut observations[0].verdict else {
        unreachable!("fixture uses valid runs")
    };
    measurements.extend([
        Measurement {
            name: MetricName::OverlayCount,
            value: -1.0,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: 10.0,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::OverlayCount,
            value: 4.0,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::LspDiagnosticCount,
            value: 1.5,
            unit: MetricUnit::Count,
        },
    ]);

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input(), &observations)
    else {
        panic!("malformed non-primary measurements were ignored")
    };

    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::InvalidMetricValue {
            metric: MetricName::OverlayCount,
            ..
        }
    )));
    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::InvalidMetricValue {
            metric: MetricName::LspDiagnosticCount,
            ..
        }
    )));
    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::MetricUnitMismatch {
            expected: MetricUnit::Count,
            actual: MetricUnit::Microseconds,
            ..
        }
    )));
    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::DuplicateMetric {
            metric: MetricName::OverlayCount,
            ..
        }
    )));
}

#[test]
fn child_run_metadata_must_match_the_comparison_input() {
    let mut observations = valid_observations();
    observations[0].run.iterations = 11;

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input(), &observations)
    else {
        panic!("mismatched child metadata was accepted")
    };

    assert!(reasons.contains(&ComparisonRejection::IterationsMismatch {
        role: ComparisonRunRole::Baseline,
        sample_index: 0,
        run_id: "Baseline-0".to_string(),
        expected: 10,
        actual: 11,
    }));
}

#[test]
fn one_editor_build_cannot_change_between_samples() {
    let mut observations = valid_observations();
    observations[3]
        .run
        .editor_provenance
        .as_mut()
        .expect("fixture provenance")
        .executable_sha256 = "rebuilt-between-samples".to_string();

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input(), &observations)
    else {
        panic!("changing editor build was accepted")
    };

    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::EditorProvenanceMismatch {
            role: ComparisonRunRole::Baseline,
            sample_index: 1,
            ..
        }
    )));
}

#[test]
fn one_child_run_cannot_be_reused_as_two_samples() {
    let mut observations = valid_observations();
    observations[3].run.run_id = observations[0].run.run_id.clone();
    observations[3].run.artifact_path = observations[0].run.artifact_path.clone();

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input(), &observations)
    else {
        panic!("reused child run was accepted")
    };

    assert!(reasons.contains(&ComparisonRejection::DuplicateRunId {
        run_id: "Baseline-0".to_string(),
    }));
    assert!(
        reasons.contains(&ComparisonRejection::DuplicateArtifactPath {
            artifact_path: PathBuf::from("Baseline-0/artifact.json"),
        })
    );
}

#[test]
fn native_video_comparison_requires_input_identity_for_every_child() {
    let (input, mut observations) = native_video_comparison();
    observations[2].run.native_video_input = None;

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations) else {
        panic!("native-video comparison accepted a child with no input identity")
    };

    assert!(
        reasons.contains(&ComparisonRejection::MissingNativeVideoInput {
            role: ComparisonRunRole::Candidate,
            sample_index: 1,
            run_id: "Candidate-1".to_owned(),
        })
    );
}

#[test]
fn native_video_comparison_rejects_changed_input_digest_between_samples() {
    let (input, mut observations) = native_video_comparison();
    observations[3]
        .run
        .native_video_input
        .as_mut()
        .expect("fixture input identity")
        .video_file_sha256 = "changed-video-sha256".to_owned();

    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations) else {
        panic!("native-video comparison accepted different compressed input bytes")
    };

    assert!(reasons.iter().any(|reason| matches!(
        reason,
        ComparisonRejection::NativeVideoInputMismatch {
            role: ComparisonRunRole::Baseline,
            sample_index: 1,
            ..
        }
    )));
}

#[test]
fn native_video_identity_is_content_based_not_path_based() {
    let first_path = PathBuf::from("fixtures/original.mp4");
    let second_path = PathBuf::from("target/staged-copy.mp4");

    assert_ne!(first_path, second_path);
    assert_eq!(
        native_video_identity("same-content"),
        native_video_identity("same-content")
    );
}
