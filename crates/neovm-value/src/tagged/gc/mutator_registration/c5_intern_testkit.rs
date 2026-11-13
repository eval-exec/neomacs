//! C5 intern-growth test-support bridge (LEAD 05:3xZ; peer B2 host ruling).
//!
//! Value's `symbol_intern` histories are `cfg(test)`-only, so the real
//! intern-growth transaction has no dependency-build route.  The display
//! mode-line held-identity oracle and eval's intern boundary both need it.
//! This module is the test-support mirror of
//! `symbol_intern/tests/histories.rs` (the same exercise/assert code, minus
//! the evaluator-only `Context` constructors and the JIT epoch attribution,
//! which the eval-side AUTHORITY fence supplies as a per-actor probe).  It is
//! compiled only when value is a dependency test-support build; the cfg(test)
//! original stays the home of value's own unit tests until the deferred C5
//! intern split consolidates the two.
//!
//! Every entry point keeps the original transaction: real owners, real root
//! homes, real stopped heap setup and retirement before the copied receipt
//! returns.  No World view, fact struct or interior reference escapes.

use super::EraKeeper;
use super::symbol_functions::retry::{self as retry_protocol, Deadline, Retry, retry_owned};
use crate::intern::SymId;
use crate::intern::parallel::name_publication_for_test;
use crate::symbol::parallel::InitialInternFacts;
use crate::tagged::gc::{
    FunctionError, GlobalError, HeapIdentity, InitialInternError, Mutator, MutatorPermit,
    OwnerRootSlot, PlainSymbolBacking, ProofEra as ParallelHeapEra, RegistrationError, RunAccess,
    SymbolHandle,
};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::sync::Barrier;
use std::sync::atomic::Ordering;

/// Per-actor function-epoch observation hook owned by the upper layer.
///
/// Value has no evaluator counter type and never inspects the value the probe
/// returns; the test-support caller supplies one that reads its own thread's
/// monotonic counter, so any bump is attributed to the committing actor.
/// `None` is the historic non-jit shape (the check then sees zero deltas).
pub(crate) type EpochProbe = fn() -> u64;

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
    fn retry(&self) -> Retry {
        match self {
            Self::Control(error) => retry_protocol::registration(error),
            Self::Global(error) => retry_protocol::global(error),
            Self::Intern(error) => retry_protocol::initial_intern(error),
            Self::Function(error) => retry_protocol::function(error),
            Self::Fixture(_) => Retry::Refuse,
        }
    }
    fn explanation(&self) -> String {
        match self {
            Self::Fixture(message) => (*message).to_owned(),
            Self::Control(_) | Self::Global(_) | Self::Intern(_) | Self::Function(_) => {
                format!("{self:?}")
            }
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
    retry_protocol::retry(
        Deadline::contention(),
        || attempt_once_observed(owner, &mut body, &mut refused),
        AttemptFailure::retry,
    )
    .map_err(|failure| failure.explanation(owner, operation))
}

fn attempt_once_observed<'era, T>(
    owner: &mut Mutator<'era>,
    body: &mut impl FnMut(&mut RunAccess<'_, 'era>) -> Result<T, Failure>,
    refused: &mut impl FnMut(&Mutator<'era>, &Failure) -> Result<(), String>,
) -> Result<T, AttemptFailure> {
    let result = match owner.run_access() {
        Ok(mut run) => body(&mut run),
        Err(error) => Err(error.into()),
    };
    // The failed RunAccess has ended before observation or yielding.
    if let Err(error) = &result {
        if let Retry::Busy = error.retry() {
            refused(owner, error).map_err(AttemptFailure::Observation)?;
        }
    }
    result.map_err(AttemptFailure::Operation)
}

enum AttemptFailure {
    Operation(Failure),
    Observation(String),
}

impl AttemptFailure {
    fn retry(&self) -> Retry {
        match self {
            Self::Operation(error) => error.retry(),
            Self::Observation(_) => Retry::Refuse,
        }
    }

    fn explanation(self, owner: &Mutator<'_>, operation: &str) -> String {
        match self {
            Self::Operation(error) => format!(
                "C5 history {operation}: {}; {}",
                error.explanation(),
                diagnostics(owner),
            ),
            Self::Observation(message) => message,
        }
    }
}

fn activate<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
) -> Result<Mutator<'era>, String> {
    retry_owned(
        Deadline::contention(),
        permit,
        |permit| permit.activate(heap, id),
        |failed| match retry_protocol::registration(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_permit()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| format!("C5 history activation: {failed:?}"))
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
    retry_owned(
        Deadline::contention(),
        owner,
        |owner| owner.retire_rooted(&[]),
        |failed| match retry_protocol::rendezvous(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_owner()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| format!("C5 history retirement: {failed:?}"))?;
    cleared.and(outcome)
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
    function_bumps: u64,
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
    preparation_charge: usize,
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

fn perform<'era>(
    owner: &mut Mutator<'era>,
    mut homes: ActorHomes<'era>,
    epoch_probe: Option<EpochProbe>,
) -> Result<Row, String> {
    let function_before = epoch_probe.map(|probe| probe());
    let id = attempt(owner, "initial_intern_into", |run| {
        run.initial_intern_into(&homes.name, &mut homes.dst)?;
        // SAFETY: the destination home was just rooted by the real intern
        // transaction; the fixture asserts it round-trips to a bare symbol.
        unsafe { Value::from_bits(homes.dst.word_for_test(run)) }
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
        function_bumps: match (epoch_probe, function_before) {
            (Some(probe), Some(before)) => probe().saturating_sub(before),
            _ => 0,
        },
    })
}

/// Preparation/observer owners retire outside the racing extent. Exactly N
/// real actors, N=2..4, overlap. Even failed setup reaches the start barrier;
/// after it, no wait depends on an operation succeeding. All homes and owners
/// are cleaned before returning either the receipt or an error to the keeper.
fn exercise_with_relocation<K: EraKeeper>(
    era: &ParallelHeapEra<'_, K>,
    inputs: &[SymId],
    scenario: Scenario,
    relocate: bool,
    epoch_probe: Option<EpochProbe>,
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
    let registry_before = crate::intern::parallel::identity_census_for_test();
    let preparation_charge = if relocate {
        force_spine_move(era, handles[0], &nil, inputs[0], &before)?
    } else {
        0
    };
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
                    Ok(owner) => guarded(owner, |owner| perform(owner, seeded?, epoch_probe)),
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
    let registry_after = crate::intern::parallel::identity_census_for_test();
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
                preparation_charge,
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
        preparation_charge,
    } = receipt;
    assert!(
        rows.iter().all(|row| row.function_bumps == 0),
        "intern, repeated intern and soft lookup contribute no function-epoch reason"
    );
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
    assert!(
        rows.iter().map(|row| row.charge).sum::<usize>() + preparation_charge
            >= after.bytes - before.bytes
    );
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

fn force_spine_move<K: EraKeeper>(
    era: &ParallelHeapEra<'_, K>,
    holder: SymbolHandle,
    nil: &SharedRoot,
    id: SymId,
    before: &InitialInternFacts,
) -> Result<usize, String> {
    use crate::symbol::parallel::InitialInternFault;
    guarded(
        activate(
            era.register(9100, 3)
                .map_err(|e| format!("C5 relocation permit: {e:?}"))?,
            era.heap_identity(),
            9100,
        )?,
        |owner| {
            let mut homes = seed(owner, &holder, nil)?;
            attempt(owner, "arm spine relocation", |run| {
                Ok(run.initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove)?)
            })?;
            let refused = attempt(owner, "clean relocating refusal", |run| {
                match run.initial_intern_into(&homes.name, &mut homes.dst) {
                    Err(InitialInternError::Authority(RegistrationError::Busy)) => {
                        Err(RegistrationError::Busy.into())
                    }
                    outcome => Ok(outcome),
                }
            })?;
            let (after, charge) = attempt(owner, "relocation facts", |run| {
                Ok((
                    run.initial_facts_for_test(id)?,
                    run.owner
                        .local
                        .as_ref()
                        .ok_or(Failure::Fixture("relocation owner absent"))?
                        .hot
                        .bytes_since_gc,
                ))
            })?;
            if refused != Err(InitialInternError::Authority(RegistrationError::Capacity))
                || before.spine_address == after.spine_address
                || !after.spine_current
                || (
                    before.members,
                    before.epoch,
                    before.slots,
                    before.chunks,
                    before.sides,
                ) != (
                    after.members,
                    after.epoch,
                    after.slots,
                    after.chunks,
                    after.sides,
                )
            {
                return Err(format!(
                    "C5 clean relocation facts mismatch: {refused:?}; {before:?} -> {after:?}"
                ));
            }
            eprintln!("P3SYM_C5_SERIAL_RELOCATION_REFUSAL capacity=1 moved=1 logical_change=0");
            Ok(charge)
        },
    )
}

/// Copied C5 growth receipt: plain scalars only. The signed value fence
/// re-exports this type; the testkit module itself stays crate-private so the
/// fence is the item's only public path.
pub struct C5InternGrowthReceipt {
    before_chunks: usize,
    before_sides: usize,
    before_spine_address: usize,
    after_chunks: usize,
    after_sides: usize,
    after_spine_address: usize,
    winner: SymId,
    registry_before: (u64, usize),
    registry_after: (u64, usize),
}

impl C5InternGrowthReceipt {
    pub fn before_chunks(&self) -> usize {
        self.before_chunks
    }
    pub fn before_sides(&self) -> usize {
        self.before_sides
    }
    pub fn before_spine_address(&self) -> usize {
        self.before_spine_address
    }
    pub fn after_chunks(&self) -> usize {
        self.after_chunks
    }
    pub fn after_sides(&self) -> usize {
        self.after_sides
    }
    pub fn after_spine_address(&self) -> usize {
        self.after_spine_address
    }
    pub fn winner(&self) -> SymId {
        self.winner
    }
    pub fn registry_before(&self) -> (u64, usize) {
        self.registry_before
    }
    pub fn registry_after(&self) -> (u64, usize) {
        self.registry_after
    }
}

/// Only existing input-holder identities enter. All owners and operand homes
/// retire before the receipt or assertion outcome returns.
fn c5_growth_receipt<K: EraKeeper>(
    era: &ParallelHeapEra<'_, K>,
    holders: [SymId; 2],
    relocate: bool,
    epoch_probe: Option<EpochProbe>,
) -> Result<C5InternGrowthReceipt, String> {
    let receipt =
        exercise_with_relocation(era, &holders, Scenario::SameName, relocate, epoch_probe)?;
    // Return failures to the serial caller rather than unwinding across the
    // era's finish capabilities. No formatter/Context enters this bridge.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_history(&receipt, Scenario::SameName, None, true);
    }))
    .map_err(|_| "C5 growth receipt assertion after owner retirement".to_owned())?;
    Ok(C5InternGrowthReceipt {
        before_chunks: receipt.before.chunks,
        before_sides: receipt.before.sides,
        before_spine_address: receipt.before.spine_address,
        after_chunks: receipt.after.chunks,
        after_sides: receipt.after.sides,
        after_spine_address: receipt.after.spine_address,
        winner: receipt.rows[0].id,
        registry_before: receipt.registry_before,
        registry_after: receipt.registry_after,
    })
}

/// C5 growth bridge: the real two-actor intern-growth transaction on `era`.
pub(crate) fn c5_intern_growth_for_test<K: EraKeeper>(
    era: &ParallelHeapEra<'_, K>,
    holders: [SymId; 2],
    epoch_probe: Option<EpochProbe>,
) -> Result<C5InternGrowthReceipt, String> {
    c5_growth_receipt(era, holders, false, epoch_probe)
}

/// C5 relocating bridge: first force the qualified clean reservation refusal
/// to replace the pointer spine; its reserved capacity covers the later win.
pub(crate) fn c5_intern_relocating_for_test<K: EraKeeper>(
    era: &ParallelHeapEra<'_, K>,
    holders: [SymId; 2],
    epoch_probe: Option<EpochProbe>,
) -> Result<C5InternGrowthReceipt, String> {
    c5_growth_receipt(era, holders, true, epoch_probe)
}

fn diagnostics(owner: &Mutator<'_>) -> String {
    format!(
        "owner={owner:?} authority_ready={:?} admission={} phase={}",
        owner.registry.symbols.ready(),
        owner.entry.admission.load(Ordering::Acquire),
        owner.registry.phase.load()
    )
}
