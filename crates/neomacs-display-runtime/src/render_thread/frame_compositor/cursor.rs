//! Cursor visuals owned by the compositor.
//!
//! Blink, motion, size transitions and the click halo are renderer-owned
//! visual state layered over the immutable editor scene; none of them changes
//! the cursor position the evaluator committed.
//!
//! Paths here are absolute on purpose: `super::cursor` inside this module
//! would resolve to this file, not to `render_thread::cursor`.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::render_thread::cursor::{CursorState, CursorTarget};
use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_renderer_wgpu::WgpuRenderer;

impl GuiFrameRenderState {
    pub(in crate::render_thread) fn tick_cursor_animation(&mut self) -> bool {
        let mut dirty = self.cursor.tick_animation();
        for cursor in self.compositor.visual_cursors.values_mut() {
            dirty |= cursor.tick_animation();
        }
        if dirty {
            self.compositor.dirty = true;
        }
        dirty
    }

    pub(in crate::render_thread) fn tick_cursor_blink(
        &mut self,
        now: Instant,
        cursor_wake_enabled: bool,
        renderer: Option<&WgpuRenderer>,
    ) -> bool {
        if !self.cursor.blink_enabled || self.cursor.target_cloned().is_none() {
            return false;
        }
        if now.duration_since(self.cursor.last_blink_toggle) < self.cursor.blink_interval {
            return false;
        }
        let was_off = !self.cursor.blink_on;
        self.cursor.blink_on = !self.cursor.blink_on;
        self.cursor.last_blink_toggle = now;
        if was_off
            && self.cursor.blink_on
            && cursor_wake_enabled
            && let Some(renderer) = renderer
        {
            renderer.trigger_transient_cursor_wake(&mut self.compositor.renderer_effects, now);
        }
        // A blink changes the cursor layer and nothing else, so it asks for a
        // composite of the retained scene rather than a repaint. When the
        // toggle also triggers the cursor-wake effect above, that effect marks
        // the window dirty through mark_active_visuals_dirty on this same
        // about_to_wait pass, which outranks this and forces the full render
        // it needs.
        self.compositor.cursor_dirty = true;
        true
    }

    pub(in crate::render_thread) fn force_cursor_blink_on(&mut self) -> bool {
        if !self.cursor.force_blink_on() {
            return false;
        }
        self.compositor.cursor_dirty = true;
        true
    }

    pub(in crate::render_thread) fn mark_active_visuals_dirty(&mut self) -> bool {
        if !self.compositor.renderer_effects.needs_redraw()
            && !self.compositor.transitions.has_active()
        {
            return false;
        }
        self.compositor.dirty = true;
        true
    }

    pub(in crate::render_thread) fn trigger_click_halo(
        &mut self,
        x: f32,
        y: f32,
        now: Instant,
        duration_ms: u32,
    ) {
        self.compositor
            .renderer_effects
            .trigger_click_halo(x, y, now, duration_ms);
        self.compositor.dirty = true;
    }

    pub(in crate::render_thread) fn tick_cursor_size_animation(&mut self) -> bool {
        let mut dirty = self.cursor.tick_size_animation();
        for cursor in self.compositor.visual_cursors.values_mut() {
            dirty |= cursor.tick_size_animation();
        }
        if dirty {
            self.compositor.dirty = true;
        }
        dirty
    }

    pub(in crate::render_thread) fn sync_visual_cursors_from_current_frame(
        &mut self,
        cursor_config: impl Fn(&mut CursorState),
    ) {
        let Some(current_frame) = self.compositor.current_frame.as_ref() else {
            self.compositor.visual_cursors.clear();
            return;
        };
        let mut live_visual_cursor_ids = HashSet::new();
        for cursor in &current_frame.window_cursors {
            if cursor.window_id.get() >= 0 {
                continue;
            }
            live_visual_cursor_ids.insert(cursor.window_id.get());
            let state = self
                .compositor
                .visual_cursors
                .entry(cursor.window_id.get())
                .or_default();
            cursor_config(state);
            let (_, target_moved) = state.set_target(CursorTarget {
                window_id: cursor.window_id.get(),
                x: cursor.x,
                y: cursor.y,
                width: cursor.width,
                height: cursor.height,
                style: cursor.style,
                frame_id: self.emacs_frame_id,
            });
            if target_moved {
                self.compositor.dirty = true;
            }
        }
        self.compositor
            .visual_cursors
            .retain(|id, _| live_visual_cursor_ids.contains(id));
    }

    pub(in crate::render_thread) fn sync_cursor_config(
        &mut self,
        defaults: &CursorState,
        dirty: bool,
    ) {
        let config = defaults.config_snapshot();
        self.cursor.apply_config(config);
        for cursor in self.compositor.visual_cursors.values_mut() {
            cursor.apply_config(config);
        }
        if dirty {
            self.compositor.dirty = true;
        }
    }

    pub(in crate::render_thread) fn apply_visual_cursor_animations(&mut self) {
        if self.compositor.visual_cursors.is_empty() {
            return;
        }
        let visual_cursor_rects: HashMap<i64, (f32, f32, f32, f32)> = self
            .compositor
            .visual_cursors
            .iter()
            .map(|(id, state)| {
                (
                    *id,
                    (
                        state.current_x,
                        state.current_y,
                        state.current_w,
                        state.current_h,
                    ),
                )
            })
            .collect();
        let Some(frame) = self.compositor.current_frame.as_mut() else {
            return;
        };
        for cursor in &mut frame.window_cursors {
            let Some((x, y, width, height)) = visual_cursor_rects.get(&cursor.window_id.get())
            else {
                continue;
            };
            cursor.x = *x;
            cursor.y = *y;
            cursor.width = *width;
            cursor.height = *height;
        }
    }
}
