//! Browser WebAssembly application adapter for Neomacs.

#![forbid(unsafe_code)]

use neomacs_app::host::HostProfile;

#[cfg(target_family = "wasm")]
mod platform;

/// Capabilities exposed by the browser WebAssembly product adapter.
pub const fn host_profile() -> HostProfile {
    HostProfile::WASM
}
