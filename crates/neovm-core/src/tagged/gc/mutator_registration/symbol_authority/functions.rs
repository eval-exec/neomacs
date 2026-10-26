//! Direct function-cell answers move into owner roots under symbol authority.
mod call;
mod constant;
mod name;
pub(super) use constant::ReturnSubset;
#[cfg(test)]
pub(in crate::tagged::gc::mutator_registration) use name::PreparedNameFunction;
pub(super) mod write;
use super::*;
use crate::emacs_core::symbol::FunctionCellSnapshot;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue as Value;
pub use call::{FunctionActivation, FunctionCallCache, FunctionStep};

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

/// The stopped mixed-input checker shares the exact running parser. The
/// copied child stays inside symbol authority and never becomes an executor
/// operand or an owning root in an admission receipt.
#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
pub(super) fn frozen_constant(
    value: Value,
    lambda: SymId,
    subset: ReturnSubset,
) -> Result<Value, FunctionError> {
    constant::decode(value, lambda, subset)
}

// Test-only labels for the remaining preparation bodies; no runtime opcode.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionOpKind {
    Function,
    Fset,
    Fmakunbound,
    CallBegin,
    CallStep,
    Reconcile,
}
#[cfg(test)]
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
        self.symbol_ref(symbol)?.function_into(dst.index)
    }
}

impl SymbolRef<'_, '_, '_> {
    fn function_into(&mut self, index: usize) -> Result<(), FunctionError> {
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
        // No callback, allocation or safepoint can occur after the copy. The
        // SymbolRef still holds its word while installing the canonical home;
        // even an unexpected stale home returns a total error under that lease.
        let dst = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *dst = MarkWord::of(value);
        Ok(())
    }
}
