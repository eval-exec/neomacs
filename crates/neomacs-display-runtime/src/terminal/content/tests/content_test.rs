use super::*;

#[test]
fn test_render_cell_creation() {
    let cell = RenderCell {
        col: 0,
        row: 0,
        c: 'A',
        fg: Color::WHITE,
        bg: Color::BLACK,
        flags: CellFlags::empty(),
    };
    assert_eq!(cell.c, 'A');
    assert_eq!(cell.col, 0);
}

#[test]
fn test_terminal_content_default() {
    let content = TerminalContent {
        cells: vec![],
        cols: 80,
        rows: 24,
        cursor: RenderCursor {
            col: 0,
            row: 0,
            visible: true,
        },
        default_bg: Color::BLACK,
        default_fg: Color::WHITE,
    };
    assert_eq!(content.cols, 80);
    assert_eq!(content.rows, 24);
    assert!(content.cursor.visible);
}
