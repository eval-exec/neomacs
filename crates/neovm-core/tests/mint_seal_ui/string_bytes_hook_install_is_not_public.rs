//! N9: the byte-mutation stamp installer is seam-only authority. At its
//! defining path (neovm-value) it is `pub unsafe fn`: a SAFE call fails with
//! E0133. The table itself stays private (companion golden
//! string_bytes_hook_field_cannot_be_forged).
static STAMP: fn() = || {};

fn main() {
    let _ = neovm_value::heap_types::install_string_bytes_changed_hook(&STAMP);
}
