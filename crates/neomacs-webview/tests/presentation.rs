use neomacs_display_protocol::{DeviceScale, DisplayWindowId, RootSurfaceRect};
use neomacs_webview::{
    HostWindowId, ResolvedWebViewPlacement, ResolvedWebViewScene, WebViewId, WebViewOccurrenceId,
    WebViewSceneError, WebViewSceneRevision,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> RootSurfaceRect {
    RootSurfaceRect::new(x, y, width, height).expect("test rectangle is valid")
}

fn placement(view: u32, occurrence: u64, x: f32) -> ResolvedWebViewPlacement {
    ResolvedWebViewPlacement::new(
        WebViewId::new(view),
        WebViewOccurrenceId::new(occurrence),
        DisplayWindowId::new(7),
        rect(x, 10.0, 100.0, 80.0),
        rect(x + 5.0, 14.0, 90.0, 70.0),
        DeviceScale::new(1.5).unwrap(),
    )
    .unwrap()
}

#[test]
fn scene_rejects_two_occurrences_of_one_webview() {
    let duplicate = WebViewId::new(41);
    let error = ResolvedWebViewScene::try_new(
        HostWindowId::new(3),
        WebViewSceneRevision::new(9),
        vec![
            placement(duplicate.get(), 1, 0.0),
            placement(duplicate.get(), 2, 120.0),
        ],
    )
    .unwrap_err();

    assert_eq!(
        error,
        WebViewSceneError::DuplicateView {
            view: duplicate,
            first: WebViewOccurrenceId::new(1),
            duplicate: WebViewOccurrenceId::new(2),
        }
    );
}

#[test]
fn placement_records_the_resolved_all_edge_clip_offset() {
    let placement = placement(42, 5, 20.0);

    assert_eq!(placement.content_offset().x(), 5.0);
    assert_eq!(placement.content_offset().y(), 4.0);
    assert_eq!(placement.visible_rect(), rect(25.0, 14.0, 90.0, 70.0));
}
