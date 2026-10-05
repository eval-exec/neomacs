use super::*;

#[derive(Clone, Copy)]
enum BeginChange {
    Expand,
    ContractThird,
    ContractHalf,
    SpreadSeven,
    Collapse,
    SpreadEleven,
}

fn node_slots(
    order: &GnuOverlayOrder<u16>,
) -> Vec<
    Option<(
        u16,
        Color,
        Option<OrderNodeId>,
        Option<OrderNodeId>,
        Option<OrderNodeId>,
    )>,
> {
    order
        .nodes
        .iter()
        .map(|slot| {
            slot.as_ref().map(|node| {
                (
                    node.identity,
                    node.color,
                    node.parent,
                    node.left,
                    node.right,
                )
            })
        })
        .collect()
}

#[test]
fn conversion_reinsert_preserves_exact_gnu_topology_without_changing_arena_or_identity_storage() {
    for count in [1_u16, 2, 3, 31, 257] {
        let mut starts: FxHashMap<_, _> = (0..count)
            .map(|identity| (identity, usize::from(identity.wrapping_mul(73) % 41)))
            .collect();
        let mut reference = GnuOverlayOrder::new();
        for identity in 0..count {
            assert!(reference.insert_by(identity, |existing| {
                starts[&identity].cmp(&starts[&existing])
            }));
        }
        // Include vacant slots, so a conversion cannot silently select a
        // different already-free slot or reorder the owner's existing free list.
        if count > 3 {
            for identity in (0..count).filter(|identity| identity % 7 == 0) {
                assert!(reference.remove(identity));
                starts.remove(&identity);
            }
        }
        let mut retained = reference.clone();
        let capacities = (
            retained.nodes.capacity(),
            retained.free.capacity(),
            retained.by_identity.capacity(),
        );
        for phase in [
            BeginChange::Expand,
            BeginChange::ContractThird,
            BeginChange::ContractHalf,
            BeginChange::SpreadSeven,
            BeginChange::Collapse,
            BeginChange::SpreadEleven,
        ] {
            let mut snapshot = Vec::new();
            reference.for_each_inorder(|identity| snapshot.push(identity));
            for identity in snapshot.iter().copied() {
                let old_begin = starts[&identity];
                let new_begin = match phase {
                    BeginChange::Expand => old_begin * 2 + 3,
                    BeginChange::ContractThird => old_begin / 3,
                    BeginChange::ContractHalf => old_begin / 2,
                    BeginChange::SpreadSeven => usize::from(identity % 7),
                    BeginChange::Collapse => 0,
                    BeginChange::SpreadEleven => usize::from(identity % 11),
                };
                if old_begin == new_begin {
                    continue;
                }
                let retained_slot = retained.by_identity[&identity];
                // Reference is the existing GNU-compatible remove/insert
                // implementation. Reuse must preserve full topology, because
                // future front-advancing edits expose pre-order, not just rank.
                assert!(reference.remove(identity));
                starts.insert(identity, new_begin);
                assert!(
                    reference.insert_by(identity, |existing| new_begin.cmp(&starts[&existing]))
                );
                assert!(
                    retained.reinsert_by(identity, |existing| new_begin.cmp(&starts[&existing]))
                );
                reference.assert_invariants();
                retained.assert_invariants();
                assert_eq!(retained.by_identity[&identity], retained_slot);
                assert_eq!(retained.root, reference.root);
                assert_eq!(node_slots(&retained), node_slots(&reference));
                assert_eq!(retained.free, reference.free);
                assert_eq!(retained.by_identity, reference.by_identity);
                assert_eq!(
                    retained.subset_in_preorder(&snapshot),
                    reference.subset_in_preorder(&snapshot)
                );
                assert_eq!(
                    (
                        retained.nodes.capacity(),
                        retained.free.capacity(),
                        retained.by_identity.capacity()
                    ),
                    capacities
                );
            }
        }
    }
}

#[test]
fn conversion_reinsert_missing_identity_keeps_the_tree_untouched() {
    let mut order = GnuOverlayOrder::<u16>::new();
    assert!(order.insert_by(1, |_| Ordering::Equal));
    let before = node_slots(&order);
    assert!(!order.reinsert_by(2, |_| panic!("missing identity must not compare")));
    assert_eq!(node_slots(&order), before);
    order.assert_invariants();
}
