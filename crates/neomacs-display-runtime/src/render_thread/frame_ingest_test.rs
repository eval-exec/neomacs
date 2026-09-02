//! Scene-clock behaviour of `frame_ingest.rs`: placement resolution runs once
//! per presentation, and the scene revision moves only on a real change.

use neomacs_display_protocol::{
    DeviceScale, DisplayWindowId, FrameGlyphBuffer, GlyphRowRole, Rect, RootSurfaceRect, WebViewId,
    XwidgetContentExtent, XwidgetId,
};
use neomacs_webview::{
    HostWindowId, ResolvedWebViewPlacement, WebViewOccurrenceId, WebViewSceneRevision,
};

use super::{WebViewPlacementInputs, WebViewSceneClock, collect_frame_webviews};

fn placement(view: u32) -> ResolvedWebViewPlacement {
    let rect = RootSurfaceRect::new(0.0, 0.0, 20.0, 10.0).unwrap();
    ResolvedWebViewPlacement::new(
        WebViewId::new(view),
        WebViewOccurrenceId::new(u64::from(view)),
        DisplayWindowId::new(1),
        rect,
        rect,
        DeviceScale::ONE,
    )
    .unwrap()
}

fn inputs(root: u64, children: &[u64]) -> WebViewPlacementInputs {
    use neomacs_display_protocol::frame_chrome::PresentationId;
    WebViewPlacementInputs {
        root: Some((PresentationId::new(root), 800.0, 600.0)),
        scale: 2.0,
        children: children
            .iter()
            .map(|child| {
                (
                    *child,
                    PresentationId::new(*child),
                    10.0,
                    20.0,
                    neomacs_display_protocol::PresentedClip::Empty,
                )
            })
            .collect(),
    }
}

/// The glyph walk runs once per presentation, not once per event-loop
/// pass: equal inputs answer from the cache without calling `compute`.
#[test]
fn unchanged_inputs_reuse_the_cached_placements_without_walking_glyphs() {
    let host = HostWindowId::new(3);
    let mut clock = WebViewSceneClock::default();
    let mut walks = 0;
    let mut walk = |placements: Vec<ResolvedWebViewPlacement>| {
        walks += 1;
        placements
    };

    let first = clock
        .resolve_cached(host, inputs(1, &[]), || walk(vec![placement(7)]))
        .unwrap();
    let idle = clock
        .resolve_cached(host, inputs(1, &[]), || walk(vec![placement(9)]))
        .unwrap();

    assert_eq!(walks, 1, "the second pass had nothing new to look at");
    assert_eq!(first.revision(), WebViewSceneRevision::new(1));
    assert_eq!(idle.revision(), WebViewSceneRevision::new(1));
    assert_eq!(idle.placements(), first.placements());
}

/// A new presentation, a moved child frame, or a scale change is a new
/// input set: the walk runs again, and the revision advances only when
/// the resolved placements really differ.
#[test]
fn changed_inputs_walk_again_and_advance_only_on_a_real_difference() {
    let host = HostWindowId::new(3);
    let mut clock = WebViewSceneClock::default();
    let mut walks = 0;
    let mut walk = |placements: Vec<ResolvedWebViewPlacement>| {
        walks += 1;
        placements
    };

    let a = clock
        .resolve_cached(host, inputs(1, &[]), || walk(vec![placement(7)]))
        .unwrap();
    let same_glyphs_new_presentation = clock
        .resolve_cached(host, inputs(2, &[]), || walk(vec![placement(7)]))
        .unwrap();
    let child_appeared = clock
        .resolve_cached(host, inputs(2, &[5]), || walk(Vec::new()))
        .unwrap();

    assert_eq!(walks, 3);
    assert_eq!(a.revision(), WebViewSceneRevision::new(1));
    assert_eq!(
        same_glyphs_new_presentation.revision(),
        WebViewSceneRevision::new(1),
        "redisplay that left the web view where it was is not a scene change"
    );
    assert_eq!(child_appeared.revision(), WebViewSceneRevision::new(2));
}

#[test]
fn removing_the_newest_child_advances_the_host_scene_revision() {
    let host = HostWindowId::new(3);
    let mut clock = WebViewSceneClock::default();
    let with_child = clock
        .resolve_cached(host, inputs(1, &[7]), || vec![placement(7)])
        .unwrap();
    let unchanged = clock
        .resolve_cached(host, inputs(2, &[7]), || vec![placement(7)])
        .unwrap();
    let child_removed = clock
        .resolve_cached(host, inputs(3, &[]), Vec::new)
        .unwrap();

    assert_eq!(with_child.revision(), WebViewSceneRevision::new(1));
    assert_eq!(unchanged.revision(), WebViewSceneRevision::new(1));
    assert_eq!(child_removed.revision(), WebViewSceneRevision::new(2));
/// GNU sizes the native view from the widget (`xww->width`) and clips it to
/// the text area (`x_draw_xwidget_glyph_string`, src/xwidget.c:2841-2849,
/// emacs-31.0.90); the glyph's cropped advance is not the view's size.  A
/// 600 px page cropped to a 304 px slot is therefore a 600 px viewport behind
/// a 304 px clip, not a 304 px viewport.
#[test]
fn a_cropped_xwidget_slot_keeps_the_native_content_width_behind_the_clip() {
    let mut frame = FrameGlyphBuffer::new();
    frame.set_draw_context(
        DisplayWindowId::new(1),
        GlyphRowRole::Text,
        Some(Rect::new(0.0, 0.0, 312.0, 120.0)),
    );
    let content = XwidgetContentExtent::new(600.0, 40.0).expect("content extent");
    frame.add_xwidget(
        XwidgetId::new(7),
        WebViewId::new(91),
        8.0,
        16.0,
        content,
        304.0,
    );

    let mut occurrence = 0;
    let mut placements = std::collections::HashMap::new();
    collect_frame_webviews(
        &frame,
        0.0,
        0.0,
        RootSurfaceRect::new(0.0, 0.0, 320.0, 120.0).expect("root clip"),
        DeviceScale::ONE,
        &mut occurrence,
        &mut placements,
    );

    let placement = placements
        .get(&WebViewId::new(91))
        .expect("the cropped xwidget is placed");
    assert_eq!(placement.content_rect().width(), 600.0, "GNU: xww->width");
    assert_eq!(placement.content_rect().height(), 40.0);
    assert_eq!(placement.content_rect().x(), 8.0);
    assert_eq!(
        placement.visible_rect().width(),
        304.0,
        "clip_right = text_area_x + text_area_width - x"
    );
    assert_eq!(placement.visible_rect().x(), 8.0);
}
