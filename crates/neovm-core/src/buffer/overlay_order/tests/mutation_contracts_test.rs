use super::*;

fn assert_unchanged(order: &GnuOverlayOrder<u8>, before: &GnuOverlayOrder<u8>) {
    assert_eq!(order.root, before.root);
    assert_eq!(order.by_identity, before.by_identity);
    assert_eq!(order.free, before.free);
    assert_eq!(order.nodes.len(), before.nodes.len());
    for (actual, expected) in order.nodes.iter().zip(&before.nodes) {
        let fields = |node: &OrderNode<u8>| {
            (
                node.identity,
                node.color,
                node.parent,
                node.left,
                node.right,
            )
        };
        assert_eq!(actual.as_ref().map(fields), expected.as_ref().map(fields));
    }
    order.assert_invariants();
}

fn mirror_with_vacant_slot() -> GnuOverlayOrder<u8> {
    let mut order = GnuOverlayOrder::new();
    for identity in [1, 2, 3, 4, 5] {
        assert_eq!(
            order.insert_by(identity, |existing| identity.cmp(&existing)),
            Ok(())
        );
    }
    assert_eq!(order.remove(3), Ok(()));
    order.assert_invariants();
    order
}

#[test]
fn duplicate_insertions_return_typed_errors_without_comparing_or_changing_storage() {
    let mut order = mirror_with_vacant_slot();
    let before = order.clone();
    assert_eq!(
        order.insert_by(2, |_| panic!("duplicate insertion must not compare")),
        Err(GnuOverlayOrderError::AlreadyPresent)
    );
    assert_unchanged(&order, &before);
    assert_eq!(
        order.insert_before(2, Some(4)),
        Err(GnuOverlayOrderError::AlreadyPresent)
    );
    assert_unchanged(&order, &before);
}

#[test]
fn missing_removal_and_reinsertion_return_typed_errors_without_mutating_the_mirror() {
    let mut order = mirror_with_vacant_slot();
    let before = order.clone();
    assert_eq!(order.remove(3), Err(GnuOverlayOrderError::MissingIdentity));
    assert_unchanged(&order, &before);
    assert_eq!(
        order.reinsert_by(3, |_| panic!("missing reinsertion must not compare")),
        Err(GnuOverlayOrderError::MissingIdentity)
    );
    assert_unchanged(&order, &before);
}

#[test]
fn consuming_a_retained_node_keeps_its_slot_while_removal_recycles_it_once() {
    let mut order = mirror_with_vacant_slot();
    let retained_slot = order.by_identity[&2];
    let old_free = order.free.clone();
    assert_eq!(order.reinsert_by(2, |_| Ordering::Greater), Ok(()));
    assert_eq!(order.by_identity[&2], retained_slot);
    assert_eq!(order.free, old_free);
    order.assert_invariants();

    assert_eq!(order.remove(2), Ok(()));
    assert_eq!(order.free.last(), Some(&retained_slot));
    assert_eq!(order.free.len(), old_free.len() + 1);
    assert!(order.nodes[retained_slot.index()].is_none());
    assert_eq!(order.insert_by(6, |existing| 6_u8.cmp(&existing)), Ok(()));
    assert_eq!(order.by_identity[&6], retained_slot);
    assert_eq!(order.free, old_free);
    order.assert_invariants();
}
