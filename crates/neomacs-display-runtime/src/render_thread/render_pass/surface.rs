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
pub(super) fn acquire_current_texture(
    surface: &wgpu::Surface<'static>,
    device_lost: &mut DeviceLossDetector,
    emacs_frame_id: u64,
) -> Result<wgpu::SurfaceTexture, FrameRenderFailure> {
    match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(output)
        | wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
            device_lost.record_surface_acquired();
            Ok(output)
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
