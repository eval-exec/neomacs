//! Product polls: Continue on an admissible phase word, Resumed only after restore.

use super::frames::{FrameArena, OwnerScanJoin, RootGroup};
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::WorldProofEra;
use crate::emacs_core::eval::world::poll::PollOutcome;
use crate::emacs_core::eval::world::poll::{
    poll_interp_back_edge, poll_interp_call_entry, poll_region_refill, poll_serial_request,
    poll_vm_back_edge, poll_vm_call,
};
use crate::emacs_core::eval::world::serial_gate::{
    SerialEndError, SerialTicket, end_serial_for_test, lend_serial_for_test, serial_gc_for_test,
    stop_for_test,
};
use neovm_value::tagged::gc::mutator_registration::a8_probes;
use neovm_value::tagged::gc::{
    AllStopped, ClosedConsReserve, HeapIdentity, MeasuredInterval, Mutator, MutatorPermit,
    OwnerEraReserve, RegistrationError, Rendezvous, RendezvousError, Request, ReserveError,
};
use neovm_value::tagged::header::ConsCell;
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(super) const OWNER: u64 = 7501;
const KEEPER_OWNER: u64 = 7502;
const DEADLINE: Duration = Duration::from_secs(30);

pub(super) fn activate(
    mut permit: MutatorPermit<'_>,
    heap: HeapIdentity,
    owner: u64,
) -> Mutator<'_> {
    let start = Instant::now();
    loop {
        match permit.activate(heap, owner) {
            Ok(owner) => return owner,
            Err(failure)
                if failure.reason == RegistrationError::Busy && start.elapsed() < DEADLINE =>
            {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activate: {:?}", failure.reason),
        }
    }
}

pub(super) fn retry<T>(label: &str, mut op: impl FnMut() -> Result<T, RendezvousError>) -> T {
    let start = Instant::now();
    loop {
        match op() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) if start.elapsed() < DEADLINE => thread::yield_now(),
            Err(error) => panic!("{label}: {error:?}"),
        }
    }
}

pub(super) fn retire(mut owner: Mutator<'_>) {
    let start = Instant::now();
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && start.elapsed() < DEADLINE =>
            {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire: {:?}", failure.reason),
        }
    }
}

pub(super) fn finish(era: WorldProofEra<'_>) {
    let (collector, keeper) = era.finish().expect("finish the era");
    drop(collector);
    drop(keeper);
}

pub(super) fn request_stop(rendezvous: Rendezvous<'_>) {
    let start = Instant::now();
    loop {
        match rendezvous.request() {
            Ok(Request::Active(_)) => return,
            Ok(Request::Queued) if start.elapsed() < DEADLINE => thread::yield_now(),
            Ok(Request::Queued) => panic!("request stayed queued"),
            Err(RendezvousError::Busy) if start.elapsed() < DEADLINE => thread::yield_now(),
            Err(error) => panic!("request: {error:?}"),
        }
    }
}

fn owns(era: &WorldProofEra<'_>, stopped: &AllStopped<'_>, bits: usize) -> bool {
    era.with_stopped_heap_for_test(stopped, |heap| {
        heap.owns_heap_value_for_test(value_from_bits(bits))
    })
    .expect("stopped heap")
}

fn admissible(owner: &Mutator<'_>) -> bool {
    let word = owner.phase_word_for_test();
    word & 0x0F == 0 && word & (1 << 4) == 0
}

pub(super) fn recv<T>(rx: &mpsc::Receiver<T>) -> T {
    rx.recv_timeout(DEADLINE)
        .expect("peer reached the poll boundary")
}

fn value_from_bits(bits: usize) -> Value {
    // SAFETY: `bits` is `Value::bits` of a cons this heap allocated and still holds.
    unsafe { Value::from_bits(bits) }
}

fn cons_at(addr: usize) -> Value {
    let cell = addr as *const ConsCell;
    // SAFETY: `addr` is a cell this owner's reserve just carved. The stores
    // finish before the value is rooted.
    unsafe {
        (*cell).set_car(Value::NIL);
        (*cell).set_cdr(Value::NIL);
        Value::from_cons_ptr(cell)
    }
}

struct HeldCons {
    bits: usize,
    root: SharedRoot,
    heap: usize,
}

fn hold_cons(context: &mut Context) -> HeldCons {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.tagged_heap.close_alloc_regions();
    let value = context.tagged_heap.alloc_cons(Value::fixnum(7), Value::NIL);
    // SAFETY: setup is the serial mutator. The root keeps the cons reachable
    // until the owner scan publishes it.
    let root = unsafe { SharedRoot::new(&context.tagged_heap, value) };
    HeldCons {
        bits: value.bits(),
        root,
        heap: context.tagged_heap.heap_root().as_ptr() as usize,
    }
}

fn heap_mut<'a>(addr: usize) -> &'a mut neovm_value::tagged::gc::TaggedHeap {
    // SAFETY: `addr` is the era heap. Callers dereference it only while the
    // keeper is not inside `serial_gc_for_test`.
    unsafe { &mut *(addr as *mut neovm_value::tagged::gc::TaggedHeap) }
}

fn quiet_setup(context: &mut Context) -> usize {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.tagged_heap.close_alloc_regions();
    // SAFETY: setup is the sole serial mutator, before the era admits anyone.
    // A post-image owner grant records the new block on the resident young
    // shell, and `restore_owner_payload` refuses that shell. Pinning
    // generational off keeps the grant off that log.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut context.tagged_heap,
            false,
        );
    }
    context.tagged_heap.heap_root().as_ptr() as usize
}

/// Retry `Busy` without dropping `AllStopped`. Dropping the failure poisons the era.
pub(super) fn lend<'era, 'heap>(
    era: &'era WorldProofEra<'heap>,
    mut stopped: AllStopped<'era>,
) -> SerialTicket<'era, 'heap> {
    let start = Instant::now();
    loop {
        let keeper = era.keeper_for_test().expect("keeper");
        match lend_serial_for_test(era, keeper, stopped) {
            Ok(ticket) => return ticket,
            Err(failure)
                if failure.reason == RendezvousError::Busy && start.elapsed() < DEADLINE =>
            {
                stopped = failure.stopped;
                thread::yield_now();
            }
            Err(failure) => panic!("lend: {:?}", failure.reason),
        }
    }
}

pub(super) fn collect(ticket: &mut SerialTicket<'_, '_>) {
    retry("serial_gc", || serial_gc_for_test(ticket));
}

pub(super) fn end_epoch(mut ticket: SerialTicket<'_, '_>) {
    let start = Instant::now();
    loop {
        match end_serial_for_test(ticket) {
            Ok(()) => return,
            Err(failure)
                if matches!(
                    failure.reason,
                    SerialEndError::Complete(RendezvousError::Busy)
                ) && start.elapsed() < DEADLINE =>
            {
                ticket = failure.ticket;
                thread::yield_now();
            }
            Err(failure) => panic!("end: {:?}", failure.reason),
        }
    }
}

struct ReleaseHammers<'a>(&'a AtomicBool);

impl Drop for ReleaseHammers<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

#[test]
fn poll_continue_when_idle_records_no_publication() {
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let (tx, rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, OWNER);
                    a8_probes::enable();
                    let calls = AtomicUsize::new(0);
                    let outcome = owner
                        .poll(|| {
                            calls.fetch_add(1, Ordering::Relaxed);
                            Ok(Vec::new())
                        })
                        .expect("poll");
                    let events = a8_probes::take_events();
                    tx.send((
                        outcome,
                        calls.load(Ordering::Relaxed),
                        events,
                        admissible(&owner),
                    ))
                    .expect("report");
                    retire(owner);
                });
                let (outcome, calls, events, still_admissible) = recv(&rx);
                assert_eq!(outcome, PollOutcome::Continue);
                assert_eq!(calls, 0, "the idle fast path must not build a batch");
                assert!(
                    events.is_empty(),
                    "Continue must not publish, ack, or back out: {events:?}"
                );
                assert!(still_admissible);
            });
            finish(era);
        },
    );
}

#[test]
fn poll_fast_path_does_not_call_the_closure() {
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let (tx, rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, OWNER);
                    let mut total = 0usize;
                    let outcome = poll_interp_call_entry(&mut owner, || {
                        total += 1;
                        Ok(Vec::new())
                    });
                    assert_eq!(outcome.expect("call entry"), PollOutcome::Continue);
                    let outcome = poll_interp_back_edge(&mut owner, || {
                        total += 1;
                        Ok(Vec::new())
                    });
                    assert_eq!(outcome.expect("back edge"), PollOutcome::Continue);
                    let outcome = poll_vm_back_edge(&mut owner, || {
                        total += 1;
                        Ok(Vec::new())
                    });
                    assert_eq!(outcome.expect("vm back edge"), PollOutcome::Continue);
                    let outcome = poll_vm_call(&mut owner, || {
                        total += 1;
                        Ok(Vec::new())
                    });
                    assert_eq!(outcome.expect("vm call"), PollOutcome::Continue);
                    let outcome = poll_serial_request(&mut owner, || {
                        total += 1;
                        Ok(Vec::new())
                    });
                    assert_eq!(outcome.expect("serial request"), PollOutcome::Continue);
                    tx.send(total).expect("report");
                    retire(owner);
                });
                assert_eq!(recv(&rx), 0);
            });
            finish(era);
        },
    );
}

#[test]
fn poll_resumed_after_restore_keeps_the_published_root() {
    Context::with_test_collection_era(hold_cons, |era, held| {
        let permit = era.register(OWNER, 0).expect("register");
        let heap_id = era.heap_identity();
        let rendezvous = era.rendezvous();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel();
        let bits = held.bits;
        let heap_addr = held.heap;
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap_id, OWNER);
                let mut arena = FrameArena::new(1);
                arena
                    .roots_mut()
                    .push(RootGroup::Misc, value_from_bits(bits))
                    .expect("root");
                ready_tx.send(()).expect("ready");
                recv(&go_rx);
                let outcome = poll_interp_back_edge(&mut owner, || {
                    let mut batch = Vec::new();
                    arena.publish_scan(heap_mut(heap_addr), &mut batch)?;
                    Ok(batch)
                })
                .expect("poll");
                done_tx.send(outcome).expect("done");
                retire(owner);
            });
            recv(&ready_rx);
            request_stop(rendezvous);
            go_tx.send(()).expect("go");
            let stopped = retry("stop", || stop_for_test(&era));
            let before = owns(&era, &stopped, bits);
            let mut ticket = lend(&era, stopped);
            collect(&mut ticket);
            end_epoch(ticket);
            assert!(before, "the cons was not in the heap at the stop");
            assert!(matches!(recv(&done_rx), PollOutcome::Resumed { .. }));
        });
        drop(held.root);
        let stopped = retry("survived", || stop_for_test(&era));
        assert!(
            owns(&era, &stopped, bits),
            "the published cons did not survive serial_gc"
        );
        let mut ticket = lend(&era, stopped);
        collect(&mut ticket);
        end_epoch(ticket);
        finish(era);
    });
}

#[test]
fn poll_region_refill_on_reserve_miss_then_grant() {
    Context::with_test_collection_era(quiet_setup, |era, heap_addr| {
        let permit = era.register(OWNER, 0).expect("register");
        let heap_id = era.heap_identity();
        let rendezvous = era.rendezvous();
        let (keeper_tx, keeper_rx) = mpsc::channel::<ClosedConsReserve>();
        let (owner_tx, owner_rx) = mpsc::channel::<ClosedConsReserve>();
        let (carved_tx, carved_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let mut carved_bits = 0usize;
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap_id, OWNER);
                let mut arena = FrameArena::new(1);
                let mut reserve = OwnerEraReserve::new();
                // Complete the failed carve and end its mutable book borrow
                // before publication, owned handoff, and parking.
                assert_eq!(
                    arena.carve_granted_cons(heap_mut(heap_addr), &mut reserve),
                    Err(ReserveError::ReserveMiss)
                );
                arena
                    .close_granted_reserve(heap_mut(heap_addr), &mut reserve)
                    .expect("close the missed book");
                keeper_tx
                    .send(reserve.into_closed().expect("closed refill book"))
                    .expect("hand the book to the keeper");
                let outcome = poll_region_refill(&mut owner, rendezvous, || {
                    let mut batch = Vec::new();
                    arena.publish_scan(heap_mut(heap_addr), &mut batch)?;
                    Ok(batch)
                })
                .expect("refill poll");
                assert!(matches!(outcome, PollOutcome::Resumed { .. }));
                // Ownership returns after the stopped grant. No active book
                // borrow, reference, or raw book designator crossed the park.
                let mut reserve = recv(&owner_rx).into_active();
                arena
                    .carve_granted_cons(heap_mut(heap_addr), &mut reserve)
                    .expect("refill carve");
                let addr = arena.take_cons_cell().expect("carved cell");
                let kept = cons_at(addr);
                arena
                    .close_granted_reserve(heap_mut(heap_addr), &mut reserve)
                    .expect("close the carved book");
                arena
                    .roots_mut()
                    .push(RootGroup::Misc, kept)
                    .expect("root the new cons");
                keeper_tx
                    .send(reserve.into_closed().expect("closed return book"))
                    .expect("hand the book back for return");
                carved_tx.send(kept.bits()).expect("carved bits");
                recv(&go_rx);
                // Publish the new cons and park so the keeper's collection
                // traces it before the following ownership check.
                let outcome = poll_interp_back_edge(&mut owner, || {
                    let mut batch = Vec::new();
                    arena.publish_scan(heap_mut(heap_addr), &mut batch)?;
                    Ok(batch)
                })
                .expect("publish the carved cons");
                assert!(matches!(outcome, PollOutcome::Resumed { .. }));
                retire(owner);
            });
            let book = recv(&keeper_rx);
            let stopped = retry("stop", || stop_for_test(&era));
            let mut ticket = lend(&era, stopped);
            collect(&mut ticket);
            // Mint after the sweep so the new block is not this cycle's garbage.
            let mut reserve = book.into_active();
            {
                let heap = heap_mut(heap_addr);
                heap.close_alloc_regions();
                reserve
                    .grant_at_stop(ticket.stopped_proof(), heap, true, MeasuredInterval::zero())
                    .expect("grant at the stop");
            }
            owner_tx
                .send(reserve.into_closed().expect("closed granted book"))
                .expect("return book ownership to the executor");
            end_epoch(ticket);
            carved_bits = recv(&carved_rx);
            let mut reserve = recv(&keeper_rx).into_active();
            request_stop(rendezvous);
            go_tx.send(()).expect("publish the carved cons");
            let stopped = retry("reach", || stop_for_test(&era));
            let mut ticket = lend(&era, stopped);
            // The keeper owns the closed book while the executor is parked.
            // Return the uncarved tail before restoring the actual image.
            {
                let heap = heap_mut(heap_addr);
                reserve
                    .return_unused_before_sweep(ticket.stopped_proof(), heap)
                    .expect("return before the survival sweep");
                heap.clear_inter_collection_cons_charge_for_test();
            }
            collect(&mut ticket);
            end_epoch(ticket);
        });
        let stopped = retry("reachable", || stop_for_test(&era));
        assert!(
            owns(&era, &stopped, carved_bits),
            "the cons carved after the grant did not survive collection"
        );
        let mut ticket = lend(&era, stopped);
        collect(&mut ticket);
        end_epoch(ticket);
        finish(era);
    });
}

#[test]
fn poll_on_the_keeper_continues_while_the_executor_resumes() {
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let exec_permit = era.register(OWNER, 0).expect("executor");
            let keeper_permit = era.register(KEEPER_OWNER, 0).expect("keeper owner");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let mut owner = activate(exec_permit, heap, OWNER);
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    let outcome = owner.poll(|| Ok(Vec::new())).expect("executor poll");
                    done_tx.send(outcome).expect("done");
                    retire(owner);
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                let mut keeper_owner = activate(keeper_permit, heap, KEEPER_OWNER);
                let outcome = keeper_owner.poll(|| Ok(Vec::new())).expect("keeper poll");
                assert_eq!(outcome, PollOutcome::Continue);
                assert_eq!(
                    keeper_owner.phase_word_for_test() & 0x0F,
                    1,
                    "the word was STOPPING, so Continue came from the keeper identity, not the fast path"
                );
                go_tx.send(()).expect("go");
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                assert!(matches!(recv(&done_rx), PollOutcome::Resumed { .. }));
                retire(keeper_owner);
            });
            finish(era);
        },
    );
}

#[test]
fn poll_retries_busy_on_the_same_owner() {
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (first_go_tx, first_go_rx) = mpsc::channel::<()>();
            let (first_tx, first_rx) = mpsc::channel();
            let (second_ready_tx, second_ready_rx) = mpsc::channel::<()>();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (second_tx, second_rx) = mpsc::channel();
            let hammer = Arc::new(AtomicBool::new(true));
            thread::scope(|scope| {
                let _release = ReleaseHammers(&hammer);
                for _ in 0..2 {
                    let run = Arc::clone(&hammer);
                    let gate = rendezvous;
                    scope.spawn(move || {
                        while run.load(Ordering::Relaxed) {
                            let _ = gate.snapshot();
                            thread::yield_now();
                        }
                    });
                }
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, OWNER);
                    let calls = AtomicUsize::new(0);
                    ready_tx.send(()).expect("ready");
                    recv(&first_go_rx);
                    let outcome = owner
                        .poll(|| {
                            calls.fetch_add(1, Ordering::Relaxed);
                            Ok(Vec::new())
                        })
                        .expect("poll retried Busy");
                    first_tx
                        .send((outcome, calls.load(Ordering::Relaxed)))
                        .expect("first");
                    second_ready_tx.send(()).expect("second ready");
                    recv(&go_rx);
                    let again = owner.poll(|| Ok(Vec::new())).expect("later poll");
                    second_tx.send(again).expect("second");
                    retire(owner);
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                first_go_tx.send(()).expect("first go");
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                let (outcome, calls) = recv(&first_rx);
                assert!(matches!(outcome, PollOutcome::Resumed { .. }));
                assert_eq!(calls, 1, "Busy retry must reuse the same batch");
                hammer.store(false, Ordering::Relaxed);
                recv(&second_ready_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go");
                let stopped = retry("later stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                assert!(matches!(recv(&second_rx), PollOutcome::Resumed { .. }));
            });
            assert!(
                !era.registry_poisoned_for_test(),
                "a Busy retry poisoned the owner"
            );
            finish(era);
        },
    );
}

#[test]
fn poll_closure_error_leaves_the_owner_usable() {
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (failed_tx, failed_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, OWNER);
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    let error = owner
                        .poll(|| Err(RendezvousError::Capacity))
                        .expect_err("closure error");
                    assert_eq!(error, RendezvousError::Capacity);
                    failed_tx.send(()).expect("failed");
                    let outcome = owner.poll(|| Ok(Vec::new())).expect("following poll");
                    done_tx.send(outcome).expect("done");
                    retire(owner);
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go");
                recv(&failed_rx);
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                assert!(matches!(recv(&done_rx), PollOutcome::Resumed { .. }));
            });
            assert!(!era.registry_poisoned_for_test());
            finish(era);
        },
    );
}

#[test]
fn poll_fast_path_continue_ns() {
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let mut owner = activate(permit, heap, OWNER);
            let iterations = 20_000u32;
            let start = Instant::now();
            for _ in 0..iterations {
                let outcome = std::hint::black_box(owner.poll(|| {
                    panic!("fast path called the closure");
                }))
                .expect("poll");
                assert_eq!(outcome, PollOutcome::Continue);
            }
            let ns = start.elapsed().as_nanos() / u128::from(iterations);
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tmp/pentry-r2/e2b/poll-fast-ns.txt");
            // The macmini lane does not receive this repo's tmp/ directory.
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create ns receipt directory");
            }
            std::fs::write(&path, format!("POLL_FAST_NS_PER_ITER={ns}\n"))
                .expect("write ns receipt");
            eprintln!("POLL_FAST_NS_PER_ITER={ns}");
            assert!(admissible(&owner));
            retire(owner);
            finish(era);
        },
    );
}

#[test]
fn poll_during_draining_resumes_returns_continue_without_deadlock() {
    // HIGH-3 regression: after `end_epoch`, the era stays in
    // `Phase::DrainingResumes` (`TAG_DRAINING == 3`, `!admissible`) until
    // every frozen owner finishes `apply()` on its `ResumeGcImage`. Hold Peer
    // A paused after `begin_restore(false)` and before `apply()`, assert Peer
    // B observes `phase_word & 0x0F == 3`, verify `owner_b.poll(...)` returns
    // `Ok(PollOutcome::Continue)` with 0 `roots()` invocations, release Peer
    // A, then trigger a second stop `Stopping(epoch1 + 1)` and verify Peer B
    // still parks and resumes cleanly.
    const PEER_OWNER: u64 = 7503;
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit_a = era.register(OWNER, 0).expect("register A");
            let permit_b = era.register(PEER_OWNER, 0).expect("register B");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let (ready_a_tx, ready_a_rx) = mpsc::channel();
            let (ready_b_tx, ready_b_rx) = mpsc::channel();
            let (go_b1_tx, go_b1_rx) = mpsc::channel::<()>();
            let (a_ended_tx, a_ended_rx) = mpsc::channel::<()>();
            let (a_paused_tx, a_paused_rx) = mpsc::channel::<()>();
            let (a_apply_tx, a_apply_rx) = mpsc::channel::<()>();
            let (a_applied_tx, a_applied_rx) = mpsc::channel::<()>();
            let (b_draining_tx, b_draining_rx) = mpsc::channel();
            let (go_b2_tx, go_b2_rx) = mpsc::channel::<()>();
            let (done_b2_tx, done_b2_rx) = mpsc::channel();
            let (go_a2_tx, go_a2_rx) = mpsc::channel::<()>();
            thread::scope(|scope| {
                // Peer A: ACKs epoch 1, waits for `end_epoch`, calls
                // `begin_restore(false)` and pauses BEFORE `apply()` so the
                // era remains deterministically in `Phase::DrainingResumes`.
                scope.spawn(move || {
                    let mut owner = activate(permit_a, heap, OWNER);
                    ready_a_tx.send(()).expect("A ready");
                    let epoch1 = retry("A wait_request", || owner.wait_request());
                    retry("A ack", || owner.acknowledge(epoch1, &[]));
                    recv(&a_ended_rx);
                    let start = Instant::now();
                    let mut pending = loop {
                        match owner.begin_restore(false) {
                            Ok(image) => break image,
                            Err(RendezvousError::Busy) if start.elapsed() < DEADLINE => {
                                thread::yield_now();
                            }
                            Err(error) => panic!("A begin_restore: {error:?}"),
                        }
                    };
                    a_paused_tx.send(()).expect("A paused in DrainingResumes");
                    recv(&a_apply_rx);
                    loop {
                        match pending.apply() {
                            Ok(()) => break,
                            Err(failure) if failure.reason == RendezvousError::Busy => {
                                pending = failure.into_image();
                                thread::yield_now();
                            }
                            Err(failure) => panic!("A apply: {:?}", failure.reason),
                        }
                    }
                    a_applied_tx.send(()).expect("A applied");
                    recv(&go_a2_rx);
                    let second = owner.poll(|| Ok(Vec::new())).expect("A second poll");
                    assert!(matches!(second, PollOutcome::Resumed { epoch } if epoch.get() == epoch1.get() + 1));
                    retire(owner);
                });
                // Peer B: resumes from epoch 1 via `poll()`, observes
                // `TAG_DRAINING == 3` while Peer A holds `begin_restore`
                // without `apply()`, calls `poll()` -> `Continue` with 0
                // `roots()` calls, then parks cleanly on `Stopping(epoch1+1)`.
                scope.spawn(move || {
                    let mut owner = activate(permit_b, heap, PEER_OWNER);
                    ready_b_tx.send(()).expect("B ready");
                    recv(&go_b1_rx);
                    let first = owner.poll(|| Ok(Vec::new())).expect("B first poll");
                    let PollOutcome::Resumed { epoch: epoch1 } = first else {
                        panic!("expected Resumed on epoch 1, got {first:?}");
                    };
                    recv(&a_paused_rx);
                    let phase_tag = owner.phase_word_for_test() & 0x0F;
                    let non_admissible = !admissible(&owner);
                    let closure_calls = AtomicUsize::new(0);
                    let during_drain = owner
                        .poll(|| {
                            closure_calls.fetch_add(1, Ordering::Relaxed);
                            Ok(Vec::new())
                        })
                        .expect("B poll during DrainingResumes");
                    b_draining_tx
                        .send((
                            epoch1,
                            phase_tag,
                            non_admissible,
                            during_drain,
                            closure_calls.load(Ordering::Relaxed),
                        ))
                        .expect("B draining report");
                    recv(&go_b2_rx);
                    let second_calls = AtomicUsize::new(0);
                    let second = owner
                        .poll(|| {
                            second_calls.fetch_add(1, Ordering::Relaxed);
                            Ok(Vec::new())
                        })
                        .expect("B second poll");
                    done_b2_tx
                        .send((second, second_calls.load(Ordering::Relaxed)))
                        .expect("B second report");
                    retire(owner);
                });
                recv(&ready_a_rx);
                recv(&ready_b_rx);
                request_stop(rendezvous);
                go_b1_tx.send(()).expect("go B1");
                let stopped = retry("stop 1", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                a_ended_tx.send(()).expect("epoch 1 ended");

                let (epoch1, phase_tag, non_admissible, during_drain, calls_during_drain) =
                    recv(&b_draining_rx);
                assert_eq!(
                    phase_tag, 3,
                    "Peer B must observe TAG_DRAINING (3) while Peer A holds resume debt"
                );
                assert!(non_admissible);
                assert_eq!(during_drain, PollOutcome::Continue);
                assert_eq!(
                    calls_during_drain, 0,
                    "poll during DrainingResumes must not build a root batch"
                );

                // Release Peer A so the era returns to Idle, then verify a
                // subsequent Stopping(epoch1 + 1) still parks Peer B cleanly.
                a_apply_tx.send(()).expect("release A apply");
                recv(&a_applied_rx);
                request_stop(rendezvous);
                go_a2_tx.send(()).expect("go A2");
                go_b2_tx.send(()).expect("go B2");
                let stopped2 = retry("stop 2", || stop_for_test(&era));
                let mut ticket2 = lend(&era, stopped2);
                collect(&mut ticket2);
                end_epoch(ticket2);
                let (second, second_calls) = recv(&done_b2_rx);
                assert_eq!(
                    second,
                    PollOutcome::Resumed {
                        epoch: std::num::NonZeroU64::new(epoch1.get() + 1).expect("epoch+1"),
                    }
                );
                assert_eq!(second_calls, 1, "subsequent Stopping must park Peer B once");
            });
            finish(era);
        },
    );
}

#[test]
fn poll_in_native_stopped_mode_refuses_busy_without_spinning() {
    // LOW-2 regression: calling `Mutator::poll` while the owner is already in
    // `Mode::NativeStopped` and `!admissible` must immediately refuse with
    // `Err(RendezvousError::Busy)` and 0 `roots()` invocations rather than
    // spinning forever in `poll_stopped`.
    Context::with_test_collection_era(
        |_context| (),
        |era, ()| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let (native_tx, native_rx) = mpsc::channel::<()>();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (done_tx, done_rx) = mpsc::channel();
            let (leave_tx, leave_rx) = mpsc::channel::<()>();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, OWNER);
                    owner.enter_native(&[]).expect("enter_native");
                    native_tx.send(()).expect("native entered");
                    recv(&go_rx);
                    let calls = AtomicUsize::new(0);
                    let result = owner.poll(|| {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(Vec::new())
                    });
                    done_tx
                        .send((result, calls.load(Ordering::Relaxed)))
                        .expect("poll result");
                    recv(&leave_rx);
                    owner.leave_native().expect("leave_native");
                    retire(owner);
                });
                recv(&native_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go poll in native");
                let (result, calls) = recv(&done_rx);
                assert_eq!(result, Err(RendezvousError::Busy));
                assert_eq!(calls, 0);
                let stopped = retry("native stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                leave_tx.send(()).expect("release leave_native");
            });
            finish(era);
        },
    );
}
