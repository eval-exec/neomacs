use neovm_core::emacs_core::forward::LispKboardObjFwd;
use std::marker::PhantomData;
struct ThreadTransfer<T>(PhantomData<T>);
impl<T: Send + Sync> ThreadTransfer<T> {
    fn transfer() {}
}
fn main() {
    ThreadTransfer::<LispKboardObjFwd>::transfer();
}
