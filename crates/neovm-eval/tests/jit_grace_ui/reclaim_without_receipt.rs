//! Reclaiming compiled leaves takes a serial receipt.
fn main() {
    neovm_eval::emacs_core::jit::cache::reclaim_with_receipt();
}
