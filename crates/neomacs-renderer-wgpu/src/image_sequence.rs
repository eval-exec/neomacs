use image::AnimationDecoder;
use neomacs_display_protocol::{
    EncodedBytes, ImageAnimationPolicy, ImageColorContext, ImageEmbeddedMetadata, ImageFrameDelay,
    ImageFrameIndex, ImageSequenceId, ImageSequenceRetirement,
};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_SEQUENCE_CACHE_BYTES: usize = 64 * 1024 * 1024;
/// Drop idle GIF streams after 60 s, matching GNU `anim_prune_animation_cache`
/// (`src/image.c:3768-3794`).
const IDLE_STREAM_TTL: Duration = Duration::from_secs(60);
/// Streams accessed within this window are treated as actively animating and
/// spared from budget eviction until idle streams and materialized sequences
/// have been evicted first.
const ACTIVE_STREAM_WINDOW: Duration = Duration::from_secs(5);
/// Upper bound for a single GIF stream's reverse-playback window.
const MAX_BACKWARD_WINDOW_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct DecodedImageSequence {
    frames: Vec<DecodedSequenceFrame>,
    memory_size: usize,
    loop_start: Option<ImageFrameIndex>,
}

#[derive(Clone)]
struct DecodedSequenceFrame {
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
    delay: ImageFrameDelay,
    embedded: Option<ImageEmbeddedMetadata>,
}

impl DecodedImageSequence {
    /// Assemble a sequence from already-decoded frames.
    ///
    /// Uniform-delay sequences have no introductory interval. Producers that
    /// carry a timed introduction use `from_timed_frames` instead.
    pub(crate) fn from_frames(
        frames: Vec<(u32, u32, Vec<u8>)>,
        delay: ImageFrameDelay,
    ) -> Arc<Self> {
        let mut memory_size = 0_usize;
        let frames = frames
            .into_iter()
            .map(|(width, height, rgba)| {
                memory_size = memory_size.saturating_add(rgba.len());
                let rgba: Arc<[u8]> = rgba.into();
                DecodedSequenceFrame {
                    width,
                    height,
                    rgba,
                    delay,
                    embedded: None,
                }
            })
            .collect();
        Arc::new(Self {
            frames,
            memory_size,
            loop_start: None,
        })
    }

    /// Preserve individual delays and a validated repeatable tail.
    pub(crate) fn from_timed_frames(
        frames: Vec<(u32, u32, Vec<u8>, ImageFrameDelay)>,
        loop_start: Option<ImageFrameIndex>,
    ) -> Option<Arc<Self>> {
        let count = u32::try_from(frames.len()).ok()?;
        if count == 0 || loop_start.is_some_and(|start| start.get() >= u64::from(count)) {
            return None;
        }
        let loop_start = loop_start.filter(|start| !start.is_first());
        if let Some(start) = loop_start {
            let start = usize::try_from(start.get()).ok()?;
            let prefix_delay = frames.first()?.3;
            let loop_delay = frames.get(start)?.3;
            // The compatibility metadata describes one delay per segment.
            // Refuse a sequence that cannot be represented by that table.
            if frames[..start].iter().any(|frame| frame.3 != prefix_delay)
                || frames[start..].iter().any(|frame| frame.3 != loop_delay)
            {
                return None;
            }
        }
        let mut memory_size = 0_usize;
        let frames = frames
            .into_iter()
            .map(|(width, height, rgba, delay)| {
                memory_size = memory_size.checked_add(rgba.len())?;
                Some(DecodedSequenceFrame {
                    width,
                    height,
                    rgba: rgba.into(),
                    delay,
                    embedded: None,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Arc::new(Self {
            frames,
            memory_size,
            loop_start,
        }))
    }

    fn frame(&self, index: ImageFrameIndex) -> Option<ImageSequenceFrame> {
        let index = usize::try_from(index.get()).ok()?;
        let frame = self.frames.get(index)?;
        let mut embedded = if let Some(embedded) = &frame.embedded {
            embedded.clone()
        } else if self.frames.len() > 1 {
            ImageEmbeddedMetadata::animation(u32::try_from(self.frames.len()).ok()?, frame.delay)
        } else {
            ImageEmbeddedMetadata::EMPTY
        };
        if let Some(loop_start) = self.loop_start {
            let start = usize::try_from(loop_start.get()).ok()?;
            embedded = embedded.with_introduction(
                loop_start,
                self.frames.first()?.delay,
                self.frames.get(start)?.delay,
            )?;
        }
        Some(frame.sequence_frame_with(embedded))
    }
}

impl DecodedSequenceFrame {
    /// The frame as its caller receives it.
    fn sequence_frame_with(&self, embedded: ImageEmbeddedMetadata) -> ImageSequenceFrame {
        ImageSequenceFrame {
            width: self.width,
            height: self.height,
            rgba: self.rgba.to_vec(),
            embedded,
        }
    }

    /// The frame with the metadata its decoder attached, as a stream frame
    /// always has one.
    fn sequence_frame(&self) -> ImageSequenceFrame {
        self.sequence_frame_with(
            self.embedded
                .clone()
                .unwrap_or(ImageEmbeddedMetadata::EMPTY),
        )
    }

    /// Pixels plus the extension payload the frame carries.
    fn memory_size(&self) -> usize {
        self.rgba.len()
            + self
                .embedded
                .as_ref()
                .and_then(|metadata| metadata.gif_extensions())
                .map_or(0, |blocks| {
                    blocks.iter().map(|block| block.bytes.len()).sum()
                })
    }
}

pub(crate) struct ImageSequenceFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    embedded: ImageEmbeddedMetadata,
}

impl ImageSequenceFrame {
    pub(crate) const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn into_parts(self) -> (Vec<u8>, ImageEmbeddedMetadata) {
        (self.rgba, self.embedded)
    }

    #[cfg(test)]
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

pub(crate) enum ImageSequenceResolution {
    NotAnimated,
    Frame(ImageSequenceFrame),
    MissingFrame,
    /// A sequence format owned by this cache (GIF) failed validation or
    /// decoding and must not fall back to the generic `image`-crate still
    /// decoder.
    Failed,
}

impl ImageSequenceResolution {
    #[cfg(test)]
    fn expect_frame(self, message: &str) -> ImageSequenceFrame {
        match self {
            Self::Frame(frame) => frame,
            Self::NotAnimated | Self::MissingFrame | Self::Failed => panic!("{message}"),
        }
    }
}

/// Cache key distinguishing materialized sequences from color-dependent GIF
/// streams so two displays of the same GIF with different `:background` or
/// frame background colors coexist without evicting each other every tick.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum SequenceCacheKey {
    Materialized(ImageSequenceId),
    GifStream(ImageSequenceId, GifBackgroundColors),
}

impl SequenceCacheKey {
    const fn sequence(self) -> ImageSequenceId {
        match self {
            Self::Materialized(sequence) | Self::GifStream(sequence, _) => sequence,
        }
    }
}

/// Inputs that determine a sequence's decoded frames.
///
/// Authored raster sequences have no computed rendering context. Computed SVG
/// sequences require every sampling input to match before a resident entry can
/// be reused; the enum prevents publishing one without its policy or resources.
#[derive(Clone, Debug, Eq, PartialEq)]
enum SequenceMaterialization {
    AuthoredRaster,
    ComputedSvg {
        colors: ImageColorContext,
        resources: crate::svg::SvgResourceContext,
        policy: ImageAnimationPolicy,
    },
}

enum SequenceCacheEntry {
    Animated {
        sequence: Arc<DecodedImageSequence>,
        materialization: SequenceMaterialization,
        last_access: u64,
    },
    /// A GIF decoded frame by frame: GNU's `anim_cache` entry. Keyed by
    /// `(ImageSequenceId, GifBackgroundColors)` in `ImageSequenceCacheState`
    /// because GNU `gif_load` bakes `frame_bg` and `specified_bg` into
    /// transparent pixels and uncovered borders.
    Stream {
        stream: Arc<GifStream>,
        memory_size: usize,
        last_access: u64,
        last_access_at: Instant,
    },
}

impl SequenceCacheEntry {
    const fn last_access(&self) -> u64 {
        match self {
            Self::Animated { last_access, .. } | Self::Stream { last_access, .. } => *last_access,
        }
    }

    fn memory_size(&self) -> usize {
        match self {
            Self::Animated { sequence, .. } => sequence.memory_size,
            Self::Stream { memory_size, .. } => *memory_size,
        }
    }

    /// Eviction tier under budget pressure:
    /// 0 = idle stream (not stepped within `ACTIVE_STREAM_WINDOW`)
    /// 1 = materialized `Animated` sequence
    /// 2 = actively animating stream
    fn eviction_priority(&self, now: Instant) -> u8 {
        match self {
            Self::Stream { last_access_at, .. }
                if now.saturating_duration_since(*last_access_at) >= ACTIVE_STREAM_WINDOW =>
            {
                0
            }
            Self::Animated { .. } => 1,
            Self::Stream { .. } => 2,
        }
    }

    /// The materialized sequence when this entry holds one with exactly the
    /// inputs of `materialization`. A stream is never that entry: it is
    /// looked up by [`ImageSequenceCacheState::stream_for`] alone.
    fn animated_sequence(
        &self,
        materialization: &SequenceMaterialization,
    ) -> Option<Arc<DecodedImageSequence>> {
        match self {
            Self::Animated {
                sequence,
                materialization: resident,
                ..
            } if resident == materialization => Some(Arc::clone(sequence)),
            _ => None,
        }
    }

    fn touch(&mut self, stamp: u64, now: Instant) {
        match self {
            Self::Animated { last_access, .. } => {
                *last_access = stamp;
            }
            Self::Stream {
                last_access,
                last_access_at,
                ..
            } => {
                *last_access = stamp;
                *last_access_at = now;
            }
        }
    }
}

#[derive(Default)]
struct ImageSequenceCacheState {
    entries: HashMap<SequenceCacheKey, SequenceCacheEntry>,
    in_flight: HashMap<ImageSequenceId, usize>,
    individually_retired: HashSet<ImageSequenceId>,
    retired_through: Option<ImageSequenceId>,
    total_bytes: usize,
    access_clock: u64,
    hits: u64,
    misses: u64,
}

impl ImageSequenceCacheState {
    fn next_access(&mut self) -> u64 {
        self.access_clock = self.access_clock.saturating_add(1);
        self.access_clock
    }

    fn is_retired(&self, sequence: ImageSequenceId) -> bool {
        self.retired_through
            .is_some_and(|retired_through| sequence <= retired_through)
            || self.individually_retired.contains(&sequence)
    }

    /// Drop GIF streams idle for at least `ttl`, matching GNU
    /// `anim_prune_animation_cache` (`src/image.c:3768-3794`).
    fn prune_idle_streams(&mut self, now: Instant, ttl: Duration) {
        let expired: Vec<SequenceCacheKey> = self
            .entries
            .iter()
            .filter_map(|(key, entry)| match entry {
                SequenceCacheEntry::Stream { last_access_at, .. }
                    if now.saturating_duration_since(*last_access_at) >= ttl =>
                {
                    Some(*key)
                }
                _ => None,
            })
            .collect();
        for key in expired {
            self.remove_key(key);
        }
    }

    /// A resident entry with exactly the inputs requested by this worker.
    fn entry_for(
        &mut self,
        sequence: ImageSequenceId,
        materialization: &SequenceMaterialization,
        now: Instant,
    ) -> Option<&mut SequenceCacheEntry> {
        self.prune_idle_streams(now, IDLE_STREAM_TTL);
        let stamp = self.next_access();
        let hit = self
            .entries
            .get_mut(&SequenceCacheKey::Materialized(sequence))
            .filter(|entry| entry.animated_sequence(materialization).is_some());
        match hit {
            Some(entry) => {
                self.hits = self.hits.saturating_add(1);
                entry.touch(stamp, now);
                Some(entry)
            }
            None => {
                self.misses = self.misses.saturating_add(1);
                None
            }
        }
    }

    fn remove_key(&mut self, key: SequenceCacheKey) {
        if let Some(entry) = self.entries.remove(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.memory_size());
        }
    }

    fn remove_sequence(&mut self, sequence: ImageSequenceId) {
        let keys: Vec<SequenceCacheKey> = self
            .entries
            .keys()
            .copied()
            .filter(|key| key.sequence() == sequence)
            .collect();
        for key in keys {
            self.remove_key(key);
        }
    }

    /// The resident stream for `(sequence, colors)`, without counting a miss
    /// when absent.
    fn existing_stream_for(
        &mut self,
        sequence: ImageSequenceId,
        colors: GifBackgroundColors,
        now: Instant,
    ) -> Option<Arc<GifStream>> {
        self.prune_idle_streams(now, IDLE_STREAM_TTL);
        let stamp = self.next_access();
        match self
            .entries
            .get_mut(&SequenceCacheKey::GifStream(sequence, colors))
        {
            Some(SequenceCacheEntry::Stream {
                stream,
                last_access,
                last_access_at,
                ..
            }) => {
                self.hits = self.hits.saturating_add(1);
                *last_access = stamp;
                *last_access_at = now;
                Some(Arc::clone(stream))
            }
            _ => None,
        }
    }

    /// The resident stream for `(sequence, colors)`, counted like `entry_for`
    /// counts a materialized hit. Any other entry, or none, is a miss.
    fn stream_for(
        &mut self,
        sequence: ImageSequenceId,
        colors: GifBackgroundColors,
        now: Instant,
    ) -> Option<Arc<GifStream>> {
        match self.existing_stream_for(sequence, colors, now) {
            Some(stream) => Some(stream),
            None => {
                self.misses = self.misses.saturating_add(1);
                None
            }
        }
    }

    /// Make `stream` the resident stream for `(sequence, colors)`. A stream
    /// opened while another request retired the identity is not published,
    /// exactly as a late materialized decode is not, and a concurrent creator's
    /// stream for the same `(sequence, colors)` wins. Distinct background
    /// contexts on the same GIF coexist without evicting each other.
    fn insert_stream(
        &mut self,
        sequence: ImageSequenceId,
        colors: GifBackgroundColors,
        stream: Arc<GifStream>,
        now: Instant,
    ) -> Arc<GifStream> {
        if self.is_retired(sequence) {
            return stream;
        }
        let key = SequenceCacheKey::GifStream(sequence, colors);
        if let Some(SequenceCacheEntry::Stream {
            stream: resident, ..
        }) = self.entries.get(&key)
        {
            return Arc::clone(resident);
        }
        self.remove_key(key);
        let last_access = self.next_access();
        self.entries.insert(
            key,
            SequenceCacheEntry::Stream {
                stream: Arc::clone(&stream),
                memory_size: 0,
                last_access,
                last_access_at: now,
            },
        );
        stream
    }

    /// Record what `stream` retains, then keep the budget by evicting expired
    /// idle streams first, then LRU idle streams and materialized entries,
    /// before displacing any other actively animating stream. A stream is never
    /// refused for its own size: GNU retains the composed frame of any
    /// animation however large (its `anim_cache` has no byte budget,
    /// src/image.c:3773-3793), and refusing one is the redecode on every frame
    /// that #823 reports.
    fn account_stream(
        &mut self,
        sequence: ImageSequenceId,
        colors: GifBackgroundColors,
        stream: &Arc<GifStream>,
        bytes: usize,
        max_bytes: usize,
        now: Instant,
    ) {
        self.prune_idle_streams(now, IDLE_STREAM_TTL);
        let key = SequenceCacheKey::GifStream(sequence, colors);
        {
            let Some(SequenceCacheEntry::Stream {
                stream: resident,
                memory_size,
                last_access_at,
                ..
            }) = self.entries.get_mut(&key)
            else {
                return;
            };
            if !Arc::ptr_eq(resident, stream) {
                return;
            }
            self.total_bytes = self
                .total_bytes
                .saturating_sub(*memory_size)
                .saturating_add(bytes);
            *memory_size = bytes;
            *last_access_at = now;
        }
        while self.total_bytes > max_bytes {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(candidate, entry)| **candidate != key && entry.memory_size() > 0)
                .min_by_key(|(candidate, entry)| {
                    (
                        entry.eviction_priority(now),
                        entry.last_access(),
                        **candidate,
                    )
                })
                .map(|(candidate, _)| *candidate)
            else {
                break;
            };
            self.remove_key(victim);
        }
    }

    /// A stream whose source failed to decode is dropped, as a failed
    /// materialized decode was never published.
    fn discard_stream(
        &mut self,
        sequence: ImageSequenceId,
        colors: GifBackgroundColors,
        stream: &Arc<GifStream>,
    ) {
        let key = SequenceCacheKey::GifStream(sequence, colors);
        let is_resident = matches!(
            self.entries.get(&key),
            Some(SequenceCacheEntry::Stream { stream: resident, .. }) if Arc::ptr_eq(resident, stream)
        );
        if is_resident {
            self.remove_key(key);
        }
    }

    fn begin_decode(&mut self, sequence: ImageSequenceId) {
        *self.in_flight.entry(sequence).or_default() += 1;
    }

    fn finish_decode(&mut self, sequence: ImageSequenceId) {
        let Some(count) = self.in_flight.get_mut(&sequence) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.in_flight.remove(&sequence);
            self.individually_retired.remove(&sequence);
        }
    }
}

/// Decoder/compositor cache shared by the image worker pool.
///
/// It owns CPU-side composited animation frames only. GPU textures remain in
/// `ImageCache`, keyed by `ImageId`; this separation matches GNU's independent
/// animation and image caches and avoids re-decoding an entire sequence for
/// every `:index` mutation. Concurrent misses may decode redundantly rather
/// than holding the mutex across decoder work; publication coalesces them into
/// one resident entry and retirement fences every late result.
///
/// A multi-frame GIF holds no materialized entry: its source keeps a
/// [`GifStream`] (retained encoded bytes, decoder positions, composed frames,
/// and a bounded reverse-playback window), stepped under the stream's own lock.
/// Still (single-frame) GIFs decode without staying resident in this cache.
/// Streams idle for 60 s are pruned (GNU `anim_prune_animation_cache`), and
/// budget pressure evicts idle streams before actively animating ones.
pub(crate) struct ImageSequenceCache {
    max_bytes: usize,
    state: Mutex<ImageSequenceCacheState>,
    #[cfg(test)]
    decoded_frames: Arc<std::sync::atomic::AtomicU64>,
}

impl ImageSequenceCache {
    pub(crate) fn new() -> Self {
        Self::with_max_bytes(DEFAULT_SEQUENCE_CACHE_BYTES)
    }

    fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            state: Mutex::new(ImageSequenceCacheState::default()),
            #[cfg(test)]
            decoded_frames: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    fn backward_window_limit(&self) -> usize {
        (self.max_bytes / 2).min(MAX_BACKWARD_WINDOW_BYTES)
    }

    #[cfg(test)]
    fn decoded_frames(&self) -> u64 {
        self.decoded_frames
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    fn reset_decoded_frames(&self) {
        self.decoded_frames
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn resolve(
        &self,
        sequence: ImageSequenceId,
        data: &EncodedBytes,
        frame: ImageFrameIndex,
        colors: ImageColorContext,
    ) -> ImageSequenceResolution {
        self.resolve_at(sequence, data, frame, colors, Instant::now())
    }

    fn resolve_at(
        &self,
        sequence: ImageSequenceId,
        data: &EncodedBytes,
        frame: ImageFrameIndex,
        colors: ImageColorContext,
        now: Instant,
    ) -> ImageSequenceResolution {
        if crate::image_probe::has_tiff_signature(data) {
            return match tiff_pages::decode(data, frame) {
                Some(frame) => ImageSequenceResolution::Frame(frame),
                None => ImageSequenceResolution::MissingFrame,
            };
        }
        if matches!(image::guess_format(data), Ok(image::ImageFormat::Gif)) {
            return self.resolve_stream_at(
                sequence,
                data,
                frame,
                GifBackgroundColors::from_context(colors),
                now,
            );
        }
        self.resolve_with(
            sequence,
            frame,
            SequenceMaterialization::AuthoredRaster,
            || decode_sequence(data),
        )
    }

    /// Step an already-resident GIF stream without re-reading the file from disk.
    ///
    /// Returns `None` when no stream is resident for `sequence` with matching
    /// GIF background colors, without counting a cache miss (the caller still
    /// runs the full decode path).
    pub(crate) fn resolve_resident_gif(
        &self,
        sequence: ImageSequenceId,
        frame: ImageFrameIndex,
        colors: ImageColorContext,
    ) -> Option<ImageSequenceResolution> {
        let now = Instant::now();
        let gif_colors = GifBackgroundColors::from_context(colors);
        let stream = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .existing_stream_for(sequence, gif_colors, now)?;
        Some(self.step_stream(sequence, gif_colors, stream, frame, now))
    }

    /// One frame of a GIF, from the stream retained for its source.
    fn resolve_stream_at(
        &self,
        sequence: ImageSequenceId,
        source: &EncodedBytes,
        frame: ImageFrameIndex,
        colors: GifBackgroundColors,
        now: Instant,
    ) -> ImageSequenceResolution {
        self.resolve_stream_with_at(sequence, frame, colors, now, || {
            GifStream::open(source.clone(), colors)
        })
    }

    #[cfg(test)]
    fn resolve_stream_with(
        &self,
        sequence: ImageSequenceId,
        frame: ImageFrameIndex,
        colors: ImageColorContext,
        open: impl FnOnce() -> Option<GifStream>,
    ) -> ImageSequenceResolution {
        self.resolve_stream_with_at(
            sequence,
            frame,
            GifBackgroundColors::from_context(colors),
            Instant::now(),
            open,
        )
    }

    fn resolve_stream_with_at(
        &self,
        sequence: ImageSequenceId,
        frame: ImageFrameIndex,
        colors: GifBackgroundColors,
        now: Instant,
        open: impl FnOnce() -> Option<GifStream>,
    ) -> ImageSequenceResolution {
        let existing_or_lease = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            match state.stream_for(sequence, colors, now) {
                Some(stream) => Ok(stream),
                None => {
                    // Track the in-flight open under `DecodeLease` so an
                    // individual `retire(One(sequence))` arriving while the
                    // header is read fences publication via `individually_retired`.
                    state.begin_decode(sequence);
                    Err(DecodeLease {
                        cache: self,
                        sequence,
                    })
                }
            }
        };
        // The header read happens outside the cache lock: it is this
        // request's work, not a reason to stall every other sequence.
        let stream = match existing_or_lease {
            Ok(stream) => stream,
            Err(_lease) => {
                #[allow(unused_mut)]
                let Some(mut stream) = open() else {
                    // GIF is owned by `GifStream` (both stills and animations):
                    // reject the whole image instead of falling back to the
                    // generic `image`-crate still decoder.
                    return ImageSequenceResolution::Failed;
                };
                #[cfg(test)]
                stream.attach_counter(Arc::clone(&self.decoded_frames));
                if stream.frame_count() <= 1 {
                    // Still (1-frame) GIFs carry GCE metadata through the same
                    // decoder, but GNU does not create an `anim_cache` entry
                    // for stills: answer the caller without retaining a stream.
                    let (decoded, _) = stream.frame(frame, 0);
                    return match decoded {
                        StreamFrame::Ready(frame) => ImageSequenceResolution::Frame(frame),
                        StreamFrame::Failed => ImageSequenceResolution::Failed,
                        StreamFrame::Missing => ImageSequenceResolution::MissingFrame,
                    };
                }
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.insert_stream(sequence, colors, Arc::new(stream), now)
            }
        };
        self.step_stream(sequence, colors, stream, frame, now)
    }

    fn step_stream(
        &self,
        sequence: ImageSequenceId,
        colors: GifBackgroundColors,
        stream: Arc<GifStream>,
        frame: ImageFrameIndex,
        now: Instant,
    ) -> ImageSequenceResolution {
        // Step and measure stream memory under the stream's own lock BEFORE
        // locking `self.state`, so the cache state lock is never held while
        // waiting on a stream decode.
        let (decoded, memory_size) = stream.frame(frame, self.backward_window_limit());
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            match &decoded {
                StreamFrame::Ready(_) => state.account_stream(
                    sequence,
                    colors,
                    &stream,
                    memory_size,
                    self.max_bytes,
                    now,
                ),
                StreamFrame::Failed => state.discard_stream(sequence, colors, &stream),
                StreamFrame::Missing => {}
            }
        }
        match decoded {
            StreamFrame::Ready(frame) => ImageSequenceResolution::Frame(frame),
            StreamFrame::Failed => ImageSequenceResolution::Failed,
            StreamFrame::Missing => ImageSequenceResolution::MissingFrame,
        }
    }

    /// Resolve one frame of a computed animation (an SVG document sampled
    /// on its grid).
    ///
    /// Mirrors [`Self::resolve`]: a hit is served from the resident entry,
    /// a miss samples under the policy and publishes through the same
    /// budget/retirement path, and concurrent misses may sample redundantly
    /// rather than holding the mutex across decoder work.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_svg(
        &self,
        sequence: ImageSequenceId,
        data: &[u8],
        frame: ImageFrameIndex,
        colors: ImageColorContext,
        resources: &crate::svg::SvgResourceContext,
        policy: ImageAnimationPolicy,
    ) -> ImageSequenceResolution {
        self.resolve_with(
            sequence,
            frame,
            SequenceMaterialization::ComputedSvg {
                colors,
                resources: resources.clone(),
                policy,
            },
            || crate::svg_animation::sample_svg_sequence(data, colors, resources, policy),
        )
    }

    fn resolve_with(
        &self,
        sequence: ImageSequenceId,
        frame: ImageFrameIndex,
        materialization: SequenceMaterialization,
        decode: impl FnOnce() -> Option<Arc<DecodedImageSequence>>,
    ) -> ImageSequenceResolution {
        let now = Instant::now();
        let decode_lease = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(sequence) = state
                .entry_for(sequence, &materialization, now)
                .and_then(|entry| entry.animated_sequence(&materialization))
            {
                return sequence
                    .frame(frame)
                    .map(ImageSequenceResolution::Frame)
                    .unwrap_or(ImageSequenceResolution::MissingFrame);
            }
            state.begin_decode(sequence);
            DecodeLease {
                cache: self,
                sequence,
            }
        };

        let Some(decoded) = decode() else {
            return match materialization {
                // A computed plan that cannot be materialized has no indexed
                // sequence. Its SVG fallback follows GNU's static semantics.
                SequenceMaterialization::ComputedSvg { .. } => ImageSequenceResolution::NotAnimated,
                SequenceMaterialization::AuthoredRaster => undecodable(frame),
            };
        };
        let published = decode_lease.publish(decoded, materialization);
        published
            .frame(frame)
            .map(ImageSequenceResolution::Frame)
            .unwrap_or(ImageSequenceResolution::MissingFrame)
    }

    fn publish_decoded(
        &self,
        sequence: ImageSequenceId,
        decoded: Arc<DecodedImageSequence>,
        materialization: SequenceMaterialization,
    ) -> Arc<DecodedImageSequence> {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.prune_idle_streams(now, IDLE_STREAM_TTL);
        if state.is_retired(sequence) {
            return decoded;
        }
        let key = SequenceCacheKey::Materialized(sequence);
        if let Some(resident) = state
            .entries
            .get(&key)
            .and_then(|entry| entry.animated_sequence(&materialization))
        {
            // A duplicate decode must return the same sequence every subsequent
            // hit will serve, including its frame count and delay metadata.
            return resident;
        }
        // A different materialization cannot satisfy this request.
        state.remove_key(key);
        let memory_size = decoded.memory_size;
        if memory_size > self.max_bytes {
            return decoded;
        }
        while state.total_bytes.saturating_add(memory_size) > self.max_bytes {
            let Some(victim) = state
                .entries
                .iter()
                .filter(|(_, entry)| entry.memory_size() > 0)
                .min_by_key(|(candidate, entry)| {
                    (
                        entry.eviction_priority(now),
                        entry.last_access(),
                        **candidate,
                    )
                })
                .map(|(candidate, _)| *candidate)
            else {
                break;
            };
            state.remove_key(victim);
        }
        let stamp = state.next_access();
        state.total_bytes = state.total_bytes.saturating_add(memory_size);
        state.entries.insert(
            key,
            SequenceCacheEntry::Animated {
                sequence: Arc::clone(&decoded),
                materialization,
                last_access: stamp,
            },
        );
        decoded
    }

    pub(crate) fn retire(&self, retirement: ImageSequenceRetirement) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match retirement {
            ImageSequenceRetirement::One(sequence) => {
                state.remove_sequence(sequence);
                if state.in_flight.contains_key(&sequence) {
                    state.individually_retired.insert(sequence);
                }
            }
            ImageSequenceRetirement::AllocatedThrough(sequence) => {
                let retired_through = state
                    .retired_through
                    .map_or(sequence, |current| current.max(sequence));
                state.retired_through = Some(retired_through);
                state
                    .individually_retired
                    .retain(|id| *id > retired_through);
                let stale = state
                    .entries
                    .keys()
                    .copied()
                    .filter(|key| key.sequence() <= retired_through)
                    .collect::<Vec<_>>();
                for key in stale {
                    state.remove_key(key);
                }
            }
        }
    }

    #[cfg(test)]
    fn prune_idle_streams_at(&self, now: Instant) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .prune_idle_streams(now, IDLE_STREAM_TTL);
    }

    #[cfg(test)]
    fn contains(&self, sequence: ImageSequenceId) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries
            .keys()
            .any(|key| key.sequence() == sequence)
    }

    #[cfg(test)]
    fn stats(&self) -> ImageSequenceCacheStats {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        ImageSequenceCacheStats {
            hits: state.hits,
            misses: state.misses,
        }
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.prune_idle_streams(Instant::now(), IDLE_STREAM_TTL);
        state.total_bytes
    }
}

/// Balances a cache miss independently of decoder success or panic unwinding.
struct DecodeLease<'a> {
    cache: &'a ImageSequenceCache,
    sequence: ImageSequenceId,
}

impl DecodeLease<'_> {
    fn publish(
        self,
        decoded: Arc<DecodedImageSequence>,
        materialization: SequenceMaterialization,
    ) -> Arc<DecodedImageSequence> {
        self.cache
            .publish_decoded(self.sequence, decoded, materialization)
    }
}

impl Drop for DecodeLease<'_> {
    fn drop(&mut self) {
        let mut state = self
            .cache
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.finish_decode(self.sequence);
    }
}

#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
struct ImageSequenceCacheStats {
    hits: u64,
    misses: u64,
}

/// Every frame of an animated WebP or APNG source. A GIF is not decoded
/// here: it streams from [`GifStream`], and its record scan lives beside it.
pub(crate) fn decode_sequence(data: &[u8]) -> Option<Arc<DecodedImageSequence>> {
    let format = image::guess_format(data).ok()?;
    let frames = match format {
        image::ImageFormat::WebP => {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(data)).ok()?;
            if !decoder.has_animation() {
                return None;
            }
            decoder.into_frames()
        }
        image::ImageFormat::Png => {
            let decoder = image::codecs::png::PngDecoder::new(Cursor::new(data)).ok()?;
            if !decoder.is_apng().ok()? {
                return None;
            }
            decoder.apng().ok()?.into_frames()
        }
        _ => return None,
    };

    let mut decoded = Vec::new();
    let mut memory_size = 0_usize;
    for frame in frames {
        let frame = frame.ok()?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let rgba = frame.into_buffer();
        let (width, height) = rgba.dimensions();
        let bytes: Arc<[u8]> = rgba.into_raw().into();
        memory_size = memory_size.checked_add(bytes.len())?;
        decoded.push(DecodedSequenceFrame {
            width,
            height,
            rgba: bytes,
            delay: ImageFrameDelay::milliseconds(numerator, denominator)?,
            embedded: None,
        });
    }
    if format == image::ImageFormat::WebP {
        let count = u32::try_from(decoded.len()).ok()?;
        for frame in &mut decoded {
            frame.embedded = Some(ImageEmbeddedMetadata::webp(count, frame.delay));
        }
    }
    (!decoded.is_empty()).then(|| {
        Arc::new(DecodedImageSequence {
            frames: decoded,
            memory_size,
            loop_start: None,
        })
    })
}

#[cfg(test)]
#[path = "image_sequence/tests/image_sequence_test.rs"]
mod tests;

/// What a source that does not decode means for a frame. Frame zero is not
/// an animation at all, so its caller takes GNU's still-image path; a later
/// `:index` of it does not exist.
fn undecodable(frame: ImageFrameIndex) -> ImageSequenceResolution {
    if frame.is_first() {
        ImageSequenceResolution::NotAnimated
    } else {
        ImageSequenceResolution::MissingFrame
    }
}

mod gif_metadata;

mod gif_stream;
use gif_stream::{GifBackgroundColors, GifStream, StreamFrame};

mod tiff_pages;

pub(crate) fn probe_tiff_frame(
    data: &[u8],
    frame: ImageFrameIndex,
) -> Option<neomacs_display_protocol::ImageNativeExtent> {
    tiff_pages::probe(data, frame)
}
