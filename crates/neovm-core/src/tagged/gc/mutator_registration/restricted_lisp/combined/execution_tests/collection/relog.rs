//! Trusted native mutation of one private cons; never a tape capability.
//! This covers the union of dirty-card and recorded-cons child tracing.
use super::fresh::{Cycle, Placement, Preparation, dispatch_role, with_allocation};
use super::minor_adapter::{Age, age};
use super::*;
use crate::tagged::gc::{OwnerRootSlot, TaggedHeap};

enum Extension {
    None,
    MutableOwner,
}
impl Extension {
    fn for_worker(scenario: Scenario, role: Role) -> Self {
        match (scenario, role) {
            (Scenario::Relog | Scenario::RelogNoop | Scenario::RelogUnwind, Role::Publisher) => {
                Self::MutableOwner
            }
            (
                Scenario::Relog | Scenario::RelogNoop | Scenario::RelogUnwind,
                Role::Caller | Role::Observer,
            )
            | (
                Scenario::Transfer
                | Scenario::Noop
                | Scenario::Unwind
                | Scenario::MinorAdapter
                | Scenario::Fresh
                | Scenario::FreshNoop
                | Scenario::FreshUnwind
                | Scenario::NameMinor
                | Scenario::NameMinorNoop
                | Scenario::NameMinorUnwind,
                Role::Publisher | Role::Caller | Role::Observer,
            ) => Self::None,
        }
    }
}
/// Conversion to registration's scalar root budget occurs only at its boundary.
pub(super) struct ReservedHomes(usize);
impl ReservedHomes {
    pub(super) fn for_worker(
        frame: executor::FrameRootCapacity,
        scenario: Scenario,
        role: Role,
    ) -> Self {
        let base = frame.as_usize();
        let homes = match Extension::for_worker(scenario, role) {
            Extension::None => base,
            Extension::MutableOwner => base.checked_add(1).unwrap(),
        };
        Self(homes)
    }
    pub(super) fn as_usize(&self) -> usize {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnerCons(WordIdentity);
#[derive(Clone, Copy, Debug)]
pub(super) struct Replacement {
    owner: OwnerCons,
    discarded: WordIdentity,
    kept: WordIdentity,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Facts {
    Owner(OwnerCons),
    Replaced(Replacement),
}
#[must_use = "drop the extra handle before the fixture clears the actual bank"]
pub(super) enum Mutation<'era> {
    Disabled,
    Reserved(OwnerRootSlot<'era>),
    Ready {
        home: OwnerRootSlot<'era>,
        owner: OwnerCons,
    },
}
impl<'era> Mutation<'era> {
    pub(super) fn seed(scenario: Scenario, role: Role, owner: &mut Mutator<'era>) -> Self {
        match Extension::for_worker(scenario, role) {
            Extension::MutableOwner => {
                let home = contention::retry(
                    Deadline::contention(),
                    || owner.run_access()?.private_nil_root(),
                    contention::registration,
                )
                .unwrap();
                Self::Reserved(home)
            }
            Extension::None => Self::Disabled,
        }
    }
    pub(super) fn initialize(
        &mut self,
        preparation: &Preparation,
        worker: &mut Worker<'era>,
    ) -> Facts {
        let mut home = match self {
            Self::Reserved(home) => home.clone(),
            Self::Disabled | Self::Ready { .. } => panic!("private cons initialization phase"),
        };
        let atoms = preparation.atoms();
        let word = with_allocation(&mut worker.owner, |run| {
            run.cons_into(&atoms.nil, &atoms.nil, &mut home)?;
            Ok(WordIdentity(home.word_for_test(run)))
        });
        let owner = OwnerCons(word);
        *self = Self::Ready { home, owner };
        Facts::Owner(owner)
    }
    pub(super) fn replace(
        &mut self,
        preparation: &Preparation,
        worker: &mut Worker<'era>,
    ) -> Facts {
        let (owner_home, owner) = match self {
            Self::Ready { home, owner } => (home.clone(), *owner),
            Self::Disabled | Self::Reserved(_) => {
                panic!("private cons replacement before initialization")
            }
        };
        let frame = match &worker.frame {
            FrameState::Empty => panic!("replacement before frame seed"),
            FrameState::Ready(frame) => frame,
        };
        let mut discarded = frame.home(home(0)).unwrap().clone();
        let mut kept = frame.home(home(1)).unwrap().clone();
        let atoms = preparation.atoms();
        let facts = with_allocation(&mut worker.owner, |run| {
            run.cons_into(&atoms.answer, &atoms.nil, &mut discarded)?;
            run.cons_into(&atoms.answer, &atoms.nil, &mut kept)?;
            let facts = Replacement {
                owner,
                discarded: WordIdentity(discarded.word_for_test(run)),
                kept: WordIdentity(kept.word_for_test(run)),
            };
            // The normal Result exit lets the wrapper finish the slice. No
            // deliberate unwind is injected into an unfinished slice.
            run.with_slice(|slice| -> Result<(), RegistrationError> {
                let owner = slice.bind(&owner_home)?;
                let discarded = slice.bind(&discarded)?;
                let kept = slice.bind(&kept)?;
                slice.store_car(&owner, &discarded)?;
                slice.store_car(&owner, &kept)?;
                Ok(())
            })??;
            run.clear_owner_root_for_test(&discarded)?;
            run.clear_owner_root_for_test(&kept)?;
            Ok(facts)
        });
        Facts::Replaced(facts)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Card {
    Clean,
    Dirty,
}
#[derive(Debug)]
enum OwnerSnapshot {
    Missing,
    Cons {
        age: Age,
        mirror: Age,
        card: Card,
        child: WordIdentity,
    },
}
fn owner_snapshot(heap: &TaggedHeap, owner: OwnerCons) -> OwnerSnapshot {
    let value = TaggedValue::from_bits(owner.0.0);
    let owner_age = age(heap, owner.0);
    if owner_age == Age::Missing || !value.is_cons() {
        return OwnerSnapshot::Missing;
    }
    let mirror = match heap.cons_age_is_old_for_test(value) {
        true => Age::Old,
        false => Age::Young,
    };
    let card = match heap.owner_card_is_dirty_for_test(value) {
        true => Card::Dirty,
        false => Card::Clean,
    };
    // SAFETY: the genuine stopped heap owns this exact live cons (checked by
    // age before payload access); no owner can mutate it across this read.
    let child = unsafe { (*value.gc_cons_ptr()).load_car() };
    OwnerSnapshot::Cons {
        age: owner_age,
        mirror,
        card,
        child: WordIdentity(child.bits()),
    }
}
#[derive(Clone, Copy, Debug)]
enum Observation {
    Warm(OwnerCons),
    Replacement(Replacement),
    Final {
        previous: WordIdentity,
        current: Replacement,
    },
}
impl Observation {
    fn owner(self) -> OwnerCons {
        match self {
            Self::Warm(owner) => owner,
            Self::Replacement(facts) | Self::Final { current: facts, .. } => facts.owner,
        }
    }
}
#[derive(Debug)]
struct Snapshot {
    owner: OwnerSnapshot,
    children: Vec<Age>,
}
fn snapshot(heap: &TaggedHeap, observation: Observation) -> Snapshot {
    let children = match observation {
        Observation::Warm(_) => Vec::new(),
        Observation::Replacement(facts) => [facts.discarded, facts.kept]
            .map(|word| age(heap, word))
            .to_vec(),
        Observation::Final { previous, current } => [previous, current.discarded, current.kept]
            .map(|word| age(heap, word))
            .to_vec(),
    };
    Snapshot {
        owner: owner_snapshot(heap, observation.owner()),
        children,
    }
}
fn expect_owner(
    snapshot: &Snapshot,
    expected_age: Age,
    expected_card: Card,
    expected_child: WordIdentity,
) {
    match snapshot.owner {
        OwnerSnapshot::Cons {
            age,
            mirror,
            card,
            child,
        } => {
            assert_eq!(age, expected_age);
            assert_eq!(
                mirror, expected_age,
                "canonical old bit and barrier age mirror agree"
            );
            assert_eq!(card, expected_card);
            assert_eq!(child, expected_child);
        }
        OwnerSnapshot::Missing => panic!("private owner cons unexpectedly unallocated"),
    }
}
struct Receipt {
    epoch: CollectionEpoch,
    cycle: CollectionCycle,
}
fn round(
    era: &ParallelHeapEra<'_>,
    cohort: Cohort,
    placement: &RefCell<Placement>,
    observation: Observation,
) -> Receipt {
    let mut stopped = contention::retry(
        Deadline::contention(),
        || era.stop(),
        contention::rendezvous,
    )
    .unwrap();
    let mut placement = placement.borrow_mut();
    placement.return_for_finish(era, &stopped);
    let regular = placement.cycle();
    let cycle = match observation {
        Observation::Warm(_) | Observation::Replacement(_) => regular,
        Observation::Final { .. } => Cycle::Full,
    };
    stopped.symbols().unwrap().reconcile_functions().unwrap();
    let roots = ledger(era, &mut stopped);
    let before = era
        .with_stopped_heap_for_test(&stopped, |heap| snapshot(heap, observation))
        .unwrap();
    let report = match cycle {
        Cycle::Minor => era.collect_stopped_minor(&mut stopped).unwrap(),
        Cycle::Full => era.collect_stopped(&mut stopped).unwrap(),
    };
    let after = era
        .with_stopped_heap_for_test(&stopped, |heap| snapshot(heap, observation))
        .unwrap();
    let response = ledger(era, &mut stopped);
    placement.resume(era, &stopped);
    stopped.complete().unwrap();
    let owner_age = match regular {
        Cycle::Minor => Age::Old,
        Cycle::Full => Age::Young,
    };
    assert_eq!(report.major, matches!(cycle, Cycle::Full));
    assert_eq!(report.frozen_owners, cohort.roles().len());
    assert!(
        roots.other_work.is_empty(),
        "unexpected root-bearing work: {roots:?}"
    );
    assert!(
        response.cons_writes.is_empty(),
        "published cons writes drained for next response: {response:?}"
    );
    assert!(
        response.young_owners.is_empty(),
        "young inventory captured into response: {response:?}"
    );
    match observation {
        Observation::Warm(owner) => {
            expect_owner(
                &before,
                Age::Young,
                Card::Clean,
                WordIdentity(TaggedValue::NIL.bits()),
            );
            expect_owner(
                &after,
                owner_age,
                Card::Clean,
                WordIdentity(TaggedValue::NIL.bits()),
            );
            assert!(roots.cons_writes.is_empty());
            assert_eq!(owner, observation.owner());
        }
        Observation::Replacement(facts) => {
            let before_card = match regular {
                Cycle::Minor => Card::Dirty,
                Cycle::Full => Card::Clean,
            };
            expect_owner(&before, owner_age, before_card, facts.kept);
            expect_owner(&after, owner_age, Card::Clean, facts.kept);
            assert_eq!(
                before.children,
                vec![Age::Young; 2],
                "both children freshly allocated after prior response"
            );
            assert_eq!(
                after.children,
                vec![Age::Missing, owner_age],
                "discarded young child reclaimed; current child survives"
            );
            assert_eq!(
                roots.cons_writes,
                vec![(OwnerId(1), facts.owner.0); 2],
                "both stores record only the actual owner"
            );
            assert!(roots.young_owners.iter().all(|peer| *peer == OwnerId(1)));
            match regular {
                Cycle::Minor => assert!(!roots.young_owners.is_empty()),
                Cycle::Full => {}
            }
            let children = [facts.discarded, facts.kept];
            assert!(
                roots
                    .world
                    .iter()
                    .chain(roots.without_world())
                    .all(|word| !children.contains(word)),
                "direct child root masked current-owner child tracing: {roots:?}"
            );
            let owning: Vec<_> = roots
                .owners
                .iter()
                .filter(|(_, words)| words.contains(&facts.owner.0))
                .collect();
            assert_eq!(owning.len(), 1);
            assert_eq!(owning[0].0, OwnerId(1));
            assert_eq!(
                owning[0]
                    .1
                    .iter()
                    .filter(|word| **word == facts.owner.0)
                    .count(),
                1
            );
        }
        Observation::Final { current, .. } => {
            expect_owner(&before, owner_age, Card::Clean, current.kept);
            match regular {
                Cycle::Minor => assert_eq!(
                    before.children,
                    vec![Age::Old, Age::Missing, Age::Old],
                    "previous old child must still be live BEFORE the final full job"
                ),
                Cycle::Full => assert_eq!(
                    before.children,
                    vec![Age::Missing, Age::Missing, Age::Young],
                    "GEN0 preceding full already reclaimed the previous child"
                ),
            }
            expect_owner(&after, owner_age, Card::Clean, current.kept);
            assert_eq!(
                after.children,
                vec![Age::Missing, Age::Missing, owner_age],
                "full job reclaims previous child after replacement"
            );
        }
    }
    eprintln!(
        "P3SYM_C6_RELOG_COLLECTION kind={cycle:?} observation={observation:?} epoch={} cycle={} owners={}",
        report.epoch, report.cycle, report.frozen_owners
    );
    Receipt {
        epoch: CollectionEpoch(report.epoch),
        cycle: CollectionCycle(report.cycle),
    }
}
fn command(
    commands: &Commands,
    cohort: Cohort,
    command: Command,
    await_all: &impl Fn(),
    facts: &mpsc::Receiver<Construction>,
) -> Facts {
    dispatch_role(commands, cohort, Role::Publisher, command, await_all);
    match facts
        .try_recv()
        .expect("native replacement facts precede publication")
    {
        Construction::Relog(facts) => facts,
        Construction::Definition(_) | Construction::Winner(_) => {
            panic!("wrong native construction kind")
        }
    }
}
fn repark(commands: &Commands, await_all: &impl Fn()) {
    for tx in &commands.0 {
        tx.send(Command::Repark).unwrap();
    }
    await_all();
}
pub(super) fn exercise(
    era: &ParallelHeapEra<'_>,
    cohort: Cohort,
    scenario: Scenario,
    commands: &Commands,
    await_all: &impl Fn(),
    facts: &mpsc::Receiver<Construction>,
    placement: &RefCell<Placement>,
) {
    let owner = match command(commands, cohort, Command::InitializeRelog, await_all, facts) {
        Facts::Owner(owner) => owner,
        Facts::Replaced(_) => panic!("wrong private-owner initialization receipt"),
    };
    let warm = round(era, cohort, placement, Observation::Warm(owner));
    repark(commands, await_all);
    match scenario {
        Scenario::RelogNoop => return,
        Scenario::Relog | Scenario::RelogUnwind => {}
        Scenario::Transfer
        | Scenario::Noop
        | Scenario::Unwind
        | Scenario::MinorAdapter
        | Scenario::Fresh
        | Scenario::FreshNoop
        | Scenario::FreshUnwind
        | Scenario::NameMinor
        | Scenario::NameMinorNoop
        | Scenario::NameMinorUnwind => panic!("wrong replacement scenario"),
    }
    let first = match command(commands, cohort, Command::ReplaceRelog, await_all, facts) {
        Facts::Replaced(facts) => facts,
        Facts::Owner(_) => panic!("wrong first replacement receipt"),
    };
    let first_round = round(era, cohort, placement, Observation::Replacement(first));
    assert!(first_round.epoch > warm.epoch && first_round.cycle > warm.cycle);
    repark(commands, await_all);
    match scenario {
        Scenario::RelogUnwind => {
            panic!("controlled replacement unwind after first completed collection")
        }
        Scenario::Relog | Scenario::RelogNoop => {}
        Scenario::Transfer
        | Scenario::Noop
        | Scenario::Unwind
        | Scenario::MinorAdapter
        | Scenario::Fresh
        | Scenario::FreshNoop
        | Scenario::FreshUnwind
        | Scenario::NameMinor
        | Scenario::NameMinorNoop
        | Scenario::NameMinorUnwind => panic!("wrong replacement scenario"),
    }
    let second = match command(commands, cohort, Command::ReplaceRelog, await_all, facts) {
        Facts::Replaced(facts) => facts,
        Facts::Owner(_) => panic!("wrong second replacement receipt"),
    };
    assert_ne!(
        first.kept, second.kept,
        "a new child, not an old-child repetition"
    );
    let second_round = round(era, cohort, placement, Observation::Replacement(second));
    assert!(second_round.epoch > first_round.epoch && second_round.cycle > first_round.cycle);
    repark(commands, await_all);
    let final_round = round(
        era,
        cohort,
        placement,
        Observation::Final {
            previous: first.kept,
            current: second,
        },
    );
    assert!(final_round.epoch > second_round.epoch && final_round.cycle > second_round.cycle);
    match placement.borrow().cycle() {
        Cycle::Minor => eprintln!(
            "P3SYM_C6_RELOG_WITNESSES two_fresh_replacements current_child_survives discarded_young_reclaimed work_drained final_full_reclaims_previously_live_old_child {cohort:?}"
        ),
        Cycle::Full => eprintln!(
            "P3SYM_C6_RELOG_WITNESSES two_fresh_replacements current_child_survives discarded_young_reclaimed work_drained serial_full_child_reclaimed {cohort:?}"
        ),
    }
}

#[test]
fn p3sym_c6_relog_two_plain() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Two,
        Scenario::Relog,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_relog_two_projected() {
    cohort(PlainSymbolBacking::Projected, Cohort::Two, Scenario::Relog).unwrap();
}
#[test]
fn p3sym_c6_relog_four_plain() {
    cohort(
        PlainSymbolBacking::Unprojected,
        Cohort::Four,
        Scenario::Relog,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_relog_four_projected() {
    cohort(PlainSymbolBacking::Projected, Cohort::Four, Scenario::Relog).unwrap();
}
#[test]
fn p3sym_c6_relog_noop_returns_catalog_and_releases_banks() {
    cohort(
        PlainSymbolBacking::Projected,
        Cohort::Two,
        Scenario::RelogNoop,
    )
    .unwrap();
}
#[test]
fn p3sym_c6_relog_unwind_returns_catalog_and_releases_banks() {
    let error = cohort(
        PlainSymbolBacking::Projected,
        Cohort::Two,
        Scenario::RelogUnwind,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<&str>(),
        Some(&"controlled replacement unwind after first completed collection")
    );
}

static_assertions::assert_not_impl_any!(Mutation<'static>: Send, Sync, Clone, Copy);
