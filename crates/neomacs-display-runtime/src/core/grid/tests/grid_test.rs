use super::*;

#[test]
fn test_basic_operations() {
    let mut grid = CharacterGrid::new(10, 5);

    // Set a cell
    grid.set_cell(0, 0, "A".to_string(), None, 1);
    assert_eq!(grid.get_cell(0, 0).unwrap().text, "A");

    // Row should be dirty
    assert!(grid.is_row_dirty(0));

    // Mark clean
    grid.mark_row_clean(0);
    assert!(!grid.is_row_dirty(0));
}

#[test]
fn test_scroll() {
    let mut grid = CharacterGrid::new(10, 5);
    grid.set_cell(0, 0, "A".to_string(), None, 1);

    // Scroll down by 1
    grid.scroll(1);

    // Row 0 content should now be at row 4 (wrapped)
    // But actually the content stays in place, view shifts
    // So row 0 now shows what was at row -1 (which wraps to row 4)
}

#[test]
fn test_resize() {
    let mut grid = CharacterGrid::new(10, 5);
    grid.set_cell(0, 0, "A".to_string(), None, 1);

    grid.resize(20, 10);
    assert_eq!(grid.width, 20);
    assert_eq!(grid.height, 10);

    // Original cell should still be there
    assert_eq!(grid.get_cell(0, 0).unwrap().text, "A");
}

#[test]
fn test_update_row_ascii_width() {
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("H".to_string(), None),
        ("e".to_string(), None),
        ("l".to_string(), None),
        ("l".to_string(), None),
        ("o".to_string(), None),
    ];
    grid.update_row(0, 0, &cells);

    // Each ASCII character should have width=1
    for x in 0..5 {
        assert_eq!(grid.get_cell(x, 0).unwrap().width, 1,
            "ASCII char at column {} should have width 1", x);
    }
    // Verify text content
    assert_eq!(grid.get_cell(0, 0).unwrap().text, "H");
    assert_eq!(grid.get_cell(4, 0).unwrap().text, "o");
}

#[test]
fn test_update_row_cjk_unified_ideographs() {
    // CJK Unified Ideographs: U+4E00..U+9FFF
    // '中' = U+4E2D, '文' = U+6587
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("\u{4E2D}".to_string(), None),  // 中
        ("\u{6587}".to_string(), None),  // 文
    ];
    grid.update_row(0, 0, &cells);

    // First CJK char at column 0, width=2
    assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{4E2D}");
    assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

    // Second CJK char at column 2 (0 + width 2), width=2
    assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{6587}");
    assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);
}

#[test]
fn test_update_row_fullwidth_forms() {
    // Fullwidth Forms: U+FF01..U+FF60
    // 'Ａ' = U+FF21, 'Ｂ' = U+FF22
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("\u{FF21}".to_string(), None),  // Ａ
        ("\u{FF22}".to_string(), None),  // Ｂ
    ];
    grid.update_row(0, 0, &cells);

    assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{FF21}");
    assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

    assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{FF22}");
    assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);
}

#[test]
fn test_update_row_hangul_syllables() {
    // Hangul Syllables: U+AC00..U+D7AF
    // '한' = U+D55C, '글' = U+AE00
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("\u{D55C}".to_string(), None),  // 한
        ("\u{AE00}".to_string(), None),  // 글
    ];
    grid.update_row(0, 0, &cells);

    assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{D55C}");
    assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

    assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{AE00}");
    assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);
}

#[test]
fn test_update_row_column_advance() {
    // Verify that column positions advance by the character's width
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("\u{4E2D}".to_string(), None),  // 中 (width=2, occupies col 0-1)
        ("A".to_string(), None),          // A  (width=1, occupies col 2)
        ("\u{D55C}".to_string(), None),  // 한 (width=2, occupies col 3-4)
        ("B".to_string(), None),          // B  (width=1, occupies col 5)
    ];
    grid.update_row(0, 0, &cells);

    // '中' at column 0
    assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{4E2D}");
    assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

    // 'A' at column 2
    assert_eq!(grid.get_cell(2, 0).unwrap().text, "A");
    assert_eq!(grid.get_cell(2, 0).unwrap().width, 1);

    // '한' at column 3
    assert_eq!(grid.get_cell(3, 0).unwrap().text, "\u{D55C}");
    assert_eq!(grid.get_cell(3, 0).unwrap().width, 2);

    // 'B' at column 5
    assert_eq!(grid.get_cell(5, 0).unwrap().text, "B");
    assert_eq!(grid.get_cell(5, 0).unwrap().width, 1);
}

#[test]
fn test_update_row_mixed_ascii_and_wide() {
    // A complete mixed-content row:
    // "Hi中文ok" should lay out as:
    //   col 0: 'H' (w=1)
    //   col 1: 'i' (w=1)
    //   col 2: '中' (w=2)
    //   col 4: '文' (w=2)
    //   col 6: 'o' (w=1)
    //   col 7: 'k' (w=1)
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("H".to_string(), None),
        ("i".to_string(), None),
        ("\u{4E2D}".to_string(), None),  // 中
        ("\u{6587}".to_string(), None),  // 文
        ("o".to_string(), None),
        ("k".to_string(), None),
    ];
    grid.update_row(0, 0, &cells);

    assert_eq!(grid.get_cell(0, 0).unwrap().text, "H");
    assert_eq!(grid.get_cell(0, 0).unwrap().width, 1);

    assert_eq!(grid.get_cell(1, 0).unwrap().text, "i");
    assert_eq!(grid.get_cell(1, 0).unwrap().width, 1);

    assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{4E2D}");
    assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);

    assert_eq!(grid.get_cell(4, 0).unwrap().text, "\u{6587}");
    assert_eq!(grid.get_cell(4, 0).unwrap().width, 2);

    assert_eq!(grid.get_cell(6, 0).unwrap().text, "o");
    assert_eq!(grid.get_cell(6, 0).unwrap().width, 1);

    assert_eq!(grid.get_cell(7, 0).unwrap().text, "k");
    assert_eq!(grid.get_cell(7, 0).unwrap().width, 1);

    // The row should be marked dirty
    assert!(grid.is_row_dirty(0));
}

#[test]
fn test_update_row_col_start_offset() {
    // Verify that col_start offsets wide character placement
    let mut grid = CharacterGrid::new(20, 5);
    let cells = vec![
        ("\u{4E2D}".to_string(), None),  // 中 (width=2)
    ];
    grid.update_row(0, 5, &cells);

    // '中' should be placed at column 5
    assert_eq!(grid.get_cell(5, 0).unwrap().text, "\u{4E2D}");
    assert_eq!(grid.get_cell(5, 0).unwrap().width, 2);

    // Columns before 5 should be untouched (default space)
    assert_eq!(grid.get_cell(0, 0).unwrap().text, " ");
    assert_eq!(grid.get_cell(4, 0).unwrap().text, " ");
}
