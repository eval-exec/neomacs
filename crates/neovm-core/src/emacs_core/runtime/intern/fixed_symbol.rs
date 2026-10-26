//! Fixed native spellings resolved once per existing registry epoch.

use super::{SymId, global_symbol_registry, symbol_registry_epoch_value};
use parking_lot::RwLock;

#[derive(Clone, Copy, Debug)]
struct ResolvedSymbol {
    epoch: u64,
    symbol: SymId,
}

/// A process-owned fixed spelling; it retains no evaluator or Lisp heap data.
///
/// The entry's full registry epoch and ID are published together under a small
/// lock. A warm lookup validates the epoch with Acquire and does not hash or
/// intern the spelling. Cold lookup always locks the registry before this cell,
/// so it cannot publish a stale ID with a newer epoch after concurrent unintern.
/// This uses the existing full-width registry generation, not a packed counter.
#[derive(Debug)]
pub struct EpochCachedSymbol {
    name: &'static str,
    resolved: RwLock<Option<ResolvedSymbol>>,
}

impl EpochCachedSymbol {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            resolved: RwLock::new(None),
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    #[inline]
    pub fn get(&self) -> SymId {
        {
            let resolved = self.resolved.read();
            if let Some(resolved) = *resolved
                && resolved.epoch == symbol_registry_epoch_value()
            {
                return resolved.symbol;
            }
        }
        self.resolve()
    }

    #[cold]
    #[inline(never)]
    fn resolve(&self) -> SymId {
        // Public intern can fall through to this same registry write lock, so
        // calling it with this guard held could deadlock. Direct resolution
        // keeps the name, canonical ID and publication epoch in one snapshot.
        let mut registry = global_symbol_registry().write();
        let epoch = symbol_registry_epoch_value();
        let mut resolved = self.resolved.write();
        if let Some(cached) = *resolved {
            if cached.epoch == epoch {
                return cached.symbol;
            }
            // The epoch also advances for fresh registrations, which preserve
            // every existing canonical mapping (unintern clears the slot's
            // canonical flag and dump restore reuses a live canonical mapping
            // for a name). Revalidate the cached identity under this same
            // registry guard instead of repeating the intern lookup; a cached
            // ID that is no longer canonical falls through to the lookup.
            if registry.is_canonical_id(cached.symbol) {
                *resolved = Some(ResolvedSymbol {
                    epoch,
                    symbol: cached.symbol,
                });
                return cached.symbol;
            }
        }
        let symbol = registry.intern(self.name);
        *resolved = Some(ResolvedSymbol { epoch, symbol });
        symbol
    }
}

#[cfg(test)]
#[path = "tests/fixed_symbol_test.rs"]
mod tests;
