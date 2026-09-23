//! Windows output mode and screen-buffer ownership.
//! Crossterm owns handles; the cursor adapter uses Microsoft windows-sys bindings.
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

    fn screen(&self) -> &ScreenBuffer {
        self.alternate.as_ref().unwrap_or(&self.original)
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
        cursor_info::set(self.session.screen(), 99, false)
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
        if let Some((row, col, shape)) = cursor {
            self.session.move_to(usize::from(row), usize::from(col))?;
            // Legacy consoles specify horizontal cursor height, not a vertical
            // bar. Use GNU's nearly full-cell block for that fallback.
            let size = match shape {
                TerminalCursorShape::Underline => 25,
                TerminalCursorShape::Block | TerminalCursorShape::Bar => 99,
            };
            cursor_info::set(self.session.screen(), size, true)?;
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
#[path = "tty_output_windows/tests/tty_output_windows_test.rs"]
mod tests;

// Keep the two missing crossterm operations together. No raw handle escapes
// this adapter, and ScreenBuffer keeps it alive for each synchronous call.
mod cursor_info {
    use super::*;
    use windows_sys::Win32::System::Console::{CONSOLE_CURSOR_INFO, SetConsoleCursorInfo};

    pub(super) fn set(screen: &ScreenBuffer, size: u32, visible: bool) -> io::Result<()> {
        if !(1..=100).contains(&size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cursor height must be 1..=100",
            ));
        }
        let info = CONSOLE_CURSOR_INFO {
            dwSize: size,
            bVisible: i32::from(visible),
        };
        // SAFETY: the borrowed screen owns a live console handle. `info` is
        // initialized and lives through the call; Windows retains no pointer.
        if unsafe { SetConsoleCursorInfo((*screen.handle()).cast(), &info) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(super) fn get(screen: &ScreenBuffer) -> io::Result<(u32, bool)> {
        let mut info = CONSOLE_CURSOR_INFO {
            dwSize: 0,
            bVisible: 0,
        };
        // SAFETY: live borrowed handle and writable, correctly sized output.
        if unsafe {
            windows_sys::Win32::System::Console::GetConsoleCursorInfo(
                (*screen.handle()).cast(),
                &mut info,
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok((info.dwSize, info.bVisible != 0))
        }
    }
}
