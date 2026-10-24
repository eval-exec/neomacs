//! Cold canonical registration, rooted rendezvous and scoped stopped collection.
//! Running authority admits rooted cons and sealed hash-table allocation.
//! The serial inline iterator remains the Sim representation. The overlap executor
//! is the `restricted_lisp` test module and is absent from every
//! `NEOVM_THREADS` mode. Production still has no public Lisp overlap entry.
//!
//! An era owns the actual World Baton and the quiescent heap loan. Only a
//! metadata permit crosses a thread boundary; owner buffers are built there.
//! Unfinished teardown retains both authority and backing instead of returning
//! a usable serial World after an abandoned owner.
#![allow(dead_code)]

mod admission;
mod authority_word;
mod collection;
mod cons_allocation;
mod cons_catalog;
mod gate;
mod image;
mod native_extent;
mod root_slot;
mod run_slice;
mod table_allocation;
mod table_authority;
mod weak_policy;
pub use collection::{CollectionError, CollectionReport};
use gate::RegistryGuard;
pub use root_slot::OwnerRootSlot;
pub use run_slice::{RunSlice, SliceRoot};
mod rendezvous;
mod summaries;
mod symbol_authority;
#[cfg(test)]
pub(crate) mod symbol_functions;
#[cfg(test)]
mod symbol_globals;
#[cfg(test)]
mod symbol_intern;
#[cfg(test)]
pub use symbol_authority::FunctionOpKind;
pub use symbol_authority::{
    FunctionActivation, FunctionCallCache, FunctionError, FunctionStep, GlobalError,
    InitialInternError, KeeperSymbols, PlainSymbolBacking, SymbolHandle, SymbolMut, SymbolRef,
};
pub(crate) use symbol_authority::{SymbolReadToken, SymbolWriteToken};
mod token;
// The extent API ships ahead of its first production call site (the Green
// mutex/condition release points); the re-export is for that integration.
#[allow(unused_imports)]
pub use native_extent::{NativeReturn, NativeRoots};
pub use rendezvous::{
    AllStopped, AllStoppedFailure, Mode, Phase, Rendezvous, RendezvousError, Request,
    ResumeGcImage, ResumeGcImageFailure, RootedRetireFailure, Snapshot,
};
pub(crate) use rendezvous::{OwnerRootAdmission, StoppedRootAdmission};

pub use summaries::{RegistrationCensus, StoppedSummaries};
pub use table_authority::{KeeperTableMut, SealedTableAccess, TableMut, TableRef};
pub use token::{
    Mutator, MutatorFinishFailure, MutatorPermit, MutatorStartFailure, OwnerPublication, RunAccess,
    YoungWorkerLoan,
};

use admission::{AdmissionWord, PhaseLine, admissions_quiescent};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering, fence};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;

use rustc_hash::FxHashMap;

use super::young_registry::{YoungRegistrationId, YoungRegistryError};
use super::{HeapIdentity, QuiescentCollector};
use crate::emacs_core::eval::world::{Baton, FrozenLegacyRoots};
use image::{OwnerGcState, PublishedGcImage};
use weak_policy::WeakSweepPermit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationError {
    WrongHeap,
    WrongOwner,
    StaleRegistration,
    DuplicateOwner,
    WrongThread,
    Poisoned,
    Busy,
    Exhausted,
    Capacity,
    OutstandingLoan,
    OpenRegion,
    ResidualSatb,
    Publication,
    PendingRoots,
    PendingYoung,
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mutator registration refused: {self:?}")
    }
}
impl std::error::Error for RegistrationError {}

impl From<YoungRegistryError> for RegistrationError {
    fn from(error: YoungRegistryError) -> Self {
        match error {
            YoungRegistryError::WrongHeap => Self::WrongHeap,
            #[cfg(test)]
            YoungRegistryError::RegistrationExhausted => Self::Exhausted,
            _ => Self::OutstandingLoan,
        }
    }
}

// Heap/owner pairs and local registration ordinals can recur in a later era.
// A checked generation never does, including for the frozen legacy entry.
static NEXT_REGISTRATION_GENERATION: AtomicU64 = AtomicU64::new(1);

fn issue_generation(counter: &AtomicU64) -> Result<NonZeroU64, RegistrationError> {
    let issued = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| RegistrationError::Exhausted)?;
    NonZeroU64::new(issued).ok_or(RegistrationError::Exhausted)
}

/// Copyable identity, never execution or World authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MutatorKey {
    heap: HeapIdentity,
    world: HeapIdentity,
    owner: u64,
    registration: NonZeroU64,
    generation: NonZeroU64,
}

impl MutatorKey {
    pub fn heap_identity(self) -> HeapIdentity {
        self.heap
    }
    pub fn owner(self) -> u64 {
        self.owner
    }
    pub fn generation(self) -> NonZeroU64 {
        self.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryPhase {
    Frozen,
    Starting,
    Running,
    Published,
    Retiring,
    Retired,
}

struct EntryState {
    phase: EntryPhase,
    owner_thread: Option<ThreadId>,
    publication_generation: u64,
    epoch: u64,
    image: Option<PublishedGcImage>,
    run_loan: bool,
    worker_loan: bool,
    readers: usize,
    rendezvous: rendezvous::control::OwnerState,
    roots: Option<crate::tagged::transport::ParallelRootBatchLease>,
    retained_roots: Vec<crate::tagged::transport::ParallelRootBatchLease>,
}

/// Stable Send + Sync metadata. It never contains a Context, heap reference,
/// owner-state borrow, or owner-confined SATB buffer.
/// Line-padded so adjacent entries' admission words and the entry mutex
/// never share a cacheline across owners.
#[repr(align(64))]
pub struct MutatorEntry {
    key: MutatorKey,
    young: YoungRegistrationId,
    cancelled_start: AtomicBool,
    /// Owner-written slice admission word (single writer: the owner thread).
    admission: AtomicU64,
    state: Mutex<EntryState>,
}

impl std::fmt::Debug for MutatorEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutatorEntry")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

fn entry_lock_error<T>(error: std::sync::TryLockError<T>) -> RegistrationError {
    match error {
        std::sync::TryLockError::WouldBlock => RegistrationError::Busy,
        std::sync::TryLockError::Poisoned(_) => RegistrationError::Poisoned,
    }
}

impl MutatorEntry {
    pub fn key(&self) -> MutatorKey {
        self.key
    }
}

struct RegistryState {
    entries: Vec<Arc<MutatorEntry>>,
    // An index into the SAME entry set, never another payload/owner registry.
    young_index: FxHashMap<YoungRegistrationId, usize>,
    next_registration: u64,
    generation_limit: u64,
    catalog_transition: bool,
    rendezvous: rendezvous::control::State,
}

/// Per-acquisition gate words packed on one line: a single exclusive
/// cacheline transfer per lock covers both the mutex word and the owner tag.
#[repr(align(64))]
struct GateLine {
    state: Mutex<RegistryState>,
    gate_owner: AtomicU64,
}

/// Park/poison/notify words on their own line. `parked` is read on every
/// run-slice drop, so it must not ride the gate line's transfers, and the
/// condvar is RMW'd only while some waiter is parked.
#[repr(align(64))]
struct ParkLine {
    parked: AtomicUsize,
    poisoned: AtomicBool,
    wake: std::sync::Condvar,
}

struct Registry {
    heap: HeapIdentity,
    world: HeapIdentity,
    gate: GateLine,
    park: ParkLine,
    /// Read-mostly phase word; the only shared line the per-slice fast
    /// path loads. Kept off the gate and park lines so gate transfers and
    /// park RMWs never invalidate an owner's poll.
    phase: PhaseLine,
    request_epoch: AtomicU64,
    completed_epoch: AtomicU64,
    keeper_thread: ThreadId,
    root_epoch: super::FacadeEpochRetention,
    cons_catalog: super::cons_storage::ConsCatalog,
    symbols: symbol_authority::SymbolSidecar,
}

impl Registry {
    fn validate(
        &self,
        state: &RegistryState,
        key: MutatorKey,
    ) -> Result<Arc<MutatorEntry>, RegistrationError> {
        let entry = self.validate_ref(state, key)?;
        Ok(Arc::clone(entry))
    }

    /// Admission identity check without the per-slice reference-count RMW.
    /// The borrow is only used while the caller still holds the gate.
    fn validate_ref<'a>(
        &self,
        state: &'a RegistryState,
        key: MutatorKey,
    ) -> Result<&'a Arc<MutatorEntry>, RegistrationError> {
        if key.heap != self.heap || key.world != self.world {
            return Err(RegistrationError::WrongHeap);
        }
        let index = usize::try_from(key.registration.get() - 1)
            .map_err(|_| RegistrationError::StaleRegistration)?;
        let entry = state
            .entries
            .get(index)
            .ok_or(RegistrationError::StaleRegistration)?;
        if entry.key.owner != key.owner {
            return Err(RegistrationError::WrongOwner);
        }
        if entry.key != key {
            return Err(RegistrationError::StaleRegistration);
        }
        Ok(entry)
    }
}

/// Keeper-confined RAII admission, with both heap and World retained.
/// There is deliberately no Deref, heap accessor, or constructor from IDs.
#[must_use = "finish explicitly; abandonment retains the World and poisons the era"]
pub struct ParallelHeapEra<'heap> {
    collector: RefCell<Option<QuiescentCollector<'heap>>>,
    suspended_cons: RefCell<Option<super::facade_mark::SuspendedConsCollector<'heap>>>,
    legacy_roots: Option<FrozenLegacyRoots>,
    #[cfg(test)]
    weak_sweep: Option<WeakSweepPermit>,
    keeper: Option<Baton>,
    registry: Option<Arc<Registry>>,
    _thread: PhantomData<Rc<()>>,
}

pub(crate) struct EraAdmissionFailure<'heap> {
    pub(crate) reason: RegistrationError,
    pub(crate) collector: QuiescentCollector<'heap>,
    pub(crate) keeper: Baton,
}

impl std::fmt::Debug for EraAdmissionFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EraAdmissionFailure")
            .field(&self.reason)
            .finish()
    }
}
impl std::fmt::Debug for ParallelHeapEra<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelHeapEra")
            .field("heap", &self.registry.as_ref().map(|r| r.heap))
            .finish_non_exhaustive()
    }
}

/// Busy/error recovery returns the entire admission; it never restores serial
/// authority merely because one owner is delayed.
pub(crate) struct EraFinishFailure<'heap> {
    pub(crate) reason: RegistrationError,
    pub(crate) era: ParallelHeapEra<'heap>,
}
impl std::fmt::Debug for EraFinishFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EraFinishFailure")
            .field(&self.reason)
            .finish()
    }
}

impl<'heap> ParallelHeapEra<'heap> {
    #[cold]
    #[inline(never)]
    pub(crate) fn admit(
        mut collector: QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        let heap = collector.heap_identity();
        let reason = if heap != keeper.parallel_heap_identity() {
            Some(RegistrationError::WrongHeap)
        } else if !keeper.held_here() {
            Some(RegistrationError::WrongThread)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(EraAdmissionFailure {
                reason,
                collector,
                keeper,
            });
        }
        let generation = match issue_generation(&NEXT_REGISTRATION_GENERATION) {
            Ok(generation) => generation,
            Err(reason) => {
                return Err(EraAdmissionFailure {
                    reason,
                    collector,
                    keeper,
                });
            }
        };
        let root_epoch = match keeper.parallel_epoch_retention() {
            Ok(epoch) => epoch,
            Err(_) => {
                return Err(EraAdmissionFailure {
                    reason: RegistrationError::Exhausted,
                    collector,
                    keeper,
                });
            }
        };
        let image = match PublishedGcImage::from_legacy(collector.registration_heap_mut()) {
            Ok(image) => image,
            Err(error) => {
                return Err(EraAdmissionFailure {
                    reason: error.into(),
                    collector,
                    keeper,
                });
            }
        };
        let young = image.young_id();
        let key = MutatorKey {
            heap,
            world: heap,
            owner: 0,
            registration: NonZeroU64::MIN,
            generation,
        };
        let entry = Arc::new(MutatorEntry {
            key,
            young,
            cancelled_start: AtomicBool::new(false),
            admission: AtomicU64::new(AdmissionWord::OUT),
            state: Mutex::new(EntryState {
                phase: EntryPhase::Frozen,
                owner_thread: Some(std::thread::current().id()),
                publication_generation: 1,
                epoch: 0,
                image: Some(image),
                run_loan: false,
                worker_loan: false,
                readers: 0,
                rendezvous: rendezvous::control::OwnerState::new(rendezvous::Mode::StartingStopped),
                roots: None,
                retained_roots: Vec::new(),
            }),
        });
        let mut young_index = FxHashMap::default();
        young_index.insert(young, 0);
        let symbols =
            symbol_authority::SymbolSidecar::new(keeper.parallel_symbol_backing(), generation);
        Ok(Self {
            collector: RefCell::new(Some(collector)),
            suspended_cons: RefCell::new(None),
            legacy_roots: None,
            #[cfg(test)]
            weak_sweep: None,
            keeper: Some(keeper),
            registry: Some(Arc::new(Registry {
                heap,
                world: heap,
                gate: GateLine {
                    state: Mutex::new(RegistryState {
                        entries: vec![entry],
                        young_index,
                        next_registration: 2,
                        generation_limit: u64::MAX,
                        catalog_transition: false,
                        rendezvous: rendezvous::control::State::new(),
                    }),
                    gate_owner: AtomicU64::new(0),
                },
                park: ParkLine {
                    parked: AtomicUsize::new(0),
                    poisoned: AtomicBool::new(false),
                    wake: std::sync::Condvar::new(),
                },
                phase: PhaseLine::new(),
                request_epoch: AtomicU64::new(0),
                completed_epoch: AtomicU64::new(0),
                keeper_thread: std::thread::current().id(),
                root_epoch,
                cons_catalog: super::cons_storage::ConsCatalog::new(heap),
                symbols,
            })),
            _thread: PhantomData,
        })
    }

    fn registry(&self) -> &Registry {
        self.registry.as_ref().expect("live era")
    }

    pub fn heap_identity(&self) -> HeapIdentity {
        self.registry().heap
    }

    #[cold]
    #[inline(never)]
    pub fn register(
        &self,
        owner: u64,
        root_capacity: usize,
    ) -> Result<MutatorPermit<'_>, RegistrationError> {
        if owner == 0 {
            return Err(RegistrationError::WrongOwner);
        }
        let registry = self.registry();
        let mut state = registry.lock()?;
        for entry in &state.entries {
            if entry.key.owner == owner && !entry.cancelled_start.load(Ordering::Acquire) {
                let current = entry.state.try_lock().map_err(entry_lock_error)?;
                if current.phase != EntryPhase::Retired {
                    return Err(RegistrationError::DuplicateOwner);
                }
            }
        }
        let next = state.next_registration;
        let following = next
            .checked_add(1)
            .filter(|n| *n <= state.generation_limit)
            .ok_or(RegistrationError::Exhausted)?;
        let registration = NonZeroU64::new(next).ok_or(RegistrationError::Exhausted)?;
        state
            .entries
            .try_reserve(1)
            .map_err(|_| RegistrationError::Capacity)?;
        state
            .young_index
            .try_reserve(1)
            .map_err(|_| RegistrationError::Capacity)?;
        let young = YoungRegistrationId::issue_parallel(registry.heap)
            .map_err(|_| RegistrationError::Exhausted)?;
        let generation = issue_generation(&NEXT_REGISTRATION_GENERATION)?;
        let key = MutatorKey {
            heap: registry.heap,
            world: registry.world,
            owner,
            registration,
            generation,
        };
        let entry = Arc::new(MutatorEntry {
            key,
            young,
            cancelled_start: AtomicBool::new(false),
            admission: AtomicU64::new(AdmissionWord::OUT),
            state: Mutex::new(EntryState {
                phase: EntryPhase::Starting,
                owner_thread: None,
                publication_generation: 0,
                epoch: 0,
                image: None,
                run_loan: false,
                worker_loan: false,
                readers: 0,
                rendezvous: rendezvous::control::OwnerState::new(rendezvous::Mode::StartingStopped),
                roots: None,
                retained_roots: Vec::new(),
            }),
        });
        let index = state.entries.len();
        state.young_index.insert(young, index);
        state.entries.push(Arc::clone(&entry));
        state.next_registration = following;
        Ok(MutatorPermit {
            registry,
            entry,
            root_capacity,
            armed: true,
            _unshared: PhantomData,
        })
    }

    pub fn stopped_summaries(&self) -> Result<StoppedSummaries<'_>, RegistrationError> {
        StoppedSummaries::new(self.registry())
    }

    /// Explicit teardown performs no collection or implicit owner resumption.
    /// Collection and owner handoff must discharge young logs and execution
    /// roots before this terminal transfer can restore serial ownership.
    pub(crate) fn finish(
        mut self,
    ) -> Result<(QuiescentCollector<'heap>, Baton), EraFinishFailure<'heap>> {
        if let Err(reason) = self.prepare_finish() {
            return Err(EraFinishFailure { reason, era: self });
        }
        let collector = self.collector.get_mut().take().expect("live era");
        let keeper = self.keeper.take().expect("live era");
        self.registry.take();
        Ok((collector, keeper))
    }

    fn prepare_finish(&mut self) -> Result<(), RegistrationError> {
        self.registry().symbols.ready()?;
        self.registry().symbols.functions.finish_ready()?;
        if self.suspended_cons.get_mut().is_some() {
            return Err(RegistrationError::OpenRegion);
        }
        use crate::tagged::transport::{ParallelRootBatchError, RootBatchError};

        fn entry_guard<'a>(
            registry: &Registry,
            entry: &'a MutatorEntry,
        ) -> Result<MutexGuard<'a, EntryState>, RegistrationError> {
            match entry.state.try_lock() {
                Ok(current) => Ok(current),
                Err(std::sync::TryLockError::WouldBlock) => Err(RegistrationError::Busy),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    registry.poison();
                    Err(RegistrationError::Poisoned)
                }
            }
        }

        fn control_ready(state: &RegistryState) -> Result<(), RegistrationError> {
            if state.rendezvous.phase == rendezvous::Phase::Poisoned {
                return Err(RegistrationError::Poisoned);
            }
            if state.rendezvous.phase != rendezvous::Phase::Idle
                || !state.rendezvous.debt.is_empty()
                || state.rendezvous.queued
            {
                return Err(RegistrationError::Busy);
            }
            if state.rendezvous.active_readers != 0 {
                return Err(RegistrationError::OutstandingLoan);
            }
            Ok(())
        }

        fn entry_ready(
            entry: &MutatorEntry,
            index: usize,
            current: &EntryState,
        ) -> Result<(), RegistrationError> {
            if entry.admitted() || current.run_loan || current.worker_loan || current.readers != 0 {
                return Err(RegistrationError::OutstandingLoan);
            }
            if entry.cancelled_start.load(Ordering::Acquire) {
                // A cancelled permit never acquired an image or root lease.
                return if current.phase == EntryPhase::Starting
                    && current.image.is_none()
                    && current.roots.is_none()
                    && current.retained_roots.is_empty()
                {
                    Ok(())
                } else {
                    Err(RegistrationError::Publication)
                };
            }
            if (index == 0 && current.phase != EntryPhase::Frozen)
                || (index != 0 && current.phase != EntryPhase::Retiring)
            {
                return Err(RegistrationError::Busy);
            }
            let image = current
                .image
                .as_ref()
                .ok_or(RegistrationError::Publication)?;
            if !image.roots_empty() {
                return Err(RegistrationError::PendingRoots);
            }
            if index != 0 && !image.young_empty() {
                return Err(RegistrationError::PendingYoung);
            }
            // Validate every nonempty publication before retiring ANY lease.
            // Retained generations are ordered by replacement; a previous
            // successful cleanup attempt may already have removed any suffix.
            let mut previous = 0;
            for roots in &current.retained_roots {
                let identity = roots.identity();
                let generation = identity.publication_generation().get();
                if identity.key() != entry.key
                    || generation <= previous
                    || generation > current.publication_generation
                    || identity.publication_epoch() > current.epoch
                {
                    return Err(RegistrationError::Publication);
                }
                previous = generation;
            }
            if let Some(roots) = &current.roots {
                let identity = roots.identity();
                if identity.key() != entry.key
                    || identity.publication_generation().get() != current.publication_generation
                    || identity.publication_generation().get() <= previous
                    || identity.publication_epoch() != current.epoch
                {
                    return Err(RegistrationError::Publication);
                }
            }
            Ok(())
        }

        fn root_error(error: ParallelRootBatchError) -> RegistrationError {
            match error {
                ParallelRootBatchError::Transport(RootBatchError::Busy) => RegistrationError::Busy,
                ParallelRootBatchError::Transport(RootBatchError::ReadersOutstanding {
                    ..
                }) => RegistrationError::OutstandingLoan,
                ParallelRootBatchError::Transport(RootBatchError::ForeignHeap { .. }) => {
                    RegistrationError::WrongHeap
                }
                _ => RegistrationError::Publication,
            }
        }

        let registry = self.registry.as_ref().expect("live era");
        let entries = {
            let state = registry.lock()?;
            control_ready(&state)?;
            for (index, entry) in state.entries.iter().enumerate() {
                registry.validate(&state, entry.key)?;
                entry_ready(entry, index, &*entry_guard(registry, entry)?)?;
            }
            // All owners are now terminal: an earlier zero word/clean flag
            // alone could race an owner's last write before retirement.
            registry.symbols.functions.finish_ready()?;
            // Only Arc snapshots of the canonical set: no second owner or
            // payload registry. All image/root preflight precedes any removal.
            let mut entries = Vec::new();
            entries
                .try_reserve_exact(state.entries.len())
                .map_err(|_| RegistrationError::Capacity)?;
            entries.extend(state.entries.iter().cloned());
            entries
        };

        // Root-table operations never overlap the canonical control gate.
        // Preflight every reader obligation before the first root retirement.
        for (index, entry) in entries.iter().enumerate() {
            let current = entry_guard(registry, entry)?;
            entry_ready(entry, index, &current)?;
            if let Some(roots) = &current.roots {
                roots.validate_no_readers().map_err(root_error)?;
            }
            for roots in &current.retained_roots {
                roots.validate_no_readers().map_err(root_error)?;
            }
        }
        for entry in &entries {
            let mut current = entry_guard(registry, entry)?;
            if let Some(roots) = current.roots.take() {
                if let Err(failure) = roots.try_finish() {
                    let (roots, error) = failure.into_parts();
                    current.roots = Some(roots);
                    return Err(root_error(error));
                }
            }
            while let Some(roots) = current.retained_roots.pop() {
                if let Err(failure) = roots.try_finish() {
                    let (roots, error) = failure.into_parts();
                    // Pop retained its capacity, so restoring cannot allocate.
                    current.retained_roots.push(roots);
                    return Err(root_error(error));
                }
            }
        }

        let state = registry.lock()?;
        control_ready(&state)?;
        if state.entries.len() != entries.len()
            || state
                .entries
                .iter()
                .zip(&entries)
                .any(|(current, snapshot)| !Arc::ptr_eq(current, snapshot))
        {
            return Err(RegistrationError::Publication);
        }
        // Acquire every entry guard before taking the first GC image. A
        // contended later entry must return Busy without partially merging or
        // retiring any canonical owner image.
        let mut entry_guards = Vec::new();
        entry_guards
            .try_reserve_exact(state.entries.len())
            .map_err(|_| RegistrationError::Capacity)?;
        for (index, entry) in state.entries.iter().enumerate() {
            let current = entry_guard(registry, entry)?;
            entry_ready(entry, index, &current)?;
            if current.roots.is_some() || !current.retained_roots.is_empty() {
                return Err(RegistrationError::Busy);
            }
            entry_guards.push(current);
        }
        let (legacy, owners) = entry_guards
            .split_first_mut()
            .ok_or(RegistrationError::Publication)?;
        let mut image = legacy.image.take().expect("preflighted frozen image");
        for (entry, current) in state.entries.iter().skip(1).zip(owners.iter_mut()) {
            if entry.cancelled_start.load(Ordering::Acquire) {
                continue;
            }
            let other = current.image.take().expect("preflighted retiring image");
            if let Err(failure) = image.merge_non_young(other) {
                current.image = Some(failure.image);
                legacy.image = Some(image);
                registry.poison();
                return Err(failure.error.into());
            }
            current.phase = EntryPhase::Retired;
        }
        if let Err(failure) = image.restore_legacy(
            self.collector
                .get_mut()
                .as_mut()
                .expect("live era")
                .registration_heap_mut(),
        ) {
            legacy.image = Some(failure.image);
            registry.poison();
            return Err(failure.error.into());
        }
        legacy.phase = EntryPhase::Retired;
        Ok(())
    }
}

impl Drop for ParallelHeapEra<'_> {
    fn drop(&mut self) {
        let Some(registry) = self.registry.take() else {
            return;
        };
        registry.poison();
        // Poison serializes with the predicate gate; no collection, join,
        // callback, or implicit serial resumption follows.
        // The leaked keeper retains World/heap backing and facade exclusion.
        std::mem::forget(registry);
        if let Some(keeper) = self.keeper.take() {
            std::mem::forget(keeper);
        }
    }
}

static_assertions::assert_not_impl_any!(ParallelHeapEra<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(MutatorEntry: Send, Sync);
static_assertions::assert_impl_all!(MutatorKey: Send, Sync, Copy);

#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod peer_young_collection_tests;
#[cfg(test)]
mod tests;

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
mod collection_scalar_tests;

#[cfg(test)]
mod cons_allocation_tests;
#[cfg(test)]
mod cons_micro_tests;
#[cfg(test)]
mod cons_tails_tests;
#[cfg(test)]
mod root_slot_tests;

#[cfg(test)]
mod slice_tests;

#[cfg(test)]
mod a8_latency;
#[cfg(test)]
mod a8_probes;
#[cfg(test)]
mod native_extent_tests;
#[cfg(test)]
mod restricted_lisp;
