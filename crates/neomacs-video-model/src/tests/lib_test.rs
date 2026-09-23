use crate::{
    FrameImportPolicy, VideoCompositorImport, VideoDecodeBackend, VideoDecodeResidency,
    VideoDiagnostics, VideoEvent, VideoFramePath, VideoImportCounts, VideoPresentationPath,
    VideoServiceRequest, VideoServiceTiming, VideoSessionDiagnostics, VideoSessionState,
};
use neomacs_display_protocol::types::VideoId;
use std::time::{Duration, Instant};

#[test]
fn frame_path_keeps_decoder_import_and_presentation_evidence_independent() {
    let path = VideoFramePath::new(
        VideoDecodeResidency::Unknown,
        VideoCompositorImport::BorrowedNativeSurface,
        VideoPresentationPath::WgpuComposited,
    );

    assert_eq!(path.decode_residency(), VideoDecodeResidency::Unknown);
    assert_eq!(
        path.compositor_import(),
        VideoCompositorImport::BorrowedNativeSurface
    );
    assert_eq!(path.presentation(), VideoPresentationPath::WgpuComposited);
}

#[test]
fn import_policy_considers_only_compositor_work() {
    assert!(
        FrameImportPolicy::RequireDirectSurface
            .permits(VideoCompositorImport::BorrowedNativeSurface)
    );
    assert!(!FrameImportPolicy::RequireDirectSurface.permits(VideoCompositorImport::GpuBlit));
    assert!(FrameImportPolicy::AllowGpuBlit.permits(VideoCompositorImport::GpuBlit));
    assert!(!FrameImportPolicy::AllowGpuBlit.permits(VideoCompositorImport::CpuUpload));
    assert!(FrameImportPolicy::AllowCpuUpload.permits(VideoCompositorImport::CpuUpload));
}

#[test]
fn performance_default_never_silently_uploads_video_through_the_cpu() {
    let policy = FrameImportPolicy::PERFORMANCE_DEFAULT;

    assert!(policy.permits(VideoCompositorImport::BorrowedNativeSurface));
    assert!(policy.permits(VideoCompositorImport::GpuBlit));
    assert!(!policy.permits(VideoCompositorImport::CpuUpload));
}

#[test]
fn video_service_timing_cannot_target_an_already_missed_presentation() {
    let now = Instant::now();
    let missed_target = now - Duration::from_millis(4);

    let timing = VideoServiceTiming::new(now, missed_target);

    assert_eq!(timing.service_time(), now);
    assert_eq!(timing.target_presentation_time(), now);
    assert_eq!(timing.time_until_presentation(), Duration::ZERO);
}

#[test]
fn video_service_request_keeps_an_independent_earliest_target_per_video() {
    let now = Instant::now();
    let fast = VideoId::new(1);
    let slow = VideoId::new(2);
    let hidden = VideoId::new(3);
    let mut request = VideoServiceRequest::new(now);

    request.set_presentation_target(fast, now + Duration::from_millis(16));
    request.set_presentation_target(slow, now + Duration::from_millis(16));
    request.set_presentation_target(fast, now + Duration::from_millis(8));

    assert_eq!(
        request.timing_for(fast).target_presentation_time(),
        now + Duration::from_millis(8)
    );
    assert_eq!(
        request.timing_for(slow).target_presentation_time(),
        now + Duration::from_millis(16)
    );
    assert_eq!(request.timing_for(hidden).target_presentation_time(), now);
    assert!(request.is_presented(fast));
    assert!(!request.is_presented(hidden));
}

#[test]
fn video_event_identity_is_remapped_without_rebuilding_its_payload() {
    let native = VideoId::new(41);
    let editor = VideoId::new(7);
    let previous = VideoFramePath::new(
        VideoDecodeResidency::Unknown,
        VideoCompositorImport::GpuBlit,
        VideoPresentationPath::WgpuComposited,
    );
    let current = VideoFramePath::new(
        VideoDecodeResidency::HardwareDecoderReportsRendererDevice,
        VideoCompositorImport::BorrowedNativeSurface,
        VideoPresentationPath::WgpuComposited,
    );
    let event = VideoEvent::FramePathChanged {
        id: native,
        previous: Some(previous),
        current,
    };

    assert_eq!(event.id(), native);
    assert_eq!(
        event.with_id(editor),
        VideoEvent::FramePathChanged {
            id: editor,
            previous: Some(previous),
            current,
        }
    );
}

#[test]
fn diagnostic_identity_remapping_drops_stale_native_sessions() {
    let current_native = VideoId::new(41);
    let stale_native = VideoId::new(42);
    let editor = VideoId::new(7);
    let session = |id| VideoSessionDiagnostics {
        id,
        backend: VideoDecodeBackend::GStreamer,
        decoder: None,
        state: VideoSessionState::Playing,
        frame_path: None,
        frame_format: None,
        colorimetry: None,
        decoded_frames: 0,
        replaced_frames: 0,
        late_dropped_frames: 0,
        imported_frames: 0,
        backpressured_frames: 0,
        output_reconfigurations: 0,
        import_counts: VideoImportCounts::default(),
        presentation_counts: crate::VideoPresentationCounts::default(),
        presentation_timing: crate::VideoPresentationTiming::default(),
        gpu_timing: crate::VideoGpuTiming::default(),
        terminal_error: None,
    };
    let diagnostics = VideoDiagnostics {
        renderer: None,
        sessions: vec![session(current_native), session(stale_native)],
        surface_pools: Vec::new(),
        gpu_memory_bytes: 4096,
    };

    let diagnostics =
        diagnostics.filter_map_session_ids(|id| (id == current_native).then_some(editor));

    assert_eq!(diagnostics.sessions.len(), 1);
    assert_eq!(diagnostics.sessions[0].id, editor);
    assert_eq!(diagnostics.gpu_memory_bytes, 4096);
}
