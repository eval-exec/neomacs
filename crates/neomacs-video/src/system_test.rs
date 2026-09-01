use super::{LoopMode, PlaybackRate, VideoModelError};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use neomacs_display_protocol::types::VideoId;

use super::backend::{
    BackendEvent, CompletedFrameTransfer, DecodedFrame, DecodedFrameTransfer, DecoderBackend,
    DecoderReconfiguration, FrameImportOutcome, FrameImporter, ImportedFrame, Platform,
    require_fixed_transfer_path,
};
use super::system::VideoSystemImpl;
use super::{
    FrameTiming, FrameTransferPolicy, GpuGeneration, InitialPlayback, MediaTime, PackedVideoFormat,
    PlaybackEpoch, PresentationVisibility, VideoColorimetry, VideoCommand, VideoEvent,
    VideoFrameFormat, VideoFrameReady, VideoGeometry, VideoInitError, VideoSessionState,
    VideoSource, VideoTransferPath,
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
fn transfer_policy_orders_direct_gpu_copy_and_cpu_fallback_explicitly() {
    assert!(
        FrameTransferPolicy::RequireDirectSurface.permits(VideoTransferPath::DirectExternalSurface)
    );
    assert!(!FrameTransferPolicy::RequireDirectSurface.permits(VideoTransferPath::GpuInteropCopy));
    assert!(FrameTransferPolicy::AllowGpuInteropCopy.permits(VideoTransferPath::GpuInteropCopy));
    assert!(!FrameTransferPolicy::AllowGpuInteropCopy.permits(VideoTransferPath::CpuUpload));
    assert!(FrameTransferPolicy::AllowCpuUpload.permits(VideoTransferPath::CpuUpload));
}

#[test]
fn fixed_native_path_is_rejected_before_platform_startup() {
    assert_eq!(
        require_fixed_transfer_path(
            super::VideoDecodeBackend::MediaFoundation,
            FrameTransferPolicy::RequireDirectSurface,
            VideoTransferPath::GpuInteropCopy,
        ),
        Err(VideoInitError::TransferForbidden {
            backend: super::VideoDecodeBackend::MediaFoundation,
            policy: FrameTransferPolicy::RequireDirectSurface,
            path: VideoTransferPath::GpuInteropCopy,
        })
    );
    assert_eq!(
        require_fixed_transfer_path(
            super::VideoDecodeBackend::MediaFoundation,
            FrameTransferPolicy::AllowGpuInteropCopy,
            VideoTransferPath::GpuInteropCopy,
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

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
        self.events.lock().unwrap().drain(..).collect()
    }
}

struct FakeImporter;

impl FrameImporter<u64> for FakeImporter {
    type Sampled = u64;

    fn transfer_path(&self, _frame: &DecodedFrame<u64>) -> VideoTransferPath {
        VideoTransferPath::DirectExternalSurface
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            transfer: CompletedFrameTransfer::DirectExternalSurface,
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

    fn transfer_path(&self, _frame: &DecodedFrame<u64>) -> VideoTransferPath {
        VideoTransferPath::GpuInteropCopy
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            transfer: CompletedFrameTransfer::GpuInteropCopy {
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
            FrameTransferPolicy::RequireDirectSurface,
        ),
        FakeControl { events },
    )
}

struct BackpressuredImporter;

impl FrameImporter<u64> for BackpressuredImporter {
    type Sampled = u64;

    fn transfer_path(&self, _frame: &DecodedFrame<u64>) -> VideoTransferPath {
        VideoTransferPath::DirectExternalSurface
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

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
        self.events.lock().unwrap().drain(..).collect()
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejected: VideoFrameFormat,
    ) -> Result<DecoderReconfiguration, String> {
        self.reconfigurations.lock().unwrap().push((id, rejected));
        Ok(DecoderReconfiguration::Applied)
    }
}

struct RecoveringImporter {
    attempts: usize,
}

impl FrameImporter<u64> for RecoveringImporter {
    type Sampled = u64;

    fn transfer_path(&self, _frame: &DecodedFrame<u64>) -> VideoTransferPath {
        VideoTransferPath::GpuInteropCopy
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        self.attempts += 1;
        if self.attempts == 1 {
            return Ok(FrameImportOutcome::ReconfigureDecoder {
                rejected: frame.format,
                reason: "native target rejected".to_owned(),
            });
        }
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            transfer: CompletedFrameTransfer::GpuInteropCopy {
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

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
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
fn service_imports_only_the_latest_due_frame_and_reports_its_timestamp() {
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
            timing: FrameTiming {
                pts: MediaTime::from_nanos(10),
                duration: MediaTime::from_nanos(10),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
        },
    });
    control.publish(BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease: 2,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(10),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
        },
    });

    let opened_at = Instant::now();
    let opening = system.service(opened_at);
    assert!(opening.ready_frames.is_empty());
    assert_eq!(
        opening.next_deadline,
        Some(opened_at + Duration::from_nanos(20))
    );

    let result = system.service(opened_at + Duration::from_nanos(20));
    assert_eq!(
        result.ready_frames,
        vec![VideoFrameReady {
            id,
            pts: MediaTime::from_nanos(20),
            transfer_path: VideoTransferPath::DirectExternalSurface,
        }]
    );
    assert_eq!(system.sampled(id), Some(&2));
    assert_eq!(
        system.session_diagnostics(),
        vec![super::VideoSessionDiagnostics {
            id,
            backend: super::VideoDecodeBackend::GStreamer,
            state: VideoSessionState::Playing,
            transfer_path: Some(VideoTransferPath::DirectExternalSurface),
            frame_format: Some(VideoFrameFormat::Packed(PackedVideoFormat::Rgba8)),
            colorimetry: Some(VideoColorimetry::SRGB),
            decoded_frames: 2,
            replaced_frames: 1,
            late_dropped_frames: 0,
            imported_frames: 1,
            backpressured_frames: 0,
            transfer_counts: super::VideoTransferCounts {
                direct_external_frames: 1,
                gpu_interop_copy_frames: 0,
                cpu_upload_frames: 0,
                reported_gpu_copy_bytes: 0,
                cpu_upload_bytes: 0,
            },
        }]
    );
}

#[test]
fn decoder_completed_transfers_are_counted_even_when_the_frame_is_replaced() {
    let id = VideoId::new(82);
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let control = FakeControl {
        events: Arc::clone(&events),
    };
    let mut system = VideoSystemImpl::<GpuCopyPlatform>::new(
        FakeDecoder { events },
        GpuCopyImporter,
        FrameTransferPolicy::AllowGpuInteropCopy,
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
        frame.decoder_transfer =
            DecodedFrameTransfer::Completed(CompletedFrameTransfer::GpuInteropCopy {
                reported_bytes: Some(4),
            });
        control.publish(BackendEvent::Frame { id, frame });
    }

    system.service(Instant::now());

    let diagnostics = system.session_diagnostics();
    assert_eq!(diagnostics[0].decoded_frames, 2);
    assert_eq!(diagnostics[0].replaced_frames, 1);
    assert_eq!(diagnostics[0].imported_frames, 1);
    assert_eq!(diagnostics[0].transfer_counts.gpu_interop_copy_frames, 2);
    assert_eq!(diagnostics[0].transfer_counts.reported_gpu_copy_bytes, 8);
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
        FrameTransferPolicy::RequireDirectSurface,
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
            timing: FrameTiming {
                pts: MediaTime::ZERO,
                duration: MediaTime::from_nanos(16_666_667),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
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
        FrameTransferPolicy::AllowGpuInteropCopy,
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

    control.publish(fake_frame(id, 2, 0));
    let second = system.service(now);
    assert_eq!(second.ready_frames.len(), 1);
    assert_eq!(system.sampled(id), Some(&2));
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
            timing: FrameTiming {
                pts: MediaTime::from_nanos(1_000_000_000),
                duration: MediaTime::from_nanos(41_666_667),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(320, 200),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
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
            timing: FrameTiming {
                pts: MediaTime::ZERO,
                duration: MediaTime::from_nanos(1),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
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
        FrameTransferPolicy::RequireDirectSurface,
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
    BackendEvent::Frame {
        id,
        frame: DecodedFrame {
            lease,
            timing: FrameTiming {
                pts: MediaTime::from_nanos(pts),
                duration: MediaTime::from_nanos(1),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
        },
    }
}

struct ForbiddenImporter {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl FrameImporter<u64> for ForbiddenImporter {
    type Sampled = u64;

    fn transfer_path(&self, _frame: &DecodedFrame<u64>) -> VideoTransferPath {
        VideoTransferPath::CpuUpload
    }

    fn import(
        &mut self,
        frame: DecodedFrame<u64>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled: frame.lease,
            transfer: CompletedFrameTransfer::CpuUpload { bytes: 4 },
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

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
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
fn strict_transfer_policy_rejects_a_frame_before_import_side_effects() {
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
        FrameTransferPolicy::RequireDirectSurface,
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
            error: super::VideoCommandError::TransferForbidden {
                policy: FrameTransferPolicy::RequireDirectSurface,
                path: VideoTransferPath::CpuUpload,
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
            timing: FrameTiming {
                pts: MediaTime::from_nanos(10),
                duration: MediaTime::from_nanos(5),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
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
            timing: FrameTiming {
                pts: MediaTime::ZERO,
                duration: MediaTime::from_nanos(1),
                epoch: PlaybackEpoch::INITIAL.next(),
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
        },
    });

    let current = system.service(now);

    assert_eq!(
        current.ready_frames,
        vec![VideoFrameReady {
            id,
            pts: MediaTime::ZERO,
            transfer_path: VideoTransferPath::DirectExternalSurface,
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

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
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

#[test]
fn presentation_visibility_is_a_deduplicated_native_decoder_input() {
    let id = VideoId::new(94);
    let commands = Arc::new(Mutex::new(Vec::new()));
    let mut system = VideoSystemImpl::<RecordingPlatform>::new(
        RecordingDecoder {
            commands: Arc::clone(&commands),
        },
        FakeImporter,
        FrameTransferPolicy::RequireDirectSurface,
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
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(1_000),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
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
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(1_000),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
        },
    });
    assert_eq!(
        system.service(now + Duration::from_nanos(110)).ready_frames,
        vec![VideoFrameReady {
            id,
            pts: MediaTime::from_nanos(20),
            transfer_path: VideoTransferPath::DirectExternalSurface,
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
            timing: FrameTiming {
                pts: MediaTime::from_nanos(20),
                duration: MediaTime::from_nanos(1_000),
                epoch: PlaybackEpoch::INITIAL,
            },
            geometry: VideoGeometry::packed(1, 1),
            format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
            colorimetry: VideoColorimetry::SRGB,
            decoder_transfer: DecodedFrameTransfer::Deferred,
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
