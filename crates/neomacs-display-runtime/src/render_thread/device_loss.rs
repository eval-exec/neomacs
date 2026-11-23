//! GPU device-loss detection for the render thread.
//!
//! A user shader with an infinite loop can hang the GPU
//! (`docs/display-engine/SHADER_SURFACES.md`: naga guarantees memory safety,
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

/// What exit does with the wgpu context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GpuExitDisposition {
    /// Live device. Drop device, queue, and instance. Forget the adapter so
    /// `eglTerminate` does not run after the Wayland display is gone.
    Release,
    /// Lost device. `Drop` waits inside the wedged driver. Forget the
    /// context; the process is exiting and the OS reclaims it.
    Abandon,
}

/// Recovery runs only after a device loss. Dropping the lost renderer or the
/// GPU context waits inside the wedged driver, so recovery always abandons.
pub(super) fn gpu_recovery_disposition() -> GpuExitDisposition {
    GpuExitDisposition::Abandon
}

/// A latched device loss must not drop into the wedged driver on exit.
pub(super) fn gpu_exit_disposition(device_lost: bool) -> GpuExitDisposition {
    if device_lost {
        GpuExitDisposition::Abandon
    } else {
        GpuExitDisposition::Release
    }
}

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

    /// Read the latch without draining it. Exit uses this so a lost device
    /// is not dropped into a wedged driver.
    pub(super) fn is_latched(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
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
#[path = "device_loss/tests/device_loss_test.rs"]
mod tests;
