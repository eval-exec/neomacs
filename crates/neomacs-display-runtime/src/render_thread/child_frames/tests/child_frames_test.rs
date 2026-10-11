use super::*;

const TEST_ROOT_FRAME_ID: u64 = u64::MAX;

/// Helper: create a FrameGlyphBuffer with specified frame_id, position, size, and z_order.
fn make_child_buf(
    frame_id: u64,
    parent_x: f32,
    parent_y: f32,
    width: f32,
    height: f32,
    z_order: i32,
) -> FrameGlyphBuffer {
    let mut buf = FrameGlyphBuffer::with_size(width, height);
    buf.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(frame_id),
        neomacs_display_protocol::DisplayFrameId::new(TEST_ROOT_FRAME_ID),
        parent_x,
        parent_y,
        z_order,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    buf
}

fn make_manager() -> ChildFrameManager {
    let mut manager = ChildFrameManager::new();
    let mut root = FrameGlyphBuffer::with_size(10_000.0, 10_000.0);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(TEST_ROOT_FRAME_ID),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        i32::MIN,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    manager.set_root_frame(Some(&root));
    manager
}

// ===================================================================
// Default / empty state
// ===================================================================

#[test]
fn new_manager_is_empty() {
    let mgr = make_manager();
    assert!(mgr.is_empty());
    assert!(mgr.sorted_for_rendering().is_empty());
    assert!(mgr.frames.is_empty());
}

#[test]
fn new_manager_frame_counter_starts_at_zero() {
    let mgr = make_manager();
    assert_eq!(mgr.frame_counter, 0);
}

#[test]
fn empty_manager_hit_test_returns_none() {
    let mgr = make_manager();
    assert_eq!(mgr.hit_test(100.0, 100.0), None);
}

// ===================================================================
// tick()
// ===================================================================

#[test]
fn tick_increments_frame_counter() {
    let mut mgr = make_manager();
    assert_eq!(mgr.frame_counter, 0);
    mgr.tick();
    assert_eq!(mgr.frame_counter, 1);
    mgr.tick();
    assert_eq!(mgr.frame_counter, 2);
}

// ===================================================================
// update_frame()
// ===================================================================

#[test]
fn update_frame_inserts_single_frame() {
    let mut mgr = make_manager();
    let buf = make_child_buf(100, 50.0, 75.0, 200.0, 100.0, 0);
    mgr.update_frame(buf);

    assert!(!mgr.is_empty());
    assert_eq!(mgr.frames.len(), 1);
    assert!(mgr.frames.contains_key(&100));

    let entry = mgr.frames.get(&100).unwrap();
    assert_eq!(entry.frame_id, 100);
    assert_eq!(entry.abs_x, 50.0);
    assert_eq!(entry.abs_y, 75.0);
    assert_eq!(entry.frame.width, 200.0);
    assert_eq!(entry.frame.height, 100.0);
}

#[test]
fn update_frame_sets_last_updated_to_current_counter() {
    let mut mgr = make_manager();
    mgr.tick(); // counter = 1
    mgr.tick(); // counter = 2

    let buf = make_child_buf(10, 0.0, 0.0, 100.0, 100.0, 0);
    mgr.update_frame(buf);

    let entry = mgr.frames.get(&10).unwrap();
    assert_eq!(entry.last_updated, 2);
}

#[test]
fn update_frame_replaces_existing_frame() {
    let mut mgr = make_manager();

    // Insert initial
    let buf1 = make_child_buf(42, 10.0, 20.0, 100.0, 50.0, 1);
    mgr.update_frame(buf1);

    // Replace with new position and size
    let buf2 = make_child_buf(42, 30.0, 40.0, 200.0, 80.0, 2);
    mgr.update_frame(buf2);

    assert_eq!(mgr.frames.len(), 1);
    let entry = mgr.frames.get(&42).unwrap();
    assert_eq!(entry.abs_x, 30.0);
    assert_eq!(entry.abs_y, 40.0);
    assert_eq!(entry.frame.width, 200.0);
    assert_eq!(entry.frame.height, 80.0);
    assert_eq!(entry.frame.frame_placement.z_order(), 2);
}

#[test]
fn update_frame_keeps_ingest_seq_for_identical_frame() {
    let mut mgr = make_manager();
    let buf = make_child_buf(42, 30.0, 40.0, 200.0, 80.0, 2);

    mgr.update_frame(buf.clone());
    let first = mgr.frames.get(&42).unwrap().ingest_seq;
    mgr.tick();

    mgr.update_frame(buf);

    let entry = mgr.frames.get(&42).unwrap();
    assert_eq!(
        entry.ingest_seq, first,
        "an unchanged child frame must not look newly installed every render tick"
    );
    assert_eq!(
        entry.last_updated, 1,
        "the manager may refresh liveness without invalidating the rendered payload"
    );
}

#[test]
fn update_frame_abs_position_from_parent_xy() {
    let mut mgr = make_manager();
    let buf = make_child_buf(1, 123.5, 456.7, 300.0, 200.0, 0);
    mgr.update_frame(buf);

    let entry = mgr.frames.get(&1).unwrap();
    assert_eq!(entry.abs_x, 123.5);
    assert_eq!(entry.abs_y, 456.7);
}

#[test]
fn nested_child_composes_parent_relative_offsets_once_and_clips_to_root() {
    let mut mgr = make_manager();
    let mut root = FrameGlyphBuffer::with_size(200.0, 160.0);
    root.presentation_id = neomacs_display_protocol::PresentationId::new(1);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(10),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.set_root_frame(Some(&root));
    let mut parent = make_child_buf(20, 100.0, 80.0, 120.0, 100.0, 2);
    parent.presentation_id = neomacs_display_protocol::PresentationId::new(2);
    parent.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(20),
        root.frame_placement.frame(),
        100.0,
        80.0,
        2,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.update_frame(parent);
    let mut nested = make_child_buf(30, 15.0, 12.0, 100.0, 80.0, 4);
    nested.presentation_id = neomacs_display_protocol::PresentationId::new(3);
    nested.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(30),
        neomacs_display_protocol::DisplayFrameId::new(20),
        15.0,
        12.0,
        4,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.update_frame(nested);

    let entry = mgr.frames.get(&30).unwrap();
    assert_eq!(
        (
            entry.frame.frame_placement.outer_in_parent().x(),
            entry.frame.frame_placement.outer_in_parent().y()
        ),
        (15.0, 12.0)
    );
    assert_eq!((entry.abs_x, entry.abs_y), (115.0, 92.0));
    assert_eq!(
        entry.clip_in_root,
        PresentedClip::Rect(
            neomacs_display_protocol::RootSurfaceRect::new(115.0, 92.0, 85.0, 68.0).unwrap(),
        )
    );
    assert_eq!(mgr.hit_test(199.0, 159.0).unwrap().0, 30);
    assert_eq!(mgr.hit_test(201.0, 159.0), None);
}

#[test]
fn fully_clipped_child_is_not_hittable() {
    let mut mgr = make_manager();
    let mut root = FrameGlyphBuffer::with_size(100.0, 100.0);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(10),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.set_root_frame(Some(&root));
    let mut child = make_child_buf(20, 150.0, 0.0, 40.0, 40.0, 1);
    child.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(20),
        neomacs_display_protocol::DisplayFrameId::new(10),
        150.0,
        0.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    assert!(mgr.update_frame(child));
    assert_eq!(mgr.frames[&20].clip_in_root, PresentedClip::Empty);
    assert_eq!(mgr.hit_test(155.0, 5.0), None);
}

#[test]
fn negative_child_origin_is_preserved_and_hit_testing_uses_the_clipped_extent() {
    let mut mgr = make_manager();
    let mut root = FrameGlyphBuffer::with_size(100.0, 100.0);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(10),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.set_root_frame(Some(&root));

    let mut child = make_child_buf(20, -5.0, -7.0, 40.0, 30.0, 1);
    child.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(20),
        neomacs_display_protocol::DisplayFrameId::new(10),
        -5.0,
        -7.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );

    assert!(mgr.update_frame(child));
    let entry = &mgr.frames[&20];
    assert_eq!((entry.abs_x, entry.abs_y), (-5.0, -7.0));
    assert_eq!(
        entry.clip_in_root,
        PresentedClip::Rect(
            neomacs_display_protocol::RootSurfaceRect::new(0.0, 0.0, 35.0, 23.0).unwrap(),
        )
    );
    assert_eq!(mgr.hit_test(0.0, 0.0), Some((20, 5.0, 7.0)));
}

#[test]
fn missing_parent_is_rejected_instead_of_rewritten_as_a_root() {
    let mut mgr = make_manager();
    let mut orphan = make_child_buf(30, 15.0, 12.0, 100.0, 80.0, 4);
    orphan.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(30),
        neomacs_display_protocol::DisplayFrameId::new(20),
        15.0,
        12.0,
        4,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );

    assert!(!mgr.update_frame(orphan));
    assert!(!mgr.frames.contains_key(&30));
}

#[test]
fn overflowing_derived_child_origin_is_rejected_without_panicking() {
    let mut mgr = make_manager();
    let parent = make_child_buf(20, f32::MAX, 0.0, 10.0, 10.0, 1);
    assert!(mgr.update_frame(parent));

    let mut nested = make_child_buf(30, f32::MAX, 0.0, 10.0, 10.0, 2);
    nested.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(30),
        neomacs_display_protocol::DisplayFrameId::new(20),
        f32::MAX,
        0.0,
        2,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );

    assert!(!mgr.update_frame(nested));
    assert!(!mgr.frames.contains_key(&30));
}

#[test]
fn cyclic_replacement_is_rejected_transactionally() {
    let mut mgr = make_manager();
    let mut root = FrameGlyphBuffer::with_size(200.0, 160.0);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(10),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.set_root_frame(Some(&root));
    let mut parent = make_child_buf(20, 100.0, 80.0, 100.0, 80.0, 1);
    parent.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(20),
        neomacs_display_protocol::DisplayFrameId::new(10),
        100.0,
        80.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    assert!(mgr.update_frame(parent));
    let mut nested = make_child_buf(30, 15.0, 12.0, 80.0, 60.0, 2);
    nested.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(30),
        neomacs_display_protocol::DisplayFrameId::new(20),
        15.0,
        12.0,
        2,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    assert!(mgr.update_frame(nested));

    let mut cyclic_parent = make_child_buf(20, 100.0, 80.0, 100.0, 80.0, 1);
    cyclic_parent.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(20),
        neomacs_display_protocol::DisplayFrameId::new(30),
        100.0,
        80.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    assert!(!mgr.update_frame(cyclic_parent));
    assert_eq!(
        mgr.frames[&20]
            .frame
            .frame_placement
            .parent()
            .unwrap()
            .get(),
        10
    );
    assert_eq!(
        (mgr.frames[&30].abs_x, mgr.frames[&30].abs_y),
        (115.0, 92.0)
    );
}

#[test]
fn removing_parent_cascades_to_all_descendants() {
    let mut mgr = make_manager();
    let mut root = FrameGlyphBuffer::with_size(200.0, 160.0);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(10),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    mgr.set_root_frame(Some(&root));
    let mut parent = make_child_buf(20, 100.0, 80.0, 100.0, 80.0, 1);
    parent.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(20),
        neomacs_display_protocol::DisplayFrameId::new(10),
        100.0,
        80.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    assert!(mgr.update_frame(parent));
    let mut nested = make_child_buf(30, 15.0, 12.0, 80.0, 60.0, 2);
    nested.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(30),
        neomacs_display_protocol::DisplayFrameId::new(20),
        15.0,
        12.0,
        2,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    assert!(mgr.update_frame(nested));

    assert!(mgr.remove_frame(20));
    assert!(!mgr.frames.contains_key(&20));
    assert!(!mgr.frames.contains_key(&30));
}

// ===================================================================
// Z-order sorting (render_order / sorted_for_rendering)
// ===================================================================

#[test]
fn sorted_for_rendering_orders_by_z_order_ascending() {
    let mut mgr = make_manager();

    // Insert in non-sorted order
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 10));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 1));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 100.0, 100.0, 5));

    let order = mgr.sorted_for_rendering();
    assert_eq!(order.len(), 3);
    assert_eq!(order[0], 2); // z_order=1, lowest
    assert_eq!(order[1], 3); // z_order=5
    assert_eq!(order[2], 1); // z_order=10, highest
}

#[test]
fn sorted_for_rendering_handles_negative_z_order() {
    let mut mgr = make_manager();

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, -5));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 100.0, 100.0, 5));

    let order = mgr.sorted_for_rendering();
    assert_eq!(order[0], 2); // z_order=-5
    assert_eq!(order[1], 1); // z_order=0
    assert_eq!(order[2], 3); // z_order=5
}

#[test]
fn sorted_for_rendering_same_z_order_is_stable_count() {
    let mut mgr = make_manager();

    // All same z_order -- we just verify all 3 are present
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 100.0, 100.0, 0));

    let order = mgr.sorted_for_rendering();
    assert_eq!(order.len(), 3);
    let mut sorted = order.to_vec();
    sorted.sort();
    assert_eq!(sorted, vec![1, 2, 3]);
}

#[test]
fn render_order_updates_when_z_order_changes() {
    let mut mgr = make_manager();

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 1));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 2));

    // Frame 1 should be first (lower z)
    assert_eq!(mgr.sorted_for_rendering()[0], 1);

    // Now update frame 1 with higher z_order
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 10));

    // Frame 2 should now be first
    assert_eq!(mgr.sorted_for_rendering()[0], 2);
    assert_eq!(mgr.sorted_for_rendering()[1], 1);
}

// ===================================================================
// remove_frame()
// ===================================================================

#[test]
fn remove_frame_removes_existing() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 1));

    mgr.remove_frame(1);
    assert_eq!(mgr.frames.len(), 1);
    assert!(!mgr.frames.contains_key(&1));
    assert!(mgr.frames.contains_key(&2));
    assert_eq!(mgr.sorted_for_rendering(), &[2]);
}

#[test]
fn remove_frame_nonexistent_is_noop() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));

    mgr.remove_frame(999); // does not exist
    assert_eq!(mgr.frames.len(), 1);
    assert_eq!(mgr.sorted_for_rendering().len(), 1);
}

#[test]
fn remove_all_frames_leaves_empty() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 1));

    mgr.remove_frame(1);
    mgr.remove_frame(2);
    assert!(mgr.is_empty());
    assert!(mgr.sorted_for_rendering().is_empty());
}

// ===================================================================
// hit_test()
// ===================================================================

#[test]
fn hit_test_single_frame_inside() {
    let mut mgr = make_manager();
    // Frame at (100, 200) with size 300x150
    mgr.update_frame(make_child_buf(1, 100.0, 200.0, 300.0, 150.0, 0));

    // Click inside the frame
    let result = mgr.hit_test(150.0, 250.0);
    assert!(result.is_some());
    let (frame_id, local_x, local_y) = result.unwrap();
    assert_eq!(frame_id, 1);
    assert_eq!(local_x, 50.0); // 150 - 100
    assert_eq!(local_y, 50.0); // 250 - 200
}

#[test]
fn hit_test_single_frame_outside() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 100.0, 200.0, 300.0, 150.0, 0));

    // Click to the left
    assert_eq!(mgr.hit_test(50.0, 250.0), None);
    // Click above
    assert_eq!(mgr.hit_test(150.0, 150.0), None);
    // Click to the right (400 = 100 + 300, boundary is exclusive)
    assert_eq!(mgr.hit_test(400.0, 250.0), None);
    // Click below (350 = 200 + 150, boundary is exclusive)
    assert_eq!(mgr.hit_test(150.0, 350.0), None);
}

#[test]
fn hit_test_frame_boundary_top_left_corner() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 100.0, 200.0, 300.0, 150.0, 0));

    // Exact top-left corner (inclusive)
    let result = mgr.hit_test(100.0, 200.0);
    assert!(result.is_some());
    let (_, lx, ly) = result.unwrap();
    assert_eq!(lx, 0.0);
    assert_eq!(ly, 0.0);
}

#[test]
fn hit_test_frame_boundary_bottom_right_exclusive() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));

    // Right at the boundary (100.0 is exclusive)
    assert_eq!(mgr.hit_test(100.0, 50.0), None);
    assert_eq!(mgr.hit_test(50.0, 100.0), None);

    // Just inside the boundary
    let result = mgr.hit_test(99.9, 99.9);
    assert!(result.is_some());
}

#[test]
fn hit_test_overlapping_frames_returns_topmost() {
    let mut mgr = make_manager();

    // Frame A: z_order=1, at (0,0) 200x200
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 200.0, 200.0, 1));
    // Frame B: z_order=10, at (50,50) 200x200 -- overlaps with A, on top
    mgr.update_frame(make_child_buf(2, 50.0, 50.0, 200.0, 200.0, 10));

    // Click in overlap region (100, 100)
    let result = mgr.hit_test(100.0, 100.0);
    assert!(result.is_some());
    let (frame_id, local_x, local_y) = result.unwrap();
    assert_eq!(frame_id, 2); // Topmost (higher z_order)
    assert_eq!(local_x, 50.0); // 100 - 50
    assert_eq!(local_y, 50.0); // 100 - 50
}

#[test]
fn hit_test_overlapping_frames_returns_lower_when_top_not_hit() {
    let mut mgr = make_manager();

    // Frame A: z_order=1, at (0,0) 200x200
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 200.0, 200.0, 1));
    // Frame B: z_order=10, at (50,50) 200x200
    mgr.update_frame(make_child_buf(2, 50.0, 50.0, 200.0, 200.0, 10));

    // Click in area only covered by Frame A (25, 25)
    let result = mgr.hit_test(25.0, 25.0);
    assert!(result.is_some());
    let (frame_id, local_x, local_y) = result.unwrap();
    assert_eq!(frame_id, 1); // Only frame A covers this point
    assert_eq!(local_x, 25.0);
    assert_eq!(local_y, 25.0);
}

#[test]
fn hit_test_no_frames_hit() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 100.0, 100.0, 50.0, 50.0, 0));
    mgr.update_frame(make_child_buf(2, 200.0, 200.0, 50.0, 50.0, 1));

    // Click between the two frames
    assert_eq!(mgr.hit_test(175.0, 175.0), None);
}

#[test]
fn hit_test_three_overlapping_layers() {
    let mut mgr = make_manager();

    // Three frames all overlapping at (100, 100), different z orders
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 200.0, 200.0, 1));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 200.0, 200.0, 5));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 200.0, 200.0, 10));

    let result = mgr.hit_test(100.0, 100.0);
    assert_eq!(result.unwrap().0, 3); // Topmost z_order=10
}

// ===================================================================
// prune_stale()
// ===================================================================

#[test]
fn prune_stale_removes_old_frames() {
    let mut mgr = make_manager();

    // Frame inserted at counter=0
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));

    // Advance counter by 5
    for _ in 0..5 {
        mgr.tick();
    }

    // Frame inserted at counter=5
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 1));

    assert_eq!(mgr.frames.len(), 2);

    // Prune frames not updated in last 3 cycles
    // counter=5, threshold=5-3=2, so frame 1 (updated at 0) is stale
    mgr.prune_stale(3);

    assert_eq!(mgr.frames.len(), 1);
    assert!(!mgr.frames.contains_key(&1));
    assert!(mgr.frames.contains_key(&2));
}

#[test]
fn prune_stale_keeps_recently_updated() {
    let mut mgr = make_manager();

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.tick(); // counter=1

    // Frame updated at counter=0, max_age=5, threshold=1-5=0 (saturating)
    // So threshold=0, frame updated at 0 >= 0, keep it
    mgr.prune_stale(5);

    assert_eq!(mgr.frames.len(), 1);
}

#[test]
fn prune_stale_no_frames_is_noop() {
    let mut mgr = make_manager();
    mgr.tick();
    mgr.prune_stale(1);
    assert!(mgr.is_empty());
}

#[test]
fn prune_stale_all_stale_leaves_empty() {
    let mut mgr = make_manager();

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 1));

    // Advance far past both frames
    for _ in 0..100 {
        mgr.tick();
    }

    mgr.prune_stale(1);
    assert!(mgr.is_empty());
    assert!(mgr.sorted_for_rendering().is_empty());
}

#[test]
fn prune_stale_updates_render_order() {
    let mut mgr = make_manager();

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 1));
    mgr.tick(); // counter=1
    mgr.tick(); // counter=2
    mgr.tick(); // counter=3

    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 2));

    // Frame 1 updated at 0, frame 2 updated at 3
    // Prune with max_age=2: threshold=3-2=1, frame 1 (0 < 1) is stale
    mgr.prune_stale(2);

    let order = mgr.sorted_for_rendering();
    assert_eq!(order.len(), 1);
    assert_eq!(order[0], 2);
}

#[test]
fn prune_stale_boundary_exact_threshold() {
    let mut mgr = make_manager();

    // counter=0, insert frame
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));

    mgr.tick(); // counter=1
    mgr.tick(); // counter=2

    // max_age=2, counter=2, threshold=2-2=0
    // Frame updated at 0 >= 0, so it should be kept (boundary case)
    mgr.prune_stale(2);
    assert_eq!(mgr.frames.len(), 1);
}

#[test]
fn prune_stale_max_age_zero_removes_all_except_current() {
    let mut mgr = make_manager();

    // Insert at counter=0
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));

    // max_age=0, counter=0, threshold=0-0=0
    // Frame updated at 0 >= 0, keep
    mgr.prune_stale(0);
    assert_eq!(mgr.frames.len(), 1);

    // Now tick and prune with 0
    mgr.tick(); // counter=1
    // threshold=1-0=1, frame updated at 0 < 1, stale
    mgr.prune_stale(0);
    assert!(mgr.is_empty());
}

#[test]
fn prune_stale_saturating_sub_prevents_underflow() {
    let mut mgr = make_manager();
    // counter=0, max_age=1000 -- saturating_sub yields 0
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.prune_stale(1000);
    // Frame updated at 0 >= 0, so it should be kept
    assert_eq!(mgr.frames.len(), 1);
}

// ===================================================================
// render_order() after insertions and removals
// ===================================================================

#[test]
fn render_order_correct_after_mixed_operations() {
    let mut mgr = make_manager();

    // Insert 3 frames
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 10));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 5));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 100.0, 100.0, 15));

    // Order should be: 2(z=5), 1(z=10), 3(z=15)
    assert_eq!(mgr.sorted_for_rendering(), &[2, 1, 3]);

    // Remove middle one
    mgr.remove_frame(1);
    assert_eq!(mgr.sorted_for_rendering(), &[2, 3]);

    // Add new frame with z_order between remaining
    mgr.update_frame(make_child_buf(4, 0.0, 0.0, 100.0, 100.0, 7));
    assert_eq!(mgr.sorted_for_rendering(), &[2, 4, 3]);
}

#[test]
fn render_order_after_update_preserves_all_frames() {
    let mut mgr = make_manager();

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 1));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 2));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 100.0, 100.0, 3));

    // Update frame 2 with new z_order
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 100));

    let order = mgr.sorted_for_rendering();
    assert_eq!(order.len(), 3);
    // Order: 1(z=1), 3(z=3), 2(z=100)
    assert_eq!(order[0], 1);
    assert_eq!(order[1], 3);
    assert_eq!(order[2], 2);
}

#[test]
fn render_order_single_frame() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(42, 10.0, 20.0, 300.0, 200.0, 0));

    let order = mgr.sorted_for_rendering();
    assert_eq!(order.len(), 1);
    assert_eq!(order[0], 42);
}

// ===================================================================
// is_empty()
// ===================================================================

#[test]
fn is_empty_after_insert_and_remove() {
    let mut mgr = make_manager();
    assert!(mgr.is_empty());

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    assert!(!mgr.is_empty());

    mgr.remove_frame(1);
    assert!(mgr.is_empty());
}

// ===================================================================
// Combined: hit_test after prune_stale
// ===================================================================

#[test]
fn hit_test_after_prune_stale() {
    let mut mgr = make_manager();

    // Insert two non-overlapping frames
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));

    // Advance and insert second
    for _ in 0..10 {
        mgr.tick();
    }
    mgr.update_frame(make_child_buf(2, 200.0, 0.0, 100.0, 100.0, 1));

    // Prune frame 1 (stale)
    mgr.prune_stale(3);
    assert_eq!(mgr.frames.len(), 1);

    // Hit test on frame 1's position should miss
    assert_eq!(mgr.hit_test(50.0, 50.0), None);

    // Hit test on frame 2's position should hit
    let result = mgr.hit_test(250.0, 50.0);
    assert!(result.is_some());
    assert_eq!(result.unwrap().0, 2);
}

// ===================================================================
// Combined: z-order and hit test interaction
// ===================================================================

#[test]
fn hit_test_respects_updated_z_order() {
    let mut mgr = make_manager();

    // Two overlapping frames, frame 1 on top
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 200.0, 200.0, 10));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 200.0, 200.0, 5));

    // Frame 1 is on top
    assert_eq!(mgr.hit_test(100.0, 100.0).unwrap().0, 1);

    // Swap z-orders by updating
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 200.0, 200.0, 1));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 200.0, 200.0, 20));

    // Now frame 2 is on top
    assert_eq!(mgr.hit_test(100.0, 100.0).unwrap().0, 2);
}

// ===================================================================
// Lifecycle animation state
// ===================================================================

use neomacs_display_protocol::motion_spec::{MotionDuration, TweenSpec};
use neomacs_display_protocol::scroll_animation::TransitionEasing;

fn tween_100ms() -> MotionSpec {
    MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::new(std::time::Duration::from_millis(100))
            .expect("positive duration"),
        easing: TransitionEasing::EaseOutQuad,
        bezier: None,
    })
}

fn instant() -> MotionSpec {
    MotionSpec::Instant
}

fn sample_at(origin: EventTime, millis: u64) -> FrameSample {
    FrameSample::new(
        origin.plus(std::time::Duration::from_millis(millis)),
        std::time::Duration::ZERO,
    )
}

/// A child frame whose *parent* is another child frame, so subtree removal
/// and z-order tests exercise ancestry rather than the fixture's root.
fn make_nested_buf(
    frame_id: u64,
    parent_id: u64,
    parent_x: f32,
    parent_y: f32,
    width: f32,
    height: f32,
    z_order: i32,
) -> FrameGlyphBuffer {
    let mut buf = make_child_buf(frame_id, parent_x, parent_y, width, height, z_order);
    buf.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(frame_id),
        neomacs_display_protocol::DisplayFrameId::new(parent_id),
        parent_x,
        parent_y,
        z_order,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    buf
}

fn origin_now() -> EventTime {
    // The same observation the production lifecycle paths use
    // (`observe_platform_now`); the motion tests share the idiom.
    neomacs_display_protocol::frame_time::observe_platform_now()
}

#[test]
fn open_animation_advances_by_sampling_not_stepping() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    let origin = origin_now();
    mgr.begin_open_animation(1, tween_100ms(), origin, 8.0, 1.0);

    // Sampling, not stepping: the same instant gives the same answer.
    let early = mgr
        .frames
        .get(&1)
        .unwrap()
        .animation
        .as_ref()
        .unwrap()
        .motion
        .sample(sample_at(origin, 0));
    assert_eq!(early.progress, 0.0);
    let mid = mgr
        .frames
        .get(&1)
        .unwrap()
        .animation
        .as_ref()
        .unwrap()
        .motion
        .sample(sample_at(origin, 50));
    assert!(
        (mid.progress - 0.75).abs() < 1e-3,
        "ease-out-quad at half time"
    );
    assert!(mid.content_mix.get() > 0.0 && mid.content_mix.get() <= 1.0);
    let late = mgr
        .frames
        .get(&1)
        .unwrap()
        .animation
        .as_ref()
        .unwrap()
        .motion
        .sample(sample_at(origin, 101));
    assert!(late.finished);
}

#[test]
fn open_animation_survives_a_content_refresh() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.begin_open_animation(1, tween_100ms(), origin_now(), 8.0, 1.0);

    // A completion popup redraws per keystroke; the fade must continue, not
    // restart.
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 120.0, 100.0, 0));
    assert!(mgr.frames.get(&1).unwrap().animation.is_some());
}

#[test]
fn an_instant_open_spec_builds_no_animation_state() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.begin_open_animation(1, instant(), origin_now(), 8.0, 1.0);
    assert!(mgr.frames.get(&1).unwrap().animation.is_none());
}

#[test]
fn retire_moves_the_subtree_to_dying_and_out_of_interaction() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_nested_buf(2, 1, 0.0, 100.0, 100.0, 100.0, 0)); // child of 1

    let origin = origin_now();
    assert!(mgr.retire_frame(1, tween_100ms(), origin, 6.0, 1.0));

    // Gone from every interaction path...
    assert!(!mgr.frames.contains_key(&1));
    assert!(!mgr.frames.contains_key(&2));
    assert_eq!(mgr.hit_test(50.0, 50.0), None);
    // ...but retained for the fade, with a bound close animation.
    assert_eq!(mgr.dying.len(), 2);
    assert!(mgr.has_dying());
    assert!(mgr.dying_entry(1).is_some());
    assert!(mgr.dying_entry(2).is_some());

    // The close path fades: content_mix rises while visible opacity falls.
    let early = mgr
        .dying_entry(1)
        .unwrap()
        .animation
        .motion
        .sample(sample_at(origin, 0));
    assert_eq!(early.content_mix.get(), 0.0, "not yet faded");
    let late = mgr
        .dying_entry(1)
        .unwrap()
        .animation
        .motion
        .sample(sample_at(origin, 101));
    assert!(late.finished, "past the tween's duration");
}

#[test]
fn an_instant_close_drops_outright_with_no_dying_entry() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    let origin = origin_now();
    assert!(mgr.retire_frame(1, instant(), origin, 6.0, 1.0));
    assert!(mgr.dying.is_empty());
    assert!(mgr.frames.is_empty());
}

#[test]
fn reinstalling_a_dying_frame_id_resurrects_it_alone() {
    // Emacs re-showing the same popup mid-fade-out must cancel the fade and
    // bring the frame back, while unrelated dying frames keep dying.
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 100.0, 100.0, 100.0, 0));
    let origin = origin_now();
    mgr.retire_frame(1, tween_100ms(), origin, 0.0, 1.0);
    mgr.retire_frame(2, tween_100ms(), origin, 0.0, 1.0);
    assert_eq!(mgr.dying.len(), 2);

    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    assert!(mgr.frames.contains_key(&1));
    assert!(
        mgr.dying_entry(1).is_none(),
        "the resurrected id stops dying"
    );
    assert!(
        mgr.dying_entry(2).is_some(),
        "unrelated dying frames continue"
    );
}

#[test]
fn prune_dying_drops_only_finished_entries() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 0));
    mgr.update_frame(make_child_buf(2, 0.0, 100.0, 100.0, 100.0, 0));
    let origin = origin_now();
    mgr.retire_frame(1, tween_100ms(), origin, 0.0, 1.0);
    mgr.retire_frame(2, tween_100ms(), origin, 0.0, 1.0);
    assert_eq!(mgr.dying.len(), 2);

    mgr.prune_dying(sample_at(origin, 10));
    assert_eq!(mgr.dying.len(), 2, "mid-fade entries are retained");

    mgr.prune_dying(sample_at(origin, 200));
    assert!(mgr.dying.is_empty(), "finished entries are dropped");
}

#[test]
fn merged_render_order_interleaves_living_and_dying_by_z() {
    let mut mgr = make_manager();
    // z 5 (living), z 3 (will die), z 1 (living), z 1 (will die)
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 5));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 3));
    mgr.update_frame(make_child_buf(3, 0.0, 0.0, 100.0, 100.0, 1));
    mgr.update_frame(make_child_buf(4, 0.0, 0.0, 100.0, 100.0, 1));
    let origin = origin_now();
    mgr.retire_frame(2, tween_100ms(), origin, 0.0, 1.0);
    mgr.retire_frame(4, tween_100ms(), origin, 0.0, 1.0);

    let order: Vec<u64> = mgr
        .merged_render_order()
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        order,
        vec![4, 3, 2, 1],
        "z ascending, with the z-3 corpse between the z-1 and z-5 living frames"
    );
}

#[test]
fn merged_render_order_puts_a_corpse_beneath_a_living_frame_at_equal_z() {
    // The dismissal case: the dying popup is being replaced by the popup
    // that took its place at the same z. The old picture must recede
    // *beneath* the new one.
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 7));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 7));
    let origin = origin_now();
    mgr.retire_frame(1, tween_100ms(), origin, 0.0, 1.0);

    let order = mgr.merged_render_order();
    assert_eq!(order, vec![(1, true), (2, false)]);
}

#[test]
fn merged_render_order_is_empty_with_no_frames() {
    let mgr = make_manager();
    assert!(mgr.merged_render_order().is_empty());
}

#[test]
fn merged_render_order_keeps_living_frames_when_only_they_exist() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 0.0, 0.0, 100.0, 100.0, 2));
    mgr.update_frame(make_child_buf(2, 0.0, 0.0, 100.0, 100.0, 1));
    let order: Vec<u64> = mgr
        .merged_render_order()
        .iter()
        .map(|(id, dying)| {
            assert!(!dying);
            *id
        })
        .collect();
    assert_eq!(order, vec![2, 1]);
}

#[test]
fn drift_glides_from_its_departure_toward_the_placement() {
    let mut mgr = make_manager();
    // The popup is placed at (240, 180); the re-anchor moved it there from
    // (40, 0).
    mgr.update_frame(make_child_buf(1, 240.0, 180.0, 100.0, 100.0, 0));
    let origin = origin_now();
    assert!(mgr.begin_drift(1, 40.0, 0.0, tween_100ms(), origin));

    // Sampled, not stepped: the same instant gives the same position.
    let entry = mgr.frames.get(&1).unwrap();
    let drift = entry.drift.as_ref().unwrap();
    let early = drift.motion.sample(sample_at(origin, 0));
    assert_eq!(early.progress, 0.0, "not yet departed");
    let mid = drift.motion.sample(sample_at(origin, 50));
    assert!(
        (mid.progress - 0.75).abs() < 1e-3,
        "ease-out-quad at half time"
    );
    // The interpolation itself is the scene pass's job; the drift owns the
    // departure and the curve.
    let late = drift.motion.sample(sample_at(origin, 101));
    assert!(late.finished);
}

#[test]
fn drift_retargets_from_the_last_drawn_position() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 240.0, 180.0, 100.0, 100.0, 0));
    let origin = origin_now();
    mgr.begin_drift(1, 40.0, 0.0, tween_100ms(), origin);

    // The scene pass painted the popup at (140, 90), moving at 1.0/s.
    mgr.record_drift_drawn(1, 140.0, 90.0, 1.0);

    // A second re-anchor arrives mid-drift: the fresh drift departs from
    // where the popup was seen, not from the old anchor.
    mgr.retarget_drift(1, tween_100ms(), origin_now());
    let drift = mgr.frames.get(&1).unwrap().drift.as_ref().unwrap();
    assert!(
        (drift.from_x - 140.0).abs() < 1e-4 && (drift.from_y - 90.0).abs() < 1e-4,
        "the retargeted drift must depart from the last drawn position"
    );
    // The last-drawn state resets: the new curve has not been painted yet.
    assert!(drift.last_drawn.is_none());
}

#[test]
fn drift_retarget_without_a_drawn_position_fails_cleanly() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 240.0, 180.0, 100.0, 100.0, 0));
    // No drift began (the movement slot was off at its anchor): a retarget
    // has nothing to carry on from and must refuse.
    assert!(!mgr.retarget_drift(1, tween_100ms(), origin_now()));
    assert!(mgr.frames.get(&1).unwrap().drift.is_none());
}

#[test]
fn finished_drifts_clear_so_the_retained_path_returns() {
    let mut mgr = make_manager();
    mgr.update_frame(make_child_buf(1, 240.0, 180.0, 100.0, 100.0, 0));
    let origin = origin_now();
    mgr.begin_drift(1, 40.0, 0.0, tween_100ms(), origin);
    assert!(mgr.has_animation_activity());

    // Mid-drift the drift pins full renders.
    assert!(mgr.clear_finished_drifts(sample_at(origin, 10)) == false);
    assert!(mgr.has_animation_activity());

    // Past the tween the drift clears, releasing the retained path.
    assert!(mgr.clear_finished_drifts(sample_at(origin, 200)));
    assert!(!mgr.has_animation_activity());
    assert!(mgr.frames.get(&1).unwrap().drift.is_none());
}
