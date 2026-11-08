//! Eager macro-expansion failures propagate like GNU's (P4.1 S1.0).
//!
//! GNU `readevalloop_eager_expand_eval` (src/lread.c:2135-2152) has no
//! fallback: the error `internal-macroexpand-for-load` raises
//! (lisp/emacs-lisp/macroexp.el:915-948) aborts the load or `eval-buffer` at
//! the failing form.  The expected strings are GNU 31.1's output for the same
//! forms (`emacs -Q --batch`); the oracle pins are
//! `neovm-oracle-tests/src/eager_expansion_failure_semantics.rs`.
//!
//! The three forms that need `runtime_startup_eval_one` live next to that
//! harness in core. This file keeps the image-construction policy test.

use super::{
    EagerExpansionFailure, ImageConstructionExpansionScope, eager_expansion_failure_policy,
};

/// The fallback is scoped to image construction and restored on exit.
#[test]
fn image_construction_scope_is_the_only_fallback_and_nests() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eager_expansion_failure_policy(),
        EagerExpansionFailure::Propagate
    );
    {
        let _outer = ImageConstructionExpansionScope::enter();
        assert_eq!(
            eager_expansion_failure_policy(),
            EagerExpansionFailure::FallBackWhileBuildingImage
        );
        {
            let _inner = ImageConstructionExpansionScope::enter();
        }
        assert_eq!(
            eager_expansion_failure_policy(),
            EagerExpansionFailure::FallBackWhileBuildingImage
        );
    }
    assert_eq!(
        eager_expansion_failure_policy(),
        EagerExpansionFailure::Propagate
    );
}
