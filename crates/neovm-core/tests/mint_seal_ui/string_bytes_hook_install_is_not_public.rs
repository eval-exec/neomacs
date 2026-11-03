//! N9: the byte-mutation stamp installer is seam-only authority. Promoted
//! `pub` per the peer-ruled PC2 shape (the stamp is a `&'static fn()`
//! identity, like the other one-time hook installers): a safe call fails
//! with E0133. The table itself stays private (companion golden
//! string_bytes_hook_field_cannot_be_forged).
static STAMP: fn() = || {};

fn main() {
    let _ = neovm_core::heap_types::install_string_bytes_changed_hook(&STAMP);
}
