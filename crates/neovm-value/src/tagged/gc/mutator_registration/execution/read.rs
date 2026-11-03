//! Closed cell-to-root transfers for a bounded production activation.
//! This boundary exports no loaded word, cell address or mutable root bank.
use super::super::{
    FunctionActivation, FunctionError, FunctionSelection, GlobalError, OwnerRootSlot, SymbolHandle,
};
use super::ExecutionActivation;
use crate::tagged::gc::RegistrationError;
use crate::tagged::gc::symbol_table::SymbolSlotIndex;

/// Separate domains require an explicit choice; no bool selects an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellRead {
    GlobalValue,
    GlobalBoundp,
    Function,
    FunctionBoundp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellReadError {
    Global(GlobalError),
    Function(FunctionError),
}

impl<'era> ExecutionActivation<'_, '_, 'era> {
    /// Return checked handle metadata without loading a symbol cell or backing.
    pub(crate) fn symbol_slot(
        &self,
        symbol: &SymbolHandle,
    ) -> Result<SymbolSlotIndex, RegistrationError> {
        self.run.checked_symbol_slot(symbol)
    }

    /// A matching named definition is installed in the private bounded-call
    /// activation under the reader. No boolean can authorize a separate callee.
    pub(crate) fn select_named_function(
        &mut self,
        symbol: &SymbolHandle,
        expected: &OwnerRootSlot<'era>,
        selected: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, FunctionError> {
        self.run.sym_named_call_begin(symbol, expected, selected)
    }

    /// Reuse the identity-bound counted reader and canonical resident home.
    /// The existing primitive installs even a managed answer before releasing
    /// reader authority. A refusal preserves the previous home and returns
    /// Busy as a typed result; contention retry belongs to the caller.
    pub(crate) fn read_cell(
        &mut self,
        symbol: &SymbolHandle,
        operation: CellRead,
    ) -> Result<(), CellReadError> {
        // Clone aliases the same already-validated home; it neither adds a
        // census entry nor returns a raw word to the generated-code boundary.
        let mut destination = self.result.clone();
        match operation {
            CellRead::GlobalValue => self
                .run
                .sym_value_into(symbol, &mut destination)
                .map_err(CellReadError::Global),
            CellRead::GlobalBoundp => self
                .run
                .sym_boundp_into(symbol, &mut destination)
                .map_err(CellReadError::Global),
            CellRead::Function => self
                .run
                .sym_function_into(symbol, &mut destination)
                .map_err(CellReadError::Function),
            CellRead::FunctionBoundp => self
                .run
                .sym_fboundp_into(symbol, &mut destination)
                .map_err(CellReadError::Function),
        }
    }
}

#[cfg(test)]
#[path = "read/tests/cell_test.rs"]
mod tests;
