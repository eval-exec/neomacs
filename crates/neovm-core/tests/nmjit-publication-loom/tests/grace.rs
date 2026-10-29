//! Registry/activation/grace specification. The mutex is the registry admission
//! boundary, not a proposed lock on generated table reads. Publication ordering
//! is explored separately in publication.rs.
mod support;

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TableId(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StopGeneration(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnerSlot(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableState {
    Owned,
    Retired,
    Reclaimed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    Running,
    Closing,
    Stopped,
    Poisoned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Ready,
    Starting,
    Active,
    Native,
    Cancelled,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    Busy,
    Closed,
    LiveActivation,
    Poisoned,
}

struct State {
    admission: Admission,
    owners: [Owner; 2],
    tables: [TableState; 2],
    generation: StopGeneration,
}
struct Runtime {
    state: Mutex<State>,
    current: AtomicUsize,
}

#[must_use]
struct View {
    runtime: Arc<Runtime>,
    table: TableId,
    owner: OwnerSlot,
}
impl View {
    fn read(&self) {
        let alive =
            self.runtime.state.lock().unwrap().tables[self.table.0] != TableState::Reclaimed;
        assert!(alive, "OLD_TABLE_RECLAIMED_BEFORE_GRACE");
    }
}
impl Drop for View {
    fn drop(&mut self) {
        self.runtime.state.lock().unwrap().owners[self.owner.0] = Owner::Ready;
    }
}

#[must_use]
struct JitGracePeriod {
    runtime: Arc<Runtime>,
    generation: StopGeneration,
}

fn runtime() -> Arc<Runtime> {
    Arc::new(Runtime {
        state: Mutex::new(State {
            admission: Admission::Running,
            owners: [Owner::Ready; 2],
            tables: [TableState::Owned; 2],
            generation: StopGeneration(0),
        }),
        current: AtomicUsize::new(0),
    })
}

impl Runtime {
    fn enter(self: &Arc<Self>, owner: OwnerSlot) -> Result<View, Refusal> {
        {
            let mut state = self.state.lock().unwrap();
            match state.admission {
                Admission::Running => {}
                Admission::Closing | Admission::Stopped => return Err(Refusal::Closed),
                Admission::Poisoned => return Err(Refusal::Poisoned),
            }
            match state.owners[owner.0] {
                Owner::Ready => state.owners[owner.0] = Owner::Active,
                Owner::Starting | Owner::Active => return Err(Refusal::Busy),
                Owner::Native | Owner::Cancelled | Owner::Retired => return Err(Refusal::Closed),
            }
        }
        // Active is recorded BEFORE this load; stop cannot clear the ledger
        // during the acquisition window.
        Ok(View {
            runtime: self.clone(),
            table: TableId(self.current.load(Ordering::Acquire)),
            owner,
        })
    }

    fn replace(&self) -> Result<(), Refusal> {
        let mut state = self.state.lock().unwrap();
        match state.admission {
            Admission::Running => {
                // Both table arrays were fully prepared before this model step.
                state.tables[0] = TableState::Retired;
                self.current.store(1, Ordering::Release);
                Ok(())
            }
            Admission::Closing | Admission::Stopped => Err(Refusal::Closed),
            Admission::Poisoned => Err(Refusal::Poisoned),
        }
    }

    fn stop(self: &Arc<Self>) -> Result<JitGracePeriod, Refusal> {
        let mut state = self.state.lock().unwrap();
        match state.admission {
            Admission::Running => state.admission = Admission::Closing,
            Admission::Closing => {}
            Admission::Stopped => return Err(Refusal::Closed),
            Admission::Poisoned => return Err(Refusal::Poisoned),
        }
        for owner in state.owners {
            match owner {
                Owner::Starting | Owner::Active => return Err(Refusal::LiveActivation),
                Owner::Ready | Owner::Native | Owner::Cancelled | Owner::Retired => {}
            }
        }
        state.admission = Admission::Stopped;
        state.generation.0 += 1;
        Ok(JitGracePeriod {
            runtime: self.clone(),
            generation: state.generation,
        })
    }

    fn reclaim(&self, grace: JitGracePeriod) {
        assert!(std::ptr::eq(self, &*grace.runtime));
        let mut state = self.state.lock().unwrap();
        assert_eq!(state.admission, Admission::Stopped);
        assert_eq!(state.generation, grace.generation);
        for table in &mut state.tables {
            match table {
                TableState::Retired => *table = TableState::Reclaimed,
                TableState::Owned | TableState::Reclaimed => {}
            }
        }
    }
}

#[test]
fn replacement_waits_for_reader_and_stop_blocks_new_entries() {
    support::check(4, || {
        let runtime = runtime();
        let reader = {
            let runtime = runtime.clone();
            thread::spawn(move || match runtime.enter(OwnerSlot(0)) {
                Ok(view) => {
                    view.read();
                    thread::yield_now();
                    view.read();
                }
                Err(Refusal::Busy | Refusal::Closed | Refusal::Poisoned) => {}
                Err(Refusal::LiveActivation) => panic!("entry classifier mismatch"),
            })
        };
        let writer = {
            let runtime = runtime.clone();
            thread::spawn(move || {
                let _legal_refusal_or_commit = runtime.replace();
            })
        };
        let keeper = {
            let runtime = runtime.clone();
            thread::spawn(move || match runtime.stop() {
                Ok(grace) => runtime.reclaim(grace),
                Err(Refusal::LiveActivation) => {}
                Err(other) => panic!("unexpected first stop refusal: {other:?}"),
            })
        };
        support::join(reader);
        support::join(writer);
        support::join(keeper);
        // An incomplete stop is retried after joined activations exit, never
        // by unwrapping a contended try-acquire or assuming yield-count progress.
        let phase = runtime.state.lock().unwrap().admission;
        match phase {
            Admission::Closing => {
                let grace = match runtime.stop() {
                    Ok(grace) => grace,
                    Err(other) => panic!("joined owners did not quiesce: {other:?}"),
                };
                runtime.reclaim(grace);
            }
            Admission::Stopped => {}
            Admission::Running | Admission::Poisoned => panic!("stop did not close admissions"),
        }
        assert!(matches!(runtime.enter(OwnerSlot(1)), Err(Refusal::Closed)));
    });
}

#[test]
fn negative_reclaim_while_old_view_lives() {
    support::negative(3, "OLD_TABLE_RECLAIMED_BEFORE_GRACE", || {
        let runtime = runtime();
        let view = match runtime.enter(OwnerSlot(0)) {
            Ok(view) => view,
            Err(other) => panic!("fresh runtime refused: {other:?}"),
        };
        let writer = {
            let runtime = runtime.clone();
            thread::spawn(move || {
                let _ = runtime.replace();
                // Missing grace proof is the sole deliberately broken edge.
                runtime.state.lock().unwrap().tables[0] = TableState::Reclaimed;
            })
        };
        thread::yield_now();
        view.read();
        support::join(writer);
    });
}

#[test]
fn negative_ack_with_live_activation() {
    support::negative(3, "ACK_WITH_LIVE_JIT_VIEW", || {
        let runtime = runtime();
        let view = match runtime.enter(OwnerSlot(0)) {
            Ok(view) => view,
            Err(other) => panic!("fresh runtime refused: {other:?}"),
        };
        let keeper = {
            let runtime = runtime.clone();
            thread::spawn(move || {
                let mut state = runtime.state.lock().unwrap();
                state.admission = Admission::Stopped; // missing activation exclusion
                let no_live_view = state.owners.iter().all(|owner| *owner != Owner::Active);
                drop(state); // the witness must not poison teardown's registry
                assert!(no_live_view, "ACK_WITH_LIVE_JIT_VIEW");
            })
        };
        support::join(keeper);
        drop(view);
    });
}

#[test]
fn negative_resume_uses_saved_pre_grace_pointer() {
    support::negative(2, "RESUME_USES_PRE_GRACE_POINTER", || {
        let runtime = runtime();
        let view = match runtime.enter(OwnerSlot(0)) {
            Ok(view) => view,
            Err(other) => panic!("fresh runtime refused: {other:?}"),
        };
        let saved = view.table;
        drop(view); // broken compiler contract: saved address will be reused
        assert_eq!(runtime.replace(), Ok(()));
        let grace = match runtime.stop() {
            Ok(grace) => grace,
            Err(other) => panic!("quiescent runtime refused: {other:?}"),
        };
        runtime.reclaim(grace);
        assert!(
            runtime.state.lock().unwrap().tables[saved.0] != TableState::Reclaimed,
            "RESUME_USES_PRE_GRACE_POINTER"
        );
    });
}

#[test]
fn resume_reloads_descriptor_after_grace() {
    support::exhaustive(2, || {
        let runtime = runtime();
        let old = match runtime.enter(OwnerSlot(0)) {
            Ok(view) => view,
            Err(other) => panic!("fresh runtime refused: {other:?}"),
        };
        old.read();
        drop(old);
        assert_eq!(runtime.replace(), Ok(()));
        let grace = match runtime.stop() {
            Ok(grace) => grace,
            Err(other) => panic!("quiescent runtime refused: {other:?}"),
        };
        runtime.reclaim(grace);
        runtime.state.lock().unwrap().admission = Admission::Running;
        let resumed = match runtime.enter(OwnerSlot(0)) {
            Ok(view) => view,
            Err(other) => panic!("resumed runtime refused: {other:?}"),
        };
        assert_eq!(resumed.table, TableId(1));
        resumed.read();
    });
}

#[test]
fn starting_cancelled_native_and_retired_membership() {
    support::check(3, || {
        let runtime = runtime();
        runtime.state.lock().unwrap().owners[1] = Owner::Starting;
        let owner = {
            let runtime = runtime.clone();
            thread::spawn(move || {
                let mut state = runtime.state.lock().unwrap();
                state.owners[1] = match state.admission {
                    Admission::Running => Owner::Native,
                    Admission::Closing | Admission::Stopped => Owner::Cancelled,
                    Admission::Poisoned => Owner::Retired,
                };
            })
        };
        match runtime.stop() {
            Ok(grace) => runtime.reclaim(grace),
            Err(Refusal::LiveActivation) => {}
            Err(other) => panic!("unexpected stop refusal: {other:?}"),
        }
        support::join(owner);
        let phase = runtime.state.lock().unwrap().admission;
        if phase == Admission::Closing {
            match runtime.stop() {
                Ok(grace) => runtime.reclaim(grace),
                Err(other) => panic!("cancelled starter blocked stop: {other:?}"),
            }
        }
    });
}

#[test]
fn busy_entry_is_legal_and_never_unwrapped() {
    support::check(3, || {
        let runtime = runtime();
        let first = {
            let runtime = runtime.clone();
            thread::spawn(move || match runtime.enter(OwnerSlot(0)) {
                Ok(view) => view.read(),
                Err(Refusal::Busy) => {}
                Err(Refusal::Closed | Refusal::LiveActivation | Refusal::Poisoned) => {
                    panic!("running entry refused without contention")
                }
            })
        };
        match runtime.enter(OwnerSlot(0)) {
            Ok(view) => view.read(),
            Err(Refusal::Busy) => {}
            Err(Refusal::Closed | Refusal::LiveActivation | Refusal::Poisoned) => {
                panic!("running entry refused without contention")
            }
        }
        support::join(first);
        assert_eq!(runtime.state.lock().unwrap().owners[0], Owner::Ready);
    });
}

#[test]
fn native_handoff_and_terminal_shutdown_follow_activation_exit() {
    support::exhaustive(3, || {
        let runtime = runtime();
        let worker = {
            let runtime = runtime.clone();
            thread::spawn(move || match runtime.enter(OwnerSlot(0)) {
                Ok(view) => {
                    view.read();
                    drop(view);
                    runtime.state.lock().unwrap().owners[0] = Owner::Native;
                }
                Err(Refusal::Closed) => {}
                Err(Refusal::Busy | Refusal::LiveActivation | Refusal::Poisoned) => {
                    panic!("fresh handoff refused unexpectedly")
                }
            })
        };
        support::join(worker);
        runtime.state.lock().unwrap().owners[0] = Owner::Retired;
        let grace = match runtime.stop() {
            Ok(grace) => grace,
            Err(other) => panic!("terminal handoff failed to quiesce: {other:?}"),
        };
        runtime.reclaim(grace);
        let mut state = runtime.state.lock().unwrap();
        assert!(state.owners.iter().all(|owner| *owner != Owner::Active));
        // Terminal owner/backing teardown may now free CURRENT as well.
        state.tables = [TableState::Reclaimed; 2];
    });
}

#[test]
fn poison_retains_published_and_retired_allocations() {
    support::exhaustive(3, || {
        let runtime = runtime();
        assert_eq!(runtime.replace(), Ok(()));
        let poisoner = {
            let runtime = runtime.clone();
            thread::spawn(move || runtime.state.lock().unwrap().admission = Admission::Poisoned)
        };
        let reader = {
            let runtime = runtime.clone();
            thread::spawn(move || {
                if let Ok(view) = runtime.enter(OwnerSlot(0)) {
                    view.read();
                }
            })
        };
        support::join(poisoner);
        support::join(reader);
        assert!(matches!(runtime.stop(), Err(Refusal::Poisoned)));
        assert_eq!(
            runtime.state.lock().unwrap().tables,
            [TableState::Retired, TableState::Owned]
        );
    });
}

#[derive(Clone, Copy)]
enum RootTransfer {
    RootBeforeRelease,
    OmitRoot,
}

#[derive(Clone, Copy)]
enum DefinitionLife {
    Live,
    Swept,
}
struct Definitions {
    current: usize,
    life: [DefinitionLife; 2],
    roots: [usize; 2],
    function_epoch: usize,
}

fn selected_definition(transfer: RootTransfer) {
    let state = Arc::new(Mutex::new(Definitions {
        current: 0,
        life: [DefinitionLife::Live; 2],
        roots: [0; 2],
        function_epoch: 1,
    }));
    // Cell selection and root installation share the existing authority leaf.
    let selected = {
        let mut state = state.lock().unwrap();
        let selected = state.current;
        match transfer {
            RootTransfer::RootBeforeRelease => state.roots[selected] += 1,
            RootTransfer::OmitRoot => {}
        }
        selected
    };
    let writer = {
        let state = state.clone();
        thread::spawn(move || {
            let mut state = state.lock().unwrap();
            state.current = 1;
            state.function_epoch += 1; // fset's commit boundary
            if state.roots[0] == 0 {
                state.life[0] = DefinitionLife::Swept; // stopped sweep lacks old root
            }
        })
    };
    support::join(writer);
    let state = state.lock().unwrap();
    assert!(
        matches!(state.life[selected], DefinitionLife::Live),
        "UNROOTED_SELECTED_DEFINITION"
    );
    assert_eq!(state.current, 1); // next selection observes the new definition
    assert_eq!(state.function_epoch, 2); // cached epoch 1 must revalidate
}

#[test]
fn rooted_old_call_survives_and_next_call_revalidates() {
    support::exhaustive(3, || selected_definition(RootTransfer::RootBeforeRelease));
}

#[test]
fn negative_unrooted_function_selection() {
    support::negative(3, "UNROOTED_SELECTED_DEFINITION", || {
        selected_definition(RootTransfer::OmitRoot)
    });
}
