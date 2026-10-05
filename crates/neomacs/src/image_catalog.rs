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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, TryLockError};
use std::time::Duration;

use neomacs_display_protocol::image_diagnostic::ImageDiagnostic;
use neomacs_display_protocol::{
    ImageNativeExtent, ImageSequenceId, ImageSequenceRetirement, ProvisionalExtent, SlotChange,
};
use neomacs_display_runtime::render_thread::{
    ImageDecodeTerminal, ImageProbeSource, ImageTerminalProbe, SharedImageRenderState,
    probe_image_layout,
};
use neomacs_display_runtime::thread_comm::{AssetCommand, RenderCommand};
use neovm_core::emacs_core::image_catalog::{
    FailedImage, ImageAnimationInvalidation, ImageCatalog, ImageId, ImageInvalidation,
    ImageInvalidationResult, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageLookup,
    ImagePlacement, ImageResolveRequest, ImageResolveSource, ImageSizeLimit, ImageStateEvent,
    PendingImage, ReadyImage,
};
use neovm_core::emacs_core::image_path::ImageFileRequest;
use neovm_core::emacs_core::load::image_data_directory;
use neovm_core::heap_types::LispString;

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
    input: crossbeam_channel::Sender<neovm_core::keyboard::InputEvent>,
    notifier: Option<neovm_core::emacs_core::process::WaitNotifier>,
}

impl RedisplayWaker {
    pub(super) fn new(
        input: crossbeam_channel::Sender<neovm_core::keyboard::InputEvent>,
        notifier: Option<neovm_core::emacs_core::process::WaitNotifier>,
    ) -> Self {
        Self { input, notifier }
    }

    /// `LayoutInvalidated` is this codebase's "a display dependency changed and
    /// evaluator layout must be republished": the image's geometry just became
    /// known, and the slot it reserved is stale.
    fn request_redisplay(&self) {
        if self
            .input
            .send(neovm_core::keyboard::InputEvent::LayoutInvalidated)
            .is_err()
        {
            return;
        }
        if let Some(notifier) = &self.notifier
            && let Err(error) = notifier.notify()
        {
            tracing::warn!(%error, "failed to wake the evaluator for resolved image geometry");
        }
    }
}

/// Deep host-side module that owns image request identity, state transitions,
/// renderer scheduling, and completion observation.
pub(super) struct AsyncImageCatalog {
    cmd_tx: crossbeam_channel::Sender<RenderCommand>,
    render_waker: Option<GuiEventLoopWaker>,
    image_metadata: SharedImageRenderState,
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
        cmd_tx: crossbeam_channel::Sender<RenderCommand>,
        render_waker: Option<GuiEventLoopWaker>,
        image_metadata: SharedImageRenderState,
        redisplay_waker: Option<RedisplayWaker>,
    ) -> Self {
        Self {
            cmd_tx,
            render_waker,
            image_metadata,
            header_layouts: Arc::new(Mutex::new(HashMap::new())),
            redisplay_waker,
            entries: RefCell::new(HashMap::new()),
            geometry_layouts: Mutex::new(HashMap::new()),
            slot_moves: Cell::new(0),
            sequence_ids: RefCell::new(HashMap::new()),
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
                ImageResolveSource::File(LispString::from_utf8(resolution.cache_key())),
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
            *state = match schedule_image_command(
                &self.cmd_tx,
                self.render_waker.as_ref(),
                command,
                resolution.as_ref(),
            ) {
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
        let normalized_request = self.classify_request(request.clone()).0;
        let pending = match self.lookup_inner(request.clone(), limit) {
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

        let Some(terminal) =
            wait_for_image_metadata(&self.image_metadata, pending.load(), Duration::from_secs(1))
        else {
            // Bounded wait: do not invent dimensions. Callers (image-size, etc.)
            // surface this as a failed resolve rather than a wrong pixel size.
            return Err(format!(
                "Timed out waiting for image decode (id {})",
                placement.image_id()
            ));
        };
        let state = image_lookup_from_terminal(pending, terminal);
        self.entries
            .borrow_mut()
            .insert(normalized_request, CatalogEntry::from_lookup(state.clone()));

        match state {
            ImageLookup::Ready(image) => Ok(Some(image)),
            ImageLookup::Failed(failed) => Err(failed.error.message()),
            ImageLookup::Pending(_) => unreachable!("terminal decode cannot remain pending"),
        }
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
}

impl AsyncImageCatalog {
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
            let state = match schedule_image_command(
                &self.cmd_tx,
                self.render_waker.as_ref(),
                command,
                resolution.as_ref(),
            ) {
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
            *state = match schedule_image_command(
                &self.cmd_tx,
                self.render_waker.as_ref(),
                command,
                resolution.as_ref(),
            ) {
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
        let resolved = image_lookup_from_terminal(pending.clone(), terminal);
        self.report_header_disagreement(&request, &resolved);
        *state = CatalogEntry::from_lookup(resolved);
        state
            .as_lookup()
            .expect("terminal state is observable through the catalog")
    }

    fn invalidate(&self, target: ImageInvalidation) -> ImageInvalidationResult {
        let target = match target {
            ImageInvalidation::Dependency(source) => {
                ImageInvalidation::Dependency(self.classify_source(source).0)
            }
            other => other,
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
        self.retire_image_ids(removed);
        result
    }

    fn cached_size_bytes(&self) -> i64 {
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
        if let Err(error) =
            schedule_image_command(&self.cmd_tx, self.render_waker.as_ref(), command, None)
        {
            tracing::warn!(%error, "failed to retire image sequence cache entry");
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

    fn retire_image_ids(&self, removed: Vec<ImageId>) {
        {
            let mut reported = self.reported_failures.borrow_mut();
            for image in &removed {
                reported.remove(image);
            }
        }
        for image in removed {
            let command = RenderCommand::Asset(AssetCommand::ImageRetire { image });
            if let Err(error) =
                schedule_image_command(&self.cmd_tx, self.render_waker.as_ref(), command, None)
            {
                tracing::warn!(image = %image, %error, "failed to schedule invalidated image release");
            }
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
    probe_image_layout(source, request.size, request.rotation, request.realization)
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
    target: crossbeam_channel::Sender<RenderCommand>,
    waker: Option<GuiEventLoopWaker>,
    command: RenderCommand,
    /// How to turn the command's raw `:file` into an absolute path off-thread,
    /// or `None` for `:data` loads and inline-resolved absolute paths.
    resolution: Option<ImageFileRequest>,
}

fn deferred_render_command_sender() -> &'static crossbeam_channel::Sender<DeferredRenderCommand> {
    static SENDER: OnceLock<crossbeam_channel::Sender<DeferredRenderCommand>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::unbounded::<DeferredRenderCommand>();
        let _ = std::thread::Builder::new()
            .name("neomacs-image-command-submit".to_owned())
            .spawn(move || {
                while let Ok(deferred) = rx.recv() {
                    let command =
                        resolve_deferred_image_path(deferred.command, deferred.resolution.as_ref());
                    if deferred.target.send(command).is_ok()
                        && let Some(waker) = deferred.waker
                    {
                        waker.wake();
                    }
                }
            });
        tx
    })
}

/// Hand a load command to the renderer, deferring to the submission worker when
/// the `:file` needs off-thread resolution (relative search, `~user` NSS) or
/// when the renderer channel is full.
fn schedule_image_command(
    target: &crossbeam_channel::Sender<RenderCommand>,
    waker: Option<&GuiEventLoopWaker>,
    command: RenderCommand,
    resolution: Option<&ImageFileRequest>,
) -> Result<(), String> {
    if resolution.is_some_and(ImageFileRequest::needs_off_thread) {
        return defer_render_command(target, waker, command, resolution.cloned());
    }
    match target.try_send(command) {
        Ok(()) => {
            if let Some(waker) = waker {
                waker.wake();
            }
            Ok(())
        }
        Err(crossbeam_channel::TrySendError::Full(command)) => {
            defer_render_command(target, waker, command, resolution.cloned())
        }
        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
            Err("failed to queue image load: channel disconnected".to_owned())
        }
    }
}

fn defer_render_command(
    target: &crossbeam_channel::Sender<RenderCommand>,
    waker: Option<&GuiEventLoopWaker>,
    command: RenderCommand,
    resolution: Option<ImageFileRequest>,
) -> Result<(), String> {
    deferred_render_command_sender()
        .send(DeferredRenderCommand {
            target: target.clone(),
            waker: waker.cloned(),
            command,
            resolution,
        })
        .map_err(|error| format!("failed to defer image load command: {error}"))
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
/// Frames of a raster source composite onto the canvas its header names, so the
/// probe's answer cannot depend on which frame is selected. Keying without the
/// frame is what lets an animation resolve its geometry once instead of once
/// per frame — see [`AsyncImageCatalog::geometry_layouts`].
fn geometry_key(request: &ImageResolveRequest) -> ImageResolveRequest {
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
