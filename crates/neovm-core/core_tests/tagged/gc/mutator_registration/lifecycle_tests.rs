//! Lifecycle refusal tests re-homed from value (P1 value tree, MEMQ-TAIL-SHAPES 3).
//!
//! The era fixture is eval-level (`Context::with_test_mutator_era`), so the
//! whole module lives here. The value-side fields stay private (N1); the
//! entry/state facts are read through copied observers and the three run-state
//! seeds go through typed unsafe fences (class C3). Every assertion is the
//! pre-move assertion.

use super::era_fixtures::EraFixtureExt;
use crate::emacs_core::eval::Context;
use crate::tagged::value::TaggedValue;
use neovm_eval::test_support::authority::WorldProofEraFinishFailure as EraFinishFailure;
use neovm_value::tagged::gc::mutator_registration::{EntryPhase, RegistrationError};
use neovm_value::test_support::authority::{
    push_remset_for_test, push_root_scratch_for_test, set_promoted_since_major_for_test,
    take_young_loan_for_test,
};
use std::num::NonZeroU64;

fn epoch() -> NonZeroU64 {
    NonZeroU64::new(1).unwrap()
}

fn retain_failed_era(failure: EraFinishFailure<'_>, expected: RegistrationError) {
    assert_eq!(failure.reason(), expected);
    let retained = failure.retained_for_test();
    assert!(retained.keeper);
    assert!(retained.collector);
    assert!(retained.registry);
    // The fallback must retain authority/backing. No test repairs a forgotten
    // resource or pretends that its missing destructor discharged an obligation.
    drop(failure);
}

#[test]
fn forgotten_start_permit_is_not_treated_as_cancelled_or_omitted() {
    Context::with_test_mutator_era(|era| {
        let permit = era.register(41, 2).unwrap();
        let entry = permit.entry_for_test();
        std::mem::forget(permit);
        assert!(!entry.cancelled_start_for_test());
        assert_eq!(entry.phase_for_test(), EntryPhase::Starting);
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        let failure = era.finish().unwrap_err();
        assert_eq!(entry.phase_for_test(), EntryPhase::Starting);
        retain_failed_era(failure, RegistrationError::Busy);
        assert!(!entry.cancelled_start_for_test());
        assert_eq!(entry.phase_for_test(), EntryPhase::Starting);
    });
}

#[test]
fn forgotten_running_mutator_never_becomes_a_stopped_image() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(42, 1)
            .unwrap()
            .activate(era.heap_identity(), 42)
            .unwrap();
        let entry = token.entry();
        {
            let mut run = token.run_access().unwrap();
            // SAFETY: this is the owner thread's exclusive run access on a
            // serial fixture; no collection runs concurrently and the seeded
            // values are fixnums (same-heap immediates).
            unsafe {
                push_root_scratch_for_test(&mut run, TaggedValue::fixnum(42));
                set_promoted_since_major_for_test(&mut run, 73);
            }
        }
        std::mem::forget(token);
        {
            assert_eq!(entry.phase_for_test(), EntryPhase::Running);
            assert!(entry.image_root_words_for_test().is_none());
        }
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        retain_failed_era(era.finish().unwrap_err(), RegistrationError::Busy);
        assert_eq!(entry.phase_for_test(), EntryPhase::Running);
        assert!(entry.image_root_words_for_test().is_none());
    });
}

#[test]
fn forgotten_worker_loan_keeps_publication_retirement_and_era_finish_blocked() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(43, 1)
            .unwrap()
            .activate(era.heap_identity(), 43)
            .unwrap();
        let entry = token.entry();
        let loan = take_young_loan_for_test(&mut token).unwrap();
        std::mem::forget(loan);
        assert_eq!(
            token.begin_publication(epoch()).unwrap().publish(),
            Err(RegistrationError::OutstandingLoan)
        );
        let failure = token.finish(epoch()).unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OutstandingLoan);
        // A lost loan cannot be reconstructed from the empty owner shell. Keep
        // the blocked owner rather than fabricating a return or publication.
        std::mem::forget(failure.into_owner());
        {
            assert_eq!(entry.phase_for_test(), EntryPhase::Running);
            assert!(entry.worker_loan_for_test());
            assert!(entry.image_root_words_for_test().is_none());
        }
        retain_failed_era(
            era.finish().unwrap_err(),
            RegistrationError::OutstandingLoan,
        );
        assert!(entry.worker_loan_for_test());
    });
}

#[test]
fn forgotten_summary_guard_refuses_reentry_and_teardown_without_waiting() {
    Context::with_test_mutator_era(|era| {
        let view = era.stopped_summaries().unwrap();
        assert_eq!(view.census().unwrap().entries, 1);
        std::mem::forget(view);
        assert_eq!(era.register(44, 0).unwrap_err(), RegistrationError::Busy);
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        let failure = era.finish().unwrap_err();
        assert!(failure.registry_gate_held_for_test());
        // This Drop must not try to recover the leaked mutex guard or wait for
        // it. The whole era remains retained behind its conservative fallback.
        retain_failed_era(failure, RegistrationError::Busy);
    });
}

#[test]
fn dropping_published_owner_invalidates_an_already_open_summary_view() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(45, 1)
            .unwrap()
            .activate(era.heap_identity(), 45)
            .unwrap();
        {
            let mut run = token.run_access().unwrap();
            // SAFETY: owner-thread exclusive run access on a serial fixture;
            // no collection runs concurrently and the seed is a fixnum.
            unsafe {
                push_root_scratch_for_test(&mut run, TaggedValue::fixnum(45));
            }
        }
        token.begin_publication(epoch()).unwrap().publish().unwrap();
        let entry = token.entry();
        let mut view = era.stopped_summaries().unwrap();
        assert_eq!(view.census().unwrap().entries, 2);
        drop(token); // Atomic poison must work while the view owns the gate.
        assert_eq!(view.census(), Err(RegistrationError::Poisoned));
        let mut visited = false;
        assert_eq!(
            view.visit_work_count_for_test(|| visited = true),
            Err(RegistrationError::Poisoned)
        );
        assert!(!visited);
        assert_eq!(
            view.with_young_entry_for_test(&entry),
            Err(RegistrationError::Poisoned)
        );
        drop(view);
        {
            assert_eq!(entry.phase_for_test(), EntryPhase::Published);
            assert_eq!(entry.image_root_words_for_test(), Some(1));
        }
        retain_failed_era(era.finish().unwrap_err(), RegistrationError::Poisoned);
        assert_eq!(entry.image_root_words_for_test(), Some(1));
    });
}

#[test]
fn work_visitor_reentry_into_same_view_refuses_busy_without_waiting() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(46, 0)
            .unwrap()
            .activate(era.heap_identity(), 46)
            .unwrap();
        {
            let mut run = token.run_access().unwrap();
            // SAFETY: owner-thread exclusive run access on a serial fixture;
            // no collection runs concurrently and the seed is a fixnum.
            unsafe {
                push_remset_for_test(&mut run, TaggedValue::fixnum(46));
            }
        }
        token.finish(epoch()).unwrap();
        let view = era.stopped_summaries().unwrap();
        let before = view.census().unwrap();
        let mut callbacks = 0;
        view.visit_work_count_for_test(|| {
            assert_eq!(view.census(), Err(RegistrationError::Busy));
            callbacks += 1;
        })
        .unwrap();
        assert!(
            callbacks > 0,
            "the seeded work must exercise visitor reentry"
        );
        assert_eq!(view.census().unwrap(), before);
        drop(view);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}
