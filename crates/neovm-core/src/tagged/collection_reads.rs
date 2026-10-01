//! Bounded dependency observations for short-lived layout queries.
//!
//! Object identities are words, never GC roots or dereferenced pointers. A
//! fixed mutation journal allows an observation to survive unrelated writes.
//! Losing journal history or exceeding the read budget always rejects reuse.
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
/// | `NEOVM_COLLECTION_READ_GLOBAL=on` | off | Skip TLS when no mutator has an active capture. |
/// | `NEOVM_COLLECTION_READ_HOIST=on` | off | Select an unobserved list walk outside callback-free loops. |
/// | `NEOVM_COLLECTION_WRITE_LAZY=on` | off | Skip revision/journal work until the first process capture. |
///
/// Scope state and journals remain local to each mutator. The process gates
/// contain no Lisp identities and are safe with concurrent mutators. The low
/// bit keeps the legacy TLS path enabled when the global knob is off; each
/// admitted scope contributes two. Relaxed ordering suffices: a mutator's own
/// begin is sequenced before its observation, so it cannot read a count from
/// before that begin. Other mutators only make the gate more conservative.
static CAPTURE_SCOPES: AtomicUsize = AtomicUsize::new(1);

/// Once any mutator has started a capture, all subsequent writes keep their
/// thread's history, including writes between scopes while certificates live.
/// Never reset this gate on scope exit. No Lisp state is shared by this flag.
static HISTORY_REQUIRED: AtomicBool = AtomicBool::new(true);

/// A process configuration flag, immutable after initialization. This does not
/// cache Lisp state and is safe to read from several mutators.
static HOIST_READS: AtomicBool = AtomicBool::new(false);

static CONFIG: LazyLock<()> = LazyLock::new(|| {
    if std::env::var("NEOVM_COLLECTION_READ_GLOBAL").as_deref() == Ok("on") {
        CAPTURE_SCOPES.fetch_and(!1, Ordering::Relaxed);
    }
    if std::env::var("NEOVM_COLLECTION_WRITE_LAZY").as_deref() == Ok("on") {
        HISTORY_REQUIRED.store(false, Ordering::Relaxed);
    }
    HOIST_READS.store(
        std::env::var("NEOVM_COLLECTION_READ_HOIST").as_deref() == Ok("on"),
        Ordering::Relaxed,
    );
});

pub(super) fn initialize() {
    LazyLock::force(&CONFIG);
}

#[inline]
pub(crate) fn hoist_reads() -> bool {
    HOIST_READS.load(Ordering::Relaxed)
}

#[inline]
pub(crate) fn is_active() -> bool {
    CAPTURE_SCOPES.load(Ordering::Relaxed) != 0 && ACTIVE.with(Cell::get)
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

struct State {
    writes: Box<[usize; JOURNAL_SIZE]>,
    captures: Vec<Capture>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            writes: Box::new([0; JOURNAL_SIZE]),
            captures: Vec::new(),
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
                observe_bits(bits);
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
            CAPTURE_SCOPES.fetch_add(2, Ordering::Relaxed);
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
            ACTIVE.with(|active| active.set(!state.captures.is_empty()));
            CAPTURE_SCOPES.fetch_sub(2, Ordering::Relaxed);
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
    if !is_active() {
        return;
    }
    observe_bits(value.bits());
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
    let seen = RECENT_READS.with(|recent| {
        let slot = &recent[((bits >> 3) ^ (bits >> 11)) & 255];
        bits != 0 && slot.replace(bits) == bits
    });
    if !seen {
        observe_uncached(bits);
    }
}

#[cold]
#[inline(never)]
fn observe_uncached(bits: usize) {
    #[cfg(test)]
    OBSERVATION_STATE_ACCESSES.with(|count| count.set(count.get() + 1));
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        for capture in &mut state.captures {
            if capture.overflow {
                continue;
            }
            if capture.reads.len() == MAX_READS && !capture.reads.contains_key(&bits) {
                capture.overflow = true;
            } else {
                capture
                    .reads
                    .entry(bits)
                    .or_insert_with(LispCollectionRevision::current);
            }
        }
    });
}

pub(super) fn record_write(value: TaggedValue, revision: u64) {
    STATE.with(|state| state.borrow_mut().writes[revision as usize % JOURNAL_SIZE] = value.bits());
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
