//! P3.5 stage B: silent frames (B1).

use super::*;

/// A 10x4 renderer that has already painted one frame with `text` on row 0
/// and the cursor at (1, 2).
fn painted(silent: bool, text: &str) -> TtyRif {
    let mut rif = TtyRif::new(10, 4);
    rif.set_silent_frames(silent);
    for (col, ch) in text.chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.cursor_visible = true;
    rif.cursor_row = 1;
    rif.cursor_col = 2;
    rif.diff_and_render();
    rif.take_output();
    rif
}

/// Paint the same content again with the cursor at (ROW, COL) and SHAPE.
fn repaint(rif: &mut TtyRif, text: &str, cursor: Option<(u16, u16)>, shape: TerminalCursorShape) {
    rif.desired.clear(None);
    for (col, ch) in text.chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.cursor_visible = cursor.is_some();
    if let Some((row, col)) = cursor {
        rif.cursor_row = row;
        rif.cursor_col = col;
    }
    rif.cursor_shape = shape;
    rif.diff_and_render();
}

fn output_string(rif: &mut TtyRif) -> String {
    String::from_utf8(rif.take_output()).expect("ASCII output")
}

#[test]
fn silent_knob_parses_on_and_defaults_off() {
    assert!(!parse_tty_silent_knob(None));
    assert!(!parse_tty_silent_knob(Some("")));
    assert!(!parse_tty_silent_knob(Some("off")));
    assert!(parse_tty_silent_knob(Some("on")));
    assert!(parse_tty_silent_knob(Some(" 1 ")));
}

#[test]
fn idle_frame_writes_nothing_when_silent() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "");
    assert_eq!(rif.frame_stats().bytes, 0);
    assert_eq!(rif.current.cells[0].ch, 'h', "the screen model is kept");
}

#[test]
fn idle_frame_keeps_the_old_framing_when_the_knob_is_off() {
    let mut rif = painted(false, "hello");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    let out = output_string(&mut rif);
    assert!(out.contains("\x1b[?25l"), "hide: {out:?}");
    assert!(out.contains("\x1b[2;3H"), "goto: {out:?}");
    assert!(out.contains("\x1b[2 q"), "shape every frame: {out:?}");
    assert!(out.contains("\x1b[?25h"), "show: {out:?}");
}

#[test]
fn cursor_motion_alone_writes_only_the_motion() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", Some((2, 5)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "\x1b[3;6H");
}

#[test]
fn cursor_shape_change_alone_writes_only_the_shape() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Bar);
    assert_eq!(output_string(&mut rif), "\x1b[6 q");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Bar);
    assert_eq!(output_string(&mut rif), "", "the new shape is remembered");
}

#[test]
fn cursor_visibility_changes_are_written_once() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", None, TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "\x1b[?25l");
    repaint(&mut rif, "hello", None, TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "\x1b[2;3H\x1b[?25h");
}

#[test]
fn a_frame_with_cell_writes_resends_the_shape_only_when_it_changed() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hellO", Some((1, 2)), TerminalCursorShape::Block);
    let out = output_string(&mut rif);
    assert!(out.contains('O'), "the changed cell is written: {out:?}");
    assert!(
        !out.contains("\x1b[2 q"),
        "unchanged shape not re-sent: {out:?}"
    );
    assert!(out.contains("\x1b[?25l") && out.contains("\x1b[?25h"));
    repaint(
        &mut rif,
        "hello",
        Some((1, 2)),
        TerminalCursorShape::Underline,
    );
    let out = output_string(&mut rif);
    assert!(out.contains("\x1b[4 q"), "changed shape re-sent: {out:?}");
}

#[test]
fn a_forced_redraw_forgets_the_terminal_cursor() {
    let mut rif = painted(true, "hello");
    rif.force_redraw();
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    let out = output_string(&mut rif);
    assert!(out.contains("hello"), "full repaint: {out:?}");
    assert!(
        out.contains("\x1b[2 q"),
        "shape re-sent after a redraw: {out:?}"
    );
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "");
}
