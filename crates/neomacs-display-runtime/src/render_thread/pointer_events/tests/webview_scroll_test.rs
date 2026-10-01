use super::*;

#[test]
fn embedded_browser_keeps_subpixel_distance_and_its_own_wheel_policy() {
    assert_eq!(
        webview_scroll_delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.25, -0.5)),
            2.0
        ),
        Some(WebViewScrollDelta::Pixels { x: 0.125, y: -0.25 })
    );
    assert_eq!(
        webview_scroll_delta(MouseScrollDelta::ContinuousLineDelta(0.125, -1.0), 2.0),
        Some(WebViewScrollDelta::Lines { x: 0.125, y: -1.0 })
    );
    assert_eq!(
        webview_scroll_delta(MouseScrollDelta::LineDelta(0.0, 3.0), 2.0),
        Some(WebViewScrollDelta::Lines { x: 0.0, y: 3.0 })
    );
    assert_eq!(
        webview_scroll_delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(f64::NAN, 0.0)),
            2.0
        ),
        None
    );
}
