use crate::{
    ImageBackgroundPolicy, ImageCacheUsage, ImageColorContext, ImageId, ImageLoadAttempt,
    ImageLoadToken, ImageNativeExtent, ImageRealization, ImageRgb, ImageRotation, ImageSequenceId,
    ImageSizeSpec, ImageStateEvent,
};

#[test]
fn image_realization_resolves_each_geometry_space_without_crossing_units() {
    let native = ImageNativeExtent::new(40, 20);
    let realization = ImageRealization::new(0.75, 2.0, 4.0 / 3.0);

    let geometry =
        realization.resolve_geometry(ImageSizeSpec::default(), native, ImageRotation::Quarter);

    assert_eq!(geometry.layout().dimensions(), (15, 30));
    assert_eq!(geometry.reported().dimensions(), (20, 40));
    assert_eq!(geometry.raster().dimensions(), (30, 60));
}

#[test]
fn decode_completion_carries_the_exact_load_attempt() {
    let load = ImageLoadToken::new(
        ImageId::new(17),
        ImageLoadAttempt::new(3).expect("non-zero attempt"),
    );

    let event = ImageStateEvent::DecodeCompleted(load);

    assert_eq!(event.image(), ImageId::new(17));
    assert_eq!(event, ImageStateEvent::DecodeCompleted(load));
}

#[test]
fn image_sequence_identity_excludes_the_retirement_sentinel() {
    assert!(ImageSequenceId::new(0).is_none());
    assert_eq!(ImageSequenceId::new(7).map(ImageSequenceId::get), Some(7));
}

#[test]
fn image_cache_usage_keeps_texture_and_sequence_domains_distinct() {
    let usage = ImageCacheUsage::new(4_096, 768);

    assert_eq!(usage.texture_bytes(), 4_096);
    assert_eq!(usage.decoded_sequence_bytes(), 768);
    assert_eq!(usage.total_bytes(), 4_864);
}

#[test]
fn image_rgb_preserves_black_as_a_real_opaque_color() {
    let black = ImageRgb::from_pixel(0x0000_0000);

    assert_eq!(black.rgb24(), 0x0000_0000);
    assert_eq!(black.rgb8(), [0, 0, 0]);
    assert_eq!(black.rgba8(), [0, 0, 0, 0xff]);
    assert_eq!(
        ImageRgb::from_pixel(0xaa_12_34_56).rgb8(),
        [0x12, 0x34, 0x56]
    );
}

#[test]
fn image_color_context_keeps_foreground_and_background_roles_distinct() {
    let colors = ImageColorContext::from_pixels(0xaa_12_34_56, 0xbb_65_43_21);

    assert_eq!(colors.foreground().rgb24(), 0x12_34_56);
    assert_eq!(colors.background().rgb24(), 0x65_43_21);
    // The frame foreground defaults to the face's own; a caller that knows the
    // frame -- the layout path, whose face basis carries the default face --
    // replaces it (issue #550).
    assert_eq!(colors.frame_foreground().rgb24(), 0x12_34_56);
    assert_eq!(
        colors
            .with_frame_foreground(0x00_ab_cd_ef)
            .frame_foreground()
            .rgb8(),
        [0xab, 0xcd, 0xef]
    );
    assert_eq!(colors.frame_background().rgb24(), 0x65_43_21);
    assert_eq!(colors.specified_background(), None);
    let with_frame_and_spec = colors
        .with_frame_background(0x00_fe_dc_ba)
        .with_specified_background(0x00_11_22_33);
    assert_eq!(
        with_frame_and_spec.frame_background().rgb8(),
        [0xfe, 0xdc, 0xba]
    );
    assert_eq!(
        with_frame_and_spec
            .specified_background()
            .map(ImageRgb::rgb8),
        Some([0x11, 0x22, 0x33])
    );
}

#[test]
fn gif_background_helpers_follow_gnu_frame_and_specified_background_precedence() {
    let base = ImageColorContext::from_pixels(0x00_10_20_30, 0x00_aa_bb_cc)
        .with_frame_background(0x00_ee_dd_cc);
    assert_eq!(base.gif_frame_background_rgba8(), [0xee, 0xdd, 0xcc, 0xff]);
    assert_eq!(
        base.gif_transparent_background_rgba8(),
        [0xee, 0xdd, 0xcc, 0xff],
        "without :background, GIF transparency uses frame background rather than face background"
    );

    let with_spec = base.with_specified_background(0x00_11_22_33);
    assert_eq!(
        with_spec.gif_frame_background_rgba8(),
        [0xee, 0xdd, 0xcc, 0xff],
        "borders outside frame 0 still use frame background even when :background is specified"
    );
    assert_eq!(
        with_spec.gif_transparent_background_rgba8(),
        [0x11, 0x22, 0x33, 0xff],
        "explicit :background overrides frame background for transparent GIF pixels"
    );

    let chrome = with_spec.with_background_policy(ImageBackgroundPolicy::Transparent);
    assert_eq!(chrome.gif_frame_background_rgba8(), [0, 0, 0, 0]);
    assert_eq!(chrome.gif_transparent_background_rgba8(), [0, 0, 0, 0]);
}

/// Issue #556: an absent `frame_foreground` on the wire means the face
/// foreground, as `from_pixels` documents -- not black, which a plain
/// `#[serde(default)]` field silently produced. Likewise an absent
/// `frame_background` reads as the face background.
#[test]
fn an_absent_wire_frame_foreground_reads_as_the_face_foreground() {
    let colors = ImageColorContext::from_pixels(0x00_11_22_33, 0x00_44_55_66);
    let mut wire = serde_json::to_value(colors).expect("serialize");
    let map = wire.as_object_mut().expect("an object");
    map.remove("frame_foreground");
    map.remove("frame_background");
    map.remove("specified_background");

    let decoded: ImageColorContext = serde_json::from_value(wire).expect("deserialize");
    assert_eq!(decoded.foreground().rgb24(), 0x11_22_33);
    assert_eq!(
        decoded.frame_foreground().rgb24(),
        0x11_22_33,
        "the absence means the face foreground, not black"
    );
    assert_eq!(decoded.background().rgb24(), 0x44_55_66);
    assert_eq!(
        decoded.frame_background().rgb24(),
        0x44_55_66,
        "the absence means the face background, not black"
    );
    assert_eq!(decoded.specified_background(), None);
}

/// With the fields present, the values survive the round trip.
#[test]
fn an_explicit_wire_frame_foreground_survives_the_round_trip() {
    let colors = ImageColorContext::from_pixels(0x00_11_22_33, 0x00_44_55_66)
        .with_frame_foreground(0x00_12_34_56)
        .with_frame_background(0x00_65_43_21)
        .with_specified_background(0x00_aa_bb_cc);

    let json = serde_json::to_string(&colors).expect("serialize");
    let decoded: ImageColorContext = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded, colors);
}

#[test]
fn unresolved_image_color_context_preserves_the_visible_monochrome_fallback() {
    let colors = ImageColorContext::default();

    assert_eq!(colors.foreground().rgb24(), 0x00ff_ffff);
    assert_eq!(colors.background().rgb24(), 0x0000_0000);
}
