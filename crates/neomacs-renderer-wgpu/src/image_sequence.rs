use image::AnimationDecoder;
use neomacs_display_protocol::{
    ImageAnimationPolicy, ImageColorContext, ImageEmbeddedMetadata, ImageFrameDelay,
    ImageFrameIndex, ImageRealization, ImageRotation, ImageSequenceId, ImageSequenceRetirement,
    ImageSizeSpec,
};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

const DEFAULT_SEQUENCE_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct DecodedImageSequence {
    frames: Vec<DecodedSequenceFrame>,
    memory_size: usize,
}

#[derive(Clone)]
struct DecodedSequenceFrame {
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
    delay: ImageFrameDelay,
}

impl DecodedImageSequence {
    /// Assemble a sequence from already-decoded frames.
    ///
    /// Computed animation (the SVG sampler) produces frames the same shape
    /// the raster decoders do; this is the one door those producers use
    /// into the cache, so budget accounting stays centralized.
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
                }
            })
            .collect();
        Arc::new(Self {
            frames,
            memory_size,
        })
    }

    fn frame(&self, index: ImageFrameIndex) -> Option<ImageSequenceFrame> {
        let index = usize::try_from(index.get()).ok()?;
        let frame = self.frames.get(index)?;
        let embedded = if self.frames.len() > 1 {
            ImageEmbeddedMetadata::animation(u32::try_from(self.frames.len()).ok()?, frame.delay)
        } else {
            ImageEmbeddedMetadata::EMPTY
        };
        Some(ImageSequenceFrame {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba.to_vec(),
            embedded,
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
}

impl ImageSequenceResolution {
    #[cfg(test)]
    fn expect_frame(self, message: &str) -> ImageSequenceFrame {
        match self {
            Self::Frame(frame) => frame,
            Self::NotAnimated | Self::MissingFrame => panic!("{message}"),
        }
    }
}

/// Which decode path produced an entry.
///
/// Sequence identity follows the resolve source, but the two producers
/// answer to different gates: an authored raster sequence (GIF, WebP,
/// APNG) is always live, while a computed SVG sequence exists only under
/// an enabled `:animation` policy. One id can therefore name either kind
/// across the lifetime of a source, and a hit is only a hit for the path
/// asking for its own kind — the raster resolve must never serve frames
/// a policy-off request never asked to materialize.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SequenceKind {
    AuthoredRaster,
    ComputedSvg,
}

enum SequenceCacheEntry {
    Animated {
        sequence: Arc<DecodedImageSequence>,
        kind: SequenceKind,
        /// The face colors computed frames were baked with
        /// (`currentColor`, the background rect). Authored raster frames
        /// ignore it; for computed frames a different color context is a
        /// different materialization and must not be served.
        colors: ImageColorContext,
        last_access: u64,
    },
}

impl SequenceCacheEntry {
    const fn last_access(&self) -> u64 {
        match self {
            Self::Animated { last_access, .. } => *last_access,
        }
    }

    fn memory_size(&self) -> usize {
        match self {
            Self::Animated { sequence, .. } => sequence.memory_size,
        }
    }

    fn kind(&self) -> SequenceKind {
        match self {
            Self::Animated { kind, .. } => *kind,
        }
    }

    fn matches(&self, kind: SequenceKind, colors: ImageColorContext) -> bool {
        match self {
            Self::Animated {
                kind: entry_kind,
                colors: entry_colors,
                ..
            } => {
                *entry_kind == kind
                    && (kind == SequenceKind::AuthoredRaster || *entry_colors == colors)
            }
        }
    }

    fn touch(&mut self, stamp: u64) {
        match self {
            Self::Animated { last_access, .. } => {
                *last_access = stamp;
            }
        }
    }

    fn resolve(&self, frame: ImageFrameIndex) -> ImageSequenceResolution {
        match self {
            Self::Animated { sequence, .. } => sequence
                .frame(frame)
                .map(ImageSequenceResolution::Frame)
                .unwrap_or(ImageSequenceResolution::MissingFrame),
        }
    }
}

#[derive(Default)]
struct ImageSequenceCacheState {
    entries: HashMap<ImageSequenceId, SequenceCacheEntry>,
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

    /// The resident entry for `sequence` when it was produced by `kind`.
    ///
    /// An entry of the other kind is not a hit: the caller proceeds down
    /// its own miss path, and publication below replaces the mismatched
    /// entry rather than being fenced out by first-wins.
    fn entry_of_kind(
        &mut self,
        sequence: ImageSequenceId,
        kind: SequenceKind,
        colors: ImageColorContext,
    ) -> Option<&mut SequenceCacheEntry> {
        let stamp = self.next_access();
        let hit = self
            .entries
            .get_mut(&sequence)
            .filter(|entry| entry.matches(kind, colors));
        match hit {
            Some(entry) => {
                self.hits = self.hits.saturating_add(1);
                entry.touch(stamp);
                Some(entry)
            }
            None => {
                self.misses = self.misses.saturating_add(1);
                None
            }
        }
    }

    fn remove(&mut self, sequence: ImageSequenceId) {
        if let Some(entry) = self.entries.remove(&sequence) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.memory_size());
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
pub(crate) struct ImageSequenceCache {
    max_bytes: usize,
    state: Mutex<ImageSequenceCacheState>,
}

impl ImageSequenceCache {
    pub(crate) fn new() -> Self {
        Self::with_max_bytes(DEFAULT_SEQUENCE_CACHE_BYTES)
    }

    fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            state: Mutex::new(ImageSequenceCacheState::default()),
        }
    }

    pub(crate) fn resolve(
        &self,
        sequence: ImageSequenceId,
        data: &[u8],
        frame: ImageFrameIndex,
    ) -> ImageSequenceResolution {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(entry) = state.entry_of_kind(
                sequence,
                SequenceKind::AuthoredRaster,
                ImageColorContext::default(),
            ) {
                return entry.resolve(frame);
            }
            state.begin_decode(sequence);
        }

        let Some(decoded) = decode_sequence(data) else {
            self.finish_decode(sequence);
            return if frame.is_first() {
                ImageSequenceResolution::NotAnimated
            } else {
                ImageSequenceResolution::MissingFrame
            };
        };
        let result = decoded
            .frame(frame)
            .map(ImageSequenceResolution::Frame)
            .unwrap_or(ImageSequenceResolution::MissingFrame);
        self.publish_decoded(
            sequence,
            decoded,
            SequenceKind::AuthoredRaster,
            ImageColorContext::default(),
        );
        result
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
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(entry) = state.entry_of_kind(sequence, SequenceKind::ComputedSvg, colors) {
                return entry.resolve(frame);
            }
            state.begin_decode(sequence);
        }

        let Some(decoded) =
            crate::svg_animation::sample_svg_sequence(data, colors, resources, policy)
        else {
            self.finish_decode(sequence);
            return if frame.is_first() {
                ImageSequenceResolution::NotAnimated
            } else {
                ImageSequenceResolution::MissingFrame
            };
        };
        let result = decoded
            .frame(frame)
            .map(ImageSequenceResolution::Frame)
            .unwrap_or(ImageSequenceResolution::MissingFrame);
        self.publish_decoded(sequence, decoded, SequenceKind::ComputedSvg, colors);
        result
    }

    fn finish_decode(&self, sequence: ImageSequenceId) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.finish_decode(sequence);
    }

    fn publish_decoded(
        &self,
        sequence: ImageSequenceId,
        decoded: Arc<DecodedImageSequence>,
        kind: SequenceKind,
        colors: ImageColorContext,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.is_retired(sequence) {
            state.finish_decode(sequence);
            return;
        }
        if state
            .entries
            .get(&sequence)
            .is_some_and(|entry| entry.matches(kind, colors))
        {
            // First publication of a kind wins: a concurrent duplicate
            // decode of the same source coalesces into the resident entry.
            state.finish_decode(sequence);
            return;
        }
        // An entry the asking path would not serve (other kind, or computed
        // frames baked for other colors) is stale for it — the source
        // changed which materialization owns it — so it is replaced.
        state.remove(sequence);
        let memory_size = decoded.memory_size;
        if memory_size > self.max_bytes {
            state.finish_decode(sequence);
            return;
        }
        while state.total_bytes.saturating_add(memory_size) > self.max_bytes {
            let Some(victim) = state
                .entries
                .iter()
                .filter(|(_, entry)| entry.memory_size() > 0)
                .min_by_key(|(id, entry)| (entry.last_access(), **id))
                .map(|(id, _)| *id)
            else {
                break;
            };
            state.remove(victim);
        }
        let stamp = state.next_access();
        state.total_bytes = state.total_bytes.saturating_add(memory_size);
        state.entries.insert(
            sequence,
            SequenceCacheEntry::Animated {
                sequence: decoded,
                kind,
                colors,
                last_access: stamp,
            },
        );
        state.finish_decode(sequence);
    }

    pub(crate) fn retire(&self, retirement: ImageSequenceRetirement) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match retirement {
            ImageSequenceRetirement::One(sequence) => {
                state.remove(sequence);
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
                    .filter(|id| *id <= retired_through)
                    .collect::<Vec<_>>();
                for id in stale {
                    state.remove(id);
                }
            }
        }
    }

    #[cfg(test)]
    fn contains(&self, sequence: ImageSequenceId) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries
            .contains_key(&sequence)
    }

    #[cfg(test)]
    fn mark_in_flight(&self, sequence: ImageSequenceId) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .begin_decode(sequence);
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
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .total_bytes
    }
}

#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
struct ImageSequenceCacheStats {
    hits: u64,
    misses: u64,
}

pub(crate) fn decode_sequence(data: &[u8]) -> Option<Arc<DecodedImageSequence>> {
    let frames = match image::guess_format(data).ok()? {
        image::ImageFormat::Gif => {
            let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(data)).ok()?;
            decoder.into_frames()
        }
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
        });
    }
    (!decoded.is_empty()).then(|| {
        Arc::new(DecodedImageSequence {
            frames: decoded,
            memory_size,
        })
    })
}

#[cfg(test)]
#[path = "image_sequence/tests/image_sequence_test.rs"]
mod tests;
