use super::*;
use std::io::Write;

const ANIMATED: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"><animate attributeName="opacity" from="1" to="0" dur="1s" repeatCount="indefinite"/></rect></svg>"#;
const PREFIXED: &[u8] = br#"<s:svg xmlns:s="http://www.w3.org/2000/svg" width="2" height="2"><s:rect width="2" height="2" fill="red"><s:animate attributeName="opacity" from="1" to="0" dur="1s" repeatCount="indefinite"/></s:rect></s:svg>"#;
const STATIC: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"/></svg>"#;

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn decode_data(bytes: &[u8], frame: u64, policy: ImageAnimationPolicy) -> Option<DecodedPixels> {
    ImageCache::decode_data(
        EncodedBytes::copy_of(bytes),
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageColorContext::default(),
        ImageRealization::default(),
        ImageMaskPolicy::Preserve,
        policy,
        ImageFrameIndex::new(frame),
        crate::svg::SvgResourceContext::Isolated,
        &ImageSequenceCache::new(),
        ImageSequenceId::new(1).unwrap(),
        None,
    )
}

fn with_file(bytes: &[u8], name: &str, test: impl FnOnce(&str)) {
    let directory = std::path::Path::new("tmp/svg-forms").join(std::process::id().to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    std::fs::write(&path, bytes).unwrap();
    test(path.to_str().unwrap());
    std::fs::remove_file(&path).unwrap();
}

fn decode_file(path: &str, frame: u64, policy: ImageAnimationPolicy) -> Option<DecodedPixels> {
    ImageCache::decode_file(
        path,
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageColorContext::default(),
        ImageRealization::default(),
        ImageMaskPolicy::Preserve,
        policy,
        ImageFrameIndex::new(frame),
        &ImageSequenceCache::new(),
        ImageSequenceId::new(1).unwrap(),
        None,
    )
}

#[test]
fn svgz_data_materializes_the_same_animation_as_plain_svg() {
    let policy = ImageAnimationPolicy::enabled(Some(4));
    let plain = decode_data(ANIMATED, 1, policy).expect("plain frame one");
    let compressed = decode_data(&gzip(ANIMATED), 1, policy).expect("SVGZ frame one");
    assert_eq!(compressed.rgba, plain.rgba);
    assert_eq!(compressed.embedded, plain.embedded);
    assert_eq!(compressed.embedded.frame_count(), Some(4));
}

#[test]
fn namespace_prefixed_data_materializes_the_same_animation_as_plain_svg() {
    let policy = ImageAnimationPolicy::enabled(Some(4));
    let plain = decode_data(ANIMATED, 1, policy).expect("plain frame one");
    let prefixed = decode_data(PREFIXED, 1, policy).expect("prefixed frame one");
    assert_eq!(prefixed.rgba, plain.rgba);
    assert_eq!(prefixed.embedded, plain.embedded);
}

#[test]
fn compressed_and_namespace_prefixed_files_animate() {
    let policy = ImageAnimationPolicy::enabled(Some(4));
    let plain = decode_data(ANIMATED, 1, policy).expect("plain frame one");
    for (name, bytes) in [
        ("compressed.svgz", gzip(ANIMATED)),
        ("prefixed.svg", PREFIXED.to_vec()),
        ("prefixed.svgz", gzip(PREFIXED)),
    ] {
        with_file(&bytes, name, |path| {
            let frame = decode_file(path, 1, policy).expect("file frame one");
            assert_eq!(frame.rgba, plain.rgba);
            assert_eq!(frame.embedded, plain.embedded);
        });
    }
}

#[test]
fn static_svg_data_ignores_index_like_gnu() {
    for (bytes, policy) in [
        (STATIC, ImageAnimationPolicy::enabled(Some(4))),
        (ANIMATED, ImageAnimationPolicy::disabled()),
    ] {
        let first = decode_data(bytes, 0, policy).expect("static first frame");
        let indexed = decode_data(bytes, 7, policy).expect("static SVG ignores index");
        assert_eq!(first.rgba, indexed.rgba);
        assert!(indexed.embedded.is_empty());
    }
}

#[test]
fn static_svg_file_ignores_index_like_gnu() {
    with_file(STATIC, "static.svg", |path| {
        let first = decode_file(path, 0, ImageAnimationPolicy::disabled()).unwrap();
        let indexed = decode_file(path, 7, ImageAnimationPolicy::disabled())
            .expect("static SVG file ignores index");
        assert_eq!(first.rgba, indexed.rgba);
        assert!(indexed.embedded.is_empty());
    });
}

#[test]
fn computed_animation_out_of_range_index_does_not_become_static() {
    let policy = ImageAnimationPolicy::enabled(Some(4));
    assert!(decode_data(ANIMATED, 7, policy).is_none());
    with_file(ANIMATED, "out-of-range.svg", |path| {
        assert!(decode_file(path, 7, policy).is_none())
    });
}
