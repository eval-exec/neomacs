use super::sampled_format;

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
