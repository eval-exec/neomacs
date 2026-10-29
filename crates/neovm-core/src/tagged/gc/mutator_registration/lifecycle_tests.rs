#![forbid(unsafe_code)]

use super::super::mark_word::MarkWord;
use super::{EntryPhase, ProofEraFinishFailure as EraFinishFailure, RegistrationError};
use crate::emacs_core::eval::Context;
use crate::tagged::value::TaggedValue;
use std::num::NonZeroU64;
use std::sync::atomic::Ordering;

fn epoch() -> NonZeroU64 {
    NonZeroU64::new(1).unwrap()
}

fn retain_failed_era(failure: EraFinishFailure<'_>, expected: RegistrationError) {
    assert_eq!(failure.reason, expected);
    assert!(failure.era.core.keeper.is_some());
    assert!(failure.era.core.collector.borrow().is_some());
    assert!(failure.era.core.registry.is_some());
    // The fallback must retain authority/backing. No test repairs a forgotten
    // resource or pretends that its missing destructor discharged an obligation.
    drop(failure);
}

#[test]
fn forgotten_start_permit_is_not_treated_as_cancelled_or_omitted() {
    Context::with_test_mutator_era(|era| {
        let permit = era.register(41, 2).unwrap();
        let entry = std::sync::Arc::clone(&permit.entry);
        std::mem::forget(permit);
        assert!(!entry.cancelled_start.load(Ordering::Acquire));
        assert_eq!(entry.state.lock().unwrap().phase, EntryPhase::Starting);
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        let failure = era.finish().unwrap_err();
        assert_eq!(entry.state.lock().unwrap().phase, EntryPhase::Starting);
        retain_failed_era(failure, RegistrationError::Busy);
        assert!(!entry.cancelled_start.load(Ordering::Acquire));
        assert_eq!(entry.state.lock().unwrap().phase, EntryPhase::Starting);
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
            let local = run.state();
            local
                .root_scratch
                .push(MarkWord::of(TaggedValue::fixnum(42)));
            local.hot.pacing.promoted_since_major = 73;
        }
        std::mem::forget(token);
        {
            let current = entry.state.lock().unwrap();
            assert_eq!(current.phase, EntryPhase::Running);
            assert!(current.image.is_none());
        }
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        retain_failed_era(era.finish().unwrap_err(), RegistrationError::Busy);
        let current = entry.state.lock().unwrap();
        assert_eq!(current.phase, EntryPhase::Running);
        assert!(current.image.is_none());
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
        let loan = token.take_young_loan().unwrap();
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
            let current = entry.state.lock().unwrap();
            assert_eq!(current.phase, EntryPhase::Running);
            assert!(current.worker_loan);
            assert!(current.image.is_none());
        }
        retain_failed_era(
            era.finish().unwrap_err(),
            RegistrationError::OutstandingLoan,
        );
        assert!(entry.state.lock().unwrap().worker_loan);
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
        assert!(matches!(
            failure.era.registry().gate.state.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
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
            run.state()
                .root_scratch
                .push(MarkWord::of(TaggedValue::fixnum(45)));
        }
        token.begin_publication(epoch()).unwrap().publish().unwrap();
        let entry = token.entry();
        let mut view = era.stopped_summaries().unwrap();
        assert_eq!(view.census().unwrap().entries, 2);
        drop(token); // Atomic poison must work while the view owns the gate.
        assert_eq!(view.census(), Err(RegistrationError::Poisoned));
        let mut visited = false;
        assert_eq!(
            view.visit_work(|_, _| visited = true),
            Err(RegistrationError::Poisoned)
        );
        assert!(!visited);
        assert_eq!(
            view.with_young(entry.key, entry.young, |_| ()),
            Err(RegistrationError::Poisoned)
        );
        drop(view);
        {
            let current = entry.state.lock().unwrap();
            assert_eq!(current.phase, EntryPhase::Published);
            assert_eq!(current.image.as_ref().unwrap().summary().root_words, 1);
        }
        retain_failed_era(era.finish().unwrap_err(), RegistrationError::Poisoned);
        assert_eq!(
            entry
                .state
                .lock()
                .unwrap()
                .image
                .as_ref()
                .unwrap()
                .summary()
                .root_words,
            1
        );
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
            run.state().hot.remset.push(TaggedValue::fixnum(46));
        }
        token.finish(epoch()).unwrap();
        let view = era.stopped_summaries().unwrap();
        let before = view.census().unwrap();
        let mut callbacks = 0;
        view.visit_work(|_, _| {
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
