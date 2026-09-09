//! Terminal output selected before screen operations become ANSI bytes.
use super::terminal_capabilities::{StringCapability, TerminalCapabilityDatabase};
use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use neomacs_display_runtime::backend::tty::rif::painter::{TtyPainter, encode_cells};
use neomacs_display_runtime::backend::tty::rif::{TerminalCursorShape, TtyCell, TtyRif};
use std::io::{self, Write};

#[cfg(windows)]
#[path = "tty_output_windows.rs"]
pub(crate) mod windows;

pub(crate) const CONTROL_NAMES: &[&str] = &[
    "cm", "ho", "up", "do", "nl", "le", "bc", "nd", "cr", "cl", "ce", "vi", "ve", "ti", "te", "ks",
    "ke", "ic", "IC", "im", "ei", "ip", "se",
];

#[derive(Clone)]
pub(crate) struct Capabilities {
    strings: std::collections::BTreeMap<&'static str, Vec<u8>>,
    pub(crate) attributes: TtyAttributeCapabilities,
    pub(crate) ansi: bool,
    auto_wrap: bool,
}

impl Capabilities {
    pub(crate) fn load(term: &str) -> Result<Self, String> {
        if term.is_empty() {
            return Err("Please set the environment variable TERM".to_owned());
        }
        let mut database = super::terminal_capabilities::open_terminal_capability_database(term)
            .ok_or_else(|| format!("Terminal type \"{term}\" is not defined, or its terminfo database cannot be read"))?;
        Self::from_database(
            database.as_mut(),
            &std::env::var("COLORTERM").unwrap_or_default(),
        )
    }

    fn from_database(
        database: &mut dyn TerminalCapabilityDatabase,
        colorterm: &str,
    ) -> Result<Self, String> {
        let ansi = super::terminal_capabilities::termcap_cap_is(database, "cm", b"\x1b[%i%d;%dH");
        let mut strings: std::collections::BTreeMap<_, _> = CONTROL_NAMES
            .iter()
            .filter_map(|name| {
                database
                    .get_string(StringCapability::Termcap(name))
                    .filter(|bytes| !bytes.is_empty())
                    .map(|bytes| (*name, bytes))
            })
            .collect();
        use super::terminal_capabilities::FlagCapability::Termcap;
        if !strings.contains_key("do")
            && let Some(value) = strings.get("nl").cloned()
        {
            strings.insert("do", value);
        }
        if database.get_flag(Termcap("bs")) {
            strings.insert("le", vec![8]);
        } else if !strings.contains_key("le")
            && let Some(value) = strings.get("bc").cloned()
        {
            strings.insert("le", value);
        }
        let backward_wrap = database.get_flag(Termcap("bw"));
        let result = Self {
            strings,
            ansi,
            auto_wrap: database.get_flag(Termcap("am")),
            attributes: super::terminal_capabilities::resolve_tty_attribute_capabilities(
                database, colorterm,
            ),
        };
        if result.control("cm").is_empty()
            && ["up", "do", "le", "nd"]
                .iter()
                .any(|name| result.control(name).is_empty())
        {
            return Err(
                "Terminal lacks absolute cursor addressing or sufficient relative cursor movement"
                    .to_owned(),
            );
        }
        if result.control("cm").is_empty()
            && backward_wrap
            && result.control("ho").is_empty()
            && result.control("cr").is_empty()
        {
            return Err("Relative cursor addressing needs home or carriage return on a terminal with backward wrapping".to_owned());
        }
        // Validate the program before raw mode or alternate-screen entry.
        let mut probe = Vec::new();
        result
            .goto(&mut probe, 0, 0, 80, 24)
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn control(&self, name: &str) -> &[u8] {
        self.strings.get(name).map_or(&[], Vec::as_slice)
    }

    fn append(&self, output: &mut Vec<u8>, name: &str) {
        output.extend_from_slice(&super::terminal_capabilities::rendition_sequence(
            self.control(name),
        ));
    }

    fn goto(
        &self,
        output: &mut Vec<u8>,
        row: usize,
        col: usize,
        width: usize,
        height: usize,
    ) -> io::Result<()> {
        if !self.control("cm").is_empty() {
            let mut parameters = [0; 9];
            parameters[0] = i32::try_from(row).map_err(io::Error::other)?;
            parameters[1] = i32::try_from(col).map_err(io::Error::other)?;
            let sequence = neomacs_terminfo::expand_numeric(self.control("cm"), parameters)
                .map_err(io::Error::other)?;
            output.extend_from_slice(&super::terminal_capabilities::rendition_sequence(&sequence));
        } else {
            // Re-anchor each run, including after a failed/partial previous
            // write. Home is preferred; relative movement clamps at margins.
            if !self.control("ho").is_empty() {
                self.append(output, "ho");
            } else {
                for _ in 0..height {
                    self.append(output, "up");
                }
                if !self.control("cr").is_empty() {
                    self.append(output, "cr");
                } else {
                    for _ in 0..width {
                        self.append(output, "le");
                    }
                }
            }
            for _ in 0..row {
                self.append(output, "do");
            }
            for _ in 0..col {
                self.append(output, "nd");
            }
        }
        Ok(())
    }

    fn reset_modes(&self, output: &mut Vec<u8>) {
        self.append(output, "ei");
        if let Some(sequence) = &self.attributes.exit_attribute_mode {
            output.extend_from_slice(sequence);
        } else {
            if let Some(sequence) = &self.attributes.exit_underline_mode {
                output.extend_from_slice(sequence);
            }
            self.append(output, "se");
        }
        if let Some(colors) = self.attributes.colors.entry() {
            output.extend_from_slice(colors.orig_pair());
        }
    }

    pub(crate) fn enter(&self) -> Vec<u8> {
        let mut output = Vec::new();
        for name in ["ti", "ks", "vi", "cl"] {
            self.append(&mut output, name);
        }
        if self.ansi {
            output.extend_from_slice(b"\x1b[?2004h");
        }
        output
    }

    pub(crate) fn leave(&self) -> Vec<u8> {
        let mut output = Vec::new();
        self.reset_modes(&mut output);
        if self.ansi {
            output.extend_from_slice(b"\x1b[?2004l");
        }
        for name in ["ve", "ke", "te"] {
            self.append(&mut output, name);
        }
        output
    }
}

#[cfg(not(windows))]
pub(crate) fn primary() -> Result<&'static Capabilities, String> {
    static CAPS: std::sync::OnceLock<Result<Capabilities, String>> = std::sync::OnceLock::new();
    CAPS.get_or_init(|| Capabilities::load(&std::env::var("TERM").unwrap_or_default()))
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn render_to(
    rif: &mut TtyRif,
    output: &mut impl Write,
    caps: &Capabilities,
) -> io::Result<()> {
    if caps.ansi {
        rif.diff_and_render();
        let mut bytes = Vec::new();
        caps.reset_modes(&mut bytes);
        bytes.extend_from_slice(&rif.take_output());
        if let Err(error) = output.write_all(&bytes).and_then(|()| output.flush()) {
            rif.force_redraw();
            return Err(error);
        }
        Ok(())
    } else {
        paint_to(rif, output, caps)
    }
}

// Secondary terminals always use their own complete capability snapshot,
// including attributes; the primary ANSI encoder has a global attribute record.
pub(crate) fn paint_to(
    rif: &mut TtyRif,
    output: &mut impl Write,
    caps: &Capabilities,
) -> io::Result<()> {
    rif.paint(&mut TerminfoPainter {
        output,
        caps,
        bytes: Vec::new(),
        width: 0,
        height: 0,
    })
}

struct TerminfoPainter<'a, W> {
    output: &'a mut W,
    caps: &'a Capabilities,
    bytes: Vec<u8>,
    width: usize,
    height: usize,
}
impl<W: Write> TerminfoPainter<'_, W> {
    fn bottom_row(&mut self, row: usize, cells: &[TtyCell]) -> io::Result<()> {
        // GNU tty_write_glyphs writes the suffix first, then inserts the first
        // glyph so no ordinary write touches the physical bottom-right cell.
        let first_width = 1 + cells.iter().skip(1).take_while(|cell| cell.padding).count();
        let insert = !self.caps.control("ic").is_empty() || !self.caps.control("IC").is_empty();
        let insert_mode =
            !self.caps.control("im").is_empty() && !self.caps.control("ei").is_empty();
        if first_width < cells.len() && (insert || insert_mode) {
            encode_cells(
                &mut self.bytes,
                &cells[first_width..],
                &self.caps.attributes,
            );
            self.caps
                .goto(&mut self.bytes, row, 0, self.width, self.height)?;
            let multi_insert = !self.caps.control("IC").is_empty();
            if multi_insert {
                let mut parameters = [0; 9];
                parameters[0] = first_width as i32;
                let sequence =
                    neomacs_terminfo::expand_numeric(self.caps.control("IC"), parameters)
                        .map_err(io::Error::other)?;
                self.bytes
                    .extend_from_slice(&super::terminal_capabilities::rendition_sequence(
                        &sequence,
                    ));
            } else {
                // Some terminals require insert mode even for their single-
                // character insertion command; GNU uses im together with ic.
                if insert_mode {
                    self.caps.append(&mut self.bytes, "im");
                }
                for _ in 0..first_width {
                    self.caps.append(&mut self.bytes, "ic");
                }
            }
            encode_cells(
                &mut self.bytes,
                &cells[..first_width],
                &self.caps.attributes,
            );
            if !multi_insert {
                self.caps.append(&mut self.bytes, "ip");
                if insert_mode {
                    self.caps.append(&mut self.bytes, "ei");
                }
            }
            return Ok(());
        }
        // A trailing default blank can instead be erased without advancing.
        if let Some(last) = cells.last()
            && last.ch == ' '
            && !last.padding
            && last.extenders.is_none()
            && last.attrs == Default::default()
            && !self.caps.control("ce").is_empty()
        {
            encode_cells(
                &mut self.bytes,
                &cells[..cells.len() - 1],
                &self.caps.attributes,
            );
            self.caps.goto(
                &mut self.bytes,
                row,
                cells.len() - 1,
                self.width,
                self.height,
            )?;
            self.caps.append(&mut self.bytes, "ce");
            return Ok(());
        }
        Err(io::Error::other(
            "terminal cannot safely paint the bottom-right cell without insertion or erase support",
        ))
    }
}

impl<W: Write> TtyPainter for TerminfoPainter<'_, W> {
    fn begin(&mut self, width: usize, height: usize) -> io::Result<()> {
        self.width = width;
        self.height = height;
        // A preceding partial write may have left insertion or rendition on.
        // Reset before repainting, even when the first desired face is default.
        self.caps.reset_modes(&mut self.bytes);
        self.caps.append(&mut self.bytes, "vi");
        Ok(())
    }
    fn row(&mut self, row: usize, cells: &[TtyCell]) -> io::Result<()> {
        if cells.is_empty() {
            return Ok(());
        }
        self.caps
            .goto(&mut self.bytes, row, 0, self.width, self.height)?;
        if self.caps.auto_wrap && row + 1 == self.height && cells.len() == self.width {
            self.bottom_row(row, cells)?;
        } else {
            encode_cells(&mut self.bytes, cells, &self.caps.attributes);
        }
        Ok(())
    }
    fn finish(&mut self, cursor: Option<(u16, u16, TerminalCursorShape)>) -> io::Result<()> {
        if let Some((row, col, shape)) = cursor {
            self.caps.goto(
                &mut self.bytes,
                usize::from(row),
                usize::from(col),
                self.width,
                self.height,
            )?;
            if self.caps.ansi {
                let style = match shape {
                    TerminalCursorShape::Block => 2,
                    TerminalCursorShape::Underline => 4,
                    TerminalCursorShape::Bar => 6,
                };
                write!(self.bytes, "\x1b[{style} q")?;
            }
            self.caps.append(&mut self.bytes, "ve");
        }
        self.output.write_all(&self.bytes)?;
        self.output.flush()
    }
}

pub(crate) fn popup_line(row: usize, col: usize, text: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::popup_line(row, col, text)
    }
    #[cfg(not(windows))]
    {
        let caps = primary().map_err(io::Error::other)?;
        let (width, height) = super::tty_init::query_terminal_size_cells().unwrap_or((80, 24));
        let mut bytes = Vec::new();
        caps.goto(&mut bytes, row, col, width as usize, height as usize)?;
        let cells: Vec<_> = text
            .chars()
            .map(|ch| TtyCell {
                ch,
                attrs: neomacs_display_runtime::backend::tty::rif::CellAttrs {
                    inverse: true,
                    ..Default::default()
                },
                ..TtyCell::default()
            })
            .collect();
        encode_cells(&mut bytes, &cells, &caps.attributes);
        caps.reset_modes(&mut bytes);
        let mut stdout = io::stdout();
        stdout.write_all(&bytes)?;
        stdout.flush()
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
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
        fn get_flag(
            &mut self,
            cap: super::super::terminal_capabilities::FlagCapability<'_>,
        ) -> bool {
            match cap {
                super::super::terminal_capabilities::FlagCapability::Termcap(name) => {
                    self.0.contains_key(name)
                }
                _ => false,
            }
        }
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
        assert_eq!(caps.enter(), b"ENTERKEYS");
        assert_eq!(caps.leave(), b"NORMALLEAVE");
        let mut output = Vec::new();
        let mut painter = TerminfoPainter {
            output: &mut output,
            caps: &caps,
            bytes: Vec::new(),
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
        let mut bytes = Vec::new();
        caps.goto(&mut bytes, 2, 3, 80, 24).unwrap();
        assert_eq!(bytes, b"HOMEDDRRR");
        database.0.remove("up");
        assert!(Capabilities::from_database(&mut database, "").is_err());
    }
    fn paint_bottom(database: &mut Database, cells: &[TtyCell]) -> io::Result<Vec<u8>> {
        let caps = Capabilities::from_database(database, "").unwrap();
        let mut output = Vec::new();
        let mut painter = TerminfoPainter {
            output: &mut output,
            caps: &caps,
            bytes: Vec::new(),
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
        let mut output = Vec::new();
        caps.goto(&mut output, 1, 1, 2, 2).unwrap();
        assert_eq!(output, b"UULLDR");
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
}
