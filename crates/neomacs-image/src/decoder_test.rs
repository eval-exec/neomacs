use super::*;
use crate::image_bands::{BandPlacement, DecodedBand, RasterBand, RowRange};
use crate::image_probe::{ImageProbeSource, probe_image_layout};
use neomacs_display_protocol::{AxisSize, ImageFrameDelay};
use std::io::Cursor;
#[test]
fn decoder_recovers_from_a_panicking_request() {
    let load = ImageLoadToken::new(ImageId::new(1), ImageLoadAttempt::new(1).unwrap());
    let sequences = ImageSequenceCache::new();
    let request = DecodeRequest {
        load,
        source: ImageSource::Panic,
        size: Default::default(),
        rotation: Default::default(),
        realization: ImageRealization::default(),
        colors: ImageColorContext::default(),
        mask: ImageMaskPolicy::default(),
        frame: ImageFrameIndex::default(),
    };
    assert!(matches!(ImageDecoder::decode_request(request, &sequences),
        WorkerDecodeOutcome::Failed(failed) if failed == load));
    assert!(
        ImageDecoder::decode_data_with_metadata(
            &png_bytes(vec![0, 255, 0, 255], 1, 1),
            ImageSizeSpec::default(),
            ImageRotation::None,
            (0, 0)
        )
        .is_some()
    );
}

#[cfg(not(target_family = "wasm"))]
#[cfg(not(target_family = "wasm"))]
use std::num::NonZeroUsize;

#[cfg(not(target_family = "wasm"))]
#[cfg(not(target_family = "wasm"))]
use std::num::NonZeroUsize;

#[cfg(not(target_family = "wasm"))]
#[test]
fn toolbar_svg_keeps_alpha_and_symbolic_foreground_without_recoloring_artwork() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="3" height="1"><rect width="1" height="1" fill="currentColor"/><rect x="1" width="1" height="1" fill="#123456"/></svg>"##;
    let pixels = decode_toolbar_pixels(data);
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
    assert_eq!(&pixels[4..8], &[0x12, 0x34, 0x56, 255]);
    assert_eq!(&pixels[8..12], &[0, 0, 0, 0]);
}

#[test]
fn toolbar_xbm_set_bits_use_foreground_and_unset_bits_are_transparent() {
    let data = b"#define icon_width 2\n#define icon_height 1\nstatic unsigned char icon_bits[] = { 0x01 };";
    assert_eq!(decode_toolbar_pixels(data), [255, 0, 0, 255, 0, 0, 0, 0]);
}

#[test]
fn toolbar_xpm_keeps_intrinsic_colors_and_transparency() {
    let data = br##"/* XPM */
static char *icon[] = {
"2 1 2 1",
". c #123456",
"  c None",
". "};"##;
    assert_eq!(
        decode_toolbar_pixels(data),
        [0x12, 0x34, 0x56, 255, 0, 0, 0, 0]
    );
}

#[test]
fn toolbar_png_keeps_intrinsic_colors_and_alpha() {
    let pixels = vec![0x12, 0x34, 0x56, 255, 0, 0, 255, 128, 0, 0, 0, 0];
    let data = png_bytes(pixels.clone(), 3, 1);
    assert_eq!(decode_toolbar_pixels(&data), pixels);
}

fn decode_toolbar_pixels(data: &[u8]) -> Vec<u8> {
    let pixels = ImageDecoder::decode_data(
        data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageColorContext::from_pixels(0xff0000, 0xabcdef)
            .with_background_policy(neomacs_display_protocol::ImageBackgroundPolicy::Transparent),
        ImageRealization::with_device_scale(1.0, 1.0),
        ImageMaskPolicy::Preserve,
        ImageFrameIndex::default(),
        crate::svg::SvgResourceContext::Isolated,
        &ImageSequenceCache::new(),
        ImageSequenceId::new(1).unwrap(),
        None,
    )
    .expect("decode toolbar SVG");
    // Return the public decoder result's unpremultiplied RGBA payload.
    ImageDecoder::decoded_image(
        ImageLoadToken::new(ImageId::new(1), ImageLoadAttempt::new(1).unwrap()),
        pixels,
    )
    .data
}

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
        loads.take_current(WorkerDecodeOutcome::Failed {
            load: failed,
            diagnostic: test_diagnostic()
        }),
        Some(WorkerDecodeOutcome::Failed { .. })
    ));
    assert!(loads.active.is_empty());
    assert!(
        loads
            .take_current(WorkerDecodeOutcome::Failed {
                load: failed,
                diagnostic: test_diagnostic()
            })
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
            identity: test_load_identity(),
        })
        .unwrap();
    request_tx
        .send(DecodeRequest {
            load: following,
            source: ImageSource::Data {
                data: EncodedBytes::new(png_bytes(vec![0x12, 0x34, 0x56, 0xff], 1, 1)),
                resources: crate::svg::SvgResourceContext::Isolated,
                sequence: ImageSequenceId::new(62).expect("non-zero sequence"),
            },
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: test_load_identity(),
        })
        .unwrap();
    drop(request_tx);

    assert!(matches!(
        outcome_rx.recv().unwrap(),
        WorkerDecodeOutcome::Failed { load, .. } if load == panicking
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
    let decoded = ImageDecoder::decode_data_with_metadata_for_frame(
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
        ImageDecoder::decode_data_with_metadata_for_frame(&still, ImageFrameIndex::new(1))
            .is_none()
    );
    assert!(
        ImageDecoder::decode_data_with_metadata_for_frame(
            &animated_gif_bytes(),
            ImageFrameIndex::new(2),
        )
        .is_none()
    );
}

#[test]
fn decoded_opaque_png_reports_gnu_corner_background_without_lisp_background() {
    let data = png_bytes([0x12, 0x34, 0x56, 0xff].repeat(4), 2, 2);
    let decoded = ImageDecoder::decode_data_with_metadata(
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
    let decoded = ImageDecoder::decode_data_with_metadata(
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
    let decoded = ImageDecoder::decode_data_with_metadata(
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
        let decoded = ImageDecoder::decode_data_with_metadata(
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
        ImageDecoder::decode_data_with_metadata(
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
    let decoded = ImageDecoder::decode_data_with_metadata(
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
    let path = neomacs_infra::workspace_root()
        .as_path()
        .join("etc/images/down.svg");
    let data = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read symbolic widget SVG at {}: {error}", path.display()));

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (80, 40));

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn dimensionless_svg_ignores_stylesheet_percentages_during_measurement() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <style>.relative { stroke: #000000; stroke-width: 100% }</style>
        <rect width="80" height="40" fill="#ff0000"/>
        <path class="relative" d="M 0 40 L 80 40"/>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn decoded_dimensionless_svg_scales_document_coordinates_to_requested_size() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <rect width="80" height="40" fill="#000000"/>
        <rect y="36" width="80" height="4" fill="#ff0000"/>
    </svg>"##;

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    assert!(ImageDecoder::query_data_dimensions(&data).is_none());
    assert!(
        ImageDecoder::decode_data_with_metadata(
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

    assert!(ImageDecoder::query_data_dimensions(&compressed).is_none());
    assert!(
        ImageDecoder::decode_data_with_metadata(
            &compressed,
            ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
            ImageRotation::None,
            (0, 0)
        )
        .is_none()
    );
}

#[test]
fn fractional_svg_pending_geometry_matches_decoded_geometry() {
    let data =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="52.910000000000004" height="17.75"/>"##;
    // The public dimension query keeps its bounding-pixel contract. Pending
    // sizing must not feed that rounded answer back into the aspect ratio.
    assert_eq!(
        ImageDecoder::query_data_dimensions(data)
            .unwrap()
            .dimensions(),
        (53, 18)
    );
    let size = ImageSizeSpec::new(AxisSize::Exact(1000), AxisSize::Native);
    let intrinsic = ImageDecoder::query_data_intrinsic_extent(data).unwrap();
    let pending =
        ImageRealization::default().resolve_geometry(size, intrinsic, ImageRotation::None);
    let decoded = ImageDecoder::decode_data_with_metadata(data, size, ImageRotation::None, (0, 0))
        .expect("fractional SVG should decode");
    // Observed in GNU Emacs 31.1 by image-size-oracle.el.
    assert_eq!(pending.layout().dimensions(), (1000, 336));
    assert_eq!(pending, decoded.geometry);
}

#[test]
fn svg_physical_units_are_resolved_at_96_dpi() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1in" height="25.4mm">
        <rect width="100%" height="100%" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (96, 96));
}

#[test]
fn svg_single_explicit_dimension_uses_view_box_aspect_ratio() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" viewBox="0 0 100 50">
        <rect width="100" height="50" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (200, 100));

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let height_only = ImageDecoder::query_data_dimensions(height_only).unwrap();
    let view_box_only = ImageDecoder::query_data_dimensions(view_box_only).unwrap();
    assert_eq!(height_only.dimensions(), (200, 100));
    assert_eq!(view_box_only.dimensions(), (100, 50));
}

#[test]
fn svg_percentage_root_dimensions_defer_to_view_box_or_visible_geometry() {
    let with_view_box = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 100 50"/>"#;
    let dimensionless = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%"><rect width="80" height="40"/></svg>"#;

    let with_view_box = ImageDecoder::query_data_dimensions(with_view_box).unwrap();
    let dimensionless = ImageDecoder::query_data_dimensions(dimensionless).unwrap();
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
        assert!(ImageDecoder::query_data_dimensions(data).is_none());
        assert!(ImageDecoder::decode_data_with_metadata(data, ImageSizeSpec::new(AxisSize::Native, AxisSize::Native), ImageRotation::None, (0, 0)).is_none());
    }
}

#[test]
fn recursive_svg_references_fail_closed_without_panicking() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
        <defs><g id="recursive"><use href="#recursive"/></g></defs>
        <use href="#recursive"/>
    </svg>"##;

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
    assert_eq!(dimensions.dimensions(), (15, 10));
}

#[test]
fn dimensionless_svg_fallback_keeps_gnu_positive_extent_for_negative_origins() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <rect x="-10" y="-5" width="30" height="15" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
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

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();
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

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (25, 25));
}

#[test]
fn dimensionless_svg_fallback_includes_group_transforms() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <g transform="translate(5 3)">
            <rect width="10" height="7" fill="#123456"/>
        </g>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (15, 10));
}

#[test]
fn dimensionless_svg_fallback_includes_root_transforms() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" transform="translate(5 3)">
        <rect width="10" height="7" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (15, 10));
}

#[test]
fn dimensionless_svg_preserves_percentages_inside_a_nested_viewport() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <svg width="80" height="40">
            <rect width="100%" height="100%" fill="#123456"/>
        </svg>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();

    assert_eq!(dimensions.dimensions(), (80, 40));
}

#[test]
fn dimensionless_svg_preserves_percentages_on_a_resolved_root_axis() {
    let data = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80">
        <rect width="100%" height="40" fill="#123456"/>
    </svg>"##;

    let dimensions = ImageDecoder::query_data_dimensions(data).unwrap();

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

    let symbol = ImageDecoder::query_data_dimensions(symbol).unwrap();

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

    let marker = ImageDecoder::query_data_dimensions(marker).unwrap();
    let clipped = ImageDecoder::query_data_dimensions(clipped_data).unwrap();
    assert_eq!(marker.dimensions(), (20, 10));
    assert_eq!(clipped.dimensions(), (20, 20));

    let clipped = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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
    let repository_root = neomacs_infra::workspace_root();
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
    let repository_root = neomacs_infra::workspace_root();
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata(
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

    let decoded = ImageDecoder::decode_data_with_metadata_at_realization(
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

    let decoded = ImageDecoder::decode_data_with_metadata_at_realization(
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

    let decoded = ImageDecoder::decode_data_with_metadata_at_scale(
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

#[test]
fn splash_svg_native_extent_is_333_by_233() {
    let path = neomacs_infra::workspace_root()
        .as_path()
        .join("etc/images/splash.svg");
    let data = std::fs::read(&path).unwrap_or_else(|err| {
        panic!("read splash.svg at {}: {err}", path.display());
    });
    let decoded = ImageDecoder::decode_data_with_metadata(
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

#[test]
fn splash_svg_scale_default_hidpi_preserves_gnu_image_pixel_extent() {
    let path = neomacs_infra::workspace_root()
        .as_path()
        .join("etc/images/splash.svg");
    let data = std::fs::read(&path).expect("splash.svg");
    // layout_scale = 1/1.25, report_scale = 1.25 (ImageScalePolicy::Default).
    let realization = ImageRealization::new(1.0 / 1.25, 1.25, 1.25);
    let decoded = ImageDecoder::decode_data_with_metadata_at_full_realization(
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

    let transparent = ImageDecoder::decode_data_with_metadata(
        transparent,
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        ImageRotation::None,
        (0, 0),
    )
    .unwrap();
    let opaque = ImageDecoder::decode_data_with_metadata(
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

    let result = ImageDecoder::convert_argb32_to_rgba(&data, width, height, stride);
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

    let result = ImageDecoder::convert_argb32_to_rgba(&data, width, height, stride);
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
    let result = ImageDecoder::convert_argb32_to_rgba(&data, 2, 2, 8);
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

    let result = ImageDecoder::convert_rgb24_to_rgba(&data, width, height, stride);
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

    let result = ImageDecoder::convert_rgb24_to_rgba(&data, width, height, stride);
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
    let result = ImageDecoder::convert_rgb24_to_rgba(&data, 2, 2, 6);
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
    let result = ImageDecoder::convert_argb32_to_rgba(&data, 1, 1, 4);
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
    let result = ImageDecoder::convert_rgb24_to_rgba(&data, 1, 1, 3);
    assert!(result.is_some());

    let (w, h, rgba) = result.unwrap();
    assert_eq!(w, 1);
    assert_eq!(h, 1);
    assert_eq!(rgba, vec![128, 64, 32, 255]); // R=128, G=64, B=32, A=255
}

fn encoded_sized_image(format: image::ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let pixels = (0..width * height)
        .flat_map(|index| [index as u8, 0x40, 0x80, 0xff])
        .collect::<Vec<u8>>();
    let image = image::RgbaImage::from_raw(width, height, pixels).expect("pixel buffer");
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, format)
        .expect("format is encodable");
    bytes.into_inner()
}

#[test]
fn header_layout_equals_the_decoded_layout_for_every_probed_format() {
    let formats = [
        ("png", image::ImageFormat::Png),
        ("jpeg", image::ImageFormat::Jpeg),
        ("gif", image::ImageFormat::Gif),
        ("webp", image::ImageFormat::WebP),
        ("bmp", image::ImageFormat::Bmp),
        ("tiff", image::ImageFormat::Tiff),
        ("ico", image::ImageFormat::Ico),
    ];
    let sizes = [
        ImageSizeSpec::default(),
        ImageSizeSpec::new(AxisSize::AtMost(3), AxisSize::AtMost(9)),
        ImageSizeSpec::new(AxisSize::Exact(4), AxisSize::Native),
        ImageSizeSpec::new(AxisSize::Native, AxisSize::Exact(2)),
        ImageSizeSpec::new(AxisSize::Exact(6), AxisSize::AtMost(2)),
    ];
    let rotations = [
        ImageRotation::None,
        ImageRotation::Quarter,
        ImageRotation::Half,
        ImageRotation::ThreeQuarter,
    ];
    let realizations = [
        ImageRealization::default(),
        ImageRealization::with_device_scale(1.0, 2.0),
        ImageRealization::new(1.30 / 1.75, 1.75, 1.75),
    ];

    for (name, format) in formats {
        let data = encoded_sized_image(format, 5, 3);
        for size in sizes {
            for rotation in rotations {
                for realization in realizations {
                    let decoded = ImageDecoder::decode_data_with_metadata_at_full_realization(
                        &data,
                        size,
                        rotation,
                        (0xffff_ffff, 0),
                        realization,
                    )
                    .unwrap_or_else(|| panic!("{name} should decode"));
                    let probed = probe_image_layout(
                        ImageProbeSource::Data(&data),
                        size,
                        rotation,
                        realization,
                    )
                    .unwrap_or_else(|| panic!("{name} header should resolve"));

                    assert_eq!(
                        probed, decoded.metadata.layout,
                        "{name} header layout moved at decode ({size:?} {rotation:?} {realization:?})"
                    );
                }
            }
        }
    }
}

#[test]
fn header_layout_of_the_repository_image_fixture_equals_its_decode() {
    let fixture = neomacs_infra::workspace_root().join("test/data/image/blank-100x200.png");
    let path = fixture.to_str().expect("utf8 fixture path");
    let data = std::fs::read(&fixture).expect("fixture bytes");
    let size = ImageSizeSpec::new(AxisSize::AtMost(50), AxisSize::Native);
    let realization = ImageRealization::default();

    let decoded = ImageDecoder::decode_data_with_metadata_at_full_realization(
        &data,
        size,
        ImageRotation::None,
        (0xffff_ffff, 0),
        realization,
    )
    .expect("fixture should decode");
    let probed = probe_image_layout(
        ImageProbeSource::File(path),
        size,
        ImageRotation::None,
        realization,
    )
    .expect("fixture header should resolve");

    // 100x200 clamped to a 50px width keeps the aspect ratio: 50x100.
    assert_eq!(probed, decoded.metadata.layout);
    assert_eq!(probed.dimensions(), (50, 100));
}

#[test]
fn header_layout_of_a_selected_animation_frame_uses_the_canvas() {
    let data = animated_gif_bytes();

    let decoded = ImageDecoder::decode_data_with_metadata_for_frame(&data, ImageFrameIndex::new(1))
        .expect("frame 1 should decode");
    let probed = probe_image_layout(
        ImageProbeSource::Data(&data),
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageRealization::default(),
    )
    .expect("animated GIF header should resolve");

    assert_eq!(probed, decoded.metadata.layout);
}

#[test]
fn probe_declines_sources_it_cannot_measure_without_decoding() {
    let spec = ImageSizeSpec::default();
    let realization = ImageRealization::default();

    assert!(
        probe_image_layout(
            ImageProbeSource::Data(b"not an image at all"),
            spec,
            ImageRotation::None,
            realization
        )
        .is_none()
    );
    assert!(
        probe_image_layout(
            ImageProbeSource::File("/nonexistent/neomacs/probe.png"),
            spec,
            ImageRotation::None,
            realization
        )
        .is_none()
    );
    // A vector document's extent comes from parsing the document, including a
    // bounding-box fallback that depends on resolved resources and colors, so
    // it is not a header and is deliberately not probed.
    assert!(
        probe_image_layout(
            ImageProbeSource::Data(
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="3" height="1"/>"#
            ),
            spec,
            ImageRotation::None,
            realization
        )
        .is_none()
    );
}

fn varying_png(width: u32, height: u32) -> Vec<u8> {
    let pixels = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                [
                    (x % 251) as u8,
                    (y % 253) as u8,
                    ((x + y) % 241) as u8,
                    0xff,
                ]
            })
        })
        .collect();
    png_bytes(pixels, width, height)
}

fn whole_pixels(data: &[u8]) -> NativePixels {
    ImageDecoder::decode_whole(data).expect("fixture decodes whole")
}

fn decode_with_bands(
    data: &[u8],
    scale: f32,
    bands: &mut Vec<DecodedBand>,
) -> Option<DecodedPixels> {
    ImageDecoder::decode_data(
        data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageColorContext::default(),
        ImageRealization::with_device_scale(scale, scale),
        ImageMaskPolicy::Preserve,
        ImageFrameIndex::default(),
        crate::svg::SvgResourceContext::Isolated,
        &ImageSequenceCache::new(),
        ImageSequenceId::new(1).expect("non-zero test sequence"),
        Some(&mut |band| bands.push(band)),
    )
}

#[test]
fn a_large_png_decodes_in_bands_that_add_up_to_the_whole_image() {
    let (width, height) = (2000, 2000);
    let data = varying_png(width, height);
    let mut bands = Vec::new();

    let decoded = decode_with_bands(&data, 1.0, &mut bands).expect("decode");
    let raster = decoded.geometry.raster();

    assert!(
        bands.len() > 1,
        "a four-megapixel source must band, got {} band(s)",
        bands.len()
    );
    let mut expected_start = 0;
    for band in &bands {
        assert_eq!(band.source().start(), expected_start);
        assert_eq!(band.placed().placement().raster(), raster);
        assert_eq!(band.placed().placement().rows().start(), expected_start);
        expected_start = band.placed().placement().rows().end();
    }
    assert_eq!(expected_start, height, "the bands cover the whole raster");
    assert_eq!(
        decoded.rgba.len(),
        raster.width() as usize * raster.height() as usize * 4,
        "the decode ends as the raster, not as a native-size image"
    );
    assert_eq!(
        decoded.rgba,
        whole_pixels(&data).rgba,
        "a banded decode must publish the whole decode's pixels"
    );
}

#[test]
fn a_source_below_the_size_threshold_publishes_no_bands() {
    let data = varying_png(40, 30);
    let mut bands = Vec::new();

    let decoded = decode_with_bands(&data, 1.0, &mut bands).expect("decode");

    assert!(bands.is_empty(), "a small source has nothing to report");
    assert_eq!(decoded.rgba.len(), 40 * 30 * 4);
}

#[test]
fn the_threshold_is_the_boundary_between_the_two_paths() {
    let threshold = crate::image_bands::BANDING_MIN_PIXELS;
    let (below_width, below_height) = (1999, 2000);
    let (above_width, above_height) = (2000, 2000);
    assert!(u64::from(below_width) * u64::from(below_height) < threshold);
    assert!(u64::from(above_width) * u64::from(above_height) >= threshold);

    let mut below = Vec::new();
    decode_with_bands(&varying_png(below_width, below_height), 1.0, &mut below).expect("decode");
    let mut above = Vec::new();
    decode_with_bands(&varying_png(above_width, above_height), 1.0, &mut above).expect("decode");

    assert!(
        below.is_empty(),
        "a source under the threshold decodes whole"
    );
    assert!(
        !above.is_empty(),
        "a source at the threshold decodes in bands"
    );
}

#[test]
fn an_abandoned_banded_decode_still_yields_the_whole_image() {
    let (width, height) = (2000, 2000);
    let data = varying_png(width, height);
    let mut bands = Vec::new();

    let remaining = ABANDON_AFTER_BANDS.load(Ordering::Relaxed);
    ABANDON_AFTER_BANDS.store(1, Ordering::Relaxed);
    let decoded = decode_with_bands(&data, 1.0, &mut bands);
    ABANDON_AFTER_BANDS.store(remaining, Ordering::Relaxed);

    let decoded = decoded.expect("an abandoned attempt must still produce the image");
    assert_eq!(bands.len(), 1, "the attempt was abandoned mid-stream");
    assert_eq!(
        decoded.rgba,
        whole_pixels(&data).rgba,
        "the image must be whole, not the prefix the abandoned attempt reached"
    );
}

#[test]
fn a_truncated_source_fails_instead_of_publishing_a_prefix() {
    let (width, height) = (2000, 2000);
    let data = varying_png(width, height);
    let truncated = &data[..data.len() / 2];
    let mut bands = Vec::new();

    let decoded = decode_with_bands(truncated, 1.0, &mut bands);

    assert!(
        decoded.is_none(),
        "a decode that could not finish must not publish an image"
    );
    assert!(
        !bands.is_empty(),
        "the failure happened after the banded path had reported progress"
    );
    let covered = bands.last().map_or(0, |band| band.source().end());
    assert!(
        covered > 0 && covered < height,
        "the failure is mid-stream: {covered} of {height} rows"
    );
}

#[test]
fn every_band_lands_in_the_raster_the_finished_upload_resolves() {
    let (width, height) = (2000_u32, 2000_u32);
    let data = varying_png(width, height);
    let scales = [
        // Native size: the raster is the source, and every band is its own rows.
        // (layout scale and device scale both apply: 2000 stays 2000.)
        (1.0_f32, (2000_u32, 2000_u32)),
        // Halved twice over, and the raster is an eighth of the source: a ratio
        // whose output rows are made of several source rows rather than one.
        (0.5, (500, 500)),
    ];
    for (scale, expected) in scales {
        let realization = ImageRealization::with_device_scale(scale, scale);
        let mut bands = Vec::new();
        let decoded = ImageDecoder::decode_data(
            &data,
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageColorContext::default(),
            realization,
            ImageMaskPolicy::Preserve,
            ImageFrameIndex::default(),
            crate::svg::SvgResourceContext::Isolated,
            &ImageSequenceCache::new(),
            ImageSequenceId::new(1).expect("non-zero test sequence"),
            Some(&mut |band| bands.push(band)),
        )
        .expect("decode");
        assert!(!bands.is_empty(), "{width}x{height}@{scale} bands");

        let raster = decoded.geometry.raster();
        assert_eq!(raster.dimensions(), expected, "{width}x{height}@{scale}");
        assert_eq!(
            decoded.rgba.len(),
            raster.width() as usize * raster.height() as usize * 4,
            "the decode ends as the raster"
        );
        let mut expected_start = 0;
        for band in &bands {
            let placed = band.placed();
            assert_eq!(
                placed.placement().raster(),
                raster,
                "a band is mapped into the raster the image is realized to"
            );
            assert_eq!(
                placed.placement().rows().start(),
                expected_start,
                "bands tile the raster from row zero"
            );
            assert_eq!(
                placed.pixels().len(),
                raster.width() as usize * placed.placement().rows().len().get() as usize * 4,
                "a placed band carries exactly the rows it fills"
            );
            expected_start = placed.placement().rows().end();
        }
        assert_eq!(
            expected_start,
            raster.height(),
            "{width}x{height}@{scale}: the bands cover the whole raster"
        );
    }
}

#[test]
fn a_rotation_or_a_rewriting_mask_decodes_the_whole_image() {
    let (width, height) = (2000_u32, 2000_u32);
    let data = varying_png(width, height);
    let cases = [
        (
            "a quarter turn",
            ImageRotation::Quarter,
            ImageMaskPolicy::Preserve,
        ),
        (
            "no turn, a mask",
            ImageRotation::None,
            ImageMaskPolicy::Suppress,
        ),
        (
            "no turn, a heuristic mask",
            ImageRotation::None,
            ImageMaskPolicy::Heuristic(ImageHeuristicMask::FourCorners),
        ),
    ];
    for (name, rotation, mask) in cases {
        let mut bands = Vec::new();
        let decoded = ImageDecoder::decode_data(
            &data,
            ImageSizeSpec::default(),
            rotation,
            ImageColorContext::default(),
            ImageRealization::default(),
            mask,
            ImageFrameIndex::default(),
            crate::svg::SvgResourceContext::Isolated,
            &ImageSequenceCache::new(),
            ImageSequenceId::new(1).expect("non-zero test sequence"),
            Some(&mut |band| bands.push(band)),
        )
        .expect("decode");

        assert!(bands.is_empty(), "{name}: no band has a destination");
        let raster = decoded.geometry.raster();
        let turned = matches!(
            rotation,
            ImageRotation::Quarter | ImageRotation::ThreeQuarter
        );
        assert_eq!(
            raster.dimensions(),
            if turned {
                (height, width)
            } else {
                (width, height)
            },
            "{name}: the geometry is still resolved as it was"
        );
        assert_eq!(
            decoded.rgba.len(),
            raster.width() as usize * raster.height() as usize * 4,
            "{name}: the whole-image path still realizes the image"
        );
    }
}

#[test]
fn a_source_below_the_threshold_keeps_the_whole_image_paths_filter() {
    let (width, height) = (40_u32, 30_u32);
    assert!(
        u64::from(width) * u64::from(height) < crate::image_bands::BANDING_MIN_PIXELS,
        "the fixture is below the threshold"
    );
    let data = varying_png(width, height);
    let mut bands = Vec::new();
    let decoded = ImageDecoder::decode_data(
        &data,
        ImageSizeSpec::new(AxisSize::Exact(20), AxisSize::Exact(15)),
        ImageRotation::None,
        ImageColorContext::default(),
        ImageRealization::default(),
        ImageMaskPolicy::Preserve,
        ImageFrameIndex::default(),
        crate::svg::SvgResourceContext::Isolated,
        &ImageSequenceCache::new(),
        ImageSequenceId::new(1).expect("non-zero test sequence"),
        Some(&mut |band| bands.push(band)),
    )
    .expect("decode");
    assert!(bands.is_empty(), "a source this small does not band");

    let whole = image::load_from_memory(&data)
        .expect("the fixture decodes")
        .to_rgba8();
    let expected =
        image::imageops::resize(&whole, 20, 15, image::imageops::FilterType::Lanczos3).into_raw();
    assert_eq!(
        decoded.rgba, expected,
        "a source below the threshold is still resampled the old way"
    );
}

#[test]
fn the_rows_a_band_hands_over_are_the_rows_the_finished_image_holds() {
    let (width, height) = (2000_u32, 2000_u32);
    let data = varying_png(width, height);
    let mut bands = Vec::new();
    // A realization that really reduces: the raster is 500x500, so a band of
    // source rows has to be filtered rather than passed through.
    let decoded = ImageDecoder::decode_data(
        &data,
        ImageSizeSpec::new(AxisSize::Exact(500), AxisSize::Exact(500)),
        ImageRotation::None,
        ImageColorContext::default(),
        ImageRealization::default(),
        ImageMaskPolicy::Preserve,
        ImageFrameIndex::default(),
        crate::svg::SvgResourceContext::Isolated,
        &ImageSequenceCache::new(),
        ImageSequenceId::new(1).expect("non-zero test sequence"),
        Some(&mut |band| bands.push(band)),
    )
    .expect("decode");
    assert!(!bands.is_empty(), "a four-megapixel source bands");

    let raster = decoded.geometry.raster();
    assert_eq!(raster.dimensions(), (500, 500));
    let stride = raster.width() as usize * 4;
    let mut assembled = vec![0_u8; stride * raster.height() as usize];
    let mut expected_start = 0;
    for band in &bands {
        let rows = band.placed().placement().rows();
        assert_eq!(rows.start(), expected_start, "bands tile the raster");
        assembled[rows.start() as usize * stride..rows.end() as usize * stride]
            .copy_from_slice(band.placed().pixels());
        expected_start = rows.end();
    }
    assert!(
        expected_start == raster.height(),
        "the bands cover the raster"
    );
    assert_eq!(
        assembled, decoded.rgba,
        "a band is a slice of the finished pixels, not an approximation of them"
    );
}

#[test]
fn the_texture_a_banded_decode_fills_ends_as_the_whole_image_paths() {
    let (width, height) = (2000, 2000);
    let data = varying_png(width, height);
    let (size, realization) = (ImageSizeSpec::default(), ImageRealization::default());

    let mut bands = Vec::new();
    let decoded = decode_with_bands(&data, 1.0, &mut bands).expect("decode");
    let raster = decoded.geometry.raster();
    let (raster_width, raster_height) = raster.dimensions();

    // Stand in for the texture `ImageCache` fills band by band: the same buffer
    // shape, filled with a placeholder the fixture's own pixels cannot be —
    // transparent, where every pixel of `varying_png` is opaque — so a row
    // nobody wrote is visible.
    let mut texture = vec![0u8; raster_width as usize * raster_height as usize * 4];
    let stride = raster_width as usize * 4;
    let mut filled = FilledRows::empty(raster);
    for band in &bands {
        let placed = band.placed();
        let rows = placed.placement().rows();
        let advanced = filled
            .extend(rows)
            .expect("each band continues the rows already written");
        texture[rows.start() as usize * stride..rows.end() as usize * stride]
            .copy_from_slice(placed.pixels());
        filled = advanced;
    }
    assert!(
        filled.is_complete(),
        "the bands wrote every row of the texture"
    );
    for row in 0..raster_height as usize {
        let written = &texture[row * stride..(row + 1) * stride];
        assert!(
            written.chunks_exact(4).all(|texel| texel[3] != 0),
            "row {row} of the texture was never written"
        );
    }

    // The finished upload, as `ImageDecoder::upload_texture` runs it when the
    // decode completes: the whole-image realization, written over the whole
    // texture in one call.
    let whole = ImageDecoder::decode_whole(&data)
        .expect("fixture decodes whole")
        .realize_bitmap(
            size,
            ImageRotation::None,
            realization,
            ImageMaskPolicy::Preserve,
        )
        .expect("realize");
    assert_eq!(
        whole.geometry.raster(),
        raster,
        "the finished upload writes the same raster the bands were mapped into"
    );
    assert_eq!(
        whole.rgba.len(),
        texture.len(),
        "the finished upload's buffer is the whole texture"
    );
    assert_eq!(
        decoded.rgba, whole.rgba,
        "the banded decode ends as the whole-image path's pixels"
    );
    texture.copy_from_slice(&whole.rgba);
    // The oracle, stated without reference to any of the above: this fixture
    // realizes at its native size, so the whole-image path is `image`'s own
    // decode of the same file with no resample at all.
    let expected = image::load_from_memory(&data)
        .expect("fixture decodes")
        .to_rgba8()
        .into_raw();
    assert_eq!(
        texture, expected,
        "the finished texture holds the whole-image path's bytes"
    );
}

/// A JPEG of `width` x `height` carrying `pixels` as RGB.
///
/// `image` 0.25's `jpeg` feature decodes only, so the tests bring their own
/// encoder. Quality 85 selects the encoder's 2x2 chroma subsampling, which is
/// the 4:2:0 shape: one MCU row is sixteen output rows, so this is the case
/// where a band cannot be a single row.
fn jpeg_bytes(width: u32, height: u32, pixels: Vec<u8>, progressive: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, 85);
    encoder.set_progressive(progressive);
    encoder
        .encode(
            &pixels,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .expect("JPEG is encodable");
    bytes
}

/// A baseline JPEG of `width` x `height` whose every pixel differs, so a band
/// placed at the wrong offset is visible rather than hidden by identical rows.
fn varying_jpeg(width: u32, height: u32) -> Vec<u8> {
    let rgb: Vec<u8> = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| [(x % 251) as u8, (y % 253) as u8, ((x + y) % 241) as u8])
        })
        .collect();
    jpeg_bytes(width, height, rgb, false)
}

/// The same picture, encoded as a progressive frame.
fn varying_progressive_jpeg(width: u32, height: u32) -> Vec<u8> {
    let rgb: Vec<u8> = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| [(x % 251) as u8, (y % 253) as u8, ((x + y) % 241) as u8])
        })
        .collect();
    jpeg_bytes(width, height, rgb, true)
}

/// A large baseline JPEG takes the banded path, and the image it ends as is the
/// whole-image path's pixels, byte for byte.
///
/// This is the acceptance criterion for JPEG, stated the way the PNG one is: at
/// the source's own size the filter is the identity, so the finished raster is
/// `image`'s own decode of the same file and a band that lost, doubled or moved
/// a row would show as a difference rather than as a blur.
#[test]
fn a_large_baseline_jpeg_bands_that_add_up_to_the_whole_image() {
    let (width, height) = (2200_u32, 2000_u32);
    assert!(u64::from(width) * u64::from(height) >= crate::image_bands::BANDING_MIN_PIXELS);
    let data = varying_jpeg(width, height);
    let mut bands = Vec::new();

    let decoded = decode_with_bands(&data, 1.0, &mut bands).expect("decode");
    assert_eq!(decoded.geometry.raster().dimensions(), (width, height));

    assert!(
        bands.len() > 1,
        "a four-megapixel JPEG must band, got {} band(s)",
        bands.len()
    );
    let mut expected_start = 0;
    for band in &bands {
        let rows = band.placed().placement().rows();
        assert_eq!(rows.start(), expected_start, "bands tile the raster");
        expected_start = rows.end();
    }
    assert_eq!(expected_start, height, "the bands cover the raster");

    assert_eq!(
        decoded.rgba,
        whole_pixels(&data).rgba,
        "the banded decode ends as the whole-image path's pixels"
    );
    let expected = image::load_from_memory(&data)
        .expect("fixture decodes")
        .to_rgba8()
        .into_raw();
    assert_eq!(
        decoded.rgba, expected,
        "which at this realization is `image`'s own decode of the same file"
    );
}

/// The acceptance criterion at the shape a large JPEG actually has on screen: a
/// source wider than `MAX_TEXTURE_SIZE`, so the raster is the clamp and the
/// filter is the streamed Lanczos3 from `f7bc3b18f8` rather than the identity.
///
/// Two things are pinned. **The bands are slices of the finished pixels** —
/// they assemble into exactly the raster the decode ends with, so the preview
/// during a decode is not an approximation of the image but the part of it that
/// exists. And **the finished texture is the whole-image path's**: the same
/// kernel over the same source rows, differing only where two implementations
/// of one kernel round differently, by at most one level of 255 here.
///
/// That last clause is the same one the PNG arm carries and is not new here:
/// `RasterTarget` accumulates in `i32` and clamps once at the end, where
/// `image::imageops::resize` clamps per axis, so the two agree exactly wherever
/// the filter is the identity — which is what
/// `a_large_baseline_jpeg_bands_that_add_up_to_the_whole_image` pins byte for
/// byte — and differ by rounding at a reduced raster. The bound is asserted
/// rather than the equality so that a wrong kernel, a dropped band or a band at
/// the wrong offset, all of which move a channel by far more than one level,
/// still fail this test.
#[test]
fn a_clamped_baseline_jpeg_ends_as_the_whole_image_paths_texture() {
    let (width, height) = (4400_u32, 1000_u32);
    assert!(u64::from(width) * u64::from(height) >= crate::image_bands::BANDING_MIN_PIXELS);
    let data = varying_jpeg(width, height);
    let mut bands = Vec::new();

    let decoded = decode_with_bands(&data, 1.0, &mut bands).expect("decode");
    let raster = decoded.geometry.raster();
    assert!(
        raster.width() < width,
        "the source is wider than the texture limit, got a {}x{} raster",
        raster.width(),
        raster.height()
    );
    assert!(
        bands.len() > 1,
        "a four-megapixel source wider than the texture limit bands, got {} band(s)",
        bands.len()
    );

    let stride = raster.width() as usize * 4;
    let mut assembled = vec![0_u8; stride * raster.height() as usize];
    let mut expected_start = 0;
    for band in &bands {
        let rows = band.placed().placement().rows();
        assert_eq!(rows.start(), expected_start, "bands tile the raster");
        assembled[rows.start() as usize * stride..rows.end() as usize * stride]
            .copy_from_slice(band.placed().pixels());
        expected_start = rows.end();
    }
    assert_eq!(
        expected_start,
        raster.height(),
        "the bands cover the raster"
    );
    assert_eq!(
        assembled, decoded.rgba,
        "a band is a slice of the finished pixels, not an approximation of them"
    );

    let whole = ImageCache::decode_whole(EncodedBytes::copy_of(&data))
        .expect("the fixture decodes whole")
        .realize_bitmap(
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageRealization::default(),
            ImageMaskPolicy::Preserve,
        )
        .expect("realize");
    assert_eq!(whole.geometry.raster(), raster);
    let (mut differing, mut worst) = (0_usize, 0_i32);
    for (banded, whole) in decoded.rgba.iter().zip(&whole.rgba) {
        let delta = i32::from(*banded) - i32::from(*whole);
        if delta != 0 {
            differing += 1;
        }
        worst = worst.max(delta.abs());
    }
    assert!(
        worst <= 1,
        "the banded raster is {worst} levels from the whole-image path's; \
         one level is the filter's rounding, more is a different filter"
    );
    assert!(
        differing * 4 < decoded.rgba.len(),
        "{differing} of {} bytes differ, which is more than rounding",
        decoded.rgba.len()
    );
    tracing::debug!(
        differing,
        worst,
        width = raster.width(),
        height = raster.height(),
        "banded against whole at the clamped raster"
    );
}

/// A progressive JPEG has no usable band, so it takes the whole-image path —
/// and the picture it produces is the one a baseline encoding of the same
/// pixels produces, to within what the two encodings cost.
///
/// The point is not that the two are identical (they are not: two lossy
/// encodings of one picture are two pictures) but that neither is *wrong*: the
/// progressive frame decodes to the same dimensions and the same
/// `image::load_from_memory` bytes as every other path produces for it, which
/// is what a band taken from an unfinished scan would not.
#[test]
fn a_progressive_jpeg_decodes_through_the_whole_path() {
    let (width, height) = (2200_u32, 2000_u32);
    let data = varying_progressive_jpeg(width, height);
    let mut bands = Vec::new();

    let decoded = decode_with_bands(&data, 1.0, &mut bands).expect("decode");
    assert!(
        bands.is_empty(),
        "a progressive frame has no bands to publish, got {}",
        bands.len()
    );
    assert_eq!(decoded.geometry.raster().dimensions(), (width, height));
    assert_eq!(
        decoded.rgba,
        whole_pixels(&data).rgba,
        "a progressive frame decodes whole, like every other unbandable source"
    );
    let expected = image::load_from_memory(&data)
        .expect("fixture decodes")
        .to_rgba8()
        .into_raw();
    assert_eq!(decoded.rgba, expected);
}

/// JPEG takes the same size threshold PNG does: the same kind of source bands
/// on one side of it and decodes whole on the other.
#[test]
fn a_jpeg_below_the_threshold_publishes_no_bands() {
    let threshold = crate::image_bands::BANDING_MIN_PIXELS;
    // 4:2:0 rounds the MCU rows to a multiple of sixteen, so the fixtures are
    // chosen with that in mind rather than to make the arithmetic tidy.
    let (below_width, below_height) = (1999_u32, 1999_u32);
    let (above_width, above_height) = (2000_u32, 2000_u32);
    assert!(u64::from(below_width) * u64::from(below_height) < threshold);
    assert!(u64::from(above_width) * u64::from(above_height) >= threshold);

    let mut below = Vec::new();
    decode_with_bands(&varying_jpeg(below_width, below_height), 1.0, &mut below).expect("decode");
    let mut above = Vec::new();
    decode_with_bands(&varying_jpeg(above_width, above_height), 1.0, &mut above).expect("decode");

    assert!(below.is_empty(), "a JPEG under the threshold decodes whole");
    assert!(
        !above.is_empty(),
        "a JPEG at the threshold decodes in bands"
    );
}

#[path = "decode_diagnostic.rs"]
mod decode_diagnostic;

/// A diagnostic for tests that only exercise scheduling, not wording.
fn test_diagnostic() -> neomacs_display_protocol::image_diagnostic::ImageDiagnostic {
    neomacs_display_protocol::image_diagnostic::ImageDiagnostic::InvalidSize
}

fn test_load_identity() -> neomacs_display_protocol::image_diagnostic::ImageLoadIdentity {
    use neomacs_display_protocol::image_diagnostic::{ImageDiagnosticSubject, ImageFormatName};
    ImageLoadIdentity::new(
        ImageFormatName::Png,
        ImageDiagnosticSubject::File(String::new()),
    )
}
