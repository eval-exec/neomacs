//! Second-thread product cases for the #7712 serial gate.
//! The ticket stays on the keeper. The participant and `Rendezvous` cross
//! threads. A non-keeper `stop` stays uncallable: `WorldProofEra` is
//! `!Send + !Sync`. The owner loop is the `join_test` pattern.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use neovm_value::tagged::gc::{
    HeapIdentity, Mutator, MutatorPermit, RegistrationError, RendezvousError, Request,
};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::test_support::authority::{
    begin_restore_retry_for_test, install_restore_gate_hook_for_test,
};
use neovm_value::value::Value;

use super::serial_gate::{
    SerialEndError, end_serial_for_test, lend_serial_for_test, serial_gc_for_test, stop_for_test,
};

const OWNER: u64 = 7402;
const DEADLINE: Duration = Duration::from_secs(30);
const HELD_WINDOW: Duration = Duration::from_millis(250);

fn cons(context: &mut crate::emacs_core::eval::Context, tag: i64) -> SharedRoot {
    let value = context
        .tagged_heap
        .alloc_cons(Value::fixnum(tag), Value::NIL);
    // SAFETY: setup is the serial mutator of this context's heap. `value`
    // was allocated here, and the returned root keeps it reachable.
    unsafe { SharedRoot::new(&context.tagged_heap, value) }
}

fn activate(mut permit: MutatorPermit<'_>, heap: HeapIdentity) -> Mutator<'_> {
    let start = Instant::now();
    loop {
        match permit.activate(heap, OWNER) {
            Ok(mutator) => return mutator,
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

fn request_epoch(rendezvous: neovm_value::tagged::gc::Rendezvous<'_>) -> std::num::NonZeroU64 {
    let start = Instant::now();
    loop {
        match rendezvous.request() {
            Ok(Request::Active(epoch)) => return epoch,
            Ok(Request::Queued) if start.elapsed() < DEADLINE => thread::yield_now(),
            Ok(Request::Queued) => panic!("request stayed Queued"),
            Err(RendezvousError::Busy) if start.elapsed() < DEADLINE => thread::yield_now(),
            Err(error) => panic!("request: {error:?}"),
        }
    }
}

fn acknowledge(owner: &mut Mutator<'_>, epoch: std::num::NonZeroU64, roots: &[SharedRoot]) {
    let start = Instant::now();
    loop {
        match owner.acknowledge(epoch, roots) {
            Ok(()) => return,
            Err(RendezvousError::Busy) if start.elapsed() < DEADLINE => thread::yield_now(),
            Err(error) => panic!("acknowledge: {error:?}"),
        }
    }
}

fn restore(owner: &mut Mutator<'_>) {
    let mut image = begin_restore_retry_for_test(owner, false).expect("begin restore");
    let start = Instant::now();
    loop {
        match image.apply() {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && start.elapsed() < DEADLINE =>
            {
                image = failure.into_image();
                thread::yield_now();
            }
            Err(failure) => panic!("restore: {:?}", failure.reason),
        }
    }
}

fn retire(mut owner: Mutator<'_>) {
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

fn retry_reg<T>(label: &str, mut op: impl FnMut() -> Result<T, RegistrationError>) -> T {
    let start = Instant::now();
    loop {
        match op() {
            Ok(value) => return value,
            Err(RegistrationError::Busy) if start.elapsed() < DEADLINE => thread::yield_now(),
            Err(error) => panic!("{label}: {error:?}"),
        }
    }
}

fn retry_gate<T>(label: &str, mut op: impl FnMut() -> Result<T, RendezvousError>) -> T {
    let start = Instant::now();
    loop {
        match op() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) if start.elapsed() < DEADLINE => thread::yield_now(),
            Err(error) => panic!("{label}: {error:?}"),
        }
    }
}

fn end_collected(mut ticket: super::serial_gate::SerialTicket<'_, '_>) {
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

fn recv<T>(rx: &mpsc::Receiver<T>, label: &str) -> T {
    rx.recv_timeout(DEADLINE)
        .unwrap_or_else(|_| panic!("{label}"))
}

/// Inline the collection era. A higher-ranked callback would borrow the era
/// for its whole lifetime, and `finish` could not move it.
macro_rules! with_participant {
    (|$era:ident, $keeper:ident, $heap:ident, $rendezvous:ident, $permit:ident, $car:ident, $cdr:ident| $body:block) => {
        crate::emacs_core::eval::Context::with_test_collection_era(
            |context| {
                context.set_gc_stress_for_test(false);
                context.tagged_heap.set_gc_threshold(usize::MAX);
                (cons(context, 301), cons(context, 302))
            },
            |era_owned, ($car, $cdr)| {
                let $heap = era_owned.heap_identity();
                let $rendezvous = era_owned.rendezvous();
                let $permit = era_owned
                    .register(OWNER, 4)
                    .expect("register the participant");
                let $keeper = era_owned.keeper_for_test().expect("fixture keeper");
                let $era = &era_owned;
                $body
                let (collector, baton) = era_owned.finish().expect("finish the era");
                drop(collector);
                drop(baton);
            },
        );
    };
}

/// Retry `Busy` on the proof `lend_serial_for_test` returns. Dropping that
/// failure drops `AllStopped` and poisons the era, so the proof is moved out
/// before the next attempt.
fn lend_retrying<'era, 'heap>(
    era: &'era crate::emacs_core::eval::world::WorldProofEra<'heap>,
    keeper: &'era crate::emacs_core::eval::world::Baton,
    stopped: neovm_value::tagged::gc::AllStopped<'era>,
) -> super::serial_gate::SerialTicket<'era, 'heap> {
    let mut stopped = stopped;
    let start = Instant::now();
    loop {
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

fn collect_and_end<'era, 'heap>(
    era: &'era crate::emacs_core::eval::world::WorldProofEra<'heap>,
    keeper: &'era crate::emacs_core::eval::world::Baton,
    stopped: neovm_value::tagged::gc::AllStopped<'era>,
    catalog: bool,
) {
    if catalog {
        retry_reg("return cons", || era.return_cons_storage(&stopped));
    }
    let mut ticket = lend_retrying(era, keeper, stopped);
    retry_gate("serial_gc", || serial_gc_for_test(&mut ticket));
    if catalog {
        retry_reg("resume cons", || {
            era.resume_cons_storage(ticket.stopped_proof())
        });
    }
    end_collected(ticket);
}

/// (a) Ack, then the keeper runs the gate. Restore, allocate one cons, and a
/// later stop still owns that word.
#[test]
fn participant_allocates_after_end() {
    with_participant!(|era, keeper, heap, rendezvous, permit, car, cdr| {
        let (acked_tx, acked_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let (word_tx, word_rx) = mpsc::channel();
        let (acked2_tx, acked2_rx) = mpsc::channel();
        let (checked_tx, checked_rx) = mpsc::channel();
        // Parallel `run_access::cons` needs the catalog. It must be installed
        // before the participant activates, returned before `serial_gc`, and
        // resumed before `end` so the post-restore cons is admitted.
        era.start_cons_storage().expect("cons storage");
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let epoch = request_epoch(rendezvous);
                acknowledge(&mut owner, epoch, &[car.clone(), cdr.clone()]);
                acked_tx.send(()).expect("ack");
                // Stay out of `prepare_restore` until the ticket is gone.
                // A concurrent restore holds the registry lock, `lend` sees
                // `Busy`, and dropping `AllStopped` poisons the era.
                recv(&ready_rx, "first cycle ended");
                restore(&mut owner);
                let mut run = owner.run_access().expect("run");
                let made = run.cons(&car, &cdr).expect("cons");
                run.close_cons_region().expect("close the cons region");
                drop(run);
                let word = made.parallel_word();
                word_tx.send(word).expect("word");
                let epoch = request_epoch(rendezvous);
                acknowledge(&mut owner, epoch, &[made, car, cdr]);
                acked2_tx.send(()).expect("second ack");
                recv(&checked_rx, "reachable check");
                restore(&mut owner);
                retire(owner);
            });
            recv(&acked_rx, "first ack");
            let stopped = retry_gate("stop", || stop_for_test(era));
            collect_and_end(era, keeper, stopped, true);
            ready_tx.send(()).expect("first cycle ended");
            let word = recv(&word_rx, "allocated word");
            recv(&acked2_rx, "second ack");
            let stopped = retry_gate("second stop", || stop_for_test(era));
            retry_reg("second return cons", || era.return_cons_storage(&stopped));
            let mut ticket = lend_retrying(era, keeper, stopped);
            retry_gate("second serial_gc", || serial_gc_for_test(&mut ticket));
            let owned = era
                .with_stopped_heap_for_test(ticket.stopped_proof(), |heap| {
                    // SAFETY: `word` is `parallel_word` of the cons `run_access`
                    // allocated in this heap. The root is still held on the
                    // participant thread.
                    heap.owns_heap_value_for_test(unsafe { Value::from_bits(word) })
                })
                .expect("stopped heap");
            assert!(owned, "allocated cons is not reachable");
            // Leave the catalog returned. `finish` refuses a still-suspended
            // cons catalog as `OpenRegion`. The participant does not allocate
            // again.
            end_collected(ticket);
            checked_tx.send(()).expect("checked");
        });
        eprintln!("PENTRY_TSAN case=participant_alloc_after_end kind=PRODUCT");
    });
}

/// (b) While the ticket is held, restore does not return. The participant
/// enters `prepare_restore` only after the lend, because that wait holds the
/// registry lock and a failed lend would poison the era. `prepare_restore`
/// then waits through `Tracing` for a non-keeper; `apply` is not reached.
#[test]
fn participant_restore_waits_while_ticket_held() {
    with_participant!(|era, keeper, heap, rendezvous, permit, car, cdr| {
        let (acked_tx, acked_rx) = mpsc::channel::<()>();
        let (held_tx, held_rx) = mpsc::channel::<()>();
        let (entered_tx, entered_rx) = mpsc::channel::<()>();
        let (resumed_tx, resumed_rx) = mpsc::channel::<()>();
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let epoch = request_epoch(rendezvous);
                acknowledge(&mut owner, epoch, &[car, cdr]);
                acked_tx.send(()).expect("ack");
                recv(&held_rx, "ticket held");
                entered_tx.send(()).expect("entered");
                restore(&mut owner);
                resumed_tx.send(()).expect("resumed");
                retire(owner);
            });
            recv(&acked_rx, "ack");
            let stopped = retry_gate("stop", || stop_for_test(era));
            let mut ticket = lend_retrying(era, keeper, stopped);
            held_tx.send(()).expect("ticket held");
            recv(&entered_rx, "participant entered restore");
            thread::sleep(HELD_WINDOW);
            assert!(
                resumed_rx.try_recv().is_err(),
                "participant resumed while the ticket was held"
            );
            retry_gate("serial_gc", || serial_gc_for_test(&mut ticket));
            end_collected(ticket);
            recv(&resumed_rx, "resume after end");
        });
        eprintln!("PENTRY_TSAN case=restore_waits_while_ticket_held kind=PRODUCT");
    });
}

/// (c) End before collection is `NotCollected`. Restore still does not return
/// until the keeper collects and ends.
#[test]
fn retained_ticket_blocks_restore_until_end() {
    with_participant!(|era, keeper, heap, rendezvous, permit, car, cdr| {
        let (acked_tx, acked_rx) = mpsc::channel::<()>();
        let (held_tx, held_rx) = mpsc::channel::<()>();
        let (entered_tx, entered_rx) = mpsc::channel::<()>();
        let (resumed_tx, resumed_rx) = mpsc::channel::<()>();
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let epoch = request_epoch(rendezvous);
                acknowledge(&mut owner, epoch, &[car, cdr]);
                acked_tx.send(()).expect("ack");
                recv(&held_rx, "ticket retained");
                entered_tx.send(()).expect("entered");
                restore(&mut owner);
                resumed_tx.send(()).expect("resumed");
                retire(owner);
            });
            recv(&acked_rx, "ack");
            let stopped = retry_gate("stop", || stop_for_test(era));
            let ticket = lend_retrying(era, keeper, stopped);
            let retained = end_serial_for_test(ticket).expect_err("uncollected end");
            assert_eq!(retained.reason, SerialEndError::NotCollected);
            let mut ticket = retained.ticket;
            held_tx.send(()).expect("ticket retained");
            recv(&entered_rx, "participant entered restore");
            thread::sleep(HELD_WINDOW);
            assert!(
                resumed_rx.try_recv().is_err(),
                "participant resumed on a NotCollected ticket"
            );
            retry_gate("serial_gc", || serial_gc_for_test(&mut ticket));
            end_collected(ticket);
            recv(&resumed_rx, "resume after the retained ticket ends");
        });
        eprintln!("PENTRY_TSAN case=retained_ticket_blocks_restore kind=PRODUCT");
    });
}

/// (d) During `Tracing`, `request` returns `Active` for the in-progress epoch.
/// Immediately after end, while this owner still owes its resume, `request`
/// returns `Queued`. After restore clears that debt, a new `request` is
/// `Active` for a later epoch.
#[test]
fn request_during_ticket_then_admitted_after_end() {
    with_participant!(|era, keeper, heap, rendezvous, permit, car, cdr| {
        let (first_tx, first_rx) = mpsc::channel();
        let (holding_tx, holding_rx) = mpsc::channel::<()>();
        let (during_tx, during_rx) = mpsc::channel();
        let (ended_tx, ended_rx) = mpsc::channel::<()>();
        let (after_tx, after_rx) = mpsc::channel();
        let (admitted_tx, admitted_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let epoch = request_epoch(rendezvous);
                acknowledge(&mut owner, epoch, &[car.clone(), cdr.clone()]);
                first_tx.send(epoch).expect("first epoch");
                recv(&holding_rx, "ticket held");
                during_tx
                    .send(rendezvous.request())
                    .expect("request while held");
                recv(&ended_rx, "ticket ended");
                let after = rendezvous.request().expect("request after end");
                after_tx.send(after).expect("after");
                restore(&mut owner);
                let admitted = request_epoch(rendezvous);
                acknowledge(&mut owner, admitted, &[car, cdr]);
                admitted_tx.send(admitted).expect("admitted");
                recv(&done_rx, "admitted epoch ended");
                restore(&mut owner);
                retire(owner);
            });
            let first = recv(&first_rx, "first epoch");
            let stopped = retry_gate("stop", || stop_for_test(era));
            let mut ticket = lend_retrying(era, keeper, stopped);
            holding_tx.send(()).expect("holding");
            assert_eq!(
                recv(&during_rx, "request while held"),
                Ok(Request::Active(first))
            );
            retry_gate("serial_gc", || serial_gc_for_test(&mut ticket));
            end_collected(ticket);
            ended_tx.send(()).expect("ended");
            assert_eq!(recv(&after_rx, "request after end"), Request::Queued);
            let admitted = recv(&admitted_rx, "request after restore");
            assert_ne!(admitted, first);
            let stopped = retry_gate("cleanup stop", || stop_for_test(era));
            let mut ticket = lend_retrying(era, keeper, stopped);
            retry_gate("cleanup serial_gc", || serial_gc_for_test(&mut ticket));
            end_collected(ticket);
            done_tx.send(()).expect("done");
        });
        eprintln!("PENTRY_TSAN case=request_during_ticket kind=PRODUCT");
    });
}

/// (e) The participant holds the registry gate inside `prepare_restore`
/// before lend. `Busy` returns that same `AllStopped`. The keeper retries
/// it, then `serial_gc` and end succeed, and the participant restores.
#[test]
fn lend_races_participant_restore() {
    with_participant!(|era, keeper, heap, rendezvous, permit, car, cdr| {
        let (acked_tx, acked_rx) = mpsc::channel::<()>();
        let (stopped_tx, stopped_rx) = mpsc::channel::<()>();
        let (holding_tx, holding_rx) = mpsc::channel::<()>();
        let (resumed_tx, resumed_rx) = mpsc::channel::<()>();
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let epoch = request_epoch(rendezvous);
                acknowledge(&mut owner, epoch, &[car, cdr]);
                acked_tx.send(()).expect("ack");
                recv(&stopped_rx, "keeper stopped");
                install_restore_gate_hook_for_test(Box::new(move || {
                    holding_tx.send(()).expect("holding the gate");
                    thread::sleep(Duration::from_millis(500));
                }));
                restore(&mut owner);
                resumed_tx.send(()).expect("resumed");
                retire(owner);
            });
            recv(&acked_rx, "ack");
            let stopped = retry_gate("stop", || stop_for_test(era));
            stopped_tx.send(()).expect("keeper stopped");
            recv(&holding_rx, "participant holds the gate");
            let failure = lend_serial_for_test(era, keeper, stopped)
                .expect_err("lend while the gate is held");
            assert_eq!(failure.reason, RendezvousError::Busy);
            let mut ticket = lend_retrying(era, keeper, failure.stopped);
            retry_gate("serial_gc", || serial_gc_for_test(&mut ticket));
            end_collected(ticket);
            recv(&resumed_rx, "participant restored");
        });
        eprintln!("PENTRY_TSAN case=lend_races_participant_restore kind=PRODUCT");
    });
}
