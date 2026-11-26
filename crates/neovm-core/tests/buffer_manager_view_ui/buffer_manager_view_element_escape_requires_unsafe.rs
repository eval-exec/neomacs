//! rt A3 2c (d) Fixture 3: `BufferManagerMut` does not expose safe `&mut Buffer`
//! element getters (`get_mut` / `current_buffer_mut`), its single audited
//! closure-scoped element escape (`with_buffer_mut_no_swap`) requires `unsafe`,
//! and the `&mut Buffer` argument cannot escape the closure scope.
#![allow(unused_mut)]

use neovm_core::buffer::{Buffer, BufferId, BufferManagerMut};

fn safe_call_to_escape_fails(mut view: BufferManagerMut<'_>, id: BufferId) {
    let _ = view.with_buffer_mut_no_swap(id, |_buf| ());
}

fn safe_get_mut_rejected(mut view: BufferManagerMut<'_>, id: BufferId) {
    let _ = view.get_mut(id);
}

fn safe_current_buffer_mut_rejected(mut view: BufferManagerMut<'_>) {
    let _ = view.current_buffer_mut();
}

fn escape_buffer_ref_out_of_closure_rejected<'a>(
    mut view: BufferManagerMut<'a>,
    id: BufferId,
) -> Option<&'a mut Buffer> {
    let mut escaped: Option<&'a mut Buffer> = None;
    unsafe {
        let _ = view.with_buffer_mut_no_swap(id, |buf| {
            escaped = Some(buf);
        });
    }
    escaped
}

fn main() {}
