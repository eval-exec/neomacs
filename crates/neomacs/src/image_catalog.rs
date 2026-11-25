//! Evaluator-owned asynchronous image catalog.
//!
//! Ordinary redisplay only mutates evaluator-local state and probes renderer
//! completion with `try_lock`. Queue backpressure is handed to one submission
//! worker, so lookup never waits for the renderer thread.
//!
//! Geometry is resolved the same way: a decode reports its size only when it
//! finishes, so a second worker reads the encoded header and publishes the
//! layout that decode will confirm. Redisplay picks that layout up on its next
//! `try_lock` probe — it never waits for it — which is what lets a large image
//! reserve its real box while its pixels are still being decoded.
//!
//! Synchronous Lisp metadata queries observe the shared CPU decoder directly;
//! GPU acquisition and texture residency do not gate full image validation.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use neomacs_display_protocol::image_diagnostic::ImageDiagnostic;
use neomacs_display_protocol::{
    ImageSequenceId, ImageSequenceRetirement, ProvisionalExtent, SlotChange,
};
use neomacs_display_runtime::render_thread::{
    ImageDecodeClient, ImageDecodeTerminal, ImageProbeSource, ImageTerminalProbe,
    SharedImageRenderState, probe_image_layout_for_frame,
};
use neomacs_display_runtime::thread_comm::{AssetCommand, RenderCommand, SendTimeoutOutcome};
use neovm_core::emacs_core::image_catalog::{
    FailedImage, ImageAnimationInvalidation, ImageCatalog, ImageFileName, ImageId,
    ImageInvalidation, ImageInvalidationResult, ImageLayoutExtent, ImageLoadAttempt,
    ImageLoadToken, ImageLookup, ImagePlacement, ImageResolveRequest, ImageResolveSource,
    ImageSizeLimit, ImageStateEvent, PendingImage, ReadyImage,
};
use neovm_core::emacs_core::image_path::ImageFileRequest;
use neovm_core::emacs_core::load::image_data_directory;

use super::GuiEventLoopWaker;

const HOST_IMAGE_ID_START: u32 = 0x4000_0000;
static HOST_IMAGE_ID_ALLOCATOR: AtomicU32 = AtomicU32::new(HOST_IMAGE_ID_START);

fn next_host_image_id() -> ImageId {
    let raw = HOST_IMAGE_ID_ALLOCATOR
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("host image identity space exhausted");
    ImageId::new(raw)
}

/// Catalog-owned lifecycle. `Evicted` is deliberately not exposed through
/// [`ImageCatalog`]: the next lookup atomically schedules a reload and returns
/// the ordinary `Pending` state to redisplay.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CatalogEntry {
    Pending(PendingImage),
    Resident(ReadyImage),
    Failed(FailedImage),
    Evicted(ImagePlacement),
}

impl CatalogEntry {
    fn from_lookup(state: ImageLookup) -> Self {
        match state {
            ImageLookup::Pending(image) => Self::Pending(image),
            ImageLookup::Ready(image) => Self::Resident(image),
            ImageLookup::Failed(image) => Self::Failed(image),
        }
    }

    fn as_lookup(&self) -> Option<ImageLookup> {
        match self {
            Self::Pending(image) => Some(ImageLookup::Pending(image.clone())),
            Self::Resident(image) => Some(ImageLookup::Ready(image.clone())),
            Self::Failed(image) => Some(ImageLookup::Failed(image.clone())),
            Self::Evicted(_) => None,
        }
    }

    fn placement(&self) -> ImagePlacement {
        match self {
            Self::Pending(image) => image.placement(),
            Self::Resident(image) => ImagePlacement::new(image.image_id(), image.metadata.layout),
            Self::Failed(image) => image.placement(),
            Self::Evicted(placement) => *placement,
        }
    }
}

/// Layout resolved from an encoded header, tagged with the load it was
/// resolved for.
///
/// The tag is the probe's identity check: an entry that was invalidated or
/// re-queued decodes under a fresh [`ImageLoadToken`], so a probe still in
/// flight for the previous one is ignored rather than adopted. `None` records
/// a header this crate could not measure, so a request is probed once.
type HeaderLayouts =
    Arc<Mutex<HashMap<ImageResolveRequest, (ImageLoadToken, Option<ImageLayoutExtent>)>>>;

/// Asks the evaluator to republish layout from a producer thread.
///
/// Header geometry arrives on the prober's thread, but only the evaluator owns
/// layout. Without this wake the resolved box would wait for whatever redisplay
/// happened next — exactly the wait this step removes.
#[derive(Clone)]
pub(super) struct RedisplayWaker {
    input: neovm_core::host_input::HostInputPublisher,
}

impl RedisplayWaker {
    pub(super) fn new(input: neovm_core::host_input::HostInputPublisher) -> Self {
        Self { input }
    }

    /// Only the evaluator republishes layout after off-thread image geometry.
    fn request_redisplay(&self) {
        let _ = super::input_bridge::finish_host_input_publication(
            self.input
                .send(neovm_core::keyboard::InputEvent::LayoutInvalidated),
        );
    }
}

/// Deep host-side module that owns image request identity, state transitions,
/// renderer scheduling, and completion observation.
pub(super) struct AsyncImageCatalog {
    cmd_tx: neomacs_display_runtime::thread_comm::RenderCommandSender,
    render_waker: Option<GuiEventLoopWaker>,
    image_metadata: SharedImageRenderState,
    image_decode_client: ImageDecodeClient,
    /// Geometry read from encoded headers, off-thread (see [`HeaderProbeRequest`]).
    header_layouts: HeaderLayouts,
    /// Woken once per header probe that produced geometry.
    redisplay_waker: Option<RedisplayWaker>,
    entries: RefCell<HashMap<ImageResolveRequest, CatalogEntry>>,
    /// Resolved geometry keyed by everything the header probe reads *except*
    /// the animation frame (see [`geometry_key`]).
    ///
    /// A raster source's frames composite onto one canvas, so the probe's
    /// answer cannot depend on which frame is selected. Keying without the
    /// frame lets frame N+1 of an animation start life already resolved instead
    /// of reserving a provisional extent and moving when its own probe lands —
    /// which, at animation rates, is a layout shift the user sees.
    geometry_layouts: Mutex<HashMap<ImageResolveRequest, ImageLayoutExtent>>,
    /// Slots that moved when resolved geometry landed (see [`SlotChange`]).
    ///
    /// The invariant is zero for a spec that pins both axes: the reservation is
    /// computed by the same resolution the header and the decoder use. Anything
    /// else is a geometry disagreement being paid for by a frame.
    slot_moves: Cell<u64>,
    sequence_ids: RefCell<HashMap<ImageResolveSource, ImageSequenceId>>,
    /// An invalidation or animation prune has been queued, so the published
    /// usage snapshot is known stale.  The first `image-cache-size` after it
    /// waits for the render thread to execute the ordered invalidation
    /// commands and publish the post-invalidation total; ordinary reads stay
    /// snapshot-only.
    usage_sync_pending: Cell<bool>,
    /// The pending sync covers a whole-cache clear. GNU frees that cache on
    /// the caller's thread, so its post state is zero even when the renderer
    /// cannot answer (exited, or wedged past the read's bound).
    usage_sync_clear_all: Cell<bool>,
    /// Ordered invalidation commands admitted but not yet committed (the
    /// renderer did not make room before the bound). They are retried in FIFO
    /// order before the next publish, so a slow renderer cannot turn an
    /// invalidation into a permanently stale total; an exited renderer clears
    /// the queue because no later delivery is possible.
    pending_invalidations: RefCell<VecDeque<QueuedRenderWork>>,
    /// A resolution-requiring load handed to the image-command submission
    /// worker. While it is set no later queued work may be sent: the worker
    /// commits the load and acknowledges, preserving queue order.
    ordered_in_flight: RefCell<Option<std::sync::mpsc::Receiver<()>>>,
    /// Loads that were already in flight when the last whole-cache clear was
    /// admitted. Their late terminal reports describe images the clear freed,
    /// so they must not re-add bytes to the known post-clear zero.
    pre_clear_loads: RefCell<Vec<ImageLoadToken>>,
    next_load_attempt: Cell<u64>,
    next_sequence_id: Cell<u64>,
    /// The `max-image-size` bound the most recent lookup resolved.
    ///
    /// Only the device-loss re-queue needs it: that path rebuilds every
    /// entry's load command without a frame in hand, and it runs while the
    /// same frames are being redisplayed, so the bound those entries were
    /// built under is the one to re-check them against. It starts at GNU's
    /// registered initializer rather than at "no limit", so a re-queue can
    /// never be the one path that loads without a bound.
    size_limit: Cell<ImageSizeLimit>,
    home_directory: Option<String>,
    /// GNU `image_find_image_fd` search path (`data-directory/images`, then
    /// `x-bitmap-file-path`), used to resolve relative image `:file`s.
    search_path: Vec<String>,
    /// Failures a display path has observed and the evaluator has not yet
    /// logged, in observation order.
    ///
    /// Filled inside `lookup` rather than at its call sites, which is the
    /// point: a consumer that only wants the placeholder geometry would
    /// otherwise drop the reason, and that is exactly how this codebase came
    /// to have no image-failure path at all.
    failed_diagnostics: RefCell<Vec<ImageDiagnostic>>,
    /// The load attempt each image's failure has already been reported for.
    ///
    /// GNU reports from inside `lookup_image` and can afford to, because its
    /// display iterator does not run between glyph regenerations. Neomacs'
    /// layout consults this catalog once per pass whether or not anything
    /// changed, so a report per lookup would grow a line in *Messages* on
    /// every redisplay tick. Keying the report on the load attempt keeps the
    /// cadence a user sees the same as GNU's: one line when the image first
    /// fails to draw, none while the frame merely redisplays, and another
    /// whenever the image is loaded again.
    reported_failures: RefCell<HashMap<ImageId, ImageLoadToken>>,
}

impl AsyncImageCatalog {
    pub(super) fn new(
        cmd_tx: neomacs_display_runtime::thread_comm::RenderCommandSender,
        render_waker: Option<GuiEventLoopWaker>,
        image_metadata: SharedImageRenderState,
        redisplay_waker: Option<RedisplayWaker>,
        image_decode_client: ImageDecodeClient,
    ) -> Self {
        Self {
            cmd_tx,
            render_waker,
            image_metadata,
            image_decode_client,
            header_layouts: Arc::new(Mutex::new(HashMap::new())),
            redisplay_waker,
            entries: RefCell::new(HashMap::new()),
            geometry_layouts: Mutex::new(HashMap::new()),
            slot_moves: Cell::new(0),
            sequence_ids: RefCell::new(HashMap::new()),
            usage_sync_pending: Cell::new(false),
            usage_sync_clear_all: Cell::new(false),
            pending_invalidations: RefCell::new(VecDeque::new()),
            ordered_in_flight: RefCell::new(None),
            pre_clear_loads: RefCell::new(Vec::new()),
            next_load_attempt: Cell::new(0),
            next_sequence_id: Cell::new(0),
            size_limit: Cell::new(ImageSizeLimit::default()),
            home_directory: home_directory_from_environment(),
            search_path: vec![image_data_directory().to_string_lossy().into_owned()],
            failed_diagnostics: RefCell::new(Vec::new()),
            reported_failures: RefCell::new(HashMap::new()),
        }
    }

    fn next_load(&self, image: ImageId) -> ImageLoadToken {
        let attempt = self
            .next_load_attempt
            .get()
            .checked_add(1)
            .expect("image load attempt space exhausted");
        self.next_load_attempt.set(attempt);
        ImageLoadToken::new(
            image,
            ImageLoadAttempt::new(attempt).expect("incremented attempt is non-zero"),
        )
    }

    fn sequence_id(&self, source: &ImageResolveSource) -> ImageSequenceId {
        // Source identity deliberately excludes `:index` and every realization
        // field: one encoded source owns one decoder/compositor sequence while
        // its individual frame textures remain full-spec catalog entries.
        if let Some(sequence) = self.sequence_ids.borrow().get(source).copied() {
            return sequence;
        }
        let raw = self
            .next_sequence_id
            .get()
            .checked_add(1)
            .expect("image sequence identity space exhausted");
        self.next_sequence_id.set(raw);
        let sequence = ImageSequenceId::new(raw).expect("incremented sequence id is non-zero");
        self.sequence_ids
            .borrow_mut()
            .insert(source.clone(), sequence);
        sequence
    }

    /// One decode of an image `:file`: classify it into an [`ImageFileRequest`]
    /// and rewrite the request's source to its [`ImageFileRequest::cache_key`]
    /// (the stable string entries dedup on). Returns the classification so the
    /// caller can route [`ImageFileRequest::needs_off_thread`] requests to the
    /// submission worker. `Data` sources carry no path and return `None`.
    fn classify_request(
        &self,
        mut request: ImageResolveRequest,
    ) -> (ImageResolveRequest, Option<ImageFileRequest>) {
        let (source, resolution) = self.classify_source(request.source);
        request.source = source;
        (request, resolution)
    }

    fn classify_source(
        &self,
        source: ImageResolveSource,
    ) -> (ImageResolveSource, Option<ImageFileRequest>) {
        if let ImageResolveSource::File(path) = &source
            && let Some(path_str) = path.as_utf8_str()
        {
            let resolution = ImageFileRequest::classify(
                path_str,
                self.home_directory.as_deref(),
                self.search_path.clone(),
            );
            return (
                ImageResolveSource::File(ImageFileName::from_utf8(resolution.cache_key())),
                Some(resolution),
            );
        }
        (source, None)
    }

    /// Re-queue every known entry for decode + upload after the renderer's
    /// image cache was destroyed by a GPU device loss.
    ///
    /// The catalog's map keys are the full [`ImageResolveRequest`]s (source
    /// bytes/path plus sizing/realization), so each entry can rebuild its
    /// exact original load command. Entries and their image ids are KEPT —
    /// published frames still reference those ids, so re-uploading under the
    /// same id re-textures the renderer's retained CPU frame as soon as the
    /// decode lands, without waiting for a fresh redisplay. Every entry moves
    /// to `Pending` while its renderer residency is rebuilt.
    pub(super) fn invalidate_all(&self) {
        let mut entries = self.entries.borrow_mut();
        // No frame is in hand on this path; re-check every entry against the
        // bound the redisplay that built it resolved (see `size_limit`).
        let limit = self.size_limit.get();
        for (request, state) in entries.iter_mut() {
            let placement = state.placement();
            let image_id = placement.image_id();
            let load = self.next_load(image_id);
            let (request, resolution) = self.classify_request(request.clone());
            let command =
                image_load_command(&request, load, self.sequence_id(&request.source), limit);
            let pending = PendingImage::new(load, self.renew_header_layout(&request, load));
            *state = match self.schedule_image_command_ordered(command, resolution.as_ref()) {
                Ok(()) => CatalogEntry::Pending(pending),
                Err(error) => {
                    tracing::warn!(
                        image_id = %image_id,
                        %error,
                        "failed to re-queue image decode after display reset"
                    );
                    CatalogEntry::Failed(pending.failed(ImageDiagnostic::NotDrawable))
                }
            };
        }
    }

    pub(super) fn resolve_sync(
        &self,
        request: ImageResolveRequest,
        limit: ImageSizeLimit,
    ) -> Result<Option<ReadyImage>, String> {
        let pending = match self.lookup_inner(request, limit) {
            ImageLookup::Ready(image) => return Ok(Some(image)),
            ImageLookup::Pending(image) => image,
            ImageLookup::Failed(failed) => {
                // `image-size` asks about one image and GNU logs every time it
                // is asked, so this path reports even a failure the display
                // side has already reported.
                self.record_failure_always(&failed);
                return Err(failed.error.message());
            }
        };
        let placement = pending.placement();

        let Some(terminal) = self
            .image_decode_client
            .wait_for_terminal(pending.load(), Duration::from_secs(1))
        else {
            // Bounded wait: do not invent dimensions. Callers (image-size, etc.)
            // surface this as a failed resolve rather than a wrong pixel size.
            return Err(format!(
                "Timed out waiting for image decode (id {})",
                placement.image_id()
            ));
        };
        // CPU decoding answers Lisp metadata queries independently of texture
        // upload. Leave the catalog's residency pending until the renderer's
        // normal completion event reconciles it.
        match terminal {
            Ok(metadata) => Ok(Some(ReadyImage {
                load: pending.load(),
                metadata: neomacs_display_protocol::ResolvedImageMetadata {
                    layout: metadata.layout,
                    reported: metadata.reported,
                    background: metadata.background,
                    background_transparent: metadata.background_transparent,
                    mask: metadata.mask,
                    embedded: metadata.embedded,
                },
            })),
            Err(error) => Err(error.message()),
        }
    }
}

/// How a post-invalidation cache-size read obtained its answer.
enum CacheUsageSync {
    /// The renderer published the post-invalidation total.
    Synchronized,
    /// The renderer has no GPU yet, so it can retain no image bytes: the
    /// catalog's own snapshot (or the known post-clear zero) is the answer.
    BeforeRenderer,
    /// The renderer exited or the command channel disconnected. No
    /// acknowledgement can arrive.
    RenderThreadGone,
    /// A live renderer did not answer before the read's bound.
    TimedOut,
}

/// Poll interval for the publish acknowledgement. It is also the worst-case
/// delay before an evaluator already waiting notices that the renderer
/// exited.
const CACHE_USAGE_SYNC_POLL: Duration = Duration::from_millis(10);
/// Overall bound for one read. A live renderer may legitimately be inside a
/// synchronous device-loss reinstall or a blocking surface operation, so the
/// bound is generous; it exists so a wedged renderer cannot stall Lisp
/// forever. A whole-cache clear still answers with GNU's post-clear zero.
const CACHE_USAGE_SYNC_DEADLINE: Duration = Duration::from_secs(10);

/// One ordered render command and, for an image load, the off-thread
/// `:file` resolution it needs. Invalidations carry no resolution.
struct QueuedRenderWork {
    command: RenderCommand,
    resolution: Option<ImageFileRequest>,
}

/// Where an ordered invalidation stands after an admission attempt.
enum InvalidationDelivery {
    /// Every queued invalidation, including this one, is committed.
    Committed,
    /// This invalidation is queued for retry on the next sync: a live
    /// renderer did not make room before the bound.
    Queued,
    /// The renderer is gone; no queued invalidation can ever be delivered.
    Unavailable,
}

impl AsyncImageCatalog {
    /// Ask the render thread to publish its retained-image usage now and wait
    /// for the acknowledgement.
    ///
    /// A successful return proves the published total is the
    /// post-invalidation state because the ordered invalidation commands sit
    /// ahead of the publish in the same FIFO. GNU frees the cache
    /// synchronously on the caller's thread; a renderer that is slow must
    /// delay the read, not let it observe the pre-invalidation total.
    ///
    /// The wait is liveness-plumbed rather than unbounded: before the GPU
    /// exists there is nothing to wait for, and after the renderer exits an
    /// acknowledgement still queued behind the evaluator's sender clones can
    /// never arrive.
    fn synchronize_cache_usage(&self) -> CacheUsageSync {
        if self.image_metadata.renderer_exited() {
            return CacheUsageSync::RenderThreadGone;
        }
        if !self.image_metadata.renderer_can_retain_images() {
            return CacheUsageSync::BeforeRenderer;
        }
        let (ack, wait) = std::sync::mpsc::sync_channel(1);
        let mut command = RenderCommand::Asset(AssetCommand::PublishImageCacheUsage { ack });
        let deadline = Instant::now() + CACHE_USAGE_SYNC_DEADLINE;
        // A publish must sit behind every invalidation issued before it. A
        // queued invalidation the renderer still has no room for makes this
        // read time out (bounded) instead of letting the publish jump it and
        // observe a pre-invalidation total.
        match self.flush_pending_invalidations(deadline) {
            InvalidationDelivery::Unavailable => return CacheUsageSync::RenderThreadGone,
            InvalidationDelivery::Queued => return CacheUsageSync::TimedOut,
            InvalidationDelivery::Committed => {}
        }
        loop {
            if self.image_metadata.renderer_exited() {
                return CacheUsageSync::RenderThreadGone;
            }
            let now = Instant::now();
            if now >= deadline {
                return CacheUsageSync::TimedOut;
            }
            let slice = CACHE_USAGE_SYNC_POLL.min(deadline.saturating_duration_since(now));
            match self.cmd_tx.send_timeout(command, slice) {
                SendTimeoutOutcome::Sent => break,
                SendTimeoutOutcome::TimedOut(returned) => command = returned,
                SendTimeoutOutcome::Disconnected(_) => {
                    return CacheUsageSync::RenderThreadGone;
                }
            }
        }
        if let Some(waker) = self.render_waker.as_ref() {
            waker.wake();
        }
        loop {
            match wait.recv_timeout(CACHE_USAGE_SYNC_POLL) {
                Ok(()) => return CacheUsageSync::Synchronized,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return CacheUsageSync::RenderThreadGone;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.image_metadata.renderer_exited() {
                        return CacheUsageSync::RenderThreadGone;
                    }
                    if Instant::now() >= deadline {
                        return CacheUsageSync::TimedOut;
                    }
                }
            }
        }
    }

    /// Wait until the submission worker has committed the work item it was
    /// handed, or the deadline elapses. Returns Queued while the item is
    /// still in flight: later work must not be sent before it, or FIFO order
    /// breaks.
    fn wait_for_ordered_in_flight(&self, deadline: Instant) -> Result<(), InvalidationDelivery> {
        loop {
            if self.image_metadata.renderer_exited() {
                // The worker may still hold the item, but no delivery can
                // follow: drop the gate receiver (so a completion sender can
                // observe the cancellation) and stop waiting immediately.
                self.ordered_in_flight.borrow_mut().take();
                return Err(InvalidationDelivery::Unavailable);
            }
            let mut in_flight = self.ordered_in_flight.borrow_mut();
            let Some(wait) = in_flight.as_ref() else {
                return Ok(());
            };
            let now = Instant::now();
            if now >= deadline {
                return Err(InvalidationDelivery::Queued);
            }
            let slice = CACHE_USAGE_SYNC_POLL.min(deadline.saturating_duration_since(now));
            match wait.recv_timeout(slice) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    *in_flight = None;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Keep waiting: the worker may be resolving a relative
                    // path; only the caller's overall deadline ends the wait,
                    // and the next iteration re-checks renderer exit.
                }
            }
        }
    }

    fn abandon_ordered_queue(&self) {
        self.pending_invalidations.borrow_mut().clear();
        *self.ordered_in_flight.borrow_mut() = None;
    }

    /// Try to commit every queued item in FIFO order, up to `deadline`.
    /// Commands stay queued when a live renderer does not make room; an item
    /// whose `:file` needs off-thread resolution is handed to the submission
    /// worker and blocks the queue behind its completion acknowledgement. An
    /// exited or disconnected renderer clears the queue because no later
    /// delivery is possible.
    fn flush_pending_invalidations(&self, deadline: Instant) -> InvalidationDelivery {
        loop {
            if self.image_metadata.renderer_exited() {
                self.abandon_ordered_queue();
                return InvalidationDelivery::Unavailable;
            }
            if let Err(state) = self.wait_for_ordered_in_flight(deadline) {
                if matches!(state, InvalidationDelivery::Unavailable) {
                    self.abandon_ordered_queue();
                }
                return state;
            }
            let Some(work) = self.pending_invalidations.borrow_mut().pop_front() else {
                return InvalidationDelivery::Committed;
            };
            let now = Instant::now();
            if now >= deadline {
                self.pending_invalidations.borrow_mut().push_front(work);
                return InvalidationDelivery::Queued;
            }
            if work
                .resolution
                .as_ref()
                .is_some_and(ImageFileRequest::needs_off_thread)
            {
                let (sent, wait) = std::sync::mpsc::sync_channel(1);
                if defer_render_command_with_completion(
                    &self.cmd_tx,
                    self.render_waker.as_ref(),
                    work.command,
                    work.resolution,
                    &self.image_decode_client,
                    Some(sent),
                )
                .is_err()
                {
                    self.abandon_ordered_queue();
                    return InvalidationDelivery::Unavailable;
                }
                *self.ordered_in_flight.borrow_mut() = Some(wait);
                continue;
            }
            let slice = CACHE_USAGE_SYNC_POLL.min(deadline.saturating_duration_since(now));
            match self.cmd_tx.send_timeout(work.command, slice) {
                SendTimeoutOutcome::Sent => {
                    if let Some(waker) = self.render_waker.as_ref() {
                        waker.wake();
                    }
                }
                SendTimeoutOutcome::TimedOut(returned) => {
                    self.pending_invalidations
                        .borrow_mut()
                        .push_front(QueuedRenderWork {
                            command: returned,
                            resolution: work.resolution,
                        });
                }
                SendTimeoutOutcome::Disconnected(_) => {
                    self.abandon_ordered_queue();
                    return InvalidationDelivery::Unavailable;
                }
            }
        }
    }

    /// Non-blocking flush used on lookup: drain what the channel accepts right
    /// now, hand at most one resolution-requiring item to the submission
    /// worker, and return without ever sleeping. Ordering is preserved by the
    /// in-flight acknowledgement gate.
    fn try_flush_pending_invalidations(&self) {
        loop {
            if self.image_metadata.renderer_exited() {
                self.abandon_ordered_queue();
                return;
            }
            {
                let mut in_flight = self.ordered_in_flight.borrow_mut();
                if let Some(wait) = in_flight.as_ref() {
                    let completed =
                        !matches!(wait.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));
                    if !completed {
                        return;
                    }
                    *in_flight = None;
                }
            }
            let Some(work) = self.pending_invalidations.borrow_mut().pop_front() else {
                return;
            };
            if work
                .resolution
                .as_ref()
                .is_some_and(ImageFileRequest::needs_off_thread)
            {
                let (sent, wait) = std::sync::mpsc::sync_channel(1);
                match defer_render_command_with_completion(
                    &self.cmd_tx,
                    self.render_waker.as_ref(),
                    work.command,
                    work.resolution,
                    &self.image_decode_client,
                    Some(sent),
                ) {
                    Ok(()) => *self.ordered_in_flight.borrow_mut() = Some(wait),
                    Err(_) => self.abandon_ordered_queue(),
                }
                return;
            }
            match self.cmd_tx.try_send(work.command) {
                Ok(()) => {
                    if let Some(waker) = self.render_waker.as_ref() {
                        waker.wake();
                    }
                }
                Err(crossbeam_channel::TrySendError::Full(command)) => {
                    // The head cannot be sent now: hand it to the submission
                    // worker so it is retried autonomously, and gate every
                    // later item on its completion.
                    let (sent, wait) = std::sync::mpsc::sync_channel(1);
                    if defer_render_command_with_completion(
                        &self.cmd_tx,
                        self.render_waker.as_ref(),
                        command,
                        work.resolution,
                        &self.image_decode_client,
                        Some(sent),
                    )
                    .is_err()
                    {
                        // The submission worker thread is gone: no later
                        // delivery is possible.
                        self.abandon_ordered_queue();
                        return;
                    }
                    *self.ordered_in_flight.borrow_mut() = Some(wait);
                    return;
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    self.abandon_ordered_queue();
                    return;
                }
            }
        }
    }

    /// Admit an image load without letting it overtake a queued
    /// invalidation. A load issued after a whole-cache clear must be
    /// processed after it: if the clear is waiting in the retry queue and the
    /// load went straight to the renderer, replaying the clear later would
    /// evict the newer load, which GNU's synchronous clear never would.
    fn schedule_image_command_ordered(
        &self,
        command: RenderCommand,
        resolution: Option<&ImageFileRequest>,
    ) -> Result<(), String> {
        // A `:file` needing off-thread resolution ALWAYS goes through the
        // ordered queue and its completion gate, even when nothing else is
        // queued: its command reaches the renderer asynchronously from the
        // submission worker, so without the gate a later publish could
        // overtake it and observe a pre-load total.
        let needs_worker = resolution.is_some_and(ImageFileRequest::needs_off_thread);
        if !needs_worker
            && self.pending_invalidations.borrow().is_empty()
            && self.ordered_in_flight.borrow().is_none()
        {
            // Uncontended already-resolved load: send it now. If the channel
            // is full, fall through to the ordered queue instead of the
            // untracked deferral path: a deferred load must carry the
            // completion gate or a later publish can overtake it.
            submit_image_decode(&self.image_decode_client, &command);
            match self.cmd_tx.try_send(command) {
                Ok(()) => {
                    if let Some(waker) = self.render_waker.as_ref() {
                        waker.wake();
                    }
                    return Ok(());
                }
                Err(crossbeam_channel::TrySendError::Full(command)) => {
                    // Full channel: the submission worker retries the send
                    // autonomously. The completion gate keeps later loads and
                    // publishes behind this load.
                    let (sent, wait) = std::sync::mpsc::sync_channel(1);
                    if defer_render_command_with_completion(
                        &self.cmd_tx,
                        self.render_waker.as_ref(),
                        command,
                        resolution.cloned(),
                        &self.image_decode_client,
                        Some(sent),
                    )
                    .is_err()
                    {
                        return Err("failed to defer image load command".to_owned());
                    }
                    *self.ordered_in_flight.borrow_mut() = Some(wait);
                    return Ok(());
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    return Err("failed to queue image load: channel disconnected".to_owned());
                }
            }
        }
        if self.image_metadata.renderer_exited() {
            return Err("renderer exited with the image load still queued".to_owned());
        }
        // Queue behind every pending invalidation without blocking the
        // lookup. Other loads start decoding now; the submission worker
        // resolves a `:file` and then commits its command under the gate.
        if !needs_worker {
            submit_image_decode(&self.image_decode_client, &command);
        }
        self.pending_invalidations
            .borrow_mut()
            .push_back(QueuedRenderWork {
                command,
                resolution: resolution.cloned(),
            });
        self.try_flush_pending_invalidations();
        Ok(())
    }

    /// Admit a cache-invalidation command on the ordered path.
    ///
    /// The decoder deferral path can reorder a command behind the publish that
    /// is supposed to observe it, so invalidation commands and the publish
    /// come through here: direct FIFO with each other. Admission is
    /// liveness-bounded under the same policy as the acknowledgement wait: a
    /// full channel with a wedged renderer must not hang `clear-image-cache`
    /// in Lisp forever. A command that does not fit before the bound stays
    /// queued for the next sync instead of being dropped, so the catalog's
    /// entries and the renderer's bytes cannot diverge until some future read
    /// happens to repair it.
    fn send_ordered_cache_command(
        &self,
        command: RenderCommand,
        deadline: Instant,
    ) -> InvalidationDelivery {
        self.pending_invalidations
            .borrow_mut()
            .push_back(QueuedRenderWork {
                command,
                resolution: None,
            });
        self.flush_pending_invalidations(deadline)
    }
}

impl ImageCatalog for AsyncImageCatalog {
    fn lookup(&self, request: ImageResolveRequest, limit: ImageSizeLimit) -> ImageLookup {
        let lookup = self.lookup_inner(request, limit);
        // Every return path of the inner lookup funnels through here,
        // including ones added later, so a failed lookup cannot be consumed
        // without the failure being recorded.
        if let ImageLookup::Failed(failed) = &lookup {
            self.record_failure(failed);
        }
        lookup
    }

    fn take_pending_diagnostics(&self) -> Vec<String> {
        self.failed_diagnostics
            .borrow_mut()
            .drain(..)
            .map(|diagnostic| diagnostic.message())
            .collect()
    }
    fn invalidate(&self, target: ImageInvalidation) -> ImageInvalidationResult {
        let target = match target {
            ImageInvalidation::Dependency(source) => {
                ImageInvalidation::Dependency(self.classify_source(source).0)
            }
            other => other,
        };
        // Snapshot the in-flight load tokens before the entries are removed:
        // their late terminals describe images this clear frees.
        let pre_clear_loads: Vec<ImageLoadToken> = if matches!(&target, ImageInvalidation::All) {
            self.entries
                .borrow()
                .values()
                .filter_map(|state| match state {
                    CatalogEntry::Pending(pending) => Some(pending.load()),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        let (removed, invalidated) = {
            let mut entries = self.entries.borrow_mut();
            let requests = entries
                .keys()
                .filter(|request| match &target {
                    ImageInvalidation::Spec { spec } => request.spec == *spec,
                    ImageInvalidation::Dependency(source) => request.source == *source,
                    ImageInvalidation::All => true,
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut invalidated = Vec::new();
            let removed = requests
                .into_iter()
                .filter_map(|request| {
                    let state = entries.remove(&request)?;
                    invalidated.push(request);
                    Some(state.placement().image_id())
                })
                .collect::<Vec<_>>();
            (removed, invalidated)
        };

        // An invalidated source may have changed on disk, so its header is
        // stale: drop the geometry and let the next load's probe re-read it.
        if !invalidated.is_empty() {
            let mut layouts = self.lock_header_layouts();
            for request in &invalidated {
                layouts.remove(request);
            }
        }
        // Stale geometry is stale for every frame, including frames whose
        // requests were never looked up: prune by the same predicate the slots
        // were dropped by, so a shared answer cannot outlive its source.
        if let Ok(mut geometry) = self.geometry_layouts.try_lock() {
            geometry.retain(|request, _| match &target {
                ImageInvalidation::Spec { spec } => request.spec != *spec,
                ImageInvalidation::Dependency(source) => request.source != *source,
                ImageInvalidation::All => false,
            });
        }

        let result = if removed.is_empty() {
            ImageInvalidationResult::Unchanged
        } else {
            ImageInvalidationResult::Changed
        };
        // One deadline for the whole invalidation: a wedged renderer costs at
        // most one bounded wait, not one per flushed image plus the clear.
        let ordered_deadline = Instant::now() + CACHE_USAGE_SYNC_DEADLINE;
        self.flush_image_ids_ordered(removed, ordered_deadline);
        // GNU `clear-image-cache` frees every window frame's image cache, and
        // the frame cache holds chrome icons as well as editor images.  The
        // wholesale command therefore runs even when the catalog itself had no
        // matching entry; the published total changes either way.
        let clears_everything = matches!(&target, ImageInvalidation::All);
        if clears_everything {
            // Free only what existed when the clear was issued: an eager CPU
            // decode submitted after this ticket belongs to a later load and
            // must not be retired by this physical clear.
            let through = self.image_decode_client.admission_ticket();
            let command = RenderCommand::Asset(AssetCommand::ClearImageCache { through });
            let _delivery = self.send_ordered_cache_command(command, ordered_deadline);
        }
        if clears_everything || matches!(result, ImageInvalidationResult::Changed) {
            self.usage_sync_pending.set(true);
        }
        if clears_everything {
            // A whole-cache clear's post state is zero whether the command is
            // committed now, queued for retry ahead of every later publish, or
            // undeliverable because the renderer is gone (its owned cache went
            // with it). Admitting a later load does NOT invalidate this zero:
            // a source that never becomes drawable charges 0 in GNU. Only a
            // post-clear load that actually becomes READY does (see
            // note_terminal_after_clear).
            self.usage_sync_clear_all.set(true);
            *self.pre_clear_loads.borrow_mut() = pre_clear_loads;
        }
        result
    }

    fn cached_size_bytes(&self) -> i64 {
        // GNU's cache is freed on the caller's thread, so the next
        // `image-cache-size` sees the post-invalidation state. This catalog's
        // cache lives on the render thread; when an invalidation made the
        // published snapshot stale, wait for the ordered invalidation to be
        // executed and the post-invalidation total published, then read it.
        if self.usage_sync_pending.get() {
            let cleared_everything = self.usage_sync_clear_all.get();
            match self.synchronize_cache_usage() {
                CacheUsageSync::Synchronized => {
                    self.usage_sync_pending.set(false);
                    self.usage_sync_clear_all.set(false);
                    self.pre_clear_loads.borrow_mut().clear();
                }
                CacheUsageSync::BeforeRenderer => {
                    // No renderer-owned bytes exist yet; the queued clear
                    // publishes the same zero when the GPU installs.
                    self.usage_sync_pending.set(false);
                    self.usage_sync_clear_all.set(false);
                    self.pre_clear_loads.borrow_mut().clear();
                    if cleared_everything {
                        return 0;
                    }
                }
                CacheUsageSync::RenderThreadGone | CacheUsageSync::TimedOut => {
                    // Never report the stale pre-invalidation total for a
                    // whole-cache clear: GNU's synchronous free leaves zero.
                    if cleared_everything {
                        return 0;
                    }
                    // Named Invariant-7 exception: for a partial invalidation
                    // (image flush or animation retire) the post-invalidation
                    // total exists only in the renderer's published
                    // accounting; with the renderer gone or wedged the
                    // catalog cannot compute it, so it returns the last
                    // published total and keeps the pending flag for a later
                    // retry instead of fabricating a value.
                }
            }
        }
        i64::try_from(self.image_metadata.cached_size_bytes()).unwrap_or(i64::MAX)
    }

    fn invalidate_animation(&self, target: ImageAnimationInvalidation) -> ImageInvalidationResult {
        let retirement = match target {
            ImageAnimationInvalidation::Source(source) => {
                let source = self.classify_source(source).0;
                let Some(sequence) = self.sequence_ids.borrow_mut().remove(&source) else {
                    return ImageInvalidationResult::Unchanged;
                };
                ImageSequenceRetirement::One(sequence)
            }
            ImageAnimationInvalidation::All => {
                if self.sequence_ids.borrow().is_empty() {
                    return ImageInvalidationResult::Unchanged;
                }
                self.sequence_ids.borrow_mut().clear();
                ImageSequenceRetirement::AllocatedThrough(
                    ImageSequenceId::new(self.next_sequence_id.get())
                        .expect("a non-empty sequence map has allocated an identity"),
                )
            }
        };
        let command = RenderCommand::Asset(AssetCommand::ImageSequenceRetire { retirement });
        // Decoded-sequence bytes are part of `image-cache-size`; wait for the
        // retire's publish when the retire is committed or queued for retry.
        let deadline = Instant::now() + CACHE_USAGE_SYNC_DEADLINE;
        if matches!(
            self.send_ordered_cache_command(command, deadline),
            InvalidationDelivery::Committed | InvalidationDelivery::Queued
        ) {
            self.usage_sync_pending.set(true);
        }
        ImageInvalidationResult::Changed
    }

    fn reconcile_renderer_state(&self, event: ImageStateEvent) {
        // Block: this runs only after ImageStateChanged, when redisplay is
        // about to rebuild matrices and must observe the renderer's exact
        // terminal/residency state.
        let mut entries = self.entries.borrow_mut();
        let Some((request, state)) = entries
            .iter_mut()
            .find(|(_, state)| state.placement().image_id() == event.image())
        else {
            return;
        };
        match event {
            ImageStateEvent::DecodeCompleted(load) => {
                let CatalogEntry::Pending(pending) = state else {
                    return;
                };
                if pending.load() != load {
                    return;
                }
                let Some(terminal) = self.image_metadata.terminal(load) else {
                    return;
                };
                self.note_terminal_after_clear(load, &terminal);
                let resolved = image_lookup_from_terminal(pending.clone(), terminal);
                self.report_header_disagreement(request, &resolved);
                *state = CatalogEntry::from_lookup(resolved);
            }
            ImageStateEvent::Evicted(_) => {
                let placement = state.placement();
                *state = CatalogEntry::Evicted(placement);
            }
        }
    }
}

impl AsyncImageCatalog {
    /// Record a terminal state against a pending whole-cache clear.
    ///
    /// A source that fails or never becomes drawable charges nothing in GNU,
    /// so it must not invalidate the clear's known zero. A load that becomes
    /// READY adds bytes and does invalidate it (its exact byte count is the
    /// evaluator-ledger case). A terminal for a pre-clear load describes an
    /// image the clear freed and is discarded.
    fn note_terminal_after_clear(&self, load: ImageLoadToken, terminal: &ImageDecodeTerminal) {
        if !self.usage_sync_clear_all.get() {
            return;
        }
        {
            let mut pre_clear = self.pre_clear_loads.borrow_mut();
            if let Some(position) = pre_clear.iter().position(|token| *token == load) {
                pre_clear.remove(position);
                return;
            }
        }
        if matches!(terminal, ImageDecodeTerminal::Ready(_)) {
            self.usage_sync_clear_all.set(false);
        }
    }

    /// Report a failure unless this load attempt has already been reported.
    fn record_failure(&self, failed: &FailedImage) {
        let image = failed.load().image();
        let mut reported = self.reported_failures.borrow_mut();
        if reported.get(&image) == Some(&failed.load()) {
            return;
        }
        reported.insert(image, failed.load());
        self.failed_diagnostics
            .borrow_mut()
            .push(failed.error.clone());
    }

    /// Report a failure regardless of whether it has been reported before.
    ///
    /// For the synchronous path, where the caller asked about this one image
    /// and GNU answers with `image_error` every time it is asked.
    fn record_failure_always(&self, failed: &FailedImage) {
        self.reported_failures
            .borrow_mut()
            .insert(failed.load().image(), failed.load());
        self.failed_diagnostics
            .borrow_mut()
            .push(failed.error.clone());
    }

    fn lookup_inner(&self, request: ImageResolveRequest, limit: ImageSizeLimit) -> ImageLookup {
        // Every lookup advances queued ordered work, including one for an
        // existing entry that will not schedule a new load.
        self.try_flush_pending_invalidations();
        self.size_limit.set(limit);
        let (request, resolution) = self.classify_request(request);
        let mut entries = self.entries.borrow_mut();
        if !entries.contains_key(&request) {
            let image_id = next_host_image_id();
            let load = self.next_load(image_id);
            // Another frame of the same image may already have resolved this
            // geometry: reserving the resolved extent means this slot never
            // moves, and the probe below has nothing left to learn.
            let known = self.known_geometry(&request);
            let layout = known.unwrap_or_else(|| placeholder_image_extent(&request));
            let pending = PendingImage::new(load, layout);
            let command =
                image_load_command(&request, load, self.sequence_id(&request.source), limit);
            let state = match self.schedule_image_command_ordered(command, resolution.as_ref()) {
                Ok(()) => CatalogEntry::Pending(pending),
                Err(error) => {
                    // Not a decode failure: the render thread could not be
                    // handed the job at all. GNU has no sentence for this,
                    // because it has no second thread to lose.
                    tracing::warn!(%error, "image load command could not be scheduled");
                    CatalogEntry::Failed(pending.failed(ImageDiagnostic::NotDrawable))
                }
            };
            entries.insert(request.clone(), state);
            if known.is_none() {
                self.schedule_header_probe(&request, resolution.as_ref(), load);
            }
        }

        let state = entries
            .get_mut(&request)
            .expect("image catalog entry inserted above");
        if let CatalogEntry::Evicted(placement) = state {
            let load = self.next_load(placement.image_id());
            let pending = PendingImage::new(load, placement.layout());
            let command =
                image_load_command(&request, load, self.sequence_id(&request.source), limit);
            *state = match self.schedule_image_command_ordered(command, resolution.as_ref()) {
                Ok(()) => CatalogEntry::Pending(pending),
                Err(error) => {
                    // Not a decode failure: the render thread could not be
                    // handed the job at all. GNU has no sentence for this,
                    // because it has no second thread to lose.
                    tracing::warn!(%error, "image load command could not be scheduled");
                    CatalogEntry::Failed(pending.failed(ImageDiagnostic::NotDrawable))
                }
            };
            self.schedule_header_probe(&request, resolution.as_ref(), load);
        }
        let CatalogEntry::Pending(pending) = state else {
            return state
                .as_lookup()
                .expect("evicted entry was transitioned above");
        };
        let load = pending.load();
        // Header geometry may have landed since the slot was reserved: adopt it
        // by narrowing the slot in place. The identity is unchanged, so glyphs
        // already published against this image keep pointing at it.
        //
        // This is the only place a reserved slot may move, and the move is
        // classified and counted here rather than left to be inferred from a
        // frame that shifted. A slot that already holds resolved geometry
        // (renewed from a known layout, or reserved from another frame's
        // answer) finalizes as Unchanged, so a move is counted once per
        // reservation, not once per lookup.
        if let Some(layout) = self.header_layout(&request, load) {
            let settled = ProvisionalExtent::new(pending.placement().layout()).finalize(layout);
            if let SlotChange::Moved { from, to } = settled {
                self.record_slot_move(&request, from, to);
                *pending = PendingImage::new(load, layout);
            }
            self.remember_geometry(&request, layout);
        }
        let terminal = match self.image_metadata.try_terminal(load) {
            ImageTerminalProbe::Busy => {
                return state.as_lookup().expect("pending state is observable");
            }
            ImageTerminalProbe::Available(terminal) => terminal,
        };
        let Some(terminal) = terminal else {
            return state.as_lookup().expect("pending state is observable");
        };
        self.note_terminal_after_clear(load, &terminal);
        let resolved = image_lookup_from_terminal(pending.clone(), terminal);
        self.report_header_disagreement(&request, &resolved);
        *state = CatalogEntry::from_lookup(resolved);
        state
            .as_lookup()
            .expect("terminal state is observable through the catalog")
    }
}

impl AsyncImageCatalog {
    fn lock_header_layouts(
        &self,
    ) -> std::sync::MutexGuard<
        '_,
        HashMap<ImageResolveRequest, (ImageLoadToken, Option<ImageLayoutExtent>)>,
    > {
        self.header_layouts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Layout a completed header probe resolved for this exact load.
    ///
    /// Never waits: a probe still in flight (or one that found no header) just
    /// leaves the request on its placeholder until a later lookup.
    fn header_layout(
        &self,
        request: &ImageResolveRequest,
        load: ImageLoadToken,
    ) -> Option<ImageLayoutExtent> {
        match self.header_layouts.try_lock() {
            Ok(layouts) => probed_layout(&layouts, request, load),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                probed_layout(&poisoned.into_inner(), request, load)
            }
        }
    }

    /// Geometry already resolved for this image by any of its frames.
    ///
    /// Never waits, for the same reason [`Self::header_layout`] never does: a
    /// concurrent writer leaves this lookup on its placeholder, exactly like a
    /// probe still in flight, and the next lookup finds the answer.
    fn known_geometry(&self, request: &ImageResolveRequest) -> Option<ImageLayoutExtent> {
        let key = geometry_key(request);
        match self.geometry_layouts.try_lock() {
            Ok(layouts) => layouts.get(&key).copied(),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().get(&key).copied(),
        }
    }

    /// Share a resolved geometry with every other frame of the same image.
    ///
    /// Best effort: contention only means a later frame pays for a probe it
    /// could have been spared, never that a slot reserves something wrong.
    fn remember_geometry(&self, request: &ImageResolveRequest, layout: ImageLayoutExtent) {
        if let Ok(mut layouts) = self.geometry_layouts.try_lock() {
            layouts.insert(geometry_key(request), layout);
        }
    }

    /// Count a reserved slot moving when resolved geometry landed.
    ///
    /// Nothing here stops the frame from moving — the geometry is what it is —
    /// but a move outside the aspect case is a disagreement, and a counter is
    /// how it gets noticed instead of being paid for at animation rates.
    fn record_slot_move(
        &self,
        request: &ImageResolveRequest,
        from: ImageLayoutExtent,
        to: ImageLayoutExtent,
    ) {
        self.slot_moves.set(self.slot_moves.get() + 1);
        tracing::debug!(
            size = ?request.size,
            rotation = ?request.rotation,
            ?from,
            ?to,
            "image slot moved when resolved geometry landed"
        );
    }

    /// Slots that moved when resolved geometry landed, for tests.
    #[cfg(test)]
    fn slot_moves(&self) -> u64 {
        self.slot_moves.get()
    }

    /// Carry a known header layout over to a replacement load of the same
    /// source, or fall back to the request's placeholder.
    fn renew_header_layout(
        &self,
        request: &ImageResolveRequest,
        load: ImageLoadToken,
    ) -> ImageLayoutExtent {
        let layout = self.header_layout(request, load).or_else(|| {
            let mut layouts = self.lock_header_layouts();
            let (_, layout) = layouts.get(request).copied()?;
            let layout = layout?;
            // Same bytes, new load: the header did not change, only the
            // renderer residency it will rebuild.
            layouts.insert(request.clone(), (load, Some(layout)));
            Some(layout)
        });
        layout.unwrap_or_else(|| placeholder_image_extent(request))
    }

    /// The invariant pending geometry rests on: a layout resolved from the
    /// header must equal the layout the decode reports. Say so when it does
    /// not, rather than letting the frame move quietly.
    fn report_header_disagreement(&self, request: &ImageResolveRequest, resolved: &ImageLookup) {
        let ImageLookup::Ready(ready) = resolved else {
            return;
        };
        let Some(probed) = self.header_layout(request, ready.load) else {
            return;
        };
        if probed != ready.metadata.layout {
            tracing::warn!(
                image = %ready.load.image(),
                ?probed,
                decoded = ?ready.metadata.layout,
                "image geometry resolved from the header disagrees with the decode"
            );
        }
    }

    /// Submit an off-thread header probe for `request` under `load`.
    ///
    /// The probe is the whole reason a large image can take its real box
    /// before its pixels exist, so it is deliberately *not* deferred to the
    /// renderer: four decoder threads busy with large images must not delay a
    /// header read, and a slow decode must not delay anyone else's layout.
    fn schedule_header_probe(
        &self,
        request: &ImageResolveRequest,
        resolution: Option<&ImageFileRequest>,
        load: ImageLoadToken,
    ) {
        let probe = HeaderProbeRequest {
            request: request.clone(),
            resolution: resolution.cloned(),
            load,
            layouts: Arc::clone(&self.header_layouts),
            redisplay_waker: self.redisplay_waker.clone(),
        };
        if header_probe_sender().send(probe).is_err() {
            tracing::warn!(image = %load.image(), "failed to queue image header probe");
        }
    }

    /// Flush images on the ordered path: free each immediately, sending the
    /// command directly and blocking for channel space (never through the
    /// decoder deferral path), so a later `PublishImageCacheUsage` on the same
    /// channel lands after it.  GNU `uncache_image` frees the pixmaps
    /// synchronously.
    fn flush_image_ids_ordered(&self, removed: Vec<ImageId>, deadline: Instant) {
        {
            let mut reported = self.reported_failures.borrow_mut();
            for image in &removed {
                reported.remove(image);
            }
        }
        for image in removed {
            self.image_decode_client.retire_image(image);
            let command = RenderCommand::Asset(AssetCommand::ImageFlush { image });
            // A flush that cannot be committed is not applied; the read that
            // follows falls back under the Invariant-7 policy below.
            let _ = self.send_ordered_cache_command(command, deadline);
        }
    }
}

fn image_lookup_from_terminal(pending: PendingImage, terminal: ImageDecodeTerminal) -> ImageLookup {
    let load = pending.load();
    match terminal {
        // Intermediate: the decode is still running, so the slot keeps the
        // geometry the header gave it and stays pending until `Ready` lands.
        ImageDecodeTerminal::Band(_) => ImageLookup::Pending(pending),
        ImageDecodeTerminal::Ready(metadata) => ImageLookup::Ready(ReadyImage { load, metadata }),
        ImageDecodeTerminal::Failed(error) => ImageLookup::Failed(pending.failed(error)),
    }
}

#[cfg(test)]
pub(super) fn wait_for_image_metadata(
    shared: &SharedImageRenderState,
    load: ImageLoadToken,
    timeout: Duration,
) -> Option<ImageDecodeTerminal> {
    shared.wait_for_terminal(load, timeout)
}

/// One off-thread header probe.
///
/// Carries its catalog's result map and waker so a single process-wide prober
/// can serve every catalog instance; a catalog is free to drop its end.
struct HeaderProbeRequest {
    request: ImageResolveRequest,
    resolution: Option<ImageFileRequest>,
    load: ImageLoadToken,
    layouts: HeaderLayouts,
    redisplay_waker: Option<RedisplayWaker>,
}

// Admission to the off-thread header-probe channel requires owned plain data.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<HeaderProbeRequest>();
};

fn header_probe_sender() -> &'static crossbeam_channel::Sender<HeaderProbeRequest> {
    static SENDER: OnceLock<crossbeam_channel::Sender<HeaderProbeRequest>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::unbounded::<HeaderProbeRequest>();
        let _ = std::thread::Builder::new()
            .name("neomacs-image-header-probe".to_owned())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    // Drain what is already queued: one evaluation pass can
                    // schedule every image of a buffer, and the evaluator only
                    // needs one redisplay for all of them.
                    let mut batch = vec![first];
                    while let Ok(next) = rx.try_recv() {
                        batch.push(next);
                    }
                    for probe in batch {
                        run_header_probe(probe);
                    }
                }
            });
        tx
    })
}

fn run_header_probe(probe: HeaderProbeRequest) {
    let layout = probe_layout(&probe);
    {
        let mut layouts = probe
            .layouts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        layouts.insert(probe.request.clone(), (probe.load, layout));
    }
    let Some(layout) = layout else {
        return;
    };
    tracing::debug!(
        image = %probe.load.image(),
        width = layout.width(),
        height = layout.height(),
        "resolved image geometry from the encoded header"
    );
    if let Some(waker) = &probe.redisplay_waker {
        waker.request_redisplay();
    }
}

/// Resolve the header's layout exactly the way the decoder will resolve the
/// decoded one: same native extent, same size spec, same realization.
fn probe_layout(probe: &HeaderProbeRequest) -> Option<ImageLayoutExtent> {
    let request = &probe.request;
    let resolved;
    let source = match &request.source {
        ImageResolveSource::File(path) => {
            let name = path.as_utf8_str()?;
            // `Search` and `~user` are resolved here, not on the evaluator
            // thread; a path that resolves to nothing keeps the classified
            // name and simply fails the probe.
            resolved = probe
                .resolution
                .as_ref()
                .and_then(ImageFileRequest::resolve);
            ImageProbeSource::File(resolved.as_deref().unwrap_or(name))
        }
        ImageResolveSource::Data(data) => ImageProbeSource::Data(data.bytes()),
    };
    probe_image_layout_for_frame(
        source,
        request.frame,
        request.size,
        request.rotation,
        request.realization,
    )
}

fn probed_layout(
    layouts: &HashMap<ImageResolveRequest, (ImageLoadToken, Option<ImageLayoutExtent>)>,
    request: &ImageResolveRequest,
    load: ImageLoadToken,
) -> Option<ImageLayoutExtent> {
    let (token, layout) = layouts.get(request)?;
    (*token == load).then_some(*layout).flatten()
}

struct DeferredRenderCommand {
    target: neomacs_display_runtime::thread_comm::RenderCommandSender,
    waker: Option<GuiEventLoopWaker>,
    command: RenderCommand,
    /// How to turn the command's raw `:file` into an absolute path off-thread,
    /// or `None` for `:data` loads and inline-resolved absolute paths.
    resolution: Option<ImageFileRequest>,
    decoder: ImageDecodeClient,
    /// Signals that the command has been committed to the renderer (or the
    /// channel is gone). An ordered queue waits on this before sending the
    /// next item, so an off-thread `:file` resolution cannot reorder.
    sent: Option<std::sync::mpsc::SyncSender<()>>,
}

fn deferred_render_command_sender() -> &'static crossbeam_channel::Sender<DeferredRenderCommand> {
    static SENDER: OnceLock<crossbeam_channel::Sender<DeferredRenderCommand>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::unbounded::<DeferredRenderCommand>();
        let _ = std::thread::Builder::new()
            .name("neomacs-image-command-submit".to_owned())
            .spawn(move || {
                while let Ok(deferred) = rx.recv() {
                    let DeferredRenderCommand {
                        target,
                        waker,
                        command,
                        resolution,
                        decoder,
                        sent,
                    } = deferred;
                    let command = resolve_deferred_image_path(command, resolution.as_ref());
                    submit_image_decode(&decoder, &command);
                    let committed = target.send(command).is_ok();
                    if committed && let Some(waker) = waker {
                        waker.wake();
                    }
                    if let Some(sent) = sent {
                        // A disconnected target committed nothing, but the
                        // queue must still move on (it then observes the
                        // disconnect); signal either way.
                        let _ = sent.try_send(());
                    }
                }
            });
        tx
    })
}

fn defer_render_command_with_completion(
    target: &neomacs_display_runtime::thread_comm::RenderCommandSender,
    waker: Option<&GuiEventLoopWaker>,
    command: RenderCommand,
    resolution: Option<ImageFileRequest>,
    decoder: &ImageDecodeClient,
    sent: Option<std::sync::mpsc::SyncSender<()>>,
) -> Result<(), String> {
    deferred_render_command_sender()
        .send(DeferredRenderCommand {
            target: target.clone(),
            waker: waker.cloned(),
            command,
            resolution,
            decoder: decoder.clone(),
            sent,
        })
        .map_err(|error| format!("failed to defer image load command: {error}"))
}

/// Submit CPU work before renderer queue backpressure or GPU acquisition can
/// delay it. The renderer adopts the same load token and receives these pixels
/// once; repeated admission of a deferred command cannot start a second decode.
fn submit_image_decode(decoder: &ImageDecodeClient, command: &RenderCommand) {
    use neomacs_display_runtime::render_thread::SvgResourceContext;
    match command {
        RenderCommand::Asset(AssetCommand::ImageLoadFile {
            load,
            path,
            size,
            rotation,
            realization,
            colors,
            mask,
            animation,
            frame,
            sequence,
            identity,
            limit,
        }) => decoder.load_file_with_id(
            *load,
            path,
            *size,
            *rotation,
            *realization,
            *colors,
            *mask,
            *animation,
            *frame,
            *sequence,
            identity.clone(),
            *limit,
        ),
        RenderCommand::Asset(AssetCommand::ImageLoadData {
            load,
            data,
            size,
            rotation,
            realization,
            colors,
            mask,
            animation,
            frame,
            sequence,
            identity,
            limit,
        }) => {
            let (data, resources) = match data {
                neomacs_display_protocol::ImageDataSource::Isolated(data) => {
                    (data.clone(), SvgResourceContext::Isolated)
                }
                neomacs_display_protocol::ImageDataSource::WithBaseUri { data, base_uri } => (
                    data.clone(),
                    SvgResourceContext::BaseUri(
                        base_uri.as_utf8_str().unwrap_or_default().to_owned(),
                    ),
                ),
            };
            decoder.load_data_with_id(
                *load,
                data,
                *size,
                *rotation,
                *realization,
                *colors,
                *mask,
                *animation,
                *frame,
                *sequence,
                resources,
                identity.clone(),
                *limit,
            );
        }
        RenderCommand::Asset(AssetCommand::ImageRetire { image })
        | RenderCommand::Asset(AssetCommand::ImageFlush { image }) => decoder.retire_image(*image),
        RenderCommand::Asset(AssetCommand::ImageSequenceRetire { retirement }) => {
            decoder.retire_sequence(*retirement)
        }
        _ => {}
    }
}

/// The single off-thread resolution step: resolve the [`ImageFileRequest`] and
/// patch the load command's path with the result. `Direct` (including commands
/// deferred only by backpressure) resolves to the same path; a `Search` that
/// finds nothing leaves the path untouched and the renderer reports the decode
/// failure, matching GNU's "Cannot open image file".
fn resolve_deferred_image_path(
    mut command: RenderCommand,
    resolution: Option<&ImageFileRequest>,
) -> RenderCommand {
    if let Some(resolution) = resolution
        && let RenderCommand::Asset(AssetCommand::ImageLoadFile { path, .. }) = &mut command
        && let Some(resolved) = resolution.resolve()
    {
        *path = resolved;
    }
    command
}

/// Build one load command.
///
/// `limit` travels with the command rather than staying behind in the catalog
/// because only the decoder side can apply it: GNU checks the bound in the
/// loader, against the header it has just read, before it allocates a pixel
/// buffer (`check_image_size`, `src/image.c:1811`), and the renderer's header
/// read is the port's equivalent of that moment.
fn image_load_command(
    request: &ImageResolveRequest,
    load: ImageLoadToken,
    sequence: ImageSequenceId,
    limit: ImageSizeLimit,
) -> RenderCommand {
    match &request.source {
        ImageResolveSource::File(path) => RenderCommand::Asset(AssetCommand::ImageLoadFile {
            load,
            path: path.as_utf8_str().unwrap_or_default().to_owned(),
            size: request.size,
            rotation: request.rotation,
            realization: request.realization,
            colors: request.colors,
            mask: request.mask,
            animation: request.animation,
            frame: request.frame,
            sequence,
            limit,
            identity: request.identity.clone(),
        }),
        ImageResolveSource::Data(data) => RenderCommand::Asset(AssetCommand::ImageLoadData {
            load,
            data: data.clone(),
            size: request.size,
            rotation: request.rotation,
            realization: request.realization,
            colors: request.colors,
            mask: request.mask,
            animation: request.animation,
            frame: request.frame,
            sequence,
            limit,
            identity: request.identity.clone(),
        }),
    }
}

/// The extent a request reserves before any header or decode has landed.
///
/// The resolution lives behind [`ImageRealization::resolve_provisional`] rather
/// than here, so that the reservation, the header probe and the decoder are one
/// computation with three levels of knowledge instead of three computations
/// that have to be kept in agreement by hand.
fn provisional_extent(request: &ImageResolveRequest) -> ProvisionalExtent {
    request
        .realization
        .resolve_provisional(request.size, request.rotation)
}

fn placeholder_image_extent(request: &ImageResolveRequest) -> ImageLayoutExtent {
    provisional_extent(request).layout()
}

/// The request a resolved geometry is keyed on: everything the header probe
/// reads except the animation frame.
///
/// Animated raster frames composite onto one canvas, so they can share a
/// resolved extent. TIFF pages keep their index because each directory can
/// describe different dimensions. See [`AsyncImageCatalog::geometry_layouts`].
fn geometry_key(request: &ImageResolveRequest) -> ImageResolveRequest {
    // TIFF :index selects independent pages, whose dimensions can differ.
    // Animated formats share a compositing canvas across every frame instead.
    if matches!(
        request.identity.format(),
        neomacs_display_protocol::image_diagnostic::ImageFormatName::Tiff
    ) {
        return request.clone();
    }
    ImageResolveRequest {
        frame: Default::default(),
        ..request.clone()
    }
}

fn home_directory_from_environment() -> Option<String> {
    std::env::var_os("HOME")
        .or({
            #[cfg(windows)]
            {
                std::env::var_os("APPDATA").or_else(|| std::env::var_os("USERPROFILE"))
            }
            #[cfg(not(windows))]
            {
                None
            }
        })
        .map(|home| home.to_string_lossy().into_owned())
}

#[cfg(test)]
#[path = "image_catalog/tests/image_catalog_test.rs"]
mod tests;

/// What the catalog does with an image load's intermediate publications.
///
/// Separate from `tests` because it pokes the terminal-to-lookup step directly:
/// what a band must not do is a property of that step, and it holds whatever
/// else the catalog is in the middle of.
#[cfg(test)]
#[path = "image_catalog/tests/image_band_terminal_test.rs"]
mod band_terminal_tests;

#[cfg(test)]
#[path = "image_catalog/tests/cache_liveness_test.rs"]
mod cache_liveness_test;
