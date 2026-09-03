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

#[test]
fn deserialization_goes_through_the_constructor() {
    let extent = XwidgetContentExtent::new(600.0, 40.0).expect("valid extent");
    let json = serde_json::to_string(&extent).expect("serialize");
    assert_eq!(json, r#"{"width_px":600.0,"height_px":40.0}"#);
    let round_trip: XwidgetContentExtent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round_trip, extent);

    for rejected in [
        r#"{"width_px":0.0,"height_px":40.0}"#,
        r#"{"width_px":600.0,"height_px":-1.0}"#,
        r#"{"width_px":600.0,"height_px":null}"#,
    ] {
        let error = serde_json::from_str::<XwidgetContentExtent>(rejected)
            .expect_err("an extent the constructor refuses must not deserialize");
        assert!(
            error
                .to_string()
                .contains("not finite and strictly positive")
                || error.to_string().contains("invalid type"),
            "{rejected}: {error}"
        );
    }
}
