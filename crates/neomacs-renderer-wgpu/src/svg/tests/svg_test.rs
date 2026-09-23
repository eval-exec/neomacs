use super::*;

#[test]
fn fractional_svg_sizing_matches_gnu_before_rasterization() {
    use neomacs_display_protocol::image::AxisSize::{Exact, Native};

    // Measured with GNU Emacs 31.1 by image-size-oracle.el. The native
    // path truncates before scaling; constrained axes retain the ratio.
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="52.910000000000004" height="17.75"/>"##;
    let intrinsic = query_intrinsic_extent(svg).expect("measure fractional SVG");
    for (size, scale, expected) in [
        (ImageSizeSpec::default(), 1.0, (52, 17)),
        (ImageSizeSpec::default(), 1.3, (68, 23)),
        (ImageSizeSpec::new(Exact(40), Native), 1.0, (40, 14)),
        (ImageSizeSpec::new(Native, Exact(12)), 1.0, (36, 12)),
        (ImageSizeSpec::new(Exact(1000), Native), 1.0, (1000, 336)),
        (ImageSizeSpec::new(Native, Exact(17)), 1.0, (51, 17)),
    ] {
        let decoded = decode(
            svg,
            size,
            ImageRotation::None,
            ImageRealization::with_device_scale(scale, 1.0),
            ImageColorContext::default(),
            SvgResourceContext::Isolated,
        )
        .expect("fractional SVG should decode");
        assert_eq!(decoded.geometry.layout().dimensions(), expected);
        assert_eq!(decoded.geometry.reported().dimensions(), expected);
        assert_eq!(decoded.geometry.raster().dimensions(), expected);
        assert_eq!(decoded.rgba.len(), (expected.0 * expected.1 * 4) as usize);
        let pending = ImageRealization::with_device_scale(scale, 1.0).resolve_geometry(
            size,
            intrinsic,
            ImageRotation::None,
        );
        assert_eq!(pending, decoded.geometry);
    }
}

/// Telega's `etc/symbols/reply.svg`: a default-black (no `fill`, no
/// `currentColor`) path, loaded with `:mask heuristic` like
/// `telega-etc-file-create-image` produces.
const REPLY_SVG: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 4.2333332 4.2333335" version="1.1" id="svg8">
  <g transform="translate(0,-292.76665)" id="layer1">
    <path style="stroke:none;stroke-width:0.31644347px;stroke-linecap:butt;stroke-linejoin:miter;stroke-opacity:1" d="m 3.2522301,295.81981 c -0.060907,-0.54693 -0.1986797,-0.65348 -0.4169559,-0.6559 -0.3938777,-0.004 -1.3128545,9e-5 -1.3128545,9e-5 v 0.42324 l -1.33166282,-0.82011 1.33166282,-0.82031 v 0.42343 c 0,0 1.2007043,-0.0295 1.6660151,-9e-5 0.4653107,0.0294 0.3270887,0.26026 0.3678221,1.44965" id="path817"/>
  </g>
</svg>"#;

/// REGRESSION (telega reply icon invisible on dark backgrounds): a
/// backgroundless SVG whose path has the default black fill must still
/// be painted over the face background, like GNU's wrapper rect does
/// (src/image.c:12344).  `:mask heuristic` samples the four corners as
/// its background color: without the injected rect the corners are
/// transparent black, the mask classifies the black path as background,
/// and the whole icon vanishes.
#[test]
fn face_background_rect_keeps_default_black_paths_masked_against_face_bg() {
    let colors = ImageColorContext::from_pixels(0x1885f3, 0x333333);
    let decoded = decode(
        REPLY_SVG.as_bytes(),
        ImageSizeSpec::new(
            neomacs_display_protocol::image::AxisSize::AtMost(32),
            neomacs_display_protocol::image::AxisSize::AtMost(18),
        ),
        ImageRotation::None,
        ImageRealization::default(),
        colors,
        SvgResourceContext::Isolated,
    )
    .expect("decode reply svg");

    let (w, h) = decoded.geometry.raster().dimensions();
    let (w, h) = (w as usize, h as usize);
    let pixel = |x: usize, y: usize| {
        let base = (y * w + x) * 4;
        [
            decoded.rgba[base],
            decoded.rgba[base + 1],
            decoded.rgba[base + 2],
            decoded.rgba[base + 3],
        ]
    };
    // GNU's wrapper rect: the corners carry the face background, so the
    // heuristic mask selects it — not transparent black — as background.
    for corner in [(0usize, 0usize), (w - 1, 0), (w - 1, h - 1), (0, h - 1)] {
        let px = pixel(corner.0, corner.1);
        assert_eq!(
            px,
            [0x33, 0x33, 0x33, 255],
            "corner {corner:?} must be the opaque face background"
        );
    }
    // The default-black path itself stays opaque: a GNU-style heuristic
    // mask (pixel != corner color) keeps every black pixel drawable.
    let black_opaque = decoded
        .rgba
        .chunks_exact(4)
        .filter(|px| px[3] > 0 && px[..3] == [0, 0, 0])
        .count();
    assert!(
        black_opaque > 0,
        "the default-black path must survive as opaque pixels"
    );
}
