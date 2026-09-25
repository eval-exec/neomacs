//! Tests for the GNU value grammar and the elisp-to-policy transport.

use super::*;
use crate::emacs_core::display_host::DisplayHost;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use neomacs_display_protocol::{EmacsModifierKey, ModifierEventKind, PhysicalModifierKey};
use std::sync::{Arc, Mutex};

/// Read Lisp values into the parts shape, the way the subr does.
fn policy_from(values: &[(&str, Value)]) -> neomacs_display_protocol::ModifierPolicy {
    let mut parts = neomacs_display_protocol::ModifierPolicyParts::default();
    for (name, key) in POLICY_VARS {
        let value = values
            .iter()
            .find(|(var, _)| *var == name)
            .map(|(_, value)| value.clone())
            .unwrap_or(Value::NIL);
        let assignment = assignment_from_lisp(&value, key.is_right());
        match key {
            PhysicalModifierKey::LeftCommand => parts.command_left = into_explicit(assignment),
            PhysicalModifierKey::RightCommand => parts.command_right = assignment,
            PhysicalModifierKey::LeftOption => parts.option_left = into_explicit(assignment),
            PhysicalModifierKey::RightOption => parts.option_right = assignment,
            PhysicalModifierKey::LeftControl => parts.ctrl_left = into_explicit(assignment),
            PhysicalModifierKey::RightControl => parts.ctrl_right = assignment,
            PhysicalModifierKey::Function => parts.function = into_explicit(assignment),
        }
    }
    neomacs_display_protocol::ModifierPolicy::from(parts)
}

fn evaluate(text: &str) -> Value {
    let mut eval = Context::new();
    eval.eval_str(text).expect("evaluation should succeed")
}

/// GNU nsterm.m:11576,11587: `nil' behaves as `none' (uniform), and the
/// right-side `left' marker inherits.
#[test]
fn nil_and_none_mean_no_modifier() {
    let policy = policy_from(&[("ns-alternate-modifier", Value::symbol("none"))]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        None
    );
    let policy = policy_from(&[]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        None
    );
}

/// The issue's configuration: plain symbols compile to uniform assignments.
/// The right-side variables are at their GNU defaults (`left' markers,
/// `src/nsterm.m:11587,11612,11633'), which `lisp/term/neo-win.el' installs.
#[test]
fn plain_symbols_compile_to_uniform_assignments() {
    let policy = policy_from(&[
        ("ns-command-modifier", Value::symbol("meta")),
        ("ns-alternate-modifier", Value::symbol("alt")),
        ("ns-right-command-modifier", Value::symbol("left")),
    ]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftCommand)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Meta)
    );
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Alt)
    );
    // Right sides untouched by the issue's config keep `left' inheritance.
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::RightCommand)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Meta)
    );
}

/// nsterm.m:11569-11572: the plist grammar maps kinds independently.
#[test]
fn plist_values_map_kinds_independently() {
    let policy = policy_from(&[(
        "ns-alternate-modifier",
        evaluate("'(:ordinary none :function meta :mouse hyper)"),
    )]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        None
    );
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Function),
        Some(EmacsModifierKey::Meta)
    );
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Mouse),
        Some(EmacsModifierKey::Hyper)
    );
}

/// `left' as a plist slot answers 0 in GNU (`parse_solitary_modifier`
/// does not recognize it) -- it behaves as `none', not as inheritance.
#[test]
fn left_inside_a_plist_slot_behaves_as_none() {
    let policy = policy_from(&[(
        "ns-alternate-modifier",
        evaluate("'(:ordinary left :function meta :mouse meta)"),
    )]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        None
    );
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Function),
        Some(EmacsModifierKey::Meta)
    );
}

/// `ns-right-command-modifier' = `left' keeps the inheritance marker.
#[test]
fn right_variable_left_marker_inherits() {
    let policy = policy_from(&[
        ("ns-command-modifier", Value::symbol("hyper")),
        ("ns-right-command-modifier", Value::symbol("left")),
    ]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::RightCommand)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Hyper)
    );
}

/// keyboard.c:7917: unrecognized symbols answer 0, so they degrade to
/// `none' rather than erroring -- GNU's own leniency.
#[test]
fn unrecognized_symbols_degrade_to_none() {
    let policy = policy_from(&[("ns-command-modifier", Value::symbol("hyperbolics"))]);
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftCommand)
            .for_kind(ModifierEventKind::Ordinary),
        None
    );
}

/// The real surface: the subr reads the seven variables and ships the
/// compiled policy through the display host.
#[test]
fn subr_compiles_the_seven_variables_and_ships_the_policy() {
    #[derive(Default, Clone)]
    struct CapturingHost {
        policies: Arc<Mutex<Vec<neomacs_display_protocol::ModifierPolicy>>>,
    }
    impl DisplayHost for CapturingHost {
        fn realize_gui_frame(
            &mut self,
            _request: crate::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(
            &mut self,
            _request: crate::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }
        fn set_modifier_policy(
            &mut self,
            policy: neomacs_display_protocol::ModifierPolicy,
        ) -> Result<(), String> {
            self.policies.lock().expect("policy log").push(policy);
            Ok(())
        }
    }

    let host = CapturingHost::default();
    let mut eval = Context::new();
    eval.set_display_host(Box::new(host.clone()));
    eval.eval_str(
        r#"(progn (setq ns-command-modifier 'meta)
                  (setq ns-alternate-modifier 'alt)
                  (neomacs-set-modifier-policy))"#,
    )
    .expect("subr should run");
    let policies = host.policies.lock().expect("policy log");
    let policy = policies.last().expect("one policy push");
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftCommand)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Meta)
    );
    assert_eq!(
        policy
            .assignment(PhysicalModifierKey::LeftOption)
            .for_kind(ModifierEventKind::Ordinary),
        Some(EmacsModifierKey::Alt)
    );
}

/// `neomacs-set-modifier-policy` is registered with GNU's arity discipline.
#[test]
fn subr_is_registered() {
    let mut eval = Context::new();
    let found = eval
        .eval_str("(list (fboundp 'neomacs-set-modifier-policy))")
        .expect("evaluation");
    assert_eq!(
        found,
        Value::list(vec![Value::T]),
        "subr must be registered"
    );
}
