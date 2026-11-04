//! Redisplay ownership state owned by the evaluator layer (stage-4 W2 E5).
//!
//! `ModeLineOutputOwner` is GNU's shared formatter state for one evaluator
//! World: the World owns the counter, nested `format-mode-line` calls reset it
//! even for NOPROP, and GNU's unwind vector restores the target but never the
//! counter. The state is plain data (no Lisp values, no retained output); the
//! display-side formatter scopes and attribution stay in xdisp and drive it
//! through the accessors below.

use std::cell::Cell;
use std::rc::Rc;

use neovm_value::error::{Flow, LispCondition, signal};
use neovm_value::tagged::value::Fixnum;
use neovm_value::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeLineElementId(Fixnum);

impl Default for ModeLineElementId {
    fn default() -> Self {
        Self(Fixnum::saturating(0))
    }
}

impl ModeLineElementId {
    #[doc(hidden)]
    pub fn next(self) -> Result<Self, Flow> {
        i64::from(self.0)
            .checked_add(1)
            .and_then(|value| Fixnum::try_from(value).ok())
            .map(Self)
            .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))
    }

    pub fn into_value(self) -> Value {
        Value::from_fixnum(self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputKind {
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
pub struct ModeLineOutputOwner(Rc<OutputState>);

static_assertions::assert_not_impl_any!(ModeLineOutputOwner: Send, Sync);
static_assertions::assert_eq_size!(ModeLineOutputOwner, usize);

impl ModeLineOutputOwner {
    pub fn display_active(&self) -> bool {
        self.0.display_active.get()
    }

    /// Set the active-display flag and return the previous extent's value.
    /// Drop guards in the display formatter restore it with this result.
    pub fn replace_display_active(&self, active: bool) -> bool {
        self.0.display_active.replace(active)
    }

    pub fn advance(&self) -> Result<ModeLineElementId, Flow> {
        let next = self.0.element.get().next()?;
        self.0.element.set(next);
        Ok(next)
    }

    pub fn current(&self) -> ModeLineElementId {
        self.0.element.get()
    }

    pub fn target(&self) -> OutputKind {
        self.0.target.get()
    }

    /// Install a new output kind for a nested formatter extent, returning the
    /// enclosing target for restoration on Drop.
    pub fn replace_target(&self, target: OutputKind) -> OutputKind {
        self.0.target.replace(target)
    }

    pub fn set_target(&self, target: OutputKind) {
        self.0.target.set(target);
    }

    pub fn reset_element(&self) {
        self.0.element.set(ModeLineElementId::default());
    }
}
