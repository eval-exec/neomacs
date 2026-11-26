//! Real Vulkan DMA-BUF imports. Select this target with
//! `--features native-video-validation --test gui -E 'test(native_video_validation::)'` and set
//! NEOMACS_VIDEO_VALIDATION_ARTIFACTS plus the Vulkan layer/driver environment.
//! Missing prerequisites fail; this target never substitutes a passing probe.
#![cfg(target_os = "linux")]

use neomacs_video::{BiPlanarVideoFormat, assert_native_dmabuf_plane_validation};

#[test]
fn native_nv12_dmabuf_plane_views_obey_vulkan_validation() {
    assert_native_dmabuf_plane_validation(BiPlanarVideoFormat::Nv12);
}

#[test]
fn native_p010_dmabuf_plane_views_obey_vulkan_validation() {
    assert_native_dmabuf_plane_validation(BiPlanarVideoFormat::P010);
}
