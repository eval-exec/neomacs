use super::ProofEraFinishFailure as EraFinishFailure;
use super::image::PublishedGcWork;
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue;

fn epoch(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}
fn finish(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}

pub(super) fn activate_retry<'era>(
    mut permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    owner: u64,
) -> Mutator<'era> {
    loop {
        match permit.activate(heap, owner) {
            Ok(token) => return token,
            Err(failed) if failed.reason == RegistrationError::Busy => {
                permit = failed.into_permit();
                std::thread::yield_now();
            }
            Err(failed) => panic!("activation failed: {failed:?}"),
        }
    }
}
pub(super) fn finish_retry(mut token: Mutator<'_>, epoch: NonZeroU64) {
    loop {
        match token.finish(epoch) {
            Ok(()) => return,
            Err(failed) if failed.reason == RegistrationError::Busy => {
                token = failed.into_owner();
                std::thread::yield_now();
            }
            Err(failed) => panic!("retirement failed: {failed:?}"),
        }
    }
}

#[test]
fn two_destination_threads_publish_one_canonical_census() {
    Context::with_test_mutator_era(|era| {
        let initial = era.stopped_summaries().unwrap().census().unwrap();
        let heap = era.heap_identity();
        let permits = [era.register(1, 8).unwrap(), era.register(2, 8).unwrap()];
        std::thread::scope(|scope| {
            for (index, permit) in permits.into_iter().enumerate() {
                scope.spawn(move || {
                    let owner = index as u64 + 1;
                    let mut token = activate_retry(permit, heap, owner);
                    assert_eq!(token.thread, std::thread::current().id());
                    loop {
                        match token.run_access() {
                            Ok(mut run) => {
                                let state = run.state();
                                state.hot.allocated_count = owner as usize;
                                state.hot.bytes_since_gc = owner as usize * 16;
                                state.hot.pacing.promoted_since_major = owner as usize * 10;
                                state.hot.pacing.minors_since_major = owner as usize;
                                state.hot.remset.push(TaggedValue::NIL);
                                state.hot.major_cons_writes.push(TaggedValue::NIL);
                                break;
                            }
                            Err(RegistrationError::Busy) => std::thread::yield_now(),
                            Err(error) => panic!("run access failed: {error:?}"),
                        }
                    }
                    finish_retry(token, epoch(1));
                });
            }
        });
        let view = era.stopped_summaries().unwrap();
        let total = view.census().unwrap();
        assert_eq!(total.entries, 3); // frozen serial owner plus BOTH real tokens
        assert_eq!(total.retiring, 2);
        assert_eq!(total.allocated_count, initial.allocated_count + 3);
        assert_eq!(total.bytes_since_gc, initial.bytes_since_gc + 48);
        assert_eq!(
            total.promoted_since_major,
            initial.promoted_since_major + 30
        );
        assert_eq!(total.minors_since_major, initial.minors_since_major + 3);
        assert_eq!(total.remembered_words, initial.remembered_words + 2);
        assert_eq!(total.cons_writes, initial.cons_writes + 2);
        let mut visited = Vec::new();
        view.visit_work(|key, work| {
            if matches!(
                work,
                PublishedGcWork::Remembered(_) | PublishedGcWork::ConsWrite(_)
            ) && key.owner != 0
            {
                visited.push(key.owner);
            }
        })
        .unwrap();
        visited.sort_unstable();
        assert_eq!(visited, [1, 1, 2, 2]);
        drop(view);
        finish(era);
    });
}

#[test]
fn cancelled_starts_are_never_runnable_and_owner_can_retry() {
    Context::with_test_mutator_era(|era| {
        let first = era.register(17, 2).unwrap();
        let old = first.key();
        drop(first);
        let permit = era.register(17, 2).unwrap();
        assert_ne!(permit.key(), old);
        let heap = era.heap_identity();
        permit.activate(heap, 17).unwrap().finish(epoch(1)).unwrap();
        let census = era.stopped_summaries().unwrap().census().unwrap();
        assert_eq!(census.entries, 2);
        assert_eq!(census.cancelled_starts, 1);
        finish(era);
    });
}

#[test]
fn wrong_start_heap_or_owner_returns_the_live_permit() {
    Context::with_test_mutator_era(|era| {
        let other = super::super::TaggedHeap::new();
        let permit = era.register(1, 2).unwrap();
        let failure = permit.activate(other.heap_identity(), 1).unwrap_err();
        assert_eq!(failure.reason, RegistrationError::WrongHeap);
        let failure = failure
            .into_permit()
            .activate(era.heap_identity(), 99)
            .unwrap_err();
        assert_eq!(failure.reason, RegistrationError::WrongOwner);
        failure
            .into_permit()
            .activate(era.heap_identity(), 1)
            .unwrap()
            .finish(epoch(1))
            .unwrap();
        finish(era);
    });
}

#[test]
fn wrong_key_heap_owner_and_generation_refuse_without_changes() {
    Context::with_test_mutator_era(|era| {
        let permit = era.register(1, 0).unwrap();
        let key = permit.key();
        let foreign = super::super::TaggedHeap::new();
        let gate = era.registry().lock().unwrap();
        for (mut bad, expected) in [
            (key, RegistrationError::WrongHeap),
            (key, RegistrationError::WrongOwner),
            (key, RegistrationError::StaleRegistration),
        ] {
            match expected {
                RegistrationError::WrongHeap => bad.heap = foreign.heap_identity(),
                RegistrationError::WrongOwner => bad.owner += 1,
                _ => bad.generation = epoch(key.generation.get() + 1),
            }
            assert_eq!(era.registry().validate(&gate, bad).unwrap_err(), expected);
        }
        assert_eq!(gate.entries.len(), 2);
        drop(gate);
        drop(permit);
        finish(era);
    });
}

#[test]
fn generation_exhaustion_and_duplicate_owner_leave_membership_unchanged() {
    Context::with_test_mutator_era(|era| {
        let permit = era.register(1, 0).unwrap();
        assert_eq!(
            era.register(1, 0).unwrap_err(),
            RegistrationError::DuplicateOwner
        );
        let mut gate = era.registry().lock().unwrap();
        let before = gate.entries.len();
        let next = gate.next_registration;
        gate.generation_limit = next;
        drop(gate);
        assert_eq!(
            era.register(2, 0).unwrap_err(),
            RegistrationError::Exhausted
        );
        let gate = era.registry().lock().unwrap();
        assert_eq!(gate.entries.len(), before);
        assert_eq!(gate.next_registration, next);
        drop(gate);
        drop(permit);
        finish(era);
    });
}

#[test]
fn running_or_starting_owner_cannot_be_omitted_from_summary() {
    Context::with_test_mutator_era(|era| {
        let permit = era.register(1, 0).unwrap();
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        let mut token = permit.activate(era.heap_identity(), 1).unwrap();
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        token
            .begin_publication(epoch(1))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(
            era.stopped_summaries().unwrap().census().unwrap().entries,
            2
        );
        token.finish(epoch(1)).unwrap();
        finish(era);
    });
}

#[test]
fn owner_publication_restores_exact_image_and_refuses_old_epoch() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(1, 3)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        {
            let mut run = token.run_access().unwrap();
            run.state().hot.pacing.promoted_since_major = 47;
        }
        token
            .begin_publication(epoch(1))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(token.restore(epoch(2)), Err(RegistrationError::Publication));
        token.restore(epoch(1)).unwrap();
        assert_eq!(
            token
                .local
                .as_ref()
                .unwrap()
                .hot
                .pacing
                .promoted_since_major,
            47
        );
        assert_eq!(
            token.begin_publication(epoch(1)).unwrap().publish(),
            Err(RegistrationError::Publication)
        );
        token.finish(epoch(2)).unwrap();
        finish(era);
    });
}

#[test]
fn outstanding_worker_loan_refuses_publication_and_retirement_then_recovers() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(1, 0)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let loan = token.take_young_loan().unwrap();
        assert_eq!(
            token.begin_publication(epoch(1)).unwrap().publish(),
            Err(RegistrationError::OutstandingLoan)
        );
        let failed = token.finish(epoch(1)).unwrap_err();
        assert_eq!(failed.reason, RegistrationError::OutstandingLoan);
        let mut token = failed.into_owner();
        token.return_young_loan(loan).unwrap();
        token.finish(epoch(1)).unwrap();
        finish(era);
    });
}

#[test]
fn wrong_owner_worker_return_keeps_the_only_payload_for_retry() {
    Context::with_test_mutator_era(|era| {
        let heap = era.heap_identity();
        let mut first = era.register(1, 0).unwrap().activate(heap, 1).unwrap();
        let mut second = era.register(2, 0).unwrap().activate(heap, 2).unwrap();
        let loan = first.take_young_loan().unwrap();
        let failed = second.return_young_loan(loan).unwrap_err();
        assert_eq!(failed.reason, RegistrationError::WrongOwner);
        first.return_young_loan(failed.into_loan()).unwrap();
        first.finish(epoch(1)).unwrap();
        second.finish(epoch(1)).unwrap();
        finish(era);
    });
}

#[test]
fn young_resolver_checks_one_canonical_entry_and_retiring_generation() {
    Context::with_test_mutator_era(|era| {
        let token = era
            .register(1, 0)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let key = token.key();
        let young = token.entry.young;
        token.finish(epoch(1)).unwrap();
        let mut view = era.stopped_summaries().unwrap();
        assert_eq!(view.with_young(key, young, |log| log.id()).unwrap(), young);
        let mut bad = key;
        bad.generation = epoch(key.generation.get() + 1);
        assert_eq!(
            view.with_young(bad, young, |_| ()),
            Err(RegistrationError::StaleRegistration)
        );
        assert_eq!(view.census().unwrap().retiring, 1);
        drop(view);
        finish(era);
    });
}

#[test]
fn panic_abandonment_poison_retains_owner_state_and_world_authority() {
    Context::with_test_mutator_era(|era| {
        let initial = era.registry().gate.state.lock().unwrap().entries.len();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut token = era
                .register(1, 3)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            token
                .local
                .as_mut()
                .unwrap()
                .root_scratch
                .push(MarkWord::of(TaggedValue::NIL));
            panic!("owner failed before publication");
        }));
        assert!(result.is_err());
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Poisoned)
        ));
        assert_eq!(
            era.registry().gate.state.lock().unwrap().entries.len(),
            initial + 1
        );
        let failed = era.finish().unwrap_err();
        assert_eq!(failed.reason, RegistrationError::Poisoned);
        assert!(failed.era.core.keeper.is_some());
        drop(failed); // Conservative fallback retains the World and entry set.
    });
}

#[test]
fn unfinished_token_drop_and_lost_loan_fail_closed() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(1, 0)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let loan = token.take_young_loan().unwrap();
        drop(loan);
        assert_eq!(
            token.begin_publication(epoch(1)).err(),
            Some(RegistrationError::Poisoned)
        );
        drop(token);
        assert!(matches!(
            era.finish(),
            Err(EraFinishFailure {
                reason: RegistrationError::Poisoned,
                ..
            })
        ));
    });
}

#[test]
fn summary_reentry_refuses_busy_instead_of_blocking_keeper() {
    Context::with_test_mutator_era(|era| {
        let view = era.stopped_summaries().unwrap();
        assert_eq!(era.register(1, 0).unwrap_err(), RegistrationError::Busy);
        assert!(matches!(
            era.stopped_summaries(),
            Err(RegistrationError::Busy)
        ));
        assert_eq!(view.census().unwrap().entries, 1);
        drop(view);
        drop(era.register(1, 0).unwrap());
        finish(era);
    });
}

#[test]
fn root_capacity_failure_returns_unclaimed_permit_for_safe_cancellation() {
    Context::with_test_mutator_era(|era| {
        let permit = era.register(1, usize::MAX).unwrap();
        let failed = permit.activate(era.heap_identity(), 1).unwrap_err();
        assert_eq!(failed.reason, RegistrationError::Capacity);
        drop(failed.into_permit());
        assert_eq!(
            era.stopped_summaries()
                .unwrap()
                .census()
                .unwrap()
                .cancelled_starts,
            1
        );
        finish(era);
    });
}

#[test]
fn pending_root_retirement_failure_preserves_entry_and_allows_explicit_transfer() {
    Context::with_test_mutator_era(|era| {
        let mut token = era
            .register(1, 1)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        token
            .local
            .as_mut()
            .unwrap()
            .root_scratch
            .push(MarkWord::of(TaggedValue::NIL));
        token.finish(epoch(1)).unwrap();
        let failed = era.finish().unwrap_err();
        assert_eq!(failed.reason, RegistrationError::PendingRoots);
        let gate = failed.era.registry().lock().unwrap();
        let mut state = gate.entries[1].state.lock().unwrap();
        let retained = state.image.as_mut().unwrap().take_root_scratch();
        assert_eq!(retained.len(), 1);
        // NIL is immediate; no heap referent is released by this transfer.
        drop(retained);
        drop(state);
        drop(gate);
        finish(failed.era);
    });
}

#[test]
fn old_registration_key_is_rejected_after_reentering_the_same_world() {
    Context::with_test_mutator_era(|era| {
        let token = era
            .register(1, 0)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let old = token.key();
        token.finish(epoch(1)).unwrap();
        let (collector, keeper) = era.finish().unwrap();
        let again = ParallelHeapEra::admit(collector, keeper).unwrap();
        let permit = again.register(1, 0).unwrap();
        let new = permit.key();
        assert_eq!(old.heap, new.heap);
        assert_eq!(old.owner, new.owner);
        assert_eq!(old.registration, new.registration);
        assert_ne!(old.generation, new.generation);
        let gate = again.registry().lock().unwrap();
        assert_eq!(
            again.registry().validate(&gate, old).unwrap_err(),
            RegistrationError::StaleRegistration
        );
        drop(gate);
        drop(permit);
        finish(again);
    });
}

#[test]
fn checked_generation_issuer_refuses_wrap_without_changing_the_counter() {
    let counter = AtomicU64::new(u64::MAX - 1);
    assert_eq!(issue_generation(&counter).unwrap().get(), u64::MAX - 1);
    assert_eq!(
        issue_generation(&counter),
        Err(RegistrationError::Exhausted)
    );
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
}
