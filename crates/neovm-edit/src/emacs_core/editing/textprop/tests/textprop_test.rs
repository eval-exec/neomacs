use super::*;

use crate::buffer::{CharLen, CharRange};

use crate::emacs_core::buffer::{
    builtin_delete_overlay, builtin_make_overlay, builtin_move_overlay,
    builtin_next_overlay_change, builtin_overlay_buffer, builtin_overlay_end, builtin_overlay_get,
    builtin_overlay_properties, builtin_overlay_put, builtin_overlay_start, builtin_overlayp,
    builtin_overlays_at, builtin_overlays_in, builtin_previous_overlay_change,
};

use crate::emacs_core::textprop_entries::{
    builtin_get_char_property, builtin_get_char_property_and_overlay,
};

use malachite::Integer;

use neovm_value::error::{FlowKind, FlowResultExt as _};

#[cfg(test)]
#[path = "string_property_removal.rs"]
mod string_property_removal;
