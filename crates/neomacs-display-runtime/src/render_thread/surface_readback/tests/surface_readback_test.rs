use super::*;
use std::io::BufReader;

#[test]
fn published_readback_remains_complete_while_next_capture_replaces_it() {
    let root = neomacs_infra::crate_root!().join("../../tmp");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::tempdir_in(root).unwrap();
    let path = directory.path().join("readback.png");
    let publish = |color: [u8; 4]| {
        write_surface_readback_png(
            &path,
            &color.repeat(4),
            8,
            wgpu::TextureFormat::Rgba8Unorm,
            2,
            2,
        )
        .unwrap();
    };
    publish([255, 0, 0, 255]);
    // The reader may be the evaluator's copy-file, concurrently opening the
    // last capture while the render thread starts its next PNG. An opened
    // capture must remain immutable instead of being truncated underneath it.
    let previous = std::fs::File::open(&path).unwrap();
    publish([0, 0, 255, 255]);
    let previous = image::ImageReader::new(BufReader::new(previous))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();
    assert_eq!(previous.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(
        image::open(&path).unwrap().to_rgba8().get_pixel(0, 0).0,
        [0, 0, 255, 255]
    );
}

#[test]
fn readback_usage_disables_unsupported_captures_and_retains_supported_requests() {
    for supports_copy in [false, true] {
        for first_frame_requested in [false, true] {
            for requested_frames in [0, 2] {
                let mut first_frame_pending = first_frame_requested;
                let mut remaining_frames = requested_frames;
                let supported = if supports_copy {
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                };

                let usage = surface_usage_for_debug_readback(
                    supported,
                    &mut first_frame_pending,
                    &mut remaining_frames,
                );

                let capture_enabled = first_frame_requested || requested_frames > 0;
                assert_eq!(
                    usage.contains(wgpu::TextureUsages::COPY_SRC),
                    supports_copy && capture_enabled,
                );
                assert!(usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
                assert_eq!(first_frame_pending, supports_copy && first_frame_requested);
                assert_eq!(
                    remaining_frames,
                    if supports_copy { requested_frames } else { 0 },
                );
            }
        }
    }
}
