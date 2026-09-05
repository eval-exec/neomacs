//! Cursor animation, blinking, and size transition state.

use crate::core::frame_glyphs::CursorStyle;
use crate::core::types::{
    AnimatedCursor, CursorAnimStyle, DisplayFrameId, DisplayWindowId, ease_in_out_cubic,
    ease_linear, ease_out_cubic, ease_out_expo, ease_out_quad,
};
use neomacs_display_protocol::VisualConfig;

/// Target position/style for cursor animation
#[derive(Debug, Clone)]
pub(super) struct CursorTarget {
    pub(super) window_id: i64,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) style: CursorStyle,
    /// Which frame owns this cursor (0 = root frame, non-zero = child frame_id)
    pub(super) frame_id: u64,
}

/// Per-corner spring state for the 4-corner cursor trail animation.
/// Each corner has its own position, velocity, and spring frequency.
#[derive(Debug, Clone, Copy)]
pub(super) struct CornerSpring {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) vx: f32,
    pub(super) vy: f32,
    pub(super) target_x: f32,
    pub(super) target_y: f32,
    pub(super) omega: f32,
}

/// Copyable cursor settings, excluding live animation and target state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CursorConfigSnapshot {
    blink_enabled: bool,
    blink_interval: std::time::Duration,
    anim_enabled: bool,
    anim_speed: f32,
    anim_style: CursorAnimStyle,
    anim_duration: f32,
    trail_size: f32,
    size_transition_enabled: bool,
    size_transition_duration: f32,
}

/// Cursor animation, blinking, and size transition state.
///
/// Extracted from RenderApp to group all cursor-related fields together.
pub(super) struct CursorState {
    // Blink state (managed by render thread)
    pub(super) blink_on: bool,
    pub(super) blink_enabled: bool,
    pub(super) last_blink_toggle: neomacs_display_protocol::frame_time::EventTime,
    pub(super) blink_interval: std::time::Duration,

    // Animation (smooth motion)
    pub(super) anim_enabled: bool,
    pub(super) anim_speed: f32,
    pub(super) anim_style: CursorAnimStyle,
    pub(super) anim_duration: f32, // seconds, for non-Exponential styles
    pub(super) target: Option<CursorTarget>,
    pub(super) current_x: f32,
    pub(super) current_y: f32,
    pub(super) current_w: f32,
    pub(super) current_h: f32,
    pub(super) animating: bool,
    pub(super) last_anim_time: neomacs_display_protocol::frame_time::EventTime,
    // For easing/linear styles: capture start position when animation begins
    pub(super) start_x: f32,
    pub(super) start_y: f32,
    pub(super) start_w: f32,
    pub(super) start_h: f32,
    pub(super) anim_start_time: neomacs_display_protocol::frame_time::EventTime,
    // 4-corner spring trail state (TL, TR, BR, BL)
    pub(super) corner_springs: [CornerSpring; 4],
    pub(super) trail_size: f32,
    // Previous target center for computing travel direction
    pub(super) prev_target_cx: f32,
    pub(super) prev_target_cy: f32,

    // Size transition (independent of position animation)
    pub(super) size_transition_enabled: bool,
    pub(super) size_transition_duration: f32, // seconds
    pub(super) size_animating: bool,
    pub(super) size_start_w: f32,
    pub(super) size_start_h: f32,
    pub(super) size_target_w: f32,
    pub(super) size_target_h: f32,
    pub(super) size_anim_start: neomacs_display_protocol::frame_time::EventTime,
}

impl CursorState {
    /// Cursor state for a window that has just been created.
    ///
    /// There is no `Default`: every timing anchor here needs a real moment,
    /// and the four `Instant::now()` calls this replaces could disagree with
    /// each other by however long construction took.
    pub(super) fn new(at: neomacs_display_protocol::frame_time::EventTime) -> Self {
        let visual = VisualConfig::default();
        Self {
            blink_on: true,
            blink_enabled: visual.cursor_blink.enabled,
            last_blink_toggle: at,
            blink_interval: visual.cursor_blink.interval,
            anim_enabled: visual.cursor_motion.enabled,
            anim_speed: visual.cursor_motion.speed,
            anim_style: visual.cursor_motion.style,
            anim_duration: visual.cursor_motion.duration.as_secs_f32(),
            target: None,
            current_x: 0.0,
            current_y: 0.0,
            current_w: 0.0,
            current_h: 0.0,
            animating: false,
            last_anim_time: at,
            start_x: 0.0,
            start_y: 0.0,
            start_w: 0.0,
            start_h: 0.0,
            anim_start_time: at,
            corner_springs: [CornerSpring {
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                target_x: 0.0,
                target_y: 0.0,
                omega: 26.7,
            }; 4],
            trail_size: visual.cursor_motion.trail_size,
            prev_target_cx: 0.0,
            prev_target_cy: 0.0,
            size_transition_enabled: visual.cursor_size_transition.enabled,
            size_transition_duration: visual.cursor_size_transition.duration.as_secs_f32(),
            size_animating: false,
            size_start_w: 0.0,
            size_start_h: 0.0,
            size_target_w: 0.0,
            size_target_h: 0.0,
            size_anim_start: at,
        }
    }
}

impl CursorState {
    pub(super) fn apply_visual_config(&mut self, config: &VisualConfig) {
        self.blink_enabled = config.cursor_blink.enabled;
        self.blink_interval = config.cursor_blink.interval;
        self.anim_enabled = config.cursor_motion.enabled;
        self.anim_speed = config.cursor_motion.speed;
        self.anim_style = config.cursor_motion.style;
        self.anim_duration = config.cursor_motion.duration.as_secs_f32();
        self.trail_size = config.cursor_motion.trail_size;
        self.size_transition_enabled = config.cursor_size_transition.enabled;
        self.size_transition_duration = config.cursor_size_transition.duration.as_secs_f32();
        if !self.anim_enabled {
            self.animating = false;
        }
        if !self.size_transition_enabled {
            self.size_animating = false;
        }
        if !self.blink_enabled {
            self.blink_on = true;
        }
    }

    pub(super) fn config_snapshot(&self) -> CursorConfigSnapshot {
        CursorConfigSnapshot {
            blink_enabled: self.blink_enabled,
            blink_interval: self.blink_interval,
            anim_enabled: self.anim_enabled,
            anim_speed: self.anim_speed,
            anim_style: self.anim_style,
            anim_duration: self.anim_duration,
            trail_size: self.trail_size,
            size_transition_enabled: self.size_transition_enabled,
            size_transition_duration: self.size_transition_duration,
        }
    }

    pub(super) fn apply_config(&mut self, config: CursorConfigSnapshot) {
        self.blink_enabled = config.blink_enabled;
        self.blink_interval = config.blink_interval;
        self.anim_enabled = config.anim_enabled;
        self.anim_speed = config.anim_speed;
        self.anim_style = config.anim_style;
        self.anim_duration = config.anim_duration;
        self.trail_size = config.trail_size;
        self.size_transition_enabled = config.size_transition_enabled;
        self.size_transition_duration = config.size_transition_duration;
        if !self.anim_enabled {
            self.animating = false;
        }
        if !self.size_transition_enabled {
            self.size_animating = false;
        }
    }

    pub(super) fn set_target(
        &mut self,
        new_target: CursorTarget,
        at: neomacs_display_protocol::frame_time::EventTime,
    ) -> (bool, bool) {
        let had_target = self.target.is_some();
        let target_moved = self.target.as_ref().is_none_or(|old| {
            (old.x - new_target.x).abs() > 0.5
                || (old.y - new_target.y).abs() > 0.5
                || (old.width - new_target.width).abs() > 0.5
                || (old.height - new_target.height).abs() > 0.5
        });

        if !had_target || !self.anim_enabled {
            self.current_x = new_target.x;
            self.current_y = new_target.y;
            self.current_w = new_target.width;
            self.current_h = new_target.height;
            self.animating = false;
            let corners = Self::target_corners(&new_target);
            for (spring, (x, y)) in self.corner_springs.iter_mut().zip(corners) {
                spring.x = x;
                spring.y = y;
                spring.vx = 0.0;
                spring.vy = 0.0;
                spring.target_x = x;
                spring.target_y = y;
            }
            self.prev_target_cx = new_target.x + new_target.width / 2.0;
            self.prev_target_cy = new_target.y + new_target.height / 2.0;
        } else if target_moved {
            self.animating = true;
            self.last_anim_time = at;
            self.start_x = self.current_x;
            self.start_y = self.current_y;
            self.start_w = self.current_w;
            self.start_h = self.current_h;
            self.anim_start_time = at;

            if self.anim_style == CursorAnimStyle::CriticallyDampedSpring {
                let new_corners = Self::target_corners(&new_target);
                let new_cx = new_target.x + new_target.width / 2.0;
                let new_cy = new_target.y + new_target.height / 2.0;
                let old_cx = self.prev_target_cx;
                let old_cy = self.prev_target_cy;

                let dx = new_cx - old_cx;
                let dy = new_cy - old_cy;
                let len = (dx * dx + dy * dy).sqrt();
                let (dir_x, dir_y) = if len > 0.001 {
                    (dx / len, dy / len)
                } else {
                    (1.0, 0.0)
                };

                let corner_dirs: [(f32, f32); 4] =
                    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

                let mut dots: [(f32, usize); 4] = corner_dirs
                    .iter()
                    .enumerate()
                    .map(|(i, (cx, cy))| (cx * dir_x + cy * dir_y, i))
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();
                dots.sort_by(|a, b| a.0.total_cmp(&b.0));

                let base_dur = self.anim_duration;
                for (rank, &(_dot, corner_idx)) in dots.iter().enumerate() {
                    let factor = 1.0 - self.trail_size * (rank as f32 / 3.0);
                    let duration_i = (base_dur * factor).max(0.01);
                    let omega_i = 4.0 / duration_i;

                    self.corner_springs[corner_idx].target_x = new_corners[corner_idx].0;
                    self.corner_springs[corner_idx].target_y = new_corners[corner_idx].1;
                    self.corner_springs[corner_idx].omega = omega_i;
                }

                self.prev_target_cx = new_cx;
                self.prev_target_cy = new_cy;
            }
        }

        if self.size_transition_enabled {
            let dw = (new_target.width - self.size_target_w).abs();
            let dh = (new_target.height - self.size_target_h).abs();
            if dw > 2.0 || dh > 2.0 {
                self.size_animating = true;
                self.size_start_w = self.current_w;
                self.size_start_h = self.current_h;
                self.size_anim_start = at;
            }
            self.size_target_w = new_target.width;
            self.size_target_h = new_target.height;
        }

        self.target = Some(new_target);
        (had_target, target_moved)
    }

    pub(super) fn animated_cursor(&self) -> Option<AnimatedCursor> {
        let target = self.target.as_ref()?;
        if !self.anim_enabled {
            return None;
        }
        let corners =
            if self.anim_style == CursorAnimStyle::CriticallyDampedSpring && self.animating {
                Some([
                    (self.corner_springs[0].x, self.corner_springs[0].y),
                    (self.corner_springs[1].x, self.corner_springs[1].y),
                    (self.corner_springs[2].x, self.corner_springs[2].y),
                    (self.corner_springs[3].x, self.corner_springs[3].y),
                ])
            } else {
                None
            };
        Some(AnimatedCursor {
            window_id: DisplayWindowId::new(target.window_id),
            x: self.current_x,
            y: self.current_y,
            width: self.current_w,
            height: self.current_h,
            corners,
            frame_id: DisplayFrameId::new(target.frame_id),
        })
    }

    pub(super) fn target_cloned(&self) -> Option<CursorTarget> {
        self.target.clone()
    }

    pub(super) fn clear_target(&mut self) {
        self.target = None;
        self.animating = false;
        self.size_animating = false;
    }

    pub(super) fn is_animating(&self) -> bool {
        self.animating || self.size_animating
    }

    pub(super) fn next_blink_deadline(
        &self,
    ) -> Option<neomacs_display_protocol::frame_time::EventTime> {
        (self.blink_enabled && self.target.is_some())
            .then_some(self.last_blink_toggle.plus(self.blink_interval))
    }

    /// Compute the 4 target corners for a cursor based on its style.
    /// Returns [TL, TR, BR, BL] as (x, y) tuples.
    pub(super) fn target_corners(target: &CursorTarget) -> [(f32, f32); 4] {
        match target.style {
            CursorStyle::FilledBox => {
                // Filled box: full rectangle
                let x0 = target.x;
                let y0 = target.y;
                let x1 = target.x + target.width;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            CursorStyle::Bar(bar_w) => {
                // Bar: thin vertical line
                let x0 = target.x;
                let y0 = target.y;
                let x1 = target.x + bar_w;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            CursorStyle::Hbar(hbar_h) => {
                // Underline: thin horizontal line at bottom
                let x0 = target.x;
                let y0 = target.y + target.height - hbar_h;
                let x1 = target.x + target.width;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            CursorStyle::Hollow => {
                // Hollow: full rectangle (border only drawn elsewhere)
                let x0 = target.x;
                let y0 = target.y;
                let x1 = target.x + target.width;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
        }
    }

    /// Tick cursor animation, returns true if position changed (needs redraw)
    pub(super) fn tick_animation(
        &mut self,
        at: neomacs_display_protocol::frame_time::EventTime,
    ) -> bool {
        if !self.anim_enabled || !self.animating {
            return false;
        }

        // The anchor belongs to the integrator, so it advances on every tick
        // this gate admits -- including one that finds no target. Leaving it
        // behind there would make the next tick with a target measure `dt`
        // across both intervals and take a doubled, visible step.
        let dt = at.saturating_since(self.last_anim_time).as_secs_f32();
        self.last_anim_time = at;

        let target = match self.target.as_ref() {
            Some(t) => t.clone(),
            None => return false,
        };

        match self.anim_style {
            CursorAnimStyle::Exponential => {
                let factor = 1.0 - (-self.anim_speed * dt).exp();
                let dx = target.x - self.current_x;
                let dy = target.y - self.current_y;
                let dw = target.width - self.current_w;
                let dh = target.height - self.current_h;
                self.current_x += dx * factor;
                self.current_y += dy * factor;
                self.current_w += dw * factor;
                self.current_h += dh * factor;
                if dx.abs() < 0.5 && dy.abs() < 0.5 && dw.abs() < 0.5 && dh.abs() < 0.5 {
                    self.snap(&target);
                }
            }
            CursorAnimStyle::CriticallyDampedSpring => {
                let mut all_settled = true;
                for i in 0..4 {
                    let spring = &mut self.corner_springs[i];
                    let omega = spring.omega;
                    let exp_term = (-omega * dt).exp();

                    let x0 = spring.x - spring.target_x;
                    let vx0 = spring.vx;
                    let new_x = (x0 + (vx0 + omega * x0) * dt) * exp_term;
                    spring.vx = ((vx0 + omega * x0) * exp_term)
                        - omega * (x0 + (vx0 + omega * x0) * dt) * exp_term;
                    spring.x = spring.target_x + new_x;

                    let y0 = spring.y - spring.target_y;
                    let vy0 = spring.vy;
                    let new_y = (y0 + (vy0 + omega * y0) * dt) * exp_term;
                    spring.vy = ((vy0 + omega * y0) * exp_term)
                        - omega * (y0 + (vy0 + omega * y0) * dt) * exp_term;
                    spring.y = spring.target_y + new_y;

                    let dist =
                        (spring.x - spring.target_x).abs() + (spring.y - spring.target_y).abs();
                    let vel = spring.vx.abs() + spring.vy.abs();
                    if dist > 0.5 || vel > 1.0 {
                        all_settled = false;
                    }
                }

                let min_x = self
                    .corner_springs
                    .iter()
                    .map(|s| s.x)
                    .fold(f32::INFINITY, f32::min);
                let min_y = self
                    .corner_springs
                    .iter()
                    .map(|s| s.y)
                    .fold(f32::INFINITY, f32::min);
                let max_x = self
                    .corner_springs
                    .iter()
                    .map(|s| s.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let max_y = self
                    .corner_springs
                    .iter()
                    .map(|s| s.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                self.current_x = min_x;
                self.current_y = min_y;
                self.current_w = max_x - min_x;
                self.current_h = max_y - min_y;

                if all_settled {
                    let target_corners = Self::target_corners(&target);
                    for (spring, corner) in
                        self.corner_springs.iter_mut().zip(target_corners.iter())
                    {
                        spring.x = corner.0;
                        spring.y = corner.1;
                    }
                    // Velocities are cleared by `snap`, which every style's
                    // completion path shares.
                    self.snap(&target);
                }
            }
            style => {
                let elapsed = at.saturating_since(self.anim_start_time).as_secs_f32();
                // A duration that is not positive means "no animation": finish
                // at once. Dividing by it would leave completion to float edge
                // cases -- 0.0/0.0 is NaN and only snaps because `f32::min`
                // discards NaN, and a negative duration would run backwards
                // and never reach 1.0 at all.
                let raw_t = if self.anim_duration > 0.0 {
                    (elapsed / self.anim_duration).min(1.0)
                } else {
                    1.0
                };
                let t = match style {
                    CursorAnimStyle::EaseOutQuad => ease_out_quad(raw_t),
                    CursorAnimStyle::EaseOutCubic => ease_out_cubic(raw_t),
                    CursorAnimStyle::EaseOutExpo => ease_out_expo(raw_t),
                    CursorAnimStyle::EaseInOutCubic => ease_in_out_cubic(raw_t),
                    CursorAnimStyle::Linear => ease_linear(raw_t),
                    _ => raw_t,
                };
                self.current_x = self.start_x + (target.x - self.start_x) * t;
                self.current_y = self.start_y + (target.y - self.start_y) * t;
                self.current_w = self.start_w + (target.width - self.start_w) * t;
                self.current_h = self.start_h + (target.height - self.start_h) * t;
                if raw_t >= 1.0 {
                    self.snap(&target);
                }
            }
        }

        true
    }

    /// Snap cursor to target and stop animating.
    ///
    /// This is the single exit from cursor motion -- every style's completion
    /// path ends here -- so it also brings the per-corner springs to rest.
    /// A spring left with velocity by one style's completion check would carry
    /// that velocity into the next animation.
    pub(super) fn snap(&mut self, target: &CursorTarget) {
        self.current_x = target.x;
        self.current_y = target.y;
        self.current_w = target.width;
        self.current_h = target.height;
        for spring in &mut self.corner_springs {
            spring.vx = 0.0;
            spring.vy = 0.0;
        }
        self.animating = false;
    }

    /// Tick cursor size transition, returns true if size changed (needs redraw).
    pub(super) fn tick_size_animation(
        &mut self,
        at: neomacs_display_protocol::frame_time::EventTime,
    ) -> bool {
        if !self.size_transition_enabled || !self.size_animating {
            return false;
        }
        let elapsed = at.saturating_since(self.size_anim_start).as_secs_f32();
        let raw_t = (elapsed / self.size_transition_duration).min(1.0);
        let t = raw_t * (2.0 - raw_t); // ease-out-quad
        self.current_w = self.size_start_w + (self.size_target_w - self.size_start_w) * t;
        self.current_h = self.size_start_h + (self.size_target_h - self.size_start_h) * t;
        if raw_t >= 1.0 {
            self.current_w = self.size_target_w;
            self.current_h = self.size_target_h;
            self.size_animating = false;
        }
        true
    }

    /// Reset blink to visible (e.g. when new frame arrives)
    pub(super) fn reset_blink(&mut self, at: neomacs_display_protocol::frame_time::EventTime) {
        self.blink_on = true;
        self.last_blink_toggle = at;
    }

    pub(super) fn force_blink_on(&mut self) -> bool {
        if self.blink_on {
            return false;
        }
        self.blink_on = true;
        true
    }
}

#[cfg(test)]
#[path = "cursor_test.rs"]
mod tests;
