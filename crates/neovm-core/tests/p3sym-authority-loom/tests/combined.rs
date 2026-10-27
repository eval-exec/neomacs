//! C6 specification models: mixed root transfers and repeated stop censuses.
//! Object bits and generations are finite specifications, not an allocator or
//! evidence that the engine ran a minor. Scripted transfer phases isolate root
//! obligations; the separate ACK model explores publication without that script.
mod model;
#[path = "combined/request_snapshot.rs"]
mod request_snapshot;
mod support;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::thread;
use model::{Authority, Checked, Latch};

/// A finite model object set, never an owner index or stop sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
struct Objects(usize);

impl Objects {
    const EMPTY: Self = Self(0);
}

impl std::ops::BitOr for Objects {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl std::ops::BitOrAssign for Objects {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}
impl std::ops::BitAnd for Objects {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
impl std::ops::BitAndAssign for Objects {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}
impl std::ops::Not for Objects {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

const OLD_FUNCTION: Objects = Objects(1);
const NAME: Objects = Objects(2);
const NEW_FUNCTION: Objects = Objects(4);

#[repr(transparent)]
struct RootWord(AtomicUsize);
impl RootWord {
    fn empty() -> Self {
        Self(AtomicUsize::new(0))
    }
    fn load(&self, order: Ordering) -> Objects {
        Objects(self.0.load(order))
    }
    fn store(&self, roots: Objects, order: Ordering) {
        self.0.store(roots.0, order);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct StopSequence(usize);
impl StopSequence {
    const UNREQUESTED: Self = Self(0);
    const FIRST: Self = Self(1);
    const SECOND: Self = Self(2);
}

#[repr(transparent)]
struct StopWord(AtomicUsize);
impl StopWord {
    fn unrequested() -> Self {
        Self(AtomicUsize::new(StopSequence::UNREQUESTED.0))
    }
    fn load(&self, order: Ordering) -> StopSequence {
        StopSequence(self.0.load(order))
    }
    fn store(&self, stop: StopSequence, order: Ordering) {
        self.0.store(stop.0, order);
    }
}

static_assertions::assert_eq_size!(Objects, usize);
static_assertions::assert_eq_size!(StopSequence, usize);
static_assertions::assert_eq_size!(RootWord, AtomicUsize);
static_assertions::assert_eq_size!(StopWord, AtomicUsize);

#[derive(Clone, Copy)]
enum Owner {
    A,
    B,
}
impl Owner {
    const ALL: [Self; 2] = [Self::A, Self::B];
    fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

#[derive(Clone, Copy)]
enum Backing {
    Plain,
    ProjectedPlain,
}

#[must_use = "handle a refused authority attempt without assuming progress"]
#[derive(Clone, Copy)]
enum Attempt {
    Committed,
    Busy,
}

#[derive(Clone, Copy)]
enum WordGate {
    Checked,
    Bypassed,
}

#[derive(Clone, Copy)]
enum AckOrder {
    RootsBeforeAck,
    AckBeforeRoots,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootAudit {
    Complete,
    Omitted,
}

fn reachable(roots: Objects) -> Objects {
    // F0 returns this exact immutable name object; F1 returns an immediate.
    roots
        | if roots & OLD_FUNCTION != Objects::EMPTY {
            NAME
        } else {
            Objects::EMPTY
        }
}

#[derive(Clone, Copy, Default)]
struct Cells {
    global: Objects,
    projection: Objects,
    function: Objects,
    journal: Objects,
    names: Objects,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    NoGlobalReseed,
    NoProjection,
    NoFunction,
    NoActivation,
    NoName,
}

impl Fault {
    fn witness(self) -> &'static str {
        match self {
            Self::None => "C6 combined live roots were lost",
            Self::NoGlobalReseed => "C6 second stop omitted live global reseeding",
            Self::NoProjection => "C6 stop omitted canonical projection backing",
            Self::NoFunction => "C6 stop omitted the live function-cell root",
            Self::NoActivation => "C6 replacement lost the active old definition",
            Self::NoName => "C6 stop omitted the exact registry name root",
        }
    }
}

struct World {
    authority: Authority,
    cells: Checked<Cells>,
    backing: Backing,
    homes: [RootWord; 2],
    activations: [RootWord; 2],
    published_homes: [RootWord; 2],
    published_activations: [RootWord; 2],
    requested: StopWord,
    ack: [StopWord; 2],
}

impl World {
    fn new(backing: Backing) -> Self {
        Self {
            authority: Authority::new(),
            cells: Checked::new(Cells::default()),
            backing,
            homes: std::array::from_fn(|_| RootWord::empty()),
            activations: std::array::from_fn(|_| RootWord::empty()),
            published_homes: std::array::from_fn(|_| RootWord::empty()),
            published_activations: std::array::from_fn(|_| RootWord::empty()),
            requested: StopWord::unrequested(),
            ack: std::array::from_fn(|_| StopWord::unrequested()),
        }
    }

    fn global(&self, cells: Cells) -> Objects {
        match self.backing {
            Backing::Plain => cells.global,
            Backing::ProjectedPlain => cells.projection,
        }
    }

    fn set_global(&self, cells: &mut Cells, value: Objects) {
        match self.backing {
            Backing::Plain => cells.global = value,
            Backing::ProjectedPlain => cells.projection = value,
        }
    }

    fn write(&self, body: impl FnOnce(&mut Cells)) -> Attempt {
        let Some(_writer) = self.authority.write() else {
            return Attempt::Busy;
        };
        let mut cells = self.cells.get();
        body(&mut cells);
        self.cells.set(cells);
        Attempt::Committed
    }

    fn global_into_home(&self, owner: Owner) -> Attempt {
        let Some(_reader) = self.authority.read() else {
            return Attempt::Busy;
        };
        self.homes[owner.index()].store(self.global(self.cells.get()), Ordering::Relaxed);
        // The actual answer home exists before releasing the reader lease.
        Attempt::Committed
    }

    fn begin_call(&self, owner: Owner) -> Attempt {
        let Some(_reader) = self.authority.read() else {
            return Attempt::Busy;
        };
        self.activations[owner.index()].store(self.cells.get().function, Ordering::Relaxed);
        Attempt::Committed
    }

    fn publish_roots(&self, owner: Owner) {
        // Atomic scalar catalog words model a frozen root image, not atomic
        // symbol payloads. ACK, not a new root mutex, publishes their contents.
        self.published_homes[owner.index()].store(
            self.homes[owner.index()].load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.published_activations[owner.index()].store(
            self.activations[owner.index()].load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    fn acknowledge(&self, owner: Owner, epoch: StopSequence) {
        assert_eq!(self.requested.load(Ordering::Acquire), epoch);
        self.authority.assert_quiescent();
        self.publish_roots(owner);
        self.ack[owner.index()].store(epoch, Ordering::Release);
    }

    fn trace(&self, epoch: StopSequence, fault: Fault) -> Option<Objects> {
        self.trace_with_word_gate(epoch, fault, WordGate::Checked)
    }

    fn trace_with_word_gate(
        &self,
        epoch: StopSequence,
        fault: Fault,
        gate_word: WordGate,
    ) -> Option<Objects> {
        if self.requested.load(Ordering::Acquire) != epoch
            || self
                .ack
                .iter()
                .any(|ack| ack.load(Ordering::Acquire) != epoch)
        {
            return None;
        }
        if matches!(gate_word, WordGate::Checked)
            && self.authority.word.load(Ordering::Acquire) != 0
        {
            return None;
        }
        // Independent invariant catches the broken admission gate before any
        // ordinary storage read can produce an unrelated causality witness.
        assert!(
            self.authority.word.load(Ordering::Acquire) == 0,
            "C6 keeper traced while a symbol lease was held"
        );
        let cells = self.cells.get();
        let global = if fault == Fault::NoGlobalReseed && epoch >= StopSequence::SECOND {
            Objects::EMPTY
        } else if matches!(self.backing, Backing::ProjectedPlain) && fault == Fault::NoProjection {
            Objects::EMPTY
        } else {
            self.global(cells)
        };
        let function = match fault {
            Fault::NoFunction => Objects::EMPTY,
            Fault::None
            | Fault::NoGlobalReseed
            | Fault::NoProjection
            | Fault::NoActivation
            | Fault::NoName => cells.function,
        };
        let mut roots = global | function | cells.journal;
        if fault != Fault::NoName {
            roots |= cells.names;
        }
        for owner in Owner::ALL {
            roots |= self.published_homes[owner.index()].load(Ordering::Relaxed);
            if fault != Fault::NoActivation {
                roots |= self.published_activations[owner.index()].load(Ordering::Relaxed);
            }
        }
        Some(reachable(roots))
    }

    fn reconcile(&self) {
        self.authority.assert_quiescent();
        let mut cells = self.cells.get();
        // Suspended serial caches have been evicted; no journal may mask an
        // omitted active call root in the subsequent collector census.
        cells.journal = Objects::EMPTY;
        self.cells.set(cells);
    }
}

fn completed(done: Attempt) {
    // All scripted actions are separated by stopped/joined handoffs. A Busy
    // result is handled explicitly; no refused try-acquisition is unwrapped.
    match done {
        Attempt::Committed => {}
        Attempt::Busy => panic!("C6 scripted uncontended action refused authority"),
    }
}

#[derive(Clone, Copy)]
enum CollectionMode {
    Gen0Full,
    Gen1Reseed,
}

#[derive(Clone, Copy)]
enum CollectionKind {
    Major,
    Minor,
}

#[derive(Clone, Copy)]
enum Phase {
    GlobalStored,
    GlobalReseeded,
    FunctionStored,
    ActiveOldDefinition,
    NamePublished,
    FunctionCleared,
}
impl Phase {
    const ALL: [Self; 6] = [
        Self::GlobalStored,
        Self::GlobalReseeded,
        Self::FunctionStored,
        Self::ActiveOldDefinition,
        Self::NamePublished,
        Self::FunctionCleared,
    ];
    fn index(self) -> usize {
        match self {
            Self::GlobalStored => 0,
            Self::GlobalReseeded => 1,
            Self::FunctionStored => 2,
            Self::ActiveOldDefinition => 3,
            Self::NamePublished => 4,
            Self::FunctionCleared => 5,
        }
    }
    fn stop(self) -> StopSequence {
        StopSequence(self.index() + 1)
    }
    fn expected(self) -> Objects {
        match self {
            Self::GlobalStored | Self::GlobalReseeded | Self::FunctionStored => OLD_FUNCTION | NAME,
            Self::ActiveOldDefinition => OLD_FUNCTION | NAME | NEW_FUNCTION,
            Self::NamePublished => NAME | NEW_FUNCTION,
            Self::FunctionCleared => NAME,
        }
    }
    fn collection(self, mode: CollectionMode) -> CollectionKind {
        match mode {
            CollectionMode::Gen0Full => CollectionKind::Major,
            CollectionMode::Gen1Reseed => match self {
                Self::GlobalStored
                | Self::GlobalReseeded
                | Self::FunctionStored
                | Self::ActiveOldDefinition => CollectionKind::Minor,
                Self::NamePublished | Self::FunctionCleared => CollectionKind::Major,
            },
        }
    }
}

/// Finite generation bookkeeping only. Every call starts with a NEW mark set.
/// Real minor collection, remsets and image restoration need engine evidence.
fn sweep(alive: &mut Objects, young: &mut Objects, marks: Objects, kind: CollectionKind) {
    let eligible = match kind {
        CollectionKind::Major => *alive,
        CollectionKind::Minor => *young,
    };
    *alive &= !eligible | marks;
    *young &= *alive & !marks; // surviving modeled young objects promote.
}

fn transfers(backing: Backing, mode: CollectionMode, fault: Fault) {
    let world = Arc::new(World::new(backing));
    let starts = Arc::new(std::array::from_fn::<_, 6, _>(|_| Latch::default()));
    let done = Arc::new(std::array::from_fn::<_, 6, _>(|_| Latch::default()));
    let (other, worker_starts, worker_done) =
        (Arc::clone(&world), Arc::clone(&starts), Arc::clone(&done));
    let owner_a = thread::spawn(move || {
        for phase in Phase::ALL {
            worker_starts[phase.index()].wait();
            match phase {
                Phase::GlobalStored => {
                    other.homes[Owner::A.index()].store(OLD_FUNCTION, Ordering::Relaxed);
                    completed(other.write(|cells| other.set_global(cells, OLD_FUNCTION)));
                    other.homes[Owner::A.index()].store(Objects::EMPTY, Ordering::Relaxed);
                }
                Phase::ActiveOldDefinition => completed(other.begin_call(Owner::A)),
                Phase::NamePublished => {
                    assert_eq!(
                        other.activations[Owner::A.index()].load(Ordering::Relaxed),
                        OLD_FUNCTION
                    );
                    other.homes[Owner::A.index()].store(NAME, Ordering::Relaxed); // bounded old call returns.
                    other.activations[Owner::A.index()].store(Objects::EMPTY, Ordering::Relaxed);
                    completed(other.write(|cells| other.set_global(cells, NAME)));
                    other.homes[Owner::A.index()].store(Objects::EMPTY, Ordering::Relaxed);
                }
                Phase::GlobalReseeded | Phase::FunctionStored | Phase::FunctionCleared => {}
            }
            other.acknowledge(Owner::A, phase.stop());
            worker_done[phase.index()].set();
            // Wait for the next keeper resume/request; never re-enter merely
            // because ACK has been published. Latches are script edges only.
        }
    });
    let mut alive = OLD_FUNCTION | NAME;
    let mut young = alive;
    let mut root_audit = RootAudit::Complete;
    for phase in Phase::ALL {
        let epoch = phase.stop();
        world.requested.store(epoch, Ordering::Release);
        starts[phase.index()].set();
        done[phase.index()].wait();
        match phase {
            Phase::FunctionStored => {
                completed(world.global_into_home(Owner::B));
                let received = world.homes[Owner::B.index()].load(Ordering::Relaxed);
                assert_eq!(received, OLD_FUNCTION);
                completed(world.write(|cells| {
                    cells.function = received;
                    world.set_global(cells, Objects::EMPTY);
                }));
                world.homes[Owner::B.index()].store(Objects::EMPTY, Ordering::Relaxed);
            }
            Phase::ActiveOldDefinition => {
                alive |= NEW_FUNCTION;
                young |= NEW_FUNCTION;
                completed(world.write(|cells| {
                    cells.journal |= cells.function;
                    cells.function = NEW_FUNCTION;
                }));
            }
            Phase::NamePublished => {
                completed(world.global_into_home(Owner::B));
                let received = world.homes[Owner::B.index()].load(Ordering::Relaxed);
                assert_eq!(received, NAME);
                completed(world.write(|cells| {
                    cells.names |= received; // exact winning name index.
                    world.set_global(cells, Objects::EMPTY);
                }));
                world.homes[Owner::B.index()].store(Objects::EMPTY, Ordering::Relaxed);
            }
            Phase::FunctionCleared => completed(world.write(|cells| {
                cells.journal |= cells.function;
                cells.function = Objects::EMPTY;
            })),
            Phase::GlobalStored | Phase::GlobalReseeded => {}
        }
        world.acknowledge(Owner::B, epoch);
        world.reconcile();
        let observed = world.trace(epoch, fault).expect("matching combined stop");
        let expected = phase.expected();
        if observed != expected {
            root_audit = RootAudit::Omitted;
        }
        // Fault controls omit one family from an AUDIT census only. Complete
        // every resume/handoff with the real root set, then report the named
        // witness after joining the worker; no failed check strands a phase.
        let marks = world
            .trace(epoch, Fault::None)
            .expect("complete root census");
        sweep(&mut alive, &mut young, marks, phase.collection(mode));
        assert_eq!(
            alive, expected,
            "C6 reachable objects differ after stopped sweep"
        );
        if matches!(phase, Phase::NamePublished | Phase::FunctionCleared) {
            assert_eq!(
                alive & OLD_FUNCTION,
                Objects::EMPTY,
                "completed old activation must retire"
            );
        }
        // Resetting marks on the second stop defeats allocation/birth retention
        // as a substitute for canonical global/projection reseeding.
    }
    owner_a.join().unwrap();
    world.authority.assert_quiescent();
    assert!(root_audit == RootAudit::Complete, "{}", fault.witness());
}

#[test]
fn gen0_plain_global_function_activation_name_transfer() {
    support::check(2, || {
        transfers(Backing::Plain, CollectionMode::Gen0Full, Fault::None)
    });
}

#[test]
fn gen0_projected_global_function_activation_name_transfer() {
    support::check(2, || {
        transfers(
            Backing::ProjectedPlain,
            CollectionMode::Gen0Full,
            Fault::None,
        )
    });
}

#[test]
fn gen1_plain_reseed_and_later_major_retire_old_definition() {
    support::check(2, || {
        transfers(Backing::Plain, CollectionMode::Gen1Reseed, Fault::None)
    });
}

#[test]
fn gen1_projected_reseed_and_later_major_retire_old_definition() {
    support::check(2, || {
        transfers(
            Backing::ProjectedPlain,
            CollectionMode::Gen1Reseed,
            Fault::None,
        )
    });
}

#[derive(Clone, Copy)]
enum ReplacementPhase {
    Original,
    Replaced,
    Retired,
}
impl ReplacementPhase {
    const ALL: [Self; 3] = [Self::Original, Self::Replaced, Self::Retired];
    fn stop(self) -> StopSequence {
        match self {
            Self::Original => StopSequence::FIRST,
            Self::Replaced => StopSequence::SECOND,
            Self::Retired => StopSequence(3),
        }
    }
    fn collection(self) -> CollectionKind {
        match self {
            Self::Original | Self::Replaced => CollectionKind::Minor,
            Self::Retired => CollectionKind::Major,
        }
    }
}

fn old_global_young_replacement(backing: Backing) {
    let world = World::new(backing);
    completed(world.write(|cells| world.set_global(cells, OLD_FUNCTION)));
    let mut alive = OLD_FUNCTION | NAME;
    let mut young = Objects::EMPTY; // original binding has already promoted.
    for phase in ReplacementPhase::ALL {
        let epoch = phase.stop();
        world.requested.store(epoch, Ordering::Release);
        if matches!(phase, ReplacementPhase::Replaced) {
            alive |= NEW_FUNCTION;
            young |= NEW_FUNCTION;
            completed(world.write(|cells| world.set_global(cells, NEW_FUNCTION)));
        }
        world.acknowledge(Owner::A, epoch);
        world.acknowledge(Owner::B, epoch);
        let marks = world
            .trace(epoch, Fault::None)
            .expect("fresh stopped census");
        let (expected, expected_young) = match phase {
            ReplacementPhase::Original => (OLD_FUNCTION | NAME, Objects::EMPTY),
            ReplacementPhase::Replaced | ReplacementPhase::Retired => (NEW_FUNCTION, NEW_FUNCTION),
        };
        assert_eq!(
            marks, expected,
            "old global must reseed its current young replacement"
        );
        sweep(&mut alive, &mut young, marks, phase.collection());
        assert_eq!(alive & NEW_FUNCTION, expected_young);
    }
    assert_eq!(
        alive, NEW_FUNCTION,
        "later full job reclaims unreachable old objects"
    );
}

#[test]
fn gen1_old_plain_global_reseeds_young_replacement_at_second_stop() {
    support::check(1, || old_global_young_replacement(Backing::Plain));
}

#[test]
fn gen1_old_projected_global_reseeds_young_replacement_at_second_stop() {
    support::check(1, || old_global_young_replacement(Backing::ProjectedPlain));
}

fn root_ack_history(order: AckOrder) {
    let world = Arc::new(World::new(Backing::Plain));
    completed(world.write(|cells| cells.global = OLD_FUNCTION));
    world
        .requested
        .store(StopSequence::FIRST, Ordering::Release);
    world.acknowledge(Owner::B, StopSequence::FIRST);
    let other = Arc::clone(&world);
    let actor = thread::spawn(move || {
        // One attempt can legally refuse; the completed suffix below handles
        // that branch. No actor owns a lease after returning or acknowledging.
        let Some(writer) = other.authority.write() else {
            return Attempt::Busy;
        };
        let mut cells = other.cells.get();
        other.homes[Owner::A.index()].store(cells.global, Ordering::Relaxed);
        cells.global = Objects::EMPTY;
        other.cells.set(cells);
        drop(writer);
        match order {
            AckOrder::AckBeforeRoots => {
                other.ack[Owner::A.index()].store(StopSequence::FIRST, Ordering::Release);
                thread::yield_now();
                other.publish_roots(Owner::A);
            }
            AckOrder::RootsBeforeAck => other.acknowledge(Owner::A, StopSequence::FIRST),
        }
        Attempt::Committed
    });
    let observed = world.trace(StopSequence::FIRST, Fault::None);
    let completed = actor.join().unwrap();
    if let Some(marks) = observed {
        assert!(
            marks == OLD_FUNCTION | NAME,
            "C6 ACK preceded publication of its owner root image"
        );
    }
    match completed {
        Attempt::Committed => {}
        Attempt::Busy => {
            // Keeper observation never acquires authority, so refusal cannot occur
            // in this particular schedule; keep the branch legal and explicit.
            world.acknowledge(Owner::A, StopSequence::FIRST);
        }
    }
    assert_eq!(
        world.trace(StopSequence::FIRST, Fault::None),
        Some(OLD_FUNCTION | NAME)
    );
    world.authority.assert_quiescent();
}

#[test]
fn root_publication_and_lease_release_happen_before_ack() {
    support::check(2, || root_ack_history(AckOrder::RootsBeforeAck));
}

#[test]
fn held_lease_refuses_even_a_complete_ack_census() {
    support::check(1, || {
        let world = World::new(Backing::Plain);
        world
            .requested
            .store(StopSequence::FIRST, Ordering::Release);
        if let Some(lease) = world.authority.read() {
            // Adversarial ACKs: the keeper must independently check the word.
            for ack in &world.ack {
                ack.store(StopSequence::FIRST, Ordering::Release);
            }
            assert_eq!(world.trace(StopSequence::FIRST, Fault::None), None);
            drop(lease);
        }
        world.acknowledge(Owner::A, StopSequence::FIRST);
        world.acknowledge(Owner::B, StopSequence::FIRST);
        assert_eq!(
            world.trace(StopSequence::FIRST, Fault::None),
            Some(Objects::EMPTY)
        );
    });
}

#[test]
fn previous_stop_acknowledgements_do_not_authorize_the_next_census() {
    support::check(1, || {
        let world = World::new(Backing::Plain);
        world
            .requested
            .store(StopSequence::FIRST, Ordering::Release);
        world.acknowledge(Owner::A, StopSequence::FIRST);
        world.acknowledge(Owner::B, StopSequence::FIRST);
        assert_eq!(
            world.trace(StopSequence::FIRST, Fault::None),
            Some(Objects::EMPTY)
        );
        world
            .requested
            .store(StopSequence::SECOND, Ordering::Release);
        assert_eq!(world.trace(StopSequence::SECOND, Fault::None), None);
        world.acknowledge(Owner::A, StopSequence::SECOND);
        assert_eq!(world.trace(StopSequence::SECOND, Fault::None), None);
        world.acknowledge(Owner::B, StopSequence::SECOND);
        assert_eq!(
            world.trace(StopSequence::SECOND, Fault::None),
            Some(Objects::EMPTY)
        );
    });
}

#[test]
fn omitted_second_global_reseed_has_a_witness() {
    support::negative(2, Fault::NoGlobalReseed.witness(), || {
        transfers(
            Backing::Plain,
            CollectionMode::Gen0Full,
            Fault::NoGlobalReseed,
        );
    });
}

#[test]
fn omitted_canonical_projection_backing_has_a_witness() {
    support::negative(2, Fault::NoProjection.witness(), || {
        transfers(
            Backing::ProjectedPlain,
            CollectionMode::Gen1Reseed,
            Fault::NoProjection,
        );
    });
}

#[test]
fn omitted_function_cell_root_has_a_witness() {
    support::negative(2, Fault::NoFunction.witness(), || {
        transfers(Backing::Plain, CollectionMode::Gen0Full, Fault::NoFunction);
    });
}

#[test]
fn omitted_activation_after_replacement_has_a_witness() {
    support::negative(2, Fault::NoActivation.witness(), || {
        transfers(
            Backing::Plain,
            CollectionMode::Gen0Full,
            Fault::NoActivation,
        );
    });
}

#[test]
fn omitted_exact_name_registry_root_has_a_witness() {
    support::negative(2, Fault::NoName.witness(), || {
        transfers(
            Backing::ProjectedPlain,
            CollectionMode::Gen1Reseed,
            Fault::NoName,
        );
    });
}

#[test]
fn acknowledgement_before_root_publication_has_a_witness() {
    support::negative(
        2,
        "C6 ACK preceded publication of its owner root image",
        || {
            root_ack_history(AckOrder::AckBeforeRoots);
        },
    );
}

#[test]
fn keeper_bypassing_held_lease_gate_has_a_witness() {
    support::negative(1, "C6 keeper traced while a symbol lease was held", || {
        let world = World::new(Backing::Plain);
        world
            .requested
            .store(StopSequence::FIRST, Ordering::Release);
        if let Some(_lease) = world.authority.read() {
            for ack in &world.ack {
                ack.store(StopSequence::FIRST, Ordering::Release);
            }
            let _ =
                world.trace_with_word_gate(StopSequence::FIRST, Fault::None, WordGate::Bypassed);
        }
    });
}
