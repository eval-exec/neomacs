use super::*;

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn item(label: &str, enabled: bool, depth: u32) -> PopupMenuItem {
    PopupMenuItem {
        label: label.to_string(),
        shortcut: String::new(),
        enabled,
        separator: false,
        submenu: false,
        depth,
    }
}

fn separator(depth: u32) -> PopupMenuItem {
    PopupMenuItem {
        label: String::new(),
        shortcut: String::new(),
        enabled: false,
        separator: true,
        submenu: false,
        depth,
    }
}

fn submenu_item(label: &str, depth: u32) -> PopupMenuItem {
    PopupMenuItem {
        label: label.to_string(),
        shortcut: String::new(),
        enabled: true,
        separator: false,
        submenu: true,
        depth,
    }
}

fn item_with_shortcut(label: &str, shortcut: &str, depth: u32) -> PopupMenuItem {
    PopupMenuItem {
        label: label.to_string(),
        shortcut: shortcut.to_string(),
        enabled: true,
        separator: false,
        submenu: false,
        depth,
    }
}

/// Standard font metrics used in most tests.
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 18.0;
const CHAR_WIDTH: f32 = FONT_SIZE * 0.6;

/// Convenience for building a simple top-level menu.
fn simple_menu(items: Vec<PopupMenuItem>) -> PopupMenuState {
    PopupMenuState::new(100.0, 50.0, items, None, FONT_SIZE, LINE_HEIGHT, CHAR_WIDTH)
}

// -----------------------------------------------------------------------
// 1. MenuPanel layout calculations
// -----------------------------------------------------------------------

#[test]
fn layout_panel_bounds_position() {
    let items = vec![item("Open", true, 0), item("Save", true, 0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = PopupMenuState::layout_panel(
        100.0,
        200.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(panel.x, 100.0);
    assert_eq!(panel.y, 200.0);
    assert_eq!(panel.bounds.0, 100.0);
    assert_eq!(panel.bounds.1, 200.0);
}

#[test]
fn popup_state_resolves_semantic_placement_after_measuring_panel() {
    let state = PopupMenuState::new_placed(
        neomacs_display_protocol::PopupPlacement::new(
            neomacs_display_protocol::Rect::new(180.0, 90.0, 16.0, 10.0),
            neomacs_display_protocol::PopupPreferredSide::Below,
            neomacs_display_protocol::Point::ZERO,
            neomacs_display_protocol::PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
        ),
        neomacs_display_protocol::Rect::new(0.0, 0.0, 200.0, 120.0),
        vec![item("Open", true, 0)],
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );

    assert_eq!(state.root_panel.bounds.0, 46.0);
    assert!(state.root_panel.bounds.1 < 90.0);
}

#[test]
fn layout_panel_hover_starts_at_minus_one() {
    let items = vec![item("Open", true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(panel.hover_index, -1);
}

#[test]
fn layout_panel_height_with_items() {
    let padding = 4.0_f32;
    let item_height = LINE_HEIGHT + 3.0; // 21.0
    let items = vec![item("A", true, 0), item("B", true, 0), item("C", true, 0)];
    let indices: Vec<usize> = vec![0, 1, 2];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let expected_h = padding + 3.0 * item_height + padding;
    assert!(
        (panel.bounds.3 - expected_h).abs() < 0.01,
        "height was {} expected {}",
        panel.bounds.3,
        expected_h
    );
}

#[test]
fn layout_panel_height_with_separator() {
    let padding = 4.0_f32;
    let item_height = LINE_HEIGHT + 3.0;
    let separator_height = 8.0_f32;
    let items = vec![item("A", true, 0), separator(0), item("B", true, 0)];
    let indices: Vec<usize> = vec![0, 1, 2];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let expected_h = padding + item_height + separator_height + item_height + padding;
    assert!((panel.bounds.3 - expected_h).abs() < 0.01);
}

#[test]
fn layout_panel_height_with_title() {
    let padding = 4.0_f32;
    let item_height = LINE_HEIGHT + 3.0;
    let separator_height = 8.0_f32;
    let title_height = item_height + separator_height;
    let items = vec![item("A", true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        Some("My Menu"),
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let expected_h = padding + title_height + item_height + padding;
    assert!((panel.bounds.3 - expected_h).abs() < 0.01);
}

#[test]
fn layout_panel_minimum_width() {
    // Very short label should still get at least 150px width.
    let items = vec![item("X", true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(panel.bounds.2 >= 150.0, "width was {}", panel.bounds.2);
}

#[test]
fn layout_panel_width_grows_with_label() {
    let long_label = "A".repeat(100);
    let items = vec![item(&long_label, true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let char_width = FONT_SIZE * 0.6;
    let padding = 4.0_f32;
    let expected_w = 100.0 * char_width + padding * 4.0;
    assert!(
        (panel.bounds.2 - expected_w).abs() < 0.01,
        "width was {} expected {}",
        panel.bounds.2,
        expected_w
    );
}

#[test]
fn layout_panel_width_accounts_for_shortcut() {
    let items = vec![item_with_shortcut("Save", "C-x C-s", 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // label(4) + shortcut(7) + 4 extra = 15 chars
    let char_width = FONT_SIZE * 0.6;
    let padding = 4.0_f32;
    let expected_w = (15.0 * char_width + padding * 4.0).max(150.0);
    assert!(
        (panel.bounds.2 - expected_w).abs() < 0.01,
        "width was {} expected {}",
        panel.bounds.2,
        expected_w
    );
}

#[test]
fn layout_panel_width_accounts_for_submenu_arrow() {
    let items = vec![submenu_item("Submenu", 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // label(7) + arrow(3) = 10 chars
    let char_width = FONT_SIZE * 0.6;
    let padding = 4.0_f32;
    let expected_w = (10.0 * char_width + padding * 4.0).max(150.0);
    assert!((panel.bounds.2 - expected_w).abs() < 0.01);
}

#[test]
fn layout_panel_width_uses_title_len_if_longer() {
    let title = "A Very Long Menu Title That Exceeds All Labels";
    let items = vec![item("X", true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        Some(title),
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let char_width = FONT_SIZE * 0.6;
    let padding = 4.0_f32;
    // label len = 1, title len = 46 -> title wins
    let expected_w = (title.len() as f32 * char_width + padding * 4.0).max(150.0);
    assert!((panel.bounds.2 - expected_w).abs() < 0.01);
}

#[test]
fn layout_panel_item_offsets_monotonic() {
    let items = vec![
        item("A", true, 0),
        separator(0),
        item("B", true, 0),
        item("C", true, 0),
    ];
    let indices: Vec<usize> = vec![0, 1, 2, 3];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(panel.item_offsets.len(), 4);
    for i in 1..panel.item_offsets.len() {
        assert!(
            panel.item_offsets[i] > panel.item_offsets[i - 1],
            "offset[{}]={} should be > offset[{}]={}",
            i,
            panel.item_offsets[i],
            i - 1,
            panel.item_offsets[i - 1]
        );
    }
}

#[test]
fn layout_panel_empty_indices() {
    let items = vec![item("A", true, 0)];
    let indices: Vec<usize> = vec![];
    let panel = PopupMenuState::layout_panel(
        10.0,
        20.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(panel.item_offsets.len(), 0);
    assert_eq!(panel.item_indices.len(), 0);
    let padding = 4.0_f32;
    // Height = padding top + padding bottom, no items
    assert!((panel.bounds.3 - 2.0 * padding).abs() < 0.01);
}

#[test]
fn layout_panel_item_height_matches() {
    let items = vec![item("A", true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!((panel.item_height - (LINE_HEIGHT + 3.0)).abs() < 0.01);
}

// -----------------------------------------------------------------------
// 2. PopupMenuState construction and defaults
// -----------------------------------------------------------------------

#[test]
fn new_state_defaults() {
    let state = simple_menu(vec![item("A", true, 0)]);
    assert!(state.face_fg.is_none());
    assert!(state.face_bg.is_none());
    assert!(state.title.is_none());
    assert!(state.submenu_panels.is_empty());
    assert_eq!(state.font_size, FONT_SIZE);
    assert_eq!(state.line_height, LINE_HEIGHT);
}

#[test]
fn new_state_filters_root_items() {
    // Items at depth 0 only go to root panel.
    let items = vec![
        item("Root1", true, 0),
        submenu_item("Sub", 0),
        item("Child1", true, 1),
        item("Child2", true, 1),
        item("Root2", true, 0),
    ];
    let state = simple_menu(items);
    // Root panel should have indices [0, 1, 4] (depth==0)
    assert_eq!(state.root_panel.item_indices, vec![0, 1, 4]);
}

#[test]
fn new_state_with_title() {
    let state = PopupMenuState::new(
        10.0,
        20.0,
        vec![item("A", true, 0)],
        Some("Title".to_string()),
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(state.title.as_deref(), Some("Title"));
}

#[test]
fn active_panel_returns_root_when_no_submenus() {
    let state = simple_menu(vec![item("A", true, 0)]);
    let ap = state.active_panel();
    // Should be the root panel (same pointer / same data).
    assert_eq!(ap.item_indices, state.root_panel.item_indices);
}

#[test]
fn panels_returns_only_root_when_no_submenus() {
    let state = simple_menu(vec![item("A", true, 0)]);
    let panels = state.panels();
    assert_eq!(panels.len(), 1);
}

// -----------------------------------------------------------------------
// 3. move_hover
// -----------------------------------------------------------------------

#[test]
fn move_hover_down_from_none() {
    let mut state = simple_menu(vec![
        item("A", true, 0),
        item("B", true, 0),
        item("C", true, 0),
    ]);
    // hover starts at -1, moving down (+1) should go to index 0
    assert!(state.move_hover(1));
    assert_eq!(state.root_panel.hover_index, 0);
}

#[test]
fn move_hover_down_sequential() {
    let mut state = simple_menu(vec![
        item("A", true, 0),
        item("B", true, 0),
        item("C", true, 0),
    ]);
    state.move_hover(1); // -> 0
    assert!(state.move_hover(1)); // -> 1
    assert_eq!(state.root_panel.hover_index, 1);
    assert!(state.move_hover(1)); // -> 2
    assert_eq!(state.root_panel.hover_index, 2);
}

#[test]
fn move_hover_wraps_around_bottom() {
    let mut state = simple_menu(vec![item("A", true, 0), item("B", true, 0)]);
    state.root_panel.hover_index = 1;
    // Moving down from last should wrap to 0.
    assert!(state.move_hover(1));
    assert_eq!(state.root_panel.hover_index, 0);
}

#[test]
fn move_hover_wraps_around_top() {
    let mut state = simple_menu(vec![item("A", true, 0), item("B", true, 0)]);
    state.root_panel.hover_index = 0;
    // Moving up from first should wrap to last.
    assert!(state.move_hover(-1));
    assert_eq!(state.root_panel.hover_index, 1);
}

#[test]
fn move_hover_skips_separators() {
    let mut state = simple_menu(vec![item("A", true, 0), separator(0), item("B", true, 0)]);
    state.root_panel.hover_index = 0;
    assert!(state.move_hover(1));
    // Should skip separator at index 1 and land on index 2.
    assert_eq!(state.root_panel.hover_index, 2);
}

#[test]
fn move_hover_skips_disabled_items() {
    let mut state = simple_menu(vec![
        item("A", true, 0),
        item("B", false, 0), // disabled
        item("C", true, 0),
    ]);
    state.root_panel.hover_index = 0;
    assert!(state.move_hover(1));
    // Should skip disabled item at index 1.
    assert_eq!(state.root_panel.hover_index, 2);
}

#[test]
fn move_hover_empty_panel() {
    // No depth-0 items means root panel is empty.
    let mut state = simple_menu(vec![item("Child", true, 1)]);
    assert!(!state.move_hover(1));
    assert!(!state.move_hover(-1));
}

#[test]
fn move_hover_all_disabled_returns_false() {
    let mut state = simple_menu(vec![item("A", false, 0), item("B", false, 0)]);
    assert!(!state.move_hover(1));
}

#[test]
fn move_hover_all_separators_returns_false() {
    let mut state = simple_menu(vec![separator(0), separator(0)]);
    assert!(!state.move_hover(1));
}

#[test]
fn move_hover_single_enabled_item_from_none() {
    let mut state = simple_menu(vec![item("Only", true, 0)]);
    // From -1 moving down lands on 0.
    assert!(state.move_hover(1));
    assert_eq!(state.root_panel.hover_index, 0);
}

#[test]
fn move_hover_single_enabled_item_already_there() {
    let mut state = simple_menu(vec![item("Only", true, 0)]);
    state.root_panel.hover_index = 0;
    // Moving from the only item should not change (wraps back to same).
    assert!(!state.move_hover(1));
    assert_eq!(state.root_panel.hover_index, 0);
}

#[test]
fn move_hover_up_from_none() {
    let mut state = simple_menu(vec![
        item("A", true, 0),
        item("B", true, 0),
        item("C", true, 0),
    ]);
    // hover = -1, direction = -1 => idx = -2 => wraps to len-1 = 2
    assert!(state.move_hover(-1));
    assert_eq!(state.root_panel.hover_index, 2);
}

// -----------------------------------------------------------------------
// 4. Submenu open/close
// -----------------------------------------------------------------------

fn menu_with_submenu() -> PopupMenuState {
    simple_menu(vec![
        item("Open", true, 0),      // 0
        submenu_item("Recent", 0),  // 1
        item("File1.txt", true, 1), // 2
        item("File2.txt", true, 1), // 3
        item("File3.txt", true, 1), // 4
        item("Quit", true, 0),      // 5
    ])
}

#[test]
fn open_submenu_returns_false_when_no_hover() {
    let mut state = menu_with_submenu();
    // hover_index = -1
    assert!(!state.open_submenu());
    assert!(state.submenu_panels.is_empty());
}

#[test]
fn open_submenu_returns_false_for_non_submenu_item() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 0; // "Open" is not a submenu
    assert!(!state.open_submenu());
    assert!(state.submenu_panels.is_empty());
}

#[test]
fn open_submenu_succeeds_for_submenu_item() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1; // "Recent" is a submenu
    assert!(state.open_submenu());
    assert_eq!(state.submenu_panels.len(), 1);
    // Child items should be indices [2, 3, 4] (depth 1)
    assert_eq!(state.submenu_panels[0].item_indices, vec![2, 3, 4]);
}

#[test]
fn open_submenu_does_not_duplicate_already_open_keyboard_submenu() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;

    assert!(state.open_submenu());
    assert!(!state.open_submenu());
    assert_eq!(state.submenu_panels.len(), 1);
    assert_eq!(state.submenu_panels[0].item_indices, vec![2, 3, 4]);
}

#[test]
fn open_submenu_position_is_right_of_parent() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();
    let (px, _py, pw, _ph) = state.root_panel.bounds;
    let sub_x = state.submenu_panels[0].bounds.0;
    // Should be positioned at parent_x + parent_w - 2.0
    assert!((sub_x - (px + pw - 2.0)).abs() < 0.01);
}

#[test]
fn open_submenu_y_aligns_with_hovered_item() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    let expected_y = state.root_panel.bounds.1 + state.root_panel.item_offsets[1];
    state.open_submenu();
    assert!((state.submenu_panels[0].bounds.1 - expected_y).abs() < 0.01);
}

#[test]
fn active_panel_is_submenu_after_open() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();
    let ap = state.active_panel();
    // Active panel should be the submenu, not root.
    assert_eq!(ap.item_indices, vec![2, 3, 4]);
}

#[test]
fn close_submenu_returns_true_when_open() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();
    assert!(state.close_submenu());
    assert!(state.submenu_panels.is_empty());
}

#[test]
fn close_submenu_returns_false_when_none() {
    let mut state = menu_with_submenu();
    assert!(!state.close_submenu());
}

#[test]
fn panels_includes_submenus() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();
    let panels = state.panels();
    assert_eq!(panels.len(), 2);
}

#[test]
fn open_submenu_with_empty_children_returns_false() {
    // A submenu item with no children at depth+1
    let mut state = simple_menu(vec![
        submenu_item("Empty Sub", 0), // 0 - submenu flag but no depth-1 children
        item("Next", true, 0),        // 1
    ]);
    state.root_panel.hover_index = 0;
    assert!(!state.open_submenu());
    assert!(state.submenu_panels.is_empty());
}

#[test]
fn nested_submenus() {
    // depth 0 -> depth 1 -> depth 2
    let mut state = simple_menu(vec![
        submenu_item("Level0", 0), // 0
        submenu_item("Level1", 1), // 1
        item("Level2-A", true, 2), // 2
        item("Level2-B", true, 2), // 3
    ]);
    // Open first submenu
    state.root_panel.hover_index = 0;
    assert!(state.open_submenu());
    assert_eq!(state.submenu_panels.len(), 1);
    assert_eq!(state.submenu_panels[0].item_indices, vec![1]);

    // Hover on the child submenu item and open second level
    state.submenu_panels[0].hover_index = 0;
    assert!(state.open_submenu());
    assert_eq!(state.submenu_panels.len(), 2);
    assert_eq!(state.submenu_panels[1].item_indices, vec![2, 3]);

    // Active panel should be the deepest
    assert_eq!(state.active_panel().item_indices, vec![2, 3]);

    // Close one
    assert!(state.close_submenu());
    assert_eq!(state.submenu_panels.len(), 1);
    assert_eq!(state.active_panel().item_indices, vec![1]);
}

// -----------------------------------------------------------------------
// 5. Hit testing
// -----------------------------------------------------------------------

#[test]
fn hit_test_panel_miss_outside() {
    let items = vec![item("A", true, 0), item("B", true, 0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = PopupMenuState::layout_panel(
        100.0,
        100.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // Way outside
    assert_eq!(PopupMenuState::hit_test_panel(&panel, &items, 0.0, 0.0), -1);
    // Left of panel
    assert_eq!(
        PopupMenuState::hit_test_panel(&panel, &items, 99.0, 110.0),
        -1
    );
    // Above panel
    assert_eq!(
        PopupMenuState::hit_test_panel(&panel, &items, 110.0, 99.0),
        -1
    );
}

#[test]
fn hit_test_panel_hit_first_item() {
    let items = vec![item("A", true, 0), item("B", true, 0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = PopupMenuState::layout_panel(
        100.0,
        100.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // First item starts at y = panel.bounds.1 + panel.item_offsets[0]
    let iy = panel.bounds.1 + panel.item_offsets[0];
    let mx = panel.bounds.0 + 10.0;
    let my = iy + 1.0; // Just inside item
    assert_eq!(PopupMenuState::hit_test_panel(&panel, &items, mx, my), 0);
}

#[test]
fn hit_test_panel_hit_second_item() {
    let items = vec![item("A", true, 0), item("B", true, 0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = PopupMenuState::layout_panel(
        100.0,
        100.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let iy = panel.bounds.1 + panel.item_offsets[1];
    let mx = panel.bounds.0 + 10.0;
    let my = iy + 1.0;
    assert_eq!(PopupMenuState::hit_test_panel(&panel, &items, mx, my), 1);
}

#[test]
fn hit_test_panel_skips_separator() {
    let items = vec![item("A", true, 0), separator(0), item("B", true, 0)];
    let indices: Vec<usize> = vec![0, 1, 2];
    let panel = PopupMenuState::layout_panel(
        100.0,
        100.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // Click on the separator area should return -1 if no non-separator item
    // is at that y position.
    let sep_y = panel.bounds.1 + panel.item_offsets[1] + 2.0;
    let mx = panel.bounds.0 + 10.0;
    assert_eq!(
        PopupMenuState::hit_test_panel(&panel, &items, mx, sep_y),
        -1
    );
}

#[test]
fn hit_test_all_miss() {
    let state = simple_menu(vec![item("A", true, 0)]);
    assert_eq!(state.hit_test_all(0.0, 0.0), (-1, -1));
}

#[test]
fn hit_test_all_inside_root_bounds_but_no_item() {
    let state = simple_menu(vec![item("A", true, 0)]);
    // Click in the padding area (top of panel, above first item offset).
    let mx = state.root_panel.bounds.0 + 5.0;
    let my = state.root_panel.bounds.1 + 1.0; // Inside bounds but above first item
    let (depth, idx) = state.hit_test_all(mx, my);
    assert_eq!(depth, 0);
    assert_eq!(idx, -1); // inside bounds but not on an item
}

#[test]
fn hit_test_all_hits_root_item() {
    let state = simple_menu(vec![item("A", true, 0), item("B", true, 0)]);
    let iy = state.root_panel.bounds.1 + state.root_panel.item_offsets[0];
    let mx = state.root_panel.bounds.0 + 10.0;
    let my = iy + 2.0;
    let (depth, idx) = state.hit_test_all(mx, my);
    assert_eq!(depth, 0);
    assert_eq!(idx, 0);
}

#[test]
fn hit_test_all_submenu_takes_priority() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();

    // Click on the submenu panel's first item
    let sub = &state.submenu_panels[0];
    let iy = sub.bounds.1 + sub.item_offsets[0];
    let mx = sub.bounds.0 + 10.0;
    let my = iy + 2.0;
    let (depth, idx) = state.hit_test_all(mx, my);
    assert_eq!(depth, 1);
    assert_eq!(idx, 0);
}

#[test]
fn hit_test_returns_global_index_for_enabled_leaf() {
    let state = simple_menu(vec![item("A", true, 0), item("B", true, 0)]);
    let iy = state.root_panel.bounds.1 + state.root_panel.item_offsets[1];
    let mx = state.root_panel.bounds.0 + 10.0;
    let my = iy + 2.0;
    let global = state.hit_test(mx, my);
    assert_eq!(global, 1); // global index of "B"
}

#[test]
fn hit_test_returns_minus_one_for_submenu_item() {
    // Clicking on a submenu header should not select it (it's a submenu, not a leaf).
    let state = simple_menu(vec![submenu_item("Sub", 0), item("Child", true, 1)]);
    let iy = state.root_panel.bounds.1 + state.root_panel.item_offsets[0];
    let mx = state.root_panel.bounds.0 + 10.0;
    let my = iy + 2.0;
    let global = state.hit_test(mx, my);
    assert_eq!(global, -1);
}

#[test]
fn hit_test_returns_minus_one_for_disabled_item() {
    let state = simple_menu(vec![item("Disabled", false, 0)]);
    let iy = state.root_panel.bounds.1 + state.root_panel.item_offsets[0];
    let mx = state.root_panel.bounds.0 + 10.0;
    let my = iy + 2.0;
    let global = state.hit_test(mx, my);
    assert_eq!(global, -1);
}

#[test]
fn hit_test_miss_returns_minus_one() {
    let state = simple_menu(vec![item("A", true, 0)]);
    assert_eq!(state.hit_test(0.0, 0.0), -1);
}

// -----------------------------------------------------------------------
// 6. TooltipState
// -----------------------------------------------------------------------

#[test]
fn tooltip_basic_positioning() {
    let tt = TooltipState::new(
        100.0,
        100.0,
        "Hello",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // Should be offset +10, +20 from cursor
    assert!((tt.x - 110.0).abs() < 0.01);
    assert!((tt.y - 120.0).abs() < 0.01);
}

#[test]
fn tooltip_bounds_match_position() {
    let tt = TooltipState::new(
        100.0,
        100.0,
        "Hello",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!((tt.bounds.0 - tt.x).abs() < 0.01);
    assert!((tt.bounds.1 - tt.y).abs() < 0.01);
}

#[test]
fn tooltip_width_from_text() {
    let text = "Hello World"; // 11 chars
    let tt = TooltipState::new(
        0.0,
        0.0,
        text,
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let padding = 6.0_f32;
    let char_width = FONT_SIZE * 0.6;
    let expected_w = (11.0 * char_width + padding * 2.0).max(40.0);
    assert!((tt.bounds.2 - expected_w).abs() < 0.01);
}

#[test]
fn tooltip_minimum_width() {
    let tt = TooltipState::new(
        0.0,
        0.0,
        "X",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(tt.bounds.2 >= 40.0);
}

#[test]
fn tooltip_multiline_height() {
    let text = "Line1\nLine2\nLine3";
    let tt = TooltipState::new(
        0.0,
        0.0,
        text,
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let padding = 6.0_f32;
    let expected_h = 3.0 * LINE_HEIGHT + padding * 2.0;
    assert!((tt.bounds.3 - expected_h).abs() < 0.01);
    assert_eq!(tt.lines.len(), 3);
}

#[test]
fn tooltip_clamps_right_edge() {
    // Cursor near right edge of screen
    let screen_w = 500.0;
    let tt = TooltipState::new(
        490.0,
        100.0,
        "A long tooltip text",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        screen_w,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // x + width should not exceed screen_w
    assert!(
        tt.x + tt.bounds.2 <= screen_w,
        "tooltip right edge {} exceeds screen width {}",
        tt.x + tt.bounds.2,
        screen_w
    );
}

#[test]
fn tooltip_flips_above_when_near_bottom() {
    let screen_h = 200.0;
    let cursor_y = 190.0;
    let tt = TooltipState::new(
        100.0,
        cursor_y,
        "Tooltip text",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        screen_h,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // When tooltip doesn't fit below, it flips above: ty = y - h - 5.0
    assert!(
        tt.y < cursor_y,
        "tooltip y ({}) should be above cursor y ({})",
        tt.y,
        cursor_y
    );
}

#[test]
fn tooltip_clamps_negative_x() {
    let tt = TooltipState::new(
        -50.0,
        100.0,
        "A long text for width",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // After the right-edge clamping, if tx is still negative, it's clamped to 0.
    // -50 + 10 = -40, which might be further adjusted. Should be >= 0.
    assert!(tt.x >= 0.0, "tooltip x should be >= 0 but was {}", tt.x);
}

#[test]
fn tooltip_clamps_negative_y() {
    // Cursor at top of tiny screen so flipping above goes negative.
    let tt = TooltipState::new(
        100.0,
        5.0,
        "Some text\nMore text\nEven more",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        30.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(tt.y >= 0.0, "tooltip y should be >= 0 but was {}", tt.y);
}

#[test]
fn tooltip_preserves_colors() {
    let fg = (0.1, 0.2, 0.3);
    let bg = (0.4, 0.5, 0.6);
    let tt = TooltipState::new(
        0.0,
        0.0,
        "test",
        fg,
        bg,
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(tt.fg, fg);
    assert_eq!(tt.bg, bg);
}

#[test]
fn tooltip_empty_text() {
    // Empty string produces no lines from .lines(), so max_line_len falls back to 1.
    let tt = TooltipState::new(
        0.0,
        0.0,
        "",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(tt.lines.is_empty());
    // Height = 0 lines * line_height + 2*padding = 12.0
    let padding = 6.0_f32;
    assert!((tt.bounds.3 - 2.0 * padding).abs() < 0.01);
    // Width should be at least min (40.0)
    assert!(tt.bounds.2 >= 40.0);
}

// -----------------------------------------------------------------------
// 7. Complex interaction scenarios
// -----------------------------------------------------------------------

#[test]
fn move_hover_in_submenu() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1; // "Recent" submenu
    state.open_submenu();

    // Hover should start at -1 in submenu
    assert_eq!(state.active_panel().hover_index, -1);

    // Move down in submenu
    assert!(state.move_hover(1));
    assert_eq!(state.active_panel().hover_index, 0); // "File1.txt"
    assert!(state.move_hover(1));
    assert_eq!(state.active_panel().hover_index, 1); // "File2.txt"
    assert!(state.move_hover(1));
    assert_eq!(state.active_panel().hover_index, 2); // "File3.txt"
    // Wraps back
    assert!(state.move_hover(1));
    assert_eq!(state.active_panel().hover_index, 0);
}

#[test]
fn close_submenu_restores_root_as_active() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();
    state.close_submenu();

    // Active panel should be root again
    let ap = state.active_panel();
    assert_eq!(ap.item_indices, state.root_panel.item_indices);
    // Root hover should still be on the submenu item
    assert_eq!(state.root_panel.hover_index, 1);
}

#[test]
fn hit_test_in_submenu_returns_global_index() {
    let mut state = menu_with_submenu();
    state.root_panel.hover_index = 1;
    state.open_submenu();

    // Hit the first child item in the submenu
    let sub = &state.submenu_panels[0];
    let iy = sub.bounds.1 + sub.item_offsets[0];
    let mx = sub.bounds.0 + 10.0;
    let my = iy + 2.0;
    let global = state.hit_test(mx, my);
    // global index of "File1.txt" is 2
    assert_eq!(global, 2);
}

#[test]
fn layout_panel_no_non_separator_items_uses_default_label_len() {
    // Panel with only separators: max_label_len falls back to unwrap_or(10)
    let items = vec![separator(0), separator(0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = PopupMenuState::layout_panel(
        0.0,
        0.0,
        &items,
        &indices,
        None,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let char_width = FONT_SIZE * 0.6;
    let padding = 4.0_f32;
    let expected_w = (10.0 * char_width + padding * 4.0).max(150.0);
    assert!(
        (panel.bounds.2 - expected_w).abs() < 0.01,
        "width was {} expected {}",
        panel.bounds.2,
        expected_w
    );
}

#[test]
fn open_submenu_skips_grandchildren_in_indices() {
    // Ensure only direct children (depth == parent+1) are collected, not grandchildren.
    let mut state = simple_menu(vec![
        submenu_item("Parent", 0),     // 0
        item("Child1", true, 1),       // 1
        submenu_item("Child2-Sub", 1), // 2
        item("Grandchild", true, 2),   // 3
        item("Child3", true, 1),       // 4
    ]);
    state.root_panel.hover_index = 0;
    assert!(state.open_submenu());
    // Only depth-1 items should be in submenu: [1, 2, 4]
    assert_eq!(state.submenu_panels[0].item_indices, vec![1, 2, 4]);
}

#[test]
fn update_hover_at_keeps_submenu_open_when_mouse_stays_on_parent() {
    let mut state = menu_with_submenu();
    let root = &state.root_panel;
    let mx = root.bounds.0 + 10.0;
    let my = root.bounds.1 + root.item_offsets[1] + 2.0;

    assert!(state.update_hover_at(mx, my));
    assert_eq!(state.root_panel.hover_index, 1);
    assert_eq!(state.submenu_panels.len(), 1);

    assert!(!state.update_hover_at(mx, my));
    assert_eq!(state.root_panel.hover_index, 1);
    assert_eq!(state.submenu_panels.len(), 1);
}

#[test]
fn update_hover_at_keeps_parent_submenu_open_when_mouse_enters_child_panel() {
    let mut state = menu_with_submenu();
    let root = &state.root_panel;
    let parent_x = root.bounds.0 + 10.0;
    let parent_y = root.bounds.1 + root.item_offsets[1] + 2.0;
    state.update_hover_at(parent_x, parent_y);

    let sub = &state.submenu_panels[0];
    let child_x = sub.bounds.0 + 10.0;
    let child_y = sub.bounds.1 + sub.item_offsets[0] + 2.0;

    assert!(state.update_hover_at(child_x, child_y));
    assert_eq!(state.root_panel.hover_index, 1);
    assert_eq!(state.submenu_panels.len(), 1);
    assert_eq!(state.submenu_panels[0].hover_index, 0);
}
