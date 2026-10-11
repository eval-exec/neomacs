//! Transactional evaluator context for frame-owned display Lisp.
//!
//! GNU redisplay temporarily makes the frame being redisplayed, its selected
//! window, and that window's buffer current while evaluating frame chrome.
//! Keep that policy at one non-mirror machinery seam so menu/tab/tool clients
//! cannot independently forget part of the dynamic context or its restoration.

use crate::buffer::BufferId;
use crate::emacs_core::Context;
use crate::window::{FrameId, WindowId};

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
        let count = self.specpdl.len();
        let mut guard = FrameDisplayContextGuard {
            context: self,
            saved_buffer_id,
            saved_window_selection: None,
            count,
        };
        if guard
            .context
            .set_current_buffer_unrecorded(target_buffer_id)
            .is_err()
        {
            // The Drop restores the caller's buffer; nothing was selected.
            return None;
        }
        guard.saved_window_selection = Some(
            guard
                .context
                .frame_manager_mut()
                .select_window_for_mode_line(window_id),
        );

        let result = operation(guard.context());
        Some(guard.finish(result))
    }
}

/// Owns the frame-display dynamic context: the caller's buffer, the mode-line
/// window selection, and the specpdl suffix `operation` pushes. Neither Send
/// nor Sync: it exclusively borrows one mutator's Context.
struct FrameDisplayContextGuard<'a> {
    context: &'a mut Context,
    saved_buffer_id: Option<BufferId>,
    saved_window_selection: Option<(Option<FrameId>, Option<(FrameId, WindowId)>)>,
    count: usize,
}

impl FrameDisplayContextGuard<'_> {
    fn context(&mut self) -> &mut Context {
        self.context
    }

    /// Normal exit: unwind what the operation pushed — watchers and unwind
    /// forms run inside the frame context, like GNU `unbind_to` — then
    /// restore the caller's window selection and buffer. A watcher's Rust
    /// panic falls through to `Drop`, which replays the remaining suffix
    /// storage-only.
    fn finish<R>(mut self, result: R) -> R {
        if self.context.specpdl.len() > self.count {
            self.context.unbind_to(self.count);
        }
        self.restore_context();
        result
    }

    fn restore_context(&mut self) {
        if let Some(selection) = self.saved_window_selection.take() {
            self.context
                .frame_manager_mut()
                .restore_selected_window_for_mode_line(selection);
        }
        if let Some(buffer_id) = self.saved_buffer_id.take() {
            self.context.restore_current_buffer_if_live(buffer_id);
        }
    }
}

impl Drop for FrameDisplayContextGuard<'_> {
    fn drop(&mut self) {
        // A no-op after `finish` drained the suffix; Rust-panic recovery
        // (P4.11) retires what a panicking cleanup left, storage-only.
        self.context.discard_specpdl_to(self.count);
        self.restore_context();
    }
}

#[cfg(test)]
#[path = "display_evaluation/tests/display_evaluation_test.rs"]
mod tests;
