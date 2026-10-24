//! Registry access for sealed initial-obarray operations. No TLS cache path.
use super::*;
use crate::tagged::gc::SymbolWriteToken;
use crate::tagged::gc::{RegistrationError, SymbolReadToken};

/// Scalar metadata captured under the process registry guard. It contains no
/// Lisp name object, movable spine address or authority to use a symbol cell.
#[derive(Clone, Copy)]
pub(crate) struct ExistingCanonicalSymbol {
    pub(crate) id: SymId,
    pub(crate) name: NameId,
}

/// Lock rank is the matching World WRITER, then this process registry guard.
/// Retaining the guard keeps the canonical decision through World publication.
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
