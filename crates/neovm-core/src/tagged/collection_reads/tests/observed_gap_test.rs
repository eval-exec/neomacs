//! Native false-positive hits refine protocol bounds without losing live reads.
//!
//! Each nextest isolation owns its process environment. The heap constructors
//! select GEN0 temporarily; all owners kept across GC are explicit Context
//! roots. Shared observation marks never substitute for this mutator's ledger.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::neovm_jit_setcar;
use crate::heap_types::LispString;
use crate::tagged::gc::{BarrierWindow, neovm_jit_unobserved_collection_owner, set_tagged_heap};
use crate::tagged::header::{ConsCdrOrNext, ConsCell};
use crate::tagged::value::TAG_MASK;

struct ObservedMode;

impl ObservedMode {
    fn begin() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn context() -> Context {
    struct RestoreGeneration(Option<std::ffi::OsString>);
    impl Drop for RestoreGeneration {
        fn drop(&mut self) {
            // This isolated process has no concurrent heap constructors.
            unsafe {
                match self.0.take() {
                    Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }
    let _restore = RestoreGeneration(std::env::var_os("NEOVM_GC_GENERATIONAL"));
    unsafe { std::env::remove_var("NEOVM_GC_GENERATIONAL") };
    let mut context = Context::new();
    context.gc_stress = false;
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert!(!context.tagged_heap.generational_enabled());
    context
}

fn address(owner: TaggedValue) -> usize {
    owner.bits() & !TAG_MASK
}

fn rooted_cons_triplet(context: &mut Context) -> [TaggedValue; 3] {
    let mut owners = std::array::from_fn(|_| {
        context
            .tagged_heap
            .alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL)
    });
    owners.sort_unstable_by_key(|owner| address(*owner));
    assert!(address(owners[0]) < address(owners[1]));
    assert!(address(owners[1]) < address(owners[2]));
    for owner in owners {
        context.push_specpdl_root(owner);
    }
    owners
}

fn native_setcar(context: &mut Context, owner: TaggedValue, value: TaggedValue) {
    let vmctx = (context as *mut Context).cast::<u8>();
    assert_eq!(
        neovm_jit_setcar(vmctx, owner.bits() as i64, value.bits() as i64),
        value.bits() as i64,
    );
}

fn exclude_middle(context: &Context, owners: [TaggedValue; 3]) -> BarrierWindow {
    let [left, target, right] = owners;
    let (_, certificate) = capture(|| (left.cons_car(), right.cons_car()));
    let certificate = certificate.expect("the observed endpoints are coherent");
    assert!(certificate.unchanged());
    assert!(!is_observed(target.bits()));
    assert!(
        context
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(address(target))
    );
    assert_eq!(
        neovm_jit_unobserved_collection_owner(target.bits() as i64),
        1
    );
    let gap = compiled_observation_gate(BarrierWindow::NONE)
        .excluded
        .expect("the target lies in a locally empty interval");
    assert!(gap.covers(address(target)));
    assert!(!gap.covers(address(left)));
    assert!(!gap.covers(address(right)));
    let published = context.tagged_heap.jit_barrier_window_for_test();
    assert!(!published.covers(address(target)));
    assert!(published.covers(address(left)) && published.covers(address(right)));
    gap
}

#[test]
fn observed_unobserved_cons_gap_closes_before_a_new_certificate() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut context = context();
    let owners = rooted_cons_triplet(&mut context);
    let [left, target, right] = owners;
    let (_, endpoints) = capture(|| (left.cons_car(), right.cons_car()));
    let endpoints = endpoints.expect("retained endpoint reads");
    exclude_middle(&context, owners);
    let revision = LispCollectionRevision::current();
    native_setcar(&mut context, target, TaggedValue::make_int(12));
    assert_eq!(LispCollectionRevision::current(), revision);
    assert!(endpoints.unchanged());

    let (result, reads) = capture(|| target.cons_car());
    assert_eq!(result, TaggedValue::make_int(12));
    let reads = reads.expect("a first read after the unobserved store");
    assert!(reads.unchanged());
    assert!(
        compiled_observation_gate(BarrierWindow::NONE)
            .excluded
            .is_none()
    );
    assert!(
        context
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(address(target))
    );
    assert_eq!(
        neovm_jit_unobserved_collection_owner(target.bits() as i64),
        0
    );
    native_setcar(&mut context, target, TaggedValue::make_int(23));
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );
    assert!(
        !reads.unchanged(),
        "the newly observed dependency is journaled"
    );
    assert!(endpoints.unchanged(), "only the target changed");
}

#[test]
fn observed_gc_pruning_preserves_empty_gap_and_live_certificate() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut context = context();
    let roots = context.save_specpdl_roots();
    let owners = rooted_cons_triplet(&mut context);
    let [left, target, right] = owners;
    let gap = exclude_middle(&context, owners);
    let (_, reads) = capture(|| left.cons_car());
    let reads = reads.expect("only the left observed owner stays live");
    context.restore_specpdl_roots(roots);
    context.push_specpdl_root(left);
    context.push_specpdl_root(target);
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(left));
    assert!(context.tagged_heap.owns_heap_value_for_test(target));
    assert!(!context.tagged_heap.owns_heap_value_for_test(right));
    assert_eq!(
        compiled_observation_gate(BarrierWindow::NONE).excluded,
        Some(gap)
    );
    let published = context.tagged_heap.jit_barrier_window_for_test();
    assert!(published.covers(address(left)));
    assert!(!published.covers(address(target)));
    assert!(reads.unchanged());
    native_setcar(&mut context, left, TaggedValue::make_int(23));
    assert!(
        !reads.unchanged(),
        "GC retained the live observed dependency"
    );
}

#[test]
fn observed_heap_switch_dump_change_discards_empty_gap() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut first = context();
    let mut second = context();
    set_tagged_heap(&mut first.tagged_heap);
    let owners = rooted_cons_triplet(&mut first);
    exclude_middle(&first, owners);
    let (_, reads) = capture(|| owners[0].cons_car());
    let reads = reads.expect("the first heap's owner remains retained");

    set_tagged_heap(&mut second.tagged_heap);
    // This valid writable image allocation outlives both heaps, as an actual
    // pdump mapping does. No fabricated header or non-object address is used.
    let image = Box::leak(Box::new(ConsCell {
        car: TaggedValue::make_int(7),
        cdr_or_next: ConsCdrOrNext {
            cdr: TaggedValue::NIL,
        },
    }));
    unsafe { second.tagged_heap.register_mapped_cons_range(image, 1) };
    let dump = second.tagged_heap.collection_dump_window();
    assert_ne!(dump, BarrierWindow::NONE);
    assert!(compiled_observation_gate(dump).excluded.is_none());
    assert!(
        second
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(image as *mut ConsCell as usize)
    );
    set_tagged_heap(&mut first.tagged_heap);
    assert!(
        compiled_observation_gate(BarrierWindow::NONE)
            .excluded
            .is_none()
    );
    assert!(reads.unchanged());
    native_setcar(&mut first, owners[0], TaggedValue::make_int(23));
    assert!(!reads.unchanged());
}

#[test]
fn observed_concurrent_barrier_overrides_empty_gap() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut context = context();
    let owners = rooted_cons_triplet(&mut context);
    let target = owners[1];
    let old_child = context
        .tagged_heap
        .alloc_cons(TaggedValue::make_int(7), TaggedValue::NIL);
    context.push_specpdl_root(old_child);
    target.set_car(old_child);
    let gap = exclude_middle(&context, owners);
    context.tagged_heap.set_concurrent_active_for_test(true);
    assert_eq!(
        context.tagged_heap.jit_barrier_window_for_test(),
        BarrierWindow::ALL
    );
    assert_eq!(
        neovm_jit_unobserved_collection_owner(target.bits() as i64),
        0
    );
    native_setcar(&mut context, target, TaggedValue::NIL);
    assert!(
        context
            .tagged_heap
            .take_satb_shared_for_test()
            .contains(&old_child)
    );
    context.tagged_heap.set_concurrent_active_for_test(false);
    assert_eq!(
        compiled_observation_gate(BarrierWindow::NONE).excluded,
        Some(gap)
    );
    assert!(
        !context
            .tagged_heap
            .jit_barrier_window_for_test()
            .covers(address(target))
    );
}

#[test]
fn observed_cons_gap_retains_vector_and_string_dependencies() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut context = context();
    let cons = context
        .tagged_heap
        .alloc_cons(TaggedValue::make_int(7), TaggedValue::NIL);
    let vector = context
        .tagged_heap
        .alloc_vector(vec![TaggedValue::make_int(8)]);
    let string = context
        .tagged_heap
        .alloc_string(LispString::from_utf8("abc"));
    let target = context
        .tagged_heap
        .alloc_cons(TaggedValue::make_int(9), TaggedValue::NIL);
    for owner in [cons, vector, string, target] {
        context.push_specpdl_root(owner);
    }
    let (_, reads) = capture(|| {
        cons.cons_car();
        vector.as_vector_data().expect("vector")[0];
        string.as_str_owned().expect("string");
    });
    let reads = reads.expect("all three real collection dependencies");
    assert_eq!(
        neovm_jit_unobserved_collection_owner(target.bits() as i64),
        1
    );
    let gap = compiled_observation_gate(BarrierWindow::NONE)
        .excluded
        .expect("empty target interval");
    assert!(gap.covers(address(target)));
    let published = context.tagged_heap.jit_barrier_window_for_test();
    for owner in [cons, vector, string] {
        assert!(!gap.covers(address(owner)));
        assert!(published.covers(address(owner)));
    }
    native_setcar(&mut context, target, TaggedValue::make_int(12));
    assert!(reads.unchanged());
    assert!(crate::tagged::mutate::set_vector_slot(
        vector,
        0,
        TaggedValue::make_int(23)
    ));
    assert!(
        !reads.unchanged(),
        "a different tag still bounds the Cons exclusion"
    );
}

#[test]
fn observed_mode_transition_republishes_an_off_capture_dependency() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut context = context();
    let owner = context
        .tagged_heap
        .alloc_cons(TaggedValue::make_int(7), TaggedValue::NIL);
    context.push_specpdl_root(owner);
    force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
    let (_, reads) = capture(|| {
        assert_eq!(owner.cons_car(), TaggedValue::make_int(7));
        assert!(!is_observed(owner.bits()));
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        assert_eq!(owner.cons_car(), TaggedValue::make_int(7));
        assert!(
            is_observed(owner.bits()),
            "an Off-mode recent hit cannot skip publication"
        );
        native_setcar(&mut context, owner, TaggedValue::make_int(23));
    });
    assert!(
        reads.is_none(),
        "the dependency's read still precedes its native mutation"
    );
}
