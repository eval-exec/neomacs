use super::*;
use neomacs_display_protocol::{ImageFrameIndex, ImageSequenceId, ImageSequenceRetirement};

fn sequence(id: u64) -> ImageSequenceId {
    ImageSequenceId::new(id).expect("test sequence ids are non-zero")
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
        .resolve(id, &animated_gif_bytes(), ImageFrameIndex::new(0))
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
        .resolve(sequence, &bytes, ImageFrameIndex::new(0))
        .expect_frame("first frame");
    let second = cache
        .resolve(sequence, &bytes, ImageFrameIndex::new(1))
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
                    decode_sequence(&bytes)
                },
            )
        });
        ready.wait();
        cache.retire(ImageSequenceRetirement::AllocatedThrough(old));
        cache
            .resolve(future, &bytes, ImageFrameIndex::new(0))
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
    // extension payloads, rather than trusting DecodedImageSequence.memory_size.
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
fn sequence_cache_pixel_only_budget_refuses_resident_gif_extension_payloads() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    assert_eq!((pixel_bytes, extension_bytes), (16, 8));
    let cache = ImageSequenceCache::with_max_bytes(pixel_bytes);
    let frame = cache
        .resolve(sequence(1), &bytes, ImageFrameIndex::new(0))
        .expect_frame("the caller still receives a frame when metadata exceeds the budget");
    assert_eq!(frame.dimensions(), (2, 1));
    assert!(!cache.contains(sequence(1)));
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn sequence_cache_has_an_exact_decoded_byte_budget_and_does_not_keep_stills() {
    let bytes = animated_gif_bytes();
    let (pixel_bytes, extension_bytes) = gif_fixture_payload_sizes(&bytes);
    assert_eq!(pixel_bytes, 2 * 2 * 1 * 4);
    assert_eq!(extension_bytes, 4 + 4);
    let exact_budget = pixel_bytes + extension_bytes;

    let cache = ImageSequenceCache::with_max_bytes(exact_budget);
    cache
        .resolve(sequence(1), &bytes, ImageFrameIndex::new(0))
        .expect_frame("an exactly budgeted sequence decodes");
    assert!(cache.contains(sequence(1)));
    assert_eq!(cache.resident_bytes(), exact_budget);

    cache
        .resolve(sequence(2), &bytes, ImageFrameIndex::new(0))
        .expect_frame("replacement sequence decodes");
    assert!(!cache.contains(sequence(1)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), exact_budget);

    let too_small = ImageSequenceCache::with_max_bytes(exact_budget - 1);
    too_small
        .resolve(sequence(4), &bytes, ImageFrameIndex::new(0))
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
        cache.resolve(sequence(3), encoded.get_ref(), ImageFrameIndex::new(0),),
        ImageSequenceResolution::NotAnimated
    ));
    assert!(!cache.contains(sequence(3)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), exact_budget);
}

#[test]
fn single_gif_keeps_graphics_control_delay_metadata() {
    // GNU image.c:10276-10314 publishes GCE data even for a single frame.
    let bytes = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff\x21\xf9\x04\x01\x0a\0\0\0\x2c\0\0\0\0\x01\0\x01\0\0\x02\x02\x44\x01\0\x3b";
    let sequence = decode_sequence(bytes).expect("one-frame GIF");
    let (_, metadata) = sequence
        .frame(ImageFrameIndex::new(0))
        .unwrap()
        .into_parts();
    assert_eq!(metadata.frame_count(), None);
    assert_eq!(
        metadata.frame_delay(),
        ImageFrameDelay::milliseconds(100, 1),
        "a single-frame GIF still owns its GCE delay"
    );
}
