//! The cold parallel dispatch tables of E4-DESIGN §3.1.
//!
//! The E1 source scanner generates one dense [`DispatchSlot`] per entry
//! (indexed by the E1 registration index) plus exhaustive opcode and leaf
//! projections. Each slot carries the executor arm and the green-guard
//! fast-path re-entry axis, folded into the same table so there is exactly
//! one classification source. A miss or an unadmitted entry is
//! [`ParallelArm::World(WorldSplit::Escape)`] by construction; the functions
//! here are the dormant E4 accessors over that data.

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::subr::effect_catalog::{
    inventory, leaf_dispatch_slot, opcode_dispatch_slot, primitive_dispatch,
};
use crate::emacs_core::subr::effect_schema::{
    DispatchSlot, FastPathReentry, ParallelArm, WorldSplit,
};
use crate::emacs_core::subr::leaf::LeafId;

#[allow(unused_imports)] // Stage 1 proves arms through this re-export.
pub(crate) use crate::emacs_core::subr::effect_schema::{DispatchShapeError, dispatch_arm};

/// The dispatch row of the entry with the given E1 registration index.
/// Out-of-range indexes are `None`; every real entry (including every future
/// registration) has the conservative World escape row.
#[allow(dead_code)] // E4-S0 is dormant; stage 1's executor consumes it.
pub(crate) fn primitive_slot(index: usize) -> Option<DispatchSlot> {
    primitive_dispatch(index)
}

/// The dispatch row of one bytecode opcode, from the generated exhaustive
/// match over the real `Op` enum.
#[allow(dead_code)] // E4-S0 is dormant; stage 1's parallel VM consumes it.
pub(crate) fn opcode_slot(op: &Op) -> DispatchSlot {
    opcode_dispatch_slot(op)
}

/// The dispatch row of one leaf registry entry, from the generated
/// exhaustive match over the real `LeafId` enum.
#[allow(dead_code)] // E4-S0 is dormant; stage 1's executor consumes it.
pub(crate) fn leaf_slot(entry: LeafId) -> DispatchSlot {
    leaf_dispatch_slot(entry)
}

/// The row of the primitive NAME registered in the catalog, for cold
/// resolution paths and tests. A name that is not an E1 Primitive row is
/// absent; callers must treat absence exactly like a World escape.
#[allow(dead_code)] // E4-S0 is dormant; stage 1's cold resolution consumes it.
pub(crate) fn primitive_slot_by_name(name: &str) -> Option<DispatchSlot> {
    use crate::emacs_core::subr::effect_schema::CatalogKind;
    inventory()
        .iter()
        .position(|entry| entry.kind == CatalogKind::Primitive && entry.name == name)
        .and_then(primitive_slot)
}

/// Whether an arm may run inside the owner without the serial escape (it may
/// still signal, which is a Flow under §2.4). `World(Escape)` never does; a
/// typed World split may run only after its runtime guard admits it.
#[allow(dead_code)] // E4-S0 is dormant; stage 1's executor consumes it.
pub(crate) const fn admits_owner_local(slot: DispatchSlot) -> bool {
    match slot.arm {
        ParallelArm::Scalar | ParallelArm::Sealed(_) => true,
        ParallelArm::World(WorldSplit::Escape) => false,
        ParallelArm::World(
            WorldSplit::Return(_) | WorldSplit::PlainVar(_) | WorldSplit::Arith(_),
        ) => true,
    }
}

/// The re-entry axis of the generated table, exposed for the E4 escape
/// accounting (a `CannotReenter` site never needs a return-side revalidation).
#[allow(dead_code)] // E4-S0 is dormant; stage 1's executor consumes it.
pub(crate) const fn fast_path_reentry(slot: DispatchSlot) -> FastPathReentry {
    slot.reentry
}
