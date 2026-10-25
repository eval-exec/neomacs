//! Cold latency probes for the A8 qualification rows.
//!
//! Every call site is `#[cfg(test)]`, so production images carry none of
//! this. A probe is one relaxed atomic load while disabled; recording is a
//! mutex-guarded vector push on a per-collection cold path. The overhead is
//! quantified by [`overhead_ns`] and excluded from throughput rows, which run
//! with probes disabled on non-instrumented images.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

mod suppression;
pub(crate) use suppression::suppress;

pub(crate) const REQUEST_PUBLISHED: u8 = 1;
/// `publish_roots` entered (owner woke and holds its run-closed state).
pub(crate) const PUBLISH_ENTERED: u8 = 2;
/// `validate_publication` returned: regions confirmed closed.
pub(crate) const REGION_CLOSED: u8 = 3;
/// Root batch reserved and the GC image published (host allocation done).
pub(crate) const HOSTALLOC_DONE: u8 = 4;
/// `Mode::Stopped`/`NativeStopped` committed under the control mutex.
pub(crate) const ACK_COMMITTED: u8 = 5;
/// `Mode::NativeStopped` committed (an extent entry, not a round ACK).
pub(crate) const NATIVE_ACK: u8 = 12;
/// Keeper finished enumerating every owner's roots plus World heads.
pub(crate) const ROOT_CENSUS_DONE: u8 = 6;
/// `collect_stopped` returned: mark and sweep finished.
pub(crate) const MARK_SWEEP_DONE: u8 = 7;
/// Keeper `complete` finished draining into resume handoff.
pub(crate) const KEEPER_COMPLETED: u8 = 8;
/// One owner reached `Mode::Running` again.
pub(crate) const OWNER_RUNNING: u8 = 9;
/// The native extent body returned on its owner thread.
pub(crate) const NATIVE_RETURNED: u8 = 10;
/// The returning owner is `Running` again (permission granted).
pub(crate) const NATIVE_PERMITTED: u8 = 11;
/// A slice admission backed out: the phase word was not admissible, so the
/// owner refused without running (cold backout branch only; the admitted
/// fast path carries no probe).
pub(crate) const ADMISSION_BACKOUT: u8 = 13;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbeEvent {
    pub kind: u8,
    pub epoch: u64,
    pub owner: u64,
    pub nanos: u64,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static START: Mutex<Option<Instant>> = Mutex::new(None);
static EVENTS: Mutex<Vec<ProbeEvent>> = Mutex::new(Vec::new());
static NATIVE_SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) fn enable() {
    *START.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(Instant::now());
    EVENTS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    NATIVE_SEQ.store(0, Ordering::SeqCst);
    ENABLED.store(true, Ordering::SeqCst);
}

pub(crate) fn disable() {
    ENABLED.store(false, Ordering::SeqCst);
}

pub(crate) fn take_events() -> Vec<ProbeEvent> {
    EVENTS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .drain(..)
        .collect()
}

/// The next native-extent sequence number; stamped on both the return and
/// permission events so they pair without an epoch.
pub(crate) fn native_seq() -> u64 {
    match suppression::scope() {
        suppression::ProbeScope::Suppressed => return 0,
        suppression::ProbeScope::Recording => {}
    }
    NATIVE_SEQ.fetch_add(1, Ordering::SeqCst) + 1
}

#[inline(always)]
pub(crate) fn mark(kind: u8, epoch: u64, owner: u64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    match suppression::scope() {
        suppression::ProbeScope::Suppressed => return,
        suppression::ProbeScope::Recording => {}
    }
    let nanos = START
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .map(|start| start.elapsed().as_nanos() as u64)
        .unwrap_or(0);
    EVENTS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push(ProbeEvent {
            kind,
            epoch,
            owner,
            nanos,
        });
}

/// ns cost of one disabled and one recording probe call, for the report.
pub(crate) fn overhead_ns(iterations: usize) -> (f64, f64) {
    let disabled = Instant::now();
    for _ in 0..iterations {
        mark(0, 0, 0);
    }
    let disabled = disabled.elapsed().as_nanos() as f64 / iterations as f64;
    enable();
    let enabled = Instant::now();
    for i in 0..iterations as u64 {
        mark(0, i, 0);
    }
    let enabled = enabled.elapsed().as_nanos() as f64 / iterations as f64;
    disable();
    take_events();
    (disabled, enabled)
}
