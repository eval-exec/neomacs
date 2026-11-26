//! Slot-to-slot Plain globals. No legacy TLS or raw-value public API.
use super::intern::OwnerRootIndex;
use super::*;
use crate::intern::parallel::RetainedBytes;
use crate::symbol::parallel::FunctionUpdate;
use crate::tagged::gc::mark_word::MarkWord;
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalError {
    Authority(RegistrationError),
    VoidVariable,
}

impl From<RegistrationError> for GlobalError {
    fn from(error: RegistrationError) -> Self {
        Self::Authority(error)
    }
}

/// No constructor outside this module. The word reference is borrowed only
/// while the owning SymbolRef remains live; the token cannot cross threads.
pub(crate) struct SymbolReadToken<'lease> {
    _word: &'lease AtomicU64,
    _thread: PhantomData<Rc<()>>,
}

/// Only a live writer operation can construct this token. Shared cell stores
/// use it instead of the serial TLS mark gate; the era excludes the marker.
#[doc(hidden)]
pub struct SymbolWriteToken<'lease> {
    _word: &'lease AtomicU64,
    _thread: PhantomData<Rc<()>>,
}

pub(in crate::tagged::gc::mutator_registration) fn check_home(
    run: &RunAccess<'_, '_>,
    home: &OwnerRootSlot<'_>,
) -> Result<(), RegistrationError> {
    let local = run
        .owner
        .local
        .as_ref()
        .ok_or(RegistrationError::StaleRegistration)?;
    home.check_binding(
        run.owner.entry.key,
        local.root_slot_generation,
        local.root_scratch_len(),
    )
}

impl AdmissionWriter<'_> {
    pub(super) fn write_token(&self) -> SymbolWriteToken<'_> {
        SymbolWriteToken {
            _word: &self.0.word,
            _thread: PhantomData,
        }
    }
}

impl super::intern::InitialObarrayRef<'_, '_, '_> {
    pub(super) fn read_token(&self) -> SymbolReadToken<'_> {
        SymbolReadToken {
            _word: &self.run.owner.registry.symbols.word,
            _thread: PhantomData,
        }
    }
}

impl super::intern::InitialObarrayMut<'_, '_, '_> {
    pub(super) fn write_token(&self) -> SymbolWriteToken<'_> {
        SymbolWriteToken {
            _word: &self.run.owner.registry.symbols.word,
            _thread: PhantomData,
        }
    }

    pub(super) fn intern_into(
        &mut self,
        name_index: OwnerRootIndex,
        dst_index: OwnerRootIndex,
    ) -> Result<(), super::intern::InitialInternError> {
        let registry = self.run.owner.registry;
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let word = local
            .root_slot_word(usize::from(name_index))
            .ok_or(RegistrationError::StaleRegistration)?;
        // SAFETY: `self.run` is an active thread-checked run on the owning
        // mutator; this root slot stays live in its resident root bank.
        let value = unsafe { word.value() };
        let dst_slot = usize::from(dst_index);
        if dst_slot >= local.root_scratch_len() {
            return Err(RegistrationError::StaleRegistration.into());
        }
        let authority = SymbolWriteToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        let mut charged = RetainedBytes::default();
        let result = super::intern::publish_initial(
            &registry.symbols,
            registry.heap,
            value,
            &mut charged,
            &authority,
        );
        // Retained reservations remain chargeable on a typed refusal, too.
        // This is the one conversion back to the existing collector counter.
        local.hot.bytes_since_gc = local
            .hot
            .bytes_since_gc
            .saturating_add(usize::from(charged));
        let answer = result?;
        // SAFETY: `answer` is a bare symbol produced by `publish_initial` for
        // this active era; `dst_slot` was validated resident above.
        unsafe { local.set_root_slot(dst_slot, answer) }
            .ok_or(RegistrationError::StaleRegistration)?;
        Ok(())
    }
}

impl<'era> RunAccess<'_, 'era> {
    /// A void error leaves dst unchanged. The loaded value becomes a root
    /// before the counted-reader lease is released, including heap pointers.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn sym_value_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), GlobalError> {
        check_home(self, dst)?;
        self.symbol_ref(symbol)?.copy_into(dst.index, false)
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn sym_boundp_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), GlobalError> {
        check_home(self, dst)?;
        self.symbol_ref(symbol)?.copy_into(dst.index, true)
    }
}

impl SymbolRef<'_, '_, '_> {
    /// The constructor stays beside the token's private fields. Only a real
    /// identity-checked reader lease can mint this short backing admission.
    pub(super) fn read_token(&self) -> SymbolReadToken<'_> {
        SymbolReadToken {
            _word: &self.run.owner.registry.symbols.word,
            _thread: PhantomData,
        }
    }

    fn copy_into(&mut self, index: usize, boundp: bool) -> Result<(), GlobalError> {
        let registry = self.run.owner.registry;
        // Validate destination slot before loading: no fallible work or poll is
        // permitted between the load and root installation.
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .filter(|local| index < local.root_scratch_len())
            .ok_or(RegistrationError::StaleRegistration)?;
        let authority = SymbolReadToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        // SAFETY: this lease validates identity and holds the real reader word;
        // World/backing is retained, and index belongs to this resident owner.
        let value = unsafe {
            registry
                .symbols
                .backing
                .read_plain(self.key.symbol, &authority)
        }
        .ok_or(RegistrationError::Publication)?;
        let answer = if boundp {
            Value::bool_val(!value.is_unbound())
        } else if value.is_unbound() {
            return Err(GlobalError::VoidVariable);
        } else {
            value
        };
        // SAFETY: `answer` came from this active era's retained backing under
        // `SymbolRef` and is installed into this owner's validated root slot.
        unsafe { local.set_root_slot(index, answer) }
            .ok_or(RegistrationError::StaleRegistration)?;
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(SymbolReadToken<'static>: Send, Sync, Clone, Copy);

impl<'era> RunAccess<'_, 'era> {
    /// Validate both homes before authority; aliases load the source first.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn sym_set_into(
        &mut self,
        symbol: &SymbolHandle,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), GlobalError> {
        check_home(self, src)?;
        check_home(self, dst)?;
        let word = self
            .owner
            .local
            .as_ref()
            .and_then(|local| local.root_slot_word(src.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        // SAFETY: `check_home` verified this slot belongs to the active
        // owning mutator thread's live root bank.
        let value = unsafe { word.value() };
        if value.is_unbound() {
            // UNBOUND is internal storage state, never a Lisp set operand.
            return Err(RegistrationError::Publication.into());
        }
        self.symbol_mut(symbol)?.store_into(dst.index, value, value)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn sym_makunbound_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), GlobalError> {
        check_home(self, dst)?;
        let mut lease = self.symbol_mut(symbol)?;
        let answer = Value::from_sym_id(lease.key.symbol);
        lease.store_into(dst.index, Value::UNBOUND, answer)
    }
}

impl SymbolMut<'_, '_, '_> {
    pub(super) fn write_function_into(
        &mut self,
        index: OwnerRootIndex,
        update: FunctionUpdate,
        shape: super::functions::CallShape,
    ) -> Result<(), FunctionError> {
        let registry = self.run.owner.registry;
        let dst_slot = usize::from(index);
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .filter(|local| dst_slot < local.root_scratch_len())
            .ok_or(RegistrationError::StaleRegistration)?;
        let authority = SymbolWriteToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        let answer = functions::write::publish(
            &registry.symbols,
            self.key.symbol,
            update,
            &authority,
            shape,
        )?;
        // SAFETY: `answer` belongs to this active era's heap/symbol table and
        // `dst_slot` was validated resident above.
        unsafe { local.set_root_slot(dst_slot, answer) }
            .ok_or(RegistrationError::StaleRegistration)?;
        Ok(())
    }

    fn store_into(&mut self, index: usize, value: Value, answer: Value) -> Result<(), GlobalError> {
        let registry = self.run.owner.registry;
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .filter(|local| index < local.root_scratch_len())
            .ok_or(RegistrationError::StaleRegistration)?;
        let authority = SymbolWriteToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        // SAFETY: the identity-checked lease holds WRITER, and the same era
        // excludes every marker and legacy World view. No allocation can poll.
        let (result, grown) = unsafe {
            registry
                .symbols
                .backing
                .write_plain(self.key.symbol, value, &authority)
        };
        // Charge Rust projection storage even if fallible reservation refused
        // after enlarging one metadata vector. No TLS and no heap-object card.
        local.hot.bytes_since_gc = local.hot.bytes_since_gc.saturating_add(grown);
        if result == Err(RegistrationError::Poisoned) {
            // Publication refused after descriptor insertion: fail closed
            // before releasing WRITER; Drop poisons the control gate afterward.
            registry.symbols.poison();
        }
        result?;
        // SAFETY: `answer` came from this owner's validated root bank or symbol
        // ID and stays live under the active era protocol.
        unsafe { local.set_root_slot(index, answer) }
            .ok_or(RegistrationError::StaleRegistration)?;
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(SymbolWriteToken<'static>: Send, Sync, Clone, Copy);
