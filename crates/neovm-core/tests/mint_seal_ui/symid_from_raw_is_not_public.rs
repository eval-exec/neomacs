//! N8: minting a SymId from a raw id is crate authority, not public API. At
//! its defining path in neovm-value `from_raw` is a `#[doc(hidden)] pub const
//! unsafe fn`; a SAFE call fails with E0133.
use neovm_value::intern::SymId;

fn main() {
    let _ = SymId::from_raw(1);
}
