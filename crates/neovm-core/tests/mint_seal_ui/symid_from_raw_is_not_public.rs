//! N8: minting a SymId from a raw id is crate authority, not public API.
//! Today: private (E0624). After the split promotes `from_raw` to `pub` in
//! neovm-value: a safe call fails with E0133.
use neovm_core::emacs_core::intern::SymId;

fn main() {
    let _ = SymId::from_raw(1);
}
