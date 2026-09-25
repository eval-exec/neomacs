//! P3.5 stage B: terminal output that costs what changed.
//!
//! - **B1, silent frames** (`NEOMACS_TTY_SILENT=off|on`). A frame whose plan
//!   has no operation and whose cursor did not move writes nothing at all;
//!   GNU writes 0 bytes on an idle redisplay. A frame where only the cursor
//!   moved writes only the cursor motion, and the cursor shape is re-sent
//!   only when it changed (re-sending it resets the blink in several
//!   terminals).
//!
//! Every knob is read once per process, when the first [`TtyRif`] is built,
//! and defaults to the old behaviour until measured (same-binary A/B).

use super::*;
use std::sync::OnceLock;

/// The value `NEOMACS_TTY_SILENT` selects.
pub fn parse_tty_silent_knob(value: Option<&str>) -> bool {
    matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("on" | "1" | "true" | "yes")
    )
}

pub(super) fn knob_silent_frames() -> bool {
    static SILENT: OnceLock<bool> = OnceLock::new();
    *SILENT
        .get_or_init(|| parse_tty_silent_knob(std::env::var("NEOMACS_TTY_SILENT").ok().as_deref()))
}

/// Where the terminal's cursor is known to be after the last frame written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmittedCursor {
    /// Nothing is known (no frame yet, or a forced full repaint is due).
    Unknown,
    Hidden,
    Shown {
        row: u16,
        col: u16,
    },
}

/// Per-[`TtyRif`] state of the stage-B knobs.
#[derive(Clone, Debug)]
pub(super) struct DamageState {
    /// B1 is on for this renderer.
    pub(super) silent: bool,
    /// The cursor the terminal shows after the last frame written (B1).
    pub(super) cursor: EmittedCursor,
    /// The cursor shape last written, if known (B1).
    pub(super) shape: Option<TerminalCursorShape>,
}

impl DamageState {
    pub(super) fn from_knobs() -> Self {
        Self {
            silent: knob_silent_frames(),
            cursor: EmittedCursor::Unknown,
            shape: None,
        }
    }

    /// Forget what the terminal shows: the next frame writes it all.
    pub(super) fn forget_terminal(&mut self) {
        self.cursor = EmittedCursor::Unknown;
        self.shape = None;
    }
}

impl TtyRif {
    /// Turn B1 silent frames on or off for this renderer, overriding
    /// `NEOMACS_TTY_SILENT` (tests and embedders).
    pub fn set_silent_frames(&mut self, silent: bool) {
        self.damage.silent = silent;
        self.damage.forget_terminal();
    }

    /// The cursor this frame leaves on the terminal.
    fn frame_cursor(&self) -> EmittedCursor {
        if self.cursor_visible {
            EmittedCursor::Shown {
                row: self.cursor_row,
                col: self.cursor_col,
            }
        } else {
            EmittedCursor::Hidden
        }
    }

    /// B1: write a frame whose plan is empty. Returns false when the frame
    /// needs the ordinary framing (B1 off, or a forced repaint).
    ///
    /// Nothing is written when the cursor is where the terminal already shows
    /// it; otherwise only the cursor motion is written: no synchronized-update
    /// bracket and no hide/show pair around a frame that writes no cell.
    pub(super) fn write_quiet_frame(&mut self, ops: &[TermOp]) -> bool {
        if !self.damage.silent || !ops.is_empty() || self.force_full_render {
            return false;
        }
        let now = self.frame_cursor();
        let before = self.damage.cursor;
        match now {
            EmittedCursor::Shown { row, col } => {
                if before != now {
                    write_cursor_goto(&mut self.output, row + 1, col + 1);
                }
                if self.damage.shape != Some(self.cursor_shape) {
                    write_cursor_shape(&mut self.output, self.cursor_shape);
                    self.damage.shape = Some(self.cursor_shape);
                }
                if !matches!(before, EmittedCursor::Shown { .. }) {
                    self.output.extend_from_slice(b"\x1b[?25h");
                }
            }
            EmittedCursor::Hidden => {
                if before != EmittedCursor::Hidden {
                    self.output.extend_from_slice(b"\x1b[?25l");
                }
            }
            EmittedCursor::Unknown => unreachable!("a frame always knows its cursor"),
        }
        self.damage.cursor = now;
        true
    }

    /// B1 for a frame that writes cells: the cursor shape is re-sent only when
    /// it changed. With B1 off it is sent every frame, as before.
    pub(super) fn cursor_shape_due(&self) -> bool {
        !self.damage.silent || self.damage.shape != Some(self.cursor_shape)
    }

    /// Record the cursor an ordinarily framed frame left on the terminal.
    pub(super) fn note_framed_cursor(&mut self, shape_written: bool) {
        self.damage.cursor = self.frame_cursor();
        if shape_written {
            self.damage.shape = Some(self.cursor_shape);
        }
    }
}

#[cfg(test)]
#[path = "damage/tests/damage_test.rs"]
mod tests;
