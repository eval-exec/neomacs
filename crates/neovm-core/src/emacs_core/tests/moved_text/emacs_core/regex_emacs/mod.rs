pub use neovm_text::emacs_core::regex_emacs::*;
#[path = "../text/regex/tests/casefold_scan_test.rs"]
mod casefold_scan_tests;
#[path = "../text/regex/tests/declared_boundary_positions.rs"]
mod declared_boundary_positions_tests;
pub mod dfa;
#[path = "../text/regex/tests/fail_stack_parity_test.rs"]
mod fail_stack_parity_tests;
#[path = "../text/regex/tests/raw_byte_syntax_test.rs"]
mod raw_byte_syntax_tests;
#[path = "../text/regex/tests/start_anchor_test.rs"]
mod start_anchor_tests;
#[path = "../text/regex/tests/emacs_test.rs"]
mod tests;
