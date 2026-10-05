use neovm_core::emacs_core::jit::compile::CompiledLeaf;

fn requires_contract<T: Send>() {}

fn main() {
    requires_contract::<CompiledLeaf>();
}
