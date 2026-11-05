//! Sealed process publication of the upper byte-code structural operations.
//!
//! The immutable descriptor is inert configuration until the unsafe installer
//! vouches for it. Installed readers retain the original callback-table slot.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::hook_publication::{HookInstallError, HookInstallStatus};
use super::{EqualKind, EqualSeenPair, Flow, HashKey, Value};

/// Structural operations used only on byte-code values.
#[derive(Clone, Copy)]
pub struct BytecodeSemantics {
    to_equal_key: fn(Value, usize, &mut Vec<usize>, bool) -> HashKey,
    equal: fn(&Value, &Value, usize, &mut Option<HashSet<EqualSeenPair>>, bool, EqualKind) -> bool,
    try_equal: fn(
        &Value,
        &Value,
        usize,
        &mut Option<HashSet<EqualSeenPair>>,
        bool,
        EqualKind,
    ) -> Result<bool, Flow>,
    sxhash: fn(&Value, usize, crate::tagged::SymbolMode) -> Option<u64>,
}

impl BytecodeSemantics {
    /// Declare inert, typed callback data. This neither installs the table
    /// nor proves its equal/key/hash laws; installation is the unsafe grant.
    pub const fn new(
        to_equal_key: fn(Value, usize, &mut Vec<usize>, bool) -> HashKey,
        equal: fn(
            &Value,
            &Value,
            usize,
            &mut Option<HashSet<EqualSeenPair>>,
            bool,
            EqualKind,
        ) -> bool,
        try_equal: fn(
            &Value,
            &Value,
            usize,
            &mut Option<HashSet<EqualSeenPair>>,
            bool,
            EqualKind,
        ) -> Result<bool, Flow>,
        sxhash: fn(&Value, usize, crate::tagged::SymbolMode) -> Option<u64>,
    ) -> Self {
        Self {
            to_equal_key,
            equal,
            try_equal,
            sxhash,
        }
    }

    #[inline]
    pub(crate) fn to_equal_key(
        &self,
        value: Value,
        depth: usize,
        seen: &mut Vec<usize>,
        symbols_with_pos_enabled: bool,
    ) -> HashKey {
        (self.to_equal_key)(value, depth, seen, symbols_with_pos_enabled)
    }

    #[inline]
    pub(crate) fn equal(
        &self,
        left: &Value,
        right: &Value,
        depth: usize,
        seen: &mut Option<HashSet<EqualSeenPair>>,
        symbols_with_pos_enabled: bool,
        kind: EqualKind,
    ) -> bool {
        (self.equal)(left, right, depth, seen, symbols_with_pos_enabled, kind)
    }

    #[inline]
    pub(crate) fn try_equal(
        &self,
        left: &Value,
        right: &Value,
        depth: usize,
        seen: &mut Option<HashSet<EqualSeenPair>>,
        symbols_with_pos_enabled: bool,
        kind: EqualKind,
    ) -> Result<bool, Flow> {
        (self.try_equal)(left, right, depth, seen, symbols_with_pos_enabled, kind)
    }

    #[inline]
    pub(crate) fn sxhash(
        &self,
        value: &Value,
        depth: usize,
        symbol_mode: crate::tagged::SymbolMode,
    ) -> Option<u64> {
        (self.sxhash)(value, depth, symbol_mode)
    }
}

// Keep the installed payload at its original callback-only footprint.
const _: () = {
    assert!(size_of::<BytecodeSemantics>() == 4 * size_of::<fn()>());
    assert!(align_of::<BytecodeSemantics>() == align_of::<fn()>());
};

static BYTECODE_SEMANTICS: OnceLock<BytecodeSemantics> = OnceLock::new();
static BYTECODE_SEMANTICS_IDENTITY: OnceLock<&'static BytecodeSemantics> = OnceLock::new();

/// Publish the upper layer's one canonical structural callback table.
///
/// # Safety
/// Call only through core's `bytecode::semantics::install` adapter, reached
/// only from `Context::new_inner` or `Context::from_dump`, before that Context
/// can expose byte-code values to structural operations. `semantics` must
/// borrow that adapter's single named `CANONICAL_BYTECODE_SEMANTICS` static;
/// its callbacks must preserve the canonical structural equality, key and
/// sxhash laws and the current value/GC confinement contracts. All callbacks
/// live for the process. A different descriptor is refused and must not be
/// ignored.
#[must_use = "handle a conflicting byte-code semantics publication"]
#[cold]
#[inline(never)]
pub unsafe fn install_bytecode_semantics(
    semantics: &'static BytecodeSemantics,
) -> Result<HookInstallStatus, HookInstallError> {
    super::hook_publication::publish(&BYTECODE_SEMANTICS, &BYTECODE_SEMANTICS_IDENTITY, semantics)
}

pub(crate) fn bytecode_semantics() -> &'static BytecodeSemantics {
    BYTECODE_SEMANTICS
        .get()
        .expect("bytecode semantics are installed before any byte-code value exists")
}

#[cfg(test)]
#[path = "bytecode_semantics/tests.rs"]
mod tests;
