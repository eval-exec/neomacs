//! Bounded dependency observations for short-lived layout queries.
//!
//! Object identities are words, never GC roots or dereferenced pointers. A
//! fixed mutation journal allows an observation to survive unrelated writes.
//! Losing journal history or exceeding the read budget always rejects reuse.
//! Pointer reads first check this mutator's existing private scope membership;
//! an inactive mutator skips the shared gate. Active reads retain the process
//! policy gate and cold recorder, while traversal selection keeps its own gate.
use super::{mutate::LispCollectionRevision, value::TaggedValue};
use rustc_hash::FxHashSet;
use std::cell::{Cell, RefCell};
use std::sync::{
    LazyLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// Process knobs, read once before a heap or capture is created.
///
/// | Knob | Default | Effect |
/// | --- | --- | --- |
/// | `NEOVM_COLLECTION_READ_GLOBAL=off` | on | Use `off` to retain the inactive traversal selector's TLS membership check. |
/// | `NEOVM_COLLECTION_READ_HOIST=off` | on | Use `off` to retain observed traversal reads when this mutator has no capture. |
/// | `NEOVM_COLLECTION_WRITE_LAZY=on` | off | Skip revision/journal work until the first process capture. |
///
/// Scope state and journals remain local to each mutator. The process gates
/// contain no Lisp identities and are safe with concurrent mutators. The low
/// two bits retain legacy observation and observed traversal independently.
/// Every admitted scope contributes four, preserving both immutable flags.
/// Relaxed ordering suffices: a mutator's own begin is sequenced before its observation, so it cannot read a count from
/// before that begin. Other mutators only make the gate more conservative.
#[repr(usize)]
enum ReadPolicyBit {
    LegacyObserve = 1,
    ObservedTraversal = 2,
}

const POLICY_MASK: usize =
    ReadPolicyBit::LegacyObserve as usize | ReadPolicyBit::ObservedTraversal as usize;
const SCOPE_INCREMENT: usize = POLICY_MASK + 1;

// The count can represent usize::MAX / SCOPE_INCREMENT simultaneous scopes;
// a process cannot approach that bound. Policy bits never enter the count.
static CAPTURE_SCOPES: AtomicUsize = AtomicUsize::new(POLICY_MASK);

/// Once any mutator has started a capture, all subsequent writes keep their
/// thread's history, including writes between scopes while certificates live.
/// Never reset this gate on scope exit. No Lisp state is shared by this flag.
static HISTORY_REQUIRED: AtomicBool = AtomicBool::new(true);

#[cfg(test)]
static STATE_INITIALIZATIONS: AtomicUsize = AtomicUsize::new(0);

static CONFIG: LazyLock<()> = LazyLock::new(|| {
    if std::env::var("NEOVM_COLLECTION_READ_GLOBAL").as_deref() != Ok("off") {
        CAPTURE_SCOPES.fetch_and(!(ReadPolicyBit::LegacyObserve as usize), Ordering::Relaxed);
    }
    if std::env::var("NEOVM_COLLECTION_WRITE_LAZY").as_deref() == Ok("on") {
        HISTORY_REQUIRED.store(false, Ordering::Relaxed);
    }
    if std::env::var("NEOVM_COLLECTION_READ_HOIST").as_deref() != Ok("off") {
        CAPTURE_SCOPES.fetch_and(
            !(ReadPolicyBit::ObservedTraversal as usize),
            Ordering::Relaxed,
        );
    }
});

pub(super) fn initialize() {
    LazyLock::force(&CONFIG);
}

#[cfg(test)]
#[inline]
pub(crate) fn hoist_reads() -> bool {
    CAPTURE_SCOPES.load(Ordering::Relaxed) & ReadPolicyBit::ObservedTraversal as usize == 0
}

/// Select a traversal once, before any read or synchronous callback. This
/// caches no Lisp state: each mutator checks its own membership when another
/// mutator holds a scope. Scope contribution and immutable policy share one
/// coherent atomic word, so the default inactive arm needs only one load.
#[inline]
pub(crate) fn reads_need_observation() -> bool {
    let policy = CAPTURE_SCOPES.load(Ordering::Relaxed);
    if policy == 0 {
        return false;
    }
    policy & ReadPolicyBit::ObservedTraversal as usize != 0 || ACTIVE.with(Cell::get)
}

#[inline]
pub(crate) fn is_active() -> bool {
    CAPTURE_SCOPES.load(Ordering::Relaxed) & !POLICY_MASK != 0 && ACTIVE.with(Cell::get)
}

#[inline]
pub(super) fn history_required() -> bool {
    HISTORY_REQUIRED.load(Ordering::Relaxed)
}

const JOURNAL_SIZE: usize = 8192;
const MAX_READS: usize = 16_384;
const MAX_DEPTH: usize = 8;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static RECENT_READS: RecentReads = const { RecentReads::new() };
    static STATE: store_state::StateSlot = store_state::StateSlot::default();
    #[cfg(test)]
    static OBSERVATION_STATE_ACCESSES: Cell<usize> = const { Cell::new(0) };
    #[cfg(test)]
    static OBSERVED_OWNER_PUBLICATIONS: Cell<usize> = const { Cell::new(0) };
    #[cfg(test)]
    static RECENT_CLEARED_CELLS: Cell<usize> = const { Cell::new(0) };
    #[cfg(test)]
    static RECENT_CLEARED_MASKS: Cell<usize> = const { Cell::new(0) };
}

/// The existing mutator-local read deduplication, qualified by reclamation.
/// Actual reclamation clears it on the heap's executing mutator before slot
/// reuse. Installing another heap also clears it. The shared epoch remains a
/// cold ledger-validation signal; live-owner hits keep the original word-only
/// shortcut. A heap has one executing mutator, independently of other heaps.
#[repr(C)]
struct RecentReads {
    // Keep the original word array first, including its indexed-load offset.
    words: [Cell<usize>; 256],
    // Immutable process policy, or this mutator's scalar test override. Scope
    // and policy changes refresh it before any pointer read; no Lisp state is
    // cached by this field and other mutators cannot change its value.
    observed: Cell<bool>,
    // Use existing padding, preserving the word probe and epoch offsets.
    // Only cold misses set this; empty resets never initialize STATE.
    dirty: Cell<bool>,
    epoch: Cell<u64>,
}

const _: () = {
    #[repr(C)]
    struct LegacyRecentReads {
        words: [Cell<usize>; 256],
        observed: Cell<bool>,
        epoch: Cell<u64>,
    }
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<RecentReads>() == size_of::<LegacyRecentReads>());
    assert!(align_of::<RecentReads>() == align_of::<LegacyRecentReads>());
    assert!(offset_of!(RecentReads, words) == offset_of!(LegacyRecentReads, words));
    assert!(offset_of!(RecentReads, observed) == offset_of!(LegacyRecentReads, observed));
    assert!(offset_of!(RecentReads, epoch) == offset_of!(LegacyRecentReads, epoch));
};

impl RecentReads {
    const fn new() -> Self {
        Self {
            observed: Cell::new(false),
            dirty: Cell::new(false),
            epoch: Cell::new(0),
            words: [const { Cell::new(0) }; 256],
        }
    }

    fn clear(&self, epoch: u64) {
        if self.dirty.replace(false) {
            if let Some(dirty) = store_state::take_recent_dirty() {
                for (block, mut mask) in dirty.into_iter().enumerate() {
                    while mask != 0 {
                        let index = block * 64 + mask.trailing_zeros() as usize;
                        let word = &self.words[index];
                        if word.get() != 0 {
                            word.set(0);
                            #[cfg(test)]
                            RECENT_CLEARED_CELLS.with(|count| count.set(count.get() + 1));
                        }
                        mask &= mask - 1;
                    }
                }
            } else {
                // Teardown can retire STATE before the recent-read cache.
                // Keep reset total without accessing already destroyed TLS.
                for word in &self.words {
                    word.set(0);
                }
                #[cfg(test)]
                RECENT_CLEARED_CELLS.with(|count| count.set(count.get() + self.words.len()));
            }
        }
        self.epoch.set(epoch);
    }

    fn forget(&self, bits: usize) {
        let slot = &self.words[((bits >> 3) ^ (bits >> 11)) & 255];
        if slot.get() == bits {
            slot.set(0);
        }
    }
}

/// This mutator's journal policy mirrors its private capture stack. It caches
/// no Lisp identity: only the process's monotonic history requirement is shared.
/// Once this mutator has observed required history, it never skips later writes,
/// including writes after its last capture while a certificate can remain live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum JournalPolicy {
    JournalOnly,
    BeforeFirstCapture,
    JournalAndObserve,
}

// Cold capture metadata follows its World; the journal and revision share the
// stable StoreState allocation. No generated code addresses this cold State.
#[repr(C)]
struct State {
    captures: Vec<Capture>,
    // Exact identities and conservative bounds live in the existing mutator
    // journal. Reclamation prunes dead identities without dereferencing them;
    // Context/dump changes reclassify the surviving owner ranges.
    observed: observed_envelope::ObservedEnvelope,
    capture_pool: capture_storage::CapturePool,
    certificate_pool: certificate_storage::CertificatePool,
}

impl Default for State {
    fn default() -> Self {
        #[cfg(test)]
        STATE_INITIALIZATIONS.fetch_add(1, Ordering::Relaxed);
        // CONFIG cannot touch STATE or call Lisp. Force it before remembering
        // history permanently: WRITE_LAZY may clear the conservative initial flag.
        initialize();
        Self {
            captures: Vec::with_capacity(MAX_DEPTH),
            observed: observed_envelope::ObservedEnvelope::new(),
            capture_pool: capture_storage::CapturePool::default(),
            certificate_pool: certificate_storage::CertificatePool::default(),
        }
    }
}

struct Capture {
    reads: capture_storage::CaptureReads,
    started: LispCollectionRevision,
    overflow: bool,
    // Each entry has a completed Observed-mode projection or a validated
    // current-epoch local publication during transitive replay. Unknown replay,
    // another epoch or a non-Observed read clears this scalar proof. It caches
    // no identity beyond the existing dependency map.
    publication_epoch: Option<u64>,
}

/// Exact mutable collection reads made by one completed observation.
///
/// This grants mutation freshness, not object lifetime. Consumers must retain
/// the dependency owners, or discard their cached result when its source
/// lifetime ends. Identities are not roots: a reclaimed address can be reused
/// for an unrelated, initially unobserved object. Certificates and their
/// mutation journals remain local to the observing mutator.
#[derive(Debug)]
pub struct CollectionReads {
    reads: certificate_storage::CertificateLease,
    owner: ObservationWorld,
    _not_send: std::marker::PhantomData<*const ()>,
}

const _: () = {
    type LegacyCertificate = (FxHashSet<usize>, LispCollectionRevision);
    assert!(std::mem::size_of::<CollectionReads>() == std::mem::size_of::<LegacyCertificate>());
    assert!(std::mem::align_of::<CollectionReads>() == std::mem::align_of::<LegacyCertificate>());
    assert!(
        std::mem::size_of::<Option<CollectionReads>>()
            == std::mem::size_of::<Option<LegacyCertificate>>()
    );
    assert!(
        std::mem::align_of::<Option<CollectionReads>>()
            == std::mem::align_of::<Option<LegacyCertificate>>()
    );
};

static_assertions::assert_not_impl_any!(CollectionReads: Send, Sync);

impl CollectionReads {
    /// Numeric identities only; origin/lifetime admission belongs to the caller.
    pub(crate) fn all_dependency_bits(&self, mut predicate: impl FnMut(usize) -> bool) -> bool {
        self.reads.iter().copied().all(&mut predicate)
    }

    pub fn unchanged(&self) -> bool {
        self.reads.revision().is_some_and(|revision| {
            unchanged_since(self.reads.as_slice(), revision, self.owner.store())
        })
    }

    /// A nested cache hit reads its original dependencies transitively.
    pub fn unchanged_and_observe(&self) -> bool {
        if !self.unchanged() {
            return false;
        }
        if !self.owner.is_current() {
            return false;
        }
        if ACTIVE.with(Cell::get) {
            for &bits in self.reads.as_slice() {
                // The original observation already published its sticky mark
                // and this mutator's envelope. Certificates retain identities,
                // not object storage: transitive reuse must not dereference an
                // old header merely to re-publish that existing metadata.
                if !recently_observed(bits) {
                    observe_uncached(bits);
                }
            }
        }
        true
    }
}

impl Drop for CollectionReads {
    fn drop(&mut self) {
        let reads = &mut self.reads;
        self.owner.with_state(|state| {
            state.borrow_mut().certificate_pool.recycle(reads);
        });
    }
}

fn unchanged_since(reads: &[usize], revision: LispCollectionRevision, store: &StoreState) -> bool {
    let count = store.sequence().wrapping_sub(revision.sequence());
    if count > JOURNAL_SIZE as u64 {
        return false;
    }
    (1..=count).all(|offset| {
        let slot = revision.sequence().wrapping_add(offset) as usize % JOURNAL_SIZE;
        reads.binary_search(&store.journal()[slot].get()).is_err()
    })
}

/// Nested observations contribute reads to their enclosing observation too.
/// The guard cannot move across threads; unwinding restores the prior scope.
#[derive(Debug)]
#[must_use = "dropping the guard ends this mutator's collection-read capture"]
struct CollectionReadScope {
    active: bool,
    owner: ObservationWorld,
    _not_send: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(CollectionReadScope: Send, Sync);

impl CollectionReadScope {
    fn begin() -> Self {
        initialize();
        HISTORY_REQUIRED.store(true, Ordering::Relaxed);
        let owner = ObservationWorld::current();
        let admitted = with_state(|state| {
            let mut state = state.borrow_mut();
            with_store(|store| {
                store.mutator.policy.set(JournalPolicy::JournalAndObserve);
                store.mutator.observe.set(true);
            });
            let overflow = state.captures.len() >= MAX_DEPTH;
            if overflow {
                for capture in &mut state.captures {
                    capture.overflow = true;
                }
                return false;
            }
            clear_recent_reads();
            let reads = state.capture_pool.acquire();
            state.captures.push(Capture {
                reads,
                started: LispCollectionRevision::current(),
                overflow,
                publication_epoch: recent_publication_epoch(),
            });
            true
        });
        ACTIVE.with(|active| active.set(true));
        if admitted {
            CAPTURE_SCOPES.fetch_add(SCOPE_INCREMENT, Ordering::Relaxed);
        }
        Self {
            active: admitted,
            owner,
            _not_send: std::marker::PhantomData,
        }
    }

    fn finish(mut self) -> Option<CollectionReads> {
        if !self.active {
            return None;
        }
        let capture = self.take();
        let result = self.finish_capture(&capture);
        self.recycle(capture);
        result
    }

    fn finish_capture(&self, capture: &Capture) -> Option<CollectionReads> {
        // Guards always retire in their own State, even when a callback
        // activates a different World. A certificate is meaningful only in
        // the World in which its captured projections were made.
        if !self.owner.is_current() {
            return None;
        }
        // Reject writes after a dependency's first read. Writes before its
        // first read (including initialization of fresh objects) are harmless.
        let now = LispCollectionRevision::from_sequence(self.owner.store().sequence());
        let count = now.sequence().wrapping_sub(capture.started.sequence());
        if capture.overflow || count > JOURNAL_SIZE as u64 {
            tracing::trace!(target: "neovm_core::collection_reads", reads = capture.reads.len(), writes = count, overflow = capture.overflow, "capture rejected");
            return None;
        }
        let coherent = {
            let store = self.owner.store();
            (1..=count).all(|offset| {
                let slot = capture.started.sequence().wrapping_add(offset) as usize % JOURNAL_SIZE;
                capture
                    .reads
                    .get(&store.journal()[slot].get())
                    .is_none_or(|read_at| {
                        offset <= read_at.sequence().wrapping_sub(capture.started.sequence())
                    })
            })
        };
        if !coherent {
            tracing::trace!(target: "neovm_core::collection_reads", reads = capture.reads.len(), writes = count, "capture changed during observation");
            return None;
        }
        let reads = self.owner.with_state(|state| {
            state
                .borrow_mut()
                .certificate_pool
                .acquire(&capture.reads, now)
        });
        Some(CollectionReads {
            reads,
            owner: self.owner.clone(),
            _not_send: std::marker::PhantomData,
        })
    }

    fn recycle(&self, capture: Capture) {
        self.owner.with_state(|state| {
            state.borrow_mut().capture_pool.recycle(capture.reads);
        });
    }

    fn take(&mut self) -> Capture {
        self.active = false;
        let capture = self.owner.with_state(|state| {
            let mut state = state.borrow_mut();
            let capture = state.captures.pop().expect("collection read scope");
            let active = !state.captures.is_empty();
            let store = self.owner.store();
            store.mutator.policy.set(if active {
                JournalPolicy::JournalAndObserve
            } else {
                JournalPolicy::JournalOnly
            });
            store.mutator.observe.set(active);
            if self.owner.is_current() {
                clear_recent_reads();
                ACTIVE.with(|membership| membership.set(active));
            }
            CAPTURE_SCOPES.fetch_sub(SCOPE_INCREMENT, Ordering::Relaxed);
            capture
        });
        store_state::refresh_membership();
        capture
    }
}

impl Drop for CollectionReadScope {
    fn drop(&mut self) {
        if self.active {
            let capture = self.take();
            self.recycle(capture);
        }
    }
}

#[inline]
pub(crate) fn observe(value: TaggedValue) {
    // This private membership is read afresh for each pointer projection. A
    // different mutator cannot change it, and no state is cached across reads
    // or callbacks. begin/take restore it before running any user code.
    if !ACTIVE.with(Cell::get) {
        return;
    }
    if CAPTURE_SCOPES.load(Ordering::Relaxed) & !(ReadPolicyBit::ObservedTraversal as usize) == 0 {
        return;
    }
    observe_active(value.bits());
}

// Inactive pointer reads return at the inline membership check. Keep active
// dependency recording cold and retain its existing private membership guard;
// neither gate runs a callback or can change this mutator's capture stack.
#[cold]
#[inline(never)]
fn observe_active(bits: usize) {
    if !ACTIVE.with(Cell::get) {
        return;
    }
    store_state::reject_foreign_capture_reads();
    observe_bits(bits);
}

fn clear_recent_reads() {
    RECENT_READS.with(|recent| {
        recent
            .observed
            .set(compiled_journal_mode() == CompiledJournalMode::Observed);
        recent.clear(super::gc::collection_observation_epoch());
    });
}

fn recent_publication_epoch() -> Option<u64> {
    RECENT_READS.with(|recent| {
        recent.observed.get().then(|| {
            let epoch = super::gc::collection_observation_epoch();
            recent.epoch.set(epoch);
            epoch
        })
    })
}

#[inline]
fn observe_bits(bits: usize) {
    // Keep exact-identity hits outside the mutable capture state. Ordinary
    // cons/vector access can inline this check without entering the slow
    // dependency recorder. Scope changes clear it so every active scope
    // still observes nested reads; collisions only cause another lookup.
    // Hits already published before their first revision/data read. The
    // collector and heap-installation handshakes clear this shortcut before
    // a replacement address can be accepted, so only misses publish owners.
    if !recently_observed(bits) {
        observe_new_owner(bits);
    }
}

#[inline]
fn recently_observed(bits: usize) -> bool {
    RECENT_READS.with(|recent| {
        let slot = &recent.words[((bits >> 3) ^ (bits >> 11)) & 255];
        bits != 0 && slot.replace(bits) == bits
    })
}

// This deliberately does not borrow STATE: fused setter projection already
// owns its mutable borrow. Each miss refreshes the ledger in its cold recorder
// before publishing the compiled gate or snapshotting a dependency.
#[cold]
#[inline(never)]
pub(super) fn reclaimed_collection_observations(epoch: u64) {
    // Keep the immutable policy branch entirely on the reclamation edge.
    // Off/Eager retain their identity-only words, but acknowledge this epoch
    // so subsequent reads do not enter the cold helper again.
    // Other heaps may reclaim concurrently; they cannot recycle this heap's
    // live owner. A migrating Context is covered by the installation reset.
    // try_with also permits final heap reclamation during TLS destruction.
    let _ = RECENT_READS.try_with(|recent| {
        if recent.observed.get() {
            recent.clear(epoch);
        } else {
            recent.epoch.set(epoch);
        }
    });
}

#[cold]
#[inline(never)]
pub(super) fn reset_recent_reads_for_heap_installation() {
    let _ = RECENT_READS.try_with(|recent| {
        recent.clear(super::gc::collection_observation_epoch());
    });
}

#[cold]
#[inline(never)]
fn observe_new_owner(bits: usize) {
    store_state::note_recent_miss(bits);
    // Misses acquire the shared epoch here, rather than on every cached read.
    // A real owner remains live while its pointer projection executes, even
    // if unrelated reclamation advances the global epoch concurrently.
    let publication_epoch = recent_publication_epoch();
    with_state(|state| {
        let mut state = state.borrow_mut();
        if let Some(epoch) = publication_epoch
            && state.observed.was_refreshed_at(epoch)
            && state.captures.last().is_some_and(|capture| {
                !capture.overflow
                    && capture.publication_epoch == Some(epoch)
                    && capture.reads.contains_key(&bits)
            })
        {
            // First insertion recorded every active outer scope too. Keep
            // each scope's original revision; a subsequent write still makes
            // the certificate incoherent. A new nested scope starts empty.
            return;
        }
        publish_observed_owner_in_state(&mut state, bits);
        record_observation(
            &mut state,
            bits,
            LispCollectionRevision::current(),
            publication_epoch,
        );
    });
}

#[cold]
#[inline(never)]
fn observe_uncached(bits: usize) {
    store_state::note_recent_miss(bits);
    store_state::reject_foreign_capture_reads();
    with_state(|state| {
        let mut state = state.borrow_mut();
        // A transitive cache hit must never dereference its old owner words.
        // Safe metadata filtering still refreshes its mutator's live bounds.
        refresh_observed_envelope(&mut state);
        let recent_epoch = recent_publication_epoch();
        let publication_epoch = recent_epoch.filter(|&epoch| {
            state.observed.was_refreshed_at(epoch) && state.observed.already_published(bits)
        });
        if recent_epoch.is_some() && publication_epoch.is_none() {
            // Off-mode certificates may contain an identity which was never
            // marked. Replay must not make its next real projection a RECENT
            // hit: that read alone can safely publish the live owner.
            RECENT_READS.with(|recent| recent.forget(bits));
        }
        record_observation(
            &mut state,
            bits,
            LispCollectionRevision::current(),
            publication_epoch,
        );
    });
}

// Both ordinary reads and fused setter projections use this recorder. Its
// caller owns STATE's borrow; no callback, heap barrier or recursive borrow is
// permitted here. An existing entry retains its first-read revision.
#[cold]
#[inline(never)]
fn record_observation(
    state: &mut State,
    bits: usize,
    revision: LispCollectionRevision,
    publication_epoch: Option<u64>,
) {
    #[cfg(test)]
    OBSERVATION_STATE_ACCESSES.with(|count| count.set(count.get() + 1));
    for capture in &mut state.captures {
        if capture.publication_epoch != publication_epoch {
            capture.publication_epoch = None;
        }
        if capture.overflow {
            continue;
        }
        if !capture.reads.insert_first(bits, revision) {
            capture.overflow = true;
        }
    }
}

/// Ordered protocol evidence for native/interpreter parity tests. Identities
/// remain words; taking this snapshot does not observe or dereference owners.
#[cfg(test)]
pub(crate) fn journal_since_for_test(start: LispCollectionRevision) -> Option<Vec<usize>> {
    with_store(|store| {
        let count = store.sequence().wrapping_sub(start.sequence());
        if count > JOURNAL_SIZE as u64 {
            return None;
        }
        Some(
            (1..=count)
                .map(|offset| {
                    let sequence = start.sequence().wrapping_add(offset);
                    store.journal()[sequence as usize % JOURNAL_SIZE].get()
                })
                .collect(),
        )
    })
}

pub(super) fn record_write(value: TaggedValue, revision: u64) {
    with_store(|store| store.record(value.bits(), revision));
}

/// A string byte store retains its original journal-before-read order.
/// Observation is a separate projection after this shared history step.
#[inline(always)]
pub(super) fn record_unprojected_write_in_store(store: &StoreState, value: TaggedValue) {
    if store.mutator.policy.get() == JournalPolicy::BeforeFirstCapture {
        record_unprojected_write_before_first_capture(store, value);
        return;
    }
    let revision = store.advance();
    store.record(value.bits(), revision);
}

#[cold]
#[inline(never)]
fn record_unprojected_write_before_first_capture(store: &StoreState, value: TaggedValue) {
    if history_required() {
        store.mutator.policy.set(JournalPolicy::JournalOnly);
        let revision = store.advance();
        store.record(value.bits(), revision);
    }
}

/// Preserve ordinary projection policy without an idle ACTIVE TLS read.
#[inline(always)]
pub(crate) fn observe_store_projection_in_store(store: &StoreState, value: TaggedValue) {
    if store.mutator.observe.get() {
        observe_store_projection_active(value);
    }
}

#[cold]
#[inline(never)]
fn observe_store_projection_active(value: TaggedValue) {
    observe(value);
}

/// The exact pointer projection performed by the four fused setters. A failed
/// Cons tag check observes nothing; veclike setters observe every veclike header
/// before rejecting a subtype. This is deliberately narrower than heap objects.
#[derive(Clone, Copy)]
pub(crate) enum WriteProjection {
    Cons,
    VecLike,
}

/// Journal this setter call, then observe its eligible pointer projection at
/// the new revision. Returns whether the tag permits that projection; after
/// return the caller may form its raw pointer without another observation.
///
/// This mutator alone owns STATE, ACTIVE, its revision and recent read probes.
/// Other mutators only upgrade the process history requirement. A prefix mode
/// checks that flag on every write until true, then permanently journals writes.
/// No Lisp state is cached, and STATE's borrow ends before the caller handles
/// a header, GC barrier, backing ownership, slot bounds, or the actual store.
#[inline(always)]
pub(super) fn record_projected_write(value: TaggedValue, projection: WriteProjection) -> bool {
    super::gc::with_store_state(|store| record_projected_write_in_store(store, value, projection))
}

#[inline(always)]
pub(crate) fn record_projected_write_in_store(
    store: &StoreState,
    value: TaggedValue,
    projection: WriteProjection,
) -> bool {
    let projected = match projection {
        WriteProjection::Cons => value.is_cons(),
        WriteProjection::VecLike => value.is_veclike(),
    };
    if store.mutator.policy.get() != JournalPolicy::JournalOnly {
        record_projected_write_cold_policy(store, value, projected);
        return projected;
    }
    let revision = store.advance();
    store.record(value.bits(), revision);
    projected
}

// Every inline store carries only the policy-byte test and this cold edge.
// Prefix-policy transitions and active captures must not duplicate their
// bookkeeping at each native caller's store site.
#[cold]
#[inline(never)]
fn record_projected_write_cold_policy(store: &StoreState, value: TaggedValue, projected: bool) {
    match store.mutator.policy.get() {
        JournalPolicy::JournalAndObserve => {
            record_projected_write_observed(store, value, projected);
            return;
        }
        JournalPolicy::BeforeFirstCapture => {
            if !history_required() {
                return;
            }
            store.mutator.policy.set(JournalPolicy::JournalOnly);
        }
        JournalPolicy::JournalOnly => {}
    }
    let revision = store.advance();
    store.record(value.bits(), revision);
}

// Capture/recent/envelope TLS and the RefCell are confined to this active
// path. Owner publication still precedes the write; its first read records
// the new revision, so a setter-only capture rejects its own mutation.
#[cold]
#[inline(never)]
fn record_projected_write_observed(store: &StoreState, value: TaggedValue, projected: bool) {
    let recent = projected && recently_observed(value.bits());
    if projected && !recent {
        store_state::note_recent_miss(value.bits());
    }
    if projected {
        store_state::reject_foreign_capture_reads();
    }
    with_state(|state| {
        let mut state = state.borrow_mut();
        if projected && !recent {
            publish_observed_owner_in_state(&mut state, value.bits());
        }
        let revision = LispCollectionRevision::from_sequence(store.advance());
        store.record(value.bits(), revision.sequence());
        if projected && !recent {
            record_observation(
                &mut state,
                value.bits(),
                revision,
                recent_publication_epoch(),
            );
        }
    });
}

mod capture_storage;
mod certificate_storage;
mod store_state;
use store_state::with_state;
pub(crate) use store_state::{
    ObservationWorld, StoreState, clear_installed_store_heap, with_store,
};

mod compiled_journal;
mod observed_envelope;
#[cfg(test)]
pub(crate) use compiled_journal::force_compiled_journal_for_test;
pub(crate) use compiled_journal::{CompiledJournalMode, compiled_journal_mode, is_observed};

/// Snapshot only the current mutator's existing journal metadata. Installation
/// of a Context is exclusive on that mutator and republishes this envelope.
pub(crate) fn compiled_observation_window() -> (usize, usize) {
    if compiled_journal_mode() != CompiledJournalMode::Observed
        || !super::gc::has_collection_observations()
    {
        return (usize::MAX, 0);
    }
    with_state(|state| {
        let mut state = state.borrow_mut();
        refresh_observed_envelope(&mut state);
        state.observed.full_bounds()
    })
}

/// Non-dump owner ranges and an exact empty gap for this actual heap's dump
/// span. Callers pass the new span explicitly when installing or republishing
/// a heap; the ordinary TLS barrier mirror may still describe its prior state.
pub(crate) fn compiled_observation_gate(
    dump: super::gc::BarrierWindow,
) -> super::gc::CompiledObservationGate {
    compiled_observation_gate_for_world(&ObservationWorld::current(), dump)
}

/// Collector publication uses the named World's envelope even when another
/// World is selected in TLS. The caller owns its stopped/installation phase.
pub(crate) fn compiled_observation_gate_for_world(
    owner: &ObservationWorld,
    dump: super::gc::BarrierWindow,
) -> super::gc::CompiledObservationGate {
    if compiled_journal_mode() != CompiledJournalMode::Observed
        || !super::gc::has_collection_observations()
    {
        return super::gc::CompiledObservationGate::NONE;
    }
    owner.with_state(|state| {
        let mut state = state.borrow_mut();
        state.observed.refresh(dump);
        state.observed.gate()
    })
}

/// A native false-positive window hit can exclude its whole empty interval.
/// This is cold protocol refinement, not a Lisp-state cache: every locally
/// observed Cons/Vector/Record address bounds the chosen gap. A tiny cons gap
/// additionally supplies one exact exemption that survives gap replacement.
/// The caller has already rejected ordinary GC/dump barrier eligibility.
#[cold]
#[inline(never)]
pub(crate) fn exclude_unobserved_compiled_cons(bits: usize) -> bool {
    if compiled_journal_mode() != CompiledJournalMode::Observed
        || bits & super::value::TAG_MASK != super::value::TAG_CONS
        || bits & !super::value::TAG_MASK == 0
    {
        return false;
    }
    let dump = super::gc::current_collection_dump_window();
    with_state(|state| {
        let mut state = state.borrow_mut();
        let refreshed = state.observed.refresh(dump);
        let excluded = state.observed.exclude_unobserved(bits);
        if refreshed || excluded {
            super::gc::publish_collection_observation_window(state.observed.gate());
        }
        excluded
    })
}

fn publish_compiled_observation_window() {
    let dump = super::gc::current_collection_dump_window();
    let gate = compiled_observation_gate(dump);
    super::gc::publish_collection_observation_window(gate);
}

#[cold]
#[inline(never)]
fn refresh_observed_envelope(state: &mut State) {
    if compiled_journal_mode() != CompiledJournalMode::Observed {
        return;
    }
    let dump = super::gc::current_collection_dump_window();
    if state.observed.refresh(dump) {
        // The heap-side publisher accepts a prepared gate and does not call
        // back into STATE, whose borrow is already held here.
        super::gc::publish_collection_observation_window(state.observed.gate());
    }
}

#[cold]
#[inline(never)]
fn publish_observed_owner_in_state(state: &mut State, bits: usize) -> bool {
    if compiled_journal_mode() != CompiledJournalMode::Observed {
        return false;
    }
    // Same-epoch membership proves sticky publication, but reclamation on
    // another mutator can reuse a veclike address and mark its new lifetime
    // before this ledger refreshes. Promote cached native eligibility only
    // from a real live projection; stale certificate replay never reads a
    // header. Positive slot eligibility may conservatively survive reuse.
    let tag = bits & super::value::TAG_MASK;
    let promoted_geometry = if state.observed.already_published(bits) {
        if tag != super::value::TAG_VECLIKE || state.observed.has_slot_geometry(bits) {
            return false;
        }
        let live_geometry = native_window_collection_owner(bits);
        if live_geometry != NativeWindowCollectionOwner::Slots {
            return false;
        }
        Some(live_geometry)
    } else {
        None
    };
    let dump = super::gc::current_collection_dump_window();
    let refreshed = state.observed.refresh(dump);
    if matches!(
        tag,
        super::value::TAG_CONS | super::value::TAG_STRING | super::value::TAG_VECLIKE
    ) && bits & !super::value::TAG_MASK != 0
    {
        // Another mutator may have published the shared mark first. Its mark
        // does not insert this identity into our private journal envelope.
        // Header subtype is immutable throughout this live owner's lifetime.
        // Capture only the eligibility fact here: reclamation and transitive
        // replay must never dereference the ledger's unrooted owner addresses.
        let native_geometry =
            promoted_geometry.unwrap_or_else(|| native_window_collection_owner(bits));
        let changed = state.observed.insert(bits, native_geometry);
        let newly = if changed {
            #[cfg(test)]
            OBSERVED_OWNER_PUBLICATIONS.with(|count| count.set(count.get() + 1));
            super::gc::mark_collection_observed(bits)
        } else {
            // Epoch refresh retained this identity only after its exact metadata
            // mark was found. Its live publication needs no second radix query.
            false
        };
        if refreshed || changed {
            super::gc::publish_collection_observation_window(state.observed.gate());
        }
        newly
    } else if refreshed {
        super::gc::publish_collection_observation_window(state.observed.gate());
        false
    } else {
        false
    }
}

/// Only these native-store owners rely on the shared address-window gate.
/// String stores use their own exact header-byte guard and journal shim, so
/// they keep marks/full ledger membership without constraining this geometry.
/// This cold publication caller owns a live pointer projection and State's
/// exclusive borrow, with no callback or safepoint. Raw tag/header decoding
/// avoids recursively observing through `as_veclike_ptr`/`veclike_type`.
/// Every owner still receives the same sticky mark and full publication-ledger
/// membership; this fact only limits native address-window geometry.
#[inline]
fn native_window_collection_owner(bits: usize) -> NativeWindowCollectionOwner {
    use super::header::{VecLikeHeader, VecLikeType};
    match bits & super::value::TAG_MASK {
        super::value::TAG_CONS => NativeWindowCollectionOwner::Cons,
        super::value::TAG_VECLIKE => {
            // SAFETY: called only for a live non-null veclike owner during its
            // real first pointer projection. Serialized/stale certificate
            // identities never enter this header-reading publication path.
            let header = unsafe { &*((bits & !super::value::TAG_MASK) as *const VecLikeHeader) };
            if matches!(header.type_tag, VecLikeType::Vector | VecLikeType::Record) {
                NativeWindowCollectionOwner::Slots
            } else {
                NativeWindowCollectionOwner::Other
            }
        }
        _ => NativeWindowCollectionOwner::Other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeWindowCollectionOwner {
    Other,
    Cons,
    Slots,
}

/// Observe reads made by `read`, rejecting reuse if its dependency budget or
/// coherent mutation history cannot be retained. No Lisp object is rooted.
pub fn capture<T>(read: impl FnOnce() -> T) -> (T, Option<CollectionReads>) {
    let scope = CollectionReadScope::begin();
    let value = read();
    (value, scope.finish())
}

/// Normalize derived inputs before observing an output. Dependencies read by
/// normalization remain live, but its own writes precede the output being
/// cached. Only this scope is rebased; an enclosing observation still rejects
/// writes to anything it read before normalization.
pub fn capture_normalized<S, T>(
    state: &mut S,
    normalize: impl FnOnce(&mut S),
    read: impl FnOnce(&mut S) -> T,
) -> (T, Option<CollectionReads>) {
    let scope = CollectionReadScope::begin();
    normalize(state);
    if scope.active {
        scope.owner.with_state(|state| {
            let mut state = state.borrow_mut();
            let capture = state.captures.last_mut().expect("normalization scope");
            let now = LispCollectionRevision::from_sequence(scope.owner.store().sequence());
            capture.started = now;
            capture.reads.rebase(now);
        });
    }
    let value = read(state);
    (value, scope.finish())
}

#[cfg(test)]
#[path = "collection_reads/tests/collection_reads_test.rs"]
mod tests;

#[cfg(test)]
#[path = "collection_reads/tests/allocation_test.rs"]
mod allocation_tests;

#[cfg(test)]
#[path = "collection_reads/tests/recent_clear_test.rs"]
mod recent_clear_tests;

/// Round 3w (plan-3w.md section 7, C1): this World state lives in a
/// thread-local while its World's Baton holder runs, and in the World while
/// it is parked; the Green runtime moves it at each park and resume, so
/// every Lisp thread sees one value, as in GNU. The recent-read cache is
/// cleared instead: it only ever answers again. (N-mutator mode needs shared
/// storage instead: a recorded obligation.)
#[cold]
pub(crate) fn world_state_placeholder() -> Box<dyn std::any::Any> {
    store_state::carried_placeholder()
}

#[cold]
pub(crate) fn swap_world_state(slot: &mut dyn std::any::Any) {
    store_state::swap_carried(slot);
}

#[cfg(test)]
#[path = "tests/write_fusion_test.rs"]
mod write_fusion_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "collection_reads/tests/lazy_window_test.rs"]
mod lazy_window_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "collection_reads/tests/observed_epoch_test.rs"]
mod observed_epoch_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "collection_reads/tests/observed_gap_test.rs"]
mod observed_gap_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "collection_reads/tests/observed_collisions_test.rs"]
mod observed_collisions_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "collection_reads/tests/observed_reused_subtype_test.rs"]
mod observed_reused_subtype_tests;
