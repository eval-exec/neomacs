use super::XwidgetContentExtent;

#[test]
fn a_content_extent_needs_two_finite_positive_dimensions() {
    let extent = XwidgetContentExtent::new(600.0, 40.0).expect("valid extent");
    assert_eq!(extent.width_px(), 600.0);
    assert_eq!(extent.height_px(), 40.0);

    assert_eq!(XwidgetContentExtent::new(0.0, 40.0), None);
    assert_eq!(XwidgetContentExtent::new(600.0, -1.0), None);
    assert_eq!(XwidgetContentExtent::new(f32::NAN, 40.0), None);
    assert_eq!(XwidgetContentExtent::new(600.0, f32::INFINITY), None);
}
