use neovm_core::emacs_core::jit::compile::CompiledLeaf;

fn requires_contract<T: Sync>() {}

fn main() {
    requires_contract::<CompiledLeaf>();
}
