//! Compile-target-selected identity for the current Emacs process.
//!
//! Native builds expose the operating-system process ID. A browser Worker is
//! already an isolated editor process but has no OS PID, so its Lisp-visible
//! identity is the stable synthetic value `1`.

/// Return the current editor process identity.
#[must_use]
pub fn id() -> u32 {
    std::cfg_select! {
        target_family = "wasm" => { 1 }
        _ => { std::process::id() }
    }
}
