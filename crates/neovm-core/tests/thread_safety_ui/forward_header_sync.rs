use neovm_core::emacs_core::forward::LispFwd;
use std::marker::PhantomData;
struct ThreadTransfer<T>(PhantomData<T>);
impl<T: Sync> ThreadTransfer<T> {
    fn transfer() {}
}
fn main() {
    ThreadTransfer::<LispFwd>::transfer();
}
