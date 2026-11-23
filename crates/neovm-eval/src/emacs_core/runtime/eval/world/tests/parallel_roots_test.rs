//! Admission/root-source negatives beside the sealed capture constructor.
//! No production factory permits the direct capture access used by this child.

use super::*;
use crate::emacs_core::eval::world::ParallelHeapEraWorldCollectExt;
use crate::emacs_core::eval::world::WorldProofEra as ParallelHeapEra;
use crate::emacs_core::eval::{
    ThreadingMode, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use neovm_value::intern::intern;
use neovm_value::tagged::gc::RegistrationError;
use neovm_value::value::{HashTableTest, HashTableWeakness, LispHashTable};

struct ScratchScope(usize);

#[test]
fn p3sym_c4_cold_function_admission_refuses_live_legacy_roots() {
    use crate::emacs_core::eval::SpecBinding;
    let mut context = Context::new_minimal_vm_harness(crate::test_utils::unit_runtime());
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
    let mut first = Context::new(crate::test_utils::unit_runtime());
    let first_keeper = first.issue_green_baton().unwrap();
    let mut second = Context::new(crate::test_utils::unit_runtime());
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
    let mut first = Context::new(crate::test_utils::unit_runtime());
    first.setup_thread_locals();
    let first_keeper = first.issue_green_baton().unwrap();
    let frozen = first.capture_parallel_legacy_roots(&first_keeper).unwrap();
    let first_heap = frozen.heap_identity();
    // Keep the source Context and keeper alive without running it between
    // capture and the deliberate foreign admission attempt.
    let mut second = Context::new(crate::test_utils::unit_runtime());
    second.setup_thread_locals();
    let keeper = second.issue_green_baton().unwrap();
    let second_heap = keeper.parallel_heap_identity();
    assert_ne!(first_heap, second_heap);
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = second.tagged_heap.heap_root().as_ptr();
    // SAFETY: this fresh Context is the sole serial owner; enumeration only
    // seeds its roots, and all capture/identity reads precede the heap loan.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            second.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .unwrap();
    let failure = ParallelHeapEra::admit_rooted(collector, keeper, frozen).unwrap_err();
    assert_eq!(*failure.reason(), RegistrationError::WrongHeap);
    let (_reason, collector, keeper) = failure.into_parts();
    assert_eq!(collector.heap_identity(), second_heap);
    assert_eq!(keeper.parallel_heap_identity(), second_heap);
    assert!(keeper.held_here());
    drop(collector);
    drop(keeper);
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
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context, no callback/manager loan, private capture
    // completed before this heap loan, and keeper excludes concurrent mark.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            context.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .unwrap();
    let failure = ParallelHeapEra::admit_rooted(collector, keeper, frozen).unwrap_err();
    assert_eq!(*failure.reason(), RegistrationError::Publication);
    let (_reason, collector, keeper) = failure.into_parts();
    assert_eq!(collector.heap_identity(), expected);
    assert_eq!(keeper.parallel_heap_identity(), expected);
    assert!(keeper.held_here());
    // No era was admitted, so no registration/activation could precede this
    // refusal. Both input capabilities return intact for serial recovery.
    drop(collector);
    drop(keeper);
    assert!(context.tagged_heap.owns_heap_value_for_test(unsupported));
    context.lexenv = TaggedValue::NIL;
}

/// E3 slice 1 removed the blanket weak-table/finalizer refusal: an eligible
/// fixture now admits at the rooted seam, before any mutator activation. The
/// era finishes without a collection, and the rooted fixture survives it.
fn assert_fixture_admits_before_registration(
    context: &mut Context,
    make: impl FnOnce(&mut Context) -> TaggedValue,
) -> TaggedValue {
    context.setup_thread_locals();
    let eligible = make(context);
    // Retain the fixture across the admission, as the refusal twin does, so
    // the admitted era sees the live object, not setup garbage.
    context.lexenv = eligible;
    let keeper = context.issue_green_baton().unwrap();
    let expected = keeper.parallel_heap_identity();
    let frozen = context.capture_parallel_legacy_roots(&keeper).unwrap();
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context, no callback/manager loan, private capture
    // completed before this heap loan, and keeper excludes concurrent mark.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            context.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .unwrap();
    let era = ParallelHeapEra::admit_rooted(collector, keeper, frozen)
        .expect("an eligible fixture admits before registration");
    let (collector, keeper) = era.finish().expect("admitted rooted era finishes");
    assert_eq!(collector.heap_identity(), expected);
    assert_eq!(keeper.parallel_heap_identity(), expected);
    drop(collector);
    drop(keeper);
    assert!(context.tagged_heap.owns_heap_value_for_test(eligible));
    context.lexenv = TaggedValue::NIL;
    eligible
}

#[test]
fn a5_admission_admits_a_weak_hash_fixture_before_any_mutator_activation() {
    // The ineligible classes stay refused: permanent/mapped shells, pending
    // entries, active test depth, and storage authority (weak_policy tests).
    let mut context = collection_fixture_context();
    let table = assert_fixture_admits_before_registration(&mut context, |context| {
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
    assert_eq!(
        table.as_hash_table().expect("hash table fixture").weakness,
        Some(HashTableWeakness::Key)
    );
}

/// E3.2: a native payload `supported` admits is swept by a real stopped
/// major. The fixture stays out of `lexenv`: capture freezes private roots,
/// so a lexenv root would keep the object live for the whole era and the
/// sweep would never detach it. Admission still sees it, because the
/// preflight walks the object catalog.
///
/// The batch is read after `collect_stopped` returns and before
/// `stopped.complete()`. The drain runs inside `collect_stopped`, while
/// AllStopped is still held. A drain moved to era finish would leave a
/// record here.
fn assert_unreachable_native_fixture_is_admitted_and_drained(
    context: &mut Context,
    make: impl FnOnce(&mut Context) -> TaggedValue,
) {
    context.setup_thread_locals();
    let fixture = make(context);
    assert!(
        context.tagged_heap.owns_heap_value_for_test(fixture),
        "setup retains the native fixture"
    );
    let keeper = context.issue_green_baton().unwrap();
    let expected = keeper.parallel_heap_identity();
    let frozen = context.capture_parallel_legacy_roots(&keeper).unwrap();
    assert!(
        context.tagged_heap.owns_heap_value_for_test(fixture),
        "the fixture is still live at admission, so the stopped sweep is what reclaims it"
    );
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context, no callback/manager loan, private capture
    // completed before this heap loan, and keeper excludes concurrent mark.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            context.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .unwrap();
    let era = ParallelHeapEra::admit_rooted(collector, keeper, frozen)
        .expect("an admitted native fixture reaches the era");
    let mut stopped = era.stop().expect("stopped world");
    era.collect_stopped(
        &mut stopped,
        crate::test_support::authority::baton_collect_parallel_world_roots,
    )
    .expect("stopped major admits and sweeps the native fixture");
    let (batch, retained) = era
        .with_stopped_heap_for_test(&stopped, |heap| {
            (
                heap.pending_native_destroy_count_for_test(),
                heap.owns_heap_value_for_test(fixture),
            )
        })
        .expect("observe the pending-native batch before the stop completes");
    assert_eq!(
        batch, None,
        "collect_stopped drains the detached fixture before the stop completes"
    );
    assert!(
        !retained,
        "the unreachable native fixture is detached by the stopped sweep"
    );
    stopped.complete().expect("stop completion");
    let (collector, keeper) = era.finish().expect("admitted rooted era finishes");
    assert_eq!(collector.heap_identity(), expected);
    assert_eq!(keeper.parallel_heap_identity(), expected);
    drop(collector);
    drop(keeper);
    assert_eq!(
        context.tagged_heap.pending_native_destroy_count_for_test(),
        None,
        "serial recovery does not resurrect the drained batch"
    );
}

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn a5_admission_admits_a_native_resource_fixture_and_the_stopped_major_drains_it() {
    // No finalizer: admission must not depend on a callback being present.
    let mut context = collection_fixture_context();
    assert_unreachable_native_fixture_is_admitted_and_drained(&mut context, |context| {
        context
            .tagged_heap
            .alloc_user_ptr(std::ptr::null_mut(), None)
    });
}

#[test]
fn a5_admission_still_refuses_a_sqlite_resource_fixture() {
    // SQLite remains refused until its GNU-observable DB-lifecycle row lands.
    unsupported_fixture_refuses_before_registration(|context| {
        context.tagged_heap.alloc_sqlite(false, 7)
    });
}

#[test]
fn a5_capture_refuses_a_preexisting_green_runtime() {
    let mut context =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
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
    let mut context = Context::new(crate::test_utils::unit_runtime());
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
fn a5_admission_admits_a_full_context_with_gnu_default_weak_table() {
    // Ordinary Context::new correctly includes GNU's weak-value
    // window-dead-windows-table. Since E3 slice 1 that eligible default table
    // no longer refuses the rooted admission; it survives the era unchanged.
    let mut context = Context::new(crate::test_utils::unit_runtime());
    assert_fixture_admits_before_registration(&mut context, |context| {
        context
            .tagged_heap
            .alloc_cons(TaggedValue::fixnum(5104), TaggedValue::NIL)
    });
    let table = context
        .builtin_var_value_by_name("window-dead-windows-table")
        .expect("the default weak table survives the admitted era");
    assert!(table.as_hash_table().is_some());
}

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn a6_owned_admission_pins_shell_census_and_restores_empty_weak_table() {
    let mut context = Context::new(crate::test_utils::unit_runtime());
    let shell = context
        .tagged_heap
        .alloc_cons(TaggedValue::fixnum(11), TaggedValue::fixnum(13));
    context.lexenv = shell;
    let shell_bits = shell.bits();
    context
        .admit_owned_proof_parallel(|projection| {
            assert!(
                neovm_value::tagged::gc::current_tagged_heap_identity().is_none(),
                "overlap must not install the legacy heap TLS facade"
            );
            {
                let era = projection.era();
                let mut stopped = era.stop().expect("empty membership stops");
                assert!(
                    ParallelHeapEraWorldCollectExt::collect_stopped(era, &mut stopped)
                        .unwrap()
                        .major
                );
                era.with_stopped_heap_for_test(&stopped, |heap| {
                    let shell = unsafe {
                        /* SAFETY: inverse of Value::bits on this live value */
                        TaggedValue::from_bits(shell_bits)
                    };
                    assert!(heap.owns_heap_value_for_test(shell));
                    assert_eq!(shell.cons_car_unobserved().as_fixnum(), Some(11));
                    assert_eq!(shell.cons_cdr_unobserved().as_fixnum(), Some(13));
                })
                .unwrap();
                stopped.complete().unwrap();
                assert!(
                    neovm_value::tagged::gc::current_tagged_heap_identity().is_none(),
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
fn a6_owned_admission_admits_a_nonempty_weak_table_without_clearing_it() {
    let mut context = Context::new(crate::test_utils::unit_runtime());
    context
        .eval_str("(puthash 1 2 window-dead-windows-table)")
        .unwrap();
    // A non-empty table is never retired at preparation, so recovery is
    // Unchanged and the same object must come back with its live entry.
    let table = context
        .builtin_var_value_by_name("window-dead-windows-table")
        .unwrap();
    let started = std::sync::atomic::AtomicBool::new(false);
    context
        .admit_owned_proof_parallel(|projection| {
            started.store(true, std::sync::atomic::Ordering::Relaxed);
            {
                let era = projection.era();
                let mut stopped = era.stop().expect("nonempty membership stops");
                assert!(
                    ParallelHeapEraWorldCollectExt::collect_stopped(era, &mut stopped)
                        .unwrap()
                        .major
                );
                stopped.complete().unwrap();
            }
            let (collector, keeper) = projection.finish().unwrap();
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("nonempty GNU weak table admits: {error:?}"));
    assert!(started.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(
        context.builtin_var_value_by_name("window-dead-windows-table"),
        Some(table),
        "unchanged recovery preserves the original table object"
    );
    // The immediate-keyed, immediate-valued entry survives the stopped weak
    // sweep: nothing about admission clears the live entry.
    assert_eq!(
        context
            .eval_str("(gethash 1 window-dead-windows-table)")
            .unwrap()
            .as_fixnum(),
        Some(2)
    );
}

#[test]
fn heapborrow_r9_collection_fixture_clears_and_restores_serial_tls() {
    Context::with_test_collection_era_then(
        |_| (),
        |era, ()| {
            assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
            let (collector, keeper) = era.finish().unwrap_or_else(|_| panic!("fixture finish"));
            (collector, keeper, ())
        },
        |context, ()| {
            assert!(neovm_value::tagged::gc::tagged_heap_is_current(
                &context.tagged_heap
            ));
            let value = TaggedValue::cons(TaggedValue::fixnum(53), TaggedValue::NIL);
            assert_eq!(value.cons_car(), TaggedValue::fixnum(53));
        },
    );
}
