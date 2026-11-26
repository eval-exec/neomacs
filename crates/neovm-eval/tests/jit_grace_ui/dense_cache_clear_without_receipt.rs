//! DenseCache::clear takes a serial receipt.
fn main() {
    let mut cache = neovm_eval::emacs_core::jit::cache::DenseCache::default();
    cache.clear();
}
