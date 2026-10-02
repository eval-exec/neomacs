use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::{CONS_BLOCK_CELLS, TaggedHeap, set_tagged_heap};

fn populate_cache(context: &mut Context) -> Value {
    context.setup_thread_locals();
    let string = Value::string("aжλb");
    assert_eq!(
        string_char_to_byte(string, string.as_lisp_string().unwrap(), 2),
        3
    );
    assert_eq!(CACHE.with(Cell::get).unwrap().string.bits(), string.bits());
    string
}

fn collect_on_another_thread(mut context: Context, string: Value) -> Context {
    let initial_epoch = context.tagged_heap.gc_collections();
    std::thread::spawn(move || {
        context.setup_thread_locals();
        assert!(CACHE.with(Cell::get).is_none());
        context.gc_collect_exact();
        assert!(context.tagged_heap.gc_collections() > initial_epoch);
        assert!(
            !context.tagged_heap.owns_heap_value_for_test(string),
            "the destination thread must sweep the source cache's string"
        );
        context
    })
    .join()
    .unwrap()
}

#[test]
fn gc_collection_epoch_string_pos_activation_discards_swept_source_entry() {
    let mut context = Context::new();
    let heap_identity = context.tagged_heap.identity();
    let string = populate_cache(&mut context);
    let mut context = collect_on_another_thread(context, string);
    assert_eq!(context.tagged_heap.identity(), heap_identity);
    // The source thread still has the same heap installed and an entry whose
    // string has been reclaimed on the destination thread. Activation must
    // invalidate by completed collection, even though the heap did not change.
    assert!(CACHE.with(Cell::get).is_some());
    context.setup_thread_locals();
    assert!(
        CACHE.with(Cell::get).is_none(),
        "same-heap activation retained a string swept on another thread"
    );
    context.gc_collect_exact();
}

#[test]
fn gc_collection_epoch_string_pos_root_scan_discards_swept_source_entry() {
    let mut context = Context::new();
    let string = populate_cache(&mut context);
    let mut context = collect_on_another_thread(context, string);
    assert!(crate::tagged::gc::tagged_heap_is_current(
        &context.tagged_heap
    ));
    assert!(CACHE.with(Cell::get).is_some());
    // No explicit activation: the collector's already-active fast path skips
    // setup_thread_locals. Its root scan must invalidate before seeding roots.
    let mut roots = Vec::new();
    collect_string_pos_cache_gc_roots(
        &mut roots,
        context.tagged_heap.identity(),
        context.tagged_heap.gc_collections(),
        crate::tagged::gc::CacheRootScan::Collection,
    );
    assert!(
        roots.iter().all(|root| root.bits() != string.bits()),
        "root enumeration offered a string swept on another thread"
    );
    assert!(
        CACHE.with(Cell::get).is_none(),
        "root enumeration retained a reclaimed cache entry"
    );
    context.gc_collect_exact();
}

#[test]
fn gc_collection_epoch_string_pos_migration_invalidates_changed_string_layout() {
    let mut context = Context::new();
    let string = populate_cache(&mut context);
    assert_eq!(
        string_char_to_byte(string, string.as_lisp_string().unwrap(), 1),
        1
    );
    let original_data = string.as_lisp_string().unwrap().as_bytes().as_ptr() as usize;
    let original_sbytes = string.as_lisp_string().unwrap().sbytes();
    let initial_epoch = context.tagged_heap.gc_collections();
    let mut context = std::thread::spawn(move || {
        context.setup_thread_locals();
        string.with_lisp_string_mut(|string| {
            string.mutate_bytes(|bytes| bytes.copy_from_slice("жaλb".as_bytes()));
        });
        let changed = string.as_lisp_string().unwrap();
        assert_eq!(changed.as_bytes().as_ptr() as usize, original_data);
        assert_eq!(changed.sbytes(), original_sbytes);
        context
    })
    .join()
    .unwrap();
    assert_eq!(context.tagged_heap.gc_collections(), initial_epoch);
    context.setup_thread_locals();
    assert_eq!(
        string_char_to_byte(string, string.as_lisp_string().unwrap(), 1),
        2,
        "source-thread cache reused an offset invalidated on another thread"
    );
    assert_eq!(
        string_byte_to_char(string, string.as_lisp_string().unwrap(), 2),
        1
    );
}

#[test]
fn gc_collection_epoch_string_pos_same_thread_activation_keeps_warm_entry() {
    let mut context = Context::new();
    let string = populate_cache(&mut context);
    let before = CACHE.with(Cell::get).unwrap();
    context.setup_thread_locals();
    let after = CACHE
        .with(Cell::get)
        .expect("same-thread activation evicted a warm entry");
    assert_eq!(CACHE.with(Cell::get).unwrap().string.bits(), string.bits());
    assert_eq!(after.data, before.data);
    assert_eq!(after.char_pos, before.char_pos);
    assert_eq!(after.byte_pos, before.byte_pos);
    assert_eq!(after.epoch, before.epoch);
    assert_eq!(
        string_char_to_byte(string, string.as_lisp_string().unwrap(), 2),
        3
    );
}

fn activate_cache_for_heap(heap: &TaggedHeap) {
    activate_string_pos_cache(
        heap.identity(),
        heap.gc_collections(),
        heap.mark_in_progress() || heap.sweep_in_progress(),
        false,
    );
}

fn partly_swept_cached_string() -> (Box<TaggedHeap>, Value, usize) {
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    activate_cache_for_heap(&heap);
    let string = Value::string("aжλb");
    string_char_to_byte(string, string.as_lisp_string().unwrap(), 2);
    assert_eq!(CACHE.with(Cell::get).unwrap().string.bits(), string.bits());
    // Leave a second cons block to sweep after the first string page. These
    // allocations and the cache entry are intentionally absent from the mark
    // roots, as a source thread's TLS is absent from a destination scan.
    for _ in 0..2 * CONS_BLOCK_CELLS {
        heap.alloc_cons(Value::NIL, Value::NIL);
    }
    let initial_epoch = heap.gc_collections();
    heap.begin_stw_collection();
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(heap.sweep_in_progress());
    assert!(
        !heap.incremental_sweep_slice(1),
        "one slice must leave another cons block pending"
    );
    assert!(heap.sweep_in_progress());
    assert_eq!(heap.gc_collections(), initial_epoch);
    assert!(
        !heap.owns_heap_value_for_test(string),
        "the partial slice must reclaim the cached string"
    );
    (heap, string, initial_epoch)
}

#[test]
fn gc_collection_epoch_string_pos_partial_sweep_activation_discards_swept_entry() {
    let (mut heap, _, initial_epoch) = partly_swept_cached_string();
    activate_cache_for_heap(&heap);
    assert!(
        CACHE.with(Cell::get).is_none(),
        "activation retained an entry reclaimed before cycle completion"
    );
    assert_eq!(heap.gc_collections(), initial_epoch);
    heap.finish_incremental_sweep_now();
}

#[test]
fn gc_collection_epoch_string_pos_partial_sweep_root_scan_discards_swept_entry() {
    let (mut heap, string, initial_epoch) = partly_swept_cached_string();
    let mut roots = Vec::new();
    collect_string_pos_cache_gc_roots(
        &mut roots,
        heap.identity(),
        heap.gc_collections(),
        crate::tagged::gc::CacheRootScan::Snapshot {
            collection_in_progress: heap.mark_in_progress() || heap.sweep_in_progress(),
        },
    );
    assert!(
        roots.iter().all(|root| root.bits() != string.bits()),
        "root enumeration offered an entry reclaimed before cycle completion"
    );
    assert!(CACHE.with(Cell::get).is_none());
    assert_eq!(heap.gc_collections(), initial_epoch);
    heap.finish_incremental_sweep_now();
}

fn assert_warm_entry(string: Value, before: Entry) {
    let after = CACHE
        .with(Cell::get)
        .expect("owning-thread GC evicted the warm entry");
    assert_eq!(after.string.bits(), before.string.bits());
    assert_eq!(after.data, before.data);
    assert_eq!(after.sbytes, before.sbytes);
    assert_eq!(after.epoch, before.epoch);
    assert_eq!(after.char_pos, before.char_pos);
    assert_eq!(after.byte_pos, before.byte_pos);
    assert_eq!(
        cached_pair(string, string.as_lisp_string().unwrap()),
        Some((2, 3))
    );
    assert_eq!(
        string_char_to_byte(string, string.as_lisp_string().unwrap(), 2),
        3
    );
}

#[test]
fn gc_collection_epoch_string_pos_exact_collections_keep_warm_entry() {
    let mut context = Context::new();
    let string = populate_cache(&mut context);
    let before = CACHE.with(Cell::get).unwrap();
    for _ in 0..3 {
        let completed = context.tagged_heap.gc_collections();
        context.gc_collect_exact();
        assert_eq!(context.tagged_heap.gc_collections(), completed + 1);
        // Check before activation and before conversion can refill the entry.
        assert_warm_entry(string, before);
        context.setup_thread_locals();
        assert_warm_entry(string, before);
    }
}

fn start_concurrent_cycle(context: &mut Context) {
    context.tagged_heap.set_gc_threshold(1);
    context.tagged_heap.alloc_cons(Value::NIL, Value::NIL);
    context.gc_safe_point();
    assert!(context.tagged_heap.concurrent_mark_running());
    assert!(context.tagged_heap.mark_in_progress());
}

fn start_deferred_sweep(context: &mut Context) {
    for _ in 0..100_000 {
        context.gc_safe_point();
        if context.tagged_heap.sweep_in_progress() {
            return;
        }
        std::thread::yield_now();
    }
    panic!("concurrent marking did not enter deferred sweep");
}

#[test]
fn gc_collection_epoch_string_pos_concurrent_collections_keep_warm_entry() {
    let mut context = Context::new();
    context.gc_collect_exact();
    let string = populate_cache(&mut context);
    let before = CACHE.with(Cell::get).unwrap();
    for _ in 0..3 {
        let completed = context.tagged_heap.gc_collections();
        start_concurrent_cycle(&mut context);
        context.setup_thread_locals();
        assert_warm_entry(string, before);
        start_deferred_sweep(&mut context);
        assert_eq!(context.tagged_heap.gc_collections(), completed);
        context.setup_thread_locals();
        assert_warm_entry(string, before);
        context.gc_safe_point();
        assert!(!context.tagged_heap.sweep_in_progress());
        assert_eq!(context.tagged_heap.gc_collections(), completed + 1);
        context.setup_thread_locals();
        assert_warm_entry(string, before);
    }
}

#[derive(Clone, Copy)]
enum ForeignPhase {
    Mark,
    Sweep,
}

fn assert_foreign_cycle_discards_entry(phase: ForeignPhase) {
    let mut context = Context::new();
    context.gc_collect_exact();
    let string = populate_cache(&mut context);
    let completed = context.tagged_heap.gc_collections();
    let mut context = std::thread::spawn(move || {
        context.setup_thread_locals();
        start_concurrent_cycle(&mut context);
        if matches!(phase, ForeignPhase::Sweep) {
            start_deferred_sweep(&mut context);
        }
        assert_eq!(context.tagged_heap.gc_collections(), completed);
        context
    })
    .join()
    .unwrap();
    assert_eq!(CACHE.with(Cell::get).unwrap().string.bits(), string.bits());
    context.setup_thread_locals();
    assert!(
        CACHE.with(Cell::get).is_none(),
        "foreign in-progress collection retained the source entry"
    );
    // No access to the string after the destination stopped enumerating it.
    context.gc_collect_exact();
}

#[test]
fn gc_collection_epoch_string_pos_foreign_mark_discards_entry() {
    assert_foreign_cycle_discards_entry(ForeignPhase::Mark);
}

#[test]
fn gc_collection_epoch_string_pos_foreign_sweep_discards_entry() {
    assert_foreign_cycle_discards_entry(ForeignPhase::Sweep);
}
