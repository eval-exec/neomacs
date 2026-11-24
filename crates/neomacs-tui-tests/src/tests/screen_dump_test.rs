//! The canonical screen dump that the seam oracle (tests/seam_baseline.rs)
//! records and compares: equal dumps mean visibly equal screens.

use crate::CanonicalScreenDump;
use expect_test::expect;

fn dump_of(rows: u16, columns: u16, bytes: &[u8]) -> CanonicalScreenDump {
    let mut parser = vt100::Parser::new(rows, columns, 0);
    parser.process(bytes);
    CanonicalScreenDump::capture(parser.screen())
}

fn dump(bytes: &[u8]) -> CanonicalScreenDump {
    dump_of(3, 10, bytes)
}

#[test]
fn the_encoding_is_pinned() {
    // A recorded baseline is only comparable with dumps of the same encoding,
    // so any change here must also bump the header's version.
    expect![[r#"
        neomacs-tui-tests canonical-screen v1
        size 3x10
        cursor visible 1,3
        modes alternate-screen=off application-keypad=off application-cursor=off bracketed-paste=off mouse=None/Default
        text
        0|abC       |
        1|          |
        2|          |
        style
        0 0-1 glyph fg=default bg=default
        0 2-2 glyph fg=idx:1 bg=rgb:0a0b0c bold
        0 3-9 blank bg=default
        1 0-9 blank bg=default
        2 0-9 blank bg=default
    "#]]
    .assert_eq(dump(b"ab\x1b[1;31;48;2;10;11;12mC\x1b[0m\x1b[2;4H").as_str());
}

#[test]
fn a_glyph_attribute_change_changes_the_dump() {
    assert_ne!(dump(b"X"), dump(b"\x1b[1mX"));
    assert_ne!(dump(b"X"), dump(b"\x1b[3mX"));
    assert_ne!(dump(b"X"), dump(b"\x1b[32mX"));
    assert_ne!(dump(b"X"), dump(b"Y"));
}

#[test]
fn erase_history_that_paints_nothing_is_not_part_of_the_dump() {
    // Written spaces and never-written cells look the same.
    assert_eq!(dump(b"   \x1b[1;1H"), dump(b""));
    // An erase retains the active foreground on blank cells, invisibly
    // (GNU's tty_clear_end_of_line selects only the background).
    assert_eq!(dump(b"\x1b[31;1m\x1b[2K\x1b[0m"), dump(b""));
}

#[test]
fn blank_paint_that_is_visible_is_part_of_the_dump() {
    assert_ne!(dump(b"\x1b[41m\x1b[2K\x1b[0m"), dump(b""));
    assert_ne!(dump(b"\x1b[4m \x1b[0m\x1b[1;1H"), dump(b" \x1b[1;1H"));
    assert_ne!(dump(b"\x1b[7m \x1b[0m\x1b[1;1H"), dump(b" \x1b[1;1H"));
}

#[test]
fn the_cursor_is_part_of_the_dump_only_while_visible() {
    assert_ne!(dump(b"\x1b[2;3H"), dump(b""));
    assert_ne!(dump(b"\x1b[?25l"), dump(b""));
    assert_eq!(dump(b"\x1b[?25l\x1b[2;3H"), dump(b"\x1b[?25l"));
}

#[test]
fn terminal_modes_are_part_of_the_dump() {
    assert_ne!(dump(b"\x1b[?2004h"), dump(b""));
    assert_ne!(dump(b"\x1b[?1049h"), dump(b""));
    assert_ne!(dump(b"\x1b[?1000h"), dump(b""));
}

#[test]
fn a_wide_character_is_one_glyph_and_one_continuation_cell() {
    let wide = dump("中".as_bytes());
    let text = wide.as_str();
    assert!(text.contains("\n0|中        |\n"), "{text}");
    assert!(
        text.contains("\n0 0-0 wide fg=default bg=default\n"),
        "{text}"
    );
    assert!(
        text.contains("\n0 1-1 cont fg=default bg=default\n"),
        "{text}"
    );
    assert_ne!(wide, dump("\x1b[31m中".as_bytes()));
}

#[test]
fn soft_wrap_history_is_not_part_of_the_dump() {
    // Same cells and cursor; only vt100's row-wrapped flag differs.
    assert_eq!(
        dump_of(2, 10, b"0123456789X"),
        dump_of(2, 10, b"0123456789\x1b[2;1HX"),
    );
}

#[test]
fn row_labels_share_one_width() {
    let tall = dump_of(12, 2, b"");
    let text = tall.as_str();
    assert!(text.contains("\n00|  |\n"), "{text}");
    assert!(text.contains("\n11|  |\n"), "{text}");
    assert!(text.contains("\n11 0-1 blank bg=default\n"), "{text}");
}
