#![cfg(unix)]

use neomacs_tui_tests::RawTerminalSnapshot;

fn screen(rows: u16, cols: u16, bytes: &[u8]) -> vt100::Parser {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(bytes);
    parser
}

#[test]
fn snapshot_can_capture_the_full_terminal_screen() {
    let parser = screen(3, 4, b"top\x1b[3;1Hbottom");

    let snapshot = RawTerminalSnapshot::capture_full_screen(parser.screen());

    assert_eq!(snapshot.captured_rows, 0..3);
    assert_eq!(snapshot.rows.len(), 3);
    assert!(snapshot.rows.iter().all(|row| row.cells.len() == 4));
}

#[test]
fn snapshot_compares_terminal_state_not_the_original_ansi_spelling() {
    let short = screen(2, 4, b"\x1b[31mX\x1b[39m");
    let long = screen(2, 4, b"\x1b[1;1H\x1b[0;31mX\x1b[0m");

    assert_eq!(
        RawTerminalSnapshot::capture_rows(short.screen(), 0..2),
        RawTerminalSnapshot::capture_rows(long.screen(), 0..2),
    );
}

#[test]
fn snapshot_does_not_normalize_written_spaces_into_unwritten_cells() {
    let unwritten = screen(1, 3, b"X");
    let written = screen(1, 3, b"X ");

    let gnu = RawTerminalSnapshot::capture_rows(unwritten.screen(), 0..1);
    let neo = RawTerminalSnapshot::capture_rows(written.screen(), 0..1);

    assert_ne!(gnu, neo);
    assert_eq!(
        gnu.exact_differences(&neo),
        vec![
            "cursor_position: GNU (0, 1) | Neomacs (0, 2)",
            "row 0 col 1: GNU contents=\"\" fg=Default bg=Default attrs=[] wide=false continuation=false | Neomacs contents=\" \" fg=Default bg=Default attrs=[] wide=false continuation=false"
        ],
    );
}

#[test]
fn snapshot_exposes_ansi_and_control_free_grid_projections() {
    let parser = screen(2, 4, b"\x1b[31mA \x1b[39mB");
    let snapshot = RawTerminalSnapshot::capture_rows(parser.screen(), 0..2);

    assert_eq!(
        snapshot.plain_grid(),
        " 0 |A\u{2420}B\u{2205}|\n 1 |\u{2205}\u{2205}\u{2205}\u{2205}|\n",
    );
    assert_eq!(
        snapshot.ansi_grid(),
        "\x1b[0;38;5;1mA \x1b[0mB \x1b[0m\n    \x1b[0m\n",
    );
}
