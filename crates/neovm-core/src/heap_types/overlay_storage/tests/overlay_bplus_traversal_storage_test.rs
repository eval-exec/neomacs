use super::*;

#[test]
fn overlay_traversal_frame_keeps_the_bounded_stack_compact() {
    // Every query carries nine frames. Node-local indexes need only
    // represent the node's fixed fanout, not arbitrary buffer positions.
    assert!(std::mem::size_of::<OrderedTraversalFrame>() <= 16);
}
