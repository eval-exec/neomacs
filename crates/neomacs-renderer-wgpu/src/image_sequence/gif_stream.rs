//! GNU's `struct anim_cache` (src/image.c:3717-3749) for one GIF source: the
//! decoder position and the last composed frame, so the next `:index` step
//! composes one frame instead of decoding the file again.
//!
//! GNU reads the file once (`DGifSlurp`, src/image.c:9992) and then composes
//! sub-images forward onto one retained pixmap, restarting when `:index` does
//! not follow the cached frame (src/image.c:9888-9897, 10138-10144). Its
//! position is a count into every frame's decoded raster, which a bounded
//! stream cannot keep: here the position is the decoder itself, which decodes
//! a frame only when that frame is first requested. A request ahead of the
//! position decodes forward, as `webp_load` does (src/image.c:10683-10685);
//! a request behind it reopens the source.
//!
//! Each frame is composed exactly as `image`'s own GIF iterator composes it
//! (`GifFrameIterator::next`), because the whole-sequence decode this
//! replaces used that iterator. A row pins the two byte for byte.

use std::io::Cursor;
use std::sync::Mutex;

use gif::{ColorOutput, DecodeOptions, DisposalMethod};
use neomacs_display_protocol::image::ImageExtensionBlock;
use neomacs_display_protocol::{
    EncodedBytes, ImageEmbeddedMetadata, ImageFrameDelay, ImageFrameIndex,
};

#[cfg(test)]
use super::DECODED_FRAMES;
use super::{DecodedSequenceFrame, ImageSequenceFrame};

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
    /// GNU's `cache->index` and `anim_handle`: one position, stepped by one
    /// thread at a time so two requests never advance it twice.
    position: Mutex<Position>,
}

struct Position {
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
        let reader = FrameReader::open(&source)?;
        Some(Self {
            extensions,
            source,
            position: Mutex::new(Position {
                reader: Some(reader),
                next: 0,
                current: None,
            }),
        })
    }

    /// The frame at `index`, decoded from the retained position.
    pub(super) fn frame(&self, index: ImageFrameIndex) -> StreamFrame {
        let Ok(index) = u32::try_from(index.get()) else {
            return StreamFrame::Missing;
        };
        if index as usize >= self.extensions.len() {
            return StreamFrame::Missing;
        }
        let mut position = self
            .position
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((current, frame)) = &position.current
            && *current == index
        {
            return StreamFrame::Ready(frame.sequence_frame());
        }
        let reopen = match position.reader {
            Some(_) => index < position.next,
            None => true,
        };
        if reopen {
            let Some(reader) = FrameReader::open(&self.source) else {
                return StreamFrame::Failed;
            };
            position.reader = Some(reader);
            position.next = 0;
            position.current = None;
        }
        let Position {
            reader,
            next,
            current,
        } = &mut *position;
        while *next <= index {
            // Owned for the step, so a step that unwinds takes the decoder
            // with it and the next step reopens the source.
            let Some(mut frame_reader) = reader.take() else {
                return StreamFrame::Failed;
            };
            match frame_reader.read_frame(*next, &self.extensions) {
                Ok(Some(frame)) => {
                    *current = Some((*next, frame));
                    *next += 1;
                    *reader = Some(frame_reader);
                }
                Ok(None) | Err(()) => return StreamFrame::Failed,
            }
        }
        match current {
            Some((current, frame)) if *current == index => {
                StreamFrame::Ready(frame.sequence_frame())
            }
            _ => StreamFrame::Failed,
        }
    }

    /// What the stream retains: the canvas the next frame composites onto,
    /// the returned frame, and that frame's extension payload.
    pub(super) fn memory_size(&self) -> usize {
        let position = self
            .position
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        position
            .reader
            .as_ref()
            .map_or(0, |reader| reader.canvas_bytes())
            + position
                .current
                .as_ref()
                .map_or(0, |(_, frame)| frame.memory_size())
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
        let _ = DECODED_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Some(DecodedSequenceFrame {
            width: self.width,
            height: self.height,
            rgba: image_buffer.into_raw().into(),
            delay,
            embedded: Some(ImageEmbeddedMetadata::gif(count, blocks)),
        }))
    }
}
