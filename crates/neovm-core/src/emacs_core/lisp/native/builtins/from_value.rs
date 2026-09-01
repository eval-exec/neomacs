//! Typed argument extraction for builtins.
//!
//! GNU C primitives hand-write `CHECK_*` macros per argument because C has
//! no other option; the Rust port accumulated the same shape as repeated
//! `expect_int`/`expect_lisp_string` match blocks. `FromValue` centralizes
//! that pattern: the Rust parameter TYPE names the Lisp contract, and the
//! extraction derives the `wrong-type-argument` predicate from the type.
//!
//! Extraction is evaluator-aware because GNU coerces marker designators
//! wherever `integer-or-marker-p` applies, and a marker's position is read
//! from its live buffer (`marker_position_as_int_eval`). Purely structural
//! extractions simply ignore the evaluator.
//!
//! `typed_subr!` generates the fixed-arity `SubrFn`-shaped wrapper
//! (`fn(&mut Context, Value, ...) -> EvalResult`) that extracts each
//! argument before the body runs, so a builtin body starts with its
//! arguments already typed:
//!
//! ```ignore
//! typed_subr! {
//!     /// Doc comment passes through to the generated fn.
//!     pub(crate) fn builtin_example(eval, s: String, n: Option<i64>) -> EvalResult {
//!         let n = n.unwrap_or(1);
//!         Ok(Value::string(s.repeat(n.max(0) as usize)))
//!     }
//! }
//! ```
//!
//! The generated fn keeps the exact `SubrFn::A{N}` signature, so a fixed-shape
//! [`SubrSpec`](crate::emacs_core::subr::SubrSpec), the bytecode VM fast-path
//! delegates, and the JIT builtin table all consume it unchanged.

use crate::buffer::LispCharPos1;
use crate::emacs_core::error::expect_fixnum;
use crate::heap_types::LispString;

use super::*;

/// Extract a typed argument from a `Value`, signaling
/// `(wrong-type-argument PREDICATE value)` on mismatch. The implementing
/// type fixes PREDICATE, mirroring the GNU `CHECK_*` macro family.
pub(crate) trait FromValue: Sized {
    fn from_value(eval: &mut eval::Context, value: Value) -> Result<Self, Flow>;
}

/// Identity: accepts any value. Lets a typed signature keep raw `Value`
/// parameters for arguments with no single predicate.
impl FromValue for Value {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        Ok(value)
    }
}

/// `integerp` — fixnum-valued integer (mirrors `expect_int` / GNU
/// `CHECK_INTEGER` at fixnum call sites).
impl FromValue for i64 {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_int(&value)
    }
}

/// `numberp` — fixnum, float, or bignum lowered to f64 (mirrors
/// `expect_number` / GNU `CHECK_NUMBER` + `XFLOATINT`).
impl FromValue for f64 {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_number(&value)
    }
}

// `impl FromValue for &'static LispString` was here, and it was the one shape
// in this file that could not be made honest (DIVERGENCES.md 163).
//
// `FromValue::from_value` takes `value: Value` BY VALUE, so a borrow derived
// from it cannot outlive the call — yet `Self = &'static LispString` demands
// exactly that. It only compiled because `Value::as_lisp_string` launders
// `'static`; the moment `builtins::expect_lisp_string` started eliding its
// lifetime to the argument's, rustc rejected it with E0515, "returns a value
// referencing data owned by the current function". A conversion whose result
// type outlives its own input is not expressible, and no typed builtin
// declared a `&'static LispString` parameter, so it had no users to keep.
//
// `StringDesignator` below is the shape that works: a private `&'static`
// field with no public constructor and an accessor that reborrows from
// `&self`, so the borrow is bounded by the designator's own scope. Anything
// that wants a borrowed string parameter should copy that, not this.

/// `stringp` — lossy UTF-8 decode for text-only processing (mirrors
/// `expect_string_lossy`; raw eight-bit bytes become U+FFFD).
impl FromValue for String {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_string_lossy(&value)
    }
}

/// Lisp boolean: nil is false, anything else is true. Never signals.
impl FromValue for bool {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        Ok(!value.is_nil())
    }
}

/// `symbolp` — the symbol's identity (nil and keywords are symbols).
/// Honors `symbols-with-pos-enabled`: a symbol-with-pos unwraps to its
/// bare symbol exactly as GNU's `maybe_remove_pos_from_symbol` path does.
impl FromValue for SymId {
    fn from_value(eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_symbol_id_checked(&value, eval.symbols_with_pos_enabled)
    }
}

/// Optional argument: nil maps to `None`. The arity dispatcher pads
/// omitted `&optional` arguments with nil, so `Option<T>` models GNU's
/// optional-argument convention directly.
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        if value.is_nil() {
            Ok(None)
        } else {
            T::from_value(eval, value).map(Some)
        }
    }
}

/// `number-or-marker-p` — marker positions are read from their live
/// buffer (mirrors `expect_number_or_marker_eval`).
impl FromValue for NumberOrMarker {
    fn from_value(eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_number_or_marker_eval(eval, &value)
    }
}

/// `integer-or-marker-p` — a 1-based Lisp buffer position. Extraction
/// types the raw coordinate; range validation against the (possibly
/// narrowed) buffer stays with the caller, as in GNU
/// `CHECK_FIXNUM_COERCE_MARKER` + `validate_region`.
impl FromValue for LispCharPos1 {
    fn from_value(eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_integer_or_marker_eval(eval, &value).map(LispCharPos1::new)
    }
}

/// `fixnump` — strictly a fixnum (bignums rejected), mirroring
/// `expect_fixnum` / GNU `CHECK_FIXNUM`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) struct Fixnum(pub(crate) i64);

impl FromValue for Fixnum {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_fixnum(&value).map(Fixnum)
    }
}

/// `wholenump` — a non-negative fixnum, mirroring `expect_wholenump` /
/// GNU `CHECK_FIXNAT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) struct Wholenum(pub(crate) i64);

impl FromValue for Wholenum {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_wholenump(&value).map(Wholenum)
    }
}

/// `characterp` — a valid Emacs character code (0..=0x3FFFFF), mirroring
/// `expect_character_code` / GNU `CHECK_CHARACTER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) struct CharacterCode(pub(crate) i64);

impl FromValue for CharacterCode {
    fn from_value(_eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        expect_character_code(&value).map(CharacterCode)
    }
}

/// `stringp` — a borrowed string designator: strings pass through, symbols
/// contribute their exact name object (mirrors `expect_string_comparison_operand`,
/// GNU `string-equal`/`string-lessp` operand coercion).  The reference makes
/// cloning an operand into the comparison hot path impossible by construction.
///
/// GNU's `SYMBOLP` also accepts a symbol-with-pos while
/// `symbols-with-pos-enabled` is non-nil.  Resolve that dynamic view here so
/// every typed string-designator builtin has the same interpreter, bytecode,
/// and JIT contract.
///
/// DIVERGENCES.md 167: the designator now holds the OPERAND rather than a
/// borrow of it, which is how GNU writes the same coercion --
/// `if (SYMBOLP (s1)) s1 = SYMBOL_NAME (s1);` and only then `SDATA (s1)`
/// (`src/fns.c:344-353`).  Until then the inner field was a
/// `&'static LispString` with a doc comment explaining that the `'static` was
/// not a claim about lifetime; the field is now a `Value` (or, for a symbol,
/// the typed name view), so there is no `'static` left to explain and
/// [`StringDesignator::text`] reborrows from `&self` because that is the only
/// lifetime the data has.
#[derive(Clone, Copy, Debug)]
pub(crate) enum StringDesignator {
    /// A string argument, carried as the value the caller passed.
    String(Value),
    /// A symbol operand, carried as its Lisp-visible name: an object on this
    /// heap when the symbol has one, else its process-lifetime name atom.
    SymbolName(crate::emacs_core::intern::LispVisibleSymbolName),
}

impl StringDesignator {
    /// Borrow the designated string, for no longer than the designator lives.
    pub(crate) fn text(&self) -> &LispString {
        match self {
            Self::String(value) => value
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload"),
            Self::SymbolName(name) => name.text(),
        }
    }
}

impl FromValue for StringDesignator {
    fn from_value(eval: &mut eval::Context, value: Value) -> Result<Self, Flow> {
        let value = eval.unwrap_symbol(value);
        expect_string_comparison_operand(&value)
    }
}

/// Define fixed-arity builtins with typed arguments.
///
/// Expands to a plain `fn(&mut Context, Value, ...) -> EvalResult` (the
/// `SubrFn::A{N}` shape): each argument is extracted via [`FromValue`]
/// before the body runs, signaling `wrong-type-argument` with the
/// predicate derived from the parameter type. Register the result with the
/// matching `NativeFn::ContextN` variant. `SubrArity` controls which leading
/// slots are required; omitted trailing slots arrive as nil (use `Option<T>`).
macro_rules! typed_subr {
    ($($(#[$meta:meta])* $vis:vis fn $name:ident(
        $eval:ident $(, $arg:ident : $ty:ty)* $(,)?
    ) -> EvalResult $body:block)+) => {$(
        $(#[$meta])*
        $vis fn $name(
            $eval: &mut crate::emacs_core::eval::Context
            $(, $arg: crate::emacs_core::value::Value)*
        ) -> crate::emacs_core::error::EvalResult {
            $(
                let $arg = <$ty as crate::emacs_core::builtins::FromValue>::from_value(
                    $eval, $arg,
                )?;
            )*
            $body
        }
    )+};
}
pub(crate) use typed_subr;

#[cfg(test)]
#[path = "tests/from_value.rs"]
mod tests;
