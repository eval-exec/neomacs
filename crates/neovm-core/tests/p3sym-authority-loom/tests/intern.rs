//! C5 finite registry/World publication protocols.
//! No engine dependency; real strings, allocation/provenance, GC, accounting and
//! serial mapatoms need independent engine witnesses. Registry-epoch cases are
//! intentionally omitted pending the coordinator's interpretation of its brief.
mod model;
mod support;

use loom::sync::atomic::{AtomicBool, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;
use model::{Authority, Checked};

const SLOTS: usize = 6;
const CHUNK: usize = 2;
const SPELLINGS: usize = 2;
const HEAPS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Name {
    heap: usize,
    object: usize,
}

impl Name {
    fn valid(self) -> bool {
        self.heap < HEAPS && (1..8).contains(&self.object)
    }

    fn bit(self) -> u8 {
        1 << self.object
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    member: bool,
    name: Option<Name>,
    value: usize, // zero is UNBOUND in this finite model, not Lisp nil.
    function: usize,
    descriptor: usize, // zero is the admitted shared VOID, positive is Plain.
}

const EMPTY: Slot = Slot {
    member: false,
    name: None,
    value: 0,
    function: 0,
    descriptor: 0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    slots: [Slot; SLOTS],
    chunks: usize,
    side_capacity: usize,
    members: usize,
    descriptors: usize,
    storage_charge: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Registry {
    canonical: [Option<usize>; SPELLINGS],
    names: [Option<Name>; SLOTS],
    roots: [u8; HEAPS], // exact (heap, object), never string spelling/equality.
    next: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attempt<T> {
    Busy,
    Rejected,
    Poisoned,
    Done(T),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    BeforeMutation,
    AfterRegistry,
    AfterWorld,
}

struct InternWorld {
    authority: Authority,
    registry: Mutex<Registry>,
    state: Checked<State>,
    shared_void: Checked<usize>,
    poisoned: AtomicBool,
}

/// Drop order is deliberate: declare this AFTER the writer lease. An unfinished
/// commit publishes poison before the writer's drop releases the authority word.
struct Commit<'a> {
    world: &'a InternWorld,
    finished: bool,
}

impl Drop for Commit<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.world.poisoned.store(true, Ordering::Release);
        }
    }
}

impl InternWorld {
    fn new(next: usize) -> Self {
        assert!((1..SLOTS).contains(&next));
        // One pre-existing member proves that growth preserves old contents.
        // Its registry name is not one of the two new spellings in this model.
        let old = Name { heap: 1, object: 3 };
        let mut slots = [EMPTY; SLOTS];
        slots[0] = Slot {
            member: true,
            name: Some(old),
            value: 71,
            function: 73,
            descriptor: 0,
        };
        let mut names = [None; SLOTS];
        names[0] = Some(old);
        Self {
            authority: Authority::new(),
            registry: Mutex::new(Registry {
                canonical: [None; SPELLINGS],
                names,
                roots: [0, old.bit()],
                next,
            }),
            state: Checked::new(State {
                slots,
                chunks: 1,
                side_capacity: CHUNK,
                members: 1,
                descriptors: 0,
                storage_charge: 0,
            }),
            shared_void: Checked::new(0),
            poisoned: AtomicBool::new(false),
        }
    }

    fn intern(&self, spelling: usize, input: Name) -> Attempt<usize> {
        self.commit_intern(spelling, input, Fault::None, false, false)
    }

    /// Every path takes the World writer before the registry guard. Existing
    /// registry identity and World membership are separate tests: another World
    /// may already have allocated the canonical ID, leaving a gap here.
    fn commit_intern(
        &self,
        spelling: usize,
        input: Name,
        fault: Fault,
        obsolete_absence: bool,
        wrong_root_heap: bool,
    ) -> Attempt<usize> {
        if spelling >= SPELLINGS || !input.valid() {
            return Attempt::Rejected;
        }
        let _writer = match self.authority.write() {
            Some(lease) => lease,
            None => return Attempt::Busy,
        };
        if self.poisoned.load(Ordering::Acquire) {
            return Attempt::Poisoned;
        }
        let mut registry = self.registry.lock().unwrap();
        let existing = if obsolete_absence {
            None // Negative control: blindly trust an absence read before admission.
        } else {
            registry.canonical[spelling]
        };
        let id = existing.unwrap_or(registry.next);
        if id >= SLOTS || fault == Fault::BeforeMutation {
            return Attempt::Rejected;
        }
        let mut state = self.state.get();
        if existing.is_some() && state.slots[id].member {
            return Attempt::Done(id);
        }
        // Fixed model arrays stand for pre-reserved Rust storage. All capacity
        // arithmetic and fault-free rejection happen before visible mutation.
        let required_chunks = (id + 1).div_ceil(CHUNK);
        let required_side = required_chunks * CHUNK;
        let new_chunks = state.chunks.max(required_chunks);
        let new_side = state.side_capacity.max(required_side);
        let mut unfinished = Commit {
            world: self,
            finished: false,
        };
        if existing.is_none() {
            registry.next += 1;
            registry.names[id] = Some(input);
            let root_heap = if wrong_root_heap {
                1 - input.heap
            } else {
                input.heap
            };
            registry.roots[root_heap] |= input.bit();
            registry.canonical[spelling] = Some(id);
        }
        if fault == Fault::AfterRegistry {
            return Attempt::Poisoned;
        }
        let exact = match registry.names[id] {
            Some(name) => name,
            None => return Attempt::Poisoned,
        };
        state.storage_charge +=
            (new_chunks - state.chunks) * CHUNK + (new_side - state.side_capacity);
        state.chunks = new_chunks;
        state.side_capacity = new_side;
        // Initialize arms/projection before membership becomes observable. The
        // existing publication.rs separately checks name Release ordering with
        // an independent presence observer and its required negative witness.
        state.slots[id] = Slot {
            member: true,
            name: Some(exact),
            value: 0,
            function: 0,
            descriptor: 0,
        };
        state.members += 1;
        self.state.set(state);
        if fault == Fault::AfterWorld {
            return Attempt::Poisoned;
        }
        unfinished.finished = true;
        Attempt::Done(id)
    }

    /// Busy differs from a real soft miss. Neither registry-only identity nor
    /// available storage capacity is evidence of initial-obarray membership.
    fn soft(&self, spelling: usize) -> Attempt<Option<usize>> {
        if spelling >= SPELLINGS {
            return Attempt::Rejected;
        }
        let _reader = match self.authority.read() {
            Some(lease) => lease,
            None => return Attempt::Busy,
        };
        if self.poisoned.load(Ordering::Acquire) {
            return Attempt::Poisoned;
        }
        let registry = self.registry.lock().unwrap();
        let state = self.state.get();
        let Some(id) = registry.canonical[spelling] else {
            return Attempt::Done(None);
        };
        let slot = state.slots[id];
        if !slot.member {
            return Attempt::Done(None);
        }
        assert_eq!(slot.name, registry.names[id]);
        assert!(id < state.chunks * CHUNK && id < state.side_capacity);
        Attempt::Done(Some(id))
    }

    fn set_plain(&self, id: usize, value: usize, duplicate_issue: bool) -> Attempt<()> {
        let _writer = match self.authority.write() {
            Some(lease) => lease,
            None => return Attempt::Busy,
        };
        if self.poisoned.load(Ordering::Acquire) {
            return Attempt::Poisoned;
        }
        let mut state = self.state.get();
        if id >= SLOTS || !state.slots[id].member {
            return Attempt::Rejected;
        }
        if state.slots[id].descriptor == 0 || duplicate_issue {
            state.descriptors += 1;
            state.slots[id].descriptor = state.descriptors;
        }
        state.slots[id].value = value;
        self.state.set(state);
        Attempt::Done(())
    }

    fn stopped_state(&self) -> State {
        self.authority.assert_quiescent();
        assert!(!self.poisoned.load(Ordering::Acquire));
        self.state.get()
    }

    fn stopped_roots(&self, heap: usize) -> Attempt<u8> {
        self.authority.assert_quiescent();
        if self.poisoned.load(Ordering::Acquire) {
            return Attempt::Poisoned;
        }
        Attempt::Done(self.registry.lock().unwrap().roots[heap])
    }

    fn audit_counts(&self) {
        let state = self.stopped_state();
        assert_eq!(
            state.members,
            state.slots.iter().filter(|slot| slot.member).count()
        );
        assert_eq!(state.slots[0].value, 71);
        assert_eq!(state.slots[0].function, 73);
    }
}

/// Each model actor makes at most two attempts. Busy is legal and does not
/// masquerade as an intern-soft miss or a completed operation. No fairness or
/// lock-acquisition unwrap is used, including in the post-join serial suffix.
fn bounded<T>(mut operation: impl FnMut() -> Attempt<T>) -> Attempt<T> {
    for _ in 0..2 {
        let answer = operation();
        if !matches!(answer, Attempt::Busy) {
            return answer;
        }
        thread::yield_now();
    }
    Attempt::Busy
}

fn completed<T>(answer: Attempt<T>) -> T {
    match answer {
        Attempt::Done(answer) => answer,
        Attempt::Busy => panic!("joined serial completion unexpectedly refused authority"),
        Attempt::Rejected => panic!("model fixture rejected its valid input"),
        Attempt::Poisoned => panic!("model fixture unexpectedly poisoned"),
    }
}

fn finish_intern(
    world: &InternWorld,
    spelling: usize,
    input: Name,
    answer: Attempt<usize>,
) -> usize {
    // Call only after all actors joined; a new serial suffix is explicit.
    completed(match answer {
        Attempt::Busy => world.intern(spelling, input),
        answer => answer,
    })
}

const FIRST: Name = Name { heap: 0, object: 1 };
const SECOND: Name = Name { heap: 0, object: 2 };

#[test]
fn same_name_exact_object_winner_is_rooted_once() {
    support::check(2, || {
        let world = Arc::new(InternWorld::new(1));
        let w = Arc::clone(&world);
        let other = thread::spawn(move || bounded(|| w.intern(0, SECOND)));
        let first = bounded(|| world.intern(0, FIRST));
        let soft = world.soft(0);
        let other = other.join().unwrap();
        let one = finish_intern(&world, 0, FIRST, first);
        let two = finish_intern(&world, 0, SECOND, other);
        assert_eq!(one, two);
        if let Attempt::Done(Some(observed)) = soft {
            assert_eq!(observed, one);
        }
        let winner = world.stopped_state().slots[one].name.unwrap();
        assert!(winner == FIRST || winner == SECOND);
        assert_eq!(world.stopped_roots(0), Attempt::Done(winner.bit()));
        assert_eq!(completed(world.intern(0, Name { heap: 0, object: 4 })), one);
        assert_eq!(world.stopped_state().slots[one].name, Some(winner));
        let registry = world.registry.lock().unwrap();
        assert_eq!(registry.next, 2);
        assert_eq!(world.stopped_state().members, 2);
        world.audit_counts();
    });
}

#[test]
fn different_names_grow_chunks_and_side_tables_without_filling_gaps() {
    support::check(2, || {
        let world = Arc::new(InternWorld::new(3));
        let w = Arc::clone(&world);
        let other = thread::spawn(move || bounded(|| w.intern(1, SECOND)));
        let first = bounded(|| world.intern(0, FIRST));
        let other = other.join().unwrap();
        let one = finish_intern(&world, 0, FIRST, first);
        let two = finish_intern(&world, 1, SECOND, other);
        assert!(one != two && [3, 4].contains(&one) && [3, 4].contains(&two));
        let state = world.stopped_state();
        assert_eq!(state.chunks, 3);
        assert_eq!(state.side_capacity, 6);
        assert_eq!(state.storage_charge, 8);
        assert_eq!(state.members, 3);
        assert_eq!(state.slots[1], EMPTY);
        assert_eq!(state.slots[2], EMPTY);
        for id in [one, two] {
            assert_eq!(state.slots[id].value, 0);
            assert_eq!(state.slots[id].function, 0);
            assert_eq!(state.slots[id].descriptor, 0);
        }
        assert_eq!(world.soft(0), Attempt::Done(Some(one)));
        assert_eq!(world.soft(1), Attempt::Done(Some(two)));
        world.audit_counts();
    });
}

#[test]
fn registry_hit_world_miss_initializes_membership_once() {
    support::check(2, || {
        let world = Arc::new(InternWorld::new(5));
        // Fixture seeding precedes actor publication. This models an existing
        // canonical ID, not concurrent access to a second running World.
        {
            let mut registry = world.registry.lock().unwrap();
            registry.canonical[0] = Some(3);
            registry.names[3] = Some(FIRST);
            registry.roots[0] = FIRST.bit();
        }
        let w = Arc::clone(&world);
        let other = thread::spawn(move || bounded(|| w.intern(0, SECOND)));
        let first = bounded(|| world.intern(0, FIRST));
        let other = other.join().unwrap();
        assert_eq!(finish_intern(&world, 0, FIRST, first), 3);
        assert_eq!(finish_intern(&world, 0, SECOND, other), 3);
        assert_eq!(world.registry.lock().unwrap().next, 5);
        assert_eq!(world.stopped_state().slots[3].name, Some(FIRST));
        assert_eq!(world.stopped_state().members, 2);
        assert_eq!(world.stopped_roots(0), Attempt::Done(FIRST.bit()));
        world.audit_counts();
    });
}

#[test]
fn soft_missing_mapping_or_world_membership_changes_nothing() {
    support::check(2, || {
        let world = Arc::new(InternWorld::new(5));
        {
            let mut registry = world.registry.lock().unwrap();
            registry.canonical[0] = Some(4);
            registry.names[4] = Some(FIRST);
            registry.roots[0] = FIRST.bit();
        }
        let before_state = world.stopped_state();
        let before_registry = *world.registry.lock().unwrap();
        let w = Arc::clone(&world);
        let other = thread::spawn(move || bounded(|| w.soft(0)));
        let first = bounded(|| world.soft(1));
        let other = other.join().unwrap();
        for (spelling, answer) in [(0, other), (1, first)] {
            assert_eq!(
                completed(match answer {
                    Attempt::Busy => world.soft(spelling),
                    answer => answer,
                }),
                None
            );
        }
        assert_eq!(world.stopped_state(), before_state);
        assert_eq!(*world.registry.lock().unwrap(), before_registry);
        assert!(!world.poisoned.load(Ordering::Acquire));
    });
}

#[test]
fn stopped_name_roots_match_heap_and_object_identity() {
    support::check(2, || {
        let world = Arc::new(InternWorld::new(1));
        let w = Arc::clone(&world);
        let other = thread::spawn(move || bounded(|| w.intern(0, SECOND)));
        let first = bounded(|| world.intern(0, FIRST));
        let other = other.join().unwrap();
        let id = finish_intern(&world, 0, FIRST, first);
        assert_eq!(finish_intern(&world, 0, SECOND, other), id);
        // No owner input roots remain at this stopped boundary; only the exact
        // registry root can mark the selected name. Foreign roots stay foreign.
        let winner = world.stopped_state().slots[id].name.unwrap();
        let current_marks = completed(world.stopped_roots(0));
        let foreign_marks = completed(world.stopped_roots(1));
        assert_eq!(current_marks, winner.bit());
        assert_eq!(foreign_marks, 1 << 3);
        assert_eq!(current_marks & (FIRST.bit() | SECOND.bit()), winner.bit());
        assert_eq!(current_marks & (1 << 3), 0);
        world.audit_counts();
    });
}

#[test]
fn new_symbol_starts_void_and_first_assignment_issues_once() {
    support::check(2, || {
        let world = Arc::new(InternWorld::new(1));
        let id = completed(world.intern(0, FIRST));
        let initial = world.stopped_state();
        assert_eq!(initial.slots[id].descriptor, 0);
        assert_eq!(initial.slots[id].value, 0);
        assert_eq!(initial.descriptors, 0);
        let w = Arc::clone(&world);
        let other = thread::spawn(move || bounded(|| w.set_plain(id, 17, false)));
        let first = bounded(|| world.set_plain(id, 19, false));
        let other = other.join().unwrap();
        completed(match first {
            Attempt::Busy => world.set_plain(id, 19, false),
            answer => answer,
        });
        completed(match other {
            Attempt::Busy => world.set_plain(id, 17, false),
            answer => answer,
        });
        let state = world.stopped_state();
        assert_eq!(state.descriptors, 1);
        assert_eq!(state.slots[id].descriptor, 1);
        assert!(matches!(state.slots[id].value, 17 | 19));
        assert_eq!(world.shared_void.get(), 0);
        assert_eq!(state.slots[0].descriptor, 0);
        world.audit_counts();
    });
}

#[test]
fn reservation_failure_is_clean_partial_commit_is_poisoned() {
    support::check(1, || {
        // These are independent finite fault stages, not a partial commit
        // followed by an attempted rollback or permissive normal access.
        for fault in [
            Fault::BeforeMutation,
            Fault::AfterRegistry,
            Fault::AfterWorld,
        ] {
            let world = InternWorld::new(1);
            let before_state = world.stopped_state();
            let before_registry = *world.registry.lock().unwrap();
            let answer = world.commit_intern(0, FIRST, fault, false, false);
            world.authority.assert_quiescent();
            if fault == Fault::BeforeMutation {
                assert_eq!(answer, Attempt::Rejected);
                assert_eq!(world.stopped_state(), before_state);
                assert_eq!(*world.registry.lock().unwrap(), before_registry);
                assert_eq!(world.soft(0), Attempt::Done(None));
            } else {
                assert_eq!(answer, Attempt::Poisoned);
                assert!(world.poisoned.load(Ordering::Acquire));
                assert_eq!(world.soft(0), Attempt::Poisoned);
                assert_eq!(world.intern(1, SECOND), Attempt::Poisoned);
                assert_eq!(world.stopped_roots(0), Attempt::Poisoned);
            }
        }
    });
}

#[test]
fn obsolete_absence_without_registry_recheck_has_witness() {
    support::negative(2, "intern reused an obsolete absence decision", || {
        let world = Arc::new(InternWorld::new(1));
        // Both probes occurred before either actor acquired writer admission.
        // The broken commit is still writer-exclusive: the missing canonical
        // recheck, rather than an ordinary-cell data race, causes duplication.
        assert_eq!(world.registry.lock().unwrap().canonical[0], None);
        let w = Arc::clone(&world);
        let other =
            thread::spawn(move || bounded(|| w.commit_intern(0, SECOND, Fault::None, true, false)));
        let first = bounded(|| world.commit_intern(0, FIRST, Fault::None, true, false));
        let other = other.join().unwrap();
        let one = completed(match first {
            Attempt::Busy => world.commit_intern(0, FIRST, Fault::None, true, false),
            answer => answer,
        });
        let two = completed(match other {
            Attempt::Busy => world.commit_intern(0, SECOND, Fault::None, true, false),
            answer => answer,
        });
        assert!(one == two, "intern reused an obsolete absence decision");
    });
}

#[test]
fn wrong_heap_name_root_has_witness() {
    support::negative(
        1,
        "intern name root differs from winning heap object",
        || {
            let world = InternWorld::new(1);
            let id = completed(world.commit_intern(0, FIRST, Fault::None, false, true));
            let winner = world.stopped_state().slots[id].name.unwrap();
            let current = completed(world.stopped_roots(winner.heap));
            assert!(
                current == winner.bit(),
                "intern name root differs from winning heap object"
            );
        },
    );
}

#[test]
fn duplicate_plain_descriptor_issue_has_witness() {
    support::negative(
        1,
        "projected symbol issued more than one Plain descriptor",
        || {
            let world = InternWorld::new(1);
            let id = completed(world.intern(0, FIRST));
            completed(world.set_plain(id, 17, true));
            completed(world.set_plain(id, 19, true));
            assert!(
                world.stopped_state().descriptors == 1,
                "projected symbol issued more than one Plain descriptor"
            );
        },
    );
}
