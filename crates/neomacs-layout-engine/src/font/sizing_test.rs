use super::{
    FontSizing, FrameScaleSource, LogicalFontScale, ScalePolicy, points_to_layout_pixels,
    resolve_frame_scale,
};
use neomacs_display_protocol::{
    DeviceScale, DisplayGeometry, DisplayObservation, Dpi, X11DisplayObservation, XServerKind,
};
use neovm_core::emacs_core::display_host::FrameFontSize;

fn x11_observation(
    server: XServerKind,
    xft_dpi: Option<f32>,
    geometry: Option<(u32, u32)>,
) -> DisplayObservation {
    DisplayObservation::X11(X11DisplayObservation::new(
        server,
        xft_dpi.map(|dpi| Dpi::new(dpi).expect("valid test DPI")),
        geometry.map(|(height_px, height_mm)| {
            DisplayGeometry::new(height_px, height_mm).expect("valid test geometry")
        }),
        DeviceScale::ONE,
    ))
}

#[test]
fn cocoa_points_are_cocoa_logical_units() {
    let sizing = FontSizing::new(LogicalFontScale::GnuCocoaPoint);

    assert_eq!(sizing.face_height_to_layout_pixels(100), 10.0);
    assert_eq!(sizing.face_height_to_layout_pixels(120), 12.0);
    assert_eq!(sizing.layout_dpi(), 72.27);
}

#[test]
fn platform_point_rules_are_explicit_and_distinct() {
    let cocoa = FontSizing::new(LogicalFontScale::GnuCocoaPoint);
    let windows = FontSizing::new(LogicalFontScale::WindowsDip);
    let x11 = FontSizing::new(LogicalFontScale::X11 {
        effective_dpi: 100.0,
    });

    assert_eq!(cocoa.face_height_to_layout_pixels(100), 10.0);
    assert_eq!(windows.face_height_to_layout_pixels(100), 13.0);
    assert_eq!(x11.face_height_to_layout_pixels(100), 14.0);
}

#[test]
fn point_conversion_uses_gnu_printer_points() {
    assert_eq!(points_to_layout_pixels(22.0, 100.0), 30.0);
}

#[test]
fn pixel_font_requests_keep_the_same_logical_size_on_every_frontend() {
    let requested = FrameFontSize::pixels(15).expect("positive pixel size");

    for sizing in [
        FontSizing::new(LogicalFontScale::GnuCocoaPoint),
        FontSizing::new(LogicalFontScale::WindowsDip),
        FontSizing::new(LogicalFontScale::WaylandLogical),
        FontSizing::new(LogicalFontScale::X11 {
            effective_dpi: 100.0,
        }),
    ] {
        assert_eq!(
            sizing
                .font_size_px_for_request(requested)
                .expect("pixel request remains representable")
                .get(),
            15.0
        );
    }
}

#[test]
fn point_font_requests_follow_each_frontends_logical_dpi() {
    let requested = FrameFontSize::points(15.0).expect("representable point size");

    for (sizing, expected_pixels) in [
        (FontSizing::new(LogicalFontScale::GnuCocoaPoint), 15.0),
        (FontSizing::new(LogicalFontScale::WindowsDip), 20.0),
        (FontSizing::new(LogicalFontScale::WaylandLogical), 20.0),
        (
            FontSizing::new(LogicalFontScale::X11 {
                effective_dpi: 100.0,
            }),
            21.0,
        ),
    ] {
        assert_eq!(
            sizing
                .font_size_px_for_request(requested)
                .expect("point request remains representable")
                .get(),
            expected_pixels
        );
    }
}

#[test]
fn request_conversion_rejects_zero_or_infinite_logical_pixels() {
    let cocoa = FontSizing::new(LogicalFontScale::GnuCocoaPoint);
    let subpixel_point = FrameFontSize::points(0.1).expect("representable point selector");
    let overflowing_relative =
        FrameFontSize::relative(1.0e300).expect("finite positive relative selector");

    assert_eq!(cocoa.font_size_px_for_request(subpixel_point), None);
    assert_eq!(cocoa.font_size_px_for_request(overflowing_relative), None);
}

#[test]
fn realized_pixels_round_trip_through_frame_specific_face_heights() {
    for (sizing, expected_height_tenths) in [
        (FontSizing::new(LogicalFontScale::GnuCocoaPoint), 150),
        (FontSizing::new(LogicalFontScale::WindowsDip), 113),
        (FontSizing::new(LogicalFontScale::WaylandLogical), 113),
        (
            FontSizing::new(LogicalFontScale::X11 {
                effective_dpi: 100.0,
            }),
            108,
        ),
    ] {
        let height_tenths = sizing.face_height_tenths_for_layout_pixels(15);
        assert_eq!(height_tenths, expected_height_tenths);
        assert_eq!(sizing.face_height_to_layout_pixels(height_tenths), 15.0);
    }
}

#[test]
fn automatic_xorg_resolution_preserves_gnu_geometry_without_a_plausibility_clamp() {
    let profile = resolve_frame_scale(
        x11_observation(XServerKind::Xorg, None, Some((1080, 800))),
        ScalePolicy::Automatic,
    );

    assert!((profile.font_sizing().layout_dpi() - 34.29).abs() < 0.01);
    assert_eq!(profile.source(), FrameScaleSource::X11Geometry);
}

#[test]
fn automatic_xwayland_resolution_uses_logical_dpi_instead_of_physical_geometry() {
    let profile = resolve_frame_scale(
        x11_observation(XServerKind::Xwayland, None, Some((1080, 800))),
        ScalePolicy::Automatic,
    );

    assert_eq!(profile.font_sizing().layout_dpi(), 96.0);
    assert_eq!(profile.source(), FrameScaleSource::XwaylandLogicalFallback);
}

#[test]
fn explicit_xft_resource_remains_authoritative_on_xwayland() {
    let profile = resolve_frame_scale(
        x11_observation(XServerKind::Xwayland, Some(144.0), Some((1080, 800))),
        ScalePolicy::Automatic,
    );

    assert_eq!(profile.font_sizing().layout_dpi(), 144.0);
    assert_eq!(profile.source(), FrameScaleSource::XftResource);
}

#[test]
fn strict_gnu_policy_can_retain_xwayland_geometry_semantics() {
    let profile = resolve_frame_scale(
        x11_observation(XServerKind::Xwayland, None, Some((1080, 800))),
        ScalePolicy::StrictGnu,
    );

    assert!((profile.font_sizing().layout_dpi() - 34.29).abs() < 0.01);
    assert_eq!(profile.source(), FrameScaleSource::X11Geometry);
}

#[test]
fn explicit_policy_is_validated_and_overrides_display_observations() {
    let explicit = Dpi::new(120.0).expect("valid explicit DPI");
    let profile = resolve_frame_scale(
        x11_observation(XServerKind::Xwayland, Some(144.0), Some((1080, 800))),
        ScalePolicy::Explicit(explicit),
    );

    assert_eq!(profile.font_sizing().layout_dpi(), 120.0);
    assert_eq!(profile.source(), FrameScaleSource::ExplicitPolicy);
}

#[test]
fn missing_x11_geometry_preserves_gnu_100_dpi_fallback() {
    let profile = resolve_frame_scale(
        x11_observation(XServerKind::Xorg, None, None),
        ScalePolicy::Automatic,
    );

    assert_eq!(profile.font_sizing().layout_dpi(), 100.0);
    assert_eq!(profile.source(), FrameScaleSource::GnuX11Fallback);
}

#[test]
fn non_x11_frontends_keep_their_native_logical_rules_and_device_scale() {
    let device_scale = DeviceScale::new(2.0).expect("valid device scale");
    let profile = resolve_frame_scale(
        DisplayObservation::Wayland { device_scale },
        ScalePolicy::Automatic,
    );

    assert_eq!(profile.font_sizing().layout_dpi(), 96.0);
    assert_eq!(profile.device_scale(), device_scale);
    assert_eq!(profile.source(), FrameScaleSource::PlatformLogical);
}
