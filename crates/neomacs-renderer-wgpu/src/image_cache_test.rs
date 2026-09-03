use super::*;
use neomacs_display_protocol::{
    AxisSize, ImageFrameDelay, ImageFrameIndex, ImageRotation, ImageSizeSpec,
};
use std::io::Cursor;
#[cfg(not(target_family = "wasm"))]
use std::num::NonZeroUsize;

#[cfg(not(target_family = "wasm"))]
#[test]
fn image_decoder_pool_is_nonempty_and_bounded_on_large_hosts() {
    let one = NonZeroUsize::new(1).unwrap();
    let large_host = NonZeroUsize::new(256).unwrap();

    assert_eq!(
        ImageDecoderPoolSize::from_available_parallelism(Some(one)).get(),
        1
    );
    assert_eq!(
        ImageDecoderPoolSize::from_available_parallelism(Some(large_host)).get(),
        MAX_IMAGE_DECODER_THREADS
    );
    assert_eq!(
        ImageDecoderPoolSize::from_available_parallelism(None).get(),
        MAX_IMAGE_DECODER_THREADS
    );
}

#[test]
fn freed_or_replaced_image_loads_reject_late_decode_outcomes() {
    let mut loads = ImageLoadLifecycle::default();

    let freed = loads.begin_generated(ImageId::new(41));
    loads.free(ImageId::new(41));
    assert!(!loads.accept(freed));

    let old = loads.begin_generated(ImageId::new(42));
    let current = loads.begin_generated(ImageId::new(42));
    assert!(!loads.accept(old));
    assert!(loads.accept(current));
    assert!(!loads.accept(current), "a duplicate terminal is stale");
    let replacement = loads.begin_generated(ImageId::new(42));
    assert!(loads.accept(replacement), "a new generation remains valid");
    assert!(loads.active.is_empty());
}

#[test]
fn ready_and_failed_terminals_consume_their_active_generations() {
    let mut loads = ImageLoadLifecycle::default();
    let ready = loads.begin_generated(ImageId::new(51));
    let failed = loads.begin_generated(ImageId::new(52));

    let ready = WorkerDecodeOutcome::Ready(ImageCache::decoded_image(
        ready,
        DecodedPixels {
            geometry: ImageRealization::default().resolve_geometry(
                ImageSizeSpec::default(),
                ImageNativeExtent::new(1, 1),
                ImageRotation::None,
            ),
            rgba: vec![0, 0, 0, 255],
            mask: ImageMaskKind::None,
            embedded: ImageEmbeddedMetadata::default(),
        },
    ));
    assert!(matches!(
        loads.take_current(ready),
        Some(WorkerDecodeOutcome::Ready(_))
    ));
    assert_eq!(loads.active.len(), 1);

    assert!(matches!(
        loads.take_current(WorkerDecodeOutcome::Failed(failed)),
        Some(WorkerDecodeOutcome::Failed(_))
    ));
    assert!(loads.active.is_empty());
    assert!(
        loads
            .take_current(WorkerDecodeOutcome::Failed(failed))
            .is_none()
    );
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn decoder_worker_survives_a_panicking_request() {
    let (request_tx, request_rx) = mpsc::channel();
    let (outcome_tx, outcome_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        ImageCache::decoder_thread_pooled(
            0,
            Arc::new(Mutex::new(request_rx)),
            outcome_tx,
            Arc::new(ImageSequenceCache::new()),
        )
    });
    let mut loads = ImageLoadLifecycle::default();
    let panicking = loads.begin_generated(ImageId::new(61));
    let following = loads.begin_generated(ImageId::new(62));

    request_tx
        .send(DecodeRequest {
            load: panicking,
            source: ImageSource::Panic,
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
        })
        .unwrap();
    request_tx
        .send(DecodeRequest {
            load: following,
            source: ImageSource::Data {
                data: png_bytes(vec![0x12, 0x34, 0x56, 0xff], 1, 1),
                resources: crate::svg::SvgResourceContext::Isolated,
                sequence: ImageSequenceId::new(62).expect("non-zero sequence"),
            },
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
        })
        .unwrap();
    drop(request_tx);

    assert!(matches!(
        outcome_rx.recv().unwrap(),
        WorkerDecodeOutcome::Failed(load) if load == panicking
    ));
    assert!(matches!(
        outcome_rx.recv().unwrap(),
        WorkerDecodeOutcome::Ready(decoded) if decoded.load == following
    ));
    worker.join().unwrap();
}

fn png_bytes(pixels: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn encoded_image_bytes(format: image::ImageFormat) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(1, 1, vec![0x12, 0x34, 0x56, 0xff]).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
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
fn animated_gif_decodes_selected_frame_and_publishes_gnu_sequence_metadata() {
    let decoded = ImageCache::decode_data_with_metadata_for_frame(
        &animated_gif_bytes(),
        ImageFrameIndex::new(1),
    )
    .expect("animated GIF frame should decode");

    assert_eq!(decoded.data, [0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff]);
    assert_eq!(decoded.metadata.embedded.frame_count(), Some(2));
    assert_eq!(
        decoded.metadata.embedded.frame_delay(),
        Some(ImageFrameDelay::milliseconds(40, 1).unwrap())
    );
}

#[test]
fn decoder_rejects_unavailable_frame_instead_of_silently_showing_frame_zero() {
    let still = png_bytes(vec![0x12, 0x34, 0x56, 0xff], 1, 1);

    assert!(
        ImageCache::decode_data_with_metadata_for_frame(&still, ImageFrameIndex::new(1)).is_none()
    );
    assert!(
        ImageCache::decode_data_with_metadata_for_frame(
            &animated_gif_bytes(),
            ImageFrameIndex::new(2),
        )
        .is_none()
    );
}

#[test]
fn decoded_opaque_png_reports_gnu_corner_background_without_lisp_background() {
    let data = png_bytes([0x12, 0x34, 0x56, 0xff].repeat(4), 2, 2);
    let decoded = ImageCache::decode_data_with_metadata(
        &data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();

    assert_eq!(decoded.metadata.layout.dimensions(), (2, 2));
    assert!(!decoded.metadata.background_transparent);
    assert_eq!(decoded.metadata.background, 0x12_34_56);
}

#[test]
fn decoded_transparent_png_stays_transparent_with_explicit_lisp_background() {
    let data = png_bytes([0x12, 0x34, 0x56, 0x00].repeat(4), 2, 2);
    let decoded = ImageCache::decode_data_with_metadata(
        &data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        (0, 0xff_aa_bb_cc),
    )
    .unwrap();

    assert!(decoded.metadata.background_transparent);
    assert_eq!(decoded.metadata.mask, ImageMaskKind::Clipping);
    assert_ne!(decoded.metadata.background, 0xaa_bb_cc);
}

#[test]
fn decoded_partial_alpha_is_not_misreported_as_a_clipping_mask() {
    let data = png_bytes([0x12, 0x34, 0x56, 0x80].repeat(4), 2, 2);
    let decoded = ImageCache::decode_data_with_metadata(
        &data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();

    assert_eq!(decoded.metadata.mask, ImageMaskKind::AlphaChannel);
    assert!(!decoded.metadata.mask.has_clipping_mask());
}

#[test]
fn mask_suppression_removes_alpha_and_mask_identity() {
    let mut rgba = vec![
        0x12, 0x34, 0x56, 0x00, 0x12, 0x34, 0x56, 0xff, 0x12, 0x34, 0x56, 0xff, 0x12, 0x34, 0x56,
        0x00,
    ];

    let mask = apply_mask_policy(&mut rgba, (2, 2), ImageMaskPolicy::Suppress);

    assert_eq!(mask, ImageMaskKind::None);
    assert!(rgba.iter().skip(3).step_by(4).all(|alpha| *alpha == 255));
}

#[test]
fn mask_suppression_does_not_discard_continuous_alpha() {
    let mut rgba = vec![0x12, 0x34, 0x56, 0x80];

    let mask = apply_mask_policy(&mut rgba, (1, 1), ImageMaskPolicy::Suppress);

    assert_eq!(mask, ImageMaskKind::AlphaChannel);
    assert_eq!(rgba[3], 0x80);
}

#[test]
fn heuristic_mask_builds_a_binary_clip_from_the_corner_background() {
    let mut rgba = vec![
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x40, 0xff, 0xff, 0xff, 0x80, 0x00, 0x00, 0x00,
        0x00,
    ];

    let mask = apply_mask_policy(
        &mut rgba,
        (2, 2),
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::FourCorners),
    );

    assert_eq!(mask, ImageMaskKind::Clipping);
    assert_eq!(
        rgba.iter().skip(3).step_by(4).copied().collect::<Vec<_>>(),
        vec![0, 0, 0, 255]
    );
}

#[test]
fn heuristic_mask_accepts_gnu_sixteen_bit_rgb_components() {
    let mut rgba = vec![0xff, 0x80, 0x00, 0xff, 0xfe, 0x80, 0x00, 0xff];

    let mask = apply_mask_policy(
        &mut rgba,
        (2, 1),
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::Rgb16([65535, 32896, 0])),
    );

    assert_eq!(mask, ImageMaskKind::Clipping);
    assert_eq!(rgba[3], 0);
    assert_eq!(rgba[7], 255);
}

#[test]
fn decoded_partial_alpha_png_corners_are_gnu_draw_not_transparent_mask() {
    for alpha in [1, 254] {
        let data = png_bytes([0x12, 0x34, 0x56, alpha].repeat(4), 2, 2);
        let decoded = ImageCache::decode_data_with_metadata(
            &data,
            ImageSizeSpec::default(),
            ImageRotation::None,
            (0, 0),
        )
        .unwrap();

        assert!(
            !decoded.metadata.background_transparent,
            "GNU mask DRAW includes nonzero alpha {alpha}"
        );
    }
}

#[test]
fn decoded_corner_mask_tie_uses_gnu_first_corner_winner() {
    let metadata = |alphas: [u8; 4]| {
        let pixels = alphas
            .into_iter()
            .flat_map(|alpha| [0x12, 0x34, 0x56, alpha])
            .collect();
        let data = png_bytes(pixels, 2, 2);
        ImageCache::decode_data_with_metadata(
            &data,
            ImageSizeSpec::default(),
            ImageRotation::None,
            (0, 0),
        )
        .unwrap()
        .metadata
    };

    assert!(!metadata([1, 0, 0, 254]).background_transparent);
    assert!(metadata([0, 1, 254, 0]).background_transparent);
}

#[test]
fn explicit_lisp_background_paints_the_svg_wrapper_background() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect x="1" y="1" width="2" height="2" fill="#123456"/></svg>"##;
    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0xff_aa_bb_cc),
    )
    .unwrap();

    // GNU's SVG wrapper paints a full-bleed rect with the Lisp :background
    // "instead of leaving it transparent" (src/image.c:12344).
    assert!(!decoded.metadata.background_transparent);
    assert_eq!(decoded.metadata.background, 0xaa_bb_cc);
}

#[test]
fn symbolic_widget_svg_uses_the_resolved_face_foreground() {
    let path = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("etc/images/down.svg");
    let data = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read symbolic widget SVG at {}: {error}", path.display()));

    let decoded = ImageCache::decode_data_with_metadata(
        &data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0x00ff_ffff, 0x0000_0000),
    )
    .expect("decode symbolic widget SVG");

    let light_opaque_pixels = decoded
        .data
        .chunks_exact(4)
        .filter(|pixel| pixel[0] >= 0x80 && pixel[1] >= 0x80 && pixel[2] >= 0x80 && pixel[3] != 0)
        .count();
    assert!(
        light_opaque_pixels >= 10,
        "GNU paints down.svg's currentColor from the white face foreground; got {light_opaque_pixels} light pixels"
    );
}

#[test]
fn symbolic_svg_explicit_root_color_overrides_the_face_foreground() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" color="#123456">
        <rect width="1" height="1" fill="currentColor"/>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0x00ff_ffff, 0x0000_0000),
    )
    .expect("decode explicitly colored SVG");

    assert_eq!(&decoded.data[..4], &[0x12, 0x34, 0x56, 0xff]);
}

#[test]
fn decoded_dimensionless_svg_uses_gnu_visible_geometry() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <rect width="100%" height="100%" fill="#000000"/>
        <rect width="80" height="40" fill="#ff0000"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (80, 40));

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(24)),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();

    assert_eq!(decoded.metadata.layout.dimensions(), (48, 24));
}

#[test]
fn dimensionless_svg_ignores_inline_style_percentages_during_measurement() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <rect width="80" height="40" fill="#ff0000"/>
        <path d="M 0 40 L 80 40" style="stroke: #000000; stroke-width: 100%"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn dimensionless_svg_ignores_stylesheet_percentages_during_measurement() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <style>.relative { stroke: #000000; stroke-width: 100% }</style>
        <rect width="80" height="40" fill="#ff0000"/>
        <path class="relative" d="M 0 40 L 80 40"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn decoded_dimensionless_svg_scales_document_coordinates_to_requested_size() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <rect width="80" height="40" fill="#000000"/>
        <rect y="36" width="80" height="4" fill="#ff0000"/>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(20)),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();

    assert_eq!(decoded.geometry.raster().dimensions(), (40, 20));
    let (raster_width, raster_height) = decoded.geometry.raster().dimensions();
    let bottom_left = ((raster_height - 1) * raster_width * 4) as usize;
    assert_eq!(
        &decoded.data[bottom_left..bottom_left + 4],
        &[0xff, 0x00, 0x00, 0xff],
        "the natural-coordinate bottom band must be scaled into the constrained output"
    );
}

#[test]
fn oversized_svg_input_is_rejected_before_parsing() {
    let mut data = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">"#.to_vec();
    data.resize(crate::svg::MAX_SVG_INPUT_SIZE + 1, b' ');
    data.extend_from_slice(b"</svg>");

    assert!(ImageCache::query_data_dimensions(&data).is_none());
    assert!(
        ImageCache::decode_data_with_metadata(
            &data,
            ImageSizeSpec::default(),
            ImageRotation::None,
            (0, 0)
        )
        .is_none()
    );
}

#[test]
fn svgz_expanding_past_the_svg_input_limit_is_rejected() {
    use std::io::Write;

    let mut svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">"#.to_vec();
    svg.resize(crate::svg::MAX_SVG_INPUT_SIZE + 1, b' ');
    svg.extend_from_slice(b"</svg>");

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&svg).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(
        compressed.len() < 64 * 1024,
        "fixture must be a compact SVGZ"
    );

    assert!(ImageCache::query_data_dimensions(&compressed).is_none());
    assert!(
        ImageCache::decode_data_with_metadata(
            &compressed,
            ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
            ImageRotation::None,
            (0, 0)
        )
        .is_none()
    );
}

#[test]
fn svg_physical_units_are_resolved_at_96_dpi() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1in" height="25.4mm">
        <rect width="100%" height="100%" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (96, 96));
}

#[test]
fn svg_single_explicit_dimension_uses_view_box_aspect_ratio() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" viewBox="0 0 100 50">
        <rect width="100" height="50" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (200, 100));

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    assert_eq!(decoded.geometry.raster().dimensions(), (200, 100));
    assert_eq!(&decoded.data[0..4], &[0x12, 0x34, 0x56, 0xff]);
    assert_eq!(
        &decoded.data[decoded.data.len() - 4..],
        &[0x12, 0x34, 0x56, 0xff]
    );
}

#[test]
fn svg_height_only_and_view_box_only_documents_preserve_aspect_ratio() {
    let height_only =
        br#"<svg xmlns="http://www.w3.org/2000/svg" height="100" viewBox="0 0 100 50"/>"#;
    let view_box_only = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"/>"#;

    let height_only = ImageCache::query_data_dimensions(height_only).unwrap();
    let view_box_only = ImageCache::query_data_dimensions(view_box_only).unwrap();
    assert_eq!(height_only.dimensions(), (200, 100));
    assert_eq!(view_box_only.dimensions(), (100, 50));
}

#[test]
fn svg_percentage_root_dimensions_defer_to_view_box_or_visible_geometry() {
    let with_view_box = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 100 50"/>"#;
    let dimensionless = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%"><rect width="80" height="40"/></svg>"#;

    let with_view_box = ImageCache::query_data_dimensions(with_view_box).unwrap();
    let dimensionless = ImageCache::query_data_dimensions(dimensionless).unwrap();
    assert_eq!(with_view_box.dimensions(), (100, 50));
    assert_eq!(dimensionless.dimensions(), (80, 40));
}

#[test]
fn svg_rejects_empty_malformed_and_invalid_dimension_documents() {
    for data in [
        br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.as_slice(),
        br#"<svg xmlns="http://www.w3.org/2000/svg">"#.as_slice(),
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="10" viewBox="0 0 20 10"/>"#.as_slice(),
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="NaN" height="10" viewBox="0 0 20 10"/>"#.as_slice(),
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1e999" height="10" viewBox="0 0 20 10"/>"#.as_slice(),
        b"\xff\xfe\xfd".as_slice(),
    ] {
        assert!(ImageCache::query_data_dimensions(data).is_none());
        assert!(ImageCache::decode_data_with_metadata(data, ImageSizeSpec::new(AxisSize::Native, AxisSize::Native), ImageRotation::None, (0, 0)).is_none());
    }
}

#[test]
fn recursive_svg_references_fail_closed_without_panicking() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
        <defs><g id="recursive"><use href="#recursive"/></g></defs>
        <use href="#recursive"/>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    // The recursive reference fails closed: nothing but the wrapper
    // background rect (opaque black face background) may paint.
    assert!(
        decoded
            .data
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn dimensionless_svg_fallback_geometry_includes_strokes() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <path d="M 5 5 L 15 5" stroke="#123456" stroke-width="10"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (15, 10));
}

#[test]
fn dimensionless_svg_fallback_keeps_gnu_positive_extent_for_negative_origins() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <rect x="-10" y="-5" width="30" height="15" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (20, 10));
}

#[test]
fn dimensionless_svg_fallback_includes_filter_layer_extent() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <defs>
            <filter id="blur" filterUnits="userSpaceOnUse" x="0" y="0" width="20" height="20">
                <feGaussianBlur stdDeviation="2"/>
            </filter>
        </defs>
        <rect x="5" y="5" width="10" height="10" fill="#123456" filter="url(#blur)"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (20, 20));
}

#[test]
fn dimensionless_svg_preserves_object_bounding_box_filter_percentages() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <defs>
            <filter id="blur" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="1"/>
            </filter>
        </defs>
        <rect x="10" y="10" width="10" height="10" fill="#123456" filter="url(#blur)"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (25, 25));
}

#[test]
fn dimensionless_svg_fallback_includes_group_transforms() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <g transform="translate(5 3)">
            <rect width="10" height="7" fill="#123456"/>
        </g>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (15, 10));
}

#[test]
fn dimensionless_svg_fallback_includes_root_transforms() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" transform="translate(5 3)">
        <rect width="10" height="7" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (15, 10));
}

#[test]
fn dimensionless_svg_preserves_percentages_inside_a_nested_viewport() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <svg width="80" height="40">
            <rect width="100%" height="100%" fill="#123456"/>
        </svg>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn dimensionless_svg_preserves_percentages_on_a_resolved_root_axis() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80">
        <rect width="100%" height="40" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageCache::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn dimensionless_svg_preserves_percentages_inside_a_symbol_viewport() {
    let symbol = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <defs>
            <symbol id="tile" viewBox="0 0 80 40">
                <rect width="100%" height="100%" fill="#123456"/>
            </symbol>
        </defs>
        <use href="#tile" width="80" height="40"/>
    </svg>"##;

    let symbol = ImageCache::query_data_dimensions(symbol).unwrap();

    assert_eq!(symbol.dimensions(), (80, 40));
}

#[test]
fn dimensionless_svg_fallback_includes_markers_and_rasterization_applies_clipping() {
    let marker = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <defs>
            <marker id="dot" markerUnits="userSpaceOnUse" markerWidth="10" markerHeight="10" refX="5" refY="5">
                <circle cx="5" cy="5" r="5" fill="#123456"/>
            </marker>
        </defs>
        <path d="M 5 5 L 15 5" marker-end="url(#dot)"/>
    </svg>"##;
    let clipped_data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <defs><clipPath id="clip"><rect width="10" height="10"/></clipPath></defs>
        <rect width="20" height="20" clip-path="url(#clip)" fill="#123456"/>
    </svg>"##;

    let marker = ImageCache::query_data_dimensions(marker).unwrap();
    let clipped = ImageCache::query_data_dimensions(clipped_data).unwrap();
    assert_eq!(marker.dimensions(), (20, 10));
    assert_eq!(clipped.dimensions(), (20, 20));

    let clipped = ImageCache::decode_data_with_metadata(
        clipped_data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    let raster_width = clipped.geometry.raster().width();
    let inside = ((5 * raster_width + 5) * 4) as usize;
    let outside = ((15 * raster_width + 15) * 4) as usize;
    assert_eq!(&clipped.data[inside..inside + 4], &[0x12, 0x34, 0x56, 0xff]);
    // Outside the clip path only the wrapper background rect (opaque black
    // face background) paints.
    assert_eq!(&clipped.data[outside..outside + 4], &[0, 0, 0, 0xff]);
}

#[test]
fn svg_masks_gradients_and_inline_css_survive_rasterization() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1">
        <style>.paint { fill: url(#gradient); color: #123456 }</style>
        <defs>
            <linearGradient id="gradient"><stop stop-color="currentColor"/><stop offset="1" stop-color="#abcdef"/></linearGradient>
            <mask id="half"><rect width="2" height="1" fill="white" fill-opacity="0.5"/></mask>
        </defs>
        <rect class="paint" width="2" height="1" color="#123456" mask="url(#half)"/>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    // The 0.5-white mask halves the content, which the wrapper background
    // rect then composites over the opaque face background (black).
    assert_eq!(decoded.data[3], 0xff);
    assert_eq!(decoded.data[7], 0xff);
    assert_ne!(&decoded.data[..3], &decoded.data[4..7]);
}

#[test]
fn svg_group_transforms_are_applied_before_rasterization() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
        <g transform="translate(5 0)">
            <rect width="5" height="10" fill="#123456"/>
        </g>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    assert_eq!(&decoded.data[0..4], &[0, 0, 0, 0xff]);
    let translated_pixel = (5 * 4) as usize;
    assert_eq!(
        &decoded.data[translated_pixel..translated_pixel + 4],
        &[0x12, 0x34, 0x56, 0xff]
    );
}

#[test]
fn svg_does_not_load_images_relative_to_the_process_working_directory() {
    let data = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">
        <image href="crates/neomacs-display-runtime/assets/window-icon.svg" width="4" height="4"/>
    </svg>"#;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    // The blocked relative reference paints nothing: only the wrapper
    // background rect (opaque black face background) remains.
    assert!(
        decoded
            .data
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn svg_explicit_base_uri_resolves_a_relative_raster() {
    let repository_root = Path::new(env!("CARGO_WORKSPACE_DIR"));
    let base_uri = repository_root.join("telega-avatar.svg");
    let data = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">
        <image href="assets/logo-128.png" width="4" height="4"/>
    </svg>"#;

    let decoded = crate::svg::decode(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        ImageRealization::default(),
        ImageColorContext::default(),
        crate::svg::SvgResourceContext::BaseUri(base_uri.to_string_lossy().into_owned()),
    )
    .expect("an explicit :base-uri authorizes its relative raster");

    assert!(
        decoded.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0),
        "the referenced raster must contribute visible pixels"
    );
}

#[test]
fn svg_base_uri_cannot_authorize_parent_directory_escape() {
    let repository_root = Path::new(env!("CARGO_WORKSPACE_DIR"));
    let base_uri = repository_root
        .join("crates")
        .join("neomacs-renderer-wgpu")
        .join("telega-avatar.svg");
    let data = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">
        <image href="../../assets/logo-128.png" width="4" height="4"/>
    </svg>"#;

    let decoded = crate::svg::decode(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        ImageRealization::default(),
        ImageColorContext::default(),
        crate::svg::SvgResourceContext::BaseUri(base_uri.to_string_lossy().into_owned()),
    )
    .expect("outer SVG remains valid");

    // The parent-directory escape fails closed: only the wrapper background
    // rect may paint.
    assert!(
        decoded
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn svg_keeps_embedded_data_images_enabled() {
    let data = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%221%22%20height%3D%221%22%3E%3Crect%20width%3D%221%22%20height%3D%221%22%20fill%3D%22%23ff0000%22%2F%3E%3C%2Fsvg%3E"/>
    </svg>"#;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    assert_eq!(&decoded.data, &[0xff, 0x00, 0x00, 0xff]);
}

#[test]
fn nested_svg_cannot_escape_the_external_resource_policy() {
    let data = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%221%22%20height%3D%221%22%3E%3Cimage%20href%3D%22neomacs-display-runtime%2Fassets%2Fwindow-icon.svg%22%20width%3D%221%22%20height%3D%221%22%2F%3E%3C%2Fsvg%3E"/>
    </svg>"#;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    // Nothing but the wrapper background rect (opaque black face background)
    // may paint: the nested external resource must not escape the policy.
    assert!(
        decoded
            .data
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn embedded_raster_formats_and_svgz_remain_enabled() {
    use resvg::usvg::ImageKind;
    use std::io::Write;

    let options = resvg::usvg::Options::default();
    for (mime, format, expected) in [
        ("image/png", image::ImageFormat::Png, "png"),
        ("image/jpeg", image::ImageFormat::Jpeg, "jpeg"),
        ("image/gif", image::ImageFormat::Gif, "gif"),
        ("image/webp", image::ImageFormat::WebP, "webp"),
    ] {
        let kind = crate::svg::resolve_embedded_image(
            mime,
            Arc::new(encoded_image_bytes(format)),
            &options,
        )
        .unwrap();
        assert!(
            matches!(
                (&kind, expected),
                (ImageKind::PNG(_), "png")
                    | (ImageKind::JPEG(_), "jpeg")
                    | (ImageKind::GIF(_), "gif")
                    | (ImageKind::WEBP(_), "webp")
            ),
            "wrong embedded image kind for {mime}"
        );
    }

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder
        .write_all(br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#)
        .unwrap();
    let svgz = encoder.finish().unwrap();
    assert!(matches!(
        crate::svg::resolve_embedded_image("image/svg+xml-compressed", Arc::new(svgz), &options,),
        Some(ImageKind::SVG(_))
    ));
}

#[test]
fn svg_text_uses_the_shared_system_font_database() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="24">
        <text x="1" y="18" font-family="sans-serif" font-size="18" fill="#123456">SVG</text>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    assert!(decoded.data.chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn semitransparent_svg_pixels_are_returned_as_straight_rgba() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <rect width="1" height="1" fill="#804020" fill-opacity="0.5"/>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    // The wrapper background rect (face background, black here) composites
    // under the semi-transparent content, so the result is opaque straight
    // RGB — never alpha-premultiplied.
    assert_eq!(decoded.data[3], 0xff);
    for (actual, expected) in decoded.data[..3].iter().zip([0x40_u8, 0x20, 0x10]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "RGB must be straight rather than alpha-premultiplied"
        );
    }
}

#[test]
fn hidpi_svg_keeps_logical_layout_extent_and_uses_device_pixel_raster_extent() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">
        <text x="2" y="20">HiDPI</text>
    </svg>"##;

    let decoded = crate::svg::decode(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(24)),
        ImageRotation::None,
        neomacs_display_protocol::ImageRealization::with_device_scale(1.0, 1.75),
        ImageColorContext::default(),
        crate::svg::SvgResourceContext::Isolated,
    )
    .expect("SVG decode");

    assert_eq!(decoded.geometry.layout().dimensions(), (48, 24));
    assert_eq!(decoded.geometry.raster().dimensions(), (84, 42));
    let (raster_width, raster_height) = decoded.geometry.raster().dimensions();
    assert_eq!(
        decoded.rgba.len(),
        raster_width as usize * raster_height as usize * 4
    );
}

#[test]
fn telega_cell_sized_custom_emoji_stays_logical_and_renders_the_full_image_at_2x() {
    let mut pixels = Vec::with_capacity(128 * 128 * 4);
    for y in 0..128 {
        for x in 0..128 {
            let color = match (x < 64, y < 64) {
                (true, true) => [0xff, 0x00, 0x00, 0xff],
                (false, true) => [0x00, 0xff, 0x00, 0xff],
                (true, false) => [0x00, 0x00, 0xff, 0xff],
                (false, false) => [0xff, 0xff, 0x00, 0xff],
            };
            pixels.extend_from_slice(&color);
        }
    }
    let data = png_bytes(pixels, 128, 128);

    let decoded = ImageCache::decode_data_with_metadata_at_realization(
        &data,
        ImageSizeSpec::new(AxisSize::Exact(16), AxisSize::AtMost(18)),
        ImageRotation::None,
        (0, 0),
        1.0,
        2.0,
    )
    .expect("custom emoji decode");

    assert_eq!(decoded.metadata.layout.dimensions(), (16, 16));
    assert_eq!(decoded.geometry.raster().dimensions(), (32, 32));
    let raster_width = decoded.geometry.raster().width();
    let pixel = |x: usize, y: usize| {
        let start = (y * raster_width as usize + x) * 4;
        &decoded.data[start..start + 4]
    };
    assert_eq!(pixel(0, 0), [0xff, 0x00, 0x00, 0xff]);
    assert_eq!(pixel(31, 0), [0x00, 0xff, 0x00, 0xff]);
    assert_eq!(pixel(0, 31), [0x00, 0x00, 0xff, 0xff]);
    assert_eq!(pixel(31, 31), [0xff, 0xff, 0x00, 0xff]);
}

#[test]
fn resolved_auto_scale_controls_both_svg_layout_and_raster_extents() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">
        <text x="2" y="20">HiDPI</text>
    </svg>"##;

    let decoded = crate::svg::decode(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(24)),
        ImageRotation::None,
        neomacs_display_protocol::ImageRealization::with_device_scale(1.3 / 1.75, 1.75),
        ImageColorContext::default(),
        crate::svg::SvgResourceContext::Isolated,
    )
    .expect("SVG decode");

    // The natural 80x40 scales to 59x30, which exceeds `:max-height 24`, so the
    // clamp wins: height 24, width follows the NATIVE ratio (24 * 80/40 = 48).
    // GNU does not scale `:max-*` — only `:width`/`:height` targets are scaled
    // (src/image.c:2771-2779) — so the scale is overridden here, not compounded.
    assert_eq!(decoded.geometry.layout().dimensions(), (48, 24));
    assert_eq!(decoded.geometry.raster().dimensions(), (84, 42));
}

#[test]
fn resolved_auto_scale_controls_bitmap_layout_and_raster_extents() {
    let data = png_bytes([0x12, 0x34, 0x56, 0xff].repeat(48 * 24), 48, 24);

    let decoded = ImageCache::decode_data_with_metadata_at_realization(
        &data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(24)),
        ImageRotation::None,
        (0, 0),
        1.3 / 1.75,
        1.75,
    )
    .expect("PNG decode");

    assert_eq!(decoded.metadata.layout.dimensions(), (36, 18));
    assert_eq!(decoded.geometry.raster().dimensions(), (63, 32));
}

#[test]
fn hidpi_svg_decode_metadata_stays_logical_while_texture_pixels_are_physical() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">
        <text x="2" y="20">HiDPI</text>
    </svg>"##;

    let decoded = ImageCache::decode_data_with_metadata_at_scale(
        data,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::AtMost(24)),
        ImageRotation::None,
        (0, 0),
        1.75,
    )
    .expect("SVG decode");

    assert_eq!(decoded.metadata.layout.dimensions(), (48, 24));
    // report_scale=1 when layout already lives in image-pixel space.
    assert_eq!(decoded.metadata.reported.dimensions(), (48, 24));
    assert_eq!(decoded.geometry.raster().dimensions(), (84, 42));
}

/// Real `etc/images/splash.svg` is 333×233 — the asset behind HiDPI #243.
#[test]
fn splash_svg_native_extent_is_333_by_233() {
    let path = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("etc/images/splash.svg");
    let data = std::fs::read(&path).unwrap_or_else(|err| {
        panic!("read splash.svg at {}: {err}", path.display());
    });
    let decoded = ImageCache::decode_data_with_metadata(
        &data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        (0, 0),
    )
    .expect("splash.svg decode");
    assert_eq!(
        decoded.metadata.layout.dimensions(),
        (333, 233),
        "native layout extent"
    );
    assert_eq!(
        decoded.metadata.reported.dimensions(),
        (333, 233),
        "image-pixel extent matches layout when report_scale=1"
    );
}

/// `:scale default` on 1.25× HiDPI: layout shrinks for redisplay; pixel_*
/// recovers GNU Fimage_size (333×233) via report_scale.
#[test]
fn splash_svg_scale_default_hidpi_preserves_gnu_image_pixel_extent() {
    let path = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("etc/images/splash.svg");
    let data = std::fs::read(&path).expect("splash.svg");
    // layout_scale = 1/1.25, report_scale = 1.25 (ImageScalePolicy::Default).
    let realization = ImageRealization::new(1.0 / 1.25, 1.25, 1.25);
    let decoded = ImageCache::decode_data_with_metadata_at_full_realization(
        &data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        (0, 0),
        realization,
    )
    .expect("splash.svg HiDPI decode");
    // scale_image_size ceils: ceil(333*0.8)=267, ceil(233*0.8)=187.
    assert_eq!(
        decoded.metadata.layout.dimensions(),
        (267, 187),
        "logical layout for :scale default @ 1.25"
    );
    // Pixel extent re-runs compute_image_size at layout×report (=1.0), not
    // ceil(layout×report), so we recover the true native 333×233.
    assert_eq!(
        decoded.metadata.reported.dimensions(),
        (333, 233),
        "Fimage_size PIXELS recovers native via layout×report scale"
    );
    // Texture is physical: ceil(267*1.25)=334, ceil(187*1.25)=234.
    assert_eq!(decoded.geometry.raster().dimensions(), (334, 234));
}

#[test]
fn decoded_xpm_distinguishes_transparent_and_opaque_corner_backgrounds() {
    let transparent = br#"/* XPM */
static char *icon[] = {
"2 2 2 1",
". c None",
"x c #123456",
"..",
".x"};"#;
    let opaque = br#"/* XPM */
static char *icon[] = {
"2 2 1 1",
"x c #123456",
"xx",
"xx"};"#;

    let transparent = ImageCache::decode_data_with_metadata(
        transparent,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    let opaque = ImageCache::decode_data_with_metadata(
        opaque,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    assert!(transparent.metadata.background_transparent);
    assert!(!opaque.metadata.background_transparent);
    assert_eq!(opaque.metadata.background, 0x12_34_56);
}

#[test]
fn test_convert_argb32_to_rgba_basic() {
    // Create a 2x2 ARGB32 image
    // ARGB32 format: A, R, G, B (4 bytes per pixel)
    let width = 2u32;
    let height = 2u32;
    let stride = width * 4; // No padding
    let data: Vec<u8> = vec![
        // Row 0
        255, 100, 150, 200, // Pixel (0,0): A=255, R=100, G=150, B=200
        128, 50, 75, 100, // Pixel (1,0): A=128, R=50, G=75, B=100
        // Row 1
        64, 25, 37, 50, // Pixel (0,1): A=64, R=25, G=37, B=50
        0, 0, 0, 0, // Pixel (1,1): A=0, R=0, G=0, B=0 (transparent)
    ];

    let result = ImageCache::convert_argb32_to_rgba(&data, width, height, stride);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 2);
    assert_eq!(h, 2);
    assert_eq!(rgba.len(), 16); // 2x2x4 bytes

    // Expected RGBA output: R, G, B, A
    // Pixel (0,0): R=100, G=150, B=200, A=255
    assert_eq!(&rgba[0..4], &[100, 150, 200, 255]);
    // Pixel (1,0): R=50, G=75, B=100, A=128
    assert_eq!(&rgba[4..8], &[50, 75, 100, 128]);
    // Pixel (0,1): R=25, G=37, B=50, A=64
    assert_eq!(&rgba[8..12], &[25, 37, 50, 64]);
    // Pixel (1,1): R=0, G=0, B=0, A=0
    assert_eq!(&rgba[12..16], &[0, 0, 0, 0]);
}

#[test]
fn test_convert_argb32_with_stride_padding() {
    // 2x2 image with stride = 12 (4 bytes padding per row)
    let width = 2u32;
    let height = 2u32;
    let stride = 12u32; // 8 bytes data + 4 bytes padding per row
    let data: Vec<u8> = vec![
        // Row 0 (8 bytes data + 4 bytes padding)
        255, 100, 150, 200, // Pixel (0,0)
        128, 50, 75, 100, // Pixel (1,0)
        0, 0, 0, 0, // Padding (ignored)
        // Row 1 (8 bytes data + 4 bytes padding)
        64, 25, 37, 50, // Pixel (0,1)
        32, 10, 20, 30, // Pixel (1,1)
        0, 0, 0, 0, // Padding (ignored)
    ];

    let result = ImageCache::convert_argb32_to_rgba(&data, width, height, stride);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 2);
    assert_eq!(h, 2);

    // Verify conversion (padding should be ignored)
    assert_eq!(&rgba[0..4], &[100, 150, 200, 255]); // Pixel (0,0)
    assert_eq!(&rgba[4..8], &[50, 75, 100, 128]); // Pixel (1,0)
    assert_eq!(&rgba[8..12], &[25, 37, 50, 64]); // Pixel (0,1)
    assert_eq!(&rgba[12..16], &[10, 20, 30, 32]); // Pixel (1,1)
}

#[test]
fn test_convert_argb32_invalid_data_size() {
    // Data too small for 2x2 image
    let data: Vec<u8> = vec![255, 100, 150, 200]; // Only 1 pixel
    let result = ImageCache::convert_argb32_to_rgba(&data, 2, 2, 8);
    assert!(result.is_none());
}

#[test]
fn test_convert_rgb24_to_rgba_basic() {
    // Create a 2x2 RGB24 image
    // RGB24 format: R, G, B (3 bytes per pixel)
    let width = 2u32;
    let height = 2u32;
    let stride = width * 3; // No padding
    let data: Vec<u8> = vec![
        // Row 0
        100, 150, 200, // Pixel (0,0): R=100, G=150, B=200
        50, 75, 100, // Pixel (1,0): R=50, G=75, B=100
        // Row 1
        25, 37, 50, // Pixel (0,1): R=25, G=37, B=50
        0, 0, 0, // Pixel (1,1): R=0, G=0, B=0 (black)
    ];

    let result = ImageCache::convert_rgb24_to_rgba(&data, width, height, stride);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 2);
    assert_eq!(h, 2);
    assert_eq!(rgba.len(), 16); // 2x2x4 bytes

    // Expected RGBA output: R, G, B, A (A should always be 255)
    assert_eq!(&rgba[0..4], &[100, 150, 200, 255]);
    assert_eq!(&rgba[4..8], &[50, 75, 100, 255]);
    assert_eq!(&rgba[8..12], &[25, 37, 50, 255]);
    assert_eq!(&rgba[12..16], &[0, 0, 0, 255]);
}

#[test]
fn test_convert_rgb24_with_stride_padding() {
    // 2x2 image with stride = 8 (2 bytes padding per row)
    let width = 2u32;
    let height = 2u32;
    let stride = 8u32; // 6 bytes data + 2 bytes padding per row
    let data: Vec<u8> = vec![
        // Row 0 (6 bytes data + 2 bytes padding)
        100, 150, 200, // Pixel (0,0)
        50, 75, 100, // Pixel (1,0)
        0, 0, // Padding (ignored)
        // Row 1 (6 bytes data + 2 bytes padding)
        25, 37, 50, // Pixel (0,1)
        10, 20, 30, // Pixel (1,1)
        0, 0, // Padding (ignored)
    ];

    let result = ImageCache::convert_rgb24_to_rgba(&data, width, height, stride);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 2);
    assert_eq!(h, 2);

    // Verify conversion (padding should be ignored)
    assert_eq!(&rgba[0..4], &[100, 150, 200, 255]); // Pixel (0,0)
    assert_eq!(&rgba[4..8], &[50, 75, 100, 255]); // Pixel (1,0)
    assert_eq!(&rgba[8..12], &[25, 37, 50, 255]); // Pixel (0,1)
    assert_eq!(&rgba[12..16], &[10, 20, 30, 255]); // Pixel (1,1)
}

#[test]
fn test_convert_rgb24_invalid_data_size() {
    // Data too small for 2x2 image
    let data: Vec<u8> = vec![100, 150, 200]; // Only 1 pixel
    let result = ImageCache::convert_rgb24_to_rgba(&data, 2, 2, 6);
    assert!(result.is_none());
}

#[test]
fn constrain_dimensions_only_enforces_the_texture_limit() {
    // `:max-width` / `:max-height` moved to `ImageSizeSpec::desired`, which
    // knows the native size and so can keep the aspect ratio against the right
    // numbers. What remains here is purely the GPU's 4096 texture ceiling.
    assert_eq!(constrain_dimensions(100, 100), (100, 100));
    assert_eq!(constrain_dimensions(4096, 4096), (4096, 4096));

    // Over the limit on one axis: the other follows to keep the ratio.
    assert_eq!(constrain_dimensions(8192, 4096), (4096, 2048));
    assert_eq!(constrain_dimensions(4096, 8192), (2048, 4096));

    // Never degenerate to zero.
    let (width, height) = constrain_dimensions(1, 8192);
    assert_eq!(width, 1);
    assert_eq!(height, 4096);
}

#[test]
fn test_convert_argb32_single_pixel() {
    // Single pixel image - edge case
    let data: Vec<u8> = vec![255, 128, 64, 32]; // A=255, R=128, G=64, B=32
    let result = ImageCache::convert_argb32_to_rgba(&data, 1, 1, 4);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 1);
    assert_eq!(h, 1);
    assert_eq!(rgba, vec![128, 64, 32, 255]); // R=128, G=64, B=32, A=255
}

#[test]
fn test_convert_rgb24_single_pixel() {
    // Single pixel image - edge case
    let data: Vec<u8> = vec![128, 64, 32]; // R=128, G=64, B=32
    let result = ImageCache::convert_rgb24_to_rgba(&data, 1, 1, 3);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 1);
    assert_eq!(h, 1);
    assert_eq!(rgba, vec![128, 64, 32, 255]); // R=128, G=64, B=32, A=255
}

#[test]
fn lru_victim_prefers_least_recent_stamp_over_smallest_id() {
    // Insert order 1, 2, 3 (stamps 1, 2, 3), then id 1 is accessed again
    // (stamp 4). FIFO-by-smallest-id would evict 1; LRU must evict 2.
    let entries = [
        (ImageId::new(1), 4u64),
        (ImageId::new(2), 2),
        (ImageId::new(3), 3),
    ];
    assert_eq!(
        lru_unpresented_victim(entries.iter().copied(), &Default::default()),
        Some(ImageId::new(2))
    );
}

#[test]
fn lru_victim_repeated_touches_protect_hot_entries() {
    // 3 was inserted last but 1 and 3 were both re-read afterwards; the
    // coldest entry is 2 regardless of insertion order.
    let entries = [
        (ImageId::new(1), 5u64),
        (ImageId::new(2), 2),
        (ImageId::new(3), 6),
    ];
    assert_eq!(
        lru_unpresented_victim(entries.iter().copied(), &Default::default()),
        Some(ImageId::new(2))
    );
}

#[test]
fn lru_victim_matches_insert_order_when_never_touched() {
    let entries = [
        (ImageId::new(1), 1u64),
        (ImageId::new(2), 2),
        (ImageId::new(3), 3),
    ];
    assert_eq!(
        lru_unpresented_victim(entries.iter().copied(), &Default::default()),
        Some(ImageId::new(1))
    );
}

#[test]
fn lru_victim_of_no_entries_is_none() {
    assert_eq!(
        lru_unpresented_victim(std::iter::empty(), &Default::default()),
        None
    );
}

#[test]
fn retiring_image_is_released_only_after_its_presentation_stops_referencing_it() {
    let image = ImageId::new(41);
    let mut lifecycle = ImageResidencyLifecycle::default();
    lifecycle.request_retirement(image);

    let retained = [image].into_iter().collect::<RetainedImageSet>();
    assert!(lifecycle.take_releasable(&retained).is_empty());
    assert_eq!(lifecycle.take_releasable(&Default::default()), [image]);
}

#[test]
fn lru_never_selects_an_image_referenced_by_an_active_presentation() {
    let retained = [ImageId::new(1)].into_iter().collect::<RetainedImageSet>();
    let entries = [(ImageId::new(1), 1u64), (ImageId::new(2), 2)];

    assert_eq!(
        lru_unpresented_victim(entries.into_iter(), &retained),
        Some(ImageId::new(2))
    );
}
