use std::num::NonZeroU32;

use super::{
    BiPlanarVideoFormat, LoopMode, MediaTime, PackedVideoFormat, PixelAspectRatio, PixelRect,
    PlaybackEpoch, PlaybackRate, PresentationVisibility, VideoChromaLocation, VideoColorPrimaries,
    VideoColorRange, VideoColorimetry, VideoFrameFormat, VideoFrameLayoutError, VideoGeometry,
    VideoMatrixCoefficients, VideoPlaneFormat, VideoRecoveryManifest, VideoRotation,
    VideoSampleKind, VideoSource, VideoTextureCoordinates, VideoTransferCharacteristic,
};

#[test]
fn video_frame_format_owns_plane_layout_and_allocation_size() {
    let geometry = VideoGeometry::packed(1920, 1080);

    assert_eq!(
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8).plane_formats(),
        &[VideoPlaneFormat::Bgra8UnormSrgb]
    );
    assert_eq!(
        VideoFrameFormat::Packed(PackedVideoFormat::Rgba8)
            .allocation_bytes(geometry)
            .unwrap(),
        1920 * 1080 * 4
    );
    assert_eq!(
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12).plane_formats(),
        &[VideoPlaneFormat::R8Unorm, VideoPlaneFormat::Rg8Unorm]
    );
    assert_eq!(
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
            .allocation_bytes(geometry)
            .unwrap(),
        1920 * 1080 * 3 / 2
    );
    assert_eq!(
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010).plane_formats(),
        &[VideoPlaneFormat::R16Unorm, VideoPlaneFormat::Rg16Unorm]
    );
    assert_eq!(
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)
            .allocation_bytes(geometry)
            .unwrap(),
        1920 * 1080 * 3
    );
}

#[test]
fn subsampled_formats_reject_geometry_that_cannot_describe_two_complete_planes() {
    let format = VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12);

    assert_eq!(
        format.allocation_bytes(VideoGeometry::packed(1919, 1080)),
        Err(VideoFrameLayoutError::OddSubsampledDimensions {
            width: 1919,
            height: 1080,
        })
    );
    assert_eq!(
        format.allocation_bytes(VideoGeometry::packed(1920, 1079)),
        Err(VideoFrameLayoutError::OddSubsampledDimensions {
            width: 1920,
            height: 1079,
        })
    );
}

#[test]
fn colorimetry_is_a_complete_typed_contract() {
    assert_eq!(
        VideoColorimetry::SRGB,
        VideoColorimetry {
            primaries: VideoColorPrimaries::Bt709,
            transfer: VideoTransferCharacteristic::Srgb,
            matrix: VideoMatrixCoefficients::Identity,
            range: VideoColorRange::Full,
            chroma_location: VideoChromaLocation::Center,
        }
    );
    assert_eq!(
        VideoColorimetry::BT709_LIMITED,
        VideoColorimetry {
            primaries: VideoColorPrimaries::Bt709,
            transfer: VideoTransferCharacteristic::Bt709,
            matrix: VideoMatrixCoefficients::Bt709,
            range: VideoColorRange::Limited,
            chroma_location: VideoChromaLocation::Left,
        }
    );
}

#[test]
fn frame_format_selects_the_renderer_pipeline_without_platform_types() {
    assert_eq!(
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8).sample_kind(),
        VideoSampleKind::Packed
    );
    assert_eq!(
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010).sample_kind(),
        VideoSampleKind::BiPlanar
    );
}

#[test]
fn playback_epoch_round_trips_through_the_backend_boundary() {
    let epoch = PlaybackEpoch::INITIAL.next();

    assert_eq!(PlaybackEpoch::from_raw(epoch.get()), Some(epoch));
    assert_eq!(PlaybackEpoch::from_raw(0), None);
}

fn assert_coordinates(actual: VideoTextureCoordinates, expected: [[f32; 2]; 4]) {
    for (actual, expected) in actual.corners().into_iter().zip(expected) {
        assert!((actual[0] - expected[0]).abs() < 0.000_01);
        assert!((actual[1] - expected[1]).abs() < 0.000_01);
    }
}

#[test]
fn sampling_transform_crops_to_the_visible_coded_pixel_rectangle() {
    let geometry = VideoGeometry {
        coded_width: 100,
        coded_height: 80,
        visible_rect: PixelRect {
            x: 10,
            y: 20,
            width: 40,
            height: 20,
        },
        display_width: 80,
        display_height: 40,
        pixel_aspect_ratio: PixelAspectRatio {
            numerator: NonZeroU32::new(2).unwrap(),
            denominator: NonZeroU32::MIN,
        },
        rotation: VideoRotation::None,
    };

    assert_coordinates(
        geometry.sampling_transform().coordinates(),
        [[0.1, 0.25], [0.5, 0.25], [0.5, 0.5], [0.1, 0.5]],
    );
}

#[test]
fn sampling_transform_rotates_source_coordinates_at_the_typed_boundary() {
    let mut geometry = VideoGeometry::packed(4, 2);
    geometry.rotation = VideoRotation::Clockwise90;

    assert_coordinates(
        geometry.sampling_transform().coordinates(),
        [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
    );

    geometry.rotation = VideoRotation::Clockwise270;
    assert_coordinates(
        geometry.sampling_transform().coordinates(),
        [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
    );
}

#[test]
fn destination_clipping_is_composed_after_crop_and_rotation() {
    let mut geometry = VideoGeometry::packed(4, 2);
    geometry.rotation = VideoRotation::Clockwise90;

    assert_coordinates(
        geometry
            .sampling_transform()
            .coordinates_for_destination_rect(0.0, 1.0, 0.25, 0.75),
        [[0.25, 1.0], [0.25, 0.0], [0.75, 0.0], [0.75, 1.0]],
    );
}

#[test]
fn pixel_aspect_ratio_produces_square_pixel_display_dimensions() {
    let geometry = VideoGeometry::with_pixel_aspect_ratio(
        720,
        480,
        PixelRect {
            x: 0,
            y: 0,
            width: 720,
            height: 480,
        },
        PixelAspectRatio {
            numerator: NonZeroU32::new(8).unwrap(),
            denominator: NonZeroU32::new(9).unwrap(),
        },
        VideoRotation::None,
    );

    assert_eq!(
        (geometry.display_width, geometry.display_height),
        (640, 480)
    );
}

#[test]
fn rotated_display_dimensions_do_not_distort_pixel_aspect_ratio() {
    let geometry = VideoGeometry::with_visible_rect_and_display_size(
        1920,
        1080,
        PixelRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        1080,
        1920,
        VideoRotation::Clockwise90,
    );

    assert_eq!(geometry.pixel_aspect_ratio, PixelAspectRatio::SQUARE);
}

#[test]
fn recovery_manifest_updates_intent_without_carrying_session_identity() {
    let manifest = VideoRecoveryManifest::new(
        VideoSource::Uri("https://example.invalid/movie.mp4".into()),
        LoopMode::Infinite,
        true,
        PlaybackRate::new(1.5).unwrap(),
        MediaTime::from_nanos(42),
        PresentationVisibility::Hidden,
    );

    let resumed = manifest
        .clone()
        .with_presentation(PresentationVisibility::Presented);
    assert_eq!(resumed.source(), manifest.source());
    assert_eq!(resumed.loop_mode(), LoopMode::Infinite);
    assert!(resumed.desired_playing());
    assert_eq!(resumed.rate(), PlaybackRate::new(1.5).unwrap());
    assert_eq!(resumed.position(), MediaTime::from_nanos(42));
    assert_eq!(resumed.presentation(), PresentationVisibility::Presented);
}
