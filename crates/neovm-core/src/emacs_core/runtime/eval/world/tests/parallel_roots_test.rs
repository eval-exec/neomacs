//! Admission/root-source negatives beside the sealed capture constructor.
//! No production factory permits the direct capture access used by this child.

use super::*;
use crate::emacs_core::eval::{
    ThreadingMode, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{HashTableTest, HashTableWeakness, LispHashTable};
use crate::tagged::gc::{ProofEra as ParallelHeapEra, RegistrationError};

struct ScratchScope(usize);

#[test]
fn p3sym_c4_cold_function_admission_refuses_live_legacy_roots() {
    use crate::emacs_core::eval::SpecBinding;
    let mut context = Context::new_minimal_vm_harness();
    assert!(context.function_cells_quiescent());
    context.jit_root_stack_top = 1;
    let rooted_native = context.function_cells_quiescent();
    context.jit_root_stack_top = 0;
    assert!(!rooted_native);
    let depth = context.specpdl.len();
    context.specpdl.push(SpecBinding::Backtrace1 {
        debug_on_exit: false,
        function: TaggedValue::NIL,
        arg: TaggedValue::NIL,
    });
    let legacy_call = context.function_cells_quiescent();
    context.specpdl.truncate(depth);
    assert!(!legacy_call);
    assert!(context.function_cells_quiescent());
}
impl Drop for ScratchScope {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

#[test]
fn frozen_legacy_capture_rejects_a_foreign_world_keeper() {
    let mut first = Context::new();
    let first_keeper = first.issue_green_baton().unwrap();
    let mut second = Context::new();
    let second_keeper = second.issue_green_baton().unwrap();
    assert_ne!(
        first_keeper.parallel_heap_identity(),
        second_keeper.parallel_heap_identity()
    );
    assert!(matches!(
        first.capture_parallel_legacy_roots(&second_keeper),
        Err(RendezvousError::Identity)
    ));
    assert!(matches!(
        second.capture_parallel_legacy_roots(&first_keeper),
        Err(RendezvousError::Identity)
    ));
    assert!(first_keeper.held_here());
    assert!(second_keeper.held_here());
    drop(second_keeper);
    drop(first_keeper);
}

#[test]
fn rooted_admission_rejects_real_capture_from_another_heap_and_returns_capabilities() {
    let mut first = Context::new();
    first.setup_thread_locals();
    let first_keeper = first.issue_green_baton().unwrap();
    let frozen = first.capture_parallel_legacy_roots(&first_keeper).unwrap();
    let first_heap = frozen.heap_identity();
    // Keep the source Context and keeper alive without running it between
    // capture and the deliberate foreign admission attempt.
    let mut second = Context::new();
    second.setup_thread_locals();
    let keeper = second.issue_green_baton().unwrap();
    let second_heap = keeper.parallel_heap_identity();
    assert_ne!(first_heap, second_heap);
    let heap: *mut crate::tagged::gc::TaggedHeap = second.tagged_heap.heap_root().as_ptr();
    // SAFETY: this fresh Context is the sole serial owner; enumeration only
    // seeds its roots, and all capture/identity reads precede the heap loan.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            second.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .unwrap();
    let failure = ParallelHeapEra::admit_rooted(collector, keeper, frozen).unwrap_err();
    assert_eq!(failure.reason, RegistrationError::WrongHeap);
    assert_eq!(failure.collector.heap_identity(), second_heap);
    assert_eq!(failure.keeper.parallel_heap_identity(), second_heap);
    assert!(failure.keeper.held_here());
    drop(failure.collector);
    drop(failure.keeper);
    drop(first_keeper);
}

fn unsupported_fixture_refuses_before_registration(make: impl FnOnce(&mut Context) -> TaggedValue) {
    // Clean the same known unsupported GNU default BEFORE the injected
    // fixture object, so weak/native/finalizer negatives are discriminating.
    let mut context = collection_fixture_context();
    assert_fixture_refuses_before_registration(&mut context, make);
}

fn assert_fixture_refuses_before_registration(
    context: &mut Context,
    make: impl FnOnce(&mut Context) -> TaggedValue,
) {
    context.setup_thread_locals();
    let unsupported = make(context);
    // Retain the unsupported object across both serial admission drains so a
    // preflight cannot pass merely because setup garbage was already swept.
    context.lexenv = unsupported;
    let keeper = context.issue_green_baton().unwrap();
    let expected = keeper.parallel_heap_identity();
    let frozen = context.capture_parallel_legacy_roots(&keeper).unwrap();
    let heap: *mut crate::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context, no callback/manager loan, private capture
    // completed before this heap loan, and keeper excludes concurrent mark.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            context.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .unwrap();
    let failure = ParallelHeapEra::admit_rooted(collector, keeper, frozen).unwrap_err();
    assert_eq!(failure.reason, RegistrationError::Publication);
    assert_eq!(failure.collector.heap_identity(), expected);
    assert_eq!(failure.keeper.parallel_heap_identity(), expected);
    assert!(failure.keeper.held_here());
    // No era was admitted, so no registration/activation could precede this
    // refusal. Both input capabilities return intact for serial recovery.
    drop(failure.collector);
    drop(failure.keeper);
    assert!(context.tagged_heap.owns_heap_value_for_test(unsupported));
    context.lexenv = TaggedValue::NIL;
}

#[test]
fn a5_admission_refuses_weak_hash_fixture_before_any_mutator_activation() {
    unsupported_fixture_refuses_before_registration(|context| {
        context
            .tagged_heap
            .alloc_hash_table(LispHashTable::new_with_options(
                HashTableTest::Eq,
                4,
                Some(HashTableWeakness::Key),
                1.5,
                0.8,
            ))
    });
}

#[test]
fn a5_admission_refuses_native_resource_fixture_even_without_a_finalizer() {
    unsupported_fixture_refuses_before_registration(|context| {
        context
            .tagged_heap
            .alloc_user_ptr(std::ptr::null_mut(), None)
    });
}

#[test]
fn a5_admission_refuses_lisp_finalizer_fixture_before_any_mutator_activation() {
    unsupported_fixture_refuses_before_registration(|context| {
        context.tagged_heap.alloc_finalizer(TaggedValue::T)
    });
}

#[test]
fn a5_capture_refuses_a_preexisting_green_runtime() {
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    let keeper = context.start_green_runtime().unwrap();
    assert!(keeper.held_here());
    assert!(matches!(
        context.capture_parallel_legacy_roots(&keeper),
        Err(RendezvousError::Publication)
    ));
    drop(keeper);
}

#[test]
fn frozen_legacy_capture_includes_own_slab_and_heap_matched_parked_tls_roots() {
    let mut context = Context::new();
    context.setup_thread_locals();
    context
        .eval_str("(defvar p1rv-a5-capture-binding nil)")
        .unwrap();
    let symbol = intern("p1rv-a5-capture-binding");
    context.project_thread_variable_for_test(symbol).unwrap();
    let binding_base = context.specpdl.len();
    let binding = context
        .tagged_heap
        .alloc_cons(TaggedValue::fixnum(5101), TaggedValue::NIL);
    context.try_specbind(symbol, binding).unwrap();
    let scratch = context
        .tagged_heap
        .alloc_cons(TaggedValue::fixnum(5102), TaggedValue::NIL);
    let scratch_scope = ScratchScope(save_scratch_gc_roots());
    push_scratch_gc_roots(&[scratch]);
    let mut private = Vec::new();
    context.trace_private_roots(&mut private);
    assert!(
        !private.contains(&binding),
        "current projected binding must test the separate own-slab source"
    );
    assert!(
        !private.contains(&scratch),
        "scratch must test the separate Parked TLS source"
    );
    let keeper = context.issue_green_baton().unwrap();
    let frozen = context.capture_parallel_legacy_roots(&keeper).unwrap();
    assert_eq!(frozen.heap_identity(), keeper.parallel_heap_identity());
    let mut words = Vec::new();
    frozen.append_words(&mut words);
    assert!(words.contains(&binding));
    assert!(words.contains(&scratch));
    // This is a source-census test, not an admitted era. Destroy its capture
    // before allowing any further Context execution or root mutation.
    drop(frozen);
    drop(keeper);
    drop(scratch_scope);
    context.unbind_to(binding_base);
}

#[cfg(feature = "gc-memory-telemetry")]
#[test]
fn a5_admission_refuses_telemetry_without_unsupported_heap_objects() {
    // A plain cons is supported in ordinary GEN0/GEN1 builds. With telemetry
    // enabled, A5 cannot account for the owner-private Rust buffer inventory,
    // so admission itself must refuse and preserve both serial capabilities.
    unsupported_fixture_refuses_before_registration(|context| {
        context
            .tagged_heap
            .alloc_cons(TaggedValue::fixnum(5103), TaggedValue::NIL)
    });
}

#[test]
fn a5_admission_refuses_full_context_with_gnu_default_weak_table() {
    // Ordinary Context::new correctly includes GNU's weak-value
    // window-dead-windows-table. It remains outside A5's minimal fixture.
    let mut context = Context::new();
    assert_fixture_refuses_before_registration(&mut context, |context| {
        context
            .tagged_heap
            .alloc_cons(TaggedValue::fixnum(5104), TaggedValue::NIL)
    });
}

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn a6_owned_admission_pins_shell_census_and_restores_empty_weak_table() {
    let mut context = Context::new();
    let shell = context
        .tagged_heap
        .alloc_cons(TaggedValue::fixnum(11), TaggedValue::fixnum(13));
    context.lexenv = shell;
    let shell_bits = shell.bits();
    context
        .admit_owned_proof_parallel(|projection| {
            assert!(
                crate::tagged::gc::current_tagged_heap_identity().is_none(),
                "overlap must not install the legacy heap TLS facade"
            );
            {
                let era = projection.era();
                let mut stopped = era.stop().expect("empty membership stops");
                assert!(era.collect_stopped(&mut stopped).unwrap().major);
                era.with_stopped_heap_for_test(&stopped, |heap| {
                    let shell = TaggedValue::from_bits(shell_bits);
                    assert!(heap.owns_heap_value_for_test(shell));
                    assert_eq!(shell.cons_car_unobserved().as_fixnum(), Some(11));
                    assert_eq!(shell.cons_cdr_unobserved().as_fixnum(), Some(13));
                })
                .unwrap();
                stopped.complete().unwrap();
                assert!(
                    crate::tagged::gc::current_tagged_heap_identity().is_none(),
                    "stopped collection must not install the legacy heap TLS facade"
                );
            }
            let (collector, keeper) = projection.finish().unwrap();
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("owned admission: {error:?}"));
    assert_eq!(context.lexenv.bits(), shell_bits);
    assert_eq!(context.lexenv.cons_car_unobserved().as_fixnum(), Some(11));
    let table = context.eval_str("window-dead-windows-table").unwrap();
    let hash = table
        .as_hash_table()
        .expect("serial recovery installs a fresh empty weak table");
    assert_eq!(hash.test, HashTableTest::Eql);
    assert_eq!(hash.weakness, Some(HashTableWeakness::Value));
    assert_eq!(hash.data.len(), 0);
    assert!(
        hash.data
            .pending_entries()
            .is_none_or(|entries| entries.is_empty())
    );
}

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn a6_owned_admission_refuses_nonempty_weak_table_without_clearing_it() {
    let mut context = Context::new();
    context
        .eval_str("(puthash 1 2 window-dead-windows-table)")
        .unwrap();
    let started = std::sync::atomic::AtomicBool::new(false);
    let refused: Result<(), OwnedParallelError> = context.admit_owned_proof_parallel(|_| {
        started.store(true, std::sync::atomic::Ordering::Relaxed);
        unreachable!("non-empty weak table must refuse before the body");
    });
    let error = refused.expect_err("non-empty GNU weak table stays outside admission");
    assert_eq!(error, OwnedParallelError::Publication);
    assert!(!started.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(
        context
            .eval_str("(gethash 1 window-dead-windows-table)")
            .unwrap()
            .as_fixnum(),
        Some(2)
    );
    assert!(
        !context
            .eval_str("window-dead-windows-table")
            .unwrap()
            .is_nil()
    );
}

#[test]
fn heapborrow_r9_collection_fixture_clears_and_restores_serial_tls() {
    Context::with_test_collection_era_then(
        |_| (),
        |era, ()| {
            assert!(!crate::tagged::gc::tagged_heap_is_installed());
            let (collector, keeper) = era.finish().unwrap_or_else(|_| panic!("fixture finish"));
            (collector, keeper, ())
        },
        |context, ()| {
            assert!(crate::tagged::gc::tagged_heap_is_current(
                &context.tagged_heap
            ));
            let value = TaggedValue::cons(TaggedValue::fixnum(53), TaggedValue::NIL);
            assert_eq!(value.cons_car(), TaggedValue::fixnum(53));
        },
    );
}
