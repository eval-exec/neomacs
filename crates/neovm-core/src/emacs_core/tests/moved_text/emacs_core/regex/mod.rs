pub use neovm_text::emacs_core::regex::*;
#[path = "../text/regex/tests/declared_signed_positions.rs"]
mod declared_signed_position_tests;
#[path = "../text/regex/tests/gc_collection_epoch_minor_test.rs"]
mod gc_collection_epoch_minor_tests;
#[path = "../text/regex/tests/gc_collection_epoch_test.rs"]
mod gc_collection_epoch_tests;
#[path = "../text/regex/tests/gc_literal_epoch_test.rs"]
mod gc_literal_epoch_tests;
#[path = "../text/regex/tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;
#[path = "../text/regex/tests/pike_vm_fuzz_regression_test.rs"]
mod pike_vm_fuzz_regression_tests;
#[path = "../text/regex/tests/positioned_policy_test.rs"]
mod positioned_policy_tests;
#[path = "../text/regex/tests/regex_test.rs"]
mod tests;
