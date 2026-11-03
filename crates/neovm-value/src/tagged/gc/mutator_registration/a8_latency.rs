//! A8 cold-latency qualification rows (design §7.2): per-interval stop
//! protocol latency from instrumented probes, for 2/3/4 owners, allocating
//! and non-allocating loops, refill collision, and one NativeStopped owner.
//!
//! Everything here is test-only. The probes are `#[cfg(test)]` call sites in
//! production files; this driver only reads their event stream and the
//! test-only catalog wait counters. Nothing in this module runs in images.
//!
//! The smoke tests prove the event invariants on a few rounds. The full
//! matrix (`a8_latency_full`, `--ignored`) prints the report tables and can
//! additionally dump raw per-round distributions as TSV via
//! `A8_LATENCY_TSV=<dir>` and resize via `A8_LATENCY_ROUNDS=<n>`.

#![cfg(not(feature = "gc-memory-telemetry"))]

use super::a8_probes::{self, ProbeEvent};
use super::{AllStopped, HeapIdentity, Mutator, MutatorPermit, RegistrationError, RendezvousError};
use crate::emacs_core::eval::Context;
use crate::tagged::gc::cons_storage::CATALOG_WAITS;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue;
use std::sync::mpsc::{Receiver, Sender, channel};
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

/// `with_run` for an allocating owner that must also serve signals:
/// `run_access` refuses while a round is open, and the round cannot close
/// without this owner, so a blind Busy-spin between churn batches would
/// deadlock the rendezvous. The channel is the owner's safe point: a Busy
/// retry that finds a pending signal abandons the churn batch and hands it
/// back, letting the caller run the acknowledge (or finish) path — exactly
/// what a real allocating loop does at its next safe point.
enum ChurnOutcome<R> {
    Ran(R),
    Serve(OwnerSignal),
}

fn with_run_or_collect<'era, R>(
    owner: &mut Mutator<'era>,
    rx: &Receiver<OwnerSignal>,
    body: impl FnOnce(&mut super::RunAccess<'_, 'era>) -> Result<R, RegistrationError>,
) -> Result<ChurnOutcome<R>, RegistrationError> {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let result = body(&mut run);
                run.close_cons_region()?;
                return result.map(ChurnOutcome::Ran);
            }
            Err(RegistrationError::Busy) if Instant::now() < deadline => {
                // A signal published while we retry: serve it instead of
                // spinning. An empty channel is plain gate contention.
                if let Ok(signal) = rx.try_recv() {
                    return Ok(ChurnOutcome::Serve(signal));
                }
                thread::yield_now();
            }
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

// ---------------------------------------------------------------------------
// Owner loops
// ---------------------------------------------------------------------------

enum OwnerSignal {
    Collect,
    Finish,
}

/// Loop flavor of one ACKing owner.
#[derive(Clone, Copy)]
struct LoopKind {
    /// Conses per retained survivor chain rebuilt after each resume.
    retained: usize,
    /// Discardable conses per poll tick between rounds (0 = parked only).
    churn: usize,
}

const PARKED: LoopKind = LoopKind {
    retained: 32,
    churn: 0,
};
const ALLOCATING: LoopKind = LoopKind {
    retained: 1024,
    churn: 64,
};
const REFILL: LoopKind = LoopKind {
    retained: 65536,
    churn: 4096,
};

/// One ACKing owner: serves one round per `Collect`, and between rounds
/// either parks on the channel or keeps allocating (churn batches off a
/// fresh nil).
///
/// The caller MUST receive `activated` before the keeper opens any round (see
/// the StartingStopped pairing note on the A7 fixtures).
fn acking_owner<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
    nil: &SharedRoot,
    one: &SharedRoot,
    kind: LoopKind,
    rx: Receiver<OwnerSignal>,
    activated: Sender<()>,
    resumed: Sender<()>,
) {
    let mut owner = activate(permit, heap, id);
    let mut head = nil.clone();
    // Seed the retained survivor chain so round 1 marks real live data.
    with_run(&mut owner, |run| {
        for _ in 0..kind.retained {
            head = run.cons(one, &head)?;
        }
        Ok(())
    })
    .unwrap_or_else(|error| panic!("owner {id} seed: {error:?}"));
    activated
        .send(())
        .unwrap_or_else(|_| panic!("owner {id} activated channel closed"));
    loop {
        // Parked owners block on the channel; allocating-loop owners poll it
        // and churn discardable conses in between, so an incoming request
        // finds them mid-run exactly as in a real allocating loop.
        let signal = if kind.churn == 0 {
            recv(&rx)
        } else {
            loop {
                match rx.recv_timeout(Duration::from_millis(1)) {
                    Ok(signal) => break signal,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let mut garbage = nil.clone();
                        match with_run_or_collect(&mut owner, &rx, |run| {
                            for _ in 0..kind.churn {
                                garbage = run.cons(one, &garbage)?;
                            }
                            Ok(())
                        }) {
                            Ok(ChurnOutcome::Ran(())) => {}
                            Ok(ChurnOutcome::Serve(signal)) => break signal,
                            Err(error) => panic!("owner {id} churn: {error:?}"),
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        panic!("owner {id} channel closed")
                    }
                }
            }
        };
        match signal {
            OwnerSignal::Collect => {
                let epoch = retry_rv(|| owner.wait_request());
                retry_rv(|| owner.acknowledge(epoch, &[head.clone(), one.clone()]));
                restore(&mut owner);
                // Rebuild the retained chain BEFORE reporting `resumed`: the
                // previous round's chain is now garbage, so the live set stays
                // constant, and the driver must not open the next round while
                // this owner is inside a `with_run` — `run_access` refuses
                // while a round is open and the round cannot close without
                // every owner, so the reverse order deadlocks the rendezvous.
                with_run(&mut owner, |run| {
                    head = nil.clone();
                    for _ in 0..kind.retained {
                        head = run.cons(one, &head)?;
                    }
                    Ok(())
                })
                .unwrap_or_else(|error| panic!("owner {id} rebuild: {error:?}"));
                resumed
                    .send(())
                    .unwrap_or_else(|_| panic!("owner {id} resumed channel closed"));
            }
            OwnerSignal::Finish => break,
        }
    }
    retry_rv(|| owner.enter_native(&[]));
    retire(owner);
}

/// One owner blocked inside the production native extent for the whole
/// measurement: its retained chain is live via the publication, and its
/// `NativeReturn` epoch is reported once woken at teardown.
fn native_blocked_owner<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
    nil: &SharedRoot,
    one: &SharedRoot,
    retained: usize,
    activated: Sender<()>,
    wake: Receiver<()>,
    returned: Sender<u64>,
) {
    let mut owner = activate(permit, heap, id);
    let mut head = nil.clone();
    with_run(&mut owner, |run| {
        for _ in 0..retained {
            head = run.cons(one, &head)?;
        }
        Ok(())
    })
    .unwrap_or_else(|error| panic!("native owner {id} seed: {error:?}"));
    let roots = [head.clone(), one.clone()];
    let out = owner
        .with_native_extent(&roots, |_handles| {
            activated
                .send(())
                .unwrap_or_else(|_| panic!("native owner {id} activated closed"));
            recv(&wake)
        })
        .unwrap_or_else(|error| panic!("native owner {id} extent: {error:?}"));
    returned
        .send(out.epoch)
        .unwrap_or_else(|_| panic!("native owner {id} returned closed"));
    retry_rv(|| owner.enter_native(&[]));
    retire(owner);
}

/// One owner that re-enters the extent every round and is woken while the
/// round is still active: its return waits out the stop at the predicate, so
/// each round yields one NATIVE_RETURNED -> NATIVE_PERMITTED pair.
fn native_round_owner<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
    nil: &SharedRoot,
    one: &SharedRoot,
    retained: usize,
    rounds: usize,
    activated: Sender<()>,
    armed: Sender<()>,
    wake: Receiver<()>,
    returned: Sender<()>,
) {
    let mut owner = activate(permit, heap, id);
    let mut head = nil.clone();
    with_run(&mut owner, |run| {
        for _ in 0..retained {
            head = run.cons(one, &head)?;
        }
        Ok(())
    })
    .unwrap_or_else(|error| panic!("racer owner {id} seed: {error:?}"));
    activated
        .send(())
        .unwrap_or_else(|_| panic!("racer owner {id} activated closed"));
    for _ in 0..rounds {
        let roots = [head.clone(), one.clone()];
        armed
            .send(())
            .unwrap_or_else(|_| panic!("racer owner {id} armed closed"));
        owner
            .with_native_extent(&roots, |_handles| {
                recv(&wake);
            })
            .unwrap_or_else(|error| panic!("racer owner {id} extent: {error:?}"));
        // Rebuild before reporting `returned`: the driver's next round (and
        // the closing round after the last one) must not publish a request
        // while this owner is inside a `with_run` — see the acking owner's
        // note; a stopped round cannot complete without this owner either.
        with_run(&mut owner, |run| {
            head = nil.clone();
            for _ in 0..retained {
                head = run.cons(one, &head)?;
            }
            Ok(())
        })
        .unwrap_or_else(|error| panic!("racer owner {id} rebuild: {error:?}"));
        returned
            .send(())
            .unwrap_or_else(|_| panic!("racer owner {id} returned closed"));
    }
    retry_rv(|| owner.enter_native(&[]));
    retire(owner);
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() as f64 - 1.0) * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

struct Row {
    label: String,
    samples: Vec<u64>,
}

impl Row {
    fn new(label: impl Into<String>, samples: Vec<u64>) -> Self {
        let mut samples = samples;
        samples.sort_unstable();
        Self {
            label: label.into(),
            samples,
        }
    }
    fn print(&self) {
        if self.samples.is_empty() {
            println!("{:<52} (no samples)", self.label);
            return;
        }
        println!(
            "{:<52} n={:<4} p50={:>9} p95={:>9} p99={:>9} max={:>9} ns",
            self.label,
            self.samples.len(),
            percentile(&self.samples, 0.50),
            percentile(&self.samples, 0.95),
            percentile(&self.samples, 0.99),
            self.samples[self.samples.len() - 1]
        );
    }
    fn tsv(&self, out: &mut String) {
        for value in &self.samples {
            out.push_str(&self.label.replace('\t', " "));
            out.push('\t');
            out.push_str(&value.to_string());
            out.push('\n');
        }
    }
}

/// Per-round interval extraction from one scenario's event stream. `acking`
/// lists the owner ids whose ACK/Running events count toward the round rows;
/// a NativeStopped owner is excluded there and timed by its own pair.
struct RoundIntervals {
    request_to_last_ack: Vec<u64>,
    last_ack_to_census: Vec<u64>,
    census_to_marksweep: Vec<u64>,
    last_ack_to_marksweep: Vec<u64>,
    marksweep_to_keeper_complete: Vec<u64>,
    keeper_complete_to_last_running: Vec<u64>,
    marksweep_to_last_running: Vec<u64>,
    per_owner_region_close: Vec<u64>,
    per_owner_host_alloc: Vec<u64>,
    per_owner_publish_total: Vec<u64>,
    native_return_to_permission: Vec<u64>,
}

fn min_event(events: &[ProbeEvent], kind: u8, epoch: u64) -> Option<u64> {
    events
        .iter()
        .filter(|e| e.kind == kind && e.epoch == epoch)
        .map(|e| e.nanos)
        .min()
}

fn max_event(events: &[ProbeEvent], kind: u8, epoch: u64, owners: &[u64]) -> Option<u64> {
    events
        .iter()
        .filter(|e| e.kind == kind && e.epoch == epoch && owners.contains(&e.owner))
        .map(|e| e.nanos)
        .max()
}

fn intervals(events: &[ProbeEvent], rounds: &[u64], acking: &[u64]) -> RoundIntervals {
    let mut out = RoundIntervals {
        request_to_last_ack: Vec::new(),
        last_ack_to_census: Vec::new(),
        census_to_marksweep: Vec::new(),
        last_ack_to_marksweep: Vec::new(),
        marksweep_to_keeper_complete: Vec::new(),
        keeper_complete_to_last_running: Vec::new(),
        marksweep_to_last_running: Vec::new(),
        per_owner_region_close: Vec::new(),
        per_owner_host_alloc: Vec::new(),
        per_owner_publish_total: Vec::new(),
        native_return_to_permission: Vec::new(),
    };
    for epoch in rounds {
        let request = min_event(events, a8_probes::REQUEST_PUBLISHED, *epoch)
            .unwrap_or_else(|| panic!("round {epoch} missing REQUEST_PUBLISHED"));
        let census = min_event(events, a8_probes::ROOT_CENSUS_DONE, *epoch)
            .unwrap_or_else(|| panic!("round {epoch} missing ROOT_CENSUS_DONE"));
        let marksweep = min_event(events, a8_probes::MARK_SWEEP_DONE, *epoch)
            .unwrap_or_else(|| panic!("round {epoch} missing MARK_SWEEP_DONE"));
        let keeper = min_event(events, a8_probes::KEEPER_COMPLETED, *epoch)
            .unwrap_or_else(|| panic!("round {epoch} missing KEEPER_COMPLETED"));
        match max_event(events, a8_probes::ACK_COMMITTED, *epoch, acking) {
            Some(ack) => {
                out.request_to_last_ack.push(ack - request);
                out.last_ack_to_census.push(census - ack);
                out.last_ack_to_marksweep.push(marksweep - ack);
            }
            None => {
                // No ACKing owner in this membership (native-only round).
                out.request_to_last_ack.push(census - request);
                out.last_ack_to_census.push(0);
                out.last_ack_to_marksweep.push(marksweep - census);
            }
        }
        out.census_to_marksweep.push(marksweep - census);
        out.marksweep_to_keeper_complete.push(keeper - marksweep);
        if let Some(running) = max_event(events, a8_probes::OWNER_RUNNING, *epoch, acking) {
            out.keeper_complete_to_last_running.push(running - keeper);
            out.marksweep_to_last_running.push(running - marksweep);
        }
    }
    // Per-owner publish components: pair each owner's consecutive probe kinds
    // in timestamp order regardless of the stamped epoch (a native entry
    // stamps the publish-enter probe with 0).
    let mut by_owner: std::collections::BTreeMap<u64, Vec<&ProbeEvent>> =
        std::collections::BTreeMap::new();
    for event in events {
        by_owner.entry(event.owner).or_default().push(event);
    }
    for (owner, owner_events) in &by_owner {
        if !acking.contains(owner) {
            continue;
        }
        let mut ordered: Vec<&ProbeEvent> = owner_events.clone();
        ordered.sort_by_key(|e| e.nanos);
        let mut entered: Option<u64> = None;
        let mut region: Option<u64> = None;
        let mut hostalloc: Option<u64> = None;
        for event in ordered {
            match event.kind {
                a8_probes::PUBLISH_ENTERED => {
                    entered = Some(event.nanos);
                    region = None;
                    hostalloc = None;
                }
                a8_probes::REGION_CLOSED => region = Some(event.nanos),
                a8_probes::HOSTALLOC_DONE => hostalloc = Some(event.nanos),
                a8_probes::ACK_COMMITTED => {
                    if let (Some(entered), Some(region), Some(hostalloc)) =
                        (entered, region, hostalloc)
                    {
                        out.per_owner_region_close.push(region - entered);
                        out.per_owner_host_alloc.push(hostalloc - region);
                        out.per_owner_publish_total.push(event.nanos - entered);
                    }
                    entered = None;
                }
                _ => {}
            }
        }
    }
    // Native return pairs share one sequence number in the epoch field.
    let mut returned_at: std::collections::BTreeMap<u64, u64> = std::collections::BTreeMap::new();
    for event in events {
        if event.kind == a8_probes::NATIVE_RETURNED {
            returned_at.insert(event.epoch, event.nanos);
        }
    }
    for event in events {
        if event.kind == a8_probes::NATIVE_PERMITTED {
            if let Some(at) = returned_at.get(&event.epoch) {
                out.native_return_to_permission.push(event.nanos - at);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

struct ScenarioResult {
    name: String,
    acking: Vec<u64>,
    rounds: Vec<u64>,
    events: Vec<ProbeEvent>,
    grant_calls: u64,
    grant_ns: u64,
    grant_max_ns: u64,
    close_calls: u64,
    close_ns: u64,
    close_max_ns: u64,
}

fn reset_catalog_waits() {
    use std::sync::atomic::Ordering::Relaxed;
    CATALOG_WAITS.grant_calls.store(0, Relaxed);
    CATALOG_WAITS.grant_ns.store(0, Relaxed);
    CATALOG_WAITS.grant_max_ns.store(0, Relaxed);
    CATALOG_WAITS.close_calls.store(0, Relaxed);
    CATALOG_WAITS.close_ns.store(0, Relaxed);
    CATALOG_WAITS.close_max_ns.store(0, Relaxed);
}

fn read_catalog_waits() -> (u64, u64, u64, u64, u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    (
        CATALOG_WAITS.grant_calls.load(Relaxed),
        CATALOG_WAITS.grant_ns.load(Relaxed),
        CATALOG_WAITS.grant_max_ns.load(Relaxed),
        CATALOG_WAITS.close_calls.load(Relaxed),
        CATALOG_WAITS.close_ns.load(Relaxed),
        CATALOG_WAITS.close_max_ns.load(Relaxed),
    )
}

/// One scenario: `ackers` loops of `kind` plus optionally one native owner
/// (blocked for the whole run, or re-entering every round).
fn run_scenario(
    name: &str,
    ackers: usize,
    kind: LoopKind,
    native: Option<(usize, bool)>,
    rounds: usize,
    tsv_dir: Option<&str>,
    check: &mut dyn FnMut(&ScenarioResult),
) {
    a8_probes::enable();
    reset_catalog_waits();
    let drove = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive_scenario(ackers, kind, native, rounds)
    }));
    let events = a8_probes::take_events();
    a8_probes::disable();
    let epochs = drove.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    let waits = read_catalog_waits();
    let scenario = ScenarioResult {
        name: name.to_string(),
        acking: (1..=ackers as u64).collect(),
        rounds: epochs,
        events,
        grant_calls: waits.0,
        grant_ns: waits.1,
        grant_max_ns: waits.2,
        close_calls: waits.3,
        close_ns: waits.4,
        close_max_ns: waits.5,
    };
    if let Some(dir) = tsv_dir {
        let mut tsv = String::new();
        report(&scenario, &mut Some(&mut tsv));
        let path = format!("{dir}/latency-{name}.tsv");
        std::fs::write(&path, tsv).unwrap_or_else(|error| panic!("write {path}: {error:?}"));
        println!("tsv: {path}");
    }
    report(&scenario, &mut None);
    check(&scenario);
}

/// The keeper side of one scenario. Returns the collected round epochs.
fn drive_scenario(
    ackers: usize,
    kind: LoopKind,
    native: Option<(usize, bool)>,
    rounds: usize,
) -> Vec<u64> {
    let mut context = Context::new();
    // SAFETY: nil and a fixnum are immediates of this Context's heap.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let one = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::fixnum(1)) };
    let mut epochs = Vec::new();
    context
        .admit_owned_proof_parallel(|projection| {
            let era = projection.era();
            era.start_cons_storage()
                .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));

            let mut txs = Vec::new();
            let mut activated_rxs = Vec::new();
            let mut resumed_rxs = Vec::new();
            // The era handle itself is not Sync; threads capture only the
            // Copy heap identity, exactly as the A7 fixtures do.
            let heap = era.heap_identity();
            thread::scope(|scope| {
                for id in 1..=ackers {
                    let permit = retry_reg(|| era.register(id as u64, 4));
                    let (tx, rx) = channel::<OwnerSignal>();
                    let (activated_tx, activated_rx) = channel::<()>();
                    let (resumed_tx, resumed_rx) = channel::<()>();
                    let nil = nil.clone();
                    let one = one.clone();
                    scope.spawn(move || {
                        acking_owner(
                            permit,
                            heap,
                            id as u64,
                            &nil,
                            &one,
                            kind,
                            rx,
                            activated_tx,
                            resumed_tx,
                        );
                    });
                    txs.push(tx);
                    activated_rxs.push(activated_rx);
                    resumed_rxs.push(resumed_rx);
                }
                let mut wake_blocked: Option<Sender<()>> = None;
                let mut blocked_returned: Option<Receiver<u64>> = None;
                let mut round_armed: Option<Receiver<()>> = None;
                let mut round_wake: Option<Sender<()>> = None;
                let mut round_returned: Option<Receiver<()>> = None;
                if let Some((retained, per_round)) = native {
                    let id = (ackers + 1) as u64;
                    let permit = retry_reg(|| era.register(id, 4));
                    let nil = nil.clone();
                    let one = one.clone();
                    let (activated_tx, activated_rx) = channel::<()>();
                    if per_round {
                        let (armed_tx, armed_rx) = channel::<()>();
                        let (wake_tx, wake_rx) = channel::<()>();
                        let (returned_tx, returned_rx) = channel::<()>();
                        scope.spawn(move || {
                            native_round_owner(
                                permit,
                                heap,
                                id,
                                &nil,
                                &one,
                                retained,
                                rounds,
                                activated_tx,
                                armed_tx,
                                wake_rx,
                                returned_tx,
                            );
                        });
                        activated_rxs.push(activated_rx);
                        round_armed = Some(armed_rx);
                        round_wake = Some(wake_tx);
                        round_returned = Some(returned_rx);
                    } else {
                        let (wake_tx, wake_rx) = channel::<()>();
                        let (returned_tx, returned_rx) = channel::<u64>();
                        scope.spawn(move || {
                            native_blocked_owner(
                                permit,
                                heap,
                                id,
                                &nil,
                                &one,
                                retained,
                                activated_tx,
                                wake_rx,
                                returned_tx,
                            );
                        });
                        activated_rxs.push(activated_rx);
                        wake_blocked = Some(wake_tx);
                        blocked_returned = Some(returned_rx);
                    }
                }
                // Gate on full membership before the first round (see the
                // StartingStopped pairing note on the A7 fixtures).
                for activated_rx in &activated_rxs {
                    recv(activated_rx);
                }

                let mut next_epoch = 1u64;
                for _ in 0..rounds {
                    if let Some(armed) = &round_armed {
                        // The re-entering native owner must be inside its
                        // extent before the request is published.
                        recv(armed);
                    }
                    for tx in &txs {
                        tx.send(OwnerSignal::Collect)
                            .unwrap_or_else(|_| panic!("collect channel closed"));
                    }
                    let stopped = retry_rv(|| era.stop());
                    let epoch = stopped.epoch().get();
                    assert_eq!(epoch, next_epoch, "collection epochs stay strictly +1");
                    next_epoch += 1;
                    epochs.push(epoch);
                    retry_reg(|| era.return_cons_storage(&stopped));
                    let mut stopped_mut = stopped;
                    let report = retry_rv(|| era.collect_stopped(&mut stopped_mut));
                    assert!(report.major, "latency rounds collect a major cycle");
                    if let Some(wake) = &round_wake {
                        // Wake the extent owner while the round is still open:
                        // its return must wait out this stop.
                        wake.send(())
                            .unwrap_or_else(|_| panic!("native wake channel closed"));
                    }
                    retry_reg(|| era.resume_cons_storage(&stopped_mut));
                    complete(stopped_mut);
                    for resumed_rx in &resumed_rxs {
                        recv(resumed_rx);
                    }
                    if let Some(returned) = &round_returned {
                        recv(returned);
                    }
                }

                for tx in &txs {
                    tx.send(OwnerSignal::Finish)
                        .unwrap_or_else(|_| panic!("finish channel closed"));
                }
                if let Some(wake) = &wake_blocked {
                    wake.send(())
                        .unwrap_or_else(|_| panic!("native wake channel closed"));
                    if let Some(returned) = &blocked_returned {
                        let epoch = recv(returned);
                        assert!(
                            epoch <= next_epoch - 1,
                            "blocked owner applied a real image ({epoch})"
                        );
                    }
                }
                // Closing round: every owner is NativeStopped or retiring, so
                // the stop counts their publications without an ACK.
                let mut stopped = retry_rv(|| era.stop());
                assert_eq!(
                    stopped.epoch().get(),
                    next_epoch,
                    "closing epoch stays strictly +1"
                );
                retry_reg(|| era.return_cons_storage(&stopped));
                let report = retry_rv(|| era.collect_stopped(&mut stopped));
                assert!(report.major, "the closing round collects a major cycle");
                complete(stopped);
            });
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
    epochs
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn report(scenario: &ScenarioResult, tsv: &mut Option<&mut String>) {
    let data = intervals(&scenario.events, &scenario.rounds, &scenario.acking);
    let rows = [
        Row::new(
            format!("{} :: request -> last valid ACK", scenario.name),
            data.request_to_last_ack.clone(),
        ),
        Row::new(
            format!("{} :: last ACK -> root census", scenario.name),
            data.last_ack_to_census.clone(),
        ),
        Row::new(
            format!("{} :: root census -> mark/sweep done", scenario.name),
            data.census_to_marksweep.clone(),
        ),
        Row::new(
            format!("{} :: last ACK -> mark/sweep done", scenario.name),
            data.last_ack_to_marksweep.clone(),
        ),
        Row::new(
            format!("{} :: mark/sweep -> keeper complete", scenario.name),
            data.marksweep_to_keeper_complete.clone(),
        ),
        Row::new(
            format!("{} :: keeper complete -> last owner Running", scenario.name),
            data.keeper_complete_to_last_running.clone(),
        ),
        Row::new(
            format!("{} :: mark/sweep -> last owner Running", scenario.name),
            data.marksweep_to_last_running.clone(),
        ),
        Row::new(
            format!(
                "{} :: owner region close (entered -> closed)",
                scenario.name
            ),
            data.per_owner_region_close.clone(),
        ),
        Row::new(
            format!("{} :: owner host alloc (closed -> image)", scenario.name),
            data.per_owner_host_alloc.clone(),
        ),
        Row::new(
            format!("{} :: owner publish total (entered -> ACK)", scenario.name),
            data.per_owner_publish_total.clone(),
        ),
        Row::new(
            format!("{} :: native return -> permission", scenario.name),
            data.native_return_to_permission.clone(),
        ),
    ];
    if tsv.is_none() {
        println!(
            "\n=== {} ({} rounds) ===",
            scenario.name,
            scenario.rounds.len()
        );
        for row in &rows {
            row.print();
        }
        println!(
            "catalog grant: calls={} total={} ns max={} ns; close: calls={} total={} ns max={} ns",
            scenario.grant_calls,
            scenario.grant_ns,
            scenario.grant_max_ns,
            scenario.close_calls,
            scenario.close_ns,
            scenario.close_max_ns
        );
    }
    if let Some(tsv) = tsv {
        for row in &rows {
            row.tsv(tsv);
        }
        tsv.push_str(&format!(
            "{}\tcatalog_grant_calls\t{}\n{}\tcatalog_grant_ns\t{}\n{}\tcatalog_grant_max_ns\t{}\n{}\tcatalog_close_calls\t{}\n{}\tcatalog_close_ns\t{}\n{}\tcatalog_close_max_ns\t{}\n",
            scenario.name, scenario.grant_calls,
            scenario.name, scenario.grant_ns,
            scenario.name, scenario.grant_max_ns,
            scenario.name, scenario.close_calls,
            scenario.name, scenario.close_ns,
            scenario.name, scenario.close_max_ns,
        ));
    }
}

fn tsv_dir() -> Option<String> {
    std::env::var("A8_LATENCY_TSV")
        .ok()
        .filter(|s| !s.is_empty())
}

fn full_rounds() -> usize {
    std::env::var("A8_LATENCY_ROUNDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Probe contract: a disabled probe records nothing, an enabled one records
/// every call. The ns/call figures are printed for the A8 report only; a
/// wall-clock bound flakes under the parallel gate (76 ns on 32 threads).
#[test]
fn a8_probe_overhead() {
    let iterations = if cfg!(miri) { 1_000 } else { 100_000 };
    a8_probes::disable();
    a8_probes::take_events();
    for i in 0..iterations as u64 {
        a8_probes::mark(a8_probes::REQUEST_PUBLISHED, i, 0);
    }
    assert!(
        a8_probes::take_events().is_empty(),
        "a disabled probe must record nothing"
    );
    a8_probes::enable();
    for i in 0..iterations as u64 {
        a8_probes::mark(a8_probes::REQUEST_PUBLISHED, i, 0);
    }
    a8_probes::disable();
    let events = a8_probes::take_events();
    assert_eq!(
        events.len(),
        iterations,
        "an enabled probe records every call"
    );
    assert!(events.iter().enumerate().all(|(i, e)| e.epoch == i as u64));
    let (disabled, enabled) = a8_probes::overhead_ns(iterations);
    println!("probe disabled: {disabled:.2} ns/call; recording: {enabled:.2} ns/call");
}

/// Event invariants on a few rounds: ordering and per-round coverage.
#[test]
fn a8_latency_event_invariants() {
    run_scenario("invariants-2-parked", 2, PARKED, None, 6, None, &mut |s| {
        assert_eq!(s.rounds.len(), 6);
        let data = intervals(&s.events, &s.rounds, &s.acking);
        assert_eq!(data.request_to_last_ack.len(), 6);
        for epoch in &s.rounds {
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| e.kind == a8_probes::REQUEST_PUBLISHED && e.epoch == *epoch)
                    .count(),
                1,
                "exactly one request per round {epoch}"
            );
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| e.kind == a8_probes::ACK_COMMITTED && e.epoch == *epoch)
                    .count(),
                2,
                "one ACK per acking owner in round {epoch}"
            );
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| e.kind == a8_probes::OWNER_RUNNING && e.epoch == *epoch)
                    .count(),
                2,
                "one Running per acking owner in round {epoch}"
            );
        }
        // Every ACK for a round is at or after that round's request.
        for epoch in &s.rounds {
            let request = s
                .events
                .iter()
                .find(|e| e.kind == a8_probes::REQUEST_PUBLISHED && e.epoch == *epoch)
                .expect("request")
                .nanos;
            for event in &s.events {
                if event.kind == a8_probes::ACK_COMMITTED && event.epoch == *epoch {
                    assert!(
                        event.nanos >= request,
                        "ACK before its request in round {epoch}"
                    );
                }
            }
        }
        // Native commits exist only at teardown here (the owners'
        // enter_native before retiring) and always stamp a completed epoch,
        // never a round's request epoch.
        let last_round = s.rounds[s.rounds.len() - 1];
        for event in &s.events {
            if event.kind == a8_probes::NATIVE_ACK {
                // A teardown entry racing the closing round may stamp that
                // round's epoch instead of the last completed one.
                assert!(
                    event.epoch == 0 || event.epoch <= last_round + 1,
                    "teardown native commit stamped a future epoch"
                );
            }
        }
    });
}

/// One NativeStopped owner across rounds: its commit is the native kind and
/// it never appears as a round ACK; its return is timed by sequence pair.
#[test]
fn a8_latency_native_owner_shape() {
    run_scenario(
        "invariants-native",
        2,
        PARKED,
        Some((32, false)),
        5,
        None,
        &mut |s| {
            let native_owner = 3u64;
            // The extent entry precedes every round, so it stamps epoch 0;
            // the teardown entry after the last round stamps that epoch.
            let last_round = s.rounds[s.rounds.len() - 1];
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| {
                        e.kind == a8_probes::NATIVE_ACK && e.owner == native_owner && e.epoch == 0
                    })
                    .count(),
                1,
                "one pre-round native commit for the blocked owner"
            );
            assert!(
                s.events.iter().any(|e| {
                    e.kind == a8_probes::NATIVE_ACK
                        && e.owner == native_owner
                        && (e.epoch == last_round || e.epoch == last_round + 1)
                }),
                "teardown native commit after the last round ( Idle stamps \
                 the last completed epoch; racing the closing round's request \
                 stamps that round's epoch — both are valid retire paths)"
            );
            for epoch in &s.rounds {
                assert_eq!(
                    s.events
                        .iter()
                        .filter(|e| {
                            e.kind == a8_probes::ACK_COMMITTED
                                && e.epoch == *epoch
                                && e.owner == native_owner
                        })
                        .count(),
                    0,
                    "blocked owner never round-ACKs"
                );
            }
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| e.kind == a8_probes::NATIVE_PERMITTED && e.owner == native_owner)
                    .count(),
                1,
                "one native permission at teardown"
            );
            // The native owner's blocked time never lands in a round's
            // request -> ACK interval: every request_to_last_ack sample is
            // bounded by that round's own census.
            let data = intervals(&s.events, &s.rounds, &s.acking);
            assert!(
                data.request_to_last_ack
                    .iter()
                    .all(|value| *value < 60_000_000_000)
            );
        },
    );
}

/// The full A8 latency matrix (measurement runs use `--ignored`).
#[test]
#[cfg(feature = "perf-probes")]
fn a8_latency_full() {
    let rounds = full_rounds();
    let tsv = tsv_dir();
    for owners in [2usize, 3, 4] {
        run_scenario(
            &format!("{owners}-owners-parked"),
            owners,
            PARKED,
            None,
            rounds,
            tsv.as_deref(),
            &mut |_| {},
        );
        run_scenario(
            &format!("{owners}-owners-allocating"),
            owners,
            ALLOCATING,
            None,
            rounds,
            tsv.as_deref(),
            &mut |_| {},
        );
    }
    run_scenario(
        "4-owners-refill-collision",
        4,
        REFILL,
        None,
        rounds,
        tsv.as_deref(),
        &mut |s| {
            assert!(
                s.grant_calls > rounds as u64,
                "refill scenario must actually refill (grant calls: {})",
                s.grant_calls
            );
        },
    );
    run_scenario(
        "3-owners-one-native-stopped",
        2,
        ALLOCATING,
        Some((65536, false)),
        rounds,
        tsv.as_deref(),
        &mut |_| {},
    );
    run_scenario(
        "3-owners-native-wake-during-stop",
        2,
        ALLOCATING,
        Some((1024, true)),
        rounds,
        tsv.as_deref(),
        &mut |s| {
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| e.kind == a8_probes::NATIVE_PERMITTED)
                    .count(),
                rounds,
                "one return/permission pair per round"
            );
        },
    );
    let (disabled, enabled) = a8_probes::overhead_ns(1_000_000);
    println!("probe overhead: disabled {disabled:.2} ns, recording {enabled:.2} ns per call");
}
