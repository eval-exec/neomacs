use neovm_core::emacs_core::forward::LispIntFwd;
use std::marker::PhantomData;
struct ThreadTransfer<T>(PhantomData<T>);
impl<T: Send + Sync> ThreadTransfer<T> {
    fn transfer() {}
}
fn main() {
    ThreadTransfer::<LispIntFwd>::transfer();
}
