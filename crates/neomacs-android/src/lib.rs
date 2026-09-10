//! Android application adapter for Neomacs.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod environment;

// Exercise the platform-independent connection policy on the host as well.
#[cfg(all(test, not(target_os = "android")))]
#[path = "platform/ime/connection.rs"]
mod ime_connection;

#[cfg(target_os = "android")]
mod platform;

use neomacs_app::host::HostProfile;

/// Capabilities exposed by the Android product adapter.
pub const fn host_profile() -> HostProfile {
    HostProfile::android()
}
