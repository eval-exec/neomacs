use super::*;

// Behavior expectations, independent of the production scheduling enum.
#[derive(Debug, PartialEq, Eq)]
enum FrontendEventClass {
    Command,
    LispSpecial,
    Internal,
}

#[derive(Clone, Copy)]
enum PendingPolicy {
    Always,
    Never,
    TrackMouse,
    Focus { focused: bool },
    Filterable(&'static str),
}

#[test]
fn frame_shader_failure_is_visible_without_optional_lisp_library() {
    let mut eval = crate::emacs_core::Context::new();
    assert!(
        eval.obarray
            .symbol_value("neomacs-frame-shader-error-functions")
            .is_none(),
        "the -Q primitive path starts without neomacs-surface.el"
    );

    let effects = report_frame_shader_failure(&mut eval, "device rejected module")
        .expect("the fallback reporter must not hide the original failure");

    assert_eq!(
        eval.current_message_text().as_deref(),
        Some("neomacs frame shader failed to build: device rejected module")
    );
    assert!(effects.redisplay_needed);
}

#[test]
fn frame_shader_failure_uses_customizable_hook_when_loaded() {
    let mut eval = crate::emacs_core::Context::new();
    eval.eval_str(
        r#"(setq neomacs-frame-shader-error-functions
                 (list (lambda (error)
                         (setq neomacs-frame-shader-test-error error))))"#,
    )
    .expect("install frame shader error hook");

    let effects = report_frame_shader_failure(&mut eval, "backend detail")
        .expect("frame shader hook should run");

    let captured = eval
        .obarray
        .symbol_value("neomacs-frame-shader-test-error")
        .copied()
        .expect("hook captured the renderer error");
    assert_eq!(
        captured
            .as_lisp_string()
            .and_then(|string| string.as_utf8_str()),
        Some("backend detail")
    );
    assert!(effects.redisplay_needed);
}

fn assert_policy(
    event: InputEvent,
    class: FrontendEventClass,
    pending: PendingPolicy,
    expected_interrupts: bool,
    wait_special: bool,
) {
    let actual_class = match semantics(&event) {
        FrontendEventSemantics::Command => FrontendEventClass::Command,
        FrontendEventSemantics::Internal(_) => FrontendEventClass::Internal,
        FrontendEventSemantics::MouseMotion
        | FrontendEventSemantics::ServiceDuringWait
        | FrontendEventSemantics::SpecialInput { .. } => FrontendEventClass::LispSpecial,
    };
    assert_eq!(actual_class, class, "class for {event:?}");
    assert_eq!(
        interrupts(&event),
        expected_interrupts,
        "interrupt policy for {event:?}"
    );
    for track_mouse in [false, true] {
        let (expected_pending, ignored_symbol) = match pending {
            PendingPolicy::Always => (true, None),
            PendingPolicy::Never => (false, None),
            PendingPolicy::TrackMouse => (track_mouse, None),
            PendingPolicy::Focus { focused } => {
                (true, Some(if focused { "focus-in" } else { "focus-out" }))
            }
            PendingPolicy::Filterable(symbol) => (true, Some(symbol)),
        };
        assert_eq!(
            counts_as_input(
                &event,
                FrontendInputQuery::Pending(InputPendingFilter::ConfiguredIgnoreList),
                track_mouse,
                &|_| false
            ),
            expected_pending,
            "pending policy for {event:?}, track-mouse={track_mouse}"
        );
        assert_eq!(
            counts_as_input(
                &event,
                FrontendInputQuery::Pending(InputPendingFilter::ConfiguredIgnoreList),
                track_mouse,
                &|symbol| Some(symbol) == ignored_symbol
            ),
            expected_pending && ignored_symbol.is_none(),
            "filtered pending policy for {event:?}, track-mouse={track_mouse}"
        );
        assert_eq!(
            counts_as_input(&event, FrontendInputQuery::Readable, track_mouse, &|_| true),
            expected_pending,
            "a reader must not apply input-pending-p filters for {event:?}"
        );
        assert_eq!(
            is_wait_special(&event, track_mouse),
            wait_special && !(matches!(pending, PendingPolicy::TrackMouse) && track_mouse),
            "wait policy for {event:?}, track-mouse={track_mouse}"
        );
    }
}

#[test]
fn every_transport_variant_has_locked_down_semantics() {
    use crate::keyboard::{KeyEvent, Modifiers, MouseButton};

    let command_events = [
        InputEvent::raw_tty_bytes(vec![0x1b], 0),
        InputEvent::TtyByte {
            byte: b'k',
            target: crate::keyboard::TtyInputTarget::SelectedFrame,
        },
        InputEvent::TtyCharacter {
            character: crate::emacs_core::emacs_char::EmacsChar::from_char('k'),
            target: crate::keyboard::TtyInputTarget::SelectedFrame,
        },
        InputEvent::key_press(KeyEvent::char('k')),
        InputEvent::MousePress {
            button: MouseButton::Left,
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::none(),
            target_frame_id: 0,
        },
        InputEvent::MouseRelease {
            button: MouseButton::Left,
            x: 0.0,
            y: 0.0,
            target_frame_id: 0,
        },
        InputEvent::MouseScroll {
            delta_x: 0.0,
            delta_y: 1.0,
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::none(),
            target_frame_id: 0,
        },
        InputEvent::MenuSelection {
            index: 0,
            token: None,
        },
        InputEvent::ToolBarClick {
            index: 0,
            emacs_frame_id: 0,
        },
        InputEvent::PresentedPointer {
            presentation: 1,
            interaction: 0,
            pressed: true,
            button: 1,
            x: 0.0,
            y: 0.0,
            emacs_frame_id: 0,
        },
        InputEvent::MenuBarClick {
            request_id: None,
            index: 0,
            key: "file".to_string(),
            menu_x: 0.0,
            menu_y: 0.0,
            anchor_x: 0.0,
            anchor_y: 0.0,
            anchor_width: 0.0,
            anchor_height: 0.0,
            emacs_frame_id: 0,
        },
    ];
    for event in command_events {
        assert_policy(
            event,
            FrontendEventClass::Command,
            PendingPolicy::Always,
            true,
            false,
        );
    }

    assert_policy(
        InputEvent::MouseMove {
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::none(),
            target_frame_id: 0,
        },
        FrontendEventClass::LispSpecial,
        PendingPolicy::TrackMouse,
        false,
        true,
    );
    assert_policy(
        InputEvent::PixelScroll {
            delta_x: 0.0,
            delta_y: 1.0,
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::none(),
            target_frame_id: 0,
        },
        FrontendEventClass::Command,
        PendingPolicy::Always,
        true,
        false,
    );
    assert_policy(
        InputEvent::Resize {
            width: 1,
            height: 1,
            scale_factor: 1.0,
            emacs_frame_id: 0,
        },
        FrontendEventClass::LispSpecial,
        PendingPolicy::Never,
        false,
        true,
    );
    assert_policy(
        InputEvent::DisplayReset,
        FrontendEventClass::LispSpecial,
        PendingPolicy::Never,
        false,
        true,
    );
    assert_policy(
        InputEvent::Focus {
            focused: true,
            emacs_frame_id: 0,
        },
        FrontendEventClass::LispSpecial,
        PendingPolicy::Focus { focused: true },
        false,
        false,
    );
    assert_policy(
        InputEvent::MonitorsChanged { monitors: vec![] },
        FrontendEventClass::LispSpecial,
        PendingPolicy::Filterable("monitors-changed"),
        false,
        true,
    );
    assert_policy(
        InputEvent::SelectWindow {
            window_id: crate::window::WindowId(1),
        },
        FrontendEventClass::LispSpecial,
        PendingPolicy::Filterable("select-window"),
        true,
        false,
    );
    assert_policy(
        InputEvent::WindowClose { emacs_frame_id: 0 },
        FrontendEventClass::LispSpecial,
        PendingPolicy::Always,
        true,
        true,
    );
    assert_policy(
        InputEvent::PresentedRegion {
            presentation: 1,
            hit: None,
            x: 22.0,
            y: 12.0,
            target_frame_id: 0,
        },
        FrontendEventClass::Internal,
        PendingPolicy::Never,
        false,
        false,
    );
    assert_policy(
        InputEvent::LayoutInvalidated,
        FrontendEventClass::Internal,
        PendingPolicy::Never,
        false,
        false,
    );
    assert_policy(
        InputEvent::ImageStateChanged {
            event: neomacs_display_protocol::ImageStateEvent::Evicted(
                neomacs_display_protocol::ImageId::new(7),
            ),
        },
        FrontendEventClass::Internal,
        PendingPolicy::Never,
        false,
        false,
    );
    assert_policy(
        InputEvent::PresentationRetired { presentation: 1 },
        FrontendEventClass::Internal,
        PendingPolicy::Never,
        false,
        false,
    );
}

#[test]
fn presentation_retirement_is_internal_scheduler_noise() {
    assert_policy(
        InputEvent::PresentationRetired { presentation: 1 },
        FrontendEventClass::Internal,
        PendingPolicy::Never,
        false,
        false,
    );
}

#[test]
fn presentation_activation_and_discard_are_internal_service_actions() {
    let mut queue = FrontendEventQueue::default();
    queue.push_back(InputEvent::PresentationActivated {
        presentation: 41,
        emacs_frame_id: 0x1_0000_0000,
    });
    queue.push_back(InputEvent::PresentationDiscarded {
        presentation: 42,
        emacs_frame_id: 0x1_0000_0000,
    });

    for event in [
        InputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        },
        InputEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        },
    ] {
        assert_policy(
            event,
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
        );
    }

    assert_eq!(
        queue.take_leading_internal(),
        Some(InternalFrontendEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        })
    );
    assert_eq!(
        queue.take_leading_internal(),
        Some(InternalFrontendEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        })
    );
    assert!(queue.is_empty());
}

#[test]
fn layout_invalidation_is_internal_with_an_explicit_service_action() {
    let mut queue = FrontendEventQueue::default();
    queue.push_back(InputEvent::LayoutInvalidated);

    assert_policy(
        InputEvent::LayoutInvalidated,
        FrontendEventClass::Internal,
        PendingPolicy::Never,
        false,
        false,
    );
    assert_eq!(
        queue.take_leading_internal(),
        Some(InternalFrontendEvent::LayoutInvalidated)
    );
}

#[test]
fn image_state_change_preserves_identity_and_reason_as_internal_input() {
    let mut queue = FrontendEventQueue::default();
    let event = neomacs_display_protocol::ImageStateEvent::Evicted(
        neomacs_display_protocol::ImageId::new(41),
    );
    queue.push_back(InputEvent::ImageStateChanged { event });

    assert_eq!(
        queue.take_leading_internal(),
        Some(InternalFrontendEvent::ImageStateChanged { event })
    );
}
