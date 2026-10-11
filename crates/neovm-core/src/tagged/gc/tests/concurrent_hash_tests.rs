//! Tier-H initialized-word snapshots, per-mutator retirement and real cycles.

use super::super::cold_gc::ConcurrentHashMutatorState;
use super::super::{TaggedHeap, clear_tagged_heap_if_installed, knobs, set_tagged_heap};
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::hashtab::with_user_test_guard;
use crate::emacs_core::intern::intern_uninterned;
use crate::emacs_core::value::{HashKey, HashTableTest, HashTableWeakness, LispHashTable};
use crate::tagged::header::VecLikeHeader;
use std::cell::UnsafeCell;
use std::sync::{Arc, Barrier};

fn table_with_entries(count: usize) -> LispHashTable {
    let mut table = LispHashTable::new(HashTableTest::Eq);
    for i in 0..count {
        table.insert(
            HashKey::Int(i as i64),
            TaggedValue::fixnum(i as i64),
            TaggedValue::fixnum(100 + i as i64),
        );
    }
    table
}

fn boxed_table(table: LispHashTable) -> Box<HashTableObj> {
    Box::new(HashTableObj {
        header: VecLikeHeader::new(VecLikeType::HashTable),
        table,
    })
}

fn address(table: &HashTableObj) -> usize {
    table as *const HashTableObj as usize
}

fn captured_children(snapshot: &HashTableScanSnapshot, owner: usize) -> Vec<usize> {
    let mut children = Vec::new();
    // SAFETY: fixtures retain their original until this single reader lease
    // finishes; every subsequent writer holds the same entry mutex.
    unsafe {
        snapshot.scan_entry(snapshot.get(owner).unwrap(), |value| {
            children.push(value.bits());
        })
    };
    children
}

fn expected_children(table: &LispHashTable) -> Vec<usize> {
    let mut values = Vec::new();
    for entry in table.data.entries_in_slot_order() {
        values.extend([entry.key.bits(), entry.value.bits()]);
    }
    values.extend(
        [table.user_cmp_function, table.user_hash_function]
            .into_iter()
            .flatten()
            .map(TaggedValue::bits),
    );
    values
}

#[test]
fn tier_h_lease_reads_only_initialized_some_fields_and_copied_callbacks() {
    let mut object = boxed_table(table_with_entries(4));
    object.table.data.remove(&HashKey::Int(1));
    object.table.data.remove(&HashKey::Int(3));
    object.table.user_cmp_function = Some(TaggedValue::fixnum(701));
    object.table.user_hash_function = Some(TaggedValue::fixnum(702));
    let owner = address(&object);
    let mut snapshot = HashTableScanSnapshot::new();
    // SAFETY: this Box is complete, exclusively owned and remains live.
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot.slot_count(), 4, "holes remain in the slots Vec");
    assert_eq!(snapshot.initialized_entry_count(), 2);
    assert_eq!(
        snapshot.descriptor_bytes(),
        0,
        "capture stores no field descriptors"
    );
    assert!(
        snapshot.get(owner + 8).is_none(),
        "exact owner addresses only"
    );
    assert_eq!(
        captured_children(&snapshot, owner),
        [0, 100, 2, 102, 701, 702]
            .map(|n| TaggedValue::fixnum(n).bits())
            .to_vec(),
    );
}

#[test]
fn tier_h_start_eligibility_excludes_weak_pending_and_generation_black() {
    let mut weak = boxed_table(table_with_entries(1));
    weak.table.weakness = Some(HashTableWeakness::Key);
    let mut pending = boxed_table(LispHashTable::new(HashTableTest::Eq));
    pending.table.set_pending_dump_entries(vec![(
        HashKey::Int(1),
        TaggedValue::fixnum(2),
        Some(TaggedValue::fixnum(1)),
    )]);
    let mut old = boxed_table(table_with_entries(1));
    old.header.gc.tenured = true;
    let mut permanent = boxed_table(table_with_entries(1));
    permanent.header.gc.make_permanent();
    let mut snapshot = HashTableScanSnapshot::new();
    for object in [&weak, &pending, &old, &permanent] {
        // SAFETY: the four exclusive Boxes are complete and remain live.
        assert!(!unsafe { snapshot.capture_owned(address(object), CollectionScope::Young) });
    }
    assert_eq!(snapshot.len(), 0);
    // SAFETY: the old Box remains exclusively owned and live.
    assert!(unsafe { snapshot.capture_owned(address(&old), CollectionScope::Full) });
    assert!(!unsafe { snapshot.capture_owned(address(&permanent), CollectionScope::Full) });
    assert!(!unsafe { snapshot.capture_owned(address(&old), CollectionScope::Full) });
    pending.table.hydrate_pending();
    weak.table.weakness = None;
    assert!(snapshot.get(address(&pending)).is_none());
    assert!(snapshot.get(address(&weak)).is_none());
}

#[test]
fn tier_h_first_write_retires_once_through_resize_clear_and_whole_replacement() {
    let mut object = boxed_table(table_with_entries(3));
    object.table.user_cmp_function = Some(TaggedValue::fixnum(801));
    object.table.user_hash_function = Some(TaggedValue::fixnum(802));
    let owner = address(&object);
    let original = object.table.data.concurrent_slots_snapshot();
    let mut snapshot = HashTableScanSnapshot::new();
    // SAFETY: the exclusive Box and its eventual retirement log remain live.
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let expected = expected_children(&object.table);
    let entry = snapshot.get(owner).unwrap();
    let mut mutator = ConcurrentHashMutatorState::new();
    {
        let mut guard = snapshot.lock_mutation(owner).unwrap();
        assert!(
            guard.clone_on_first_write(&mut object.table.data, &mut mutator.retired_hash_buffers,)
        );
        assert_eq!(entry.cow.load(Ordering::Acquire), READY);
        if guard.claim_dirty() {
            mutator
                .written_hash_owners
                .push(unsafe { TaggedValue::from_veclike_ptr(&object.header) });
        }
        assert!(!guard.claim_dirty());
        object.table.data.reserve(1000);
        object.table.data.remove(&HashKey::Int(1));
        object.table.insert(
            HashKey::Int(9),
            TaggedValue::fixnum(9),
            TaggedValue::fixnum(109),
        );
    }
    {
        let mut guard = snapshot.lock_mutation(owner).unwrap();
        assert!(
            !guard.clone_on_first_write(&mut object.table.data, &mut mutator.retired_hash_buffers,)
        );
        object.table.data.clear();
        object.table = table_with_entries(200);
        object.table.user_cmp_function = Some(TaggedValue::fixnum(901));
    }
    assert_eq!(mutator.retired_hash_buffers.len(), 1);
    assert_eq!(mutator.retired_hash_buffers[0].as_ptr(), original.0);
    assert_eq!(mutator.retired_hash_buffers[0].len(), original.1);
    assert_eq!(mutator.written_hash_owners.len(), 1);
    assert_eq!(captured_children(&snapshot, owner), expected);
    drop(snapshot);
    // The admitted reader has finished before retired allocations are dropped.
    mutator.retired_hash_buffers.clear();
}

#[test]
fn tier_h_exact_box_inventory_needs_no_per_mutator_address_registry() {
    let mut heap = real_heap(false);
    let owners: Vec<_> = (1..=3)
        .map(|count| heap.alloc_hash_table(table_with_entries(count)))
        .collect();
    let addresses: Vec<_> = owners
        .iter()
        .map(|owner| owner.as_veclike_ptr().unwrap() as usize)
        .collect();
    let non_hash = heap.alloc_bool_vector(1, vec![1]);
    for address in &addresses {
        assert!(heap.non_cons_object_addrs.contains(address));
    }
    assert!(heap.concurrent_hash_mutators().next().is_none());
    let mut roots = owners.clone();
    roots.push(non_hash);
    start_cycle(&mut heap, &roots);
    {
        let snapshot = heap.concurrent_hash_snapshot().unwrap();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot.initialized_entry_count(), 6);
        for &owner in &owners {
            assert!(
                snapshot
                    .get(owner.as_veclike_ptr().unwrap() as usize)
                    .is_some()
            );
        }
        assert!(
            snapshot
                .get(non_hash.as_veclike_ptr().unwrap() as usize)
                .is_none()
        );
    }
    finish_cycle(&mut heap, &roots);
    // No table has been mutated, so neither allocation, capture nor free
    // needs a per-mutator log entry. The canonical inventory drops dead Boxes.
    roots.remove(1);
    start_cycle(&mut heap, &roots);
    finish_cycle(&mut heap, &roots);
    assert!(!heap.non_cons_object_addrs.contains(&addresses[1]));
    assert!(heap.concurrent_hash_mutators().next().is_none());
    start_cycle(&mut heap, &roots);
    let snapshot = heap.concurrent_hash_snapshot().unwrap();
    assert_eq!(snapshot.len(), 2);
    assert!(snapshot.get(addresses[1]).is_none());
    finish_cycle(&mut heap, &roots);
    clear_tagged_heap_if_installed(&mut heap);
}

/// The fixture's live payload is accessed only under its captured entry lock.
/// The reader never accesses this cell: its lease reads the captured allocation.
struct GuardedTable(UnsafeCell<Box<HashTableObj>>);
unsafe impl Send for GuardedTable {}
unsafe impl Sync for GuardedTable {}

#[test]
fn tier_h_n_writers_retire_once_while_atomic_reader_keeps_original_words() {
    const WRITERS: usize = 4;
    let table = GuardedTable(UnsafeCell::new(boxed_table(table_with_entries(32))));
    // SAFETY: no threads have started; this is the stopped-world capture.
    let owner = unsafe { address(&*table.0.get()) };
    let mut snapshot = HashTableScanSnapshot::new();
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let expected = unsafe { expected_children(&(*table.0.get()).table) };
    let snapshot = Arc::new(snapshot);
    let table = Arc::new(table);
    let start = Arc::new(Barrier::new(WRITERS + 1));
    let (copied, copy_finished) = std::sync::mpsc::sync_channel(1);
    let logs = std::thread::scope(|scope| {
        let mut writers = Vec::new();
        let mut reader_finished = Vec::new();
        for writer in 0..WRITERS {
            let snapshot = snapshot.clone();
            let table = table.clone();
            let start = start.clone();
            let copied = copied.clone();
            let (finished, read_finished) = std::sync::mpsc::sync_channel(1);
            reader_finished.push(finished);
            writers.push(scope.spawn(move || {
                let mut mutator = ConcurrentHashMutatorState::new();
                start.wait();
                for iteration in 0..200 {
                    let mut guard = snapshot.lock_mutation(owner).unwrap();
                    // SAFETY: this entry guard serializes every fixture writer;
                    // no shared live-payload reference escapes its activation.
                    let object = unsafe { &mut *table.0.get() };
                    let _preimage_count = object.table.data.entries_in_slot_order().count();
                    if guard.clone_on_first_write(
                        &mut object.table.data,
                        &mut mutator.retired_hash_buffers,
                    ) {
                        copied.send(()).unwrap();
                    }
                    if guard.claim_dirty() {
                        mutator
                            .written_hash_owners
                            .push(unsafe { TaggedValue::from_veclike_ptr(&object.header) });
                    }
                    let key = (writer * 200 + iteration) as i64;
                    object.table.insert(
                        HashKey::Int(key),
                        TaggedValue::fixnum(key),
                        TaggedValue::fixnum(key + 5000),
                    );
                }
                // Keep each original slots allocation on its writer until the
                // reader has completed. Only plain counts cross the join;
                // neither local Values nor their retirement buffers move.
                if let Err(error) = read_finished.recv_timeout(std::time::Duration::from_secs(10)) {
                    // A failed completion wait cannot prove the reader ended.
                    // Retain its possible backing before failing this fixture.
                    std::mem::forget(mutator.retired_hash_buffers);
                    panic!("reader did not finish before writer retirement: {error}");
                }
                (
                    mutator.retired_hash_buffers.len(),
                    mutator.written_hash_owners.len(),
                )
            }));
        }
        let reader_snapshot = snapshot.clone();
        let reader_start = start.clone();
        let reader = scope.spawn(move || {
            reader_start.wait();
            // Force one writer to retire before reader admission; subsequent
            // concurrent writers race through their own retirement/dirty logs.
            copy_finished
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            assert_eq!(captured_children(&reader_snapshot, owner), expected);
            assert!(reader_snapshot.get(owner).unwrap().reader_done());
            assert!(
                reader_snapshot
                    .lock_reader(reader_snapshot.get(owner).unwrap())
                    .is_none()
            );
            for finished in reader_finished {
                finished.send(()).unwrap();
            }
        });
        let logs: Vec<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        reader.join().unwrap();
        logs
    });
    assert_eq!(logs.iter().map(|log| log.0).sum::<usize>(), 1);
    assert_eq!(logs.iter().map(|log| log.1).sum::<usize>(), 1);
    assert!(!snapshot.is_poisoned());
    drop(snapshot);
    drop(logs);
}

#[test]
fn tier_h_unwind_poison_does_not_publish_incomplete_cow_as_ready() {
    let object = boxed_table(table_with_entries(1));
    let owner = address(&object);
    let mut snapshot = HashTableScanSnapshot::new();
    // SAFETY: the exclusive Box remains live for the whole fixture.
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let entry = snapshot.get(owner).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = snapshot.lock_mutation(owner).unwrap();
        entry
            .cow
            .compare_exchange(UNTOUCHED, COPYING, Ordering::AcqRel, Ordering::Acquire)
            .unwrap();
        panic!("interrupt retirement before READY");
    }));
    assert!(result.is_err());
    assert_eq!(entry.cow.load(Ordering::Acquire), COPYING);
    assert!(snapshot.is_poisoned());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            snapshot.lock_mutation(owner);
        }))
        .is_err()
    );
}

#[test]
fn tier_h_stop_requested_during_fresh_claim_still_scans_every_captured_word() {
    let object = boxed_table(table_with_entries(32));
    let owner = address(&object);
    let mut snapshot = HashTableScanSnapshot::new();
    // SAFETY: this exclusive fixture keeps its original allocation alive.
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let expected = expected_children(&object.table);
    let parity = super::super::MarkParity::One;
    object.header.gc.set_marked(parity.flip());
    let mut reader = snapshot.lock_reader(snapshot.get(owner).unwrap()).unwrap();
    assert!(object.header.gc.mark_claim_at(parity));
    let stop = AtomicBool::new(false);
    let mut seen = Vec::new();
    // SAFETY: a fresh claim must finish its leased traversal even after
    // the callback requests the worker's next outer-loop stop check.
    unsafe {
        reader.scan(|value| {
            seen.push(value.bits());
            if seen.len() == expected.len() / 2 {
                stop.store(true, Ordering::Release);
            }
        });
    }
    reader.finish();
    drop(reader);
    assert!(stop.load(Ordering::Acquire));
    assert_eq!(seen, expected);
    assert!(snapshot.get(owner).unwrap().reader_done());
}

#[test]
fn tier_h_done_mutation_copies_only_the_legacy_policy() {
    for policy in [
        HashTableScanPolicy::AlwaysClone,
        HashTableScanPolicy::CloneUntilTraced,
        HashTableScanPolicy::DeferWrites,
    ] {
        let mut object = boxed_table(table_with_entries(3));
        let owner = address(&object);
        let expected = expected_children(&object.table);
        let mut snapshot = HashTableScanSnapshot::unbound_with_policy(1, policy);
        assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
        assert_eq!(captured_children(&snapshot, owner), expected);
        let mut mutator = ConcurrentHashMutatorState::new();
        {
            let mut guard = snapshot.lock_mutation(owner).unwrap();
            assert_eq!(
                guard.clone_on_first_write(
                    &mut object.table.data,
                    &mut mutator.retired_hash_buffers
                ),
                policy == HashTableScanPolicy::AlwaysClone,
            );
            assert!(guard.claim_dirty());
            object.table.data.clear();
            object.table.data.reserve(2048);
            object.table = table_with_entries(200);
        }
        let entry = snapshot.get(owner).unwrap();
        assert!(entry.reader_done());
        assert!(!entry.is_deferred());
        // DONE forbids another read even when resize/replacement freed the
        // original allocation. The test never dereferences its stale pointer.
        assert!(snapshot.lock_reader(entry).is_none());
        assert_eq!(
            mutator.retired_hash_buffers.len(),
            usize::from(policy == HashTableScanPolicy::AlwaysClone)
        );
        assert!(!snapshot.is_poisoned());
    }
}

#[test]
fn tier_h_first_writer_defers_before_reader_admission_without_retirement() {
    let mut object = boxed_table(table_with_entries(4));
    let owner = address(&object);
    let parity = super::super::MarkParity::One;
    object.header.gc.set_marked(parity.flip());
    let mut snapshot =
        HashTableScanSnapshot::unbound_with_policy(1, HashTableScanPolicy::DeferWrites);
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let mut mutator = ConcurrentHashMutatorState::new();
    {
        let mut guard = snapshot.lock_mutation(owner).unwrap();
        assert!(
            snapshot.get(owner).unwrap().is_deferred(),
            "election precedes the preimage barrier"
        );
        assert!(
            !guard.clone_on_first_write(&mut object.table.data, &mut mutator.retired_hash_buffers)
        );
        assert!(guard.claim_dirty());
        object.table.data.clear();
        object.table.data.reserve(2048);
        let mut replacement = table_with_entries(2);
        replacement.weakness = Some(HashTableWeakness::Key);
        replacement.user_cmp_function = Some(TaggedValue::fixnum(901));
        object.table = replacement;
    }
    let entry = snapshot.get(owner).unwrap();
    assert!(entry.is_deferred());
    assert!(snapshot.lock_reader(entry).is_none());
    assert!(
        !object.header.gc.is_marked_at(parity),
        "deferral cannot claim the header"
    );
    assert_eq!(entry.cow.load(Ordering::Acquire), UNTOUCHED);
    assert!(mutator.retired_hash_buffers.is_empty());
    assert!(!snapshot.is_poisoned());
}

#[test]
fn tier_h_n_deferred_writers_admit_one_dirty_owner_and_no_retirement() {
    const WRITERS: usize = 4;
    let table = GuardedTable(UnsafeCell::new(boxed_table(table_with_entries(4))));
    // SAFETY: every writer starts after this stopped-world capture.
    let owner = unsafe { address(&*table.0.get()) };
    let mut snapshot =
        HashTableScanSnapshot::unbound_with_policy(1, HashTableScanPolicy::DeferWrites);
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let snapshot = Arc::new(snapshot);
    let table = Arc::new(table);
    let start = Arc::new(Barrier::new(WRITERS));
    let logs = std::thread::scope(|scope| {
        let mut writers = Vec::new();
        for index in 0..WRITERS {
            let snapshot = snapshot.clone();
            let table = table.clone();
            let start = start.clone();
            writers.push(scope.spawn(move || {
                let mut mutator = ConcurrentHashMutatorState::new();
                start.wait();
                for iteration in 0..32 {
                    let mut guard = snapshot.lock_mutation(owner).unwrap();
                    // SAFETY: every live-payload access holds the shared guard.
                    let object = unsafe { &mut *table.0.get() };
                    assert!(snapshot.get(owner).unwrap().is_deferred());
                    assert!(!guard.clone_on_first_write(
                        &mut object.table.data,
                        &mut mutator.retired_hash_buffers,
                    ));
                    if guard.claim_dirty() {
                        mutator
                            .written_hash_owners
                            .push(unsafe { TaggedValue::from_veclike_ptr(&object.header) });
                    }
                    let key = (index * 32 + iteration) as i64;
                    object.table.data.reserve(32);
                    object.table.insert(
                        HashKey::Int(key),
                        TaggedValue::fixnum(key),
                        TaggedValue::fixnum(key + 5000),
                    );
                }
                // A deferred entry cannot admit a reader, so local retirement
                // buffers may end here. Return only the exact count evidence.
                (
                    mutator.retired_hash_buffers.len(),
                    mutator.written_hash_owners.len(),
                )
            }));
        }
        writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(logs.iter().map(|(retired, _)| retired).sum::<usize>(), 0);
    assert_eq!(logs.iter().map(|(_, dirty)| dirty).sum::<usize>(), 1);
    let entry = snapshot.get(owner).unwrap();
    assert!(entry.is_deferred());
    assert!(snapshot.lock_reader(entry).is_none());
    assert!(!snapshot.is_poisoned());
}

#[test]
fn tier_h_reader_lease_blocks_writers_until_every_word_and_callback_is_routed() {
    for policy in [
        HashTableScanPolicy::CloneUntilTraced,
        HashTableScanPolicy::DeferWrites,
    ] {
        let mut object = boxed_table(table_with_entries(16));
        object.table.user_cmp_function = Some(TaggedValue::fixnum(910));
        object.table.user_hash_function = Some(TaggedValue::fixnum(911));
        let owner = address(&object);
        let expected = expected_children(&object.table);
        let table = Arc::new(GuardedTable(UnsafeCell::new(object)));
        let mut snapshot = HashTableScanSnapshot::unbound_with_policy(1, policy);
        assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
        let snapshot = Arc::new(snapshot);
        let (paused, read_paused) = std::sync::mpsc::sync_channel(1);
        let (release, read_release) = std::sync::mpsc::sync_channel(1);
        let (attempted, write_attempted) = std::sync::mpsc::sync_channel(1);
        let (acquired, write_acquired) = std::sync::mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let reader_snapshot = snapshot.clone();
            let reader = scope.spawn(move || {
                let entry = reader_snapshot.get(owner).unwrap();
                let mut lease = reader_snapshot.lock_reader(entry).unwrap();
                let mut seen = Vec::new();
                unsafe {
                    lease.scan(|child| {
                        seen.push(child.bits());
                        if seen.len() == 1 {
                            paused.send(()).unwrap();
                            read_release
                                .recv_timeout(std::time::Duration::from_secs(10))
                                .unwrap();
                        }
                    });
                }
                lease.finish();
                seen
            });
            read_paused
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            assert_eq!(
                snapshot.get(owner).unwrap().reader.load(Ordering::Acquire),
                READING
            );
            assert!(snapshot.get(owner).unwrap().mutation.try_lock().is_err());
            let writer_snapshot = snapshot.clone();
            let writer_table = table.clone();
            let writer = scope.spawn(move || {
                attempted.send(()).unwrap();
                let mut guard = writer_snapshot.lock_mutation(owner).unwrap();
                acquired.send(()).unwrap();
                let mut mutator = ConcurrentHashMutatorState::new();
                // SAFETY: the same captured entry mutex excludes the reader
                // and every other writer from this live payload activation.
                let object = unsafe { &mut *writer_table.0.get() };
                assert!(!guard.clone_on_first_write(
                    &mut object.table.data,
                    &mut mutator.retired_hash_buffers
                ));
                assert!(guard.claim_dirty());
                object.table.data.reserve(2048);
                object.table.data.clear();
                object.table = table_with_entries(200);
                // Reader completion precedes this writer's lock admission;
                // local buffers can end here without any backing reader.
                mutator.retired_hash_buffers.len()
            });
            write_attempted
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            assert!(matches!(
                write_acquired.recv_timeout(std::time::Duration::from_millis(20)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ));
            release.send(()).unwrap();
            assert_eq!(reader.join().unwrap(), expected);
            write_acquired
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            assert_eq!(writer.join().unwrap(), 0);
        });
        assert!(snapshot.get(owner).unwrap().reader_done());
        assert!(!snapshot.is_poisoned());
    }
}

#[test]
fn tier_h_incomplete_and_unwound_reader_leases_poison_the_cycle() {
    for unwind in [false, true] {
        let object = boxed_table(table_with_entries(2));
        let owner = address(&object);
        let mut snapshot = HashTableScanSnapshot::new();
        assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
        if unwind {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut reader = snapshot.lock_reader(snapshot.get(owner).unwrap()).unwrap();
                unsafe { reader.scan(|_| panic!("interrupt a claimed table's scan")) };
            }));
            assert!(result.is_err());
        } else {
            drop(snapshot.lock_reader(snapshot.get(owner).unwrap()).unwrap());
        }
        assert!(snapshot.is_poisoned());
        assert!(!snapshot.get(owner).unwrap().reader_done());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || snapshot.lock_mutation(owner)
            ))
            .is_err()
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || snapshot.lock_reader(snapshot.get(owner).unwrap())
            ))
            .is_err()
        );
    }
}

#[test]
fn tier_h_failed_header_claim_finishes_without_reading_the_backing() {
    let object = boxed_table(table_with_entries(1));
    let owner = address(&object);
    let parity = super::super::MarkParity::One;
    object.header.gc.set_marked(parity);
    let mut snapshot = HashTableScanSnapshot::new();
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    let mut reader = snapshot.lock_reader(snapshot.get(owner).unwrap()).unwrap();
    assert!(!object.header.gc.mark_claim_at(parity));
    reader.finish();
    drop(reader);
    assert!(snapshot.get(owner).unwrap().reader_done());
    assert!(!snapshot.is_poisoned());
}

fn real_heap(generational: bool) -> Box<TaggedHeap> {
    knobs::set_concurrent_claims_for_test(Some(true));
    let mut heap = Box::new(TaggedHeap::new());
    knobs::set_concurrent_claims_for_test(None);
    heap.generational.enabled = generational;
    assert!(heap.concurrent_claims());
    set_tagged_heap(&mut heap);
    heap
}

fn insert(owner: TaggedValue, key: TaggedValue, value: TaggedValue) {
    assert!(
        crate::tagged::mutate::with_hash_table_mut(owner, |table| {
            table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
        })
        .is_some()
    );
}

fn wait_for_worker(heap: &TaggedHeap) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !heap.concurrent_mark_done() {
        assert!(
            std::time::Instant::now() < deadline,
            "Tier-H worker did not finish"
        );
        std::thread::yield_now();
    }
}

fn start_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    wait_for_worker(heap);
}

fn terminate_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.incremental_drain_all();
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
}

fn finish_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    terminate_cycle(heap, roots);
    heap.finish_incremental_sweep_now();
}

#[test]
fn tier_h_real_wrapper_retraces_done_table_and_releases_any_legacy_retirement_at_finish() {
    for generational in [false, true] {
        let mut heap = real_heap(generational);
        let removed = heap.alloc_record(vec![TaggedValue::fixnum(1001)]);
        let inserted = heap.alloc_record(vec![TaggedValue::fixnum(1002)]);
        let owner = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
        insert(owner, TaggedValue::fixnum(1), removed);
        let mut weak = LispHashTable::new(HashTableTest::Eq);
        weak.weakness = Some(HashTableWeakness::Key);
        weak.insert(
            inserted.to_hash_key(&HashTableTest::Eq),
            inserted,
            TaggedValue::T,
        );
        let weak = heap.alloc_hash_table(weak);
        let roots = [owner, weak];
        let original = owner
            .as_hash_table()
            .unwrap()
            .data
            .concurrent_slots_snapshot();
        start_cycle(&mut heap, &roots);
        assert!(heap.is_value_marked(owner));
        assert!(
            !heap.is_value_marked(inserted),
            "preexisting weak key starts white"
        );
        let snapshot = heap.concurrent_hash_snapshot().unwrap().clone();
        let entry = snapshot
            .get(owner.as_veclike_ptr().unwrap() as usize)
            .unwrap();
        assert!(entry.reader_done());
        assert!(snapshot.lock_reader(entry).is_none());
        let retired_count =
            usize::from(knobs::concurrent_hash_scan_policy() == HashTableScanPolicy::AlwaysClone);
        assert!(
            crate::tagged::mutate::with_hash_table_mut(owner, |table| {
                table.data.remove(&HashKey::Int(1));
                table.data.reserve(2048);
                table.insert(HashKey::Int(2), TaggedValue::fixnum(2), inserted);
            })
            .is_some()
        );
        assert!(
            crate::tagged::mutate::with_hash_table_mut(owner, |table| {
                table.data.remove(&HashKey::Int(2));
                table.data.clear();
                table.insert(HashKey::Int(3), TaggedValue::fixnum(3), inserted);
            })
            .is_some()
        );
        assert_eq!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .len(),
            retired_count
        );
        if retired_count != 0 {
            assert_eq!(
                heap.current_concurrent_hash_mutator()
                    .lock()
                    .unwrap()
                    .retired_hash_buffers[0]
                    .as_ptr(),
                original.0
            );
        }
        assert!(
            snapshot.lock_reader(entry).is_none(),
            "DONE cannot reread a changed backing"
        );
        heap.join_concurrent_mark();
        assert_eq!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .len(),
            retired_count,
            "join does not end any legacy retirement lifetime"
        );
        assert!(heap.concurrent_hash_snapshot().is_some());
        heap.reseed_runtime_and_remembered_roots();
        for root in roots {
            heap.seed_root(root);
        }
        heap.incremental_drain_all();
        assert!(
            heap.is_value_marked(inserted),
            "current-child retrace covers white insertion"
        );
        drop(snapshot);
        heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
        assert!(heap.concurrent_hash_snapshot().is_none());
        assert!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .is_empty()
        );
        assert!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .written_hash_owners
                .is_empty()
        );
        assert!(
            heap.is_value_marked(removed),
            "snapshot child survives this cycle"
        );
        assert_eq!(weak.as_hash_table().unwrap().data.len(), 1);
        heap.finish_incremental_sweep_now();
        crate::tagged::mutate::with_hash_table_mut(owner, |table| table.data.clear());
        start_cycle(&mut heap, &roots);
        finish_cycle(&mut heap, &roots);
        assert!(!heap.owns_heap_value_for_test(removed));
        assert!(!heap.owns_heap_value_for_test(inserted));
        assert_eq!(weak.as_hash_table().unwrap().data.len(), 0);
        clear_tagged_heap_if_installed(&heap);
    }
}

#[test]
fn tier_h_real_demand_trace_does_not_retain_dead_table_children() {
    for generational in [false, true] {
        let mut heap = real_heap(generational);
        let child = heap.alloc_record(vec![TaggedValue::fixnum(1101)]);
        let dead = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
        insert(dead, TaggedValue::fixnum(1), child);
        let kept = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
        start_cycle(&mut heap, &[kept]);
        finish_cycle(&mut heap, &[kept]);
        assert!(!heap.owns_heap_value_for_test(dead));
        assert!(!heap.owns_heap_value_for_test(child));
        assert!(
            !heap
                .non_cons_object_addrs
                .contains(&(dead.as_veclike_ptr().unwrap() as usize))
        );
        clear_tagged_heap_if_installed(&heap);
    }
}

#[test]
fn tier_h_real_weak_and_pending_tables_defer_before_header_claim() {
    for generational in [false, true] {
        for weakness in [
            HashTableWeakness::Key,
            HashTableWeakness::Value,
            HashTableWeakness::KeyOrValue,
            HashTableWeakness::KeyAndValue,
        ] {
            let mut heap = real_heap(generational);
            let key = heap.alloc_record(vec![TaggedValue::fixnum(1201)]);
            let value = heap.alloc_record(vec![TaggedValue::fixnum(1202)]);
            let mut table = LispHashTable::new(HashTableTest::Eq);
            table.weakness = Some(weakness);
            table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
            let weak = heap.alloc_hash_table(table);
            let pending_child = heap.alloc_record(vec![TaggedValue::fixnum(1203)]);
            let mut table = LispHashTable::new(HashTableTest::Eq);
            table.set_pending_dump_entries(vec![(
                HashKey::Int(1),
                pending_child,
                Some(TaggedValue::fixnum(1)),
            )]);
            let pending = heap.alloc_hash_table(table);
            let roots = [weak, pending];
            start_cycle(&mut heap, &roots);
            let snapshot = heap.concurrent_hash_snapshot().unwrap();
            assert!(
                snapshot
                    .get(weak.as_veclike_ptr().unwrap() as usize)
                    .is_none()
            );
            assert!(
                snapshot
                    .get(pending.as_veclike_ptr().unwrap() as usize)
                    .is_none()
            );
            assert!(!heap.is_value_marked(weak));
            assert!(!heap.is_value_marked(pending));
            // Hydration after capture does not retroactively admit a claim.
            assert_eq!(pending.as_hash_table().unwrap().data.len(), 1);
            assert!(
                heap.current_concurrent_hash_mutator()
                    .lock()
                    .unwrap()
                    .retired_hash_buffers
                    .is_empty()
            );
            finish_cycle(&mut heap, &roots);
            assert_eq!(weak.as_hash_table().unwrap().data.len(), 0);
            assert!(heap.owns_heap_value_for_test(pending_child));
            assert!(!heap.owns_heap_value_for_test(key));
            assert!(!heap.owns_heap_value_for_test(value));
            clear_tagged_heap_if_installed(&heap);
        }
    }
}

#[test]
fn tier_h_real_whole_replacement_preserves_old_and_new_callback_children() {
    for generational in [false, true] {
        let mut heap = real_heap(generational);
        let old_cmp = heap.alloc_lambda(vec![TaggedValue::fixnum(1301)]);
        let old_hash = heap.alloc_lambda(vec![TaggedValue::fixnum(1302)]);
        let new_cmp = heap.alloc_lambda(vec![TaggedValue::fixnum(1303)]);
        let new_hash = heap.alloc_lambda(vec![TaggedValue::fixnum(1304)]);
        let mut table = table_with_entries(3);
        table.user_cmp_function = Some(old_cmp);
        table.user_hash_function = Some(old_hash);
        let owner = heap.alloc_hash_table(table);
        start_cycle(&mut heap, &[owner]);
        assert!(!heap.is_value_marked(new_cmp));
        assert!(!heap.is_value_marked(new_hash));
        let mut replacement = table_with_entries(200);
        replacement.user_cmp_function = Some(new_cmp);
        replacement.user_hash_function = Some(new_hash);
        assert!(owner.replace_hash_table(replacement));
        finish_cycle(&mut heap, &[owner]);
        for callback in [old_cmp, old_hash, new_cmp, new_hash] {
            assert!(heap.owns_heap_value_for_test(callback));
        }
        start_cycle(&mut heap, &[owner]);
        finish_cycle(&mut heap, &[owner]);
        assert!(!heap.owns_heap_value_for_test(old_cmp));
        assert!(!heap.owns_heap_value_for_test(old_hash));
        assert!(heap.owns_heap_value_for_test(new_cmp));
        assert!(heap.owns_heap_value_for_test(new_hash));
        clear_tagged_heap_if_installed(&heap);
    }
}

#[test]
fn tier_h_old_table_remembers_young_child_for_next_minor() {
    let mut heap = real_heap(true);
    let owner = heap.alloc_hash_table(table_with_entries(1));
    start_cycle(&mut heap, &[owner]);
    finish_cycle(&mut heap, &[owner]);
    assert!(heap.value_is_old_for_test(owner));
    let child = heap.alloc_record(vec![TaggedValue::fixnum(1401)]);
    insert(owner, TaggedValue::fixnum(2), child);
    assert!(heap.current_mutator_gc().remset.contains(&owner));
    heap.begin_minor_collection();
    heap.seed_root(owner);
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    assert!(heap.owns_heap_value_for_test(child));
    assert!(heap.value_is_old_for_test(child));
    crate::tagged::mutate::with_hash_table_mut(owner, |table| table.data.remove(&HashKey::Int(2)));
    start_cycle(&mut heap, &[owner]);
    finish_cycle(&mut heap, &[owner]);
    assert!(!heap.owns_heap_value_for_test(child));
    clear_tagged_heap_if_installed(&heap);
}

#[test]
fn tier_h_snapshot_symbol_handoff_keeps_only_strongly_held_weak_keys() {
    for generational in [false, true] {
        let mut heap = real_heap(generational);
        let symbol = TaggedValue::from_sym_id(intern_uninterned("u35-tier-h-weak-symbol"));
        let id = symbol.as_symbol_id().unwrap();
        let strong = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
        insert(strong, TaggedValue::fixnum(1), symbol);
        let mut weak = LispHashTable::new(HashTableTest::Eq);
        weak.weakness = Some(HashTableWeakness::Key);
        weak.insert(
            symbol.to_hash_key(&HashTableTest::Eq),
            symbol,
            TaggedValue::fixnum(1501),
        );
        let weak = heap.alloc_hash_table(weak);
        let roots = [strong, weak];
        start_cycle(&mut heap, &roots);
        finish_cycle(&mut heap, &roots);
        assert!(heap.marked_symbols.contains(id));
        assert_eq!(weak.as_hash_table().unwrap().data.len(), 1);
        crate::tagged::mutate::with_hash_table_mut(strong, |table| table.data.clear());
        start_cycle(&mut heap, &roots);
        finish_cycle(&mut heap, &roots);
        assert!(!heap.marked_symbols.contains(id));
        assert_eq!(weak.as_hash_table().unwrap().data.len(), 0);
        clear_tagged_heap_if_installed(&heap);
    }
}

#[test]
fn tier_h_strong_to_weak_replacement_registers_current_weakness_and_callbacks() {
    for generational in [false, true] {
        let mut heap = real_heap(generational);
        let old_key = heap.alloc_record(vec![TaggedValue::fixnum(1601)]);
        let old_value = heap.alloc_record(vec![TaggedValue::fixnum(1602)]);
        let new_key = heap.alloc_record(vec![TaggedValue::fixnum(1603)]);
        let new_value = heap.alloc_record(vec![TaggedValue::fixnum(1604)]);
        let callback = heap.alloc_lambda(vec![TaggedValue::fixnum(1605)]);
        let owner = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
        insert(owner, old_key, old_value);
        start_cycle(&mut heap, &[owner]);
        for white in [new_key, new_value, callback] {
            assert!(!heap.is_value_marked(white));
        }
        let mut replacement = LispHashTable::new(HashTableTest::Eq);
        replacement.weakness = Some(HashTableWeakness::Key);
        replacement.user_cmp_function = Some(callback);
        replacement.insert(new_key.to_hash_key(&HashTableTest::Eq), new_key, new_value);
        assert!(owner.replace_hash_table(replacement));
        finish_cycle(&mut heap, &[owner]);
        assert_eq!(owner.as_hash_table().unwrap().data.len(), 0);
        assert!(
            heap.owns_heap_value_for_test(callback),
            "weak callbacks remain strong"
        );
        assert!(!heap.owns_heap_value_for_test(new_key));
        assert!(!heap.owns_heap_value_for_test(new_value));
        for old in [old_key, old_value] {
            assert!(
                heap.owns_heap_value_for_test(old),
                "old strong snapshot floats this cycle"
            );
        }
        start_cycle(&mut heap, &[owner]);
        finish_cycle(&mut heap, &[owner]);
        for old in [old_key, old_value] {
            assert!(!heap.owns_heap_value_for_test(old));
        }
        assert!(heap.owns_heap_value_for_test(callback));
        clear_tagged_heap_if_installed(&heap);
    }
}

#[test]
fn tier_h_wrapper_unwind_poison_is_observed_and_join_refuses_to_sweep() {
    for generational in [false, true] {
        let mut heap = real_heap(generational);
        let owner = heap.alloc_hash_table(table_with_entries(1));
        start_cycle(&mut heap, &[owner]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_hash_table_mut(owner, |table| {
                table.data.clear();
                panic!("caller escapes while Tier-H guard is held");
            });
        }));
        assert!(result.is_err());
        assert!(heap.gc_locks_poisoned());
        assert!(heap.concurrent_hash_snapshot().unwrap().is_poisoned());
        assert_eq!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .len(),
            usize::from(knobs::concurrent_hash_scan_policy() == HashTableScanPolicy::AlwaysClone)
        );
        assert!(matches!(
            heap.finish_concurrent_mark(),
            Err(super::super::MarkFinishError::PoisonedCollectorState)
        ));
        let join = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            heap.join_concurrent_mark();
        }));
        assert!(join.is_err());
        // The failed finish retains the active state, so nothing can sweep.
        assert!(heap.concurrent_mark_running);
        assert!(!heap.sweep_in_progress);
        clear_tagged_heap_if_installed(&heap);
    }
}

fn assert_user_guard_did_not_enter_tier_h(context: &Context, table: TaggedValue, epoch: u64) {
    let heap = &context.tagged_heap;
    assert!(heap.concurrent_mark_running());
    let snapshot = heap.concurrent_hash_snapshot().unwrap();
    let entry = snapshot
        .get(table.as_veclike_ptr().unwrap() as usize)
        .unwrap();
    assert_eq!(entry.cow.load(Ordering::Acquire), UNTOUCHED);
    assert!(!entry.dirty.load(Ordering::Acquire));
    assert!(!snapshot.is_poisoned());
    assert!(!heap.gc_locks_poisoned());
    assert!(
        heap.current_concurrent_hash_mutator()
            .lock()
            .unwrap()
            .retired_hash_buffers
            .is_empty()
    );
    assert!(
        heap.current_concurrent_hash_mutator()
            .lock()
            .unwrap()
            .written_hash_owners
            .is_empty()
    );
    let current = table.as_hash_table().unwrap();
    assert_eq!(current.data.switch_epoch, epoch);
    assert_eq!(current.data.len(), 1);
}

#[test]
fn tier_h_user_test_guards_inhibit_gc_without_cow_and_restore_after_all_exits() {
    assert!(crate::emacs_core::hashtab::hash_test_parity_enabled());
    for generational in [false, true] {
        // Freeze the policy before Context bootstrap allocates any tables, so
        // the real table enters the chosen Tier-H ownership inventory.
        knobs::set_concurrent_claims_for_test(Some(true));
        let mut context = Context::new();
        knobs::set_concurrent_claims_for_test(None);
        context.tagged_heap.generational.enabled = generational;
        // Bootstrap through the actual Context driver before requesting an
        // automatic concurrent major. This fixture deliberately tests that
        // path even when the process-wide suite requests GC stress elsewhere.
        context.gc_stress = false;
        context.set_gc_threshold(usize::MAX);
        context.gc_collect_exact();
        assert!(context.tagged_heap.should_run_concurrent());
        let table = context
            .eval_str(
                "(progn
                   (setq neovm-u35-guard-table (make-hash-table :test 'eq))
                   (puthash 1 7 neovm-u35-guard-table)
                   neovm-u35-guard-table)",
            )
            .expect("create the Lisp-bound table through the real builtins");
        let gcs_done = context.eval_str("gcs-done").unwrap();
        let gc_count = context.gc_count;
        let epoch = table.as_hash_table().unwrap().data.switch_epoch;
        let inserted = context
            .tagged_heap
            .alloc_record(vec![TaggedValue::fixnum(8091)]);
        // A Context termination skips the old obarray cells because the real
        // start handshake scans their snapshot concurrently. A table-only
        // heap start omits that snapshot and would sweep live Context state.
        if generational {
            context
                .tagged_heap
                .current_mutator_gc_mut()
                .pacing
                .minors_since_major = usize::MAX;
        }
        context.gc_pending = true;
        context.gc_safe_point_exact();
        assert!(context.tagged_heap.concurrent_mark_running());
        assert!(context.tagged_heap.concurrent_obarray_start_slots.is_some());
        wait_for_worker(&context.tagged_heap);
        assert!(context.tagged_heap.is_value_marked(table));
        assert!(!context.tagged_heap.is_value_marked(inserted));

        with_user_test_guard(&mut context, table, |context| {
            assert_eq!(context.gc_inhibit_depth, 1);
            assert!(!table.as_hash_table().unwrap().mutable);
            with_user_test_guard(context, table, |context| {
                assert_eq!(context.gc_inhibit_depth, 1);
                assert_eq!(
                    context
                        .eval_str("(gethash 1 neovm-u35-guard-table)")
                        .unwrap()
                        .as_fixnum(),
                    Some(7),
                );
                assert_user_guard_did_not_enter_tier_h(context, table, epoch);
            });
            assert!(
                context
                    .eval_str("(garbage-collect)")
                    .expect("explicit GC obeys the active user-test inhibition")
                    .is_nil()
            );
            assert_eq!(context.eval_str("gcs-done").unwrap(), gcs_done);
            assert_eq!(context.gc_count, gc_count);
            assert_user_guard_did_not_enter_tier_h(context, table, epoch);
        });

        for operation in [
            "(puthash 1 7 neovm-u35-guard-table)",
            "(puthash 2 9 neovm-u35-guard-table)",
            "(remhash 1 neovm-u35-guard-table)",
            "(remhash 2 neovm-u35-guard-table)",
            "(clrhash neovm-u35-guard-table)",
        ] {
            let result = with_user_test_guard(&mut context, table, |context| {
                assert_eq!(context.gc_inhibit_depth, 1);
                context.eval_str(operation)
            });
            let crate::emacs_core::error::EvalError::Signal { symbol, data, .. } =
                result.expect_err("the guard rejects writes before the mutation wrapper")
            else {
                panic!("expected the GNU mutation signal");
            };
            assert_eq!(symbol, crate::emacs_core::intern::intern("error"));
            assert_eq!(
                data[0].as_str_owned().as_deref(),
                Some("hash table test modifies table")
            );
            assert_eq!(data[1], table);
            assert!(table.as_hash_table().unwrap().mutable);
            assert_eq!(context.gc_inhibit_depth, 0);
            assert!(context.user_test_gc_accounting_pointer().is_none());
            assert_user_guard_did_not_enter_tier_h(&context, table, epoch);
        }

        for exit in [
            "(signal 'error '(u35-guard-signal))",
            "(throw 'u35-guard-exit 23)",
        ] {
            let result =
                with_user_test_guard(&mut context, table, |context| context.eval_str(exit));
            assert!(result.is_err());
            assert!(table.as_hash_table().unwrap().mutable);
            assert_eq!(context.gc_inhibit_depth, 0);
            assert!(context.user_test_gc_accounting_pointer().is_none());
            assert_user_guard_did_not_enter_tier_h(&context, table, epoch);
        }
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_user_test_guard(&mut context, table, |context| {
                assert_eq!(context.gc_inhibit_depth, 1);
                panic!("user callback unwinds without entering the Tier-H wrapper");
            });
        }));
        assert!(unwind.is_err());
        assert!(table.as_hash_table().unwrap().mutable);
        assert_eq!(context.gc_inhibit_depth, 0);
        assert!(context.user_test_gc_accounting_pointer().is_none());
        assert_eq!(context.gc_count, gc_count);
        assert_user_guard_did_not_enter_tier_h(&context, table, epoch);

        // Enter the same native puthash used by Lisp without an evaluator safe
        // point first: that would normally terminate this already-done worker.
        assert_eq!(
            crate::emacs_core::builtins::builtin_puthash_3(
                &mut context,
                TaggedValue::fixnum(2),
                inserted,
                table,
            )
            .expect("legal puthash after every guard exit"),
            inserted,
        );
        assert_eq!(
            context
                .tagged_heap
                .current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .len(),
            usize::from(knobs::concurrent_hash_scan_policy() == HashTableScanPolicy::AlwaysClone)
        );
        assert_eq!(
            context
                .tagged_heap
                .current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .written_hash_owners,
            [table]
        );
        assert_eq!(
            table.as_hash_table().unwrap().data.switch_epoch,
            epoch.wrapping_add(1)
        );
        assert!(!context.tagged_heap.gc_locks_poisoned());
        // The Context driver re-seeds its complete roots before termination;
        // a hand-written table-only sweep would discard bootstrap state.
        context.gc_collect_exact();
        assert!(context.tagged_heap.concurrent_hash_snapshot().is_none());
        assert!(
            context
                .tagged_heap
                .current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .is_empty()
        );
        assert!(
            context
                .tagged_heap
                .current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .written_hash_owners
                .is_empty()
        );
        assert!(context.tagged_heap.owns_heap_value_for_test(inserted));
        assert_eq!(
            context
                .eval_str("(gethash 2 neovm-u35-guard-table)")
                .unwrap(),
            inserted,
        );
        let record = inserted.as_record_data().unwrap();
        assert_eq!(record.len(), 1);
        assert_eq!(record[0], TaggedValue::fixnum(8091));
    }
}

/// A claims heap whose marker is "running" and whose completion this test
/// sends, like the shutdown tests' stalled marker.
fn stalled_claims_mark(
    heap: &mut TaggedHeap,
) -> std::sync::mpsc::Sender<super::super::ConcurrentMarkResult> {
    let (tx, rx) = std::sync::mpsc::channel();
    heap.concurrent_mark_running = true;
    heap.gc_exited = Some(rx);
    tx
}

fn admitted_snapshot(heap: &mut TaggedHeap, owner: usize) -> Arc<HashTableScanSnapshot> {
    // SAFETY: the fixture heap is this test's only writer, and the captured
    // Box outlives every lease and the heap's use of the snapshot.
    let world = unsafe { super::super::scan_contract::SingleMutatorWorld::from_heap(heap) };
    let mut snapshot =
        HashTableScanSnapshot::with_policy(1, HashTableScanPolicy::CloneUntilTraced, &world);
    // SAFETY: a complete, exclusively owned Box, captured under the admission.
    assert!(unsafe { snapshot.capture_owned(owner, CollectionScope::Full) });
    assert_eq!(snapshot.heap_identity(), world.heap_identity());
    Arc::new(snapshot)
}

#[test]
#[should_panic(expected = "Tier-H snapshot belongs to another heap")]
fn tier_h_snapshot_admitted_by_one_heap_is_refused_by_another() {
    let object = boxed_table(table_with_entries(1));
    let mut admitting = TaggedHeap::new_for_concurrent_hash_test(false);
    let mut other = TaggedHeap::new_for_concurrent_hash_test(false);
    let snapshot = admitted_snapshot(&mut admitting, address(&object));
    other.set_concurrent_hash_snapshot(Some(snapshot));
}

#[test]
fn poisoned_tier_h_protocol_fails_finish_before_state_changes() {
    let object = boxed_table(table_with_entries(1));
    let owner = address(&object);
    let mut heap = TaggedHeap::new_for_concurrent_hash_test(false);
    let snapshot = admitted_snapshot(&mut heap, owner);
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = snapshot.lock_mutation(owner).unwrap();
        snapshot
            .get(owner)
            .unwrap()
            .cow
            .compare_exchange(UNTOUCHED, COPYING, Ordering::AcqRel, Ordering::Acquire)
            .unwrap();
        panic!("interrupt a Tier-H retirement");
    }));
    assert!(interrupted.is_err());
    assert!(snapshot.is_poisoned());
    heap.set_concurrent_hash_snapshot(Some(snapshot));
    let tx = stalled_claims_mark(&mut heap);
    tx.send(Default::default()).unwrap();
    assert!(matches!(
        heap.finish_concurrent_mark(),
        Err(super::super::MarkFinishError::PoisonedCollectorState)
    ));
    assert!(
        heap.concurrent_mark_running(),
        "failure retains active state"
    );
    assert!(heap.concurrent_hash_snapshot().is_some());
}

#[test]
fn poisoned_tier_h_dirty_log_fails_finish_before_state_changes() {
    let mut heap = TaggedHeap::new_for_concurrent_hash_test(false);
    let entry = heap.current_concurrent_hash_mutator();
    let poison = std::panic::catch_unwind(|| {
        let _held = entry.lock().unwrap();
        panic!("poison a Tier-H mutator log");
    });
    assert!(poison.is_err());
    let tx = stalled_claims_mark(&mut heap);
    tx.send(Default::default()).unwrap();
    assert!(matches!(
        heap.finish_concurrent_mark(),
        Err(super::super::MarkFinishError::PoisonedCollectorState)
    ));
    assert!(
        heap.concurrent_mark_running(),
        "failure retains active state"
    );
}

#[test]
fn abandoned_marker_retains_retired_tier_h_original() {
    let mut heap = TaggedHeap::new_for_concurrent_hash_test(false);
    let tx = stalled_claims_mark(&mut heap);
    let original = vec![Some(HashTableEntry {
        key: TaggedValue::fixnum(41),
        value: TaggedValue::fixnum(42),
    })];
    let retired = original.clone();
    let retired_address = retired.as_ptr();
    heap.current_concurrent_hash_mutator()
        .lock()
        .unwrap()
        .retired_hash_buffers
        .push(retired);
    drop(heap);
    // SAFETY: active-marker abandonment retained the retired original with
    // the claims state; this reader runs after owner destruction.
    let retained = unsafe { std::slice::from_raw_parts(retired_address, original.len()) };
    assert_eq!(
        retained[0].map(|e| (e.key, e.value)),
        original[0].map(|e| (e.key, e.value))
    );
    assert!(tx.send(Default::default()).is_err());
}
