//! The sustained-native-video scenario: decode, zero-copy import, GPU
//! composition, and pacing on the caller's physical Linux display.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorKind, EditorProvenance, Frontend, HarnessProvenance, HostProvenance,
    Measurement, MetricName, MetricUnit, PreparedScenario, PreparedWorkload, RunRequest,
    ScenarioId, ScenarioOutcome, ScenarioStatus, collect_editor_provenance,
    collect_harness_provenance, collect_host_provenance, deserialize_optional_error, mismatch,
    prepare_gui_runtime_directory, require_positive_phase, scenario_outcome, sha256_file,
};
const NATIVE_VIDEO_RESULT_SCHEMA_VERSION: u32 = 4;

use crate::native_video::{
    NativeVideoBuildProfile, NativeVideoComparisonIdentity, NativeVideoDecoderKind,
    NativeVideoExecutionIdentity, NativeVideoFrameFormat, NativeVideoGpuTimingStatus,
    NativeVideoGraphicsBackend, NativeVideoPresentationTarget, discover_media_metadata,
};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    if !cfg!(target_os = "linux") {
        return Err(
            "sustained native-video performance is currently a Linux acceptance scenario"
                .to_string(),
        );
    }
    let presentation_target = NativeVideoPresentationTarget::from_frontend(request.frontend())
        .map_err(|error| error.to_string())?;
    let video_file = request.video_file().ok_or_else(|| {
        "sustained-native-video requires --video-file pointing to a readable video".to_string()
    })?;
    if !video_file.is_file() {
        return Err(format!(
            "native-video input is not a file: {}",
            video_file.display()
        ));
    }
    let video_file = fs::canonicalize(video_file).map_err(|error| {
        format!(
            "failed to resolve native-video input {}: {error}",
            video_file.display()
        )
    })?;
    let media = discover_media_metadata(&video_file)?;
    media.validate_4k60()?;
    let display_environment: BTreeMap<String, String> = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XAUTHORITY",
    ]
    .into_iter()
    .filter_map(|name| {
        std::env::var_os(name).map(|value| (name.to_string(), value.to_string_lossy().into_owned()))
    })
    .collect();
    let gstreamer_environment: BTreeMap<String, String> =
        ["GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SCANNER_1_0"]
            .into_iter()
            .filter_map(|name| {
                std::env::var_os(name)
                    .map(|value| (name.to_string(), value.to_string_lossy().into_owned()))
            })
            .collect();
    if !display_environment.contains_key("DISPLAY")
        && !display_environment.contains_key("WAYLAND_DISPLAY")
    {
        return Err(
            "sustained native-video performance requires the caller's graphical session"
                .to_string(),
        );
    }

    let sandbox = MelpaSandbox::new("perf-sustained-native-video")?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    if editor.kind != EditorKind::Neomacs {
        return Err("sustained native-video performance requires a Neomacs executable".into());
    }
    if !editor.capabilities.video_playback {
        return Err("Neomacs was built without native video playback support".into());
    }
    let build_profile = NativeVideoBuildProfile::from_version(&editor.version)?;
    let fixture_source =
        workspace_root.join("crates/neomacs-perf/fixtures/sustained-native-video.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("sustained-native-video.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let metadata = fs::metadata(&video_file).map_err(|error| {
        format!(
            "failed to inspect native-video input {}: {error}",
            video_file.display()
        )
    })?;
    let video_file_sha256 = sha256_file(&video_file)?;
    let harness = collect_harness_provenance(&workspace_root)?;
    if harness.source_tree_dirty {
        return Err(
            "sustained native-video acceptance requires a clean tracked source tree".to_owned(),
        );
    }
    let comparison_identity = NativeVideoComparisonIdentity {
        workload_fixture_sha256: sha256_file(&fixture_source)?,
        video_file_sha256: video_file_sha256.clone(),
        video_file_size_bytes: metadata.len(),
        media,
        presentation_width_pixels: presentation_target.width(),
        presentation_height_pixels: presentation_target.height(),
        display_environment: display_environment.clone(),
        gstreamer_environment,
        gpu_frame_timing: "requested".to_owned(),
    };
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = NativeVideoInputProvenanceManifest {
        editor,
        editor_build_profile: build_profile,
        host: collect_host_provenance(request.machine_policy()),
        harness,
        video_file: video_file.to_string_lossy().into_owned(),
        comparison_identity,
    };
    let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
        .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
    fs::write(&provenance, provenance_json).map_err(|error| {
        format!(
            "failed to write input provenance {}: {error}",
            provenance.display()
        )
    })?;

    Ok(PreparedScenario {
        fixture,
        provenance,
        result: run_directory.join("scenario-result.json"),
        sentinel: run_directory.join("completed"),
        terminal_bytes: run_directory.join("terminal.ansi"),
        gui_app_log: run_directory.join("gui-app.log"),
        gui_weston_log: run_directory.join("weston.log"),
        gui_runtime_directory: prepare_gui_runtime_directory(&workspace_root)?,
        sandbox,
        workload: PreparedWorkload::NativeVideo {
            video_file,
            video_file_sha256,
            video_file_size_bytes: metadata.len(),
            display_environment,
            presentation_target,
        },
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkBackend {
    Gstreamer,
}

impl std::fmt::Display for VideoBenchmarkBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Gstreamer => "gstreamer",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkImport {
    BorrowedNativeSurface,
    GpuBlit,
    CpuUpload,
}

impl std::fmt::Display for VideoBenchmarkImport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::BorrowedNativeSurface => "borrowed-native-surface",
            Self::GpuBlit => "gpu-blit",
            Self::CpuUpload => "cpu-upload",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkPresentation {
    WgpuComposited,
}

impl std::fmt::Display for VideoBenchmarkPresentation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("wgpu-composited")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkDecodeResidency {
    HardwareDecoderReportsRendererDevice,
    HardwareUnverified,
    Software,
    Unknown,
}

impl std::fmt::Display for VideoBenchmarkDecodeResidency {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::HardwareDecoderReportsRendererDevice => {
                "hardware-decoder-reports-renderer-device"
            }
            Self::HardwareUnverified => "hardware-unverified",
            Self::Software => "software",
            Self::Unknown => "unknown",
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "SustainedNativeVideoResultWire")]
pub(crate) struct SustainedNativeVideoResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_cpu_us: u64,
    elapsed_wall_us: u64,
    presentation_target: NativeVideoPresentationTarget,
    viewport_width_pixels: u32,
    viewport_height_pixels: u32,
    backend: VideoBenchmarkBackend,
    decode_residency: VideoBenchmarkDecodeResidency,
    decoder_factory: String,
    decoder_plugin: String,
    decoder_kind: NativeVideoDecoderKind,
    gpu_adapter_name: String,
    gpu_vendor: u32,
    gpu_device: u32,
    gpu_device_type: String,
    graphics_backend: NativeVideoGraphicsBackend,
    gpu_driver: String,
    gpu_driver_info: String,
    drm_render_node: Option<String>,
    display_refresh_hz: Option<u16>,
    frame_format: NativeVideoFrameFormat,
    compositor_import: VideoBenchmarkImport,
    presentation: VideoBenchmarkPresentation,
    decoded_frames: u64,
    replaced_frames: u64,
    late_dropped_frames: u64,
    imported_frames: u64,
    backpressured_frames: u64,
    borrowed_native_frames: u64,
    gpu_blit_frames: u64,
    cpu_upload_frames: u64,
    submitted_frames: u64,
    presented_frames: u64,
    interval_samples: u64,
    interval_p50_us: u64,
    interval_p95_us: u64,
    interval_p99_us: u64,
    interval_max_us: u64,
    gpu_timing_status: NativeVideoGpuTimingStatus,
    gpu_pass_samples: u64,
    gpu_pass_total_us: u64,
    gpu_pass_min_us: Option<u64>,
    gpu_pass_max_us: Option<u64>,
    gpu_memory_bytes: u64,
    pool_capacity: u64,
    pool_allocations: u64,
    pool_reuses: u64,
    pool_backpressured_acquires: u64,
    pool_in_flight_high_water: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SustainedNativeVideoResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_cpu_us: u64,
    elapsed_wall_us: u64,
    presentation_width_pixels: u32,
    presentation_height_pixels: u32,
    viewport_width_pixels: u32,
    viewport_height_pixels: u32,
    backend: VideoBenchmarkBackend,
    decode_residency: VideoBenchmarkDecodeResidency,
    decoder_factory: String,
    decoder_plugin: String,
    decoder_kind: NativeVideoDecoderKind,
    gpu_adapter_name: String,
    gpu_vendor: u32,
    gpu_device: u32,
    gpu_device_type: String,
    graphics_backend: NativeVideoGraphicsBackend,
    gpu_driver: String,
    gpu_driver_info: String,
    drm_render_node: Option<String>,
    display_refresh_hz: Option<u16>,
    frame_format: NativeVideoFrameFormat,
    compositor_import: VideoBenchmarkImport,
    presentation: VideoBenchmarkPresentation,
    decoded_frames: u64,
    replaced_frames: u64,
    late_dropped_frames: u64,
    imported_frames: u64,
    backpressured_frames: u64,
    borrowed_native_frames: u64,
    gpu_blit_frames: u64,
    cpu_upload_frames: u64,
    submitted_frames: u64,
    presented_frames: u64,
    interval_samples: u64,
    interval_p50_us: u64,
    interval_p95_us: u64,
    interval_p99_us: u64,
    interval_max_us: u64,
    gpu_timing_status: NativeVideoGpuTimingStatus,
    gpu_pass_samples: u64,
    gpu_pass_total_us: u64,
    gpu_pass_min_us: Option<u64>,
    gpu_pass_max_us: Option<u64>,
    gpu_memory_bytes: u64,
    pool_capacity: u64,
    pool_allocations: u64,
    pool_reuses: u64,
    pool_backpressured_acquires: u64,
    pool_in_flight_high_water: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<SustainedNativeVideoResultWire> for SustainedNativeVideoResult {
    type Error = String;

    fn try_from(wire: SustainedNativeVideoResultWire) -> Result<Self, Self::Error> {
        let presentation_target = NativeVideoPresentationTarget::from_frontend(Frontend::Gui {
            width: wire.presentation_width_pixels,
            height: wire.presentation_height_pixels,
        })
        .map_err(|error| error.to_string())?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome: scenario_outcome(wire.status, wire.error)?,
            iterations: wire.iterations,
            elapsed_cpu_us: wire.elapsed_cpu_us,
            elapsed_wall_us: wire.elapsed_wall_us,
            presentation_target,
            viewport_width_pixels: wire.viewport_width_pixels,
            viewport_height_pixels: wire.viewport_height_pixels,
            backend: wire.backend,
            decode_residency: wire.decode_residency,
            decoder_factory: wire.decoder_factory,
            decoder_plugin: wire.decoder_plugin,
            decoder_kind: wire.decoder_kind,
            gpu_adapter_name: wire.gpu_adapter_name,
            gpu_vendor: wire.gpu_vendor,
            gpu_device: wire.gpu_device,
            gpu_device_type: wire.gpu_device_type,
            graphics_backend: wire.graphics_backend,
            gpu_driver: wire.gpu_driver,
            gpu_driver_info: wire.gpu_driver_info,
            drm_render_node: wire.drm_render_node,
            display_refresh_hz: wire.display_refresh_hz,
            frame_format: wire.frame_format,
            compositor_import: wire.compositor_import,
            presentation: wire.presentation,
            decoded_frames: wire.decoded_frames,
            replaced_frames: wire.replaced_frames,
            late_dropped_frames: wire.late_dropped_frames,
            imported_frames: wire.imported_frames,
            backpressured_frames: wire.backpressured_frames,
            borrowed_native_frames: wire.borrowed_native_frames,
            gpu_blit_frames: wire.gpu_blit_frames,
            cpu_upload_frames: wire.cpu_upload_frames,
            submitted_frames: wire.submitted_frames,
            presented_frames: wire.presented_frames,
            interval_samples: wire.interval_samples,
            interval_p50_us: wire.interval_p50_us,
            interval_p95_us: wire.interval_p95_us,
            interval_p99_us: wire.interval_p99_us,
            interval_max_us: wire.interval_max_us,
            gpu_timing_status: wire.gpu_timing_status,
            gpu_pass_samples: wire.gpu_pass_samples,
            gpu_pass_total_us: wire.gpu_pass_total_us,
            gpu_pass_min_us: wire.gpu_pass_min_us,
            gpu_pass_max_us: wire.gpu_pass_max_us,
            gpu_memory_bytes: wire.gpu_memory_bytes,
            pool_capacity: wire.pool_capacity,
            pool_allocations: wire.pool_allocations,
            pool_reuses: wire.pool_reuses,
            pool_backpressured_acquires: wire.pool_backpressured_acquires,
            pool_in_flight_high_water: wire.pool_in_flight_high_water,
        })
    }
}

#[derive(Serialize)]
struct NativeVideoInputProvenanceManifest {
    editor: EditorProvenance,
    editor_build_profile: NativeVideoBuildProfile,
    host: HostProvenance,
    harness: HarnessProvenance,
    video_file: String,
    comparison_identity: NativeVideoComparisonIdentity,
}

pub(crate) fn validate_sustained_native_video_result(
    request: &RunRequest,
    result: &SustainedNativeVideoResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        NATIVE_VIDEO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    let requested_target = NativeVideoPresentationTarget::from_frontend(request.frontend())
        .expect("native-video preparation rejects a non-GUI or zero-sized frontend");
    mismatch(
        &mut mismatches,
        "presentation-width",
        requested_target.width(),
        result.presentation_target.width(),
    );
    mismatch(
        &mut mismatches,
        "presentation-height",
        requested_target.height(),
        result.presentation_target.height(),
    );
    if result.viewport_width_pixels < result.presentation_target.width()
        || result.viewport_height_pixels < result.presentation_target.height()
    {
        mismatches.push(CorrectnessMismatch {
            invariant: "presentation-fits-viewport".to_string(),
            expected: format!(
                "viewport at least {}x{}",
                result.presentation_target.width(),
                result.presentation_target.height()
            ),
            actual: format!(
                "{}x{}",
                result.viewport_width_pixels, result.viewport_height_pixels
            ),
        });
    }
    mismatch(
        &mut mismatches,
        "backend",
        VideoBenchmarkBackend::Gstreamer,
        result.backend,
    );
    mismatch(
        &mut mismatches,
        "decode-residency",
        VideoBenchmarkDecodeResidency::HardwareDecoderReportsRendererDevice,
        result.decode_residency,
    );
    mismatch(
        &mut mismatches,
        "decoder-kind",
        NativeVideoDecoderKind::Hardware,
        result.decoder_kind,
    );
    for (name, value) in [
        ("decoder-factory", result.decoder_factory.as_str()),
        ("decoder-plugin", result.decoder_plugin.as_str()),
        ("gpu-adapter-name", result.gpu_adapter_name.as_str()),
        ("gpu-device-type", result.gpu_device_type.as_str()),
        ("gpu-driver", result.gpu_driver.as_str()),
    ] {
        if value.is_empty() {
            mismatches.push(CorrectnessMismatch {
                invariant: name.to_owned(),
                expected: "non-empty".to_owned(),
                actual: "empty".to_owned(),
            });
        }
    }
    if result.decoder_plugin == "unknown" {
        mismatches.push(CorrectnessMismatch {
            invariant: "decoder-plugin".to_owned(),
            expected: "identified GStreamer plugin".to_owned(),
            actual: "unknown".to_owned(),
        });
    }
    mismatch(
        &mut mismatches,
        "graphics-backend",
        NativeVideoGraphicsBackend::Vulkan,
        result.graphics_backend,
    );
    require_positive_phase(&mut mismatches, "gpu-vendor", u64::from(result.gpu_vendor));
    require_positive_phase(&mut mismatches, "gpu-device", u64::from(result.gpu_device));
    if result.drm_render_node.is_none() {
        mismatches.push(CorrectnessMismatch {
            invariant: "drm-render-node".to_owned(),
            expected: "identified Linux render node".to_owned(),
            actual: "unknown".to_owned(),
        });
    }
    if result.display_refresh_hz.is_none_or(|rate| rate == 0) {
        mismatches.push(CorrectnessMismatch {
            invariant: "display-refresh-rate".to_owned(),
            expected: "positive reported or bounded fallback rate".to_owned(),
            actual: format!("{:?}", result.display_refresh_hz),
        });
    }
    // Some wgpu backends expose no supplementary driver-info string; the
    // field remains recorded for adapters that do.
    let _gpu_driver_info = &result.gpu_driver_info;
    if !matches!(
        result.frame_format,
        NativeVideoFrameFormat::Nv12 | NativeVideoFrameFormat::P010
    ) {
        mismatches.push(CorrectnessMismatch {
            invariant: "frame-format".to_string(),
            expected: "nv12 or p010".to_string(),
            actual: result.frame_format.to_string(),
        });
    }
    mismatch(
        &mut mismatches,
        "compositor-import",
        VideoBenchmarkImport::BorrowedNativeSurface,
        result.compositor_import,
    );
    mismatch(
        &mut mismatches,
        "presentation",
        VideoBenchmarkPresentation::WgpuComposited,
        result.presentation,
    );
    mismatch(
        &mut mismatches,
        "borrowed-import-count",
        result.imported_frames,
        result.borrowed_native_frames,
    );
    for (name, value) in [
        ("elapsed-cpu-time", result.elapsed_cpu_us),
        ("elapsed-wall-time", result.elapsed_wall_us),
        ("decoded-frames", result.decoded_frames),
        ("imported-frames", result.imported_frames),
        ("submitted-frames", result.submitted_frames),
        ("presented-frames", result.presented_frames),
        ("presentation-interval-samples", result.interval_samples),
        ("presentation-p50", result.interval_p50_us),
        ("presentation-p95", result.interval_p95_us),
        ("presentation-p99", result.interval_p99_us),
        ("presentation-max", result.interval_max_us),
        ("gpu-memory", result.gpu_memory_bytes),
        ("surface-pool-capacity", result.pool_capacity),
        ("surface-pool-reuses", result.pool_reuses),
        (
            "surface-pool-in-flight-high-water",
            result.pool_in_flight_high_water,
        ),
    ] {
        require_positive_phase(&mut mismatches, name, value);
    }
    mismatch(
        &mut mismatches,
        "gpu-blit-frames",
        0,
        result.gpu_blit_frames,
    );
    mismatch(
        &mut mismatches,
        "cpu-upload-frames",
        0,
        result.cpu_upload_frames,
    );
    if result.presented_frames > result.submitted_frames {
        mismatches.push(CorrectnessMismatch {
            invariant: "presentation-count-order".to_string(),
            expected: "presented <= submitted".to_string(),
            actual: format!("{} > {}", result.presented_frames, result.submitted_frames),
        });
    }
    if result.interval_samples > result.presented_frames {
        mismatches.push(CorrectnessMismatch {
            invariant: "presentation-interval-count".to_string(),
            expected: "interval samples <= presented frames".to_string(),
            actual: format!("{} > {}", result.interval_samples, result.presented_frames),
        });
    }
    if result.pool_in_flight_high_water > result.pool_capacity {
        mismatches.push(CorrectnessMismatch {
            invariant: "surface-pool-high-water".to_string(),
            expected: format!("<= {}", result.pool_capacity),
            actual: result.pool_in_flight_high_water.to_string(),
        });
    }
    match result.gpu_timing_status {
        NativeVideoGpuTimingStatus::Enabled => {
            for (name, value) in [
                ("gpu-pass-samples", result.gpu_pass_samples),
                ("gpu-pass-total-time", result.gpu_pass_total_us),
                ("gpu-pass-min-time", result.gpu_pass_min_us.unwrap_or(0)),
                ("gpu-pass-max-time", result.gpu_pass_max_us.unwrap_or(0)),
            ] {
                require_positive_phase(&mut mismatches, name, value);
            }
        }
        NativeVideoGpuTimingStatus::Disabled => mismatches.push(CorrectnessMismatch {
            invariant: "gpu-timing-status".to_string(),
            expected: "enabled or unsupported".to_string(),
            actual: "disabled".to_string(),
        }),
        NativeVideoGpuTimingStatus::Unsupported => {}
    }
    mismatches
}

pub(crate) fn valid_sustained_native_video_measurements(
    result: &SustainedNativeVideoResult,
    process_wall_us: u128,
) -> Vec<Measurement> {
    let elapsed_seconds = result.elapsed_wall_us.max(1) as f64 / 1_000_000.0;
    let mut measurements = vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: process_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_cpu_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadWallTime,
            value: result.elapsed_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::VideoPresentationFramesPerSecond,
            value: result.presented_frames as f64 / elapsed_seconds,
            unit: MetricUnit::FramesPerSecond,
        },
        Measurement {
            name: MetricName::VideoDecodeFramesPerSecond,
            value: result.decoded_frames as f64 / elapsed_seconds,
            unit: MetricUnit::FramesPerSecond,
        },
        Measurement {
            name: MetricName::P50VideoPresentationInterval,
            value: result.interval_p50_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::P95VideoPresentationInterval,
            value: result.interval_p95_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::P99VideoPresentationInterval,
            value: result.interval_p99_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::MaxVideoPresentationInterval,
            value: result.interval_max_us as f64,
            unit: MetricUnit::Microseconds,
        },
    ];
    for (name, value) in [
        (MetricName::VideoDecodedFrames, result.decoded_frames),
        (MetricName::VideoPresentedFrames, result.presented_frames),
        (MetricName::VideoReplacedFrames, result.replaced_frames),
        (
            MetricName::VideoLateDroppedFrames,
            result.late_dropped_frames,
        ),
        (
            MetricName::VideoBackpressuredFrames,
            result.backpressured_frames,
        ),
        (MetricName::VideoGpuPassSamples, result.gpu_pass_samples),
        (
            MetricName::VideoSurfacePoolAllocations,
            result.pool_allocations,
        ),
        (MetricName::VideoSurfacePoolReuses, result.pool_reuses),
        (
            MetricName::VideoSurfacePoolBackpressuredAcquires,
            result.pool_backpressured_acquires,
        ),
        (
            MetricName::VideoSurfacePoolInFlightHighWater,
            result.pool_in_flight_high_water,
        ),
    ] {
        measurements.push(Measurement {
            name,
            value: value as f64,
            unit: MetricUnit::Count,
        });
    }
    measurements.push(Measurement {
        name: MetricName::VideoGpuMemoryBytes,
        value: result.gpu_memory_bytes as f64,
        unit: MetricUnit::Bytes,
    });
    if result.gpu_pass_samples > 0 {
        measurements.push(Measurement {
            name: MetricName::AverageVideoGpuPassTime,
            value: result.gpu_pass_total_us as f64 / result.gpu_pass_samples as f64,
            unit: MetricUnit::MicrosecondsPerFrame,
        });
    }
    measurements
}

impl SustainedNativeVideoResult {
    pub(crate) fn execution_identity(&self) -> NativeVideoExecutionIdentity {
        NativeVideoExecutionIdentity {
            decoder_factory: self.decoder_factory.clone(),
            decoder_plugin: self.decoder_plugin.clone(),
            decoder_kind: self.decoder_kind,
            gpu_adapter_name: self.gpu_adapter_name.clone(),
            gpu_vendor: self.gpu_vendor,
            gpu_device: self.gpu_device,
            gpu_device_type: self.gpu_device_type.clone(),
            graphics_backend: self.graphics_backend,
            gpu_driver: self.gpu_driver.clone(),
            gpu_driver_info: self.gpu_driver_info.clone(),
            drm_render_node: self.drm_render_node.clone(),
            display_refresh_hz: self.display_refresh_hz,
            frame_format: self.frame_format,
            gpu_timing_status: self.gpu_timing_status,
        }
    }
}
