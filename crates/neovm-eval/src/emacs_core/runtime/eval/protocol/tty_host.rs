//! The cold TTY host channel owned by the evaluator layer
//! (stage-4 W2 E5).
//!
//! The evaluator owns frame/terminal identity; the binary owns file
//! descriptors, raw mode, input threads and the renderer bound to them.
//! These request/result records and the two cold host traits carry only
//! value/display-protocol vocabulary and an evaluator Context reference, so
//! the evaluator can hold the one factory channel without naming a display
//! type. The display terminal module re-exports every item.

use std::num::NonZeroU32;

use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use neovm_value::display_vocab::FrameId;

/// GNU's initial terminal id (`initial_terminal`).
pub const TERMINAL_ID: u64 = 0;

use super::super::Context;

/// Evaluator operations invoked by [`TerminalHost::run_emergency_escape`]
/// during GNU's controlling-TTY 2nd-`C-g` prompts (`src/keyboard.c:12667-12702`).
pub trait EmergencyEscapeOps {
    /// GNU `Fdo_auto_save (Qt, Qnil)` for `"Auto-save? (y or n) "`.
    fn auto_save(&mut self);

    /// GNU `emacs_abort ()` -> `terminate_due_to_signal (SIGABRT, 40)` for
    /// `"Abort (and dump core)? (y or n) "` (`src/sysdep.c:2401`,
    /// `src/emacs.c:441-483`, `shut_down_emacs` `:3104-3160`).
    fn abort_orderly(&mut self) -> !;
}

pub trait TerminalHost {
    fn suspend_tty(&mut self, reset_strings: &[u8]) -> Result<(), String>;
    fn resume_tty(&mut self, set_strings: &[u8]) -> Result<(), String>;
    fn delete_terminal(&mut self, _reset_strings: &[u8]) -> Result<(), String> {
        Ok(())
    }
    /// Stop the process and yield the controlling TTY to its superior
    /// (GNU `sys_suspend`, `src/sysdep.c:629`).
    ///
    /// Only the primary interactive TTY host stops the process; test and
    /// secondary hosts leave this as a no-op so unit tests never send
    /// `SIGTSTP` to the test runner.
    fn sys_suspend(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// GNU `Fsend_string_to_terminal`'s `fwrite`+`fflush` on the terminal's
    /// output stream (src/dispnew.c:6838-6843).  The host owns the fd, so the
    /// write and its flush belong behind this method.  A host that owns no
    /// output stream rejects; the caller distinguishes GNU's "not a termcap
    /// terminal device" from "Terminal is currently suspended" using the
    /// terminal record, not this error.
    fn write_bytes(&mut self, _bytes: &[u8]) -> Result<(), String> {
        Err("terminal host has no output stream".to_owned())
    }

    /// Current winsize of this host's own input, GNU `get_tty_size` on
    /// `FRAME_TTY(f)->input` (`src/sysdep.c:1396`).
    ///
    /// `Some((0, 0))` is a failed ioctl or a successful 0×0 report. `None`
    /// means this host has no input fd, so the caller keeps the size stored
    /// on the terminal when it was opened.
    fn tty_winsize_cells(&mut self) -> Option<(u32, u32)> {
        None
    }

    /// GNU `handle_interrupt` controlling-TTY emergency escape
    /// (`src/keyboard.c:12622-12711`): restore terminal modes, suspend with
    /// `SIGTSTP`, and on resume prompt for auto-save / abort / continue before
    /// re-entering raw mode.
    ///
    /// Returns `true` when this host owns the interactive controlling TTY and
    /// handled the emergency escape; returns `false` when the host does not
    /// support emergency escape so the evaluator falls back to GNU's
    /// `force_quit_count` path (`src/keyboard.c:12712-12720`).
    fn run_emergency_escape(&mut self, _ops: &mut dyn EmergencyEscapeOps) -> bool {
        false
    }
}
/// Validated identity of a text terminal a frontend must open for one frame.
///
/// The Lisp-facing `tty` and `tty-type` parameters are loose values; this is
/// the narrow, owned request that crosses from the VM into platform code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyFrameOpenRequest {
    pub terminal_id: u64,
    pub frame_id: FrameId,
    pub device: String,
    pub terminal_type: String,
}

impl TtyFrameOpenRequest {
    pub fn new(
        terminal_id: u64,
        frame_id: FrameId,
        device: String,
        terminal_type: String,
    ) -> Result<Self, String> {
        if device.is_empty() {
            return Err("Invalid terminal device".to_string());
        }
        if terminal_type.is_empty() {
            return Err("Invalid terminal type".to_string());
        }
        Ok(Self {
            terminal_id,
            frame_id,
            device,
            terminal_type,
        })
    }

    pub fn terminal_id(&self) -> u64 {
        self.terminal_id
    }

    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn terminal_type(&self) -> &str {
        &self.terminal_type
    }
}

/// Character-cell dimensions of an opened TTY. Zero-sized terminals cannot
/// enter the frame model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtyFrameSize {
    columns: NonZeroU32,
    rows: NonZeroU32,
}

impl TtyFrameSize {
    pub fn new(columns: u32, rows: u32) -> Option<Self> {
        Some(Self {
            columns: NonZeroU32::new(columns)?,
            rows: NonZeroU32::new(rows)?,
        })
    }

    pub fn columns(self) -> u32 {
        self.columns.get()
    }

    pub fn rows(self) -> u32 {
        self.rows.get()
    }
}

/// Resources returned only after platform code has successfully opened and
/// initialized a TTY.
pub struct OpenedTtyFrameHost {
    pub size: TtyFrameSize,
    pub attribute_capabilities: TtyAttributeCapabilities,
    /// The ERASE byte of the modes this terminal had before raw mode was
    /// entered.  The host reads it at open because only the host owns the
    /// descriptor; the evaluator publishes it as `tty-erase-char`, as GNU's
    /// `init_sys_modes` does for every terminal it initializes
    /// (src/sysdep.c:1130) — a daemon must not keep the answer it read from
    /// its own stdin.
    pub erase_char: u8,
    pub host: Box<dyn TerminalHost>,
}

impl OpenedTtyFrameHost {
    pub fn new(
        size: TtyFrameSize,
        attribute_capabilities: TtyAttributeCapabilities,
        erase_char: u8,
        host: Box<dyn TerminalHost>,
    ) -> Self {
        Self {
            size,
            attribute_capabilities,
            erase_char,
            host,
        }
    }

    pub fn erase_char(&self) -> u8 {
        self.erase_char
    }
}

/// Frontend-owned factory for OS terminal resources.
///
/// `neovm-core` owns Lisp/frame/terminal identity; the binary owns file
/// descriptors, raw mode, input threads, and the renderer bound to them.
pub trait TtyFrameHostFactory {
    fn open_tty(&mut self, request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String>;

    /// Take the one-shot installer for batch redisplay, if the frontend deferred it.
    ///
    /// GNU batch skips `init_display` and acquires a terminal only inside
    /// `init_tty` (`src/frame.c:1811`, from `Fmake_terminal_frame` at
    /// `:1754-1823`). Until that open succeeds there is no TTY to redisplay,
    /// and a TTY frame never loads a system font database (`src/frame.c:1184-1185`).
    /// The default keeps interactive and test factories unchanged.
    fn take_deferred_redisplay(&mut self) -> Option<Box<dyn FnOnce(&mut Context)>> {
        None
    }
}
