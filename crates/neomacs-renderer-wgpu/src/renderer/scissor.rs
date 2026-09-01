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
mod tests {
    use super::SurfaceScissor;

    fn parts(scissor: SurfaceScissor) -> (u32, u32, u32, u32) {
        (
            scissor.x,
            scissor.y,
            scissor.width.get(),
            scissor.height.get(),
        )
    }

    #[test]
    fn surface_scissor_preserves_contained_rect() {
        let scissor = SurfaceScissor::intersect((8, 10, 20, 30), 100, 80).unwrap();
        assert_eq!(parts(scissor), (8, 10, 20, 30));
    }

    #[test]
    fn surface_scissor_clips_partial_rect() {
        let scissor = SurfaceScissor::intersect((80, 70, 30, 20), 100, 80).unwrap();
        assert_eq!(parts(scissor), (80, 70, 20, 10));
    }

    #[test]
    fn surface_scissor_rejects_empty_intersection() {
        assert_eq!(SurfaceScissor::intersect((8, 102, 8, 18), 824, 85), None);
        assert_eq!(SurfaceScissor::intersect((824, 10, 8, 18), 824, 85), None);
        assert_eq!(SurfaceScissor::intersect((8, 10, 0, 18), 824, 85), None);
    }

    #[test]
    fn surface_scissor_handles_overflowing_extent() {
        let scissor = SurfaceScissor::intersect((90, 70, u32::MAX, u32::MAX), 100, 80).unwrap();
        assert_eq!(parts(scissor), (90, 70, 10, 10));
    }
}
