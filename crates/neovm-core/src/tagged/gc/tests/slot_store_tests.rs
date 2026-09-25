//! Record and closure slot stores (`mutate::set_record_slot`,
//! `mutate::set_closure_slot`) are atomic, as vector slot stores are: a
//! thread reading the slots with atomic loads while the mutator stores sees
//! whole values and is not a data race (TSan checks that directly), and the
//! stores keep their semantics and their barrier through a concurrent mark.

use super::*;
use crate::tagged::header::{LambdaObj, MacroObj, RecordObj, load_value_atomic};
use crate::tagged::mutate::{set_closure_slot, set_record_slot};

fn slot_base(value: TaggedValue) -> (*const TaggedValue, usize) {
    let ptr = value.as_veclike_ptr().expect("a veclike");
    // SAFETY: a live record, lambda or macro; all three keep their slots in
    // a `LispValueVec` named `data`.
    unsafe {
        match value.veclike_type() {
            Some(VecLikeType::Record) => {
                let data = &(*(ptr as *const RecordObj)).data;
                (data.as_slice().as_ptr(), data.len())
            }
            Some(VecLikeType::Lambda) => {
                let data = &(*(ptr as *const LambdaObj)).data;
                (data.as_slice().as_ptr(), data.len())
            }
            Some(VecLikeType::Macro) => {
                let data = &(*(ptr as *const MacroObj)).data;
                (data.as_slice().as_ptr(), data.len())
            }
            other => panic!("not a slot object: {other:?}"),
        }
    }
}

fn slot(value: TaggedValue, index: usize) -> TaggedValue {
    let (base, len) = slot_base(value);
    assert!(index < len);
    // SAFETY: in bounds of a live object's owned slots.
    load_value_atomic(unsafe { &*base.add(index) })
}

#[test]
fn slot_stores_keep_their_semantics() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let record = heap.alloc_record(vec![TaggedValue::NIL; 3]);
    let lambda = heap.alloc_lambda(vec![TaggedValue::NIL; 4]);
    let macro_ = heap.alloc_macro(vec![TaggedValue::NIL; 4]);
    for (owner, len) in [(record, 3), (lambda, 4), (macro_, 4)] {
        let set = |i, v| {
            if owner.veclike_type() == Some(VecLikeType::Record) {
                set_record_slot(owner, i, v)
            } else {
                set_closure_slot(owner, i, v)
            }
        };
        assert!(set(len - 1, TaggedValue::fixnum(7)));
        assert_eq!(slot(owner, len - 1), TaggedValue::fixnum(7));
        assert!(!set(len, TaggedValue::fixnum(8)), "out of range");
        assert!(!set(usize::MAX, TaggedValue::fixnum(8)), "out of range");
        for i in 0..len - 1 {
            assert_eq!(slot(owner, i), TaggedValue::NIL, "untouched");
        }
    }
    // The wrong kind is refused.
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    assert!(!set_record_slot(vector, 0, TaggedValue::T));
    assert!(!set_closure_slot(record, 0, TaggedValue::T));
}

/// A reader thread loads every slot atomically while the mutator stores
/// through the setters: whole values only (each is one of the stored
/// fixnums), and no data race — the property the atomic stores exist for.
#[test]
fn slot_stores_race_free_against_an_atomic_reader() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let record = heap.alloc_record(vec![TaggedValue::fixnum(0); 8]);
    let lambda = heap.alloc_lambda(vec![TaggedValue::fixnum(0); 8]);
    let owners = [record, lambda];
    let bases: Vec<(usize, usize)> = owners
        .iter()
        .map(|&owner| {
            let (base, len) = slot_base(owner);
            (base as usize, len)
        })
        .collect();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let passes = std::sync::Arc::new(AtomicUsize::new(0));
    let reader = {
        let stop = stop.clone();
        let passes = passes.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                for &(base, len) in &bases {
                    for i in 0..len {
                        // SAFETY: the owners outlive the thread (joined
                        // below) and their owned slots never reallocate:
                        // single-slot stores do not grow them.
                        let v = load_value_atomic(unsafe { &*(base as *const TaggedValue).add(i) });
                        let n = v.as_fixnum().expect("a whole fixnum");
                        assert!(n >= 0);
                    }
                }
                passes.fetch_add(1, Ordering::Release);
            }
        })
    };
    // At least 1000 rounds, and on until the reader has overlapped some of
    // them (under load it may start late).
    let mut round = 0i64;
    while round < 1000 || passes.load(Ordering::Acquire) < 3 {
        for (k, &owner) in owners.iter().enumerate() {
            let index = (round as usize + k) % 8;
            let stored = if k == 0 {
                set_record_slot(owner, index, TaggedValue::fixnum(round))
            } else {
                set_closure_slot(owner, index, TaggedValue::fixnum(round))
            };
            assert!(stored);
        }
        round += 1;
        if round >= 1000 {
            std::thread::yield_now();
        }
    }
    stop.store(true, Ordering::Release);
    reader.join().expect("reader");
}

/// Through a concurrent mark: a child stored into a record or closure slot
/// mid-cycle survives it (the insertion is covered by the termination), and
/// the value it overwrote survives the cycle too (the barrier logged it).
#[test]
fn slot_stores_during_a_concurrent_mark_keep_both_values() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let old_child = heap.alloc_string(crate::heap_types::LispString::from_utf8("old"));
    let record = heap.alloc_record(vec![old_child, TaggedValue::NIL]);
    let old_closure_child = heap.alloc_vector(vec![TaggedValue::fixnum(3)]);
    let lambda = heap.alloc_lambda(vec![old_closure_child, TaggedValue::NIL, TaggedValue::NIL]);
    let root = heap.alloc_cons(record, lambda);
    heap.collect_exact(std::iter::once(root));
    assert!(heap.should_run_concurrent());

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    let new_child = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let new_closure_child = heap.alloc_string(crate::heap_types::LispString::from_utf8("new"));
    assert!(set_record_slot(record, 0, new_child));
    assert!(set_closure_slot(lambda, 0, new_closure_child));
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    assert_eq!(slot(record, 0), new_child);
    assert_eq!(slot(lambda, 0), new_closure_child);
    assert!(heap.is_value_marked(new_child));
    assert!(heap.owns_string_object(new_closure_child.as_string_ptr().unwrap() as *const u8));
    // The overwritten values survived the cycle (SATB), unreferenced now.
    assert!(heap.owns_string_object(old_child.as_string_ptr().unwrap() as *const u8));
    assert!(heap.owns_veclike_object(old_closure_child.as_veclike_ptr().unwrap() as *const u8));
}
