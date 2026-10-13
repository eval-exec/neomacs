//! Lisp-facing video-session interface.
//!
//! Video decoding and GPU import belong to the display host's render thread.
//! This module owns only the evaluator seam: typed Lisp handles and commands
//! addressed to the stable video-session identity wrapped by those handles.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use super::error::{EvalResult, Flow, signal};
use super::eval::{Context, VideoResolveRequest, VideoResolveSource};
use super::value::Value;
use neomacs_display_protocol::VideoId;
use neomacs_video_model::{
    BiPlanarVideoFormat, InitialPlayback, LoopMode, PackedVideoFormat, PlaybackAction,
    VideoChromaLocation, VideoColorPrimaries, VideoColorRange, VideoColorimetry,
    VideoCompositorImport, VideoDecodeBackend, VideoDecodeResidency, VideoDecoderIdentity,
    VideoDecoderKind, VideoDiagnostics, VideoFrameFormat, VideoFramePath, VideoGpuTiming,
    VideoGpuTimingStatus, VideoGraphicsBackend, VideoImportCounts, VideoMatrixCoefficients,
    VideoOpenRequest, VideoPresentationCounts, VideoPresentationPath, VideoPresentationTiming,
    VideoRendererIdentity, VideoSessionDiagnostics, VideoSessionState, VideoSource,
    VideoSurfacePoolDiagnostics, VideoSurfacePoolRole, VideoTransferCharacteristic,
};
use std::path::PathBuf;

/// Exactly one valid identity for a Lisp `(video ...)` display specification.
///
/// A session handle is already open and stateful.  A source request is
/// declarative and must be resolved by the display host.  Keeping the two in
/// an enum makes opening a second decoder for a handle unrepresentable after
/// this parsing boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoDisplayReference {
    Session(VideoId),
    Resolve(VideoResolveRequest),
}

/// Parse the identity and initial playback policy shared by inline video and
/// shader-channel display specs.
///
/// `default_autoplay` differs by presentation: ordinary inline declarations
/// default paused, while a video sampled only by a shader channel must play to
/// produce useful frames.  Explicit state options are rejected for `:id`
/// handles because that session is controlled through the native handle API.
pub fn parse_video_display_reference(
    items: &[Value],
    default_autoplay: bool,
) -> Option<VideoDisplayReference> {
    if items.first()?.as_bare_symbol_name() != Some("video") || !(items.len() - 1).is_multiple_of(2)
    {
        return None;
    }

    let mut session = None;
    let mut source = None;
    let mut loop_count = 0;
    let mut autoplay = default_autoplay;
    let mut has_playback_options = false;
    let mut index = 1;
    while index + 1 < items.len() {
        let value = items[index + 1];
        match items[index].as_bare_symbol_name() {
            Some(":id") => {
                if session.is_some() || source.is_some() {
                    return None;
                }
                session = Some(value.as_video_handle()?);
            }
            Some(":file") => {
                if session.is_some() || source.is_some() {
                    return None;
                }
                source = Some(VideoResolveSource::File(value.as_lisp_string()?.clone()));
            }
            Some(":uri") => {
                if session.is_some() || source.is_some() {
                    return None;
                }
                source = Some(VideoResolveSource::Uri(value.as_lisp_string()?.clone()));
            }
            Some(":loop" | ":loop-count") => {
                has_playback_options = true;
                loop_count = if value.is_nil() {
                    0
                } else if value.is_bare_symbol_named("t") {
                    -1
                } else {
                    let count = i32::try_from(value.as_int()?).ok()?;
                    (count >= -1).then_some(count)?
                };
            }
            Some(":autoplay") => {
                has_playback_options = true;
                autoplay = value.is_truthy();
            }
            _ => {}
        }
        index += 2;
    }

    match (session, source) {
        (Some(id), None) if !has_playback_options => Some(VideoDisplayReference::Session(id)),
        (None, Some(source)) => Some(VideoDisplayReference::Resolve(VideoResolveRequest {
            source,
            loop_count,
            autoplay,
        })),
        _ => None,
    }
}

fn video_error(message: impl Into<String>) -> super::error::Flow {
    signal("error", vec![Value::string(message.into())])
}

fn wrong_type(predicate: &str, value: Value) -> Flow {
    signal("wrong-type-argument", vec![Value::symbol(predicate), value])
}

fn display_host<'eval>(
    eval: &'eval Context,
    operation: &str,
) -> Result<&'eval dyn super::eval::DisplayHost, Flow> {
    eval.display_host.as_deref().ok_or_else(|| {
        video_error(format!(
            "{operation}: no GUI video display host in this session"
        ))
    })
}

fn video_id(value: Value) -> Result<VideoId, Flow> {
    value
        .as_video_handle()
        .ok_or_else(|| wrong_type("neomacs-video-p", value))
}

fn predicate(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    Ok(Value::bool(args[0].is_video_handle()))
}

fn source(value: Value) -> Result<VideoSource, Flow> {
    let text = value
        .as_lisp_string()
        .ok_or_else(|| wrong_type("stringp", value))?
        .as_utf8_str()
        .ok_or_else(|| video_error("neomacs-video-load: source must be UTF-8"))?;
    let is_uri = text.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.bytes().enumerate().all(|(index, byte)| match byte {
                b'a'..=b'z' | b'A'..=b'Z' => true,
                b'0'..=b'9' | b'+' | b'-' | b'.' => index > 0,
                _ => false,
            })
    });
    Ok(if is_uri {
        VideoSource::Uri(text.to_owned())
    } else {
        VideoSource::File(PathBuf::from(text))
    })
}

fn loop_mode(value: Value) -> Result<LoopMode, Flow> {
    if value.is_nil() {
        return Ok(LoopMode::Off);
    }
    let count = value
        .as_int()
        .ok_or_else(|| wrong_type("integerp", value))?;
    let count = i32::try_from(count)
        .map_err(|_| video_error("neomacs-video-load: loop count is outside the i32 range"))?;
    LoopMode::from_legacy(count).map_err(|error| video_error(error.to_string()))
}

/// `(neomacs-video-load SOURCE &optional LOOP-COUNT AUTOPLAY)`.
///
/// Allocate one compositor-owned playback session. The returned opaque value
/// is deliberately not an integer: Lisp can copy and compare it, but cannot
/// accidentally use a glyph, image, or stale renderer id as a video session.
fn load(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let request = VideoOpenRequest {
        source: source(args[0])?,
        loop_mode: loop_mode(args.get(1).copied().unwrap_or(Value::NIL))?,
        initial_playback: if args.get(2).is_some_and(|value| value.is_truthy()) {
            InitialPlayback::Playing
        } else {
            InitialPlayback::Paused
        },
    };
    let id = display_host(eval, "neomacs-video-load")?
        .create_video(request)
        .map_err(video_error)?;
    Ok(Value::make_video_handle(id))
}

fn control(eval: &Context, value: Value, operation: &str, action: PlaybackAction) -> EvalResult {
    let id = video_id(value)?;
    display_host(eval, operation)?
        .control_video(id, action)
        .map_err(video_error)?;
    Ok(Value::T)
}

fn play(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    control(eval, args[0], "neomacs-video-play", PlaybackAction::Play)
}

fn pause(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    control(eval, args[0], "neomacs-video-pause", PlaybackAction::Pause)
}

fn stop(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    control(eval, args[0], "neomacs-video-stop", PlaybackAction::Stop)
}

fn set_loop(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let mode = loop_mode(args[1])?;
    control(
        eval,
        args[0],
        "neomacs-video-set-loop",
        PlaybackAction::SetLoop(mode),
    )
}

fn destroy(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let id = video_id(args[0])?;
    display_host(eval, "neomacs-video-destroy")?
        .destroy_video(id)
        .map_err(video_error)?;
    Ok(Value::T)
}

/// A GC-safe builder for Lisp diagnostic lists.
///
/// The Neomacs collector is precise: a `Value` held only in a Rust `Vec` is
/// invisible to it. Root each nested plist as soon as it crosses this builder
/// seam, and keep it rooted until the outer list has been allocated.
struct RootedListBuilder {
    saved_roots: usize,
    values: Vec<Value>,
}

impl RootedListBuilder {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            saved_roots: super::eval::save_scratch_gc_roots(),
            values: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, value: Value) {
        super::eval::push_scratch_gc_root(value);
        self.values.push(value);
    }

    fn field(&mut self, name: &'static str, value: Value) {
        // Root `value` before interning the key: `value` may itself be a
        // freshly allocated nested list that otherwise exists only in Rust.
        super::eval::push_scratch_gc_root(value);
        let key = Value::keyword(name);
        self.values.push(key);
        self.values.push(value);
    }

    fn finish(mut self) -> Value {
        Value::list(std::mem::take(&mut self.values))
    }
}

impl Drop for RootedListBuilder {
    fn drop(&mut self) {
        super::eval::restore_scratch_gc_roots(self.saved_roots);
    }
}

fn diagnostic_integer(value: impl ToString) -> Value {
    Value::make_integer_from_str_or_zero(&value.to_string())
}

fn diagnostic_symbol(name: &'static str) -> Value {
    Value::symbol(name)
}

fn decode_residency_to_lisp(value: VideoDecodeResidency) -> Value {
    diagnostic_symbol(match value {
        VideoDecodeResidency::HardwareDecoderReportsRendererDevice => {
            "hardware-decoder-reports-renderer-device"
        }
        VideoDecodeResidency::HardwareUnverified => "hardware-unverified",
        VideoDecodeResidency::Software => "software",
        VideoDecodeResidency::Unknown => "unknown",
    })
}

fn compositor_import_to_lisp(value: VideoCompositorImport) -> Value {
    diagnostic_symbol(match value {
        VideoCompositorImport::BorrowedNativeSurface => "borrowed-native-surface",
        VideoCompositorImport::GpuBlit => "gpu-blit",
        VideoCompositorImport::CpuUpload => "cpu-upload",
    })
}

fn presentation_path_to_lisp(value: VideoPresentationPath) -> Value {
    diagnostic_symbol(match value {
        VideoPresentationPath::WgpuComposited => "wgpu-composited",
        VideoPresentationPath::NativeOverlay => "native-overlay",
    })
}

fn frame_path_to_lisp(path: VideoFramePath) -> Value {
    let mut plist = RootedListBuilder::with_capacity(6);
    plist.field(
        "decode-residency",
        decode_residency_to_lisp(path.decode_residency()),
    );
    plist.field(
        "compositor-import",
        compositor_import_to_lisp(path.compositor_import()),
    );
    plist.field(
        "presentation",
        presentation_path_to_lisp(path.presentation()),
    );
    plist.finish()
}

fn frame_format_to_lisp(format: VideoFrameFormat) -> Value {
    diagnostic_symbol(match format {
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8) => "bgra8",
        VideoFrameFormat::Packed(PackedVideoFormat::Rgba8) => "rgba8",
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12) => "nv12",
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010) => "p010",
    })
}

fn colorimetry_to_lisp(color: VideoColorimetry) -> Value {
    let mut plist = RootedListBuilder::with_capacity(10);
    plist.field(
        "primaries",
        diagnostic_symbol(match color.primaries {
            VideoColorPrimaries::Bt601_525 => "bt601-525",
            VideoColorPrimaries::Bt601_625 => "bt601-625",
            VideoColorPrimaries::Bt709 => "bt709",
            VideoColorPrimaries::Bt2020 => "bt2020",
        }),
    );
    plist.field(
        "transfer",
        diagnostic_symbol(match color.transfer {
            VideoTransferCharacteristic::Srgb => "srgb",
            VideoTransferCharacteristic::Bt709 => "bt709",
            VideoTransferCharacteristic::Pq => "pq",
            VideoTransferCharacteristic::Hlg => "hlg",
        }),
    );
    plist.field(
        "matrix",
        diagnostic_symbol(match color.matrix {
            VideoMatrixCoefficients::Identity => "identity",
            VideoMatrixCoefficients::Bt601 => "bt601",
            VideoMatrixCoefficients::Bt709 => "bt709",
            VideoMatrixCoefficients::Bt2020NonConstantLuminance => "bt2020-non-constant-luminance",
        }),
    );
    plist.field(
        "range",
        diagnostic_symbol(match color.range {
            VideoColorRange::Limited => "limited",
            VideoColorRange::Full => "full",
        }),
    );
    plist.field(
        "chroma-location",
        diagnostic_symbol(match color.chroma_location {
            VideoChromaLocation::Left => "left",
            VideoChromaLocation::Center => "center",
            VideoChromaLocation::TopLeft => "top-left",
        }),
    );
    plist.finish()
}

fn import_counts_to_lisp(counts: VideoImportCounts) -> Value {
    let mut plist = RootedListBuilder::with_capacity(10);
    plist.field(
        "borrowed-native-frames",
        diagnostic_integer(counts.borrowed_native_frames),
    );
    plist.field(
        "gpu-blit-frames",
        diagnostic_integer(counts.gpu_blit_frames),
    );
    plist.field(
        "cpu-upload-frames",
        diagnostic_integer(counts.cpu_upload_frames),
    );
    plist.field(
        "reported-gpu-blit-bytes",
        diagnostic_integer(counts.reported_gpu_blit_bytes),
    );
    plist.field(
        "cpu-upload-bytes",
        diagnostic_integer(counts.cpu_upload_bytes),
    );
    plist.finish()
}

fn presentation_counts_to_lisp(counts: VideoPresentationCounts) -> Value {
    let mut plist = RootedListBuilder::with_capacity(4);
    plist.field(
        "submitted-frames",
        diagnostic_integer(counts.submitted_frames),
    );
    plist.field(
        "presented-frames",
        diagnostic_integer(counts.presented_frames),
    );
    plist.finish()
}

fn presentation_timing_to_lisp(timing: VideoPresentationTiming) -> Value {
    let optional_integer = |value: Option<u64>| value.map_or(Value::NIL, diagnostic_integer);
    let mut plist = RootedListBuilder::with_capacity(14);
    plist.field(
        "interval-samples",
        diagnostic_integer(timing.interval_samples),
    );
    plist.field(
        "interval-total-us",
        diagnostic_integer(timing.interval_total_us),
    );
    plist.field("interval-min-us", optional_integer(timing.interval_min_us));
    plist.field("interval-max-us", optional_integer(timing.interval_max_us));
    plist.field("interval-p50-us", optional_integer(timing.interval_p50_us));
    plist.field("interval-p95-us", optional_integer(timing.interval_p95_us));
    plist.field("interval-p99-us", optional_integer(timing.interval_p99_us));
    plist.finish()
}

fn gpu_timing_to_lisp(timing: VideoGpuTiming) -> Value {
    let optional_integer = |value: Option<u64>| value.map_or(Value::NIL, diagnostic_integer);
    let mut plist = RootedListBuilder::with_capacity(10);
    plist.field(
        "status",
        diagnostic_symbol(match timing.status {
            VideoGpuTimingStatus::Disabled => "disabled",
            VideoGpuTimingStatus::Unsupported => "unsupported",
            VideoGpuTimingStatus::Enabled => "enabled",
        }),
    );
    plist.field("pass-samples", diagnostic_integer(timing.pass_samples));
    plist.field("pass-total-us", diagnostic_integer(timing.pass_total_us));
    plist.field("pass-min-us", optional_integer(timing.pass_min_us));
    plist.field("pass-max-us", optional_integer(timing.pass_max_us));
    plist.finish()
}

fn decoder_identity_to_lisp(decoder: VideoDecoderIdentity) -> Value {
    let mut plist = RootedListBuilder::with_capacity(6);
    plist.field("factory", Value::string(decoder.factory));
    plist.field("plugin", Value::string(decoder.plugin));
    plist.field(
        "kind",
        diagnostic_symbol(match decoder.kind {
            VideoDecoderKind::Hardware => "hardware",
            VideoDecoderKind::Software => "software",
            VideoDecoderKind::Unknown => "unknown",
        }),
    );
    plist.finish()
}

fn renderer_identity_to_lisp(renderer: VideoRendererIdentity) -> Value {
    let mut plist = RootedListBuilder::with_capacity(18);
    plist.field("adapter-name", Value::string(renderer.adapter_name));
    plist.field("vendor", diagnostic_integer(renderer.vendor));
    plist.field("device", diagnostic_integer(renderer.device));
    plist.field("device-type", Value::string(renderer.device_type));
    plist.field(
        "graphics-backend",
        diagnostic_symbol(match renderer.graphics_backend {
            VideoGraphicsBackend::Vulkan => "vulkan",
            VideoGraphicsBackend::Metal => "metal",
            VideoGraphicsBackend::Dx12 => "dx12",
            VideoGraphicsBackend::Gl => "gl",
            VideoGraphicsBackend::BrowserWebGpu => "browser-webgpu",
            VideoGraphicsBackend::Other => "other",
        }),
    );
    plist.field("driver", Value::string(renderer.driver));
    plist.field("driver-info", Value::string(renderer.driver_info));
    plist.field(
        "drm-render-node",
        renderer.drm_render_node.map_or(Value::NIL, Value::string),
    );
    plist.field(
        "display-refresh-hz",
        renderer
            .display_refresh_hz
            .map_or(Value::NIL, diagnostic_integer),
    );
    plist.finish()
}

fn session_diagnostics_to_lisp(session: VideoSessionDiagnostics) -> Value {
    let mut plist = RootedListBuilder::with_capacity(36);
    plist.field("id", diagnostic_integer(session.id.get()));
    plist.field(
        "backend",
        diagnostic_symbol(match session.backend {
            VideoDecodeBackend::GStreamer => "gstreamer",
            VideoDecodeBackend::AvFoundation => "avfoundation",
            VideoDecodeBackend::MediaFoundation => "media-foundation",
            VideoDecodeBackend::Unsupported => "unsupported",
        }),
    );
    plist.field(
        "decoder",
        session.decoder.map_or(Value::NIL, decoder_identity_to_lisp),
    );
    plist.field(
        "state",
        diagnostic_symbol(match session.state {
            VideoSessionState::Opening => "opening",
            VideoSessionState::Paused => "paused",
            VideoSessionState::Playing => "playing",
            VideoSessionState::Ended => "ended",
            VideoSessionState::Failed => "failed",
            VideoSessionState::Closed => "closed",
        }),
    );
    plist.field(
        "frame-path",
        session.frame_path.map_or(Value::NIL, frame_path_to_lisp),
    );
    plist.field(
        "frame-format",
        session
            .frame_format
            .map_or(Value::NIL, frame_format_to_lisp),
    );
    plist.field(
        "colorimetry",
        session.colorimetry.map_or(Value::NIL, colorimetry_to_lisp),
    );
    plist.field("decoded-frames", diagnostic_integer(session.decoded_frames));
    plist.field(
        "replaced-frames",
        diagnostic_integer(session.replaced_frames),
    );
    plist.field(
        "late-dropped-frames",
        diagnostic_integer(session.late_dropped_frames),
    );
    plist.field(
        "imported-frames",
        diagnostic_integer(session.imported_frames),
    );
    plist.field(
        "backpressured-frames",
        diagnostic_integer(session.backpressured_frames),
    );
    plist.field(
        "output-reconfigurations",
        diagnostic_integer(session.output_reconfigurations),
    );
    plist.field(
        "import-counts",
        import_counts_to_lisp(session.import_counts),
    );
    plist.field(
        "presentation-counts",
        presentation_counts_to_lisp(session.presentation_counts),
    );
    plist.field(
        "presentation-timing",
        presentation_timing_to_lisp(session.presentation_timing),
    );
    plist.field("gpu-timing", gpu_timing_to_lisp(session.gpu_timing));
    plist.field(
        "terminal-error",
        session
            .terminal_error
            .map_or(Value::NIL, |error| Value::string(error.to_string())),
    );
    plist.finish()
}

fn surface_pool_diagnostics_to_lisp(pool: VideoSurfacePoolDiagnostics) -> Value {
    let mut plist = RootedListBuilder::with_capacity(18);
    plist.field(
        "role",
        diagnostic_symbol(match pool.role {
            VideoSurfacePoolRole::DecoderOutput => "decoder-output",
            VideoSurfacePoolRole::CompositorImport => "compositor-import",
        }),
    );
    plist.field("capacity", diagnostic_integer(pool.capacity));
    plist.field("allocated", diagnostic_integer(pool.allocated));
    plist.field("idle", diagnostic_integer(pool.idle));
    plist.field("in-flight", diagnostic_integer(pool.in_flight));
    plist.field("allocations", diagnostic_integer(pool.allocations));
    plist.field("reuses", diagnostic_integer(pool.reuses));
    plist.field(
        "backpressured-acquires",
        diagnostic_integer(pool.backpressured_acquires),
    );
    plist.field(
        "in-flight-high-water",
        diagnostic_integer(pool.in_flight_high_water),
    );
    plist.finish()
}

fn diagnostics_to_lisp(snapshot: VideoDiagnostics, filter: Option<VideoId>) -> Value {
    let mut plist = RootedListBuilder::with_capacity(6);
    plist.field(
        "renderer",
        snapshot
            .renderer
            .map_or(Value::NIL, renderer_identity_to_lisp),
    );
    let mut sessions = RootedListBuilder::with_capacity(snapshot.sessions.len());
    for session in snapshot
        .sessions
        .into_iter()
        .filter(|session| filter.is_none_or(|id| session.id == id))
    {
        sessions.push(session_diagnostics_to_lisp(session));
    }
    plist.field("sessions", sessions.finish());

    let mut pools = RootedListBuilder::with_capacity(snapshot.surface_pools.len());
    for pool in snapshot.surface_pools {
        pools.push(surface_pool_diagnostics_to_lisp(pool));
    }
    plist.field("surface-pools", pools.finish());
    plist.field(
        "gpu-memory-bytes",
        diagnostic_integer(snapshot.gpu_memory_bytes),
    );
    plist.finish()
}

/// `(neomacs-video-diagnostics &optional VIDEO)`.
///
/// Return a coherent renderer snapshot. With VIDEO, retain only that stable
/// Lisp session in `:sessions`; shared surface-pool and GPU-memory accounting
/// remain global because attributing them to one session would double-count.
fn diagnostics(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let filter = args.first().copied().map(video_id).transpose()?;
    let snapshot = display_host(eval, "neomacs-video-diagnostics")?
        .video_diagnostics()
        .map_err(video_error)?;
    Ok(diagnostics_to_lisp(snapshot, filter))
}

/// `(neomacs-video-begin-measurement-epoch)`.
///
/// Reset observation-only native-video counters without changing playback or
/// releasing pooled GPU surfaces. Return the zero-point snapshot captured by
/// the same acknowledged render-thread command, so no post-boundary frame can
/// slip between reset and the baseline.
fn begin_measurement_epoch(eval: &mut Context, _args: Vec<Value>) -> EvalResult {
    let snapshot = display_host(eval, "neomacs-video-begin-measurement-epoch")?
        .begin_video_measurement_epoch()
        .map_err(video_error)?;
    Ok(diagnostics_to_lisp(snapshot, None))
}
