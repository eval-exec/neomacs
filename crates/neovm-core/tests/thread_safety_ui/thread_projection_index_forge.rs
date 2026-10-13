use neovm_core::emacs_core::forward::TlsIndex;
use std::num::NonZeroU32;

fn main() {
    let _forged = TlsIndex(NonZeroU32::MIN);
}
