//! Host capability cfgs for neovm-display.
//!
//! Display's own source does not branch on the host cfgs, but its cfg(test)
//! unit runtime `include!`s eval's H3 upper snapshot, whose rows carry
//! `#[cfg(neomacs_have_dbus)]`. Those attributes are evaluated in THIS crate,
//! so display must emit the same shared probe view as eval (peer ruling
//! 04:01:35Z (2)); otherwise the compiled table silently drops the dbus rows
//! while host_cfg::HostCfgClass (compiled in eval) still marks them active.

#[path = "../../build_support/host_cfg.rs"]
mod host_cfg;

fn main() {
    host_cfg::configure_all();
}
