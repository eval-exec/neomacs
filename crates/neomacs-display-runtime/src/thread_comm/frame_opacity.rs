//! Synchronous native opacity controls, independent of replaceable scenes.
//!
//! Setters and keyboard focus serialize on this small CPU state. In particular
//! nil replaces the policy but retains the scalar already applied by the prior
//! setter, even when no scene carrying that setter was ever rendered.
use std::collections::HashMap;

#[derive(Debug)]
struct FrameOpacity {
    pair: [f32; 2],
    limit: f32,
    applied: f32,
}

#[derive(Debug, Default)]
pub struct FrameOpacityState {
    frames: HashMap<u64, FrameOpacity>,
    redirects: HashMap<u64, Option<u64>>,
    native_focus: Option<u64>,
}

impl FrameOpacityState {
    pub fn accept(&mut self, frame: u64, pair: [f32; 2], limit: f32) {
        let old_highlight = self.highlight();
        let state = self.frames.entry(frame).or_insert(FrameOpacity {
            pair: [-1.0; 2],
            limit,
            applied: 1.0,
        });
        state.pair = pair;
        state.limit = limit;
        self.rehighlight(old_highlight);
        self.apply(frame);
    }

    /// Publishing the variable does not itself reapply alpha. GNU consults
    /// the current scalar on the next setter or focus/highlight operation.
    pub fn set_lower_limit(&mut self, limit: f32) {
        for state in self.frames.values_mut() {
            state.limit = limit;
        }
    }

    pub fn focus(&mut self, frame: u64, focused: bool) {
        let old_highlight = self.highlight();
        if focused {
            self.native_focus = Some(frame);
        } else if self.native_focus == Some(frame) {
            // A late leave from A cannot clear a newer enter on B.
            self.native_focus = None;
        }
        self.rehighlight(old_highlight);
    }

    pub fn set_redirects(&mut self, redirects: Vec<(u64, Option<u64>)>) {
        let old_highlight = self.highlight();
        self.redirects = redirects.into_iter().collect();
        self.rehighlight(old_highlight);
    }

    pub fn retire(&mut self, frame: u64) {
        let old_highlight = self.highlight();
        self.frames.remove(&frame);
        self.redirects.remove(&frame);
        if self.native_focus == Some(frame) {
            self.native_focus = None;
        }
        self.rehighlight(old_highlight);
    }

    pub fn applied(&self, frame: u64) -> Option<f32> {
        self.frames.get(&frame).map(|state| state.applied)
    }

    fn highlight(&self) -> Option<u64> {
        // GNU x_frame_rehighlight resolves one hop, falling back to the
        // focused frame when its redirect is absent or no longer live.
        self.native_focus.map(|frame| {
            self.redirects
                .get(&frame)
                .copied()
                .flatten()
                .filter(|target| self.frames.contains_key(target))
                .unwrap_or(frame)
        })
    }

    fn rehighlight(&mut self, old_highlight: Option<u64>) {
        let highlight = self.highlight();
        // GNU re-highlights only the old/new participants on a real change.
        // No-op redirects and stale focus events must not replay unrelated
        // policies under a temporarily bound lower limit.
        if old_highlight != highlight {
            for frame in old_highlight.into_iter().chain(highlight) {
                self.apply(frame);
            }
        }
    }

    fn apply(&mut self, frame: u64) {
        let highlight = self.highlight();
        if let Some(state) = self.frames.get_mut(&frame) {
            let alpha = state.pair[usize::from(highlight != Some(frame))];
            if alpha >= 0.0 {
                state.applied = if (0.0..=1.0).contains(&state.limit) {
                    alpha.max(state.limit)
                } else {
                    alpha
                };
            }
        }
    }
}

#[cfg(test)]
#[path = "frame_opacity/tests/frame_opacity_test.rs"]
mod tests;
