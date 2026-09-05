//! Getting hold of the swapchain texture one frame draws into, and naming the
//! ways that can fail.
//!
//! Owns: the call to `wgpu::Surface::get_current_texture`, the mapping from
//! every `CurrentSurfaceTexture` variant to a [`FrameRenderFailure`], and the
//! device-loss streak bookkeeping that a repeated `Lost` feeds.
//!
//! Must not: draw, sample motion, touch the compositor, or advance any
//! retained state. Everything here runs *before* the frame is composed, and
//! every outcome but success abandons the frame — so work placed here is work
//! thrown away outright on a lost, outdated or occluded surface. The pass
//! keeps that work strictly below the acquisition for that reason.

use crate::render_thread::device_loss::{CONSECUTIVE_SURFACE_LOST_THRESHOLD, DeviceLossDetector};
use crate::render_thread::frame_sched::PresentResult;

/// Failures before a frame reaches `present`, kept distinct until the frame
/// coordinator consumes them.  In particular, missing editor content is not a
/// GPU timeout and must not manufacture an expose-retry loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameRenderFailure {
    AwaitingContent,
    WindowNotReady,
    SurfaceLost,
    SurfaceTimeout,
    SurfaceOccluded,
}

impl FrameRenderFailure {
    pub(super) const fn present_result(self) -> PresentResult {
        match self {
            Self::AwaitingContent => PresentResult::AwaitingContent,
            Self::WindowNotReady | Self::SurfaceTimeout => PresentResult::Timeout,
            Self::SurfaceLost => PresentResult::SurfaceLost,
            Self::SurfaceOccluded => PresentResult::Occluded,
        }
    }
}

/// Acquire the swapchain texture for `surface`, classifying every non-success
/// outcome into the failure the frame coordinator acts on.
///
/// `emacs_frame_id` is only ever logged; nothing here consults frame state.
/// Proof that this frame's surface was acquired.
///
/// Acquisition is where a frame stops being hypothetical. Everything above it
/// can still return — a lost, outdated or occluded surface, or a validation
/// error — and work done above those returns is either thrown away or, worse,
/// leaves state describing a frame that nobody ever composed.
///
/// Three things in the draw order may therefore run only below acquisition:
/// materializing the frame, sampling the pane layout, and draining the pending
/// continuity observations. Each of those takes one of these, and this type has
/// no public constructor and a private field, so [`acquire_current_texture`] is
/// the only thing that can produce one. Moving such a call above acquisition is
/// a compile error rather than a comment somebody has to notice.
///
/// That distinction is not hypothetical: both a projection published for a
/// frame that was abandoned, and a settled projection discarded by the
/// compositor-only path, were shipped bugs whose root cause was this ordering
/// being enforced by prose.
#[derive(Debug)]
pub(in crate::render_thread) struct SurfaceAcquired(());

impl SurfaceAcquired {
    /// A stand-in for a test that is exercising one phase in isolation.
    ///
    /// `#[cfg(test)]`, so production still has exactly one way to obtain the
    /// proof — acquiring a surface. A test asserting what a phase computes is
    /// not asserting anything about the draw order, and should not have to
    /// stand up a GPU surface to say so.
    #[cfg(test)]
    pub(in crate::render_thread) const fn for_test() -> Self {
        Self(())
    }
}

/// A frame's surface, and the proof that acquiring it succeeded.
pub(in crate::render_thread) struct AcquiredSurface {
    pub(in crate::render_thread) output: wgpu::SurfaceTexture,
    pub(in crate::render_thread) acquired: SurfaceAcquired,
}

pub(super) fn acquire_current_texture(
    surface: &wgpu::Surface<'static>,
    device_lost: &mut DeviceLossDetector,
    emacs_frame_id: u64,
) -> Result<AcquiredSurface, FrameRenderFailure> {
    match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(output)
        | wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
            device_lost.record_surface_acquired();
            Ok(AcquiredSurface {
                output,
                acquired: SurfaceAcquired(()),
            })
        }
        wgpu::CurrentSurfaceTexture::Lost => {
            // A one-off Lost is a swapchain hiccup; an unbroken
            // streak means the device itself is gone (TDR) and only
            // a full GPU rebuild brings frames back.
            if device_lost.record_surface_lost() {
                tracing::error!(
                    "Surface for frame 0x{:x} lost {} times in a row: treating the wgpu device as lost",
                    emacs_frame_id,
                    CONSECUTIVE_SURFACE_LOST_THRESHOLD
                );
            } else {
                tracing::info!(
                    "Skipping redraw for frame 0x{:x}: surface lost",
                    emacs_frame_id
                );
            }
            Err(FrameRenderFailure::SurfaceLost)
        }
        wgpu::CurrentSurfaceTexture::Outdated => {
            tracing::info!(
                "Skipping redraw for frame 0x{:x}: surface outdated",
                emacs_frame_id
            );
            Err(FrameRenderFailure::SurfaceLost)
        }
        wgpu::CurrentSurfaceTexture::Timeout => Err(FrameRenderFailure::SurfaceTimeout),
        wgpu::CurrentSurfaceTexture::Occluded => Err(FrameRenderFailure::SurfaceOccluded),
        wgpu::CurrentSurfaceTexture::Validation => {
            tracing::warn!("Surface validation error for frame 0x{:x}", emacs_frame_id);
            Err(FrameRenderFailure::SurfaceTimeout)
        }
    }
}
