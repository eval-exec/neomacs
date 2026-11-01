//! E4-S0 generated dispatch-table tests: one row per entry, typed splits,
//! and the folded green-guard re-entry axis.

use super::dispatch::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::subr::effect_catalog::inventory;
use crate::emacs_core::subr::effect_schema::{
    ArithOperands, CatalogKind, FastPathReentry, INLINED_NATIVE_OPS, ParallelArm, PlainVarAccess,
    ReturnDisposition, SealedAuthority, WorldSplit,
};
use crate::emacs_core::subr::leaf::LeafId;

#[test]
fn generated_dispatch_is_dense_and_equals_the_catalog_projection() {
    let entries = inventory();
    assert_eq!(entries.len(), 1_739);
    for (index, entry) in entries.iter().enumerate() {
        let slot = primitive_slot(index).expect("dense row");
        assert_eq!(
            slot.arm,
            dispatch_arm(entry.class, entry.variant).expect("valid class/variant pair"),
            "arm row {} ({:?})",
            entry.name,
            entry.kind
        );
        let call_free =
            entry.kind == CatalogKind::Primitive && INLINED_NATIVE_OPS.contains(&entry.name);
        assert_eq!(
            slot.reentry,
            if call_free {
                FastPathReentry::CannotReenter
            } else {
                FastPathReentry::MayReenter
            },
            "reentry row {} ({:?})",
            entry.name,
            entry.kind
        );
    }
    assert!(primitive_slot(entries.len()).is_none());
}

#[test]
fn opcode_sites_carry_the_typed_split_and_escape_arms() {
    assert_eq!(
        opcode_slot(&Op::Car).arm,
        ParallelArm::Sealed(SealedAuthority::ConsFieldRead)
    );
    assert_eq!(
        opcode_slot(&Op::Cons).arm,
        ParallelArm::Sealed(SealedAuthority::ConsAllocation)
    );
    assert_eq!(
        opcode_slot(&Op::Return).arm,
        ParallelArm::World(WorldSplit::Return(ReturnDisposition::OwnerLocal))
    );
    assert_eq!(
        opcode_slot(&Op::VarRef(0)).arm,
        ParallelArm::World(WorldSplit::PlainVar(PlainVarAccess::Ref))
    );
    assert_eq!(
        opcode_slot(&Op::Add1).arm,
        ParallelArm::World(WorldSplit::Arith(ArithOperands::Numeric))
    );
    assert_eq!(
        opcode_slot(&Op::TrapOutOfRangeConstant(0)).arm,
        ParallelArm::Scalar
    );
    // A World opcode with no typed split keeps the conservative escape.
    assert_eq!(
        opcode_slot(&Op::Call(0)).arm,
        ParallelArm::World(WorldSplit::Escape)
    );
}

#[test]
fn leaf_sites_carry_their_authority() {
    assert_eq!(
        leaf_slot(LeafId::Gethash).arm,
        ParallelArm::Sealed(SealedAuthority::HashRead)
    );
    assert_eq!(
        leaf_slot(LeafId::Keywordp).arm,
        ParallelArm::Sealed(SealedAuthority::CanonicalSymbol)
    );
    assert_eq!(
        leaf_slot(LeafId::Nthcdr).arm,
        ParallelArm::World(WorldSplit::Escape)
    );
}

#[test]
fn reentry_axis_is_folded_into_the_same_generated_rows() {
    for name in INLINED_NATIVE_OPS {
        let slot = primitive_slot_by_name(name)
            .unwrap_or_else(|| panic!("{name} is a registered primitive"));
        assert_eq!(slot.reentry, FastPathReentry::CannotReenter, "{name}");
    }
    for name in ["car", "cons", "sleep-for", "thread-yield", "ash"] {
        let slot = primitive_slot_by_name(name)
            .unwrap_or_else(|| panic!("{name} is a registered primitive"));
        assert_eq!(slot.reentry, FastPathReentry::MayReenter, "{name}");
    }
    assert!(primitive_slot_by_name("no-such-primitive-exists").is_none());
    assert_eq!(
        fast_path_reentry(primitive_slot_by_name("mod").expect("mod")),
        FastPathReentry::CannotReenter
    );
}

#[test]
fn owner_local_admission_is_typed() {
    assert!(admits_owner_local(opcode_slot(&Op::Car)));
    assert!(admits_owner_local(opcode_slot(&Op::Add1)));
    assert!(admits_owner_local(opcode_slot(&Op::Return)));
    assert!(!admits_owner_local(opcode_slot(&Op::Call(0))));
    assert!(!admits_owner_local(
        primitive_slot_by_name("sleep-for").expect("sleep-for")
    ));
}
