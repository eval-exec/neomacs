use super::*;

#[test]
fn tty_row_edge_reserves_one_column_for_the_edge_glyph() {
    assert_eq!(RowEdge::tty(80, 9.0, LineWrap::Truncate).x, 711.0);
    assert_eq!(RowEdge::tty(80, 9.0, LineWrap::WindowWrap).x, 711.0);
    // A one-column body cannot go below one usable column.
    assert_eq!(RowEdge::tty(1, 9.0, LineWrap::Truncate).x, 9.0);
}
