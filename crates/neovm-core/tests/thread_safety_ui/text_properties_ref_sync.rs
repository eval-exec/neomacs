use neovm_core::buffer::text_props::TextPropertiesRef;
use std::marker::PhantomData;

struct ThreadTransfer<T>(PhantomData<T>);

impl<T: Sync> ThreadTransfer<T> {
    fn transfer() {}
}

fn main() {
    ThreadTransfer::<TextPropertiesRef<'static>>::transfer();
}
