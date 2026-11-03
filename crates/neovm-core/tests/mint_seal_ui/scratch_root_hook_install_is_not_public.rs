//! N9: the scratch-root hook installer is seam-only authority. Today the
//! bridge module is crate-internal (E0603). After the split promotes the
//! installer to `pub` in neovm-value: a safe call fails with E0133.
fn main() {
    let _install = neovm_core::emacs_core::value::scratch_bridge::install_scratch_root_hooks;
}
