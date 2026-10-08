//! Immediate and rooted transport: validation, rooting through collections,
//! retirement from foreign threads, and heap identity.

use std::sync::{Arc, Barrier, mpsc};

use super::root_table::cell_census;
use super::{
    ImmediateValue, NotImmediate, SharedRoot, SharedRootError, collect_shared_root_gc_roots,
};
use crate::emacs_core::intern::{
    intern, intern_uninterned, is_canonical_id, unintern_canonical_id,
};
use crate::heap_types::LispString;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::value::TaggedValue;

/// Collect with this heap's shared roots as the only roots.
fn collect_with_shared_roots(heap: &mut TaggedHeap) {
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(heap.heap_identity(), &mut roots);
    heap.collect_exact(roots.into_iter());
}

/// A root that keeps `heap`'s table alive, so retired cells stay observable:
/// a table is freed together with its last root.
fn table_anchor(heap: &TaggedHeap) -> SharedRoot {
    SharedRoot::new(
        heap,
        TaggedValue::from_sym_id(intern_uninterned("p73-table-anchor")),
    )
}

fn alloc_named_cons(heap: &mut TaggedHeap, name: &str) -> (TaggedValue, TaggedValue) {
    let text = heap.alloc_string(LispString::from_utf8(name));
    (heap.alloc_cons(text, TaggedValue::fixnum(7)), text)
}

#[test]
fn immediate_values_are_fixnums_nil_and_t() {
    assert_eq!(ImmediateValue::NIL.value().bits(), TaggedValue::NIL.bits());
    assert_eq!(ImmediateValue::T.value().bits(), TaggedValue::T.bits());
    let fixnum = ImmediateValue::fixnum(-42).expect("in range");
    assert_eq!(fixnum.value().as_fixnum(), Some(-42));
    assert_eq!(
        ImmediateValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM).map(ImmediateValue::value),
        Some(TaggedValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM))
    );
    assert_eq!(
        ImmediateValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM + 1),
        None
    );
    assert_eq!(
        ImmediateValue::fixnum(TaggedValue::MOST_NEGATIVE_FIXNUM - 1),
        None
    );
    assert_eq!(
        ImmediateValue::try_from(TaggedValue::NIL),
        Ok(ImmediateValue::NIL)
    );
    assert_eq!(
        ImmediateValue::try_from(TaggedValue::T),
        Ok(ImmediateValue::T)
    );
    assert_eq!(TaggedValue::from(fixnum).bits(), fixnum.value().bits());
}

#[test]
fn immediate_values_reject_heap_objects_and_uninterned_symbols() {
    let mut heap = TaggedHeap::new();
    let (cons, text) = alloc_named_cons(&mut heap, "not-immediate");
    assert_eq!(
        ImmediateValue::try_from(cons),
        Err(NotImmediate::HeapObject)
    );
    assert_eq!(
        ImmediateValue::try_from(text),
        Err(NotImmediate::HeapObject)
    );
    let uninterned = TaggedValue::from_sym_id(intern_uninterned("p73-uninterned"));
    assert_eq!(
        ImmediateValue::try_from(uninterned),
        Err(NotImmediate::SymbolNeedsRoot)
    );
}

#[test]
fn immediate_values_reject_a_canonical_symbol_before_and_after_unintern() {
    let id = intern("p73-immediate-canonical-rejection");
    let symbol = TaggedValue::from_sym_id(id);
    assert!(is_canonical_id(id));
    assert_eq!(
        ImmediateValue::try_from(symbol),
        Err(NotImmediate::SymbolNeedsRoot)
    );

    assert!(unintern_canonical_id(id));
    assert!(!is_canonical_id(id));
    assert_eq!(
        ImmediateValue::try_from(symbol),
        Err(NotImmediate::SymbolNeedsRoot)
    );
}

#[test]
fn shared_root_alone_keeps_an_object_alive_until_it_retires() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    let (cons, text) = alloc_named_cons(&mut heap, "shared-root-payload");
    let (dead_cons, dead_text) = alloc_named_cons(&mut heap, "unrooted-control");
    let _anchor = table_anchor(&heap);
    let root = SharedRoot::new(&heap, cons);
    assert_eq!(cell_census(heap.heap_identity()), (2, 0));

    for _ in 0..3 {
        collect_with_shared_roots(&mut heap);
        assert!(!heap.owns_heap_value_for_test(dead_cons));
        assert!(!heap.owns_heap_value_for_test(dead_text));
        assert!(heap.owns_heap_value_for_test(cons));
        assert!(heap.owns_heap_value_for_test(text));
        let local = root.materialize(&heap).expect("same heap");
        assert_eq!(local.value().bits(), cons.bits());
        assert_eq!(
            local.value().cons_car().as_str_owned().as_deref(),
            Some("shared-root-payload")
        );
    }

    // The root is the only holder while another thread owns it across a
    // collection; it retires there, on a thread that is not a mutator.
    let collected = Arc::new(Barrier::new(2));
    let (to_holder, holder_rx) = mpsc::channel::<SharedRoot>();
    let holder_collected = Arc::clone(&collected);
    let holder = std::thread::spawn(move || {
        let root = holder_rx.recv().expect("root arrives");
        holder_collected.wait();
        holder_collected.wait();
        drop(root);
    });
    to_holder.send(root).expect("holder alive");
    collected.wait();
    collect_with_shared_roots(&mut heap);
    assert!(heap.owns_heap_value_for_test(cons));
    collected.wait();
    holder.join().expect("holder exits");

    assert_eq!(cell_census(heap.heap_identity()), (1, 1));
    collect_with_shared_roots(&mut heap);
    assert!(!heap.owns_heap_value_for_test(cons));
    assert!(!heap.owns_heap_value_for_test(text));
}

#[test]
fn shared_root_materializes_only_on_its_own_heap() {
    let mut first = TaggedHeap::new();
    let second = TaggedHeap::new();
    let (cons, _) = alloc_named_cons(&mut first, "first-heap");
    let root = SharedRoot::new(&first, cons);
    assert_eq!(root.heap_identity(), first.heap_identity());
    assert_eq!(
        root.materialize(&second).map(|local| local.value().bits()),
        Err(SharedRootError::ForeignHeap {
            owner: first.heap_identity(),
            mutator: second.heap_identity(),
        })
    );
    assert_ne!(first.heap_identity(), second.heap_identity());
    assert_eq!(
        root.materialize(&first).map(|local| local.value().bits()),
        Ok(cons.bits())
    );
}

#[test]
fn untraced_values_take_no_root_cell() {
    let heap = TaggedHeap::new();
    let _anchor = table_anchor(&heap);
    let roots = [
        SharedRoot::new(&heap, TaggedValue::NIL),
        SharedRoot::new(&heap, TaggedValue::T),
        SharedRoot::new(&heap, TaggedValue::fixnum(99)),
    ];
    assert_eq!(cell_census(heap.heap_identity()), (1, 0));
    for (root, expected) in
        roots
            .iter()
            .zip([TaggedValue::NIL, TaggedValue::T, TaggedValue::fixnum(99)])
    {
        assert_eq!(
            root.materialize(&heap).expect("same heap").value().bits(),
            expected.bits()
        );
    }
    // Symbols other than nil and t are traced: an uninterned symbol's cells
    // survive only while something marks it.
    let uninterned = SharedRoot::new(
        &heap,
        TaggedValue::from_sym_id(intern_uninterned("p73-rooted-symbol")),
    );
    assert_eq!(cell_census(heap.heap_identity()), (2, 0));
    drop(uninterned);
    assert_eq!(cell_census(heap.heap_identity()), (1, 1));
}

#[test]
fn clones_share_one_root_and_retired_cells_are_recycled() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    let (cons, _) = alloc_named_cons(&mut heap, "cloned");
    let _anchor = table_anchor(&heap);
    let root = SharedRoot::new(&heap, cons);
    let clone = root.clone();
    assert!(clone.is_same_object(&root));
    assert_eq!(cell_census(identity), (2, 0));
    drop(root);
    collect_with_shared_roots(&mut heap);
    assert!(heap.owns_heap_value_for_test(cons));
    drop(clone);
    assert_eq!(cell_census(identity), (1, 1));

    // The next registration recycles the retired cell instead of growing.
    let (other, _) = alloc_named_cons(&mut heap, "recycled");
    let recycled = SharedRoot::new(&heap, other);
    assert_eq!(cell_census(identity), (2, 0));
    assert!(!recycled.is_same_object(&SharedRoot::new(&heap, cons)));
}

#[test]
fn a_table_is_freed_with_its_last_root() {
    let heap = TaggedHeap::new();
    let root = table_anchor(&heap);
    assert_eq!(cell_census(heap.heap_identity()), (1, 0));
    std::thread::spawn(move || drop(root))
        .join()
        .expect("dropping thread exits");
    assert_eq!(cell_census(heap.heap_identity()), (0, 0));
}

#[test]
fn shared_roots_cross_threads_while_the_mutator_collects() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    let mut payloads = Vec::new();
    let mut roots = Vec::new();
    for index in 0..32 {
        let (cons, text) = alloc_named_cons(&mut heap, &format!("concurrent-{index}"));
        payloads.push((cons, text));
        roots.push(SharedRoot::new(&heap, cons));
    }

    // Workers clone, hold and drop roots on their own threads while the
    // mutator collects; each returns one clone per root it was given.
    let (returned_tx, returned_rx) = mpsc::channel::<Vec<SharedRoot>>();
    let workers: Vec<_> = roots
        .chunks(8)
        .map(|chunk| {
            let chunk = chunk.to_vec();
            let returned_tx = returned_tx.clone();
            std::thread::spawn(move || {
                for _ in 0..200 {
                    let clones: Vec<SharedRoot> = chunk.iter().map(SharedRoot::clone).collect();
                    drop(clones);
                }
                returned_tx.send(chunk).expect("mutator alive");
            })
        })
        .collect();
    drop(returned_tx);
    drop(roots);
    for _ in 0..4 {
        collect_with_shared_roots(&mut heap);
    }
    let returned: Vec<SharedRoot> = returned_rx.into_iter().flatten().collect();
    for worker in workers {
        worker.join().expect("worker exits");
    }

    collect_with_shared_roots(&mut heap);
    assert_eq!(returned.len(), payloads.len());
    for (cons, text) in &payloads {
        assert!(heap.owns_heap_value_for_test(*cons));
        assert!(heap.owns_heap_value_for_test(*text));
    }
    for root in &returned {
        let local = root.materialize(&heap).expect("same heap");
        assert!(
            payloads
                .iter()
                .any(|(cons, _)| cons.bits() == local.value().bits())
        );
    }
    assert_eq!(cell_census(identity).0, payloads.len());
    drop(returned);
    collect_with_shared_roots(&mut heap);
    for (cons, text) in &payloads {
        assert!(!heap.owns_heap_value_for_test(*cons));
        assert!(!heap.owns_heap_value_for_test(*text));
    }
}

#[test]
fn context_root_walk_includes_shared_roots() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::emacs_core::eval::Context::new();
    let text = TaggedValue::string("context-shared-root");
    let root = SharedRoot::from_current_heap(text).expect("Context installs its heap");
    assert_eq!(root.heap_identity(), context.tagged_heap.heap_identity());
    let holder = std::thread::spawn(move || root);
    let root = holder.join().expect("holder returns the root");
    for _ in 0..3 {
        context.gc_collect_exact();
        assert!(context.tagged_heap.owns_heap_value_for_test(text));
    }
    let local = root
        .materialize(&context.tagged_heap)
        .expect("same heap")
        .value();
    assert_eq!(local.as_str_owned().as_deref(), Some("context-shared-root"));
    drop(root);
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(text));
}
