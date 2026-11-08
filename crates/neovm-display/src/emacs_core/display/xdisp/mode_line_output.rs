//! STRING-only element attribution and GNU's shared formatter state.
//!
//! The World owns the counter. Nested Fformat_mode_line calls reset it even
//! for NOPROP; GNU's unwind vector restores the target, never the counter.
//! The owner/state itself lives below the evaluator
//! (`emacs_core::eval::data::redisplay`); this module keeps the display-side
//! scopes and the element attribution that runs on them.

use neovm_eval::emacs_core::eval::data::redisplay::{ModeLineElementId, OutputKind};

pub(crate) use neovm_eval::emacs_core::eval::data::redisplay::ModeLineOutputOwner;

use super::{Flow, Value};

/// Keep the active display extent across Lisp callbacks without borrowing Context.
/// Drop restores the enclosing extent on both normal return and unwinding.
pub(super) struct ModeLineDisplayScope {
    owner: ModeLineOutputOwner,
    previous: bool,
}

impl ModeLineDisplayScope {
    pub(super) fn enter(owner: &ModeLineOutputOwner) -> Self {
        Self {
            owner: owner.clone(),
            previous: owner.replace_display_active(true),
        }
    }
}

impl Drop for ModeLineDisplayScope {
    fn drop(&mut self) {
        self.owner.replace_display_active(self.previous);
    }
}

/// STRING authority borrowed from a live admitted formatter extent.
/// The scope owns the shared state across callbacks; this handle cannot outlive
/// that scope and copies the live authority rather than an ordinal snapshot.
#[derive(Clone, Copy, Debug)]
pub(super) struct ModeLineStringTarget<'scope>(&'scope ModeLineOutputOwner);

static_assertions::assert_not_impl_any!(ModeLineStringTarget<'static>: Send, Sync);
static_assertions::assert_eq_size!(ModeLineStringTarget<'static>, usize);

impl ModeLineStringTarget<'_> {
    pub(super) fn advance(self) -> Result<ModeLineElementId, Flow> {
        self.0.advance()
    }

    pub(super) fn current(self) -> ModeLineElementId {
        self.0.current()
    }
}

/// Admitted Fformat_mode_line extent. The ordinal intentionally survives Drop.
#[must_use]
pub(super) struct ModeLineOutputScope {
    owner: ModeLineOutputOwner,
    previous_target: OutputKind,
}

static_assertions::assert_not_impl_any!(ModeLineOutputScope: Send, Sync);

impl ModeLineOutputScope {
    pub(super) fn enter(
        eval: &neovm_eval::emacs_core::eval::Context,
        args: &[Value],
    ) -> Result<Self, Flow> {
        // Keep all Lisp-reachable validation before the shared reset, including
        // the nil/batch paths handled by the enclosing builtin.
        super::expect_args_range("format-mode-line", args, 1, 4)?;
        super::validate_optional_window_designator(
            eval,
            args.get(2),
            crate::emacs_core::window_cmds::WindowDomain::Any,
        )?;
        super::validate_optional_buffer_designator(eval, args.get(3))?;
        let kind = if super::mode_line_face_suppresses_properties(args) {
            OutputKind::NoProperties
        } else {
            OutputKind::String
        };
        let owner = eval.mode_line_output_owner();
        let previous_target = owner.replace_target(kind);
        owner.reset_element();
        Ok(Self {
            owner,
            previous_target,
        })
    }

    pub(super) fn target(&self) -> super::StoredTarget<'_> {
        match self.owner.target() {
            OutputKind::String => super::StoredTarget::String(ModeLineStringTarget(&self.owner)),
            OutputKind::NoProperties => super::StoredTarget::NoProperties,
        }
    }
}

impl Drop for ModeLineOutputScope {
    fn drop(&mut self) {
        self.owner.set_target(self.previous_target);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ElementAttribution {
    FormatElement(ModeLineElementId),
    SyntheticNumericPadding,
}

enum ElementProperty {
    Generated { name: Value, value: Value },
    Unattributed { name: Value },
}

impl ElementAttribution {
    // One definition of the Lisp property and its ordinal conversion, shared
    // by stored text and GNU's percent arm's inherited PROPS continuation.
    fn property(self) -> ElementProperty {
        let name = Value::symbol("mode-line-elt-no");
        match self {
            Self::FormatElement(id) => ElementProperty::Generated {
                name,
                value: id.into_value(),
            },
            Self::SyntheticNumericPadding => ElementProperty::Unattributed { name },
        }
    }

    pub(super) fn apply_to_table(
        self,
        table: &mut super::TextPropertyTable,
        range: super::CharRange,
    ) {
        match self.property() {
            ElementProperty::Generated { name, value } => {
                // GNU's final mode-line interval order appends
                // `mode-line-elt-no` after the source plist (xdisp
                // store_mode_line_string computes it with `plist_put`); an
                // already-present key keeps its position.
                table.put_property_appending_new_in_char_range(range, name, value);
            }
            ElementProperty::Unattributed { name } => {
                table.remove_property_in_char_range(range, name);
            }
        }
    }

    pub(super) fn apply_to_inherited(self, properties: Value) -> super::EvalResult {
        match self.property() {
            ElementProperty::Generated { name, value } => {
                neovm_eval::emacs_core::builtins::builtin_plist_put(vec![properties, name, value])
            }
            // Numeric padding stores no property and does not change the
            // inherited context for a later real element.
            ElementProperty::Unattributed { .. } => Ok(properties),
        }
    }
}
