//! Instruction anchors carried through Cranelift's source-location metadata.
//! Anchors contain no runtime state and insert no instructions. A compiler
//! invocation marks individual instructions without changing its builder's
//! current source location; multiple mutators compile independently.

use cranelift_codegen::CompiledCode;
use cranelift_codegen::ir::{Function, Inst, SourceLoc};

use super::{PlacementConfig, PlacementError, PlacementOffset, PlacementPolicy};

/// Compiler-owned source-location identities. This reserved namespace is used
/// only for placement metadata, never for Lisp source or JIT-visible layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementAnchor {
    SwitchConsGuard,
}

impl PlacementAnchor {
    #[doc(hidden)]
    pub fn location(self) -> SourceLoc {
        match self {
            Self::SwitchConsGuard => SourceLoc::new(0x4a49_5401),
        }
    }
}

/// Mark this instruction only when its experimental placement policy is on.
pub(crate) fn mark_instruction(func: &mut Function, inst: Inst, anchor: PlacementAnchor) {
    if cfg!(all(target_os = "linux", target_arch = "x86_64"))
        && matches!(
            super::config(),
            Ok(PlacementConfig::Policy(PlacementPolicy::Switch64))
        )
    {
        func.set_srcloc(inst, anchor.location());
    }
}

pub(crate) fn first_anchor_offset(code: &CompiledCode, anchor: PlacementAnchor) -> Option<u32> {
    code.buffer
        .get_srclocs_sorted()
        .iter()
        .filter(|range| range.loc == anchor.location() && range.start < range.end)
        .map(|range| range.start)
        .min()
}

/// Round the required entry offset down to a legal buffer alignment. The
/// source range can include the fused compare preceding its conditional jump;
/// placing its start just before the boundary leaves the jump near the start
/// of the following line. Unmarked functions retain offset zero. The rule is
/// independent of the Lisp name and preceding arena allocations.
pub(crate) fn placement_for_anchor(
    alignment: u64,
    anchor_offset: Option<u32>,
) -> Result<PlacementOffset, PlacementError> {
    if !alignment.is_power_of_two() {
        return Err(PlacementError::InvalidAlignment(alignment));
    }
    let desired = anchor_offset.map_or(0, |offset| (64 - u64::from(offset) % 64) % 64);
    let rounded = if alignment >= 64 {
        0
    } else {
        desired & !(alignment - 1)
    };
    PlacementOffset::new(rounded as usize)
}

#[cfg(test)]
#[path = "anchors/tests/anchor_test.rs"]
mod tests;
