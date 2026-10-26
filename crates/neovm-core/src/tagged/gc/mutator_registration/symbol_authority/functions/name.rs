//! Narrow internal C6 capability; the general parallel Lisp entry is not open.
#![allow(dead_code)] // The reviewed mixed executor currently exists only in tests.
use super::*;

/// Prepared metadata bound to the existing heap/World/era/generation key.
/// It owns no definition/name root and grants no mutable graph capability.
#[must_use = "use the prepared capability with the matching running owner"]
#[derive(Clone, Copy)]
pub(in crate::tagged::gc::mutator_registration) struct PreparedNameFunction {
    pub(super) key: SymbolHandle,
}

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
impl PreparedNameFunction {
    pub(in crate::tagged::gc::mutator_registration) fn validate_keeper(
        &self,
        keeper: &KeeperSymbols<'_>,
    ) -> Result<(), RegistrationError> {
        self.keeper_lambda(keeper).map(|_| ())
    }

    pub(in crate::tagged::gc::mutator_registration) fn keeper_lambda(
        &self,
        keeper: &KeeperSymbols<'_>,
    ) -> Result<SymId, RegistrationError> {
        keeper.backing(&self.key)?;
        keeper
            .registry
            .symbols
            .functions
            .lambda_for_keeper(self.key.symbol, keeper)
    }
}

impl ParallelHeapEra<'_> {
    pub(in crate::tagged::gc::mutator_registration) fn prepare_name_function_symbol(
        &self,
        key: &SymbolHandle,
    ) -> Result<PreparedNameFunction, FunctionError> {
        self.prepare_function_subset(key, ReturnSubset::ScalarOrName)?;
        Ok(PreparedNameFunction { key: *key })
    }
}

impl<'era> RunAccess<'_, 'era> {
    /// Roots stay in owner homes. Default C4 fset still selects Scalar even
    /// when another preparation minted a name capability for the same symbol.
    pub(in crate::tagged::gc::mutator_registration) fn sym_name_fset_into(
        &mut self,
        symbol: &PreparedNameFunction,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError> {
        self.fset_with_subset(&symbol.key, src, dst, ReturnSubset::ScalarOrName)
    }

    /// Only this typed entry authorizes a broader activation. Shared step
    /// later consumes its saved subset, never a caller-provided mode flag.
    pub(in crate::tagged::gc::mutator_registration) fn sym_name_call_begin(
        &mut self,
        symbol: &PreparedNameFunction,
        cache: &mut FunctionCallCache<'era>,
        activation: &mut FunctionActivation<'era>,
    ) -> Result<(), FunctionError> {
        self.call_begin_with_subset(&symbol.key, cache, activation, ReturnSubset::ScalarOrName)
    }
}

static_assertions::assert_impl_all!(PreparedNameFunction: Send, Sync, Copy);
static_assertions::assert_eq_size!(PreparedNameFunction, SymbolHandle);
