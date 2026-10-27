//! Dynamic symbol identities owned by one mode-line formatter pass.

use super::{
    ModeLinePercentContext, ModeLineSymbol, StoredTarget, TargetPolicy, Value, VariableView,
};
use crate::emacs_core::intern::{SymId, intern, symbol_registry_epoch_value};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// A spelling already resolved by this pass. Use it for adjacent native reads,
/// before recursion or a Lisp callback; values and properties are never cached.
#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedModeLineSymbol(SymId);

impl ResolvedModeLineSymbol {
    #[inline]
    pub(super) fn value(
        self,
        vars: VariableView<'_>,
        buffers: &crate::buffer::BufferManager,
    ) -> Option<Value> {
        let obarray = vars.obarray();
        if let Some(buf) = buffers.current_buffer()
            && let Some(value) =
                buf.get_buffer_local_by_sym_id_gated(self.0, obarray.is_localized(self.0))
        {
            return Some(value);
        }
        vars.value(self.0)
    }

    #[inline]
    pub(super) fn is_risky(self, obarray: &crate::emacs_core::symbol::Obarray) -> bool {
        obarray
            .get_property_id(self.0, ModeLineSymbol::RiskyLocalVariable.id())
            .is_some_and(|value| !value.is_nil())
    }
}

/// Threading: these immutable spelling/ID entries belong to one formatter's
/// owned state. Its RefCell enforces exclusive same-thread native borrows;
/// it provides no synchronization between mutators. No borrow
/// survives a callback, and no ambient/TLS or cross-pass state is introduced.
/// A registry epoch change discards identities, never variable/property values.
pub(super) struct DynamicSymbols {
    epoch: u64,
    names: Names,
}

enum Names {
    Inline(SmallVec<[(&'static str, SymId); 4]>),
    Hashed(FxHashMap<&'static str, SymId>),
}

impl Default for DynamicSymbols {
    fn default() -> Self {
        Self {
            epoch: symbol_registry_epoch_value(),
            names: Names::Inline(SmallVec::new()),
        }
    }
}

impl DynamicSymbols {
    #[inline]
    fn resolve(&mut self, name: &'static str) -> ResolvedModeLineSymbol {
        let epoch = symbol_registry_epoch_value();
        if self.epoch != epoch {
            self.names = Names::Inline(SmallVec::new());
            self.epoch = epoch;
        }
        let found = match &self.names {
            Names::Inline(entries) => entries
                .iter()
                .find(|entry| entry.0 == name)
                .map(|entry| entry.1),
            Names::Hashed(entries) => entries.get(name).copied(),
        };
        if let Some(id) = found {
            return ResolvedModeLineSymbol(id);
        }

        // Preserve the old &str conversion exactly: input symbol IDs need not
        // be the current canonical ID, even when their bytes are valid UTF-8.
        let id = intern(name);
        self.remember(name, id);
        // Keep the pre-intern epoch: a concurrent invalidation during intern
        // forces the next lookup to resolve afresh, rather than publishing an
        // old identity under a newer epoch. A hit linearizes at its epoch read.
        ResolvedModeLineSymbol(id)
    }

    #[inline]
    fn remember(&mut self, name: &'static str, id: SymId) {
        match &mut self.names {
            Names::Inline(entries) if entries.len() < entries.inline_size() => {
                entries.push((name, id));
            }
            Names::Inline(entries) => {
                // Do not turn a long user-supplied format into quadratic work.
                let mut hashed =
                    FxHashMap::with_capacity_and_hasher(entries.len() * 2, Default::default());
                hashed.extend(entries.drain(..));
                hashed.insert(name, id);
                self.names = Names::Hashed(hashed);
            }
            Names::Hashed(entries) => {
                entries.insert(name, id);
            }
        }
    }
}

static_assertions::assert_not_impl_any!(ModeLinePercentContext<StoredTarget<'static>>: Sync);

impl<T: TargetPolicy> ModeLinePercentContext<T> {
    #[inline]
    pub(super) fn resolve_dynamic_symbol(&self, name: &'static str) -> ResolvedModeLineSymbol {
        self.dynamic_symbols.borrow_mut().resolve(name)
    }
}

#[cfg(test)]
#[path = "tests/mode_line_dynamic_symbols_test.rs"]
mod tests;
