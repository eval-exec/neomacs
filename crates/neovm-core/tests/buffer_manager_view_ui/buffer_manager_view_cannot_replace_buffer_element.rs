//! rt A3 2c (d) Fixture 4: safe code cannot `std::mem::replace` a `Buffer`
//! element inside `BufferManagerMut` with a freshly constructed `Buffer`.
#![allow(unused_mut)]

use neovm_core::buffer::{Buffer, BufferId, BufferManagerMut};
use neovm_core::emacs_core::value::Value;

fn replace_via_get_mut_rejected(mut view: BufferManagerMut<'_>, id: BufferId) {
    if let Some(slot) = view.get_mut(id) {
        let _ = std::mem::replace(slot, Buffer::new_standalone(id, Value::NIL));
    }
}

fn replace_via_escape_without_unsafe_rejected(mut view: BufferManagerMut<'_>, id: BufferId) {
    let _ = view.with_buffer_mut_no_swap(id, |slot| {
        let _ = std::mem::replace(slot, Buffer::new_standalone(id, Value::NIL));
    });
}

fn main() {}
