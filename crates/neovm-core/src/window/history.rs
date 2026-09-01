use super::{FrameManager, Window, WindowId};
use crate::emacs_core::value::Value;

impl FrameManager {
    fn live_window_history(&self, window_id: WindowId) -> Option<&super::WindowHistoryState> {
        self.frames
            .values()
            .find_map(|frame| frame.find_window(window_id).and_then(Window::history))
    }

    fn live_window_history_mut(
        &mut self,
        window_id: WindowId,
    ) -> Option<&mut super::WindowHistoryState> {
        let frame_id = self.find_window_frame_id(window_id)?;
        self.get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .and_then(Window::history_mut)
    }

    /// Return previous-buffer list object for WINDOW-ID, or nil when unset.
    pub fn window_prev_buffers(&self, window_id: WindowId) -> Value {
        self.live_window_history(window_id)
            .map(|history| history.prev_buffers)
            .unwrap_or(Value::NIL)
    }

    /// Set previous-buffer list object for WINDOW-ID.
    pub fn set_window_prev_buffers(&mut self, window_id: WindowId, prev_buffers: Value) {
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.prev_buffers = prev_buffers;
        }
    }

    /// Return next-buffer list object for WINDOW-ID, or nil when unset.
    pub fn window_next_buffers(&self, window_id: WindowId) -> Value {
        self.live_window_history(window_id)
            .map(|history| history.next_buffers)
            .unwrap_or(Value::NIL)
    }

    /// Set next-buffer list object for WINDOW-ID.
    pub fn set_window_next_buffers(&mut self, window_id: WindowId, next_buffers: Value) {
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.next_buffers = next_buffers;
        }
    }

    /// Return the use-time for WINDOW-ID.
    pub fn window_use_time(&self, window_id: WindowId) -> i64 {
        self.live_window_history(window_id)
            .map(|history| history.use_time)
            .unwrap_or(0)
    }

    /// Mark WINDOW-ID as the most recently selected window.
    pub fn note_window_selected(&mut self, window_id: WindowId) -> i64 {
        self.window_select_count = self.window_select_count.saturating_add(1);
        let next_use_time = self.window_select_count;
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.use_time = next_use_time;
        }
        next_use_time
    }

    /// Mark WINDOW-ID as second-most recently used.
    ///
    /// Returns the new use-time of WINDOW-ID when the bump happened, nil-like
    /// behavior (`None`) otherwise.
    pub fn bump_window_use_time(
        &mut self,
        selected_window_id: WindowId,
        window_id: WindowId,
    ) -> Option<i64> {
        if window_id == selected_window_id {
            return None;
        }
        if self.window_use_time(selected_window_id) != self.window_select_count {
            return None;
        }

        let bumped_use_time = self.window_select_count;
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.use_time = bumped_use_time;
        }
        self.window_select_count = self.window_select_count.saturating_add(1);
        let selected_use_time = self.window_select_count;
        if let Some(history) = self.live_window_history_mut(selected_window_id) {
            history.use_time = selected_use_time;
        }
        Some(bumped_use_time)
    }

    /// Return the old selected window, when tracked.
    pub fn old_selected_window(&self) -> Option<WindowId> {
        self.old_selected_window
    }
}
