//! Distinct bounded mapping entry; no generic or serial callback fallback.
use super::*;
use neovm_value::tagged::gc::ExecutionActivation;
use neovm_value::tagged::gc::HofOperation;

#[repr(C)]
pub(super) struct HofInput {
    pub(super) mapper: TableSlotIndex,
    pub(super) callback: TableSlotIndex,
}

/// # Safety
/// The retained entry implements the closed paired descriptor-only four-pointer
/// ABI. It reads the two initialized slot indices, acquires one descriptor,
/// performs bounded immutable walks and returns status. It cannot access cells,
/// call helpers, allocate, poll, unwind or retain an activation/address.
pub(super) unsafe fn lookup(
    entry: ParallelEntry,
    activation: &mut ExecutionActivation<'_, '_, '_>,
    mapper: &SymbolHandle,
    callback: &SymbolHandle,
) -> Result<(), ParallelEntryError> {
    let input = HofInput {
        mapper: activation
            .symbol_slot(mapper)
            .map_err(ParallelEntryError::Authority)?,
        callback: activation
            .symbol_slot(callback)
            .map_err(ParallelEntryError::Authority)?,
    };
    let status = {
        let activation_address = std::ptr::from_mut(activation).cast();
        let runtime = ParallelRuntime {
            version: NativeAbiVersion::V1,
            publication: activation.publication(),
        };
        let mut unused_output = ScalarOutput {
            word: ImmediateResult::Nil.word(),
        };
        // SAFETY: the caller owns this verified executable mapping; runtime,
        // initialized paired input and output are live stack objects borrowing
        // the original IN_SLICE activation. All descriptor addresses die at
        // native return, before the rooted guard/callback/allocation leaf.
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

/// The operation is a closed fixed builtin; executable mappings and native
/// addresses never escape this owner or convert to serial cached code.
#[must_use = "retain the compiled mapping until its bounded activation returns"]
pub(crate) struct ParallelCompiledHofCode {
    operation: HofOperation,
    #[cfg(feature = "jit")]
    native: native::NativeCompiledCode,
    #[cfg(not(feature = "jit"))]
    unavailable: std::convert::Infallible,
}
impl ParallelCompiledHofCode {
    #[doc(hidden)]
    pub fn compile(
        _quiescent: &Gen1Admission<'_>,
        operation: HofOperation,
    ) -> Result<Self, ParallelCompileError> {
        #[cfg(feature = "jit")]
        {
            native::NativeCompiledCode::compile(EntryPlan::Hof)
                .map(|native| Self { operation, native })
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = operation;
            Err(ParallelCompileError::BackendUnavailable)
        }
    }

    #[doc(hidden)]
    pub fn run<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        mapper: &PreparedHofBuiltin,
        callback: &PreparedUnaryFunction,
        expected: &OwnerRootSlot<'era>,
        input: &mut HofSingleton<'era>,
        scratch: &mut HofScratch<'era>,
    ) -> Result<FunctionSelection, ParallelEntryError> {
        #[cfg(feature = "jit")]
        {
            admission
                .with_activation(run, destination, |activation| {
                    if mapper.operation() != self.operation {
                        return Ok(FunctionSelection::Changed);
                    }
                    self.native
                        .lookup_hof(activation, mapper.symbol(), callback.symbol())?;
                    activation
                        .map_singleton(mapper, callback, expected, input, scratch)
                        .map_err(ParallelEntryError::Hof)
                })
                .map_err(ParallelEntryError::Authority)?
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (
                admission,
                run,
                destination,
                mapper,
                callback,
                expected,
                input,
                scratch,
            );
            match self.unavailable {}
        }
    }
}
static_assertions::assert_impl_all!(ParallelCompiledHofCode: Send);
static_assertions::assert_not_impl_any!(ParallelCompiledHofCode: Clone, Copy);
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(ParallelCompiledHofCode: Sync);

#[cfg(test)]
#[path = "hof/tests/mapping_test.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::manifest as manifest_for_test;
