//! Publication-only cold operands; the small handoff fixture allocates none.
use super::*;
use crate::emacs_core::eval::world::WorldEra as ParallelHeapEra;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::Obarray;
use crate::tagged::gc::FunctionActivation;
use crate::tagged::gc::symbol_table::publication::TableStatistics;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy)]
struct Namespace(u64);
#[derive(Clone, Copy, Debug)]
struct ChunkIdentity(usize);
/// A diagnostic observation, never registry-epoch or interner authority.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ObservedRegistryCensus((u64, usize));
impl ObservedRegistryCensus {
    pub(crate) fn assert_issuance(self, observed: (u64, usize), owners: CohortSize) {
        let unique = owners
            .count()
            .checked_add(1)
            .expect("shared plus distinct names");
        let expected = (
            self.0
                .0
                .checked_add(u64::try_from(unique).expect("small cohort"))
                .expect("registry observation epoch"),
            self.0
                .1
                .checked_add(unique)
                .expect("registry observation count"),
        );
        assert_eq!(
            observed, expected,
            "one registry id/epoch per unique spelling"
        );
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct PublicationFacts {
    pub(crate) heap: HeapIdentity,
    first_chunk: ChunkIdentity,
    pub(crate) registry: ObservedRegistryCensus,
    pub(crate) tables: TableStatistics,
    pub(crate) slots: crate::tagged::gc::symbol_table::SymbolSlotCount,
    pub(crate) chunks: crate::tagged::gc::symbol_table::SymbolChunkCount,
    pub(crate) chunk_slots: crate::tagged::gc::symbol_table::SymbolChunkSlots,
}
impl PublicationFacts {
    pub(crate) fn original_chunk_is_retained(
        &self,
        table: &crate::tagged::gc::symbol_table::PublishedSymbolTable,
    ) -> bool {
        match table.chunk(0.into()) {
            Some(chunk) => chunk.as_ptr().addr() == self.first_chunk.0,
            None => false,
        }
    }
}
pub(crate) struct PublicationInputs<'era> {
    pub(crate) hof: HofTestInputs<'era>,
    pub(crate) named_symbol: SymbolHandle,
    pub(crate) named_old: OwnerRootSlot<'era>,
    pub(crate) named_new: OwnerRootSlot<'era>,
    pub(crate) named_activation: FunctionActivation<'era>,
    pub(crate) shared_name: OwnerRootSlot<'era>,
    pub(crate) distinct_name: OwnerRootSlot<'era>,
    pub(crate) fresh_symbol: OwnerRootSlot<'era>,
    pub(crate) fresh_value: OwnerRootSlot<'era>,
    pub(crate) facts: PublicationFacts,
}
pub(super) struct ColdInputs {
    named: SymId,
    named_new: SymId,
    scalar: SymId,
    names: Vec<(SymId, SymId)>,
}
#[derive(Clone, Copy)]
pub(super) struct PublicationKeys {
    named: SymbolHandle,
    named_new: SymbolHandle,
    scalar: SymbolHandle,
    shared: SymbolHandle,
    distinct: SymbolHandle,
}
pub(super) enum ScenarioSeed {
    Handoff,
    Publication(ColdInputs),
}
pub(super) enum PreparedInputs {
    Handoff,
    Publication {
        keys: Vec<PublicationKeys>,
        facts: PublicationFacts,
    },
}
pub(super) enum CohortInputs<'era> {
    Handoff(HofTestInputs<'era>),
    Publication(PublicationInputs<'era>),
}
#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Handoff,
    Publication,
}
impl Scenario {
    pub(super) fn capacity(self) -> usize {
        match self {
            Self::Handoff => 32,
            Self::Publication => 96,
        }
    }
    pub(super) fn initialize(self, obarray: &mut Obarray, size: CohortSize) -> ScenarioSeed {
        match self {
            Self::Handoff => ScenarioSeed::Handoff,
            Self::Publication => {
                let namespace = Namespace(
                    NEXT_NAMESPACE
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                            current.checked_add(1)
                        })
                        .expect("publication fixture namespace exhausted"),
                );
                let prefix = format!("nmjit-native-publication-{}", namespace.0);
                let named = intern(&format!("{prefix}-named-holder"));
                let named_new = intern(&format!("{prefix}-named-new-holder"));
                let scalar = intern(&format!("{prefix}-scalar-holder"));
                let original = definition(FixtureParameters::Nullary, 73);
                obarray.seed_value_id(named, original);
                obarray.set_symbol_function_id(named, original);
                obarray.seed_value_id(named_new, definition(FixtureParameters::Nullary, 97));
                obarray.seed_value_id(scalar, TaggedValue::fixnum(73));
                let shared_spelling = format!("{prefix}-fresh-shared");
                assert!(crate::emacs_core::intern::lookup_interned(&shared_spelling).is_none());
                let mut names = Vec::new();
                for index in 0..size.count() {
                    let shared = intern(&format!("{prefix}-shared-holder-{index}"));
                    let distinct = intern(&format!("{prefix}-distinct-holder-{index}"));
                    let distinct_spelling = format!("{prefix}-fresh-distinct-{index}");
                    assert!(
                        crate::emacs_core::intern::lookup_interned(&distinct_spelling).is_none()
                    );
                    // Equal shared spellings have independent managed objects.
                    obarray.seed_value_id(shared, TaggedValue::string(shared_spelling.clone()));
                    obarray.seed_value_id(distinct, TaggedValue::string(distinct_spelling));
                    names.push((shared, distinct));
                }
                // Only registry ids grow here. The World has already seeded ALL
                // holders; the first live intern must append actual World chunks.
                let mut gap =
                    crate::emacs_core::intern::intern_uninterned("nmjit-native-publication-gap");
                let chunk_slots = crate::emacs_core::symbol::OBARRAY_CHUNK_SLOTS as u64;
                let target = (u64::from(gap.0) / chunk_slots + 2) * chunk_slots + 17;
                while u64::from(gap.0) + 1 < target {
                    gap = crate::emacs_core::intern::intern_uninterned(
                        "nmjit-native-publication-gap",
                    );
                }
                ScenarioSeed::Publication(ColdInputs {
                    named,
                    named_new,
                    scalar,
                    names,
                })
            }
        }
    }
}
impl ScenarioSeed {
    pub(super) fn prepare(self, era: &ParallelHeapEra<'_>) -> PreparedInputs {
        match self {
            Self::Handoff => PreparedInputs::Handoff,
            Self::Publication(cold) => {
                let named = era
                    .admit_plain_symbol(cold.named)
                    .expect("publication nullary target");
                era.prepare_function_symbol(&named)
                    .expect("publication nullary journal");
                let named_new = era
                    .admit_plain_symbol(cold.named_new)
                    .expect("publication nullary replacement");
                let scalar = era
                    .admit_plain_symbol(cold.scalar)
                    .expect("publication scalar holder");
                let keys = cold
                    .names
                    .into_iter()
                    .map(|(shared, distinct)| PublicationKeys {
                        named,
                        named_new,
                        scalar,
                        shared: era
                            .admit_plain_symbol(shared)
                            .expect("publication shared-name holder"),
                        distinct: era
                            .admit_plain_symbol(distinct)
                            .expect("publication distinct-name holder"),
                    })
                    .collect();
                let registry = era.core.registry();
                let view = registry.symbols.tables.view();
                let table = view.table();
                let facts = PublicationFacts {
                    heap: registry.heap,
                    first_chunk: ChunkIdentity(
                        table
                            .chunk(0.into())
                            .expect("seeded first chunk")
                            .as_ptr()
                            .addr(),
                    ),
                    registry: ObservedRegistryCensus(
                        crate::emacs_core::intern::parallel::identity_census_for_test(),
                    ),
                    tables: registry
                        .symbols
                        .tables
                        .stats()
                        .expect("publication initial table statistics"),
                    slots: table.slots(),
                    chunks: table.chunk_count(),
                    chunk_slots: table.chunk_slots(),
                };
                PreparedInputs::Publication { keys, facts }
            }
        }
    }
}
fn root_value<'era>(
    run: &mut RunAccess<'_, 'era>,
    key: &SymbolHandle,
) -> Result<OwnerRootSlot<'era>, String> {
    let mut home = run
        .private_nil_root()
        .map_err(|error| format!("publication root: {error:?}"))?;
    contention::retry(
        Deadline::contention(),
        || run.sym_value_into(key, &mut home),
        contention::global,
    )
    .map_err(|error| format!("publication value: {error:?}"))?;
    Ok(home)
}
impl PreparedInputs {
    pub(super) fn seed<'era>(
        &self,
        index: usize,
        run: &mut RunAccess<'_, 'era>,
        hof: HofTestInputs<'era>,
    ) -> Result<CohortInputs<'era>, String> {
        match self {
            Self::Handoff => Ok(CohortInputs::Handoff(hof)),
            Self::Publication { keys, facts } => {
                let keys = keys.get(index).ok_or("publication worker key absent")?;
                let named_old = root_value(run, &keys.named)?;
                let named_new = root_value(run, &keys.named_new)?;
                let named_activation = FunctionActivation::new(run)
                    .map_err(|error| format!("publication activation: {error:?}"))?;
                let shared_name = root_value(run, &keys.shared)?;
                let distinct_name = root_value(run, &keys.distinct)?;
                let fresh_symbol = run
                    .private_nil_root()
                    .map_err(|error| format!("publication symbol home: {error:?}"))?;
                let fresh_value = root_value(run, &keys.scalar)?;
                Ok(CohortInputs::Publication(PublicationInputs {
                    hof,
                    named_symbol: keys.named,
                    named_old,
                    named_new,
                    named_activation,
                    shared_name,
                    distinct_name,
                    fresh_symbol,
                    fresh_value,
                    facts: *facts,
                }))
            }
        }
    }
}
