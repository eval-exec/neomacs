//! Bounded dependency observations for short-lived layout queries.
//!
//! Object identities are words, never GC roots or dereferenced pointers. A
//! fixed mutation journal allows an observation to survive unrelated writes.
//! Losing journal history or exceeding the read budget always rejects reuse.
use super::{mutate::LispCollectionRevision, value::TaggedValue};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};

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
    if !ACTIVE.with(Cell::get) {
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
mod tests {
    use super::*;
    use crate::emacs_core::Value;

    #[test]
    fn repeated_reads_skip_capture_state_but_preserve_nested_dependencies() {
        let source = Value::cons(Value::NIL, Value::NIL);
        let (_, outer) = capture(|| {
            OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
            for _ in 0..1024 {
                source.cons_car();
            }
            assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 1);
            let (_, inner) = capture(|| {
                for _ in 0..1024 {
                    source.cons_cdr();
                }
            });
            assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 2);
            source.set_car(Value::T);
            assert!(!inner.unwrap().unchanged());
            source.cons_car();
            assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 3);
        });
        assert!(
            outer.is_none(),
            "the first read must still precede the mutation"
        );
    }

    #[test]
    fn observed_cons_survives_unrelated_mutation_but_not_its_own() {
        let source = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
        let unrelated = Value::cons(Value::NIL, Value::NIL);
        let (_, reads) = capture(|| source.cons_cdr().cons_car());
        let reads = reads.unwrap();
        unrelated.set_cdr(Value::T);
        assert!(reads.unchanged());
        source.cons_cdr().set_car(Value::fixnum(3));
        assert!(!reads.unchanged());
    }

    #[test]
    fn nested_capture_and_write_during_observation_are_conservative() {
        let source = Value::cons(Value::NIL, Value::NIL);
        let (_, outer) = capture(|| {
            let (_, inner) = capture(|| source.cons_car());
            assert!(inner.unwrap().unchanged());
        });
        let outer = outer.unwrap();
        source.set_car(Value::T);
        assert!(!outer.unchanged());
        let (_, invalid) = capture(|| {
            source.cons_car();
            source.set_car(Value::NIL);
        });
        assert!(invalid.is_none());
        let (_, fresh) = capture(|| {
            source.set_car(Value::T);
            source.cons_car()
        });
        assert!(fresh.unwrap().unchanged());
    }

    #[test]
    fn nested_cache_hit_propagates_dependencies() {
        let source = Value::cons(Value::NIL, Value::NIL);
        let (_, inner) = capture(|| source.cons_car());
        let inner = inner.unwrap();
        let (_, outer) = capture(|| assert!(inner.unchanged_and_observe()));
        source.set_car(Value::T);
        assert!(!outer.unwrap().unchanged());
    }

    #[test]
    fn normalization_retains_dependencies_without_rebasing_outer_reads() {
        let mut source = Value::cons(Value::NIL, Value::NIL);
        let (_, outer) = capture(|| {
            source.cons_car();
            let (result, inner) = capture_normalized(
                &mut source,
                |source| {
                    source.cons_car();
                    source.set_car(Value::T);
                },
                |source| source.cons_car(),
            );
            assert_eq!(result, Value::T);
            assert!(inner.unwrap().unchanged());
        });
        assert!(outer.is_none());
        let (_, reads) = capture_normalized(
            &mut source,
            |source| {
                source.cons_car();
                source.set_car(Value::NIL);
            },
            |_| (),
        );
        source.set_car(Value::T);
        assert!(!reads.unwrap().unchanged());
    }

    #[test]
    fn nested_scope_budget_is_bounded_and_recovers() {
        fn nested(depth: usize) {
            let (_, reads) = capture(|| {
                assert!(STATE.with(|s| s.borrow().captures.len()) <= MAX_DEPTH);
                if depth > 0 {
                    nested(depth - 1);
                }
            });
            assert!(reads.is_none());
        }
        nested(MAX_DEPTH + 5);
        assert!(!ACTIVE.with(Cell::get));
        assert!(capture(|| ()).1.unwrap().unchanged());
    }

    #[test]
    fn vector_string_and_character_table_mutations_invalidate_reads() {
        let vector = Value::vector(vec![Value::NIL]);
        let string = Value::string("abc");
        let table = Value::make_char_table(Value::NIL, Value::NIL, 0);
        let (_, reads) = capture(|| vector.as_vector_data().unwrap().len());
        vector.set_vector_slot(0, Value::T);
        assert!(!reads.unwrap().unchanged());
        let (_, reads) = capture(|| string.as_str_owned());
        string.set_string_byte_same_char_count(0, b'z');
        assert!(!reads.unwrap().unchanged());
        let (_, reads) = capture(|| table.as_char_table_obj().unwrap().defalt);
        table.with_char_table_mut(|table| table.defalt = Value::T);
        assert!(!reads.unwrap().unchanged());
    }

    #[test]
    fn lost_mutation_history_refuses_reuse() {
        let source = Value::cons(Value::NIL, Value::NIL);
        let unrelated = Value::cons(Value::NIL, Value::NIL);
        let (_, reads) = capture(|| source.cons_car());
        let reads = reads.unwrap();
        for _ in 0..=JOURNAL_SIZE {
            unrelated.set_car(Value::T);
        }
        assert!(!reads.unchanged());
    }

    #[test]
    fn observation_budget_and_unwind_restore_the_outer_scope() {
        let values: Vec<_> = (0..=MAX_READS)
            .map(|_| Value::cons(Value::NIL, Value::NIL))
            .collect();
        let (_, reads) = capture(|| {
            for value in &values {
                value.cons_car();
            }
        });
        assert!(reads.is_none());
        let _ = std::panic::catch_unwind(|| capture(|| panic!("unwind")));
        assert!(!ACTIVE.with(Cell::get));
        let (_, reads) = capture(|| values[0].cons_car());
        assert!(reads.unwrap().unchanged());
    }
}
