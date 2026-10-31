//! Guard selection is rooted under one reader after the native walk returns.
use super::*;

/// # Safety
/// entry is retained for the complete call and implements the same closed,
/// descriptor-only four-pointer ABI as cell::execute. It cannot retain an
/// address, access mutable cells, invoke a helper, poll, or unwind.
pub(super) unsafe fn execute<'era>(
    entry: ParallelEntry,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    destination: &OwnerRootSlot<'era>,
    target: &SymbolHandle,
    designator: &SymbolHandle,
    expected: &OwnerRootSlot<'era>,
    selected: &mut FunctionActivation<'era>,
) -> Result<FunctionSelection, ParallelEntryError> {
    admission
        .with_activation(run, destination, |activation| {
            // These are validated opaque identities, never arbitrary symbol words.
            activation
                .symbol_slot(target)
                .map_err(ParallelEntryError::Authority)?;
            activation
                .symbol_slot(designator)
                .map_err(ParallelEntryError::Authority)?;
            if target != designator {
                return Ok(FunctionSelection::Changed);
            }
            // SAFETY: caller retains the verified mapping and the activation keeps
            // IN_SLICE across this walk. All descriptor temporaries end at return.
            unsafe { super::cell::lookup(entry, activation, target) }?;
            activation
                .select_named_function(target, expected, selected)
                .map_err(ParallelEntryError::Function)
        })
        .map_err(ParallelEntryError::Authority)?
}
