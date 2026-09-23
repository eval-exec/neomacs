use strum::{EnumString, IntoStaticStr};

use super::value::{Value, ValueKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
enum RawPrefixSymbol {
    #[strum(serialize = "-")]
    Minus,
}

impl RawPrefixSymbol {
    fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

/// Return GNU Emacs's numeric meaning of a raw prefix argument.
///
/// Mirrors `src/callint.c::Fprefix_numeric_value`: nil means 1, the
/// symbol `-' means -1, a cons contributes its fixnum car, a fixnum is
/// returned as-is, and every other object means 1.
pub(crate) fn prefix_numeric_value(raw: &Value) -> i64 {
    if raw.is_nil() {
        return 1;
    }
    if RawPrefixSymbol::from_lisp_value(raw) == Some(RawPrefixSymbol::Minus) {
        return -1;
    }
    if let ValueKind::Cons = raw.kind() {
        return match raw.cons_car().kind() {
            ValueKind::Fixnum(n) => n,
            _ => 1,
        };
    }
    match raw.kind() {
        ValueKind::Fixnum(n) => n,
        _ => 1,
    }
}

#[cfg(test)]
#[path = "tests/prefix_test.rs"]
mod tests;
