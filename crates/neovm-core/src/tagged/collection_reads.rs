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
    static RECENT_READS: [Cell<usize>; 256] = const { [const { Cell::new(0) }; 256] };
    static STATE: RefCell<State> = RefCell::new(State::default());
    #[cfg(test)]
    static OBSERVATION_STATE_ACCESSES: Cell<usize> = const { Cell::new(0) };
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

struct State {
    writes: Box<[usize; JOURNAL_SIZE]>,
    captures: Vec<Capture>,
    journal_policy: JournalPolicy,
    // A conservative envelope of this mutator's sticky observed owners. It
    // lives with the existing journal, not a Context, and never resets while
    // certificates can survive. Holes are filtered by exact object marks.
    observed_lo: usize,
    observed_hi: usize,
}

impl Default for State {
    fn default() -> Self {
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
            observed_lo: usize::MAX,
            observed_hi: 0,
        }
    }
}

struct Capture {
    reads: FxHashMap<usize, LispCollectionRevision>,
    started: LispCollectionRevision,
    overflow: bool,
}

/// Exact mutable collection reads made by one completed observation.
pub struct CollectionReads {
    reads: FxHashSet<usize>,
    revision: LispCollectionRevision,
    _not_send: std::marker::PhantomData<*const ()>,
}

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
struct CollectionReadScope {
    active: bool,
    _not_send: std::marker::PhantomData<*const ()>,
}

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
        for slot in recent {
            slot.set(0);
        }
    });
}

#[inline]
fn observe_bits(bits: usize) {
    // Keep exact-identity hits outside the mutable capture state. Ordinary
    // cons/vector access can inline this check without entering the slow
    // dependency recorder. Scope changes clear it so every active scope
    // still observes nested reads; collisions only cause another lookup.
    // Mark before the revision snapshot and before the read-cache shortcut:
    // GC may have recycled this address since its prior observation.
    let newly_observed = publish_observed_owner(bits);
    if newly_observed || !recently_observed(bits) {
        observe_uncached(bits);
    }
}

#[inline]
fn recently_observed(bits: usize) -> bool {
    RECENT_READS.with(|recent| {
        let slot = &recent[((bits >> 3) ^ (bits >> 11)) & 255];
        bits != 0 && slot.replace(bits) == bits
    })
}

#[cold]
#[inline(never)]
fn observe_uncached(bits: usize) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        record_observation(&mut state, bits, LispCollectionRevision::current());
    });
}

// Both ordinary reads and fused setter projections use this recorder. Its
// caller owns STATE's borrow; no callback, heap barrier or recursive borrow is
// permitted here. An existing entry retains its first-read revision.
#[cold]
#[inline(never)]
fn record_observation(state: &mut State, bits: usize, revision: LispCollectionRevision) {
    #[cfg(test)]
    OBSERVATION_STATE_ACCESSES.with(|count| count.set(count.get() + 1));
    for capture in &mut state.captures {
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
        if observe && projected {
            publish_observed_owner_in_state(&mut state, value.bits());
        }
        let revision = LispCollectionRevision::advance();
        state.writes[revision.sequence() as usize % JOURNAL_SIZE] = value.bits();
        if observe && projected && !recently_observed(value.bits()) {
            record_observation(&mut state, value.bits(), revision);
        }
    });
    projected
}

mod compiled_journal;
#[cfg(test)]
pub(crate) use compiled_journal::force_compiled_journal_for_test;
pub(crate) use compiled_journal::{CompiledJournalMode, compiled_journal_mode, is_observed};

/// Snapshot only the current mutator's existing journal metadata. Installation
/// of a Context is exclusive on that mutator and republishes this envelope.
pub(crate) fn compiled_observation_window() -> (usize, usize) {
    if compiled_journal_mode() != CompiledJournalMode::Observed {
        return (usize::MAX, 0);
    }
    STATE.with(|state| {
        let state = state.borrow();
        (state.observed_lo, state.observed_hi)
    })
}

fn publish_compiled_observation_window() {
    let (lo, hi) = compiled_observation_window();
    super::gc::publish_collection_observation_window(lo, hi);
}

#[cold]
fn publish_observed_owner(bits: usize) -> bool {
    if compiled_journal_mode() != CompiledJournalMode::Observed {
        return false;
    }
    STATE.with(|state| publish_observed_owner_in_state(&mut state.borrow_mut(), bits))
}

fn publish_observed_owner_in_state(state: &mut State, bits: usize) -> bool {
    if compiled_journal_mode() != CompiledJournalMode::Observed {
        return false;
    }
    let newly = super::gc::mark_collection_observed(bits);
    let tag = bits & super::value::TAG_MASK;
    if matches!(
        tag,
        super::value::TAG_CONS | super::value::TAG_STRING | super::value::TAG_VECLIKE
    ) && bits & !super::value::TAG_MASK != 0
    {
        let address = bits & !super::value::TAG_MASK;
        let lo = state.observed_lo.min(address);
        let hi = state.observed_hi.max(address + 1);
        if lo != state.observed_lo || hi != state.observed_hi {
            state.observed_lo = lo;
            state.observed_hi = hi;
            // No callback or STATE reborrow occurs in the heap-side publisher.
            super::gc::publish_collection_observation_window(lo, hi);
        }
    }
    newly
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
mod tests;

#[cfg(test)]
#[path = "tests/write_fusion.rs"]
mod write_fusion_tests;
