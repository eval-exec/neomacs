//! Lisp-visible video session tests.

use super::eval::{Context, DisplayHost, GuiFrameHostRequest};
use super::value::Value;
use super::video::{VideoDisplayReference, parse_video_display_reference};
use neomacs_display_protocol::VideoId;
use neomacs_video_model::{
    BiPlanarVideoFormat, PlaybackAction, VideoCompositorImport, VideoDecodeBackend,
    VideoDecodeResidency, VideoDiagnostics, VideoFrameFormat, VideoFramePath, VideoImportCounts,
    VideoOpenRequest, VideoPresentationPath, VideoSessionDiagnostics, VideoSessionState,
    VideoSurfacePoolDiagnostics, VideoSurfacePoolRole,
};
use std::sync::{Arc, Mutex};

const STUB_VIDEO_ID: VideoId = VideoId::new(42);

#[derive(Clone, Default)]
struct RecordingVideoDisplayHost {
    opens: Arc<Mutex<Vec<VideoOpenRequest>>>,
    controls: Arc<Mutex<Vec<(VideoId, PlaybackAction)>>>,
    destroys: Arc<Mutex<Vec<VideoId>>>,
}

impl DisplayHost for RecordingVideoDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn create_video(&self, request: VideoOpenRequest) -> Result<VideoId, String> {
        self.opens.lock().expect("video host opens").push(request);
        Ok(STUB_VIDEO_ID)
    }

    fn control_video(&self, id: VideoId, action: PlaybackAction) -> Result<(), String> {
        self.controls
            .lock()
            .expect("video host controls")
            .push((id, action));
        Ok(())
    }

    fn destroy_video(&self, id: VideoId) -> Result<(), String> {
        self.destroys.lock().expect("video host destroys").push(id);
        Ok(())
    }

    fn video_diagnostics(&self) -> Result<VideoDiagnostics, String> {
        Ok(VideoDiagnostics {
            renderer: None,
            sessions: vec![VideoSessionDiagnostics {
                id: STUB_VIDEO_ID,
                backend: VideoDecodeBackend::GStreamer,
                decoder: None,
                state: VideoSessionState::Playing,
                frame_path: Some(VideoFramePath::new(
                    VideoDecodeResidency::HardwareDecoderReportsRendererDevice,
                    VideoCompositorImport::BorrowedNativeSurface,
                    VideoPresentationPath::WgpuComposited,
                )),
                frame_format: Some(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)),
                colorimetry: None,
                decoded_frames: 9,
                replaced_frames: 2,
                late_dropped_frames: 1,
                imported_frames: 4,
                backpressured_frames: 1,
                output_reconfigurations: 2,
                import_counts: VideoImportCounts {
                    borrowed_native_frames: 4,
                    ..VideoImportCounts::default()
                },
                presentation_counts: neomacs_video_model::VideoPresentationCounts {
                    submitted_frames: 3,
                    presented_frames: 2,
                },
                presentation_timing: neomacs_video_model::VideoPresentationTiming {
                    interval_samples: 99,
                    interval_total_us: 1_650_000,
                    interval_min_us: Some(15_900),
                    interval_max_us: Some(20_000),
                    interval_p50_us: Some(16_667),
                    interval_p95_us: Some(17_100),
                    interval_p99_us: Some(20_000),
                },
                gpu_timing: neomacs_video_model::VideoGpuTiming {
                    status: neomacs_video_model::VideoGpuTimingStatus::Enabled,
                    pass_samples: 97,
                    pass_total_us: 48_500,
                    pass_min_us: Some(400),
                    pass_max_us: Some(900),
                },
                terminal_error: None,
            }],
            surface_pools: vec![VideoSurfacePoolDiagnostics {
                role: VideoSurfacePoolRole::CompositorImport,
                capacity: 64,
                allocated: 3,
                idle: 2,
                in_flight: 1,
                allocations: 5,
                reuses: 7,
                backpressured_acquires: 1,
                in_flight_high_water: 3,
            }],
            gpu_memory_bytes: 4096,
        })
    }

    fn begin_video_measurement_epoch(&self) -> Result<VideoDiagnostics, String> {
        self.video_diagnostics()
    }
}

fn video_context() -> (Context, RecordingVideoDisplayHost) {
    let host = RecordingVideoDisplayHost::default();
    let mut ctx = Context::new();
    ctx.set_display_host(Box::new(host.clone()));
    (ctx, host)
}

fn eval(ctx: &mut Context, source: &str) -> Value {
    ctx.eval_str(source).expect("video form should evaluate")
}

#[test]
fn advertised_video_session_functions_are_bound() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    let available = ctx
        .eval_str(
            r#"(and (fboundp 'neomacs-video-p)
                     (fboundp 'neomacs-video-load)
                     (fboundp 'neomacs-video-play)
                     (fboundp 'neomacs-video-pause)
                     (fboundp 'neomacs-video-stop)
                     (fboundp 'neomacs-video-set-loop)
                     (fboundp 'neomacs-video-destroy)
                     (fboundp 'neomacs-video-diagnostics)
                     (fboundp 'neomacs-video-begin-measurement-epoch))"#,
        )
        .expect("video function probe should evaluate");

    assert!(available.is_truthy());
}

#[test]
fn measurement_boundary_returns_its_coherent_baseline_snapshot() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = video_context();

    let decoded = eval(
        &mut ctx,
        r#"(plist-get
             (car (plist-get (neomacs-video-begin-measurement-epoch) :sessions))
             :decoded-frames)"#,
    );

    assert_eq!(decoded.as_int(), Some(9));
}

#[test]
fn diagnostics_reports_the_observed_end_to_end_path_and_pool_pressure() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _) = video_context();
    eval(
        &mut ctx,
        r#"(setq video-test-handle (neomacs-video-load "movie.mp4"))"#,
    );

    let rendered = eval(
        &mut ctx,
        r#"(prin1-to-string
             (let* ((snapshot (neomacs-video-diagnostics video-test-handle))
                    (session (car (plist-get snapshot :sessions)))
                    (path (plist-get session :frame-path))
                    (pool (car (plist-get snapshot :surface-pools))))
               (list (plist-get session :id)
                     (plist-get session :backend)
                     (plist-get session :state)
                     (plist-get path :decode-residency)
                     (plist-get path :compositor-import)
                     (plist-get path :presentation)
                     (plist-get session :frame-format)
                     (plist-get session :decoded-frames)
                    (plist-get session :imported-frames)
                    (plist-get (plist-get session :presentation-counts)
                               :submitted-frames)
                    (plist-get (plist-get session :presentation-counts)
                               :presented-frames)
                    (plist-get (plist-get session :presentation-timing)
                               :interval-p99-us)
                    (plist-get (plist-get session :gpu-timing) :pass-samples)
                    (plist-get session :terminal-error)
                    (plist-get snapshot :gpu-memory-bytes)
                     (plist-get pool :role)
                     (plist-get pool :capacity)
                     (plist-get pool :allocated)
                     (plist-get pool :idle)
                     (plist-get pool :in-flight))))"#,
    );
    assert_eq!(
        rendered
            .as_lisp_string()
            .and_then(|value| value.as_utf8_str()),
        Some(
            "(42 gstreamer playing hardware-decoder-reports-renderer-device borrowed-native-surface \
             wgpu-composited nv12 9 4 3 2 20000 97 nil 4096 compositor-import 64 3 2 1)"
        )
    );
}

#[test]
fn load_returns_typed_handle_and_controls_route_the_same_video_id() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, host) = video_context();

    let handle = eval(
        &mut ctx,
        r#"(setq video-test-handle (neomacs-video-load "movie.mp4"))"#,
    );
    assert!(handle.is_video_handle());
    assert!(eval(&mut ctx, "(neomacs-video-p video-test-handle)").is_truthy());
    assert_eq!(handle.as_video_handle(), Some(STUB_VIDEO_ID));
    assert_eq!(handle.as_int(), None);

    eval(
        &mut ctx,
        r#"(progn
             (neomacs-video-play video-test-handle)
             (neomacs-video-pause video-test-handle)
             (neomacs-video-stop video-test-handle)
             (neomacs-video-set-loop video-test-handle -1))"#,
    );

    assert_eq!(host.opens.lock().expect("video host opens").len(), 1);
    assert_eq!(
        *host.controls.lock().expect("video host controls"),
        vec![
            (STUB_VIDEO_ID, PlaybackAction::Play),
            (STUB_VIDEO_ID, PlaybackAction::Pause),
            (STUB_VIDEO_ID, PlaybackAction::Stop),
            (
                STUB_VIDEO_ID,
                PlaybackAction::SetLoop(neomacs_video_model::LoopMode::Infinite),
            ),
        ]
    );
}

#[test]
fn dropping_video_handle_destroys_the_host_session_once() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, host) = video_context();

    eval(
        &mut ctx,
        r#"(setq video-test-handle (neomacs-video-load "movie.mp4"))"#,
    );
    eval(&mut ctx, "(garbage-collect)");
    assert!(
        host.destroys
            .lock()
            .expect("video host destroys")
            .is_empty()
    );

    eval(&mut ctx, "(setq video-test-handle nil)");
    eval(&mut ctx, "(garbage-collect)");
    assert_eq!(
        *host.destroys.lock().expect("video host destroys"),
        vec![STUB_VIDEO_ID]
    );

    eval(&mut ctx, "(garbage-collect)");
    assert_eq!(
        *host.destroys.lock().expect("video host destroys"),
        vec![STUB_VIDEO_ID]
    );
}

#[test]
fn display_reference_keeps_open_sessions_distinct_from_source_requests() {
    crate::test_utils::init_test_tracing();
    let handle = Value::make_video_handle(STUB_VIDEO_ID);
    let items = vec![Value::symbol("video"), Value::keyword("id"), handle];
    assert_eq!(
        parse_video_display_reference(&items, false),
        Some(VideoDisplayReference::Session(STUB_VIDEO_ID))
    );

    let raw_id = vec![
        Value::symbol("video"),
        Value::keyword("id"),
        Value::fixnum(i64::from(STUB_VIDEO_ID.get())),
    ];
    assert_eq!(parse_video_display_reference(&raw_id, false), None);

    let ambiguous = vec![
        Value::symbol("video"),
        Value::keyword("id"),
        handle,
        Value::keyword("file"),
        Value::string("movie.mp4"),
    ];
    assert_eq!(parse_video_display_reference(&ambiguous, false), None);

    let stateful_display = vec![
        Value::symbol("video"),
        Value::keyword("id"),
        handle,
        Value::keyword("autoplay"),
        Value::T,
    ];
    assert_eq!(
        parse_video_display_reference(&stateful_display, false),
        None
    );
}
