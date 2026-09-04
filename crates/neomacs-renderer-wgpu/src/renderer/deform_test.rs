use super::*;

#[test]
fn test_noise2d_deterministic() {
    let a = noise2d(1.0, 2.0);
    let b = noise2d(1.0, 2.0);
    assert_eq!(a, b);
    // Different inputs should give different results (usually)
    let c = noise2d(3.0, 4.0);
    assert_ne!(a, c);
}

#[test]
fn test_noise2d_range() {
    // noise2d returns fract() of (sin(...) * large_number).
    // Rust's fract() preserves sign: (-1.3f32).fract() == -0.3
    // So the output range is (-1, 1), not [0, 1).
    for i in 0..100 {
        for j in 0..100 {
            let val = noise2d(i as f32 * 0.7, j as f32 * 1.3);
            assert!(val > -1.0, "noise2d({}, {}) = {} <= -1", i, j, val);
            assert!(val < 1.0, "noise2d({}, {}) = {} >= 1", i, j, val);
        }
    }
}

#[test]
fn test_smooth_noise2d_deterministic() {
    let a = smooth_noise2d(1.5, 2.7);
    let b = smooth_noise2d(1.5, 2.7);
    assert_eq!(a, b);
}

#[test]
fn test_smooth_noise2d_range() {
    // smooth_noise2d bilinearly interpolates noise2d values which
    // are in (-1, 1) due to Rust's fract() preserving sign.
    // Interpolation keeps the result within the same range.
    for i in 0..50 {
        for j in 0..50 {
            let val = smooth_noise2d(i as f32 * 0.3, j as f32 * 0.3);
            assert!(
                val > -1.0 && val < 1.0,
                "smooth_noise2d out of range: {}",
                val
            );
        }
    }
}

#[test]
fn test_wobbly_deform_at_full_progress() {
    // At eased_t=1.0, damping=0, so deformation should be zero
    let (x, y) = wobbly_deform(5, 10, 0.5, 1.0, 1.0, 20.0);
    assert!(
        x.abs() < 0.001,
        "Wobbly x should be ~0 at full progress, got {}",
        x
    );
    assert_eq!(y, 0.0);
}

#[test]
fn test_wobbly_deform_scroll_direction() {
    // Different scroll directions should produce different deformations
    let (x_down, _) = wobbly_deform(2, 10, 0.3, 0.5, 1.0, 20.0);
    let (x_up, _) = wobbly_deform(2, 10, 0.3, 0.5, -1.0, 20.0);
    // strip_t differs based on direction, so x offsets should differ
    assert_ne!(x_down, x_up);
}

#[test]
fn test_liquid_deform_at_full_progress() {
    // At eased_t=1.0, damping=0, so liquid should be zero
    let (x, y) = liquid_deform(2, 10, 0.5, 1.0, 1.0, 30.0);
    assert!(
        x.abs() < 0.001,
        "Liquid x should be ~0 at full progress, got {}",
        x
    );
    assert!(
        y.abs() < 0.001,
        "Liquid y should be ~0 at full progress, got {}",
        y
    );
}

#[test]
fn test_page_curl_above_curl_line_is_flat() {
    // curl_progress=0.5 → curl_y=0.5
    // At t=0.2 (above curl), should be flat
    let (x, y, alpha) = page_curl_transform(0.2, 0.5, 400.0);
    assert_eq!(x, 0.0);
    assert_eq!(y, 0.0);
    assert_eq!(alpha, 1.0);
}

#[test]
fn test_page_curl_below_curl_line_is_deformed() {
    // curl_progress=0.5 → curl_y=0.5
    // At t=0.8 (below curl), should have deformation
    let (_x, y, alpha) = page_curl_transform(0.8, 0.5, 400.0);
    // y_offset should be negative (curling away)
    assert!(y < 0.0, "Page curl y should be negative, got {}", y);
    // Alpha should be reduced (darkened backside)
    assert!(
        alpha < 1.0,
        "Page curl alpha should be < 1.0, got {}",
        alpha
    );
    assert!(
        alpha >= 0.2,
        "Page curl alpha should be >= 0.2, got {}",
        alpha
    );
}
