//! The actual worker must finish every child of a freshly claimed Tier-H owner.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::intern::intern_uninterned;
use crate::emacs_core::value::{
    HashKey, HashTableTest, HashTableWeakness, LambdaParams, LispHashTable,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn heap(generational: bool) -> Box<TaggedHeap> {
    knobs::set_concurrent_claims_for_test(Some(true));
    let mut heap = Box::new(TaggedHeap::new());
    knobs::set_concurrent_claims_for_test(None);
    heap.generational.enabled = generational;
    set_tagged_heap(&mut heap);
    heap
}

fn finish_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.incremental_drain_all();
    heap.incremental_finish(heap.live_bytes(), Instant::now());
    heap.finish_incremental_sweep_now();
}

fn ordinary_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !heap.concurrent_mark_done() {
        assert!(Instant::now() < deadline, "Tier-H worker did not finish");
        std::thread::yield_now();
    }
    finish_cycle(heap, roots);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SymbolAncestor {
    Cons,
    Bytecode,
    Vector,
}

fn transitive_bare_symbol_cycle(claims: bool, generational: bool, ancestor: SymbolAncestor) {
    // The ordinary helper freezes claims ON. Construct both policies explicitly
    // without changing process environment while nextest runs other tests.
    knobs::set_concurrent_claims_for_test(Some(claims));
    let mut heap = Box::new(TaggedHeap::new());
    knobs::set_concurrent_claims_for_test(None);
    heap.generational.enabled = generational;
    set_tagged_heap(&mut heap);

    let symbol = TaggedValue::from_sym_id(intern_uninterned("u35-transitive-worker-weak-key"));
    let id = symbol.as_symbol_id().unwrap();
    let child = match ancestor {
        SymbolAncestor::Cons => heap.alloc_cons(symbol, TaggedValue::NIL),
        SymbolAncestor::Bytecode => {
            let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
            function.constants = vec![symbol].into();
            heap.alloc_bytecode(function)
        }
        SymbolAncestor::Vector => heap.alloc_vector(vec![symbol]),
    };
    let mut strong = LispHashTable::new(HashTableTest::Eq);
    strong.insert(HashKey::Int(1), TaggedValue::fixnum(1), child);
    let strong = heap.alloc_hash_table(strong);
    let mut weak = LispHashTable::new(HashTableTest::Eq);
    weak.weakness = Some(HashTableWeakness::Key);
    weak.insert(
        symbol.to_hash_key(&HashTableTest::Eq),
        symbol,
        TaggedValue::fixnum(35),
    );
    let weak = heap.alloc_hash_table(weak);
    let roots = [strong, weak];

    heap.concurrent_begin();
    for &root in &roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !heap.concurrent_mark_done() {
        assert!(
            Instant::now() < deadline,
            "transitive-symbol worker did not finish"
        );
        std::thread::yield_now();
    }
    // No mutator drain has run. In ON cycles both ancestors are already marked
    // by the actual worker, so root reseeding cannot repair a lost symbol edge.
    assert_eq!(heap.is_value_marked(strong), claims);
    assert_eq!(heap.is_value_marked(child), claims);
    if claims {
        assert_eq!(
            heap.concurrent_hash_claimed()
                .unwrap()
                .load(Ordering::Relaxed),
            1,
        );
        match ancestor {
            SymbolAncestor::Bytecode => {
                assert_eq!(heap.concurrent_bc_claimed.load(Ordering::Relaxed), 1);
            }
            SymbolAncestor::Vector => {
                assert_eq!(heap.concurrent_vec_claimed.load(Ordering::Relaxed), 1);
            }
            SymbolAncestor::Cons => {}
        }
    } else {
        assert!(heap.concurrent_hash_snapshot().is_none());
        assert!(heap.concurrent_hash_claimed().is_none());
    }
    finish_cycle(&mut heap, &roots);
    assert!(
        heap.marked_symbols.contains(id),
        "claims={claims} gen={generational} ancestor={ancestor:?}"
    );
    assert_eq!(
        weak.as_hash_table()
            .unwrap()
            .data
            .get(&symbol.to_hash_key(&HashTableTest::Eq))
            .copied(),
        Some(TaggedValue::fixnum(35)),
        "claims={claims} gen={generational} ancestor={ancestor:?}",
    );

    if ancestor == SymbolAncestor::Vector {
        // Tier-B captures even an unrooted owned vector's backing. Remove this
        // edge while GC is inactive so the next blanket scan cannot retain the
        // symbol independently of the now-unrooted strong hash.
        assert!(!heap.concurrent_mark_running);
        assert!(crate::tagged::mutate::set_vector_slot(
            child,
            0,
            TaggedValue::NIL,
        ));
    }
    // A later parity with only the weak table rooted must release both the
    // transitive symbol edge and its unrooted owning heap objects.
    ordinary_cycle(&mut heap, &[weak]);
    assert!(!heap.marked_symbols.contains(id));
    assert_eq!(weak.as_hash_table().unwrap().data.len(), 0);
    assert!(!heap.owns_heap_value_for_test(strong));
    assert!(!heap.owns_heap_value_for_test(child));
}

#[test]
fn tier_h_actual_worker_transitive_cons_symbol_keeps_only_strongly_held_weak_keys() {
    for claims in [false, true] {
        for generational in [false, true] {
            transitive_bare_symbol_cycle(claims, generational, SymbolAncestor::Cons);
        }
    }
}

#[test]
fn tier_h_actual_worker_transitive_bytecode_symbol_keeps_only_strongly_held_weak_keys() {
    for claims in [false, true] {
        for generational in [false, true] {
            transitive_bare_symbol_cycle(claims, generational, SymbolAncestor::Bytecode);
        }
    }
}

#[test]
fn tier_h_actual_worker_transitive_vector_symbol_keeps_only_strongly_held_weak_keys() {
    for claims in [false, true] {
        for generational in [false, true] {
            transitive_bare_symbol_cycle(claims, generational, SymbolAncestor::Vector);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PauseAt {
    BeforeClaim,
    AfterFirstWord,
}

struct PausedWorker {
    reached: std::sync::mpsc::Receiver<()>,
    resume: std::sync::mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

impl PausedWorker {
    fn wait(&self) {
        self.reached
            .recv_timeout(Duration::from_secs(10))
            .expect("worker did not reach its hash claim rendezvous");
    }

    fn resume_and_stop(self, heap: &TaggedHeap) {
        heap.gc_stop.store(true, Ordering::Release);
        self.resume.send(()).unwrap();
        self.thread.join().expect("Tier-H worker panicked");
    }
}

fn paused_worker(
    heap: &mut TaggedHeap,
    owner: TaggedValue,
    other_roots: &[TaggedValue],
    pause_at: PauseAt,
) -> PausedWorker {
    paused_worker_with_policy(
        heap,
        owner,
        other_roots,
        pause_at,
        concurrent_hash::HashTableScanPolicy::CloneUntilTraced,
    )
}

fn paused_worker_with_policy(
    heap: &mut TaggedHeap,
    owner: TaggedValue,
    other_roots: &[TaggedValue],
    pause_at: PauseAt,
    policy: concurrent_hash::HashTableScanPolicy,
) -> PausedWorker {
    heap.concurrent_begin();
    heap.close_alloc_regions();
    let mut snapshot = concurrent_hash::HashTableScanSnapshot::with_policy(1, policy);
    // SAFETY: a complete live owned Box, captured with this mutator stopped.
    // The heap and worker retain the snapshot through reader join.
    assert!(unsafe {
        snapshot.capture_owned(
            owner.as_veclike_ptr().unwrap() as usize,
            heap.collection_scope(),
        )
    });
    let snapshot = Arc::new(snapshot);
    heap.set_concurrent_hash_snapshot(Some(snapshot.clone()));
    let (exited, result) = std::sync::mpsc::channel();
    let mut pages = heap.page_snapshot_for_mark();
    let leaves = heap.leaf_page_snapshot_for_mark(&mut pages);
    let job = ConcurrentMarkJob {
        // Pop the owner first so its first fresh claim reaches the latch.
        gray: other_roots.iter().copied().chain([owner]).collect(),
        claims: ConcurrentClaimJob {
            parity: heap.mark_parity,
            major: heap.generational.major_in_progress,
            pages,
            dump_lo: heap.dump_addr_lo,
            dump_hi: heap.dump_addr_hi,
            drop_dump_children: false,
            str_claimed: heap.concurrent_str_claimed.clone(),
            float_claimed: heap.concurrent_float_claimed.clone(),
            vec_claimed: heap.concurrent_vec_claimed.clone(),
            bc_claimed: heap.concurrent_bc_claimed.clone(),
            subr_dropped: heap.concurrent_subr_dropped.clone(),
        },
        satb: heap.satb_shared.clone(),
        deferred: heap.deferred_veclikes.clone(),
        done: heap.gc_done.clone(),
        stop: heap.gc_stop.clone(),
        wake: heap.gc_wake.clone(),
        exited,
        obarray: None,
        vectors: None,
        mapped_cons_ranges: None,
        mapped_veclikes: None,
    };
    let job = EnabledConcurrentMarkJob {
        job,
        claims: EnabledClaims {
            leaves,
            leaf_claimed: heap.concurrent_leaf_claimed().cloned(),
            hashes: Some(snapshot),
            hash_claimed: heap.concurrent_hash_claimed().cloned(),
        },
    };
    heap.concurrent_mark_running = true;
    heap.gc_exited = Some(result);
    set_tagged_heap(heap);
    let (reached, receiver) = std::sync::mpsc::channel();
    let (resume, continue_scan) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let hook: Box<dyn FnOnce()> = Box::new(move || {
            reached.send(()).expect("hash test receiver dropped");
            continue_scan
                .recv_timeout(Duration::from_secs(10))
                .expect("hash test failed to release its worker rendezvous");
        });
        match pause_at {
            PauseAt::BeforeClaim => {
                CONCURRENT_HASH_BEFORE_CLAIM_HOOK.with(|slot| {
                    assert!(slot.borrow().is_none());
                    *slot.borrow_mut() = Some(hook);
                });
            }
            PauseAt::AfterFirstWord => {
                CONCURRENT_HASH_SCAN_HOOK.with(|slot| {
                    assert!(slot.borrow().is_none());
                    *slot.borrow_mut() = Some(hook);
                });
            }
        }
        run_concurrent_mark_enabled(job);
        CONCURRENT_HASH_BEFORE_CLAIM_HOOK.with(|slot| assert!(slot.borrow().is_none()));
        CONCURRENT_HASH_SCAN_HOOK.with(|slot| assert!(slot.borrow().is_none()));
    });
    PausedWorker {
        reached: receiver,
        resume,
        thread,
    }
}

#[test]
fn tier_h_actual_worker_stop_mid_scan_hands_off_every_child_before_termination() {
    // The worker's stop quantum is 512; a larger record population makes it
    // exit with residual gray work instead of simply draining before stop.
    const CHILDREN: usize = 4096;
    for generational in [false, true] {
        let mut heap = heap(generational);
        let records: Vec<_> = (0..CHILDREN)
            .map(|i| heap.alloc_record(vec![TaggedValue::fixnum(i as i64)]))
            .collect();
        let symbol = TaggedValue::from_sym_id(intern_uninterned("u35-worker-stop-callback"));
        let mut table = LispHashTable::new(HashTableTest::Eq);
        for (i, &record) in records.iter().enumerate() {
            let key = TaggedValue::fixnum(i as i64);
            table.insert(key.to_hash_key(&HashTableTest::Eq), key, record);
        }
        // Callback words are scanned after every entry; this also proves the
        // worker's symbol result survives a stop issued at the first key word.
        table.user_cmp_function = Some(symbol);
        table.user_hash_function = Some(symbol);
        let owner = heap.alloc_hash_table(table);
        let mut weak = LispHashTable::new(HashTableTest::Eq);
        weak.weakness = Some(HashTableWeakness::Key);
        weak.insert(
            symbol.to_hash_key(&HashTableTest::Eq),
            symbol,
            TaggedValue::fixnum(35),
        );
        let weak = heap.alloc_hash_table(weak);

        let worker = paused_worker(&mut heap, owner, &[weak], PauseAt::AfterFirstWord);
        worker.wait();
        // No later child has been routed when this stop is requested.
        worker.resume_and_stop(&heap);

        assert!(heap.is_value_marked(owner));
        assert_eq!(
            heap.concurrent_hash_claimed()
                .unwrap()
                .load(Ordering::Relaxed),
            1,
        );
        let deferred = heap.deferred_veclikes.lock().unwrap();
        let mut remaining: FxHashSet<_> = records.iter().map(|record| record.bits()).collect();
        for value in deferred.iter() {
            remaining.remove(&value.bits());
        }
        assert!(
            remaining.is_empty(),
            "a claimed snapshot lost children after stop"
        );
        assert_eq!(deferred.len(), CHILDREN + 1, "records plus weak owner");
        drop(deferred);
        for &record in &records {
            assert!(
                !heap.is_value_marked(record),
                "records must reach the drain as white children"
            );
        }

        // The actual receiver and shared deferred buffer are folded by the
        // normal join. Reseeding the already-marked owner cannot repair an
        // incomplete hash scan, so survival proves its complete child handoff.
        finish_cycle(&mut heap, &[owner, weak]);
        assert!(heap.marked_symbols.contains(symbol.as_symbol_id().unwrap()));
        assert_eq!(weak.as_hash_table().unwrap().data.len(), 1);
        for &record in &records {
            assert!(heap.owns_heap_value_for_test(record));
            if generational {
                assert!(heap.value_is_old_for_test(record));
            }
        }
        if generational {
            assert!(heap.value_is_old_for_test(owner));
        }
        ordinary_cycle(&mut heap, &[weak]);
        assert!(!heap.owns_heap_value_for_test(owner));
        assert_eq!(weak.as_hash_table().unwrap().data.len(), 0);
        for record in records {
            assert!(!heap.owns_heap_value_for_test(record));
        }
    }
}

#[test]
fn tier_h_actual_worker_first_write_before_and_after_claim_preserves_both_child_sets() {
    for generational in [false, true] {
        for pause_at in [PauseAt::BeforeClaim, PauseAt::AfterFirstWord] {
            let mut heap = heap(generational);
            let old_children: Vec<_> = (0..64)
                .map(|i| heap.alloc_record(vec![TaggedValue::fixnum(1700 + i)]))
                .collect();
            let old_callback = heap.alloc_record(vec![TaggedValue::fixnum(1801)]);
            // These preexisting white children are deliberately absent from
            // every explicit root and from the start snapshot's child set.
            let inserted = heap.alloc_record(vec![TaggedValue::fixnum(1802)]);
            let new_callback = heap.alloc_record(vec![TaggedValue::fixnum(1803)]);
            let mut table = LispHashTable::new(HashTableTest::Eq);
            for (i, &child) in old_children.iter().enumerate() {
                let key = TaggedValue::fixnum(i as i64);
                table.insert(key.to_hash_key(&HashTableTest::Eq), key, child);
            }
            table.user_cmp_function = Some(old_callback);
            let owner = heap.alloc_hash_table(table);
            let original = owner
                .as_hash_table()
                .unwrap()
                .data
                .concurrent_slots_snapshot();
            // Inspect the live typed fixture before the worker starts. A
            // snapshot read would consume its sole AVAILABLE reader lease.
            let expected_words: Vec<_> = owner
                .as_hash_table()
                .unwrap()
                .data
                .entries_in_slot_order()
                .flat_map(|entry| [entry.key, entry.value])
                .chain([old_callback])
                .collect();
            let mut worker = Some(paused_worker(&mut heap, owner, &[], pause_at));
            worker.as_ref().unwrap().wait();
            assert_eq!(
                heap.is_value_marked(owner),
                pause_at == PauseAt::AfterFirstWord
            );
            for child in [inserted, new_callback] {
                assert!(!heap.is_value_marked(child));
            }
            let snapshot = heap.concurrent_hash_snapshot().unwrap().clone();
            let owner_addr = owner.as_veclike_ptr().unwrap() as usize;
            if pause_at == PauseAt::AfterFirstWord {
                // The running reader holds the mutation mutex. Release and
                // stop it before mutating on this thread; DONE then proves
                // that reserve/clear can change the original without copying.
                worker.take().unwrap().resume_and_stop(&heap);
                assert!(snapshot.get(owner_addr).unwrap().reader_done());
            }

            assert!(
                crate::tagged::mutate::with_hash_table_mut(owner, |table| {
                    assert_eq!(table.data.remove(&HashKey::Int(0)), Some(old_children[0]));
                    // Reuse the free live slot, then force allocation growth
                    // beyond the captured 64-slot backing.
                    table.insert(HashKey::Int(0), TaggedValue::fixnum(0), inserted);
                    table.data.reserve(8192);
                    table.user_cmp_function = Some(new_callback);
                })
                .is_some()
            );
            assert!(
                crate::tagged::mutate::with_hash_table_mut(owner, |table| {
                    table.data.clear();
                    table.insert(HashKey::Int(99), TaggedValue::fixnum(99), inserted);
                })
                .is_some()
            );

            let entry = heap.current_concurrent_hash_mutator();
            let state = entry.lock().unwrap();
            let retired = usize::from(pause_at == PauseAt::BeforeClaim);
            assert_eq!(
                state.retired_hash_buffers.len(),
                retired,
                "{generational:?} {pause_at:?}"
            );
            assert_eq!(state.written_hash_owners, [owner]);
            if retired == 1 {
                assert_eq!(state.retired_hash_buffers[0].as_ptr(), original.0);
                assert_eq!(state.retired_hash_buffers[0].len(), original.1);
                let retired_words: Vec<_> = state.retired_hash_buffers[0]
                    .iter()
                    .flatten()
                    .flat_map(|entry| [entry.key, entry.value])
                    .chain([old_callback])
                    .collect();
                assert_eq!(retired_words, expected_words);
                assert_ne!(
                    owner
                        .as_hash_table()
                        .unwrap()
                        .data
                        .concurrent_slots_snapshot()
                        .0,
                    original.0
                );
            }
            drop(state); // Release per-mutator log lease before join/retrace.
            assert!(!heap.gc_locks_poisoned());
            if let Some(worker) = worker.take() {
                worker.resume_and_stop(&heap);
            }
            assert!(snapshot.get(owner_addr).unwrap().reader_done());
            heap.join_concurrent_mark();
            assert_eq!(heap.sweep_stats().last_concurrent_hash_claimed, 1);
            assert!(heap.concurrent_hash_snapshot().is_some());
            assert_eq!(
                heap.current_concurrent_hash_mutator()
                    .lock()
                    .unwrap()
                    .retired_hash_buffers
                    .len(),
                retired
            );
            assert!(
                !heap.is_value_marked(inserted),
                "the snapshot must not fabricate the inserted edge"
            );
            assert!(!heap.is_value_marked(new_callback));

            heap.reseed_runtime_and_remembered_roots();
            heap.seed_root(owner);
            heap.incremental_drain_all();
            for child in [inserted, new_callback, old_callback] {
                assert!(heap.is_value_marked(child), "{generational:?} {pause_at:?}");
            }
            for &child in &old_children {
                assert!(heap.is_value_marked(child));
            }
            // Pre-admission copies remain owned through termination, even
            // though the sole reader has published DONE. The after-scan path
            // has no retired allocation to retain or release.
            assert!(heap.concurrent_hash_snapshot().is_some());
            assert_eq!(
                heap.current_concurrent_hash_mutator()
                    .lock()
                    .unwrap()
                    .retired_hash_buffers
                    .len(),
                retired
            );
            if retired == 1 {
                assert_eq!(
                    heap.current_concurrent_hash_mutator()
                        .lock()
                        .unwrap()
                        .retired_hash_buffers[0]
                        .as_ptr(),
                    original.0
                );
            }
            drop(snapshot);
            heap.incremental_finish(heap.live_bytes(), Instant::now());
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
            heap.finish_incremental_sweep_now();
            for child in old_children
                .iter()
                .copied()
                .chain([old_callback, inserted, new_callback])
            {
                assert!(heap.owns_heap_value_for_test(child));
                if generational {
                    assert!(heap.value_is_old_for_test(child));
                }
            }
            ordinary_cycle(&mut heap, &[]);
            assert!(!heap.owns_heap_value_for_test(owner));
            for child in old_children
                .into_iter()
                .chain([old_callback, inserted, new_callback])
            {
                assert!(!heap.owns_heap_value_for_test(child));
            }
        }
    }
}

#[test]
fn tier_h_actual_worker_deferred_first_write_keeps_header_white_and_both_child_sets_live() {
    for generational in [false, true] {
        let mut heap = heap(generational);
        let removed = heap.alloc_record(vec![TaggedValue::fixnum(1901)]);
        let inserted = heap.alloc_record(vec![TaggedValue::fixnum(1902)]);
        let old_callback = heap.alloc_record(vec![TaggedValue::fixnum(1903)]);
        let new_callback = heap.alloc_record(vec![TaggedValue::fixnum(1904)]);
        let mut table = LispHashTable::new(HashTableTest::Eq);
        table.insert(HashKey::Int(1), TaggedValue::fixnum(1), removed);
        table.user_cmp_function = Some(old_callback);
        let owner = heap.alloc_hash_table(table);
        let worker = paused_worker_with_policy(
            &mut heap,
            owner,
            &[],
            PauseAt::BeforeClaim,
            concurrent_hash::HashTableScanPolicy::DeferWrites,
        );
        worker.wait();
        assert!(!heap.is_value_marked(owner));
        let snapshot = heap.concurrent_hash_snapshot().unwrap().clone();
        let address = owner.as_veclike_ptr().unwrap() as usize;
        assert!(
            crate::tagged::mutate::with_hash_table_mut(owner, |table| {
                assert_eq!(table.data.remove(&HashKey::Int(1)), Some(removed));
                table.insert(HashKey::Int(1), TaggedValue::fixnum(1), inserted);
                table.data.reserve(8192);
                table.data.clear();
                table.insert(HashKey::Int(2), TaggedValue::fixnum(2), inserted);
                table.user_cmp_function = Some(new_callback);
            })
            .is_some()
        );
        assert!(snapshot.get(address).unwrap().is_deferred());
        assert!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .retired_hash_buffers
                .is_empty()
        );
        assert_eq!(
            heap.current_concurrent_hash_mutator()
                .lock()
                .unwrap()
                .written_hash_owners,
            [owner]
        );
        worker.resume_and_stop(&heap);
        assert!(!heap.is_value_marked(owner));
        assert!(
            snapshot
                .lock_reader(snapshot.get(address).unwrap())
                .is_none()
        );
        assert_eq!(
            heap.concurrent_hash_claimed()
                .unwrap()
                .load(Ordering::Relaxed),
            0,
        );
        assert!(heap.deferred_veclikes.lock().unwrap().contains(&owner));

        // No reader ever touches the changed backing. SATB retains removed
        // words, while the deferred owner/current-child retrace retains writes.
        drop(snapshot);
        finish_cycle(&mut heap, &[owner]);
        for child in [removed, inserted, old_callback, new_callback] {
            assert!(heap.owns_heap_value_for_test(child));
        }
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
        ordinary_cycle(&mut heap, &[]);
        assert!(!heap.owns_heap_value_for_test(owner));
        for child in [removed, inserted, old_callback, new_callback] {
            assert!(!heap.owns_heap_value_for_test(child));
        }
    }
}

fn worker_hash_table(child: TaggedValue) -> LispHashTable {
    let mut table = LispHashTable::new(HashTableTest::Eq);
    table.insert(HashKey::Int(1), TaggedValue::fixnum(1), child);
    table
}

#[test]
fn tier_h_builtin_cold_mutation_paths_keep_deleted_and_inserted_children_live() {
    use crate::emacs_core::builtins::collections::{
        builtin_clrhash, builtin_puthash_with_symbols, builtin_remhash_values,
    };
    use concurrent_hash::HashTableScanPolicy::{CloneUntilTraced, DeferWrites};

    for generational in [false, true] {
        for policy in [CloneUntilTraced, DeferWrites] {
            let mut heap = heap(generational);
            let removed = heap.alloc_record(vec![TaggedValue::fixnum(2301)]);
            let inserted = heap.alloc_record(vec![TaggedValue::fixnum(2302)]);
            let mut table = LispHashTable::new(HashTableTest::Eq);
            table.insert(HashKey::Int(1), TaggedValue::fixnum(1), removed);
            let owner = heap.alloc_hash_table(table);
            let worker =
                paused_worker_with_policy(&mut heap, owner, &[], PauseAt::BeforeClaim, policy);
            worker.wait();
            assert!(concurrent_hash_mutation_active());
            assert_eq!(
                builtin_remhash_values(TaggedValue::fixnum(1), owner, false).unwrap(),
                TaggedValue::NIL,
            );
            assert_eq!(
                builtin_puthash_with_symbols(vec![TaggedValue::fixnum(2), inserted, owner], false)
                    .unwrap(),
                inserted,
            );
            assert_eq!(builtin_clrhash(vec![owner]).unwrap(), owner);
            assert_eq!(
                builtin_puthash_with_symbols(vec![TaggedValue::fixnum(3), inserted, owner], false)
                    .unwrap(),
                inserted,
            );
            // JSON creates post-start owners. Its active insertion clone must
            // tolerate a snapshot miss without fabricating a retired owner.
            let input = heap.alloc_string(crate::heap_types::LispString::from_utf8(
                "{\"member\":{\"nested\":7}}",
            ));
            let parsed = crate::emacs_core::json::builtin_json_parse_string(vec![input]).unwrap();
            let nested = parsed
                .as_hash_table()
                .unwrap()
                .data
                .entries_in_slot_order()
                .next()
                .unwrap()
                .value;
            assert_eq!(
                nested
                    .as_hash_table()
                    .unwrap()
                    .data
                    .entries_in_slot_order()
                    .next()
                    .unwrap()
                    .value,
                TaggedValue::fixnum(7)
            );
            let entry = heap.current_concurrent_hash_mutator();
            let state = entry.lock().unwrap();
            assert_eq!(state.written_hash_owners, [owner]);
            assert_eq!(
                state.retired_hash_buffers.len(),
                usize::from(policy == CloneUntilTraced)
            );
            drop(state);
            worker.resume_and_stop(&heap);
            finish_cycle(&mut heap, &[owner, parsed]);
            assert!(!concurrent_hash_mutation_active());
            for child in [removed, inserted, nested] {
                assert!(heap.owns_heap_value_for_test(child));
            }
            assert_eq!(
                owner.as_hash_table().unwrap().data.get(&HashKey::Int(3)),
                Some(&inserted)
            );
            ordinary_cycle(&mut heap, &[]);
            for child in [owner, parsed, removed, inserted, nested] {
                assert!(!heap.owns_heap_value_for_test(child));
            }
        }
    }
}

fn wait_for_hash_worker(heap: &TaggedHeap) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !heap.concurrent_mark_done() {
        assert!(Instant::now() < deadline, "Tier-H worker did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn tier_h_exact_inventory_tracks_current_boxes_and_ordinary_old_owners() {
    for generational in [false, true] {
        // This image-shaped fixture is owned outside the heap and must outlive
        // its mapped registry. It has no ordinary-Box ownership membership.
        let mut mapped = Box::new(HashTableObj {
            header: VecLikeHeader::new(VecLikeType::HashTable),
            table: worker_hash_table(TaggedValue::fixnum(2000)),
        });
        let mut heap = heap(generational);
        let mapped_address = &mut mapped.header as *mut VecLikeHeader as usize;
        // SAFETY: this complete image-shaped Box outlives the heap registry.
        unsafe {
            heap.register_mapped_veclike_object(
                mapped_address as *mut VecLikeHeader,
                size_of::<HashTableObj>(),
            );
        }
        // A newly registered image requires its normal first-partition trace
        // and blackening. Complete that bootstrap before ordinary allocation:
        // these later Boxes must become ordinary-old, not image permanents.
        heap.collect_exact(std::iter::empty());
        assert!(heap.dump_blackened);
        assert!(!heap.is_partition_first_cycle());
        let child = heap.alloc_record(vec![TaggedValue::fixnum(2001)]);
        let strong = heap.alloc_hash_table(worker_hash_table(child));
        let dead_child = heap.alloc_record(vec![TaggedValue::fixnum(2002)]);
        let dead = heap.alloc_hash_table(worker_hash_table(dead_child));
        let dead_address = dead.as_veclike_ptr().unwrap() as usize;
        let non_hash = heap.alloc_bool_vector(1, vec![1]);
        let mut weak_table = worker_hash_table(TaggedValue::fixnum(2003));
        weak_table.weakness = Some(HashTableWeakness::Key);
        let weak = heap.alloc_hash_table(weak_table);
        let mut pending_table = LispHashTable::new(HashTableTest::Eq);
        pending_table.set_pending_dump_entries(vec![(
            HashKey::Int(1),
            TaggedValue::fixnum(2004),
            Some(TaggedValue::fixnum(1)),
        )]);
        let pending = heap.alloc_hash_table(pending_table);
        assert!(heap.concurrent_hash_mutators().next().is_none());
        let roots = [strong, non_hash, weak, pending];
        heap.concurrent_begin();
        for &root in &roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        let snapshot = heap.concurrent_hash_snapshot().unwrap().clone();
        assert_eq!(snapshot.len(), 2, "rooted and unrooted strong owned Boxes");
        for owner in [strong, dead] {
            assert!(
                snapshot
                    .get(owner.as_veclike_ptr().unwrap() as usize)
                    .is_some()
            );
        }
        for owner in [non_hash, weak, pending] {
            assert!(
                snapshot
                    .get(owner.as_veclike_ptr().unwrap() as usize)
                    .is_none()
            );
        }
        assert!(!heap.non_cons_object_addrs.contains(&mapped_address));
        assert!(snapshot.get(mapped_address).is_none());

        // An address can join the authoritative live inventory after launch
        // without gaining membership in the current immutable scan snapshot.
        let post_start = heap.alloc_hash_table(worker_hash_table(TaggedValue::fixnum(2005)));
        let post_address = post_start.as_veclike_ptr().unwrap() as usize;
        assert!(heap.non_cons_object_addrs.contains(&post_address));
        assert!(snapshot.get(post_address).is_none());
        wait_for_hash_worker(&heap);
        assert!(heap.is_value_marked(strong));
        assert!(
            !heap.is_value_marked(dead),
            "capture must not root dead owners"
        );
        drop(snapshot);
        let roots = [strong, non_hash, weak, pending, post_start];
        finish_cycle(&mut heap, &roots);
        assert!(!heap.non_cons_object_addrs.contains(&dead_address));
        assert!(!heap.owns_heap_value_for_test(dead));
        assert!(!heap.owns_heap_value_for_test(dead_child));
        assert!(heap.owns_heap_value_for_test(child));
        if generational {
            assert!(heap.value_is_old_for_test(strong));
            assert!(!heap.generational.old_objects.is_null());
        }

        heap.concurrent_begin();
        for &root in &roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        let snapshot = heap.concurrent_hash_snapshot().unwrap();
        assert!(
            snapshot
                .get(strong.as_veclike_ptr().unwrap() as usize)
                .is_some()
        );
        assert!(snapshot.get(post_address).is_some());
        assert!(snapshot.get(dead_address).is_none());
        assert!(snapshot.get(mapped_address).is_none());
        wait_for_hash_worker(&heap);
        assert!(heap.is_value_marked(strong));
        finish_cycle(&mut heap, &roots);
        assert!(heap.concurrent_hash_mutators().next().is_none());
        ordinary_cycle(&mut heap, &[]);
        assert!(!heap.owns_heap_value_for_test(strong));
        assert!(!heap.owns_heap_value_for_test(child));
        assert!(!heap.owns_heap_value_for_test(post_start));
        assert!(heap.concurrent_hash_snapshot().is_none());
        drop(heap);
        // `mapped` drops only after all heap-side users have disappeared.
    }
}

#[test]
fn tier_h_exact_inventory_includes_boxes_from_independent_allocation_states() {
    use super::super::mutator_gc::MutatorGcState;

    let mut heap = heap(true);
    let mut states = Vec::new();
    let mut roots = Vec::new();
    for i in 0..3 {
        // Model three independent allocation-state owners. Ordinary Box
        // inventory is coordinator-owned, so switching the current state
        // must not hide any previously allocated table from capture.
        assert_eq!(heap.current_mutator_gc().allocated_count, 0);
        let non_hash = heap.alloc_bool_vector(1, vec![1]);
        let owner = heap.alloc_hash_table(worker_hash_table(TaggedValue::fixnum(2100 + i)));
        roots.extend([non_hash, owner]);
        let state = std::mem::replace(heap.current_mutator_gc_mut(), MutatorGcState::new());
        assert!(heap.concurrent_hash_mutators().next().is_none());
        states.push(state);
    }
    assert_eq!(heap.non_cons_object_addrs.len(), 6);
    // The current runtime installs one state. Combine only its accounting so
    // normal collection remains valid; do not create a hash registry union.
    for state in states {
        let current = heap.current_mutator_gc_mut();
        current.allocated_count += state.allocated_count;
        current.bytes_since_gc += state.bytes_since_gc;
        current.bytes_banked_at_resets += state.bytes_banked_at_resets;
        for (current_count, count) in current
            .memory_use_counts
            .iter_mut()
            .zip(state.memory_use_counts)
        {
            *current_count += count;
        }
    }
    assert!(heap.concurrent_hash_mutators().next().is_none());
    heap.concurrent_begin();
    for &root in &roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    let snapshot = heap.concurrent_hash_snapshot().unwrap();
    assert_eq!(snapshot.len(), 3);
    for &owner in roots.iter().skip(1).step_by(2) {
        assert!(
            snapshot
                .get(owner.as_veclike_ptr().unwrap() as usize)
                .is_some()
        );
    }
    wait_for_hash_worker(&heap);
    assert_eq!(
        heap.concurrent_hash_claimed()
            .unwrap()
            .load(Ordering::Relaxed),
        3,
    );
    finish_cycle(&mut heap, &roots);
    ordinary_cycle(&mut heap, &[]);
    assert!(heap.non_cons_object_addrs.is_empty());
}
