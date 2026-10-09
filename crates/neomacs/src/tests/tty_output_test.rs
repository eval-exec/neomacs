use super::*;
struct Database(std::collections::BTreeMap<&'static str, &'static [u8]>);
impl TerminalCapabilityDatabase for Database {
    fn get_string(&mut self, cap: StringCapability<'_>) -> Option<Vec<u8>> {
        match cap {
            StringCapability::Termcap(name) => self.0.get(name).map(|bytes| bytes.to_vec()),
            _ => None,
        }
    }
    fn get_termcap_number(&mut self, _: &str) -> Option<i32> {
        None
    }
    fn get_flag(&mut self, cap: super::super::terminal_capabilities::FlagCapability<'_>) -> bool {
        match cap {
            super::super::terminal_capabilities::FlagCapability::Termcap(name) => {
                self.0.contains_key(name)
            }
            _ => false,
        }
    }
}
#[test]
fn standout_closes_before_plain_text_with_dedicated_or_borrowed_exit() {
    for (enter, exit) in [("so", "se"), ("us", "ue"), ("so", "me")] {
        let mut database = Database(
            [
                ("cm", b"G%p1%d,%p2%d;".as_slice()),
                (enter, b"ON"),
                (exit, b"OFF"),
            ]
            .into(),
        );
        let caps = Capabilities::from_database(&mut database, "").unwrap();
        let mut row = cells("AB");
        row[0].attrs.inverse = true;
        let mut bytes = Output::default();
        encode_cells(&mut bytes, &row, &caps.attributes);
        assert_eq!(bytes.bytes, b"ONAOFFB", "{enter}/{exit}");
    }
}

#[test]
fn native_padded_output_keeps_literal_text_and_scales_clear_screen() {
    if super::super::terminal_capabilities::tests::run_native_fixture_child() {
        return;
    }
    let mut caps = Capabilities::load("neo-app-padding").unwrap();
    assert!(caps.ansi && caps.needs_padding);
    // npc uses a sleep rather than baud-dependent padding bytes.
    let file = tempfile::tempfile().unwrap();
    caps.attach(&file).unwrap();
    let mut output = Vec::new();
    caps.enter(2).write_to(&mut output, &caps).unwrap();
    assert_eq!(output, b"CLEAR\x1b[?2004h");
    output.clear();
    let mut row = cells("A$<10/>B");
    row[0].attrs.inverse = true;
    let mut painter = TerminfoPainter {
        output: &mut output,
        caps: &caps,
        bytes: Output::default(),
        width: 0,
        height: 0,
    };
    painter.begin(8, 1).unwrap();
    painter.row(0, &row).unwrap();
    painter.finish(None).unwrap();
    assert_eq!(output, b"OFF\x1b[1;1HONAOFF$<10/>B");
    output.clear();
    // ANSI cursor spelling with padding still uses the native painter.
    render_to(&mut TtyRif::new(8, 1), &mut output, &caps).unwrap();
    assert!(!output.windows(2).any(|pair| pair == b"$<"));
    output.clear();
    caps.leave().write_to(&mut output, &caps).unwrap();
    assert_eq!(output, b"OFF\x1b[?2004l");
}

#[test]
fn vt52_output_and_lifecycle_use_native_capabilities() {
    let mut database = Database(
        [
            ("cm", b"\x1bY%p1%' '%+%c%p2%' '%+%c".as_slice()),
            ("ti", b"ENTER"),
            ("te", b"LEAVE"),
            ("ks", b"KEYS"),
            ("ke", b"NORMAL"),
        ]
        .into(),
    );
    let caps = Capabilities::from_database(&mut database, "").unwrap();
    assert!(!caps.ansi);
    assert_eq!(caps.enter(24).bytes, b"ENTERKEYS");
    assert_eq!(caps.leave().bytes, b"NORMALLEAVE");
    let mut output = Vec::new();
    let mut painter = TerminfoPainter {
        output: &mut output,
        caps: &caps,
        bytes: Output::default(),
        width: 0,
        height: 0,
    };
    painter.begin(80, 24).unwrap();
    let cell = TtyCell {
        ch: 'A',
        ..TtyCell::default()
    };
    painter.row(1, &[cell]).unwrap();
    painter
        .finish(Some((1, 2, TerminalCursorShape::Block)))
        .unwrap();
    assert_eq!(output, b"\x1bY! A\x1bY!\"");
}
#[test]
fn relative_cursor_entry_is_usable_without_ansi_addressing() {
    let mut database = Database(
        [
            ("ho", b"HOME".as_slice()),
            ("up", b"U"),
            ("do", b"D"),
            ("le", b"L"),
            ("nd", b"R"),
        ]
        .into(),
    );
    let caps = Capabilities::from_database(&mut database, "").unwrap();
    let mut bytes = Output::default();
    caps.goto(&mut bytes, 2, 3, 80, 24).unwrap();
    assert_eq!(bytes.bytes, b"HOMEDDRRR");
    database.0.remove("up");
    assert!(Capabilities::from_database(&mut database, "").is_err());
}
fn paint_bottom(database: &mut Database, cells: &[TtyCell]) -> io::Result<Vec<u8>> {
    let caps = Capabilities::from_database(database, "").unwrap();
    let mut output = Vec::new();
    let mut painter = TerminfoPainter {
        output: &mut output,
        caps: &caps,
        bytes: Output::default(),
        width: 0,
        height: 0,
    };
    painter.begin(cells.len(), 1)?;
    painter.row(0, cells)?;
    painter.finish(None)?;
    Ok(output)
}
fn cells(text: &str) -> Vec<TtyCell> {
    text.chars()
        .map(|ch| TtyCell {
            ch,
            ..TtyCell::default()
        })
        .collect()
}
#[test]
fn bottom_right_uses_insert_or_erase_without_scrolling() {
    let mut database = Database(
        [
            ("cm", b"G%p1%d,%p2%d;".as_slice()),
            ("am", b""),
            ("ic", b"I"),
            ("ip", b"P"),
            ("ce", b"E"),
        ]
        .into(),
    );
    assert_eq!(
        paint_bottom(&mut database, &cells("ABC")).unwrap(),
        b"G0,0;BCG0,0;IAP"
    );
    database.0.remove("ic");
    assert_eq!(
        paint_bottom(&mut database, &cells("AB ")).unwrap(),
        b"G0,0;ABG0,2;E"
    );
    assert!(paint_bottom(&mut database, &cells("ABC")).is_err());
    database.0.insert("im", b"BEGIN");
    database.0.insert("ei", b"END");
    assert_eq!(
        paint_bottom(&mut database, &cells("ABC")).unwrap(),
        b"ENDG0,0;BCG0,0;BEGINAPEND"
    );
    database.0.insert("ic", b"I");
    assert_eq!(
        paint_bottom(&mut database, &cells("ABC")).unwrap(),
        b"ENDG0,0;BCG0,0;BEGINIAPEND"
    );
}
#[test]
fn bottom_right_inserts_the_full_width_of_a_wide_first_glyph() {
    let mut database = Database(
        [
            ("cm", b"G%p1%d,%p2%d;".as_slice()),
            ("am", b""),
            ("IC", b"I%p1%d;"),
        ]
        .into(),
    );
    let mut row = cells("界 AB");
    row[1].padding = true;
    assert_eq!(
        paint_bottom(&mut database, &row).unwrap(),
        "G0,0;ABG0,0;I2;界".as_bytes()
    );
}
#[test]
fn relative_motion_uses_termcap_aliases_and_requires_a_safe_anchor() {
    let mut database = Database(
        [
            ("up", b"U".as_slice()),
            ("nl", b"D"),
            ("bc", b"L"),
            ("nd", b"R"),
        ]
        .into(),
    );
    let caps = Capabilities::from_database(&mut database, "").unwrap();
    let mut output = Output::default();
    caps.goto(&mut output, 1, 1, 2, 2).unwrap();
    assert_eq!(output.bytes, b"UULLDR");
    database.0.insert("bs", b"");
    let caps = Capabilities::from_database(&mut database, "").unwrap();
    assert_eq!(caps.control("le"), b"\x08");
    database.0.insert("bw", b"");
    assert!(Capabilities::from_database(&mut database, "").is_err());
    database.0.insert("cr", b"C");
    assert!(Capabilities::from_database(&mut database, "").is_ok());
}
#[test]
fn retry_exits_modes_left_active_by_a_partial_write() {
    struct PartialWriter {
        bytes: Vec<u8>,
        fail: bool,
    }
    impl Write for PartialWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail {
                if let Some(end) = bytes.windows(5).position(|part| part == b"BEGIN") {
                    self.bytes.extend_from_slice(&bytes[..end + 5]);
                    return Ok(end + 5);
                }
                return Err(io::Error::other("disconnected after entering insert mode"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut database = Database(
        [
            ("cm", b"G%p1%d,%p2%d;".as_slice()),
            ("am", b""),
            ("im", b"BEGIN"),
            ("ei", b"END"),
            ("me", b"RESET"),
        ]
        .into(),
    );
    let caps = Capabilities::from_database(&mut database, "").unwrap();
    let mut rif = TtyRif::new(3, 1);
    let mut writer = PartialWriter {
        bytes: Vec::new(),
        fail: true,
    };
    assert!(paint_to(&mut rif, &mut writer, &caps).is_err());
    assert!(writer.bytes.ends_with(b"BEGIN"));
    writer.fail = false;
    writer.bytes.clear();
    paint_to(&mut rif, &mut writer, &caps).unwrap();
    assert!(writer.bytes.starts_with(b"ENDRESETG0,0;"));
    assert!(writer.bytes.windows(9).any(|part| part == b"BEGIN END"));
}
#[test]
fn missing_terminal_description_is_an_initialization_error() {
    assert!(Capabilities::load("").is_err());
    assert!(Capabilities::load("neo-certainly-no-such-terminal-363").is_err());
}
