// Only test builds of the crate itself have a projection table to issue from.
use neovm_core::emacs_core::forward::ThreadProjectionTable;

fn main() {
    let _ = std::mem::size_of::<ThreadProjectionTable>();
}
