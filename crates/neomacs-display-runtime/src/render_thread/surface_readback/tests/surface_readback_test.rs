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
