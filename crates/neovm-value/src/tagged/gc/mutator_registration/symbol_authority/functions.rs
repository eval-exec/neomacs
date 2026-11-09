//! Direct function-cell answers move into owner roots under symbol authority.
mod call;
mod constant;
#[cfg(not(feature = "test-support"))]
mod hof;
#[cfg(feature = "test-support")]
pub mod hof;
mod name;
#[doc(hidden)]
pub use hof::PreparedHofBuiltin;
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) use hof::{EraCoreHofExt, RunAccessHofExt};
#[cfg(not(feature = "test-support"))]
mod unary;
#[cfg(feature = "test-support")]
pub mod unary;
pub(super) use constant::{CallShape, ReturnSubset};
#[cfg(any(test, feature = "test-support"))]
pub(in crate::tagged::gc::mutator_registration) use name::PreparedNameFunction;
#[doc(hidden)]
pub use unary::PreparedUnaryFunction;
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) use unary::{EraCoreUnaryExt, RunAccessUnaryExt};
pub(super) mod write;
use super::intern::OwnerRootIndex;
use super::*;
use crate::symbol::FunctionCellSnapshot;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue as Value;
pub use call::{FunctionActivation, FunctionCallCache, FunctionSelection, FunctionStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionError {
    Authority(RegistrationError),
    VoidFunction,
    UnsupportedDefinition,
    EpochExhausted,
    ActiveActivation,
    InactiveActivation,
}
impl From<RegistrationError> for FunctionError {
    fn from(error: RegistrationError) -> Self {
        Self::Authority(error)
    }
}

/// Both queries copy one snapshot and install their answer under the lease.
enum FunctionRead {
    Definition,
    Boundp,
}

/// The stopped mixed-input checker shares the exact running parser. The
/// copied child stays inside symbol authority and never becomes an executor
/// operand or an owning root in an admission receipt.
#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
pub(super) fn frozen_constant(
    value: Value,
    lambda: SymId,
    subset: ReturnSubset,
) -> Result<Value, FunctionError> {
    constant::decode(value, lambda, subset)
}

// Test-only labels for the remaining preparation bodies; no runtime opcode.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionOpKind {
    Function,
    Fset,
    Fmakunbound,
    CallBegin,
    CallStep,
    Reconcile,
}
#[cfg(any(test, feature = "test-support"))]
impl FunctionOpKind {
    pub(crate) fn method(self) -> &'static str {
        match self {
            Self::Function => "sym_function_into",
            Self::Fset => "sym_fset_into",
            Self::Fmakunbound => "sym_fmakunbound_into",
            Self::CallBegin => "sym_call_begin",
            Self::CallStep => "sym_call_step",
            Self::Reconcile => "reconcile_functions",
        }
    }
}

impl<'era> RunAccess<'_, 'era> {
    /// GNU symbol-function returns nil for both empty and explicitly unbound
    /// cells. The returned definition is rooted before reader release. This
    /// copies only the cell: it neither resolves aliases nor executes a callee.
    pub fn sym_function_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError> {
        globals::check_home(self, dst)?;
        self.symbol_ref(symbol)?
            .function_into(OwnerRootIndex::from(&*dst), FunctionRead::Definition)
    }

    /// Test the direct function cell without resolving aliases or executing it.
    /// The canonical boolean reaches the resident home before reader release.
    pub fn sym_fboundp_into(
        &mut self,
        symbol: &SymbolHandle,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError> {
        globals::check_home(self, dst)?;
        self.symbol_ref(symbol)?
            .function_into(OwnerRootIndex::from(&*dst), FunctionRead::Boundp)
    }
}

impl SymbolRef<'_, '_, '_> {
    fn function_into(
        &mut self,
        index: OwnerRootIndex,
        operation: FunctionRead,
    ) -> Result<(), FunctionError> {
        // The ephemeral token borrows this real lease. It cannot outlive the
        // lease or escape with a cell reference; the snapshot is an owned copy.
        let snapshot = {
            let token = self.read_token();
            // SAFETY: validated heap/World/era/symbol identity, retained roots,
            // stable membership and the matching counted-reader word.
            unsafe {
                self.run
                    .owner
                    .registry
                    .symbols
                    .backing
                    .read_function(self.key.symbol, &token)
            }
        }
        .ok_or(RegistrationError::Publication)?;
        let value = match snapshot {
            FunctionCellSnapshot::Bound(value) if !value.is_unbound() => value,
            FunctionCellSnapshot::Empty | FunctionCellSnapshot::ExplicitlyUnbound => Value::NIL,
            FunctionCellSnapshot::Bound(_) => return Err(FunctionError::UnsupportedDefinition),
        };
        let answer = match operation {
            FunctionRead::Definition => value,
            FunctionRead::Boundp => {
                if value.is_nil() {
                    Value::NIL
                } else {
                    Value::T
                }
            }
        };
        // No callback, allocation or safepoint can occur after the copy. The
        // SymbolRef still holds its word while installing the canonical home;
        // even an unexpected stale home returns a total error under that lease.
        let dst = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(usize::from(index)))
            .ok_or(RegistrationError::StaleRegistration)?;
        *dst = MarkWord::of(answer);
        Ok(())
    }
}
