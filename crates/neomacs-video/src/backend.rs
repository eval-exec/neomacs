use neomacs_display_protocol::types::VideoId;
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(target_os = "linux")]
use crossbeam_channel::{Receiver, Sender};

#[cfg(target_os = "linux")]
use crate::mailbox::PresentationFrameQueue;
use crate::sampling::GpuVideoContext;
use crate::system::VideoWake;
use crate::{
    FrameTiming, VideoColorimetry, VideoCommand, VideoCommandError, VideoCompositorImport,
    VideoFrameFormat, VideoGeometry, VideoInitError, VideoSessionState,
};

/// Decoder output with all information needed to replay the exact frame.
pub(crate) struct DecodedFrame<F> {
    pub(crate) lease: F,
    pub(crate) decode_residency: crate::VideoDecodeResidency,
    pub(crate) timing: FrameTiming,
    pub(crate) geometry: VideoGeometry,
    pub(crate) format: VideoFrameFormat,
    pub(crate) colorimetry: VideoColorimetry,
    /// Identity of the decoder-output representation that produced this
    /// frame. This is deliberately independent of [`crate::PlaybackEpoch`]:
    /// seeks change media position, while output generations change the
    /// native surface contract used by the GPU importer.
    pub(crate) output_generation: DecoderOutputGeneration,
    /// Compositor import work already completed before this frame entered the
    /// common bounded presentation queue. Keeping this state on the affine frame
    /// prevents replacement or late-drop paths from hiding real GPU work.
    pub(crate) decoder_import: DecodedFrameImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DecoderOutputGeneration(u32);

impl DecoderOutputGeneration {
    pub(crate) const INITIAL: Self = Self(0);

    pub(crate) fn next(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("decoder output generation space exhausted"),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecoderOutputRejection {
    pub(crate) generation: DecoderOutputGeneration,
    pub(crate) format: VideoFrameFormat,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecodedFrameImport {
    Deferred,
    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    Completed(CompletedFrameImport),
}

impl DecodedFrameImport {
    pub(crate) const fn completed(self) -> Option<CompletedFrameImport> {
        match self {
            Self::Deferred => None,
            Self::Completed(completed_import) => Some(completed_import),
        }
    }

    pub(crate) const fn path(self) -> Option<VideoCompositorImport> {
        match self.completed() {
            Some(completed_import) => Some(completed_import.path()),
            None => None,
        }
    }
}

/// Event emitted by a platform decoder adapter.
pub(crate) enum BackendEvent<F> {
    Opened {
        id: VideoId,
        width: u32,
        height: u32,
        initial_state: VideoSessionState,
    },
    /// Codec implementation selected by a backend that can expose it without
    /// leaking a native handle. Backends that cannot prove this leave the
    /// session identity absent rather than guessing from output residency.
    DecoderSelected {
        id: VideoId,
        decoder: crate::VideoDecoderIdentity,
    },
    Frame {
        id: VideoId,
        frame: DecodedFrame<F>,
    },
    /// Frames coalesced from the native bounded presentation queue before the compositor
    /// serviced it. This aggregates instead of turning diagnostic accounting
    /// into an unbounded per-frame control queue.
    #[cfg(any(target_os = "linux", test))]
    FramesReplaced {
        id: VideoId,
        count: u64,
    },
    /// Decoder output caps moved once to a lower, explicitly constrained tier.
    #[cfg(any(target_os = "linux", test))]
    OutputReconfigured {
        id: VideoId,
        generation: DecoderOutputGeneration,
    },
    StateChanged {
        id: VideoId,
        state: VideoSessionState,
    },
    /// Native loop handling consumed one replay permission.
    Looped {
        id: VideoId,
        remaining: crate::LoopMode,
    },
    Ended {
        id: VideoId,
    },
    Failed {
        id: VideoId,
        error: VideoCommandError,
    },
}

/// Whether a queued backend event crosses a diagnostic measurement boundary.
///
/// Lifecycle and capability events must remain observable after counters are
/// reset. Frame evidence produced before the acknowledged boundary must not be
/// counted in the new epoch. Keeping this as a closed enum, and classifying
/// events with an exhaustive match, makes every newly added event choose its
/// behavior explicitly.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeasurementEpochDisposition {
    Retain,
    Discard,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
impl MeasurementEpochDisposition {
    pub(crate) const fn retains_event(self) -> bool {
        matches!(self, Self::Retain)
    }
}

impl<F> BackendEvent<F> {
    pub(crate) const fn id(&self) -> VideoId {
        match self {
            Self::Opened { id, .. }
            | Self::DecoderSelected { id, .. }
            | Self::Frame { id, .. }
            | Self::StateChanged { id, .. }
            | Self::Looped { id, .. }
            | Self::Ended { id }
            | Self::Failed { id, .. } => *id,
            #[cfg(any(target_os = "linux", test))]
            Self::FramesReplaced { id, .. } | Self::OutputReconfigured { id, .. } => *id,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    pub(crate) const fn measurement_epoch_disposition(&self) -> MeasurementEpochDisposition {
        match self {
            Self::Frame { .. } => MeasurementEpochDisposition::Discard,
            #[cfg(any(target_os = "linux", test))]
            Self::FramesReplaced { .. } => MeasurementEpochDisposition::Discard,
            Self::Opened { .. }
            | Self::DecoderSelected { .. }
            | Self::StateChanged { .. }
            | Self::Looped { .. }
            | Self::Ended { .. }
            | Self::Failed { .. } => MeasurementEpochDisposition::Retain,
            #[cfg(any(target_os = "linux", test))]
            Self::OutputReconfigured { .. } => MeasurementEpochDisposition::Retain,
        }
    }
}

pub(crate) trait DecoderBackend {
    type Frame;

    fn command(&mut self, command: VideoCommand) -> Result<(), VideoCommandError>;

    /// Advance native playback without blocking and return newly available
    /// events. Pull-based adapters use the presentation target to select the
    /// frame the compositor is about to show; push-based adapters only drain
    /// their bounded event bridge.
    fn service(&mut self, request: &crate::VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>>;

    /// Ask the decoder to replace an output representation that the GPU
    /// importer rejected.  Most backends have no in-place fallback; the
    /// explicit result prevents an expected native-format rejection from
    /// being confused with a terminal decoder error.
    fn reconfigure_after_import_failure(
        &mut self,
        _id: VideoId,
        _rejection: &DecoderOutputRejection,
    ) -> Result<DecoderReconfiguration, String> {
        Ok(DecoderReconfiguration::Unsupported)
    }

    /// Earliest time at which the platform adapter needs another service
    /// pass even when it has not published a cross-thread wake event.
    ///
    /// Most decoders push frame availability and therefore return `None`.
    /// Pull-based native APIs use this seam to participate in the common
    /// render scheduler instead of installing an independent display clock.
    fn next_service_deadline(&self, _now: Instant) -> Option<Instant> {
        None
    }

    /// Snapshot a decoder-owned native surface pool, when this backend owns
    /// one. The default makes the absence of a pool explicit without forcing
    /// fake or push-only decoders to manufacture telemetry.
    fn surface_pool_diagnostics(&self) -> Option<crate::VideoSurfacePoolDiagnostics> {
        None
    }

    /// Reset observation-only counters at an acknowledged benchmark boundary.
    fn begin_measurement_epoch(&mut self) {}
}

#[cfg(target_os = "linux")]
pub(crate) struct BackendPublisher<F> {
    events: Sender<BackendEvent<F>>,
    latest_frames: Arc<Mutex<HashMap<VideoId, PublishedFrameQueue<F>>>>,
    measurement_epoch: Arc<AtomicU64>,
    wake: VideoWake,
}

#[cfg(target_os = "linux")]
impl<F> Clone for BackendPublisher<F> {
    fn clone(&self) -> Self {
        Self {
            events: self.events.clone(),
            latest_frames: Arc::clone(&self.latest_frames),
            measurement_epoch: Arc::clone(&self.measurement_epoch),
            wake: self.wake.clone(),
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) struct BackendInbox<F> {
    events: Receiver<BackendEvent<F>>,
    latest_frames: Arc<Mutex<HashMap<VideoId, PublishedFrameQueue<F>>>>,
    measurement_epoch: Arc<AtomicU64>,
}

/// Observation generation captured before a decoder begins pulling a frame.
///
/// Unlike playback epochs, this has no media meaning. It makes an acknowledged
/// benchmark boundary reject work that began before the boundary but reached
/// the cross-thread publisher afterward.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackendMeasurementEpoch(u64);

#[cfg(target_os = "linux")]
struct PublishedFrameQueue<F> {
    frames: PresentationFrameQueue<DecodedFrame<F>>,
    replaced: u64,
}

#[cfg(target_os = "linux")]
pub(crate) fn backend_bridge<F>(wake: VideoWake) -> (BackendPublisher<F>, BackendInbox<F>) {
    let (events, incoming) = crossbeam_channel::unbounded();
    let latest_frames = Arc::new(Mutex::new(HashMap::new()));
    let measurement_epoch = Arc::new(AtomicU64::new(0));
    (
        BackendPublisher {
            events,
            latest_frames: Arc::clone(&latest_frames),
            measurement_epoch: Arc::clone(&measurement_epoch),
            wake,
        },
        BackendInbox {
            events: incoming,
            latest_frames,
            measurement_epoch,
        },
    )
}

#[cfg(target_os = "linux")]
impl<F> BackendPublisher<F> {
    pub(crate) fn event(&self, event: BackendEvent<F>) {
        if self.events.send(event).is_ok() {
            self.wake.notify();
        }
    }

    pub(crate) fn measurement_epoch(&self) -> BackendMeasurementEpoch {
        BackendMeasurementEpoch(self.measurement_epoch.load(Ordering::Acquire))
    }

    pub(crate) fn frame(
        &self,
        epoch: BackendMeasurementEpoch,
        id: VideoId,
        frame: DecodedFrame<F>,
    ) {
        let mut latest = lock_unpoisoned(&self.latest_frames);
        if epoch.0 != self.measurement_epoch.load(Ordering::Acquire) {
            return;
        }
        let published = latest.entry(id).or_insert_with(|| PublishedFrameQueue {
            frames: PresentationFrameQueue::default(),
            replaced: 0,
        });
        if published.frames.publish(frame).is_some() {
            published.replaced = published.replaced.saturating_add(1);
        }
        drop(latest);
        self.wake.notify();
    }
}

#[cfg(target_os = "linux")]
impl<F> BackendInbox<F> {
    pub(crate) fn drain(&self) -> Vec<BackendEvent<F>> {
        let mut events: Vec<_> = self.events.try_iter().collect();
        let frames = std::mem::take(&mut *lock_unpoisoned(&self.latest_frames));
        for (id, mut published) in frames {
            while let Some(frame) = published.frames.take() {
                events.push(BackendEvent::Frame { id, frame });
            }
            if published.replaced != 0 {
                events.push(BackendEvent::FramesReplaced {
                    id,
                    count: published.replaced,
                });
            }
        }
        events
    }

    pub(crate) fn remove_frame(&self, id: VideoId) {
        lock_unpoisoned(&self.latest_frames).remove(&id);
    }

    /// Advance the accepted observation generation and drop frames published
    /// before the boundary while retaining ordered lifecycle/failure events.
    /// Holding the frame-map lock across both operations closes the race with
    /// a producer publishing an old-generation frame.
    pub(crate) fn begin_measurement_epoch(&self) {
        let mut frames = lock_unpoisoned(&self.latest_frames);
        self.measurement_epoch.fetch_add(1, Ordering::AcqRel);
        frames.clear();
    }
}

#[cfg(target_os = "linux")]
fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "backend_bridge_test.rs"]
mod bridge_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecoderReconfiguration {
    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    Applied {
        generation: DecoderOutputGeneration,
    },
    /// The decoder has already moved past the rejected output generation.
    /// The caller should discard this stale frame without counting another
    /// transition or poisoning the live session.
    Superseded,
    Unsupported,
}

pub(crate) struct ImportedFrame<S> {
    pub(crate) sampled: S,
    pub(crate) completed_import: CompletedFrameImport,
}

/// What the importer actually did, including byte volume only where the
/// platform API makes it observable. The enum prevents impossible states such
/// as reporting CPU-upload bytes for a direct external surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletedFrameImport {
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", test)),
        allow(dead_code)
    )]
    BorrowedNativeSurface,
    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    GpuBlit { reported_bytes: Option<u64> },
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    CpuUpload { bytes: u64 },
}

impl CompletedFrameImport {
    pub(crate) const fn path(self) -> VideoCompositorImport {
        match self {
            Self::BorrowedNativeSurface => VideoCompositorImport::BorrowedNativeSurface,
            Self::GpuBlit { .. } => VideoCompositorImport::GpuBlit,
            Self::CpuUpload { .. } => VideoCompositorImport::CpuUpload,
        }
    }
}

/// Validate a platform whose native API has one unavoidable compositor import.
///
/// This check belongs before decoder construction because some native
/// decoders materialize their output before the common importer is called.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", test)),
    allow(dead_code)
)]
pub(crate) fn require_fixed_compositor_import(
    backend: crate::VideoDecodeBackend,
    policy: crate::FrameImportPolicy,
    path: VideoCompositorImport,
) -> Result<(), VideoInitError> {
    if policy.permits(path) {
        Ok(())
    } else {
        Err(VideoInitError::ImportForbidden {
            backend,
            policy,
            path,
        })
    }
}

/// Result of attempting to import one decoded frame into compositor-owned
/// GPU state. Backpressure is an expected bounded-resource condition: the
/// caller drops this already-stale decoded frame and waits for the next one
/// instead of poisoning the playback session.
pub(crate) enum FrameImportOutcome<S> {
    Ready(ImportedFrame<S>),
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Backpressured,
    /// The representation is valid decoder output but unusable by the active
    /// GPU interop stack.  The common system keeps playback alive only when
    /// the decoder confirms that it installed a lower-tier representation.
    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    ReconfigureDecoder {
        rejection: DecoderOutputRejection,
    },
}

pub(crate) trait FrameImporter<F> {
    type Sampled;

    /// Classify compositor work before performing native import, mapping, copy,
    /// or upload work. The policy boundary uses this to reject forbidden
    /// fallback paths without causing their side effects first.
    fn compositor_import(&self, frame: &DecodedFrame<F>) -> VideoCompositorImport;

    fn import(
        &mut self,
        frame: DecodedFrame<F>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String>;

    /// Snapshot an importer-owned native surface pool, when present.
    fn surface_pool_diagnostics(&self) -> Option<crate::VideoSurfacePoolDiagnostics> {
        None
    }

    /// Reset observation-only counters without discarding reusable surfaces.
    fn begin_measurement_epoch(&mut self) {}
}

pub(crate) trait Platform {
    const BACKEND: crate::VideoDecodeBackend;
    type Frame;
    type Sampled;
    type Decoder: DecoderBackend<Frame = Self::Frame>;
    type Importer: FrameImporter<Self::Frame, Sampled = Self::Sampled>;
}

pub(crate) trait ProductionPlatform: Platform {
    fn create(
        gpu: GpuVideoContext,
        policy: crate::FrameImportPolicy,
        wake: VideoWake,
    ) -> Result<(Self::Decoder, Self::Importer), VideoInitError>;
}
