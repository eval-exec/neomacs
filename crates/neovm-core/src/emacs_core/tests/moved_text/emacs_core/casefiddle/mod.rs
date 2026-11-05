pub use neovm_text::emacs_core::casefiddle::*;
#[path = "../text/casefiddle/tests/casing_types.rs"]
mod casing_types;
#[path = "../text/casefiddle/tests/casefiddle_test.rs"]
mod tests;
pub(crate) use crate::emacs_core::casefiddle_entries::{
    builtin_capitalize_word, builtin_upcase_region,
};
