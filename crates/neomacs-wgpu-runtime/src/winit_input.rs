//! Pure winit input translation shared by portable product adapters.

use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendModifiers,
};
use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Stateful winit-to-editor input boundary.
///
/// Winit reports modifiers separately from key events. Keeping the latest
/// sample here lets every adapter emit one complete, atomic frontend event.
#[derive(Clone, Copy, Debug, Default)]
pub struct WinitFrontendInput {
    modifiers: FrontendModifiers,
}

impl WinitFrontendInput {
    /// Replace the currently sampled host modifier state.
    pub fn set_modifiers(&mut self, state: ModifiersState) {
        self.modifiers = FrontendModifiers::new(
            state.shift_key(),
            state.control_key(),
            state.alt_key(),
            state.super_key(),
        );
    }

    /// Translate one winit key observation into the host-neutral editor ABI.
    ///
    /// Printable text is committed atomically when no command modifier is
    /// active. Named keys and modified characters preserve GNU GUI keysyms.
    #[must_use]
    pub fn translate_key(
        &self,
        logical_key: &Key,
        text: Option<&str>,
        state: ElementState,
        target: FrontendFrameId,
    ) -> Option<FrontendEvent> {
        let key_state = match state {
            ElementState::Pressed => FrontendKeyState::Pressed,
            ElementState::Released => FrontendKeyState::Released,
        };

        if state == ElementState::Pressed && matches!(logical_key, Key::Character(_)) {
            if let Some(control) = text.and_then(single_control_character) {
                return Some(self.key_event(control as u32, key_state, target));
            }
            if !self.command_modifier_active()
                && let Some(committed) = text.filter(|text| has_printable_character(text))
            {
                return Some(FrontendEvent::text_committed(committed, target));
            }
        }

        let symbol = winit_key_symbol(logical_key)?;
        Some(self.key_event(symbol, key_state, target))
    }

    /// Translate committed text supplied independently by a host IME.
    #[must_use]
    pub fn committed_text(&self, text: &str, target: FrontendFrameId) -> Option<FrontendEvent> {
        has_printable_character(text).then(|| FrontendEvent::text_committed(text, target))
    }

    fn key_event(
        &self,
        symbol: u32,
        state: FrontendKeyState,
        target: FrontendFrameId,
    ) -> FrontendEvent {
        FrontendEvent::Key(FrontendKeyEvent::new(
            FrontendKeySymbol::new(symbol),
            self.modifiers,
            state,
            target,
        ))
    }

    const fn command_modifier_active(self) -> bool {
        self.modifiers.control() || self.modifiers.meta() || self.modifiers.super_()
    }
}

fn single_control_character(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let character = characters.next()?;
    (characters.next().is_none() && character.is_control()).then_some(character)
}

fn has_printable_character(text: &str) -> bool {
    text.chars().any(|character| !character.is_control())
}

fn winit_key_symbol(key: &Key) -> Option<u32> {
    match key {
        Key::Named(named) => named_key_symbol(*named),
        Key::Character(text) => text.chars().next().map(char::into),
        _ => None,
    }
}

fn named_key_symbol(key: NamedKey) -> Option<u32> {
    let symbol = match key {
        NamedKey::F1 => 0xffbe,
        NamedKey::F2 => 0xffbf,
        NamedKey::F3 => 0xffc0,
        NamedKey::F4 => 0xffc1,
        NamedKey::F5 => 0xffc2,
        NamedKey::F6 => 0xffc3,
        NamedKey::F7 => 0xffc4,
        NamedKey::F8 => 0xffc5,
        NamedKey::F9 => 0xffc6,
        NamedKey::F10 => 0xffc7,
        NamedKey::F11 => 0xffc8,
        NamedKey::F12 => 0xffc9,
        NamedKey::Escape => 0xff1b,
        NamedKey::Enter => 0xff0d,
        NamedKey::Tab => 0xff09,
        NamedKey::Backspace => 0xff08,
        NamedKey::Delete => 0xffff,
        NamedKey::Insert => 0xff63,
        NamedKey::Home => 0xff50,
        NamedKey::End => 0xff57,
        NamedKey::PageUp => 0xff55,
        NamedKey::PageDown => 0xff56,
        NamedKey::ArrowLeft => 0xff51,
        NamedKey::ArrowUp => 0xff52,
        NamedKey::ArrowRight => 0xff53,
        NamedKey::ArrowDown => 0xff54,
        NamedKey::Space => 0x20,
        NamedKey::PrintScreen => 0xff61,
        NamedKey::ScrollLock => 0xff14,
        NamedKey::Pause => 0xff13,
        _ => return None,
    };
    Some(symbol)
}

#[cfg(test)]
mod tests {
    use neomacs_app::frontend_event::{
        FrontendEvent, FrontendFrameId, FrontendKeyState, FrontendKeySymbol,
    };
    use winit::event::ElementState;
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    use super::WinitFrontendInput;

    const TARGET: FrontendFrameId = FrontendFrameId::new(17);

    #[test]
    fn named_keys_preserve_gnu_gui_keysyms() {
        let input = WinitFrontendInput::default();

        let event = input
            .translate_key(
                &Key::Named(NamedKey::ArrowLeft),
                None,
                ElementState::Pressed,
                TARGET,
            )
            .expect("left arrow is a supported key");

        assert_eq!(
            event,
            FrontendEvent::Key(neomacs_app::frontend_event::FrontendKeyEvent::new(
                FrontendKeySymbol::new(0xff51),
                Default::default(),
                FrontendKeyState::Pressed,
                TARGET,
            ))
        );
    }

    #[test]
    fn printable_key_text_is_one_atomic_unicode_commit() {
        let input = WinitFrontendInput::default();

        let event = input
            .translate_key(
                &Key::Character("λ".into()),
                Some("λ🙂"),
                ElementState::Pressed,
                TARGET,
            )
            .expect("printable text is committed");

        assert_eq!(event, FrontendEvent::text_committed("λ🙂", TARGET));
    }

    #[test]
    fn command_modifiers_keep_key_semantics_instead_of_committing_text() {
        let mut input = WinitFrontendInput::default();
        input.set_modifiers(ModifiersState::CONTROL | ModifiersState::ALT);

        let event = input
            .translate_key(
                &Key::Character("x".into()),
                Some("x"),
                ElementState::Pressed,
                TARGET,
            )
            .expect("modified character is a key event");
        let FrontendEvent::Key(key) = event else {
            panic!("modified character must not become committed text");
        };

        assert_eq!(key.symbol(), FrontendKeySymbol::new('x' as u32));
        assert!(key.modifiers().control());
        assert!(key.modifiers().meta());
        assert!(!key.modifiers().super_());
    }

    #[test]
    fn unsupported_modifier_keys_do_not_invent_input() {
        let input = WinitFrontendInput::default();

        assert!(
            input
                .translate_key(
                    &Key::Named(NamedKey::Shift),
                    None,
                    ElementState::Pressed,
                    TARGET,
                )
                .is_none()
        );
    }
}
