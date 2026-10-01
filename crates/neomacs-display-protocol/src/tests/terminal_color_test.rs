use super::terminal_color::TerminalColor;

/// The INDEX element of `tty-color-desc` is a palette subscript below 24-bit
/// colour and a packed pixel at it -- `tty-color-24bit`'s own test
/// (lisp/term/tty-colors.el:834).
///
/// The pixel row is read out of GNU 31.0.90 on a real pty with
/// `COLORTERM=truecolor` and `TERM=xterm-256color`, where
/// `(display-color-cells)` is 16777216:
///
/// ```text
/// (tty-color-desc "red")     => ("red" 13434880 52685 0 0)
/// (tty-color-desc "#123456") => ("#123456" 1193046 4626 13364 22102)
/// ```
///
/// 13434880 is 0xCD0000 and 1193046 is 0x123456, which is why GNU's
/// `TF_rgb_separate` `setaf` can split the very same slot into three channels
/// (`fg >> 16`, `(fg >> 8) & 0xFF`, `fg & 0xFF`, src/term.c:2103).
#[test]
fn tty_color_desc_index_reads_as_subscript_or_pixel_by_cell_count() {
    assert_eq!(
        TerminalColor::from_tty_color_desc(145, 256),
        Some(TerminalColor::Indexed(145))
    );
    assert_eq!(
        TerminalColor::from_tty_color_desc(12, 16),
        Some(TerminalColor::Indexed(12))
    );
    assert_eq!(
        TerminalColor::from_tty_color_desc(13_434_880, 16_777_216),
        Some(TerminalColor::Direct { r: 205, g: 0, b: 0 })
    );
    assert_eq!(
        TerminalColor::from_tty_color_desc(1_193_046, 16_777_216),
        Some(TerminalColor::Direct {
            r: 0x12,
            g: 0x34,
            b: 0x56
        })
    );
}

/// A malformed Lisp answer is not a colour. GNU gives up on the whole
/// descriptor when the INDEX is not a fixnum (`tty_lookup_color`,
/// src/xfaces.c:1098-1099); the equivalent here is refusing values that cannot
/// be a palette subscript at all, so nothing downstream has to decide what a
/// negative index means.
#[test]
fn a_value_that_cannot_be_a_terminal_colour_is_not_one() {
    assert_eq!(TerminalColor::from_tty_color_desc(-1, 256), None);
    assert_eq!(TerminalColor::from_tty_color_desc(-1, 16_777_216), None);
    assert_eq!(TerminalColor::from_tty_color_desc(70_000, 256), None);
    assert_eq!(
        TerminalColor::from_tty_color_desc(1 << 40, 16_777_216),
        None
    );
}
