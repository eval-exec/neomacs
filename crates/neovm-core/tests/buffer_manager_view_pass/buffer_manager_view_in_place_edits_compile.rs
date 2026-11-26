//! rt A3 2c (d) Positive pass control: safe in-place edits and reads through
//! `BufferManagerMut` and `EditRuntimeMut` compile without `unsafe`.

use neovm_core::buffer::EmacsBytePos;
use neovm_core::emacs_core::eval::Context;

#[allow(dead_code)]
fn safe_in_place_edits_through_views(ctx: &mut Context) {
    let id = {
        let mut bm = ctx.buffer_manager_view_mut();
        let id = bm.create_buffer("pass-control");
        bm.set_current(id);
        let _ = bm.replace_buffer_contents(id, "abc");
        let _ = bm.goto_buffer_emacs_byte_pos(id, EmacsBytePos::new(2));
        let _current = bm.current_buffer_id();
        id
    };
    {
        let (_frames, mut bm) = ctx.frame_manager_and_buffer_view_mut();
        let _ = bm.set_buffer_modified_flag(id, false);
    }
    {
        let mut edit = ctx.edit_view_mut();
        edit.set_combine_after_change_buffer(Some(id));
        edit.push_combine_after_change_record((0, 0, 3));
        edit.clear_combine_after_change_list();
        edit.clear_last_overlay_modification_hooks();
        let _ = edit.buffers_mut().note_overlay_modification(id);
    }
}

fn main() {}
