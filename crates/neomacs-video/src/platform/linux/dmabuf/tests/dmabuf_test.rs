use super::sampled_format;
use super::{ImportPlan, validate_surface_layout};
use crate::{BiPlanarVideoFormat, PackedVideoFormat, VideoFrameFormat, VideoGeometry};
use ash::vk;

#[test]
fn vulkan_import_accepts_native_bi_planar_and_alpha_packed_formats() {
    assert!(sampled_format(0x3432_5241).is_some()); // AR24
    assert!(sampled_format(0x3432_4241).is_some()); // AB24
    assert_eq!(
        sampled_format(0x3231_564e), // NV12
        Some((
            ash::vk::Format::G8_B8R8_2PLANE_420_UNORM,
            wgpu::TextureFormat::NV12,
        ))
    );
    assert_eq!(
        sampled_format(0x3031_3050), // P010
        Some((
            ash::vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            wgpu::TextureFormat::P010,
        ))
    );
    assert!(sampled_format(0x3432_5258).is_none()); // XR24
    assert!(sampled_format(0x3432_4258).is_none()); // XB24
}

#[test]
fn multi_planar_import_plan_keeps_only_compatible_mutable_plane_formats() {
    for (fourcc, format, views) in [
        (
            0x3231_564e,
            BiPlanarVideoFormat::Nv12,
            [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM],
        ),
        (
            0x3031_3050,
            BiPlanarVideoFormat::P010,
            [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM],
        ),
    ] {
        for disjoint in [false, true] {
            let plan = ImportPlan::new(
                fourcc,
                VideoGeometry::packed(64, 32),
                VideoFrameFormat::BiPlanar420(format),
                disjoint,
            )
            .unwrap();
            assert!(plan.flags.contains(
                vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE
            ));
            assert_eq!(
                plan.flags.contains(vk::ImageCreateFlags::DISJOINT),
                disjoint
            );
            assert_eq!(plan.view_formats, views);
            assert!(!plan.view_formats.contains(&plan.vk_format));
            assert!(
                plan.usage
                    .contains(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC)
            );
        }
    }
    let packed = ImportPlan::new(
        0x3432_5241,
        VideoGeometry::packed(63, 31),
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8),
        false,
    )
    .unwrap();
    assert!(packed.flags.is_empty());
    assert!(packed.view_formats.is_empty());
}

#[test]
fn native_import_rejects_mismatched_and_invalid_decoded_layouts() {
    let nv12 = VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12);
    let p010 = VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010);
    let bgra = VideoFrameFormat::Packed(PackedVideoFormat::Bgra8);
    assert!(validate_surface_layout(0x3231_564e, VideoGeometry::packed(64, 32), p010).is_err());
    assert!(validate_surface_layout(0x3231_564e, VideoGeometry::packed(64, 32), bgra).is_err());
    assert!(validate_surface_layout(0, VideoGeometry::packed(64, 32), nv12).is_err());
    for (width, height) in [
        (0, 32),
        (64, 0),
        (63, 32),
        (64, 31),
        (u32::MAX - 1, u32::MAX - 1),
    ] {
        assert!(
            validate_surface_layout(0x3031_3050, VideoGeometry::packed(width, height), p010)
                .is_err()
        );
    }
    assert_eq!(
        validate_surface_layout(0x3231_564e, VideoGeometry::packed(64, 32), nv12).unwrap(),
        3072
    );
}

#[test]
fn disjoint_import_rejects_external_dedicated_only_memory() {
    let external = vk::ExternalMemoryProperties::default()
        .external_memory_features(
            vk::ExternalMemoryFeatureFlags::IMPORTABLE
                | vk::ExternalMemoryFeatureFlags::DEDICATED_ONLY,
        )
        .compatible_handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let plan = |disjoint| {
        ImportPlan::new(
            0x3231_564e,
            VideoGeometry::packed(64, 32),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12),
            disjoint,
        )
        .unwrap()
    };
    assert!(plan(false).check_external_support(external).is_ok());
    assert!(
        plan(true).check_external_support(external).is_err(),
        "VUID01797 forbids dedicating any allocation to a DISJOINT image"
    );
}
