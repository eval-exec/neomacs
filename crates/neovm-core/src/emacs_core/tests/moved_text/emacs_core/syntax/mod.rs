pub use neovm_text::emacs_core::syntax::*;
#[path = "../text/syntax/tests/back_comment_safe_positions_test.rs"]
mod back_comment_safe_positions_test;
#[path = "../text/syntax/tests/declared_logical_state.rs"]
mod declared_logical_state_tests;
#[path = "../text/syntax/tests/forwarded_locals_gnu_test.rs"]
mod forwarded_locals_gnu_tests;
#[path = "../text/syntax/tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;
#[path = "../text/syntax/tests/issue554_ascii_memo_test.rs"]
mod issue554_ascii_memo_tests;
pub mod parse_cache;
pub mod parse_loop;
#[path = "../text/syntax/tests/parse_state_divergence_gnu_test.rs"]
mod parse_state_divergence_gnu_tests;
pub mod pps_propertize;
#[path = "../text/syntax/tests/pps_propertize_gnu_test.rs"]
mod pps_propertize_gnu_tests;
#[path = "../text/syntax/tests/scan_error_data_gnu_test.rs"]
mod scan_error_data_gnu_tests;
#[path = "../text/syntax/tests/signed_string_positions.rs"]
mod signed_string_positions_tests;
#[path = "../text/syntax/tests/skip_syntax_classes_test.rs"]
mod skip_syntax_classes_tests;
#[path = "../text/syntax/tests/syntax_prop_byte_run_test.rs"]
mod syntax_prop_byte_run_test;
#[path = "../text/syntax/tests/syntax_test.rs"]
mod tests;
