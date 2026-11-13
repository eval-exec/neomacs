//! Peer 5719: unlinked display fontsets have the exact empty generation.

use super::Context;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn fontset_generation_matches_the_empty_registry_snapshot() {
    let _context = Context::new(crate::test_utils::unit_runtime());
    let generation = super::display_query_hooks::fontset_generation();
    let snapshot = super::lifecycle::fontset_registry_snapshot();
    assert_eq!(generation, snapshot.generation);
    assert_eq!(generation, 0);
}

#[test]
fn fontset_subrs_still_refuse_without_the_display_layer() {
    for (form, marker) in [
        (
            r#"(new-fontset "fontset-absent" nil)"#,
            "ABSENT-LAYER core::new-fontset",
        ),
        (
            "(set-fontset-font t nil nil)",
            "ABSENT-LAYER core::set-fontset-font",
        ),
        ("(fontset-font t 65)", "ABSENT-LAYER core::fontset-font"),
        (
            r#"(query-fontset "fontset-absent")"#,
            "ABSENT-LAYER core::query-fontset",
        ),
    ] {
        let mut context = Context::new(crate::test_utils::unit_runtime());
        let error = catch_unwind(AssertUnwindSafe(|| context.eval_str(form)))
            .expect_err("fontset behavior must remain ABSENT");
        let message = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| error.downcast_ref::<&str>().copied())
            .expect("ABSENT panic has text");
        assert!(message.contains(marker), "{form}: {message}");
    }
}

#[test]
fn unchanged_unit_runtime_redisplay_signatures_compare_equal() {
    let context = Context::new(crate::test_utils::unit_runtime());
    let first = context.redisplay_signature();
    let second = context.redisplay_signature();
    assert_eq!(first.fontset_generation, 0);
    assert_eq!(first, second);
}
