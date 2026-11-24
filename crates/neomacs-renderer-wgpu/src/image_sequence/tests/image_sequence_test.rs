use super::*;
use gif::{DisposalMethod, Encoder};
use neomacs_display_protocol::{
    EncodedBytes, ImageFrameIndex, ImageSequenceId, ImageSequenceRetirement,
};
use std::sync::atomic::Ordering::Relaxed;

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
        .resolve(id, &source(&animated_gif_bytes()), ImageFrameIndex::new(0))
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
        .resolve(sequence, &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("first frame");
    let second = cache
        .resolve(sequence, &source(&bytes), ImageFrameIndex::new(1))
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
            .resolve(future, &source(&bytes), ImageFrameIndex::new(0))
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
        .resolve(retired, &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("the request itself is still answered");

    assert!(!cache.contains(retired));
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn stream_residency_counts_the_canvas_the_frame_and_its_extensions() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    assert_eq!((pixel_bytes, extension_bytes), (16, 8));
    let frame_bytes = pixel_bytes / 2;
    let control_bytes = extension_bytes / 2;

    // A pixel-only budget used to refuse this sequence outright (#823). A
    // stream is retained whatever its size, and what it retains is its
    // compositing canvas, the frame it last returned, and that frame's
    // graphics control extension.
    let cache = ImageSequenceCache::with_max_bytes(pixel_bytes);
    cache
        .resolve(sequence(1), &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("first frame");

    assert!(cache.contains(sequence(1)));
    assert_eq!(cache.resident_bytes(), frame_bytes * 2 + control_bytes);
}

#[test]
fn streams_evict_the_least_recently_used_other_entry_to_meet_the_budget() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    let residency = pixel_bytes + extension_bytes / 2;
    let cache = ImageSequenceCache::with_max_bytes(2 * residency);
    for id in [1, 2] {
        cache
            .resolve(sequence(id), &source(&bytes), ImageFrameIndex::new(0))
            .expect_frame("resident stream");
    }
    assert_eq!(cache.resident_bytes(), 2 * residency);

    // Stream 1 was used more recently than stream 2, so stream 3 displaces 2.
    cache
        .resolve(sequence(1), &source(&bytes), ImageFrameIndex::new(1))
        .expect_frame("stream 1 again");
    cache
        .resolve(sequence(3), &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("third stream");
    assert!(cache.contains(sequence(1)));
    assert!(!cache.contains(sequence(2)));
    assert!(cache.contains(sequence(3)));
    assert_eq!(cache.resident_bytes(), 2 * residency);

    // A displaced stream reopens on its next request, displacing stream 1.
    DECODED_FRAMES.store(0, Relaxed);
    let reopened = cache
        .resolve(sequence(2), &source(&bytes), ImageFrameIndex::new(1))
        .expect_frame("displaced stream again");
    assert_eq!(reopened.rgba(), [0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff]);
    assert_eq!(DECODED_FRAMES.load(Relaxed), 2);
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
    let cache = ImageSequenceCache::with_max_bytes(pixel_bytes + extension_bytes / 2 - 1);
    cache
        .resolve(sequence(1), &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("first frame");

    assert!(cache.contains(sequence(1)));
    DECODED_FRAMES.store(0, Relaxed);
    cache
        .resolve(sequence(1), &source(&bytes), ImageFrameIndex::new(1))
        .expect_frame("second frame");
    assert_eq!(
        DECODED_FRAMES.load(Relaxed),
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
            ImageFrameIndex::new(0)
        ),
        ImageSequenceResolution::NotAnimated
    ));
    assert!(!cache.contains(sequence(3)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), 8);
}

#[test]
fn single_gif_keeps_graphics_control_delay_metadata() {
    // GNU image.c:10276-10314 publishes GCE data even for a single frame.
    let bytes = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff\x21\xf9\x04\x01\x0a\0\0\0\x2c\0\0\0\0\x01\0\x01\0\0\x02\x02\x44\x01\0\x3b";
    let frame = ImageSequenceCache::new()
        .resolve(sequence(1), &source(bytes), ImageFrameIndex::new(0))
        .expect_frame("one-frame GIF");
    let (_, metadata) = frame.into_parts();
    assert_eq!(metadata.frame_count(), None);
    assert_eq!(
        metadata.frame_delay(),
        ImageFrameDelay::milliseconds(100, 1),
        "a single-frame GIF still owns its GCE delay"
    );
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

/// `image`'s own GIF iterator over `bytes`: frame for frame what the
/// whole-sequence decode served before streams existed. Stops on the first
/// decode error because `image`'s `GifFrameIterator` is not fused on error
/// and `decode_sequence` itself returned on the first `Err`.
fn reference_frames(bytes: &[u8]) -> Vec<Result<image::RgbaImage, String>> {
    let mut frames = Vec::new();
    for frame in image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .unwrap()
        .into_frames()
    {
        match frame {
            Ok(frame) => frames.push(Ok(frame.buffer().clone())),
            Err(error) => {
                frames.push(Err(error.to_string()));
                break;
            }
        }
    }
    frames
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
fn gif_stream_frames_are_the_whole_sequence_decode_frames() {
    let frames = disposal_frames();
    let bytes = indexed_gif(4, 3, &frames);
    let reference = reference_frames(&bytes);
    assert_eq!(reference.len(), frames.len());
    assert!(reference.iter().all(Result::is_ok), "{reference:?}");
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
            )
            .expect_frame("frame");
        assert_eq!(
            frame.rgba(),
            reference[index].as_ref().unwrap().as_raw().as_slice()
        );
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
    assert_eq!(
        reference_frames(&bytes)[0]
            .as_ref()
            .unwrap()
            .as_raw()
            .as_slice(),
        expected
    );

    let frame = ImageSequenceCache::new()
        .resolve(sequence(1), &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("interlaced frame");
    assert_eq!(frame.rgba(), expected);
}

#[test]
fn gif_stream_rewinds_by_reopening_and_decodes_forward_once() {
    let bytes = indexed_gif(4, 3, &disposal_frames());
    let reference = reference_frames(&bytes);
    let cache = ImageSequenceCache::new();
    let id = sequence(1);
    DECODED_FRAMES.store(0, Relaxed);

    let mut decoded_per_request = Vec::new();
    for index in [0, 1, 1, 4, 2, 3] {
        let before = DECODED_FRAMES.load(Relaxed);
        let frame = cache
            .resolve(
                id,
                &source(&bytes),
                ImageFrameIndex::new(u64::try_from(index).unwrap()),
            )
            .expect_frame("frame");
        assert_eq!(
            frame.rgba(),
            reference[index].as_ref().unwrap().as_raw().as_slice()
        );
        decoded_per_request.push(DECODED_FRAMES.load(Relaxed) - before);
    }
    assert_eq!(
        decoded_per_request,
        vec![1, 1, 0, 3, 3, 1],
        "one frame per step, none for the frame on display, forward to 4, \
         reopened from frame 0 for 2, then one step"
    );

    // Past the last image descriptor there is no frame to decode.
    assert!(matches!(
        cache.resolve(id, &source(&bytes), ImageFrameIndex::new(6)),
        ImageSequenceResolution::MissingFrame
    ));
    assert_eq!(DECODED_FRAMES.load(Relaxed), 9);
}

#[test]
fn concurrent_requests_step_a_stream_once() {
    let bytes = indexed_gif(4, 3, &disposal_frames());
    let reference = reference_frames(&bytes);
    let cache = std::sync::Arc::new(ImageSequenceCache::new());
    let id = sequence(1);
    DECODED_FRAMES.store(0, Relaxed);

    let frames = std::thread::scope(|scope| {
        let requests = (0..4)
            .map(|_| {
                let cache = &cache;
                let source = source(&bytes);
                scope.spawn(move || {
                    cache
                        .resolve(id, &source, ImageFrameIndex::new(3))
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

    assert!(
        frames
            .iter()
            .all(|rgba| rgba == reference[3].as_ref().unwrap().as_raw())
    );
    assert_eq!(
        DECODED_FRAMES.load(Relaxed),
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
    let mut broken = indexed_gif(4, 3, &frames);
    corrupt_nth_frame_code_size(&mut broken, 2);
    let reference = reference_frames(&broken);
    assert!(reference[0].is_ok(), "{reference:?}");
    assert!(reference[1].is_err(), "{reference:?}");

    let cache = ImageSequenceCache::new();
    let id = sequence(1);
    let first = cache
        .resolve(id, &source(&broken), ImageFrameIndex::new(0))
        .expect_frame("the frame before the broken one");
    assert_eq!(
        first.rgba(),
        reference[0].as_ref().unwrap().as_raw().as_slice()
    );
    assert!(matches!(
        cache.resolve(id, &source(&broken), ImageFrameIndex::new(1)),
        ImageSequenceResolution::MissingFrame
    ));
    assert!(!cache.contains(id));
    assert_eq!(cache.resident_bytes(), 0);

    let mut broken_first = indexed_gif(4, 3, &frames);
    corrupt_nth_frame_code_size(&mut broken_first, 1);
    assert!(reference_frames(&broken_first)[0].is_err());
    assert!(matches!(
        cache.resolve(id, &source(&broken_first), ImageFrameIndex::new(0)),
        ImageSequenceResolution::NotAnimated
    ));
    assert!(matches!(
        cache.resolve(id, &source(&broken_first), ImageFrameIndex::new(2)),
        ImageSequenceResolution::MissingFrame
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
    DECODED_FRAMES.store(0, Relaxed);
    let first = cache
        .resolve(id, &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("first frame");
    assert_eq!(first.dimensions(), (480, 480));
    let mut per_shown = Vec::new();
    for shown in 1..=SHOWN {
        let before = DECODED_FRAMES.load(Relaxed);
        // What `image-animate-timeout' does per frame (lisp/image.el:1052-1057):
        // `image-multi-frame-p' reads the metadata of the frame still
        // selected, then `image-show-frame' selects the next one.
        let displayed = cache
            .resolve(
                id,
                &source(&bytes),
                ImageFrameIndex::new(u64::from(shown - 1)),
            )
            .expect_frame("metadata of the frame on display");
        assert_eq!(displayed.into_parts().1.frame_count(), Some(FRAMES));
        let next = cache
            .resolve(id, &source(&bytes), ImageFrameIndex::new(u64::from(shown)))
            .expect_frame("next frame");
        assert_eq!(
            next.into_parts().1.frame_delay(),
            ImageFrameDelay::milliseconds(if shown % 2 == 0 { 60 } else { 70 }, 1)
        );
        per_shown.push(DECODED_FRAMES.load(Relaxed) - before);
    }
    assert_eq!(
        per_shown,
        vec![1; SHOWN as usize],
        "each shown frame must decode exactly one GIF frame"
    );
    assert!(cache.contains(id), "the stream stays resident");
    assert_eq!(
        cache.resident_bytes(),
        2 * 480 * 480 * 4 + 4,
        "a canvas, one frame and its graphics control extension, whatever the frame count"
    );
}

#[test]
fn individual_retirement_during_stream_open_does_not_repopulate() {
    let cache = ImageSequenceCache::new();
    let id = sequence(9);
    let bytes = animated_gif_bytes();
    let ready = std::sync::Barrier::new(2);
    let retired = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let opener = scope.spawn(|| {
            cache.resolve_stream_with(id, ImageFrameIndex::new(0), || {
                ready.wait();
                retired.wait();
                GifStream::open(source(&bytes))
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
    assert!(matches!(
        cache.resolve(id, &source(empty_gif), ImageFrameIndex::new(0)),
        ImageSequenceResolution::NotAnimated
    ));
    assert!(matches!(
        cache.resolve(id, &source(empty_gif), ImageFrameIndex::new(1)),
        ImageSequenceResolution::MissingFrame
    ));
    assert!(!cache.contains(id));
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn resolve_resident_gif_steps_without_source_bytes() {
    let bytes = animated_gif_bytes();
    let cache = ImageSequenceCache::new();
    let id = sequence(11);
    assert!(
        cache
            .resolve_resident_gif(id, ImageFrameIndex::new(0))
            .is_none()
    );
    assert_eq!(
        cache.stats(),
        ImageSequenceCacheStats { hits: 0, misses: 0 },
        "probing an absent resident stream must not record a double miss"
    );

    cache
        .resolve(id, &source(&bytes), ImageFrameIndex::new(0))
        .expect_frame("initial open");
    DECODED_FRAMES.store(0, Relaxed);
    let second = cache
        .resolve_resident_gif(id, ImageFrameIndex::new(1))
        .expect("resident stream hit")
        .expect_frame("second frame");
    assert_eq!(second.rgba(), [0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff]);
    assert_eq!(DECODED_FRAMES.load(Relaxed), 1);
    assert_eq!(
        cache.stats(),
        ImageSequenceCacheStats { hits: 1, misses: 1 }
    );
}
