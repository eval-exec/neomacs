//! C6 specification models: mixed root transfers and repeated stop censuses.
//! Object bits and generations are finite specifications, not an allocator or
//! evidence that the engine ran a minor. Scripted transfer phases isolate root
//! obligations; the separate ACK model explores publication without that script.
mod model;
mod support;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::thread;
use model::{Authority, Checked, Latch};

const OLD_FUNCTION: usize = 1;
const NAME: usize = 2;
const NEW_FUNCTION: usize = 4;

fn reachable(roots: usize) -> usize {
    // F0 returns this exact immutable name object; F1 returns an immediate.
    roots | if roots & OLD_FUNCTION != 0 { NAME } else { 0 }
}

#[derive(Clone, Copy, Default)]
struct Cells {
    global: usize,
    projection: usize,
    function: usize,
    journal: usize,
    names: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    NoGlobalReseed,
    NoProjection,
    NoActivation,
    NoName,
}

impl Fault {
    fn witness(self) -> &'static str {
        match self {
            Self::None => "C6 combined live roots were lost",
            Self::NoGlobalReseed => "C6 second stop omitted live global reseeding",
            Self::NoProjection => "C6 stop omitted canonical projection backing",
            Self::NoActivation => "C6 replacement lost the active old definition",
            Self::NoName => "C6 stop omitted the exact registry name root",
        }
    }
}

struct World {
    authority: Authority,
    cells: Checked<Cells>,
    projected: bool,
    homes: [AtomicUsize; 2],
    activations: [AtomicUsize; 2],
    published_homes: [AtomicUsize; 2],
    published_activations: [AtomicUsize; 2],
    requested: AtomicUsize,
    ack: [AtomicUsize; 2],
}

impl World {
    fn new(projected: bool) -> Self {
        Self {
            authority: Authority::new(),
            cells: Checked::new(Cells::default()),
            projected,
            homes: std::array::from_fn(|_| AtomicUsize::new(0)),
            activations: std::array::from_fn(|_| AtomicUsize::new(0)),
            published_homes: std::array::from_fn(|_| AtomicUsize::new(0)),
            published_activations: std::array::from_fn(|_| AtomicUsize::new(0)),
            requested: AtomicUsize::new(0),
            ack: std::array::from_fn(|_| AtomicUsize::new(0)),
        }
    }

    fn global(&self, cells: Cells) -> usize {
        if self.projected {
            cells.projection
        } else {
            cells.global
        }
    }

    fn set_global(&self, cells: &mut Cells, value: usize) {
        if self.projected {
            cells.projection = value;
        } else {
            cells.global = value;
        }
    }

    fn write(&self, body: impl FnOnce(&mut Cells)) -> bool {
        let Some(_writer) = self.authority.write() else {
            return false;
        };
        let mut cells = self.cells.get();
        body(&mut cells);
        self.cells.set(cells);
        true
    }

    fn global_into_home(&self, owner: usize) -> bool {
        let Some(_reader) = self.authority.read() else {
            return false;
        };
        self.homes[owner].store(self.global(self.cells.get()), Ordering::Relaxed);
        // The actual answer home exists before releasing the reader lease.
        true
    }

    fn begin_call(&self, owner: usize) -> bool {
        let Some(_reader) = self.authority.read() else {
            return false;
        };
        self.activations[owner].store(self.cells.get().function, Ordering::Relaxed);
        true
    }

    fn publish_roots(&self, owner: usize) {
        // Atomic scalar catalog words model a frozen root image, not atomic
        // symbol payloads. ACK, not a new root mutex, publishes their contents.
        self.published_homes[owner]
            .store(self.homes[owner].load(Ordering::Relaxed), Ordering::Relaxed);
        self.published_activations[owner].store(
            self.activations[owner].load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    fn acknowledge(&self, owner: usize, epoch: usize) {
        assert_eq!(self.requested.load(Ordering::Acquire), epoch);
        self.authority.assert_quiescent();
        self.publish_roots(owner);
        self.ack[owner].store(epoch, Ordering::Release);
    }

    fn trace(&self, epoch: usize, fault: Fault) -> Option<usize> {
        self.trace_with_word_gate(epoch, fault, true)
    }

    fn trace_with_word_gate(&self, epoch: usize, fault: Fault, gate_word: bool) -> Option<usize> {
        if self.requested.load(Ordering::Acquire) != epoch
            || self
                .ack
                .iter()
                .any(|ack| ack.load(Ordering::Acquire) != epoch)
        {
            return None;
        }
        if gate_word && self.authority.word.load(Ordering::Acquire) != 0 {
            return None;
        }
        // Independent invariant catches the broken admission gate before any
        // ordinary storage read can produce an unrelated causality witness.
        assert!(
            self.authority.word.load(Ordering::Acquire) == 0,
            "C6 keeper traced while a symbol lease was held"
        );
        let cells = self.cells.get();
        let global = if fault == Fault::NoGlobalReseed && epoch >= 2 {
            0
        } else if self.projected && fault == Fault::NoProjection {
            0
        } else {
            self.global(cells)
        };
        let mut roots = global | cells.function | cells.journal;
        if fault != Fault::NoName {
            roots |= cells.names;
        }
        for owner in 0..2 {
            roots |= self.published_homes[owner].load(Ordering::Relaxed);
            if fault != Fault::NoActivation {
                roots |= self.published_activations[owner].load(Ordering::Relaxed);
            }
        }
        Some(reachable(roots))
    }

    fn reconcile(&self) {
        self.authority.assert_quiescent();
        let mut cells = self.cells.get();
        // Suspended serial caches have been evicted; no journal may mask an
        // omitted active call root in the subsequent collector census.
        cells.journal = 0;
        self.cells.set(cells);
    }
}

fn completed(done: bool) {
    // All scripted actions are separated by stopped/joined handoffs. A Busy
    // result is handled explicitly; no refused try-acquisition is unwrapped.
    assert!(done, "C6 scripted uncontended action refused authority");
}

#[derive(Clone, Copy)]
enum CollectionMode {
    Gen0Full,
    Gen1Reseed,
}

/// Finite generation bookkeeping only. Every call starts with a NEW mark set.
/// Real minor collection, remsets and image restoration need engine evidence.
fn sweep(alive: &mut usize, young: &mut usize, marks: usize, full: bool) {
    let eligible = if full { *alive } else { *young };
    *alive &= !eligible | marks;
    *young &= *alive & !marks; // surviving modeled young objects promote.
}

fn transfers(projected: bool, mode: CollectionMode, fault: Fault) {
    let world = Arc::new(World::new(projected));
    let starts = Arc::new(std::array::from_fn::<_, 6, _>(|_| Latch::default()));
    let done = Arc::new(std::array::from_fn::<_, 6, _>(|_| Latch::default()));
    let (other, worker_starts, worker_done) =
        (Arc::clone(&world), Arc::clone(&starts), Arc::clone(&done));
    let owner_a = thread::spawn(move || {
        for phase in 0..6 {
            worker_starts[phase].wait();
            match phase {
                0 => {
                    other.homes[0].store(OLD_FUNCTION, Ordering::Relaxed);
                    completed(other.write(|cells| other.set_global(cells, OLD_FUNCTION)));
                    other.homes[0].store(0, Ordering::Relaxed);
                }
                3 => completed(other.begin_call(0)),
                4 => {
                    assert_eq!(other.activations[0].load(Ordering::Relaxed), OLD_FUNCTION);
                    other.homes[0].store(NAME, Ordering::Relaxed); // bounded old call returns.
                    other.activations[0].store(0, Ordering::Relaxed);
                    completed(other.write(|cells| other.set_global(cells, NAME)));
                    other.homes[0].store(0, Ordering::Relaxed);
                }
                _ => {}
            }
            other.acknowledge(0, phase + 1);
            worker_done[phase].set();
            // Wait for the next keeper resume/request; never re-enter merely
            // because ACK has been published. Latches are script edges only.
        }
    });
    let mut alive = OLD_FUNCTION | NAME;
    let mut young = alive;
    let mut omitted_live_root = false;
    for phase in 0..6 {
        let epoch = phase + 1;
        world.requested.store(epoch, Ordering::Release);
        starts[phase].set();
        done[phase].wait();
        match phase {
            2 => {
                completed(world.global_into_home(1));
                let received = world.homes[1].load(Ordering::Relaxed);
                assert_eq!(received, OLD_FUNCTION);
                completed(world.write(|cells| {
                    cells.function = received;
                    world.set_global(cells, 0);
                }));
                world.homes[1].store(0, Ordering::Relaxed);
            }
            3 => {
                alive |= NEW_FUNCTION;
                young |= NEW_FUNCTION;
                completed(world.write(|cells| {
                    cells.journal |= cells.function;
                    cells.function = NEW_FUNCTION;
                }));
            }
            4 => {
                completed(world.global_into_home(1));
                let received = world.homes[1].load(Ordering::Relaxed);
                assert_eq!(received, NAME);
                completed(world.write(|cells| {
                    cells.names |= received; // exact winning name index.
                    world.set_global(cells, 0);
                }));
                world.homes[1].store(0, Ordering::Relaxed);
            }
            5 => completed(world.write(|cells| {
                cells.journal |= cells.function;
                cells.function = 0;
            })),
            _ => {}
        }
        world.acknowledge(1, epoch);
        world.reconcile();
        let observed = world.trace(epoch, fault).expect("matching combined stop");
        let expected = match phase {
            0..=2 => OLD_FUNCTION | NAME,
            3 => OLD_FUNCTION | NAME | NEW_FUNCTION,
            4 => NAME | NEW_FUNCTION,
            _ => NAME,
        };
        omitted_live_root |= observed != expected;
        // Fault controls omit one family from an AUDIT census only. Complete
        // every resume/handoff with the real root set, then report the named
        // witness after joining the worker; no failed check strands a phase.
        let marks = world
            .trace(epoch, Fault::None)
            .expect("complete root census");
        let full = matches!(mode, CollectionMode::Gen0Full) || phase >= 4;
        sweep(&mut alive, &mut young, marks, full);
        assert_eq!(
            alive, expected,
            "C6 reachable objects differ after stopped sweep"
        );
        if phase >= 4 {
            assert_eq!(
                alive & OLD_FUNCTION,
                0,
                "completed old activation must retire"
            );
        }
        // Resetting marks on the second stop defeats allocation/birth retention
        // as a substitute for canonical global/projection reseeding.
    }
    owner_a.join().unwrap();
    world.authority.assert_quiescent();
    assert!(!omitted_live_root, "{}", fault.witness());
}

#[test]
fn gen0_plain_global_function_activation_name_transfer() {
    support::check(2, || {
        transfers(false, CollectionMode::Gen0Full, Fault::None)
    });
}

#[test]
fn gen0_projected_global_function_activation_name_transfer() {
    support::check(2, || transfers(true, CollectionMode::Gen0Full, Fault::None));
}

#[test]
fn gen1_plain_reseed_and_later_major_retire_old_definition() {
    support::check(2, || {
        transfers(false, CollectionMode::Gen1Reseed, Fault::None)
    });
}

#[test]
fn gen1_projected_reseed_and_later_major_retire_old_definition() {
    support::check(2, || {
        transfers(true, CollectionMode::Gen1Reseed, Fault::None)
    });
}

fn old_global_young_replacement(projected: bool) {
    let world = World::new(projected);
    completed(world.write(|cells| world.set_global(cells, OLD_FUNCTION)));
    let mut alive = OLD_FUNCTION | NAME;
    let mut young = 0; // original binding has already promoted.
    for epoch in 1..=3 {
        world.requested.store(epoch, Ordering::Release);
        if epoch == 2 {
            alive |= NEW_FUNCTION;
            young |= NEW_FUNCTION;
            completed(world.write(|cells| world.set_global(cells, NEW_FUNCTION)));
        }
        world.acknowledge(0, epoch);
        world.acknowledge(1, epoch);
        let marks = world
            .trace(epoch, Fault::None)
            .expect("fresh stopped census");
        let expected = if epoch == 1 {
            OLD_FUNCTION | NAME
        } else {
            NEW_FUNCTION
        };
        assert_eq!(
            marks, expected,
            "old global must reseed its current young replacement"
        );
        sweep(&mut alive, &mut young, marks, epoch == 3);
        assert_eq!(
            alive & NEW_FUNCTION,
            if epoch == 1 { 0 } else { NEW_FUNCTION }
        );
    }
    assert_eq!(
        alive, NEW_FUNCTION,
        "later full job reclaims unreachable old objects"
    );
}

#[test]
fn gen1_old_plain_global_reseeds_young_replacement_at_second_stop() {
    support::check(1, || old_global_young_replacement(false));
}

#[test]
fn gen1_old_projected_global_reseeds_young_replacement_at_second_stop() {
    support::check(1, || old_global_young_replacement(true));
}

fn root_ack_history(early_ack: bool) {
    let world = Arc::new(World::new(false));
    completed(world.write(|cells| cells.global = OLD_FUNCTION));
    world.requested.store(1, Ordering::Release);
    world.acknowledge(1, 1);
    let other = Arc::clone(&world);
    let actor = thread::spawn(move || {
        // One attempt can legally refuse; the completed suffix below handles
        // that branch. No actor owns a lease after returning or acknowledging.
        let Some(writer) = other.authority.write() else {
            return false;
        };
        let mut cells = other.cells.get();
        other.homes[0].store(cells.global, Ordering::Relaxed);
        cells.global = 0;
        other.cells.set(cells);
        drop(writer);
        if early_ack {
            other.ack[0].store(1, Ordering::Release); // broken publication order.
            thread::yield_now();
            other.publish_roots(0);
        } else {
            other.acknowledge(0, 1);
        }
        true
    });
    let observed = world.trace(1, Fault::None);
    let completed = actor.join().unwrap();
    if let Some(marks) = observed {
        assert!(
            marks == OLD_FUNCTION | NAME,
            "C6 ACK preceded publication of its owner root image"
        );
    }
    if completed {
        assert_eq!(world.trace(1, Fault::None), Some(OLD_FUNCTION | NAME));
    } else {
        // Keeper observation never acquires authority, so refusal cannot occur
        // in this particular schedule; keep the branch legal and explicit.
        world.acknowledge(0, 1);
        assert_eq!(world.trace(1, Fault::None), Some(OLD_FUNCTION | NAME));
    }
    world.authority.assert_quiescent();
}

#[test]
fn root_publication_and_lease_release_happen_before_ack() {
    support::check(2, || root_ack_history(false));
}

#[test]
fn held_lease_refuses_even_a_complete_ack_census() {
    support::check(1, || {
        let world = World::new(false);
        world.requested.store(1, Ordering::Release);
        if let Some(lease) = world.authority.read() {
            // Adversarial ACKs: the keeper must independently check the word.
            for ack in &world.ack {
                ack.store(1, Ordering::Release);
            }
            assert_eq!(world.trace(1, Fault::None), None);
            drop(lease);
        }
        world.acknowledge(0, 1);
        world.acknowledge(1, 1);
        assert_eq!(world.trace(1, Fault::None), Some(0));
    });
}

#[test]
fn previous_stop_acknowledgements_do_not_authorize_the_next_census() {
    support::check(1, || {
        let world = World::new(false);
        world.requested.store(1, Ordering::Release);
        world.acknowledge(0, 1);
        world.acknowledge(1, 1);
        assert_eq!(world.trace(1, Fault::None), Some(0));
        world.requested.store(2, Ordering::Release);
        assert_eq!(world.trace(2, Fault::None), None);
        world.acknowledge(0, 2);
        assert_eq!(world.trace(2, Fault::None), None);
        world.acknowledge(1, 2);
        assert_eq!(world.trace(2, Fault::None), Some(0));
    });
}

#[test]
fn omitted_second_global_reseed_has_a_witness() {
    support::negative(2, Fault::NoGlobalReseed.witness(), || {
        transfers(false, CollectionMode::Gen0Full, Fault::NoGlobalReseed);
    });
}

#[test]
fn omitted_canonical_projection_backing_has_a_witness() {
    support::negative(2, Fault::NoProjection.witness(), || {
        transfers(true, CollectionMode::Gen1Reseed, Fault::NoProjection);
    });
}

#[test]
fn omitted_activation_after_replacement_has_a_witness() {
    support::negative(2, Fault::NoActivation.witness(), || {
        transfers(false, CollectionMode::Gen0Full, Fault::NoActivation);
    });
}

#[test]
fn omitted_exact_name_registry_root_has_a_witness() {
    support::negative(2, Fault::NoName.witness(), || {
        transfers(true, CollectionMode::Gen1Reseed, Fault::NoName);
    });
}

#[test]
fn acknowledgement_before_root_publication_has_a_witness() {
    support::negative(
        2,
        "C6 ACK preceded publication of its owner root image",
        || {
            root_ack_history(true);
        },
    );
}

#[test]
fn keeper_bypassing_held_lease_gate_has_a_witness() {
    support::negative(1, "C6 keeper traced while a symbol lease was held", || {
        let world = World::new(false);
        world.requested.store(1, Ordering::Release);
        if let Some(_lease) = world.authority.read() {
            for ack in &world.ack {
                ack.store(1, Ordering::Release);
            }
            let _ = world.trace_with_word_gate(1, Fault::None, false);
        }
    });
}
