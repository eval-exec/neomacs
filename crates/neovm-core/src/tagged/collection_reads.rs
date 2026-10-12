//! Bounded dependency observations for short-lived layout queries.
//!
//! Object identities are words, never GC roots or dereferenced pointers. A
//! fixed mutation journal allows an observation to survive unrelated writes.
//! Losing journal history or exceeding the read budget always rejects reuse.
//! Pointer reads first check this mutator's existing private scope membership;
//! an inactive mutator skips the shared gate. Active reads retain the process
//! policy gate and cold recorder, while traversal selection keeps its own gate.
use super::{mutate::LispCollectionRevision, value::TaggedValue};
use rustc_hash::{FxHashMap, FxHashSet};
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
    static STATE: RefCell<State> = RefCell::new(State::default());
    #[cfg(test)]
    static OBSERVATION_STATE_ACCESSES: Cell<usize> = const { Cell::new(0) };
    #[cfg(test)]
    static OBSERVED_OWNER_PUBLICATIONS: Cell<usize> = const { Cell::new(0) };
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
    epoch: Cell<u64>,
}

impl RecentReads {
    const fn new() -> Self {
        Self {
            observed: Cell::new(false),
            epoch: Cell::new(0),
            words: [const { Cell::new(0) }; 256],
        }
    }

    fn clear(&self, epoch: u64) {
        for word in &self.words {
            word.set(0);
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
enum JournalPolicy {
    JournalOnly,
    BeforeFirstCapture,
    JournalAndObserve,
}

// Keep the journal prefix independent of cold observation metadata growth.
// This is a private Rust layout; generated code does not bake these offsets.
// The executing mutator owns this State; only atomic marks/epochs are shared.
#[repr(C)]
struct State {
    captures: Vec<Capture>,
    writes: Box<[usize; JOURNAL_SIZE]>,
    journal_policy: JournalPolicy,
    // Exact identities and conservative bounds live in the existing mutator
    // journal. Reclamation prunes dead identities without dereferencing them;
    // Context/dump changes reclassify the surviving owner ranges.
    observed: observed_envelope::ObservedEnvelope,
}

const _: () = assert!(std::mem::offset_of!(State, captures) == 0);
const _: () = assert!(std::mem::offset_of!(State, writes) == 3 * std::mem::size_of::<usize>());
const _: () =
    assert!(std::mem::offset_of!(State, journal_policy) == 4 * std::mem::size_of::<usize>());

impl Default for State {
    fn default() -> Self {
        #[cfg(test)]
        STATE_INITIALIZATIONS.fetch_add(1, Ordering::Relaxed);
        // CONFIG cannot touch STATE or call Lisp. Force it before remembering
        // history permanently: WRITE_LAZY may clear the conservative initial flag.
        initialize();
        Self {
            writes: Box::new([0; JOURNAL_SIZE]),
            captures: Vec::new(),
            journal_policy: if history_required() {
                JournalPolicy::JournalOnly
            } else {
                JournalPolicy::BeforeFirstCapture
            },
            observed: observed_envelope::ObservedEnvelope::new(),
        }
    }
}

struct Capture {
    reads: FxHashMap<usize, LispCollectionRevision>,
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
    reads: FxHashSet<usize>,
    revision: LispCollectionRevision,
    _not_send: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(CollectionReads: Send, Sync);

impl CollectionReads {
    pub fn unchanged(&self) -> bool {
        unchanged_since(&self.reads, self.revision)
    }

    /// A nested cache hit reads its original dependencies transitively.
    pub fn unchanged_and_observe(&self) -> bool {
        if !self.unchanged() {
            return false;
        }
        if ACTIVE.with(Cell::get) {
            for &bits in &self.reads {
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

fn unchanged_since(reads: &FxHashSet<usize>, revision: LispCollectionRevision) -> bool {
    let now = LispCollectionRevision::current();
    let count = now.sequence().wrapping_sub(revision.sequence());
    if count > JOURNAL_SIZE as u64 {
        return false;
    }
    STATE.with(|state| {
        let state = state.borrow();
        (1..=count).all(|offset| {
            let slot = revision.sequence().wrapping_add(offset) as usize % JOURNAL_SIZE;
            !reads.contains(&state.writes[slot])
        })
    })
}

/// Nested observations contribute reads to their enclosing observation too.
/// The guard cannot move across threads; unwinding restores the prior scope.
#[derive(Debug)]
#[must_use = "dropping the guard ends this mutator's collection-read capture"]
struct CollectionReadScope {
    active: bool,
    _not_send: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(CollectionReadScope: Send, Sync);

impl CollectionReadScope {
    fn begin() -> Self {
        initialize();
        HISTORY_REQUIRED.store(true, Ordering::Relaxed);
        let admitted = STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.journal_policy = JournalPolicy::JournalAndObserve;
            let overflow = state.captures.len() >= MAX_DEPTH;
            if overflow {
                for capture in &mut state.captures {
                    capture.overflow = true;
                }
                return false;
            }
            clear_recent_reads();
            state.captures.push(Capture {
                reads: FxHashMap::default(),
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
            _not_send: std::marker::PhantomData,
        }
    }

    fn finish(mut self) -> Option<CollectionReads> {
        if !self.active {
            return None;
        }
        let capture = self.take();
        // Reject writes after a dependency's first read. Writes before its
        // first read (including initialization of fresh objects) are harmless.
        let now = LispCollectionRevision::current();
        let count = now.sequence().wrapping_sub(capture.started.sequence());
        if capture.overflow || count > JOURNAL_SIZE as u64 {
            tracing::trace!(target: "neovm_core::collection_reads", reads = capture.reads.len(), writes = count, overflow = capture.overflow, "capture rejected");
            return None;
        }
        let coherent = STATE.with(|state| {
            let state = state.borrow();
            (1..=count).all(|offset| {
                let slot = capture.started.sequence().wrapping_add(offset) as usize % JOURNAL_SIZE;
                capture
                    .reads
                    .get(&state.writes[slot])
                    .is_none_or(|read_at| {
                        offset <= read_at.sequence().wrapping_sub(capture.started.sequence())
                    })
            })
        });
        if !coherent {
            tracing::trace!(target: "neovm_core::collection_reads", reads = capture.reads.len(), writes = count, "capture changed during observation");
            return None;
        }
        Some(CollectionReads {
            reads: capture.reads.into_keys().collect(),
            revision: now,
            _not_send: std::marker::PhantomData,
        })
    }

    fn take(&mut self) -> Capture {
        self.active = false;
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let capture = state.captures.pop().expect("collection read scope");
            clear_recent_reads();
            let active = !state.captures.is_empty();
            state.journal_policy = if active {
                JournalPolicy::JournalAndObserve
            } else {
                JournalPolicy::JournalOnly
            };
            ACTIVE.with(|membership| membership.set(active));
            CAPTURE_SCOPES.fetch_sub(SCOPE_INCREMENT, Ordering::Relaxed);
            capture
        })
    }
}

impl Drop for CollectionReadScope {
    fn drop(&mut self) {
        if self.active {
            self.take();
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
    // Misses acquire the shared epoch here, rather than on every cached read.
    // A real owner remains live while its pointer projection executes, even
    // if unrelated reclamation advances the global epoch concurrently.
    let publication_epoch = recent_publication_epoch();
    STATE.with(|state| {
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
    STATE.with(|state| {
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
        if capture.reads.len() == MAX_READS && !capture.reads.contains_key(&bits) {
            capture.overflow = true;
        } else {
            capture.reads.entry(bits).or_insert(revision);
        }
    }
}

pub(super) fn record_write(value: TaggedValue, revision: u64) {
    STATE.with(|state| state.borrow_mut().writes[revision as usize % JOURNAL_SIZE] = value.bits());
}

/// The exact pointer projection performed by the four fused setters. A failed
/// Cons tag check observes nothing; veclike setters observe every veclike header
/// before rejecting a subtype. This is deliberately narrower than heap objects.
#[derive(Clone, Copy)]
pub(super) enum WriteProjection {
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
    let projected = match projection {
        WriteProjection::Cons => value.is_cons(),
        WriteProjection::VecLike => value.is_veclike(),
    };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let observe = match state.journal_policy {
            JournalPolicy::JournalOnly => false,
            JournalPolicy::JournalAndObserve => true,
            JournalPolicy::BeforeFirstCapture => {
                if !history_required() {
                    return;
                }
                state.journal_policy = JournalPolicy::JournalOnly;
                false
            }
        };
        // Keep capture eligibility ahead of the common journal block. In
        // JournalOnly, this branches directly to the revision/write pair;
        // no observation booleans remain live across that block.
        let recent = observe && projected && recently_observed(value.bits());
        if observe && projected && !recent {
            publish_observed_owner_in_state(&mut state, value.bits());
        }
        let revision = LispCollectionRevision::advance();
        state.writes[revision.sequence() as usize % JOURNAL_SIZE] = value.bits();
        if observe && projected && !recent {
            record_observation(
                &mut state,
                value.bits(),
                revision,
                recent_publication_epoch(),
            );
        }
    });
    projected
}

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
    STATE.with(|state| {
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
    if compiled_journal_mode() != CompiledJournalMode::Observed
        || !super::gc::has_collection_observations()
    {
        return super::gc::CompiledObservationGate::NONE;
    }
    STATE.with(|state| {
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
    STATE.with(|state| {
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
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let capture = state.captures.last_mut().expect("normalization scope");
            let now = LispCollectionRevision::current();
            capture.started = now;
            for revision in capture.reads.values_mut() {
                *revision = now;
            }
        });
    }
    let value = read(state);
    (value, scope.finish())
}

#[cfg(test)]
#[path = "collection_reads/tests/collection_reads_test.rs"]
mod tests;

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
