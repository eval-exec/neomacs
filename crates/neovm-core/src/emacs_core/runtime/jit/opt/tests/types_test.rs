use super::ir::ValueBits;
use super::mem::{AliasClass, Effects};
use super::types::{Range, TypeKind, TypeSet};

#[test]
fn opt_types_lattice_laws_over_kinds_ranges_and_singletons() {
    let sets = [
        TypeSet::BOTTOM,
        TypeSet::TOP,
        TypeSet::NIL,
        TypeSet::T,
        TypeSet::FIXNUM,
        TypeSet::CONS,
        TypeSet::LIST,
        TypeSet::NUMBER,
        TypeSet::HEAP,
        TypeSet::BOOLEAN,
        TypeSet::OTHER_VECLIKE,
        TypeSet::fixnum_range(Range { lo: -10, hi: 20 }),
        TypeSet::fixnum_range(Range { lo: 10, hi: 30 }),
        TypeSet::for_constant(ValueBits(6)),
    ];
    for a in sets {
        assert_eq!(a.join(a), a);
        assert_eq!(a.meet(a), a);
        assert_eq!(a.join(TypeSet::BOTTOM), a);
        assert_eq!(a.meet(TypeSet::TOP), a);
        for b in sets {
            assert_eq!(a.join(b), b.join(a), "join {a:?} {b:?}");
            assert_eq!(a.meet(b), b.meet(a), "meet {a:?} {b:?}");
            assert!(a.is_subset(a.join(b)));
            assert!(a.meet(b).is_subset(a));
            for c in sets {
                assert_eq!(
                    a.join(b).join(c),
                    a.join(b.join(c)),
                    "join {a:?} {b:?} {c:?}"
                );
                assert_eq!(
                    a.meet(b).meet(c),
                    a.meet(b.meet(c)),
                    "meet {a:?} {b:?} {c:?}"
                );
            }
        }
    }
}

#[test]
fn opt_types_widening_terminates_at_fixnum_boundaries() {
    let first = TypeSet::fixnum_range(Range { lo: 0, hi: 0 });
    let growing = TypeSet::fixnum_range(Range { lo: -1, hi: 1 });
    let widened = first.widen(growing);
    assert_eq!(widened, TypeSet::FIXNUM);
    assert_eq!(
        widened.widen(TypeSet::fixnum_range(Range { lo: -2, hi: 2 })),
        widened
    );
    assert_eq!(
        first
            .widen(TypeSet::fixnum_range(Range { lo: 0, hi: 1 }))
            .range(),
        Some(Range {
            lo: 0,
            hi: Range::FULL.hi
        })
    );
}

#[test]
fn opt_types_branch_and_inverse_car_narrowing() {
    let list = TypeSet::LIST;
    assert_eq!(list.meet(TypeSet::NIL), TypeSet::NIL);
    assert_eq!(list.without(TypeSet::NIL), TypeSet::CONS);
    assert_eq!(TypeSet::TOP.meet(TypeSet::FIXNUM), TypeSet::FIXNUM);
    assert!(
        !TypeSet::TOP
            .without(TypeSet::FIXNUM)
            .contains(TypeKind::Fixnum)
    );
    assert_eq!(
        list.inverse_car_non_nil(TypeSet::TOP.without(TypeSet::NIL)),
        TypeSet::CONS
    );
    assert_eq!(list.inverse_car_non_nil(TypeSet::TOP), list);
    assert_eq!(TypeSet::TOP.inverse_car_non_nil(TypeSet::T), TypeSet::TOP);
}

#[test]
fn opt_types_exact_constants_preserve_identity_and_partial_subtraction() {
    let a = TypeSet::FLOAT.with_singleton(ValueBits(0x1007));
    let b = TypeSet::FLOAT.with_singleton(ValueBits(0x2007));
    assert_eq!(a.meet(b), TypeSet::BOTTOM);
    assert_eq!(a.join(b), TypeSet::FLOAT);
    assert_eq!(TypeSet::FLOAT.without(a), TypeSet::FLOAT);
    assert_eq!(a.without(a), TypeSet::BOTTOM);
    assert_eq!(
        TypeSet::FIXNUM.without(TypeSet::for_constant(ValueBits(6))),
        TypeSet::FIXNUM
    );
}

#[test]
fn opt_types_symbol_position_blocks_dynamic_identity_folding() {
    for ty in [
        TypeSet::TOP,
        TypeSet::SYMBOL.join(TypeSet::OTHER_VECLIKE),
        TypeSet::INTEGER.join(TypeSet::OTHER_VECLIKE),
    ] {
        assert!(!ty.permits_symbol_identity_folding());
    }
    for ty in [TypeSet::SYMBOL, TypeSet::INTEGER, TypeSet::CONS] {
        assert!(ty.permits_symbol_identity_folding());
    }
    assert!(TypeSet::MARKER.meet(TypeSet::NUMBER).is_bottom());
    assert!(TypeSet::MARKER.is_subset(TypeSet::NUMBER_OR_MARKER));
}

#[test]
fn opt_types_constant_classification_never_dereferences_veclike_bits() {
    let unknown_heap = TypeSet::for_constant(ValueBits(5));
    assert!(unknown_heap.contains(TypeKind::Vector));
    assert!(unknown_heap.contains(TypeKind::Record));
    assert!(unknown_heap.contains(TypeKind::OtherVeclike));
    assert!(!unknown_heap.contains(TypeKind::Cons));
    assert!(unknown_heap.may_need_root());
    assert_eq!(TypeSet::for_constant(ValueBits(0)), TypeSet::NIL);
    assert_eq!(TypeSet::for_constant(ValueBits(8)), TypeSet::T);
    assert_eq!(
        TypeSet::for_constant(ValueBits(6)).range(),
        Some(Range { lo: 1, hi: 1 })
    );
}

#[test]
fn opt_alias_stores_distinguish_cons_fields_and_constant_vector_indices() {
    assert!(AliasClass::ConsCar.store_clobbers(AliasClass::ConsCar, None, None));
    assert!(!AliasClass::ConsCar.store_clobbers(AliasClass::ConsCdr, None, None));
    assert!(!AliasClass::VecElem.store_clobbers(AliasClass::VecElem, Some(3), Some(6)));
    assert!(AliasClass::VecElem.store_clobbers(AliasClass::VecElem, Some(3), None));
    assert!(AliasClass::Unknown.store_clobbers(AliasClass::ConsCdr, None, None));
}

#[test]
fn opt_alias_polls_clobber_heap_and_binding_facts_but_keep_immutable() {
    let poll = Effects::MAY_GC.with(Effects::MAY_REENTER);
    for class in [
        AliasClass::ConsCar,
        AliasClass::ConsCdr,
        AliasClass::VecElem,
        AliasClass::RecElem,
        AliasClass::Bindings,
        AliasClass::Buffer,
        AliasClass::Match,
    ] {
        assert!(class.clobbered_by(poll));
    }
    assert!(!AliasClass::Immutable.clobbered_by(poll));
    assert!(!AliasClass::ConsCar.clobbered_by(Effects::READ_HEAP));
    assert!(TypeSet::CONS.is_subset(TypeSet::HEAP));
}
