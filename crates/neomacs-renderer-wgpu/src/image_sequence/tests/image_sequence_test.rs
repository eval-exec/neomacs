use super::*;
use gif::{DisposalMethod, Encoder};
use neomacs_display_protocol::{
    EncodedBytes, ImageBackgroundPolicy, ImageColorContext, ImageFrameIndex, ImageSequenceId,
    ImageSequenceRetirement,
};

fn sequence(id: u64) -> ImageSequenceId {
    ImageSequenceId::new(id).expect("test sequence ids are non-zero")
}

/// The bytes of a source as a decode request carries them.
fn source(bytes: &[u8]) -> EncodedBytes {
    EncodedBytes::copy_of(bytes)
}

const ANIMATED_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"><animate attributeName="opacity" from="1" to="0" dur="1s" repeatCount="indefinite"/></rect></svg>"#;

#[test]
fn timed_sequence_refuses_frames_that_cannot_use_its_segment_delay_table() {
    for delays in [[20, 40, 60, 60], [20, 20, 60, 80]] {
        let frames = delays
            .into_iter()
            .map(|milliseconds| {
                (
                    1,
                    1,
                    vec![255, 0, 0, 255],
                    ImageFrameDelay::milliseconds(milliseconds, 1).unwrap(),
                )
            })
            .collect();
        assert!(
            DecodedImageSequence::from_timed_frames(frames, Some(ImageFrameIndex::new(2)))
                .is_none(),
            "an introduction delay table cannot describe unequal delays inside a segment"
        );
    }
    let frames = [20, 40, 60, 80]
        .into_iter()
        .map(|milliseconds| {
            (
                1,
                1,
                vec![255, 0, 0, 255],
                ImageFrameDelay::milliseconds(milliseconds, 1).unwrap(),
            )
        })
        .collect();
    assert!(
        DecodedImageSequence::from_timed_frames(frames, None).is_some(),
        "ordinary timed sequences retain individual frame delays"
    );
}

#[test]
fn computed_sequence_cache_respects_each_requested_sampling_policy() {
    let cache = ImageSequenceCache::new();
    for fps in [2, 4, 2] {
        let frame = cache
            .resolve_svg(
                sequence(1),
                ANIMATED_SVG,
                ImageFrameIndex::new(0),
                ImageColorContext::default(),
                &crate::svg::SvgResourceContext::Isolated,
                ImageAnimationPolicy::enabled(Some(fps)),
            )
            .expect_frame("animated SVG frame");
        let (_, metadata) = frame.into_parts();
        assert_eq!(metadata.frame_count(), Some(fps));
        assert_eq!(
            metadata.frame_delay().unwrap().seconds(),
            Some(1.0 / f64::from(fps))
        );
    }
}

#[test]
fn concurrent_duplicate_decode_returns_the_published_sequence() {
    let cache = ImageSequenceCache::new();
    let ready = std::sync::Barrier::new(2);
    let published = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let late = scope.spawn(|| {
            cache.resolve_with(
                sequence(1),
                ImageFrameIndex::new(0),
                SequenceMaterialization::AuthoredRaster,
                || {
                    ready.wait();
                    published.wait();
                    Some(DecodedImageSequence::from_frames(
                        vec![(1, 1, vec![0, 0, 255, 255]); 3],
                        ImageFrameDelay::milliseconds(40, 1).unwrap(),
                    ))
                },
            )
        });
        ready.wait();
        let winner = cache
            .resolve_with(
                sequence(1),
                ImageFrameIndex::new(0),
                SequenceMaterialization::AuthoredRaster,
                || {
                    Some(DecodedImageSequence::from_frames(
                        vec![(1, 1, vec![255, 0, 0, 255]); 2],
                        ImageFrameDelay::milliseconds(20, 1).unwrap(),
                    ))
                },
            )
            .expect_frame("winner");
        published.wait();
        let loser = late.join().unwrap().expect_frame("coalesced duplicate");
        assert_eq!(loser.rgba(), winner.rgba());
        let (_, metadata) = loser.into_parts();
        assert_eq!(metadata.frame_count(), Some(2));
        assert_eq!(metadata.frame_delay(), ImageFrameDelay::milliseconds(20, 1));
    });
}

#[test]
fn decoder_panic_does_not_leave_retirement_fenced_forever() {
    let cache = ImageSequenceCache::new();
    let id = sequence(1);
    let panic = std::panic::catch_unwind(|| {
        cache.resolve_with(
            id,
            ImageFrameIndex::new(0),
            SequenceMaterialization::AuthoredRaster,
            || panic!("decoder failed"),
        );
    });
    assert!(panic.is_err());
    cache.retire(ImageSequenceRetirement::One(id));
    cache
        .resolve(
            id,
            &source(&animated_gif_bytes()),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("subsequent decode");
    assert!(
        cache.contains(id),
        "completed panic must release its decode lease"
    );
}

fn animated_gif_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        let frames = [
            image::Frame::from_parts(
                image::RgbaImage::from_pixel(2, 1, image::Rgba([0xff, 0, 0, 0xff])),
                0,
                0,
                image::Delay::from_numer_denom_ms(20, 1),
            ),
            image::Frame::from_parts(
                image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0xff, 0, 0xff])),
                0,
                0,
                image::Delay::from_numer_denom_ms(40, 1),
            ),
        ];
        encoder.encode_frames(frames).unwrap();
    }
    bytes
}

#[test]
fn sequence_cache_decodes_once_and_reuses_composited_frames() {
    let cache = ImageSequenceCache::with_max_bytes(1024 * 1024);
    let sequence = sequence(1);
    let bytes = animated_gif_bytes();

    let first = cache
        .resolve(
            sequence,
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("first frame");
    let second = cache
        .resolve(
            sequence,
            &source(&bytes),
            ImageFrameIndex::new(1),
            ImageColorContext::default(),
        )
        .expect_frame("second frame");

    assert_eq!(first.rgba(), [0xff, 0, 0, 0xff, 0xff, 0, 0, 0xff]);
    assert_eq!(second.rgba(), [0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff]);
    assert_eq!(
        cache.stats(),
        ImageSequenceCacheStats { hits: 1, misses: 1 }
    );
}

#[test]
fn retirement_prevents_a_late_decode_from_repopulating_stale_identity() {
    let cache = ImageSequenceCache::with_max_bytes(1024 * 1024);
    let old = sequence(4);
    let future = sequence(5);
    let bytes = animated_gif_bytes();
    let ready = std::sync::Barrier::new(2);
    let retired = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let late = scope.spawn(|| {
            cache.resolve_with(
                old,
                ImageFrameIndex::new(0),
                SequenceMaterialization::AuthoredRaster,
                || {
                    ready.wait();
                    retired.wait();
                    Some(DecodedImageSequence::from_frames(
                        vec![(2, 1, vec![0, 0, 255, 255]); 2],
                        ImageFrameDelay::milliseconds(20, 1).unwrap(),
                    ))
                },
            )
        });
        ready.wait();
        cache.retire(ImageSequenceRetirement::AllocatedThrough(old));
        cache
            .resolve(
                future,
                &source(&bytes),
                ImageFrameIndex::new(0),
                ImageColorContext::default(),
            )
            .expect_frame("future sequence remains live");
        retired.wait();
        late.join()
            .unwrap()
            .expect_frame("retired decode may finish for its caller");
    });

    assert!(!cache.contains(old));
    assert!(cache.contains(future));
}

fn gif_fixture_payload_sizes(bytes: &[u8]) -> (usize, usize) {
    // Independently measure the fixture's decoded RGBA buffers and retained
    // extension payloads, rather than trusting a cache's own accounting.
    // Two 2x1 RGBA frames occupy 16 bytes; each owns a four-byte GIF GCE.
    let frames = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    let pixel_bytes: usize = frames
        .iter()
        .map(|frame| frame.buffer().as_raw().len())
        .sum();
    let extensions = super::gif_metadata::frame_extensions(bytes).unwrap();
    let extension_bytes: usize = extensions
        .iter()
        .flatten()
        .map(|(_, payload)| payload.len())
        .sum();
    (pixel_bytes, extension_bytes)
}

#[test]
fn a_retired_identity_is_not_repopulated_by_a_stream() {
    let bytes = animated_gif_bytes();
    let cache = ImageSequenceCache::new();
    let retired = sequence(7);
    cache.retire(ImageSequenceRetirement::AllocatedThrough(retired));

    cache
        .resolve(
            retired,
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("the request itself is still answered");

    assert!(!cache.contains(retired));
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn stream_residency_counts_source_extensions_canvas_and_current_frame() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    assert_eq!((pixel_bytes, extension_bytes), (16, 8));
    let frame_bytes = pixel_bytes / 2;
    let control_bytes = extension_bytes / 2;
    let expected = bytes.len() + extension_bytes + frame_bytes * 2 + control_bytes;

    // A pixel-only budget used to refuse this sequence outright (#823). A
    // stream is retained whatever its size, and what it retains is the encoded
    // source (GNU `anim_cache.byte_size`), its parsed extensions table, its
    // compositing canvas, the frame it last returned, and that frame's GCE.
    let cache = ImageSequenceCache::with_max_bytes(pixel_bytes);
    cache
        .resolve(
            sequence(1),
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("first frame");

    assert!(cache.contains(sequence(1)));
    assert_eq!(cache.resident_bytes(), expected);
}

#[test]
fn streams_evict_the_least_recently_used_other_entry_to_meet_the_budget() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    let residency = bytes.len() + extension_bytes + pixel_bytes + extension_bytes / 2;
    let cache = ImageSequenceCache::with_max_bytes(2 * residency);
    for id in [1, 2] {
        cache
            .resolve(
                sequence(id),
                &source(&bytes),
                ImageFrameIndex::new(0),
                ImageColorContext::default(),
            )
            .expect_frame("resident stream");
    }
    assert_eq!(cache.resident_bytes(), 2 * residency);

    // Stream 1 was used more recently than stream 2, so stream 3 displaces 2.
    cache
        .resolve(
            sequence(1),
            &source(&bytes),
            ImageFrameIndex::new(1),
            ImageColorContext::default(),
        )
        .expect_frame("stream 1 again");
    cache
        .resolve(
            sequence(3),
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("third stream");
    assert!(cache.contains(sequence(1)));
    assert!(!cache.contains(sequence(2)));
    assert!(cache.contains(sequence(3)));
    assert_eq!(cache.resident_bytes(), 2 * residency);

    // A displaced stream reopens on its next request, displacing stream 1.
    cache.reset_decoded_frames();
    let reopened = cache
        .resolve(
            sequence(2),
            &source(&bytes),
            ImageFrameIndex::new(1),
            ImageColorContext::default(),
        )
        .expect_frame("displaced stream again");
    assert_eq!(reopened.rgba(), [0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff]);
    assert_eq!(cache.decoded_frames(), 2);
    assert!(!cache.contains(sequence(1)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), 2 * residency);
}

#[test]
fn a_stream_larger_than_the_whole_budget_is_still_retained() {
    // The #823 cliff at fixture scale: what the stream retains exceeds the
    // whole budget. GNU's `anim_cache` has no byte budget
    // (src/image.c:3773-3793), so refusing to retain the stream is what made
    // every frame a full redecode.
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    let residency = bytes.len() + extension_bytes + pixel_bytes + extension_bytes / 2;
    let cache = ImageSequenceCache::with_max_bytes(residency - 1);
    cache
        .resolve(
            sequence(1),
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("first frame");

    assert!(cache.contains(sequence(1)));
    cache.reset_decoded_frames();
    cache
        .resolve(
            sequence(1),
            &source(&bytes),
            ImageFrameIndex::new(1),
            ImageColorContext::default(),
        )
        .expect_frame("second frame");
    assert_eq!(
        cache.decoded_frames(),
        1,
        "the next frame is one step of the retained stream"
    );
}

#[test]
fn a_materialized_sequence_stays_within_its_exact_budget_and_stills_are_never_kept() {
    let frames = |count: usize| {
        Some(DecodedImageSequence::from_frames(
            vec![(1, 1, vec![255, 0, 0, 255]); count],
            ImageFrameDelay::milliseconds(20, 1).unwrap(),
        ))
    };
    let materialize = |cache: &ImageSequenceCache, id: u64, count: usize| {
        cache.resolve_with(
            sequence(id),
            ImageFrameIndex::new(0),
            SequenceMaterialization::AuthoredRaster,
            || frames(count),
        )
    };

    let cache = ImageSequenceCache::with_max_bytes(3 * 4);
    materialize(&cache, 1, 3).expect_frame("an exactly budgeted sequence decodes");
    assert!(cache.contains(sequence(1)));
    assert_eq!(cache.resident_bytes(), 12);

    materialize(&cache, 2, 2).expect_frame("replacement sequence decodes");
    assert!(!cache.contains(sequence(1)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), 8);

    let too_small = ImageSequenceCache::with_max_bytes(11);
    materialize(&too_small, 4, 3)
        .expect_frame("an over-budget sequence still returns pixels to its caller");
    assert!(!too_small.contains(sequence(4)));
    assert_eq!(too_small.resident_bytes(), 0);

    let still = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([0, 0, 0, 0xff]),
    ));
    let mut encoded = std::io::Cursor::new(Vec::new());
    still
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    assert!(matches!(
        cache.resolve(
            sequence(3),
            &source(encoded.get_ref()),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        ),
        ImageSequenceResolution::NotAnimated
    ));
    assert!(!cache.contains(sequence(3)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), 8);
}

#[test]
fn single_gif_keeps_graphics_control_delay_metadata() {
    // GNU image.c:10276-10314 publishes GCE data even for a single frame,
    // without creating a persistent anim_cache entry for a still image.
    let bytes = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff\x21\xf9\x04\x01\x0a\0\0\0\x2c\0\0\0\0\x01\0\x01\0\0\x02\x02\x44\x01\0\x3b";
    let cache = ImageSequenceCache::new();
    let frame = cache
        .resolve(
            sequence(1),
            &source(bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("one-frame GIF");
    let (_, metadata) = frame.into_parts();
    assert_eq!(metadata.frame_count(), None);
    assert_eq!(
        metadata.frame_delay(),
        ImageFrameDelay::milliseconds(100, 1),
        "a single-frame GIF still owns its GCE delay"
    );
    assert!(
        !cache.contains(sequence(1)),
        "still single-frame GIF must not remain resident as an animation stream"
    );
    assert_eq!(cache.resident_bytes(), 0);
}

// ---------------------------------------------------------------------------
// GIF streams
// ---------------------------------------------------------------------------

/// One indexed frame of an assembled GIF fixture.
struct IndexedFrame {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    dispose: DisposalMethod,
    transparent: Option<u8>,
    /// In centiseconds, as the file stores it.
    delay: u16,
    interlaced: bool,
    pixels: Vec<u8>,
}

const PALETTE: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];

fn indexed_gif(width: u16, height: u16, frames: &[IndexedFrame]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height, &PALETTE).unwrap();
    for frame in frames {
        encoder
            .write_frame(&gif::Frame {
                delay: frame.delay,
                dispose: frame.dispose,
                transparent: frame.transparent,
                needs_user_input: false,
                top: frame.top,
                left: frame.left,
                width: frame.width,
                height: frame.height,
                interlaced: frame.interlaced,
                palette: None,
                buffer: std::borrow::Cow::Borrowed(&frame.pixels),
            })
            .unwrap();
    }
    drop(encoder);
    assert!(
        super::gif_metadata::frame_extensions(&bytes).is_some(),
        "the fixture is a well-formed GIF"
    );
    bytes
}

/// Direct model of GNU Emacs 31.1 `gif_load` (`src/image.c:10061-10263`) over
/// an `IndexedFrame` sequence using `PALETTE`, used to verify that `GifStream`
/// produces identical frames across forward playback, rewinds, and reverse
/// steps.
fn gnu_gif_load_reference(
    width: usize,
    height: usize,
    frames: &[IndexedFrame],
    colors: ImageColorContext,
) -> Vec<Vec<u8>> {
    let frame_bg = colors.gif_frame_background_rgba8();
    let transparent_bg = colors.gif_transparent_background_rgba8();
    let mut canvas = vec![0u8; width * height * 4];
    for px in canvas.chunks_exact_mut(4) {
        px.copy_from_slice(&frame_bg);
    }
    let f0 = &frames[0];
    let (f0_l, f0_t, f0_r, f0_b) = (
        usize::from(f0.left),
        usize::from(f0.top),
        usize::from(f0.left) + usize::from(f0.width),
        usize::from(f0.top) + usize::from(f0.height),
    );
    let mut out = Vec::with_capacity(frames.len());
    for (j, frame) in frames.iter().enumerate() {
        for y in 0..height {
            for x in 0..width {
                if y < f0_t || y >= f0_b || x < f0_l || x >= f0_r {
                    let dst = (y * width + x) * 4;
                    canvas[dst..dst + 4].copy_from_slice(&frame_bg);
                }
            }
        }
        let disposal = if j == 0 {
            DisposalMethod::Background
        } else {
            match frame.dispose {
                DisposalMethod::Any | DisposalMethod::Previous => DisposalMethod::Keep,
                other => other,
            }
        };
        let overwrite_transparent = disposal != DisposalMethod::Keep;
        let sw = usize::from(frame.width);
        let sh = usize::from(frame.height);
        let sl = usize::from(frame.left);
        let st = usize::from(frame.top);
        for y in 0..sh {
            for x in 0..sw {
                let c = frame.pixels[y * sw + x];
                if frame.transparent != Some(c) || overwrite_transparent {
                    let rgba = if frame.transparent == Some(c) {
                        transparent_bg
                    } else {
                        let base = usize::from(c) * 3;
                        [PALETTE[base], PALETTE[base + 1], PALETTE[base + 2], 255]
                    };
                    let dst = ((st + y) * width + (sl + x)) * 4;
                    canvas[dst..dst + 4].copy_from_slice(&rgba);
                }
            }
        }
        out.push(canvas.clone());
    }
    out
}

/// Six frames of a 4x3 canvas: every disposal method, transparent indices
/// and sub-rectangle offsets.
fn disposal_frames() -> Vec<IndexedFrame> {
    let full = |pixels: Vec<u8>| IndexedFrame {
        left: 0,
        top: 0,
        width: 4,
        height: 3,
        dispose: DisposalMethod::Keep,
        transparent: None,
        delay: 6,
        interlaced: false,
        pixels,
    };
    vec![
        full((0..12).map(|n| (n % 4) as u8).collect()),
        IndexedFrame {
            left: 1,
            top: 0,
            width: 2,
            height: 2,
            dispose: DisposalMethod::Background,
            transparent: Some(3),
            delay: 7,
            interlaced: false,
            pixels: vec![3, 1, 3, 3],
        },
        IndexedFrame {
            left: 0,
            top: 1,
            width: 4,
            height: 2,
            dispose: DisposalMethod::Any,
            transparent: None,
            delay: 0,
            interlaced: false,
            pixels: vec![2, 2, 2, 2, 0, 1, 2, 3],
        },
        IndexedFrame {
            left: 2,
            top: 1,
            width: 2,
            height: 1,
            dispose: DisposalMethod::Previous,
            transparent: Some(1),
            delay: 300,
            interlaced: false,
            pixels: vec![1, 0],
        },
        IndexedFrame {
            left: 1,
            top: 1,
            width: 3,
            height: 2,
            dispose: DisposalMethod::Keep,
            transparent: Some(2),
            delay: 1,
            interlaced: false,
            pixels: vec![2, 2, 2, 2, 2, 2],
        },
        IndexedFrame {
            dispose: DisposalMethod::Background,
            transparent: Some(0),
            delay: 65535,
            ..full(vec![0; 12])
        },
    ]
}

#[test]
fn gif_stream_frames_match_gnu_gif_load_compositing() {
    let frames = disposal_frames();
    let bytes = indexed_gif(4, 3, &frames);
    let colors = ImageColorContext::from_pixels(0x00_ff_ff_ff, 0x00_10_20_30)
        .with_frame_background(0x00_ee_dd_cc);
    let reference = gnu_gif_load_reference(4, 3, &frames, colors);
    assert_eq!(reference.len(), frames.len());
    let extensions = super::gif_metadata::frame_extensions(&bytes).unwrap();
    let cache = ImageSequenceCache::new();
    let id = sequence(1);

    // Forward, the same frame twice, behind the position, ahead of it again.
    for index in [0, 1, 2, 3, 4, 5, 5, 2, 3, 4, 0, 5] {
        let frame = cache
            .resolve(
                id,
                &source(&bytes),
                ImageFrameIndex::new(u64::try_from(index).unwrap()),
                colors,
            )
            .expect_frame("frame");
        assert_eq!(frame.rgba(), reference[index].as_slice());
        let (_, metadata) = frame.into_parts();
        // GNU gif_load publishes the selected frame's count, delay and
        // extension blocks (src/image.c:10276-10314).
        assert_eq!(metadata.frame_count(), Some(6));
        let control = extensions[index][0].clone();
        assert_eq!(control.0, 0xf9);
        assert_eq!(
            metadata.gif_extensions().map(|blocks| blocks
                .iter()
                .map(|block| (block.function, block.bytes.clone()))
                .collect::<Vec<_>>()),
            Some(vec![control.clone()])
        );
        let delay = u16::from_le_bytes([control.1[1], control.1[2]]);
        assert_eq!(
            metadata.frame_delay(),
            if delay == 0 {
                Some(ImageFrameDelay::UseDefault)
            } else {
                ImageFrameDelay::milliseconds(u32::from(delay) * 10, 1)
            }
        );
    }
}

#[test]
fn interlaced_gif_frames_match_the_whole_sequence_decode() {
    // Interlace passes store row 0, then row 2, then rows 1 and 3.
    let rows: [[u8; 4]; 4] = [[0, 1, 2, 3], [1, 1, 1, 1], [2, 2, 2, 2], [3, 3, 3, 3]];
    let stored = [rows[0], rows[2], rows[1], rows[3]].concat();
    let bytes = indexed_gif(
        4,
        4,
        &[IndexedFrame {
            left: 0,
            top: 0,
            width: 4,
            height: 4,
            dispose: DisposalMethod::Keep,
            transparent: None,
            delay: 6,
            interlaced: true,
            pixels: stored,
        }],
    );
    let expected: Vec<u8> = rows
        .concat()
        .into_iter()
        .flat_map(|index| {
            let color = usize::from(index) * 3;
            [PALETTE[color], PALETTE[color + 1], PALETTE[color + 2], 255]
        })
        .collect();

    let frame = ImageSequenceCache::new()
        .resolve(
            sequence(1),
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("interlaced frame");
    assert_eq!(frame.rgba(), expected);
}

#[test]
fn gif_stream_rewinds_by_reopening_and_decodes_forward_once() {
    let frames = disposal_frames();
    let bytes = indexed_gif(4, 3, &frames);
    let colors = ImageColorContext::default();
    let reference = gnu_gif_load_reference(4, 3, &frames, colors);
    let cache = ImageSequenceCache::new();
    let id = sequence(1);
    cache.reset_decoded_frames();

    let mut decoded_per_request = Vec::new();
    for index in [0, 1, 1, 4, 2, 3] {
        let before = cache.decoded_frames();
        let frame = cache
            .resolve(
                id,
                &source(&bytes),
                ImageFrameIndex::new(u64::try_from(index).unwrap()),
                colors,
            )
            .expect_frame("frame");
        assert_eq!(frame.rgba(), reference[index].as_slice());
        decoded_per_request.push(cache.decoded_frames() - before);
    }
    assert_eq!(
        decoded_per_request,
        vec![1, 1, 0, 3, 3, 1],
        "one frame per step, none for the frame on display, forward to 4, \
         reopened from frame 0 for 2, then one step"
    );

    // Past the last image descriptor there is no frame to decode.
    assert!(matches!(
        cache.resolve(id, &source(&bytes), ImageFrameIndex::new(6), colors),
        ImageSequenceResolution::MissingFrame
    ));
    assert_eq!(cache.decoded_frames(), 9);
}

#[test]
fn concurrent_requests_step_a_stream_once() {
    let frames = disposal_frames();
    let bytes = indexed_gif(4, 3, &frames);
    let colors = ImageColorContext::default();
    let reference = gnu_gif_load_reference(4, 3, &frames, colors);
    let cache = std::sync::Arc::new(ImageSequenceCache::new());
    let id = sequence(1);
    cache.reset_decoded_frames();

    let decoded = std::thread::scope(|scope| {
        let requests = (0..4)
            .map(|_| {
                let cache = &cache;
                let source = source(&bytes);
                scope.spawn(move || {
                    cache
                        .resolve(id, &source, ImageFrameIndex::new(3), colors)
                        .expect_frame("frame 3")
                        .rgba()
                        .to_vec()
                })
            })
            .collect::<Vec<_>>();
        requests
            .into_iter()
            .map(|request| request.join().unwrap())
            .collect::<Vec<_>>()
    });

    assert!(decoded.iter().all(|rgba| rgba == &reference[3]));
    assert_eq!(
        cache.decoded_frames(),
        4,
        "four concurrent requests advance the position to frame 3 exactly once"
    );
}

/// Point the LZW minimum code size of the Nth image at an invalid value. The
/// record scan still succeeds; only the pixel decoder notices.
fn corrupt_nth_frame_code_size(bytes: &mut [u8], image_number: usize) {
    fn color_table_length(packed: u8) -> usize {
        if packed & 0x80 == 0 {
            0
        } else {
            3 * (1_usize << ((packed & 7) + 1))
        }
    }
    // Logical Screen Descriptor starts at offset 6; its packed byte is at 6 + 4.
    let mut at = 6 + 7 + color_table_length(bytes[10]);
    let mut seen = 0;
    loop {
        match bytes[at] {
            0x21 => {
                at += 2;
                while bytes[at] != 0 {
                    at += 1 + usize::from(bytes[at]);
                }
                at += 1;
            }
            0x2c => {
                seen += 1;
                // 1-byte image separator + 9-byte image descriptor; packed byte is last.
                at += 10;
                at += color_table_length(bytes[at - 1]);
                if seen == image_number {
                    bytes[at] = 42;
                    return;
                }
                at += 1;
                while bytes[at] != 0 {
                    at += 1 + usize::from(bytes[at]);
                }
                at += 1;
            }
            other => panic!("unexpected GIF record {other:#x}"),
        }
    }
}

#[test]
fn a_frame_that_fails_to_decode_ends_the_stream() {
    // The whole-sequence decode answered a failure the same way for every
    // frame: not an animation for frame zero, a missing frame otherwise, and
    // nothing retained. A stream cannot know about a later broken frame when
    // it answers an earlier one, so the prefix it already decoded stands;
    // from the failure on, every answer is the whole-sequence one.
    let frames = disposal_frames();
    let colors = ImageColorContext::default();
    let reference = gnu_gif_load_reference(4, 3, &frames, colors);
    let mut broken = indexed_gif(4, 3, &frames);
    corrupt_nth_frame_code_size(&mut broken, 2);

    let cache = ImageSequenceCache::new();
    let id = sequence(1);
    let first = cache
        .resolve(id, &source(&broken), ImageFrameIndex::new(0), colors)
        .expect_frame("the frame before the broken one");
    assert_eq!(first.rgba(), reference[0].as_slice());
    assert!(matches!(
        cache.resolve(id, &source(&broken), ImageFrameIndex::new(1), colors),
        ImageSequenceResolution::Failed
    ));
    assert!(!cache.contains(id));
    assert_eq!(cache.resident_bytes(), 0);

    let mut broken_first = indexed_gif(4, 3, &frames);
    corrupt_nth_frame_code_size(&mut broken_first, 1);
    assert!(matches!(
        cache.resolve(id, &source(&broken_first), ImageFrameIndex::new(0), colors),
        ImageSequenceResolution::Failed
    ));
    assert!(matches!(
        cache.resolve(id, &source(&broken_first), ImageFrameIndex::new(2), colors),
        ImageSequenceResolution::Failed
    ));
    assert!(!cache.contains(id));
}

/// GitHub #823's file shape, generated in code: 74 frames, 480x480, 6 cs on
/// even frames and 7 cs on odd ones. The pattern differs per frame and per
/// pixel so LZW has little to exploit. Never a tracked fixture.
fn large_animated_gif() -> Vec<u8> {
    const FRAMES: u32 = 74;
    const SIDE: u32 = 480;
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
    let mut rgba = image::RgbaImage::new(SIDE, SIDE);
    for index in 0..FRAMES {
        for (n, pixel) in rgba.pixels_mut().enumerate() {
            let n = n as u32;
            *pixel = image::Rgba([
                (index * 17 + n) as u8,
                (index * 31 + n * 3) as u8,
                (index * 13 + n.rotate_left(3)) as u8,
                255,
            ]);
        }
        let delay = if index % 2 == 0 { 60 } else { 70 };
        encoder
            .encode_frame(image::Frame::from_parts(
                rgba.clone(),
                0,
                0,
                image::Delay::from_numer_denom_ms(delay, 1),
            ))
            .unwrap();
    }
    drop(encoder);
    bytes
}

/// #823: the sequence decodes to 65.04 MiB, just over the 64 MiB budget, so
/// nothing retained it and every shown frame redecoded the whole file twice
/// (once for `image-multi-frame-p' through `image-metadata', once for the
/// new `:index'). GNU keeps the decoder position and the composed frame
/// (`struct anim_cache', src/image.c:3717-3749) and a step costs one frame.
#[test]
fn over_budget_gif_decodes_one_frame_per_shown_frame() {
    const FRAMES: u32 = 74;
    const SHOWN: u32 = 6;
    let bytes = large_animated_gif();
    assert_eq!(
        super::gif_metadata::frame_extensions(&bytes).map(|frames| frames.len()),
        Some(FRAMES as usize)
    );
    assert!(
        usize::try_from(FRAMES * 480 * 480 * 4).unwrap() > DEFAULT_SEQUENCE_CACHE_BYTES,
        "the decoded sequence must sit over the budget for this row to test the cliff"
    );

    let cache = ImageSequenceCache::new();
    let id = sequence(1);
    let extension_bytes: usize = super::gif_metadata::frame_extensions(&bytes)
        .unwrap()
        .iter()
        .flatten()
        .map(|(_, payload)| payload.len())
        .sum();
    cache.reset_decoded_frames();
    let first = cache
        .resolve(
            id,
            &source(&bytes),
            ImageFrameIndex::new(0),
            ImageColorContext::default(),
        )
        .expect_frame("first frame");
    assert_eq!(first.dimensions(), (480, 480));
    let mut per_shown = Vec::new();
    for shown in 1..=SHOWN {
        let before = cache.decoded_frames();
        // What `image-animate-timeout' does per frame (lisp/image.el:1052-1057):
        // `image-multi-frame-p' reads the metadata of the frame still
        // selected, then `image-show-frame' selects the next one.
        let displayed = cache
            .resolve(
                id,
                &source(&bytes),
                ImageFrameIndex::new(u64::from(shown - 1)),
                ImageColorContext::default(),
            )
            .expect_frame("metadata of the frame on display");
        assert_eq!(displayed.into_parts().1.frame_count(), Some(FRAMES));
        let next = cache
            .resolve(
                id,
                &source(&bytes),
                ImageFrameIndex::new(u64::from(shown)),
                ImageColorContext::default(),
            )
            .expect_frame("next frame");
        assert_eq!(
            next.into_parts().1.frame_delay(),
            ImageFrameDelay::milliseconds(if shown % 2 == 0 { 60 } else { 70 }, 1)
        );
        per_shown.push(cache.decoded_frames() - before);
    }
    assert_eq!(
        per_shown,
        vec![1; SHOWN as usize],
        "each shown frame must decode exactly one GIF frame"
    );
    assert!(cache.contains(id), "the stream stays resident");
    assert_eq!(
        cache.resident_bytes(),
        bytes.len() + extension_bytes + 2 * 480 * 480 * 4 + 4,
        "retained source + extensions + one canvas + one frame + its GCE"
    );
}

#[test]
fn individual_retirement_during_stream_open_does_not_repopulate() {
    let cache = ImageSequenceCache::new();
    let id = sequence(9);
    let bytes = animated_gif_bytes();
    let ready = std::sync::Barrier::new(2);
    let retired = std::sync::Barrier::new(2);
    let colors = ImageColorContext::default();
    std::thread::scope(|scope| {
        let opener = scope.spawn(|| {
            cache.resolve_stream_with(id, ImageFrameIndex::new(0), colors, || {
                ready.wait();
                retired.wait();
                GifStream::open(source(&bytes), GifBackgroundColors::from_context(colors))
            })
        });
        ready.wait();
        cache.retire(ImageSequenceRetirement::One(id));
        retired.wait();
        opener
            .join()
            .unwrap()
            .expect_frame("retired open may still answer its own caller");
    });

    assert!(
        !cache.contains(id),
        "a stream retired individually while opening must not publish"
    );
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn zero_frame_gif_is_not_retained_as_a_stream() {
    let empty_gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff\x3b";
    let cache = ImageSequenceCache::new();
    let id = sequence(10);
    let colors = ImageColorContext::default();
    assert!(matches!(
        cache.resolve(id, &source(empty_gif), ImageFrameIndex::new(0), colors),
        ImageSequenceResolution::Failed
    ));
    assert!(matches!(
        cache.resolve(id, &source(empty_gif), ImageFrameIndex::new(1), colors),
        ImageSequenceResolution::Failed
    ));
    assert!(!cache.contains(id));
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn resolve_resident_gif_steps_without_source_bytes() {
    let bytes = animated_gif_bytes();
    let cache = ImageSequenceCache::new();
    let id = sequence(11);
    let colors = ImageColorContext::default();
    assert!(
        cache
            .resolve_resident_gif(id, ImageFrameIndex::new(0), colors)
            .is_none()
    );
    assert_eq!(
        cache.stats(),
        ImageSequenceCacheStats { hits: 0, misses: 0 },
        "probing an absent resident stream must not record a double miss"
    );

    cache
        .resolve(id, &source(&bytes), ImageFrameIndex::new(0), colors)
        .expect_frame("initial open");
    cache.reset_decoded_frames();
    let second = cache
        .resolve_resident_gif(id, ImageFrameIndex::new(1), colors)
        .expect("resident stream hit")
        .expect_frame("second frame");
    assert_eq!(second.rgba(), [0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff]);
    assert_eq!(cache.decoded_frames(), 1);
    assert_eq!(
        cache.stats(),
        ImageSequenceCacheStats { hits: 1, misses: 1 }
    );
}

#[test]
fn two_out_of_phase_readers_on_one_gif_decode_one_frame_per_shown_frame() {
    // F1: two specs sharing one GIF source out of phase must not force each
    // other to reopen from frame 0 on every tick.
    let frames: Vec<IndexedFrame> = (0..12)
        .map(|i| IndexedFrame {
            left: 0,
            top: 0,
            width: 4,
            height: 3,
            dispose: DisposalMethod::Keep,
            transparent: None,
            delay: 5,
            interlaced: false,
            pixels: vec![(i % 4) as u8; 12],
        })
        .collect();
    let bytes = indexed_gif(4, 3, &frames);
    let colors = ImageColorContext::default();
    let reference = gnu_gif_load_reference(4, 3, &frames, colors);
    let cache = ImageSequenceCache::new();
    let id = sequence(12);
    let src = source(&bytes);

    // Prime spec A at frame 0..1 and spec B at frame 6.
    cache
        .resolve(id, &src, ImageFrameIndex::new(0), colors)
        .expect_frame("A frame 0");
    cache
        .resolve(id, &src, ImageFrameIndex::new(6), colors)
        .expect_frame("B frame 6");
    cache
        .resolve(id, &src, ImageFrameIndex::new(1), colors)
        .expect_frame("A frame 1");

    // Now both specs alternate forward ticks (including metadata check on
    // current frame followed by step to next frame, plus B wrapping 11 -> 0).
    let mut per_tick = Vec::new();
    for step in 0..8_u64 {
        let a_prev = 1 + step;
        let a_next = 2 + step;
        let b_prev = (6 + step) % 12;
        let b_next = (7 + step) % 12;

        let before_b = cache.decoded_frames();
        cache
            .resolve(id, &src, ImageFrameIndex::new(b_prev), colors)
            .expect_frame("B current metadata");
        let b_frame = cache
            .resolve(id, &src, ImageFrameIndex::new(b_next), colors)
            .expect_frame("B next frame");
        assert_eq!(b_frame.rgba(), reference[b_next as usize].as_slice());
        per_tick.push(cache.decoded_frames() - before_b);

        let before_a = cache.decoded_frames();
        cache
            .resolve(id, &src, ImageFrameIndex::new(a_prev), colors)
            .expect_frame("A current metadata");
        let a_frame = cache
            .resolve(id, &src, ImageFrameIndex::new(a_next), colors)
            .expect_frame("A next frame");
        assert_eq!(a_frame.rgba(), reference[a_next as usize].as_slice());
        per_tick.push(cache.decoded_frames() - before_a);
    }

    assert_eq!(
        per_tick,
        vec![1; 16],
        "each out-of-phase reader must decode exactly 1 frame per shown frame, including wraparound"
    );
}

#[test]
fn reverse_playback_reuses_backward_window_for_o1_steps() {
    // F1: negative `:speed` walks frames backwards (N-1, N-2, ..., 0).
    // Once the first backward step fills the bounded backward window, every
    // remaining reverse frame of a small GIF hits in O(1) with 0 decodes.
    let frames = disposal_frames();
    let bytes = indexed_gif(4, 3, &frames);
    let colors = ImageColorContext::default();
    let reference = gnu_gif_load_reference(4, 3, &frames, colors);
    let cache = ImageSequenceCache::new();
    let id = sequence(13);
    let src = source(&bytes);

    let last = cache
        .resolve(id, &src, ImageFrameIndex::new(5), colors)
        .expect_frame("frame 5");
    assert_eq!(last.rgba(), reference[5].as_slice());

    // First reverse step 5 -> 4 rewinds once and populates the backward window.
    let four = cache
        .resolve(id, &src, ImageFrameIndex::new(4), colors)
        .expect_frame("frame 4");
    assert_eq!(four.rgba(), reference[4].as_slice());

    cache.reset_decoded_frames();
    for index in (0..4_usize).rev() {
        let frame = cache
            .resolve(
                id,
                &src,
                ImageFrameIndex::new(u64::try_from(index).unwrap()),
                colors,
            )
            .expect_frame("reverse frame");
        assert_eq!(frame.rgba(), reference[index].as_slice());
    }
    assert_eq!(
        cache.decoded_frames(),
        0,
        "subsequent reverse steps must hit the backward window without redecoding"
    );
}

#[test]
fn idle_streams_are_pruned_after_sixty_seconds() {
    let bytes = animated_gif_bytes();
    let cache = ImageSequenceCache::new();
    let t0 = Instant::now();
    let colors = ImageColorContext::default();

    cache
        .resolve_at(
            sequence(1),
            &source(&bytes),
            ImageFrameIndex::new(0),
            colors,
            t0,
        )
        .expect_frame("idle stream");
    cache
        .resolve_at(
            sequence(2),
            &source(&bytes),
            ImageFrameIndex::new(0),
            colors,
            t0 + Duration::from_secs(30),
        )
        .expect_frame("recent stream");
    assert!(cache.contains(sequence(1)));
    assert!(cache.contains(sequence(2)));

    cache.prune_idle_streams_at(t0 + Duration::from_secs(61));
    assert!(
        !cache.contains(sequence(1)),
        "stream idle for 61 s must be pruned"
    );
    assert!(
        cache.contains(sequence(2)),
        "stream idle for only 31 s must remain resident"
    );
}

#[test]
fn budget_pressure_evicts_idle_streams_before_active_streams() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    let residency = bytes.len() + extension_bytes + pixel_bytes + extension_bytes / 2;
    let cache = ImageSequenceCache::with_max_bytes(2 * residency);
    let t0 = Instant::now();
    let colors = ImageColorContext::default();

    // 1) Materialized entry (sequence 1, lower last_access stamp) vs idle GIF
    //    stream (sequence 2, higher last_access stamp, idle >= 5 s).
    cache
        .resolve_with(
            sequence(1),
            ImageFrameIndex::new(0),
            SequenceMaterialization::AuthoredRaster,
            || {
                Some(DecodedImageSequence::from_frames(
                    vec![(1, 1, vec![0; residency])],
                    ImageFrameDelay::milliseconds(20, 1).unwrap(),
                ))
            },
        )
        .expect_frame("materialized entry");
    cache
        .resolve_at(
            sequence(2),
            &source(&bytes),
            ImageFrameIndex::new(0),
            colors,
            t0 + Duration::from_secs(1),
        )
        .expect_frame("stream 2 open");
    // At t0 + 10s, stream 2 has been idle for 9s (>= 5s ACTIVE_STREAM_WINDOW),
    // so stream 3 must evict idle stream 2 even though sequence 1 has an older
    // logical access stamp.
    cache
        .resolve_at(
            sequence(3),
            &source(&bytes),
            ImageFrameIndex::new(0),
            colors,
            t0 + Duration::from_secs(10),
        )
        .expect_frame("stream 3 open");
    assert!(
        cache.contains(sequence(1)),
        "materialized entry must survive ahead of an idle stream"
    );
    assert!(
        !cache.contains(sequence(2)),
        "idle stream must be evicted before entries in higher-priority tiers"
    );
    assert!(cache.contains(sequence(3)));

    // 2) Active stream (stream 3, accessed at t0 + 10s) vs newly idle stream
    //    (stream 4 arrives at t0 + 11s, then at t0 + 12s stream 3 is still active
    //    while sequence 1 is materialized -> sequence 1 is evicted before active
    //    stream 3).
    cache
        .resolve_at(
            sequence(4),
            &source(&bytes),
            ImageFrameIndex::new(0),
            colors,
            t0 + Duration::from_secs(12),
        )
        .expect_frame("stream 4 open");
    assert!(
        !cache.contains(sequence(1)),
        "materialized entry must be evicted before an actively animating stream"
    );
    assert!(
        cache.contains(sequence(3)),
        "active stream must be spared over materialized entry"
    );
    assert!(cache.contains(sequence(4)));
    assert_eq!(cache.resident_bytes(), 2 * residency);
}

// ---------------------------------------------------------------------------
// GNU Emacs 31.1 gif_load disposal and transparency contract (F6)
// ---------------------------------------------------------------------------

#[test]
fn gnu_gif_load_frame_zero_dispose_background_does_not_clear_canvas_for_frame_one() {
    // Minimal F6 reproducer: frame 0 fills 4x4 with red and carries
    // DISPOSE_BACKGROUND; frame 1 draws a 2x2 green subimage at (1, 1) with
    // DISPOSE_DO_NOT. GNU gif_load (src/image.c:10185-10263) never clears the
    // pixmap after a disposed frame, so frame 1 retains all 12 outer red pixels.
    let bytes = indexed_gif(
        4,
        4,
        &[
            IndexedFrame {
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                dispose: DisposalMethod::Background,
                transparent: None,
                delay: 10,
                interlaced: false,
                pixels: vec![0; 16], // red
            },
            IndexedFrame {
                left: 1,
                top: 1,
                width: 2,
                height: 2,
                dispose: DisposalMethod::Keep,
                transparent: None,
                delay: 10,
                interlaced: false,
                pixels: vec![1; 4], // green
            },
        ],
    );
    let colors = ImageColorContext::from_pixels(0x00_00_00_00, 0x00_ff_ff_ff)
        .with_frame_background(0x00_ff_ff_ff);
    let frame1 = ImageSequenceCache::new()
        .resolve(
            sequence(20),
            &source(&bytes),
            ImageFrameIndex::new(1),
            colors,
        )
        .expect_frame("frame 1");

    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let red_px = frame1
        .rgba()
        .chunks_exact(4)
        .filter(|px| *px == red)
        .count();
    let green_px = frame1
        .rgba()
        .chunks_exact(4)
        .filter(|px| *px == green)
        .count();
    assert_eq!(
        red_px, 12,
        "outer pixels from frame 0 must survive DISPOSE_BACKGROUND"
    );
    assert_eq!(green_px, 4, "inner 2x2 subimage must paint green");
}

#[test]
fn gnu_gif_load_transparent_pixels_use_specified_or_frame_bg_and_borders_use_frame_bg() {
    // 4x4 canvas:
    // - Frame 0 is 2x2 at (1, 1), top-left transparent (index 3), rest red (0).
    // - Frame 1 covers 4x4 with blue (2), painting into the border outside frame 0.
    // - Frame 2 is 2x2 at (1, 1) with Keep and all transparent (index 3).
    // GNU gif_load (src/image.c:10061-10118, :10215-10217):
    // - Border outside frame 0 is ALWAYS filled with frame_bg (even when
    //   :background is specified), and is restored before each subsequent frame.
    // - Transparent pixels inside frame 0 use :background if specified, else
    //   frame_bg, ignoring the displaying face's own background.
    let bytes = indexed_gif(
        4,
        4,
        &[
            IndexedFrame {
                left: 1,
                top: 1,
                width: 2,
                height: 2,
                dispose: DisposalMethod::Keep, // forced to Background on frame 0
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![3, 0, 0, 0],
            },
            IndexedFrame {
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                dispose: DisposalMethod::Keep,
                transparent: None,
                delay: 10,
                interlaced: false,
                pixels: vec![2; 16], // blue everywhere, including border
            },
            IndexedFrame {
                left: 1,
                top: 1,
                width: 2,
                height: 2,
                dispose: DisposalMethod::Keep,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![3; 4], // all transparent with Keep -> preserves blue inside
            },
        ],
    );
    let src = source(&bytes);
    let frame_bg = [0xee, 0xdd, 0xcc, 0xff];
    let spec_bg = [0x11, 0x22, 0x33, 0xff];
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];

    // Case A: no :background -> transparent pixel uses frame_bg, ignoring face_bg (0xaa_bb_cc).
    let no_spec_colors =
        ImageColorContext::from_pixels(0, 0x00_aa_bb_cc).with_frame_background(0x00_ee_dd_cc);
    let f0_no_spec = ImageSequenceCache::new()
        .resolve(sequence(21), &src, ImageFrameIndex::new(0), no_spec_colors)
        .expect_frame("frame 0 without :background");
    assert_eq!(
        &f0_no_spec.rgba()[0..4],
        &frame_bg,
        "border (0,0) is frame_bg"
    );
    assert_eq!(
        &f0_no_spec.rgba()[(4 + 1) * 4..(4 + 1) * 4 + 4],
        &frame_bg,
        "transparent pixel (1,1) uses frame_bg, not face_bg"
    );
    assert_eq!(
        &f0_no_spec.rgba()[(4 + 2) * 4..(4 + 2) * 4 + 4],
        &red,
        "opaque pixel (2,1) is red"
    );

    // Case B: explicit :background -> transparent pixel uses spec_bg, border stays frame_bg,
    // and frame 2 restores the border outside frame 0 back to frame_bg after frame 1 painted it.
    let with_spec_colors = no_spec_colors.with_specified_background(0x00_11_22_33);
    let cache = ImageSequenceCache::new();
    let f0_spec = cache
        .resolve(
            sequence(22),
            &src,
            ImageFrameIndex::new(0),
            with_spec_colors,
        )
        .expect_frame("frame 0 with :background");
    assert_eq!(
        &f0_spec.rgba()[0..4],
        &frame_bg,
        "border outside frame 0 ignores :background"
    );
    assert_eq!(
        &f0_spec.rgba()[(4 + 1) * 4..(4 + 1) * 4 + 4],
        &spec_bg,
        "transparent pixel inside frame 0 uses :background"
    );
    let f1_spec = cache
        .resolve(
            sequence(22),
            &src,
            ImageFrameIndex::new(1),
            with_spec_colors,
        )
        .expect_frame("frame 1");
    assert_eq!(
        &f1_spec.rgba()[0..4],
        &blue,
        "frame 1 covers border with blue"
    );
    let f2_spec = cache
        .resolve(
            sequence(22),
            &src,
            ImageFrameIndex::new(2),
            with_spec_colors,
        )
        .expect_frame("frame 2");
    assert_eq!(
        &f2_spec.rgba()[0..4],
        &frame_bg,
        "frame 2 restores border outside frame 0 back to frame_bg (src/image.c:10103-10117)"
    );
    assert_eq!(
        &f2_spec.rgba()[(4 + 1) * 4..(4 + 1) * 4 + 4],
        &blue,
        "transparent pixel inside frame 2 with Keep preserves frame 1's blue"
    );
}

#[test]
fn gnu_gif_load_disposal_modes_control_transparent_pixel_overwrites() {
    // Frame 0: 4x1 all red.
    // Frame 1: (0,0) 1x1 transparent with DisposalMethod::Any -> treated as Keep (stays red).
    // Frame 2: (1,0) 1x1 transparent with DisposalMethod::Previous -> treated as Keep (stays red).
    // Frame 3: (2,0) 1x1 transparent with DisposalMethod::Keep -> stays red.
    // Frame 4: (3,0) 1x1 transparent with DisposalMethod::Background -> overwritten with transparent_bg.
    let bytes = indexed_gif(
        4,
        1,
        &[
            IndexedFrame {
                left: 0,
                top: 0,
                width: 4,
                height: 1,
                dispose: DisposalMethod::Keep,
                transparent: None,
                delay: 10,
                interlaced: false,
                pixels: vec![0, 0, 0, 0],
            },
            IndexedFrame {
                left: 0,
                top: 0,
                width: 1,
                height: 1,
                dispose: DisposalMethod::Any,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![3],
            },
            IndexedFrame {
                left: 1,
                top: 0,
                width: 1,
                height: 1,
                dispose: DisposalMethod::Previous,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![3],
            },
            IndexedFrame {
                left: 2,
                top: 0,
                width: 1,
                height: 1,
                dispose: DisposalMethod::Keep,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![3],
            },
            IndexedFrame {
                left: 3,
                top: 0,
                width: 1,
                height: 1,
                dispose: DisposalMethod::Background,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![3],
            },
        ],
    );
    let colors = ImageColorContext::from_pixels(0, 0)
        .with_frame_background(0x00_ff_ff_ff)
        .with_specified_background(0x00_12_34_56);
    let f4 = ImageSequenceCache::new()
        .resolve(
            sequence(23),
            &source(&bytes),
            ImageFrameIndex::new(4),
            colors,
        )
        .expect_frame("frame 4");
    assert_eq!(
        f4.rgba(),
        [
            255, 0, 0, 255, // Any -> Keep
            255, 0, 0, 255, // Previous -> Keep
            255, 0, 0, 255, // Keep
            0x12, 0x34, 0x56, 255, // Background overwrites with specified_bg
        ]
    );
}

#[test]
fn transparent_background_policy_preserves_alpha_zero_for_chrome_gif_icons() {
    let bytes = indexed_gif(
        2,
        1,
        &[IndexedFrame {
            left: 0,
            top: 0,
            width: 2,
            height: 1,
            dispose: DisposalMethod::Background,
            transparent: Some(3),
            delay: 10,
            interlaced: false,
            pixels: vec![0, 3],
        }],
    );
    let chrome = ImageColorContext::from_pixels(0, 0x00_ff_ff_ff)
        .with_background_policy(ImageBackgroundPolicy::Transparent);
    let frame = ImageSequenceCache::new()
        .resolve(
            sequence(24),
            &source(&bytes),
            ImageFrameIndex::new(0),
            chrome,
        )
        .expect_frame("chrome GIF");
    assert_eq!(frame.rgba(), [255, 0, 0, 255, 0, 0, 0, 0]);
}

#[test]
fn gif_stream_cache_separates_distinct_backgrounds_and_shares_face_only_changes() {
    let bytes = indexed_gif(
        2,
        1,
        &[
            IndexedFrame {
                left: 0,
                top: 0,
                width: 2,
                height: 1,
                dispose: DisposalMethod::Keep,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![0, 3],
            },
            IndexedFrame {
                left: 0,
                top: 0,
                width: 2,
                height: 1,
                dispose: DisposalMethod::Keep,
                transparent: Some(3),
                delay: 10,
                interlaced: false,
                pixels: vec![1, 3],
            },
        ],
    );
    let src = source(&bytes);
    let cache = ImageSequenceCache::new();
    let id = sequence(25);

    let face_a = ImageColorContext::from_pixels(0x00_11_11_11, 0x00_22_22_22)
        .with_frame_background(0x00_ee_dd_cc);
    let face_b = ImageColorContext::from_pixels(0x00_33_33_33, 0x00_44_44_44)
        .with_frame_background(0x00_ee_dd_cc);
    let spec_blue = face_a.with_specified_background(0x00_00_00_ff);

    let f0 = cache
        .resolve(id, &src, ImageFrameIndex::new(0), face_a)
        .expect_frame("frame 0 under face A");
    assert_eq!(f0.rgba(), [255, 0, 0, 255, 0xee, 0xdd, 0xcc, 255]);

    // Different face bg but identical frame_bg & no :background -> reuses resident stream!
    let f1 = cache
        .resolve_resident_gif(id, ImageFrameIndex::new(1), face_b)
        .expect("face-only change must hit resident GIF stream")
        .expect_frame("frame 1 under face B");
    assert_eq!(f1.rgba(), [0, 255, 0, 255, 0xee, 0xdd, 0xcc, 255]);
    assert_eq!(
        cache.stats(),
        ImageSequenceCacheStats { hits: 1, misses: 1 }
    );

    // Explicit :background differs -> resident probe declines, full resolve opens a
    // second coexisting stream without evicting the first.
    assert!(
        cache
            .resolve_resident_gif(id, ImageFrameIndex::new(0), spec_blue)
            .is_none(),
        "different :background must not reuse stale pixels"
    );
    let f0_blue = cache
        .resolve(id, &src, ImageFrameIndex::new(0), spec_blue)
        .expect_frame("frame 0 with :background blue");
    assert_eq!(f0_blue.rgba(), [255, 0, 0, 255, 0, 0, 255, 255]);

    // Both streams remain resident simultaneously: stepping frame 1 of blue
    // and reading frame 1 of face_a cost 1 + 0 decodes (no cross-eviction).
    cache.reset_decoded_frames();
    let f1_blue = cache
        .resolve_resident_gif(id, ImageFrameIndex::new(1), spec_blue)
        .expect("blue stream must stay resident")
        .expect_frame("frame 1 with :background blue");
    assert_eq!(f1_blue.rgba(), [0, 255, 0, 255, 0, 0, 255, 255]);
    let f1_again = cache
        .resolve_resident_gif(id, ImageFrameIndex::new(1), face_a)
        .expect("original stream must stay resident beside blue stream")
        .expect_frame("frame 1 under face A");
    assert_eq!(f1_again.rgba(), [0, 255, 0, 255, 0xee, 0xdd, 0xcc, 255]);
    assert_eq!(
        cache.decoded_frames(),
        1,
        "coexisting :background streams must not evict each other"
    );

    // Retiring the sequence identity drops every background variant together.
    cache.retire(ImageSequenceRetirement::One(id));
    assert!(!cache.contains(id));
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn gnu_gif_load_rejects_whole_image_when_any_subimage_extends_outside_logical_screen() {
    // GNU src/image.c:10018-10035 checks EVERY subimage up front and rejects
    // the entire GIF for every :index (never falling back to a still decoder).
    let single_oob = indexed_gif(
        2,
        2,
        &[IndexedFrame {
            left: 1,
            top: 0,
            width: 2,
            height: 2,
            dispose: DisposalMethod::Keep,
            transparent: None,
            delay: 10,
            interlaced: false,
            pixels: vec![0; 4],
        }],
    );
    let multi_oob = indexed_gif(
        4,
        4,
        &[
            IndexedFrame {
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                dispose: DisposalMethod::Keep,
                transparent: None,
                delay: 10,
                interlaced: false,
                pixels: vec![0; 16],
            },
            IndexedFrame {
                left: 2,
                top: 0,
                width: 4,
                height: 4,
                dispose: DisposalMethod::Keep,
                transparent: None,
                delay: 10,
                interlaced: false,
                pixels: vec![1; 16],
            },
        ],
    );
    let cache = ImageSequenceCache::new();
    let colors = ImageColorContext::default();
    assert!(matches!(
        cache.resolve(
            sequence(26),
            &source(&single_oob),
            ImageFrameIndex::new(0),
            colors
        ),
        ImageSequenceResolution::Failed
    ));
    assert!(!cache.contains(sequence(26)));

    // Even frame 0 (which itself fits) must fail when frame 1 is out of bounds.
    for index in [0, 1] {
        assert!(matches!(
            cache.resolve(
                sequence(27),
                &source(&multi_oob),
                ImageFrameIndex::new(index),
                colors
            ),
            ImageSequenceResolution::Failed
        ));
    }
    assert!(!cache.contains(sequence(27)));
    assert_eq!(cache.resident_bytes(), 0);
}
