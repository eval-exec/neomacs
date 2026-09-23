// The previous `window_cursor_visual_match_uses_slot_identity` test covered the
// phys/visual dedup helper, which no longer exists: cursors are unified into a
// single per-window list (the selected window's entry is `active`), so the
// content backend draws every entry without deduplicating against a separate
// phys_cursor. There is nothing backend-specific left to assert here.

use super::stretch_decoration_rects;
use neomacs_display_protocol::face::{Face, FaceAttributes, UnderlineStyle};
use neomacs_display_protocol::{Color, ColorStop, FaceId, Gradient, Rect};

#[test]
fn child_subpixel_gradient_sampling_uses_face_paint_domain() {
    let mut face = Face::new(FaceId::new(3));
    face.background_gradient = Some(Box::new(Gradient::Linear {
        angle: 0.0,
        stops: vec![
            ColorStop::new(0.0, Color::RED),
            ColorStop::new(1.0, Color::BLUE),
        ],
    }));
    let domain = Rect::new(0.0, 0.0, 100.0, 10.0);
    let output_clip = Rect::new(50.0, 0.0, 50.0, 10.0);
    let paint = super::super::pointer_override::FacePaint::new(face.id, domain, Some(output_clip));

    let sampled =
        super::super::WgpuRenderer::sample_face_paint_background(Some(&face), None, paint);
    let domain_sample = super::super::WgpuRenderer::sample_face_background(
        Some(&face),
        None,
        domain.x,
        domain.y,
        domain.width,
        domain.height,
        None,
    );
    let reanchored = super::super::WgpuRenderer::sample_face_background(
        Some(&face),
        None,
        domain.x,
        domain.y,
        domain.width,
        domain.height,
        Some(&output_clip),
    );

    assert_eq!(sampled, domain_sample);
    assert_ne!(sampled, reanchored);
}

#[test]
fn child_stretch_decorations_follow_the_effective_face() {
    let mut face = Face::new(FaceId::new(4));
    face.attributes =
        FaceAttributes::UNDERLINE | FaceAttributes::OVERLINE | FaceAttributes::STRIKE_THROUGH;
    face.underline_style = UnderlineStyle::Double;
    face.foreground = Color::RED;
    face.font_ascent = 8;
    face.underline_placement = neomacs_display_protocol::face::UnderlinePosition::FontMetric {
        offset_from_baseline: 1,
    };
    face.underline_thickness = 1;

    let rects = stretch_decoration_rects(&face, 10.0, 5.0, 20.0);

    assert_eq!(rects.len(), 4, "double underline plus overline and strike");
    assert!(
        rects
            .iter()
            .all(|rect| rect.x >= 10.0 && rect.x + rect.width <= 30.0)
    );
    assert!(rects.iter().all(|rect| rect.color == Color::RED));
}

/// Regression guard for the glyph-rasterization hotspot fix.
///
/// The atlas cache key embeds subpixel bins. Deriving them from the fractional
/// glyph origin (the old `SubpixelBin::new`) made the key position-dependent, so
/// every column and every fractional scroll offset re-rasterized glyphs (~173
/// misses + GPU uploads per frame during scrolling, ~25% of total CPU).
/// `snap_glyph_origin` must place glyphs on whole pixels with `Zero` bins so the
/// key is position-invariant and each glyph rasterizes once.
#[test]
fn snap_glyph_origin_is_position_invariant_and_pixel_snapped() {
    use super::snap_glyph_origin;
    use cosmic_text::SubpixelBin;

    // Bins are always Zero, for any fractional offset within and across pixels.
    for &f in &[
        0.0_f32, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 12.3, 12.8, -3.2,
    ] {
        let (_, _, x_bin, y_bin) = snap_glyph_origin(f, f);
        assert_eq!(x_bin, SubpixelBin::Zero, "x_bin must be Zero for {f}");
        assert_eq!(y_bin, SubpixelBin::Zero, "y_bin must be Zero for {f}");
    }

    // Integer parts snap to the nearest whole pixel.
    assert_eq!(snap_glyph_origin(10.4, 20.6).0, 10);
    assert_eq!(snap_glyph_origin(10.4, 20.6).1, 21);
    assert_eq!(snap_glyph_origin(10.6, 20.4).0, 11);

    // Position-invariance: two origins in the same pixel produce identical
    // (int, bin) tuples, so the derived atlas key is identical -> one raster.
    assert_eq!(
        snap_glyph_origin(30.05, 40.05),
        snap_glyph_origin(30.45, 40.45)
    );
    // ...and a fractional scroll drift that stays within a pixel does not change
    // the key (this is precisely what used to thrash the atlas while scrolling).
    assert_eq!(
        snap_glyph_origin(100.0, 200.1),
        snap_glyph_origin(100.0, 200.3)
    );
}
