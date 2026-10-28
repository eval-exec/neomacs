//! STRING-only element attribution and GNU's shared formatter state.
//!
//! The World owns the counter. Nested Fformat_mode_line calls reset it even
//! for NOPROP; GNU's unwind vector restores the target, never the counter.

use std::cell::Cell;
use std::rc::Rc;

use super::{Flow, LispCondition, Value, signal};
use crate::tagged::value::Fixnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ModeLineElementId(Fixnum);

impl Default for ModeLineElementId {
    fn default() -> Self {
        Self(Fixnum::saturating(0))
    }
}

impl ModeLineElementId {
    fn next(self) -> Result<Self, Flow> {
        i64::from(self.0)
            .checked_add(1)
            .and_then(|value| Fixnum::try_from(value).ok())
            .map(Self)
            .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))
    }

    pub(super) fn into_value(self) -> Value {
        Value::from_fixnum(self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputKind {
    String,
    #[default]
    NoProperties,
}

#[derive(Debug, Default)]
struct OutputState {
    element: Cell<ModeLineElementId>,
    target: Cell<OutputKind>,
    display_active: Cell<bool>,
}

/// Native state of one evaluator World; no Lisp values or retained output.
/// The existing World/Green Baton grants one owner access at a time. Handles
/// stay on that owner thread for a formatter extent and cannot be sent/shared.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModeLineOutputOwner(Rc<OutputState>);

static_assertions::assert_not_impl_any!(ModeLineOutputOwner: Send, Sync);
static_assertions::assert_eq_size!(ModeLineOutputOwner, usize);

impl ModeLineOutputOwner {
    pub(super) fn display_active(&self) -> bool {
        self.0.display_active.get()
    }

    pub(super) fn enter_display(&self) -> ModeLineDisplayScope {
        ModeLineDisplayScope {
            owner: self.clone(),
            previous: self.0.display_active.replace(true),
        }
    }

    fn advance(&self) -> Result<ModeLineElementId, Flow> {
        let next = self.0.element.get().next()?;
        self.0.element.set(next);
        Ok(next)
    }

    fn current(&self) -> ModeLineElementId {
        self.0.element.get()
    }

    fn enter(&self, target: OutputKind) -> ModeLineOutputScope {
        let previous_target = self.0.target.replace(target);
        self.0.element.set(ModeLineElementId::default());
        ModeLineOutputScope {
            owner: self.clone(),
            previous_target,
        }
    }
}

/// Keep the active display extent across Lisp callbacks without borrowing Context.
/// Drop restores the enclosing extent on both normal return and unwinding.
pub(super) struct ModeLineDisplayScope {
    owner: ModeLineOutputOwner,
    previous: bool,
}

impl Drop for ModeLineDisplayScope {
    fn drop(&mut self) {
        self.owner.0.display_active.set(self.previous);
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
    pub(super) fn enter(eval: &crate::emacs_core::Context, args: &[Value]) -> Result<Self, Flow> {
        // Keep all Lisp-reachable validation before the shared reset, including
        // the nil/batch paths handled by the enclosing builtin.
        super::expect_args_range("format-mode-line", args, 1, 4)?;
        super::validate_optional_window_designator(
            eval,
            args.get(2),
            crate::emacs_core::window_cmds::WindowDomain::Any,
        )?;
        super::validate_optional_buffer_designator(eval, args.get(3))?;
        let kind = if super::resolve_mode_line_face_spec(args).no_props {
            OutputKind::NoProperties
        } else {
            OutputKind::String
        };
        Ok(eval.mode_line_output_owner().enter(kind))
    }

    pub(super) fn target(&self) -> super::StoredTarget<'_> {
        match self.owner.0.target.get() {
            OutputKind::String => super::StoredTarget::String(ModeLineStringTarget(&self.owner)),
            OutputKind::NoProperties => super::StoredTarget::NoProperties,
        }
    }
}

impl Drop for ModeLineOutputScope {
    fn drop(&mut self) {
        self.owner.0.target.set(self.previous_target);
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
                table.put_property_in_char_range(range, name, value);
            }
            ElementProperty::Unattributed { name } => {
                table.remove_property_in_char_range(range, name);
            }
        }
    }

    pub(super) fn apply_to_inherited(self, properties: Value) -> super::EvalResult {
        match self.property() {
            ElementProperty::Generated { name, value } => {
                crate::emacs_core::builtins::builtin_plist_put(vec![properties, name, value])
            }
            // Numeric padding stores no property and does not change the
            // inherited context for a later real element.
            ElementProperty::Unattributed { .. } => Ok(properties),
        }
    }
}
