//! Real-World root retention controls; transport-only controls live below.

use super::*;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;
use crate::tagged::transport::collect_shared_root_gc_roots;

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new_with_threading(ThreadingMode::Green)
}

fn prepare(
    registry: &mut HostRegistry,
    baton: &Baton,
    heap: &TaggedHeap,
    owner: ThreadKey,
    roots: &[Value],
) -> PreparedHostCall {
    // SAFETY: each fixture passes the original owner's live same-heap roots;
    // no collection or handoff occurs in the registration call.
    unsafe { registry.prepare(baton, heap, owner, roots) }.unwrap()
}

// Unit fixtures exercise registry transitions without seating the scheduler.
// Actual restore provenance is checked by after_restore and switch integration.
fn restored<'a>(owner: &'a Context, baton: &'a Baton, key: ThreadKey) -> RestoredHostOwner<'a> {
    RestoredHostOwner {
        context: owner,
        baton,
        owner: key,
    }
}

fn published(baton: &Baton, heap: &TaggedHeap, roots: &[Value]) -> RootBatchLease {
    let prepared = baton.prepare_parked_batch(heap, roots.len()).unwrap();
    // SAFETY: this test's original same-heap owner holds its unique Baton;
    // no GC, callback or handoff occurs while publishing the live root slice.
    unsafe { prepared.finalize(roots) }
        .unwrap()
        .publish()
        .unwrap()
}

#[test]
fn host_only_roots_are_visible_to_collection_and_world_drop() {
    let mut owner = green();
    let live = owner.world_lifetime_probe_for_test();
    let baton = owner.start_green_runtime().unwrap();
    let value = Value::string("host record is its only root");
    let key = owner.with_green_runtime(|runtime| {
        let call = prepare(
            runtime.parked.borrow_mut().hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[value],
        );
        let key = call.key();
        drop(call);
        assert!(
            !runtime.parked.borrow().is_empty(),
            "World-drop guard sees host-only roots"
        );
        key
    });
    owner.eval_str("(garbage-collect)").unwrap();
    assert_eq!(print_value(&value), "\"host record is its only root\"");
    owner.with_green_runtime(|runtime| {
        assert!(
            runtime
                .parked
                .borrow_mut()
                .hosts()
                .registration(key)
                .is_err()
        );
    });
    drop(baton);
    drop(owner);
    assert!(!live(), "World Drop drained host-only root publications");
}

#[test]
fn ready_requires_the_ordinary_parked_owner_publication() {
    let mut owner = green();
    let baton = owner.start_green_runtime().unwrap();
    owner.with_green_runtime(|runtime| {
        let mut registry = runtime.parked.borrow_mut();
        let call = prepare(
            registry.hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[],
        );
        assert!(matches!(
            registry.publish_host(call),
            Err(HostRootError::OwnerNotParked)
        ));
        assert!(!registry.is_empty());
    });
    drop(baton);
    drop(owner);
}

#[test]
fn terminal_and_owner_acknowledgement_are_distinct_from_root_retirement() {
    let mut owner = green();
    let baton = owner.start_green_runtime().unwrap();
    let value = Value::string("retire only after restored-owner acknowledgement");
    owner.with_green_runtime(|runtime| {
        let mut registry = runtime.parked.borrow_mut();
        let call = prepare(
            registry.hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[value],
        );
        registry.insert(ThreadKey::Main, published(&baton, &owner.tagged_heap, &[]));
        let key = registry.publish_host(call).unwrap();
        assert_eq!(registry.hosts().registration(key).unwrap().key(), key);
        assert!(matches!(
            registry
                .hosts()
                .acknowledge(&restored(&owner, &baton, ThreadKey::Main), key),
            Err(HostRootError::WrongStage)
        ));
        drop(registry.hosts().settle(key).unwrap());
        let mut roots = Vec::new();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(roots.contains(&value), "terminal settlement keeps roots");
        let wrong = ThreadKey::Worker(NonZeroU64::MIN);
        assert!(matches!(
            registry
                .hosts()
                .acknowledge(&restored(&owner, &baton, wrong), key),
            Err(HostRootError::ForeignOwner)
        ));
        let restored_owner = restored(&owner, &baton, ThreadKey::Main);
        let ack = registry.hosts().acknowledge(&restored_owner, key).unwrap();
        roots.clear();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(roots.contains(&value), "acknowledgement still keeps roots");
        let receipt = registry.hosts().retire(ack).unwrap();
        assert_eq!(receipt.key(), key);
        assert!(matches!(
            registry.hosts().settle(key),
            Err(HostRootError::UnknownRequest)
        ));
        super::super::root_registry::finish(registry.remove(&ThreadKey::Main).unwrap()).unwrap();
        assert!(registry.is_empty());
    });
    drop(baton);
    drop(owner);
}

#[test]
fn drain_blocked_retains_allocation_without_granting_execution_authority() {
    let owner = green();
    let live = owner.world_lifetime_probe_for_test();
    let blocked = DrainBlocked::new(owner.obarray.closing_lease(), HostRootError::WrongStage);
    drop(owner);
    assert!(live());
    assert!(matches!(blocked.error(), HostRootError::WrongStage));
    drop(blocked);
    assert!(!live());
}

#[test]
fn different_world_sessions_never_share_a_request_key() {
    let mut first = green();
    let first_baton = first.start_green_runtime().unwrap();
    let first_key = first.with_green_runtime(|runtime| {
        prepare(
            runtime.parked.borrow_mut().hosts(),
            &first_baton,
            &first.tagged_heap,
            ThreadKey::Main,
            &[],
        )
        .key()
    });
    drop(first_baton);
    drop(first);
    let mut second = green();
    let second_baton = second.start_green_runtime().unwrap();
    let second_key = second.with_green_runtime(|runtime| {
        prepare(
            runtime.parked.borrow_mut().hosts(),
            &second_baton,
            &second.tagged_heap,
            ThreadKey::Main,
            &[],
        )
        .key()
    });
    assert_ne!(first_key, second_key);
    drop(second_baton);
    drop(second);
}

fn terminal_root_case<K: RegisteredRecord>(
    terminal: impl FnOnce(
        RegisteredRootRef<'static, K>,
    ) -> crate::host_effect::contracts::HostResult<
        'static,
        crate::host_effect::contracts::MessageReply<'static>,
    >,
) {
    use crate::host_effect::contracts::{MessageArguments, MessagePlan, OwnedNativeData};
    let mut owner = green();
    let baton = owner.start_green_runtime().unwrap();
    let argument = Value::string("arguments live until owner acknowledgement");
    let result = Value::string("terminal result/error/signal roots stay live");
    let key = owner.with_green_runtime(|runtime| {
        let mut registry = runtime.parked.borrow_mut();
        let call = prepare(
            registry.hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[],
        );
        let key = call.key();
        // SAFETY: these same-heap original-owner values remain live through
        // immediate publication, with no callback, GC or Baton handoff.
        let arguments = unsafe {
            registry.hosts().register::<MessageArguments>(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Main,
                key,
                &[argument],
            )
        }
        .unwrap();
        // SAFETY: same original-owner publication contract as the arguments.
        let output = unsafe {
            registry.hosts().register::<K>(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Main,
                key,
                &[result],
            )
        }
        .unwrap();
        let restored = restored(&owner, &baton, ThreadKey::Main);
        registry
            .hosts()
            .with_registered_roots(&restored, key, &output, |roots| {
                assert_eq!(roots, &[result])
            })
            .unwrap();
        let (_, port) = MessagePlan::from_registered(OwnedNativeData::None, arguments)
            .submit()
            .into_parts();
        registry.insert(ThreadKey::Main, published(&baton, &owner.tagged_heap, &[]));
        registry.publish_host(call).unwrap();
        let response = port.complete(terminal(output)).unwrap();
        drop(response);
        drop(registry.hosts().settle(key).unwrap());
        key
    });
    owner.eval_str("(garbage-collect)").unwrap();
    assert_eq!(
        print_value(&result),
        "\"terminal result/error/signal roots stay live\""
    );
    owner.with_green_runtime(|runtime| {
        let mut registry = runtime.parked.borrow_mut();
        let mut roots = Vec::new();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(roots.contains(&argument));
        assert!(
            roots.contains(&result),
            "consuming native response cannot release Lisp roots"
        );
        let restored_owner = restored(&owner, &baton, ThreadKey::Main);
        let ack = registry.hosts().acknowledge(&restored_owner, key).unwrap();
        registry.hosts().retire(ack).unwrap();
        super::super::root_registry::finish(registry.remove(&ThreadKey::Main).unwrap()).unwrap();
        roots.clear();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(!roots.contains(&argument));
        assert!(!roots.contains(&result));
    });
    drop(baton);
    drop(owner);
}

#[test]
fn registered_returned_values_are_world_rooted_until_owner_ack() {
    use crate::host_effect::contracts::{HostResult, MessageReply, MessageResult, OwnedNativeData};
    terminal_root_case::<MessageResult>(|reference| {
        HostResult::Returned(MessageReply::from_registered(
            OwnedNativeData::None,
            reference,
        ))
    });
}

#[test]
fn registered_lisp_errors_are_world_rooted_until_owner_ack() {
    use crate::host_effect::contracts::{ConditionRecord, HostResult, RootedCondition};
    terminal_root_case::<ConditionRecord>(|reference| {
        HostResult::LispError(RootedCondition::from_registered(reference))
    });
}

#[test]
fn registered_thread_signals_are_world_rooted_until_owner_ack() {
    use crate::host_effect::contracts::{HostResult, RootedThreadSignal, ThreadSignalRecord};
    terminal_root_case::<ThreadSignalRecord>(|reference| {
        HostResult::Interrupted(RootedThreadSignal::from_registered(reference))
    });
}

#[test]
fn foreign_record_and_heap_refusals_preserve_registered_roots() {
    use crate::host_effect::contracts::MessageArguments;
    let mut owner = green();
    let baton = owner.start_green_runtime().unwrap();
    let value = Value::string("foreign refusals keep this root");
    owner.with_green_runtime(|runtime| {
        let mut registry = runtime.parked.borrow_mut();
        let first = prepare(
            registry.hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[],
        );
        let second = prepare(
            registry.hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[],
        );
        // SAFETY: the local value stays live through same-owner publication.
        let reference = unsafe {
            registry.hosts().register::<MessageArguments>(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Main,
                first.key(),
                &[value],
            )
        }
        .unwrap();
        assert!(matches!(
            registry.hosts().with_registered_roots(
                &restored(&owner, &baton, ThreadKey::Main),
                second.key(),
                &reference,
                |_| ()
            ),
            Err(HostRootError::ForeignRecord)
        ));
        let other_heap = TaggedHeap::new();
        // SAFETY: empty roots need no object provenance; foreign heap refusal
        // happens before any reservation/publication is attempted.
        assert!(matches!(
            unsafe {
                registry.hosts().register::<MessageArguments>(
                    &baton,
                    &other_heap,
                    ThreadKey::Main,
                    first.key(),
                    &[],
                )
            },
            Err(HostRootError::ForeignHeap { .. })
        ));
        let mut roots = Vec::new();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(roots.contains(&value));
        registry
            .hosts()
            .with_registered_roots(
                &restored(&owner, &baton, ThreadKey::Main),
                first.key(),
                &reference,
                |roots| assert_eq!(roots, &[value]),
            )
            .unwrap();
    });
    drop(baton);
    drop(owner);
}

#[test]
fn retirement_failure_preserves_acknowledgement_and_root_ownership() {
    let mut owner = green();
    let baton = owner.start_green_runtime().unwrap();
    let value = Value::string("retry keeps the acknowledgement and its roots");
    owner.with_green_runtime(|runtime| {
        let mut registry = runtime.parked.borrow_mut();
        let call = prepare(
            registry.hosts(),
            &baton,
            &owner.tagged_heap,
            ThreadKey::Main,
            &[value],
        );
        registry.insert(ThreadKey::Main, published(&baton, &owner.tagged_heap, &[]));
        let key = registry.publish_host(call).unwrap();
        drop(registry.hosts().settle(key).unwrap());
        let restored_owner = restored(&owner, &baton, ThreadKey::Main);
        let ack = registry.hosts().acknowledge(&restored_owner, key).unwrap();
        // Negative control: deliberately violate the stage to force the
        // fallible retirement boundary, then retry its returned linear ack.
        registry.hosts().record_mut(key).unwrap().state = RecordState::TerminalAwaitingOwner;
        let (ack, error) = registry.hosts().retire(ack).unwrap_err().into_parts();
        assert!(matches!(error, HostRootError::WrongStage));
        let mut roots = Vec::new();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(roots.contains(&value));
        registry.hosts().record_mut(key).unwrap().state = RecordState::OwnerAcknowledged;
        assert_eq!(registry.hosts().retire(ack).unwrap().key(), key);
        super::super::root_registry::finish(registry.remove(&ThreadKey::Main).unwrap()).unwrap();
    });
    drop(baton);
    drop(owner);
}

#[test]
fn an_unseated_context_cannot_mint_restored_owner_authority() {
    let mut owner = green();
    let baton = owner.start_green_runtime().unwrap();
    assert!(matches!(
        RestoredHostOwner::after_restore(&mut owner, &baton),
        Err(HostRootError::ForeignOwner)
    ));
    drop(baton);
    drop(owner);
}
