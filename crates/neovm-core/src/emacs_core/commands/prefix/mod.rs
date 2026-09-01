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
mod tests {
    use super::*;

    #[test]
    fn raw_prefix_symbol_domain_matches_gnu() {
        crate::test_utils::init_test_tracing();
        assert_eq!(
            RawPrefixSymbol::from_lisp_value(&Value::symbol("-")),
            Some(RawPrefixSymbol::Minus)
        );
        assert_eq!(RawPrefixSymbol::Minus.name(), "-");
        assert_eq!(RawPrefixSymbol::from_lisp_value(&Value::symbol("+")), None);
    }

    #[test]
    fn prefix_numeric_value_matches_gnu() {
        crate::test_utils::init_test_tracing();
        assert_eq!(prefix_numeric_value(&Value::NIL), 1);
        assert_eq!(prefix_numeric_value(&Value::symbol("-")), -1);
        assert_eq!(prefix_numeric_value(&Value::fixnum(4)), 4);
        assert_eq!(
            prefix_numeric_value(&Value::cons(Value::fixnum(16), Value::NIL)),
            16
        );
        assert_eq!(prefix_numeric_value(&Value::make_float(2.0)), 1);
        assert_eq!(
            prefix_numeric_value(&Value::cons(Value::make_float(2.0), Value::NIL)),
            1
        );
        assert_eq!(prefix_numeric_value(&Value::symbol("other")), 1);
    }
}
