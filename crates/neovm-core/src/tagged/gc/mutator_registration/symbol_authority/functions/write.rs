//! Prepared, bounded function writes and stopped serial-cache reconciliation.
use super::*;
use crate::emacs_core::symbol::parallel::FunctionUpdate;
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
    #[cfg(all(test, not(feature = "gc-memory-telemetry")))]
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
fn validate_definition(
    value: Value,
    lambda: SymId,
    subset: ReturnSubset,
) -> Result<(), FunctionError> {
    if value.is_nil() {
        return Ok(());
    }
    constant::decode(value, lambda, subset).map(|_| ())
}

/// Serial preparation may retain an existing compiled constant as an opaque
/// read/replacement target. Never materialize a saved-image function here,
/// and never inspect bytecode data during an overlapping owner operation.
/// Restricted execution and changed fset inputs still validate their typed subset.
fn opaque_initial_bytecode(value: Value) -> bool {
    use crate::emacs_core::bytecode::Op;

    value.bytecode_data_if_materialized().is_some_and(|f| {
        f.lexical
            && f.params.fixed_arity() == Some(0)
            && f.arglist.is_nil()
            && f.ops_sealed
            && f.stack_verified
            && f.ops.as_slice() == [Op::Constant(0), Op::Return]
            && f.constants.len() == 1
            && f.constants
                .first()
                .is_some_and(|answer| answer.is_nil() || answer.is_t() || answer.is_fixnum())
            && f.env.is_none()
            && f.interactive.is_none()
            && f.doc_form.is_none()
            && f.docstring.is_none()
            && f.extra_slots.is_empty()
            && f.gnu_bytecode_bytes.is_none()
            && f.gnu_byte_offset_map.is_none()
            && f.lazy_gnu_code.is_none()
    })
}

fn keeper_compiler_quiescent() -> bool {
    #[cfg(feature = "jit")]
    {
        return crate::emacs_core::jit::cache::native_depth() == 0
            && crate::emacs_core::jit::bg::pending_count() == 0;
    }
    #[cfg(not(feature = "jit"))]
    true
}

impl ParallelHeapEra<'_> {
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
        let registry = self.registry();
        if registry.keeper_thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread.into());
        }
        if !self
            .legacy_roots
            .as_ref()
            .is_some_and(FrozenLegacyRoots::permits_function_writes)
            || !keeper_compiler_quiescent()
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
        symbols.validate(key, registry)?;
        if !word::writer_enter(&symbols.word) {
            return Err(RegistrationError::Busy.into());
        }
        let guard = AdmissionWriter(symbols);
        let authority = guard.write_token();
        let lambda = crate::emacs_core::intern::lookup_interned("lambda")
            .ok_or(FunctionError::UnsupportedDefinition)?;
        // SAFETY: retained backing, identity checked and exclusive admission.
        let initial = unsafe { symbols.backing.prepare_function(key.symbol, &authority) }?;
        if let FunctionCellSnapshot::Bound(value) = initial {
            if validate_definition(value, lambda, subset).is_err()
                && !opaque_initial_bytecode(value)
            {
                return Err(FunctionError::UnsupportedDefinition);
            }
        }
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
        Ok(())
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
            subset,
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
            ReturnSubset::Scalar,
        )
    }
}

pub(in crate::tagged::gc::mutator_registration::symbol_authority) fn publish(
    symbols: &SymbolSidecar,
    id: SymId,
    update: FunctionUpdate,
    dst: &mut MarkWord,
    authority: &SymbolWriteToken<'_>,
    subset: ReturnSubset,
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
            if validate_definition(value, record.lambda, subset).is_err() {
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
    pub fn reconcile_functions(&mut self) -> Result<(), FunctionError> {
        self.registry.symbols.ready()?;
        if !keeper_compiler_quiescent() {
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
            #[cfg(feature = "jit")]
            crate::emacs_core::jit::cache::evict_inline_dependents(record.symbol);
            crate::emacs_core::eval::note_function_epoch_move(
                crate::emacs_core::symbol::FunctionEpochBump::InternalCellWrite,
                Some(record.symbol),
            );
        }
        for record in records {
            record.previous = None;
        }
        state.dirty.store(false, Ordering::Release);
        guard.armed = false;
        Ok(())
    }
}

#[cfg(test)]
impl RunAccess<'_, '_> {
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
