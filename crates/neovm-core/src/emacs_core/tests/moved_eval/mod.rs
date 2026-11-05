//! Tests re-homed from neovm-eval under the post-split option A ruling
//! (TEST-SUPPORT-DESIGN.md rev 2, lead 01:1xZ).
//!
//! File names, test names and assertions are byte-identical to the origin;
//! only `use` paths changed (public crate paths plus the documented
//! API-drift shims). Origin crate: neovm-eval.

#[cfg(test)]
#[path = "emacs_core/lisp/advice/tests/advice_test.rs"]
mod emacs_core_lisp_advice_tests_advice_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/custom/tests/custom_test.rs"]
mod emacs_core_lisp_custom_tests_custom_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/doc/tests/doc_test.rs"]
mod emacs_core_lisp_doc_tests_doc_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/lread/tests/lread_test.rs"]
mod emacs_core_lisp_lread_tests_lread_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/provide_coupled_vars/tests/runtime_surface_test.rs"]
mod emacs_core_lisp_provide_coupled_vars_tests_runtime_surface_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/quit_regression_test.rs"]
mod emacs_core_runtime_eval_tests_quit_regression_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/load/tests/stale_bytecode_test.rs"]
mod emacs_core_lisp_load_tests_stale_bytecode_test_rs;
