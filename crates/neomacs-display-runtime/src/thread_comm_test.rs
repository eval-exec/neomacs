use super::*;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_protocol::{ImageId, ImageLoadAttempt, ImageLoadToken, ImageStateEvent};

fn test_image_load(image: u32, attempt: u64) -> ImageLoadToken {
    ImageLoadToken::new(
        ImageId::new(image),
        ImageLoadAttempt::new(attempt).expect("non-zero test load attempt"),
    )
}

#[test]
fn render_capabilities_are_one_shared_evaluator_visible_snapshot() {
    let (emacs, render) = ThreadComms::new().split();
    assert_eq!(
        emacs.capabilities.frame_shader_availability(),
        FrameShaderAvailability::Pending
    );

    render
        .capabilities
        .publish_frame_shader_availability(FrameShaderAvailability::SuppressedByQualityPolicy);

    assert_eq!(
        emacs.capabilities.frame_shader_availability(),
        FrameShaderAvailability::SuppressedByQualityPolicy
    );
}

#[test]
fn frame_shader_acknowledgements_are_generation_qualified_and_transactional() {
    let capabilities = SharedRenderCapabilities::new(FrameShaderAvailability::Available);
    let first = capabilities.prepare_frame_shader_request(true);
    let first_id = first.id();
    first.commit();
    assert_eq!(
        capabilities.frame_shader_execution(first_id),
        FrameShaderExecution::Pending
    );
    assert!(capabilities.acknowledge_frame_shader(first_id, FrameShaderExecution::Rejected));

    let abandoned = capabilities.prepare_frame_shader_request(true);
    let abandoned_id = abandoned.id();
    assert_eq!(
        capabilities.frame_shader_execution(abandoned_id),
        FrameShaderExecution::Pending
    );
    drop(abandoned);
    assert_eq!(
        capabilities.frame_shader_execution(first_id),
        FrameShaderExecution::Rejected,
        "an unqueued request must restore the previous acknowledged state"
    );

    let current = capabilities.prepare_frame_shader_request(true);
    let current_id = current.id();
    current.commit();
    assert!(
        !capabilities.acknowledge_frame_shader(first_id, FrameShaderExecution::Installed),
        "the renderer must suppress failures and acknowledgements for replaced requests"
    );
    assert_eq!(
        capabilities.frame_shader_execution(current_id),
        FrameShaderExecution::Pending,
        "a late acknowledgement cannot install an older request"
    );
}

#[test]
fn renderer_reset_invalidates_effective_shader_without_losing_the_request() {
    let capabilities = SharedRenderCapabilities::new(FrameShaderAvailability::Available);
    let prepared = capabilities.prepare_frame_shader_request(true);
    let request = prepared.id();
    prepared.commit();
    assert!(capabilities.acknowledge_frame_shader(request, FrameShaderExecution::Installed));

    capabilities.begin_renderer_reset();

    assert_eq!(
        capabilities.frame_shader_execution(request),
        FrameShaderExecution::Pending,
        "device teardown must stop advertising renderer-owned installation"
    );
    capabilities.publish_frame_shader_availability(FrameShaderAvailability::Available);
    assert_eq!(
        capabilities.frame_shader_execution(request),
        FrameShaderExecution::Pending,
        "hardware recovery remains pending until replay is acknowledged"
    );
}

fn sealed_test_state(mut state: FrameDisplayState) -> SealedFramePresentation {
    if state.presentation_id == neomacs_display_protocol::PresentationId::default() {
        state.presentation_id = neomacs_display_protocol::PresentationId::new(1);
    }
    state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
        neomacs_display_protocol::DisplayFrameId::new(1),
        state.presentation_id,
        None,
        neomacs_display_protocol::ParentFrameRect::new(0.0, 0.0, 1.0, 1.0).unwrap(),
        0,
    );
    state.presented_hit_index = neomacs_display_protocol::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![],
        vec![],
    )
    .unwrap();
    SealedFramePresentation::seal(state).unwrap()
}

fn unpresented_pointer(x: f32, y: f32, target_frame_id: u64, action: PointerAction) -> InputEvent {
    InputEvent::PositionedPointer(PositionedPointerInput {
        position: PointerPosition {
            x,
            y,
            target_frame_id,
        },
        target: PointerTarget::Unpresented,
        action,
    })
}

// ===================================================================
// Constants
// ===================================================================

#[test]
fn channel_capacity_constants() {
    assert_eq!(INPUT_CHANNEL_CAPACITY, 4096);
    assert_eq!(COMMAND_CHANNEL_CAPACITY, 64);
}

// ===================================================================
// ThreadComms
// ===================================================================

#[test]
fn thread_comms_new_constructs_all_channels() {
    let (_emacs, _render) = ThreadComms::new().split();
}

#[test]
fn thread_comms_input_channel_roundtrip() {
    let comms = ThreadComms::new();

    let event = InputEvent::Key {
        keysym: 65, // 'A'
        modifiers: 0,
        pressed: true,
        emacs_frame_id: 0,
    };

    comms.input_tx.send(event.clone()).unwrap();

    let received = comms.input_rx.try_recv().unwrap();
    match received {
        InputEvent::Key {
            keysym,
            modifiers,
            pressed,
            emacs_frame_id,
        } => {
            assert_eq!(keysym, 65);
            assert_eq!(modifiers, 0);
            assert!(pressed);
            assert_eq!(emacs_frame_id, 0);
        }
        other => panic!("Expected Key event, got {:?}", other),
    }
}

#[test]
fn thread_comms_cmd_channel_roundtrip() {
    let comms = ThreadComms::new();

    comms
        .cmd_tx
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();

    let received = comms.cmd_rx.try_recv().unwrap();
    match received {
        RenderCommand::Lifecycle(LifecycleCommand::Shutdown) => {} // ok
        other => panic!("Expected Shutdown, got {:?}", other),
    }
}

#[test]
fn thread_comms_frame_channel_roundtrip() {
    let comms = ThreadComms::new();

    let buf = FrameGlyphBuffer::new();
    let state = FrameDisplayState::from_frame_glyph_buffer(&buf);
    comms.frame_tx.send(sealed_test_state(state)).unwrap();

    let received = comms.frame_rx.try_recv().unwrap();
    assert_eq!(received.frame_pixel_width, 0.0);
    assert_eq!(received.frame_pixel_height, 0.0);
}

#[test]
fn thread_comms_frame_channel_is_unbounded() {
    let comms = ThreadComms::new();

    // Send many frames without blocking -- unbounded channel
    for i in 0..100 {
        let buf = FrameGlyphBuffer::with_size(i as f32, i as f32);
        let state = FrameDisplayState::from_frame_glyph_buffer(&buf);
        comms.frame_tx.send(sealed_test_state(state)).unwrap();
    }

    // Drain and verify
    for i in 0..100 {
        let received = comms.frame_rx.try_recv().unwrap();
        assert_eq!(received.frame_pixel_width, i as f32);
    }
}

#[test]
fn thread_comms_cmd_channel_bounded_capacity() {
    let comms = ThreadComms::new();

    // Fill up the command channel to capacity
    for _ in 0..COMMAND_CHANNEL_CAPACITY {
        comms
            .cmd_tx
            .try_send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
            .unwrap();
    }

    // Next try_send should fail (channel full)
    let result = comms
        .cmd_tx
        .try_send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown));
    assert!(
        result.is_err(),
        "cmd channel should be full after {} sends",
        COMMAND_CHANNEL_CAPACITY
    );
}

#[test]
fn thread_comms_input_channel_bounded_capacity() {
    let comms = ThreadComms::new();

    // Fill up the input channel to capacity
    for _ in 0..INPUT_CHANNEL_CAPACITY {
        let event = InputEvent::Key {
            keysym: 0,
            modifiers: 0,
            pressed: false,
            emacs_frame_id: 0,
        };
        comms.input_tx.try_send(event).unwrap();
    }

    // Next try_send should fail (channel full)
    let result = comms.input_tx.try_send(InputEvent::Key {
        keysym: 0,
        modifiers: 0,
        pressed: false,
        emacs_frame_id: 0,
    });
    assert!(
        result.is_err(),
        "input channel should be full after {} sends",
        INPUT_CHANNEL_CAPACITY
    );
}

#[test]
fn presentation_lifecycle_events_roundtrip_without_losing_frame_identity() {
    let comms = ThreadComms::new();

    comms
        .input_tx
        .send(InputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        })
        .unwrap();
    comms
        .input_tx
        .send(InputEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        })
        .unwrap();

    assert!(matches!(
        comms.input_rx.try_recv().unwrap(),
        InputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        }
    ));
    assert!(matches!(
        comms.input_rx.try_recv().unwrap(),
        InputEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        }
    ));
}

// ===================================================================
// ThreadComms::split()
// ===================================================================

#[test]
fn thread_comms_split_channels_work() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();

    // Emacs sends command, render receives
    emacs
        .cmd_tx
        .send(RenderCommand::Ui(UiCommand::VisualBell {
            frame: FrameRef::Frame(7),
        }))
        .unwrap();
    let cmd = render.cmd_rx.try_recv().unwrap();
    match cmd {
        RenderCommand::Ui(UiCommand::VisualBell { frame }) => assert_eq!(frame.raw_id(), 7),
        other => panic!("Expected VisualBell, got {:?}", other),
    }

    // Render sends input, Emacs receives
    render
        .input_tx
        .send(InputEvent::WindowClose { emacs_frame_id: 42 })
        .unwrap();
    let evt = emacs.input_rx.try_recv().unwrap();
    match evt {
        InputEvent::WindowClose { emacs_frame_id } => assert_eq!(emacs_frame_id, 42),
        other => panic!("Expected WindowClose, got {:?}", other),
    }

    // Emacs sends frame, render receives
    let buf = FrameGlyphBuffer::with_size(800.0, 600.0);
    let state = FrameDisplayState::from_frame_glyph_buffer(&buf);
    emacs.frame_tx.send(sealed_test_state(state)).unwrap();
    let frame = render.frame_rx.try_recv().unwrap();
    assert_eq!(frame.frame_pixel_width, 800.0);
    assert_eq!(frame.frame_pixel_height, 600.0);
}

// ===================================================================
// RenderComms::send_input()
// ===================================================================

#[test]
fn render_comms_send_input_delivers_event() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();

    render.send_input(unpresented_pointer(
        100.0,
        200.0,
        0,
        PointerAction::Move { modifiers: 0 },
    ));

    // Event should be receivable
    let evt = emacs.input_rx.try_recv().unwrap();
    match evt {
        InputEvent::PositionedPointer(PositionedPointerInput { position, .. }) => {
            assert_eq!(position.x, 100.0);
            assert_eq!(position.y, 200.0);
        }
        other => panic!("Expected PositionedPointer, got {:?}", other),
    }
}

// ===================================================================
// InputEvent enum variant construction
// ===================================================================

#[test]
fn input_event_key_construction() {
    let event = InputEvent::Key {
        keysym: 0xFF0D, // Return
        modifiers: 4,   // Ctrl
        pressed: true,
        emacs_frame_id: 0,
    };
    match event {
        InputEvent::Key {
            keysym,
            modifiers,
            pressed,
            emacs_frame_id,
        } => {
            assert_eq!(keysym, 0xFF0D);
            assert_eq!(modifiers, 4);
            assert!(pressed);
            assert_eq!(emacs_frame_id, 0);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_mouse_button_construction() {
    let event = unpresented_pointer(
        50.5,
        100.3,
        0,
        PointerAction::Button {
            button: 1,
            pressed: true,
            modifiers: 0,
        },
    );
    match event {
        InputEvent::PositionedPointer(PositionedPointerInput {
            position,
            target: PointerTarget::Unpresented,
            action:
                PointerAction::Button {
                    button,
                    pressed,
                    modifiers,
                },
        }) => {
            assert_eq!(
                position,
                PointerPosition {
                    x: 50.5,
                    y: 100.3,
                    target_frame_id: 0
                }
            );
            assert_eq!(button, 1);
            assert!(pressed);
            assert_eq!(modifiers, 0);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_mouse_move_construction() {
    let event = unpresented_pointer(200.0, 300.0, 42, PointerAction::Move { modifiers: 1 });
    match event {
        InputEvent::PositionedPointer(PositionedPointerInput {
            position,
            target: PointerTarget::Unpresented,
            action: PointerAction::Move { modifiers },
        }) => {
            assert_eq!(position.x, 200.0);
            assert_eq!(position.y, 300.0);
            assert_eq!(modifiers, 1);
            assert_eq!(position.target_frame_id, 42);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_mouse_scroll_construction() {
    let event = unpresented_pointer(
        400.0,
        500.0,
        0,
        PointerAction::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
            modifiers: 0,
        },
    );
    match event {
        InputEvent::PositionedPointer(PositionedPointerInput {
            action: PointerAction::Scroll { delta, .. },
            ..
        }) => {
            assert_eq!(delta, ScrollDelta::Lines { x: 0.0, y: -3.0 });
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_pixel_scroll_uses_a_distinct_delta_variant() {
    let event = unpresented_pointer(
        0.0,
        0.0,
        0,
        PointerAction::Scroll {
            delta: ScrollDelta::Pixels { x: 10.5, y: -25.3 },
            modifiers: 0,
        },
    );
    match event {
        InputEvent::PositionedPointer(PositionedPointerInput {
            action: PointerAction::Scroll { delta, .. },
            ..
        }) => assert_eq!(delta, ScrollDelta::Pixels { x: 10.5, y: -25.3 }),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_window_resize_construction() {
    let event = InputEvent::WindowResize {
        width: 1920,
        height: 1080,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    };
    match event {
        InputEvent::WindowResize {
            width,
            height,
            scale_factor,
            emacs_frame_id,
        } => {
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
            assert_eq!(scale_factor, 1.0);
            assert_eq!(emacs_frame_id, 0);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_window_close_construction() {
    let event = InputEvent::WindowClose {
        emacs_frame_id: 123,
    };
    match event {
        InputEvent::WindowClose { emacs_frame_id } => assert_eq!(emacs_frame_id, 123),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_window_focus_construction() {
    let focused = InputEvent::WindowFocus {
        focused: true,
        emacs_frame_id: 0,
    };
    match focused {
        InputEvent::WindowFocus {
            focused,
            emacs_frame_id,
        } => {
            assert!(focused);
            assert_eq!(emacs_frame_id, 0);
        }
        _ => panic!("Wrong variant"),
    }

    let unfocused = InputEvent::WindowFocus {
        focused: false,
        emacs_frame_id: 5,
    };
    match unfocused {
        InputEvent::WindowFocus {
            focused,
            emacs_frame_id,
        } => {
            assert!(!focused);
            assert_eq!(emacs_frame_id, 5);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_image_terminal_state_changed_construction() {
    let state = ImageStateEvent::Evicted(ImageId::new(7));
    let event = InputEvent::ImageStateChanged { event: state };
    match event {
        InputEvent::ImageStateChanged { event } => {
            assert_eq!(event, state);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_menu_selection_construction() {
    let selected = InputEvent::MenuSelection { index: 3 };
    match selected {
        InputEvent::MenuSelection { index } => assert_eq!(index, 3),
        _ => panic!("Wrong variant"),
    }

    let cancelled = InputEvent::MenuSelection { index: -1 };
    match cancelled {
        InputEvent::MenuSelection { index } => assert_eq!(index, -1),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_file_drop_construction() {
    let event = InputEvent::FileDrop {
        paths: vec![
            "/home/user/file.txt".to_string(),
            "/tmp/image.png".to_string(),
        ],
        x: 100.0,
        y: 200.0,
    };
    match event {
        InputEvent::FileDrop { paths, x, y } => {
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0], "/home/user/file.txt");
            assert_eq!(paths[1], "/tmp/image.png");
            assert_eq!(x, 100.0);
            assert_eq!(y, 200.0);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn input_event_clone() {
    let original = InputEvent::Key {
        keysym: 42,
        modifiers: 8,
        pressed: false,
        emacs_frame_id: 0,
    };
    let cloned = original.clone();
    match cloned {
        InputEvent::Key {
            keysym,
            modifiers,
            pressed,
            emacs_frame_id,
        } => {
            assert_eq!(keysym, 42);
            assert_eq!(modifiers, 8);
            assert!(!pressed);
            assert_eq!(emacs_frame_id, 0);
        }
        _ => panic!("Clone changed variant"),
    }
}

#[test]
fn input_event_debug() {
    let event = InputEvent::Key {
        keysym: 65,
        modifiers: 0,
        pressed: true,
        emacs_frame_id: 0,
    };
    let debug = format!("{:?}", event);
    assert!(
        debug.contains("Key"),
        "Debug output should contain variant name: {}",
        debug
    );
}

// ===================================================================
// RenderCommand enum variant construction
// ===================================================================

#[test]
fn render_command_shutdown() {
    let cmd = RenderCommand::Lifecycle(LifecycleCommand::Shutdown);
    match cmd {
        RenderCommand::Lifecycle(LifecycleCommand::Shutdown) => {}
        other => panic!("Expected Shutdown, got {:?}", other),
    }
}

#[test]
fn render_command_scroll_blit() {
    let cmd = RenderCommand::Window(WindowCommand::ScrollBlit {
        x: 0,
        y: 100,
        width: 800,
        height: 500,
        from_y: 100,
        to_y: 116,
        bg_r: 0.1,
        bg_g: 0.1,
        bg_b: 0.1,
    });
    match cmd {
        RenderCommand::Window(WindowCommand::ScrollBlit {
            x,
            y,
            width,
            height,
            from_y,
            to_y,
            bg_r,
            bg_g,
            bg_b,
        }) => {
            assert_eq!(x, 0);
            assert_eq!(y, 100);
            assert_eq!(width, 800);
            assert_eq!(height, 500);
            assert_eq!(from_y, 100);
            assert_eq!(to_y, 116);
            assert_eq!(bg_r, 0.1);
            assert_eq!(bg_g, 0.1);
            assert_eq!(bg_b, 0.1);
        }
        other => panic!("Expected ScrollBlit, got {:?}", other),
    }
}

#[test]
fn render_command_image_load_file() {
    let load = test_image_load(1, 1);
    let cmd = RenderCommand::Asset(AssetCommand::ImageLoadFile {
        load,
        path: "/home/user/photo.png".to_string(),
        size: neomacs_display_protocol::ImageSizeSpec::new(
            neomacs_display_protocol::AxisSize::AtMost(1024),
            neomacs_display_protocol::AxisSize::AtMost(768),
        ),
        rotation: neomacs_display_protocol::ImageRotation::None,
        realization: neomacs_display_protocol::ImageRealization::default(),
        colors: neomacs_display_protocol::ImageColorContext::default(),
        mask: neomacs_display_protocol::ImageMaskPolicy::default(),
        frame: neomacs_display_protocol::ImageFrameIndex::new(3),
        sequence: neomacs_display_protocol::ImageSequenceId::new(11)
            .expect("non-zero test sequence"),
    });
    match cmd {
        RenderCommand::Asset(AssetCommand::ImageLoadFile {
            load: actual_load,
            path,
            size,
            rotation: _,
            realization,
            colors,
            mask,
            frame,
            sequence,
        }) => {
            assert_eq!(actual_load, load);
            assert_eq!(path, "/home/user/photo.png");
            assert_eq!(
                size,
                neomacs_display_protocol::ImageSizeSpec::new(
                    neomacs_display_protocol::AxisSize::AtMost(1024),
                    neomacs_display_protocol::AxisSize::AtMost(768),
                )
            );
            assert_eq!(
                realization,
                neomacs_display_protocol::ImageRealization::default()
            );
            assert_eq!(
                colors,
                neomacs_display_protocol::ImageColorContext::default()
            );
            assert_eq!(mask, neomacs_display_protocol::ImageMaskPolicy::Preserve);
            assert_eq!(frame, neomacs_display_protocol::ImageFrameIndex::new(3));
            assert_eq!(
                sequence,
                neomacs_display_protocol::ImageSequenceId::new(11).expect("non-zero test sequence")
            );
        }
        other => panic!("Expected ImageLoadFile, got {:?}", other),
    }
}

#[test]
fn render_command_image_retire() {
    let image = ImageId::new(42);
    let cmd = RenderCommand::Asset(AssetCommand::ImageRetire { image });
    match cmd {
        RenderCommand::Asset(AssetCommand::ImageRetire { image: actual }) => {
            assert_eq!(actual, image)
        }
        other => panic!("Expected ImageRetire, got {:?}", other),
    }
}

#[test]
fn render_command_image_sequence_retirement_preserves_generation_fence() {
    let retirement = neomacs_display_protocol::ImageSequenceRetirement::AllocatedThrough(
        neomacs_display_protocol::ImageSequenceId::new(37).expect("non-zero test sequence"),
    );
    let command = RenderCommand::Asset(AssetCommand::ImageSequenceRetire { retirement });

    assert!(matches!(
        command,
        RenderCommand::Asset(AssetCommand::ImageSequenceRetire { retirement: actual })
            if actual == retirement
    ));
}

#[test]
fn webview_commands_and_events_cross_the_common_runtime_protocol() {
    use neomacs_display_protocol::WebViewId;
    use neomacs_webview::{
        BrowsingRelationship, FocusIntent, HistoryAction, NavigationTarget, ScriptRequest,
        ScriptRequestId, ScriptWorld, StoragePartition, WebContentSize, WebProfileId,
        WebViewCommand, WebViewCreate, WebViewEvent, WebViewGeneration, WebViewPolicy,
    };

    let id = WebViewId::new(17);
    let commands = [
        WebViewCommand::Create(WebViewCreate {
            id,
            storage: StoragePartition::Persistent(WebProfileId::new(1)),
            relationship: BrowsingRelationship::Independent,
            initial_size: WebContentSize::new(800, 600).unwrap(),
            policy: WebViewPolicy::default(),
            initial_navigation: Some(NavigationTarget::Uri("https://example.test".to_owned())),
        }),
        WebViewCommand::SetModelSize {
            id,
            size: WebContentSize::new(1024, 768).unwrap(),
        },
        WebViewCommand::Navigate {
            id,
            target: NavigationTarget::Uri("https://example.test/next".to_owned()),
        },
        WebViewCommand::History {
            id,
            action: HistoryAction::Reload,
        },
        WebViewCommand::EvaluateScript(ScriptRequest {
            request: ScriptRequestId::new(9),
            view: id,
            source: "document.title".to_owned(),
            world: ScriptWorld::Page,
        }),
        WebViewCommand::Focus {
            id,
            intent: FocusIntent::Focus,
        },
        WebViewCommand::Close { id },
    ];
    for command in commands {
        let command = RenderCommand::Asset(AssetCommand::WebView(command));
        let RenderCommand::Asset(AssetCommand::WebView(command)) = command else {
            panic!("expected nested webview command");
        };
        assert_eq!(command.id(), id);
    }

    let event = InputEvent::WebView(WebViewEvent::Ready {
        id,
        generation: WebViewGeneration::new(3),
    });
    assert!(!RenderComms::is_lossy_input_event(&event));
    assert_eq!(RenderComms::event_name(&event), "webview-ready");
}

#[test]
fn render_command_set_mouse_cursor() {
    let cmd = RenderCommand::Window(WindowCommand::SetMouseCursor { cursor_type: 2 });
    match cmd {
        RenderCommand::Window(WindowCommand::SetMouseCursor { cursor_type }) => {
            assert_eq!(cursor_type, 2)
        }
        other => panic!("Expected SetMouseCursor, got {:?}", other),
    }
}

#[test]
fn render_command_warp_mouse() {
    let cmd = RenderCommand::Window(WindowCommand::WarpMouse { x: 500, y: 300 });
    match cmd {
        RenderCommand::Window(WindowCommand::WarpMouse { x, y }) => {
            assert_eq!(x, 500);
            assert_eq!(y, 300);
        }
        other => panic!("Expected WarpMouse, got {:?}", other),
    }
}

#[test]
fn render_command_set_window_title() {
    let cmd = RenderCommand::Window(WindowCommand::SetWindowTitle {
        title: "Neomacs - main.rs".to_string(),
    });
    match cmd {
        RenderCommand::Window(WindowCommand::SetWindowTitle { title }) => {
            assert_eq!(title, "Neomacs - main.rs");
        }
        other => panic!("Expected SetWindowTitle, got {:?}", other),
    }
}

#[test]
fn render_command_set_window_fullscreen() {
    for mode in [
        WindowFullscreenMode::None,
        WindowFullscreenMode::Fullboth,
        WindowFullscreenMode::Fullscreen,
        WindowFullscreenMode::Fullwidth,
        WindowFullscreenMode::Fullheight,
        WindowFullscreenMode::Maximized,
    ] {
        let cmd = RenderCommand::Window(WindowCommand::SetWindowFullscreen {
            frame: FrameRef::Frame(99),
            mode,
        });
        match cmd {
            RenderCommand::Window(WindowCommand::SetWindowFullscreen { frame, mode: m }) => {
                assert_eq!(frame, FrameRef::Frame(99));
                assert_eq!(m, mode);
            }
            other => panic!("Expected SetWindowFullscreen, got {:?}", other),
        }
    }
}

#[test]
fn render_command_set_window_minimized() {
    let cmd = RenderCommand::Window(WindowCommand::SetWindowMinimized { minimized: true });
    match cmd {
        RenderCommand::Window(WindowCommand::SetWindowMinimized { minimized }) => {
            assert!(minimized)
        }
        other => panic!("Expected SetWindowMinimized, got {:?}", other),
    }
}

#[test]
fn render_command_set_window_position() {
    let cmd = RenderCommand::Window(WindowCommand::SetWindowPosition { x: 100, y: 200 });
    match cmd {
        RenderCommand::Window(WindowCommand::SetWindowPosition { x, y }) => {
            assert_eq!(x, 100);
            assert_eq!(y, 200);
        }
        other => panic!("Expected SetWindowPosition, got {:?}", other),
    }
}

#[test]
fn render_command_set_window_size() {
    let cmd = RenderCommand::Window(WindowCommand::SetWindowSize {
        width: 1280,
        height: 720,
    });
    match cmd {
        RenderCommand::Window(WindowCommand::SetWindowSize { width, height }) => {
            assert_eq!(width, 1280);
            assert_eq!(height, 720);
        }
        other => panic!("Expected SetWindowSize, got {:?}", other),
    }
}

#[test]
fn render_command_resize_window() {
    let geometry_hints = GuiFrameGeometryHints {
        base_width: 42,
        base_height: 58,
        min_width: 42,
        min_height: 58,
        width_inc: 26,
        height_inc: 58,
    };
    let cmd = RenderCommand::Window(WindowCommand::ResizeWindow {
        frame: FrameRef::Frame(99),
        width: 1024,
        height: 768,
        geometry_hints,
    });
    match cmd {
        RenderCommand::Window(WindowCommand::ResizeWindow {
            frame,
            width,
            height,
            geometry_hints: actual_hints,
        }) => {
            assert_eq!(frame.raw_id(), 99);
            assert_eq!(width, 1024);
            assert_eq!(height, 768);
            assert_eq!(actual_hints, geometry_hints);
        }
        other => panic!("Expected ResizeWindow, got {:?}", other),
    }
}

#[test]
fn render_command_set_frame_geometry_hints() {
    let geometry_hints = GuiFrameGeometryHints {
        base_width: 42,
        base_height: 58,
        min_width: 42,
        min_height: 58,
        width_inc: 26,
        height_inc: 58,
    };
    let cmd = RenderCommand::Window(WindowCommand::SetFrameGeometryHints {
        frame: FrameRef::Primary,
        geometry_hints,
    });
    match cmd {
        RenderCommand::Window(WindowCommand::SetFrameGeometryHints {
            frame,
            geometry_hints: actual_hints,
        }) => {
            assert_eq!(frame.raw_id(), 0);
            assert_eq!(actual_hints, geometry_hints);
        }
        other => panic!("Expected SetFrameGeometryHints, got {:?}", other),
    }
}

#[test]
fn render_command_set_window_decorated() {
    let cmd = RenderCommand::Window(WindowCommand::SetWindowDecorated { decorated: false });
    match cmd {
        RenderCommand::Window(WindowCommand::SetWindowDecorated { decorated }) => {
            assert!(!decorated)
        }
        other => panic!("Expected SetWindowDecorated, got {:?}", other),
    }
}

#[test]
fn render_command_show_popup_menu() {
    let items = vec![
        PopupMenuItem {
            label: "Open".to_string(),
            shortcut: "C-x C-f".to_string(),
            enabled: true,
            separator: false,
            submenu: false,
            depth: 0,
        },
        PopupMenuItem {
            label: String::new(),
            shortcut: String::new(),
            enabled: false,
            separator: true,
            submenu: false,
            depth: 0,
        },
        PopupMenuItem {
            label: "Quit".to_string(),
            shortcut: "C-x C-c".to_string(),
            enabled: true,
            separator: false,
            submenu: false,
            depth: 0,
        },
    ];

    let cmd = RenderCommand::Ui(UiCommand::ShowPopupMenu {
        frame: FrameRef::Frame(0x1000),
        placement: neomacs_display_protocol::PopupPlacement::at(
            neomacs_display_protocol::Point::new(100.0, 200.0),
        ),
        items: items.clone(),
        title: Some("File".to_string()),
        fg: Some((1.0, 1.0, 1.0)),
        bg: Some((0.1, 0.1, 0.1)),
    });
    match cmd {
        RenderCommand::Ui(UiCommand::ShowPopupMenu {
            frame,
            placement,
            items: menu_items,
            title,
            fg,
            bg,
        }) => {
            assert_eq!(frame.raw_id(), 0x1000);
            assert_eq!(
                placement,
                neomacs_display_protocol::PopupPlacement::at(neomacs_display_protocol::Point::new(
                    100.0, 200.0
                ),)
            );
            assert_eq!(menu_items.len(), 3);
            assert_eq!(menu_items[0].label, "Open");
            assert_eq!(menu_items[0].shortcut, "C-x C-f");
            assert!(menu_items[0].enabled);
            assert!(menu_items[1].separator);
            assert!(!menu_items[1].enabled);
            assert_eq!(title, Some("File".to_string()));
            assert_eq!(fg, Some((1.0, 1.0, 1.0)));
            assert_eq!(bg, Some((0.1, 0.1, 0.1)));
        }
        other => panic!("Expected ShowPopupMenu, got {:?}", other),
    }
}

#[test]
fn render_command_hide_popup_menu() {
    let cmd = RenderCommand::Ui(UiCommand::HidePopupMenu);
    match cmd {
        RenderCommand::Ui(UiCommand::HidePopupMenu) => {}
        other => panic!("Expected HidePopupMenu, got {:?}", other),
    }
}

#[test]
fn render_command_show_tooltip() {
    let cmd = RenderCommand::Ui(UiCommand::ShowTooltip {
        frame: FrameRef::Frame(0x2000),
        x: 300.0,
        y: 400.0,
        text: "This is a tooltip".to_string(),
        fg_r: 1.0,
        fg_g: 1.0,
        fg_b: 1.0,
        bg_r: 0.0,
        bg_g: 0.0,
        bg_b: 0.0,
    });
    match cmd {
        RenderCommand::Ui(UiCommand::ShowTooltip {
            frame,
            x,
            y,
            text,
            fg_r,
            fg_g: _,
            fg_b: _,
            bg_r,
            bg_g: _,
            bg_b: _,
        }) => {
            assert_eq!(frame.raw_id(), 0x2000);
            assert_eq!(x, 300.0);
            assert_eq!(y, 400.0);
            assert_eq!(text, "This is a tooltip");
            assert_eq!(fg_r, 1.0);
            assert_eq!(bg_r, 0.0);
        }
        other => panic!("Expected ShowTooltip, got {:?}", other),
    }
}

#[test]
fn render_command_hide_tooltip() {
    let cmd = RenderCommand::Ui(UiCommand::HideTooltip);
    match cmd {
        RenderCommand::Ui(UiCommand::HideTooltip) => {}
        other => panic!("Expected HideTooltip, got {:?}", other),
    }
}

#[test]
fn render_command_visual_bell() {
    let cmd = RenderCommand::Ui(UiCommand::VisualBell {
        frame: FrameRef::Frame(99),
    });
    match cmd {
        RenderCommand::Ui(UiCommand::VisualBell { frame }) => assert_eq!(frame.raw_id(), 99),
        other => panic!("Expected VisualBell, got {:?}", other),
    }
}

#[test]
fn render_command_request_attention() {
    let cmd = RenderCommand::Window(WindowCommand::RequestAttention { urgent: true });
    match cmd {
        RenderCommand::Window(WindowCommand::RequestAttention { urgent }) => assert!(urgent),
        other => panic!("Expected RequestAttention, got {:?}", other),
    }
}

#[test]
fn render_command_update_visual_config() {
    let cmd = RenderCommand::Config(ConfigCommand::SetVisualConfig(VisualConfig::default()));
    match cmd {
        RenderCommand::Config(ConfigCommand::SetVisualConfig(_)) => {}
        other => panic!("Expected SetVisualConfig, got {:?}", other),
    }
}

#[test]
fn render_command_set_scroll_indicators() {
    let cmd = RenderCommand::Config(ConfigCommand::SetScrollIndicators { enabled: true });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetScrollIndicators { enabled }) => assert!(enabled),
        other => panic!("Expected SetScrollIndicators, got {:?}", other),
    }
}

#[test]
fn render_command_set_titlebar_height() {
    let cmd = RenderCommand::Config(ConfigCommand::SetTitlebarHeight { height: 32.0 });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetTitlebarHeight { height }) => {
            assert_eq!(height, 32.0)
        }
        other => panic!("Expected SetTitlebarHeight, got {:?}", other),
    }
}

#[test]
fn render_command_set_show_fps() {
    let cmd = RenderCommand::Config(ConfigCommand::SetShowFps { enabled: true });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetShowFps { enabled }) => assert!(enabled),
        other => panic!("Expected SetShowFps, got {:?}", other),
    }
}

#[test]
fn render_command_set_corner_radius() {
    let cmd = RenderCommand::Config(ConfigCommand::SetCornerRadius { radius: 8.0 });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetCornerRadius { radius }) => assert_eq!(radius, 8.0),
        other => panic!("Expected SetCornerRadius, got {:?}", other),
    }
}

#[test]
fn render_command_set_extra_spacing() {
    let cmd = RenderCommand::Config(ConfigCommand::SetExtraSpacing {
        line_spacing: 2.0,
        letter_spacing: 0.5,
    });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetExtraSpacing {
            line_spacing,
            letter_spacing,
        }) => {
            assert_eq!(line_spacing, 2.0);
            assert_eq!(letter_spacing, 0.5);
        }
        other => panic!("Expected SetExtraSpacing, got {:?}", other),
    }
}

#[test]
fn render_command_set_ligatures_enabled() {
    let cmd = RenderCommand::Config(ConfigCommand::SetLigaturesEnabled { enabled: true });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetLigaturesEnabled { enabled }) => assert!(enabled),
        other => panic!("Expected SetLigaturesEnabled, got {:?}", other),
    }
}

#[test]
fn render_command_remove_child_frame() {
    let cmd = RenderCommand::Window(WindowCommand::RemoveChildFrame { frame_id: 0xDEAD });
    match cmd {
        RenderCommand::Window(WindowCommand::RemoveChildFrame { frame_id }) => {
            assert_eq!(frame_id, 0xDEAD)
        }
        other => panic!("Expected RemoveChildFrame, got {:?}", other),
    }
}

#[test]
fn render_command_show_child_frame() {
    let cmd = RenderCommand::Window(WindowCommand::ShowChildFrame { frame_id: 0xBEEF });
    match cmd {
        RenderCommand::Window(WindowCommand::ShowChildFrame { frame_id }) => {
            assert_eq!(frame_id, 0xBEEF)
        }
        other => panic!("Expected ShowChildFrame, got {:?}", other),
    }
}

#[test]
fn render_command_create_window() {
    let geometry_hints = GuiFrameGeometryHints {
        base_width: 42,
        base_height: 58,
        min_width: 42,
        min_height: 58,
        width_inc: 26,
        height_inc: 58,
    };
    let cmd = RenderCommand::Window(WindowCommand::CreateWindow {
        frame: FrameRef::Frame(99),
        width: 1024,
        height: 768,
        title: "New Frame".to_string(),
        geometry_hints,
    });
    match cmd {
        RenderCommand::Window(WindowCommand::CreateWindow {
            frame,
            width,
            height,
            title,
            geometry_hints: actual_hints,
        }) => {
            assert_eq!(frame.raw_id(), 99);
            assert_eq!(width, 1024);
            assert_eq!(height, 768);
            assert_eq!(title, "New Frame");
            assert_eq!(actual_hints, geometry_hints);
        }
        other => panic!("Expected CreateWindow, got {:?}", other),
    }
}

#[test]
fn render_command_destroy_window() {
    let cmd = RenderCommand::Window(WindowCommand::DestroyWindow {
        frame: FrameRef::Frame(99),
    });
    match cmd {
        RenderCommand::Window(WindowCommand::DestroyWindow { frame }) => {
            assert_eq!(frame.raw_id(), 99)
        }
        other => panic!("Expected DestroyWindow, got {:?}", other),
    }
}

#[test]
fn render_command_set_child_frame_style() {
    let cmd = RenderCommand::Config(ConfigCommand::SetChildFrameStyle {
        corner_radius: 12.0,
        shadow_enabled: true,
        shadow_layers: 3,
        shadow_offset: 4.0,
        shadow_opacity: 0.5,
    });
    match cmd {
        RenderCommand::Config(ConfigCommand::SetChildFrameStyle {
            corner_radius,
            shadow_enabled,
            shadow_layers,
            shadow_offset,
            shadow_opacity,
        }) => {
            assert_eq!(corner_radius, 12.0);
            assert!(shadow_enabled);
            assert_eq!(shadow_layers, 3);
            assert_eq!(shadow_offset, 4.0);
            assert_eq!(shadow_opacity, 0.5);
        }
        other => panic!("Expected SetChildFrameStyle, got {:?}", other),
    }
}

#[test]
fn render_command_video_lifecycle() {
    let create = RenderCommand::Asset(AssetCommand::VideoCreate {
        id: 1,
        source: MediaSource::File("/home/user/video.mp4".to_string()),
        loop_count: -1,
        autoplay: true,
    });
    match create {
        RenderCommand::Asset(AssetCommand::VideoCreate {
            id,
            source,
            loop_count,
            autoplay,
        }) => {
            assert_eq!(id, 1);
            assert!(matches!(
                source,
                MediaSource::File(path) if path == "/home/user/video.mp4"
            ));
            assert_eq!(loop_count, -1);
            assert!(autoplay);
        }
        other => panic!("Expected VideoCreate, got {:?}", other),
    }

    let play = RenderCommand::Asset(AssetCommand::VideoPlay { id: 1 });
    match play {
        RenderCommand::Asset(AssetCommand::VideoPlay { id }) => assert_eq!(id, 1),
        other => panic!("Expected VideoPlay, got {:?}", other),
    }

    let pause = RenderCommand::Asset(AssetCommand::VideoPause { id: 1 });
    match pause {
        RenderCommand::Asset(AssetCommand::VideoPause { id }) => assert_eq!(id, 1),
        other => panic!("Expected VideoPause, got {:?}", other),
    }

    let destroy = RenderCommand::Asset(AssetCommand::VideoDestroy { id: 1 });
    match destroy {
        RenderCommand::Asset(AssetCommand::VideoDestroy { id }) => assert_eq!(id, 1),
        other => panic!("Expected VideoDestroy, got {:?}", other),
    }
}

#[test]
fn render_command_debug() {
    let cmd = RenderCommand::Lifecycle(LifecycleCommand::Shutdown);
    let debug = format!("{:?}", cmd);
    assert!(debug.contains("Shutdown"), "Debug output: {}", debug);
}

// ===================================================================
// PopupMenuItem
// ===================================================================

#[test]
fn popup_menu_item_construction() {
    let item = PopupMenuItem {
        label: "Save".to_string(),
        shortcut: "C-x C-s".to_string(),
        enabled: true,
        separator: false,
        submenu: false,
        depth: 0,
    };
    assert_eq!(item.label, "Save");
    assert_eq!(item.shortcut, "C-x C-s");
    assert!(item.enabled);
    assert!(!item.separator);
    assert!(!item.submenu);
    assert_eq!(item.depth, 0);
}

#[test]
fn popup_menu_item_separator() {
    let sep = PopupMenuItem {
        label: String::new(),
        shortcut: String::new(),
        enabled: false,
        separator: true,
        submenu: false,
        depth: 0,
    };
    assert!(sep.separator);
    assert!(!sep.enabled);
}

#[test]
fn popup_menu_item_submenu() {
    let sub = PopupMenuItem {
        label: "Recent Files".to_string(),
        shortcut: String::new(),
        enabled: true,
        separator: false,
        submenu: true,
        depth: 1,
    };
    assert!(sub.submenu);
    assert_eq!(sub.depth, 1);
}

#[test]
fn popup_menu_item_clone() {
    let item = PopupMenuItem {
        label: "Test".to_string(),
        shortcut: "M-x".to_string(),
        enabled: true,
        separator: false,
        submenu: false,
        depth: 2,
    };
    let cloned = item.clone();
    assert_eq!(cloned.label, "Test");
    assert_eq!(cloned.depth, 2);
}

#[test]
fn popup_menu_item_debug() {
    let item = PopupMenuItem {
        label: "Debug".to_string(),
        shortcut: String::new(),
        enabled: true,
        separator: false,
        submenu: false,
        depth: 0,
    };
    let debug = format!("{:?}", item);
    assert!(debug.contains("PopupMenuItem"), "Debug output: {}", debug);
}

// ===================================================================
// Channel operations: send through crossbeam, receive correctly
// ===================================================================

#[test]
fn channel_sends_multiple_input_events_in_order() {
    let comms = ThreadComms::new();

    let events = vec![
        InputEvent::Key {
            keysym: 1,
            modifiers: 0,
            pressed: true,
            emacs_frame_id: 0,
        },
        InputEvent::Key {
            keysym: 2,
            modifiers: 0,
            pressed: true,
            emacs_frame_id: 0,
        },
        InputEvent::Key {
            keysym: 3,
            modifiers: 0,
            pressed: true,
            emacs_frame_id: 0,
        },
        unpresented_pointer(10.0, 20.0, 0, PointerAction::Move { modifiers: 0 }),
        InputEvent::WindowResize {
            width: 800,
            height: 600,
            scale_factor: 1.0,
            emacs_frame_id: 0,
        },
    ];

    for e in &events {
        comms.input_tx.send(e.clone()).unwrap();
    }

    // Receive and verify order
    for (i, expected) in events.iter().enumerate() {
        let received = comms.input_rx.try_recv().unwrap();
        let expected_debug = format!("{:?}", expected);
        let received_debug = format!("{:?}", received);
        assert_eq!(
            expected_debug, received_debug,
            "Event {} mismatch: expected {:?}, got {:?}",
            i, expected_debug, received_debug
        );
    }

    // No more events
    assert!(comms.input_rx.try_recv().is_err());
}

#[test]
fn channel_sends_multiple_commands_in_order() {
    let comms = ThreadComms::new();

    comms
        .cmd_tx
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();
    comms
        .cmd_tx
        .send(RenderCommand::Ui(UiCommand::VisualBell {
            frame: FrameRef::Primary,
        }))
        .unwrap();
    comms
        .cmd_tx
        .send(RenderCommand::Ui(UiCommand::HideTooltip))
        .unwrap();

    match comms.cmd_rx.try_recv().unwrap() {
        RenderCommand::Lifecycle(LifecycleCommand::Shutdown) => {}
        other => panic!("Expected Shutdown, got {:?}", other),
    }
    match comms.cmd_rx.try_recv().unwrap() {
        RenderCommand::Ui(UiCommand::VisualBell { frame }) => assert_eq!(frame.raw_id(), 0),
        other => panic!("Expected VisualBell, got {:?}", other),
    }
    match comms.cmd_rx.try_recv().unwrap() {
        RenderCommand::Ui(UiCommand::HideTooltip) => {}
        other => panic!("Expected HideTooltip, got {:?}", other),
    }

    assert!(comms.cmd_rx.try_recv().is_err());
}

#[test]
fn channel_empty_recv_returns_error() {
    let comms = ThreadComms::new();
    assert!(comms.input_rx.try_recv().is_err());
    assert!(comms.cmd_rx.try_recv().is_err());
    assert!(comms.frame_rx.try_recv().is_err());
}

// ===================================================================
// Cross-thread usage simulation
// ===================================================================

#[test]
fn cross_thread_input_event_delivery() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();

    let handle = std::thread::spawn(move || {
        render.send_input(InputEvent::Key {
            keysym: 0x61, // 'a'
            modifiers: 0,
            pressed: true,
            emacs_frame_id: 0,
        });
        render.send_input(InputEvent::WindowResize {
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            emacs_frame_id: 0,
        });
    });

    handle.join().unwrap();

    // Both events should be receivable on the Emacs side
    let evt1 = emacs.input_rx.try_recv().unwrap();
    match evt1 {
        InputEvent::Key { keysym, .. } => assert_eq!(keysym, 0x61),
        other => panic!("Expected Key, got {:?}", other),
    }

    let evt2 = emacs.input_rx.try_recv().unwrap();
    match evt2 {
        InputEvent::WindowResize { width, height, .. } => {
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
        }
        other => panic!("Expected WindowResize, got {:?}", other),
    }
}

#[test]
fn cross_thread_command_delivery() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();

    let handle = std::thread::spawn(move || {
        let cmd = render.cmd_rx.recv().unwrap();
        match cmd {
            RenderCommand::Window(WindowCommand::SetWindowTitle { title }) => {
                assert_eq!(title, "test-title");
            }
            other => panic!("Expected SetWindowTitle, got {:?}", other),
        }
    });

    emacs
        .cmd_tx
        .send(RenderCommand::Window(WindowCommand::SetWindowTitle {
            title: "test-title".to_string(),
        }))
        .unwrap();

    handle.join().unwrap();
}

#[test]
fn cross_thread_frame_delivery() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();

    let handle = std::thread::spawn(move || {
        let frame = render.frame_rx.recv().unwrap();
        assert_eq!(frame.frame_pixel_width, 1920.0);
        assert_eq!(frame.frame_pixel_height, 1080.0);
    });

    let buf = FrameGlyphBuffer::with_size(1920.0, 1080.0);
    let state = FrameDisplayState::from_frame_glyph_buffer(&buf);
    emacs.frame_tx.send(sealed_test_state(state)).unwrap();

    handle.join().unwrap();
}
