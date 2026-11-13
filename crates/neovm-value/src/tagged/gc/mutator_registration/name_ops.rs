//! C6 name-capability operations for the re-homed names tests.
//! The capability type stays in this module. Callers pass the symbol handle.
#![cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]

use super::symbol_authority::PreparedNameFunction;
use super::wrappers::ProofEra;
use super::{
    FunctionActivation, FunctionCallCache, FunctionError, OwnerRootSlot, RunAccess, SymbolHandle,
};
use crate::tagged::gc::mutator_registration::EraKeeper;

pub(crate) fn prepare_name_symbol<K: EraKeeper>(
    era: &ProofEra<'_, K>,
    key: &SymbolHandle,
) -> Result<SymbolHandle, FunctionError> {
    era.prepare_name_function_symbol(key)
        .map(PreparedNameFunction::handle)
}

pub(crate) fn sym_name_fset_into<'era>(
    run: &mut RunAccess<'_, 'era>,
    symbol: SymbolHandle,
    src: &OwnerRootSlot<'era>,
    dst: &mut OwnerRootSlot<'era>,
) -> Result<(), FunctionError> {
    run.sym_name_fset_into(&PreparedNameFunction::from_handle(symbol), src, dst)
}

pub(crate) fn sym_name_call_begin<'era>(
    run: &mut RunAccess<'_, 'era>,
    symbol: SymbolHandle,
    cache: &mut FunctionCallCache<'era>,
    activation: &mut FunctionActivation<'era>,
) -> Result<(), FunctionError> {
    run.sym_name_call_begin(
        &PreparedNameFunction::from_handle(symbol),
        cache,
        activation,
    )
}
