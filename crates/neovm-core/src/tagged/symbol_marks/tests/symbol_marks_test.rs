use super::super::symbol_marks::SymbolMarkBits;
use crate::emacs_core::intern::SymId;

#[test]
fn marks_are_per_id_and_grow_on_demand() {
    let mut bits = SymbolMarkBits::default();
    assert!(!bits.contains(SymId(0)));
    assert!(!bits.contains(SymId(70_000)));

    bits.insert(SymId(0));
    bits.insert(SymId(63));
    bits.insert(SymId(64));
    bits.insert(SymId(70_000));

    assert!(bits.contains(SymId(0)));
    assert!(bits.contains(SymId(63)));
    assert!(bits.contains(SymId(64)));
    assert!(bits.contains(SymId(70_000)));
    assert!(!bits.contains(SymId(1)));
    assert!(!bits.contains(SymId(65)));
    assert!(!bits.contains(SymId(69_999)));
    assert_eq!(bits.count(), 4);
}

#[test]
fn inserting_twice_is_idempotent() {
    let mut bits = SymbolMarkBits::default();
    bits.insert(SymId(5));
    bits.insert(SymId(5));
    assert_eq!(bits.count(), 1);
}

#[test]
fn clear_forgets_every_mark_but_keeps_answering_for_high_ids() {
    let mut bits = SymbolMarkBits::default();
    bits.insert(SymId(3));
    bits.insert(SymId(9_000));
    bits.clear();
    assert!(!bits.contains(SymId(3)));
    assert!(!bits.contains(SymId(9_000)));
    assert_eq!(bits.count(), 0);
    bits.insert(SymId(9_000));
    assert!(bits.contains(SymId(9_000)));
}
