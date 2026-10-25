//! Registry access for sealed initial-obarray operations. No TLS cache path.
use super::*;
use crate::tagged::gc::SymbolWriteToken;
use crate::tagged::gc::{RegistrationError, SymbolReadToken};
mod fresh;
pub(crate) use fresh::PreparedFreshCanonical;
#[cfg(test)]
pub(crate) use fresh::{FreshRegistryFacts, fresh_registry_facts_for_test};

/// Scalar metadata captured under the process registry guard. It contains no
/// Lisp name object, movable spine address or authority to use a symbol cell.
#[derive(Clone, Copy)]
pub(crate) struct ExistingCanonicalSymbol {
    id: SymId,
    name: NameId,
}
impl ExistingCanonicalSymbol {
    pub(crate) fn id(self) -> SymId {
        self.id
    }

    pub(crate) fn name(self) -> NameId {
        self.name
    }
}

/// Lock rank is the matching World WRITER, then this process registry guard.
/// Retaining the guard keeps the canonical decision through World publication.
#[must_use = "retain the registry writer through World and canonical publication"]
pub(crate) struct InitialRegistryWriter<'authority> {
    registry: parking_lot::RwLockWriteGuard<'static, SymbolRegistry>,
    _authority: std::marker::PhantomData<&'authority SymbolWriteToken<'authority>>,
}
impl<'authority> InitialRegistryWriter<'authority> {
    pub(crate) fn enter(
        _authority: &'authority SymbolWriteToken<'_>,
    ) -> Result<Self, RegistrationError> {
        let registry = global_symbol_registry()
            .try_write()
            .ok_or(RegistrationError::Busy)?;
        Ok(Self {
            registry,
            _authority: std::marker::PhantomData,
        })
    }

    pub(crate) fn existing(
        &self,
        name: &LispString,
    ) -> Result<Option<ExistingCanonicalSymbol>, RegistrationError> {
        let Some(id) = self.registry.lookup_lisp_string(name) else {
            return Ok(None);
        };
        let slot = self
            .registry
            .slot(id)
            .ok_or(RegistrationError::Publication)?;
        if !slot.canonical {
            return Err(RegistrationError::Publication);
        }
        Ok(Some(ExistingCanonicalSymbol {
            id,
            name: slot.name,
        }))
    }
}

/// The rooted name is immutable for the closed era. A refused registry loan
/// releases the caller's symbol reader before it may service a stop request.
/// No serial intern, lazy name materialization or canonical-map mutation runs.
pub(crate) fn lookup(
    name: &LispString,
    _authority: &SymbolReadToken<'_>,
) -> Result<Option<SymId>, RegistrationError> {
    let registry = global_symbol_registry()
        .try_read()
        .ok_or(RegistrationError::Busy)?;
    Ok(registry.lookup_lisp_string(name))
}

#[cfg(test)]
pub(crate) fn with_registry_writer_for_test(body: impl FnOnce()) {
    let _guard = global_symbol_registry().write();
    body();
}

#[cfg(test)]
pub(crate) fn identity_census_for_test() -> (u64, usize) {
    let registry = global_symbol_registry().read();
    (symbol_registry_epoch_value(), registry.symbols.len())
}

/// Scalar observations of exact root identity; never materialize a name or
/// install a heap scope while a parallel era is live.
#[cfg(test)]
pub(crate) fn name_publication_for_test(
    heap: crate::tagged::gc::HeapIdentity,
    id: SymId,
    losing_name: usize,
) -> (Option<usize>, bool, bool, bool) {
    let registry = global_symbol_registry().read();
    let heap = SymbolNameHeapId(heap.get());
    let name = registry.name_values.get(&id).copied();
    let roots = registry.name_value_roots.by_heap.get(&heap);
    (
        name.filter(|name| name.heap_id == heap)
            .map(|name| name.word.0),
        name.is_some_and(|name| {
            roots.is_some_and(|roots| {
                roots.get(&SymbolNameObjectId::of(name.word)) == Some(&name.word)
            })
        }),
        roots.is_some_and(|roots| roots.contains_key(&SymbolNameObjectId(losing_name))),
        registry.slot(id).is_some_and(|slot| {
            !registry
                .names
                .resolve_lisp_string(slot.name)
                .has_intervals()
        }),
    )
}
