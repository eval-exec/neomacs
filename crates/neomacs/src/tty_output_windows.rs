//! Windows output mode and screen-buffer ownership, using safe crossterm APIs.
use crossterm::Command;
use crossterm_winapi::{Console, ConsoleMode, ScreenBuffer};
use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use neomacs_display_runtime::backend::tty::rif::painter::TtyPainter;
use neomacs_display_runtime::backend::tty::rif::{CellAttrs, TerminalCursorShape, TtyCell, TtyRif};
use std::cell::RefCell;
use std::io;

const PROCESSED_OUTPUT: u32 = 1;
const VIRTUAL_TERMINAL_PROCESSING: u32 = 4;
const DISABLE_NEWLINE_AUTO_RETURN: u32 = 8;

thread_local! { static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) }; }

fn with_session<T>(operation: impl FnOnce(&mut Session) -> io::Result<T>) -> io::Result<T> {
    SESSION.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(Session::new()?);
        }
        operation(slot.as_mut().expect("initialized console session"))
    })
}

struct Session {
    original: ScreenBuffer,
    original_mode: u32,
    original_attributes: u16,
    alternate: Option<ScreenBuffer>,
    vt: bool,
    active: bool,
}

impl Session {
    fn new() -> io::Result<Self> {
        let original = ScreenBuffer::current()?;
        let mode = ConsoleMode::from(original.handle().clone());
        let original_mode = mode.mode()?;
        let original_attributes = original.info()?.attributes();
        // Probe the actual handle. TERM and crossterm's ANSI heuristic cannot
        // establish whether this console accepts virtual-terminal processing.
        let vt = windows_version::OsVersion::current()
            >= windows_version::OsVersion::new(10, 0, 0, 15064)
            && mode
                .set_mode(
                    original_mode
                        | PROCESSED_OUTPUT
                        | VIRTUAL_TERMINAL_PROCESSING
                        | DISABLE_NEWLINE_AUTO_RETURN,
                )
                .is_ok();
        // Capability discovery must not leave the shell's output mode changed,
        // including when startup exits before raw mode is entered.
        mode.set_mode(original_mode)?;
        Ok(Self {
            original,
            original_mode,
            original_attributes,
            alternate: None,
            vt,
            active: false,
        })
    }

    fn console(&self) -> Console {
        Console::from(
            self.alternate
                .as_ref()
                .unwrap_or(&self.original)
                .handle()
                .clone(),
        )
    }

    fn enter(&mut self) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        if self.vt {
            ConsoleMode::from(self.original.handle().clone()).set_mode(
                self.original_mode
                    | PROCESSED_OUTPUT
                    | VIRTUAL_TERMINAL_PROCESSING
                    | DISABLE_NEWLINE_AUTO_RETURN,
            )?;
            self.active = true;
            self.console()
                .write_char_buffer(super::super::tty_init::tty_enter_sequence())?;
        } else {
            let alternate = ScreenBuffer::create()?;
            // Disable automatic wrapping: writing the bottom-right cell must
            // not scroll the screen behind the renderer's model.
            ConsoleMode::from(alternate.handle().clone()).set_mode(PROCESSED_OUTPUT)?;
            alternate.show()?;
            self.alternate = Some(alternate);
            self.active = true;
        }
        Ok(())
    }

    fn leave(&mut self) -> io::Result<()> {
        let output = (|| {
            if self.active {
                if self.vt {
                    self.console()
                        .write_char_buffer(super::super::tty_init::tty_leave_sequence())?;
                } else {
                    self.original.show()?;
                    self.alternate = None;
                }
                self.active = false;
            }
            Ok(())
        })();
        // Restore the mode even if writing the exit sequence fails.
        let restored =
            ConsoleMode::from(self.original.handle().clone()).set_mode(self.original_mode);
        output.and(restored)
    }

    fn move_to(&self, row: usize, col: usize) -> io::Result<()> {
        let window = self
            .alternate
            .as_ref()
            .unwrap_or(&self.original)
            .info()?
            .terminal_window();
        let x = i16::try_from(col)
            .ok()
            .and_then(|col| col.checked_add(window.left));
        let y = i16::try_from(row)
            .ok()
            .and_then(|row| row.checked_add(window.top));
        let (Some(x), Some(y)) = (x, y) else {
            return Err(io::Error::other("console coordinate out of range"));
        };
        crossterm::cursor::MoveTo(
            u16::try_from(x).map_err(io::Error::other)?,
            u16::try_from(y).map_err(io::Error::other)?,
        )
        .execute_winapi()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

pub(crate) fn check() -> Result<(), String> {
    with_session(|_| Ok(())).map_err(|error| error.to_string())
}
pub(crate) fn attributes() -> TtyAttributeCapabilities {
    let vt = with_session(|session| Ok(session.vt)).unwrap_or(false);
    let mut caps =
        TtyAttributeCapabilities::full_with_color_cells(if vt { 16_777_216 } else { 16 });
    if !vt {
        caps.italic_sequence = None;
        caps.dim_sequence = None;
        caps.strike_through_sequence = None;
        caps.styled_underline = None;
    }
    caps
}
pub(crate) fn enter() -> io::Result<()> {
    with_session(|session| {
        if let Err(error) = session.enter() {
            let _ = session.leave();
            Err(error)
        } else {
            Ok(())
        }
    })
}
pub(crate) fn leave() -> io::Result<()> {
    with_session(Session::leave)
}

pub(crate) fn render(rif: &mut TtyRif) -> io::Result<()> {
    with_session(|session| {
        if session.vt {
            rif.diff_and_render();
            let mut bytes = b"\x1b[4l\x1b[0m".to_vec();
            bytes.extend_from_slice(&rif.take_output());
            if let Err(error) = session.console().write_char_buffer(&bytes) {
                rif.force_redraw();
                return Err(error);
            }
            Ok(())
        } else {
            rif.paint(&mut LegacyPainter { session })
        }
    })
}

struct LegacyPainter<'a> {
    session: &'a Session,
}
impl TtyPainter for LegacyPainter<'_> {
    fn begin(&mut self, _: usize, _: usize) -> io::Result<()> {
        crossterm::cursor::Hide.execute_winapi()
    }
    fn row(&mut self, row: usize, cells: &[TtyCell]) -> io::Result<()> {
        self.session.move_to(row, 0)?;
        let console = self.session.console();
        let mut last = None;
        let mut text = String::new();
        for cell in cells.iter().filter(|cell| !cell.padding) {
            let attributes = legacy_attributes(&cell.attrs, self.session.original_attributes);
            if last != Some(attributes) {
                if !text.is_empty() {
                    console.write_char_buffer(text.as_bytes())?;
                    text.clear();
                }
                console.set_text_attribute(attributes)?;
                last = Some(attributes);
            }
            text.push(cell.ch);
            if let Some(extenders) = &cell.extenders {
                text.push_str(extenders);
            }
        }
        if !text.is_empty() {
            console.write_char_buffer(text.as_bytes())?;
        }
        Ok(())
    }
    fn finish(&mut self, cursor: Option<(u16, u16, TerminalCursorShape)>) -> io::Result<()> {
        self.session
            .console()
            .set_text_attribute(self.session.original_attributes)?;
        if let Some((row, col, _)) = cursor {
            self.session.move_to(usize::from(row), usize::from(col))?;
            crossterm::cursor::Show.execute_winapi()?;
        }
        Ok(())
    }
}

fn legacy_attributes(attrs: &CellAttrs, default: u16) -> u16 {
    // Neomacs' realized indexed palette is ANSI ordered; the native console
    // uses BGR bits. Invalid realized indices retain the original default.
    let bits = |color: neomacs_display_protocol::TerminalColor| {
        let index = color.realized_pixel();
        (index < 16).then(|| {
            let index = index as u16;
            (index & 2) | ((index & 1) << 2) | ((index & 4) >> 2) | (index & 8)
        })
    };
    // GNU reverses defaults first. Explicit foreground/background colors have
    // already been reversed during face realization and must not swap again.
    let mut foreground = default & 15;
    let mut background = (default >> 4) & 15;
    if attrs.inverse {
        std::mem::swap(&mut foreground, &mut background);
    }
    if let Some(color) = attrs.fg.and_then(bits) {
        foreground = color;
    }
    if let Some(color) = attrs.bg.and_then(bits) {
        background = color;
    }
    foreground | (background << 4)
}

pub(crate) fn popup_line(row: usize, col: usize, text: &str) -> io::Result<()> {
    with_session(|session| {
        if session.vt {
            session.console().write_char_buffer(
                format!("\x1b[{};{}H\x1b[7m{text}\x1b[0m", row + 1, col + 1).as_bytes(),
            )?;
        } else {
            session.move_to(row, col)?;
            let attrs = CellAttrs {
                inverse: true,
                ..CellAttrs::default()
            };
            session
                .console()
                .set_text_attribute(legacy_attributes(&attrs, session.original_attributes))?;
            let result = session.console().write_char_buffer(text.as_bytes());
            let restored = session
                .console()
                .set_text_attribute(session.original_attributes);
            result?;
            restored?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
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
        session.vt = false; // Exercise the fallback even on modern Windows.
        ConsoleMode::from(session.original.handle().clone())
            .set_mode(session.original_mode)
            .unwrap();
        session.enter().unwrap();
        assert!(session.alternate.is_some());
        let mut painter = LegacyPainter { session: &session };
        painter.begin(80, 24).unwrap();
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
        assert_eq!(restored.info().unwrap().cursor_pos(), original.cursor_pos());
        assert_eq!(restored.info().unwrap().attributes(), original.attributes());
        assert_eq!(
            ConsoleMode::from(restored.handle().clone()).mode().unwrap(),
            session.original_mode
        );
    }
}
