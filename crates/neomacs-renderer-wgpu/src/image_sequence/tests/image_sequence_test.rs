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

#[test]
fn sequence_cache_has_an_exact_decoded_byte_budget_and_does_not_keep_stills() {
    let cache = ImageSequenceCache::with_max_bytes(16);
    let bytes = animated_gif_bytes();
    cache.resolve(sequence(1), &bytes, ImageFrameIndex::new(0));
    assert_eq!(cache.resident_bytes(), 16);

    cache.resolve(sequence(2), &bytes, ImageFrameIndex::new(0));
    assert!(!cache.contains(sequence(1)));
    assert!(cache.contains(sequence(2)));
    assert_eq!(cache.resident_bytes(), 16);

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
}
