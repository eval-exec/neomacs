//! Initial-obarray name operations: operands and answers remain owner roots.
use super::*;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue as Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialInternError {
    Authority(RegistrationError),
    ExpectedString,
    ExpectedPlainSymbol,
    UnsupportedName,
    /// This incremental subset admits existing registry identities only.
    NewRegistryName,
}
impl From<RegistrationError> for InitialInternError {
    fn from(error: RegistrationError) -> Self {
        Self::Authority(error)
    }
}

/// An era/World-bound obarray reader, without a pre-existing symbol key.
/// It borrows RunAccess until release and shares the C2/C3 authority word.
pub(super) struct InitialObarrayRef<'lease, 'owner, 'era> {
    pub(super) run: &'lease mut RunAccess<'owner, 'era>,
}
pub(super) struct InitialObarrayMut<'lease, 'owner, 'era> {
    pub(super) run: &'lease mut RunAccess<'owner, 'era>,
}
impl Drop for InitialObarrayMut<'_, '_, '_> {
    fn drop(&mut self) {
        let registry = self.run.owner.registry;
        if std::thread::panicking() {
            registry.symbols.poison();
        }
        let poisoned = registry.symbols.poisoned.load(Ordering::Acquire);
        word::writer_exit(&registry.symbols.word);
        self.run.symbol_loan = false;
        if poisoned {
            registry.poison();
        }
    }
}
impl Drop for InitialObarrayRef<'_, '_, '_> {
    fn drop(&mut self) {
        word::reader_exit(&self.run.owner.registry.symbols.word);
        self.run.symbol_loan = false;
    }
}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    #[cfg(test)]
    pub(crate) fn initial_fault_for_test(
        &mut self,
        fault: crate::emacs_core::symbol::parallel::InitialInternFault,
    ) -> Result<(), RegistrationError> {
        let lease = self.initial_obarray_mut()?;
        let authority = lease.write_token();
        // SAFETY: same retained World, no concurrent operation, WRITER held.
        unsafe {
            lease
                .run
                .owner
                .registry
                .symbols
                .backing
                .arm_initial_fault_for_test(fault, &authority)
        };
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn initial_poison_receipt_for_test(&self) -> (bool, bool, u8) {
        let symbols = &self.owner.registry.symbols;
        (
            symbols.poisoned.load(Ordering::Acquire),
            word::quiescent(&symbols.word),
            symbols.backing.initial_fault_receipt_for_test(),
        )
    }

    #[cfg(test)]
    pub(crate) fn initial_facts_for_test(
        &mut self,
        id: SymId,
    ) -> Result<crate::emacs_core::symbol::parallel::InitialInternFacts, RegistrationError> {
        let lease = self.initial_obarray_mut()?;
        let authority = lease.write_token();
        // SAFETY: assertion-only snapshot under the actual matching writer.
        unsafe {
            lease
                .run
                .owner
                .registry
                .symbols
                .backing
                .initial_facts_for_test(id, &authority)
        }
    }

    /// Running admission is from a validated owner home, never an arbitrary
    /// SymId. The exclusive word checks actual membership and canonical arm.
    pub fn initial_symbol_handle(
        &mut self,
        symbol: &OwnerRootSlot<'era>,
    ) -> Result<SymbolHandle, InitialInternError> {
        globals::check_home(self, symbol)?;
        let lease = self.initial_obarray_mut()?;
        let value = lease
            .run
            .owner
            .local
            .as_ref()
            .and_then(|local| local.root_scratch.get(symbol.index))
            .ok_or(RegistrationError::StaleRegistration)?
            .value();
        let id = value
            .as_bare_symbol_id()
            .ok_or(InitialInternError::ExpectedPlainSymbol)?;
        let registry = lease.run.owner.registry;
        let authority = lease.write_token();
        // SAFETY: exact era/World WRITER, validated real rooted symbol operand.
        let kind = unsafe {
            registry
                .symbols
                .backing
                .classify_initial_member(id, &authority)
        }
        .ok_or(RegistrationError::Publication)?;
        Ok(SymbolHandle {
            heap: registry.heap,
            world: registry.symbols.backing.world,
            era: registry.symbols.era,
            obarray: registry.symbols.backing.generation,
            symbol: id,
            kind,
        })
    }

    fn initial_obarray_mut(
        &mut self,
    ) -> Result<InitialObarrayMut<'_, 'owner, 'era>, RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let symbols = &self.owner.registry.symbols;
        if symbols.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if !word::writer_enter(&symbols.word) {
            return Err(RegistrationError::Busy);
        }
        if symbols.poisoned.load(Ordering::Acquire) {
            word::writer_exit(&symbols.word);
            return Err(RegistrationError::Poisoned);
        }
        self.symbol_loan = true;
        Ok(InitialObarrayMut { run: self })
    }

    /// Existing canonical registry identities acquire actual initial-obarray
    /// membership in this World. Gaps stay absent; present cells keep all
    /// their payloads. A fresh registry name is explicitly outside this stage.
    pub fn initial_intern_into(
        &mut self,
        name: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InitialInternError> {
        globals::check_home(self, name)?;
        globals::check_home(self, dst)?;
        self.initial_obarray_mut()?
            .intern_into(name.index, dst.index)
    }

    fn initial_obarray_ref(
        &mut self,
    ) -> Result<InitialObarrayRef<'_, 'owner, 'era>, RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let symbols = &self.owner.registry.symbols;
        if symbols.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if !word::reader_enter(&symbols.word) {
            return Err(RegistrationError::Busy);
        }
        if symbols.poisoned.load(Ordering::Acquire) {
            word::reader_exit(&symbols.word);
            return Err(RegistrationError::Poisoned);
        }
        self.symbol_loan = true;
        Ok(InitialObarrayRef { run: self })
    }

    /// Ordinary initial-obarray intern-soft. NIL is a successful miss;
    /// contention is a typed refusal, never a miss. Both outcomes leave every
    /// registry/World storage and semantic clock unchanged. String contents
    /// are immutable for this closed era; no shorthand or callback is run.
    pub fn initial_intern_soft_into(
        &mut self,
        name: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InitialInternError> {
        globals::check_home(self, name)?;
        globals::check_home(self, dst)?;
        self.initial_obarray_ref()?.soft_into(name.index, dst.index)
    }
}

impl InitialObarrayRef<'_, '_, '_> {
    fn soft_into(&mut self, name_index: usize, dst_index: usize) -> Result<(), InitialInternError> {
        let answer = {
            let local = self
                .run
                .owner
                .local
                .as_ref()
                .ok_or(RegistrationError::StaleRegistration)?;
            let value = local
                .root_scratch
                .get(name_index)
                .ok_or(RegistrationError::StaleRegistration)?
                .value();
            let name = value
                .as_lisp_string()
                .ok_or(InitialInternError::ExpectedString)?;
            // Keyword creation and keyword-tagged answers are outside this
            // proof's ordinary-name set, including its soft-lookup operator.
            if name.as_bytes().first() == Some(&b':') {
                return Err(InitialInternError::UnsupportedName);
            }
            let authority = self.read_token();
            match crate::emacs_core::intern::parallel::lookup(name, &authority)? {
                None => Value::NIL,
                Some(id) => {
                    // SAFETY: this lease borrows the exact era/World backing
                    // and holds its counted-reader word. No manager escapes.
                    let member = unsafe {
                        self.run
                            .owner
                            .registry
                            .symbols
                            .backing
                            .initial_contains(id, &authority)
                    }
                    .ok_or(RegistrationError::Publication)?;
                    if member {
                        Value::from_sym_id(id)
                    } else {
                        Value::NIL
                    }
                }
            }
        };
        // Source was copied before this store, so an aliased operand works.
        // No allocation/callback/poll occurs between selection and rooting,
        // and the reader remains held through installation into the real home.
        let dst = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(dst_index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *dst = MarkWord::of(answer);
        Ok(())
    }
}
static_assertions::assert_not_impl_any!(InitialObarrayRef<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(InitialObarrayMut<'static, 'static, 'static>: Send, Sync, Clone, Copy);

/// The caller installed destination and accounting loans before publication.
/// WRITER and the process registry guard span the entire membership commit.
pub(super) fn publish_existing(
    symbols: &SymbolSidecar,
    value: Value,
    dst: &mut MarkWord,
    charged: &mut usize,
    authority: &SymbolWriteToken<'_>,
) -> Result<(), InitialInternError> {
    let name = value
        .as_lisp_string()
        .ok_or(InitialInternError::ExpectedString)?;
    if name.as_bytes().first() == Some(&b':') {
        return Err(InitialInternError::UnsupportedName);
    }
    let registry = crate::emacs_core::intern::parallel::InitialRegistryWriter::enter(authority)?;
    let metadata = registry
        .existing(name)?
        .ok_or(InitialInternError::NewRegistryName)?;
    // SAFETY: exact era/World writer, with its guarded canonical metadata.
    // No recursive registry, serial projection-scope or mark-gate entry runs.
    let (result, grown) = unsafe { symbols.backing.ensure_initial_member(metadata, authority) };
    *charged = charged.saturating_add(grown);
    if result == Err(RegistrationError::Poisoned) {
        symbols.poison();
        #[cfg(test)]
        symbols.backing.record_initial_poison_for_test(
            symbols.poisoned.load(Ordering::Acquire)
                && symbols.word.load(Ordering::Acquire) == super::super::authority_word::WRITER,
        );
    }
    result?;
    *dst = MarkWord::of(Value::from_sym_id(metadata.id));
    Ok(())
}
