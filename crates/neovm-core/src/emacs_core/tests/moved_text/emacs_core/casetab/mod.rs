pub use neovm_text::emacs_core::casetab::*;
#[path = "../text/casetab/tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;
#[path = "../text/casetab/tests/casetab_test.rs"]
mod tests;
pub(crate) use crate::emacs_core::casetab_entries::builtin_set_case_table;
