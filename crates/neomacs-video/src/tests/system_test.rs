use super::{LoopMode, PlaybackRate, VideoModelError};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use neomacs_display_protocol::types::VideoId;

use super::backend::{
    BackendEvent, CompletedFrameImport, DecodedFrame, DecodedFrameImport, DecoderBackend,
    DecoderOutputGeneration, DecoderOutputRejection, DecoderReconfiguration, FrameImportOutcome,
    FrameImporter, ImportedFrame, Platform, require_fixed_compositor_import,
};
use super::system::VideoSystemImpl;
use super::{
    FrameImportPolicy, FrameTiming, GpuGeneration, InitialPlayback, MediaTime, PackedVideoFormat,
    PlaybackEpoch, PresentationVisibility, VideoColorimetry, VideoCommand, VideoCompositorImport,
    VideoDecodeResidency, VideoEvent, VideoFrameFormat, VideoFramePath, VideoFrameReady,
    VideoGeometry, VideoInitError, VideoPresentationPath, VideoServiceRequest, VideoServiceTiming,
    VideoSessionState, VideoSource,
};

#[test]
fn legacy_loop_count_has_one_typed_interpretation() {
    assert_eq!(LoopMode::from_legacy(-1), Ok(LoopMode::Infinite));
    assert_eq!(LoopMode::from_legacy(0), Ok(LoopMode::Off));
    assert_eq!(
        LoopMode::from_legacy(3),
        Ok(LoopMode::Count(std::num::NonZeroU32::new(3).unwrap()))
    );
    assert_eq!(
        LoopMode::from_legacy(-2),
        Err(VideoModelError::InvalidLoopCount)
    );
}

#[test]
fn playback_rate_rejects_values_that_cannot_drive_a_clock() {
    assert_eq!(PlaybackRate::new(1.5).map(PlaybackRate::get), Ok(1.5));
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            PlaybackRate::new(invalid),
            Err(VideoModelError::InvalidPlaybackRate)
        );
    }
}

#[test]
fn finite_loop_permission_is_consumed_without_an_untyped_counter_sentinel() {
    let mut mode = LoopMode::Count(std::num::NonZeroU32::new(2).unwrap());
    assert!(mode.consume_replay());
    assert_eq!(mode, LoopMode::Count(std::num::NonZeroU32::new(1).unwrap()));
    assert!(mode.consume_replay());
    assert_eq!(mode, LoopMode::Off);
    assert!(!mode.consume_replay());
}

#[test]
fn import_policy_orders_direct_gpu_copy_and_cpu_fallback_explicitly() {
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
fn fixed_native_path_is_rejected_before_platform_startup() {
    assert_eq!(
        require_fixed_compositor_import(
            super::VideoDecodeBackend::MediaFoundation,
            FrameImportPolicy::RequireDirectSurface,
            VideoCompositorImport::GpuBlit,
        ),
        Err(VideoInitError::ImportForbidden {
            backend: super::VideoDecodeBackend::MediaFoundation,
            policy: FrameImportPolicy::RequireDirectSurface,
            path: VideoCompositorImport::GpuBlit,
        })
    );
    assert_eq!(
        require_fixed_compositor_import(
            super::VideoDecodeBackend::MediaFoundation,
            FrameImportPolicy::AllowGpuBlit,
            VideoCompositorImport::GpuBlit,
        ),
        Ok(())
    );
}

#[test]
fn renderer_recovery_advances_a_nonzero_gpu_generation() {
    assert_eq!(GpuGeneration::INITIAL.get(), 1);
    assert_eq!(GpuGeneration::INITIAL.next().get(), 2);
}

#[derive(Clone)]
struct FakeControl {
    events: Arc<Mutex<VecDeque<BackendEvent<u64>>>>,
}

impl FakeControl {
    fn publish(&self, event: BackendEvent<u64>) {
        self.events.lock().unwrap().push_back(event);
    }
}

struct FakeDecoder {
    events: Arc<Mutex<VecDeque<BackendEvent<u64>>>>,
}

impl DecoderBackend for FakeDecoder {
    type Frame = u64;

    fn command(&mut self, _command: VideoCommand) -> Result<(), super::VideoCommandError> {
        Ok(())
    }

    fn service(&mut self, _request: &VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.events.lock().unwrap().drain(..).collect()
    }
}

struct FakeImporter;

impl FrameImporter<u64> for FakeImporter {
    type Sampled = u64;

    fn compositor_import(&self, _frame: &DecodedFrame<u64>) -> VideoCompositorImport {
        VideoCompositorImport::BorrowedNativeSurface
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            completed_import: CompletedFrameImport::BorrowedNativeSurface,
        }))
    }
}

struct FakePlatform;

impl Platform for FakePlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::GStreamer;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = FakeDecoder;
    type Importer = FakeImporter;
}

struct GpuCopyImporter;

impl FrameImporter<u64> for GpuCopyImporter {
    type Sampled = u64;

    fn compositor_import(&self, _frame: &DecodedFrame<u64>) -> VideoCompositorImport {
        VideoCompositorImport::GpuBlit
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            completed_import: CompletedFrameImport::GpuBlit {
                reported_bytes: Some(4),
            },
        }))
    }
}

struct GpuCopyPlatform;

impl Platform for GpuCopyPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::MediaFoundation;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = FakeDecoder;
    type Importer = GpuCopyImporter;
}

fn fake_system() -> (VideoSystemImpl<FakePlatform>, FakeControl) {
    let events = Arc::new(Mutex::new(VecDeque::new()));
    (
        VideoSystemImpl::new(
            FakeDecoder {
                events: Arc::clone(&events),
            },
            FakeImporter,
            FrameImportPolicy::RequireDirectSurface,
        ),
        FakeControl { events },
    )
}

struct BackpressuredImporter;

impl FrameImporter<u64> for BackpressuredImporter {
    type Sampled = u64;

    fn compositor_import(&self, _frame: &DecodedFrame<u64>) -> VideoCompositorImport {
        VideoCompositorImport::BorrowedNativeSurface
    }

    fn import(
        &mut self,
        _frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        Ok(FrameImportOutcome::Backpressured)
    }
}

struct BackpressuredPlatform;

impl Platform for BackpressuredPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::GStreamer;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = FakeDecoder;
    type Importer = BackpressuredImporter;
}

struct RecoveringDecoder {
    events: Arc<Mutex<VecDeque<BackendEvent<u64>>>>,
    reconfigurations: Arc<Mutex<Vec<(VideoId, VideoFrameFormat)>>>,
}

impl DecoderBackend for RecoveringDecoder {
    type Frame = u64;

    fn command(&mut self, _command: VideoCommand) -> Result<(), super::VideoCommandError> {
        Ok(())
    }

    fn service(&mut self, _request: &VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.events.lock().unwrap().drain(..).collect()
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejection: &DecoderOutputRejection,
    ) -> Result<DecoderReconfiguration, String> {
        self.reconfigurations
            .lock()
            .unwrap()
            .push((id, rejection.format));
        Ok(DecoderReconfiguration::Applied {
            generation: rejection.generation.next(),
        })
    }
}

struct RecoveringImporter {
    attempts: usize,
}

impl FrameImporter<u64> for RecoveringImporter {
    type Sampled = u64;

    fn compositor_import(&self, _frame: &DecodedFrame<u64>) -> VideoCompositorImport {
        VideoCompositorImport::GpuBlit
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        self.attempts += 1;
        if self.attempts == 1 {
            return Ok(FrameImportOutcome::ReconfigureDecoder {
                rejection: DecoderOutputRejection {
                    generation: frame.output_generation,
                    format: frame.format,
                    reason: "native target rejected".to_owned(),
                },
            });
        }
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            completed_import: CompletedFrameImport::GpuBlit {
                reported_bytes: None,
            },
        }))
    }
}

struct RecoveringPlatform;

impl Platform for RecoveringPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::MediaFoundation;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = RecoveringDecoder;
    type Importer = RecoveringImporter;
}

struct CloseFailDecoder {
    events: Arc<Mutex<VecDeque<BackendEvent<u64>>>>,
}

impl DecoderBackend for CloseFailDecoder {
    type Frame = u64;

    fn command(&mut self, command: VideoCommand) -> Result<(), super::VideoCommandError> {
        if matches!(command, VideoCommand::Close { .. }) {
            Err("native decoder had already exited".into())
        } else {
            Ok(())
        }
    }

    fn service(&mut self, _request: &VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.events.lock().unwrap().drain(..).collect()
    }
}

struct CloseFailPlatform;

impl Platform for CloseFailPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::GStreamer;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = CloseFailDecoder;
    type Importer = FakeImporter;
}

#[test]
fn opening_becomes_ready_only_after_the_native_adapter_acknowledges_it() {
    let id = VideoId::new(7);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Paused,
            loop_mode: LoopMode::Off,
        })
        .unwrap();

    assert_eq!(system.state(id), Some(VideoSessionState::Opening));
    control.publish(BackendEvent::Opened {
        id,
        width: 1920,
        height: 1080,
        initial_state: VideoSessionState::Paused,
    });

    let result = system.service(Instant::now());
    assert_eq!(system.state(id), Some(VideoSessionState::Paused));
    assert_eq!(
        result.events,
        vec![VideoEvent::Ready {
            id,
            width: 1920,
            height: 1080,
        }]
    );
}

#[test]
fn duplicate_open_is_rejected_without_destroying_the_existing_session() {
    let id = VideoId::new(70);
    let (mut system, _) = fake_system();
    let open = || VideoCommand::Open {
        id,
        source: VideoSource::File("movie.mp4".into()),
        initial_playback: InitialPlayback::Paused,
        loop_mode: LoopMode::Off,
    };

    system.command(open()).unwrap();
    assert_eq!(
        system.command(open()),
        Err(super::VideoCommandError::SessionAlreadyOpen { id: 70 })
    );
    assert_eq!(system.state(id), Some(VideoSessionState::Opening));
}

#[test]
fn future_frames_cannot_continuously_replace_the_next_presentable_frame() {
    let id = VideoId::new(83);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 320,
        height: 200,
        initial_state: VideoSessionState::Playing,
    });

    let started_at = Instant::now();
    control.publish(fake_frame(id, 1, 10));
    assert_eq!(
        system.service(started_at).next_deadline,
        Some(started_at + Duration::from_nanos(10))
    );

    // A decoder running at the presentation rate publishes each successor
    // before the previous frame's deadline. Replacing the only pending slot
    // here would move the deadline forever and starve the compositor.
    control.publish(fake_frame(id, 2, 20));
    assert!(
        system
            .service(started_at + Duration::from_nanos(5))
            .ready_frames
            .is_empty()
    );
    control.publish(fake_frame(id, 3, 30));
    let due = system.service(started_at + Duration::from_nanos(10));

    assert_eq!(due.ready_frames.len(), 1);
    assert_eq!(due.ready_frames[0].id, id);
    assert_eq!(due.ready_frames[0].pts, MediaTime::from_nanos(10));
    assert_eq!(system.sampled(id), Some(&1));
}

#[test]
fn service_preserves_the_next_future_frame_then_imports_the_latest_due_frame() {
    let id = VideoId::new(8);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 320,
        height: 200,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 1,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(10),
                duration: MediaTime::from_nanos(10),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 2,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(10),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });

    let opened_at = Instant::now();
    let opening = system.service(opened_at);
    assert!(opening.ready_frames.is_empty());
    assert_eq!(
        opening.next_deadline,
        Some(opened_at + Duration::from_nanos(10))
    );

    let result = system.service(opened_at + Duration::from_nanos(20));
    let frame_path = VideoFramePath::new(
        VideoDecodeResidency::Unknown,
        VideoCompositorImport::BorrowedNativeSurface,
        VideoPresentationPath::WgpuComposited,
    );
    assert_eq!(
        result.events,
        vec![VideoEvent::FramePathChanged {
            id,
            previous: None,
            current: frame_path,
        }]
    );
    assert_eq!(
        result.ready_frames,
        vec![VideoFrameReady {
            id,
            pts: MediaTime::from_nanos(20),
            frame_path,
        }]
    );
    assert_eq!(system.sampled(id), Some(&2));
    assert_eq!(
        system.session_diagnostics(),
        vec![super::VideoSessionDiagnostics {
            id,
            backend: super::VideoDecodeBackend::GStreamer,
            decoder: None,
            state: VideoSessionState::Playing,
            frame_path: Some(frame_path),
            frame_format: Some(VideoFrameFormat::Packed(PackedVideoFormat::Rgba8)),
            colorimetry: Some(VideoColorimetry::SRGB),
            decoded_frames: 2,
            replaced_frames: 1,
            late_dropped_frames: 0,
            imported_frames: 1,
            backpressured_frames: 0,
            output_reconfigurations: 0,
            import_counts: super::VideoImportCounts {
                borrowed_native_frames: 1,
                gpu_blit_frames: 0,
                cpu_upload_frames: 0,
                reported_gpu_blit_bytes: 0,
                cpu_upload_bytes: 0,
            },
            presentation_counts: super::VideoPresentationCounts::default(),
            presentation_timing: super::VideoPresentationTiming::default(),
            gpu_timing: super::VideoGpuTiming::default(),
            terminal_error: None,
        }]
    );

    control.publish(fake_frame(id, 3, 30));
    let unchanged_path = system.service(opened_at + Duration::from_nanos(30));
    assert!(unchanged_path.events.is_empty());
}

#[test]
fn decoder_completed_imports_are_counted_even_when_the_frame_is_replaced() {
    let id = VideoId::new(82);
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let control = FakeControl {
        events: Arc::clone(&events),
    };
    let mut system = VideoSystemImpl::<GpuCopyPlatform>::new(
        FakeDecoder { events },
        GpuCopyImporter,
        FrameImportPolicy::AllowGpuBlit,
    );
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    for lease in [1, 2] {
        let BackendEvent::Frame { id, mut frame } = fake_frame(id, lease, 0) else {
            unreachable!("fake_frame always constructs a frame event");
        };
        frame.decoder_import = DecodedFrameImport::Completed(CompletedFrameImport::GpuBlit {
            reported_bytes: Some(4),
        });
        control.publish(BackendEvent::Frame { id, frame });
    }

    system.service(Instant::now());

    let diagnostics = system.session_diagnostics();
    assert_eq!(diagnostics[0].decoded_frames, 2);
    assert_eq!(diagnostics[0].replaced_frames, 1);
    assert_eq!(diagnostics[0].imported_frames, 1);
    assert_eq!(diagnostics[0].import_counts.gpu_blit_frames, 2);
    assert_eq!(diagnostics[0].import_counts.reported_gpu_blit_bytes, 8);
}

#[test]
fn bounded_importer_backpressure_drops_a_frame_without_failing_playback() {
    let id = VideoId::new(81);
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let control = FakeControl {
        events: Arc::clone(&events),
    };
    let mut system = VideoSystemImpl::<BackpressuredPlatform>::new(
        FakeDecoder { events },
        BackpressuredImporter,
        FrameImportPolicy::RequireDirectSurface,
    );
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 320,
        height: 200,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 1,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::ZERO,
                duration: MediaTime::from_nanos(16_666_667),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });

    let result = system.service(Instant::now());

    assert!(result.ready_frames.is_empty());
    assert!(result.events.iter().all(|event| !matches!(
        event,
        VideoEvent::Failed { id: event_id, .. } if *event_id == id
    )));
    assert_eq!(system.state(id), Some(VideoSessionState::Playing));
    assert_eq!(system.sampled(id), None);
    let diagnostics = system.session_diagnostics();
    assert_eq!(diagnostics[0].backpressured_frames, 1);
    assert_eq!(
        diagnostics[0].frame_format,
        Some(VideoFrameFormat::Packed(PackedVideoFormat::Rgba8))
    );
}

#[test]
fn recoverable_import_failure_reconfigures_decoder_without_poisoning_session() {
    let id = VideoId::new(82);
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let control = FakeControl {
        events: Arc::clone(&events),
    };
    let reconfigurations = Arc::new(Mutex::new(Vec::new()));
    let mut system = VideoSystemImpl::<RecoveringPlatform>::new(
        RecoveringDecoder {
            events,
            reconfigurations: Arc::clone(&reconfigurations),
        },
        RecoveringImporter { attempts: 0 },
        FrameImportPolicy::AllowGpuBlit,
    );
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 320,
        height: 200,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 1, 0));

    let now = Instant::now();
    let first = system.service(now);
    assert_eq!(system.state(id), Some(VideoSessionState::Playing));
    assert!(first.ready_frames.is_empty());
    assert!(
        first
            .events
            .iter()
            .all(|event| !matches!(event, VideoEvent::Failed { .. }))
    );
    assert_eq!(
        *reconfigurations.lock().unwrap(),
        [(id, VideoFrameFormat::Packed(PackedVideoFormat::Rgba8))]
    );

    control.publish(fake_frame(id, 99, 0));
    let stale = system.service(now);
    assert!(stale.ready_frames.is_empty());
    assert_eq!(system.state(id), Some(VideoSessionState::Playing));
    assert_eq!(system.sampled(id), None);
    assert_eq!(system.session_diagnostics()[0].output_reconfigurations, 0);

    control.publish(fake_frame_with_generation(
        id,
        2,
        0,
        DecoderOutputGeneration::INITIAL.next(),
    ));
    let recovered = system.service(now);
    assert_eq!(recovered.ready_frames.len(), 1);
    assert_eq!(system.sampled(id), Some(&2));
    assert_eq!(system.session_diagnostics()[0].output_reconfigurations, 1);
}

#[test]
fn decoder_output_acknowledgements_are_generation_checked_and_idempotent() {
    let id = VideoId::new(83);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    let fallback = DecoderOutputGeneration::INITIAL.next();
    control.publish(BackendEvent::OutputReconfigured {
        id,
        generation: fallback,
    });
    control.publish(BackendEvent::OutputReconfigured {
        id,
        generation: fallback,
    });
    control.publish(fake_frame_with_generation(id, 7, 0, fallback));

    let result = system.service(Instant::now());

    assert_eq!(result.ready_frames.len(), 1);
    assert_eq!(system.sampled(id), Some(&7));
    assert_eq!(system.session_diagnostics()[0].output_reconfigurations, 1);
}

#[test]
fn successor_frame_promotes_output_without_a_control_acknowledgement() {
    let id = VideoId::new(84);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame_with_generation(
        id,
        7,
        0,
        DecoderOutputGeneration::INITIAL.next(),
    ));

    let result = system.service(Instant::now());

    assert_eq!(result.ready_frames.len(), 1);
    assert!(
        result
            .events
            .iter()
            .all(|event| !matches!(event, VideoEvent::Failed { .. }))
    );
    assert_eq!(system.sampled(id), Some(&7));
    assert_eq!(system.session_diagnostics()[0].output_reconfigurations, 1);
}

#[test]
fn frame_driven_output_generation_promotion_discards_an_already_queued_future_frame() {
    let id = VideoId::new(84);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 7, 1_000_000_000));
    assert!(system.service(now).ready_frames.is_empty());

    control.publish(fake_frame_with_generation(
        id,
        8,
        1_000_000_000,
        DecoderOutputGeneration::INITIAL.next(),
    ));
    assert!(system.service(now).ready_frames.is_empty());

    let presented = system.service(now + Duration::from_secs(1));
    assert_eq!(presented.ready_frames.len(), 1);
    assert_eq!(system.sampled(id), Some(&8));
}

#[test]
fn a_new_session_anchors_decoder_pts_to_its_open_acknowledgement() {
    let id = VideoId::new(9);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 320,
        height: 200,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 11,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(1_000_000_000),
                duration: MediaTime::from_nanos(41_666_667),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });

    let much_later_than_process_start = Instant::now() + Duration::from_secs(60);
    let result = system.service(much_later_than_process_start);

    assert!(result.ready_frames.is_empty());
    assert_eq!(
        result.next_deadline,
        Some(much_later_than_process_start + Duration::from_secs(1))
    );
}

#[test]
fn replacing_a_presented_frame_moves_it_to_affine_gpu_retirement() {
    let id = VideoId::new(10);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 1, 0));
    system.service(now);
    assert_eq!(system.sampled(id), Some(&1));
    assert!(system.take_retired().is_empty());

    control.publish(fake_frame(id, 2, 0));
    system.service(now);
    assert_eq!(system.sampled(id), Some(&2));
    assert_eq!(system.take_retired(), vec![1]);
}

#[test]
fn teardown_drains_current_and_already_replaced_native_leases_together() {
    let id = VideoId::new(99);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 1, 0));
    system.service(now);
    control.publish(fake_frame(id, 2, 0));
    system.service(now);

    let mut drained = system.take_all_sampled_for_retirement();
    drained.sort_unstable();
    assert_eq!(drained, vec![1, 2]);
    assert_eq!(system.sampled(id), None);
    assert!(system.take_retired().is_empty());
}

#[test]
fn closing_a_presented_video_retires_its_native_lease_after_gpu_submission() {
    let id = VideoId::new(83);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 41,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::ZERO,
                duration: MediaTime::from_nanos(1),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });
    system.service(Instant::now());

    system.command(VideoCommand::Close { id }).unwrap();

    assert_eq!(system.state(id), None);
    assert_eq!(system.take_retired(), vec![41]);
}

#[test]
fn close_cleans_local_state_even_if_the_native_decoder_already_failed() {
    let id = VideoId::new(84);
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let control = FakeControl {
        events: Arc::clone(&events),
    };
    let mut system = VideoSystemImpl::<CloseFailPlatform>::new(
        CloseFailDecoder { events },
        FakeImporter,
        FrameImportPolicy::RequireDirectSurface,
    );
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 42, 0));
    system.service(Instant::now());

    assert_eq!(
        system.command(VideoCommand::Close { id }),
        Err("native decoder had already exited".into())
    );
    assert_eq!(system.state(id), None);
    assert_eq!(system.take_retired(), vec![42]);
}

fn fake_frame(id: VideoId, lease: u64, pts: u64) -> BackendEvent<u64> {
    fake_frame_with_duration(id, lease, pts, 1)
}

fn fake_frame_with_generation(
    id: VideoId,
    lease: u64,
    pts: u64,
    output_generation: DecoderOutputGeneration,
) -> BackendEvent<u64> {
    let BackendEvent::Frame { id, mut frame } = fake_frame(id, lease, pts) else {
        unreachable!("fake_frame always constructs a frame event");
    };
    frame.output_generation = output_generation;
    BackendEvent::Frame { id, frame }
}

fn fake_frame_with_duration(id: VideoId, lease: u64, pts: u64, duration: u64) -> BackendEvent<u64> {
    BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(pts),
                duration: MediaTime::from_nanos(duration),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    }
}

struct ForbiddenImporter {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl FrameImporter<u64> for ForbiddenImporter {
    type Sampled = u64;

    fn compositor_import(&self, _frame: &DecodedFrame<u64>) -> VideoCompositorImport {
        VideoCompositorImport::CpuUpload
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            completed_import: CompletedFrameImport::CpuUpload { bytes: 4 },
        }))
    }
}

struct QuiescingDecoder {
    events: Arc<Mutex<VecDeque<BackendEvent<u64>>>>,
    commands: Arc<Mutex<Vec<VideoCommand>>>,
}

impl DecoderBackend for QuiescingDecoder {
    type Frame = u64;

    fn command(&mut self, command: VideoCommand) -> Result<(), super::VideoCommandError> {
        self.commands.lock().unwrap().push(command);
        Ok(())
    }

    fn service(&mut self, _request: &VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.events.lock().unwrap().drain(..).collect()
    }
}

struct ForbiddenPlatform;

impl Platform for ForbiddenPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::GStreamer;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = QuiescingDecoder;
    type Importer = ForbiddenImporter;
}

#[test]
fn strict_import_policy_rejects_a_frame_before_import_side_effects() {
    let id = VideoId::new(91);
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let control = FakeControl {
        events: Arc::clone(&events),
    };
    let commands = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut system = VideoSystemImpl::<ForbiddenPlatform>::new(
        QuiescingDecoder {
            events,
            commands: Arc::clone(&commands),
        },
        ForbiddenImporter {
            calls: Arc::clone(&calls),
        },
        FrameImportPolicy::RequireDirectSurface,
    );
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 1, 0));

    let result = system.service(Instant::now());

    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert_eq!(system.state(id), Some(VideoSessionState::Failed));
    assert!(matches!(
        result.events.last(),
        Some(VideoEvent::Failed {
            error: super::VideoCommandError::ImportForbidden {
                policy: FrameImportPolicy::RequireDirectSurface,
                path: VideoCompositorImport::CpuUpload,
            },
            ..
        })
    ));
    assert!(matches!(
        commands.lock().unwrap().last(),
        Some(VideoCommand::Close { id: closed_id }) if *closed_id == id
    ));

    // Native close is asynchronous. A racing frame from the failed
    // incarnation must neither import nor revive the logical session.
    control.publish(fake_frame(id, 2, 0));
    let after_close = system.service(Instant::now());
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert_eq!(system.state(id), Some(VideoSessionState::Failed));
    assert!(after_close.ready_frames.is_empty());
    assert!(after_close.events.is_empty());
    assert_eq!(
        system.command(VideoCommand::Playback {
            id,
            action: super::PlaybackAction::Play,
        }),
        Err(super::VideoCommandError::SessionFailed { id: id.get() })
    );
    assert_eq!(
        system.set_presentation(id, PresentationVisibility::Presented),
        Err(super::VideoCommandError::SessionFailed { id: id.get() })
    );
}

#[test]
fn expired_frame_is_dropped_before_native_import() {
    let id = VideoId::new(92);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let opened_at = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 1, 0));
    system.service(opened_at);
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 2,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(10),
                duration: MediaTime::from_nanos(5),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });

    let result = system.service(opened_at + Duration::from_nanos(100));

    assert!(result.ready_frames.is_empty());
    assert_eq!(system.sampled(id), Some(&1));
    let diagnostics = system.session_diagnostics();
    assert_eq!(diagnostics[0].decoded_frames, 2);
    assert_eq!(diagnostics[0].imported_frames, 1);
    assert_eq!(diagnostics[0].late_dropped_frames, 1);
}

#[test]
fn seek_epoch_rejects_a_decoder_frame_from_before_the_discontinuity() {
    let id = VideoId::new(93);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);
    system
        .command_at(
            VideoCommand::Playback {
                id,
                action: super::PlaybackAction::Seek(MediaTime::from_nanos(1_000)),
            },
            now,
        )
        .unwrap();
    control.publish(fake_frame(id, 7, 0));

    let result = system.service(now + Duration::from_nanos(2_000));

    assert!(result.ready_frames.is_empty());
    assert_eq!(system.sampled(id), None);
}

#[test]
fn seek_discards_both_queued_frames_from_the_previous_epoch() {
    let id = VideoId::new(94);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);
    control.publish(fake_frame_with_duration(id, 10, 100, 10_000));
    control.publish(fake_frame_with_duration(id, 20, 200, 10_000));

    let queued = system.service(now);
    assert!(queued.ready_frames.is_empty());
    system
        .command_at(
            VideoCommand::Playback {
                id,
                action: super::PlaybackAction::Seek(MediaTime::ZERO),
            },
            now,
        )
        .unwrap();

    let after_seek = system.service(now + Duration::from_nanos(300));

    assert!(after_seek.ready_frames.is_empty());
    assert_eq!(system.sampled(id), None);
}

#[test]
fn measurement_boundary_discards_a_frame_already_in_the_common_mailbox() {
    let id = VideoId::new(95);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);
    control.publish(fake_frame_with_duration(id, 10, 1_000, 10_000));

    let queued = system.service(now);
    assert!(queued.ready_frames.is_empty());
    system.begin_measurement_epoch();

    let after_boundary = system.service(now + Duration::from_nanos(2_000));
    assert!(after_boundary.ready_frames.is_empty());
    assert_eq!(system.sampled(id), None);
    let diagnostics = system.session_diagnostics();
    assert_eq!(diagnostics[0].decoded_frames, 0);
    assert_eq!(diagnostics[0].imported_frames, 0);
}

#[test]
fn loop_boundary_rejects_a_terminal_frame_from_the_previous_epoch() {
    let id = VideoId::new(100);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Infinite,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);

    // Native adapters publish loop control before the latest-frame mailbox is
    // drained. A terminal sample from the completed epoch must not cross that
    // discontinuity merely because it is appended after the control event.
    control.publish(BackendEvent::Looped {
        id,
        remaining: LoopMode::Infinite,
    });
    control.publish(fake_frame(id, 7, 0));

    let stale = system.service(now);

    assert!(stale.ready_frames.is_empty());
    assert_eq!(system.sampled(id), None);

    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 8,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::ZERO,
                duration: MediaTime::from_nanos(1),
                epoch: PlaybackEpoch::INITIAL.next(),
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });

    let current = system.service(now);

    assert_eq!(
        current.ready_frames,
        vec![VideoFrameReady {
            id,
            pts: MediaTime::ZERO,
            frame_path: VideoFramePath::new(
                VideoDecodeResidency::Unknown,
                VideoCompositorImport::BorrowedNativeSurface,
                VideoPresentationPath::WgpuComposited,
            ),
        }]
    );
    assert_eq!(system.sampled(id), Some(&8));
}

struct RecordingDecoder {
    commands: Arc<Mutex<Vec<VideoCommand>>>,
}

impl DecoderBackend for RecordingDecoder {
    type Frame = u64;

    fn command(&mut self, command: VideoCommand) -> Result<(), super::VideoCommandError> {
        self.commands.lock().unwrap().push(command);
        Ok(())
    }

    fn service(&mut self, _request: &VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        Vec::new()
    }
}

struct RecordingPlatform;

impl Platform for RecordingPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::GStreamer;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = RecordingDecoder;
    type Importer = FakeImporter;
}

struct TimingDecoder {
    id: VideoId,
    observed: Arc<Mutex<Vec<VideoServiceTiming>>>,
}

impl DecoderBackend for TimingDecoder {
    type Frame = u64;

    fn command(&mut self, _command: VideoCommand) -> Result<(), super::VideoCommandError> {
        Ok(())
    }

    fn service(&mut self, request: &VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.observed
            .lock()
            .unwrap()
            .push(request.timing_for(self.id));
        Vec::new()
    }
}

struct TimingPlatform;

impl Platform for TimingPlatform {
    const BACKEND: super::VideoDecodeBackend = super::VideoDecodeBackend::AvFoundation;
    type Frame = u64;
    type Sampled = u64;
    type Decoder = TimingDecoder;
    type Importer = FakeImporter;
}

#[test]
fn per_video_presentation_target_reaches_the_pull_based_decoder_boundary() {
    let id = VideoId::new(95);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut system = VideoSystemImpl::<TimingPlatform>::new(
        TimingDecoder {
            id,
            observed: Arc::clone(&observed),
        },
        FakeImporter,
        FrameImportPolicy::RequireDirectSurface,
    );
    let now = Instant::now();
    let mut request = VideoServiceRequest::new(now);
    request.set_presentation_target(id, now + Duration::from_millis(8));

    system.service_with_request(request);

    assert_eq!(
        *observed.lock().unwrap(),
        vec![VideoServiceTiming::new(now, now + Duration::from_millis(8))]
    );
}

#[test]
fn presentation_visibility_is_a_deduplicated_native_decoder_input() {
    let id = VideoId::new(94);
    let commands = Arc::new(Mutex::new(Vec::new()));
    let mut system = VideoSystemImpl::<RecordingPlatform>::new(
        RecordingDecoder {
            commands: Arc::clone(&commands),
        },
        FakeImporter,
        FrameImportPolicy::RequireDirectSurface,
    );
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();

    system
        .set_presentation(id, PresentationVisibility::Hidden)
        .unwrap();
    system
        .set_presentation(id, PresentationVisibility::Hidden)
        .unwrap();
    system
        .set_presentation(id, PresentationVisibility::Presented)
        .unwrap();

    let commands = commands.lock().unwrap();
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, VideoCommand::Presentation { .. }))
            .count(),
        2
    );
    assert!(matches!(
        commands.last(),
        Some(VideoCommand::Presentation {
            id: command_id,
            visibility: PresentationVisibility::Presented,
        }) if *command_id == id
    ));
}

#[test]
fn hiding_a_session_retires_its_current_gpu_surface() {
    let id = VideoId::new(99);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 73, 0));
    system.service(Instant::now());
    assert_eq!(system.sampled(id), Some(&73));

    system
        .set_presentation(id, PresentationVisibility::Hidden)
        .unwrap();

    assert_eq!(system.sampled(id), None);
    assert_eq!(system.take_retired(), vec![73]);
}

#[test]
fn hidden_presentation_freezes_media_time_until_the_decoder_is_presented_again() {
    let id = VideoId::new(96);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);
    system
        .command_at(
            VideoCommand::Presentation {
                id,
                visibility: PresentationVisibility::Hidden,
            },
            now + Duration::from_nanos(10),
        )
        .unwrap();
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 1,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(1_000),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });
    system
        .command_at(
            VideoCommand::Playback {
                id,
                action: super::PlaybackAction::Play,
            },
            now + Duration::from_nanos(50),
        )
        .unwrap();
    // Native backends acknowledge accepted control commands asynchronously.
    // That acknowledgement must not restart only the common clock while the
    // native decoder remains visibility-paused.
    control.publish(BackendEvent::StateChanged {
        id,
        state: VideoSessionState::Playing,
    });
    assert!(
        system
            .service(now + Duration::from_nanos(60))
            .ready_frames
            .is_empty(),
        "the hidden acknowledgement itself must not present a frame"
    );

    assert!(
        system
            .service(now + Duration::from_nanos(100))
            .ready_frames
            .is_empty(),
        "a hidden decoder must not advance its media clock"
    );

    system
        .command_at(
            VideoCommand::Presentation {
                id,
                visibility: PresentationVisibility::Presented,
            },
            now + Duration::from_nanos(100),
        )
        .unwrap();
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 2,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(1_000),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });
    assert_eq!(
        system.service(now + Duration::from_nanos(110)).ready_frames,
        vec![VideoFrameReady {
            id,
            pts: MediaTime::from_nanos(20),
            frame_path: VideoFramePath::new(
                VideoDecodeResidency::Unknown,
                VideoCompositorImport::BorrowedNativeSurface,
                VideoPresentationPath::WgpuComposited,
            ),
        }]
    );
}

#[test]
fn hidden_autoplay_open_acknowledgement_keeps_the_media_clock_frozen() {
    let id = VideoId::new(98);
    let (mut system, control) = fake_system();
    let now = Instant::now();
    system
        .command_at(
            VideoCommand::Open {
                id,
                source: VideoSource::File("movie.mp4".into()),
                initial_playback: InitialPlayback::Playing,
                loop_mode: LoopMode::Off,
            },
            now,
        )
        .unwrap();
    system
        .command_at(
            VideoCommand::Presentation {
                id,
                visibility: PresentationVisibility::Hidden,
            },
            now,
        )
        .unwrap();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 1,
            decode_residency: VideoDecodeResidency::Unknown,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(1_000),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            output_generation: DecoderOutputGeneration::INITIAL,
            decoder_import: DecodedFrameImport::Deferred,
        },
    });

    assert!(
        system
            .service(now + Duration::from_nanos(100))
            .ready_frames
            .is_empty(),
        "a hidden Opened(Playing) acknowledgement must not start the clock"
    );
}

#[test]
fn recovery_manifest_comes_from_authoritative_playback_state() {
    let id = VideoId::new(95);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Count(std::num::NonZeroU32::new(2).unwrap()),
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    control.publish(fake_frame(id, 1, 25));
    control.publish(BackendEvent::Looped {
        id,
        remaining: LoopMode::Count(std::num::NonZeroU32::new(1).unwrap()),
    });
    system.service(now);
    system.service(now + Duration::from_nanos(25));
    let recovery_rate = PlaybackRate::new(1.5).unwrap();
    system
        .command_at(
            VideoCommand::Playback {
                id,
                action: super::PlaybackAction::SetRate(recovery_rate),
            },
            now + Duration::from_nanos(25),
        )
        .unwrap();
    system
        .command_at(
            VideoCommand::Playback {
                id,
                action: super::PlaybackAction::Pause,
            },
            now + Duration::from_nanos(25),
        )
        .unwrap();

    let recoveries = system.recovery_sessions_at(now + Duration::from_nanos(25));

    assert_eq!(recoveries.len(), 1);
    let recovery = &recoveries[0];
    assert_eq!(recovery.id(), id);
    let manifest = recovery.manifest();
    assert_eq!(manifest.source(), &VideoSource::File("movie.mp4".into()));
    assert_eq!(
        manifest.loop_mode(),
        LoopMode::Count(std::num::NonZeroU32::new(1).unwrap())
    );
    assert!(!manifest.desired_playing());
    assert_eq!(manifest.rate(), recovery_rate);
    assert_eq!(manifest.position(), MediaTime::from_nanos(25));
    assert_eq!(manifest.presentation(), PresentationVisibility::Presented);
}

#[test]
fn device_recovery_does_not_reopen_a_parked_hidden_session() {
    let id = VideoId::new(96);
    let (mut original, _) = fake_system();
    original
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    original
        .set_presentation(id, PresentationVisibility::Hidden)
        .unwrap();
    let recovery = original.recovery_sessions_at(Instant::now()).remove(0);
    let manifest = recovery.into_manifest();
    assert_eq!(manifest.presentation(), PresentationVisibility::Hidden);

    let (mut recovered, recovered_control) = fake_system();
    recovered.open_from_manifest(id, &manifest).unwrap();
    assert_eq!(recovered.state(id), None);

    let resumed = manifest.with_presentation(PresentationVisibility::Presented);
    recovered.open_from_manifest(id, &resumed).unwrap();
    assert_eq!(recovered.state(id), Some(VideoSessionState::Opening));
    recovered_control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Paused,
    });
    recovered.service(Instant::now());
    assert_eq!(
        recovered.recovery_sessions_at(Instant::now())[0]
            .manifest()
            .rate(),
        resumed.rate()
    );
}

#[test]
fn recovery_manifest_snapshots_media_clock_between_presented_frames() {
    let id = VideoId::new(97);
    let (mut system, control) = fake_system();
    system
        .command(VideoCommand::Open {
            id,
            source: VideoSource::File("movie.mp4".into()),
            initial_playback: InitialPlayback::Playing,
            loop_mode: LoopMode::Off,
        })
        .unwrap();
    let now = Instant::now();
    control.publish(BackendEvent::Opened {
        id,
        width: 1,
        height: 1,
        initial_state: VideoSessionState::Playing,
    });
    system.service(now);

    let recoveries = system.recovery_sessions_at(now + Duration::from_nanos(30));

    assert_eq!(
        recoveries[0].manifest().position(),
        MediaTime::from_nanos(30)
    );
}
