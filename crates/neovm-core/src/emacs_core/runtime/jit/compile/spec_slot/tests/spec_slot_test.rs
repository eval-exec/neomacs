//! `SpecSlot` v2 (S0.8): the four words at the offsets generated code bakes,
//! the slot kind of every site kind, the immutable subr words, and the
//! direct entry armed after its leaf and cleared before it.

use super::*;

fn word_at(slot: &SpecSlot, offset: usize) -> u64 {
    // SAFETY: `offset` is one of the const-asserted word offsets.
    unsafe {
        std::ptr::from_ref(slot)
            .cast::<u8>()
            .add(offset)
            .cast::<u64>()
            .read()
    }
}

#[test]
fn the_four_words_sit_where_generated_code_reads_them() {
    let slot = SpecSlot::at_epoch(0x1234);
    let leaf = 0x7f00_0000_1000usize as *const CompiledLeaf;
    let consts = 0x7f00_0000_2000usize as *const Value;
    let entry = 0x7f00_0000_3000usize as *const u8;
    slot.arm_leaf(leaf, consts, false, false);
    slot.arm_direct_entry(entry);
    assert_eq!(word_at(&slot, SPEC_SLOT_EPOCH_OFFSET), 0x1234);
    assert_eq!(word_at(&slot, SPEC_SLOT_LEAF_OFFSET), leaf as u64);
    assert_eq!(word_at(&slot, SPEC_SLOT_KEY_OFFSET), consts as u64);
    assert_eq!(word_at(&slot, SPEC_SLOT_DIRECT_ENTRY_OFFSET), entry as u64);
    assert_eq!(slot.direct_entry(), entry);
    // The clear drops all three leaf words (the direct entry first).
    slot.clear_leaf();
    for off in [
        SPEC_SLOT_LEAF_OFFSET,
        SPEC_SLOT_KEY_OFFSET,
        SPEC_SLOT_DIRECT_ENTRY_OFFSET,
    ] {
        assert_eq!(word_at(&slot, off), 0, "offset {off}");
    }
    assert_eq!(word_at(&slot, SPEC_SLOT_EPOCH_OFFSET), 0x1234, "epoch kept");
}

#[test]
fn every_site_kind_names_its_slot_kind() {
    let subr_kinds = [
        SpecCalleeKind::SubrGeneral,
        SpecCalleeKind::PredRecordp,
        SpecCalleeKind::PredSymbolWithPos,
        SpecCalleeKind::PredTypeOf,
        SpecCalleeKind::PredClTypeOf,
        SpecCalleeKind::PredFboundp,
        SpecCalleeKind::PredAutoloadDoLoad,
        SpecCalleeKind::EqInclProps,
        SpecCalleeKind::ArithIntrinsic { op: 0 },
        SpecCalleeKind::CbsymTierA { which: 0 },
        SpecCalleeKind::CbsymTierB,
    ];
    assert_eq!(SpecCalleeKind::Bytecode.slot_kind(), SpecSlotKind::Bytecode);
    for kind in subr_kinds {
        assert_eq!(kind.slot_kind(), SpecSlotKind::Subr, "{kind:?}");
    }
}

#[test]
fn a_subr_general_slot_is_built_with_its_binding_and_the_others_empty() {
    let expected =
        crate::emacs_core::value::Value::subr_from_sym_id(crate::emacs_core::intern::intern("car"))
            .bits() as u64;
    let bound = SpecSlot::for_site(SpecCalleeKind::SubrGeneral, 7, 42, expected);
    assert!(bound.holds_subr_binding());
    assert_eq!(bound.subr_binding(), (SymId(42), expected));
    assert_eq!(word_at(&bound, SPEC_SLOT_EPOCH_OFFSET), 7);
    assert!(bound.direct_entry().is_null());
    for kind in [
        SpecCalleeKind::Bytecode,
        SpecCalleeKind::PredRecordp,
        SpecCalleeKind::EqInclProps,
    ] {
        let slot = SpecSlot::for_site(kind, 9, 42, expected);
        assert!(!slot.holds_subr_binding(), "{kind:?}");
        assert!(slot.leaf_ptr().is_null());
        assert_eq!(word_at(&slot, SPEC_SLOT_KEY_OFFSET), 0);
        assert_eq!(word_at(&slot, SPEC_SLOT_EPOCH_OFFSET), 9);
    }
}

/// The direct entry goes with a cached, exact-arity, frameless leaf: armed
/// without one (a debug assertion) it would let a site enter nothing.
#[cfg(debug_assertions)]
#[test]
fn a_direct_entry_needs_its_leaf_and_a_frameless_key() {
    let slot = SpecSlot::at_epoch(1);
    let entry = 0x7f00_0000_3000usize as *const u8;
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| slot.arm_direct_entry(entry)))
            .is_err(),
        "no leaf cached"
    );
    let leaf = 0x7f00_0000_1000usize as *const CompiledLeaf;
    let consts = 0x7f00_0000_2000usize as *const Value;
    slot.arm_leaf(leaf, consts, false, true);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| slot.arm_direct_entry(entry)))
            .is_err(),
        "a framed leaf is never called directly"
    );
    assert!(slot.direct_entry().is_null());
}
