//! GNU's `struct anim_cache` (src/image.c:3717-3749) for one GIF source:
//! retained source bytes, decoder positions, and composed frames, so each
//! `:index` step composes one frame instead of decoding the file again.
//!
//! GNU reads the file once (`DGifSlurp`, src/image.c:9992) and composes
//! sub-images forward onto one retained pixmap per spec. Here a source retains
//! its encoded bytes once, keeps a small pool of forward cursors so
//! out-of-phase specs on the same GIF each step in O(1), and keeps a bounded
//! backward-seek window so reverse playback (`:speed` < 0) avoids restarting
//! from frame 0 on every step.
//!
//! Each frame is composed exactly as `image`'s own GIF iterator composes it
//! (`GifFrameIterator::next`), because the whole-sequence decode this
//! replaces used that iterator. A row pins the two byte for byte.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Mutex;
#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use gif::{ColorOutput, DecodeOptions, DisposalMethod};
use neomacs_display_protocol::image::ImageExtensionBlock;
use neomacs_display_protocol::{
    EncodedBytes, ImageEmbeddedMetadata, ImageFrameDelay, ImageFrameIndex,
};

use super::{DecodedSequenceFrame, ImageSequenceFrame};

/// Independent forward decoder positions kept per GIF source so multiple specs
/// animating the same GIF out of phase each step in O(1) without restarting.
const MAX_CURSORS: usize = 4;

pub(super) enum StreamFrame {
    Ready(ImageSequenceFrame),
    /// No frame decoded: what the whole-sequence decode's failure meant for
    /// every frame of the source.
    Failed,
    /// No image descriptor carries this index.
    Missing,
}

pub(super) struct GifStream {
    /// The extension blocks before each image descriptor, so the frame count
    /// and each frame's `image-metadata` blocks are known before any pixel
    /// is decoded.
    extensions: Vec<Vec<(u8, Vec<u8>)>>,
    source: EncodedBytes,
    /// Fixed retained memory: encoded GIF bytes (GNU's `anim_cache.byte_size`,
    /// src/image.c:9946/9963) plus the parsed extension payload table.
    fixed_bytes: usize,
    /// Canvas dimensions from the logical screen descriptor.
    width: u32,
    height: u32,
    /// Per-source decoder state, stepped under one mutex so concurrent
    /// requests never advance a cursor twice.
    state: Mutex<StreamState>,
    #[cfg(test)]
    decoded_frames: Option<Arc<AtomicU64>>,
}

struct StreamState {
    cursors: Vec<CursorSlot>,
    /// Bounded cache of frames populated on a reverse step (`index + 1 ==
    /// last_returned`), so negative `:speed` playback serves consecutive
    /// reverse frames in O(1) instead of reopening from frame 0 each tick.
    backward_window: HashMap<u32, DecodedSequenceFrame>,
    backward_window_bytes: usize,
    last_returned: Option<u32>,
    access_clock: u64,
}

struct CursorSlot {
    /// The decoder, positioned before frame `next`. `None` after a failed
    /// or interrupted step, so the next step reopens the source instead of
    /// resuming a decoder an unwinding panic may have left half-advanced.
    reader: Option<FrameReader>,
    next: u32,
    /// The frame at `next - 1` as last returned. GNU keeps the composed
    /// frame too (its `pixmap`): `image-animate-timeout` asks for the
    /// metadata of the frame still displayed before each step
    /// (lisp/image.el:1052-1057), and that frame is behind the position.
    current: Option<(u32, DecodedSequenceFrame)>,
    last_access: u64,
}

impl CursorSlot {
    fn new(reader: FrameReader, last_access: u64) -> Self {
        Self {
            reader: Some(reader),
            next: 0,
            current: None,
            last_access,
        }
    }

    fn memory_size(&self) -> usize {
        self.reader
            .as_ref()
            .map_or(0, |reader| reader.canvas_bytes())
            + self
                .current
                .as_ref()
                .map_or(0, |(_, frame)| frame.memory_size())
    }
}

impl StreamState {
    fn next_stamp(&mut self) -> u64 {
        self.access_clock = self.access_clock.saturating_add(1);
        self.access_clock
    }

    fn clear_backward_window(&mut self) {
        self.backward_window.clear();
        self.backward_window_bytes = 0;
    }

    fn dynamic_memory_size(&self) -> usize {
        self.cursors
            .iter()
            .map(CursorSlot::memory_size)
            .sum::<usize>()
            .saturating_add(self.backward_window_bytes)
    }
}

const fn assert_send<T: Send>() {}
const _: () = assert_send::<GifStream>();

impl GifStream {
    /// Read the record structure and the header, as GNU's `DGifSlurp` and
    /// `gif_load` do before the first frame. No frame is decoded.
    pub(super) fn open(source: EncodedBytes) -> Option<Self> {
        let extensions = super::gif_metadata::frame_extensions(&source)?;
        if extensions.is_empty() {
            return None;
        }
        let extension_bytes: usize = extensions
            .iter()
            .flatten()
            .map(|(_, payload)| payload.len())
            .sum();
        let fixed_bytes = source.len().saturating_add(extension_bytes);
        let reader = FrameReader::open(&source)?;
        let (width, height) = (reader.width, reader.height);
        Some(Self {
            extensions,
            source,
            fixed_bytes,
            width,
            height,
            state: Mutex::new(StreamState {
                cursors: vec![CursorSlot::new(reader, 1)],
                backward_window: HashMap::new(),
                backward_window_bytes: 0,
                last_returned: None,
                access_clock: 1,
            }),
            #[cfg(test)]
            decoded_frames: None,
        })
    }

    #[cfg(test)]
    pub(super) fn attach_counter(&mut self, counter: Arc<AtomicU64>) {
        self.decoded_frames = Some(counter);
    }

    /// Number of image descriptors discovered in the GIF record structure.
    pub(super) fn frame_count(&self) -> usize {
        self.extensions.len()
    }

    /// The frame at `index`, decoded from a retained position, paired with the
    /// stream's post-step memory footprint (computed under the same lock so
    /// the cache never nests its own state lock over this stream lock).
    pub(super) fn frame(
        &self,
        index: ImageFrameIndex,
        backward_window_limit: usize,
    ) -> (StreamFrame, usize) {
        let Ok(index) = u32::try_from(index.get()) else {
            return (StreamFrame::Missing, self.memory_size());
        };
        if index as usize >= self.extensions.len() {
            return (StreamFrame::Missing, self.memory_size());
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let stamp = state.next_stamp();

        // 1. Instant hit on any cursor's last returned frame (0 decodes).
        if let Some(slot) = state
            .cursors
            .iter_mut()
            .find(|slot| slot.current.as_ref().is_some_and(|(cur, _)| *cur == index))
        {
            slot.last_access = stamp;
            let frame = slot
                .current
                .as_ref()
                .expect("matched current frame")
                .1
                .sequence_frame();
            state.last_returned = Some(index);
            let total_bytes = self.fixed_bytes.saturating_add(state.dynamic_memory_size());
            return (StreamFrame::Ready(frame), total_bytes);
        }

        // 2. Instant hit on the bounded backward-seek window (0 decodes).
        if let Some(frame) = state.backward_window.get(&index).cloned() {
            state.last_returned = Some(index);
            let total_bytes = self.fixed_bytes.saturating_add(state.dynamic_memory_size());
            return (StreamFrame::Ready(frame.sequence_frame()), total_bytes);
        }

        // 3. Closest forward cursor (`reader.is_some()` and `next <= index`).
        if let Some(slot_idx) = state
            .cursors
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.reader.is_some() && slot.next <= index)
            .min_by_key(|(_, slot)| index - slot.next)
            .map(|(i, _)| i)
        {
            state.clear_backward_window();
            let result = self.advance_slot(&mut state.cursors[slot_idx], index, stamp, None);
            if matches!(result, StreamFrame::Ready(_)) {
                state.last_returned = Some(index);
            }
            let total_bytes = self.fixed_bytes.saturating_add(state.dynamic_memory_size());
            return (result, total_bytes);
        }

        // 4. Request is behind every live cursor: classify reverse step vs
        //    loop wraparound vs independent out-of-phase reader.
        let is_reverse_step = index.checked_add(1).is_some_and(|next_idx| {
            state.last_returned == Some(next_idx)
                || state.cursors.iter().any(|slot| {
                    slot.current
                        .as_ref()
                        .is_some_and(|(cur, _)| *cur == next_idx)
                })
        });

        let Some(fresh_reader) = FrameReader::open(&self.source) else {
            let total_bytes = self.fixed_bytes.saturating_add(state.dynamic_memory_size());
            return (StreamFrame::Failed, total_bytes);
        };

        let count = u32::try_from(self.extensions.len()).unwrap_or(u32::MAX);
        let slot_idx = if is_reverse_step {
            // Rewind the cursor that just showed `index + 1` (or the LRU slot)
            // in place so reverse playback does not consume extra cursor slots.
            state
                .cursors
                .iter()
                .position(|slot| {
                    slot.current
                        .as_ref()
                        .is_some_and(|(cur, _)| Some(*cur) == index.checked_add(1))
                })
                .or_else(|| {
                    state
                        .cursors
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, slot)| slot.last_access)
                        .map(|(i, _)| i)
                })
                .unwrap_or(0)
        } else if index == 0
            && let Some(finished_idx) = state
                .cursors
                .iter()
                .position(|slot| slot.reader.is_none() || slot.next >= count)
        {
            // Normal loop wraparound (`count - 1 -> 0`): reuse the finished
            // cursor in place so single-reader loops keep one cursor.
            finished_idx
        } else if let Some(dead_idx) = state.cursors.iter().position(|slot| slot.reader.is_none()) {
            dead_idx
        } else if state.cursors.len() < MAX_CURSORS {
            // Out-of-phase reader: keep existing cursors intact so every
            // reader steps forward at O(1).
            state.cursors.push(CursorSlot::new(fresh_reader, stamp));
            let new_idx = state.cursors.len() - 1;
            let result = self.advance_slot(&mut state.cursors[new_idx], index, stamp, None);
            if matches!(result, StreamFrame::Ready(_)) {
                state.last_returned = Some(index);
            }
            let total_bytes = self.fixed_bytes.saturating_add(state.dynamic_memory_size());
            return (result, total_bytes);
        } else {
            state
                .cursors
                .iter()
                .enumerate()
                .min_by_key(|(_, slot)| slot.last_access)
                .map(|(i, _)| i)
                .unwrap_or(0)
        };

        state.cursors[slot_idx] = CursorSlot::new(fresh_reader, stamp);
        state.clear_backward_window();

        let window_start = if is_reverse_step && index > 0 && backward_window_limit > 0 {
            let approx_frame_bytes = usize::try_from(self.width)
                .ok()
                .and_then(|w| {
                    usize::try_from(self.height)
                        .ok()
                        .and_then(|h| w.checked_mul(h))
                })
                .and_then(|px| px.checked_mul(4))
                .unwrap_or(usize::MAX)
                .max(1);
            let max_frames = (backward_window_limit / approx_frame_bytes).min(index as usize);
            if max_frames > 0 {
                Some(index.saturating_sub(max_frames as u32))
            } else {
                None
            }
        } else {
            None
        };

        let mut captured = window_start.map(|start| (start, backward_window_limit, Vec::new()));
        let result = self.advance_slot(
            &mut state.cursors[slot_idx],
            index,
            stamp,
            captured.as_mut(),
        );
        if matches!(result, StreamFrame::Ready(_)) {
            state.last_returned = Some(index);
            if let Some((_, _, frames)) = captured {
                for (frame_idx, frame) in frames {
                    let frame_bytes = frame.memory_size();
                    if state.backward_window_bytes.saturating_add(frame_bytes)
                        <= backward_window_limit
                    {
                        state.backward_window_bytes =
                            state.backward_window_bytes.saturating_add(frame_bytes);
                        state.backward_window.insert(frame_idx, frame);
                    }
                }
            }
        }
        let total_bytes = self.fixed_bytes.saturating_add(state.dynamic_memory_size());
        (result, total_bytes)
    }

    fn advance_slot(
        &self,
        slot: &mut CursorSlot,
        index: u32,
        stamp: u64,
        mut capture_window: Option<&mut (u32, usize, Vec<(u32, DecodedSequenceFrame)>)>,
    ) -> StreamFrame {
        slot.last_access = stamp;
        while slot.next <= index {
            // Owned for the step, so a step that unwinds takes the decoder
            // with it and the next step reopens the source.
            let Some(mut frame_reader) = slot.reader.take() else {
                return StreamFrame::Failed;
            };
            let decoded = frame_reader.read_frame(
                slot.next,
                &self.extensions,
                #[cfg(test)]
                self.decoded_frames.as_deref(),
            );
            match decoded {
                Ok(Some(frame)) => {
                    if let Some((start, _, collected)) = capture_window.as_deref_mut()
                        && slot.next >= *start
                        && slot.next < index
                    {
                        collected.push((slot.next, frame.clone()));
                    }
                    slot.current = Some((slot.next, frame));
                    slot.next += 1;
                    slot.reader = Some(frame_reader);
                }
                Ok(None) | Err(()) => return StreamFrame::Failed,
            }
        }
        match &slot.current {
            Some((current, frame)) if *current == index => {
                StreamFrame::Ready(frame.sequence_frame())
            }
            _ => StreamFrame::Failed,
        }
    }

    /// What the stream retains: encoded source bytes, parsed extensions table,
    /// active decoder canvases + current frames, and any backward-seek window.
    pub(super) fn memory_size(&self) -> usize {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.fixed_bytes.saturating_add(state.dynamic_memory_size())
    }
}

/// The decoder plus the canvas `image`'s iterator composites onto.
struct FrameReader {
    decoder: gif::Decoder<Cursor<EncodedBytes>>,
    width: u32,
    height: u32,
    /// The canvas as the disposal method of the last frame left it
    /// (`non_disposed_frame` in `image`'s `GifFrameIterator`).
    canvas: Option<image::RgbaImage>,
}

/// A decode failure, as `image`'s iterator reports it: every error there
/// ends the sequence, so the reason needs no distinction here.
type FrameResult<T> = Result<T, ()>;

impl FrameReader {
    /// What `image::codecs::gif::GifDecoder::new` reads: the header, with
    /// RGBA output.
    fn open(source: &EncodedBytes) -> Option<Self> {
        let mut options = DecodeOptions::new();
        options.set_color_output(ColorOutput::RGBA);
        let decoder = options.read_info(Cursor::new(source.clone())).ok()?;
        let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
        Some(Self {
            decoder,
            width,
            height,
            canvas: None,
        })
    }

    fn canvas_bytes(&self) -> usize {
        self.canvas
            .as_ref()
            .map_or(0, |canvas| canvas.as_raw().len())
    }

    /// Decode frame `index` and composite it onto the canvas. `Ok(None)` is
    /// the end of the stream.
    fn read_frame(
        &mut self,
        index: u32,
        extensions: &[Vec<(u8, Vec<u8>)>],
        #[cfg(test)] decoded_frames: Option<&AtomicU64>,
    ) -> FrameResult<Option<DecodedSequenceFrame>> {
        let (left, top, width, height, dispose, delay) = match self.decoder.next_frame_info() {
            Ok(Some(frame)) => (
                u32::from(frame.left),
                u32::from(frame.top),
                u32::from(frame.width),
                u32::from(frame.height),
                frame.dispose,
                u32::from(frame.delay),
            ),
            Ok(None) => return Ok(None),
            Err(_) => return Err(()),
        };
        // `image` ignores the background color "for web compatibility".
        let non_disposed_frame = self.canvas.get_or_insert_with(|| {
            image::RgbaImage::from_pixel(self.width, self.height, image::Rgba([0, 0, 0, 0]))
        });
        let mut raw = vec![0; self.decoder.buffer_size()];
        if self.decoder.read_into_buffer(&mut raw).is_err() {
            return Err(());
        }
        let Some(mut frame_buffer) = image::ImageBuffer::from_raw(width, height, raw) else {
            return Err(());
        };

        // Blend the frame with the canvas, then update the canvas according
        // to the disposal method.
        fn blend_and_dispose_pixel(
            dispose: DisposalMethod,
            previous: &mut image::Rgba<u8>,
            current: &mut image::Rgba<u8>,
        ) {
            let pixel_alpha = current.0[3];
            if pixel_alpha == 0 {
                *current = *previous;
            }

            match dispose {
                DisposalMethod::Any | DisposalMethod::Keep => {
                    // do not dispose (keep pixels from this frame); `Any` is
                    // underspecified in the GIF spec, but most viewers treat
                    // it identically to `Keep`
                    *previous = *current;
                }
                DisposalMethod::Background => {
                    // restore to background color (background shows through
                    // transparent pixels in the next frame)
                    *previous = image::Rgba([0, 0, 0, 0]);
                }
                DisposalMethod::Previous => {
                    // restore to previous (dispose frames leaving the last
                    // none disposal frame)
                }
            }
        }

        // A frame that exactly covers the canvas is used directly; any other
        // frame gets a canvas-sized buffer.
        let image_buffer =
            if (left, top) == (0, 0) && (self.width, self.height) == frame_buffer.dimensions() {
                for (x, y, pixel) in frame_buffer.enumerate_pixels_mut() {
                    let previous_pixel = non_disposed_frame.get_pixel_mut(x, y);
                    blend_and_dispose_pixel(dispose, previous_pixel, pixel);
                }
                frame_buffer
            } else {
                image::ImageBuffer::from_fn(self.width, self.height, |x, y| {
                    let frame_x = x.wrapping_sub(left);
                    let frame_y = y.wrapping_sub(top);
                    let previous_pixel = non_disposed_frame.get_pixel_mut(x, y);

                    if frame_x < frame_buffer.width() && frame_y < frame_buffer.height() {
                        let mut pixel = *frame_buffer.get_pixel(frame_x, frame_y);
                        blend_and_dispose_pixel(dispose, previous_pixel, &mut pixel);
                        pixel
                    } else {
                        // out of bounds: the canvas pixel stays as it was
                        *previous_pixel
                    }
                })
            };

        // GNU gif_load (src/image.c:10273-10318) publishes the extension
        // blocks of the selected sub-image, its count, and its own delay.
        let count = u32::try_from(extensions.len()).map_err(|_| ())?;
        let blocks = extensions
            .get(index as usize)
            .ok_or(())?
            .iter()
            .map(|(function, bytes)| ImageExtensionBlock {
                function: *function,
                bytes: bytes.clone(),
            })
            .collect();
        // `image` carries a GIF delay of N centiseconds as N*10 ms.
        let delay = ImageFrameDelay::milliseconds(delay * 10, 1).ok_or(())?;
        #[cfg(test)]
        if let Some(counter) = decoded_frames {
            let _ = counter.fetch_add(1, Ordering::Relaxed);
        }
        Ok(Some(DecodedSequenceFrame {
            width: self.width,
            height: self.height,
            rgba: image_buffer.into_raw().into(),
            delay,
            embedded: Some(ImageEmbeddedMetadata::gif(count, blocks)),
        }))
    }
}
