//! Tests for the typed NS modifier policy.
//!
//! Every assertion here is a restatement of `src/nsterm.m` semantics, cited
//! inline.  The policy is the single compile-time-checked shape through which
//! the window server's raw modifier facts become Emacs modifier bits, so the
//! tests pin both the GNU value grammar (`symbol` | plist | `left` | `none`)
//! and the cooking arithmetic (`EV_MODIFIERS2` + `ev_modifiers_helper`).

use crate::modifier_policy::{
    EmacsModifierKey, ModifierEventKind, ModifierPolicy, ModifierSideState, PhysicalModifierKey,
    RawModifiers, TransportModifierBit,
};

const KINDS: [ModifierEventKind; 3] = [
    ModifierEventKind::Ordinary,
    ModifierEventKind::Function,
    ModifierEventKind::Mouse,
];

/// GNU nsterm.m:11576,11587,11597-11600,11612,11622,11633,11643 -- the values
/// `syms_of_nsterm` installs under NS_IMPL_COCOA.
#[test]
fn gnu_ns_defaults_match_syms_of_nsterm() {
    let policy = ModifierPolicy::gnu_ns_default();
    // Option = `ns-alternate-modifier' = meta (11576).
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Meta)
    );
    // Right Option = `ns-right-alternate-modifier' = `left' (11587): inherits.
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::RightOption)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Meta)
    );
    // Command = `ns-command-modifier' = super (11598, Cocoa arm).
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftCommand)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Super)
    );
    // Right Command = `left' (11612): inherits.
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::RightCommand)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Super)
    );
    // Control = `control' (11622); Right Control = `left' (11633).
    for key in [
        PhysicalModifierKey::LeftControl,
        PhysicalModifierKey::RightControl,
    ] {
        assert_eq!(
            policy.assignment(key).for_kind(ModifierEventKind::Ordinary),
            Some(EmacsModifierKey::Ctrl)
        );
    }
    // fn = `ns-function-modifier' = none (11643).
    for kind in KINDS {
        assert_eq!(
            policy
                .assignment(PhysicalModifierKey::Function)
                .for_kind(kind),
            None
        );
    }
}

/// nsterm.m:2722-2730: with the default policy, an Option chord is
/// control-like (`nil_or_none' is false), so winit must rewrite Option as
/// alt for the text path (GNU reads `charactersIgnoringModifiers').
#[test]
fn option_is_command_like_under_gnu_defaults() {
    let policy = ModifierPolicy::gnu_ns_default();
    for kind in KINDS {
        assert!(policy.option_is_command_like(kind));
    }
}

/// nsterm.m:2722-2730 with `ns-alternate-modifier none': Option becomes
/// shift-like; the composed character must flow through
/// `interpretKeyEvents'.
#[test]
fn option_is_shift_like_when_mapped_to_none() {
    let policy = ModifierPolicy::gnu_ns_default().with_option_none();
    assert!(!policy.option_is_command_like(ModifierEventKind::Ordinary));
}

/// GNU's cooked default: Command maps to super.  (Cocoa arm, nsterm.m:11598.)
#[test]
fn cooking_command_yields_super_by_default() {
    let policy = ModifierPolicy::gnu_ns_default();
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::LeftOnly),
        option: None,
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert_eq!(bits.get(TransportModifierBit::Super), true);
    assert_eq!(bits.get(TransportModifierBit::Meta), false);
    assert_eq!(bits.get(TransportModifierBit::Alt), false);
    assert_eq!(bits.get(TransportModifierBit::Ctrl), false);
}

/// The reporter's configuration from issue #442:
/// mac-command-modifier -> meta, mac-option-modifier -> alt.
/// Command+X must arrive as M-x and Option+X as A-x.
#[test]
fn cooking_reports_swapped_command_and_option() {
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_option(EmacsModifierKey::Alt);
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::LeftOnly),
        option: Some(ModifierSideState::LeftOnly),
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta));
    assert!(bits.get(TransportModifierBit::Alt));
    assert!(!bits.get(TransportModifierBit::Super));
}

/// nsterm.m:7318-7339 with `ns-option-modifier none': an Option chord has no
/// control-like modifiers, so nothing may reach Emacs as a modifier bit --
/// the composed character path takes over.
#[test]
fn cooking_option_none_yields_no_option_bits() {
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_option_none();
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::LeftOnly),
        option: Some(ModifierSideState::LeftOnly),
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta)); // Command -> meta
    assert!(!bits.get(TransportModifierBit::Alt));
    assert!(!bits.get(TransportModifierBit::Super));
}

/// nsterm.m:371-395 (`ev_modifiers_helper'): the right key is only consulted
/// when its own mask is set AND its value is not `left'; otherwise the left
/// value applies.  Right-Option = `left' means left-Option's value.
#[test]
fn right_side_inherits_left_when_mapped_to_left() {
    let policy = ModifierPolicy::gnu_ns_default().with_command(EmacsModifierKey::Meta);
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::RightOnly),
        option: None,
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta)); // inherited from left
    assert!(!bits.get(TransportModifierBit::Super));
}

/// nsterm.m:384-391: with BOTH sides down, GNU parses right first and then
/// left (the `left_key || ! right_key' arm), so both mappings apply.
#[test]
fn both_sides_down_parse_both_assignments() {
    let policy = ModifierPolicy::gnu_ns_default()
        .with_command(EmacsModifierKey::Meta)
        .with_right_command(EmacsModifierKey::Hyper);
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::Both),
        option: None,
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta)); // left
    assert!(bits.get(TransportModifierBit::Hyper)); // right
}

/// nsterm.m:387-391 comment: when the window server cannot tell which side
/// was pressed (Unknown), the LEFT value applies.
#[test]
fn unknown_side_uses_left_assignment() {
    let policy = ModifierPolicy::gnu_ns_default().with_command(EmacsModifierKey::Meta);
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::Unknown),
        option: None,
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Meta));
}

/// nsterm.m:7322: `kind' is `function' when the keysym is a function key, and
/// the :function slot of each assignment applies.
#[test]
fn function_kind_uses_the_function_slot() {
    let policy = ModifierPolicy::gnu_ns_default().with_option_assignment(
        None,
        Some(EmacsModifierKey::Meta),
        Some(EmacsModifierKey::Meta),
    );
    // Ordinary chord on the right option key: :ordinary is none -> no bit.
    let ordinary = RawModifiers {
        shift: false,
        ctrl: None,
        command: None,
        option: Some(ModifierSideState::RightOnly),
    };
    assert!(
        !policy
            .cook(ordinary, ModifierEventKind::Ordinary)
            .get(TransportModifierBit::Meta)
    );
    // Function-key chord on the right option key: :function is meta -> M-.
    let function = RawModifiers {
        shift: false,
        ctrl: None,
        command: None,
        option: Some(ModifierSideState::RightOnly),
    };
    assert!(
        policy
            .cook(function, ModifierEventKind::Function)
            .get(TransportModifierBit::Meta)
    );
}

/// nsterm.m:7318-7323 comment: EV_MODIFIERS2's value is exactly the set of
/// control-like modifiers, and `parse_solitary_modifier' returns 0 for
/// shift-like values, so Shift itself never turns a chord into a command.
#[test]
fn shift_is_reported_but_never_cooks_a_command_chord() {
    let policy = ModifierPolicy::gnu_ns_default();
    let raw = RawModifiers {
        shift: true,
        ctrl: None,
        command: None,
        option: None,
    };
    let bits = policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(bits.get(TransportModifierBit::Shift));
    assert!(!bits.get(TransportModifierBit::Meta));
    assert!(!bits.get(TransportModifierBit::Super));
    assert!(!bits.get(TransportModifierBit::Ctrl));
    assert!(!bits.get(TransportModifierBit::Alt));
    assert!(!bits.get(TransportModifierBit::Hyper));
}

/// keyboard.c:7917 (`parse_solitary_modifier'): "control" maps to the ctrl
/// bit.  (Unknown *lisp symbols* yield 0 in GNU; that leniency lives in the
/// neovm-core parser, which degrades unknowns to `none' before this type.)
#[test]
fn ctrl_symbol_maps_to_the_ctrl_bit() {
    let policy = ModifierPolicy::gnu_ns_default().with_command(EmacsModifierKey::Ctrl);
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: Some(ModifierSideState::LeftOnly),
        option: None,
    };
    assert!(
        policy
            .cook(raw, ModifierEventKind::Ordinary)
            .get(TransportModifierBit::Ctrl)
    );
}

/// GNU distinguishes A- from M- (`parse_solitary_modifier': "alt" and "meta"
/// return different bits).  The transport must carry both.
#[test]
fn meta_and_alt_are_distinct_bits() {
    let meta_policy = ModifierPolicy::gnu_ns_default().with_option(EmacsModifierKey::Meta);
    let alt_policy = ModifierPolicy::gnu_ns_default().with_option(EmacsModifierKey::Alt);
    let raw = RawModifiers {
        shift: false,
        ctrl: None,
        command: None,
        option: Some(ModifierSideState::LeftOnly),
    };
    let meta_bits = meta_policy.cook(raw, ModifierEventKind::Ordinary);
    let alt_bits = alt_policy.cook(raw, ModifierEventKind::Ordinary);
    assert!(meta_bits.get(TransportModifierBit::Meta));
    assert!(!meta_bits.get(TransportModifierBit::Alt));
    assert!(alt_bits.get(TransportModifierBit::Alt));
    assert!(!alt_bits.get(TransportModifierBit::Meta));
}
