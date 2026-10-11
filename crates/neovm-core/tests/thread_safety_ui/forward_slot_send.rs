use neovm_core::emacs_core::forward::ForwardSlot;
use std::marker::PhantomData;
struct ThreadTransfer<T>(PhantomData<T>);
impl<T: Send> ThreadTransfer<T> {
    fn transfer() {}
}
fn main() {
    ThreadTransfer::<ForwardSlot<'static>>::transfer();
}
