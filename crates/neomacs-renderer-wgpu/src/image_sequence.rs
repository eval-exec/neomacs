use image::AnimationDecoder;
use neomacs_display_protocol::{
    ImageAnimationPolicy, ImageColorContext, ImageEmbeddedMetadata, ImageFrameDelay,
    ImageFrameIndex, ImageSequenceId, ImageSequenceRetirement,
};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

const DEFAULT_SEQUENCE_CACHE_BYTES: usize = 64 * 1024 * 1024;

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
        let mut embedded = if self.frames.len() > 1 {
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

    fn matches(&self, materialization: &SequenceMaterialization) -> bool {
        match self {
            Self::Animated {
                materialization: resident,
                ..
            } => resident == materialization,
        }
    }

    fn sequence(&self) -> Arc<DecodedImageSequence> {
        match self {
            Self::Animated { sequence, .. } => Arc::clone(sequence),
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

    /// A resident entry with exactly the inputs requested by this worker.
    fn entry_for(
        &mut self,
        sequence: ImageSequenceId,
        materialization: &SequenceMaterialization,
    ) -> Option<&mut SequenceCacheEntry> {
        let stamp = self.next_access();
        let hit = self
            .entries
            .get_mut(&sequence)
            .filter(|entry| entry.matches(materialization));
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
        self.resolve_with(
            sequence,
            frame,
            SequenceMaterialization::AuthoredRaster,
            || decode_sequence(data),
        )
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
        let decode_lease = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(entry) = state.entry_for(sequence, &materialization) {
                return entry.resolve(frame);
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
                SequenceMaterialization::AuthoredRaster if !frame.is_first() => {
                    ImageSequenceResolution::MissingFrame
                }
                SequenceMaterialization::AuthoredRaster => ImageSequenceResolution::NotAnimated,
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
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.is_retired(sequence) {
            return decoded;
        }
        if let Some(entry) = state
            .entries
            .get(&sequence)
            .filter(|entry| entry.matches(&materialization))
        {
            // A duplicate decode must return the same sequence every subsequent
            // hit will serve, including its frame count and delay metadata.
            return entry.sequence();
        }
        // A different materialization cannot satisfy this request.
        state.remove(sequence);
        let memory_size = decoded.memory_size;
        if memory_size > self.max_bytes {
            return decoded;
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
            loop_start: None,
        })
    })
}

#[cfg(test)]
#[path = "image_sequence/tests/image_sequence_test.rs"]
mod tests;
