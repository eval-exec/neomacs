//! Sealed symbol authority; running values move only between owner root homes.
#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
pub(super) mod combined_inputs;
#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
pub(super) fn check_owner_home_for_test(
    run: &RunAccess<'_, '_>,
    home: &OwnerRootSlot<'_>,
) -> Result<(), RegistrationError> {
    globals::check_home(run, home)
}
mod globals;
mod intern;
mod roots;
pub use globals::GlobalError;
pub(crate) use globals::{SymbolReadToken, SymbolWriteToken};
pub use intern::InitialInternError;
mod functions;
#[cfg(test)]
pub use functions::FunctionOpKind;
#[cfg(test)]
pub(super) use functions::PreparedNameFunction;
pub use functions::{FunctionActivation, FunctionCallCache, FunctionError, FunctionStep};
#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
mod closed_graph;
mod word;
use super::*;
use crate::emacs_core::intern::SymId;
pub use crate::emacs_core::symbol::parallel::PlainSymbolBacking;
pub(super) use crate::emacs_core::symbol::parallel::{KeeperWorldDesignator, SymbolBacking};

#[cfg(test)]
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
    #[cfg(test)]
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
            #[cfg(test)]
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

// Keep the existing symbol-vocabulary boundary in this module. Nominal era
// wrappers do not add their own dependency on the upper interner module.
impl<K: EraKeeper> ParallelHeapEra<'_, K> {
    pub fn admit_plain_symbol(&self, symbol: SymId) -> Result<SymbolHandle, RegistrationError> {
        self.core.admit_plain_symbol(symbol)
    }
}
#[cfg(test)]
impl ProofEra<'_> {
    pub fn admit_plain_symbol(&self, symbol: SymId) -> Result<SymbolHandle, RegistrationError> {
        self.core.admit_plain_symbol(symbol)
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
