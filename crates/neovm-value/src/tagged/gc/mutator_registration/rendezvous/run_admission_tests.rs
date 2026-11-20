//! Bodies for the cold resume-drain admission rows.
//!
//! The core shell opens the rooted era. These functions stay here so they can
//! drive the private gate-held entry. They are not `#[test]` items: the era
//! opener lives in neovm-eval.
#![forbid(unsafe_code)]

use std::num::NonZeroU64;
use std::sync::mpsc;

use super::super::super::admission::PhaseLine;
use super::super::super::{EraKeeper, MutatorKey, RegistrationError};
use super::super::fixture_helpers::RootedEraFn;
use super::super::{Phase, Request};

#[derive(Clone, Copy)]
enum DrainRelease {
    FinalRestore,
    Poison,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Acknowledge(NonZeroU64),
    ReleaseDrain,
    Retire,
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Ready(MutatorKey),
    Acknowledged,
    GateBlocked,
    Restored,
}

fn peer_drain<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
    release: DrainRelease,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let mut first = era.register(4101, 1).unwrap().activate(heap, 4101).unwrap();
        let permit = era.register(4102, 1).unwrap();
        let registry = era.registry();
        std::thread::scope(|scope| {
            // These senders drop before the scoped join on every unwind path.
            let (commands, command_rx) = mpsc::channel();
            let (events, event_rx) = mpsc::channel();
            let peer_roots = vec![roots[1].clone()];
            let peer = scope.spawn(move || {
                let mut owner = permit.activate(heap, 4102).unwrap();
                events.send(Event::Ready(owner.key())).unwrap();
                let Command::Acknowledge(epoch) = command_rx.recv().unwrap() else {
                    panic!("acknowledge command required")
                };
                owner.acknowledge(epoch, &peer_roots).unwrap();
                events.send(Event::Acknowledged).unwrap();
                assert_eq!(command_rx.recv().unwrap(), Command::ReleaseDrain);
                // An observed real failed mutex acquisition, not a before-call
                // message or a sleep, forces the atomic wait-release boundary.
                assert!(registry.predicate_mutex_would_block());
                events.send(Event::GateBlocked).unwrap();
                match release {
                    DrainRelease::FinalRestore => {
                        // The actual product restore driver waits out initial
                        // metadata contention and finishes this final debt.
                        owner.restore_wait(false).unwrap();
                        events.send(Event::Restored).unwrap();
                        assert_eq!(command_rx.recv().unwrap(), Command::Retire);
                        owner.retire_rooted(&peer_roots).unwrap();
                    }
                    DrainRelease::Poison => {
                        registry.poison();
                        drop(owner);
                    }
                }
            });
            let Event::Ready(peer_key) = event_rx.recv().unwrap() else {
                panic!("ready owner required")
            };
            let Request::Active(epoch) = era.rendezvous().request().unwrap() else {
                panic!("first request must activate")
            };
            first.acknowledge(epoch, &roots[..1]).unwrap();
            commands.send(Command::Acknowledge(epoch)).unwrap();
            assert_eq!(event_rx.recv().unwrap(), Event::Acknowledged);
            let mut stopped = era.stop().unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            first.restore_wait(false).unwrap();

            let gate = registry.lock_blocking().unwrap();
            assert_eq!(gate.rendezvous.phase, Phase::DrainingResumes);
            assert_eq!(gate.rendezvous.debt, vec![peer_key]);
            assert!(matches!(first.run_access(), Err(RegistrationError::Busy)));
            commands.send(Command::ReleaseDrain).unwrap();
            assert_eq!(event_rx.recv().unwrap(), Event::GateBlocked);
            // Exercise the same canonical body as the public entry, with the
            // predicate gate already held to force the park vs finish edge.
            let admitted = first.run_access_after_resume_drain_with_gate(gate);
            match release {
                DrainRelease::FinalRestore => {
                    let run = admitted.unwrap();
                    assert!(PhaseLine::admissible(registry.phase.load()));
                    assert_eq!(event_rx.recv().unwrap(), Event::Restored);
                    // The API must release its predicate gate before handing
                    // back the ordinary run capability.
                    let gate = registry.lock().unwrap();
                    assert_eq!(gate.rendezvous.phase, Phase::Idle);
                    assert!(gate.rendezvous.debt.is_empty());
                    drop(gate);
                    drop(run);
                    first.retire_rooted(&roots[..1]).unwrap();
                    commands.send(Command::Retire).unwrap();
                }
                DrainRelease::Poison => {
                    assert!(matches!(&admitted, Err(RegistrationError::Poisoned)));
                    drop(admitted);
                    drop(first);
                }
            }
            peer.join().unwrap();
        });
        match release {
            DrainRelease::FinalRestore => {
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
            }
            DrainRelease::Poison => {
                assert_eq!(
                    era.finish().unwrap_err().reason,
                    RegistrationError::Poisoned
                );
            }
        }
    });
}

pub(crate) fn first_restored_owner_waits_for_the_final_peer_debt_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    peer_drain(rooted_era, DrainRelease::FinalRestore);
}

pub(crate) fn poison_wakes_a_restored_owner_waiting_for_peer_debt_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    peer_drain(rooted_era, DrainRelease::Poison);
}

pub(crate) fn stop_and_unrestored_owner_refuse_instead_of_waiting_for_themselves_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let mut owner = era
            .register(4103, 1)
            .unwrap()
            .activate(era.heap_identity(), 4103)
            .unwrap();
        drop(owner.run_access_after_resume_drain().unwrap());
        let Request::Active(epoch) = era.rendezvous().request().unwrap() else {
            panic!("first request must activate")
        };
        assert!(matches!(
            owner.run_access_after_resume_drain(),
            Err(RegistrationError::Busy)
        ));
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        let mut stopped = era.stop().unwrap();
        assert!(matches!(
            owner.run_access_after_resume_drain(),
            Err(RegistrationError::Busy)
        ));
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        // Global drain is waitable only after this owner's own image returns.
        assert!(matches!(
            owner.run_access_after_resume_drain(),
            Err(RegistrationError::Busy)
        ));
        owner.restore_wait(false).unwrap();
        drop(owner.run_access_after_resume_drain().unwrap());
        owner.retire_rooted(&roots[..1]).unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}
