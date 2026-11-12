use super::super::*;
use neovm_core::emacs_core::display_host::{FontOpeningSize, PositiveFontScalar};

#[test]
fn listing_zero_size_remains_zero_while_font_opening_stays_usable() {
    let sizing = FontSizing::gnu_x11_fallback();
    assert_eq!(
        font_listing_pixel_size(sizing, FontOpeningSize::SmallestUsable),
        0
    );
    assert_eq!(
        sizing
            .font_opening_size_px(FontOpeningSize::SmallestUsable)
            .get(),
        1
    );
}

#[test]
fn listing_point_sizes_can_round_to_zero_without_changing_positive_pixel_sizes() {
    let sizing = FontSizing::gnu_x11_fallback();
    let points = |value| FontOpeningSize::Points {
        size: PositiveFontScalar::new(value).unwrap(),
        dpi: std::num::NonZeroU32::new(96),
    };
    assert_eq!(font_listing_pixel_size(sizing, points(0.1)), 0);
    assert_eq!(font_listing_pixel_size(sizing, points(12.0)), 16);
    assert_eq!(
        font_listing_pixel_size(
            sizing,
            FontOpeningSize::Pixels(std::num::NonZeroU32::new(16).unwrap())
        ),
        16
    );
}

#[test]
fn font_query_preserves_explicit_empty_family_as_a_restriction() {
    use neovm_core::emacs_core::eval::FontSpecSelection;
    let mut request = FontSpecResolveRequest {
        selection: FontSpecSelection::Enumerate,
        frame_id: neovm_core::window::FrameId(1),
        family: None,
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
    };
    let omitted = font_listing_entity_query(&request);
    assert!(omitted.is_some());
    for empty in ["", "   "] {
        request.family = Some(LispString::from_utf8(empty));
        assert_ne!(
            font_listing_entity_query(&request),
            omitted,
            "an explicit empty family must never become an unrestricted all-family query"
        );
    }
}
