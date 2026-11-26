//! Numeric cursor output belongs to the leaf attempt that produced it.
use neovm_core::buffer::BufferId;
use neovm_core::emacs_core::Context;
use neovm_core::window::{
    FrameId, WindowCursorKind, WindowCursorPos, WindowCursorSnapshot, WindowId,
};

/// Preserve accepted cursor facts while a leaf emits provisional progress.
/// Point/start changes and cursor configuration belong to Lisp, so they are
/// deliberately excluded. A hook that switches the buffer owns the new state.
pub(super) struct LeafCursorCheckpoint {
    buffer: BufferId,
    cursor: Option<WindowCursorPos>,
    physical: Option<WindowCursorSnapshot>,
    output: Option<WindowCursorPos>,
    physical_kind: WindowCursorKind,
    physical_on: bool,
    last_off: bool,
    last_vpos: i64,
}

impl LeafCursorCheckpoint {
    pub(super) fn capture(eval: &Context, frame: FrameId, window: WindowId) -> Option<Self> {
        let window = eval.frame_manager().get(frame)?.find_window(window)?;
        let buffer = window.buffer_id()?;
        let display = window.display()?;
        Some(Self {
            buffer,
            cursor: display.cursor,
            physical: display.phys_cursor.clone(),
            output: display.output_cursor,
            physical_kind: display.phys_cursor_type,
            physical_on: display.phys_cursor_on_p,
            last_off: display.last_cursor_off_p,
            last_vpos: display.last_cursor_vpos,
        })
    }

    pub(super) fn restore(self, eval: &mut Context, frame: FrameId, window: WindowId) {
        let Some(window) = eval
            .frame_manager_mut()
            .get_mut(frame)
            .and_then(|frame| frame.find_window_mut(window))
            .filter(|window| window.buffer_id() == Some(self.buffer))
        else {
            return;
        };
        let Some(display) = window.display_mut() else {
            return;
        };
        display.cursor = self.cursor;
        display.phys_cursor = self.physical;
        display.output_cursor = self.output;
        display.phys_cursor_type = self.physical_kind;
        display.phys_cursor_on_p = self.physical_on;
        display.last_cursor_off_p = self.last_off;
        display.last_cursor_vpos = self.last_vpos;
    }
}
