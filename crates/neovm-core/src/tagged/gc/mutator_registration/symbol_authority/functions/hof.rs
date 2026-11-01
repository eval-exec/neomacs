//! Canonical mapper identity and callback selection share one symbol reader.
use super::*;
use crate::tagged::gc::function_write_hooks::{
    HofOperation, HofPreparationError, HofRegistrationClock, HofRegistrationEpoch,
    StaticSubrIdentity,
};

/// The fixed-spelling canonical builtin is sealed during exclusive preparation.
/// It owns no chunk, managed value, function pointer or executable entry.
#[must_use = "retain this receipt for the matching era and mapper operation"]
#[derive(Clone, Copy)]
pub(crate) struct PreparedHofBuiltin {
    operation: HofOperation,
    key: SymbolHandle,
    identity: StaticSubrIdentity,
    registration_epoch: HofRegistrationEpoch,
    registration_clock: HofRegistrationClock,
}
impl PreparedHofBuiltin {
    pub(crate) fn operation(&self) -> HofOperation {
        self.operation
    }
    pub(crate) fn symbol(&self) -> &SymbolHandle {
        &self.key
    }
}

impl<K: EraKeeper> EraCore<'_, K> {
    pub(crate) fn prepare_hof_builtin(
        &self,
        operation: HofOperation,
        key: &SymbolHandle,
    ) -> Result<PreparedHofBuiltin, FunctionError> {
        self.prepare_function_checked(key, |initial, _lambda, authority| {
            // SAFETY: the shared checked constructor retains this backing's
            // exclusive admission writer before ANY owner registration. The
            // cold hook resolves a fixed canonical builtin, never this cell.
            let seed = unsafe {
                self.keeper
                    .as_ref()
                    .ok_or(RegistrationError::StaleRegistration)?
                    .prepare_hof_builtin(operation, authority)
            }
            .map_err(|error| match error {
                HofPreparationError::MissingCanonicalSymbol
                | HofPreparationError::UnsupportedBuiltin => FunctionError::UnsupportedDefinition,
                HofPreparationError::SubrMetadataBusy => {
                    FunctionError::Authority(RegistrationError::Busy)
                }
            })?;
            if seed.symbol() != key.symbol || seed.operation() != operation {
                return Err(FunctionError::UnsupportedDefinition);
            }
            match initial {
                FunctionCellSnapshot::Bound(value) if seed.identity().matches(value) => {
                    Ok(PreparedHofBuiltin {
                        operation: seed.operation(),
                        key: *key,
                        identity: seed.identity(),
                        registration_epoch: seed.registration_epoch(),
                        registration_clock: seed.registration_clock(),
                    })
                }
                FunctionCellSnapshot::Bound(_)
                | FunctionCellSnapshot::Empty
                | FunctionCellSnapshot::ExplicitlyUnbound => {
                    Err(FunctionError::UnsupportedDefinition)
                }
            }
        })
    }
}

impl<'era> RunAccess<'_, 'era> {
    /// This is a guarded callback selection, not generic HOF execution. Both
    /// handles and homes validate before the single reader is acquired. A
    /// later callback redefinition cannot redirect a rooted old selection.
    pub(crate) fn sym_hof_call_begin(
        &mut self,
        mapper: &PreparedHofBuiltin,
        callback: &PreparedUnaryFunction,
        expected: &OwnerRootSlot<'era>,
        argument: &OwnerRootSlot<'era>,
        activation: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, FunctionError> {
        self.check_named_operands(expected, activation)?;
        unary::check_argument(self, argument)?;
        self.check_symbol(&mapper.key)?;
        self.check_symbol(callback.key())?;
        self.symbol_ref(callback.key())?.select_hof(
            mapper,
            OwnerRootIndex::from(expected),
            activation,
            callback.shape(),
        )
    }
}

impl SymbolRef<'_, '_, '_> {
    fn select_hof(
        &mut self,
        mapper: &PreparedHofBuiltin,
        expected: OwnerRootIndex,
        activation: &mut FunctionActivation<'_>,
        shape: CallShape,
    ) -> Result<FunctionSelection, FunctionError> {
        // The callback/mapper cells stay fixed throughout this reader. The
        // existing native registration publication clock is sampled here,
        // giving the combined guard one valid semantic boundary even if a
        // different World publishes native registration later. No metadata
        // or function pointer is read by this running path.
        if mapper.registration_clock.snapshot() != mapper.registration_epoch {
            return Ok(FunctionSelection::Changed);
        }
        let snapshot = {
            let token = self.read_token();
            // SAFETY: the identity-checked callback reader covers this same
            // retained obarray and the already-validated mapper handle. The
            // one authority word excludes writers of EITHER function cell.
            unsafe {
                self.run
                    .owner
                    .registry
                    .symbols
                    .backing
                    .read_function(mapper.key.symbol, &token)
            }
            .ok_or(RegistrationError::Publication)?
        };
        match snapshot {
            FunctionCellSnapshot::Bound(value) if mapper.identity.matches(value) => {}
            FunctionCellSnapshot::Bound(_)
            | FunctionCellSnapshot::Empty
            | FunctionCellSnapshot::ExplicitlyUnbound => return Ok(FunctionSelection::Changed),
        }
        // This SAME SymbolRef is still held: there is no split lease, nested
        // acquisition or callback between mapper validation and root install.
        self.select_named(expected, activation, shape)
    }
}

static_assertions::assert_impl_all!(PreparedHofBuiltin: Send, Sync, Copy);

#[cfg(test)]
#[path = "hof/tests.rs"]
mod tests;
