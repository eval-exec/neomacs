use neovm_core::emacs_core::Context;

fn requires_contract<T: Send>() {}

fn main() {
    requires_contract::<Context>();
}
