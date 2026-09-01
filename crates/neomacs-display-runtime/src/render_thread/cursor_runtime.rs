use super::frame_windows::GuiFrameWindowState;
use super::{ImeCursorArea, RenderApp};
use crate::render_thread::cursor::CursorTarget;

impl RenderApp {
    fn ime_cursor_area_for_window_target(
        window_state: &GuiFrameWindowState,
        target: &CursorTarget,
    ) -> ImeCursorArea {
        let (ime_off_x, ime_off_y) = if target.frame_id != window_state.render.emacs_frame_id {
            window_state
                .render
                .compositor
                .child_frames
                .frames
                .get(&target.frame_id)
                .map(|e| (e.abs_x as f64, e.abs_y as f64))
                .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };

        let scale_factor = window_state.scale_factor();

        ImeCursorArea {
            x: ((target.x as f64 + ime_off_x) * scale_factor).round() as i32,
            y: ((target.y as f64 + target.height as f64 + ime_off_y) * scale_factor).round() as i32,
            width: ((target.width as f64 * scale_factor).max(1.0)).round() as u32,
            height: ((target.height as f64 * scale_factor).max(1.0)).round() as u32,
        }
    }

    /// Compute physical IME cursor rectangle for the current cursor target.
    pub(super) fn ime_cursor_area_for_target(&self, target: &CursorTarget) -> ImeCursorArea {
        // If cursor is in a child frame, offset by the child's absolute position.
        let (ime_off_x, ime_off_y) = if target.frame_id != 0 {
            self.frame_windows
                .primary_window()
                .expect("primary child frames")
                .render
                .compositor
                .child_frames
                .frames
                .get(&target.frame_id)
                .map(|e| (e.abs_x as f64, e.abs_y as f64))
                .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };
        let scale_factor = self
            .frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor());

        ImeCursorArea {
            x: ((target.x as f64 + ime_off_x) * scale_factor).round() as i32,
            y: ((target.y as f64 + target.height as f64 + ime_off_y) * scale_factor).round() as i32,
            width: ((target.width as f64 * scale_factor).max(1.0)).round() as u32,
            height: ((target.height as f64 * scale_factor).max(1.0)).round() as u32,
        }
    }

    /// Update IME cursor area only when IME is active and the rectangle changed.
    pub(super) fn update_ime_cursor_area_if_needed(&mut self, target: &CursorTarget) {
        if !self
            .frame_windows
            .primary_window()
            .is_some_and(|ws| ws.ime_enabled())
            && !self
                .frame_windows
                .primary_window()
                .is_some_and(|ws| ws.render.has_ime_preedit())
        {
            return;
        }
        let area = self.ime_cursor_area_for_target(target);
        let Some(window_state) = self.frame_windows.primary_window_mut() else {
            return;
        };
        window_state.update_ime_cursor_area(area);
    }

    /// Update a secondary frame window's IME cursor area when composition is active.
    pub(super) fn update_frame_window_ime_cursor_area_if_needed(
        window_state: &mut GuiFrameWindowState,
        target: &CursorTarget,
    ) {
        if !window_state.ime_enabled() && !window_state.render.has_ime_preedit() {
            return;
        }

        let area = Self::ime_cursor_area_for_window_target(window_state, target);
        window_state.update_ime_cursor_area(area);
    }
}
