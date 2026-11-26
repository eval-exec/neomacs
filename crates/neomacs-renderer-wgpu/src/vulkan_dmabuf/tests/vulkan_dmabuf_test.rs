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

#[cfg(target_os = "linux")]
fn headless_device_and_queue() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("headless wgpu adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vulkan_dmabuf_test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .expect("headless wgpu device")
}

#[cfg(target_os = "linux")]
#[test]
fn mmap_import_rejects_overflowing_stride_times_height() {
    use std::io::Write;
    use std::os::fd::AsFd;

    let (device, queue) = headless_device_and_queue();
    let mut file = tempfile::tempfile().expect("temp file");
    file.write_all(&[0u8; 64]).expect("write");

    let params = DmaBufImportParams {
        fds: vec![file.as_fd()],
        strides: vec![u32::MAX],
        offsets: vec![0],
        num_planes: 1,
        width: 1,
        height: 2,
        fourcc: drm_fourcc::DRM_FORMAT_ARGB8888,
        modifier: drm_fourcc::DRM_FORMAT_MOD_LINEAR,
    };

    assert!(
        unsafe { import_dmabuf_via_mmap(&device, &queue, &params) }.is_none(),
        "overflowing stride * height must be rejected instead of overflowing"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn mmap_import_rejects_short_backing_file_without_sigbus() {
    use std::io::Write;
    use std::os::fd::AsFd;

    let (device, queue) = headless_device_and_queue();
    let mut file = tempfile::tempfile().expect("temp file");
    file.write_all(&[0u8; 16]).expect("write");

    // Same shape as the audit trigger: a 16-byte backing file claiming a
    // 4096x4096 linear buffer must return None rather than SIGBUSing in mmap.
    let params = DmaBufImportParams {
        fds: vec![file.as_fd()],
        strides: vec![4096 * 4],
        offsets: vec![0],
        num_planes: 1,
        width: 4096,
        height: 4096,
        fourcc: drm_fourcc::DRM_FORMAT_ARGB8888,
        modifier: drm_fourcc::DRM_FORMAT_MOD_LINEAR,
    };

    assert!(
        unsafe { import_dmabuf_via_mmap(&device, &queue, &params) }.is_none(),
        "backing file shorter than offset + stride * height must be rejected before mmap"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn mmap_import_rejects_stride_smaller_than_row_bytes() {
    use std::io::Write;
    use std::os::fd::AsFd;

    let (device, queue) = headless_device_and_queue();
    let mut file = tempfile::tempfile().expect("temp file");
    file.write_all(&[0u8; 256]).expect("write");

    let params = DmaBufImportParams {
        fds: vec![file.as_fd()],
        strides: vec![4],
        offsets: vec![0],
        num_planes: 1,
        width: 16,
        height: 1,
        fourcc: drm_fourcc::DRM_FORMAT_ARGB8888,
        modifier: drm_fourcc::DRM_FORMAT_MOD_LINEAR,
    };

    assert!(
        unsafe { import_dmabuf_via_mmap(&device, &queue, &params) }.is_none(),
        "stride smaller than width * bpp must be rejected"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn mmap_import_rejects_zero_dimensions_and_succeeds_for_valid_linear_file() {
    use std::io::Write;
    use std::os::fd::AsFd;

    let (device, queue) = headless_device_and_queue();
    let mut file = tempfile::tempfile().expect("temp file");
    // 4x2 BGRA8 linear pixels (stride 16, 32 bytes total).
    file.write_all(&[0x7f; 32]).expect("write");

    let zero_params = DmaBufImportParams {
        fds: vec![file.as_fd()],
        strides: vec![16],
        offsets: vec![0],
        num_planes: 1,
        width: 0,
        height: 2,
        fourcc: drm_fourcc::DRM_FORMAT_ARGB8888,
        modifier: drm_fourcc::DRM_FORMAT_MOD_LINEAR,
    };
    assert!(unsafe { import_dmabuf_via_mmap(&device, &queue, &zero_params) }.is_none());

    let valid_params = DmaBufImportParams {
        fds: vec![file.as_fd()],
        strides: vec![16],
        offsets: vec![0],
        num_planes: 1,
        width: 4,
        height: 2,
        fourcc: drm_fourcc::DRM_FORMAT_ARGB8888,
        modifier: drm_fourcc::DRM_FORMAT_MOD_LINEAR,
    };
    let texture = unsafe { import_dmabuf_via_mmap(&device, &queue, &valid_params) }
        .expect("valid linear backing file should import via mmap");
    assert_eq!(texture.width(), 4);
    assert_eq!(texture.height(), 2);
}
