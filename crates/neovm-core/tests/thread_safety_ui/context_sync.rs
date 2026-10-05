use neovm_core::emacs_core::Context;

fn requires_contract<T: Sync>() {}

fn main() {
    requires_contract::<Context>();
}
