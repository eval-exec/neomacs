use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendLogicalExtent, FrontendModifiers, FrontendPresentationId, FrontendViewport,
};

#[test]
fn key_symbols_preserve_the_nul_control_character() {
    assert_eq!(FrontendKeySymbol::new(0).get(), 0);
    assert_eq!(FrontendKeySymbol::new('x' as u32).get(), 'x' as u32);
}

#[test]
fn key_event_keeps_state_modifiers_and_target_together() {
    let modifiers = FrontendModifiers::new(true, false, true, false);
    let event = FrontendEvent::Key(FrontendKeyEvent::new(
        FrontendKeySymbol::new('x' as u32),
        modifiers,
        FrontendKeyState::Pressed,
        FrontendFrameId::new(42),
    ));

    let FrontendEvent::Key(key) = event else {
        panic!("constructed key event changed variant");
    };
    assert_eq!(key.symbol().get(), 'x' as u32);
    assert_eq!(key.modifiers(), modifiers);
    assert_eq!(key.state(), FrontendKeyState::Pressed);
    assert_eq!(key.target().get(), 42);
}

#[test]
fn viewport_validates_scale_at_the_adapter_boundary() {
    let logical_extent = FrontendLogicalExtent::new(800, 600);
    assert!(FrontendViewport::new(logical_extent, 0.0, FrontendFrameId::PRIMARY).is_err());
    assert!(FrontendViewport::new(logical_extent, f64::NAN, FrontendFrameId::PRIMARY).is_err());
    assert!(
        FrontendViewport::new(logical_extent, f64::INFINITY, FrontendFrameId::PRIMARY).is_err()
    );

    let scale = 1.500_000_000_000_000_2;
    let viewport = FrontendViewport::new(logical_extent, scale, FrontendFrameId::PRIMARY).unwrap();
    assert_eq!(viewport.logical_extent(), logical_extent);
    assert_eq!(
        (logical_extent.width(), logical_extent.height()),
        (800, 600)
    );
    assert_eq!(viewport.scale().get(), scale);
    assert_eq!(viewport.target(), FrontendFrameId::PRIMARY);
}

#[test]
fn committed_text_remains_atomic_until_the_evaluator_adapter() {
    let event = FrontendEvent::text_committed("λ🙂", FrontendFrameId::new(7));

    let FrontendEvent::TextCommitted { text, target } = event else {
        panic!("committed text changed variant");
    };
    assert_eq!(text, "λ🙂");
    assert_eq!(target.get(), 7);
}

#[test]
fn presentation_feedback_keeps_revision_and_frame_identity_together() {
    let presentation = FrontendPresentationId::new(23);
    let target = FrontendFrameId::new(7);

    assert_eq!(
        FrontendEvent::PresentationActivated {
            presentation,
            target,
        },
        FrontendEvent::PresentationActivated {
            presentation: FrontendPresentationId::new(23),
            target: FrontendFrameId::new(7),
        }
    );
    assert_eq!(presentation.get(), 23);
}
