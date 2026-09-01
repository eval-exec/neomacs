use super::*;

#[test]
fn test_drm_fourcc_to_wgpu() {
    assert_eq!(
        drm_fourcc_to_wgpu_format(drm_fourcc::DRM_FORMAT_ARGB8888),
        Some(wgpu::TextureFormat::Bgra8UnormSrgb)
    );
    assert_eq!(
        drm_fourcc_to_wgpu_format(drm_fourcc::DRM_FORMAT_RGBA8888),
        Some(wgpu::TextureFormat::Rgba8UnormSrgb)
    );
}
