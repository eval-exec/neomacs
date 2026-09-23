use super::*;
fn heading(key: &str) -> MenuHeading {
    MenuHeading {
        frame: 1,
        parent: WindowId::from_raw(1),
        key: key.into(),
        index: 5,
        compact: false,
    }
}

#[test]
fn repeated_hover_is_idempotent_while_pending_and_shown() {
    let mut bar = MenuBarTracking::default();
    let HeadingAction::Request(id) = bar.select(heading("help"), true) else {
        panic!()
    };
    for _ in 0..100 {
        assert_eq!(bar.select(heading("help"), false), HeadingAction::Keep);
    }
    bar.shown(id, MenuToken::fresh());
    for _ in 0..100 {
        assert_eq!(bar.select(heading("help"), false), HeadingAction::Keep);
    }
}

#[test]
fn a_b_a_switch_rejects_old_a_response() {
    let mut bar = MenuBarTracking::default();
    let HeadingAction::Request(old) = bar.select(heading("help"), true) else {
        panic!()
    };
    bar.select(heading("interactively"), false);
    let HeadingAction::Request(new) = bar.select(heading("help"), false) else {
        panic!()
    };
    assert!(!bar.accepts(old));
    assert!(bar.accepts(new));
    assert_eq!(bar.select(heading("help"), true), HeadingAction::Close);
    assert!(!bar.accepts(new));
}
