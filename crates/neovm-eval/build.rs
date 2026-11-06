//! Host capability cfgs for neovm-eval.
//!
//! Eval's source branches on `cfg(neomacs_have_dbus)` (while-no-input ignore
//! list and coupled-variable policy) and `cfg(neomacs_have_lcms2)` (the lcms
//! subr batch and its candidate-library env). Before this script existed eval
//! emitted no cfgs at all, so those branches were permanently false after the
//! stage-4 move. The detection is the workspace's single shared probe (lead
//! ruling 04:01Z; see build_support/host_cfg.rs).

#[path = "../../build_support/host_cfg.rs"]
mod host_cfg;

fn main() {
    host_cfg::configure_all();
}
