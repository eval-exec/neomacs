//! Bounded precompiled entries, distinct from serial cached code.
//!
//! GEN1 construction witnesses confine compilation to the exclusive phase.
//! Backend absence is a typed compile refusal, not a serial fallback. The
//! native plans have no helper call or back edge. Cell plans return from
//! native code before their sealed, rooted semantic epilogue.
use neovm_value::tagged::gc::symbol_table::publication::PublicationView;
use neovm_value::tagged::gc::{
    CellRead, CellReadError, FunctionActivation, FunctionError, FunctionSelection, Gen1Admission,
    HofError, HofScratch, HofSingleton, ImmediateResult, ImmediateResultError, OwnerRootSlot,
    ParallelExecutionAdmission, PreparedHofBuiltin, PreparedUnaryFunction, RegistrationError,
    RunAccess, SymbolHandle,
};
use std::ffi::c_void;

pub(crate) use neovm_value::tagged::gc::symbol_table::SymbolSlotIndex as TableSlotIndex;

/// This closed plan cannot contain a pointer, allocation, call or loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScalarProgram {
    Constant(ImmediateResult),
    Argument,
    /// True means immutable chunk backing exists, including any unused gaps.
    /// This is not Lisp fboundp or intern membership.
    HasSlotBacking(TableSlotIndex),
}

#[derive(Debug)]
pub(crate) enum ParallelCompileError {
    BackendUnavailable,
    Module(String),
    Define(String),
    Finalize(String),
    UnsupportedPointerWidth,
    AbiLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParallelEntryError {
    Authority(RegistrationError),
    UnsupportedAbi,
    MalformedStatus,
    NonImmediateResult,
    CellRead(CellReadError),
    Function(FunctionError),
    Hof(HofError),
    NoSlotBacking,
    UnexpectedEntryKind,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct NativeAbiVersion(u32);
impl NativeAbiVersion {
    #[doc(hidden)]
    pub const V1: Self = Self(1);
}

#[repr(C)]
struct ParallelRuntime<'activation> {
    version: NativeAbiVersion,
    publication: PublicationView<'activation>,
}
#[repr(C)]
struct ScalarInput {
    word: usize,
}
#[repr(C)]
struct CellInput {
    slot: TableSlotIndex,
}
#[repr(C)]
struct ScalarOutput {
    word: usize,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum NativeStatus {
    Returned = 0,
    UnsupportedAbi = 1,
    CellReadReady = 2,
    SlotAbsent = 3,
}
impl NativeStatus {
    #[doc(hidden)]
    pub fn checked(bits: u32) -> Result<Self, ParallelEntryError> {
        match bits {
            0 => Ok(Self::Returned),
            1 => Ok(Self::UnsupportedAbi),
            2 => Ok(Self::CellReadReady),
            3 => Ok(Self::SlotAbsent),
            _ => Err(ParallelEntryError::MalformedStatus),
        }
    }
}

// The opaque activation is a borrow of the value-layer activation, not a
// Context or serial frame. No function pointer or native address is exported.
type ParallelEntry = unsafe extern "C" fn(
    *const ParallelRuntime<'_>,
    *mut c_void,
    *const c_void,
    *mut ScalarOutput,
) -> u32;

mod cell;
mod hof;
mod identity;
use hof::HofInput;
pub(crate) use hof::ParallelCompiledHofCode;

#[cfg(feature = "jit")]
enum EntryPlan {
    Scalar(ScalarProgram),
    Cell,
    Hof,
}
#[cfg(feature = "jit")]
mod native;

#[must_use = "keep compiled code alive until its borrowed entry returns"]
pub(crate) struct ParallelCompiledCode {
    #[cfg(feature = "jit")]
    native: native::NativeCompiledCode,
    #[cfg(not(feature = "jit"))]
    unavailable: std::convert::Infallible,
}
impl ParallelCompiledCode {
    #[doc(hidden)]
    pub fn compile(
        _quiescent: &Gen1Admission<'_>,
        program: ScalarProgram,
    ) -> Result<Self, ParallelCompileError> {
        #[cfg(feature = "jit")]
        {
            native::NativeCompiledCode::compile(EntryPlan::Scalar(program))
                .map(|native| Self { native })
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = program;
            Err(ParallelCompileError::BackendUnavailable)
        }
    }

    #[doc(hidden)]
    pub fn run<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        input: ImmediateResult,
    ) -> Result<(), ParallelEntryError> {
        #[cfg(feature = "jit")]
        {
            self.native.run(admission, run, destination, input)
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (admission, run, destination, input);
            match self.unavailable {}
        }
    }
}

/// A cell kernel cannot be entered as an immediate-return kernel. Its closed
/// read operation is fixed while the real exclusive GEN1 witness is borrowed.
#[must_use = "retain the compiled cell kernel until its entry returns"]
pub(crate) struct ParallelCompiledCellCode {
    #[cfg(feature = "jit")]
    native: native::NativeCompiledCode,
    read: CellRead,
    #[cfg(not(feature = "jit"))]
    unavailable: std::convert::Infallible,
}
impl ParallelCompiledCellCode {
    #[doc(hidden)]
    pub fn compile(
        _quiescent: &Gen1Admission<'_>,
        read: CellRead,
    ) -> Result<Self, ParallelCompileError> {
        #[cfg(feature = "jit")]
        {
            native::NativeCompiledCode::compile(EntryPlan::Cell).map(|native| Self { native, read })
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = read;
            Err(ParallelCompileError::BackendUnavailable)
        }
    }

    #[doc(hidden)]
    pub fn run<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        symbol: &SymbolHandle,
    ) -> Result<(), ParallelEntryError> {
        #[cfg(feature = "jit")]
        {
            self.native
                .run_cell(self.read, admission, run, destination, symbol)
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (admission, run, destination, symbol);
            match self.unavailable {}
        }
    }
}
/// A named guard selects only the existing bounded constant-lambda activation.
/// Its rooted operand is never authority to enter an unrelated compiled body.
#[must_use = "retain the compiled guard until selection returns"]
pub(crate) struct ParallelCompiledNamedCode {
    #[cfg(feature = "jit")]
    native: native::NativeCompiledCode,
    #[cfg(not(feature = "jit"))]
    unavailable: std::convert::Infallible,
}
impl ParallelCompiledNamedCode {
    #[doc(hidden)]
    pub fn compile(_quiescent: &Gen1Admission<'_>) -> Result<Self, ParallelCompileError> {
        #[cfg(feature = "jit")]
        {
            native::NativeCompiledCode::compile(EntryPlan::Cell).map(|native| Self { native })
        }
        #[cfg(not(feature = "jit"))]
        {
            Err(ParallelCompileError::BackendUnavailable)
        }
    }

    #[doc(hidden)]
    pub fn select<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        target: &SymbolHandle,
        designator: &SymbolHandle,
        expected: &OwnerRootSlot<'era>,
        selected: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, ParallelEntryError> {
        #[cfg(feature = "jit")]
        {
            self.native.run_named(
                admission,
                run,
                destination,
                target,
                designator,
                expected,
                selected,
            )
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (
                admission,
                run,
                destination,
                target,
                designator,
                expected,
                selected,
            );
            match self.unavailable {}
        }
    }
}
static_assertions::assert_impl_all!(ParallelCompiledNamedCode: Send);
static_assertions::assert_not_impl_any!(ParallelCompiledNamedCode: Clone, Copy);
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(ParallelCompiledNamedCode: Sync);

static_assertions::assert_impl_all!(ParallelCompiledCellCode: Send);
static_assertions::assert_not_impl_any!(ParallelCompiledCellCode: Clone, Copy);
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(ParallelCompiledCellCode: Sync);

static_assertions::assert_impl_all!(ParallelCompiledCode: Send);
static_assertions::assert_not_impl_any!(ParallelCompiledCode: Clone, Copy);
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(ParallelCompiledCode: Sync);
static_assertions::assert_impl_all!(ScalarProgram: Send, Sync, Copy);

#[cfg(test)]
#[path = "parallel/tests/scalar_test.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::compiled_entry_manifest as compiled_entry_manifest_for_test;

#[cfg(test)]
#[path = "parallel/tests/cell_test.rs"]
mod cell_tests;
#[cfg(test)]
pub(crate) use cell_tests::varref_manifest as varref_manifest_for_test;

#[cfg(test)]
#[path = "parallel/tests/fboundp_test.rs"]
mod fboundp_tests;
#[cfg(test)]
pub(crate) use fboundp_tests::fboundp_manifest as fboundp_manifest_for_test;

#[cfg(test)]
#[path = "parallel/tests/identity_test.rs"]
mod identity_tests;
#[cfg(test)]
pub(crate) use identity_tests::named_manifest as named_manifest_for_test;

#[cfg(test)]
pub(crate) use hof::manifest_for_test as hof_manifest_for_test;

#[cfg(test)]
#[path = "parallel/tests/owner_handoff_test.rs"]
mod owner_handoff_tests;

#[cfg(test)]
#[path = "parallel/tests/publication_test.rs"]
mod publication_tests;
