use neovm_core::buffer::text_props::TextPropertiesRef;
use std::marker::PhantomData;

struct ThreadTransfer<T>(PhantomData<T>);

impl<T: Send> ThreadTransfer<T> {
    fn transfer() {}
}

fn main() {
    ThreadTransfer::<TextPropertiesRef<'static>>::transfer();
}
