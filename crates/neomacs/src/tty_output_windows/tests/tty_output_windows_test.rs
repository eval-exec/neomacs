use super::*;
use neomacs_display_protocol::TerminalColor;

#[test]
fn legacy_palette_preserves_defaults_and_realized_inverse_colors() {
    let mut attrs = CellAttrs::default();
    assert_eq!(legacy_attributes(&attrs, 0x17), 0x17);
    attrs.inverse = true;
    assert_eq!(legacy_attributes(&attrs, 0x17), 0x71);
    attrs.fg = Some(TerminalColor::Indexed(1)); // ANSI red -> native red bit4.
    attrs.bg = Some(TerminalColor::Indexed(4)); // ANSI blue -> native blue bit1.
    assert_eq!(legacy_attributes(&attrs, 0x17), 0x14);
    attrs.fg = Some(TerminalColor::Indexed(256));
    assert_eq!(legacy_attributes(&attrs, 0x17), 0x11);
}

#[test]
#[ignore = "requires a native Windows console; CI runs in CREATE_NEW_CONSOLE"]
fn native_vt_console_negotiates_renders_and_restores_output_mode() {
    let original = ScreenBuffer::current().unwrap();
    let original_mode = ConsoleMode::from(original.handle().clone()).mode().unwrap();
    let mut session = Session::new().unwrap();
    assert!(session.vt, "modern Windows CI must support the VT path");
    assert_eq!(
        ConsoleMode::from(original.handle().clone()).mode().unwrap(),
        original_mode
    );
    session.enter().unwrap();
    let mode = ConsoleMode::from(original.handle().clone()).mode().unwrap();
    assert_ne!(mode & VIRTUAL_TERMINAL_PROCESSING, 0);
    session
        .console()
        .write_char_buffer(b"\x1b[2;3H\x1b[38;2;1;2;3mA\x1b[0m")
        .unwrap();
    let active = ScreenBuffer::current().unwrap();
    let info = active.info().unwrap();
    assert_eq!(info.cursor_pos().x, info.terminal_window().left + 3);
    assert_eq!(info.cursor_pos().y, info.terminal_window().top + 1);
    session.leave().unwrap();
    assert_eq!(
        ConsoleMode::from(original.handle().clone()).mode().unwrap(),
        original_mode
    );
}

#[test]
#[ignore = "requires a native Windows console; CI runs in CREATE_NEW_CONSOLE"]
fn native_legacy_console_restores_the_original_screen_and_mode() {
    let mut session = Session::new().unwrap();
    let original = session.original.info().unwrap();
    let original_cursor = cursor_info::get(&session.original).unwrap();
    session.vt = false; // Exercise the fallback even on modern Windows.
    ConsoleMode::from(session.original.handle().clone())
        .set_mode(session.original_mode)
        .unwrap();
    session.enter().unwrap();
    assert!(session.alternate.is_some());
    let mut painter = LegacyPainter { session: &session };
    painter.begin(80, 24).unwrap();
    assert!(!cursor_info::get(session.screen()).unwrap().1);
    painter
        .row(
            0,
            &[TtyCell {
                ch: 'A',
                ..TtyCell::default()
            }],
        )
        .unwrap();
    painter
        .finish(Some((0, 1, TerminalCursorShape::Block)))
        .unwrap();
    for (shape, expected) in [
        (TerminalCursorShape::Block, 99),
        (TerminalCursorShape::Underline, 25),
        (TerminalCursorShape::Bar, 99),
    ] {
        painter.finish(Some((0, 1, shape))).unwrap();
        assert_eq!(
            cursor_info::get(session.screen()).unwrap(),
            (expected, true)
        );
    }
    painter.begin(80, 24).unwrap();
    painter.finish(None).unwrap();
    assert!(!cursor_info::get(session.screen()).unwrap().1);
    let window = session
        .alternate
        .as_ref()
        .unwrap()
        .info()
        .unwrap()
        .terminal_window();
    let position = session
        .alternate
        .as_ref()
        .unwrap()
        .info()
        .unwrap()
        .cursor_pos();
    assert_eq!(position.x, window.left + 1);
    assert_eq!(position.y, window.top);
    session.leave().unwrap();
    let restored = ScreenBuffer::current().unwrap();
    assert_eq!(cursor_info::get(&restored).unwrap(), original_cursor);
    assert_eq!(restored.info().unwrap().cursor_pos(), original.cursor_pos());
    assert_eq!(restored.info().unwrap().attributes(), original.attributes());
    assert_eq!(
        ConsoleMode::from(restored.handle().clone()).mode().unwrap(),
        session.original_mode
    );
}
