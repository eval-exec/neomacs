//! The value Display formatter installs only through the sealed unsafe hook.
//!
//! A safe call would bypass the contract that names the two one-time install
//! seams (`Context::new_inner` and `Context::from_dump`); the installer must
//! therefore stay `unsafe`.

use neovm_core::emacs_core::value::Value;
use neovm_core::emacs_core::value::display_hook::{
    ValueDisplayFormatter, install_value_display_formatter,
};

fn formatter(_value: &Value, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    Ok(())
}

fn main() {
    let table = ValueDisplayFormatter::new(formatter);
    let _ = install_value_display_formatter(table);
}
