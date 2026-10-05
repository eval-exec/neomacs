use neovm_core::emacs_core::jit::compile::CompiledLeaf;
use std::marker::PhantomData;

struct ThreadTransfer<T>(PhantomData<T>);

impl<T: Sync> ThreadTransfer<T> {
    fn transfer() {}
}

fn main() {
    ThreadTransfer::<CompiledLeaf>::transfer();
}
