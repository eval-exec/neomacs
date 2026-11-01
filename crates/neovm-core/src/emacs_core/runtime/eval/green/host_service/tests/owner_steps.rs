use super::super::super::scheduler::Control;
use super::*;
use crate::emacs_core::eval::ThreadingMode;
use crate::emacs_core::print::print_value;
use crate::host_effect::contracts::{
    CallbackStepArguments, CallbackStepResult, OwnerExecutionFrameRecord, OwnerRestoreFrameRecord,
};
use crate::host_effect::owner_step::{OwnerStepResult, PendingOwnerStep};

fn publish(baton: &Baton, heap: &TaggedHeap) -> RootBatchLease {
    let prepared = baton.prepare_parked_batch(heap, 0).unwrap();
    // SAFETY: this fixture holds its original heap's Baton; the empty root
    // slice stays live with no GC, callback or handoff through publication.
    unsafe { prepared.finalize(&[]) }
        .unwrap()
        .publish()
        .unwrap()
}

fn register<K: RegisteredRecord>(
    registry: &mut HostRegistry,
    baton: &Baton,
    heap: &TaggedHeap,
    parent: HostRequestKey,
    roots: &[Value],
) -> RegisteredRootRef<'static, K> {
    // SAFETY: these original-owner roots stay live under the held Baton for
    // the entire no-callback registration extent.
    unsafe { registry.register(baton, heap, ThreadKey::Main, parent, roots) }.unwrap()
}

#[derive(Clone, Copy)]
pub(in crate::emacs_core::eval::green::host_service) enum FixtureStepStage {
    Issued,
    Prepared,
    Published,
}

pub(in crate::emacs_core::eval::green::host_service) fn pending_fixture()
-> (Context, Baton, StepKey) {
    step_fixture(FixtureStepStage::Published)
}

pub(in crate::emacs_core::eval::green::host_service) fn step_fixture(
    stage: FixtureStepStage,
) -> (Context, Baton, StepKey) {
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    let baton = owner.start_green_runtime().unwrap();
    let input = Value::list(vec![Value::string("step-root-only"), Value::fixnum(73)]);
    let key = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        roots.insert(ThreadKey::Main, publish(&baton, &owner.tagged_heap));
        let registry = roots.hosts();
        // SAFETY: the original owner keeps input live across publication,
        // with no collection, callback or handoff in this extent.
        let call =
            unsafe { registry.prepare(&baton, &owner.tagged_heap, ThreadKey::Main, &[input]) }
                .unwrap();
        let parent = call.key();
        let arguments = register::<CallbackStepArguments>(
            registry,
            &baton,
            &owner.tagged_heap,
            parent,
            &[input, Value::NIL, Value::fixnum(5), input, Value::T],
        );
        let frame = register::<OwnerExecutionFrameRecord>(
            registry,
            &baton,
            &owner.tagged_heap,
            parent,
            &[Value::fixnum(17), Value::NIL, input, input],
        );
        let restore = register::<OwnerRestoreFrameRecord>(
            registry,
            &baton,
            &owner.tagged_heap,
            parent,
            &[Value::fixnum(29), Value::T, input],
        );
        let key = registry
            .issue_owner_step(
                &baton,
                &owner.tagged_heap,
                parent,
                ThreadKey::Main,
                OwnerInvocationId::from_owner_id(NonZeroU64::MIN),
            )
            .unwrap();
        match stage {
            FixtureStepStage::Issued => {}
            FixtureStepStage::Prepared | FixtureStepStage::Published => {
                let pending = PendingOwnerStep::callback(key, arguments, frame, restore).unwrap();
                registry
                    .prepare_owner_step(&baton, &owner.tagged_heap, pending)
                    .unwrap();
            }
        }
        roots.publish_host(call).unwrap();
        match stage {
            FixtureStepStage::Issued | FixtureStepStage::Prepared => {}
            FixtureStepStage::Published => roots.hosts().publish_owner_step(&baton, key).unwrap(),
        }
        key
    });
    (owner, baton, key)
}

fn restored<'a>(owner: &'a Context, baton: &'a Baton) -> RestoredHostOwner<'a> {
    // Registry-only controls deliberately construct the same-heap authority;
    // materialization below separately exercises the actual seated constructor.
    RestoredHostOwner {
        context: owner,
        baton,
        owner: ThreadKey::Main,
    }
}

fn successor_pending(
    registry: &mut HostRegistry,
    owner: &Context,
    baton: &Baton,
    access: &ContinuationAccess,
    next: StepKey,
) -> PendingStep {
    // SAFETY: these immediate original-owner values stay live throughout
    // same-heap registration, with no callback, collection or handoff.
    let (arguments, frame, restore) = unsafe {
        (
            registry.register_continuation::<CallbackStepArguments>(
                baton,
                &owner.tagged_heap,
                access,
                &[Value::NIL, Value::fixnum(41)],
            ),
            registry.register_continuation::<OwnerExecutionFrameRecord>(
                baton,
                &owner.tagged_heap,
                access,
                &[Value::fixnum(43)],
            ),
            registry.register_continuation::<OwnerRestoreFrameRecord>(
                baton,
                &owner.tagged_heap,
                access,
                &[Value::T],
            ),
        )
    };
    PendingOwnerStep::callback(next, arguments.unwrap(), frame.unwrap(), restore.unwrap()).unwrap()
}

fn callback_completion(
    registry: &mut HostRegistry,
    owner: &Context,
    baton: &Baton,
    access: &ContinuationAccess,
    claim: StepClaim,
) -> StepCompletion {
    // SAFETY: this immediate stays live on its original requester, without a
    // collection, callback or handoff during registration.
    let result = unsafe {
        registry.register_continuation::<CallbackStepResult>(
            baton,
            &owner.tagged_heap,
            access,
            &[Value::T],
        )
    }
    .unwrap();
    claim.complete(OwnerStepResult::Callback(result)).unwrap()
}

#[test]
fn intermediate_claim_and_completion_never_settle_parent() {
    let (owner, baton, key) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert!(registry.settle(key.parent()).is_err());
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), key)
            .unwrap();
        assert!(matches!(
            registry.records[&key.parent()].state,
            RecordState::Published(_)
        ));
        // SAFETY: result is a live immediate original-owner value; this call
        // neither collects nor invokes owner code.
        let result = unsafe {
            registry.register_continuation::<CallbackStepResult>(
                &baton,
                &owner.tagged_heap,
                &access,
                &[Value::T],
            )
        }
        .unwrap();
        registry
            .complete_owner_step(
                &baton,
                &owner.tagged_heap,
                claim.complete(OwnerStepResult::Callback(result)).unwrap(),
            )
            .unwrap();
        assert!(
            registry.settle(key.parent()).is_err(),
            "main must consume the semantic completion before final settlement"
        );
        registry.take_owner_completion(&baton, key).unwrap();
        assert!(registry.take_owner_completion(&baton, key).is_err());
        assert!(matches!(
            registry.records[&key.parent()].state,
            RecordState::Published(_)
        ));
        assert!(registry.settle(key.parent()).is_ok());
    });
}

#[test]
fn closing_allows_only_claimed_continuation_results() {
    let (owner, baton, key) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), key)
            .unwrap();
        registry.begin_closing(&baton).unwrap();
        assert!(
            registry
                .issue_owner_step(
                    &baton,
                    &owner.tagged_heap,
                    key.parent(),
                    ThreadKey::Main,
                    key.invocation()
                )
                .is_err()
        );
        // SAFETY: immediate original-owner roots remain live in both calls;
        // ordinary admission must refuse, but this exact admitted claim may finish.
        let ordinary = unsafe {
            registry.register::<CallbackStepResult>(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Main,
                key.parent(),
                &[],
            )
        };
        assert!(matches!(ordinary, Err(HostRootError::Closed)));
        // SAFETY: this same-owner claim is live; no Lisp, collection or handoff
        // happens while its result is registered during Closing.
        let result = unsafe {
            registry.register_continuation::<CallbackStepResult>(
                &baton,
                &owner.tagged_heap,
                &access,
                &[Value::T],
            )
        }
        .unwrap();
        registry
            .complete_owner_step(
                &baton,
                &owner.tagged_heap,
                claim.complete(OwnerStepResult::Callback(result)).unwrap(),
            )
            .unwrap();
        registry.take_owner_completion(&baton, key).unwrap();
        registry.settle(key.parent()).unwrap();
    });
}

#[test]
fn same_parent_result_without_step_origin_cannot_complete_claim() {
    let (owner, baton, key) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let stale = register::<CallbackStepResult>(registry, &baton, &owner.tagged_heap, key.parent(), &[Value::fixnum(1)]);
        let (claim, _) = registry.claim_owner_step(&restored(&owner, &baton), key).unwrap();
        let done = claim.complete(OwnerStepResult::Callback(stale)).unwrap();
        let rejected = registry.complete_owner_step(&baton, &owner.tagged_heap, done).unwrap_err();
        assert!(matches!(rejected.error, OwnerStepError::StaleStep));
        assert_eq!(rejected.item.key(), key, "refusal retains the owned completion");
        assert!(matches!(registry.records[&key.parent()].steps.state, OwnerStepState::Claimed { key: expected, .. } if expected == key));
    });
}

#[test]
fn two_and_three_consecutive_steps_keep_parent_published_until_last_completion() {
    for step_count in [2, 3] {
        let (owner, baton, first) = pending_fixture();
        owner.with_green_runtime(|runtime| {
            let mut roots = runtime.parked.borrow_mut();
            let registry = roots.hosts();
            let mut current = first;
            for index in 0..step_count {
                let (claim, access) = registry
                    .claim_owner_step(&restored(&owner, &baton), current)
                    .unwrap();
                let next = if index + 1 < step_count {
                    let next = registry
                        .issue_successor(&baton, &owner.tagged_heap, &access)
                        .unwrap();
                    assert_eq!(next.parent(), first.parent());
                    assert_eq!(next.owner(), first.owner());
                    assert_eq!(next.invocation(), first.invocation());
                    assert!(next.nonce() > current.nonce());
                    assert!(
                        registry
                            .issue_successor(&baton, &owner.tagged_heap, &access)
                            .is_err()
                    );
                    let pending = successor_pending(registry, &owner, &baton, &access, next);
                    registry
                        .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
                        .unwrap();
                    assert!(registry.publish_owner_step(&baton, next).is_err());
                    OwnerNextStep::Prepared(next)
                } else {
                    OwnerNextStep::Absent
                };
                let completion = callback_completion(registry, &owner, &baton, &access, claim);
                registry
                    .complete_owner_step(&baton, &owner.tagged_heap, completion)
                    .unwrap();
                assert!(
                    registry
                        .claim_owner_step(&restored(&owner, &baton), current)
                        .is_err()
                );
                assert!(registry.settle(first.parent()).is_err());
                let ready = registry.take_owner_completion(&baton, current).unwrap();
                assert_eq!(ready.completion.key(), current);
                assert_eq!(ready.next, next);
                assert!(registry.take_owner_completion(&baton, current).is_err());
                assert!(matches!(
                    registry.records[&first.parent()].state,
                    RecordState::Published(_)
                ));
                let rejected = registry
                    .complete_owner_step(&baton, &owner.tagged_heap, ready.completion)
                    .unwrap_err();
                assert_eq!(rejected.item.key(), current);
                assert!(matches!(rejected.error, OwnerStepError::StaleStep));
                assert!(
                    registry
                        .issue_successor(&baton, &owner.tagged_heap, &access)
                        .is_err()
                );
                match next {
                    OwnerNextStep::Prepared(next) => {
                        assert!(matches!(
                            &registry.records[&first.parent()].steps.state,
                            OwnerStepState::Prepared(pending) if pending.key() == next
                        ));
                        registry.publish_owner_step(&baton, next).unwrap();
                        assert!(
                            registry
                                .claim_owner_step(&restored(&owner, &baton), current)
                                .is_err()
                        );
                        current = next;
                    }
                    OwnerNextStep::Absent => {
                        assert!(matches!(
                            registry.records[&first.parent()].steps.state,
                            OwnerStepState::Idle
                        ));
                    }
                }
            }
            registry.settle(first.parent()).unwrap();
        });
    }
}

#[test]
fn successor_preparation_refuses_wrong_nonce_access_and_duplicates_without_consuming_next() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let wrong_nonce = successor_pending(registry, &owner, &baton, &access, current);
        let rejected = registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, wrong_nonce)
            .unwrap_err();
        assert_eq!(rejected.item.key(), current);
        assert!(matches!(rejected.error, OwnerStepError::StaleStep));
        assert!(matches!(
            registry.records[&current.parent()].steps.state,
            OwnerStepState::Claimed { key, successor: OwnerSuccessorPreparation::Issued(expected) }
                if key == current && expected == next
        ));
        let stale_access = ContinuationAccess {
            key: next,
            _owner: PhantomData,
        };
        assert!(
            registry
                .issue_successor(&baton, &owner.tagged_heap, &stale_access)
                .is_err()
        );
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        let rejected = registry
            .prepare_successor(&baton, &owner.tagged_heap, &stale_access, pending)
            .unwrap_err();
        assert_eq!(rejected.item.key(), next);
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, rejected.item)
            .unwrap();
        let duplicate = successor_pending(registry, &owner, &baton, &access, next);
        let rejected = registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, duplicate)
            .unwrap_err();
        assert_eq!(rejected.item.key(), next);
        assert!(matches!(rejected.error, OwnerStepError::StaleStep));
        assert!(matches!(
            &registry.records[&current.parent()].steps.state,
            OwnerStepState::Claimed { key, successor: OwnerSuccessorPreparation::Prepared(pending) }
                if *key == current && pending.key() == next
        ));
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(
            registry
                .take_owner_completion(&baton, current)
                .unwrap()
                .next,
            OwnerNextStep::Prepared(next)
        );
    });
}

#[test]
fn unfinished_successor_refuses_completion_and_preserves_it_for_retry() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        let rejected = registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap_err();
        assert_eq!(rejected.item.key(), current);
        assert!(matches!(rejected.error, OwnerStepError::WrongStage));
        assert!(registry.validate_owner_completion(&baton, current).is_err());
        assert!(registry.settle(current.parent()).is_err());
        assert!(matches!(
            registry.records[&current.parent()].steps.state,
            OwnerStepState::Claimed { key, successor: OwnerSuccessorPreparation::Issued(expected) }
                if key == current && expected == next
        ));
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
            .unwrap();
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, rejected.item)
            .unwrap();
        let ready = registry.take_owner_completion(&baton, current).unwrap();
        assert_eq!(ready.completion.key(), current);
        assert_eq!(ready.next, OwnerNextStep::Prepared(next));
    });
}

#[test]
fn successor_schema_rejection_retains_pending_and_issued_key() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (_claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        let argument = schema_slots(pending.frame_schema()).unwrap()[0];
        let registered = registry
            .records
            .get_mut(&current.parent())
            .unwrap()
            .registered
            .get_mut(&argument.record())
            .unwrap();
        let original_kind = registered.kind;
        registered.kind = RecordKind::OwnerExecutionFrame;
        let rejected = registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
            .unwrap_err();
        assert_eq!(rejected.item.key(), next);
        assert!(matches!(
            rejected.error,
            OwnerStepError::Root(HostRootError::ForeignRecord)
        ));
        registry
            .records
            .get_mut(&current.parent())
            .unwrap()
            .registered
            .get_mut(&argument.record())
            .unwrap()
            .kind = original_kind;
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, rejected.item)
            .unwrap();
    });
}

#[test]
fn closing_allows_one_admitted_cleanup_successor_and_its_completion() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        registry.begin_closing(&baton).unwrap();
        assert!(
            registry
                .issue_owner_step(
                    &baton,
                    &owner.tagged_heap,
                    current.parent(),
                    current.owner(),
                    current.invocation()
                )
                .is_err()
        );
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(
            registry
                .take_owner_completion(&baton, current)
                .unwrap()
                .next,
            OwnerNextStep::Prepared(next)
        );
        registry.publish_owner_step(&baton, next).unwrap();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), next)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(
            registry.take_owner_completion(&baton, next).unwrap().next,
            OwnerNextStep::Absent
        );
        registry.settle(current.parent()).unwrap();
    });
}

#[test]
fn ended_owner_revocation_rejects_old_nonce_and_repeating_exact_nonce_is_bounded() {
    let (owner, baton, current) = pending_fixture();
    let next = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(
            registry
                .take_owner_completion(&baton, current)
                .unwrap()
                .next,
            OwnerNextStep::Prepared(next)
        );
        registry.publish_owner_step(&baton, next).unwrap();
        next
    });
    let ended = Control::ended_service_owner_for_test(next.parent(), next.owner(), &baton);
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert!(matches!(registry.revoke_owner_step(&baton, &ended, current), Err(OwnerStepError::StaleStep)));
        assert_eq!(registry.awaiting_owner_step(next.parent()).unwrap(), next);
        registry.revoke_owner_step(&baton, &ended, next).unwrap();
        let OwnerStepState::Revoked(revoked) = &registry.records[&next.parent()].steps.state else {
            panic!("exact ended-owner step was not retained");
        };
        assert_eq!(revoked.key, next);
        assert!(matches!(revoked.retained.as_ref(), OwnerStepState::AwaitingOwner(pending) if pending.key() == next));
        let retained = revoked.retained.as_ref() as *const OwnerStepState;
        for _ in 0..3 {
            registry.revoke_owner_step(&baton, &ended, next).unwrap();
            assert!(matches!(
                &registry.records[&next.parent()].steps.state,
                OwnerStepState::Revoked(revoked) if std::ptr::eq(revoked.retained.as_ref(), retained)
            ));
        }
        assert!(matches!(registry.revoke_owner_step(&baton, &ended, current), Err(OwnerStepError::StaleStep)));
        assert!(matches!(registry.awaiting_owner_step(next.parent()), Err(OwnerStepError::Revoked)));
        assert!(registry.settle(next.parent()).is_err());
    });
}

#[test]
fn ended_owner_revocation_rejects_foreign_parent_and_requester_proofs_without_mutation() {
    let (owner, baton, current) = pending_fixture();
    let other_parent = HostRequestKey::for_test(NonZeroU64::new(u64::MAX).unwrap());
    assert_ne!(other_parent, current.parent());
    let other_request =
        Control::ended_service_owner_for_test(other_parent, current.owner(), &baton);
    let other_requester = Control::ended_service_owner_for_test(
        current.parent(),
        ThreadKey::Worker(NonZeroU64::MIN),
        &baton,
    );
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let root_count = registry.records[&current.parent()].roots.len();
        assert!(matches!(
            registry.revoke_owner_step(&baton, &other_request, current),
            Err(OwnerStepError::StaleStep)
        ));
        assert!(matches!(
            registry.revoke_owner_step(&baton, &other_requester, current),
            Err(OwnerStepError::Root(HostRootError::ForeignOwner))
        ));
        assert_eq!(
            registry.awaiting_owner_step(current.parent()).unwrap(),
            current
        );
        assert_eq!(registry.records[&current.parent()].roots.len(), root_count);
        assert!(matches!(
            registry.records[&current.parent()].state,
            RecordState::Published(_)
        ));
    });
}

#[test]
fn ended_owner_revocation_retains_completed_result_and_prepared_successor_until_native_cleanup() {
    let (owner, baton, current) = pending_fixture();
    let (next, result_record) = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        let result_record = completion.result_slot().record();
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        (next, result_record)
    });
    let ended = Control::ended_service_owner_for_test(current.parent(), current.owner(), &baton);
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let root_count = registry.records[&current.parent()].roots.len();
        registry.revoke_owner_step(&baton, &ended, current).unwrap();
        let OwnerStepState::Revoked(revoked) = &registry.records[&current.parent()].steps.state
        else {
            panic!("completed step was not retained at revocation");
        };
        let OwnerStepState::Completed {
            completion,
            successor,
        } = revoked.retained.as_ref()
        else {
            panic!("revocation dropped the completed step");
        };
        assert_eq!(completion.key(), current);
        assert_eq!(completion.result_slot().record(), result_record);
        assert!(
            matches!(successor, OwnerPreparedSuccessor::Prepared(pending) if pending.key() == next)
        );
        assert!(registry.take_owner_completion(&baton, current).is_err());
        assert!(registry.settle(current.parent()).is_err());
        assert_eq!(registry.records[&current.parent()].roots.len(), root_count);

        let settled = roots
            .services
            .native_cleanup_settled_for_test(current.parent());
        let registry = roots.hosts();
        registry.finish_native_owner_step(&baton, settled).unwrap();
        assert!(matches!(
            registry.records[&current.parent()].steps.state,
            OwnerStepState::Idle
        ));
        assert!(matches!(
            registry.records[&current.parent()].state,
            RecordState::Published(_)
        ));
        assert_eq!(
            registry.records[&current.parent()].roots.len(),
            root_count,
            "native cleanup does not retire Lisp roots"
        );
        registry.settle(current.parent()).unwrap();
    });
}

#[test]
fn ended_owner_revocation_refuses_idle_and_terminal_records() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(
            registry
                .take_owner_completion(&baton, current)
                .unwrap()
                .next,
            OwnerNextStep::Absent
        );
    });
    let ended = Control::ended_service_owner_for_test(current.parent(), current.owner(), &baton);
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert!(matches!(
            registry.revoke_owner_step(&baton, &ended, current),
            Err(OwnerStepError::StaleStep)
        ));
        assert!(matches!(
            registry.records[&current.parent()].steps.state,
            OwnerStepState::Idle
        ));
        registry.settle(current.parent()).unwrap();
        assert!(matches!(
            registry.revoke_owner_step(&baton, &ended, current),
            Err(OwnerStepError::WrongStage)
        ));
        assert!(matches!(
            registry.records[&current.parent()].state,
            RecordState::TerminalAwaitingOwner
        ));
    });
}

#[test]
fn interruption_latches_before_claim_and_survives_successor_completion() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert_eq!(
            registry.owner_interruption(&baton, current).unwrap(),
            OwnerInterruption::Uninterrupted
        );
        let first = OwnerInterruption::Requested(CancelReason::TargetSignal);
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetSignal)
                .unwrap(),
            first
        );
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        assert_eq!(registry.owner_interruption(&baton, current).unwrap(), first);
        let next = registry
            .issue_successor(&baton, &owner.tagged_heap, &access)
            .unwrap();
        let pending = successor_pending(registry, &owner, &baton, &access, next);
        registry
            .prepare_successor(&baton, &owner.tagged_heap, &access, pending)
            .unwrap();
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetQuit)
                .unwrap(),
            first
        );
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(registry.owner_interruption(&baton, current).unwrap(), first);
        let ready = registry.take_owner_completion(&baton, current).unwrap();
        assert_eq!(ready.completion.key(), current);
        assert_eq!(ready.next, OwnerNextStep::Prepared(next));
        assert_eq!(ready.interruption, first);
        assert!(matches!(
            registry.owner_interruption(&baton, current),
            Err(OwnerStepError::StaleStep)
        ));
        assert_eq!(registry.owner_interruption(&baton, next).unwrap(), first);
        registry.publish_owner_step(&baton, next).unwrap();
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), next)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        let ready = registry.take_owner_completion(&baton, next).unwrap();
        assert_eq!(ready.completion.key(), next);
        assert_eq!(ready.next, OwnerNextStep::Absent);
        assert_eq!(ready.interruption, first);
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetQuit)
                .unwrap(),
            first,
            "Published+Idle still retains its parent cancellation intent"
        );
        registry.settle(current.parent()).unwrap();
        assert!(matches!(
            registry.request_owner_interruption(
                &baton,
                current.parent(),
                CancelReason::SessionClosing
            ),
            Err(OwnerStepError::WrongStage)
        ));
        assert!(matches!(
            registry.owner_interruption(&baton, next),
            Err(OwnerStepError::WrongStage)
        ));
    });
}

#[test]
fn interruption_latch_keeps_first_quit_instead_of_reprioritizing_later_signal() {
    let (owner, baton, current) = pending_fixture();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let first = OwnerInterruption::Requested(CancelReason::TargetQuit);
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetQuit)
                .unwrap(),
            first
        );
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetSignal)
                .unwrap(),
            first
        );
        let (claim, access) = registry
            .claim_owner_step(&restored(&owner, &baton), current)
            .unwrap();
        let completion = callback_completion(registry, &owner, &baton, &access, claim);
        let result_record = completion.result_slot().record();
        registry
            .complete_owner_step(&baton, &owner.tagged_heap, completion)
            .unwrap();
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::SessionClosing)
                .unwrap(),
            first
        );
        let ready = registry.take_owner_completion(&baton, current).unwrap();
        assert_eq!(ready.interruption, first);
        assert_eq!(ready.completion.result_slot().record(), result_record);
    });
}

#[test]
fn interruption_remains_metadata_during_revocation_and_closing() {
    let (owner, baton, current) = pending_fixture();
    let ended = Control::ended_service_owner_for_test(current.parent(), current.owner(), &baton);
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        registry.revoke_owner_step(&baton, &ended, current).unwrap();
        let OwnerStepState::Revoked(revoked) = &registry.records[&current.parent()].steps.state
        else {
            panic!("the exact ended step must remain retained");
        };
        let retained = revoked.retained.as_ref() as *const OwnerStepState;
        let first = OwnerInterruption::Requested(CancelReason::TargetQuit);
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetQuit)
                .unwrap(),
            first
        );
        registry.begin_closing(&baton).unwrap();
        assert_eq!(registry.owner_interruption(&baton, current).unwrap(), first);
        assert_eq!(
            registry
                .request_owner_interruption(&baton, current.parent(), CancelReason::TargetSignal)
                .unwrap(),
            first
        );
        assert!(matches!(
            &registry.records[&current.parent()].steps.state,
            OwnerStepState::Revoked(revoked) if std::ptr::eq(revoked.retained.as_ref(), retained)
        ));
    });
}

#[test]
fn interruption_rejects_foreign_heap_owner_invocation_and_unpublished_parent() {
    let (owner, baton, current) = pending_fixture();
    let other_heap = TaggedHeap::new();
    let mut foreign_registry = HostRegistry::new(other_heap.heap_identity());
    assert!(matches!(
        foreign_registry.request_owner_interruption(
            &baton,
            current.parent(),
            CancelReason::TargetQuit
        ),
        Err(OwnerStepError::Root(HostRootError::ForeignHeap { .. }))
    ));
    assert!(matches!(
        foreign_registry.owner_interruption(&baton, current),
        Err(OwnerStepError::Root(HostRootError::ForeignHeap { .. }))
    ));
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        let wrong_owner = OwnerStepKey::new(
            current.parent(),
            ThreadKey::Worker(NonZeroU64::MIN),
            current.invocation(),
            current.nonce(),
        );
        assert!(matches!(
            registry.owner_interruption(&baton, wrong_owner),
            Err(OwnerStepError::Root(HostRootError::ForeignOwner))
        ));
        let wrong_invocation = OwnerStepKey::new(
            current.parent(),
            current.owner(),
            OwnerInvocationId::from_owner_id(NonZeroU64::new(2).unwrap()),
            current.nonce(),
        );
        assert!(matches!(
            registry.owner_interruption(&baton, wrong_invocation),
            Err(OwnerStepError::StaleStep)
        ));
        // SAFETY: empty original-owner roots stay live during this held-Baton
        // registration extent, with no GC, callback or handoff.
        let unpublished =
            unsafe { registry.prepare(&baton, &owner.tagged_heap, current.owner(), &[]) }.unwrap();
        assert!(matches!(
            registry.request_owner_interruption(
                &baton,
                unpublished.key(),
                CancelReason::TargetQuit
            ),
            Err(OwnerStepError::WrongStage)
        ));
        assert_eq!(
            registry.owner_interruption(&baton, current).unwrap(),
            OwnerInterruption::Uninterrupted
        );
        assert_eq!(
            registry.awaiting_owner_step(current.parent()).unwrap(),
            current
        );
    });
}

#[test]
fn owner_frame_roots_survive_collection_and_nested_real_wait() {
    let (mut owner, baton, key) = pending_fixture();
    let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
    let receiver = control.register_main();
    owner.take_green_seat(ThreadKey::Main, control, receiver, baton);
    let depth = owner.vm_root_frames.len();
    owner
        .with_owner_execution_frame(key, |frame, _claim, _access| {
            assert_eq!(frame.key(), key);
            assert_eq!(
                frame.interruption().unwrap(),
                OwnerInterruption::Uninterrupted
            );
            assert_eq!(
                print_value(&frame.arguments().unwrap()[0]),
                "(\"step-root-only\" 73)"
            );
            let input = frame.arguments().unwrap()[0];
            assert_eq!(
                frame.arguments().unwrap(),
                &[input, Value::NIL, Value::fixnum(5), input, Value::T]
            );
            assert_eq!(
                frame.execution_roots().unwrap(),
                &[Value::fixnum(17), Value::NIL, input, input]
            );
            assert_eq!(
                frame.restore_roots().unwrap(),
                &[Value::fixnum(29), Value::T, input]
            );
            frame.execute(|context| {
                context.gc_collect_exact();
                context
                    .green_park(super::super::super::scheduler::Wait::Runnable)
                    .unwrap();
                context.gc_collect_exact();
            });
            assert_eq!(
                frame.interruption().unwrap(),
                OwnerInterruption::Uninterrupted
            );
            assert_eq!(
                print_value(&frame.arguments().unwrap()[0]),
                "(\"step-root-only\" 73)"
            );
            let input = frame.arguments().unwrap()[0];
            assert_eq!(
                frame.arguments().unwrap(),
                &[input, Value::NIL, Value::fixnum(5), input, Value::T]
            );
            assert_eq!(
                frame.execution_roots().unwrap(),
                &[Value::fixnum(17), Value::NIL, input, input]
            );
            assert_eq!(
                frame.restore_roots().unwrap(),
                &[Value::fixnum(29), Value::T, input]
            );
        })
        .unwrap();
    assert_eq!(
        owner.vm_root_frames.len(),
        depth,
        "RAII restores the ordinary VM root scope"
    );
}

#[test]
fn retained_flow_keeps_selected_handler_and_private_backtrace_on_requester() {
    use crate::emacs_core::error::{InlineErrorOp, LispCondition, signal};
    use crate::emacs_core::eval::ResumeTarget;
    let (mut owner, baton, key) = pending_fixture();
    let control = owner.with_green_runtime(|runtime| std::sync::Arc::clone(&runtime.control));
    let receiver = control.register_main();
    owner.take_green_seat(ThreadKey::Main, control, receiver, baton);
    let bad_operand = Value::string("owned deferred backtrace operand");
    let mut flow = signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("listp"), bad_operand],
    )
    .with_bytecode_error_backtrace(InlineErrorOp::Car, &[bad_operand]);
    let original = flow.as_signal_mut().unwrap();
    assert!(original.bytecode_error_backtrace.is_some());
    original.search_complete = true;
    original.selected_resume = Some(ResumeTarget::InterpreterCatch);
    let original_payload = original as *const _;
    owner
        .with_owner_execution_frame(key, |frame, claim, access| {
            assert_eq!(
                frame.interruption().unwrap(),
                OwnerInterruption::Uninterrupted
            );
            let result = frame.retain_flow(&access, flow).unwrap();
            frame.execute(|context| context.gc_collect_exact());
            let done = claim.complete(OwnerStepResult::Flow(result)).unwrap();
            frame.execute(|context| {
                // Exercise real same-owner completion validation without copying
                // the private SignalData/backtrace fields into an RPC object.
                let seat = context.green_seat().unwrap();
                let baton = seat.take_owner_step_baton().unwrap();
                context
                    .with_green_runtime(|runtime| {
                        let mut roots = runtime.parked.borrow_mut();
                        let registry = roots.hosts();
                        let completed =
                            registry.complete_owner_step(&baton, &context.tagged_heap, done);
                        for reason in [CancelReason::TargetSignal, CancelReason::TargetQuit] {
                            assert_eq!(
                                registry
                                    .request_owner_interruption(&baton, key.parent(), reason)
                                    .unwrap(),
                                OwnerInterruption::Requested(CancelReason::TargetSignal),
                                "intent must not consume the completed original Flow"
                            );
                        }
                        completed
                    })
                    .unwrap();
                context
                    .green_seat()
                    .unwrap()
                    .restore_owner_step_baton(baton);
            });
            assert_eq!(
                frame.interruption().unwrap(),
                OwnerInterruption::Requested(CancelReason::TargetSignal)
            );
        })
        .unwrap();
    owner.gc_collect_exact();
    let returned = owner.take_owner_step_flow(key).unwrap();
    let signal = returned.as_signal().unwrap();
    assert_eq!(
        signal as *const _, original_payload,
        "the exact object is retained, not reconstructed"
    );
    assert!(signal.search_complete);
    assert!(matches!(
        signal.selected_resume,
        Some(ResumeTarget::InterpreterCatch)
    ));
    assert!(signal.bytecode_error_backtrace.is_some());
    assert_eq!(
        print_value(&signal.data[1]),
        "\"owned deferred backtrace operand\""
    );
    assert!(
        owner.take_owner_step_flow(key).is_err(),
        "delivery consumes the owner-local flow exactly once"
    );
}
