use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendModifiers, FrontendViewport,
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
    assert!(FrontendViewport::new(800, 600, 0.0, FrontendFrameId::PRIMARY).is_err());
    assert!(FrontendViewport::new(800, 600, f64::NAN, FrontendFrameId::PRIMARY).is_err());
    assert!(FrontendViewport::new(800, 600, f64::INFINITY, FrontendFrameId::PRIMARY).is_err());

    let scale = 1.500_000_000_000_000_2;
    let viewport = FrontendViewport::new(800, 600, scale, FrontendFrameId::PRIMARY).unwrap();
    assert_eq!((viewport.width(), viewport.height()), (800, 600));
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
