//! Direct Rust regression for the first-partition char-table scan/store seam.
//!
//! The fixture sends ordinary heap objects through the actual mapped-veclike
//! scan path. It never needs a pdump file, Lisp evaluation, or a timing sleep.

use super::*;
use crate::emacs_core::chartable::{builtin_char_table_range, builtin_set_char_table_range};
use crate::emacs_core::intern::intern_uninterned;
use crate::tagged::gc::mapped_veclike_scan::{MappedVeclikeScanItem, MappedVeclikeScanSnapshot};
use crate::tagged::header::LispValueVec;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Barrier, Condvar, Mutex};

const REPEATS: usize = 4096;

fn set_range(table: TaggedValue, range: TaggedValue, value: TaggedValue) {
    assert_eq!(
        builtin_set_char_table_range(vec![table, range, value], None).unwrap(),
        value,
    );
}

fn fixture_headers(table: TaggedValue) -> Vec<usize> {
    let mut values = vec![table];
    let mut cursor = 0;
    while cursor < values.len() {
        let value = values[cursor];
        cursor += 1;
        if let Some(object) = value.as_char_table_obj() {
            values.extend(
                object
                    .contents
                    .iter()
                    .copied()
                    .filter(|child| child.veclike_type() == Some(VecLikeType::SubCharTable)),
            );
        } else if let Some(object) = value.as_sub_char_table_obj() {
            values.extend(
                object
                    .contents
                    .as_slice()
                    .iter()
                    .copied()
                    .filter(|child| child.veclike_type() == Some(VecLikeType::SubCharTable)),
            );
        }
    }
    assert_eq!(
        values.len(),
        4,
        "the fixture must prebuild all three levels"
    );
    values
        .into_iter()
        .map(|value| value.as_veclike_ptr().unwrap() as usize)
        .collect()
}

fn finish_fixture_epoch(
    heap: &mut TaggedHeap,
    table: TaggedValue,
    completed: ConcurrentMarkResult,
) {
    // The explicit fixture join proved this exact handoff is final. Relay its
    // owned result into the normal completion seam, preserving every symbol
    // and promotion record rather than resetting the phase by hand.
    let (send, receive) = std::sync::mpsc::channel();
    send.send(completed)
        .expect("retain the joined reader handoff");
    heap.gc_exited = Some(receive);
    // SAFETY: both fixtures have explicitly joined their only reader. This
    // thread owns the heap and its TLS installation, with no callback, native
    // finalizer, obarray, or other writer. The table is their complete external
    // root graph; the retained fake image contains only immediate fixnums.
    // The normal finish folds SATB/deferred work and generational preimages,
    // then this closed drain traces and sweeps the existing epoch completely.
    drop(
        unsafe { heap.drain_to_quiescent(|heap| heap.seed_root(table)) }
            .expect("finish the complete fixture epoch before teardown"),
    );
    assert!(
        heap.mutators()
            .all(|mutator| mutator.major_symbol_preimages.is_empty())
    );
}

#[test]
fn concurrent_mapped_chartable_scan_and_builtin_range_writes_are_atomic() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let purpose_id = intern_uninterned("ctrace-purpose");
    let sentinel_id = intern_uninterned("ctrace-untouched-leaf");
    let purpose = TaggedValue::from_sym_id(purpose_id);
    let sentinel = TaggedValue::from_sym_id(sentinel_id);
    let table = TaggedValue::make_char_table(purpose, TaggedValue::fixnum(-1), 0);
    set_range(table, TaggedValue::fixnum(0), TaggedValue::fixnum(0));
    set_range(table, TaggedValue::fixnum(1), sentinel);
    let headers = fixture_headers(table);

    // This is the same confined manual-job pattern as the major worker
    // fixtures: begin a real owner-local mark, publish only retained addresses
    // and immutable ownership metadata, and join before reclamation. The worker
    // never gets the heap itself. The explicit phase mirror makes each builtin
    // use the real SATB preimage barrier while the job reads its slots.
    heap.concurrent_begin();
    heap.concurrent_mark_running = true;
    set_tagged_heap(&mut heap);
    let mapped_scan = {
        // SAFETY: the fixture's only heap writer is stopped. Every header was
        // allocated here and its fixed-size backing remains alive until join;
        // subsequent builtin writes cannot resize or replace an owned span.
        let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
        // SAFETY: the same-heap fixture retains these initialized headers and
        // backings through the explicit worker join. Repetition only increases
        // coverage of the same admitted slots, without creating extra readers.
        unsafe {
            MappedVeclikeScanSnapshot::capture(
                &world,
                headers
                    .repeat(REPEATS)
                    .into_iter()
                    .map(|address| address as *mut VecLikeHeader),
            )
        }
    };
    let (exited, result) = std::sync::mpsc::channel();
    let job = ConcurrentMarkJob {
        gray: MarkStack::default(),
        claims: ConcurrentClaimJob {
            major: true,
            parity: heap.mark_parity,
            pages: PageSnapshot::BaseSets {
                cons: heap.cons_blocks.iter().map(ConsBlock::base_addr).collect(),
                string: heap
                    .string_arena
                    .pages
                    .iter()
                    .map(|page| page.base_addr())
                    .collect(),
                float: heap
                    .float_arena
                    .pages
                    .iter()
                    .map(|page| page.base_addr())
                    .collect(),
                vector: heap
                    .vector_arena
                    .pages
                    .iter()
                    .map(|page| page.base_addr())
                    .collect(),
                bytecode: heap
                    .bytecode_arena
                    .pages
                    .iter()
                    .map(|page| page.base_addr())
                    .collect(),
            },
            dump_lo: heap.dump_addr_lo,
            dump_hi: heap.dump_addr_hi,
            drop_dump_children: false,
            str_claimed: Arc::new(AtomicUsize::new(0)),
            float_claimed: Arc::new(AtomicUsize::new(0)),
            vec_claimed: Arc::new(AtomicUsize::new(0)),
            bc_claimed: Arc::new(AtomicUsize::new(0)),
            subr_dropped: Arc::new(AtomicUsize::new(0)),
        },
        satb: heap.satb_shared.clone(),
        deferred: heap.deferred_veclikes.clone(),
        done: Arc::new(AtomicBool::new(false)),
        // The flat first-partition scan always completes before the loop
        // honors stop, so the job terminates without an idle wait.
        stop: Arc::new(AtomicBool::new(true)),
        wake: Arc::new((Mutex::new(()), Condvar::new())),
        exited,
        obarray: None,
        vectors: None,
        mapped_cons_ranges: None,
        mapped_veclikes: Some(mapped_scan),
    };
    let start = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let worker_start = start.clone();
        let worker = scope.spawn(move || {
            worker_start.wait();
            run_concurrent_mark(job);
        });
        start.wait();
        for ordinal in 0..REPEATS {
            let value = TaggedValue::fixnum(ordinal as i64);
            // char0 writes the existing depth-3 slot; nil writes the default
            // field. No allocation, backing replacement, or topology change
            // is possible after the start handshake.
            set_range(table, TaggedValue::fixnum(0), value);
            set_range(table, TaggedValue::NIL, value);
        }
        worker.join().expect("the confined marker must finish");
    });
    let result = result.recv().expect("the marker must return its handoff");
    assert!(result.symbols.contains(&purpose_id));
    assert!(result.symbols.contains(&sentinel_id));
    assert!(result.promo.is_empty());
    assert!(heap.satb_snapshotted_owners.contains(&table.bits()));
    let leaf_bits = *headers.last().unwrap() | crate::tagged::value::TAG_VECLIKE;
    assert!(heap.satb_snapshotted_owners.contains(&leaf_bits));
    assert_eq!(
        builtin_char_table_range(vec![table, TaggedValue::fixnum(0)], None).unwrap(),
        TaggedValue::fixnum((REPEATS - 1) as i64),
    );
    assert_eq!(
        builtin_char_table_range(vec![table, TaggedValue::NIL], None).unwrap(),
        TaggedValue::fixnum((REPEATS - 1) as i64),
    );
    assert_eq!(
        builtin_char_table_range(vec![table, TaggedValue::fixnum(1)], None).unwrap(),
        sentinel,
    );
    finish_fixture_epoch(&mut heap, table, result);
}

#[test]
fn concurrent_subchartable_snapshot_retains_mapped_span_after_builtin_cow() {
    // This simulates a pdump slot span without a pdump dependency. Declaring
    // the image first also keeps it alive beyond the heap's ordinary teardown.
    let original = vec![TaggedValue::fixnum(70); 128];
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let table = TaggedValue::make_char_table(TaggedValue::NIL, TaggedValue::fixnum(-1), 0);
    set_range(table, TaggedValue::fixnum(0), TaggedValue::fixnum(0));
    let headers = fixture_headers(table);
    let leaf = *headers.last().unwrap() as *mut SubCharTableObj;
    // SAFETY: the stopped fixture created this depth-3 leaf in this heap.
    // It has not published any reader; the initialized immutable 128-word
    // image remains alive through snapshot scan, explicit join, and heap drop.
    unsafe {
        assert_eq!((*leaf).depth, 3);
        assert_eq!((*leaf).min_char, 0);
        assert_eq!((&(*leaf).contents).len(), original.len());
        (*leaf).contents = LispValueVec::mapped(original.as_ptr(), original.len());
    }
    heap.concurrent_begin();
    heap.concurrent_mark_running = true;
    set_tagged_heap(&mut heap);
    let snapshot = {
        // SAFETY: the sole fixture writer is stopped; there is no callback or
        // active reader, and the heap owns the supplied leaf header.
        let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
        // SAFETY: the immutable image span is retained after its header changes
        // to owned storage. The worker must decode only this captured span.
        unsafe { MappedVeclikeScanSnapshot::capture(&world, [leaf.cast()]) }
    };
    let start = Arc::new(Barrier::new(2));
    let (completed, handoff) = std::sync::mpsc::channel();
    let observed = std::thread::scope(|scope| {
        let worker_start = start.clone();
        let worker = scope.spawn(move || {
            worker_start.wait();
            let mut bits = Vec::new();
            // SAFETY: this is the fixture's one reader, before explicit join.
            // The original mapping is immutable and retained by the owner;
            // builtin COW changes only the mutator's header/new owned buffer.
            unsafe {
                snapshot.scan(|item| match item {
                    MappedVeclikeScanItem::Child(value) => {
                        assert!(value.is_fixnum(), "this image contains no traced children");
                        bits.push(value.bits());
                    }
                    MappedVeclikeScanItem::Deferred(_) => panic!("leaf span must be captured"),
                });
            }
            // This reader scanned only immediate words: it made no header
            // claims, queued no child, and owes no symbol/promotion record.
            // Publish completion only after its last captured-span load.
            completed
                .send(ConcurrentMarkResult::default())
                .expect("publish the retained-span reader handoff");
            bits
        });
        // Promote after capture but before permitting the first reader load.
        // A tracer that accidentally rereads the current enum would observe
        // 100 rather than the captured mapping's 70, regardless of scheduling.
        set_range(table, TaggedValue::fixnum(0), TaggedValue::fixnum(100));
        // SAFETY: the worker is still waiting at the barrier; the sole owner
        // can inspect its metadata before releasing that one snapshot reader.
        assert!(unsafe { (*leaf).contents.is_owned() });
        start.wait();
        for ordinal in 1..REPEATS {
            set_range(
                table,
                TaggedValue::fixnum(0),
                TaggedValue::fixnum(100 + ordinal as i64),
            );
        }
        worker.join().expect("the retained-span reader must finish")
    });
    assert_eq!(
        observed,
        original
            .iter()
            .map(|value| value.bits())
            .collect::<Vec<_>>()
    );
    // SAFETY: the only reader has explicitly joined. The owner can now inspect
    // its live storage metadata; builtin COW must have detached this header.
    assert!(unsafe { (*leaf).contents.is_owned() });
    assert_eq!(
        builtin_char_table_range(vec![table, TaggedValue::fixnum(0)], None).unwrap(),
        TaggedValue::fixnum(100 + (REPEATS - 1) as i64),
    );
    assert_eq!(original[0], TaggedValue::fixnum(70));
    let leaf_bits = leaf as usize | crate::tagged::value::TAG_VECLIKE;
    assert!(heap.satb_snapshotted_owners.contains(&leaf_bits));
    finish_fixture_epoch(
        &mut heap,
        table,
        handoff.recv().expect("the retained-span reader completed"),
    );
}
