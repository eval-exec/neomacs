use super::*;
use crate::font_backend::FontSelectionSize;
use neomacs_display_protocol::geometry::DeviceScale;

fn size(logical_px: f32, scale: f32) -> FontSelectionSize {
    FontSelectionSize::new(logical_px, DeviceScale::new(scale).expect("fixture scale"))
}

#[test]
fn exact_device_request_uses_realized_size_and_preserves_selector() {
    let identity = ResolvedFontIdentity::from_file("/fixture/font.ttf", 0x0002_0003, None);
    let request = ExactDeviceFontRequest::new(&identity, size(10.5, 1.0)).unwrap();
    assert_eq!(request.selector.0, 0x0002_0003);
    assert_eq!(request.size, DeviceFontPixelSize::new(11).unwrap());
    let scaled = ExactDeviceFontRequest::new(&identity, size(10.5, 1.75)).unwrap();
    assert_eq!(scaled.size, DeviceFontPixelSize::new(18).unwrap());
}

#[test]
fn exact_device_request_refuses_foreign_backend_and_unreplayed_axis() {
    let identity = ResolvedFontIdentity::from_platform_file_with_variations(
        FontBackendKind::CoreText,
        "/fixture/font.ttf",
        0,
        None,
        vec![],
    );
    assert!(matches!(
        ExactDeviceFontRequest::new(&identity, size(14.0, 1.0)),
        Err(DeviceFontProbeError::UnsupportedBackend(
            FontBackendKind::CoreText
        ))
    ));
    let coordinate = FontVariationCoord::try_new(u32::from_be_bytes(*b"wdth"), 75.0).unwrap();
    let identity = ResolvedFontIdentity::from_file_with_variations(
        "/fixture/font.ttf",
        0,
        None,
        vec![coordinate],
    );
    assert!(
        matches!(ExactDeviceFontRequest::new(&identity, size(14.0, 1.0)),
        Err(DeviceFontProbeError::UnsupportedVariation(value)) if value == coordinate)
    );
}

#[test]
fn device_glyph_zero_advance_is_present_and_device_conversion_occurs_once() {
    let zero = DeviceGlyphAdvance {
        device_pixel_size: DeviceFontPixelSize::new(18).unwrap(),
        advance_px: 0,
    };
    assert_eq!(zero.logical_advance(DeviceScale::new(1.75).unwrap()), 0.0);
    let measured = DeviceGlyphAdvance {
        device_pixel_size: DeviceFontPixelSize::new(18).unwrap(),
        advance_px: 11,
    };
    assert_eq!(
        measured.logical_advance(DeviceScale::new(1.75).unwrap()),
        11.0 / 1.75
    );
}

#[cfg(target_os = "linux")]
#[test]
fn selected_nerd_icon_and_ascii_share_native_grid_at_explicit_realized_sizes() {
    let file = neomacs_test_fonts::jetbrains_mono_nerd_regular_3_4_0()
        .to_str()
        .unwrap();
    let identity = ResolvedFontIdentity::from_file(file, 0, None);
    // GNU explicit native size controls: pixel-size14 gives8; pixel-size11 gives7.
    // This does not assert how GNU rounds a relative face height before opening.
    for (logical_size, expected) in [(14.0, 8.0), (11.0, 7.0)] {
        let request = ExactDeviceFontRequest::new(&identity, size(logical_size, 1.0)).unwrap();
        let ascii = probe_device_ascii_advances_for_request(&request).unwrap();
        let DeviceGlyphProbe::Present(icon) =
            probe_device_glyph_advance(&request, '\u{f48a}').unwrap()
        else {
            panic!("pinned Nerd fixture must contain the icon")
        };
        assert_eq!(
            icon.logical_advance(DeviceScale::new(1.0).unwrap()),
            expected
        );
        assert_eq!(
            icon.logical_advance(DeviceScale::new(1.0).unwrap()),
            ascii
                .logical_advance(' ', DeviceScale::new(1.0).unwrap())
                .unwrap()
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn missing_unicode_glyph_is_not_ascii_glyph_zero_fallback() {
    let file = neomacs_test_fonts::jetbrains_mono_nerd_regular_3_4_0()
        .to_str()
        .unwrap();
    let identity = ResolvedFontIdentity::from_file(file, 0, None);
    let request = ExactDeviceFontRequest::new(&identity, size(14.0, 1.0)).unwrap();
    assert_eq!(
        probe_device_glyph_advance(&request, '\u{10ffff}').unwrap(),
        DeviceGlyphProbe::MissingGlyph
    );
}

#[cfg(target_os = "linux")]
#[test]
fn selected_device_probe_propagates_font_open_failure() {
    let identity = ResolvedFontIdentity::from_file("/no-such-font-cont47/font.ttf", 0, None);
    let request = ExactDeviceFontRequest::new(&identity, size(14.0, 1.0)).unwrap();
    assert!(matches!(
        probe_device_glyph_advance(&request, 'x'),
        Err(DeviceFontProbeError::FreeType(_))
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn native_combining_mark_zero_advance_remains_present() {
    let file = neomacs_test_fonts::noto_sans_variable().to_str().unwrap();
    let identity = ResolvedFontIdentity::from_file(file, 0, None);
    let request = ExactDeviceFontRequest::new(&identity, size(14.0, 1.0)).unwrap();
    let DeviceGlyphProbe::Present(mark) = probe_device_glyph_advance(&request, '\u{0301}').unwrap()
    else {
        panic!("pinned Noto fixture must contain combining acute")
    };
    assert_eq!(mark.logical_advance(DeviceScale::new(1.0).unwrap()), 0.0);
}

#[test]
fn ascii_identity_adapter_preserves_weight_only_best_effort_replay() {
    let width = FontVariationCoord::try_new(u32::from_be_bytes(*b"wdth"), 75.0).unwrap();
    let weight = FontVariationCoord::try_new(u32::from_be_bytes(*b"wght"), 500.0).unwrap();
    let identity = ResolvedFontIdentity::from_file_with_variations(
        "/fixture/font.ttf",
        0x0002_0003,
        None,
        vec![width, weight],
    );
    let request = ExactDeviceFontRequest::for_ascii(&identity, size(14.0, 1.0)).unwrap();
    assert_eq!(request.selector.0, 0x0002_0003);
    assert!(matches!(request.weight, DeviceFontWeight::LegacyBestEffort(value) if value == 500.0));
    assert!(
        matches!(ExactDeviceFontRequest::new(&identity, size(14.0, 1.0)),
        Err(DeviceFontProbeError::UnsupportedVariation(value)) if value == width)
    );
}

#[test]
fn device_font_size_rejects_zero_at_the_single_validation_boundary() {
    assert!(matches!(
        DeviceFontPixelSize::new(0),
        Err(DeviceFontProbeError::InvalidSize)
    ));
    assert_eq!(DeviceFontPixelSize::new(1).unwrap().get(), 1);
}
