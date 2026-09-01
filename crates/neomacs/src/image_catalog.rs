//! Evaluator-owned asynchronous image catalog.
//!
//! Ordinary redisplay only mutates evaluator-local state and probes renderer
//! completion with `try_lock`. Queue backpressure is handed to one submission
//! worker, so lookup never waits for the renderer thread.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use neomacs_display_protocol::{ImageSequenceId, ImageSequenceRetirement};
use neomacs_display_runtime::render_thread::{
    ImageDecodeTerminal, ImageTerminalProbe, SharedImageRenderState,
};
use neomacs_display_runtime::thread_comm::{AssetCommand, RenderCommand};
use neovm_core::emacs_core::image_catalog::{
    FailedImage, ImageAnimationInvalidation, ImageCatalog, ImageId, ImageInvalidation,
    ImageInvalidationResult, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageLookup,
    ImagePlacement, ImageResolveRequest, ImageResolveSource, ImageStateEvent, PendingImage,
    ReadyImage,
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

/// Deep host-side module that owns image request identity, state transitions,
/// renderer scheduling, and completion observation.
pub(super) struct AsyncImageCatalog {
    cmd_tx: crossbeam_channel::Sender<RenderCommand>,
    render_waker: Option<GuiEventLoopWaker>,
    image_metadata: SharedImageRenderState,
    entries: RefCell<HashMap<ImageResolveRequest, CatalogEntry>>,
    sequence_ids: RefCell<HashMap<ImageResolveSource, ImageSequenceId>>,
    next_load_attempt: Cell<u64>,
    next_sequence_id: Cell<u64>,
    home_directory: Option<String>,
    /// GNU `image_find_image_fd` search path (`data-directory/images`, then
    /// `x-bitmap-file-path`), used to resolve relative image `:file`s.
    search_path: Vec<String>,
}

impl AsyncImageCatalog {
    pub(super) fn new(
        cmd_tx: crossbeam_channel::Sender<RenderCommand>,
        render_waker: Option<GuiEventLoopWaker>,
        image_metadata: SharedImageRenderState,
    ) -> Self {
        Self {
            cmd_tx,
            render_waker,
            image_metadata,
            entries: RefCell::new(HashMap::new()),
            sequence_ids: RefCell::new(HashMap::new()),
            next_load_attempt: Cell::new(0),
            next_sequence_id: Cell::new(0),
            home_directory: home_directory_from_environment(),
            search_path: vec![image_data_directory().to_string_lossy().into_owned()],
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
        for (request, state) in entries.iter_mut() {
            let placement = state.placement();
            let image_id = placement.image_id();
            let load = self.next_load(image_id);
            let (request, resolution) = self.classify_request(request.clone());
            let command = image_load_command(&request, load, self.sequence_id(&request.source));
            let pending = PendingImage::new(load, placement.layout());
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
                    CatalogEntry::Failed(pending.failed(error))
                }
            };
        }
    }

    pub(super) fn resolve_sync(
        &self,
        request: ImageResolveRequest,
    ) -> Result<Option<ReadyImage>, String> {
        let normalized_request = self.classify_request(request.clone()).0;
        let pending = match self.lookup(request.clone()) {
            ImageLookup::Ready(image) => return Ok(Some(image)),
            ImageLookup::Pending(image) => image,
            ImageLookup::Failed(failed) => return Err(failed.error),
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
            ImageLookup::Failed(failed) => Err(failed.error),
            ImageLookup::Pending(_) => unreachable!("terminal decode cannot remain pending"),
        }
    }
}

impl ImageCatalog for AsyncImageCatalog {
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup {
        let (request, resolution) = self.classify_request(request);
        let mut entries = self.entries.borrow_mut();
        if !entries.contains_key(&request) {
            let image_id = next_host_image_id();
            let load = self.next_load(image_id);
            let layout = placeholder_image_extent(&request);
            let pending = PendingImage::new(load, layout);
            let command = image_load_command(&request, load, self.sequence_id(&request.source));
            let state = match schedule_image_command(
                &self.cmd_tx,
                self.render_waker.as_ref(),
                command,
                resolution.as_ref(),
            ) {
                Ok(()) => CatalogEntry::Pending(pending),
                Err(error) => CatalogEntry::Failed(pending.failed(error)),
            };
            entries.insert(request.clone(), state);
        }

        let state = entries
            .get_mut(&request)
            .expect("image catalog entry inserted above");
        if let CatalogEntry::Evicted(placement) = state {
            let load = self.next_load(placement.image_id());
            let pending = PendingImage::new(load, placement.layout());
            let command = image_load_command(&request, load, self.sequence_id(&request.source));
            *state = match schedule_image_command(
                &self.cmd_tx,
                self.render_waker.as_ref(),
                command,
                resolution.as_ref(),
            ) {
                Ok(()) => CatalogEntry::Pending(pending),
                Err(error) => CatalogEntry::Failed(pending.failed(error)),
            };
        }
        let CatalogEntry::Pending(pending) = state else {
            return state
                .as_lookup()
                .expect("evicted entry was transitioned above");
        };
        let load = pending.load();
        let terminal = match self.image_metadata.try_terminal(load) {
            ImageTerminalProbe::Busy => {
                return state.as_lookup().expect("pending state is observable");
            }
            ImageTerminalProbe::Available(terminal) => terminal,
        };
        let Some(terminal) = terminal else {
            return state.as_lookup().expect("pending state is observable");
        };
        *state = CatalogEntry::from_lookup(image_lookup_from_terminal(pending.clone(), terminal));
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
        let removed = {
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
            requests
                .into_iter()
                .filter_map(|request| entries.remove(&request))
                .map(|state| state.placement().image_id())
                .collect::<Vec<_>>()
        };

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
        let Some(state) = entries
            .values_mut()
            .find(|state| state.placement().image_id() == event.image())
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
                *state = CatalogEntry::from_lookup(image_lookup_from_terminal(
                    pending.clone(),
                    terminal,
                ));
            }
            ImageStateEvent::Evicted(_) => {
                let placement = state.placement();
                *state = CatalogEntry::Evicted(placement);
            }
        }
    }
}

impl AsyncImageCatalog {
    fn retire_image_ids(&self, removed: Vec<ImageId>) {
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

fn image_load_command(
    request: &ImageResolveRequest,
    load: ImageLoadToken,
    sequence: ImageSequenceId,
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
        }),
    }
}

fn placeholder_image_extent(request: &ImageResolveRequest) -> ImageLayoutExtent {
    let (width, height) = request.size.placeholder_extent().unwrap_or((1, 1));
    ImageLayoutExtent::new(
        request.realization.layout_dimension(width),
        request.realization.layout_dimension(height),
    )
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
mod tests {
    use super::*;
    use neomacs_display_runtime::render_thread::ImageRenderState;
    use neovm_core::emacs_core::Context;
    use neovm_core::emacs_core::Value;
    use neovm_core::emacs_core::image_catalog::{
        AxisSize, ImageColorContext, ImageDefaultScale, ImageScaleEnvironment, ImageScalePolicy,
        ImageSizeSpec, ImageSpecIdentity,
    };
    use std::sync::Arc;

    thread_local! {
        static IMAGE_SPEC_TEST_CONTEXT: Context = Context::new();
    }

    fn file_request(path: &str) -> ImageResolveRequest {
        let spec = IMAGE_SPEC_TEST_CONTEXT.with(|_| {
            Value::list(vec![
                Value::symbol("image"),
                Value::keyword(":type"),
                Value::symbol("png"),
                Value::keyword(":file"),
                Value::string(path),
            ])
        });
        ImageResolveRequest {
            spec: ImageSpecIdentity::from_lisp_spec(&spec).expect("test image spec"),
            source: ImageResolveSource::File(LispString::from_utf8(path)),
            size: ImageSizeSpec::new(AxisSize::AtMost(24), AxisSize::AtMost(24)),
            rotation: Default::default(),
            colors: ImageColorContext::default(),
            mask: Default::default(),
            frame: Default::default(),
            realization: Default::default(),
        }
    }

    fn classify(file: &str) -> (ImageResolveRequest, Option<ImageFileRequest>) {
        let (cmd_tx, _cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, metadata);
        catalog.classify_request(file_request(file))
    }

    #[test]
    fn relative_file_is_classified_for_off_thread_search() {
        // The #242 fix: a bare relative `:file` must be searched against
        // data-directory/images off-thread, not opened verbatim from the cwd.
        let (request, resolution) = classify("splash.svg");
        assert!(matches!(
            &request.source,
            ImageResolveSource::File(p) if p.as_utf8_str() == Some("splash.svg")
        ));
        let resolution = resolution.expect("file source is classified");
        assert!(matches!(resolution, ImageFileRequest::Search { .. }));
        assert!(resolution.needs_off_thread());
    }

    #[test]
    fn absolute_file_is_resolved_inline_and_keys_on_itself() {
        let (request, resolution) = classify("/abs/icon.png");
        assert!(matches!(
            &request.source,
            ImageResolveSource::File(p) if p.as_utf8_str() == Some("/abs/icon.png")
        ));
        let resolution = resolution.expect("file source is classified");
        assert!(matches!(resolution, ImageFileRequest::Direct(_)));
        assert!(!resolution.needs_off_thread());
    }

    #[test]
    fn named_user_file_is_deferred_off_thread() {
        // `~user` may consult NSS/LDAP; keep resolution off the evaluator thread.
        let (request, resolution) = classify("~some-user/x.png");
        assert!(matches!(
            &request.source,
            ImageResolveSource::File(p) if p.as_utf8_str() == Some("~some-user/x.png")
        ));
        let resolution = resolution.expect("file source is classified");
        assert!(matches!(resolution, ImageFileRequest::ExpandHome(_)));
        assert!(resolution.needs_off_thread());
    }

    #[test]
    fn pending_slot_and_decode_command_share_one_resolved_realization() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, metadata);
        let mut request = file_request("/tmp/icon.svg");
        // Neither axis pinned: the placeholder falls back to the realization.
        request.size = ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(24));
        request.realization = ImageScaleEnvironment::new(7.2, 1.75, ImageDefaultScale::Auto)
            .resolve(ImageScalePolicy::Default);

        let placement = catalog.lookup(request).placement();

        assert_eq!(placement.width(), 18);
        assert_eq!(placement.height(), 18);
        assert!(matches!(
            cmd_rx.try_recv().expect("image load command"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile {
                realization,
                ..
            }) if (realization.layout_scale() - (1.3 / 1.75)).abs() < 0.0001
                && (realization.device_scale() - 1.75).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn invalidate_all_requeues_every_entry_under_its_existing_id() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, metadata);

        let first = catalog
            .lookup(file_request("/tmp/one.png"))
            .placement()
            .image_id();
        let second = catalog
            .lookup(file_request("/tmp/two.png"))
            .placement()
            .image_id();
        // Drain the two initial load commands.
        assert!(cmd_rx.try_recv().is_ok());
        assert!(cmd_rx.try_recv().is_ok());

        catalog.invalidate_all();

        let mut requeued_ids = Vec::new();
        while let Ok(command) = cmd_rx.try_recv() {
            match command {
                RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. }) => {
                    requeued_ids.push(load.image());
                }
                other => panic!("unexpected command re-queued: {other:?}"),
            }
        }
        requeued_ids.sort_unstable();
        let mut expected = vec![first, second];
        expected.sort_unstable();
        assert_eq!(requeued_ids, expected, "same ids, one command per entry");

        // The entries survive: a later lookup reuses the id, no new load.
        let again = catalog
            .lookup(file_request("/tmp/one.png"))
            .placement()
            .image_id();
        assert_eq!(again, first);
        assert!(cmd_rx.try_recv().is_err());
    }

    #[test]
    fn invalidating_dependency_retires_old_identity_and_next_lookup_reloads() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, metadata);
        let request = file_request("/tmp/watched.svg");

        let first = catalog.lookup(request.clone()).placement().image_id();
        assert!(matches!(
            cmd_rx.try_recv().expect("initial image load"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. })
                if load.image() == first
        ));

        catalog.invalidate(ImageInvalidation::Dependency(request.source.clone()));
        assert!(matches!(
            cmd_rx.try_recv().expect("old image identity retired"),
            RenderCommand::Asset(AssetCommand::ImageRetire { image }) if image == first
        ));

        let second = catalog.lookup(request).placement().image_id();
        assert_ne!(first, second);
        assert!(matches!(
            cmd_rx.try_recv().expect("replacement image load"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. })
                if load.image() == second
        ));
    }

    #[test]
    fn invalidating_spec_preserves_other_spec_that_uses_same_dependency() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, metadata);
        let first = file_request("/tmp/multi-page.png");
        let mut second = first.clone();
        let second_spec = Value::list(vec![
            Value::symbol("image"),
            Value::keyword(":type"),
            Value::symbol("png"),
            Value::keyword(":file"),
            Value::string("/tmp/multi-page.png"),
            Value::keyword(":index"),
            Value::fixnum(1),
        ]);
        second.spec =
            ImageSpecIdentity::from_lisp_spec(&second_spec).expect("second test image spec");

        let first_id = catalog.lookup(first.clone()).placement().image_id();
        let second_id = catalog.lookup(second.clone()).placement().image_id();
        assert_ne!(first_id, second_id);
        assert!(cmd_rx.try_recv().is_ok());
        assert!(cmd_rx.try_recv().is_ok());

        catalog.invalidate(ImageInvalidation::Spec {
            spec: first.spec.clone(),
        });
        assert!(matches!(
            cmd_rx.try_recv().expect("only exact spec identity freed"),
            RenderCommand::Asset(AssetCommand::ImageRetire { image }) if image == first_id
        ));
        assert!(cmd_rx.try_recv().is_err());

        assert_eq!(
            catalog.lookup(second).placement().image_id(),
            second_id,
            "the other spec keeps its renderer identity"
        );
        assert!(cmd_rx.try_recv().is_err());

        let replacement_id = catalog.lookup(first).placement().image_id();
        assert_ne!(replacement_id, first_id);
        assert!(matches!(
            cmd_rx.try_recv().expect("exact spec is decoded again"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. })
                if load.image() == replacement_id
        ));
    }

    #[test]
    fn renderer_reconciliation_upgrades_pending_to_ready_geometry() {
        use neomacs_display_runtime::render_thread::ImageDecodeTerminal;
        use neovm_core::emacs_core::image_catalog::{ImageLookup, ResolvedImageMetadata};

        let (cmd_tx, _cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, Arc::clone(&metadata));
        let request = file_request("/tmp/promote.png");

        let ImageLookup::Pending(pending) = catalog.lookup(request.clone()) else {
            panic!("expected pending");
        };
        let id = pending.placement().image_id();
        let load = pending.load();
        // Placeholder from AtMost(24) pins.
        assert_eq!(pending.placement().width(), 24);

        metadata.publish_terminal(
            load,
            ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
                120,
                80,
                0,
                false,
                Default::default(),
            )),
        );

        catalog.reconcile_renderer_state(ImageStateEvent::DecodeCompleted(load));
        let ImageLookup::Ready(ready) = catalog.lookup(request) else {
            panic!("promote must leave Ready geometry for rebuild");
        };
        assert_eq!(ready.metadata.layout.dimensions(), (120, 80));
        assert_eq!(ready.image_id(), id);
    }

    #[test]
    fn renderer_eviction_requeues_ready_image_under_its_stable_id() {
        use neomacs_display_runtime::render_thread::ImageDecodeTerminal;
        use neovm_core::emacs_core::image_catalog::{ImageLookup, ResolvedImageMetadata};

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, Arc::clone(&metadata));
        let request = file_request("/tmp/room-avatar.png");

        let ImageLookup::Pending(pending) = catalog.lookup(request.clone()) else {
            panic!("new avatar should begin pending");
        };
        let id = pending.placement().image_id();
        let first_load = pending.load();
        assert!(matches!(
            cmd_rx.try_recv().expect("initial avatar load"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. })
                if load == first_load
        ));

        metadata.publish_terminal(
            first_load,
            ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
                48,
                48,
                0,
                false,
                Default::default(),
            )),
        );
        catalog.reconcile_renderer_state(ImageStateEvent::DecodeCompleted(first_load));
        assert!(matches!(
            catalog.lookup(request.clone()),
            ImageLookup::Ready(_)
        ));

        // The renderer's LRU dropped the texture. Its lifecycle notification
        // removes residency metadata before asking the catalog to reconcile.
        metadata.remove_terminal(first_load);
        catalog.reconcile_renderer_state(ImageStateEvent::Evicted(id));

        let ImageLookup::Pending(reloading) = catalog.lookup(request) else {
            panic!("evicted avatar should remain pending until its reload completes");
        };
        assert!(matches!(
            cmd_rx.try_recv().expect("evicted avatar reload"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. })
                if load.image() == id && load != first_load
        ));
        assert_eq!(reloading.placement().image_id(), id);
        assert_eq!(reloading.placement().width(), 48);
        assert_eq!(reloading.placement().height(), 48);
    }

    #[test]
    fn eviction_after_decode_but_before_evaluator_service_does_not_strand_pending_image() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, metadata);
        let request = file_request("/tmp/large-chat-photo.png");

        let ImageLookup::Pending(first_load) = catalog.lookup(request.clone()) else {
            panic!("new image should begin pending");
        };
        let id = first_load.placement().image_id();
        let first_token = first_load.load();
        assert!(cmd_rx.try_recv().is_ok(), "initial load was queued");

        // The renderer can publish Ready and then evict the same image in one
        // batch. By the time the evaluator services both ordered events, the
        // shared metadata map is already empty; the typed eviction reason must
        // still move Pending -> Evicted instead of leaving it pending forever.
        catalog.reconcile_renderer_state(ImageStateEvent::DecodeCompleted(first_token));
        catalog.reconcile_renderer_state(ImageStateEvent::Evicted(id));

        let ImageLookup::Pending(reload) = catalog.lookup(request) else {
            panic!("visible evicted image should schedule another load");
        };
        assert_eq!(reload.placement().image_id(), id);
        assert!(matches!(
            cmd_rx.try_recv().expect("replacement load"),
            RenderCommand::Asset(AssetCommand::ImageLoadFile { load, .. })
                if load.image() == id && load != first_token
        ));
    }

    #[test]
    fn stale_decode_completion_cannot_promote_a_replacement_load() {
        use neomacs_display_runtime::render_thread::ImageDecodeTerminal;
        use neovm_core::emacs_core::image_catalog::ResolvedImageMetadata;

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let metadata = Arc::new(ImageRenderState::default());
        let catalog = AsyncImageCatalog::new(cmd_tx, None, Arc::clone(&metadata));
        let request = file_request("/tmp/replaced-avatar.png");

        let ImageLookup::Pending(first) = catalog.lookup(request.clone()) else {
            panic!("initial image should be pending");
        };
        let first_load = first.load();
        cmd_rx.try_recv().expect("initial load command");

        catalog.reconcile_renderer_state(ImageStateEvent::Evicted(first_load.image()));
        let ImageLookup::Pending(replacement) = catalog.lookup(request.clone()) else {
            panic!("eviction should schedule a replacement load");
        };
        let replacement_load = replacement.load();
        assert_eq!(replacement_load.image(), first_load.image());
        assert_ne!(replacement_load, first_load);
        cmd_rx.try_recv().expect("replacement load command");

        metadata.publish_terminal(
            first_load,
            ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
                120,
                80,
                0,
                false,
                Default::default(),
            )),
        );
        catalog.reconcile_renderer_state(ImageStateEvent::DecodeCompleted(first_load));

        let ImageLookup::Pending(still_replacement) = catalog.lookup(request) else {
            panic!("a stale completion must not promote the replacement attempt");
        };
        assert_eq!(still_replacement.load(), replacement_load);
    }
}
