//! Transactional evaluator context for frame-owned display Lisp.
//!
//! GNU redisplay temporarily makes the frame being redisplayed, its selected
//! window, and that window's buffer current while evaluating frame chrome.
//! Keep that policy at one non-mirror machinery seam so menu/tab/tool clients
//! cannot independently forget part of the dynamic context or its restoration.

use crate::emacs_core::Context;
use crate::window::FrameId;

impl Context {
    /// Evaluate `operation` as the selected window of `frame_id`, then restore
    /// the caller's selected frame/window and current buffer.
    ///
    /// Returns `None` without running `operation` when the target frame,
    /// selected leaf, or its buffer is no longer live. Lisp nonlocal exits are
    /// values in the callback's return type, so they cross this scope only
    /// after restoration.
    pub fn with_frame_display_context<R>(
        &mut self,
        frame_id: FrameId,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (window_id, target_buffer_id) = {
            let frame = self.frame_manager().get(frame_id)?;
            let window = frame.selected_window()?;
            (window.id(), window.buffer_id()?)
        };
        let saved_buffer_id = self.buffer_manager().current_buffer_id();
        if self
            .set_current_buffer_unrecorded(target_buffer_id)
            .is_err()
        {
            if let Some(buffer_id) = saved_buffer_id {
                self.restore_current_buffer_if_live(buffer_id);
            }
            return None;
        }
        let saved_window_selection = self
            .frame_manager_mut()
            .select_window_for_mode_line(window_id);

        let result = operation(self);

        self.frame_manager_mut()
            .restore_selected_window_for_mode_line(saved_window_selection);
        if let Some(buffer_id) = saved_buffer_id {
            self.restore_current_buffer_if_live(buffer_id);
        }
        Some(result)
    }
}

#[cfg(test)]
#[path = "display_evaluation_test.rs"]
mod tests;
