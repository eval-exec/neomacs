use super::*;
use crate::render_thread::frame_windows::{FrameLifecycle, GuiFrameRenderState};
use neomacs_display_protocol::{
    BandRect, ChromeBandRequest, Color, ContentInsets, DeviceScale, FrameChrome,
    FrameChromeContent, FrameSize, MenuBarContent, MenuBarItem, PositionedChromeItem, SurfaceState,
};

#[test]
fn native_titlebar_is_not_a_menu_hit_and_popup_anchor_stays_frame_local() {
    let mut frame = crate::core::frame_glyphs::FrameGlyphBuffer::with_size(800.0, 572.0);
    frame.frame_chrome = FrameChrome::layout(
        FrameSize::new(800.0, 572.0).unwrap(),
        vec![ChromeBandRequest::new(
            FrameChromeKind::MenuBar,
            18.0,
            FrameChromeContent::MenuBar(MenuBarContent::new(
                vec![PositionedChromeItem::new(
                    BandRect::new(8.0, 0.0, 48.0, 18.0).unwrap(),
                    MenuBarItem {
                        index: 0,
                        label: "Help".into(),
                        key: "help".into(),
                    },
                    ChromeAction::OpenMenu {
                        index: 0,
                        key: "help".into(),
                    },
                )],
                Color::WHITE,
                Color::BLACK,
            )),
        )],
    )
    .unwrap();
    let mut render = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(frame), None, Default::default(), Default::default());
    let SurfaceState::Drawable(surface) =
        SurfaceState::from_device_size(1600, 1200, DeviceScale::new(2.0).unwrap()).unwrap()
    else {
        unreachable!()
    };
    render.set_surface_state(SurfaceState::Drawable(
        surface.with_content_insets(ContentInsets::new(0, 56, 0, 0)),
    ));
    let window = GuiFrameWindowState {
        lifecycle: FrameLifecycle::Pending {
            width: 1600,
            height: 1200,
            scale_factor: 2.0,
            mouse_hidden_for_typing: false,
            ime_enabled: false,
            last_ime_cursor_area: None,
            chrome: Default::default(),
            geometry_hints: None,
        },
        render,
    };
    assert!(RenderApp::frame_window_menu_hit_test(&window, 20.0, 10.0).is_none());
    let hit = RenderApp::frame_window_menu_hit_test(&window, 20.0, 38.0)
        .expect("Help below native titlebar");
    assert_eq!(hit.key, "help");
    assert_eq!(hit.anchor.y, 0.0, "evaluator receives frame-local anchors");
    assert_eq!(
        window
            .render
            .surface_point_from_frame(hit.anchor.x, hit.anchor.y),
        Some((8.0, 28.0))
    );
    assert_eq!(
        RenderApp::frame_window_band_bounds(&window, FrameChromeKind::MenuBar)
            .unwrap()
            .y(),
        28.0
    );
}
