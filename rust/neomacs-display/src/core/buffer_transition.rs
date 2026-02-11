//! Buffer switch animation system - smooth transitions between buffers.

use std::time::{Duration, Instant};

/// Buffer transition animation effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferTransitionEffect {
    /// No animation - instant switch
    None,
    /// Simple crossfade
    #[default]
    Crossfade,
    /// Slide left (new comes from right)
    SlideLeft,
    /// Slide right (new comes from left)
    SlideRight,
    /// Slide up (new comes from bottom)
    SlideUp,
    /// Slide down (new comes from top)
    SlideDown,
    /// Scale and fade
    ScaleFade,
    /// Push (new covers old)
    Push,
    /// Blur transition
    Blur,
    /// 3D page curl (book page turn)
    PageCurl,
}

impl BufferTransitionEffect {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => Self::None,
            "crossfade" | "fade" => Self::Crossfade,
            "slide-left" | "slide" => Self::SlideLeft,
            "slide-right" => Self::SlideRight,
            "slide-up" => Self::SlideUp,
            "slide-down" => Self::SlideDown,
            "scale" | "scale-fade" => Self::ScaleFade,
            "push" | "stack" => Self::Push,
            "blur" => Self::Blur,
            "page" | "page-curl" | "book" => Self::PageCurl,
            _ => Self::Crossfade,
        }
    }
}

/// Easing function for animations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionEasing {
    Linear,
    #[default]
    EaseOut,
    EaseIn,
    EaseInOut,
    /// Overshoot then settle (bouncy)
    EaseOutBack,
}

impl TransitionEasing {
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::EaseOutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
        }
    }
}

/// Direction for directional animations (slide, push)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionDirection {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

/// State of an active buffer transition
#[derive(Debug, Clone)]
pub struct BufferTransition {
    /// The effect type
    pub effect: BufferTransitionEffect,
    
    /// Direction for directional effects
    pub direction: TransitionDirection,
    
    /// Animation progress (0.0 = start, 1.0 = complete)
    pub progress: f32,
    
    /// Total duration
    pub duration: Duration,
    
    /// Start time
    pub start_time: Instant,
    
    /// Easing function
    pub easing: TransitionEasing,
    
    /// Is the animation complete?
    pub completed: bool,
    
    /// Old buffer snapshot width
    pub old_width: f32,
    
    /// Old buffer snapshot height
    pub old_height: f32,
}

impl BufferTransition {
    pub fn new(effect: BufferTransitionEffect, direction: TransitionDirection, duration: Duration) -> Self {
        Self {
            effect,
            direction,
            progress: 0.0,
            duration,
            start_time: Instant::now(),
            easing: TransitionEasing::EaseOut,
            completed: false,
            old_width: 0.0,
            old_height: 0.0,
        }
    }
    
    /// Update progress based on elapsed time
    pub fn update(&mut self) -> bool {
        if self.completed {
            return false;
        }
        
        let elapsed = Instant::now().duration_since(self.start_time);
        let raw_progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        
        if raw_progress >= 1.0 {
            self.progress = 1.0;
            self.completed = true;
            return false;
        }
        
        self.progress = self.easing.apply(raw_progress);
        true
    }

    /// Update progress with explicit delta time
    pub fn update_with_dt(&mut self, dt: f32) -> bool {
        if self.completed {
            return false;
        }
        
        let elapsed = Instant::now().duration_since(self.start_time);
        let raw_progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        
        if raw_progress >= 1.0 {
            self.progress = 1.0;
            self.completed = true;
            return false;
        }
        
        self.progress = self.easing.apply(raw_progress);
        true
    }
    
    /// Get the eased progress value
    pub fn eased_progress(&self) -> f32 {
        self.progress
    }
    
    // === Effect-specific calculations ===
    
    /// Get crossfade opacity for old content
    pub fn crossfade_old_opacity(&self) -> f32 {
        1.0 - self.progress
    }
    
    /// Get crossfade opacity for new content  
    pub fn crossfade_new_opacity(&self) -> f32 {
        self.progress
    }
    
    /// Get slide offset for old content
    pub fn slide_old_offset(&self) -> (f32, f32) {
        let offset = self.progress;
        match self.direction {
            TransitionDirection::Left => (-offset * self.old_width, 0.0),
            TransitionDirection::Right => (offset * self.old_width, 0.0),
            TransitionDirection::Up => (0.0, -offset * self.old_height),
            TransitionDirection::Down => (0.0, offset * self.old_height),
        }
    }
    
    /// Get slide offset for new content
    pub fn slide_new_offset(&self) -> (f32, f32) {
        let offset = 1.0 - self.progress;
        match self.direction {
            TransitionDirection::Left => (offset * self.old_width, 0.0),
            TransitionDirection::Right => (-offset * self.old_width, 0.0),
            TransitionDirection::Up => (0.0, offset * self.old_height),
            TransitionDirection::Down => (0.0, -offset * self.old_height),
        }
    }
    
    /// Get scale for old content (scale-fade effect)
    pub fn scale_old(&self) -> f32 {
        1.0 - self.progress * 0.1 // Scale down to 0.9
    }
    
    /// Get scale for new content (scale-fade effect)
    pub fn scale_new(&self) -> f32 {
        0.9 + self.progress * 0.1 // Scale up from 0.9 to 1.0
    }
    
    /// Get blur radius for old content
    pub fn blur_old_radius(&self) -> f32 {
        self.progress * 15.0 // 0 to 15px blur
    }
    
    /// Get blur radius for new content
    pub fn blur_new_radius(&self) -> f32 {
        (1.0 - self.progress) * 15.0 // 15px to 0 blur
    }
    
    /// Get page curl parameters
    /// Returns (curl_progress, curl_angle, shadow_opacity)
    pub fn page_curl_params(&self) -> (f32, f32, f32) {
        let curl_progress = self.progress;
        // Angle goes from 0 to PI as page turns
        let curl_angle = self.progress * std::f32::consts::PI;
        // Shadow is strongest in the middle of the turn
        let shadow_opacity = (self.progress * std::f32::consts::PI).sin() * 0.5;
        (curl_progress, curl_angle, shadow_opacity)
    }
}

/// Buffer transition animator - manages transition state and snapshot
#[derive(Debug)]
pub struct BufferTransitionAnimator {
    /// Default effect for transitions
    pub default_effect: BufferTransitionEffect,
    
    /// Default duration
    pub default_duration: Duration,
    
    /// Currently active transition (if any)
    pub active_transition: Option<BufferTransition>,
    
    /// Whether we have a snapshot of the old buffer
    pub has_snapshot: bool,
    
    /// Snapshot texture ID (managed externally)
    pub snapshot_id: u32,
    
    /// Auto-detect buffer switches
    pub auto_detect: bool,
    
    /// Last content hash (for auto-detection)
    last_content_hash: u64,
}

impl Default for BufferTransitionAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferTransitionAnimator {
    pub fn new() -> Self {
        Self {
            default_effect: BufferTransitionEffect::Crossfade,
            default_duration: Duration::from_millis(200),
            active_transition: None,
            has_snapshot: false,
            snapshot_id: 0,
            auto_detect: true,
            last_content_hash: 0,
        }
    }
    
    /// Start a transition with default settings
    pub fn start_transition(&mut self) {
        self.start_transition_with(self.default_effect, TransitionDirection::Left);
    }
    
    /// Start a transition with specific effect and direction
    pub fn start_transition_with(&mut self, effect: BufferTransitionEffect, direction: TransitionDirection) {
        if effect == BufferTransitionEffect::None {
            self.active_transition = None;
            return;
        }
        
        self.active_transition = Some(BufferTransition::new(
            effect,
            direction,
            self.default_duration,
        ));
    }
    
    /// Request snapshot capture (call before buffer switch)
    pub fn request_snapshot(&mut self) {
        self.has_snapshot = false; // Will be set true when snapshot is captured
    }
    
    /// Mark snapshot as captured
    pub fn snapshot_captured(&mut self, width: f32, height: f32) {
        self.has_snapshot = true;
        if let Some(ref mut transition) = self.active_transition {
            transition.old_width = width;
            transition.old_height = height;
        }
    }
    
    /// Update the active transition
    /// Returns true if transition is still active (needs redraw)
    pub fn update(&mut self) -> bool {
        if let Some(ref mut transition) = self.active_transition {
            let still_active = transition.update();
            if !still_active {
                self.active_transition = None;
                self.has_snapshot = false;
            }
            still_active
        } else {
            false
        }
    }

    /// Update with explicit delta time
    pub fn update_with_dt(&mut self, dt: f32) -> bool {
        if let Some(ref mut transition) = self.active_transition {
            let still_active = transition.update_with_dt(dt);
            if !still_active {
                self.active_transition = None;
                self.has_snapshot = false;
            }
            still_active
        } else {
            false
        }
    }
    
    /// Check if a transition is currently active
    pub fn is_active(&self) -> bool {
        self.active_transition.is_some()
    }
    
    /// Get the current transition (if any)
    pub fn get_transition(&self) -> Option<&BufferTransition> {
        self.active_transition.as_ref()
    }
    
    /// Set default effect
    pub fn set_default_effect(&mut self, effect: BufferTransitionEffect) {
        self.default_effect = effect;
    }
    
    /// Set default duration
    pub fn set_default_duration(&mut self, duration: Duration) {
        self.default_duration = duration;
    }
    
    /// Simple hash for content change detection
    pub fn update_content_hash(&mut self, hash: u64) -> bool {
        let changed = hash != self.last_content_hash && self.last_content_hash != 0;
        self.last_content_hash = hash;
        changed
    }
}

/// Page curl shader parameters for GPU rendering
#[derive(Debug, Clone, Copy)]
pub struct PageCurlParams {
    /// Curl progress (0.0 = flat, 1.0 = fully turned)
    pub progress: f32,
    /// Curl cylinder radius
    pub radius: f32,
    /// Corner being lifted (0=bottom-right, 1=top-right, 2=bottom-left, 3=top-left)
    pub corner: u32,
    /// Page width
    pub width: f32,
    /// Page height
    pub height: f32,
    /// Shadow intensity
    pub shadow: f32,
    /// Backside darkening
    pub backside_darken: f32,
}

impl Default for PageCurlParams {
    fn default() -> Self {
        Self {
            progress: 0.0,
            radius: 50.0,
            corner: 0, // bottom-right
            width: 800.0,
            height: 600.0,
            shadow: 0.3,
            backside_darken: 0.2,
        }
    }
}

impl PageCurlParams {
    /// Update params based on animation progress
    pub fn from_progress(progress: f32, width: f32, height: f32) -> Self {
        Self {
            progress,
            radius: 30.0 + progress * 40.0, // Radius increases as page lifts
            corner: 0,
            width,
            height,
            shadow: (progress * std::f32::consts::PI).sin() * 0.4,
            backside_darken: 0.15,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ---- BufferTransitionEffect ----

    #[test]
    fn effect_default_is_crossfade() {
        assert_eq!(BufferTransitionEffect::default(), BufferTransitionEffect::Crossfade);
    }

    #[test]
    fn effect_from_str_known_variants() {
        assert_eq!(BufferTransitionEffect::from_str("none"), BufferTransitionEffect::None);
        assert_eq!(BufferTransitionEffect::from_str("crossfade"), BufferTransitionEffect::Crossfade);
        assert_eq!(BufferTransitionEffect::from_str("fade"), BufferTransitionEffect::Crossfade);
        assert_eq!(BufferTransitionEffect::from_str("slide-left"), BufferTransitionEffect::SlideLeft);
        assert_eq!(BufferTransitionEffect::from_str("slide"), BufferTransitionEffect::SlideLeft);
        assert_eq!(BufferTransitionEffect::from_str("slide-right"), BufferTransitionEffect::SlideRight);
        assert_eq!(BufferTransitionEffect::from_str("slide-up"), BufferTransitionEffect::SlideUp);
        assert_eq!(BufferTransitionEffect::from_str("slide-down"), BufferTransitionEffect::SlideDown);
        assert_eq!(BufferTransitionEffect::from_str("scale"), BufferTransitionEffect::ScaleFade);
        assert_eq!(BufferTransitionEffect::from_str("scale-fade"), BufferTransitionEffect::ScaleFade);
        assert_eq!(BufferTransitionEffect::from_str("push"), BufferTransitionEffect::Push);
        assert_eq!(BufferTransitionEffect::from_str("stack"), BufferTransitionEffect::Push);
        assert_eq!(BufferTransitionEffect::from_str("blur"), BufferTransitionEffect::Blur);
        assert_eq!(BufferTransitionEffect::from_str("page"), BufferTransitionEffect::PageCurl);
        assert_eq!(BufferTransitionEffect::from_str("page-curl"), BufferTransitionEffect::PageCurl);
        assert_eq!(BufferTransitionEffect::from_str("book"), BufferTransitionEffect::PageCurl);
    }

    #[test]
    fn effect_from_str_case_insensitive() {
        assert_eq!(BufferTransitionEffect::from_str("CROSSFADE"), BufferTransitionEffect::Crossfade);
        assert_eq!(BufferTransitionEffect::from_str("Slide-Left"), BufferTransitionEffect::SlideLeft);
        assert_eq!(BufferTransitionEffect::from_str("NONE"), BufferTransitionEffect::None);
    }

    #[test]
    fn effect_from_str_unknown_falls_back_to_crossfade() {
        assert_eq!(BufferTransitionEffect::from_str("unknown"), BufferTransitionEffect::Crossfade);
        assert_eq!(BufferTransitionEffect::from_str(""), BufferTransitionEffect::Crossfade);
    }

    // ---- TransitionEasing ----

    #[test]
    fn easing_default_is_ease_out() {
        assert_eq!(TransitionEasing::default(), TransitionEasing::EaseOut);
    }

    #[test]
    fn easing_linear() {
        let e = TransitionEasing::Linear;
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(0.5), 0.5);
        assert_eq!(e.apply(1.0), 1.0);
    }

    #[test]
    fn easing_clamps_input() {
        let e = TransitionEasing::Linear;
        assert_eq!(e.apply(-0.5), 0.0);
        assert_eq!(e.apply(1.5), 1.0);
    }

    #[test]
    fn easing_ease_in_starts_slow() {
        let e = TransitionEasing::EaseIn;
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(1.0), 1.0);
        // Cubic ease-in: at t=0.5, value should be 0.125 (slow start)
        assert!((e.apply(0.5) - 0.125).abs() < 1e-6);
    }

    #[test]
    fn easing_ease_out_ends_slow() {
        let e = TransitionEasing::EaseOut;
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(1.0), 1.0);
        // Cubic ease-out: at t=0.5, value should be 0.875 (fast start)
        assert!((e.apply(0.5) - 0.875).abs() < 1e-6);
    }

    #[test]
    fn easing_ease_in_out_symmetric() {
        let e = TransitionEasing::EaseInOut;
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(1.0), 1.0);
        // At midpoint, value should be exactly 0.5
        assert!((e.apply(0.5) - 0.5).abs() < 1e-6);
        // Symmetry: f(t) + f(1-t) == 1
        let t = 0.3;
        assert!((e.apply(t) + e.apply(1.0 - t) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn easing_ease_out_back_overshoots() {
        let e = TransitionEasing::EaseOutBack;
        assert_eq!(e.apply(0.0), 0.0);
        assert!((e.apply(1.0) - 1.0).abs() < 1e-6);
        // EaseOutBack should overshoot 1.0 at some point before t=1.0
        let peak = e.apply(0.85);
        assert!(peak > 1.0, "EaseOutBack should overshoot; got {}", peak);
    }

    #[test]
    fn easing_all_variants_at_boundaries() {
        let easings = [
            TransitionEasing::Linear,
            TransitionEasing::EaseIn,
            TransitionEasing::EaseOut,
            TransitionEasing::EaseInOut,
            TransitionEasing::EaseOutBack,
        ];
        for e in &easings {
            assert!(
                (e.apply(0.0)).abs() < 1e-6,
                "{:?} should start at 0",
                e
            );
            assert!(
                (e.apply(1.0) - 1.0).abs() < 1e-6,
                "{:?} should end at 1",
                e
            );
        }
    }

    // ---- BufferTransition creation and initial state ----

    #[test]
    fn transition_initial_state() {
        let t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        assert_eq!(t.effect, BufferTransitionEffect::Crossfade);
        assert_eq!(t.direction, TransitionDirection::Left);
        assert_eq!(t.progress, 0.0);
        assert!(!t.completed);
        assert_eq!(t.easing, TransitionEasing::EaseOut);
        assert_eq!(t.old_width, 0.0);
        assert_eq!(t.old_height, 0.0);
        assert_eq!(t.duration, Duration::from_millis(200));
    }

    // ---- Crossfade calculations ----

    #[test]
    fn crossfade_opacity_at_start() {
        let t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        // progress = 0.0 at creation
        assert_eq!(t.crossfade_old_opacity(), 1.0);
        assert_eq!(t.crossfade_new_opacity(), 0.0);
    }

    #[test]
    fn crossfade_opacity_at_end() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        t.progress = 1.0;
        assert_eq!(t.crossfade_old_opacity(), 0.0);
        assert_eq!(t.crossfade_new_opacity(), 1.0);
    }

    #[test]
    fn crossfade_opacities_sum_to_one() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            t.progress = p;
            let sum = t.crossfade_old_opacity() + t.crossfade_new_opacity();
            assert!((sum - 1.0).abs() < 1e-6);
        }
    }

    // ---- Slide calculations ----

    #[test]
    fn slide_offset_left_at_start() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::SlideLeft,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        t.old_width = 800.0;
        t.old_height = 600.0;
        // progress = 0 => old not moved, new fully off-screen right
        assert_eq!(t.slide_old_offset(), (0.0, 0.0));
        assert_eq!(t.slide_new_offset(), (800.0, 0.0));
    }

    #[test]
    fn slide_offset_left_at_end() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::SlideLeft,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        t.old_width = 800.0;
        t.old_height = 600.0;
        t.progress = 1.0;
        // old slides fully left, new at origin
        assert_eq!(t.slide_old_offset(), (-800.0, 0.0));
        assert_eq!(t.slide_new_offset(), (0.0, 0.0));
    }

    #[test]
    fn slide_offset_right_direction() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::SlideRight,
            TransitionDirection::Right,
            Duration::from_millis(200),
        );
        t.old_width = 800.0;
        t.progress = 1.0;
        assert_eq!(t.slide_old_offset(), (800.0, 0.0));
        assert_eq!(t.slide_new_offset(), (0.0, 0.0));
    }

    #[test]
    fn slide_offset_up_direction() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::SlideUp,
            TransitionDirection::Up,
            Duration::from_millis(200),
        );
        t.old_height = 600.0;
        t.progress = 1.0;
        assert_eq!(t.slide_old_offset(), (0.0, -600.0));
        assert_eq!(t.slide_new_offset(), (0.0, 0.0));
    }

    #[test]
    fn slide_offset_down_direction() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::SlideDown,
            TransitionDirection::Down,
            Duration::from_millis(200),
        );
        t.old_height = 600.0;
        t.progress = 1.0;
        assert_eq!(t.slide_old_offset(), (0.0, 600.0));
        assert_eq!(t.slide_new_offset(), (0.0, 0.0));
    }

    // ---- Scale-fade calculations ----

    #[test]
    fn scale_fade_boundaries() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::ScaleFade,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        // At start: old=1.0, new=0.9
        assert_eq!(t.scale_old(), 1.0);
        assert!((t.scale_new() - 0.9).abs() < 1e-6);
        // At end: old=0.9, new=1.0
        t.progress = 1.0;
        assert!((t.scale_old() - 0.9).abs() < 1e-6);
        assert_eq!(t.scale_new(), 1.0);
    }

    // ---- Blur calculations ----

    #[test]
    fn blur_radius_boundaries() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::Blur,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        // At start: old has no blur, new has full blur
        assert_eq!(t.blur_old_radius(), 0.0);
        assert_eq!(t.blur_new_radius(), 15.0);
        // At end: old has full blur, new has no blur
        t.progress = 1.0;
        assert_eq!(t.blur_old_radius(), 15.0);
        assert_eq!(t.blur_new_radius(), 0.0);
    }

    // ---- Page curl calculations ----

    #[test]
    fn page_curl_params_at_boundaries() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::PageCurl,
            TransitionDirection::Left,
            Duration::from_millis(300),
        );
        // At start
        let (curl, angle, shadow) = t.page_curl_params();
        assert_eq!(curl, 0.0);
        assert_eq!(angle, 0.0);
        assert!(shadow.abs() < 1e-6);
        // At end
        t.progress = 1.0;
        let (curl, angle, shadow) = t.page_curl_params();
        assert_eq!(curl, 1.0);
        assert!((angle - std::f32::consts::PI).abs() < 1e-6);
        assert!(shadow.abs() < 1e-6); // sin(PI) ~ 0
    }

    #[test]
    fn page_curl_shadow_peaks_at_midpoint() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::PageCurl,
            TransitionDirection::Left,
            Duration::from_millis(300),
        );
        t.progress = 0.5;
        let (_, _, shadow) = t.page_curl_params();
        // sin(0.5 * PI) = 1.0, so shadow = 0.5
        assert!((shadow - 0.5).abs() < 1e-6);
    }

    // ---- Transition completion via update() ----

    #[test]
    fn transition_completes_after_duration() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(1), // very short
        );
        // Sleep just past the duration
        std::thread::sleep(Duration::from_millis(5));
        let still_active = t.update();
        assert!(!still_active);
        assert!(t.completed);
        assert_eq!(t.progress, 1.0);
    }

    #[test]
    fn transition_update_returns_false_when_already_completed() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(1),
        );
        t.completed = true;
        assert!(!t.update());
    }

    #[test]
    fn eased_progress_matches_progress_field() {
        let mut t = BufferTransition::new(
            BufferTransitionEffect::Crossfade,
            TransitionDirection::Left,
            Duration::from_millis(200),
        );
        t.progress = 0.42;
        assert_eq!(t.eased_progress(), 0.42);
    }

    // ---- BufferTransitionAnimator ----

    #[test]
    fn animator_default_state() {
        let a = BufferTransitionAnimator::new();
        assert_eq!(a.default_effect, BufferTransitionEffect::Crossfade);
        assert_eq!(a.default_duration, Duration::from_millis(200));
        assert!(a.active_transition.is_none());
        assert!(!a.has_snapshot);
        assert!(a.auto_detect);
        assert!(!a.is_active());
    }

    #[test]
    fn animator_default_trait_matches_new() {
        let a = BufferTransitionAnimator::default();
        assert_eq!(a.default_effect, BufferTransitionEffect::Crossfade);
        assert_eq!(a.default_duration, Duration::from_millis(200));
    }

    #[test]
    fn animator_start_transition_creates_active() {
        let mut a = BufferTransitionAnimator::new();
        a.start_transition();
        assert!(a.is_active());
        let t = a.get_transition().unwrap();
        assert_eq!(t.effect, BufferTransitionEffect::Crossfade);
        assert_eq!(t.direction, TransitionDirection::Left);
    }

    #[test]
    fn animator_start_transition_with_none_clears() {
        let mut a = BufferTransitionAnimator::new();
        a.start_transition(); // first create an active transition
        assert!(a.is_active());
        a.start_transition_with(BufferTransitionEffect::None, TransitionDirection::Left);
        assert!(!a.is_active());
    }

    #[test]
    fn animator_start_transition_with_specific_effect() {
        let mut a = BufferTransitionAnimator::new();
        a.start_transition_with(BufferTransitionEffect::SlideUp, TransitionDirection::Up);
        assert!(a.is_active());
        let t = a.get_transition().unwrap();
        assert_eq!(t.effect, BufferTransitionEffect::SlideUp);
        assert_eq!(t.direction, TransitionDirection::Up);
    }

    #[test]
    fn animator_snapshot_workflow() {
        let mut a = BufferTransitionAnimator::new();
        a.start_transition();
        a.request_snapshot();
        assert!(!a.has_snapshot);
        a.snapshot_captured(800.0, 600.0);
        assert!(a.has_snapshot);
        let t = a.get_transition().unwrap();
        assert_eq!(t.old_width, 800.0);
        assert_eq!(t.old_height, 600.0);
    }

    #[test]
    fn animator_snapshot_captured_without_transition() {
        let mut a = BufferTransitionAnimator::new();
        // No active transition; should not panic
        a.snapshot_captured(800.0, 600.0);
        assert!(a.has_snapshot);
    }

    #[test]
    fn animator_update_completes_and_clears() {
        let mut a = BufferTransitionAnimator::new();
        a.default_duration = Duration::from_millis(1);
        a.start_transition();
        a.has_snapshot = true;
        std::thread::sleep(Duration::from_millis(5));
        let still_active = a.update();
        assert!(!still_active);
        assert!(!a.is_active());
        assert!(!a.has_snapshot);
    }

    #[test]
    fn animator_update_no_transition_returns_false() {
        let mut a = BufferTransitionAnimator::new();
        assert!(!a.update());
    }

    #[test]
    fn animator_set_default_effect() {
        let mut a = BufferTransitionAnimator::new();
        a.set_default_effect(BufferTransitionEffect::Blur);
        assert_eq!(a.default_effect, BufferTransitionEffect::Blur);
        a.start_transition();
        let t = a.get_transition().unwrap();
        assert_eq!(t.effect, BufferTransitionEffect::Blur);
    }

    #[test]
    fn animator_set_default_duration() {
        let mut a = BufferTransitionAnimator::new();
        a.set_default_duration(Duration::from_millis(500));
        assert_eq!(a.default_duration, Duration::from_millis(500));
        a.start_transition();
        let t = a.get_transition().unwrap();
        assert_eq!(t.duration, Duration::from_millis(500));
    }

    // ---- Content hash change detection ----

    #[test]
    fn content_hash_first_update_not_changed() {
        let mut a = BufferTransitionAnimator::new();
        // First hash from zero should not report change
        assert!(!a.update_content_hash(42));
    }

    #[test]
    fn content_hash_detects_change() {
        let mut a = BufferTransitionAnimator::new();
        a.update_content_hash(42);
        assert!(a.update_content_hash(99));
    }

    #[test]
    fn content_hash_same_value_not_changed() {
        let mut a = BufferTransitionAnimator::new();
        a.update_content_hash(42);
        assert!(!a.update_content_hash(42));
    }

    // ---- PageCurlParams ----

    #[test]
    fn page_curl_params_default() {
        let p = PageCurlParams::default();
        assert_eq!(p.progress, 0.0);
        assert_eq!(p.radius, 50.0);
        assert_eq!(p.corner, 0);
        assert_eq!(p.width, 800.0);
        assert_eq!(p.height, 600.0);
        assert_eq!(p.shadow, 0.3);
        assert_eq!(p.backside_darken, 0.2);
    }

    #[test]
    fn page_curl_params_from_progress_at_zero() {
        let p = PageCurlParams::from_progress(0.0, 1024.0, 768.0);
        assert_eq!(p.progress, 0.0);
        assert_eq!(p.radius, 30.0); // 30 + 0*40
        assert_eq!(p.width, 1024.0);
        assert_eq!(p.height, 768.0);
        assert!(p.shadow.abs() < 1e-6); // sin(0) = 0
    }

    #[test]
    fn page_curl_params_from_progress_at_half() {
        let p = PageCurlParams::from_progress(0.5, 800.0, 600.0);
        assert_eq!(p.progress, 0.5);
        assert!((p.radius - 50.0).abs() < 1e-6); // 30 + 0.5*40
        // shadow = sin(0.5*PI) * 0.4 = 1.0 * 0.4 = 0.4
        assert!((p.shadow - 0.4).abs() < 1e-6);
    }

    #[test]
    fn page_curl_params_from_progress_at_one() {
        let p = PageCurlParams::from_progress(1.0, 800.0, 600.0);
        assert_eq!(p.progress, 1.0);
        assert!((p.radius - 70.0).abs() < 1e-6); // 30 + 1.0*40
        // shadow = sin(PI) * 0.4 ~ 0
        assert!(p.shadow.abs() < 1e-5);
    }

    // ---- TransitionDirection default ----

    #[test]
    fn direction_default_is_left() {
        assert_eq!(TransitionDirection::default(), TransitionDirection::Left);
    }
}
