//! GNU-compatible interpretation of `invisible` property values.
//!
//! This is deliberately pure and independent of buffers, overlays and the
//! display iterator.  Lisp builtins, editor motion and the layout engine all
//! resolve their effective property value separately, then use this one
//! classifier so GNU's nil/t/ellipsis states cannot drift between subsystems.

use crate::emacs_core::value::{Value, eq_value};

/// The internal meaning of GNU's numeric `invisible-p` result.
///
/// Lisp compatibility still exposes nil/t/2, but native consumers match this
/// enum so adding or changing an invisibility class is compiler-checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invisibility {
    Visible,
    Hidden,
    HiddenWithEllipsis,
}

impl Invisibility {
    pub const fn hides_source(self) -> bool {
        !matches!(self, Self::Visible)
    }

    pub const fn shows_ellipsis(self) -> bool {
        matches!(self, Self::HiddenWithEllipsis)
    }

    pub(crate) fn into_lisp(self) -> Value {
        match self {
            Self::Visible => Value::NIL,
            Self::Hidden => Value::T,
            Self::HiddenWithEllipsis => Value::fixnum(2),
        }
    }
}

fn invisible_prop_member(propval: Value, list: Value) -> Invisibility {
    let mut tail = list;
    while tail.is_cons() {
        let element = tail.cons_car();
        if eq_value(&propval, &element) {
            return Invisibility::Hidden;
        }
        if element.is_cons() && eq_value(&propval, &element.cons_car()) {
            return if element.cons_cdr().is_nil() {
                Invisibility::Hidden
            } else {
                Invisibility::HiddenWithEllipsis
            };
        }
        tail = tail.cons_cdr();
    }
    Invisibility::Visible
}

fn invisible_prop(propval: Value, list: Value) -> Invisibility {
    let direct = invisible_prop_member(propval, list);
    if direct.hides_source() {
        return direct;
    }

    let mut proptail = propval;
    while proptail.is_cons() {
        let result = invisible_prop_member(proptail.cons_car(), list);
        if result.hides_source() {
            return result;
        }
        proptail = proptail.cons_cdr();
    }
    Invisibility::Visible
}

/// GNU `TEXT_PROP_MEANS_INVISIBLE`: classify a resolved `invisible` property
/// against the current buffer's `buffer-invisibility-spec`.
pub fn text_prop_means_invisible(prop: Value, invisibility_spec: Value) -> Invisibility {
    if invisibility_spec == Value::T {
        if prop.is_truthy() {
            Invisibility::Hidden
        } else {
            Invisibility::Visible
        }
    } else {
        invisible_prop(prop, invisibility_spec)
    }
}
