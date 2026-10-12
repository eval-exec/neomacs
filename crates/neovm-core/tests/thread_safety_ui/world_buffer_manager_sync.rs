use neovm_core::buffer::buffer::BufferManager;
use std::marker::PhantomData;

struct ThreadShare<T>(PhantomData<T>);

impl<T: Sync> ThreadShare<T> {
    fn share() {}
}

fn main() {
    ThreadShare::<BufferManager>::share();
}
