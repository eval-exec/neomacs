//! Effects State methods for WgpuRenderer.

#[cfg(test)]
use super::ModeLineFadeEntry;
use super::{
    ClickHaloEntry, EdgeGlowEntry, EdgeSnapEntry, LineAnimEntry, ScrollMomentumEntry,
    ScrollSpacingEntry, ScrollVelocityFadeEntry, SonarPingEntry, TextFadeEntry, WindowFadeEntry,
};
use super::{RendererFrameEffects, WgpuRenderer};
use neomacs_display_protocol::frame_time::{EventTime, observe_platform_now};
use neomacs_display_protocol::types::{Color, Rect};

impl WgpuRenderer {
    /// Update inactive window dim config
    pub fn set_inactive_dim_config(&mut self, enabled: bool, opacity: f32) {
        self.effects.inactive_dim.enabled = enabled;
        self.effects.inactive_dim.opacity = opacity;
    }

    /// Start a line animation for a window
    pub fn start_line_animation(
        &mut self,
        window_bounds: Rect,
        edit_y: f32,
        offset: f32,
        duration_ms: u32,
    ) {
        // Remove any existing animation for this window region
        self.fx.line_anim.active.retain(|a| {
            (a.window_bounds.x - window_bounds.x).abs() > 1.0
                || (a.window_bounds.y - window_bounds.y).abs() > 1.0
        });
        self.fx.line_anim.active.push(LineAnimEntry {
            window_bounds,
            edit_y,
            initial_offset: offset,
            // TRIGGER SIGNATURE: should take the `EventTime` of the edit that
            // caused the slide; with no time parameter it mints its own.
            started: observe_platform_now(),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
    }

    /// Compute Y offset for a glyph due to active line animations
    pub(super) fn line_y_offset(&self, gx: f32, gy: f32) -> f32 {
        let mut offset = 0.0;
        for anim in &self.fx.line_anim.active {
            let b = &anim.window_bounds;
            // Check if glyph is in this window and below the edit point
            if gx >= b.x
                && gx < b.x + b.width
                && gy >= b.y
                && gy < b.y + b.height
                && gy >= anim.edit_y
            {
                let elapsed = self.frame_sample.since_at_presentation(anim.started);
                let t = (elapsed.as_secs_f32() / anim.duration.as_secs_f32()).min(1.0);
                // Ease-out quadratic: t * (2 - t)
                let eased = t * (2.0 - t);
                offset += anim.initial_offset * (1.0 - eased);
            }
        }
        // Scroll line spacing accordion effect
        let now = self.frame_sample.presentation_time();
        for entry in &self.fx.scroll_spacing.active {
            let b = &entry.bounds;
            if gx >= b.x && gx < b.x + b.width && gy >= b.y && gy < b.y + b.height {
                let elapsed = now.saturating_since(entry.started).as_secs_f32();
                let total = entry.duration.as_secs_f32();
                if elapsed < total {
                    let progress = elapsed / total;
                    let decay = 1.0 - progress;
                    let decay = decay * decay;
                    let norm = ((gy - b.y) / b.height).clamp(0.0, 1.0);
                    let edge_factor = if entry.direction > 0 {
                        1.0 - norm
                    } else {
                        norm
                    };
                    offset += self.effects.scroll_line_spacing.max * decay * edge_factor;
                }
            }
        }
        offset
    }

    /// Trigger a cursor wake animation
    pub fn trigger_cursor_wake(&mut self, now: std::time::Instant) {
        self.primary_frame_effects_mut().trigger_cursor_wake(now);
    }

    /// Trigger edge snap indicator
    pub fn trigger_edge_snap(
        &mut self,
        bounds: Rect,
        mode_line_height: f32,
        at_top: bool,
        at_bottom: bool,
        now: std::time::Instant,
    ) {
        let duration_ms = self.effects.edge_snap.duration_ms;
        self.primary_frame_effects_mut().trigger_edge_snap(
            bounds,
            mode_line_height,
            at_top,
            at_bottom,
            now,
            duration_ms,
        );
    }

    /// Update typing heat map config
    pub fn set_typing_heatmap(
        &mut self,
        enabled: bool,
        color: (f32, f32, f32),
        fade_ms: u32,
        opacity: f32,
    ) {
        self.effects.typing_heatmap.enabled = enabled;
        self.effects.typing_heatmap.color = color;
        self.effects.typing_heatmap.fade_ms = fade_ms;
        self.effects.typing_heatmap.opacity = opacity;
        if !enabled {
            self.fx.typing_heatmap.entries.clear();
            self.fx.typing_heatmap.prev_cursor = None;
        }
    }

    /// Trigger click halo at position
    pub fn trigger_click_halo(&mut self, x: f32, y: f32, now: std::time::Instant) {
        let duration_ms = self.effects.click_halo.duration_ms;
        self.primary_frame_effects_mut()
            .trigger_click_halo(x, y, now, duration_ms);
    }

    /// Trigger scroll velocity fade for a window.
    ///
    /// `intensity` is a normalized `0.0..=1.0` strength, not a distance: how
    /// far the viewport moved is measured by the compositor, which knows the
    /// scale to normalize against.
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    pub fn trigger_scroll_velocity_fade(
        &mut self,
        window_id: i64,
        bounds: Rect,
        intensity: f32,
        now: std::time::Instant,
    ) {
        // Replace existing entry for this window
        self.fx
            .scroll_velocity
            .fades
            .retain(|e| e.window_id != window_id);
        self.fx.scroll_velocity.fades.push(ScrollVelocityFadeEntry {
            window_id,
            bounds,
            intensity: intensity.clamp(0.0, 1.0),
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(self.effects.scroll_velocity_fade.ms as u64),
        });
    }

    /// Trigger resize padding animation
    pub fn trigger_resize_padding(&mut self, now: std::time::Instant) {
        self.primary_frame_effects_mut().trigger_resize_padding(now);
    }

    /// Get current resize padding amount (eases from max to 0)
    pub(super) fn resize_padding_amount(&self) -> f32 {
        if let Some(started) = self.fx.resize_padding.started {
            let elapsed = self.frame_sample.since_at_presentation(started).as_millis() as f32;
            let duration = self.effects.resize_padding.duration_ms as f32;
            if elapsed >= duration {
                return 0.0;
            }
            let t = elapsed / duration;
            let ease = t * (2.0 - t); // quadratic ease-out
            self.effects.resize_padding.max * (1.0 - ease)
        } else {
            0.0
        }
    }

    /// Trigger a cursor error pulse
    pub fn trigger_cursor_error_pulse(&mut self, now: std::time::Instant) {
        self.primary_frame_effects_mut()
            .trigger_cursor_error_pulse(now);
    }

    pub fn trigger_transient_click_halo(
        &self,
        effects: &mut RendererFrameEffects,
        x: f32,
        y: f32,
        now: std::time::Instant,
    ) {
        effects.trigger_click_halo(x, y, now, self.effects.click_halo.duration_ms);
    }

    pub fn trigger_transient_edge_snap(
        &self,
        effects: &mut RendererFrameEffects,
        bounds: Rect,
        mode_line_height: f32,
        at_top: bool,
        at_bottom: bool,
        now: std::time::Instant,
    ) {
        effects.trigger_edge_snap(
            bounds,
            mode_line_height,
            at_top,
            at_bottom,
            now,
            self.effects.edge_snap.duration_ms,
        );
    }

    pub fn trigger_transient_cursor_error_pulse(
        &self,
        effects: &mut RendererFrameEffects,
        now: std::time::Instant,
    ) {
        effects.trigger_cursor_error_pulse(now);
    }

    pub fn trigger_transient_cursor_wake(
        &self,
        effects: &mut RendererFrameEffects,
        now: std::time::Instant,
    ) {
        effects.trigger_cursor_wake(now);
    }

    pub fn trigger_transient_resize_padding(
        &self,
        effects: &mut RendererFrameEffects,
        now: std::time::Instant,
    ) {
        effects.trigger_resize_padding(now);
    }

    pub fn spawn_transient_ripple(&self, effects: &mut RendererFrameEffects, cx: f32, cy: f32) {
        if self.effects.typing_ripple.enabled {
            effects.spawn_ripple(cx, cy);
        }
    }

    pub fn record_transient_cursor_trail(
        &self,
        effects: &mut RendererFrameEffects,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        if self.effects.cursor_trail_fade.enabled {
            effects.record_cursor_trail(x, y, w, h, self.effects.cursor_trail_fade.length);
        }
    }

    pub fn with_frame_effects<R>(
        &mut self,
        effects: &mut RendererFrameEffects,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let primary = self.take_frame_effects();
        self.apply_frame_effects(std::mem::take(effects));
        let result = f(self);
        *effects = self.take_frame_effects();
        self.apply_frame_effects(primary);
        result
    }

    fn primary_frame_effects_mut(&mut self) -> RendererFrameEffectsRef<'_> {
        RendererFrameEffectsRef { renderer: self }
    }

    fn take_frame_effects(&mut self) -> RendererFrameEffects {
        RendererFrameEffects {
            fx: std::mem::take(&mut self.fx),
            clocks: Some(self.clocks),
        }
    }

    fn apply_frame_effects(&mut self, effects: RendererFrameEffects) {
        let RendererFrameEffects { fx, clocks } = effects;
        self.fx = fx;
        // A fresh (default) RendererFrameEffects carries no clocks; keep the
        // renderer's current ones so animation phases stay continuous.
        if let Some(clocks) = clocks {
            self.clocks = clocks;
        }
    }

    /// Get the cursor error pulse color override, if active
    pub(super) fn cursor_error_pulse_override(&self) -> Option<Color> {
        if !self.effects.cursor_error_pulse.enabled {
            return None;
        }
        if let Some(started) = self.fx.error_pulse.started {
            let elapsed = self.frame_sample.since_at_presentation(started).as_millis() as f32;
            let duration = self.effects.cursor_error_pulse.duration_ms as f32;
            if elapsed >= duration {
                return None;
            }
            let t = elapsed / duration;
            // Flash: bright at start, fade out
            let alpha = (1.0 - t) * (1.0 - t);
            let (r, g, b) = self.effects.cursor_error_pulse.color;
            Some(Color::new(r, g, b, alpha))
        } else {
            None
        }
    }

    /// Trigger a scroll momentum indicator for a window
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    pub fn trigger_scroll_momentum(
        &mut self,
        window_id: i64,
        bounds: Rect,
        direction: i32,
        now: std::time::Instant,
    ) {
        self.fx
            .scroll_momentum
            .active
            .retain(|e| e.window_id != window_id);
        self.fx.scroll_momentum.active.push(ScrollMomentumEntry {
            window_id,
            bounds,
            direction,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(self.effects.scroll_momentum.fade_ms as u64),
        });
    }

    /// Update matrix rain config
    pub fn set_matrix_rain(
        &mut self,
        enabled: bool,
        color: (f32, f32, f32),
        column_count: u32,
        speed: f32,
        opacity: f32,
    ) {
        self.effects.matrix_rain.enabled = enabled;
        self.effects.matrix_rain.color = color;
        self.effects.matrix_rain.column_count = column_count;
        self.effects.matrix_rain.speed = speed;
        self.effects.matrix_rain.opacity = opacity;
        if !enabled {
            self.fx.matrix_rain.columns.clear();
        }
    }

    /// Update frost border config
    pub fn set_frost_border_effect(
        &mut self,
        enabled: bool,
        color: (f32, f32, f32),
        width: f32,
        opacity: f32,
    ) {
        self.effects.frost_border.enabled = enabled;
        self.effects.frost_border.color = color;
        self.effects.frost_border.width = width;
        self.effects.frost_border.opacity = opacity;
    }

    /// Trigger edge glow for a window (at_top = beginning-of-buffer)
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    pub fn trigger_edge_glow(
        &mut self,
        window_id: i64,
        bounds: Rect,
        at_top: bool,
        now: std::time::Instant,
    ) {
        self.fx
            .edge_glow
            .entries
            .retain(|e| e.window_id != window_id || e.at_top != at_top);
        self.fx.edge_glow.entries.push(EdgeGlowEntry {
            window_id,
            bounds,
            at_top,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(self.effects.edge_glow.fade_ms as u64),
        });
    }

    /// Trigger a sonar ping at cursor position
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    pub fn trigger_sonar_ping(&mut self, cx: f32, cy: f32, now: std::time::Instant) {
        self.fx.sonar_ping.entries.push(SonarPingEntry {
            cx,
            cy,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(
                self.effects.cursor_sonar_ping.duration_ms as u64,
            ),
        });
    }

    /// Get the mode-line transition alpha for a glyph at (x, y)
    pub(super) fn mode_line_fade_alpha(&self, gx: f32, gy: f32) -> f32 {
        if !self.effects.mode_line_transition.enabled || self.fx.mode_line_fade.active.is_empty() {
            return 1.0;
        }
        let now = self.frame_sample.presentation_time();
        for entry in &self.fx.mode_line_fade.active {
            if gx >= entry.bounds_x
                && gx < entry.bounds_x + entry.bounds_w
                && gy >= entry.mode_line_y
                && gy < entry.mode_line_y + entry.mode_line_h
            {
                let elapsed = now.saturating_since(entry.started).as_secs_f32();
                let total = entry.duration.as_secs_f32();
                if elapsed < total {
                    let t = elapsed / total;
                    return t; // linear fade-in
                }
            }
        }
        1.0
    }

    /// Trigger a text fade-in animation for a window
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    pub fn trigger_text_fade_in(&mut self, window_id: i64, bounds: Rect, now: std::time::Instant) {
        // Replace existing animation for this window
        self.fx
            .text_fade
            .active
            .retain(|e| e.window_id != window_id);
        self.fx.text_fade.active.push(TextFadeEntry {
            window_id,
            bounds,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(
                self.effects.text_fade_in.duration_ms as u64,
            ),
        });
    }

    /// Get the text fade-in alpha multiplier for a glyph at (x, y).
    /// Returns 1.0 if no fade is active, or 0.0-1.0 during fade-in.
    pub(super) fn text_fade_alpha(&self, gx: f32, gy: f32) -> f32 {
        if !self.effects.text_fade_in.enabled || self.fx.text_fade.active.is_empty() {
            return 1.0;
        }
        let now = self.frame_sample.presentation_time();
        for entry in &self.fx.text_fade.active {
            let b = &entry.bounds;
            if gx >= b.x && gx < b.x + b.width && gy >= b.y && gy < b.y + b.height {
                let elapsed = now.saturating_since(entry.started).as_secs_f32();
                let total = entry.duration.as_secs_f32();
                if elapsed < total {
                    // Ease-in: start at 0, end at 1
                    let t = elapsed / total;
                    return t * t; // quadratic ease-in for smooth appearance
                }
            }
        }
        1.0
    }

    /// Trigger a scroll line spacing animation for a window
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    pub fn trigger_scroll_line_spacing(
        &mut self,
        window_id: i64,
        bounds: Rect,
        direction: i32,
        now: std::time::Instant,
    ) {
        // Replace existing animation for this window
        self.fx
            .scroll_spacing
            .active
            .retain(|e| e.window_id != window_id);
        self.fx.scroll_spacing.active.push(ScrollSpacingEntry {
            window_id,
            bounds,
            direction,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(
                self.effects.scroll_line_spacing.duration_ms as u64,
            ),
        });
    }

    /// Record a new cursor position for the trail
    pub fn record_cursor_trail(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if !self.effects.cursor_trail_fade.enabled {
            return;
        }
        let length = self.effects.cursor_trail_fade.length;
        self.primary_frame_effects_mut()
            .record_cursor_trail(x, y, w, h, length);
    }

    /// Update idle dim alpha
    pub fn set_idle_dim_alpha(&mut self, alpha: f32) {
        self.fx.dim.idle_alpha = alpha;
    }

    /// Start a window switch fade for a specific window
    pub fn start_window_fade(&mut self, window_id: i64, bounds: Rect) {
        // Remove any existing fade for this window
        self.fx
            .window_fade
            .active
            .retain(|f| f.window_id != window_id);
        self.fx.window_fade.active.push(WindowFadeEntry {
            window_id,
            bounds,
            // TRIGGER SIGNATURE: should take the `EventTime` of the window
            // switch; with no time parameter it mints its own.
            started: observe_platform_now(),
            duration: std::time::Duration::from_millis(
                self.effects.window_switch_fade.duration_ms as u64,
            ),
            intensity: self.effects.window_switch_fade.intensity,
        });
    }

    /// Convert HSL to sRGB Color
    /// Scale a rectangle from its center by a given factor
    pub(super) fn scale_rect(x: f32, y: f32, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let nw = w * scale;
        let nh = h * scale;
        (cx - nw * 0.5, cy - nh * 0.5, nw, nh)
    }

    pub(super) fn hsl_to_color(h: f32, s: f32, l: f32) -> Color {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match (h * 6.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Color {
            r: r + m,
            g: g + m,
            b: b + m,
            a: 1.0,
        }
    }

    /// Spawn a new ripple at the given position
    pub fn spawn_ripple(&mut self, cx: f32, cy: f32) {
        if self.effects.typing_ripple.enabled {
            self.primary_frame_effects_mut().spawn_ripple(cx, cy);
        }
    }

    /// Update visible whitespace config
    pub fn set_show_whitespace_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.effects.show_whitespace.enabled = enabled;
        self.effects.show_whitespace.color = color;
    }

    /// Update line highlight config
    pub fn set_line_highlight_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.effects.line_highlight.enabled = enabled;
        self.effects.line_highlight.color = color;
    }
}

struct RendererFrameEffectsRef<'a> {
    renderer: &'a mut WgpuRenderer,
}

impl RendererFrameEffectsRef<'_> {
    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    fn trigger_click_halo(&mut self, x: f32, y: f32, now: std::time::Instant, duration_ms: u32) {
        self.renderer.fx.click_halo.halos.push(ClickHaloEntry {
            x,
            y,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    fn trigger_edge_snap(
        &mut self,
        bounds: Rect,
        mode_line_height: f32,
        at_top: bool,
        at_bottom: bool,
        now: std::time::Instant,
        duration_ms: u32,
    ) {
        self.renderer.fx.edge_snap.snaps.push(EdgeSnapEntry {
            bounds,
            mode_line_height,
            at_top,
            at_bottom,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    fn trigger_cursor_error_pulse(&mut self, now: std::time::Instant) {
        self.renderer.fx.error_pulse.started = Some(EventTime::from_observed_instant(now));
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    fn trigger_cursor_wake(&mut self, now: std::time::Instant) {
        self.renderer.fx.cursor_wake.started = Some(EventTime::from_observed_instant(now));
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`; it stays an
    // `Instant` only because `neomacs-display-runtime` still bridges through
    // `into_instant()` at the call site.
    fn trigger_resize_padding(&mut self, now: std::time::Instant) {
        self.renderer.fx.resize_padding.started = Some(EventTime::from_observed_instant(now));
    }

    fn spawn_ripple(&mut self, cx: f32, cy: f32) {
        self.renderer
            .fx
            .typing_ripple
            .active
            // TRIGGER SIGNATURE: should take the keypress's `EventTime`.
            .push((cx, cy, observe_platform_now()));
    }

    fn record_cursor_trail(&mut self, x: f32, y: f32, w: f32, h: f32, length: usize) {
        let trail = &mut self.renderer.fx.cursor_trail;
        let dist = ((x - trail.last_pos.0).powi(2) + (y - trail.last_pos.1).powi(2)).sqrt();
        if dist < 2.0 {
            return;
        }
        trail
            .positions
            // TRIGGER SIGNATURE: should take the cursor move's `EventTime`.
            .push((x, y, w, h, observe_platform_now()));
        trail.last_pos = (x, y);
        while trail.positions.len() > length {
            trail.positions.remove(0);
        }
    }
}

#[cfg(test)]
#[path = "effects_state_test.rs"]
mod tests;
