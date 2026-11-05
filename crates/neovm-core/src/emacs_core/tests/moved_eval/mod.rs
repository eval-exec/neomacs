//! Tests re-homed from neovm-eval under the post-split option A ruling
//! (TEST-SUPPORT-DESIGN.md rev 2, lead 01:1xZ).
//!
//! File names, test names and assertions are byte-identical to the origin;
//! only `use` paths changed (public crate paths plus the documented
//! API-drift shims). Origin crate: neovm-eval.

#[cfg(test)]
#[path = "emacs_core/lisp/custom/tests/custom_test.rs"]
mod emacs_core_lisp_custom_tests_custom_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/lread/tests/lread_test.rs"]
mod emacs_core_lisp_lread_tests_lread_test_rs;
