//! Slot-to-slot Plain globals. No legacy TLS or raw-value public API.
use super::intern::OwnerRootIndex;
use super::*;
use crate::emacs_core::intern::parallel::RetainedBytes;
use crate::emacs_core::symbol::parallel::FunctionUpdate;
use crate::emacs_core::value::Value;
use crate::tagged::gc::mark_word::MarkWord;

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
pub(crate) struct SymbolWriteToken<'lease> {
    _word: &'lease AtomicU64,
    _thread: PhantomData<Rc<()>>,
}

pub(super) fn check_home(
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
        local.root_scratch.len(),
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
        let value = local
            .root_scratch
            .get(usize::from(name_index))
            .ok_or(RegistrationError::StaleRegistration)?
            .value();
        let dst = local
            .root_scratch
            .get_mut(usize::from(dst_index))
            .ok_or(RegistrationError::StaleRegistration)?;
        let authority = SymbolWriteToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        let mut charged = RetainedBytes::default();
        let result = super::intern::publish_initial(
            &registry.symbols,
            registry.heap,
            value,
            dst,
            &mut charged,
            &authority,
        );
        // Retained reservations remain chargeable on a typed refusal, too.
        // This is the one conversion back to the existing collector counter.
        local.hot.bytes_since_gc = local
            .hot
            .bytes_since_gc
            .saturating_add(usize::from(charged));
        result
    }
}

impl<'era> RunAccess<'_, 'era> {
    /// A void error leaves dst unchanged. The loaded value becomes a root
    /// before the counted-reader lease is released, including heap pointers.
    pub fn sym_value_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), GlobalError> {
        check_home(self, dst)?;
        self.symbol_ref(symbol)?.copy_into(dst.index, false)
    }

    pub fn sym_boundp_into(
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
        // Obtain the destination before loading: no fallible work or poll is
        // permitted between the load and root installation.
        let dst = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(index))
            .ok_or(RegistrationError::StaleRegistration)?;
        let authority = SymbolReadToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        // SAFETY: this lease validates identity and holds the real reader word;
        // World/backing is retained, and dst belongs to this resident owner.
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
        *dst = MarkWord::of(answer);
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(SymbolReadToken<'static>: Send, Sync, Clone, Copy);

impl<'era> RunAccess<'_, 'era> {
    /// Validate both homes before authority; aliases load the source first.
    pub fn sym_set_into(
        &mut self,
        symbol: &SymbolHandle,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), GlobalError> {
        check_home(self, src)?;
        check_home(self, dst)?;
        let value = self
            .owner
            .local
            .as_ref()
            .and_then(|local| local.root_scratch.get(src.index))
            .ok_or(RegistrationError::StaleRegistration)?
            .value();
        if value.is_unbound() {
            // UNBOUND is internal storage state, never a Lisp set operand.
            return Err(RegistrationError::Publication.into());
        }
        self.symbol_mut(symbol)?.store_into(dst.index, value, value)
    }

    pub fn sym_makunbound_into(
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
        subset: super::functions::ReturnSubset,
    ) -> Result<(), FunctionError> {
        let registry = self.run.owner.registry;
        let dst = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(usize::from(index)))
            .ok_or(RegistrationError::StaleRegistration)?;
        let authority = SymbolWriteToken {
            _word: &registry.symbols.word,
            _thread: PhantomData,
        };
        functions::write::publish(
            &registry.symbols,
            self.key.symbol,
            update,
            dst,
            &authority,
            subset,
        )
    }

    fn store_into(&mut self, index: usize, value: Value, answer: Value) -> Result<(), GlobalError> {
        let registry = self.run.owner.registry;
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let dst = local
            .root_scratch
            .get_mut(index)
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
        *dst = MarkWord::of(answer);
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(SymbolWriteToken<'static>: Send, Sync, Clone, Copy);
