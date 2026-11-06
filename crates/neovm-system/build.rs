//! Build-script detection for the host-system crate.
//!
//! The D-Bus transport moved here with `dbusbind` (stage 6). The probe itself
//! is the workspace's single host-cfg implementation, shared with neovm-core,
//! neovm-value and neovm-eval, so every crate's `neomacs_have_dbus` view
//! agrees (lead ruling 04:01Z; see build_support/host_cfg.rs).

#[path = "../../build_support/host_cfg.rs"]
mod host_cfg;

fn main() {
    host_cfg::configure_all();
}
