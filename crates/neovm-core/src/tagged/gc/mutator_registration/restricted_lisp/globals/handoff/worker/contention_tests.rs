//! Hold the real control gate past the old fixture's entire yield budget.
use super::*;

#[derive(Clone, Copy)]
enum Contention {
    Uncontended,
    HeldPastBudget,
}

fn resume_after_contention(contention: Contention) {
    let _probes = a8_probes::suppress();
    Context::with_test_symbol_backing_era(PlainSymbolBacking::Projected, |era, _| {
        let permit = era.register(3900, 1).unwrap();
        let registry = era.registry();
        let heap = era.heap_identity();
        let (result, cleanup) = thread::scope(|scope| {
            let (paused_tx, paused_rx) = channel();
            let (resume_tx, resume_rx) = channel();
            let (budget_tx, budget_rx) = channel();
            let worker = scope.spawn(move || {
                let _probes = a8_probes::suppress();
                let mut owner = activate_owner(permit, heap, 3900);
                owner.enter_native(&[]).unwrap();
                paused_tx.send(()).unwrap();
                receive(&resume_rx);
                let mut attempts = 0;
                let result = contention::retry(
                    Deadline::contention(),
                    || {
                        attempts += 1;
                        let result = owner.leave_native();
                        if attempts == STEP_CAP {
                            budget_tx.send(()).unwrap();
                        }
                        result
                    },
                    contention::rendezvous,
                );
                // Always restore/retire before asserting the tested result.
                let cleanup = match result {
                    Ok(_) => Ok(()),
                    Err(_) => {
                        receive(&resume_rx);
                        contention::retry(
                            Deadline::contention(),
                            || owner.leave_native(),
                            contention::rendezvous,
                        )
                        .map(|_| ())
                    }
                };
                retire_owner(owner).unwrap();
                (result, cleanup)
            });
            receive(&paused_rx);
            match contention {
                Contention::Uncontended => resume_tx.send(()).unwrap(),
                Contention::HeldPastBudget => {
                    let gate = registry.lock().unwrap();
                    resume_tx.send(()).unwrap();
                    // No timing assumption about how quickly 10,000 yields
                    // execute: release only after the last old-budget attempt,
                    // then delay a further 20 ms like a preempted gate holder.
                    receive(&budget_rx);
                    thread::sleep(Duration::from_millis(20));
                    drop(gate);
                    let _ = resume_tx.send(());
                }
            }
            worker.join().unwrap()
        });
        let mut stopped = era.stop().unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        assert_eq!(cleanup, Ok(()));
        assert!(
            result.is_ok(),
            "owner restore after gate contention: {result:?}"
        );
    });
}

#[test]
fn handoff_resume_uncontended_control() {
    resume_after_contention(Contention::Uncontended);
}

#[test]
fn handoff_resume_outlasts_yield_budget() {
    resume_after_contention(Contention::HeldPastBudget);
}
