use super::*;

#[test]
fn test_buffer_format_bytes_per_pixel() {
    assert_eq!(BufferFormat::Bgra8.bytes_per_pixel(), 4);
    assert_eq!(BufferFormat::Rgba8.bytes_per_pixel(), 4);
    assert_eq!(BufferFormat::Argb8.bytes_per_pixel(), 4);
}

#[test]
fn test_shared_memory_buffer_dimensions() {
    let buffer = SharedMemoryBuffer::new(
        vec![0u8; 100 * 50 * 4],
        100,
        50,
        100 * 4,
        BufferFormat::Bgra8,
    );
    assert_eq!(buffer.dimensions(), (100, 50));
}

#[test]
fn test_rgba_to_bgra_conversion() {
    // Create a small RGBA buffer with known values
    let rgba_data = vec![
        255, 0, 0, 255, // Red pixel (RGBA)
        0, 255, 0, 255, // Green pixel (RGBA)
        0, 0, 255, 255, // Blue pixel (RGBA)
        128, 64, 32, 200, // Mixed pixel (RGBA)
    ];
    let buffer = SharedMemoryBuffer::new(rgba_data, 4, 1, 16, BufferFormat::Rgba8);

    let bgra = buffer.convert_to_bgra().expect("Should convert");
    // Red pixel should become BGRA: B=0, G=0, R=255, A=255
    assert_eq!(bgra[0..4], [0, 0, 255, 255]);
    // Green pixel should become BGRA: B=0, G=255, R=0, A=255
    assert_eq!(bgra[4..8], [0, 255, 0, 255]);
    // Blue pixel should become BGRA: B=255, G=0, R=0, A=255
    assert_eq!(bgra[8..12], [255, 0, 0, 255]);
    // Mixed pixel should become BGRA: B=32, G=64, R=128, A=200
    assert_eq!(bgra[12..16], [32, 64, 128, 200]);
}

#[test]
fn test_argb_to_bgra_conversion() {
    // Create a small ARGB buffer with known values
    let argb_data = vec![
        255, 255, 0, 0, // Red pixel (ARGB: A=255, R=255, G=0, B=0)
        255, 0, 255, 0, // Green pixel (ARGB)
        255, 0, 0, 255, // Blue pixel (ARGB)
        200, 128, 64, 32, // Mixed pixel (ARGB)
    ];
    let buffer = SharedMemoryBuffer::new(argb_data, 4, 1, 16, BufferFormat::Argb8);

    let bgra = buffer.convert_to_bgra().expect("Should convert");
    // Red pixel should become BGRA: B=0, G=0, R=255, A=255
    assert_eq!(bgra[0..4], [0, 0, 255, 255]);
    // Green pixel should become BGRA: B=0, G=255, R=0, A=255
    assert_eq!(bgra[4..8], [0, 255, 0, 255]);
    // Blue pixel should become BGRA: B=255, G=0, R=0, A=255
    assert_eq!(bgra[8..12], [255, 0, 0, 255]);
    // Mixed pixel should become BGRA: B=32, G=64, R=128, A=200
    assert_eq!(bgra[12..16], [32, 64, 128, 200]);
}

#[test]
fn test_bgra_no_conversion() {
    let bgra_data = vec![0, 0, 255, 255]; // One blue pixel
    let buffer = SharedMemoryBuffer::new(bgra_data, 1, 1, 4, BufferFormat::Bgra8);

    // Should return None since no conversion is needed
    assert!(buffer.convert_to_bgra().is_none());
}
