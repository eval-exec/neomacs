//! Booted-`Context` tests re-homed from a lower crate
//! (`TEST-SUPPORT-DESIGN.md` section 1a, lead ruling 01:1xZ option A).
//!
//! Bodies, test names and assertions are byte-identical to the origin
//! files; only `use` paths were rewritten onto public crate paths.
//! Origin crate: `neovm-value`.

#[path = "emacs_char/tests/str_as_multibyte_gnu_test.rs"]
mod emacs_char_tests_str_as_multibyte_gnu_test_rs;

#[path = "error/tests/flow_word_test.rs"]
mod error_tests_flow_word_test_rs;

#[path = "heap_types/tests/clear_string_in_place_test.rs"]
mod heap_types_tests_clear_string_in_place_test_rs;

#[path = "error/tests/gc_tls_ownership_test.rs"]
mod error_tests_gc_tls_ownership_test_rs;

#[path = "intern/tests/gc_tls_ownership_test.rs"]
mod intern_tests_gc_tls_ownership_test_rs;

#[path = "tagged_transport/tests.rs"]
mod tagged_transport_tests_rs;

#[path = "value/tests/equal_lookup_gnu_test.rs"]
mod value_tests_equal_lookup_gnu_test_rs;
