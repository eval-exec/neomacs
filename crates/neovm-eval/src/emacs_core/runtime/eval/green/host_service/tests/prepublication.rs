//! Unpublished requests cannot retain resumable authority after a failed park.
use super::*;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use crate::host_effect::contracts::{CallbackStepArguments, OwnedNativeData};
use neovm_value::tagged::transport::collect_shared_root_gc_roots;
use neovm_value::value::Value;

fn prepare(
    registry: &mut HostRegistry,
    owner: &Context,
    baton: &Baton,
    roots: &[Value],
) -> PreparedHostCall {
    // SAFETY: fixture values belong to this original held-Baton owner; no
    // collection, callback or handoff occurs during retained publication.
    unsafe { registry.prepare(baton, &owner.tagged_heap, ThreadKey::Main, roots) }.unwrap()
}

#[test]
fn explicit_prepublication_abandonment_retires_only_its_own_registered_batches() {
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    let baton = owner.start_green_runtime().unwrap();
    let graph = Value::list(vec![Value::string("unpublished graph"), Value::fixnum(51)]);
    let sibling_graph = Value::string("sibling graph remains published");
    owner.prepare_parked_root_retirement().unwrap();
    owner.with_green_runtime(|runtime| {
        let mut parked = runtime.parked.borrow_mut();
        let registry = parked.hosts();
        let call = prepare(registry, &owner, &baton, &[graph]);
        let key = call.key();
        // SAFETY: graph remains live on the original owner throughout this
        // registration with no GC, callback or ownership handoff.
        let reference = unsafe {
            registry.register::<CallbackStepArguments>(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Main,
                key,
                &[Value::NIL, graph, Value::T],
            )
        }
        .unwrap();
        let sibling = prepare(registry, &owner, &baton, &[sibling_graph]);
        let sibling_key = sibling.key();
        let abandoned = registry
            .abandon_prepared(&baton, &owner.tagged_heap, ThreadKey::Main, call)
            .unwrap();
        registry
            .retire_abandoned(&baton, &owner.tagged_heap, ThreadKey::Main, abandoned)
            .unwrap();
        assert!(!registry.records.contains_key(&key));
        assert!(matches!(
            registry.records[&sibling_key].state,
            RecordState::Prepared
        ));
        let restored = RestoredHostOwner {
            context: &owner,
            baton: &baton,
            owner: ThreadKey::Main,
        };
        assert!(matches!(
            registry.with_registered_roots(&restored, key, &reference, |_| panic!(
                "removed root record materialized"
            )),
            Err(HostRootError::UnknownRequest)
        ));
        let mut roots = Vec::new();
        collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
        assert!(!roots.contains(&graph));
        assert!(roots.contains(&sibling_graph));
        let sibling = registry
            .abandon_prepared(&baton, &owner.tagged_heap, ThreadKey::Main, sibling)
            .unwrap();
        registry
            .retire_abandoned(&baton, &owner.tagged_heap, ThreadKey::Main, sibling)
            .unwrap();
        assert!(parked.is_empty());
    });
}

#[test]
fn abandoned_admission_is_inert_and_retained_until_quiescent_drain() {
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    let live = owner.world_lifetime_probe_for_test();
    let baton = owner.start_green_runtime().unwrap();
    let graph = Value::string("abandoned roots survive collection without materialization");
    let key = owner.with_green_runtime(|runtime| {
        let mut parked = runtime.parked.borrow_mut();
        let registry = parked.hosts();
        let call = prepare(registry, &owner, &baton, &[graph]);
        let key = call.key();
        // SAFETY: original same-heap live value remains held by this owner;
        // registration performs no GC, callback or ownership handoff.
        let reference = unsafe {
            registry.register::<CallbackStepArguments>(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Main,
                key,
                &[graph],
            )
        }
        .unwrap();
        let abandoned = registry
            .abandon_prepared(&baton, &owner.tagged_heap, ThreadKey::Main, call)
            .unwrap();
        drop(abandoned); // No collector proof: retain every lease in the World.
        assert!(matches!(
            registry.records[&key].state,
            RecordState::Abandoned
        ));
        assert!(matches!(
            registry.registration(key),
            Err(HostRootError::WrongStage)
        ));
        let restored = RestoredHostOwner {
            context: &owner,
            baton: &baton,
            owner: ThreadKey::Main,
        };
        assert!(matches!(
            registry.with_registered_roots(&restored, key, &reference, |_| panic!(
                "abandoned root record materialized"
            )),
            Err(HostRootError::WrongStage)
        ));
        key
    });
    owner.gc_collect_exact();
    assert_eq!(
        print_value(&graph),
        "\"abandoned roots survive collection without materialization\""
    );
    owner.with_green_runtime(|runtime| {
        assert!(matches!(
            runtime.parked.borrow_mut().hosts().records[&key].state,
            RecordState::Abandoned
        ))
    });
    drop(baton);
    drop(owner);
    assert!(!live(), "World drain closes abandoned root retention");
}

#[test]
fn foreign_owner_cannot_abandon_an_unpublished_request() {
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    let baton = owner.start_green_runtime().unwrap();
    owner.with_green_runtime(|runtime| {
        let mut parked = runtime.parked.borrow_mut();
        let registry = parked.hosts();
        let call = prepare(
            registry,
            &owner,
            &baton,
            &[Value::string("original owner only")],
        );
        let key = call.key();
        assert!(matches!(
            registry.abandon_prepared(
                &baton,
                &owner.tagged_heap,
                ThreadKey::Worker(NonZeroU64::MIN),
                call
            ),
            Err(HostRootError::ForeignOwner)
        ));
        assert!(matches!(
            registry.records[&key].state,
            RecordState::Prepared
        ));
        assert_eq!(registry.records[&key].roots.len(), 1);
    });
}

#[test]
fn refused_park_abandons_initial_request_before_any_native_or_owner_execution() {
    let mut main = Context::new_with_threading(ThreadingMode::Green);
    main.start_green_threads().unwrap();
    let original_os = std::thread::current().id();
    let original_wait = main.with_green_runtime(|runtime| runtime.control.wait_of(ThreadKey::Main));
    main.with_gc_inhibited(|owner| {
        let graph = Value::list(vec![
            Value::string("prepublication refusal"),
            Value::fixnum(67),
        ]);
        // SAFETY: this original main owner retains graph under the held Baton;
        // admission has no collection, callback or ownership handoff.
        let call = unsafe { owner.green_prepare_host_call(&[graph]) }.unwrap();
        let failure = owner
            .green_host_call::<()>(call, OwnedNativeData::None, |_, _| {
                panic!("unpublished request reached terminal decoder")
            })
            .unwrap_err();
        let signal = failure.as_signal().unwrap();
        assert_eq!(signal.symbol_name(), "error");
        assert_eq!(signal.data.len(), 1);
        assert_eq!(
            print_value(&signal.data[0]),
            "\"Cannot switch threads while garbage collection is inhibited\""
        );
        assert_eq!(std::thread::current().id(), original_os);
        assert_eq!(owner.green_key(), Some(ThreadKey::Main));
        owner.with_green_runtime(|runtime| {
            assert_eq!(runtime.control.wait_of(ThreadKey::Main), original_wait);
            assert!(
                runtime.parked.borrow().is_empty(),
                "unused retained roots were explicitly retired before returning refusal"
            );
        });
    });
    main.gc_collect_exact();
    assert_eq!(
        print_value(
            &main
                .eval_str("(thread-join (make-thread (lambda () 71)))")
                .unwrap()
        ),
        "71"
    );
}
