use super::body::WindowBodyCellSize;
use super::*;

fn split_tree(direction: SplitDirection) -> (WindowTree, WindowId, WindowId) {
    let mut frames = FrameManager::new();
    let frame = frames.create_frame("resize", 800, 600, BufferId(1));
    let first = frames.get(frame).expect("fixture frame").tree().root_id();
    let second = frames
        .split_window(
            frame,
            first,
            direction,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("fixture split");
    (
        frames.get(frame).expect("fixture frame").tree().clone(),
        first,
        second,
    )
}

#[test]
fn resize_check_requires_complete_staging_and_existing_children() {
    let (mut tree, first, second) = split_tree(SplitDirection::Vertical);
    let root = tree.root_id();
    let cells = WindowBodyCellSize::new(8.0, 16.0);
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    tree.root_mut().set_new_pixel(Some(64));
    tree.find_mut(first).expect("first").set_new_pixel(Some(32));
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    tree.find_mut(second)
        .expect("second")
        .set_new_pixel(Some(32));
    assert!(window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    tree.remove(second);
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    assert!(!window_resize_check(
        &tree,
        second,
        SplitDirection::Vertical,
        cells
    ));
}

#[test]
fn resize_check_requires_iso_sum_and_ortho_equality() {
    let (mut tree, first, second) = split_tree(SplitDirection::Vertical);
    let root = tree.root_id();
    let cells = WindowBodyCellSize::new(8.0, 16.0);
    tree.root_mut().set_new_pixel(Some(80));
    tree.find_mut(first).expect("first").set_new_pixel(Some(32));
    tree.find_mut(second)
        .expect("second")
        .set_new_pixel(Some(48));
    assert!(window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    tree.find_mut(second)
        .expect("second")
        .set_new_pixel(Some(49));
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    for node in tree.nodes_mut() {
        node.set_new_pixel(Some(80));
    }
    assert!(window_resize_check(
        &tree,
        root,
        SplitDirection::Horizontal,
        cells
    ));
    tree.find_mut(second)
        .expect("second")
        .set_new_pixel(Some(79));
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Horizontal,
        cells
    ));
}

#[test]
fn resize_check_enforces_one_line_and_two_column_leaf_minimums() {
    let mut tree = WindowTree::new(Window::new_leaf(
        WindowId(11),
        BufferId(1),
        Rect::new(0.0, 0.0, 80.0, 64.0),
    ));
    let root = tree.root_id();
    let cells = WindowBodyCellSize::new(8.0, 16.0);
    tree.root_mut().set_new_pixel(Some(15));
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Horizontal,
        cells
    ));
    tree.root_mut().set_new_pixel(Some(16));
    assert!(window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    assert!(window_resize_check(
        &tree,
        root,
        SplitDirection::Horizontal,
        cells
    ));
    tree.root_mut().set_new_pixel(Some(-1));
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
}

#[test]
fn resize_check_rejects_overfull_and_empty_combinations() {
    let (mut tree, first, second) = split_tree(SplitDirection::Horizontal);
    let root = tree.root_id();
    let cells = WindowBodyCellSize::new(8.0, 16.0);
    for node in tree.nodes_mut() {
        node.set_new_pixel(Some(i64::MAX));
    }
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Horizontal,
        cells
    ));
    // The same huge extent is valid orthogonally: no summation or overflow.
    assert!(window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    tree.set_children(root, vec![]);
    assert!(!window_resize_check(
        &tree,
        root,
        SplitDirection::Vertical,
        cells
    ));
    assert!(tree.find(first).is_some());
    assert!(tree.find(second).is_some());
}

#[test]
fn resize_pixel_commit_publishes_floored_lines_and_child_edges() {
    let (mut tree, first, second) = split_tree(SplitDirection::Vertical);
    let root = tree.root_id();
    for node in tree.nodes_mut() {
        node.set_bounds(Rect::new(13.0, 19.0, 80.0, 65.0));
        node.commit_cell_total(false, 999);
        node.set_top_line(777);
        node.set_left_col(91);
    }
    tree.root_mut().set_new_pixel(Some(65));
    tree.find_mut(first).expect("first").set_new_pixel(Some(33));
    tree.find_mut(second)
        .expect("second")
        .set_new_pixel(Some(32));
    window_resize_apply(&mut tree, root, false, 8.0, 16.0);
    let root = tree.root();
    let first = tree.find(first).expect("first");
    let second = tree.find(second).expect("second");
    assert_eq!(
        (
            root.total_lines(16.0),
            first.total_lines(16.0),
            second.total_lines(16.0)
        ),
        (4, 2, 2)
    );
    assert_eq!(root.top_line(), 777, "the caller owns the root cell origin");
    assert_eq!((first.bounds().y, second.bounds().y), (19.0, 52.0));
    assert_eq!((first.top_line(), second.top_line()), (1, 3));
    assert_eq!((first.left_col(), second.left_col()), (91, 91));
}

#[test]
fn resize_pixel_commit_publishes_floored_columns_and_child_edges() {
    let (mut tree, first, second) = split_tree(SplitDirection::Horizontal);
    let root = tree.root_id();
    for node in tree.nodes_mut() {
        node.set_bounds(Rect::new(13.0, 19.0, 35.0, 64.0));
        node.commit_cell_total(true, 999);
        node.set_left_col(777);
        node.set_top_line(91);
    }
    tree.root_mut().set_new_pixel(Some(35));
    tree.find_mut(first).expect("first").set_new_pixel(Some(17));
    tree.find_mut(second)
        .expect("second")
        .set_new_pixel(Some(18));
    window_resize_apply(&mut tree, root, true, 8.0, 16.0);
    let root = tree.root();
    let first = tree.find(first).expect("first");
    let second = tree.find(second).expect("second");
    assert_eq!(
        (
            root.total_columns(8.0),
            first.total_columns(8.0),
            second.total_columns(8.0)
        ),
        (4, 2, 2)
    );
    assert_eq!(root.left_col(), 777, "the caller owns the root cell origin");
    assert_eq!((first.bounds().x, second.bounds().x), (13.0, 30.0));
    assert_eq!((first.left_col(), second.left_col()), (1, 3));
    assert_eq!((first.top_line(), second.top_line()), (91, 91));
}

#[test]
fn resize_pixel_commit_gives_ortho_children_one_cell_origin() {
    let (mut tree, first, second) = split_tree(SplitDirection::Vertical);
    let root = tree.root_id();
    for node in tree.nodes_mut() {
        let old = *node.bounds();
        node.set_bounds(Rect::new(13.0, old.y, 35.0, old.height));
        node.set_left_col(777);
        node.set_new_pixel(Some(35));
    }
    window_resize_apply(&mut tree, root, true, 8.0, 16.0);
    for child in [first, second] {
        let node = tree.find(child).expect("child");
        assert_eq!(node.bounds().x, 13.0);
        assert_eq!(node.left_col(), 1);
        assert_eq!(node.total_columns(8.0), 4);
    }
}
