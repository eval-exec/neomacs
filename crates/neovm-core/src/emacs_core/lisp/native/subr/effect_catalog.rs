//! Cold production-entry inventory. No Lisp or serial dispatcher consumes it.
//!
//! All existing native/leaf/opcode entries are explicitly World-only in E1.
//! Optimization leaf effects do not constitute parallel admission.

use super::effect_schema::{Authority, CatalogKind};
use crate::emacs_core::eval::parallel_entry::capability::{
    BatonWitness, ParallelExecution, SerialContext,
};
use crate::emacs_core::{error::EvalResult, value::Value};

#[allow(dead_code)] // E1 defines entry contracts without installing handlers.
pub(crate) type ScalarPrimitive = fn(&[Value]) -> EvalResult;

#[allow(dead_code)] // Future admitted handlers must carry owner/run authority.
pub(crate) type SealedPrimitive = for<'loan, 'heap, 'owner, 'shape> fn(
    &mut ParallelExecution<'loan, 'heap, 'owner, 'shape>,
    &[Value],
) -> EvalResult;

#[allow(dead_code)] // No legacy function pointer satisfies this signature.
pub(crate) type WorldPrimitive = for<'loan, 'guard, 'heap, 'owner> fn(
    &mut SerialContext<'loan, 'guard, 'heap, 'owner>,
    &BatonWitness<'guard, 'heap, 'owner>,
    &[Value],
) -> EvalResult;

#[allow(dead_code)] // These typed constructors are not a production dispatcher.
pub(crate) enum PrimitiveEntry {
    Scalar(ScalarPrimitive),
    Sealed(SealedPrimitive),
    World(WorldPrimitive),
}

#[allow(dead_code)] // Cold sidecar; no hot registry element grows.
#[derive(Debug)]
pub(crate) struct InventoryEntry {
    pub(crate) kind: CatalogKind,
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
    pub(crate) authority: Authority,
    pub(crate) target: Option<&'static str>,
}

// Regenerated explicitly by xtask; the architecture gate checks source freshness.
include!("effect_inventory.inc");

/// The current terminal function cell, rather than an alias name frozen at
/// build time. Resolving cells and retaining their owner belongs to E4/E5.
#[allow(dead_code)] // Cold E1 vocabulary has no installed runtime consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AliasTarget<'name> {
    Native(&'name str),
    Interpreted,
    Void,
    Cycle,
}

/// Project a resolved alias through the same native inventory as direct calls.
/// Unknown native targets stay unclassified; an alias cannot manufacture a
/// classification merely because its own name appeared in shipped Lisp.
#[allow(dead_code)] // E1 tests exercise this cold projection; dispatch does not.
#[cold]
pub(crate) fn alias_target_authority(target: AliasTarget<'_>) -> Option<Authority> {
    match target {
        AliasTarget::Native(name) => INVENTORY
            .iter()
            .find(|entry| entry.kind == CatalogKind::Primitive && entry.name == name)
            .map(|entry| entry.authority),
        AliasTarget::Interpreted | AliasTarget::Void | AliasTarget::Cycle => Some(Authority::World),
    }
}

#[cfg(test)]
#[path = "tests/effect_catalog_test.rs"]
mod tests;
