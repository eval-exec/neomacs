//! Translate desktop keys into menu navigation, never editor input.

use super::session::MenuSession;
use winit::keyboard::{Key, NamedKey};

pub(super) fn cancels(key: &Key, modifiers: winit::keyboard::ModifiersState) -> bool {
    matches!(key, Key::Named(NamedKey::Escape))
        || (modifiers.control_key() && matches!(key.as_ref(), Key::Character("g" | "G")))
}

pub(super) fn key(
    session: &mut MenuSession,
    key: &Key,
    modifiers: winit::keyboard::ModifiersState,
) -> Option<i32> {
    if cancels(key, modifiers) {
        return Some(-1);
    }
    match key.as_ref() {
        Key::Named(NamedKey::ArrowDown) => {
            session.move_hover(1);
        }
        Key::Named(NamedKey::ArrowUp) => {
            session.move_hover(-1);
        }
        Key::Named(NamedKey::ArrowRight) => {
            if session.open_submenu() {
                session.move_hover(1);
            }
        }
        Key::Named(NamedKey::ArrowLeft) => {
            session.close_submenu();
        }
        Key::Named(NamedKey::Home) => {
            session.active_panel_mut().hover_index = -1;
            session.move_hover(1);
        }
        Key::Named(NamedKey::End) => {
            session.active_panel_mut().hover_index = -1;
            session.move_hover(-1);
        }
        Key::Named(NamedKey::Enter) => {
            return session.activate_panel(session.submenu_panels.len());
        }
        _ => {}
    }
    None
}
