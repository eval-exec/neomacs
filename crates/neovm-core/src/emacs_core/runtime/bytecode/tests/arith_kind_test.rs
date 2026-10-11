//! `ArithGenericKind`'s discriminants are baked into generated code as
//! `kind as i64` immediates (`lower_generic_arith_site`) and into AOT
//! artifacts, so every value is pinned here to the numbering the raw-`i64`
//! tables used before the enum. Renumbering needs an `ABI_TAG_VERSION` bump.

use super::*;
use crate::emacs_core::intern::intern;
use strum::IntoEnumIterator;

/// The legacy `(op, kind, nargs, builtin)` table, verbatim.
const LEGACY: &[(Op, i64, usize, &str)] = &[
    (Op::Add, 0, 2, "+"),
    (Op::Sub, 1, 2, "-"),
    (Op::Mul, 2, 2, "*"),
    (Op::Div, 3, 2, "/"),
    (Op::Rem, 4, 2, "%"),
    (Op::Max, 5, 2, "max"),
    (Op::Min, 6, 2, "min"),
    (Op::Eqlsign, 7, 2, "="),
    (Op::Lss, 8, 2, "<"),
    (Op::Gtr, 9, 2, ">"),
    (Op::Leq, 10, 2, "<="),
    (Op::Geq, 11, 2, ">="),
    (Op::Add1, 12, 1, "1+"),
    (Op::Sub1, 13, 1, "1-"),
    (Op::Negate, 14, 1, "-"),
];

#[test]
fn arith_generic_kind_discriminants_are_pinned() {
    crate::test_utils::init_test_tracing();
    for (op, raw, nargs, builtin) in LEGACY {
        let kind = ArithGenericKind::from_op(op).expect("admitted opcode");
        assert_eq!(kind as i64, *raw, "{op:?}");
        assert_eq!(kind.arity(), *nargs, "{op:?}");
        assert_eq!(kind.builtin_id(), intern(builtin), "{op:?}");
        assert_eq!(ArithGenericKind::from_raw(*raw), Some(kind));
    }
    assert_eq!(ArithGenericKind::iter().count(), LEGACY.len());
}

#[test]
fn arith_generic_kind_from_raw_round_trips_and_rejects_the_rest() {
    for kind in ArithGenericKind::iter() {
        assert_eq!(ArithGenericKind::from_raw(kind as i64), Some(kind));
    }
    for raw in [-1, 15, 16, 100, i64::MIN, i64::MAX] {
        assert_eq!(ArithGenericKind::from_raw(raw), None, "{raw}");
    }
}

#[test]
fn only_the_arithmetic_opcodes_have_a_generic_kind() {
    for op in [Op::Car, Op::Cdr, Op::Eq, Op::Return, Op::Nth, Op::Aref] {
        assert_eq!(ArithGenericKind::from_op(&op), None, "{op:?}");
    }
}
