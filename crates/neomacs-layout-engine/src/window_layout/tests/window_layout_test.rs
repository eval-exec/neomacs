use super::metric_is_stable;

#[test]
fn chrome_convergence_requires_the_same_canonical_pixel_height() {
    assert!(metric_is_stable(17.0, 17.0));
    assert!(!metric_is_stable(17.0, 17.001));
}
