//! Host build facts for neovm-value: the target triple used by the system
//! vocabulary's PATH_EXEC resolution, and the shared host capability cfgs
//! (see build_support/host_cfg.rs).

#[path = "../../build_support/host_cfg.rs"]
mod host_cfg;

fn main() {
    // The target triple is a compile-time fact the lower system vocabulary
    // needs for PATH_EXEC resolution (moved down from neovm-core's build
    // script with the stage-4 W1/E3 TYPE-DOWN wave).
    println!(
        "cargo:rustc-env=NEOVM_HOST_TRIPLE={}",
        std::env::var("TARGET").expect("cargo sets TARGET for build scripts")
    );
    host_cfg::configure_all();
}
