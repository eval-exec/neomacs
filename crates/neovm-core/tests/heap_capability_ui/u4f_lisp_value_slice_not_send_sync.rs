use neovm_core::tagged::header::{LispValueSlice, LispValueSliceIter};

fn assert_send<T: ?Sized + Send>() {}
fn assert_sync<T: ?Sized + Sync>() {}

fn main() {
    assert_send::<LispValueSlice>();
    assert_sync::<LispValueSlice>();
    assert_send::<LispValueSliceIter<'_>>();
    assert_sync::<LispValueSliceIter<'_>>();
}
