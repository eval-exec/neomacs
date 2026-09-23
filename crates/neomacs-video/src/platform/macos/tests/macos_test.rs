use super::*;

#[test]
fn source_depth_and_range_select_the_matching_core_video_surface() {
    let output = select_mac_output_format(
        true,
        MacSourceMetadata {
            bits_per_component: Some(10),
            full_range: true,
        },
    );

    assert_eq!(
        output.frame_format(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)
    );
    assert_eq!(output.range, VideoColorRange::Full);
    assert_eq!(
        output.core_video_pixel_format(),
        kCVPixelFormatType_420YpCbCr10BiPlanarFullRange
    );
}

#[test]
fn unsupported_ten_bit_sampling_selects_a_supported_surface_format() {
    let output = select_mac_output_format(
        false,
        MacSourceMetadata {
            bits_per_component: Some(10),
            full_range: false,
        },
    );

    assert_eq!(
        output.frame_format(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
    );
    assert_eq!(output.range, VideoColorRange::Limited);
}

#[test]
fn ordinary_eight_bit_video_selects_nv12() {
    let output = select_mac_output_format(
        true,
        MacSourceMetadata {
            bits_per_component: Some(8),
            full_range: false,
        },
    );

    assert_eq!(
        output.frame_format(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
    );
}

#[test]
fn unknown_source_depth_selects_the_eight_bit_surface() {
    let output = select_mac_output_format(
        true,
        MacSourceMetadata {
            bits_per_component: None,
            full_range: false,
        },
    );

    assert_eq!(
        output.frame_format(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
    );
}

#[test]
fn twelve_bit_source_selects_the_supported_p010_surface() {
    let output = select_mac_output_format(
        true,
        MacSourceMetadata {
            bits_per_component: Some(12),
            full_range: false,
        },
    );

    assert_eq!(
        output.frame_format(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)
    );
}

#[test]
fn macos_output_fallback_is_p010_then_nv12_then_bgra() {
    let p010 = select_mac_output_format(
        true,
        MacSourceMetadata {
            bits_per_component: Some(10),
            full_range: true,
        },
    );
    let nv12 = p010
        .fallback_after_rejection(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010))
        .unwrap();
    let bgra = nv12
        .fallback_after_rejection(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12))
        .unwrap();

    assert_eq!(
        nv12.frame_format(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
    );
    assert_eq!(nv12.range, VideoColorRange::Full);
    assert_eq!(
        bgra.frame_format(),
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
    );
    assert_eq!(
        bgra.fallback_after_rejection(VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)),
        None
    );
    assert_eq!(
        p010.fallback_after_rejection(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12))
            .unwrap()
            .frame_format(),
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
    );
    assert!(p010.allows_wide_color());
    assert!(nv12.allows_wide_color());
    assert!(!bgra.allows_wide_color());
    assert!(!p010.requires_explicit_sdr_color_properties());
    assert!(!nv12.requires_explicit_sdr_color_properties());
    assert!(bgra.requires_explicit_sdr_color_properties());
}
