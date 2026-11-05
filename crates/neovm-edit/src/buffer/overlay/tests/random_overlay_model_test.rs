//! Deterministic overlay mutation model and fixed GNU ordering checks.
//! The vector model checks semantics, not B+ shape, allocation sizes or time.

use super::*;

#[test]
fn overlay_equal_start_forced_order_survives_move_delete_and_refill() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let a = alloc_overlay(2, 5);
    let b = alloc_overlay(2, 5);
    let c = alloc_overlay(2, 5);
    for value in [a, b, c] {
        list.insert_overlay(value);
    }
    // Exact checked-in GNU observation already used by the overlay family;
    // do not sort these vectors or derive expected order from the tree.
    assert_eq!(overlays_at(&list, 3), vec![c, b, a]);
    assert!(list.delete_overlay(b));
    assert_eq!(overlays_at(&list, 3), vec![c, a]);
    list.move_overlay_to_emacs_byte_range(c, emacs_byte_range(9, 12));
    assert_eq!(overlays_at(&list, 3), vec![a]);
    assert_eq!(overlays_at(&list, 10), vec![c]);
    list.move_overlay_to_emacs_byte_range(c, emacs_byte_range(2, 5));
    assert_eq!(overlays_at(&list, 3), vec![c, a]);
    assert!(list.delete_overlay(c));
    assert!(list.delete_overlay(a));
    assert!(list.is_empty());
    let replacement = alloc_overlay(2, 5);
    list.insert_overlay(replacement);
    assert_eq!(overlays_at(&list, 3), vec![replacement]);
    list.index.assert_invariants();
}

#[derive(Clone, Copy)]
struct ModelOverlay {
    value: Value,
    owner: usize,
    start: usize,
    end: usize,
    front: bool,
    rear: bool,
    priority: i64,
}
