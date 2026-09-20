use neomacs_gui_tests::interaction::{CaptureMapping, DesktopPoint, DesktopRect};

#[test]
fn retina_capture_maps_a_window_on_a_display_left_of_the_primary() {
    let mapping = CaptureMapping::new(
        DesktopRect {
            x: -1512.0,
            y: 40.0,
            width: 1512.0,
            height: 942.0,
        },
        3024,
        1884,
    )
    .unwrap();
    assert_eq!(
        mapping
            .pixel(DesktopPoint {
                x: -1502.0,
                y: 60.0
            })
            .unwrap(),
        (20, 40)
    );
    assert!(mapping.pixel(DesktopPoint { x: 0.0, y: 60.0 }).is_err());
    assert!(
        mapping
            .pixel(DesktopPoint {
                x: f64::NAN,
                y: 60.0
            })
            .is_err()
    );
    assert!(
        CaptureMapping::new(
            DesktopRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 1.0
            },
            10,
            10
        )
        .is_err()
    );
}
