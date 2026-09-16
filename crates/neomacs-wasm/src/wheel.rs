//! Normalize browser wheel samples into ordinary Emacs wheel gestures.
//!
//! This is deliberately not pixel-precision scrolling: the evaluator's normal
//! wheel bindings own scroll amount, acceleration, modifiers, and window choice.

use neomacs_display_protocol::ScrollDelta;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WheelTarget {
    pub frame: u64,
    pub window: Option<i64>,
    pub modifiers: u32,
}

#[derive(Default)]
pub(crate) struct WheelAccumulator {
    previous: Option<(WheelTarget, WheelUnit)>,
    residual: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WheelUnit {
    Pixels,
    Lines,
}

impl WheelAccumulator {
    pub fn observe(
        &mut self,
        target: WheelTarget,
        delta: ScrollDelta,
        line_height: f32,
    ) -> Option<ScrollDelta> {
        let (y, unit) = match delta {
            ScrollDelta::Pixels { y, .. } => (y, WheelUnit::Pixels),
            ScrollDelta::Lines { y, .. } => (y, WheelUnit::Lines),
        };
        if !y.is_finite() || y == 0.0 {
            return None;
        }
        let key = (target, unit);
        if self.previous != Some(key) || self.residual.signum() != y.signum() {
            self.residual = 0.0;
        }
        self.previous = Some(key);
        self.residual += y;
        // A conventional wheel notch corresponds to approximately three text
        // lines. Small high-resolution packets accumulate rather than each
        // triggering a complete mwheel-scroll command.
        let threshold = match unit {
            WheelUnit::Pixels => line_height.max(1.0) * 3.0,
            WheelUnit::Lines => 1.0,
        };
        if self.residual.abs() < threshold {
            return None;
        }
        self.residual %= threshold;
        // Match native wheel-event semantics: one observed gesture invokes one
        // Lisp command, whose amount is controlled by mouse-wheel-scroll-amount.
        Some(ScrollDelta::Lines {
            x: 0.0,
            y: y.signum(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(window: i64) -> WheelTarget {
        WheelTarget {
            frame: 1,
            window: Some(window),
            modifiers: 0,
        }
    }

    #[test]
    fn small_pixel_packets_accumulate_into_a_normal_wheel_gesture() {
        let mut wheel = WheelAccumulator::default();
        let delta = ScrollDelta::Pixels { x: 0.0, y: -10.0 };
        for _ in 0..5 {
            assert_eq!(wheel.observe(target(1), delta, 20.0), None);
        }
        assert_eq!(
            wheel.observe(target(1), delta, 20.0),
            Some(ScrollDelta::Lines { x: 0.0, y: -1.0 })
        );
    }

    #[test]
    fn residuals_never_leak_across_windows_or_modifiers_or_units() {
        let mut wheel = WheelAccumulator::default();
        let delta = ScrollDelta::Pixels { x: 0.0, y: 40.0 };
        assert_eq!(wheel.observe(target(1), delta, 20.0), None);
        assert_eq!(wheel.observe(target(2), delta, 20.0), None);
        let mut modified = target(2);
        modified.modifiers = 1;
        assert_eq!(wheel.observe(modified, delta, 20.0), None);
        assert_eq!(
            wheel.observe(modified, ScrollDelta::Lines { x: 0.0, y: 0.5 }, 20.0),
            None
        );
    }

    #[test]
    fn reversal_is_immediate_and_horizontal_only_does_not_scroll_down() {
        let mut wheel = WheelAccumulator::default();
        assert_eq!(
            wheel.observe(target(1), ScrollDelta::Lines { x: 1.0, y: 0.0 }, 20.0),
            None
        );
        assert_eq!(
            wheel.observe(target(1), ScrollDelta::Pixels { x: 0.0, y: 50.0 }, 20.0),
            None
        );
        assert_eq!(
            wheel.observe(target(1), ScrollDelta::Pixels { x: 0.0, y: -60.0 }, 20.0),
            Some(ScrollDelta::Lines { x: 0.0, y: -1.0 })
        );
    }
}
