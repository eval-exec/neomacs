use neomacs_app::evaluator_input::EvaluatorInputBatch;
use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendModifiers, FrontendPresentationId, FrontendViewport,
};
use neovm_core::keyboard::{self, InputEvent};

fn one(event: &FrontendEvent) -> Option<InputEvent> {
    let mut events = EvaluatorInputBatch::from_frontend_event(event);
    let first = events.next();
    assert!(events.next().is_none());
    first
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
        FrontendViewport::new(800, 600, 1.25, FrontendFrameId::new(9)).unwrap(),
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
