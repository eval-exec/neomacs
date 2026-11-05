//! Lock-free slice admission: the safepoint-poll shape (turn 3 design,
//! tmp/peer-constail/admission-design.md).
//!
//! The per-slice fast path performs zero RMWs on shared lines: one plain
//! `Release` store to an owner-private admission word, one full fence, one
//! plain load of a read-mostly registry phase word. Exclusivity against
//! stops and catalog transitions rests on the Dekker store->load pair
//! formed by the owner sequence (admission store, `fence(SeqCst)`, phase
//! load) and the authority sequence (phase publish, `fence(SeqCst)`,
//! admission scan) — see `admissions_quiescent` and the loom model in
//! `crates/neovm-core/tests/p1rv-rendezvous-loom/tests/admission.rs`.
use super::*;

/// Phase tag bits (0..4) of the registry phase word.
const TAG_IDLE: u64 = 0;
const TAG_STOPPING: u64 = 1;
const TAG_TRACING: u64 = 2;
const TAG_DRAINING: u64 = 3;
/// Bit 4: a catalog transition window is open (mirror of
/// `RegistryState::catalog_transition`).
const BLOCKED_BIT: u64 = 1 << 4;

/// The read-mostly registry phase word, on its own line: written only by
/// the gate authority while holding the canonical gate, never per slice.
///
/// Layout: bits 0..4 rendezvous tag, bit 4 catalog-transition block,
/// bits 8.. the rendezvous epoch (request epoch; completed epoch while
/// resumes drain). `Poisoned` is deliberately not a tag: it stays
/// authoritative on `ParkLine::poisoned` (refused by `check_thread`), and
/// the word mirrors it as a non-admissible stop shape so a post-poison
/// admission still backs out with `Busy` before the next `check_thread`
/// reports `Poisoned`.
#[repr(align(64))]
pub(super) struct PhaseLine {
    word: AtomicU64,
}

impl PhaseLine {
    /// The initial registry state (Idle, unblocked, epoch 0) encodes to 0.
    pub(super) const fn new() -> Self {
        Self {
            word: AtomicU64::new(0),
        }
    }

    /// Owner-side poll: a plain load, the only shared-line read per slice.
    #[inline(always)]
    pub(super) fn load(&self) -> u64 {
        self.word.load(Ordering::Acquire)
    }

    /// A slice may start only while no stop request, trace, resume drain or
    /// catalog transition is published.
    #[inline(always)]
    pub(super) fn admissible(word: u64) -> bool {
        word & 0x0F == TAG_IDLE && word & BLOCKED_BIT == 0
    }

    /// The rendezvous epoch carried by a phase word.
    #[inline(always)]
    pub(super) fn epoch(word: u64) -> u64 {
        word >> 8
    }

    fn pack(tag: u64, blocked: bool, epoch: u64) -> u64 {
        debug_assert!(epoch < (1 << (64 - 8)));
        (epoch << 8) | if blocked { BLOCKED_BIT } else { 0 } | tag
    }

    /// Publish the word computed from the final gated state. Called from
    /// `RegistryGuard::drop`/`take_for_wait` (one place covering every
    /// mutation site, before the mutex releases) and, as the authority half
    /// of the Dekker pair, immediately before the fenced scans. A redundant
    /// same-value store is allowed: both writers hold the gate.
    ///
    /// An Idle word carries the last completed epoch, so the epoch packed
    /// into an owner's admission word is strictly below the next
    /// activation's even across completed cycles: the ABA sequence
    /// Idle(e) -> Stopping(e+1) -> ... -> Idle(e+1) walks forward. The
    /// epoch is diagnostics only; no decision reads it.
    pub(super) fn publish(&self, state: &RegistryState) {
        let (tag, epoch) = match state.rendezvous.phase {
            Phase::Idle => (TAG_IDLE, state.rendezvous.completed),
            Phase::Stopping(epoch) => (TAG_STOPPING, epoch.get()),
            Phase::Tracing(epoch) => (TAG_TRACING, epoch.get()),
            Phase::DrainingResumes => (TAG_DRAINING, state.rendezvous.completed),
            Phase::Poisoned => (TAG_STOPPING, state.rendezvous.epoch),
        };
        let word = Self::pack(tag, state.catalog_transition, epoch);
        self.word.store(word, Ordering::Release);
    }
}

/// The owner-private admission word replacing `live_run_loan`.
///
/// `0` = OUT. `IN_SLICE` packs the phase-word epoch observed by the
/// owner's pre-admission snapshot; that epoch is diagnostics only (it
/// accounts which slice a scan observed) and never gates a decision — the
/// authority is always the fence-ordered phase load that follows the store.
pub(super) struct AdmissionWord;

impl AdmissionWord {
    pub(super) const OUT: u64 = 0;

    #[inline(always)]
    pub(super) fn in_slice(epoch: u64) -> u64 {
        (epoch << 1) | 1
    }

    #[inline(always)]
    pub(super) fn is_in_slice(word: u64) -> bool {
        word & 1 != 0
    }
}

impl MutatorEntry {
    /// Cross-thread run-loan predicate: IN_SLICE means an owner slice is
    /// between its admission and its exit store. Owner threads read their
    /// own store; authority scans read it after `admissions_quiescent`.
    #[inline(always)]
    pub(super) fn admitted(&self) -> bool {
        AdmissionWord::is_in_slice(self.admission.load(Ordering::Acquire))
    }
}

/// The authority half of the admission Dekker pair: the full fence, then
/// every entry's admission word. Returns true when no owner is mid-slice.
///
/// Callers publish the phase word (a `Release` store — either explicitly
/// from the held `RegistryState`, or implicitly at the `RegistryGuard` drop
/// that committed the stop/transition) *before* this fence. That order is
/// what makes a single-hold stop sound: if an owner's scan-read OUT store
/// precedes this fence in the fence total order, the owner's own admission
/// fence+load is SC-after the published word and backs out; if the scan
/// reads IN_SLICE, the owner's exit fence+load observes the request and it
/// reaches the gated slow path at its slice end. Scans that merely report
/// (diagnostics) need no fence; every scan the protocol acts on goes
/// through here.
pub(super) fn admissions_quiescent(entries: &[Arc<MutatorEntry>]) -> bool {
    fence(Ordering::SeqCst);
    entries.iter().all(|entry| !entry.admitted())
}
