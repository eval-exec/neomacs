//! Animation system for smooth property transitions.

use std::time::{Duration, Instant};

/// Target of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationTarget {
    /// Animate a specific window by ID.
    Window(u32),
    /// Animate the cursor.
    Cursor,
    /// Global animation (e.g., scene-wide effects).
    Global,
}

/// Property that can be animated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimatedProperty {
    /// X position.
    X,
    /// Y position.
    Y,
    /// Width.
    Width,
    /// Height.
    Height,
    /// Opacity (0.0 to 1.0).
    Opacity,
    /// Scale factor.
    Scale,
    /// Rotation around Y axis (for page flip effects).
    RotationY,
    /// Rotation around X axis.
    RotationX,
    /// Translation along Z axis (for depth effects).
    TranslateZ,
}

/// Easing function for animation timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Linear interpolation (constant speed).
    #[default]
    Linear,
    /// Ease in (start slow, end fast).
    EaseIn,
    /// Ease out (start fast, end slow).
    EaseOut,
    /// Ease in and out (start slow, speed up, end slow).
    EaseInOut,
    /// Ease out with bounce effect.
    EaseOutBounce,
}

impl Easing {
    /// Apply the easing function to a normalized time value (0.0 to 1.0).
    ///
    /// Returns a value that may extend beyond 0.0-1.0 for some easing functions
    /// (like bounce), but typically stays within that range.
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);

        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Easing::EaseOutBounce => {
                // Standard bounce formula with n1=7.5625, d1=2.75
                let n1 = 7.5625;
                let d1 = 2.75;

                if t < 1.0 / d1 {
                    n1 * t * t
                } else if t < 2.0 / d1 {
                    let t = t - 1.5 / d1;
                    n1 * t * t + 0.75
                } else if t < 2.5 / d1 {
                    let t = t - 2.25 / d1;
                    n1 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / d1;
                    n1 * t * t + 0.984375
                }
            }
        }
    }
}

/// A single animation instance.
#[derive(Debug, Clone)]
pub struct Animation {
    /// Unique identifier for this animation.
    pub id: u64,
    /// Target of the animation.
    pub target: AnimationTarget,
    /// Property being animated.
    pub property: AnimatedProperty,
    /// Starting value.
    pub from: f32,
    /// Ending value.
    pub to: f32,
    /// Duration of the animation.
    pub duration: Duration,
    /// Easing function.
    pub easing: Easing,
    /// When the animation started.
    pub started: Instant,
}

impl Animation {
    /// Get the current interpolated value based on elapsed time and easing.
    pub fn current_value(&self) -> f32 {
        let elapsed = self.started.elapsed();
        let progress = if self.duration.as_secs_f32() > 0.0 {
            (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0)
        } else {
            1.0
        };

        let eased = self.easing.apply(progress);
        self.from + (self.to - self.from) * eased
    }

    /// Check if the animation has completed.
    pub fn is_complete(&self) -> bool {
        self.started.elapsed() >= self.duration
    }
}

/// Engine for managing multiple animations.
#[derive(Debug)]
pub struct AnimationEngine {
    /// Active animations.
    animations: Vec<Animation>,
    /// Next animation ID to assign.
    next_id: u64,
}

impl AnimationEngine {
    /// Create a new animation engine.
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
            next_id: 1,
        }
    }

    /// Start a new animation.
    ///
    /// Returns the animation ID which can be used to cancel the animation.
    pub fn animate(
        &mut self,
        target: AnimationTarget,
        property: AnimatedProperty,
        from: f32,
        to: f32,
        duration: Duration,
        easing: Easing,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        // Remove any existing animation for the same target and property
        self.animations
            .retain(|a| !(a.target == target && a.property == property));

        let animation = Animation {
            id,
            target,
            property,
            from,
            to,
            duration,
            easing,
            started: Instant::now(),
        };

        self.animations.push(animation);
        id
    }

    /// Cancel an animation by ID.
    pub fn cancel(&mut self, id: u64) {
        self.animations.retain(|a| a.id != id);
    }

    /// Tick the animation engine, removing completed animations.
    ///
    /// Returns `true` if there are any active animations remaining.
    pub fn tick(&mut self) -> bool {
        self.animations.retain(|a| !a.is_complete());
        !self.animations.is_empty()
    }

    /// Get the current animated value for a target and property.
    ///
    /// Returns `None` if there is no active animation for this combination.
    pub fn get_value(&self, target: AnimationTarget, property: AnimatedProperty) -> Option<f32> {
        self.animations
            .iter()
            .find(|a| a.target == target && a.property == property)
            .map(|a| a.current_value())
    }

    /// Check if there are any active animations.
    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty()
    }
}

impl Default for AnimationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "animation_test.rs"]
mod tests;
