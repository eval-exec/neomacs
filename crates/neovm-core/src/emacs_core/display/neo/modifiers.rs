//! The NS modifier policy: Lisp surface, GNU value grammar, and the
//! evaluator-side half of issue #442's fix.
//!
//! GNU reads `ns-command-modifier' and friends at every `keyDown:`
//! (`src/nsterm.m:397-425`).  Neomacs renders through winit, which cannot
//! read Lisp per event, so Lisp compiles the whole policy into a typed
//! [`ModifierPolicy`] whenever any of the seven variables changes
//! (`add-variable-watcher' in `lisp/term/neo-win.el') and ships it to the
//! render thread through [`DisplayHost::set_modifier_policy`].
//!
//! The value grammar is GNU's own (`nsterm.m:11569-11587`): a symbol, a
//! plist `(:ordinary SYM :function SYM :mouse SYM)', or, on the right-side
//! variables, `left'.  `nil' and `none' mean the key keeps its standard
//! meaning.  GNU's `parse_solitary_modifier' (`src/keyboard.c:7917`) answers
//! 0 for unrecognized symbols, so an unrecognized value degrades to `none'
//! here rather than erroring.

mod subrs;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use crate::emacs_core::error::{EvalResult, Flow, expect_args, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{Value, list_to_vec};
use neomacs_display_protocol::{
    EmacsModifierKey, ModifierAssignment, ModifierPolicy, ModifierPolicyParts, PhysicalModifierKey,
    SideAssignment,
};

/// The seven GNU variables and their physical keys, in `syms_of_nsterm`
/// declaration order (`src/nsterm.m:11568-11643`).
pub(crate) const POLICY_VARS: [(&str, PhysicalModifierKey); 7] = [
    ("ns-alternate-modifier", PhysicalModifierKey::LeftOption),
    (
        "ns-right-alternate-modifier",
        PhysicalModifierKey::RightOption,
    ),
    ("ns-command-modifier", PhysicalModifierKey::LeftCommand),
    (
        "ns-right-command-modifier",
        PhysicalModifierKey::RightCommand,
    ),
    ("ns-control-modifier", PhysicalModifierKey::LeftControl),
    (
        "ns-right-control-modifier",
        PhysicalModifierKey::RightControl,
    ),
    ("ns-function-modifier", PhysicalModifierKey::Function),
];

/// `parse_solitary_modifier' (`src/keyboard.c:7917`) as a symbol parser:
/// the documented NS spellings plus the single letters GNU accepts, with
/// the unrecognized answer of 0 represented as `None'.
fn modifier_key_from_symbol(name: &str) -> Option<EmacsModifierKey> {
    match name {
        "alt" | "A" => Some(EmacsModifierKey::Alt),
        "ctrl" | "control" | "C" => Some(EmacsModifierKey::Ctrl),
        "hyper" | "H" => Some(EmacsModifierKey::Hyper),
        "meta" | "M" => Some(EmacsModifierKey::Meta),
        "super" | "s" => Some(EmacsModifierKey::Super),
        // `none' and anything unrecognized answer 0 in GNU.
        _ => None,
    }
}

fn symbol_name(value: &Value) -> Option<&str> {
    value.as_symbol_name()
}

/// One variable's value, or one plist slot's value.
///
/// `nil', `none' and unrecognized symbols produce a uniform no-modifier
/// assignment; a symbol produces a uniform assignment; a plist
/// `(:ordinary S :function S :mouse S)' produces per-kind assignments.  On
/// right-side variables the `left' marker means "inherit the left value",
/// which [`SideAssignment::InheritLeft`] keeps explicit.
fn assignment_from_lisp(value: &Value, right_side: bool) -> SideAssignment {
    if value.is_nil() {
        return SideAssignment::Explicit(ModifierAssignment::uniform(None));
    }
    if let Some(name) = symbol_name(value) {
        if right_side && name == "left" {
            return SideAssignment::InheritLeft;
        }
        let key = modifier_key_from_symbol(name);
        return SideAssignment::Explicit(ModifierAssignment::uniform(key));
    }
    // (:ordinary SYM :function SYM :mouse SYM) -- nsterm.m:11569-11572.
    if let Some(entries) = list_to_vec(value) {
        let slot = |slot_name: &str| -> ModifierAssignment {
            let mut assignment = ModifierAssignment::uniform(None);
            for pair in entries.chunks_exact(2) {
                if symbol_name(&pair[0])
                    .and_then(|name| name.strip_prefix(':'))
                    .is_some_and(|name| name == slot_name)
                {
                    assignment = slot_assignment(&pair[1]);
                }
            }
            assignment
        };
        return SideAssignment::Explicit(ModifierAssignment {
            ordinary: slot("ordinary").ordinary,
            function: slot("function").function,
            mouse: slot("mouse").mouse,
        });
    }
    SideAssignment::Explicit(ModifierAssignment::uniform(None))
}

/// One plist slot's value: `nil'/`none'/unknown -> no modifier, symbol ->
/// that modifier (uniform across kinds; GNU keeps a plist's slots flat).
fn slot_assignment(value: &Value) -> ModifierAssignment {
    if value.is_nil() {
        return ModifierAssignment::uniform(None);
    }
    if let Some(name) = symbol_name(value) {
        return ModifierAssignment::uniform(modifier_key_from_symbol(name));
    }
    ModifierAssignment::uniform(None)
}

/// Compile the current values of the seven NS variables into a policy.
///
/// Unbound variables read as nil, which GNU's DEFVAR_LISP initializers make
/// equivalent to `none' for the keys that matter.
pub fn modifier_policy_from_vars(eval: &Context) -> Result<ModifierPolicy, String> {
    let mut parts = ModifierPolicyParts {
        ..ModifierPolicyParts::default()
    };
    for (name, key) in POLICY_VARS {
        let value = eval
            .obarray
            .symbol_value(name)
            .copied()
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
    Ok(ModifierPolicy::from(parts))
}

/// A left-side variable cannot carry `left'; GNU has no such spelling, and
/// treating it as `none' keeps the parts struct total.
fn into_explicit(assignment: SideAssignment) -> ModifierAssignment {
    match assignment {
        SideAssignment::InheritLeft => ModifierAssignment::uniform(None),
        SideAssignment::Explicit(assignment) => assignment,
    }
}

/// `(neomacs-set-modifier-policy)' -- compile the seven NS variables and
/// ship the policy to the render thread.
///
/// No arguments: GNU's nsterm.m reads its C variables directly, so the Lisp
/// half reads the variables too, keeping the value grammar and defaults in
/// exactly one place (`lisp/term/neo-win.el').
fn policy_error(function: &str, message: impl std::fmt::Display) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("{function}: {message}"))],
    )
}

fn set_modifier_policy(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-set-modifier-policy", &args, 0)?;
    let policy = modifier_policy_from_vars(eval)
        .map_err(|message| policy_error("neomacs-set-modifier-policy", message))?;
    if let Some(host) = eval.display_host.as_mut() {
        host.set_modifier_policy(policy)
            .map_err(|message| policy_error("neomacs-set-modifier-policy", message))?;
    }
    Ok(Value::NIL)
}
