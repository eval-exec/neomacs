//! Prepared, bounded function writes and stopped serial-cache reconciliation.
use super::*;
use crate::symbol::parallel::FunctionUpdate;
use crate::tagged::gc::function_write_hooks::InitialDefinition;
use std::cell::UnsafeCell;

struct FunctionRecord {
    symbol: SymId,
    lambda: SymId,
    previous: Option<MarkWord>,
}

pub(in crate::tagged::gc::mutator_registration) struct FunctionState {
    records: UnsafeCell<Vec<FunctionRecord>>,
    dirty: AtomicBool,
}
// SAFETY: records are prepared only before registration under WRITER. Running
// writes require that same word; stopped visits require the matching keeper
// minted with word zero. Counted readers borrow records only immutably. Only encoded mark
// words and symbol identities cross threads, never a Value or manager view.
unsafe impl Sync for FunctionState {}
impl FunctionState {
    /// Read prepared scalar metadata using the actual matching stopped
    /// receiver, without the serial interner's legacy TLS lookup caches.
    #[cfg(all(
        any(test, feature = "test-support"),
        not(feature = "gc-memory-telemetry")
    ))]
    pub(super) fn lambda_for_keeper(
        &self,
        id: SymId,
        keeper: &KeeperSymbols<'_>,
    ) -> Result<SymId, RegistrationError> {
        if !std::ptr::eq(self, &keeper.registry.symbols.functions) {
            return Err(RegistrationError::StaleRegistration);
        }
        keeper.registry.symbols.ready()?;
        // SAFETY: this exact function state belongs to the retained keeper;
        // its exclusive AllStopped loan and zero word exclude any record
        // preparation, writer or running reader throughout this short copy.
        let records = unsafe { &*self.records.get() };
        records
            .iter()
            .find(|record| record.symbol == id)
            .map(|record| record.lambda)
            .ok_or(RegistrationError::Publication)
    }

    /// # Safety
    /// The matching live symbol reader holds the era's word. No writer or
    /// keeper can change the prepared record vector during this short loan.
    pub(super) unsafe fn lambda_for(
        &self,
        id: SymId,
        _authority: &SymbolReadToken<'_>,
    ) -> Option<SymId> {
        let records = unsafe { &*self.records.get() };
        records
            .iter()
            .find(|record| record.symbol == id)
            .map(|record| record.lambda)
    }
    pub(in crate::tagged::gc::mutator_registration) fn new() -> Self {
        Self {
            records: UnsafeCell::new(Vec::new()),
            dirty: AtomicBool::new(false),
        }
    }
    pub(in crate::tagged::gc::mutator_registration) fn finish_ready(
        &self,
    ) -> Result<(), RegistrationError> {
        if self.dirty.load(Ordering::Acquire) {
            Err(RegistrationError::PendingRoots)
        } else {
            Ok(())
        }
    }
}

/// The initial closed call subset: no callbacks, mutable captures, alias
/// resolution or internal UNBOUND. The shared graph is immutable in this era.
fn validate_definition(value: Value, lambda: SymId, shape: CallShape) -> Result<(), FunctionError> {
    if value.is_nil() {
        return Ok(());
    }
    constant::decode_shape(value, lambda, shape).map(|_| ())
}

impl<K: EraKeeper> EraCore<'_, K> {
    pub(crate) fn reconcile_functions(
        &self,
        symbols: &mut KeeperSymbols<'_>,
    ) -> Result<(), FunctionError> {
        if !std::ptr::eq(self.registry(), symbols.registry) {
            return Err(RegistrationError::StaleRegistration.into());
        }
        let keeper = self
            .keeper
            .as_ref()
            .ok_or(RegistrationError::StaleRegistration)?;
        let designator = keeper.keeper_designator()?;
        symbols.reconcile_functions(keeper, &designator)
    }

    /// Reserve this existing symbol's stamp and one original-target journal
    /// slot before ANY owner registers. No running write grows Rust metadata.
    pub fn prepare_function_symbol(&self, key: &SymbolHandle) -> Result<(), FunctionError> {
        self.prepare_function_subset(key, ReturnSubset::Scalar)
    }

    pub(super) fn prepare_function_subset(
        &self,
        key: &SymbolHandle,
        subset: ReturnSubset,
    ) -> Result<(), FunctionError> {
        self.prepare_function_checked(key, |initial, lambda, authority| {
            if let FunctionCellSnapshot::Bound(value) = initial {
                if validate_definition(value, lambda, CallShape::Nullary(subset)).is_err() {
                    // SAFETY: the retained original definition and this backing's
                    // exclusive admission writer remain live throughout the hook.
                    match unsafe {
                        self.keeper
                            .as_ref()
                            .ok_or(RegistrationError::StaleRegistration)?
                            .classify_initial_definition(value, authority)
                    } {
                        InitialDefinition::OpaqueScalarConstant => {}
                        InitialDefinition::Unsupported => {
                            return Err(FunctionError::UnsupportedDefinition);
                        }
                    }
                }
            }
            Ok(())
        })
    }

    /// One checked constructor for every admitted call shape. Validation must
    /// finish before the immutable record is published; all refusal returns
    /// leave authority under its ordinary RAII writer.
    pub(super) fn prepare_function_checked<T>(
        &self,
        key: &SymbolHandle,
        validate: impl FnOnce(
            FunctionCellSnapshot,
            SymId,
            &SymbolWriteToken<'_>,
        ) -> Result<T, FunctionError>,
    ) -> Result<T, FunctionError> {
        let registry = self.registry();
        let keeper = self
            .keeper
            .as_ref()
            .ok_or(RegistrationError::StaleRegistration)?;
        if registry.keeper_thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread.into());
        }
        if !self
            .legacy_roots
            .as_ref()
            .is_some_and(FrozenLegacyRoots::permits_function_writes)
            || keeper.check_compiler_quiescent().is_err()
        {
            return Err(RegistrationError::Publication.into());
        }
        {
            let gate = registry.lock()?;
            if gate.entries.len() != 1 || gate.rendezvous.phase != Phase::Idle {
                return Err(RegistrationError::Busy.into());
            }
        }
        let symbols = &registry.symbols;
        let designator = keeper.keeper_designator()?;
        if designator.heap() != registry.heap {
            return Err(RegistrationError::WrongHeap.into());
        }
        symbols
            .backing
            .check_root_designators(designator.obarray(), designator.projections())?;
        symbols.validate(key, registry)?;
        if !word::writer_enter(&symbols.word) {
            return Err(RegistrationError::Busy.into());
        }
        let guard = AdmissionWriter(symbols);
        let authority = guard.write_token();
        let lambda =
            crate::intern::lookup_interned("lambda").ok_or(FunctionError::UnsupportedDefinition)?;
        // SAFETY: retained backing, identity checked and exclusive admission.
        let initial = unsafe { symbols.backing.prepare_function(key.symbol, &authority) }?;
        let prepared = validate(initial, lambda, &authority)?;
        // SAFETY: only this keeper can register an owner, and WRITER is held.
        let records = unsafe { &mut *symbols.functions.records.get() };
        if !records.iter().any(|r| r.symbol == key.symbol) {
            records
                .try_reserve(1)
                .map_err(|_| RegistrationError::Capacity)?;
            records.push(FunctionRecord {
                symbol: key.symbol,
                lambda,
                previous: None,
            });
        }
        Ok(prepared)
    }
}

impl<'era> RunAccess<'_, 'era> {
    /// Both operands stay in resident root homes. A no-op consumes no epoch,
    /// stamp, journal slot or allocation; every refusal preserves the answer.
    pub fn sym_fset_into(
        &mut self,
        symbol: &SymbolHandle,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError> {
        self.fset_with_subset(symbol, src, dst, ReturnSubset::Scalar)
    }

    pub(super) fn fset_with_subset(
        &mut self,
        symbol: &SymbolHandle,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
        subset: ReturnSubset,
    ) -> Result<(), FunctionError> {
        self.fset_with_shape(symbol, src, dst, CallShape::Nullary(subset))
    }

    pub(super) fn fset_with_shape(
        &mut self,
        symbol: &SymbolHandle,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
        shape: CallShape,
    ) -> Result<(), FunctionError> {
        globals::check_home(self, src)?;
        globals::check_home(self, dst)?;
        let value = self
            .owner
            .local
            .as_ref()
            .and_then(|local| local.root_scratch.get(src.index))
            .ok_or(RegistrationError::StaleRegistration)?
            .value();
        self.symbol_mut(symbol)?.write_function_into(
            super::super::intern::OwnerRootIndex::from(&*dst),
            FunctionUpdate::Fset(value),
            shape,
        )
    }

    /// Store the explicit-unbound state, returning the original symbol even
    /// for a repeated no-op. Every validation precedes mutation of the answer.
    pub fn sym_fmakunbound_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError> {
        globals::check_home(self, dst)?;
        self.symbol_mut(symbol)?.write_function_into(
            super::super::intern::OwnerRootIndex::from(&*dst),
            FunctionUpdate::Fmakunbound,
            CallShape::Nullary(ReturnSubset::Scalar),
        )
    }
}

pub(in crate::tagged::gc::mutator_registration::symbol_authority) fn publish(
    symbols: &SymbolSidecar,
    id: SymId,
    update: FunctionUpdate,
    dst: &mut MarkWord,
    authority: &SymbolWriteToken<'_>,
    shape: CallShape,
) -> Result<(), FunctionError> {
    // SAFETY: private token comes from the matching identity-checked writer;
    // no running reader touches this vector, and it never grows here.
    let records = unsafe { &mut *symbols.functions.records.get() };
    let record = records
        .iter_mut()
        .find(|r| r.symbol == id)
        .ok_or(RegistrationError::Publication)?;
    let answer = match update {
        FunctionUpdate::Fset(value) => {
            if validate_definition(value, record.lambda, shape).is_err() {
                // An admitted compiled definition may be returned unchanged,
                // never installed in a different cell or executed here. Check
                // exact complete state before epoch planning, preserving the
                // unsupported-input refusal even at epoch exhaustion.
                if !value.is_bytecode()
                    // SAFETY: the same identity-bound WRITER spans this
                    // complete-state check and destination installation.
                    || !unsafe {
                        symbols
                            .backing
                            .function_write_is_noop(id, update, authority)
                    }?
                {
                    return Err(FunctionError::UnsupportedDefinition);
                }
                *dst = MarkWord::of(value);
                return Ok(());
            }
            value
        }
        FunctionUpdate::Fmakunbound => Value::from_sym_id(id),
    };
    // SAFETY: retained World and exclusive word, no callback or structural change.
    let planned = unsafe { symbols.backing.plan_function_write(id, update, authority) }?;
    if let Some(commit) = planned {
        if record.previous.is_none() {
            record.previous = Some(MarkWord::of(commit.previous));
        }
        symbols.functions.dirty.store(true, Ordering::Release);
        // Original legacy target is now a stopped trace obligation. Every
        // remaining store is allocation-free; unexpected refusal is terminal.
        if let Err(error) = unsafe {
            symbols
                .backing
                .commit_function_write(id, update, commit, authority)
        } {
            symbols.poison();
            return Err(error);
        }
    }
    *dst = MarkWord::of(answer);
    Ok(())
}

impl KeeperSymbols<'_> {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(in crate::tagged::gc::mutator_registration) fn append_function_roots(
        &self,
        roots: &mut Vec<Value>,
    ) -> Result<(), RegistrationError> {
        self.registry.symbols.ready()?;
        // SAFETY: the matching AllStopped remains exclusively borrowed; no
        // writer can modify journal entries during this stopped trace.
        let records = unsafe { &*self.registry.symbols.functions.records.get() };
        roots.extend(
            records
                .iter()
                .filter_map(|r| r.previous)
                .map(MarkWord::value),
        );
        Ok(())
    }

    /// The retained serial keeper owns these TLS caches. Original admission
    /// retirement occurs in its current returning Context receiver, before
    /// serial execution; no header pointer can outlive that receiver's loan.
    fn reconcile_functions<K: EraKeeper>(
        &mut self,
        keeper: &K,
        designator: &super::super::KeeperWorldDesignator<'_>,
    ) -> Result<(), FunctionError> {
        self.check_symbol_backing(&self.registry.symbols.backing, designator)?;
        self.registry.symbols.ready()?;
        if keeper.check_compiler_quiescent().is_err() {
            return Err(RegistrationError::Publication.into());
        }
        struct PoisonOnUnwind<'a> {
            registry: &'a Registry,
            armed: bool,
        }
        impl Drop for PoisonOnUnwind<'_> {
            fn drop(&mut self) {
                if self.armed {
                    self.registry.symbols.poison();
                    self.registry.poison();
                }
            }
        }
        let mut guard = PoisonOnUnwind {
            registry: self.registry,
            armed: true,
        };
        // SAFETY: exclusive matching AllStopped, keeper thread, zero word.
        let state = &self.registry.symbols.functions;
        let records = unsafe { &mut *state.records.get() };
        for record in records.iter().filter(|r| r.previous.is_some()) {
            // SAFETY: this matching keeper retains AllStopped with word
            // zero; the compiler preflight above preceded journal access.
            unsafe {
                keeper.reconcile_function_write(record.symbol, self);
            }
        }
        for record in records {
            record.previous = None;
        }
        state.dirty.store(false, Ordering::Release);
        guard.armed = false;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support"))]
impl RunAccess<'_, '_> {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn function_facts_for_test(
        &mut self,
        key: &SymbolHandle,
    ) -> Result<(u64, u64, bool), FunctionError> {
        let lease = self.symbol_ref(key)?;
        let token = lease.read_token();
        let symbols = &lease.run.owner.registry.symbols;
        let (epoch, stamp) = unsafe { symbols.backing.function_facts_for_test(key.symbol, &token) }
            .ok_or(RegistrationError::Publication)?;
        Ok((
            epoch,
            stamp,
            symbols.functions.dirty.load(Ordering::Acquire),
        ))
    }
}
