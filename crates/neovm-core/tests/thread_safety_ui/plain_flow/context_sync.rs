use neovm_core::emacs_core::Context;
use std::marker::PhantomData;

struct ThreadTransfer<T>(PhantomData<T>);

impl<T: Sync> ThreadTransfer<T> {
    fn transfer() {}
}

fn main() {
    ThreadTransfer::<Context>::transfer();
}
