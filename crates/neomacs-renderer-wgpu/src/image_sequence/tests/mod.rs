use super::*;
use neomacs_display_protocol::{ImageFrameIndex, ImageSequenceId, ImageSequenceRetirement};

fn sequence(id: u64) -> ImageSequenceId {
    ImageSequenceId::new(id).expect("test sequence ids are non-zero")
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
    let decoded = decode_sequence(&animated_gif_bytes()).expect("animated GIF");

    cache.mark_in_flight(old);
    cache.retire(ImageSequenceRetirement::AllocatedThrough(old));
    cache.publish_decoded(old, decoded.clone());
    cache.publish_decoded(future, decoded);

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
