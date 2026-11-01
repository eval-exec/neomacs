//! Cold production-entry inventory. No Lisp or serial dispatcher consumes it.
//!
//! `authority` remains the E1 admission (no handler is installed, so every
//! entry is World today). `class` and `variant` are the P8/E4-T typed P3
//! parallel classification: evidence-bearing declarations live in
//! `effect_parallel`, aliases inherit their resolved target, and the generated
//! dense tables are indexed by the E1 registration index. The class is data
//! for E4; no serial dispatcher reads it.

use super::effect_schema::{
    ArithOperands, Authority, CatalogKind, DispatchSlot, FastPathReentry, ParallelArm,
    ParallelClass, ParallelVariant, PlainVarAccess, ReturnDisposition, SealedAuthority, WorldSplit,
};
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
    pub(crate) class: ParallelClass,
    pub(crate) variant: Option<ParallelVariant>,
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

/// The same cold alias projection for the P3 parallel class E4 consumes.
/// Interpretation, void cells and cycles are World, exactly as in the
/// source-alias inheritance rule; unknown native targets stay unclassified.
#[allow(dead_code)] // Cold E1 vocabulary; tests exercise it, dispatch does not.
#[cold]
pub(crate) fn alias_target_class(target: AliasTarget<'_>) -> Option<ParallelClass> {
    match target {
        AliasTarget::Native(name) => INVENTORY
            .iter()
            .find(|entry| entry.kind == CatalogKind::Primitive && entry.name == name)
            .map(|entry| entry.class),
        AliasTarget::Interpreted | AliasTarget::Void | AliasTarget::Cycle => {
            Some(ParallelClass::World)
        }
    }
}

/// The cold E1 inventory, for crate modules outside this subtree (the E4
/// substrate's cold name resolution and its gate tests). The table stays
/// read-only and cold; no serial dispatcher consumes it.
#[allow(dead_code)] // E4-S0 is dormant; stage 1's cold resolution consumes it.
#[inline]
pub(crate) fn inventory() -> &'static [InventoryEntry] {
    INVENTORY
}

/// The E4 cold dispatch row of the entry with the given E1 registration
/// index (the exact order of [`INVENTORY`]). A miss is out of range; the
/// generated row for an unadmitted entry is `World(WorldSplit::Escape)`.
#[allow(dead_code)] // E4-S0 is dormant; E4 stage 1 consumes this.
#[inline]
pub(crate) fn primitive_dispatch(index: usize) -> Option<DispatchSlot> {
    PARALLEL_DISPATCH.get(index).copied()
}

/// The E4 cold dispatch row of one bytecode opcode (generated exhaustive
/// match against the real `Op` enum).
#[allow(dead_code)] // E4-S0 is dormant; E4 stage 1 consumes this.
#[inline]
pub(crate) fn opcode_dispatch_slot(entry: &crate::emacs_core::bytecode::Op) -> DispatchSlot {
    opcode_dispatch(entry)
}

/// The E4 cold dispatch row of one leaf registry entry (generated exhaustive
/// match against the real `LeafId` enum).
#[allow(dead_code)] // E4-S0 is dormant; E4 stage 1 consumes this.
#[inline]
pub(crate) fn leaf_dispatch_slot(entry: crate::emacs_core::subr::leaf::LeafId) -> DispatchSlot {
    leaf_dispatch(entry)
}

#[cfg(test)]
#[path = "tests/effect_catalog_test.rs"]
mod tests;
