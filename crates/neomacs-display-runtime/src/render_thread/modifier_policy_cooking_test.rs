//! The render-thread half of issue #442: the compiled NS modifier policy
//! cooking raw modifier facts into the transport bits Emacs receives.
//!
//! The arithmetic being pinned is GNU's `EV_MODIFIERS2`
//! (`src/nsterm.m:397-425`): the default policy must keep the pre-policy
//! hardcode's answers (Option -> meta, Command -> super), and a policy push
//! from Lisp must change what Emacs sees on the very next key event.

use super::RenderApp;
use super::tests::make_test_app;
use crate::thread_comm::ConfigCommand;
use neomacs_display_protocol::{
    EmacsModifierKey, ModifierEventKind, ModifierPolicy, ModifierSideState, OptionAsAltShape,
    PhysicalModifierKey, RawModifiers, TransportModifierBit,
};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

/// Simulate the aggregate answer `ModifiersChanged` would install.
fn set_aggregate(app: &mut RenderApp, shift: bool, ctrl: bool, alt: bool, meta: bool) {
    let mut state = ModifiersState::empty();
    if shift {
        state |= ModifiersState::SHIFT;
    }
    if ctrl {
        state |= ModifiersState::CONTROL;
    }
    if alt {
        state |= ModifiersState::ALT;
    }
    if meta {
        state |= ModifiersState::META;
    }
    app.modifier_state = state;
}

/// The default policy must reproduce the pre-policy hardcode:
/// `ModifiersChanged` mapped alt(Option) -> META and meta(Command) -> SUPER
/// (`window_events.rs` before this change).
#[test]
fn default_policy_keeps_the_pre_policy_hardcode() {
    let mut app = make_test_app();
    assert_eq!(app.modifier_policy, ModifierPolicy::gnu_ns_default());

    // Option, Command and Control all down; sides unobserved -> GNU's
    // left rule.
    set_aggregate(&mut app, false, true, true, true);
    let ordinary = app.cook_modifiers(ModifierEventKind::Ordinary);
    assert!(ordinary.get(TransportModifierBit::Meta));
    assert!(ordinary.get(TransportModifierBit::Super));
    assert!(ordinary.get(TransportModifierBit::Ctrl));
    assert!(!ordinary.get(TransportModifierBit::Alt));
}

/// The reporter's configuration (issue #442): Command -> meta,
/// Option -> alt.  A policy push must reach the next cooked key event.
#[test]
fn policy_push_swaps_command_and_option() {
    let mut app = make_test_app();
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_option(EmacsModifierKey::Alt);
    app.handle_config(ConfigCommand::SetModifierPolicy(policy));

    // Both families down (aggregate + unobserved sides -> GNU left rule).
    app.modifier_state = ModifiersState::META | ModifiersState::ALT;
    let bits = app.cook_modifiers(ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta));
    assert!(bits.get(TransportModifierBit::Alt));
    assert!(!bits.get(TransportModifierBit::Super));
}

/// GNU `nil_or_none' (nsterm.m:2686-2690): with Option mapped to none, an
/// Option chord cooks no Option bit at all.
#[test]
fn option_none_cooks_no_option_bits() {
    let mut app = make_test_app();
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_option_none();
    app.handle_config(ConfigCommand::SetModifierPolicy(policy));

    app.modifier_state = ModifiersState::ALT;
    let bits = app.cook_modifiers(ModifierEventKind::Ordinary);
    assert!(!bits.get(TransportModifierBit::Meta));
    assert!(!bits.get(TransportModifierBit::Alt));
    assert_eq!(
        app.modifier_policy.option_as_alt_shape(),
        OptionAsAltShape::None
    );
}

/// nsterm.m:7322: function keys consult the :function slots, ordinary keys
/// the :ordinary slots.
#[test]
fn function_keys_cook_the_function_slots() {
    let mut app = make_test_app();
    let policy = ModifierPolicy::gnu_ns_default().with_option_assignment(
        None,
        Some(EmacsModifierKey::Meta),
        Some(EmacsModifierKey::Meta),
    );
    app.handle_config(ConfigCommand::SetModifierPolicy(policy));
    app.modifier_state = ModifiersState::ALT;

    // Arrow-left is GNU function-key territory (ns_convert_key, 0x51).
    assert!(app.cooked_key_modifiers(0xff51) & TransportModifierBit::Meta.mask() != 0);
    // 'x' is an ordinary key: the :ordinary slot is none, so no bit.
    assert_eq!(
        app.cooked_key_modifiers('x' as u32) & TransportModifierBit::Meta.mask(),
        0
    );
    assert!(RenderApp::keysym_is_function_key(0xff09), "TAB converts");
    assert!(RenderApp::keysym_is_function_key(0xffbe), "F1 converts");
    assert!(
        RenderApp::keysym_is_function_key(0xffad),
        "KP subtract converts"
    );
    assert!(!RenderApp::keysym_is_function_key('x' as u32));
    assert!(
        !RenderApp::keysym_is_function_key(0xff13),
        "pause is not converted"
    );
}

/// The physical modifier keys' own events are the only left/right facts
/// winit offers (`ModifierSides`); MetaRight observed down, MetaLeft up.
#[test]
fn side_tracking_feeds_the_policy() {
    let mut app = make_test_app();
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_right_command(EmacsModifierKey::Hyper);
    app.handle_config(ConfigCommand::SetModifierPolicy(policy));

    app.track_modifier_key(PhysicalKey::Code(KeyCode::MetaRight), true);
    app.modifier_state = ModifiersState::META;
    let bits = app.cook_modifiers(ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Hyper));
    assert!(!bits.get(TransportModifierBit::Meta));
    app.track_modifier_key(PhysicalKey::Code(KeyCode::MetaRight), false);
    app.track_modifier_key(PhysicalKey::Code(KeyCode::MetaLeft), true);
    let bits = app.cook_modifiers(ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta));
}

/// The mouse-kind cooking feeds `self.modifiers` (pointer, menus,
/// webviews), and policy-cooked alt/hyper count as command modifiers for
/// the committed-text gate (`src/nsterm.m:7318-7339`).
#[test]
fn committed_text_gate_includes_cooked_alt_bits() {
    let mut app = make_test_app();
    let policy = ModifierPolicy::gnu_ns_default().with_option(EmacsModifierKey::Alt);
    app.handle_config(ConfigCommand::SetModifierPolicy(policy));
    set_aggregate(&mut app, false, false, true, false);
    let cooked = app.cook_modifiers(ModifierEventKind::Ordinary).bits();
    assert!(
        super::RenderApp::translate_committed_text("x", cooked).is_none(),
        "A-x is a command chord, not text"
    );
    // Plain text still passes with no command modifiers.
    assert!(super::RenderApp::translate_committed_text("x", 0).is_some());
}

/// A policy is applied to windows opened after the push too: the manager
/// remembers the shape.
#[test]
fn policy_reaches_future_windows() {
    let mut app = make_test_app();
    let policy = ModifierPolicy::gnu_ns_default().with_option_none();
    app.handle_config(ConfigCommand::SetModifierPolicy(policy));
    assert_eq!(app.frame_windows.option_as_alt, OptionAsAltShape::None);
}

/// GNU's arithmetic, restated through the raw facts the transport types:
/// the union rules for every ModifierSideState on the command family.
#[test]
fn side_states_cook_like_ev_modifiers_helper() {
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_right_command(EmacsModifierKey::Hyper);
    let cook = |side: Option<ModifierSideState>| -> u32 {
        policy
            .cook(
                RawModifiers {
                    shift: false,
                    ctrl: None,
                    command: side,
                    option: None,
                },
                ModifierEventKind::Ordinary,
            )
            .bits()
    };
    let meta = TransportModifierBit::Meta.mask();
    let hyper = TransportModifierBit::Hyper.mask();
    assert_eq!(cook(None), 0, "family up");
    assert_eq!(
        cook(Some(ModifierSideState::Unknown)),
        meta,
        "unidentified -> left"
    );
    assert_eq!(cook(Some(ModifierSideState::LeftOnly)), meta);
    assert_eq!(cook(Some(ModifierSideState::RightOnly)), hyper);
    assert_eq!(
        cook(Some(ModifierSideState::Both)),
        meta | hyper,
        "nsterm.m:384-391 parses right then left"
    );
}
