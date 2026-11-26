use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestOwner {
    Main,
    Worker,
}

fn parent(id: u64) -> HostRequestKey {
    HostRequestKey::for_test(NonZeroU64::new(id).unwrap())
}
fn key(parent: HostRequestKey) -> OwnerStepKey<TestOwner> {
    OwnerStepKey::new(
        parent,
        TestOwner::Worker,
        OwnerInvocationId::from_owner_id(NonZeroU64::MIN),
        OwnerStepSequence::new().issue().unwrap(),
    )
}
fn reference<K: RegisteredRecord>(
    parent: HostRequestKey,
    record: u64,
) -> RegisteredRootRef<'static, K> {
    // SAFETY: metadata-only unit fixture. No roots are read/materialized and no
    // reference leaves the test. Production issuance requires retained batches.
    unsafe {
        RegisteredRootRef::from_registered(
            parent,
            RootRecordId::from_owner_id(NonZeroU64::new(record).unwrap()),
        )
    }
}
fn callback(step: OwnerStepKey<TestOwner>) -> PendingOwnerStep<'static, TestOwner> {
    PendingOwnerStep::callback(
        step,
        reference(step.parent(), 1),
        reference(step.parent(), 2),
        reference(step.parent(), 3),
    )
    .unwrap_or_else(|_| panic!("matching registered callback records"))
}
fn claim(step: OwnerStepKey<TestOwner>) -> ClaimedOwnerStep<'static, TestOwner> {
    callback(step)
        .claim(step)
        .unwrap_or_else(|_| panic!("matching callback owner"))
}

#[test]
fn registered_callback_consumes_exact_port_and_result_family() {
    let step = key(parent(1));
    let pending = callback(step);
    assert_eq!(pending.key(), step);
    assert_eq!(pending.family(), OwnerStepFamily::Callback);
    let PendingOwnerStep::Callback(entry) = &pending else {
        panic!("callback association preserved")
    };
    assert_eq!(entry.published().key(), step);
    assert_eq!(
        entry.published().arguments().record(),
        reference::<CallbackStepArguments>(parent(1), 1).record()
    );
    let claimed = pending
        .claim(step)
        .unwrap_or_else(|_| panic!("right owner"));
    let ClaimedOwnerStep::Callback(entry) = &claimed else {
        panic!("callback claim preserved")
    };
    assert_eq!(
        entry.published().frame().kind(),
        RecordKind::OwnerExecutionFrame
    );
    assert_eq!(
        entry.published().restore().kind(),
        RecordKind::OwnerRestoreFrame
    );
    let completed = claimed
        .complete(OwnerStepResult::Callback(reference(step.parent(), 4)))
        .unwrap_or_else(|_| panic!("right result family"));
    assert_eq!(completed.key(), step);
    assert_eq!(completed.family(), OwnerStepFamily::Callback);
    assert_eq!(
        completed.result_slot().kind(),
        RecordKind::CallbackStepResult
    );
    assert_eq!(
        completed.result_slot().record(),
        reference::<CallbackStepResult>(step.parent(), 4).record()
    );
    let (completed_key, result) = completed.into_parts();
    assert_eq!(completed_key, step);
    let OwnerStepResult::Callback(record) = result else {
        panic!("result family preserved")
    };
    assert_eq!(
        record.record(),
        reference::<CallbackStepResult>(step.parent(), 4).record()
    );
}

#[test]
fn wrong_parent_owner_invocation_or_nonce_preserves_pending_port() {
    let step = key(parent(1));
    let mut sequence = OwnerStepSequence::new();
    let _first = sequence.issue().unwrap();
    let wrong = [
        (
            OwnerStepKey {
                parent: parent(2),
                ..step
            },
            OwnerStepRefusal::WrongParent,
        ),
        (
            OwnerStepKey {
                owner: TestOwner::Main,
                ..step
            },
            OwnerStepRefusal::WrongOwner,
        ),
        (
            OwnerStepKey {
                invocation: OwnerInvocationId::from_owner_id(NonZeroU64::new(2).unwrap()),
                ..step
            },
            OwnerStepRefusal::WrongInvocation,
        ),
        (
            OwnerStepKey {
                nonce: sequence.issue().unwrap(),
                ..step
            },
            OwnerStepRefusal::WrongNonce,
        ),
    ];
    let mut pending = callback(step);
    for (expected, reason) in wrong {
        let rejected = match pending.claim(expected) {
            Err(rejected) => rejected,
            Ok(_) => panic!("foreign claim must not consume authority"),
        };
        assert_eq!(rejected.reason(), reason);
        (pending, _) = rejected.into_parts();
        assert_eq!(pending.key(), step);
    }
    let completed = pending
        .claim(step)
        .unwrap_or_else(|_| panic!("rightful claim remains usable"))
        .complete(OwnerStepResult::Callback(reference(step.parent(), 4)))
        .unwrap_or_else(|_| panic!("rightful completion remains usable"));
    assert_eq!(completed.key(), step);
}

#[test]
fn wrong_family_and_foreign_result_preserve_claim_and_payload() {
    let step = key(parent(1));
    let foreign = OwnerStepResult::Font(reference(step.parent(), 8));
    let rejected = match claim(step).complete(foreign) {
        Err(rejected) => rejected,
        Ok(_) => panic!("font result cannot answer callback"),
    };
    let ((claimed, foreign), reason) = rejected.into_parts();
    assert_eq!(reason, OwnerStepRefusal::WrongFamily);
    let OwnerStepResult::Font(record) = foreign else {
        panic!("foreign typed result preserved")
    };
    assert!(record.matches(step.parent()));
    let rejected = match claimed.complete(OwnerStepResult::Callback(reference(parent(2), 9))) {
        Err(rejected) => rejected,
        Ok(_) => panic!("another parent cannot answer same-family step"),
    };
    let ((claimed, result), reason) = rejected.into_parts();
    assert_eq!(reason, OwnerStepRefusal::WrongRecord);
    assert!(result.matches(parent(2)));
    assert!(
        claimed
            .complete(OwnerStepResult::Callback(reference(step.parent(), 10)))
            .is_ok()
    );
}

#[test]
fn construction_rejects_foreign_frame_without_losing_registered_records() {
    let step = key(parent(1));
    let rejected = match PendingOwnerStep::callback(
        step,
        reference(step.parent(), 1),
        reference(parent(2), 2),
        reference(step.parent(), 3),
    ) {
        Err(rejected) => rejected,
        Ok(_) => panic!("foreign frame cannot mint a resume port"),
    };
    let (published, reason) = rejected.into_parts();
    assert_eq!(reason, OwnerStepRefusal::WrongRecord);
    assert_eq!(published.key(), step);
    assert!(published.arguments().matches(step.parent()));
    assert!(published.frame().matches(parent(2)));
    assert!(published.restore().matches(step.parent()));
}

#[test]
fn every_closed_family_enumerates_typed_argument_execution_and_restore_records() {
    let step = key(parent(1));
    macro_rules! check {
        ($constructor:ident, $variant:ident, $kind:ident) => {{
            let pending = PendingOwnerStep::$constructor(
                step,
                reference(step.parent(), 1),
                reference(step.parent(), 2),
                reference(step.parent(), 3),
            )
            .unwrap_or_else(|_| panic!("matching schema records"));
            let OwnerFrameSchema::$variant {
                arguments,
                frame,
                restore,
            } = pending.frame_schema()
            else {
                panic!("schema must preserve family")
            };
            assert_eq!(arguments.kind(), RecordKind::$kind);
            assert_eq!(frame.kind(), RecordKind::OwnerExecutionFrame);
            assert_eq!(restore.kind(), RecordKind::OwnerRestoreFrame);
            assert_eq!(
                arguments.record(),
                reference::<$kind>(step.parent(), 1).record()
            );
            assert_ne!(frame.record(), restore.record());
        }};
    }
    check!(redisplay, Redisplay, RedisplayStepArguments);
    check!(display_query, DisplayQuery, DisplayQueryStepArguments);
    check!(font, Font, FontStepArguments);
    check!(selection, Selection, SelectionStepArguments);
    check!(callback, Callback, CallbackStepArguments);
    check!(restore, Restore, RestoreStepArguments);
}

#[test]
fn all_four_flow_kinds_stay_registered_and_parent_bound() {
    let step = key(parent(1));
    macro_rules! check {
        ($variant:ident, $kind:ident) => {{
            let rejected = match claim(step).complete(OwnerStepResult::Flow(
                RegisteredOwnerFlow::$variant(reference(parent(2), 7)),
            )) {
                Err(rejected) => rejected,
                Ok(_) => panic!("foreign Flow must not consume owner port"),
            };
            let ((claimed, foreign), reason) = rejected.into_parts();
            assert_eq!(reason, OwnerStepRefusal::WrongRecord);
            assert!(foreign.matches(parent(2)));
            let completed = claimed
                .complete(OwnerStepResult::Flow(RegisteredOwnerFlow::$variant(
                    reference(step.parent(), 8),
                )))
                .unwrap_or_else(|_| panic!("matching registered Flow"));
            assert_eq!(completed.result_slot().kind(), RecordKind::$kind);
            let (_, result) = completed.into_parts();
            let OwnerStepResult::Flow(RegisteredOwnerFlow::$variant(record)) = result else {
                panic!("original Flow kind preserved")
            };
            assert_eq!(record.kind(), RecordKind::$kind);
            assert!(record.matches(step.parent()));
        }};
    }
    check!(Signal, OwnerSignalFlow);
    check!(Throw, OwnerThrowFlow);
    check!(ThreadBlocked, OwnerBlockedFlow);
    check!(Shutdown, OwnerShutdownFlow);
}

#[test]
fn sequence_never_wraps_or_reuses_the_final_nonce() {
    let mut sequence = OwnerStepSequence {
        state: SequenceState::Available(NonZeroU64::MAX),
    };
    assert_eq!(sequence.issue(), Ok(OwnerStepNonce(NonZeroU64::MAX)));
    assert_eq!(sequence.issue(), Err(OwnerStepIdentityExhausted));
    assert_eq!(sequence.issue(), Err(OwnerStepIdentityExhausted));
    let mut sequence = OwnerStepSequence::new();
    assert_ne!(sequence.issue().unwrap(), sequence.issue().unwrap());
}

#[test]
fn dropping_pending_metadata_does_not_cancel_or_retire_records() {
    // Dropping pending metadata is not a cancel. These refs are forged, so
    // the test does not prove a live GC root survives. It proves the same
    // record ids still construct a step, and that this file's owner has no
    // cancel and no Drop.
    let step = key(parent(1));
    let pending = callback(step);
    let OwnerFrameSchema::Callback {
        arguments,
        frame,
        restore,
    } = pending.frame_schema()
    else {
        panic!("callback schema");
    };
    let kept = (arguments.record(), frame.record(), restore.record());
    drop(pending);
    let again = callback(step);
    let OwnerFrameSchema::Callback {
        arguments,
        frame,
        restore,
    } = again.frame_schema()
    else {
        panic!("callback schema after drop");
    };
    assert_eq!((arguments.record(), frame.record(), restore.record()), kept);
    let source = include_str!("../owner_step.rs");
    assert!(
        !source.contains("fn cancel") && !source.contains("impl Drop"),
        "pending metadata has no cancel and no Drop that retires roots"
    );
}

#[test]
fn named_contract_negatives_stay_on_the_existing_witnesses() {
    // Locks the existing compile-fail witnesses. It does not re-run trybuild
    // and does not enable hosted admission.
    let unmapped =
        include_str!("../../../../neovm-core/tests/host_effect_ui/owner_step_result_unmapped.rs");
    let unmapped_err = include_str!(
        "../../../../neovm-core/tests/host_effect_ui/owner_step_result_unmapped.stderr"
    );
    assert!(!unmapped.contains("ContractInjected"));
    assert!(unmapped_err.contains("error[E0004]"));
    assert!(unmapped_err.contains("OwnerStepResult::ContractInjected"));

    let schema =
        include_str!("../../../../neovm-core/tests/host_effect_ui/owner_frame_schema_unmapped.rs");
    let schema_err = include_str!(
        "../../../../neovm-core/tests/host_effect_ui/owner_frame_schema_unmapped.stderr"
    );
    let consumer = schema
        .split("fn incomplete_schema_consumer")
        .nth(1)
        .expect("schema consumer");
    assert!(!consumer.contains("ContractInjected"));
    assert!(schema_err.contains("error[E0004]"));
    assert!(schema_err.contains("OwnerFrameSchema::ContractInjected"));

    let duplicate =
        include_str!("../../../../neovm-core/tests/host_effect_ui/owner_step_claim_consumed.rs");
    let duplicate_err = include_str!(
        "../../../../neovm-core/tests/host_effect_ui/owner_step_claim_consumed.stderr"
    );
    assert_eq!(duplicate.matches(".complete(").count(), 2);
    assert!(duplicate_err.contains("error[E0382]"));

    let port = include_str!("../../../../neovm-core/tests/host_effect_ui/port_used_twice.rs");
    let port_err =
        include_str!("../../../../neovm-core/tests/host_effect_ui/port_used_twice.stderr");
    assert_eq!(port.matches(".complete(").count(), 2);
    assert!(port_err.contains("error[E0382]"));

    let identity = include_str!(
        "../../../../neovm-core/tests/host_effect_ui/continuation_identity_private.rs"
    );
    let identity_err = include_str!(
        "../../../../neovm-core/tests/host_effect_ui/continuation_identity_private.stderr"
    );
    assert!(identity.contains("OwnerContinuationId("));
    assert!(identity_err.contains("error[E0423]"));
}
