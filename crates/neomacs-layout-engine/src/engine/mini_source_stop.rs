//! Admit GNU move_it_to(ZV)'s hard-newline stop before eager display
//! condition evaluation. One exclusive attempt owns all borrowed source state;
//! the returned policy is numeric and retains no Lisp value or global cache.
use crate::display_when::DisplayWhenEndBoundary;
use crate::neovm_bridge::{
    LayoutBufferView, LayoutCharPropertyLookup, buffer_has_active_display_table,
};
use crate::types::{MiniWindowMeasurement, WindowParams};
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::{Context, Value};

#[cold]
#[inline(never)]
pub(super) fn condition_end_boundary(
    eval: &Context,
    params: &WindowParams,
    fontify_end: i64,
) -> DisplayWhenEndBoundary {
    let inclusive = DisplayWhenEndBoundary::InclusiveAnchor;
    if params.mini_measurement != MiniWindowMeasurement::ToEnd
        || !eval.gnu_redisplay_hooks_policy_enabled()
        || params.selective_display != 0
        || fontify_end != params.accessible_end_charpos().get()
        || fontify_end <= params.accessible_start_charpos().get()
    {
        return inclusive;
    }
    let Some(buffer) = eval
        .buffer_manager()
        .get(neovm_core::buffer::BufferId(params.buffer_id))
    else {
        return inclusive;
    };
    let last = CharPos0::new(fontify_end.saturating_sub(1) as usize);
    let byte = buffer.layout_char_pos_to_emacs_byte_pos(last);
    if buffer.char_at_emacs_byte_pos(byte) != Some('\n') || buffer_has_active_display_table(buffer)
    {
        return inclusive;
    }
    // Effective character lookup includes category, alias/default and the
    // highest-priority window-scoped overlay. Never evaluate a condition to
    // prove that its own source cannot replace or hide the terminal newline.
    for name in ["display", "invisible", "composition"] {
        if LayoutCharPropertyLookup::new(buffer, Value::symbol(name))
            .overlay_or_text_source_at(buffer, byte, Some(params.window_id as u64))
            .is_some_and(|source| !source.value.is_nil())
        {
            return inclusive;
        }
    }
    DisplayWhenEndBoundary::BufferPositionReached
}

#[cfg(test)]
#[path = "tests/mini_source_stop_test.rs"]
mod tests;
