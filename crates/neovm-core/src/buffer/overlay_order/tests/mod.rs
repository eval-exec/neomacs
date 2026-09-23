use super::*;

#[test]
fn three_equal_starts_match_gnu_preorder_and_stack_reinsertion() {
    let mut order = GnuOverlayOrder::new();
    for identity in [1_u8, 2, 3] {
        assert!(order.insert_by(identity, |_| Ordering::Equal));
        order.assert_invariants();
    }

    assert_eq!(order.subset_in_preorder(&[1, 2, 3]), vec![2, 3, 1]);

    for identity in [2, 3, 1] {
        assert!(order.remove(identity));
        order.assert_invariants();
    }
    for identity in [1, 3, 2] {
        assert!(order.insert_by(identity, |_| Ordering::Equal));
        order.assert_invariants();
    }

    assert_eq!(order.subset_in_preorder(&[1, 2, 3]), vec![3, 2, 1]);
}

#[test]
fn mixed_insertions_and_removals_preserve_red_black_invariants() {
    let mut order = GnuOverlayOrder::new();
    let starts: Vec<_> = (0_u16..257)
        .map(|identity| (identity, (identity.wrapping_mul(73) % 41) as usize))
        .collect();

    for (identity, start) in starts.iter().copied() {
        assert!(order.insert_by(identity, |existing| {
            let existing_start = starts[existing as usize].1;
            start.cmp(&existing_start)
        }));
        order.assert_invariants();
    }

    for offset in [0, 3, 1, 2] {
        for identity in (offset..257_u16).step_by(4) {
            assert!(order.remove(identity));
            order.assert_invariants();
        }
    }
    assert_eq!(order.len(), 0);
}
