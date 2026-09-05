/// A surface point in a presentation composed with nothing in motion.
///
/// See the note on the display-protocol copy: a settled projection maps by
/// identity, so this states the point directly while still going through the
/// witness the production path uses.
fn settled_point(
    presentation: neomacs_display_protocol::PresentationId,
    x: f32,
    y: f32,
) -> neomacs_display_protocol::PresentationFramePoint {
    neomacs_display_protocol::InteractionProjection::settled(presentation)
        .map(
            neomacs_display_protocol::GeometryPoint::<
                neomacs_display_protocol::RootSurfaceSpace,
                neomacs_display_protocol::LogicalPixels,
            >::from_px(x, y)
            .expect("a finite surface point"),
        )
        .expect("a settled projection maps every finite point")
}

use super::*;

fn convert_display_event(event: &DisplayEvent) -> Option<KbInputEvent> {
    let mut converted = super::convert_display_event(event).into_iter();
    let event = converted.next();
    assert!(
        converted.next().is_none(),
        "test expected a single converted evaluator event"
    );
    event
}

fn unpresented_pointer(
    x: f32,
    y: f32,
    target_frame_id: u64,
    action: PointerAction,
) -> DisplayEvent {
    DisplayEvent::PositionedPointer(PositionedPointerInput {
        position: neomacs_display_runtime::thread_comm::PointerPosition {
            x,
            y,
            target_frame_id,
        },
        target: PointerTarget::Unpresented,
        action,
    })
}

#[test]
fn mouse_move_is_excluded_from_input_bridge_debug_logging() {
    let event = unpresented_pointer(1.0, 2.0, 7, PointerAction::Move { modifiers: 0 });

    assert!(!should_log_display_event(&event));
}

#[test]
fn non_mouse_move_is_included_in_input_bridge_debug_logging() {
    let event = DisplayEvent::WindowFocus {
        focused: true,
        emacs_frame_id: 7,
    };

    assert!(should_log_display_event(&event));
}

#[test]
fn image_cache_state_change_reaches_evaluator_with_identity_and_reason() {
    let state = neovm_core::emacs_core::image_catalog::ImageStateEvent::Evicted(
        neovm_core::emacs_core::image_catalog::ImageId::new(17),
    );
    let event = convert_display_event(&DisplayEvent::ImageStateChanged { event: state });

    assert!(matches!(
        event,
        Some(KbInputEvent::ImageStateChanged { event }) if event == state
    ));
}

#[test]
fn webview_title_change_reaches_the_evaluator_with_typed_identity() {
    let id = neomacs_display_protocol::WebViewId::new(17);
    let event = convert_display_event(&DisplayEvent::WebView(
        neomacs_webview::WebViewEvent::TitleChanged {
            id,
            generation: neomacs_webview::WebViewGeneration::new(1),
            title: "Typed web title".to_owned(),
        },
    ));

    assert!(matches!(
        event,
        Some(KbInputEvent::WebView(
            neovm_core::keyboard::FrontendWebViewEvent::TitleChanged {
                id: actual,
                title,
                ..
            }
        )) if actual == id && title == "Typed web title"
    ));
}

#[test]
fn webview_process_failure_reaches_the_evaluator_without_losing_the_reason() {
    let id = neomacs_display_protocol::WebViewId::new(18);
    let event = convert_display_event(&DisplayEvent::WebView(
        neomacs_webview::WebViewEvent::ProcessFailed {
            id,
            generation: neomacs_webview::WebViewGeneration::new(7),
            failure: neomacs_webview::WebProcessFailure::Other(42),
        },
    ));

    assert!(matches!(
        event,
        Some(KbInputEvent::WebView(
            neovm_core::keyboard::FrontendWebViewEvent::ProcessFailed {
                id: actual,
                generation: 7,
                failure: neovm_core::keyboard::FrontendWebProcessFailure::Other(42),
            }
        )) if actual == id
    ));
}

#[test]
fn surface_create_failure_reaches_the_evaluator_with_id_and_error() {
    let event = convert_display_event(&DisplayEvent::SurfaceCreateFailed {
        id: 0x7000_0009,
        error: "device rejected pipeline".to_string(),
    });
    match event {
        Some(KbInputEvent::SurfaceCreateFailed { id, error }) => {
            assert_eq!(id, 0x7000_0009);
            assert_eq!(error, "device rejected pipeline");
        }
        other => panic!("expected SurfaceCreateFailed, got {other:?}"),
    }
}

#[test]
fn frame_shader_failure_reaches_the_evaluator() {
    let event = convert_display_event(&DisplayEvent::FrameShaderFailed {
        error: "device rejected frame pipeline".to_owned(),
    });
    match event {
        Some(KbInputEvent::FrameShaderFailed { error }) => {
            assert_eq!(error, "device rejected frame pipeline");
        }
        other => panic!("expected FrameShaderFailed, got {other:?}"),
    }
}

#[test]
fn terminal_lifecycle_events_reach_the_evaluator_losslessly() {
    let id = neovm_core::emacs_core::display_host::TerminalId::new(17).unwrap();
    assert!(matches!(
        convert_display_event(&DisplayEvent::TerminalCreateFailed {
            id,
            error: "shell executable not found".to_owned(),
        }),
        Some(KbInputEvent::TerminalCreateFailed { id: actual, error })
            if actual == id && error == "shell executable not found"
    ));
    assert!(matches!(
        convert_display_event(&DisplayEvent::TerminalExited { id }),
        Some(KbInputEvent::TerminalExited { id: actual }) if actual == id
    ));
    assert!(matches!(
        convert_display_event(&DisplayEvent::TerminalTitleChanged {
            id,
            title: "project shell".to_owned(),
        }),
        Some(KbInputEvent::TerminalTitleChanged { id: actual, title })
            if actual == id && title == "project shell"
    ));
}

#[test]
fn presentation_lifecycle_events_reach_the_evaluator_losslessly() {
    assert!(matches!(
        convert_display_event(&DisplayEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        }),
        Some(KbInputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        })
    ));
    assert!(matches!(
        convert_display_event(&DisplayEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        }),
        Some(KbInputEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        })
    ));
}

#[test]
fn key_release_is_dropped_by_core_transport_owner() {
    let display_event = DisplayEvent::Key {
        keysym: keyboard::XK_RETURN,
        modifiers: 0,
        pressed: false,
        emacs_frame_id: 0,
    };
    let event = convert_display_event(&display_event);
    assert!(event.is_none());
}

#[test]
fn raw_tty_bytes_cross_the_bridge_without_interpretation() {
    let primary = convert_display_event(&DisplayEvent::RawTtyBytes {
        bytes: b"p".to_vec(),
        emacs_frame_id: 0,
    });
    assert!(matches!(
        primary,
        Some(KbInputEvent::RawTtyBytes {
            target: neovm_core::keyboard::TtyInputTarget::Terminal(0),
            ..
        })
    ));

    let event = convert_display_event(&DisplayEvent::RawTtyBytes {
        bytes: b"\x1b[A".to_vec(),
        emacs_frame_id: 42,
    });

    assert!(matches!(
        event.as_ref(),
        Some(KbInputEvent::RawTtyBytes {
            bytes,
            target: neovm_core::keyboard::TtyInputTarget::Frame(
                neovm_core::window::FrameId(42)
            ),
        }) if bytes == b"\x1b[A"
    ));
    assert!(!event.expect("raw event").requests_default_quit());

    let quit = convert_display_event(&DisplayEvent::RawTtyBytes {
        bytes: vec![0x07],
        emacs_frame_id: 42,
    })
    .expect("raw quit event");
    assert!(quit.requests_default_quit());
}

#[test]
fn key_transport_preserves_source_frame_identity() {
    let display_event = DisplayEvent::Key {
        keysym: 'a' as u32,
        modifiers: keyboard::RENDER_CTRL_MASK,
        pressed: true,
        emacs_frame_id: 42,
    };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::KeyPress {
            key,
            emacs_frame_id,
        }) => {
            assert_eq!(
                key,
                keyboard::KeyEvent::char_with_mods('a', keyboard::Modifiers::ctrl())
            );
            assert_eq!(emacs_frame_id, 42);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn mouse_modifiers_use_core_transport_mapping() {
    let display_event = unpresented_pointer(
        1.0,
        2.0,
        7,
        PointerAction::Move {
            modifiers: keyboard::RENDER_SHIFT_MASK | keyboard::RENDER_CTRL_MASK,
        },
    );
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::MouseMove {
            modifiers,
            target_frame_id,
            ..
        }) => {
            assert!(modifiers.shift);
            assert!(modifiers.ctrl);
            assert!(!modifiers.meta);
            assert_eq!(target_frame_id, 7);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn mouse_button_preserves_target_frame_for_keyboard_owner() {
    let display_event = unpresented_pointer(
        10.0,
        20.0,
        42,
        PointerAction::Button {
            button: 1,
            pressed: true,
            modifiers: 0,
        },
    );
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::MousePress {
            target_frame_id: 42,
            ..
        }) => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn popup_menu_selection_reaches_keyboard_owner() {
    let display_event = DisplayEvent::MenuSelection { index: 2 };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::MenuSelection { index: 2 }) => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn tool_bar_click_reaches_keyboard_owner() {
    let display_event = DisplayEvent::ToolBarClick {
        index: 3,
        emacs_frame_id: 42,
    };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::ToolBarClick {
            index: 3,
            emacs_frame_id: 42,
        }) => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn presented_pointer_reaches_keyboard_owner_without_losing_snapshot_or_phase() {
    let display_event = DisplayEvent::PresentedPointer {
        presentation: 9,
        interaction: 2,
        pressed: false,
        button: 1,
        x: 24.0,
        y: 8.0,
        emacs_frame_id: 42,
    };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::PresentedPointer {
            presentation: 9,
            interaction: 2,
            pressed: false,
            button: 1,
            x: 24.0,
            y: 8.0,
            emacs_frame_id: 42,
        }) => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn menu_bar_click_reaches_keyboard_owner() {
    let display_event = DisplayEvent::MenuBarClick {
        index: 1,
        key: "tools".to_string(),
        menu_x: 5.0,
        anchor: neomacs_display_runtime::thread_comm::PopupAnchorRect::new(80.0, 0.0, 56.0, 24.0),
        emacs_frame_id: 42,
    };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::MenuBarClick {
            index: 1,
            key,
            menu_x: 5.0,
            menu_y: 0.0,
            anchor_x: 80.0,
            anchor_y: 0.0,
            anchor_width: 56.0,
            anchor_height: 24.0,
            emacs_frame_id: 42,
        }) => assert_eq!(key, "tools"),
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn window_focus_preserves_frame_id_for_keyboard_owner() {
    let display_event = DisplayEvent::WindowFocus {
        focused: true,
        emacs_frame_id: 42,
    };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::Focus {
            focused: true,
            emacs_frame_id: 42,
        }) => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn window_close_preserves_frame_id_for_keyboard_owner() {
    let display_event = DisplayEvent::WindowClose { emacs_frame_id: 42 };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::WindowClose { emacs_frame_id: 42 }) => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn monitor_changes_convert_to_core_monitor_snapshot() {
    let display_event = DisplayEvent::MonitorsChanged {
        monitors: vec![DisplayMonitorInfo {
            x: 10,
            y: 20,
            width: 1920,
            height: 1080,
            scale: 1.5,
            width_mm: 510,
            height_mm: 290,
            name: Some("DP-1".to_string()),
        }],
    };
    let event = convert_display_event(&display_event);

    match event {
        Some(KbInputEvent::MonitorsChanged { monitors }) => {
            assert_eq!(monitors.len(), 1);
            assert_eq!(monitors[0].name.as_deref(), Some("DP-1"));
            assert_eq!(monitors[0].width, 1920);
            assert_eq!(monitors[0].height, 1080);
            assert_eq!(monitors[0].scale, 1.5);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn positioned_wheel_expands_to_observation_then_scroll_without_recomputing_the_hit() {
    let window = neomacs_display_protocol::DisplayWindowId::new(3);
    let hit = neomacs_display_protocol::PresentedHitIndex::from_parts(
        neomacs_display_protocol::PresentationId::new(9),
        vec![neomacs_display_protocol::PresentedHitRegion::new(
            Some(window),
            neomacs_display_protocol::PresentedRegionKind::ModeLine,
            neomacs_display_protocol::FrameRect::new(0.0, 20.0, 80.0, 10.0).unwrap(),
            0,
        )],
        vec![],
    )
    .unwrap()
    .resolve(neomacs_display_protocol::PresentedHitQuery::new(
        settled_point(neomacs_display_protocol::PresentationId::new(9), 5.0, 25.0),
    ))
    .unwrap();
    let events: Vec<_> = super::convert_display_event(&DisplayEvent::PositionedPointer(
        neomacs_display_runtime::thread_comm::PositionedPointerInput {
            position: neomacs_display_runtime::thread_comm::PointerPosition {
                x: 5.0,
                y: 25.0,
                target_frame_id: 44,
            },
            target: neomacs_display_runtime::thread_comm::PointerTarget::Presented {
                presentation: 9,
                hit,
            },
            action: neomacs_display_runtime::thread_comm::PointerAction::Scroll {
                delta: neomacs_display_runtime::thread_comm::ScrollDelta::Lines { x: 0.0, y: -1.0 },
                modifiers: 0,
            },
        },
    ))
    .into_iter()
    .collect();
    assert!(matches!(
        events.as_slice(),
        [KbInputEvent::PresentedRegion {
            presentation: 9,
            hit: forwarded,
            x: 5.0,
            y: 25.0,
            target_frame_id: 44,
        }, KbInputEvent::MouseScroll {
            delta_x: 0.0,
            delta_y: -1.0,
            x: 5.0,
            y: 25.0,
            target_frame_id: 44,
            ..
        }] if *forwarded == hit
    ));
}

#[test]
fn positioned_pixel_scroll_maps_typed_pixels_to_core_smooth_scroll() {
    let events: Vec<_> = super::convert_display_event(&unpresented_pointer(
        12.0,
        18.0,
        44,
        PointerAction::Scroll {
            delta: ScrollDelta::Pixels { x: 1.5, y: -7.25 },
            modifiers: keyboard::RENDER_SHIFT_MASK,
        },
    ))
    .into_iter()
    .collect();

    assert!(matches!(
        events.as_slice(),
        [KbInputEvent::PixelScroll {
            delta_x: 1.5,
            delta_y: -7.25,
            x: 12.0,
            y: 18.0,
            modifiers,
            target_frame_id: 44,
        }] if modifiers.shift
    ));
}
