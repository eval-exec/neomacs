//! Sealed symbol authority; running values move only between owner root homes.
#[cfg(all(test, not(feature = "test-support")))]
mod closed_graph;
#[cfg(feature = "test-support")]
pub mod closed_graph;
#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
pub(super) mod combined_inputs;
#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
pub(super) fn check_owner_home_for_test(
    run: &RunAccess<'_, '_>,
    home: &OwnerRootSlot<'_>,
) -> Result<(), RegistrationError> {
    globals::check_home(run, home)
}
mod globals;
pub(super) use globals::check_home as check_symbol_home;
mod intern;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod occupancy;
#[cfg(not(feature = "test-support"))]
mod roots;
#[cfg(feature = "test-support")]
pub mod roots;
pub use globals::GlobalError;
pub(crate) use globals::SymbolReadToken;
#[doc(hidden)]
pub use globals::SymbolWriteToken;
pub use intern::InitialInternError;
#[cfg(not(feature = "test-support"))]
mod functions;
#[cfg(feature = "test-support")]
pub mod functions;
#[cfg(any(test, feature = "test-support"))]
pub use functions::FunctionOpKind;
#[cfg(any(test, feature = "test-support"))]
pub(super) use functions::PreparedNameFunction;
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) use functions::{EraCoreHofExt, EraCoreUnaryExt, RunAccessHofExt, RunAccessUnaryExt};
pub use functions::{
    FunctionActivation, FunctionCallCache, FunctionError, FunctionSelection, FunctionStep,
};
#[doc(hidden)]
pub use functions::{PreparedHofBuiltin, PreparedUnaryFunction};
mod word;
use super::*;
use crate::intern::SymId;
pub use crate::symbol::parallel::PlainSymbolBacking;
pub(super) use crate::symbol::parallel::{KeeperWorldDesignator, SymbolBacking};
use crate::tagged::gc::symbol_table::SymbolSlotIndex;

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy)]
pub(crate) enum TableCommitFault {
    None,
    AfterDescriptorPublication,
}

/// Entirely parallel-era metadata; no World, Context or symbol layout grows.
pub(super) struct SymbolSidecar {
    word: AtomicU64,
    poisoned: AtomicBool,
    backing: SymbolBacking,
    era: NonZeroU64,
    pub(super) functions: functions::write::FunctionState,
    pub(super) tables: super::super::symbol_table::publication::PublishedSymbolTables,
    #[cfg(any(test, feature = "test-support"))]
    table_commit_fault: Mutex<TableCommitFault>,
}
impl SymbolSidecar {
    pub(super) fn new(
        backing: SymbolBacking,
        era: NonZeroU64,
        tables: super::super::symbol_table::publication::PublishedSymbolTables,
    ) -> Self {
        Self {
            word: AtomicU64::new(0),
            poisoned: AtomicBool::new(false),
            backing,
            era,
            functions: functions::write::FunctionState::new(),
            tables,
            #[cfg(any(test, feature = "test-support"))]
            table_commit_fault: Mutex::new(TableCommitFault::None),
        }
    }
    pub(super) fn ready(&self) -> Result<(), RegistrationError> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if !word::quiescent(&self.word) {
            return Err(RegistrationError::OutstandingLoan);
        }
        Ok(())
    }
    /// Descriptor-only execution may overlap symbol readers/writers, but may
    /// never enter a backing retained after a terminal symbol transaction.
    pub(super) fn execution_ready(&self) -> Result<(), RegistrationError> {
        if self.poisoned.load(Ordering::Acquire) {
            Err(RegistrationError::Poisoned)
        } else {
            Ok(())
        }
    }

    pub(super) fn poison(&self) {
        self.poisoned.store(true, Ordering::Release);
    }
    fn validate(&self, key: &SymbolHandle, registry: &Registry) -> Result<(), RegistrationError> {
        if key.heap != registry.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if key.era != self.era
            || key.world != self.backing.world
            || key.obarray != self.backing.generation
        {
            return Err(RegistrationError::StaleRegistration);
        }
        if self.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        Ok(())
    }
}

/// Identity + admitted metadata, never a pointer or execution authority.
/// Private fields prevent an arbitrary SymId from forging a backing admission.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SymbolHandle {
    heap: HeapIdentity,
    world: usize,
    era: NonZeroU64,
    obarray: u64,
    symbol: SymId,
    kind: PlainSymbolBacking,
}

impl std::fmt::Debug for SymbolHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // World allocation roots remain private, including in diagnostics.
        f.debug_struct("SymbolHandle")
            .field("era", &self.era)
            .field("symbol", &self.symbol)
            .field("backing", &self.kind)
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl SymbolHandle {
    /// VIS row: copied era word for the stale-handle refusal oracle.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn era_for_test(&self) -> NonZeroU64 {
        self.era
    }

    /// VIS row: fixture-only era rewrite that makes an admitted handle stale.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn set_era_for_test(&mut self, era: NonZeroU64) {
        self.era = era;
    }

    /// VIS row: copied symbol id for the function-cell oracle.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn sym_id_for_test(&self) -> SymId {
        self.symbol
    }
}

// Keep the existing symbol-vocabulary boundary in this module. Nominal era
// wrappers do not add their own dependency on the upper interner module.
impl<K: EraKeeper> ParallelHeapEra<'_, K> {
    pub fn admit_plain_symbol(&self, symbol: SymId) -> Result<SymbolHandle, RegistrationError> {
        self.core.admit_plain_symbol(symbol)
    }
}

// Era follow-up row (peer 10:5xZ item 1): one generic proof-era impl serves
// both builds. In value's own test build the evaluator's one-parameter alias
// is the same type at K = Baton; in the test-support dependency build the
// generic alias serves the fence. Consumers still call
// `era.admit_plain_symbol(..)` in both.
#[cfg(any(test, feature = "test-support"))]
impl<K: EraKeeper> GenericProofEra<'_, K> {
    #[doc(hidden)]
    pub fn admit_plain_symbol(&self, symbol: SymId) -> Result<SymbolHandle, RegistrationError> {
        self.core.admit_plain_symbol(symbol)
    }
}

// LEAD 11:5xZ (peer-signed fence-only observers): read-only views of the
// symbol authority word and backing for the re-homed core_tests roots file.
// They expose no borrow beyond the retained era, no field is widened, and
// the private Registry/SymbolSidecar types stay unnameable outside value.
#[cfg(any(test, feature = "test-support"))]
impl<K: EraKeeper> GenericProofEra<'_, K> {
    #[doc(hidden)]
    pub fn check_symbol_backing_for_test(
        &self,
        symbols: &KeeperSymbols<'_>,
        keeper: &KeeperWorldDesignator<'_>,
    ) -> Result<(), RegistrationError> {
        symbols.check_symbol_backing(&self.core.registry().symbols.backing, keeper)
    }

    /// C4 copied registry symbol-state observer (observe-only): the retained
    /// registry's function journal serial-finish check. Returns the copied
    /// result by value; no registry handle escapes.
    #[doc(hidden)]
    pub fn function_finish_ready_for_test(&self) -> Result<(), RegistrationError> {
        self.core.registry().symbols.functions.finish_ready()
    }

    #[doc(hidden)]
    pub fn check_root_designators_for_test(
        &self,
        keeper: &KeeperWorldDesignator<'_>,
    ) -> Result<(), RegistrationError> {
        self.core
            .registry()
            .symbols
            .backing
            .check_root_designators(keeper.obarray(), keeper.projections())
    }
}

impl<K: EraKeeper> EraCore<'_, K> {
    /// Serial keeper admission only, before ANY participant is registered.
    /// Unsupported/absent cells refuse without creating or redirecting them.
    pub fn admit_plain_symbol(&self, symbol: SymId) -> Result<SymbolHandle, RegistrationError> {
        let registry = self.registry();
        if registry.keeper_thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread);
        }
        {
            let gate = registry.lock()?;
            if gate.entries.len() != 1 || gate.rendezvous.phase != Phase::Idle {
                return Err(RegistrationError::Busy);
            }
        }
        // No control lock overlaps symbol authority. Only this keeper may
        // register new owners, so the serial admission precondition stays true.
        let symbols = &registry.symbols;
        symbols.ready()?;
        if !word::writer_enter(&symbols.word) {
            return Err(RegistrationError::Busy);
        }
        let guard = AdmissionWriter(symbols);
        // SAFETY: retained World, no registered participants; WRITER held.
        let kind =
            unsafe { symbols.backing.classify(symbol) }.ok_or(RegistrationError::Publication)?;
        let key = SymbolHandle {
            heap: registry.heap,
            world: symbols.backing.world,
            era: symbols.era,
            obarray: symbols.backing.generation,
            symbol,
            kind,
        };
        drop(guard);
        Ok(key)
    }
}
struct AdmissionWriter<'a>(&'a SymbolSidecar);
impl Drop for AdmissionWriter<'_> {
    fn drop(&mut self) {
        word::writer_exit(&self.0.word);
    }
}

/// This exclusive borrow keeps the RunAccess IN_SLICE until release. The
/// forgotten-lease flag additionally keeps it there after mem::forget.
pub struct SymbolRef<'lease, 'owner, 'era> {
    run: &'lease mut RunAccess<'owner, 'era>,
    key: SymbolHandle,
}
pub struct SymbolMut<'lease, 'owner, 'era> {
    run: &'lease mut RunAccess<'owner, 'era>,
    key: SymbolHandle,
}
#[must_use = "the keeper receiver retains the exclusive stop borrow"]
pub struct KeeperSymbols<'stop> {
    registry: &'stop Registry,
    _stop: PhantomData<&'stop mut AllStopped<'stop>>,
    _thread: PhantomData<Rc<()>>,
}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    /// Validate handle identity before returning its immutable slot metadata.
    pub(super) fn checked_symbol_slot(
        &self,
        key: &SymbolHandle,
    ) -> Result<SymbolSlotIndex, RegistrationError> {
        self.check_symbol(key)?;
        let index = usize::try_from(key.symbol.0).map_err(|_| RegistrationError::Capacity)?;
        Ok(SymbolSlotIndex::new(index))
    }

    fn check_symbol(&self, key: &SymbolHandle) -> Result<(), RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        self.owner
            .registry
            .symbols
            .validate(key, self.owner.registry)
    }
    pub fn symbol_ref<'a>(
        &'a mut self,
        key: &SymbolHandle,
    ) -> Result<SymbolRef<'a, 'owner, 'era>, RegistrationError> {
        self.check_symbol(key)?;
        let symbols = &self.owner.registry.symbols;
        if !word::reader_enter(&symbols.word) {
            return Err(RegistrationError::Busy);
        }
        if symbols.poisoned.load(Ordering::Acquire) {
            word::reader_exit(&symbols.word);
            return Err(RegistrationError::Poisoned);
        }
        self.symbol_loan = true;
        Ok(SymbolRef {
            run: self,
            key: *key,
        })
    }
    pub fn symbol_mut<'a>(
        &'a mut self,
        key: &SymbolHandle,
    ) -> Result<SymbolMut<'a, 'owner, 'era>, RegistrationError> {
        self.check_symbol(key)?;
        let symbols = &self.owner.registry.symbols;
        if !word::writer_enter(&symbols.word) {
            return Err(RegistrationError::Busy);
        }
        if symbols.poisoned.load(Ordering::Acquire) {
            word::writer_exit(&symbols.word);
            return Err(RegistrationError::Poisoned);
        }
        self.symbol_loan = true;
        Ok(SymbolMut {
            run: self,
            key: *key,
        })
    }
}
impl SymbolRef<'_, '_, '_> {
    pub fn backing(&self) -> PlainSymbolBacking {
        self.key.kind
    }
}
impl SymbolMut<'_, '_, '_> {
    pub fn backing(&self) -> PlainSymbolBacking {
        self.key.kind
    }
}
impl Drop for SymbolRef<'_, '_, '_> {
    fn drop(&mut self) {
        word::reader_exit(&self.run.owner.registry.symbols.word);
        self.run.symbol_loan = false;
    }
}
impl Drop for SymbolMut<'_, '_, '_> {
    fn drop(&mut self) {
        let registry = self.run.owner.registry;
        let panicking = std::thread::panicking();
        // Sticky poison BEFORE release prevents another owner observing a
        // partial commit; the control gate is acquired only AFTER release.
        if panicking {
            registry.symbols.poison();
        }
        let poisoned = panicking || registry.symbols.poisoned.load(Ordering::Acquire);
        word::writer_exit(&registry.symbols.word);
        self.run.symbol_loan = false;
        if poisoned {
            registry.poison();
        }
    }
}
impl<'stop> KeeperSymbols<'stop> {
    pub(super) fn mint(
        stopped: &'stop mut AllStopped<'_>,
        registry: &'stop Registry,
    ) -> Result<Self, RendezvousError> {
        if !stopped.matches_registry(registry) {
            return Err(RendezvousError::Identity);
        }
        stopped.check_world_roots(registry.heap)?;
        registry.symbols.ready()?;
        Ok(Self {
            registry,
            _stop: PhantomData,
            _thread: PhantomData,
        })
    }
    pub fn backing(&self, key: &SymbolHandle) -> Result<PlainSymbolBacking, RegistrationError> {
        self.registry.symbols.validate(key, self.registry)?;
        self.registry.symbols.ready()?;
        // SAFETY: real validated AllStopped remains exclusively borrowed.
        unsafe { self.registry.symbols.backing.classify(key.symbol) }
            .ok_or(RegistrationError::Publication)
    }
}

static_assertions::assert_impl_all!(SymbolHandle: Send, Sync, Copy);
static_assertions::assert_not_impl_any!(SymbolRef<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SymbolMut<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(KeeperSymbols<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
