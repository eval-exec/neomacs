//! Fast halves of `Containment::FastOutside` leaves. These run OUTSIDE the
//! trampoline's `catch_unwind`, so a panic here would abort at the
//! `extern "C"` boundary: every function must be panic-free by
//! construction (loads, tag tests and bounded loops only), which the lints
//! below enforce at the source level. Anything a fast half cannot answer is
//! `None`, and the trampoline runs the whole body under containment.

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::arithmetic_side_effects
)]

use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// `Bnth` on the shapes a loop takes: a count in 0..=127 over a walk that
/// ends on a cons (the element) or nil (nil). A non-list tail, a count
/// outside that range or a non-fixnum count is `None`: the contained
/// `bytecode_nth_values` signals or walks it. Must agree with that function
/// wherever it answers.
pub(super) fn nth_fast(_: &Context, args: &[Value; 4]) -> Option<Value> {
    let [n, list, _, _] = *args;
    let n = n.as_fixnum()?;
    if !(0..=127).contains(&n) {
        return None;
    }
    let mut tail = list;
    for _ in 0..n {
        if !tail.is_cons() {
            break;
        }
        tail = tail.cons_cdr();
    }
    if tail.is_cons() {
        Some(tail.cons_car())
    } else if tail.is_nil() {
        Some(Value::NIL)
    } else {
        None
    }
}
