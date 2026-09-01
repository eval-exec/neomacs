use image::AnimationDecoder;
use neomacs_display_protocol::{
    ImageEmbeddedMetadata, ImageFrameDelay, ImageFrameIndex, ImageSequenceId,
    ImageSequenceRetirement,
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

enum SequenceCacheEntry {
    Animated {
        sequence: Arc<DecodedImageSequence>,
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
            let stamp = state.next_access();
            if state.entries.contains_key(&sequence) {
                state.hits = state.hits.saturating_add(1);
                let entry = state
                    .entries
                    .get_mut(&sequence)
                    .expect("entry was observed above");
                entry.touch(stamp);
                return entry.resolve(frame);
            }
            state.misses = state.misses.saturating_add(1);
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
        self.publish_decoded(sequence, decoded);
        result
    }

    fn finish_decode(&self, sequence: ImageSequenceId) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.finish_decode(sequence);
    }

    fn publish_decoded(&self, sequence: ImageSequenceId, decoded: Arc<DecodedImageSequence>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.is_retired(sequence) || state.entries.contains_key(&sequence) {
            state.finish_decode(sequence);
            return;
        }
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
#[path = "image_sequence_test.rs"]
mod tests;
