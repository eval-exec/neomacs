//! GPU device-loss detection for the render thread.
//!
//! A user shader with an infinite loop can hang the GPU
//! (`doc/display-engine/SHADER_SURFACES.md`: naga guarantees memory safety,
//! not termination). The driver then resets (TDR) and the wgpu device is
//! lost. Two signals feed one recovery decision:
//!
//! 1. wgpu's device-lost callback (`Device::set_device_lost_callback`),
//!    which may fire on any thread from inside wgpu's maintain paths, and
//! 2. a streak of consecutive `CurrentSurfaceTexture::Lost` acquisitions —
//!    some backends only report a reset through the swapchain.
//!
//! Both set one shared flag; `handle_about_to_wait` drains it and runs
//! `RenderApp::recover_from_device_loss`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Consecutive `CurrentSurfaceTexture::Lost` results escalated to a device
/// loss. A one-off Lost is a normal swapchain hiccup and is merely skipped;
/// a device that keeps answering Lost never presents again without a
/// rebuild.
pub(super) const CONSECUTIVE_SURFACE_LOST_THRESHOLD: u32 = 30;

/// Shared device-lost latch plus the consecutive surface-Lost streak.
pub(super) struct DeviceLossDetector {
    /// Set by the wgpu device-lost callback (any thread) or by the streak
    /// escalation; drained once per event-loop pass by [`Self::take`].
    flag: Arc<AtomicBool>,
    consecutive_surface_lost: u32,
}

impl DeviceLossDetector {
    pub(super) fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            consecutive_surface_lost: 0,
        }
    }

    /// Clone of the shared flag for the wgpu device-lost callback.
    pub(super) fn shared_flag(&self) -> Arc<AtomicBool> {
        self.flag.clone()
    }

    /// Latch a loss from the render thread itself (debug simulation, or a
    /// failed recovery that must be retried on the next pass).
    pub(super) fn mark_lost_now(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Drain the latch; true means a recovery pass must run now.
    pub(super) fn take(&mut self) -> bool {
        let lost = self.flag.swap(false, Ordering::SeqCst);
        if lost {
            self.consecutive_surface_lost = 0;
        }
        lost
    }

    /// Record one `CurrentSurfaceTexture::Lost` acquisition. Returns true
    /// when the streak reaches the threshold, at which point the shared flag
    /// has been latched and the streak reset.
    pub(super) fn record_surface_lost(&mut self) -> bool {
        self.consecutive_surface_lost += 1;
        if self.consecutive_surface_lost >= CONSECUTIVE_SURFACE_LOST_THRESHOLD {
            self.consecutive_surface_lost = 0;
            self.flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Record a successful surface acquisition, resetting the streak.
    pub(super) fn record_surface_acquired(&mut self) {
        self.consecutive_surface_lost = 0;
    }
}

#[cfg(test)]
#[path = "device_loss_test.rs"]
mod tests;
