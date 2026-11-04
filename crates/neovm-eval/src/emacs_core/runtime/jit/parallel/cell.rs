//! Native descriptor scope ends before the sealed cell/root semantic leaf.
use super::*;

/// # Safety
/// The retained entry must obey the closed descriptor-only ABI documented by
/// execute. The activation keeps its original IN_SLICE admission throughout.
pub(super) unsafe fn lookup(
    entry: ParallelEntry,
    activation: &mut neovm_value::tagged::gc::ExecutionActivation<'_, '_, '_>,
    symbol: &SymbolHandle,
) -> Result<(), ParallelEntryError> {
    // Identity checking reads only handle metadata. Mutable membership,
    // redirect and cell state are validated under reader authority later.
    let slot = activation
        .symbol_slot(symbol)
        .map_err(ParallelEntryError::Authority)?;
    let status = {
        let activation_address = std::ptr::from_mut(activation).cast();
        let runtime = ParallelRuntime {
            version: NativeAbiVersion::V1,
            publication: activation.publication(),
        };
        let input = CellInput { slot };
        let mut unused_output = ScalarOutput {
            word: ImmediateResult::Nil.word(),
        };
        // SAFETY: NativeCompiledCode retains the verified four-pointer C
        // mapping throughout this call. Runtime borrows the exact admitted
        // publication owner; input is the identity-checked slot request.
        // This closed kernel atomically loads one immutable descriptor,
        // checks its bounds and returns status only. It never dereferences
        // or retains the opaque activation and never calls a helper,
        // allocates, polls or writes the resident result. All descriptor
        // temporaries die at this native return, before the semantic leaf.
        let bits = unsafe {
            entry(
                &runtime,
                activation_address,
                std::ptr::from_ref(&input).cast(),
                &mut unused_output,
            )
        };
        NativeStatus::checked(bits)?
    };
    match status {
        NativeStatus::CellReadReady => Ok(()),
        NativeStatus::SlotAbsent => Err(ParallelEntryError::NoSlotBacking),
        NativeStatus::UnsupportedAbi => Err(ParallelEntryError::UnsupportedAbi),
        NativeStatus::Returned => Err(ParallelEntryError::UnexpectedEntryKind),
    }
}

/// # Safety
/// The caller retains a verified, closed four-pointer entry for this entire
/// invocation. It obeys the immutable descriptor/input ABI, never dereferences
/// the opaque activation or mutable cells, and cannot call, unwind, poll or
/// retain a borrowed address after returning. NativeCompiledCode is the sole
/// production supplier; Rust test kernels implement that same contract.
pub(super) unsafe fn execute<'era>(
    entry: ParallelEntry,
    operation: CellRead,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    destination: &OwnerRootSlot<'era>,
    symbol: &SymbolHandle,
) -> Result<(), ParallelEntryError> {
    admission
        .with_activation(run, destination, |activation| {
            // SAFETY: execute's caller retains the verified closed kernel;
            // this borrowed activation covers the whole immutable walk.
            unsafe { lookup(entry, activation, symbol) }?;
            activation
                .read_cell(symbol, operation)
                .map_err(ParallelEntryError::CellRead)
        })
        .map_err(ParallelEntryError::Authority)?
}

#[cfg(test)]
#[path = "cell/tests/abi_test.rs"]
mod tests;
