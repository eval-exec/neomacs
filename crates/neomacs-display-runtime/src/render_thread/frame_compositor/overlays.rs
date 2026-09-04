//! Transient overlays owned by the compositor: popup menus, tooltips, the
//! visual bell, typing-speed and idle-dim state.
//!
//! Paths here are absolute on purpose, matching the sibling submodules.

use std::time::Instant;

use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_display_protocol::effect_config::IdleDimConfig;
use neomacs_renderer_wgpu::{PopupMenuState, TooltipState};

impl GuiFrameRenderState {
    pub(in crate::render_thread) fn set_popup_menu(&mut self, popup_menu: Option<PopupMenuState>) {
        if popup_menu.is_some() {
            self.update_presented_pointer_motion(None);
        }
        self.overlays.popup_menu = popup_menu;
        self.compositor.dirty = true;
    }

    pub(in crate::render_thread) fn set_tooltip(&mut self, tooltip: Option<TooltipState>) {
        self.overlays.tooltip = tooltip;
        self.compositor.dirty = true;
    }

    pub(in crate::render_thread) fn set_visual_bell_start(&mut self, start: Option<Instant>) {
        self.overlays.visual_bell_start = start;
        if start.is_some() {
            self.compositor.dirty = true;
        }
    }

    pub(in crate::render_thread) fn record_typing_keypress(&mut self, now: Instant) {
        self.overlays.typing_speed.key_press_times.push(now);
        self.compositor.dirty = true;
    }

    pub(in crate::render_thread) fn record_idle_activity(&mut self, now: Instant) {
        self.overlays.idle_dim.last_activity_time = now;
        self.compositor.dirty = true;
    }

    pub(in crate::render_thread) fn update_popup_hover(&mut self, x: f32, y: f32) -> bool {
        let Some(menu) = self.overlays.popup_menu.as_mut() else {
            return false;
        };
        let dirty = menu.update_hover_at(x, y);
        if dirty {
            self.compositor.dirty = true;
        }
        dirty
    }

    pub(in crate::render_thread) fn trigger_visual_bell(
        &mut self,
        cursor_error_pulse_enabled: bool,
        edge_snap_enabled: bool,
        edge_snap_duration_ms: u32,
        now: Instant,
    ) {
        self.overlays.visual_bell_start = Some(now);
        if cursor_error_pulse_enabled {
            self.compositor
                .renderer_effects
                .trigger_cursor_error_pulse(now);
        }
        if edge_snap_enabled {
            let selected_info = self.compositor.current_frame.as_ref().and_then(|frame| {
                frame
                    .window_infos
                    .iter()
                    .find(|info| info.selected && !info.is_minibuffer)
                    .cloned()
            });
            if let Some(info) = selected_info {
                let at_top = info.window_start <= 1;
                let at_bottom = info.window_end >= info.buffer_size;
                if at_top || at_bottom {
                    self.compositor.renderer_effects.trigger_edge_snap(
                        info.bounds,
                        info.mode_line_height,
                        at_top,
                        at_bottom,
                        now,
                        edge_snap_duration_ms,
                    );
                }
            }
        }
        self.compositor.dirty = true;
    }

    pub(in crate::render_thread) fn tick_idle_dim(&mut self, config: &IdleDimConfig) -> bool {
        let idle_time = self.overlays.idle_dim.last_activity_time.elapsed();
        let target_alpha = if idle_time >= config.delay {
            config.opacity
        } else {
            0.0
        };
        let diff = target_alpha - self.overlays.idle_dim.current_alpha;
        if diff.abs() > 0.001 {
            let fade_speed = if config.fade_duration.as_secs_f32() > 0.0 {
                1.0 / config.fade_duration.as_secs_f32() * 0.016
            } else {
                1.0
            };
            if diff > 0.0 {
                self.overlays.idle_dim.current_alpha = (self.overlays.idle_dim.current_alpha
                    + fade_speed * config.opacity)
                    .min(target_alpha);
            } else {
                self.overlays.idle_dim.current_alpha =
                    (self.overlays.idle_dim.current_alpha - fade_speed * config.opacity).max(0.0);
            }
            self.overlays.idle_dim.active = true;
            self.compositor.dirty = true;
            true
        } else if self.overlays.idle_dim.current_alpha > 0.001 {
            self.overlays.idle_dim.active = true;
            false
        } else {
            self.overlays.idle_dim.active = false;
            false
        }
    }

    pub(in crate::render_thread) fn clear_idle_dim(&mut self) {
        self.overlays.idle_dim.active = false;
        self.overlays.idle_dim.current_alpha = 0.0;
    }
}
