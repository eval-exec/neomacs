//! The value Display formatter table cannot be forged field by field.
//!
//! The representation is private so a table can only be built through
//! `ValueDisplayFormatter::new`, which keeps every installer inside the
//! signed hook contract.

use neovm_core::emacs_core::value::Value;
use neovm_core::emacs_core::value::display_hook::ValueDisplayFormatter;

fn formatter(_value: &Value, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    Ok(())
}

fn main() {
    let _forged = ValueDisplayFormatter { fmt: formatter };
}
