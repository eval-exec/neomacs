use neomacs_app::evaluator_input::EvaluatorInputBatch;
use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendLogicalExtent, FrontendModifiers, FrontendPresentationId, FrontendTerminalExtent,
    FrontendTerminalViewport, FrontendViewport,
};
use neovm_core::keyboard::{self, InputEvent};

fn one(event: &FrontendEvent) -> Option<InputEvent> {
    let mut events = EvaluatorInputBatch::from_frontend_event(event);
    let first = events.next();
    assert!(events.next().is_none());
    first
}

#[test]
fn ime_replacement_remains_one_context_qualified_edit_not_key_events() {
    use neomacs_app::frontend_event::{ImeOperation, ImeSessionId};
    let operation = ImeOperation::Replace {
        before_bytes: 3,
        after_bytes: 0,
        text: "你好".into(),
    };
    let event = FrontendEvent::Ime {
        session: ImeSessionId(7),
        operation: operation.clone(),
        target: FrontendFrameId::new(42),
    };
    match one(&event) {
        Some(InputEvent::Ime {
            session,
            operation: actual,
            emacs_frame_id,
        }) => {
            assert_eq!(session, ImeSessionId(7));
            assert_eq!(actual, operation);
            assert_eq!(emacs_frame_id, 42);
        }
        actual => panic!("expected one IME replacement, got {actual:?}"),
    }
}

#[test]
fn key_release_is_dropped_but_nul_press_is_preserved() {
    let released = FrontendEvent::Key(FrontendKeyEvent::new(
        FrontendKeySymbol::new(keyboard::XK_RETURN),
        FrontendModifiers::default(),
        FrontendKeyState::Released,
        FrontendFrameId::new(42),
    ));
    assert!(one(&released).is_none());

    let nul = FrontendEvent::Key(FrontendKeyEvent::new(
        FrontendKeySymbol::new(0),
        FrontendModifiers::default(),
        FrontendKeyState::Pressed,
        FrontendFrameId::new(42),
    ));
    assert!(matches!(
        one(&nul),
        Some(InputEvent::KeyPress {
            key: keyboard::KeyEvent {
                key: keyboard::Key::Char('\0'),
                ..
            },
            emacs_frame_id: 42,
        })
    ));
}

#[test]
fn committed_text_expands_lazily_in_source_order() {
    let event = FrontendEvent::text_committed("λ🙂", FrontendFrameId::new(17));
    let converted = EvaluatorInputBatch::from_frontend_event(&event).collect::<Vec<_>>();

    assert!(matches!(
        converted.as_slice(),
        [
            InputEvent::KeyPress {
                key: keyboard::KeyEvent {
                    key: keyboard::Key::Char('λ'),
                    ..
                },
                emacs_frame_id: 17,
            },
            InputEvent::KeyPress {
                key: keyboard::KeyEvent {
                    key: keyboard::Key::Char('🙂'),
                    ..
                },
                emacs_frame_id: 17,
            }
        ]
    ));
}

#[test]
fn viewport_focus_and_close_preserve_frame_identity() {
    let viewport = FrontendEvent::ViewportChanged(
        FrontendViewport::new(
            FrontendLogicalExtent::new(800, 600),
            1.25,
            FrontendFrameId::new(9),
        )
        .unwrap(),
    );
    assert!(matches!(
        one(&viewport),
        Some(InputEvent::Resize {
            width: 800,
            height: 600,
            scale_factor: 1.25,
            emacs_frame_id: 9,
        })
    ));

    let focus = FrontendEvent::FocusChanged {
        focused: true,
        target: FrontendFrameId::new(9),
    };
    assert!(matches!(
        one(&focus),
        Some(InputEvent::Focus {
            focused: true,
            emacs_frame_id: 9,
        })
    ));

    let close = FrontendEvent::CloseRequested {
        target: FrontendFrameId::new(9),
    };
    assert!(matches!(
        one(&close),
        Some(InputEvent::WindowClose { emacs_frame_id: 9 })
    ));
}

#[test]
fn terminal_viewport_preserves_grid_units_without_claiming_pixel_geometry() {
    let viewport = FrontendEvent::TerminalViewportChanged(FrontendTerminalViewport::new(
        FrontendTerminalExtent::new(132, 43),
        FrontendFrameId::new(9),
    ));

    assert!(matches!(
        one(&viewport),
        Some(InputEvent::Resize {
            width: 132,
            height: 43,
            scale_factor: 1.0,
            emacs_frame_id: 9,
        })
    ));
}

#[test]
fn presentation_feedback_maps_without_losing_its_typed_identity() {
    let presentation = FrontendPresentationId::new(41);
    let target = FrontendFrameId::new(9);

    assert!(matches!(
        one(&FrontendEvent::PresentationActivated {
            presentation,
            target,
        }),
        Some(InputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 9,
        })
    ));
    assert!(matches!(
        one(&FrontendEvent::PresentationDiscarded {
            presentation,
            target,
        }),
        Some(InputEvent::PresentationDiscarded {
            presentation: 41,
            emacs_frame_id: 9,
        })
    ));
    assert!(matches!(
        one(&FrontendEvent::PresentationRetired { presentation }),
        Some(InputEvent::PresentationRetired { presentation: 41 })
    ));
}
