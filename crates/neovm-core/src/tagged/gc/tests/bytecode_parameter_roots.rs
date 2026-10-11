//! FunctionParams::Dynamic carries its own strong heap child, even when a
//! Rust caller assigns the observable arglist slot separately before publish.
use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, FunctionParams, Op};
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};

/// Test-local roots restored before the attached heap is dropped.
#[must_use = "dropping the guard restores the test's scratch roots"]
struct ScratchRoots(usize, std::marker::PhantomData<std::rc::Rc<()>>);
static_assertions::assert_not_impl_any!(ScratchRoots: Send, Sync);
impl ScratchRoots {
    fn new() -> Self {
        Self(save_scratch_gc_roots(), std::marker::PhantomData)
    }
    fn keep(&self, value: TaggedValue) {
        push_scratch_gc_root(value);
    }
}
impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

#[derive(Clone, Copy, Debug)]
enum Collection {
    Major,
    Minor,
    Incremental,
    Concurrent,
}

fn collect(heap: &mut TaggedHeap, owner: TaggedValue, collection: Collection) {
    match collection {
        Collection::Major => heap.collect_exact(std::iter::once(owner)),
        Collection::Minor => {
            heap.begin_minor_collection();
            heap.seed_root(owner);
            heap.complete_minor_collection();
            heap.finish_incremental_sweep_now();
        }
        Collection::Incremental | Collection::Concurrent => {
            heap.concurrent_begin();
            heap.seed_root(owner);
            if matches!(collection, Collection::Concurrent) {
                heap.launch_concurrent_mark();
                while !heap.concurrent_mark_done() {
                    std::thread::yield_now();
                }
                heap.join_concurrent_mark();
                heap.reseed_runtime_and_remembered_roots();
                heap.seed_root(owner);
            }
            let bytes_before = heap.live_bytes();
            heap.incremental_drain_all();
            heap.incremental_finish(bytes_before, std::time::Instant::now());
            heap.finish_incremental_sweep_now();
        }
    }
    assert!(!heap.mark_in_progress());
    assert!(!heap.sweep_in_progress());
}

fn independent_parameter_child_survives(collection: Collection) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = true;
    heap.publish_barrier_window();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let parameter_list = heap.alloc_cons(
        TaggedValue::symbol("p6-independent-parameter"),
        TaggedValue::NIL,
    );
    let observable_arglist = heap.alloc_cons(TaggedValue::fixnum(91), TaggedValue::NIL);
    let mut function = ByteCodeFunction::new(FunctionParams::try_from(parameter_list).unwrap());
    // Both assignments are safe public Rust API, performed BEFORE publish.
    // Neither child is rooted directly: the function is their sole owner.
    function.arglist = observable_arglist;
    function.ops = vec![Op::Nil, Op::Return];
    let owner = heap.alloc_bytecode(function);
    roots.keep(owner);
    collect(&mut heap, owner, collection);
    // No allocation since sweeping, and check ownership before dereferencing
    // either child, so a missing edge produces an ordinary assertion failure.
    assert!(
        heap.owns_heap_value_for_test(parameter_list),
        "{collection:?} lost the embedded parameter child"
    );
    assert!(
        heap.owns_heap_value_for_test(observable_arglist),
        "{collection:?} lost the observable arglist child"
    );
    assert_eq!(
        parameter_list.cons_car(),
        TaggedValue::symbol("p6-independent-parameter")
    );
    assert_eq!(observable_arglist.cons_car().as_fixnum(), Some(91));
    let children = heap.collect_veclike_children(owner.as_veclike_ptr().unwrap().cast_mut());
    assert!(children.contains(&parameter_list));
    assert!(children.contains(&observable_arglist));
}

#[test]
fn p6_dynamic_parameter_child_survives_major_collection() {
    independent_parameter_child_survives(Collection::Major);
}

#[test]
fn p6_dynamic_parameter_child_survives_minor_collection() {
    independent_parameter_child_survives(Collection::Minor);
}

#[test]
fn p6_dynamic_parameter_child_survives_incremental_collection() {
    independent_parameter_child_survives(Collection::Incremental);
}

#[test]
fn p6_dynamic_parameter_child_survives_concurrent_collection() {
    independent_parameter_child_survives(Collection::Concurrent);
}
