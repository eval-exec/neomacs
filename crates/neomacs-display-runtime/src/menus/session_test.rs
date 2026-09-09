use super::*;

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn item(label: &str, enabled: bool, depth: u32) -> PopupMenuItem {
    PopupMenuItem {
        help: None,
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
        help: None,
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
        help: None,
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
        help: None,
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
fn simple_menu(items: Vec<PopupMenuItem>) -> MenuSession {
    MenuSession::new(100.0, 50.0, items, None, FONT_SIZE, LINE_HEIGHT, CHAR_WIDTH)
}

// -----------------------------------------------------------------------
// 1. MenuPanel layout calculations
// -----------------------------------------------------------------------

#[test]
fn layout_panel_bounds_position() {
    let items = vec![item("Open", true, 0), item("Save", true, 0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = crate::menus::layout::measure_panel(
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
fn layout_panel_hover_starts_at_minus_one() {
    let items = vec![item("Open", true, 0)];
    let indices: Vec<usize> = vec![0];
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
    let panel = crate::menus::layout::measure_panel(
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
// 2. MenuSession construction and defaults
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
    let state = MenuSession::new(
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

fn menu_with_submenu() -> MenuSession {
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
fn layout_panel_no_non_separator_items_uses_default_label_len() {
    // Panel with only separators: max_label_len falls back to unwrap_or(10)
    let items = vec![separator(0), separator(0)];
    let indices: Vec<usize> = vec![0, 1];
    let panel = crate::menus::layout::measure_panel(
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
fn native_panel_hit_testing_does_not_depend_on_compositor_placement() {
    let mut menu = menu_with_submenu();
    // Coordinates are local even though this fixture's root origin is (100,50).
    menu.hover_panel(0, 10.0, 30.0);
    assert_eq!(menu.submenu_panels.len(), 1);
    menu.hover_panel(1, 10.0, 5.0);
    assert_eq!(menu.activate_panel(1), Some(2));
}

#[test]
fn disabled_native_item_cannot_be_activated() {
    let mut menu = simple_menu(vec![item("Disabled", false, 0)]);
    menu.hover_panel(0, 10.0, 5.0);
    assert_eq!(menu.activate_panel(0), None);
}

#[test]
fn native_parent_hover_preserves_its_open_child() {
    let mut menu = menu_with_submenu();
    menu.hover_panel(0, 10.0, 30.0);
    menu.hover_panel(1, 10.0, 5.0);
    menu.hover_panel(0, 10.0, 30.0);
    assert_eq!(menu.submenu_panels.len(), 1);
    assert_eq!(menu.activate_panel(1), Some(2));
}
#[test]
fn pending_result_keeps_original_token_when_another_menu_opens() {
    use neomacs_display_protocol::menu::MenuToken;
    let a = MenuToken {
        session: 100,
        revision: 1,
    };
    let b = MenuToken {
        session: 101,
        revision: 1,
    };
    let mut lifetime = MenuLifetime::default();
    assert!(lifetime.show(a));
    lifetime.finish(-1);
    assert!(lifetime.show(b));
    assert_eq!(lifetime.take_result().unwrap().token, a);
    lifetime.finish(2);
    let result = lifetime.take_result().unwrap();
    assert_eq!(result.token, b);
    assert_eq!(result.index(), 2);
}

#[test]
fn stale_show_and_hide_do_not_replace_current_menu() {
    use neomacs_display_protocol::menu::MenuToken;
    let a = MenuToken {
        session: 100,
        revision: 1,
    };
    let b = MenuToken {
        session: 101,
        revision: 1,
    };
    let mut lifetime = MenuLifetime::default();
    assert!(lifetime.show(b));
    assert!(!lifetime.show(a));
    assert!(!lifetime.hide(a));
    lifetime.finish(3);
    assert_eq!(lifetime.take_result().unwrap().token, b);
}

#[test]
fn newer_revision_can_win_a_race_with_previous_revision_dismissal() {
    use neomacs_display_protocol::menu::MenuToken;
    let a = MenuToken {
        session: 100,
        revision: 1,
    };
    let b = MenuToken {
        session: 100,
        revision: 2,
    };
    let mut lifetime = MenuLifetime::default();
    assert!(lifetime.show(a));
    lifetime.finish(-1);
    assert!(!lifetime.show(a));
    assert!(lifetime.show(b));
    assert!(!lifetime.hide(a));
    lifetime.finish(4);
    assert_eq!(lifetime.take_result().unwrap().token, a);
    assert_eq!(lifetime.take_result().unwrap().token, b);
}
#[test]
fn hide_before_show_tombstones_the_exact_snapshot() {
    let token = neomacs_display_protocol::menu::MenuToken {
        session: 100,
        revision: 1,
    };
    let mut lifetime = MenuLifetime::default();
    assert!(lifetime.hide(token));
    assert!(!lifetime.show(token));
}
