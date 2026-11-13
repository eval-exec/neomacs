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

/// Compile the C long-double bridge with Cargo's target toolchain. `cc` also
/// discovers Visual Studio tools outside PATH and selects the target CRT.
///
/// The bridge lives in the shared workspace `build_support` tree because the
/// only consumer is this crate's float formatting (the moved
/// `emacs_core::lisp::native::builtins::strings`). Rust has no C long-double
/// ABI, so the conversion stays in C. Compiling it here — rather than in
/// `neovm-core`'s build script, where it lived before the evaluator split —
/// is what makes the symbol available to every binary that links this crate
/// (the display and eval test binaries link eval without core).
fn compile_float_format() {
    cc::Build::new()
        .file("../../build_support/float_format.c")
        .std("c11")
        .pic(true)
        .compile("neovm_float_format");
}

fn main() {
    println!("cargo:rerun-if-changed=../../build_support/float_format.c");
    compile_float_format();
    host_cfg::configure_all();
}
