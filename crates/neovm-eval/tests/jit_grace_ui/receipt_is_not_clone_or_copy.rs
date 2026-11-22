//! The receipt is not Clone or Copy.
fn needs_clone_and_copy<T: Clone + Copy>() {}

fn main() {
    needs_clone_and_copy::<neovm_eval::emacs_core::jit::cache::JitGracePeriod>();
}
