use std::num::NonZeroU32;

/// A non-empty physical-pixel scissor contained by its render target.
///
/// WGPU validates scissors when command buffers are submitted, so raw scene
/// geometry must cross this boundary before reaching `set_scissor_rect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SurfaceScissor {
    x: u32,
    y: u32,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl SurfaceScissor {
    pub(super) fn intersect(
        (x, y, width, height): (u32, u32, u32, u32),
        surface_width: u32,
        surface_height: u32,
    ) -> Option<Self> {
        let right = x.saturating_add(width).min(surface_width);
        let bottom = y.saturating_add(height).min(surface_height);

        Some(Self {
            x,
            y,
            width: NonZeroU32::new(right.saturating_sub(x))?,
            height: NonZeroU32::new(bottom.saturating_sub(y))?,
        })
    }

    pub(super) fn apply(self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_scissor_rect(self.x, self.y, self.width.get(), self.height.get());
    }
}

#[cfg(test)]
#[path = "scissor/tests/scissor_test.rs"]
mod tests;
