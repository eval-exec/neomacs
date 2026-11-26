//! Hold the real control gate past the old fixture's entire yield budget.
//! The held phase is count-driven: a wall-clock contention deadline cannot
//! measure progress while the harness deliberately prevents it. Only the
//! outer watchdog bounds that phase; the normal deadline starts after release.
use super::*;

#[derive(Clone, Copy)]
enum Contention {
    Uncontended,
    HeldPastBudget,
}

enum RestorePhase {
    Held,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Refusals(u32);

#[derive(Debug, PartialEq, Eq)]
enum HeldReceipt {
    Uncontended,
    PastBudget(Refusals),
    FinishedEarly(Result<u64, RendezvousError>),
}

// The progress and cleanup drivers observe the same real attempt and update
// restoration state in one place. A refusal never means an image was restored.
fn restore_once(owner: &mut Mutator<'_>, native: &mut NativeState) -> Result<u64, RendezvousError> {
    let result = owner.leave_native();
    if result.is_ok() {
        *native = NativeState::Running;
    }
    result
}

fn restore_after_hold(
    owner: &mut Mutator<'_>,
    native: &mut NativeState,
    mut phase: RestorePhase,
    budget: Sender<HeldReceipt>,
    released: &Receiver<()>,
) -> Result<u64, RendezvousError> {
    let mut refusals = Refusals(0);
    loop {
        match phase {
            RestorePhase::Held => match restore_once(owner, native) {
                Err(RendezvousError::Busy) => {
                    refusals.0 += 1;
                    if refusals.0 == STEP_CAP {
                        budget.send(HeldReceipt::PastBudget(refusals)).unwrap();
                        released.recv().expect("holder releases or disconnects");
                        phase = RestorePhase::Released;
                    } else {
                        thread::yield_now();
                    }
                }
                result => {
                    // Report an early terminal outcome before cleanup. The
                    // holder releases even on this path; it cannot strand the
                    // native owner while inspecting a failed count witness.
                    budget.send(HeldReceipt::FinishedEarly(result)).unwrap();
                    released.recv().expect("holder releases or disconnects");
                    return result;
                }
            },
            RestorePhase::Released => {
                return contention::retry(
                    Deadline::contention(),
                    || restore_once(owner, native),
                    contention::rendezvous,
                );
            }
        }
    }
}

fn resume_after_contention<K: crate::tagged::gc::mutator_registration::EraKeeper>(
    contention: Contention,
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    let _probes = a8_probes::suppress();
    let permit = era.register(3900, 1).unwrap();
    let registry = era.registry();
    let heap = era.heap_identity();
    let (held, (result, cleanup, retired)) = thread::scope(|scope| {
        let (paused_tx, paused_rx) = channel();
        let (resume_tx, resume_rx) = channel();
        let (budget_tx, budget_rx) = channel();
        let (released_tx, released_rx) = channel();
        let worker = scope.spawn(move || {
            let _probes = a8_probes::suppress();
            // This sole owner activates before the keeper starts its hold.
            let mut owner = permit.activate(heap, 3900).unwrap();
            owner.enter_native(&[]).unwrap();
            paused_tx.send(()).unwrap();
            resume_rx.recv().expect("keeper starts the attempt");
            let mut native = NativeState::Published;
            let phase = match contention {
                Contention::Uncontended => RestorePhase::Released,
                Contention::HeldPastBudget => RestorePhase::Held,
            };
            // Ownership of budget_tx ensures every Result/unwind drops it
            // before owner cleanup, waking a holder whose witness failed.
            let result =
                restore_after_hold(&mut owner, &mut native, phase, budget_tx, &released_rx);
            // Always restore/retire before asserting the tested result.
            let cleanup = match native {
                NativeState::Running => Ok(()),
                NativeState::Published => contention::retry(
                    Deadline::contention(),
                    || restore_once(&mut owner, &mut native),
                    contention::rendezvous,
                )
                .map(|_| ()),
            };
            // No peer remains in a metadata operation after release.
            let retired = owner.retire_rooted(&[]).map_err(|failure| failure.reason);
            (result, cleanup, retired)
        });
        paused_rx.recv().expect("owner publishes its native image");
        let held = match contention {
            Contention::Uncontended => {
                resume_tx.send(()).unwrap();
                HeldReceipt::Uncontended
            }
            Contention::HeldPastBudget => {
                let gate = registry.lock().unwrap();
                resume_tx.send(()).unwrap();
                let held = budget_rx.recv().expect("count witness or disconnect");
                // Preserve the original simulated 20 ms preemption after
                // the same 10,000 real refusals. It supplies no permission
                // to restore: the release event follows the actual unlock.
                thread::sleep(Duration::from_millis(20));
                drop(gate);
                released_tx.send(()).unwrap();
                held
            }
        };
        // Drop every command/release sender before joining, including on
        // unwind, so no early worker outcome can strand a scoped peer.
        drop(resume_tx);
        drop(released_tx);
        (held, worker.join().unwrap())
    });
    assert_eq!(retired, Ok(()), "result={result:?}; cleanup={cleanup:?}");
    let mut stopped = era.stop().unwrap();
    stopped.trace(|_, _| Ok(())).unwrap();
    stopped.complete().unwrap();
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
    assert_eq!(cleanup, Ok(()));
    match contention {
        Contention::Uncontended => assert_eq!(held, HeldReceipt::Uncontended),
        Contention::HeldPastBudget => {
            assert_eq!(held, HeldReceipt::PastBudget(Refusals(STEP_CAP)))
        }
    }
    assert!(
        result.is_ok(),
        "owner restore after gate contention: {result:?}"
    );
}

pub(crate) fn handoff_resume_uncontended<K: crate::tagged::gc::mutator_registration::EraKeeper>(
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    resume_after_contention(Contention::Uncontended, era);
}

pub(crate) fn handoff_resume_outlasts_yield_budget<
    K: crate::tagged::gc::mutator_registration::EraKeeper,
>(
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    resume_after_contention(Contention::HeldPastBudget, era);
}
