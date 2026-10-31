//! Bounded precompiled entries, distinct from serial cached code.
//!
//! GEN1 construction witnesses confine compilation to the exclusive phase.
//! Backend absence is a typed compile refusal, not a serial fallback. The
//! first admitted plan returns an immediate without a call or back edge.
use crate::tagged::gc::symbol_table::publication::PublicationView;
use crate::tagged::gc::{
    Gen1Admission, ImmediateResult, ImmediateResultError, OwnerRootSlot,
    ParallelExecutionAdmission, RegistrationError, RunAccess,
};
use std::ffi::c_void;

/// A structural slot request, distinct from counts and from a live symbol
/// handle. Coverage does not prove membership or permit a cell read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableSlotIndex(usize);
impl TableSlotIndex {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

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
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct NativeAbiVersion(u32);
impl NativeAbiVersion {
    const V1: Self = Self(1);
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
struct ScalarOutput {
    word: usize,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum NativeStatus {
    Returned = 0,
    UnsupportedAbi = 1,
}
impl NativeStatus {
    fn checked(bits: u32) -> Result<Self, ParallelEntryError> {
        match bits {
            0 => Ok(Self::Returned),
            1 => Ok(Self::UnsupportedAbi),
            _ => Err(ParallelEntryError::MalformedStatus),
        }
    }
}

// The opaque activation is a borrow of the value-layer activation, not a
// Context or serial frame. No function pointer or native address is exported.
type ParallelEntry = unsafe extern "C" fn(
    *const ParallelRuntime<'_>,
    *mut c_void,
    *const ScalarInput,
    *mut ScalarOutput,
) -> u32;

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
    pub(crate) fn compile(
        _quiescent: &Gen1Admission<'_>,
        program: ScalarProgram,
    ) -> Result<Self, ParallelCompileError> {
        #[cfg(feature = "jit")]
        {
            native::NativeCompiledCode::compile(program).map(|native| Self { native })
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = program;
            Err(ParallelCompileError::BackendUnavailable)
        }
    }

    pub(crate) fn run<'era>(
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

static_assertions::assert_not_impl_any!(ParallelCompiledCode: Clone, Copy);
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(ParallelCompiledCode: Sync);
static_assertions::assert_impl_all!(ScalarProgram: Send, Sync, Copy);

#[cfg(test)]
#[path = "parallel/tests/scalar_test.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::compiled_entry_manifest as compiled_entry_manifest_for_test;
