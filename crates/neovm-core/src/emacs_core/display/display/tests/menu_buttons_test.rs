//! Menu state is resolved anew when a GUI menu is opened.
use super::menu_test_support::publish;
use super::*;
use neomacs_display_protocol::menu::{MenuCheckState, MenuIndicator};

#[test]
fn gui_menu_publication_distinguishes_toggle_on_from_off() {
    let mut eval = Context::new();
    let menu =
        r#"'(keymap (option menu-item "Option" ignore :button (:toggle . menu-option-state)))"#;
    eval.eval_str("(setq menu-option-state nil)").unwrap();
    assert_eq!(
        publish(&mut eval, menu).unwrap()[0].indicator(),
        MenuIndicator::Toggle(MenuCheckState::Off)
    );
    eval.eval_str("(setq menu-option-state t)").unwrap();
    assert_eq!(
        publish(&mut eval, menu).unwrap()[0].indicator(),
        MenuIndicator::Toggle(MenuCheckState::On)
    );
}

#[test]
fn gui_menu_radio_state_is_resolved_again_on_reopening() {
    let mut eval = Context::new();
    let menu =
        r#"'(keymap (option menu-item "Radio" ignore :button (:radio . menu-option-state)))"#;
    eval.eval_str("(setq menu-option-state nil)").unwrap();
    assert_eq!(
        publish(&mut eval, menu).unwrap()[0].indicator(),
        MenuIndicator::Radio(MenuCheckState::Off)
    );
    eval.eval_str("(setq menu-option-state t)").unwrap();
    assert_eq!(
        publish(&mut eval, menu).unwrap()[0].indicator(),
        MenuIndicator::Radio(MenuCheckState::On)
    );
}
