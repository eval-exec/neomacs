//! Numeric storage-domain tests. Never dereference a stale/foreign key.
use super::*;
use crate::tagged::value::{TAG_CONS, TAG_MASK, TAG_STRING, TAG_VECLIKE};

fn heap() -> Box<TaggedHeap> {
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    heap
}

fn gen1_heap() -> Box<TaggedHeap> {
    let mut heap = Box::new(TaggedHeap::new());
    // Select the lifecycle proof's policy on an empty heap, before publishing
    // its TLS/card-table view or allocating any observed storage.
    heap.set_generational_enabled(true);
    set_tagged_heap(&mut heap);
    heap
}

#[test]
fn rows_owned_domain_recognizes_supported_preexisting_storage() {
    let mut heap = heap();
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let string = heap.alloc_string(crate::heap_types::LispString::from_utf8("owned"));
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    let record = heap.alloc_record(vec![TaggedValue::NIL]);
    let domain = heap.capture_rows_owned_domain().expect("default chunk map");
    for value in [cons, string, vector, record] {
        assert!(heap.rows_dependency_in_start_domain(&domain, value.0));
    }
}

#[test]
fn rows_owned_domain_rejects_wholly_new_blocks_and_pages() {
    let mut heap = heap();
    let domain = heap.capture_rows_owned_domain().unwrap();
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let string = heap.alloc_string(crate::heap_types::LispString::from_utf8("new page"));
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    let record = heap.alloc_record(vec![TaggedValue::NIL]);
    for value in [cons, string, vector, record] {
        assert!(!heap.rows_dependency_in_start_domain(&domain, value.0));
    }
}

#[test]
fn rows_owned_domain_allows_new_slots_only_inside_already_owned_storage() {
    let mut heap = heap();
    let first = heap.alloc_vector(vec![TaggedValue::NIL]);
    let domain = heap.capture_rows_owned_domain().unwrap();
    let next = heap.alloc_vector(vec![TaggedValue::NIL]);
    assert_eq!(
        (first.0 & !TAG_MASK) / OBJECT_PAGE_BYTES,
        (next.0 & !TAG_MASK) / OBJECT_PAGE_BYTES
    );
    assert!(heap.rows_dependency_in_start_domain(&domain, next.0));
    // This is storage provenance, not an allocation-bit or arbitrary-pointer
    // validity oracle; the actual read recorder supplied the valid key.
}

#[test]
fn rows_owned_domain_rejects_wrong_tags_alignment_and_unsupported_classes() {
    let mut heap = heap();
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    let lambda = heap.alloc_lambda(vec![]);
    let float = heap.alloc_float(1.0);
    let domain = heap.capture_rows_owned_domain().unwrap();
    let vector_address = vector.0 & !TAG_MASK;
    for bits in [
        0,
        TAG_CONS,
        (cons.0 & !TAG_MASK) | TAG_VECLIKE,
        vector_address | TAG_STRING,
        (vector_address + 8) | TAG_VECLIKE,
        lambda.0,
        float.0,
    ] {
        assert!(
            !heap.rows_dependency_in_start_domain(&domain, bits),
            "{bits:#x}"
        );
    }
}

#[test]
fn rows_owned_domain_rejects_unregistered_box_without_reading_it() {
    let heap = heap();
    let payload = Box::new([0usize; 8]);
    let address = (&*payload as *const [usize; 8]) as usize;
    let domain = heap.capture_rows_owned_domain().unwrap();
    assert!(!heap.rows_dependency_in_start_domain(&domain, address | TAG_VECLIKE));
    drop(payload);
    assert!(!heap.rows_dependency_in_start_domain(&domain, address | TAG_VECLIKE));
}

#[test]
fn rows_owned_domain_rejects_foreign_storage_before_and_after_foreign_drop() {
    let heap = heap();
    let mut foreign = Box::new(TaggedHeap::new());
    let key = foreign.alloc_cons(TaggedValue::NIL, TaggedValue::NIL).0;
    let domain = heap.capture_rows_owned_domain().unwrap();
    assert!(!heap.rows_dependency_in_start_domain(&domain, key));
    drop(foreign);
    assert!(!heap.rows_dependency_in_start_domain(&domain, key));
    // No endpoint ownership classification and no access to the dead cell.
}

#[test]
fn rows_owned_domain_foreign_owner_can_drop_on_another_mutator() {
    let heap = heap();
    let (keys, receive_key) = std::sync::mpsc::channel();
    let (release, wait_release) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut foreign = Box::new(TaggedHeap::new());
        let key = foreign.alloc_vector(vec![TaggedValue::NIL]).0;
        keys.send(key).unwrap(); // Numeric key only; no Value crosses threads.
        wait_release.recv().unwrap();
        drop(foreign);
    });
    let key = receive_key.recv().unwrap();
    let domain = heap.capture_rows_owned_domain().unwrap();
    assert!(!heap.rows_dependency_in_start_domain(&domain, key));
    release.send(()).unwrap();
    worker.join().unwrap();
    assert!(!heap.rows_dependency_in_start_domain(&domain, key));
}

#[test]
fn rows_owned_domain_requires_existing_chunk_map() {
    let mut heap = heap();
    let map = heap.chunk_map.take();
    assert!(heap.capture_rows_owned_domain().is_none());
    heap.chunk_map = map;
}

#[test]
fn rows_owned_domain_idle_refuses_major_partial_and_deferred_without_count_change() {
    let mut heap = gen1_heap();
    assert!(
        heap.generational_enabled(),
        "the lifecycle fixture explicitly selects GEN1"
    );
    let root = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let before = heap.gc_collections();
    heap.begin_collection();
    assert_eq!(heap.gc_collections(), before);
    assert!(!heap.rows_observation_idle());
    heap.seed_root(root);
    heap.incremental_drain_all();
    let bytes = heap.live_bytes();
    heap.incremental_finish(bytes, std::time::Instant::now());
    assert!(heap.sweep_in_progress());
    assert_eq!(heap.gc_collections(), before);
    assert!(!heap.rows_observation_idle());
    heap.finish_incremental_sweep_now();
    assert!(heap.gc_collections() > before);
    assert!(heap.rows_observation_idle());
}

#[test]
fn rows_owned_domain_minor_to_deferred_handoff_has_no_idle_gap() {
    let mut heap = gen1_heap();
    assert!(heap.generational_enabled());
    let root = heap.alloc_vector(vec![TaggedValue::NIL]);
    let count = heap.gc_collections();
    heap.begin_minor_collection().unwrap();
    assert_eq!(heap.gc_collections(), count);
    assert!(!heap.rows_observation_idle());
    heap.seed_root(root);
    heap.complete_minor_collection();
    assert!(heap.sweep_in_progress());
    assert_eq!(heap.gc_collections(), count);
    assert!(!heap.rows_observation_idle());
    heap.finish_incremental_sweep_now();
    assert!(heap.gc_collections() > count);
    assert!(heap.rows_observation_idle());
}

#[test]
fn rows_owned_domain_caught_direct_gen1_reclaim_panic_keeps_visible_obligation() {
    use tracing_subscriber::prelude::*;
    struct PanicAfterReclaim;
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for PanicAfterReclaim {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if event.metadata().target() == "neovm::gc::region" {
                panic!("contained subscriber panic after eager sweep, before native count");
            }
        }
    }
    let mut heap = gen1_heap();
    assert!(heap.generational_enabled());
    let dead = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL).0;
    let count = heap.gc_collections();
    let subscriber = tracing_subscriber::registry().with(PanicAfterReclaim);
    let panic = tracing::subscriber::with_default(subscriber, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Standalone heap: empty roots really are its entire live graph.
            heap.collect_exact(std::iter::empty());
        }))
    });
    assert!(panic.is_err(), "must exercise the exact post-reclaim event");
    assert_eq!(heap.gc_collections(), count);
    assert!(heap.generational.major_in_progress);
    assert!(!heap.rows_observation_idle());
    // Never inspect the reclaimed cell or pretend endpoint-count equality is
    // enough. An unwound GEN1 collector is deliberately ineligible.
    let _ = dead;
}

#[test]
fn rows_owned_domain_unhanded_region_return_preserves_published_storage_domain() {
    let mut heap = heap();
    let published = heap.alloc_cons(TaggedValue::fixnum(17), TaggedValue::NIL);
    let domain = heap.capture_rows_owned_domain().unwrap();
    let count = heap.gc_collections();
    let blocks = heap.cons_blocks.len();
    assert!(
        heap.jit.cons_cur.get() < heap.jit.cons_lim.get(),
        "actual unhanded region tail"
    );
    heap.close_alloc_regions();
    assert_eq!(heap.gc_collections(), count);
    assert_eq!(heap.cons_blocks.len(), blocks);
    assert!(heap.rows_dependency_in_start_domain(&domain, published.0));
    assert_eq!(published.cons_car(), TaggedValue::fixnum(17));
    let next = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert!(heap.rows_dependency_in_start_domain(&domain, next.0));
}

#[test]
fn rows_owned_domain_dropped_concurrent_capture_cannot_erase_active_cycle() {
    let mut heap = heap();
    let root = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let count = heap.gc_collections();
    // SAFETY: this fixture is the only writer; capture and seeding contain
    // no callback, allocation safepoint or legacy-TLS heap writer.
    let mut capture = unsafe { heap.permit_concurrent_mark() }.unwrap().begin();
    capture.heap_mut().seed_root(root);
    drop(capture); // Drop is not a completed collection or cancellation.
    assert_eq!(heap.gc_collections(), count);
    assert!(!heap.rows_observation_idle());
    // SAFETY: the standalone owner excludes every TLS writer/callback and
    // has no native finalizers. root is its complete external graph; the
    // existing closed drain finishes the begun mark and full deferred sweep.
    drop(unsafe { heap.drain_to_quiescent(|heap| heap.seed_root(root)) }.unwrap());
    assert!(heap.gc_collections() > count);
    assert!(heap.rows_observation_idle());
}

#[test]
fn rows_owned_domain_failed_concurrent_completion_retains_busy_obligation() {
    enum Failure {
        Missing,
        Disconnected,
        Poisoned,
    }
    for failure in [Failure::Missing, Failure::Disconnected, Failure::Poisoned] {
        let mut heap = heap();
        let count = heap.gc_collections();
        // Same inert protocol fixture as gc/tests/shutdown_tests.rs: no
        // worker exists. Exercise the real finish error paths without racing
        // a live worker or claiming that a worker actually disconnected.
        let (send, receive) = std::sync::mpsc::channel();
        heap.concurrent_mark_running = true;
        heap.gc_exited = Some(receive);
        match failure {
            Failure::Missing => {
                heap.gc_exited = None;
                drop(send);
            }
            Failure::Disconnected => drop(send),
            Failure::Poisoned => {
                send.send(ConcurrentMarkResult::default()).unwrap();
                heap.satb_handoff.poison_for_test();
            }
        }
        let error = heap.finish_concurrent_mark().unwrap_err();
        assert!(matches!(
            (failure, error),
            (Failure::Missing, MarkFinishError::MissingCompletion)
                | (Failure::Disconnected, MarkFinishError::Disconnected(_))
                | (Failure::Poisoned, MarkFinishError::PoisonedCollectorState)
        ));
        assert_eq!(heap.gc_collections(), count);
        assert!(heap.concurrent_mark_running());
        assert!(!heap.rows_observation_idle());
        // Empty heap; Drop conservatively abandons the protocol fixture.
    }
}
