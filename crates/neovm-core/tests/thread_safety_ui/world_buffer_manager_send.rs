use neovm_core::buffer::buffer::BufferManager;
use std::marker::PhantomData;

struct ThreadTransfer<T>(PhantomData<T>);

impl<T: Send> ThreadTransfer<T> {
    fn transfer() {}
}

fn main() {
    ThreadTransfer::<BufferManager>::transfer();
}
