//! Convert platform units once, retaining fractional displacement until the
//! integral Lisp scroll boundary. State is bounded to the current input target.

use crate::thread_comm::ScrollDelta;
use neomacs_display_protocol::DisplayWindowId;
use winit::event::{DeviceId, MouseScrollDelta, TouchPhase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ScrollTarget {
    pub frame: u64,
    pub window: Option<DisplayWindowId>,
    pub buffer: Option<u64>,
    pub height: f32,
    pub scale: f64,
    pub x11_factor: f64,
    pub modifiers: u32,
    pub device: Option<DeviceId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Units {
    Lines,
    Pixels,
    Continuous,
}

#[derive(Default)]
pub(super) struct ScrollInput {
    context: Option<(ScrollTarget, Units)>,
    remainder: [f64; 2],
}

impl ScrollInput {
    pub(super) fn reset(&mut self) {
        self.context = None;
        self.remainder = [0.0; 2];
    }

    pub(super) fn convert(
        &mut self,
        delta: MouseScrollDelta,
        target: ScrollTarget,
        phase: TouchPhase,
    ) -> Option<ScrollDelta> {
        if phase == TouchPhase::Started {
            self.reset();
        }
        let (units, mut x, mut y) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (Units::Lines, x as f64, y as f64),
            MouseScrollDelta::ContinuousLineDelta(x, y) => (Units::Continuous, x as f64, y as f64),
            MouseScrollDelta::PixelDelta(p) => {
                (Units::Pixels, p.x / target.scale, p.y / target.scale)
            }
            _ => {
                self.reset();
                return None;
            }
        };
        if !x.is_finite() || !y.is_finite() || !target.scale.is_finite() || target.scale <= 0.0 {
            self.reset();
            return None;
        }
        if units == Units::Continuous {
            if !target.height.is_finite() || target.height <= 0.0 || !target.x11_factor.is_finite()
            {
                self.reset();
                return None;
            }
            // XI2 units are wheel increments, not physical pixels. GNU/GTK
            // choose distance using the logical viewport, independent of DPI.
            let distance = (target.height as f64).powf(2.0 / 3.0) * target.x11_factor;
            x *= distance;
            y *= distance;
        }
        if self.context != Some((target, units)) {
            self.reset();
            self.context = Some((target, units));
        }
        let mut output = [0.0; 2];
        for (axis, delta) in [x, y].into_iter().enumerate() {
            let sum = self.remainder[axis] + delta;
            if !sum.is_finite() || sum.abs() > i32::MAX as f64 {
                self.reset();
                return None;
            }
            output[axis] = if units == Units::Lines {
                sum.trunc()
            } else {
                sum.round()
            };
            self.remainder[axis] = sum - output[axis];
        }
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.reset();
        }
        let [x, y] = output.map(|n| n as f32);
        if x == 0.0 && y == 0.0 {
            return None;
        }
        Some(if units == Units::Lines {
            ScrollDelta::Lines { x, y }
        } else {
            ScrollDelta::Pixels { x, y }
        })
    }
}

#[cfg(test)]
#[path = "tests/scroll_input_test.rs"]
mod tests;
