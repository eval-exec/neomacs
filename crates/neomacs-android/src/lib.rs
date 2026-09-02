//! Android application adapter for Neomacs.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "android")]
mod platform;

use neomacs_app::host::HostProfile;

/// Capabilities exposed by the Android product adapter.
pub const fn host_profile() -> HostProfile {
    HostProfile::android()
}
