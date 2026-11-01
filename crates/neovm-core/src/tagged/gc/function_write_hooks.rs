//! Value-layer vocabulary for cold function-write validation and retirement.
//!
//! The compiler belongs above the heap. Static implementations supply its
//! checks/effects without a callback allocation, extra TLS lookup or JIT type
//! in this interface. Existing symbol authority still guards the cell itself.
use super::{KeeperSymbols, SymId, SymbolWriteToken};
use crate::tagged::value::TaggedValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialDefinition {
    OpaqueScalarConstant,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompilerActivity {
    NativeActivation,
    BackgroundCompilation,
}

/// Cold classification only; syntax alone cannot authorize parameter binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallbackParameterKind {
    Ordinary,
    Unsupported,
}

pub trait FunctionWriteHooks {
    /// Classify an existing immutable definition; never materialize a dump
    /// payload, run a callback, or return a borrowed compiler object.
    ///
    /// # Safety
    /// The symbol writer belongs to this backing, the value is retained, and
    /// serial preparation excludes overlapping definition or heap mutation.
    unsafe fn classify_initial_definition(
        &self,
        value: TaggedValue,
        authority: &SymbolWriteToken<'_>,
    ) -> InitialDefinition;

    /// Inspect the keeper's existing compiler state, in the same order as
    /// legacy admission. This is a cold preflight, not a running-owner query.
    fn check_compiler_quiescent(&self) -> Result<(), CompilerActivity>;

    /// Retire compiler observations of a changed original target. The upper
    /// implementation owns cache eviction and the internal-write reason.
    ///
    /// # Safety
    /// The keeper is this backing's matching AllStopped, on its keeper thread,
    /// with zero symbol word and compiler quiescence already established.
    unsafe fn reconcile_function_write(&self, symbol: SymId, keeper: &KeeperSymbols<'_>);
}
