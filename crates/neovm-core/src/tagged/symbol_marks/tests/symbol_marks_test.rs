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
fn first_visit_detection_preserves_marks_across_growth_and_word_boundaries() {
    let mut bits = SymbolMarkBits::default();
    let ids = [0, 63, 64, 4_097, 1, 65, 127, 128, 70_000];

    for id in ids {
        assert!(bits.insert_if_absent(SymId(id)), "first visit: {id}");
        assert!(!bits.insert_if_absent(SymId(id)), "duplicate visit: {id}");
    }
    for id in ids {
        assert!(bits.contains(SymId(id)), "mark lost during growth: {id}");
        assert!(!bits.insert_if_absent(SymId(id)));
    }
    assert!(!bits.contains(SymId(62)));
    assert!(!bits.contains(SymId(4_096)));
    assert!(!bits.contains(SymId(69_999)));
    assert_eq!(bits.count(), ids.len());
}

#[test]
fn first_visit_detection_shares_marks_with_insert_and_resets_each_cycle() {
    let mut bits = SymbolMarkBits::default();
    bits.insert(SymId(63));
    assert!(!bits.insert_if_absent(SymId(63)));
    assert!(bits.insert_if_absent(SymId(9_000)));
    bits.insert(SymId(9_000));
    assert_eq!(bits.count(), 2);

    bits.clear();
    assert!(bits.insert_if_absent(SymId(9_000)));
    assert!(!bits.insert_if_absent(SymId(9_000)));
    assert!(bits.insert_if_absent(SymId(63)));
    assert_eq!(bits.count(), 2);
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
