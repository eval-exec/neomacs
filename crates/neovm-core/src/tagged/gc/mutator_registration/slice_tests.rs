//! Generative slice boundaries over real admitted allocation and collection.
#![cfg(not(feature = "gc-memory-telemetry"))]
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

fn seeds(context: &mut Context) -> (SharedRoot, SharedRoot) {
    // SAFETY: immediate values belong to this genuine serial source heap.
    unsafe {
        (
            SharedRoot::new(&context.tagged_heap, Value::fixnum(8123)),
            SharedRoot::new(&context.tagged_heap, Value::NIL),
        )
    }
}
fn owner<'era>(era: &'era ParallelHeapEra<'_>, id: u64) -> Mutator<'era> {
    era.register(id, 2)
        .unwrap()
        .activate(era.heap_identity(), id)
        .unwrap()
}
fn with_run<'era, R>(
    owner: &mut Mutator<'era>,
    body: impl FnOnce(&mut RunAccess<'_, 'era>) -> R,
) -> R {
    let mut run = owner.run_access().unwrap();
    let result = body(&mut run);
    run.close_cons_region().unwrap();
    result
}
fn finish(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}
fn retire<const N: usize>(era: &ParallelHeapEra<'_>, mut owners: [Mutator<'_>; N]) {
    for owner in &mut owners {
        with_run(owner, |run| run.clear_root_slots().unwrap());
        owner.enter_native(&[]).unwrap();
    }
    let mut stopped = era.stop().unwrap();
    era.return_cons_storage(&stopped).unwrap();
    era.collect_stopped(&mut stopped).unwrap();
    stopped.complete().unwrap();
    for owner in owners {
        owner.retire_rooted(&[]).unwrap();
    }
}

#[test]
fn slice_bind_refuses_wrong_owner_heap_and_stale_homes_before_allocation() {
    let foreign_heap = super::super::TaggedHeap::new();
    // SAFETY: NIL is immediate, with a real different heap registration.
    let foreign = unsafe { SharedRoot::new(&foreign_heap, Value::NIL) };
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut first = owner(&era, 1901);
        let mut second = owner(&era, 1902);
        let other = with_run(&mut second, |run| run.root_slot(&nil).unwrap());
        with_run(&mut first, |run| {
            let old = run.root_slot(&car).unwrap();
            run.clear_root_slots().unwrap();
            let a = run.root_slot(&car).unwrap();
            let head = run.root_slot(&nil).unwrap();
            assert!(matches!(
                run.root_slot(&foreign),
                Err(RegistrationError::WrongHeap)
            ));
            let allocated = run.state().hot.allocated_count;
            let bank = (
                run.state().root_scratch.len(),
                run.state().root_scratch.capacity(),
            );
            let before = era.cons_catalog_snapshot_for_test().unwrap();
            run.with_slice(|slice| {
                assert!(matches!(
                    slice.bind(&old),
                    Err(RegistrationError::StaleRegistration)
                ));
                assert!(matches!(
                    slice.bind(&other),
                    Err(RegistrationError::WrongOwner)
                ));
                // The same factory admits current homes, so negative cases
                // cannot pass because the slice or catalog was unavailable.
                let _ = slice.bind(&a).unwrap();
                let _ = slice.bind(&head).unwrap();
            })
            .unwrap();
            assert_eq!(run.state().hot.allocated_count, allocated);
            assert_eq!(
                (
                    run.state().root_scratch.len(),
                    run.state().root_scratch.capacity()
                ),
                bank
            );
            assert!(run.cons_region_for_test().is_none());
            assert_eq!(era.cons_catalog_snapshot_for_test().unwrap(), before);
        });
        retire(&era, [first, second]);
        finish(era);
    });
}

#[test]
fn forgetting_callback_reference_still_closes_slice_and_roots_the_alias_chain() {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1903);
        let (a, head) = with_run(&mut owner, |run| {
            (run.root_slot(&car).unwrap(), run.root_slot(&nil).unwrap())
        });
        drop((car, nil));
        let word = with_run(&mut owner, |run| {
            run.with_slice(|slice| {
                let a = slice.bind(&a).unwrap();
                let mut head = slice.bind(&head).unwrap();
                let cdr = head.clone();
                for _ in 0..260 {
                    slice.cons_into(&a, &cdr, &mut head).unwrap();
                }
                // This is the only forget exposed by the safe API. The
                // wrapper owns the actual cursor guard and must still close.
                #[allow(forgetting_references)]
                std::mem::forget(slice);
            })
            .unwrap();
            assert!(run.cons_region_for_test().is_none());
            assert_eq!(run.state().hot.allocated_count, 260);
            head.word_for_test(run)
        });
        assert!(!era.registry().park.poisoned.load(Ordering::Acquire));
        // The closed slice left its tail lent to this owner: the charge is
        // exactly what the ACK below must publish back.
        assert!(era.cons_catalog_snapshot_for_test().unwrap().active_regions >= 1);
        owner.enter_native(&[]).unwrap();
        assert_eq!(
            era.cons_catalog_snapshot_for_test().unwrap().active_regions,
            0
        );
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        assert!(era.collect_stopped(&mut stopped).unwrap().major);
        era.with_stopped_heap_for_test(&stopped, |heap| {
            let mut cursor = Value::from_bits(word);
            for _ in 0..260 {
                assert!(heap.owns_heap_value_for_test(cursor));
                // SAFETY: exact stopped collection retained this canonical
                // bank-only chain, with no SharedRoot to hide a missing root.
                unsafe {
                    let cell = &*cursor.as_cons_ptr().unwrap();
                    assert_eq!(cell.car.as_fixnum(), Some(8123));
                    cursor = cell.cdr();
                }
            }
            assert!(cursor.is_nil());
        })
        .unwrap();
        stopped.complete().unwrap();
        owner.leave_native().unwrap();
        with_run(&mut owner, |run| run.clear_root_slots().unwrap());
        owner.retire_rooted(&[]).unwrap();
        finish(era);
    });
}

#[test]
fn poisoned_slice_entry_never_invokes_callback_or_changes_bank() {
    Context::with_test_collection_era(seeds, |era, (car, _)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1904);
        {
            let mut run = owner.run_access().unwrap();
            let home = run.root_slot(&car).unwrap();
            let before = home.word_for_test(&run);
            era.registry().poison();
            let mut invoked = false;
            assert_eq!(
                run.with_slice(|_| invoked = true),
                Err(RegistrationError::Poisoned)
            );
            assert!(!invoked);
            assert_eq!(home.word_for_test(&run), before);
            assert!(run.cons_region_for_test().is_none());
            assert_eq!(run.state().hot.allocated_count, 0);
        }
        assert_eq!(owner.enter_native(&[]), Err(RendezvousError::Poisoned));
        drop(owner);
        assert!(era.finish().is_err());
    });
}

fn abandoned_slice_retains_committed_cursor(panic_body: bool) {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, if panic_body { 1905 } else { 1906 });
        {
            let mut run = owner.run_access().unwrap();
            let a = run.root_slot(&car).unwrap();
            let head = run.root_slot(&nil).unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run.with_slice(|slice| {
                    let a = slice.bind(&a).unwrap();
                    let mut head = slice.bind(&head).unwrap();
                    let cdr = head.clone();
                    for n in 0..100_000 {
                        slice.cons_into(&a, &cdr, &mut head).unwrap();
                        if slice.remaining_for_test() != 0 {
                            break;
                        }
                        assert!(n + 1 < 100_000, "real partial grant");
                    }
                    if panic_body {
                        panic!("intentional slice body panic");
                    }
                    era.registry().poison();
                })
            }));
            if panic_body {
                let payload = result.expect_err("the intended body panic must escape");
                assert_eq!(
                    payload.downcast_ref::<&str>(),
                    Some(&"intentional slice body panic")
                );
            } else {
                assert_eq!(result.unwrap(), Err(RegistrationError::Poisoned));
            }
            assert!(era.registry().park.poisoned.load(Ordering::Acquire));
            let (start, end, unused) = run
                .cons_region_for_test()
                .expect("retained canonical grant");
            assert!(unused > 0);
            let pointer = Value::from_bits(head.word_for_test(&run))
                .as_cons_ptr()
                .unwrap()
                .addr();
            assert!(pointer >= start && pointer < end);
            assert_eq!(
                end - unused * std::mem::size_of::<super::super::ConsCell>(),
                pointer + std::mem::size_of::<super::super::ConsCell>(),
                "Drop/failed finish committed the last initialized cell"
            );
            let before = era.cons_catalog_snapshot_for_test().unwrap();
            assert!(before.active_regions >= 1);
            assert_eq!(run.close_cons_region(), Err(RegistrationError::Poisoned));
            assert_eq!(era.cons_catalog_snapshot_for_test().unwrap(), before);
        }
        assert!(owner.entry.admitted());
        assert_eq!(owner.enter_native(&[]), Err(RendezvousError::Poisoned));
        drop(owner);
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::OpenRegion
        );
        // An abandoned fixture must never resume its suspended Context.
    });
}

#[test]
fn slice_body_panic_commits_cursor_and_poison_retains_backing() {
    abandoned_slice_retains_committed_cursor(true);
}

#[test]
fn poison_at_slice_exit_commits_cursor_and_retains_backing() {
    abandoned_slice_retains_committed_cursor(false);
}

/// Two OS owners hold live branded slices before either starts writing.
#[test]
fn two_owner_branded_slice_allocator_stress() {
    use std::collections::HashSet;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;
    const PHASES: usize = 2;
    const PER_OWNER: usize = 2100;
    fn retry<T>(mut operation: impl FnMut() -> Result<T, RendezvousError>) -> T {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match operation() {
                Ok(value) => return value,
                Err(RendezvousError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("branded-slice rendezvous: {error:?}"),
            }
        }
    }
    fn registration<T>(mut operation: impl FnMut() -> Result<T, RegistrationError>) -> T {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match operation() {
                Ok(value) => return value,
                Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("branded-slice registration: {error:?}"),
            }
        }
    }
    fn run<'era, R>(
        owner: &mut Mutator<'era>,
        body: impl FnOnce(&mut RunAccess<'_, 'era>) -> R,
    ) -> R {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match owner.run_access() {
                Ok(mut access) => {
                    let result = body(&mut access);
                    access.close_cons_region().unwrap();
                    return result;
                }
                Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("branded-slice run: {error:?}"),
            }
        }
    }
    fn receive<T>(receiver: &mpsc::Receiver<T>) -> T {
        receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("branded-slice actor boundary")
    }
    struct Abort<'a>(&'a Registry);
    impl Drop for Abort<'_> {
        fn drop(&mut self) {
            if thread::panicking() {
                self.0.poison();
            }
        }
    }
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let heap = era.heap_identity();
        let permits = [
            era.register(1907, 2).unwrap(),
            era.register(1908, 2).unwrap(),
        ];
        let active = Arc::new(AtomicUsize::new(0));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::scope(|scope| {
            let _abort = Abort(era.registry());
            let mut commands = Vec::new();
            for (index, mut permit) in permits.into_iter().enumerate() {
                let ready = ready_tx.clone();
                let results = result_tx.clone();
                let car = car.clone();
                let nil = nil.clone();
                let active = Arc::clone(&active);
                let (go_tx, go_rx) = mpsc::channel();
                commands.push(go_tx);
                scope.spawn(move || {
                    let deadline = Instant::now() + Duration::from_secs(30);
                    let mut owner = loop {
                        match permit.activate(heap, 1907 + index as u64) {
                            Ok(owner) => break owner,
                            Err(failure)
                                if failure.reason == RegistrationError::Busy
                                    && Instant::now() < deadline =>
                            {
                                permit = failure.into_permit();
                                thread::yield_now();
                            }
                            Err(failure) => panic!("branded-slice activate: {:?}", failure.reason),
                        }
                    };
                    let (car_slot, tail) = run(&mut owner, |run| {
                        (run.root_slot(&car).unwrap(), run.root_slot(&nil).unwrap())
                    });
                    drop(car);
                    drop(nil);
                    for phase in 0..PHASES {
                        let head = run(&mut owner, |run| {
                            let completed = run
                                .with_slice(|slice| {
                                    let car = slice.bind(&car_slot).unwrap();
                                    let mut head = slice.bind(&tail).unwrap();
                                    let cdr = head.clone();
                                    active.fetch_add(1, Ordering::SeqCst);
                                    ready.send((index, phase)).unwrap();
                                    if go_rx.recv().is_err() {
                                        active.fetch_sub(1, Ordering::SeqCst);
                                        return false;
                                    }
                                    for sequence in 0..PER_OWNER {
                                        slice.cons_into(&car, &cdr, &mut head).unwrap();
                                        if sequence % 31 == 0 {
                                            thread::yield_now();
                                        }
                                    }
                                    active.fetch_sub(1, Ordering::SeqCst);
                                    true
                                })
                                .unwrap();
                            completed.then(|| tail.word_for_test(run))
                        });
                        let Some(head) = head else { return };
                        results.send((index, phase, owner.key(), head)).unwrap();
                        let epoch = retry(|| owner.wait_request());
                        retry(|| owner.acknowledge(epoch, &[]));
                        let mut image = owner.begin_restore_retry(false).unwrap();
                        let deadline = Instant::now() + Duration::from_secs(30);
                        loop {
                            match image.apply() {
                                Ok(()) => break,
                                Err(failure)
                                    if failure.reason == RendezvousError::Busy
                                        && Instant::now() < deadline =>
                                {
                                    image = failure.into_image();
                                    thread::yield_now();
                                }
                                Err(failure) => {
                                    panic!("branded-slice restore: {:?}", failure.reason)
                                }
                            }
                        }
                        run(&mut owner, |run| assert_eq!(tail.word_for_test(run), head));
                    }
                    run(&mut owner, |run| run.clear_root_slots().unwrap());
                    let deadline = Instant::now() + Duration::from_secs(30);
                    loop {
                        match owner.retire_rooted(&[]) {
                            Ok(()) => break,
                            Err(failure)
                                if failure.reason == RendezvousError::Busy
                                    && Instant::now() < deadline =>
                            {
                                owner = failure.into_owner();
                                thread::yield_now();
                            }
                            Err(failure) => panic!("branded-slice retire: {:?}", failure.reason),
                        }
                    }
                });
            }
            drop(ready_tx);
            drop(result_tx);
            for phase in 0..PHASES {
                let ready = [receive(&ready_rx), receive(&ready_rx)];
                assert_eq!((ready[0].1, ready[1].1), (phase, phase));
                assert_ne!(ready[0].0, ready[1].0);
                assert_eq!(active.load(Ordering::SeqCst), 2);
                for command in &commands {
                    command.send(()).unwrap();
                }
                let responses = [receive(&result_rx), receive(&result_rx)];
                assert_ne!(responses[0].0, responses[1].0);
                retry(|| era.rendezvous().request());
                let mut stopped = retry(|| era.stop());
                registration(|| era.return_cons_storage(&stopped));
                for &(_, observed_phase, key, _) in &responses {
                    assert_eq!(observed_phase, phase);
                    retry(|| {
                        stopped.with_owner_image(key, |image| {
                            assert_eq!(image.summary().root_words, 2)
                        })
                    });
                    retry(|| {
                        stopped.with_root_reader(key, |reader| {
                            let mut external = Vec::new();
                            reader.append_words(&mut external);
                            assert!(external.is_empty());
                            reader.finish().unwrap();
                        })
                    });
                }
                assert!(retry(|| era.collect_stopped(&mut stopped)).major);
                retry(|| {
                    era.with_stopped_heap_for_test(&stopped, |heap| {
                        let mut addresses = HashSet::new();
                        for &(_, _, _, head) in &responses {
                            let mut cursor = Value::from_bits(head);
                            for _ in 0..(phase + 1) * PER_OWNER {
                                assert!(heap.owns_heap_value_for_test(cursor));
                                assert!(
                                    addresses.insert(cursor.bits()),
                                    "overlapping owner chains"
                                );
                                // SAFETY: stopped exact images retain both complete
                                // chains; no owner has run or cell-write authority.
                                let cell = unsafe { &*cursor.as_cons_ptr().unwrap() };
                                assert_eq!(cell.car.as_fixnum(), Some(8123));
                                // SAFETY: the same stopped, fully initialized cell remains live.
                                cursor = unsafe { cell.cdr() };
                            }
                            assert!(cursor.is_nil());
                        }
                        assert_eq!(addresses.len(), 2 * (phase + 1) * PER_OWNER);
                    })
                });
                if phase + 1 < PHASES {
                    registration(|| era.resume_cons_storage(&stopped));
                }
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    match stopped.complete() {
                        Ok(()) => break,
                        Err(failure)
                            if failure.reason == RendezvousError::Busy
                                && Instant::now() < deadline =>
                        {
                            stopped = failure.into_stopped();
                            thread::yield_now();
                        }
                        Err(failure) => panic!("branded-slice complete: {:?}", failure.reason),
                    }
                }
            }
        });
        finish(era);
    });
}
