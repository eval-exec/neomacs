//! PEER 5786 step 2: startup census on fresh and pdump-restored Contexts.
//!
//! Dedicated rows, not a per-Context check (lead #6267): every canonical
//! symbol reachable from a function cell, a bound value cell or a plist
//! must be an obarray member, except GNU's documented startup-uninterned
//! spellings. The planted-negative control lives in the value crate
//! (`symbol::membership_audit::tests`), where a non-member can be placed
//! directly.

use neovm_eval::emacs_core::eval::Context;

fn assert_clean(eval: &Context, surface: &str) {
    let violations = eval.obarray().audit_startup_lisp_visibility();
    assert!(
        violations.is_empty(),
        "{surface}: {} Lisp-visible symbols are not obarray members: {:?}",
        violations.len(),
        violations
    );
}

#[test]
fn fresh_context_has_no_lisp_visible_non_members() {
    // The full startup surface, not the thin unit runtime: the unit
    // runtime deliberately omits the DEFSYM boot seeding, so its obarray is
    // not the product image the census is about.
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::load_boot::create_bootstrap_evaluator_cached()
        .expect("bootstrap template should succeed");
    assert_clean(&eval, "fresh bootstrap Context");
}

#[test]
fn pdump_restored_context_has_no_lisp_visible_non_members() {
    crate::test_utils::init_test_tracing();
    let template = crate::emacs_core::load_boot::create_bootstrap_evaluator_cached()
        .expect("bootstrap template should succeed");
    let snapshot = crate::emacs_core::pdump::snapshot_evaluator(&template);
    let restored =
        crate::emacs_core::pdump::restore_snapshot(&snapshot, template.hooks_installed())
            .expect("pdump restore should succeed");
    assert_clean(&restored, "pdump-restored Context");
}
