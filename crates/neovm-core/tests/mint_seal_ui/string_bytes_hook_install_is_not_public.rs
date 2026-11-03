//! N9: the byte-mutation stamp installer is seam-only authority. Today:
//! private (E0603). After the split promotes it to `pub` in neovm-value: a
//! safe call fails with E0133.
fn main() {
    let _ = neovm_core::heap_types::install_string_bytes_changed_hook(|| ());
}
