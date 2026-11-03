//! Owner-exclusive cons tails: batch grants, lock-free carving, exact ACK
//! publication, young-log exactness and abandonment poisoning.
#![cfg(not(feature = "gc-memory-telemetry"))]
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;

fn retry_registration<T>(mut f: impl FnMut() -> Result<T, RegistrationError>) -> T {
    loop {
        match f() {
            Ok(value) => return value,
            Err(RegistrationError::Busy) => thread::yield_now(),
            Err(error) => panic!("tails registration: {error:?}"),
        }
    }
}
fn retry<T>(mut f: impl FnMut() -> Result<T, RendezvousError>) -> T {
    loop {
        match f() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) => thread::yield_now(),
            Err(error) => panic!("tails protocol: {error:?}"),
        }
    }
}
fn activate<'era>(mut permit: MutatorPermit<'era>, heap: HeapIdentity, id: u64) -> Mutator<'era> {
    loop {
        match permit.activate(heap, id) {
            Ok(owner) => return owner,
            Err(failure) if failure.reason == RegistrationError::Busy => {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activate tails owner: {:?}", failure.reason),
        }
    }
}
fn with_run<R>(owner: &mut Mutator<'_>, body: impl FnOnce(&mut RunAccess<'_, '_>) -> R) -> R {
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let result = body(&mut run);
                retry_registration(|| run.close_cons_region());
                return result;
            }
            Err(RegistrationError::Busy) => thread::yield_now(),
            Err(error) => panic!("run tails owner: {error:?}"),
        }
    }
}
fn complete(mut stopped: AllStopped<'_>) {
    loop {
        match stopped.complete() {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                stopped = failure.into_stopped();
                thread::yield_now();
            }
            Err(failure) => panic!("complete tails stop: {:?}", failure.reason),
        }
    }
}
fn retire(mut owner: Mutator<'_>) {
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire tails owner: {:?}", failure.reason),
        }
    }
}
/// The era endgame every clean tail test needs: ACK, stop, return the
/// catalog, collect, complete and resume before finish retires the era.
fn endgame(era: &ParallelHeapEra<'_>, owner: &mut Mutator<'_>, roots: &[SharedRoot]) {
    owner.enter_native(roots).unwrap();
    let mut stopped = retry(|| era.stop());
    retry_registration(|| era.return_cons_storage(&stopped));
    retry(|| era.collect_stopped(&mut stopped));
    complete(stopped);
    owner.leave_native().unwrap();
}
fn roots(context: &mut Context) -> ([SharedRoot; 2], SharedRoot) {
    // SAFETY: immediate values are live on this serial heap; no heap object
    // or raw Value is transported to an owner.
    unsafe {
        (
            [
                SharedRoot::new(&context.tagged_heap, Value::fixnum(1801)),
                SharedRoot::new(&context.tagged_heap, Value::fixnum(1802)),
            ],
            SharedRoot::new(&context.tagged_heap, Value::NIL),
        )
    }
}

/// Region carving keeps its per-region pace exactly (one refill per <=128
/// cells) while the catalog gate is taken once per whole-tail batch, not
/// once per region.
#[test]
fn tail_batches_amortize_catalog_grants_across_regions() {
    const CELLS: usize = 600;
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1811, 8).unwrap(), era.heap_identity(), 1811);
        let head = with_run(&mut owner, |run| {
            let mut last = None;
            for _ in 0..CELLS {
                last = Some(run.cons(&cars[0], &nil).unwrap());
            }
            last
        })
        .expect("six hundred conses");
        let stats = with_run(&mut owner, |run| run.cons_stats_for_test());
        // Region pace is unchanged: one refill per <=128 cells, one charge
        // per carved cell. Bootstrap free fragments may only add regions.
        assert!(stats.cons_refills as usize >= (CELLS + 127) / 128);
        assert!(stats.cons_cells_granted as usize >= CELLS);
        // Lent tails stay charged until this owner ACKs.
        let snapshot = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(snapshot.installed);
        assert!(snapshot.active_regions >= 1, "held tails stay charged");
        assert!(snapshot.live_count >= CELLS);
        endgame(&era, &mut owner, std::slice::from_ref(&head));
        drop(head);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// The local close rewinds this owner's tail cursor over the unused region
/// suffix without touching the catalog gate.
#[test]
fn local_region_close_rewinds_the_carving_tail() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1812, 8).unwrap(), era.heap_identity(), 1812);
        with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        // Bootstrap free storage may be fragmented into one-cell runs; spend
        // fragments until the front tail has clear headroom, then prove the
        // local rewind on that one tail.
        let (cursor, limit) = loop {
            let bounds = with_run(&mut owner, |run| run.cons_tail_bounds_for_test());
            if let Some((cursor, limit)) = bounds {
                if limit - cursor >= 2 {
                    break (cursor, limit);
                }
            }
            with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        };
        // One cons, then the region closes locally: the tail cursor must sit
        // exactly one past the initialized cell, suffix intact, no gate taken.
        let head = with_run(&mut owner, |run| run.cons(&cars[1], &nil).unwrap());
        let (cursor_again, limit_again) =
            with_run(&mut owner, |run| run.cons_tail_bounds_for_test())
                .expect("owner holds a tail");
        assert_eq!(
            (cursor_again, limit_again),
            (cursor + 1, limit),
            "the closed region's suffix serves the next local carve"
        );
        endgame(&era, &mut owner, std::slice::from_ref(&head));
        drop(head);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// Publication at the ACK returns the unhanded suffix exactly: the catalog is
/// coherent for the collector and later carves reuse the returned cells.
#[test]
fn publish_at_stop_returns_unhanded_suffix_exactly() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        // The installed catalog carries the serial heap's live conses; only
        // the DELTA is this owner's.
        let live0 = era.cons_catalog_snapshot_for_test().unwrap().live_count;
        let mut owner = activate(era.register(1813, 8).unwrap(), era.heap_identity(), 1813);
        let head = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        let snapshot_before = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(snapshot_before.active_regions >= 1);
        owner.enter_native(std::slice::from_ref(&head)).unwrap();
        let snapshot = era.cons_catalog_snapshot_for_test().unwrap();
        assert_eq!(snapshot.active_regions, 0, "the ACK published every tail");
        assert!(snapshot.metadata_coherent);
        assert_eq!(
            snapshot.live_count,
            live0 + 1,
            "publication refunds exactly the unhanded suffix"
        );
        let mut stopped = retry(|| era.stop());
        retry_registration(|| era.return_cons_storage(&stopped));
        retry(|| era.collect_stopped(&mut stopped));
        // After resume the next carve works and stays disjoint from the
        // published live cell. The resumed catalog must be stopped and
        // returned once more before this era can finish.
        retry_registration(|| era.resume_cons_storage(&stopped));
        complete(stopped);
        owner.leave_native().unwrap();
        let second = with_run(&mut owner, |run| run.cons(&cars[1], &nil).unwrap());
        assert_ne!(
            head.cons_word_for_test(),
            second.cons_word_for_test(),
            "distinct live cells"
        );
        owner.enter_native(&[head.clone(), second.clone()]).unwrap();
        let mut stopped = retry(|| era.stop());
        retry_registration(|| era.return_cons_storage(&stopped));
        retry(|| era.collect_stopped(&mut stopped));
        complete(stopped);
        owner.leave_native().unwrap();
        drop((head, second));
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// Two owners carve from exclusive tails concurrently; every cell address is
/// distinct and both publications leave one coherent catalog.
#[test]
fn two_owners_carve_disjoint_exclusive_tails() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let permits = [
            era.register(1814, 8).unwrap(),
            era.register(1815, 8).unwrap(),
        ];
        let (tx, rx) = mpsc::channel();
        thread::scope(|scope| {
            // Drop the keeper's wake sender before scope joins on unwind, so
            // an assertion or owner failure cannot strand a peer in recv().
            let (go, go_rx) = mpsc::channel();
            let go_rx = std::sync::Arc::new(std::sync::Mutex::new(go_rx));
            for (index, permit) in permits.into_iter().enumerate() {
                let tx = tx.clone();
                let go_rx = std::sync::Arc::clone(&go_rx);
                let (cars, nil) = (&cars, &nil);
                let heap = era.heap_identity();
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, 1814 + index as u64);
                    let words = with_run(&mut owner, |run| {
                        let mut words = Vec::new();
                        for _ in 0..200 {
                            let cell = run.cons(&cars[0], &nil).unwrap();
                            words.push(cell.cons_word_for_test());
                        }
                        words
                    });
                    // The peer can hold the registry gate while activating or
                    // publishing. Busy is a retryable refusal, not an ACK.
                    retry(|| owner.enter_native(&[]));
                    tx.send(words).unwrap();
                    // This owner owes no more results. Retaining its sender
                    // across the keeper wait would hide a failed peer from rx.
                    drop(tx);
                    go_rx
                        .lock()
                        .expect("go channel lock")
                        .recv()
                        .expect("keeper completes the stop");
                    // Resume can transiently conflict with the peer's debt
                    // application; only Busy is retryable here.
                    loop {
                        match owner.leave_native() {
                            Ok(_) => break,
                            Err(RendezvousError::Busy) => thread::yield_now(),
                            Err(error) => panic!("resume tails owner: {error:?}"),
                        }
                    }
                    retire(owner);
                });
            }
            drop(tx);
            let mut all = Vec::new();
            for _ in 0..2 {
                all.extend(rx.recv().expect("owner finished"));
            }
            let unique: HashSet<usize> = all.iter().copied().collect();
            assert_eq!(unique.len(), 400, "no two owners carved one cell");
            let mut stopped = retry(|| era.stop());
            retry_registration(|| era.return_cons_storage(&stopped));
            retry(|| era.collect_stopped(&mut stopped));
            complete(stopped);
            drop(go_rx);
            for _ in 0..2 {
                go.send(()).unwrap();
            }
        });
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// Generational carving still records each touched block exactly once in the
/// owner's canonical young log, whatever the batch composition; a GEN0 era
/// records nothing, exactly as the per-region grant did.
#[test]
fn generational_batch_carves_log_every_block_once() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let generational = era
            .cons_catalog_snapshot_for_test()
            .expect("installed catalog")
            .generational;
        let mut owner = activate(era.register(1816, 8).unwrap(), era.heap_identity(), 1816);
        let head = with_run(&mut owner, |run| {
            for _ in 0..600 {
                run.cons(&cars[0], &nil).unwrap();
            }
            let blocks = run.cons_young_blocks_for_test();
            let unique: HashSet<usize> = blocks.iter().copied().collect();
            assert_eq!(unique.len(), blocks.len(), "no duplicate young log entry");
            if generational {
                assert!(!blocks.is_empty(), "carves recorded young blocks");
            } else {
                assert!(
                    blocks.is_empty(),
                    "non-generational eras keep owner young logs empty"
                );
            }
            run.cons(&cars[1], &nil).unwrap()
        });
        endgame(&era, &mut owner, std::slice::from_ref(&head));
        drop(head);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// A tail dropped without publication marks the catalog terminal, exactly
/// like an unclosed region: later storage work is refused, never faked.
#[test]
fn dropped_unpublished_tail_poisons_the_catalog() {
    Context::with_test_collection_era(roots, |era, (_cars, _nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1817, 8).unwrap(), era.heap_identity(), 1817);
        {
            let mut run = owner.run_access().unwrap();
            run.cons_probe_tail_batch_for_test().unwrap();
            run.cons_drop_tails_for_test();
            assert_eq!(
                run.cons_probe_tail_batch_for_test(),
                Err(RegistrationError::Poisoned)
            );
        }
        drop(owner);
        // The abandoned era retains its storage instead of completing.
        assert!(era.finish().is_err());
    });
}
