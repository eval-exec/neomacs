//! Core-authored regression pins (print_buffer F1 / F1-a, lead 13:08Z and
//! 13:58Z). The World print accumulator's type-state makes an unguarded
//! append impossible in-crate (`prepare` is the only mint of the `PrintActive`
//! token). A `Copy` token can still be retained, so the token carries a
//! per-prepare (epoch, depth) identity and every operation checks it against
//! the current top:
//!
//! * a token reused after its recorded unwind must panic (Idle);
//! * a token from an earlier prepare used while a NEWER print is Active must
//!   panic (ABA);
//! * an outer token is rejected while an inner prepare owns the cursor, and
//!   becomes valid again after the inner unwind (GNU `print_unwind` restores
//!   the outer epoch).
//!
//! Pre-fix these paths returned silently in release and `pending` answered
//! stale retained bytes. The fence fixtures exercise them; they must panic
//! loudly in debug AND release.

#[test]
#[should_panic(expected = "print accumulator is Idle while a prepare token is live")]
fn print_accumulator_stale_token_is_loud_in_every_profile() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    neovm_eval::test_support::authority::print_accumulator_stale_token_for_test(&mut eval);
}

/// ABA: prepare -> unwind -> prepare, then the stale first token.
#[test]
#[should_panic(expected = "print accumulator token is stale (prepare epoch mismatch)")]
fn print_accumulator_aba_stale_token_is_loud_in_every_profile() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    neovm_eval::test_support::authority::print_accumulator_aba_stale_for_test(&mut eval);
}

/// The outer token is not valid while an inner prepare owns the cursor.
#[test]
#[should_panic(expected = "print accumulator token is stale (prepare epoch mismatch)")]
fn print_accumulator_outer_token_is_rejected_while_inner_is_active() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    neovm_eval::test_support::authority::print_accumulator_outer_while_inner_active_for_test(
        &mut eval,
    );
}

/// ...and valid again after the inner unwind restores the outer epoch.
#[test]
fn print_accumulator_outer_token_is_valid_after_inner_unwind() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let pending =
        neovm_eval::test_support::authority::print_accumulator_outer_after_inner_unwind_for_test(
            &mut eval,
        );
    assert_eq!(
        pending,
        b"oO".to_vec(),
        "the outer print continues at the inner unwind's left-over pos (GNU A1 quirk)"
    );
}
