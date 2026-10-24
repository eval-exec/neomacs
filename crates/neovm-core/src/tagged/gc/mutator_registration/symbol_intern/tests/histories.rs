//! Real finite-attempt histories. Every actor uses its own rooted operands;
//! no Value, cell reference or Context view crosses an actor boundary.
use super::*;
use crate::emacs_core::intern::{SymId, parallel::name_publication_for_test};
use crate::emacs_core::symbol::parallel::InitialInternFacts;
use crate::tagged::gc::{
    FunctionError, GlobalError, HeapIdentity, InitialInternError, MutatorPermit, ParallelHeapEra,
    RegistrationError, SymbolHandle,
};
use contention::RetryBudget;
use std::collections::BTreeSet;
use std::sync::Barrier;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    SameObject,
    SameName,
    DifferentNames,
}
#[derive(Debug)]
enum Failure {
    Control(RegistrationError),
    Global(GlobalError),
    Intern(InitialInternError),
    Function(FunctionError),
    Fixture(&'static str),
}
impl From<RegistrationError> for Failure {
    fn from(error: RegistrationError) -> Self {
        Self::Control(error)
    }
}
impl From<GlobalError> for Failure {
    fn from(error: GlobalError) -> Self {
        Self::Global(error)
    }
}
impl From<InitialInternError> for Failure {
    fn from(error: InitialInternError) -> Self {
        Self::Intern(error)
    }
}
impl From<FunctionError> for Failure {
    fn from(error: FunctionError) -> Self {
        Self::Function(error)
    }
}
impl Failure {
    fn busy(&self) -> bool {
        matches!(
            self,
            Self::Control(RegistrationError::Busy)
                | Self::Global(GlobalError::Authority(RegistrationError::Busy))
                | Self::Intern(InitialInternError::Authority(RegistrationError::Busy))
                | Self::Function(FunctionError::Authority(RegistrationError::Busy))
        )
    }
    fn explanation(&self) -> String {
        match self {
            Self::Fixture(message) => (*message).to_owned(),
            other => format!("{other:?}"),
        }
    }
}

fn attempt<'era, T>(
    owner: &mut Mutator<'era>,
    operation: &str,
    body: impl FnMut(&mut RunAccess<'_, 'era>) -> Result<T, Failure>,
) -> Result<T, String> {
    attempt_observed(owner, operation, body, |_, _| Ok(()))
}

fn attempt_observed<'era, T>(
    owner: &mut Mutator<'era>,
    operation: &str,
    mut body: impl FnMut(&mut RunAccess<'_, 'era>) -> Result<T, Failure>,
    mut refused: impl FnMut(&Mutator<'era>, &Failure) -> Result<(), String>,
) -> Result<T, String> {
    let mut budget = RetryBudget::new();
    loop {
        let result = match owner.run_access() {
            Ok(mut run) => body(&mut run),
            Err(error) => Err(error.into()),
        };
        match result {
            Err(error) if error.busy() => {
                refused(owner, &error)?;
                budget
                    .wait(operation, &error)
                    .map_err(|error| format!("{error}; {}", contention::diagnostics(owner)))?;
            }
            result => {
                return result
                    .map_err(|error| format!("C5 history {operation}: {}", error.explanation()));
            }
        }
    }
}

fn activate<'era>(
    mut permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
) -> Result<Mutator<'era>, String> {
    let mut budget = RetryBudget::new();
    loop {
        match permit.activate(heap, id) {
            Ok(owner) => return Ok(owner),
            Err(failed) if failed.reason == RegistrationError::Busy => {
                permit = failed.into_permit();
                budget.wait("activate", &RegistrationError::Busy)?;
            }
            Err(failed) => return Err(format!("C5 history activation: {failed:?}")),
        }
    }
}

fn guarded<'era, T>(
    mut owner: Mutator<'era>,
    body: impl FnOnce(&mut Mutator<'era>) -> Result<T, String>,
) -> Result<T, String> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut owner)))
        .unwrap_or_else(|_| Err("C5 history assertion after start".into()));
    let cleared = attempt(&mut owner, "clear_root_slots", |run| {
        Ok(run.clear_root_slots()?)
    });
    let mut budget = RetryBudget::new();
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return cleared.and(outcome),
            Err(failed) if failed.reason == RendezvousError::Busy => {
                owner = failed.into_owner();
                budget.wait("retire_rooted", &RendezvousError::Busy)?;
            }
            Err(failed) => return Err(format!("C5 history retirement: {failed:?}")),
        }
    }
}

struct ActorHomes<'era> {
    name: OwnerRootSlot<'era>,
    dst: OwnerRootSlot<'era>,
    input: usize,
    soft_before: usize,
}
#[derive(Debug)]
struct Row {
    id: SymId,
    input: usize,
    soft_before: usize,
    soft_after: usize,
    repeated: usize,
    initially_unbound: bool,
    function_nil: bool,
    charge: usize,
}
#[derive(Debug)]
struct Receipt {
    rows: Vec<Row>,
    before: InitialInternFacts,
    after: InitialInternFacts,
    cells: Vec<InitialInternFacts>,
    names: Vec<(Option<usize>, bool, bool, bool)>,
    gaps_absent: bool,
    registry_before: (u64, usize),
    registry_after: (u64, usize),
}

fn seed<'era>(
    owner: &mut Mutator<'era>,
    holder: &SymbolHandle,
    nil: &SharedRoot,
) -> Result<ActorHomes<'era>, String> {
    let mut name = attempt(owner, "root_slot(name)", |run| Ok(run.root_slot(nil)?))?;
    let mut dst = attempt(owner, "root_slot(dst)", |run| Ok(run.root_slot(nil)?))?;
    let input = attempt(owner, "sym_value_into(input)", |run| {
        run.sym_value_into(holder, &mut name)?;
        Ok(name.word_for_test(run))
    })?;
    let soft_before = attempt(owner, "initial_intern_soft_into(before)", |run| {
        run.initial_intern_soft_into(&name, &mut dst)?;
        Ok(dst.word_for_test(run))
    })?;
    Ok(ActorHomes {
        name,
        dst,
        input,
        soft_before,
    })
}

fn perform<'era>(owner: &mut Mutator<'era>, mut homes: ActorHomes<'era>) -> Result<Row, String> {
    let id = attempt(owner, "initial_intern_into", |run| {
        run.initial_intern_into(&homes.name, &mut homes.dst)?;
        Value::from_bits(homes.dst.word_for_test(run))
            .as_bare_symbol_id()
            .ok_or(Failure::Fixture("intern did not root a bare symbol"))
    })?;
    let handle = attempt(owner, "initial_symbol_handle", |run| {
        Ok(run.initial_symbol_handle(&homes.dst)?)
    })?;
    let initially_unbound = attempt(owner, "sym_boundp_into", |run| {
        run.sym_boundp_into(&handle, &mut homes.dst)?;
        Ok(homes.dst.word_for_test(run) == Value::NIL.bits())
    })?;
    let function_nil = attempt(owner, "sym_function_into", |run| {
        run.sym_function_into(&handle, &mut homes.dst)?;
        Ok(homes.dst.word_for_test(run) == Value::NIL.bits())
    })?;
    let repeated = attempt(owner, "initial_intern_into(repeat)", |run| {
        run.initial_intern_into(&homes.name, &mut homes.dst)?;
        Ok(homes.dst.word_for_test(run))
    })?;
    let (soft_after, charge) = attempt(owner, "initial_intern_soft_into(after)", |run| {
        run.initial_intern_soft_into(&homes.name, &mut homes.dst)?;
        Ok((
            homes.dst.word_for_test(run),
            run.owner
                .local
                .as_ref()
                .ok_or(Failure::Fixture("owner absent"))?
                .hot
                .bytes_since_gc,
        ))
    })?;
    Ok(Row {
        id,
        input: homes.input,
        soft_before: homes.soft_before,
        soft_after,
        repeated,
        initially_unbound,
        function_nil,
        charge,
    })
}

/// Preparation/observer owners retire outside the racing extent. Exactly N
/// real actors, N=2..4, overlap. Even failed setup reaches the start barrier;
/// after it, no wait depends on an operation succeeding. All homes and owners
/// are cleaned before returning either the receipt or an error to the keeper.
fn exercise(
    era: &ParallelHeapEra<'_>,
    inputs: &[SymId],
    scenario: Scenario,
) -> Result<Receipt, String> {
    if !(2..=4).contains(&inputs.len()) {
        return Err("C5 history actor count".into());
    }
    let handles: Vec<_> = inputs
        .iter()
        .map(|&id| {
            era.admit_plain_symbol(id)
                .map_err(|e| format!("C5 input admission: {e:?}"))
        })
        .collect::<Result<_, _>>()?;
    let mut stopped = era.stop().map_err(|e| format!("C5 seed stop: {e:?}"))?;
    let nil = era
        .with_stopped_heap_for_test(&stopped, |heap| unsafe {
            // SAFETY: matching stopped heap and one immediate seed, not a name.
            SharedRoot::new(heap, Value::NIL)
        })
        .map_err(|e| format!("C5 seed heap: {e:?}"))?;
    stopped
        .trace(|_, _| Ok(()))
        .map_err(|e| format!("C5 seed trace: {e:?}"))?;
    stopped
        .complete()
        .map_err(|e| format!("C5 seed complete: {e:?}"))?;
    let heap = era.heap_identity();
    let before = guarded(
        activate(
            era.register(9000, 1)
                .map_err(|e| format!("C5 observer permit: {e:?}"))?,
            heap,
            9000,
        )?,
        |owner| {
            attempt(owner, "initial_facts_for_test(before)", |run| {
                Ok(run.initial_facts_for_test(inputs[0])?)
            })
        },
    )?;
    let registry_before = crate::emacs_core::intern::parallel::identity_census_for_test();
    let permits = (0..inputs.len())
        .map(|index| {
            era.register(index as u64 + 1, 3)
                .map_err(|e| format!("C5 actor permit: {e:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ready = Barrier::new(inputs.len());
    let rows = std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        for (index, permit) in permits.into_iter().enumerate() {
            let (ready, nil) = (&ready, &nil);
            let holder = handles[if scenario == Scenario::SameObject {
                0
            } else {
                index
            }];
            jobs.push(scope.spawn(move || {
                let mut owner = activate(permit, heap, index as u64 + 1);
                // No assertion/error can strand another actor at the barrier.
                let seeded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match owner
                    .as_mut()
                {
                    Ok(owner) => seed(owner, &holder, nil),
                    Err(error) => Err(error.clone()),
                }))
                .unwrap_or_else(|_| Err("C5 actor setup assertion".into()));
                ready.wait();
                match owner {
                    Ok(owner) => guarded(owner, |owner| perform(owner, seeded?)),
                    Err(error) => Err(error),
                }
            }));
        }
        jobs.into_iter()
            .map(|job| {
                job.join()
                    .map_err(|_| "C5 actor join assertion".to_string())
                    .and_then(|row| row)
            })
            .collect::<Result<Vec<_>, _>>()
    });
    let rows = rows?;
    let registry_after = crate::emacs_core::intern::parallel::identity_census_for_test();
    let receipt = guarded(
        activate(
            era.register(9001, 1)
                .map_err(|e| format!("C5 final permit: {e:?}"))?,
            heap,
            9001,
        )?,
        |owner| {
            let mut cells = Vec::new();
            let mut names = Vec::new();
            let mut gaps_absent = true;
            for (index, row) in rows.iter().enumerate() {
                cells.push(attempt(owner, "initial_facts_for_test(after)", |run| {
                    Ok(run.initial_facts_for_test(row.id)?)
                })?);
                // Every forced-gap fixture leaves the slot immediately before its
                // FIRST winner absent. Later different-name winners are contiguous.
                let first = rows.iter().map(|row| row.id.0).min().ok_or("C5 no rows")?;
                if first > 0 {
                    gaps_absent &= !attempt(owner, "initial_facts_for_test(gap)", |run| {
                        Ok(run.initial_facts_for_test(SymId(first - 1))?)
                    })?
                    .present;
                }
                names.push(name_publication_for_test(
                    heap,
                    row.id,
                    rows[(index + 1) % rows.len()].input,
                ));
            }
            let after = cells.first().copied().ok_or("C5 no cells")?;
            Ok(Receipt {
                rows,
                before,
                after,
                cells,
                names,
                gaps_absent,
                registry_before,
                registry_after,
            })
        },
    );
    drop(nil);
    receipt
}

fn assert_history(
    receipt: &Receipt,
    scenario: Scenario,
    kind: Option<PlainSymbolBacking>,
    growth: bool,
) {
    let Receipt {
        rows,
        before,
        after,
        cells,
        names,
        gaps_absent,
        registry_before,
        registry_after,
    } = receipt;
    let ids: BTreeSet<_> = rows.iter().map(|row| row.id.0).collect();
    let inputs: BTreeSet<_> = rows.iter().map(|row| row.input).collect();
    let expected = if scenario == Scenario::DifferentNames {
        rows.len()
    } else {
        1
    };
    assert_eq!(ids.len(), expected, "one canonical symbol per spelling");
    assert_eq!(
        inputs.len(),
        if scenario == Scenario::SameObject {
            1
        } else {
            rows.len()
        }
    );
    assert_eq!(
        (after.members, after.epoch),
        (before.members + expected, before.epoch + expected as u64)
    );
    assert_eq!(
        *registry_after,
        (
            registry_before.0 + expected as u64,
            registry_before.1 + expected
        ),
        "exactly one process ID/epoch per winning spelling, none for losing/hit/soft operations"
    );
    assert_eq!(after.chunks, after.sides);
    assert_eq!(
        after.projections, before.projections,
        "new projected names reuse shared VOID"
    );
    assert!(rows.iter().all(|row| row.soft_before == Value::NIL.bits()
        && row.initially_unbound
        && row.function_nil));
    assert!(
        rows.iter()
            .all(|row| row.soft_after == Value::from_sym_id(row.id).bits()
                && row.repeated == row.soft_after)
    );
    assert!(rows.iter().map(|row| row.charge).sum::<usize>() >= after.bytes - before.bytes);
    for (index, (cell, name)) in cells.iter().zip(names).enumerate() {
        assert!(cell.present && cell.member && cell.spine_current);
        if let Some(kind) = kind {
            assert_eq!(cell.backing, Some(kind));
        }
        assert_eq!(
            cell.cells,
            Some((Value::NIL.bits(), Value::NIL.bits(), false, false))
        );
        assert!(
            name.1 && name.3,
            "winning name is exactly rooted; atom has no heap properties"
        );
        let winner = name.0.expect("exact name belongs to the admitted heap");
        if scenario == Scenario::DifferentNames {
            assert_eq!(winner, rows[index].input);
            assert!(name.2, "another successful spelling has its own exact root");
        } else {
            assert!(inputs.contains(&winner));
            assert_eq!(name.2, rows[(index + 1) % rows.len()].input == winner);
            assert!(names.iter().all(|name| name.0 == Some(winner)));
        }
    }
    if growth {
        assert!(
            after.slots > before.slots && after.chunks >= before.chunks + 2,
            "actual retained chunk/side growth required"
        );
        assert!(*gaps_absent, "registry gaps remain absent in this World");
    }
    eprintln!(
        "P3SYM_C5_NATIVE_HISTORY {scenario:?} owners={} winners={} members_added={} chunks_added={}",
        rows.len(),
        ids.len(),
        after.members - before.members,
        after.chunks - before.chunks
    );
}

fn histories(scenario: Scenario) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        for count in 2..=4 {
            let prefix = format!("p3sym-c5-history-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            let spellings: Vec<_> = (0..count)
                .map(|index| {
                    format!(
                        "{prefix}-{}",
                        if scenario == Scenario::DifferentNames {
                            index
                        } else {
                            0
                        }
                    )
                })
                .collect();
            for spelling in &spellings {
                assert!(crate::emacs_core::intern::lookup_interned(spelling).is_none());
            }
            let receipt =
                Context::with_test_symbol_intern_race_era(kind, &spellings, |era, ids| {
                    let result = exercise(&era, &ids[1..], scenario);
                    let (collector, keeper) = era
                        .finish()
                        .expect("C5 history after all roots/owners retired");
                    drop(collector);
                    drop(keeper);
                    eprintln!("P3SYM_C5_NATIVE_TEARDOWN_OK {scenario:?} {kind:?} owners={count}");
                    result
                })
                .expect("C5 history after clean teardown");
            assert_history(&receipt, scenario, Some(kind), true);
        }
    }
}

#[test]
fn p3sym_c5_two_to_four_owners_share_one_name_object_winner() {
    histories(Scenario::SameObject);
}
#[test]
fn p3sym_c5_two_to_four_owners_equal_name_objects_choose_one_winner() {
    histories(Scenario::SameName);
}
#[test]
fn p3sym_c5_two_to_four_owners_grow_distinct_names_without_filling_gaps() {
    histories(Scenario::DifferentNames);
}

mod assignment;
mod boundary;
mod contention;
mod serial;
