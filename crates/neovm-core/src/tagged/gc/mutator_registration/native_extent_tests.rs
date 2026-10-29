//! Host-only native-safe extent fixtures (design 4.5 / section 7 A7): a
//! latch blocks one owner inside the production extent while a peer
//! allocates and the keeper collects, repeatedly. No Lisp mutex manager is
//! implemented here — the blocked syscall stand-in is a plain channel.
#![cfg(not(feature = "gc-memory-telemetry"))]

use super::rendezvous::entry_state;
use super::{
    AllStopped, HeapIdentity, Mode, Mutator, MutatorEntry, MutatorKey, MutatorPermit, NativeRoots,
    ProofEra as ParallelHeapEra, RegistrationError, RendezvousError, Request,
};
use crate::emacs_core::eval::Context;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

const SPIN_WAIT: Duration = Duration::from_secs(60);
const CHANNEL_WAIT: Duration = Duration::from_secs(90);

fn recv<T>(rx: &Receiver<T>) -> T {
    rx.recv_timeout(CHANNEL_WAIT)
        .unwrap_or_else(|error| panic!("owner channel closed: {error}"))
}

fn retry_rv<T>(mut op: impl FnMut() -> Result<T, RendezvousError>) -> T {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match op() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => panic!("rendezvous: {error:?}"),
        }
    }
}

fn retry_reg<T>(mut op: impl FnMut() -> Result<T, RegistrationError>) -> T {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match op() {
            Ok(value) => return value,
            Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => panic!("registration: {error:?}"),
        }
    }
}

fn with_run<'era, R>(
    owner: &mut Mutator<'era>,
    body: impl FnOnce(&mut super::RunAccess<'_, 'era>) -> Result<R, RegistrationError>,
) -> Result<R, RegistrationError> {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let result = body(&mut run);
                run.close_cons_region()?;
                return result;
            }
            Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => return Err(error),
        }
    }
}

fn activate<'era>(mut permit: MutatorPermit<'era>, heap: HeapIdentity, id: u64) -> Mutator<'era> {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match permit.activate(heap, id) {
            Ok(owner) => return owner,
            Err(failure)
                if failure.reason == RegistrationError::Busy && Instant::now() < deadline =>
            {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activate: {:?}", failure.reason),
        }
    }
}

fn restore(owner: &mut Mutator<'_>) {
    let mut image = owner
        .begin_restore_retry(false)
        .unwrap_or_else(|error| panic!("restore: {error:?}"));
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match image.apply() {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && Instant::now() < deadline =>
            {
                image = failure.into_image();
                thread::yield_now();
            }
            Err(failure) => panic!("apply: {:?}", failure.reason),
        }
    }
}

fn retire(mut owner: Mutator<'_>) {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && Instant::now() < deadline =>
            {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire: {:?}", failure.reason),
        }
    }
}

fn complete(mut stopped: AllStopped<'_>) {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match stopped.complete() {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && Instant::now() < deadline =>
            {
                stopped = failure.into_stopped();
                thread::yield_now();
            }
            Err(failure) => panic!("complete: {:?}", failure.reason),
        }
    }
}

fn on_heap<R>(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    body: impl Fn(&crate::tagged::gc::TaggedHeap) -> R,
) -> R {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match era.with_stopped_heap_for_test(stopped, &body) {
            Ok(value) => return value,
            Err(RendezvousError::Busy) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => panic!("stopped heap: {error:?}"),
        }
    }
}

fn admitted(body: impl FnOnce(&ParallelHeapEra, &SharedRoot, &SharedRoot)) {
    let mut context = Context::new();
    // SAFETY: nil and a fixnum are immediates of this Context's heap.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let one = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::fixnum(1)) };
    context
        .admit_owned_proof_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                body(era, &nil, &one);
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
}

/// What the keeper learns when an owner is inside (or entering) an extent.
struct NativeArmed {
    key: MutatorKey,
    entry: Arc<MutatorEntry>,
    heap: HeapIdentity,
    words: Vec<usize>,
}

enum PeerSignal {
    Alloc { count: usize, reply: Sender<()> },
    Collect,
    Finish,
}

enum NativeSignal {
    Collect,
    Finish,
}

fn alloc(peer_tx: &Sender<PeerSignal>, count: usize) {
    let (reply_tx, reply_rx) = channel();
    peer_tx
        .send(PeerSignal::Alloc {
            count,
            reply: reply_tx,
        })
        .unwrap_or_else(|_| panic!("peer channel closed"));
    reply_rx
        .recv_timeout(CHANNEL_WAIT)
        .unwrap_or_else(|error| panic!("alloc reply: {error}"));
}

/// Ordinary ACKer thread: allocates on request and serves one keeper round
/// per `Collect`.
///
/// The caller MUST receive `activated` before opening any keeper round: a
/// peer that registers while a round is already active joins as
/// `StartingStopped`, is excluded from that round's frozen membership, and
/// shifts the one-`Collect`-per-round pairing so a trailing `Collect` can
/// outlive the last activation and park in `wait_request` forever.
fn peer_loop<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
    one: &SharedRoot,
    rx: &Receiver<PeerSignal>,
    activated: &Sender<()>,
) {
    let mut owner = activate(permit, heap, id);
    activated
        .send(())
        .unwrap_or_else(|_| panic!("activated channel closed"));
    let mut head = one.clone();
    while let Ok(signal) = rx.recv() {
        match signal {
            PeerSignal::Alloc { count, reply } => {
                for _ in 0..count {
                    head = with_run(&mut owner, |run| run.cons(one, &head))
                        .unwrap_or_else(|error| panic!("peer alloc: {error:?}"));
                }
                reply
                    .send(())
                    .unwrap_or_else(|_| panic!("alloc reply channel closed"));
            }
            PeerSignal::Collect => {
                let epoch = retry_rv(|| owner.wait_request());
                retry_rv(|| owner.acknowledge(epoch, &[head.clone(), one.clone()]));
                restore(&mut owner);
            }
            PeerSignal::Finish => break,
        }
    }
    retry_rv(|| owner.enter_native(&[]));
    retire(owner);
}

/// One owner blocks once inside the production extent, then serves ordinary
/// stop rounds as an ACKer until `Finish`.
fn blocked_owner_thread<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
    nil: &SharedRoot,
    one: &SharedRoot,
    entered_tx: Sender<NativeArmed>,
    wake_rx: Receiver<()>,
    returned_tx: Sender<u64>,
    rx: &Receiver<NativeSignal>,
) {
    let mut owner = activate(permit, heap, id);
    let (cell, spine) = with_run(&mut owner, |run| {
        let cell = run.cons(one, nil)?;
        let spine = run.cons(one, &cell)?;
        Ok((cell, spine))
    })
    .unwrap_or_else(|error| panic!("native owner alloc: {error:?}"));
    let roots = [spine.clone(), cell.clone(), one.clone()];
    let key = owner.key();
    let entry = owner.entry();
    let out = owner
        .with_native_extent(&roots, |handles| {
            entered_tx
                .send(NativeArmed {
                    key,
                    entry,
                    heap: handles.heap_identity(),
                    words: (0..handles.len())
                        .map(|index| handles.word(index))
                        .collect(),
                })
                .unwrap_or_else(|_| panic!("entered channel closed"));
            recv(&wake_rx)
        })
        .unwrap_or_else(|error| panic!("native extent: {error:?}"));
    returned_tx
        .send(out.epoch)
        .unwrap_or_else(|_| panic!("returned channel closed"));
    loop {
        match recv(rx) {
            NativeSignal::Collect => {
                let epoch = retry_rv(|| owner.wait_request());
                retry_rv(|| owner.acknowledge(epoch, &roots));
                restore(&mut owner);
            }
            NativeSignal::Finish => break,
        }
    }
    retry_rv(|| owner.enter_native(&[]));
    retire(owner);
}

/// The race fixture owner for return-race and lost-wakeup rounds: it arms,
/// runs one extent per round, and acks exactly those requests that are
/// already active when the keeper serves it — a native publication never
/// needs this fallback, an already-returned owner does.
fn serving_racer_thread<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
    one: &SharedRoot,
    rounds: usize,
    armed_tx: Sender<NativeArmed>,
    go_rx: Receiver<()>,
    wake_rx: Receiver<u64>,
    returned_tx: Sender<(u64, u64)>,
    serve_rx: Receiver<()>,
    settled_tx: Sender<()>,
) {
    let mut owner = activate(permit, heap, id);
    let key = owner.key();
    let entry = owner.entry();
    let mut cell = one.clone();
    for _ in 0..rounds {
        cell = with_run(&mut owner, |run| run.cons(one, &cell))
            .unwrap_or_else(|error| panic!("racer alloc: {error:?}"));
        let roots = [cell.clone(), one.clone()];
        armed_tx
            .send(NativeArmed {
                key,
                entry: entry.clone(),
                heap,
                words: roots.iter().map(|root| root.parallel_word()).collect(),
            })
            .unwrap_or_else(|_| panic!("armed channel closed"));
        recv(&go_rx);
        let out = owner
            .with_native_extent(&roots, |handles| {
                assert_eq!(handles.len(), 2);
                recv(&wake_rx)
            })
            .unwrap_or_else(|error| panic!("racer extent: {error:?}"));
        returned_tx
            .send((out.epoch, out.result))
            .unwrap_or_else(|_| panic!("racer returned channel closed"));
        recv(&serve_rx);
        // The epoch probe takes entry locks; retry transient Busy contention
        // instead of mistaking it for a protocol verdict.
        let deadline = Instant::now() + SPIN_WAIT;
        loop {
            match owner.requested_epoch() {
                Ok(Some(epoch)) => {
                    retry_rv(|| owner.acknowledge(epoch, &roots));
                    restore(&mut owner);
                    break;
                }
                Ok(None) => break,
                Err(RendezvousError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("racer requested_epoch: {error:?}"),
            }
        }
        settled_tx
            .send(())
            .unwrap_or_else(|_| panic!("settled channel closed"));
    }
    retry_rv(|| owner.enter_native(&[]));
    retire(owner);
}

/// Read a frozen owner's publication words. The reader consults entry-state
/// try-locks; a Busy here is transient contention with the owner's own
/// publication machinery, never a protocol verdict.
fn root_reader_words(stopped: &AllStopped<'_>, armed: &NativeArmed) -> Vec<usize> {
    let deadline = Instant::now() + SPIN_WAIT;
    let mut words = Vec::new();
    loop {
        let attempt = stopped.with_root_reader(armed.key, |reader| {
            reader.append_words(&mut words);
            reader.finish()
        });
        match attempt {
            Ok(Ok(())) => return words,
            Ok(Err(error)) => panic!("root batch: {error:?}"),
            Err(RendezvousError::Busy) if Instant::now() < deadline => {
                words.clear();
                thread::yield_now();
            }
            Err(error) => panic!("root reader: {error:?}"),
        }
    }
}

fn entry_image_present(armed: &NativeArmed) -> bool {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match entry_state(&armed.entry) {
            Ok(state) => return state.image.is_some(),
            Err(RendezvousError::Busy) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => panic!("entry: {error:?}"),
        }
    }
}

/// The keeper's per-round view of the extent owner: frozen with the expected
/// mode, an unchanged immutable root record, its image still keeper-owned,
/// and its retained cons alive on the stopped heap.
fn check_stopped(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    armed: &NativeArmed,
    expected: Mode,
) {
    let snapshot = retry_rv(|| era.rendezvous().snapshot());
    let mode = snapshot
        .owners
        .iter()
        .find(|(key, _)| *key == armed.key)
        .map(|(_, mode)| *mode)
        .expect("extent owner still in membership");
    assert_eq!(mode, expected);
    drop(snapshot);
    let words = root_reader_words(stopped, armed);
    assert_eq!(words, armed.words, "immutable retained-root record");
    assert!(entry_image_present(armed), "keeper owns the stopped image");
    on_heap(era, stopped, |heap| {
        let retained = TaggedValue::from_bits(armed.words[0]);
        assert!(
            heap.owns_heap_value_for_test(retained),
            "retained cons survived the collection"
        );
    });
}

fn open_round<'era>(
    era: &'era ParallelHeapEra<'_>,
    peer_tx: &Sender<PeerSignal>,
    next_epoch: &mut u64,
) -> AllStopped<'era> {
    peer_tx
        .send(PeerSignal::Collect)
        .unwrap_or_else(|_| panic!("peer channel closed"));
    let stopped = retry_rv(|| era.stop());
    let epoch = stopped.epoch().get();
    assert_eq!(epoch, *next_epoch, "collection epochs stay strictly +1");
    *next_epoch += 1;
    retry_reg(|| era.return_cons_storage(&stopped));
    stopped
}

/// An interior round: collect, then reinstall the cons catalog before the
/// owners resume allocating.
fn finish_round(era: &ParallelHeapEra<'_>, stopped: AllStopped<'_>) {
    let mut stopped = stopped;
    let report = retry_rv(|| era.collect_stopped(&mut stopped));
    assert!(report.major, "extent rounds collect a major cycle");
    retry_reg(|| era.resume_cons_storage(&stopped));
    complete(stopped);
}

/// The era's closing round. Every owner is NativeStopped or Retired, so the
/// stop counts their publications without an ACK, and the cons catalog stays
/// returned on the heap: `finish` refuses a suspended cons catalog.
fn closing_round(era: &ParallelHeapEra<'_>, next_epoch: &mut u64) {
    let mut stopped = retry_rv(|| era.stop());
    let epoch = stopped.epoch().get();
    assert_eq!(epoch, *next_epoch, "collection epochs stay strictly +1");
    *next_epoch += 1;
    retry_reg(|| era.return_cons_storage(&stopped));
    let report = retry_rv(|| era.collect_stopped(&mut stopped));
    assert!(report.major, "the closing round collects a major cycle");
    complete(stopped);
}

#[test]
fn a7_extent_blocks_while_peer_allocates_and_keeper_collects_repeatedly() {
    admitted(|era, nil, one| {
        // Design 3.3: both fixed participants register while stopped, before
        // any owner thread exists.
        let peer_permit = retry_reg(|| era.register(6701, 2));
        let native_permit = retry_reg(|| era.register(6702, 2));
        let (peer_tx, peer_rx) = channel();
        let (native_tx, native_rx) = channel();
        let (entered_tx, entered_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let (activated_tx, activated_rx) = channel();
        let heap = era.heap_identity();
        thread::scope(|scope| {
            scope.spawn(move || peer_loop(peer_permit, heap, 6701, one, &peer_rx, &activated_tx));
            scope.spawn(move || {
                blocked_owner_thread(
                    native_permit,
                    heap,
                    6702,
                    nil,
                    one,
                    entered_tx,
                    wake_rx,
                    returned_tx,
                    &native_rx,
                )
            });
            // Gate on full peer membership before the first round.
            recv(&activated_rx);
            alloc(&peer_tx, 3);
            let entered = recv(&entered_rx);
            assert_eq!(entered.words.len(), 3);
            assert_eq!(entered.heap, era.heap_identity());
            let mut next_epoch = 1u64;
            for _ in 0..3 {
                alloc(&peer_tx, 2);
                let stopped = open_round(era, &peer_tx, &mut next_epoch);
                let epoch = stopped.epoch().get();
                check_stopped(era, &stopped, &entered, Mode::NativeStopped);
                let snapshot = retry_rv(|| era.rendezvous().snapshot());
                let peer_mode = snapshot
                    .owners
                    .iter()
                    .find(|(key, _)| key.owner == 6701)
                    .map(|(_, mode)| *mode)
                    .expect("peer in membership");
                assert_eq!(peer_mode, Mode::Stopped);
                drop(snapshot);
                // The blocked owner's retained structure is intact on the
                // stopped heap: spine = (1 cell), cell = (1 nil).
                on_heap(era, &stopped, |heap| {
                    let spine = TaggedValue::from_bits(entered.words[0]);
                    let cell = TaggedValue::from_bits(entered.words[1]);
                    assert!(heap.owns_heap_value_for_test(spine));
                    assert!(heap.owns_heap_value_for_test(cell));
                    assert_eq!(spine.cons_cdr_unobserved().bits(), cell.bits());
                    assert_eq!(
                        cell.cons_car_unobserved().bits(),
                        TaggedValue::fixnum(1).bits()
                    );
                    assert!(cell.cons_cdr_unobserved().is_nil());
                });
                finish_round(era, stopped);
                // The keeper folded its latest image into the entry in place.
                let state = entry_state(&entered.entry).unwrap_or_else(|e| panic!("entry: {e:?}"));
                assert_eq!(state.rendezvous.image_epoch, epoch);
            }
            wake_tx
                .send(())
                .unwrap_or_else(|_| panic!("wake channel closed"));
            let returned = recv(&returned_rx);
            assert_eq!(returned, 3, "return applied the latest image");
            // The owner is Running again and participates as an ordinary
            // ACKer: one more round through its acknowledged publication.
            native_tx
                .send(NativeSignal::Collect)
                .unwrap_or_else(|_| panic!("native channel closed"));
            let stopped = open_round(era, &peer_tx, &mut next_epoch);
            check_stopped(era, &stopped, &entered, Mode::Stopped);
            finish_round(era, stopped);
            peer_tx
                .send(PeerSignal::Finish)
                .unwrap_or_else(|_| panic!("peer channel closed"));
            native_tx
                .send(NativeSignal::Finish)
                .unwrap_or_else(|_| panic!("native channel closed"));
            closing_round(era, &mut next_epoch);
        });
    });
}

#[test]
fn a7_wake_during_active_stop_cannot_run_until_resume() {
    admitted(|era, nil, one| {
        let peer_permit = retry_reg(|| era.register(6711, 2));
        let native_permit = retry_reg(|| era.register(6712, 2));
        let (peer_tx, peer_rx) = channel();
        let (native_tx, native_rx) = channel();
        let (entered_tx, entered_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let (activated_tx, activated_rx) = channel();
        let heap = era.heap_identity();
        thread::scope(|scope| {
            scope.spawn(move || peer_loop(peer_permit, heap, 6711, one, &peer_rx, &activated_tx));
            scope.spawn(move || {
                blocked_owner_thread(
                    native_permit,
                    heap,
                    6712,
                    nil,
                    one,
                    entered_tx,
                    wake_rx,
                    returned_tx,
                    &native_rx,
                )
            });
            // Gate on full peer membership before the first round.
            recv(&activated_rx);
            alloc(&peer_tx, 2);
            let entered = recv(&entered_rx);
            let mut next_epoch = 1u64;
            let stopped = open_round(era, &peer_tx, &mut next_epoch);
            // The keeper holds the traced stop; only now does the blocked
            // syscall "return". The protocol — not timing — keeps the return
            // from running: it can only proceed through Resuming after
            // complete, so a completed message cannot exist yet.
            wake_tx
                .send(())
                .unwrap_or_else(|_| panic!("wake channel closed"));
            assert!(
                matches!(
                    returned_rx.recv_timeout(Duration::from_millis(250)),
                    Err(RecvTimeoutError::Timeout)
                ),
                "native return ran during an active stop"
            );
            check_stopped(era, &stopped, &entered, Mode::NativeStopped);
            finish_round(era, stopped);
            let returned = recv(&returned_rx);
            assert_eq!(returned, 1, "return applied the stop's image");
            // Resumed owner acks an ordinary round again.
            native_tx
                .send(NativeSignal::Collect)
                .unwrap_or_else(|_| panic!("native channel closed"));
            let stopped = open_round(era, &peer_tx, &mut next_epoch);
            check_stopped(era, &stopped, &entered, Mode::Stopped);
            finish_round(era, stopped);
            peer_tx
                .send(PeerSignal::Finish)
                .unwrap_or_else(|_| panic!("peer channel closed"));
            native_tx
                .send(NativeSignal::Finish)
                .unwrap_or_else(|_| panic!("native channel closed"));
            closing_round(era, &mut next_epoch);
        });
    });
}

#[test]
fn a7_native_entry_races_request() {
    admitted(|era, _nil, one| {
        let peer_permit = retry_reg(|| era.register(6721, 2));
        let racer_permit = retry_reg(|| era.register(6722, 2));
        let (peer_tx, peer_rx) = channel();
        let (armed_tx, armed_rx) = channel();
        let (go_tx, go_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        let (activated_tx, activated_rx) = channel();
        let heap = era.heap_identity();
        thread::scope(|scope| {
            scope.spawn(move || peer_loop(peer_permit, heap, 6721, one, &peer_rx, &activated_tx));
            scope.spawn(move || {
                let mut owner = activate(racer_permit, heap, 6722);
                let key = owner.key();
                let entry = owner.entry();
                let mut cell = one.clone();
                for _ in 0..100 {
                    cell = with_run(&mut owner, |run| run.cons(one, &cell))
                        .unwrap_or_else(|error| panic!("racer alloc: {error:?}"));
                    let roots = [cell.clone(), one.clone()];
                    armed_tx
                        .send(NativeArmed {
                            key,
                            entry: entry.clone(),
                            heap,
                            words: roots.iter().map(|root| root.parallel_word()).collect(),
                        })
                        .unwrap_or_else(|_| panic!("armed channel closed"));
                    recv(&go_rx);
                    let out = owner
                        .with_native_extent(&roots, |_| recv(&wake_rx))
                        .unwrap_or_else(|error| panic!("racer extent: {error:?}"));
                    returned_tx
                        .send(out.epoch)
                        .unwrap_or_else(|_| panic!("racer returned channel closed"));
                }
                retry_rv(|| owner.enter_native(&[]));
                retire(owner);
            });
            // Gate on full peer membership before the first round.
            recv(&activated_rx);
            let mut next_epoch = 1u64;
            for round in 0..100u64 {
                let armed = recv(&armed_rx);
                go_tx
                    .send(())
                    .unwrap_or_else(|_| panic!("go channel closed"));
                // Alternate who gets a head start: even rounds let the entry
                // begin, odd rounds activate the request immediately.
                if round % 2 == 0 {
                    thread::yield_now();
                }
                let stopped = open_round(era, &peer_tx, &mut next_epoch);
                let epoch = stopped.epoch().get();
                // The stop could only complete with the racer counted: either
                // its publication predates the request (rebound to this
                // epoch) or it was validated against the active request.
                check_stopped(era, &stopped, &armed, Mode::NativeStopped);
                finish_round(era, stopped);
                wake_tx
                    .send(())
                    .unwrap_or_else(|_| panic!("wake channel closed"));
                let returned = recv(&returned_rx);
                assert_eq!(returned, epoch, "entry publication doubles as the ACK");
            }
            peer_tx
                .send(PeerSignal::Finish)
                .unwrap_or_else(|_| panic!("peer channel closed"));
            closing_round(era, &mut next_epoch);
        });
    });
}

#[test]
fn a7_native_return_races_request() {
    admitted(|era, _nil, one| {
        let peer_permit = retry_reg(|| era.register(6731, 2));
        let racer_permit = retry_reg(|| era.register(6732, 2));
        let (peer_tx, peer_rx) = channel();
        let (armed_tx, armed_rx) = channel();
        let (go_tx, go_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        let (serve_tx, serve_rx) = channel();
        let (settled_tx, settled_rx) = channel();
        let (activated_tx, activated_rx) = channel();
        let heap = era.heap_identity();
        thread::scope(|scope| {
            scope.spawn(move || peer_loop(peer_permit, heap, 6731, one, &peer_rx, &activated_tx));
            scope.spawn(move || {
                serving_racer_thread(
                    racer_permit,
                    heap,
                    6732,
                    one,
                    100,
                    armed_tx,
                    go_rx,
                    wake_rx,
                    returned_tx,
                    serve_rx,
                    settled_tx,
                )
            });
            // Gate on full peer membership before the first round.
            recv(&activated_rx);
            let mut next_epoch = 1u64;
            for round in 0..100u64 {
                let armed = recv(&armed_rx);
                let token = round + 7;
                go_tx
                    .send(())
                    .unwrap_or_else(|_| panic!("go channel closed"));
                wake_tx
                    .send(token)
                    .unwrap_or_else(|_| panic!("wake channel closed"));
                // Even rounds give the return a head start past the gate;
                // odd rounds activate the request first.
                if round % 2 == 0 {
                    thread::yield_now();
                    thread::yield_now();
                }
                // Activate before serving the racer, so its ack decision can
                // never miss the epoch it must answer; a queued request waits
                // for the racer's return debt to drain.
                loop {
                    match retry_rv(|| era.rendezvous().request()) {
                        Request::Active(_) => break,
                        Request::Queued => thread::yield_now(),
                    }
                }
                serve_tx
                    .send(())
                    .unwrap_or_else(|_| panic!("serve channel closed"));
                let stopped = open_round(era, &peer_tx, &mut next_epoch);
                let epoch = stopped.epoch().get();
                // Won by the return: Stopped through the racer's ack. Won by
                // the request: NativeStopped, its publication counted.
                check_stopped_return_race(era, &stopped, &armed);
                finish_round(era, stopped);
                let (out_epoch, out_token) = recv(&returned_rx);
                assert_eq!(out_token, token, "native result crosses unchanged");
                assert!(
                    out_epoch == epoch || out_epoch == epoch - 1,
                    "return applied the latest image ({out_epoch} at epoch {epoch})"
                );
                recv(&settled_rx);
            }
            peer_tx
                .send(PeerSignal::Finish)
                .unwrap_or_else(|_| panic!("peer channel closed"));
            closing_round(era, &mut next_epoch);
        });
    });
}

fn check_stopped_return_race(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    armed: &NativeArmed,
) {
    let snapshot = retry_rv(|| era.rendezvous().snapshot());
    let mode = snapshot
        .owners
        .iter()
        .find(|(key, _)| *key == armed.key)
        .map(|(_, mode)| *mode)
        .expect("racer in membership");
    assert!(
        matches!(mode, Mode::NativeStopped | Mode::Stopped),
        "racer frozen with a complete publication ({mode:?})"
    );
    drop(snapshot);
    let words = root_reader_words(stopped, armed);
    assert_eq!(words, armed.words, "immutable retained-root record");
    on_heap(era, stopped, |heap| {
        let retained = TaggedValue::from_bits(armed.words[0]);
        assert!(heap.owns_heap_value_for_test(retained));
    });
}

#[test]
fn a7_lost_wakeup_stress_rotating_schedules() {
    admitted(|era, _nil, one| {
        let peer_permit = retry_reg(|| era.register(6741, 2));
        let racer_permit = retry_reg(|| era.register(6742, 2));
        let (peer_tx, peer_rx) = channel();
        let (armed_tx, armed_rx) = channel();
        let (go_tx, go_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        let (serve_tx, serve_rx) = channel();
        let (settled_tx, settled_rx) = channel();
        let (activated_tx, activated_rx) = channel();
        let heap = era.heap_identity();
        thread::scope(|scope| {
            scope.spawn(move || peer_loop(peer_permit, heap, 6741, one, &peer_rx, &activated_tx));
            scope.spawn(move || {
                serving_racer_thread(
                    racer_permit,
                    heap,
                    6742,
                    one,
                    100,
                    armed_tx,
                    go_rx,
                    wake_rx,
                    returned_tx,
                    serve_rx,
                    settled_tx,
                )
            });
            // Gate on full peer membership before the first round: an
            // ungated peer can register mid-round (StartingStopped) and
            // strand a trailing Collect after the closing round.
            recv(&activated_rx);
            let mut next_epoch = 1u64;
            for round in 0..100u64 {
                let armed = recv(&armed_rx);
                let token = round + 31;
                go_tx
                    .send(())
                    .unwrap_or_else(|_| panic!("go channel closed"));
                // Rotating wake schedules against one keeper round each:
                // 0 — complete long before the wake (a lost-wakeup notify),
                // 1 — wake while the request is actively tracing,
                // 2 — wake before any request (an idle return).
                let schedule = round % 3;
                if schedule == 2 {
                    wake_tx
                        .send(token)
                        .unwrap_or_else(|_| panic!("wake channel closed"));
                }
                // Activate before serving the racer for every schedule: the
                // request is live before its ack decision, and a queued
                // request waits for the racer's return debt to drain.
                loop {
                    match retry_rv(|| era.rendezvous().request()) {
                        Request::Active(_) => break,
                        Request::Queued => thread::yield_now(),
                    }
                }
                serve_tx
                    .send(())
                    .unwrap_or_else(|_| panic!("serve channel closed"));
                let stopped = open_round(era, &peer_tx, &mut next_epoch);
                let epoch = stopped.epoch().get();
                match schedule {
                    1 => {
                        wake_tx
                            .send(token)
                            .unwrap_or_else(|_| panic!("wake channel closed"));
                        assert!(
                            matches!(
                                returned_rx.recv_timeout(Duration::from_millis(250)),
                                Err(RecvTimeoutError::Timeout)
                            ),
                            "return ran while the stop was still held"
                        );
                        check_stopped(era, &stopped, &armed, Mode::NativeStopped);
                    }
                    0 => check_stopped(era, &stopped, &armed, Mode::NativeStopped),
                    // The schedule-2 wake only unblocked the body; whether the
                    // return beat this request decides Stopped vs
                    // NativeStopped, and both keep a complete publication.
                    _ => check_stopped_return_race(era, &stopped, &armed),
                }
                finish_round(era, stopped);
                if schedule == 0 {
                    wake_tx
                        .send(token)
                        .unwrap_or_else(|_| panic!("wake channel closed"));
                }
                let (out_epoch, out_token) = recv(&returned_rx);
                assert_eq!(out_token, token);
                assert!(
                    out_epoch == epoch || out_epoch == epoch - 1,
                    "return applied a consistent image ({out_epoch} at epoch {epoch})"
                );
                recv(&settled_rx);
            }
            peer_tx
                .send(PeerSignal::Finish)
                .unwrap_or_else(|_| panic!("peer channel closed"));
            closing_round(era, &mut next_epoch);
        });
    });
}

#[test]
fn a7_extent_handle_grants_no_thread_or_capability_escape() {
    static_assertions::assert_not_impl_any!(NativeRoots<'static>: Send, Sync, Clone, Copy);
    // The handle type reaches no Mutator, RunAccess, heap, or slice
    // capability: its only data are a heap identity and read-only words, and
    // it cannot leave the owner's thread. The blocked-owner tests above
    // exercise the same words through the keeper's root reader.
    //
    // Epoch contract: with no collection in flight, the extent returns
    // epoch zero — no image was applied.
    admitted(|era, nil, one| {
        let permit = retry_reg(|| era.register(6751, 2));
        let (native_tx, native_rx) = channel();
        let (entered_tx, entered_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let heap = era.heap_identity();
        thread::scope(|scope| {
            scope.spawn(move || {
                blocked_owner_thread(
                    permit,
                    heap,
                    6751,
                    nil,
                    one,
                    entered_tx,
                    wake_rx,
                    returned_tx,
                    &native_rx,
                )
            });
            let entered = recv(&entered_rx);
            assert_eq!(entered.words.len(), 3);
            assert_eq!(entered.heap, era.heap_identity());
            assert_eq!(
                entered.words[2],
                one.parallel_word(),
                "handle word is the retained root word"
            );
            wake_tx
                .send(())
                .unwrap_or_else(|_| panic!("wake channel closed"));
            let returned = recv(&returned_rx);
            assert_eq!(returned, 0, "no collection covered the extent");
            // A later collection acks through the resumed owner without
            // rewriting the already-returned epoch answer.
            let mut next_epoch = 1u64;
            native_tx
                .send(NativeSignal::Collect)
                .unwrap_or_else(|_| panic!("native channel closed"));
            let mut stopped = retry_rv(|| era.stop());
            assert_eq!(stopped.epoch().get(), next_epoch);
            next_epoch += 1;
            retry_reg(|| era.return_cons_storage(&stopped));
            check_stopped(era, &stopped, &entered, Mode::Stopped);
            finish_round(era, stopped);
            native_tx
                .send(NativeSignal::Finish)
                .unwrap_or_else(|_| panic!("native channel closed"));
            closing_round(era, &mut next_epoch);
        });
    });
}
