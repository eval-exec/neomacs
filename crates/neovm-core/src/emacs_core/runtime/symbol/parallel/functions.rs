//! Cold function publication under the retained World's symbol word.
use super::*;
use crate::tagged::gc::{FunctionError, RegistrationError, SymbolWriteToken};
use std::sync::atomic::Ordering;

/// The closed Lisp writers retain their reason; fmakunbound has no payload.
#[derive(Clone, Copy)]
pub(crate) enum FunctionUpdate {
    Fset(super::super::Value),
    Fmakunbound,
}
impl FunctionUpdate {
    pub(crate) fn parts(self) -> (super::super::Value, bool) {
        match self {
            Self::Fset(value) => (value, false),
            Self::Fmakunbound => (super::super::Value::NIL, true),
        }
    }

    fn reason(self) -> super::super::FunctionEpochBump {
        match self {
            Self::Fset(_) => super::super::FunctionEpochBump::Fset,
            Self::Fmakunbound => super::super::FunctionEpochBump::Fmakunbound,
        }
    }

    fn matches(self, value: super::super::Value, explicitly_unbound: bool) -> bool {
        let (desired, desired_unbound) = self.parts();
        desired_unbound == explicitly_unbound && desired.bits() == value.bits()
    }
}

pub(crate) struct FunctionCommit {
    pub(crate) previous: super::super::Value,
    epoch: u64,
}

impl SymbolBacking {
    /// Identity-only no-op check for an opaque initial bytecode definition.
    /// No bytecode payload, runtime metadata, stamp or clock is read.
    /// # Safety
    /// The matching live writer excludes every reader and legacy view.
    pub(crate) unsafe fn function_write_is_noop(
        &self,
        id: SymId,
        update: FunctionUpdate,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<bool, FunctionError> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return Err(RegistrationError::Publication.into());
        }
        let symbol = obarray
            .get_by_id(id)
            .ok_or(RegistrationError::Publication)?;
        Ok(update.matches(symbol.function.get(), symbol.function_unbound))
    }

    /// The ORIGINAL canonical clock is loaded before testing the existing
    /// per-symbol floor/stamp rule. The caller must retain this exact clock.
    /// # Safety
    /// The matching counted reader retains this World and stable symbol spine.
    pub(crate) unsafe fn function_version(
        &self,
        id: SymId,
        since: Option<u64>,
        _authority: &crate::tagged::gc::SymbolReadToken<'_>,
    ) -> Option<(u64, bool)> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation || obarray.get_by_id(id).is_none() {
            return None;
        }
        let clock = obarray.function_epoch();
        let unchanged = since.is_some_and(|since| {
            since != u64::MAX && (since == clock || obarray.fn_unchanged_since(id, since))
        });
        Some((clock, unchanged))
    }

    /// # Safety
    /// The matching serial admission holds WRITER, before owner registration.
    pub(crate) unsafe fn prepare_function(
        &self,
        id: SymId,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<super::super::FunctionCellSnapshot, FunctionError> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation || obarray.get_by_id(id).is_none() {
            return Err(RegistrationError::Publication.into());
        }
        let side = obarray
            .symbols
            .sides
            .get(Obarray::slot_index(id) >> 12)
            .ok_or(RegistrationError::Publication)?;
        side.try_prepare_stamps()
            .ok_or(RegistrationError::Capacity)?;
        Ok(obarray.function_cell_snapshot(id))
    }

    /// # Safety
    /// The matching live writer excludes every reader, marker and legacy view.
    pub(crate) unsafe fn plan_function_write(
        &self,
        id: SymId,
        update: FunctionUpdate,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<Option<FunctionCommit>, FunctionError> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return Err(RegistrationError::Publication.into());
        }
        let symbol = obarray
            .get_by_id(id)
            .ok_or(RegistrationError::Publication)?;
        if update.matches(symbol.function.get(), symbol.function_unbound) {
            return Ok(None);
        }
        let epoch = obarray
            .function_epoch()
            .checked_add(1)
            .filter(|&epoch| epoch != u64::MAX)
            .ok_or(FunctionError::EpochExhausted)?;
        let side = obarray
            .symbols
            .sides
            .get(Obarray::slot_index(id) >> 12)
            .ok_or(RegistrationError::Publication)?;
        if !side.stamps_prepared() {
            return Err(RegistrationError::Publication.into());
        }
        Ok(Some(FunctionCommit {
            previous: symbol.function.get(),
            epoch,
        }))
    }

    /// # Safety
    /// WRITER spans plan, journal retention and this commit. No callback or
    /// reference escapes. Derive the exclusive metadata loan from the original
    /// UnsafeCell root, after all symbol readers and legacy loans have ended.
    pub(crate) unsafe fn commit_function_write(
        &self,
        id: SymId,
        update: FunctionUpdate,
        commit: FunctionCommit,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<(), FunctionError> {
        let obarray = unsafe { &mut *self.obarray.as_ptr() };
        let index = Obarray::slot_index(id);
        if obarray.generation() != self.generation
            || obarray.function_epoch().checked_add(1) != Some(commit.epoch)
            || !obarray
                .symbols
                .sides
                .get(index >> 12)
                .is_some_and(|s| s.stamps_prepared())
        {
            return Err(RegistrationError::Poisoned.into());
        }
        let symbol = obarray
            .symbols
            .get_mut(index)
            .ok_or(RegistrationError::Poisoned)?;
        // Use the landed interior-mutable cell, never an atomic cast or a
        // serial setter (which would touch another owner's TLS and SATB).
        let (value, explicitly_unbound) = update.parts();
        // The flag/cell invariant (`LispSymbol::function_unbound`): an
        // explicitly-unbound update always carries NIL. Placed directly
        // after `parts()` so the C5 interning work can edit the rest of
        // this function without a textual conflict.
        debug_assert!(
            !explicitly_unbound || value.is_nil(),
            "a function-unbound update must carry NIL"
        );
        symbol.function.store_atomic(value);
        symbol.function_unbound = explicitly_unbound;
        let published = obarray.symbols.sides.get(index >> 12).is_some_and(|side| {
            side.stamp_prepared(index & (super::super::OBARRAY_CHUNK - 1), commit.epoch)
        });
        if !published {
            return Err(RegistrationError::Poisoned.into());
        }
        obarray
            .dispatch_cells
            .function_epoch
            .store(commit.epoch, Ordering::Release);
        // Exactly the legacy attribution, on this committing owner's existing
        // counters. No serial setter, Context lookup, new TLS or atomic count.
        let why = update.reason();
        crate::emacs_core::eval::note_function_epoch_move(why, Some(id));
        #[cfg(feature = "jit")]
        if let Some(observer) = crate::emacs_core::symbol::function_cell_observer() {
            (observer.epoch_bump)(why, Some(id));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) unsafe fn function_facts_for_test(
        &self,
        id: SymId,
        _authority: &crate::tagged::gc::SymbolReadToken<'_>,
    ) -> Option<(u64, u64)> {
        let obarray = unsafe { self.obarray.as_ref() };
        let index = Obarray::slot_index(id);
        let side = obarray.symbols.sides.get(index >> 12)?;
        Some((
            obarray.function_epoch(),
            side.stamp(index & (super::super::OBARRAY_CHUNK - 1)),
        ))
    }
}
