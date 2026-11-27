//! Era-scoped `nmjit` executable code registry with stop-bound grace reclamation (E7-P.1).
//!
//! `NmjitCodeRegistry` owns finalized `nmjit` leaf mappings independently of the
//! withdrawn legacy serial JIT cache (`COMPILED` / `OSR_CACHE` / `bg`). Multiple
//! admitted parallel owners execute registered leaf kernels concurrently inside
//! `ParallelExecutionAdmission::with_activation` (`IN_SLICE`) with no registry
//! lock held across native execution.
//!
//! Retiring an entry moves its `ExecutableMemory` owner out of `active` into
//! `retired` without freeing it. Retired mappings are freed ONLY when the keeper
//! supplies a matching `&mut EraGraceReceipt<'_, '_>` (minted exclusively from
//! `&mut AllStopped<'era>` during a stop) AND zero active calls are in flight,
//! or when the registry itself is dropped at terminal era teardown.
use super::*;
use neovm_value::tagged::gc::{EraGraceReceipt, HeapIdentity};
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, TryLockError};

/// Opaque identifier of a compiled `nmjit` Tier-1 leaf entry inside one
/// [`NmjitCodeRegistry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NmjitLeafId(NonZeroU64);

impl NmjitLeafId {
    #[inline]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Summary returned by [`NmjitCodeRegistry::reclaim_with_grace`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NmjitGraceReclaim {
    pub(crate) reclaimed: usize,
    pub(crate) remaining_active: usize,
}

/// Point-in-time counts of active entries, retired entries awaiting grace,
/// and currently executing leaf calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NmjitRegistryStats {
    pub(crate) active: usize,
    pub(crate) retired: usize,
    pub(crate) active_calls: usize,
}

/// Typed refusal from [`NmjitCodeRegistry`] operations.
#[derive(Debug)]
pub(crate) enum NmjitRegistryError {
    Compile(ParallelCompileError),
    Entry(ParallelEntryError),
    WrongHeap,
    RetiredOrMissing,
    Busy,
    Poisoned,
    Exhausted,
}

impl From<ParallelCompileError> for NmjitRegistryError {
    fn from(error: ParallelCompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<ParallelEntryError> for NmjitRegistryError {
    fn from(error: ParallelEntryError) -> Self {
        Self::Entry(error)
    }
}

enum RegisteredLeaf {
    Scalar(ParallelCompiledCode),
    Cell(ParallelCompiledCellCode),
}

struct RegistryState {
    next_id: u64,
    active: HashMap<NmjitLeafId, RegisteredLeaf>,
    retired: Vec<RegisteredLeaf>,
}

/// RAII counter for in-flight native leaf calls. Serves as defense-in-depth
/// alongside `EraGraceReceipt`'s `AllStopped` proof: `reclaim_with_grace`
/// refuses `Busy` whenever `active_calls > 0`.
struct ActiveCallGuard<'a> {
    counter: &'a AtomicUsize,
}

impl<'a> ActiveCallGuard<'a> {
    #[inline]
    fn enter(counter: &'a AtomicUsize) -> Self {
        let prev = counter.fetch_add(1, Ordering::Acquire);
        assert!(prev < usize::MAX, "nmjit active call counter overflow");
        Self { counter }
    }
}

impl Drop for ActiveCallGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        let prev = self.counter.fetch_sub(1, Ordering::Release);
        debug_assert!(prev > 0, "nmjit active call counter underflow");
    }
}

/// Era-scoped owner of compiled `nmjit` leaf mappings.
///
/// Automatically `Send + Sync` (all `Send` `ExecutableMemory` owners reside
/// inside `Mutex<RegistryState>`, and only finalized immutable `ParallelEntry`
/// function pointers are copied out under the mutex while `IN_SLICE` and
/// `active_calls > 0` pin the executable pages alive).
pub(crate) struct NmjitCodeRegistry {
    heap: HeapIdentity,
    state: Mutex<RegistryState>,
    active_calls: AtomicUsize,
}

static_assertions::assert_impl_all!(NmjitCodeRegistry: Send, Sync);
static_assertions::assert_not_impl_any!(NmjitCodeRegistry: Clone, Copy);

impl NmjitCodeRegistry {
    /// Create a fresh registry bound to the GEN1 heap proved by `quiescent`.
    pub(crate) fn new(quiescent: &Gen1Admission<'_>) -> Self {
        Self {
            heap: quiescent.heap_identity(),
            state: Mutex::new(RegistryState {
                next_id: 1,
                active: HashMap::new(),
                retired: Vec::new(),
            }),
            active_calls: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub(crate) fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, RegistryState>, NmjitRegistryError> {
        match self.state.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => Err(NmjitRegistryError::Busy),
            Err(TryLockError::Poisoned(_)) => Err(NmjitRegistryError::Poisoned),
        }
    }

    /// Blocking lock for short, non-blocking metadata lookups (`run_scalar`,
    /// `run_cell`, `retire`, `insert_leaf`) that never hold the lock across
    /// native execution or blocking operations. Refuses `Poisoned` without
    /// unwrapping.
    fn lock_state_blocking(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, RegistryState>, NmjitRegistryError> {
        self.state.lock().map_err(|_| NmjitRegistryError::Poisoned)
    }

    fn insert_leaf(&self, leaf: RegisteredLeaf) -> Result<NmjitLeafId, NmjitRegistryError> {
        let mut state = self.lock_state_blocking()?;
        let id = NonZeroU64::new(state.next_id).ok_or(NmjitRegistryError::Exhausted)?;
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or(NmjitRegistryError::Exhausted)?;
        let leaf_id = NmjitLeafId(id);
        state.active.insert(leaf_id, leaf);
        Ok(leaf_id)
    }

    /// Compile and register a straight-line Tier-1 scalar leaf kernel.
    /// Returns `Err(Compile(BackendUnavailable))` when built without `jit`.
    pub(crate) fn compile_scalar(
        &self,
        quiescent: &Gen1Admission<'_>,
        program: ScalarProgram,
    ) -> Result<NmjitLeafId, NmjitRegistryError> {
        if quiescent.heap_identity() != self.heap {
            return Err(NmjitRegistryError::WrongHeap);
        }
        let compiled = ParallelCompiledCode::compile(quiescent, program)?;
        self.insert_leaf(RegisteredLeaf::Scalar(compiled))
    }

    /// Compile and register a bounded Tier-1 symbol-cell leaf kernel.
    /// Returns `Err(Compile(BackendUnavailable))` when built without `jit`.
    pub(crate) fn compile_cell(
        &self,
        quiescent: &Gen1Admission<'_>,
        read: CellRead,
    ) -> Result<NmjitLeafId, NmjitRegistryError> {
        if quiescent.heap_identity() != self.heap {
            return Err(NmjitRegistryError::WrongHeap);
        }
        let compiled = ParallelCompiledCellCode::compile(quiescent, read)?;
        self.insert_leaf(RegisteredLeaf::Cell(compiled))
    }

    /// Execute a registered Tier-1 scalar leaf kernel inside an admitted
    /// `RunAccess` (`IN_SLICE`) with no registry lock held across native code.
    pub(crate) fn run_scalar<'era>(
        &self,
        id: NmjitLeafId,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        input: ImmediateResult,
    ) -> Result<(), NmjitRegistryError> {
        if admission.heap_identity() != self.heap {
            return Err(NmjitRegistryError::WrongHeap);
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (id, run, destination, input);
            Err(NmjitRegistryError::Compile(
                ParallelCompileError::BackendUnavailable,
            ))
        }
        #[cfg(feature = "jit")]
        {
            admission
                .with_activation(run, destination, |activation| {
                    let _active = ActiveCallGuard::enter(&self.active_calls);
                    let entry = {
                        let state = self.lock_state_blocking()?;
                        match state.active.get(&id) {
                            Some(RegisteredLeaf::Scalar(code)) => code.raw_entry(),
                            Some(RegisteredLeaf::Cell(_)) => {
                                return Err(NmjitRegistryError::Entry(
                                    ParallelEntryError::UnexpectedEntryKind,
                                ));
                            }
                            None => return Err(NmjitRegistryError::RetiredOrMissing),
                        }
                    };
                    let activation_address = std::ptr::from_mut(activation).cast();
                    let runtime = ParallelRuntime {
                        version: NativeAbiVersion::V1,
                        publication: activation.publication(),
                    };
                    let input = ScalarInput { word: input.word() };
                    let mut output = ScalarOutput {
                        word: ImmediateResult::Nil.word(),
                    };
                    // SAFETY: `self` owns `ExecutableMemory` inside `self.state`
                    // and never frees a mapping on `retire`: reclamation requires
                    // `&mut EraGraceReceipt` (`AllStopped`, `IN_SLICE == 0`) AND
                    // `active_calls == 0`, both of which are violated while this
                    // activation and `_active` are live. All four pointers borrow
                    // live stack/activation state matching the verified V1 ABI.
                    let status = unsafe {
                        entry(
                            &runtime,
                            activation_address,
                            std::ptr::from_ref(&input).cast(),
                            &mut output,
                        )
                    };
                    drop(runtime);
                    match NativeStatus::checked(status)? {
                        NativeStatus::Returned => {
                            let result = ImmediateResult::from_word(output.word).map_err(
                                |error| match error {
                                    ImmediateResultError::NotImmediate => {
                                        ParallelEntryError::NonImmediateResult
                                    }
                                },
                            )?;
                            activation
                                .store_immediate(result)
                                .map_err(ParallelEntryError::Authority)?;
                            Ok(())
                        }
                        NativeStatus::UnsupportedAbi => Err(NmjitRegistryError::Entry(
                            ParallelEntryError::UnsupportedAbi,
                        )),
                        NativeStatus::CellReadReady | NativeStatus::SlotAbsent => Err(
                            NmjitRegistryError::Entry(ParallelEntryError::UnexpectedEntryKind),
                        ),
                    }
                })
                .map_err(|err| NmjitRegistryError::Entry(ParallelEntryError::Authority(err)))?
        }
    }

    /// Execute a registered Tier-1 symbol-cell leaf kernel inside an admitted
    /// `RunAccess` (`IN_SLICE`) with no registry lock held across native code.
    pub(crate) fn run_cell<'era>(
        &self,
        id: NmjitLeafId,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        symbol: &SymbolHandle,
    ) -> Result<(), NmjitRegistryError> {
        if admission.heap_identity() != self.heap {
            return Err(NmjitRegistryError::WrongHeap);
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (id, run, destination, symbol);
            Err(NmjitRegistryError::Compile(
                ParallelCompileError::BackendUnavailable,
            ))
        }
        #[cfg(feature = "jit")]
        {
            admission
                .with_activation(run, destination, |activation| {
                    let _active = ActiveCallGuard::enter(&self.active_calls);
                    let (entry, operation) = {
                        let state = self.lock_state_blocking()?;
                        match state.active.get(&id) {
                            Some(RegisteredLeaf::Cell(code)) => {
                                (code.raw_entry(), code.read_kind())
                            }
                            Some(RegisteredLeaf::Scalar(_)) => {
                                return Err(NmjitRegistryError::Entry(
                                    ParallelEntryError::UnexpectedEntryKind,
                                ));
                            }
                            None => return Err(NmjitRegistryError::RetiredOrMissing),
                        }
                    };
                    // SAFETY: `self` retains the finalized mapping until a
                    // stop-bound grace pass with `active_calls == 0`, and this
                    // activation holds `IN_SLICE` across the lookup and read.
                    unsafe { super::cell::lookup(entry, activation, symbol) }?;
                    activation
                        .read_cell(symbol, operation)
                        .map_err(ParallelEntryError::CellRead)?;
                    Ok(())
                })
                .map_err(|err| NmjitRegistryError::Entry(ParallelEntryError::Authority(err)))?
        }
    }

    /// Retire an active leaf entry without freeing its executable mapping.
    /// In-flight owners complete safely; future lookups return `RetiredOrMissing`.
    pub(crate) fn retire(&self, id: NmjitLeafId) -> Result<(), NmjitRegistryError> {
        let mut state = self.lock_state_blocking()?;
        let leaf = state
            .active
            .remove(&id)
            .ok_or(NmjitRegistryError::RetiredOrMissing)?;
        state.retired.push(leaf);
        Ok(())
    }

    /// Reclaim all retired executable mappings (never `active`) once a
    /// matching stop-bound [`EraGraceReceipt`] proves all mutators are stopped.
    ///
    /// Refuses `WrongHeap` on a foreign receipt, `Busy` if the state mutex is
    /// contended or `active_calls > 0`, and `Poisoned` (retaining all active
    /// and retired mappings) if the state mutex is poisoned.
    pub(crate) fn reclaim_with_grace(
        &self,
        receipt: &mut EraGraceReceipt<'_, '_>,
    ) -> Result<NmjitGraceReclaim, NmjitRegistryError> {
        if receipt.heap_identity() != self.heap {
            return Err(NmjitRegistryError::WrongHeap);
        }
        let mut state = self.lock_state()?;
        if self.active_calls.load(Ordering::Acquire) > 0 {
            return Err(NmjitRegistryError::Busy);
        }
        let reclaimed = state.retired.len();
        // SAFETY invariant: `&mut EraGraceReceipt<'_, '_>` proves `AllStopped`
        // (`IN_SLICE == 0`) on the matching heap and `active_calls == 0`, so no
        // thread holds a `ParallelEntry` pointer into any retired mapping.
        // Dropping `state.retired` runs `ExecutableMemory::drop -> free_memory()`
        // exclusively on retired entries while leaving `state.active` intact.
        state.retired.clear();
        state.retired.shrink_to_fit();
        let remaining_active = state.active.len();
        Ok(NmjitGraceReclaim {
            reclaimed,
            remaining_active,
        })
    }

    pub(crate) fn stats(&self) -> Result<NmjitRegistryStats, NmjitRegistryError> {
        let state = self.lock_state()?;
        Ok(NmjitRegistryStats {
            active: state.active.len(),
            retired: state.retired.len(),
            active_calls: self.active_calls.load(Ordering::Acquire),
        })
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.state.lock().unwrap();
            panic!("intentional registry poison");
        }));
    }

    #[cfg(test)]
    pub(crate) fn hold_active_call_for_test<R>(&self, body: impl FnOnce() -> R) -> R {
        let _guard = ActiveCallGuard::enter(&self.active_calls);
        body()
    }

    #[cfg(test)]
    pub(crate) fn retained_counts_ignoring_poison_for_test(&self) -> (usize, usize) {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        (state.active.len(), state.retired.len())
    }
}
