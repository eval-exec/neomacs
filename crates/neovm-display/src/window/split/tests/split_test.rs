use super::*;
use SplitDirection::{Horizontal, Vertical};

/// `EQ (Vwindow_combination_limit, Qt)` short-circuits every other term,
/// and the resulting parent is sealed.
#[test]
fn limit_t_always_makes_a_sealed_new_parent() {
    for parent in [None, Some(Horizontal), Some(Vertical)] {
        for split in [Horizontal, Vertical] {
            assert_eq!(
                SplitAttachment::decide(CombinationLimit::ForceNewParent, parent, split),
                SplitAttachment::NewParent(ParentSeal::Sealed),
                "parent={parent:?} split={split:?}"
            );
        }
    }
}

/// `NILP (o->parent)`: splitting the frame root always interposes a
/// parent, but does not seal it.
#[test]
fn splitting_the_root_makes_an_unsealed_new_parent() {
    for split in [Horizontal, Vertical] {
        assert_eq!(
            SplitAttachment::decide(CombinationLimit::TreeDecides, None, split),
            SplitAttachment::NewParent(ParentSeal::Unsealed)
        );
    }
}

/// An ortho-combined parent cannot hold the new sibling.
#[test]
fn ortho_combined_parent_makes_an_unsealed_new_parent() {
    assert_eq!(
        SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Vertical), Horizontal),
        SplitAttachment::NewParent(ParentSeal::Unsealed)
    );
    assert_eq!(
        SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Horizontal), Vertical),
        SplitAttachment::NewParent(ParentSeal::Unsealed)
    );
}

/// An iso-combined parent is reused -- the flat-combination case.
#[test]
fn iso_combined_parent_is_reused() {
    assert_eq!(
        SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Horizontal), Horizontal),
        SplitAttachment::ReuseParent
    );
    assert_eq!(
        SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Vertical), Vertical),
        SplitAttachment::ReuseParent
    );
}

/// Only `t` forces a new parent; `nil` and the other `display-buffer`
/// values defer to the tree.
#[test]
fn only_the_symbol_t_forces_a_new_parent() {
    assert_eq!(
        CombinationLimit::from_is_t(true),
        CombinationLimit::ForceNewParent
    );
    assert_eq!(
        CombinationLimit::from_is_t(false),
        CombinationLimit::TreeDecides
    );
}

/// The seal is what `recombine_windows` will later read back.
#[test]
fn seal_maps_to_the_stored_slot() {
    assert!(ParentSeal::Sealed.as_stored_slot());
    assert!(!ParentSeal::Unsealed.as_stored_slot());
}
