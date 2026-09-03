//! Browser WebAssembly application adapter for Neomacs.

#![forbid(unsafe_code)]

use neomacs_app::host::HostProfile;

#[cfg(target_family = "wasm")]
mod platform;
#[cfg(any(target_family = "wasm", test))]
mod presentation_readiness;

pub use neomacs_wasm_protocol as worker_protocol;

/// Capabilities exposed by the browser WebAssembly product adapter.
pub const fn host_profile() -> HostProfile {
    HostProfile::WASM
}
