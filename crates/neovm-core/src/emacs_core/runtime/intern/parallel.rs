//! Registry access for sealed initial-obarray operations. No TLS cache path.
use super::*;
use crate::tagged::gc::{RegistrationError, SymbolReadToken};

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
