use super::*;
use crate::render_thread::frame_windows::{FrameLifecycle, GuiFrameRenderState};
use neomacs_display_protocol::{
    ChromeBandRequest, Color, ContentInsets, DeviceScale, FrameChrome, FrameChromeContent,
    FrameSize, MenuBarContent, MenuBarItem, SurfaceState,
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
                vec![
                    neomacs_display_protocol::frame_chrome::PositionedMenuHeading::measure(
                        MenuBarItem {
                            index: 0,
                            label: "Help".into(),
                            key: "help".into(),
                        },
                        8.0,
                        18.0,
                        0.0,
                        |_| neomacs_display_protocol::frame_chrome::MenuHeadingText::Cells {
                            width: 48.0,
                        },
                    ),
                ],
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
        visibility: crate::thread_comm::WindowVisibility::Visible,
        pending_scale_factor: None,
        lifecycle: FrameLifecycle::Pending {
            width: 1600,
            height: 1200,
            scale_factor: 2.0,
            mouse_hidden_for_typing: false,
            ime_enabled: false,
            last_ime_cursor_area: None,
            chrome: Default::default(),
            geometry_hints: None,
            pending_position: None,
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

#[test]
fn pointer_entered_updates_mouse_position_for_subsequent_scroll_without_motion() {
    use crate::thread_comm::{InputEvent, PointerAction, ThreadComms};
    use std::sync::{Arc, Mutex};
    use winit::{
        dpi::PhysicalPosition,
        event::{MouseScrollDelta, PointerKind, TouchPhase, WindowEvent},
        window::WindowId,
    };

    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    let window_id = WindowId::from_raw(7);
    app.frame_windows.primary_winit_id = Some(window_id);

    // Primary PointerEntered with zero subsequent PointerMoved must update
    // mouse_pos so a scroll event targets the entered coordinates instead of
    // (0.0, 0.0).
    app.dispatch_pointer_window_event(
        window_id,
        WindowEvent::PointerEntered {
            device_id: None,
            position: PhysicalPosition::new(200.0, 240.0),
            primary: true,
            kind: PointerKind::Mouse,
        },
    )
    .expect("PointerEntered is handled");
    assert_eq!(
        app.frame_windows.primary_window().unwrap().render.mouse_pos,
        (200.0, 240.0)
    );

    // Non-primary PointerEntered must not overwrite the primary pointer's position.
    app.dispatch_pointer_window_event(
        window_id,
        WindowEvent::PointerEntered {
            device_id: None,
            position: PhysicalPosition::new(12.0, 16.0),
            primary: false,
            kind: PointerKind::Mouse,
        },
    )
    .expect("non-primary PointerEntered is accepted");
    assert_eq!(
        app.frame_windows.primary_window().unwrap().render.mouse_pos,
        (200.0, 240.0)
    );

    app.dispatch_pointer_window_event(
        window_id,
        WindowEvent::MouseWheel {
            device_id: None,
            delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -4.0)),
            phase: TouchPhase::Moved,
            timestamp: None,
        },
    )
    .expect("MouseWheel is handled");

    let mut scroll_pos = None;
    while let Ok(event) = emacs.input_rx.try_recv() {
        let inner = match event {
            InputEvent::Tracked { event, .. } => *event,
            other => other,
        };
        let inner = match inner {
            InputEvent::Observed { event, .. } => *event,
            other => other,
        };
        if let InputEvent::PositionedPointer(pointer) = inner
            && matches!(pointer.action, PointerAction::Scroll { .. })
        {
            scroll_pos = Some((pointer.position.x, pointer.position.y));
        }
    }
    assert_eq!(
        scroll_pos,
        Some((200.0, 240.0)),
        "MouseWheel after PointerEntered without PointerMoved must use entered coordinates"
    );
}
