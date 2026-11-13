//! The display-owned C14 registrar (D1 / H3a-style display registrar split).
//!
//! Signed shape (LEAD 03:4xZ / peer 03:4xZ): production-order-preserving; ONE
//! typed registrar fn per canonical display step, called by core exactly where
//! the code it replaces ran. Steps, ids and boundaries are gapsurvey canonical
//! generated canonical plan (display_segment ids, descriptive/stable).
//!
//! Batch runs call the existing localized batch registrars in their canonical
//! order; inline runs register their moved specs in canonical order. The two
//! macOS-gated ns-resource specs keep their cfg.

use crate::emacs_core::frame::{builtin_next_frame, builtin_previous_frame};
use neovm_eval::emacs_core::builtins::{
    set_frame_height_interactive_spec, set_frame_width_interactive_spec,
};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::subr::{FixedMin1, NativeFn, NoEvalPlaceholder, SubrArity, SubrSpec};

/// Canonical display step `terminal_shader_video_xwidget` (46 registrations).
#[doc(hidden)]
pub fn register_terminal_shader_video_xwidget(ctx: &mut Context) {
    crate::emacs_core::neo::terminal::register_subrs(ctx);
    crate::emacs_core::shader_surface::register_subrs(ctx);
    crate::emacs_core::video::register_subrs(ctx);
    crate::emacs_core::xwidget::register_subrs(ctx);
}

/// Canonical display step `buffer_text_pixel_size` (1 registrations).
const SPEC_BUFFER_TEXT_PIXEL_SIZE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-text-pixel-size",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_buffer_text_pixel_size),
        SubrArity::new(0, Some(4)),
    ));
#[doc(hidden)]
pub fn register_buffer_text_pixel_size(ctx: &mut Context) {
    ctx.register_subr(SPEC_BUFFER_TEXT_PIXEL_SIZE_00);
}

/// Canonical display step `format_mode_line` (7 registrations).
const SPEC_FORMAT_MODE_LINE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "format-mode-line",
        NativeFn::ContextSlice(crate::emacs_core::xdisp::builtin_format_mode_line_ctx),
        SubrArity::new(1, Some(4)),
    ));
const SPEC_FORMAT_MODE_LINE_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-line-height",
        NativeFn::ContextSlice(crate::emacs_core::xdisp::builtin_window_line_height),
        SubrArity::new(0, Some(2)),
    ))
    .placeholder(NoEvalPlaceholder::WindowLineHeight);
const SPEC_FORMAT_MODE_LINE_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "posn-at-point",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_posn_at_point),
        SubrArity::new(0, Some(2)),
    ));
const SPEC_FORMAT_MODE_LINE_03: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "posn-at-x-y",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_posn_at_x_y),
        SubrArity::new(2, Some(4)),
    ));
const SPEC_FORMAT_MODE_LINE_04: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "coordinates-in-window-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_coordinates_in_window_p),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_FORMAT_MODE_LINE_05: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "tool-bar-height",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_tool_bar_height_ctx),
        SubrArity::new(0, Some(2)),
    ));
const SPEC_FORMAT_MODE_LINE_06: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "tab-bar-height",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_tab_bar_height_ctx),
        SubrArity::new(0, Some(2)),
    ));
#[doc(hidden)]
pub fn register_format_mode_line(ctx: &mut Context) {
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_00);
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_01);
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_02);
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_03);
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_04);
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_05);
    ctx.register_subr(SPEC_FORMAT_MODE_LINE_06);
}

/// Canonical display step `selected_window` (67 registrations).
const SPEC_SELECTED_WINDOW_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "selected-window",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_selected_window),
    SubrArity::new(0, Some(0)),
));
const SPEC_SELECTED_WINDOW_01: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "old-selected-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_old_selected_window),
    SubrArity::new(0, Some(0)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_02: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "minibuffer-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_minibuffer_window),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_03: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-parameter",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_parameter),
    SubrArity::new(2, Some(2)),
));
const SPEC_SELECTED_WINDOW_04: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-parameter",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_parameter),
    SubrArity::new(3, Some(3)),
));
const SPEC_SELECTED_WINDOW_05: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-parameters",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_parameters),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_06: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-parent",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_parent),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_07: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-top-child",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_top_child),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_08: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-left-child",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_left_child),
    SubrArity::new(0, Some(1)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_09: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-next-sibling",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_next_sibling),
    SubrArity::new(0, Some(1)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_10: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-prev-sibling",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_prev_sibling),
    SubrArity::new(0, Some(1)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_11: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-normal-size",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_normal_size),
    SubrArity::new(0, Some(2)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_12: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-display-table",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_display_table),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_13: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-cursor-type",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_cursor_type),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_14: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-buffer",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_buffer),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_15: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-start",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_start),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_16: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-end",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_end),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_17: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-point",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_point),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_18: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-use-time",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_use_time),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_19: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-bump-use-time",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_bump_use_time),
    SubrArity::new(0, Some(1)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_20: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-old-point",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_point),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_21: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-old-buffer",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_buffer),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_22: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-prev-buffers",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_prev_buffers),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_23: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-next-buffers",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_next_buffers),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_24: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-left-column",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_left_column),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_25: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-top-line",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_top_line),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_26: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-pixel-left",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_pixel_left),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_27: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-pixel-top",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_pixel_top),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_28: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-hscroll",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_hscroll),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_29: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-vscroll",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_vscroll),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_30: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-margins",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_margins),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_31: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-fringes",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_fringes),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_32: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-scroll-bars",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_scroll_bars),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_33: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-pixel-height",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_pixel_height),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_34: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-pixel-width",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_pixel_width),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_35: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-body-height",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_body_height),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_36: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-body-width",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_body_width),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_37: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-text-height",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_text_height),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_38: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-text-width",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_text_width),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_39: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-total-height",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_total_height),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_40: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-total-width",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_total_width),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_41: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-list",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_list),
    SubrArity::new(0, Some(3)),
));
const SPEC_SELECTED_WINDOW_42: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-list-1",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_list_1),
    SubrArity::new(0, Some(3)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_43: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "get-buffer-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_get_buffer_window),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_44: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-dedicated-p",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_dedicated_p),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_45: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-minibuffer-p",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_minibuffer_p),
    SubrArity::new(0, Some(1)),
));
const SPEC_SELECTED_WINDOW_46: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-at",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_at),
    SubrArity::new(2, Some(3)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_47: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-live-p",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_live_p),
    SubrArity::new(1, Some(1)),
));
const SPEC_SELECTED_WINDOW_48: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-start",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_start),
    SubrArity::new(2, Some(3)),
));
const SPEC_SELECTED_WINDOW_49: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-hscroll",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_hscroll),
    SubrArity::new(2, Some(2)),
));
const SPEC_SELECTED_WINDOW_50: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-margins",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_margins),
    SubrArity::new(2, Some(3)),
));
const SPEC_SELECTED_WINDOW_51: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-fringes",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_fringes),
    SubrArity::new(2, Some(5)),
));
const SPEC_SELECTED_WINDOW_52: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-vscroll",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_set_window_vscroll),
    SubrArity::new(2, Some(4)),
));
const SPEC_SELECTED_WINDOW_53: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-point",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_point),
    SubrArity::new(2, Some(2)),
));
const SPEC_SELECTED_WINDOW_54: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "split-window-internal",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_split_window_internal),
    SubrArity::new(4, Some(5)),
))
.requires_eval_state();
const SPEC_SELECTED_WINDOW_55: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "select-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_select_window),
    SubrArity::new(1, Some(2)),
));
const SPEC_SELECTED_WINDOW_56: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "scroll-up",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_up),
    SubrArity::new(0, Some(1)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("^P"),
);
const SPEC_SELECTED_WINDOW_57: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "scroll-down",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_down),
    SubrArity::new(0, Some(1)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("^P"),
);
const SPEC_SELECTED_WINDOW_58: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "scroll-left",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_left),
    SubrArity::new(0, Some(2)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("^P\np"),
);
const SPEC_SELECTED_WINDOW_59: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "scroll-right",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_right),
    SubrArity::new(0, Some(2)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("^P\np"),
);
const SPEC_SELECTED_WINDOW_60: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-resize-apply",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_resize_apply),
    SubrArity::new(0, Some(2)),
));
const SPEC_SELECTED_WINDOW_61: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "recenter",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_recenter),
    SubrArity::new(0, Some(2)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("P\np"),
);
const SPEC_SELECTED_WINDOW_62: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "next-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_next_window),
    SubrArity::new(0, Some(3)),
));
const SPEC_SELECTED_WINDOW_63: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "previous-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_previous_window),
    SubrArity::new(0, Some(3)),
));
const SPEC_SELECTED_WINDOW_64: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-window-buffer",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_buffer),
    SubrArity::new(2, Some(3)),
));
const SPEC_SELECTED_WINDOW_65: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "neomacs--record-window-navigation-intent",
    NativeFn::ContextVec(
        crate::emacs_core::window_cmds::builtin_neomacs_record_window_navigation_intent,
    ),
    SubrArity::new(1, Some(2)),
));
const SPEC_SELECTED_WINDOW_66: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "neomacs--record-frame-navigation-intent",
    NativeFn::ContextVec(
        crate::emacs_core::window_cmds::builtin_neomacs_record_frame_navigation_intent,
    ),
    SubrArity::new(1, Some(2)),
));
#[doc(hidden)]
pub fn register_selected_window(ctx: &mut Context) {
    ctx.register_subr(SPEC_SELECTED_WINDOW_00);
    ctx.register_subr(SPEC_SELECTED_WINDOW_01);
    ctx.register_subr(SPEC_SELECTED_WINDOW_02);
    ctx.register_subr(SPEC_SELECTED_WINDOW_03);
    ctx.register_subr(SPEC_SELECTED_WINDOW_04);
    ctx.register_subr(SPEC_SELECTED_WINDOW_05);
    ctx.register_subr(SPEC_SELECTED_WINDOW_06);
    ctx.register_subr(SPEC_SELECTED_WINDOW_07);
    ctx.register_subr(SPEC_SELECTED_WINDOW_08);
    ctx.register_subr(SPEC_SELECTED_WINDOW_09);
    ctx.register_subr(SPEC_SELECTED_WINDOW_10);
    ctx.register_subr(SPEC_SELECTED_WINDOW_11);
    ctx.register_subr(SPEC_SELECTED_WINDOW_12);
    ctx.register_subr(SPEC_SELECTED_WINDOW_13);
    ctx.register_subr(SPEC_SELECTED_WINDOW_14);
    ctx.register_subr(SPEC_SELECTED_WINDOW_15);
    ctx.register_subr(SPEC_SELECTED_WINDOW_16);
    ctx.register_subr(SPEC_SELECTED_WINDOW_17);
    ctx.register_subr(SPEC_SELECTED_WINDOW_18);
    ctx.register_subr(SPEC_SELECTED_WINDOW_19);
    ctx.register_subr(SPEC_SELECTED_WINDOW_20);
    ctx.register_subr(SPEC_SELECTED_WINDOW_21);
    ctx.register_subr(SPEC_SELECTED_WINDOW_22);
    ctx.register_subr(SPEC_SELECTED_WINDOW_23);
    ctx.register_subr(SPEC_SELECTED_WINDOW_24);
    ctx.register_subr(SPEC_SELECTED_WINDOW_25);
    ctx.register_subr(SPEC_SELECTED_WINDOW_26);
    ctx.register_subr(SPEC_SELECTED_WINDOW_27);
    ctx.register_subr(SPEC_SELECTED_WINDOW_28);
    ctx.register_subr(SPEC_SELECTED_WINDOW_29);
    ctx.register_subr(SPEC_SELECTED_WINDOW_30);
    ctx.register_subr(SPEC_SELECTED_WINDOW_31);
    ctx.register_subr(SPEC_SELECTED_WINDOW_32);
    ctx.register_subr(SPEC_SELECTED_WINDOW_33);
    ctx.register_subr(SPEC_SELECTED_WINDOW_34);
    ctx.register_subr(SPEC_SELECTED_WINDOW_35);
    ctx.register_subr(SPEC_SELECTED_WINDOW_36);
    ctx.register_subr(SPEC_SELECTED_WINDOW_37);
    ctx.register_subr(SPEC_SELECTED_WINDOW_38);
    ctx.register_subr(SPEC_SELECTED_WINDOW_39);
    ctx.register_subr(SPEC_SELECTED_WINDOW_40);
    ctx.register_subr(SPEC_SELECTED_WINDOW_41);
    ctx.register_subr(SPEC_SELECTED_WINDOW_42);
    ctx.register_subr(SPEC_SELECTED_WINDOW_43);
    ctx.register_subr(SPEC_SELECTED_WINDOW_44);
    ctx.register_subr(SPEC_SELECTED_WINDOW_45);
    ctx.register_subr(SPEC_SELECTED_WINDOW_46);
    ctx.register_subr(SPEC_SELECTED_WINDOW_47);
    ctx.register_subr(SPEC_SELECTED_WINDOW_48);
    ctx.register_subr(SPEC_SELECTED_WINDOW_49);
    ctx.register_subr(SPEC_SELECTED_WINDOW_50);
    ctx.register_subr(SPEC_SELECTED_WINDOW_51);
    ctx.register_subr(SPEC_SELECTED_WINDOW_52);
    ctx.register_subr(SPEC_SELECTED_WINDOW_53);
    ctx.register_subr(SPEC_SELECTED_WINDOW_54);
    ctx.register_subr(SPEC_SELECTED_WINDOW_55);
    ctx.register_subr(SPEC_SELECTED_WINDOW_56);
    ctx.register_subr(SPEC_SELECTED_WINDOW_57);
    ctx.register_subr(SPEC_SELECTED_WINDOW_58);
    ctx.register_subr(SPEC_SELECTED_WINDOW_59);
    ctx.register_subr(SPEC_SELECTED_WINDOW_60);
    ctx.register_subr(SPEC_SELECTED_WINDOW_61);
    ctx.register_subr(SPEC_SELECTED_WINDOW_62);
    ctx.register_subr(SPEC_SELECTED_WINDOW_63);
    ctx.register_subr(SPEC_SELECTED_WINDOW_64);
    ctx.register_subr(SPEC_SELECTED_WINDOW_65);
    ctx.register_subr(SPEC_SELECTED_WINDOW_66);
}

/// Canonical display step `selected_frame` (1 registrations).
const SPEC_SELECTED_FRAME_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "selected-frame",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_selected_frame),
    SubrArity::new(0, Some(0)),
));
#[doc(hidden)]
pub fn register_selected_frame(ctx: &mut Context) {
    ctx.register_subr(SPEC_SELECTED_FRAME_00);
}

/// Canonical display step `next_frame` (35 registrations).
const SPEC_NEXT_FRAME_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "next-frame",
    NativeFn::ContextVec(builtin_next_frame),
    SubrArity::new(0, Some(2)),
))
.placeholder(NoEvalPlaceholder::Nil);
const SPEC_NEXT_FRAME_01: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "previous-frame",
    NativeFn::ContextVec(builtin_previous_frame),
    SubrArity::new(0, Some(2)),
))
.placeholder(NoEvalPlaceholder::Nil);
const SPEC_NEXT_FRAME_02: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "select-frame",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_select_frame),
    SubrArity::new(1, Some(2)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("e"),
);
const SPEC_NEXT_FRAME_03: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "last-nonminibuffer-frame",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_selected_frame),
    SubrArity::new(0, Some(0)),
));
const SPEC_NEXT_FRAME_04: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "visible-frame-list",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_visible_frame_list),
    SubrArity::new(0, Some(0)),
));
const SPEC_NEXT_FRAME_05: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-list",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_list),
    SubrArity::new(0, Some(0)),
));
const SPEC_NEXT_FRAME_06: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "x-create-frame",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_x_create_frame),
    SubrArity::new(1, Some(1)),
));
const SPEC_NEXT_FRAME_07: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "make-frame-visible",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_make_frame_visible),
    SubrArity::new(0, Some(1)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(""),
);
const SPEC_NEXT_FRAME_08: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "iconify-frame",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_iconify_frame),
    SubrArity::new(0, Some(1)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(""),
);
const SPEC_NEXT_FRAME_09: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "delete-frame",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_delete_frame),
    SubrArity::new(0, Some(2)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(""),
);
const SPEC_NEXT_FRAME_10: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-char-height",
    NativeFn::ContextSlice(crate::emacs_core::frame::builtin_frame_char_height),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_11: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-char-width",
    NativeFn::ContextSlice(crate::emacs_core::frame::builtin_frame_char_width),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_12: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-native-height",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_native_height),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_13: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-native-width",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_native_width),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_14: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-text-cols",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_cols),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_15: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-text-height",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_height),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_16: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-text-lines",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_lines),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_17: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-text-width",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_width),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_18: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-total-cols",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_total_cols),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_19: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-total-lines",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_total_lines),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_20: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-position",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_position),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_21: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-parameters",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_parameters),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_22: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-frame-height",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_height),
    SubrArity::new(2, Some(4)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
        set_frame_height_interactive_spec,
    ),
);
const SPEC_NEXT_FRAME_23: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-frame-width",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_width),
    SubrArity::new(2, Some(4)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
        set_frame_width_interactive_spec,
    ),
);
const SPEC_NEXT_FRAME_24: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-frame-size",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_size),
    SubrArity::new(3, Some(4)),
));
const SPEC_NEXT_FRAME_25: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-frame-position",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_position),
    SubrArity::new(3, Some(3)),
));
const SPEC_NEXT_FRAME_26: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-visible-p",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_visible_p),
    SubrArity::new(1, Some(1)),
));
const SPEC_NEXT_FRAME_27: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-live-p",
    NativeFn::ContextSlice(crate::emacs_core::frame::builtin_frame_live_p),
    SubrArity::new(1, Some(1)),
));
const SPEC_NEXT_FRAME_28: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-initial-p",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_frame_initial_p),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_29: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-first-window",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_first_window),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_30: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-root-window",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_frame_root_window),
    SubrArity::new(0, Some(1)),
));
const SPEC_NEXT_FRAME_31: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "windowp",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_windowp),
    SubrArity::new(1, Some(1)),
));
const SPEC_NEXT_FRAME_32: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-valid-p",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_valid_p),
    SubrArity::new(1, Some(1)),
));
const SPEC_NEXT_FRAME_33: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "framep",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_framep),
    SubrArity::new(1, Some(1)),
));
const SPEC_NEXT_FRAME_34: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "window-frame",
    NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_frame),
    SubrArity::new(0, Some(1)),
));
#[doc(hidden)]
pub fn register_next_frame(ctx: &mut Context) {
    ctx.register_subr(SPEC_NEXT_FRAME_00);
    ctx.register_subr(SPEC_NEXT_FRAME_01);
    ctx.register_subr(SPEC_NEXT_FRAME_02);
    ctx.register_subr(SPEC_NEXT_FRAME_03);
    ctx.register_subr(SPEC_NEXT_FRAME_04);
    ctx.register_subr(SPEC_NEXT_FRAME_05);
    ctx.register_subr(SPEC_NEXT_FRAME_06);
    ctx.register_subr(SPEC_NEXT_FRAME_07);
    ctx.register_subr(SPEC_NEXT_FRAME_08);
    ctx.register_subr(SPEC_NEXT_FRAME_09);
    ctx.register_subr(SPEC_NEXT_FRAME_10);
    ctx.register_subr(SPEC_NEXT_FRAME_11);
    ctx.register_subr(SPEC_NEXT_FRAME_12);
    ctx.register_subr(SPEC_NEXT_FRAME_13);
    ctx.register_subr(SPEC_NEXT_FRAME_14);
    ctx.register_subr(SPEC_NEXT_FRAME_15);
    ctx.register_subr(SPEC_NEXT_FRAME_16);
    ctx.register_subr(SPEC_NEXT_FRAME_17);
    ctx.register_subr(SPEC_NEXT_FRAME_18);
    ctx.register_subr(SPEC_NEXT_FRAME_19);
    ctx.register_subr(SPEC_NEXT_FRAME_20);
    ctx.register_subr(SPEC_NEXT_FRAME_21);
    ctx.register_subr(SPEC_NEXT_FRAME_22);
    ctx.register_subr(SPEC_NEXT_FRAME_23);
    ctx.register_subr(SPEC_NEXT_FRAME_24);
    ctx.register_subr(SPEC_NEXT_FRAME_25);
    ctx.register_subr(SPEC_NEXT_FRAME_26);
    ctx.register_subr(SPEC_NEXT_FRAME_27);
    ctx.register_subr(SPEC_NEXT_FRAME_28);
    ctx.register_subr(SPEC_NEXT_FRAME_29);
    ctx.register_subr(SPEC_NEXT_FRAME_30);
    ctx.register_subr(SPEC_NEXT_FRAME_31);
    ctx.register_subr(SPEC_NEXT_FRAME_32);
    ctx.register_subr(SPEC_NEXT_FRAME_33);
    ctx.register_subr(SPEC_NEXT_FRAME_34);
}

/// Canonical display step `x_open_connection` (14 registrations).
const SPEC_X_OPEN_CONNECTION_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-open-connection",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_open_connection),
        SubrArity::new(1, Some(3)),
    ));
const SPEC_X_OPEN_CONNECTION_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-get-resource",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_get_resource),
        SubrArity::new(2, Some(4)),
    ));
#[cfg(target_os = "macos")]
const SPEC_X_OPEN_CONNECTION_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "ns-get-resource",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_ns_get_resource),
        SubrArity::new(2, Some(2)),
    ));
#[cfg(target_os = "macos")]
const SPEC_X_OPEN_CONNECTION_03: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "ns-set-resource",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_ns_set_resource),
        SubrArity::new(3, Some(3)),
    ));
const SPEC_X_OPEN_CONNECTION_04: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-list-fonts",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_list_fonts),
        SubrArity::new(1, Some(5)),
    ));
const SPEC_X_OPEN_CONNECTION_05: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-system",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_window_system),
        SubrArity::new(0, Some(1)),
    ))
    .requires_eval_state();
const SPEC_X_OPEN_CONNECTION_06: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-server-version",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_version),
        SubrArity::new(0, Some(1)),
    ));
#[cfg(not(any(windows, target_os = "macos")))]
const SPEC_X_OPEN_CONNECTION_07: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-server-input-extension-version",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_input_extension_version),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_X_OPEN_CONNECTION_08: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-server-vendor",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_vendor),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_X_OPEN_CONNECTION_09: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-mm-height",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_mm_height),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_X_OPEN_CONNECTION_10: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-mm-width",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_mm_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_X_OPEN_CONNECTION_11: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-planes",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_planes),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_X_OPEN_CONNECTION_12: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-screens",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_screens),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_X_OPEN_CONNECTION_13: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-close-connection",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_close_connection),
        SubrArity::new(1, Some(1)),
    ));
#[doc(hidden)]
pub fn register_x_open_connection(ctx: &mut Context) {
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_00);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_01);
    #[cfg(target_os = "macos")]
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_02);
    #[cfg(target_os = "macos")]
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_03);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_04);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_05);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_06);
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_07);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_08);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_09);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_10);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_11);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_12);
    ctx.register_subr(SPEC_X_OPEN_CONNECTION_13);
}

/// Canonical display step `face_font` (1 registrations).
const SPEC_FACE_FONT_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "face-font",
    NativeFn::ContextSlice(crate::emacs_core::xfaces::builtin_face_font),
    SubrArity::new(1, Some(3)),
));
#[doc(hidden)]
pub fn register_face_font(ctx: &mut Context) {
    ctx.register_subr(SPEC_FACE_FONT_00);
}

/// Canonical display step `frame_parameter` (21 registrations).
const SPEC_FRAME_PARAMETER_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-parameter",
    NativeFn::ContextSlice(crate::emacs_core::frame::builtin_frame_parameter),
    SubrArity::new(2, Some(2)),
));
const SPEC_FRAME_PARAMETER_01: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "send-string-to-terminal",
    NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_send_string_to_terminal),
    SubrArity::new(1, Some(2)),
));
const SPEC_FRAME_PARAMETER_02: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "internal-show-cursor",
    NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_internal_show_cursor),
    SubrArity::new(2, Some(2)),
));
const SPEC_FRAME_PARAMETER_03: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "internal-show-cursor-p",
    NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_internal_show_cursor_p),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_04: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "redraw-frame",
    NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_redraw_frame),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_05: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "display-supports-face-attributes-p",
    NativeFn::ContextVec(crate::emacs_core::display::builtin_display_supports_face_attributes_p,),
    SubrArity::new(1, Some(2)),
));
const SPEC_FRAME_PARAMETER_06: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "terminal-name",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_name),
    SubrArity::new(0, Some(1)),
))
.requires_eval_state();
const SPEC_FRAME_PARAMETER_07: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "terminal-live-p",
    NativeFn::ContextSlice(crate::emacs_core::terminal::pure::builtin_terminal_live_p),
    SubrArity::new(1, Some(1)),
))
.requires_eval_state();
const SPEC_FRAME_PARAMETER_08: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "terminal-parameter",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_parameter),
    SubrArity::new(2, Some(2)),
));
const SPEC_FRAME_PARAMETER_09: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "terminal-parameters",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_parameters),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_10: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "set-terminal-parameter",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_set_terminal_parameter),
    SubrArity::new(3, Some(3)),
));
const SPEC_FRAME_PARAMETER_11: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-type",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_type),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_12: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-top-frame",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_top_frame),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_13: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-display-color-p",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_display_color_p),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_14: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-display-color-cells",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_display_color_cells),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_15: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-no-underline",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_no_underline),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_16: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "controlling-tty-p",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_controlling_tty_p),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_17: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "suspend-tty",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_suspend_tty),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_18: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "resume-tty",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_resume_tty),
    SubrArity::new(0, Some(1)),
));
const SPEC_FRAME_PARAMETER_19: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "frame-terminal",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_frame_terminal),
    SubrArity::new(0, Some(1)),
));
#[cfg(not(any(windows, target_os = "macos")))]
const SPEC_FRAME_PARAMETER_20: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "x-display-monitor-attributes-list",
    NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_monitor_attributes_list),
    SubrArity::new(0, Some(1)),
));
#[doc(hidden)]
pub fn register_frame_parameter(ctx: &mut Context) {
    ctx.register_subr(SPEC_FRAME_PARAMETER_00);
    ctx.register_subr(SPEC_FRAME_PARAMETER_01);
    ctx.register_subr(SPEC_FRAME_PARAMETER_02);
    ctx.register_subr(SPEC_FRAME_PARAMETER_03);
    ctx.register_subr(SPEC_FRAME_PARAMETER_04);
    ctx.register_subr(SPEC_FRAME_PARAMETER_05);
    ctx.register_subr(SPEC_FRAME_PARAMETER_06);
    ctx.register_subr(SPEC_FRAME_PARAMETER_07);
    ctx.register_subr(SPEC_FRAME_PARAMETER_08);
    ctx.register_subr(SPEC_FRAME_PARAMETER_09);
    ctx.register_subr(SPEC_FRAME_PARAMETER_10);
    ctx.register_subr(SPEC_FRAME_PARAMETER_11);
    ctx.register_subr(SPEC_FRAME_PARAMETER_12);
    ctx.register_subr(SPEC_FRAME_PARAMETER_13);
    ctx.register_subr(SPEC_FRAME_PARAMETER_14);
    ctx.register_subr(SPEC_FRAME_PARAMETER_15);
    ctx.register_subr(SPEC_FRAME_PARAMETER_16);
    ctx.register_subr(SPEC_FRAME_PARAMETER_17);
    ctx.register_subr(SPEC_FRAME_PARAMETER_18);
    ctx.register_subr(SPEC_FRAME_PARAMETER_19);
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(SPEC_FRAME_PARAMETER_20);
}

/// Canonical display step `internal_lisp_face_p` (3 registrations).
const SPEC_INTERNAL_LISP_FACE_P_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-lisp-face-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_lisp_face_p),
        SubrArity::new(1, Some(2)),
    ));
const SPEC_INTERNAL_LISP_FACE_P_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-make-lisp-face",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_make_lisp_face),
        SubrArity::new(1, Some(2)),
    ));
const SPEC_INTERNAL_LISP_FACE_P_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-set-lisp-face-attribute",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute),
        SubrArity::new(3, Some(4)),
    ));
#[doc(hidden)]
pub fn register_internal_lisp_face_p(ctx: &mut Context) {
    ctx.register_subr(SPEC_INTERNAL_LISP_FACE_P_00);
    ctx.register_subr(SPEC_INTERNAL_LISP_FACE_P_01);
    ctx.register_subr(SPEC_INTERNAL_LISP_FACE_P_02);
}

/// Canonical display step `set_frame_size_and_position_pixelwise` (1 registrations).
const SPEC_SET_FRAME_SIZE_AND_POSITION_PIXELWISE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-frame-size-and-position-pixelwise",
        NativeFn::ContextVec(
            crate::emacs_core::frame::builtin_set_frame_size_and_position_pixelwise,
        ),
        SubrArity::new(5, Some(6)),
    ));
#[doc(hidden)]
pub fn register_set_frame_size_and_position_pixelwise(ctx: &mut Context) {
    ctx.register_subr(SPEC_SET_FRAME_SIZE_AND_POSITION_PIXELWISE_00);
}

/// Canonical display step `font_get_system_font` (2 registrations).
#[cfg(not(any(windows, target_os = "macos")))]
const SPEC_FONT_GET_SYSTEM_FONT_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "font-get-system-font",
        NativeFn::ContextVec(crate::emacs_core::font::builtin_font_get_system_font),
        SubrArity::new(0, Some(0)),
    ));
#[cfg(not(any(windows, target_os = "macos")))]
const SPEC_FONT_GET_SYSTEM_FONT_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "font-get-system-normal-font",
        NativeFn::ContextVec(crate::emacs_core::font::builtin_font_get_system_normal_font),
        SubrArity::new(0, Some(0)),
    ));
#[doc(hidden)]
#[cfg_attr(any(windows, target_os = "macos"), expect(unused_variables))]
pub fn register_font_get_system_font(ctx: &mut Context) {
    // Both subrs are X11-only, so a non-X11 build compiles an empty body and
    // the expectation above covers its unused parameter.
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(SPEC_FONT_GET_SYSTEM_FONT_00);
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(SPEC_FONT_GET_SYSTEM_FONT_01);
}

/// Canonical display step `frame_ancestor_p` (4 registrations).
const SPEC_FRAME_ANCESTOR_P_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-ancestor-p",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_ancestor_p),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_FRAME_ANCESTOR_P_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-bottom-divider-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_bottom_divider_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_FRAME_ANCESTOR_P_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-child-frame-border-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_child_frame_border_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_FRAME_ANCESTOR_P_03: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-focus",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_focus),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_frame_ancestor_p(ctx: &mut Context) {
    ctx.register_subr(SPEC_FRAME_ANCESTOR_P_00);
    ctx.register_subr(SPEC_FRAME_ANCESTOR_P_01);
    ctx.register_subr(SPEC_FRAME_ANCESTOR_P_02);
    ctx.register_subr(SPEC_FRAME_ANCESTOR_P_03);
}

/// Canonical display step `frame_internal_border_width` (3 registrations).
const SPEC_FRAME_INTERNAL_BORDER_WIDTH_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-internal-border-width",
        NativeFn::ContextSlice(crate::emacs_core::frame::builtin_frame_internal_border_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_FRAME_INTERNAL_BORDER_WIDTH_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-or-buffer-changed-p",
        NativeFn::ContextVec(crate::emacs_core::dispnew::builtin_frame_or_buffer_changed_p,),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_FRAME_INTERNAL_BORDER_WIDTH_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-parent",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_parent),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_frame_internal_border_width(ctx: &mut Context) {
    ctx.register_subr(SPEC_FRAME_INTERNAL_BORDER_WIDTH_00);
    ctx.register_subr(SPEC_FRAME_INTERNAL_BORDER_WIDTH_01);
    ctx.register_subr(SPEC_FRAME_INTERNAL_BORDER_WIDTH_02);
}

/// Canonical display step `frame_right_divider_width` (2 registrations).
const SPEC_FRAME_RIGHT_DIVIDER_WIDTH_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-right-divider-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_right_divider_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_FRAME_RIGHT_DIVIDER_WIDTH_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-scale-factor",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_scale_factor),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_frame_right_divider_width(ctx: &mut Context) {
    ctx.register_subr(SPEC_FRAME_RIGHT_DIVIDER_WIDTH_00);
    ctx.register_subr(SPEC_FRAME_RIGHT_DIVIDER_WIDTH_01);
}

/// Canonical display step `frame_window_state_change` (2 registrations).
const SPEC_FRAME_WINDOW_STATE_CHANGE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-window-state-change",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_window_state_change),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_FRAME_WINDOW_STATE_CHANGE_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "fringe-bitmaps-at-pos",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_fringe_bitmaps_at_pos),
        SubrArity::new(0, Some(2)),
    ))
    .requires_eval_state();
#[doc(hidden)]
pub fn register_frame_window_state_change(ctx: &mut Context) {
    ctx.register_subr(SPEC_FRAME_WINDOW_STATE_CHANGE_00);
    ctx.register_subr(SPEC_FRAME_WINDOW_STATE_CHANGE_01);
}

/// Canonical display step `handle_switch_frame` (1 registrations).
const SPEC_HANDLE_SWITCH_FRAME_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "handle-switch-frame",
        crate::emacs_core::frame::builtin_handle_switch_frame,
        FixedMin1::One,
    ))
    .requires_eval_state()
    .interactive(
        neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("^e"),
    );
#[doc(hidden)]
pub fn register_handle_switch_frame(ctx: &mut Context) {
    ctx.register_subr(SPEC_HANDLE_SWITCH_FRAME_00);
}

/// Canonical display step `make_frame_invisible` (1 registrations).
const SPEC_MAKE_FRAME_INVISIBLE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-frame-invisible",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_make_frame_invisible),
        SubrArity::new(0, Some(2)),
    ))
    .requires_eval_state()
    .interactive(
        neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(""),
    );
#[doc(hidden)]
pub fn register_make_frame_invisible(ctx: &mut Context) {
    ctx.register_subr(SPEC_MAKE_FRAME_INVISIBLE_00);
}

/// Canonical display step `neomacs_frame_snapshot` (3 registrations).
const SPEC_NEOMACS_FRAME_SNAPSHOT_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs--frame-snapshot",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_neomacs_frame_snapshot),
        SubrArity::new(0, Some(2)),
    ));
const SPEC_NEOMACS_FRAME_SNAPSHOT_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs--write-frame-snapshot",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_neomacs_write_frame_snapshot),
        SubrArity::new(1, Some(3)),
    ));
const SPEC_NEOMACS_FRAME_SNAPSHOT_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs--debug-lose-device",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_neomacs_debug_lose_device),
        SubrArity::new(0, Some(0)),
    ));
#[doc(hidden)]
pub fn register_neomacs_frame_snapshot(ctx: &mut Context) {
    ctx.register_subr(SPEC_NEOMACS_FRAME_SNAPSHOT_00);
    ctx.register_subr(SPEC_NEOMACS_FRAME_SNAPSHOT_01);
    ctx.register_subr(SPEC_NEOMACS_FRAME_SNAPSHOT_02);
}

/// Canonical display step `redirect_frame_focus` (1 registrations).
const SPEC_REDIRECT_FRAME_FOCUS_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "redirect-frame-focus",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_redirect_frame_focus),
        SubrArity::new(1, Some(2)),
    ));
#[doc(hidden)]
pub fn register_redirect_frame_focus(ctx: &mut Context) {
    ctx.register_subr(SPEC_REDIRECT_FRAME_FOCUS_00);
}

/// Canonical display step `resize_mini_window_internal` (1 registrations).
const SPEC_RESIZE_MINI_WINDOW_INTERNAL_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "resize-mini-window-internal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_resize_mini_window_internal),
        SubrArity::new(1, Some(1)),
    ));
#[doc(hidden)]
pub fn register_resize_mini_window_internal(ctx: &mut Context) {
    ctx.register_subr(SPEC_RESIZE_MINI_WINDOW_INTERNAL_00);
}

/// Canonical display step `set_frame_window_state_change` (1 registrations).
const SPEC_SET_FRAME_WINDOW_STATE_CHANGE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-frame-window-state-change",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_window_state_change),
        SubrArity::new(0, Some(2)),
    ));
#[doc(hidden)]
pub fn register_set_frame_window_state_change(ctx: &mut Context) {
    ctx.register_subr(SPEC_SET_FRAME_WINDOW_STATE_CHANGE_00);
}

/// Canonical display step `set_window_new_normal` (3 registrations).
const SPEC_SET_WINDOW_NEW_NORMAL_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-new-normal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_new_normal),
        SubrArity::new(1, Some(2)),
    ));
const SPEC_SET_WINDOW_NEW_NORMAL_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-new-pixel",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_new_pixel),
        SubrArity::new(2, Some(3)),
    ));
const SPEC_SET_WINDOW_NEW_NORMAL_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-new-total",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_new_total),
        SubrArity::new(2, Some(3)),
    ));
#[doc(hidden)]
pub fn register_set_window_new_normal(ctx: &mut Context) {
    ctx.register_subr(SPEC_SET_WINDOW_NEW_NORMAL_00);
    ctx.register_subr(SPEC_SET_WINDOW_NEW_NORMAL_01);
    ctx.register_subr(SPEC_SET_WINDOW_NEW_NORMAL_02);
}

/// Canonical display step `tty_frame_at` (4 registrations).
const SPEC_TTY_FRAME_AT_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-frame-at",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_at),
    SubrArity::new(2, Some(2)),
));
const SPEC_TTY_FRAME_AT_01: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-frame-edges",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_edges),
    SubrArity::new(0, Some(2)),
));
const SPEC_TTY_FRAME_AT_02: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-frame-geometry",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_geometry),
    SubrArity::new(0, Some(1)),
));
const SPEC_TTY_FRAME_AT_03: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "tty-frame-list-z-order",
    NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_list_z_order),
    SubrArity::new(0, Some(1)),
));
#[doc(hidden)]
pub fn register_tty_frame_at(ctx: &mut Context) {
    ctx.register_subr(SPEC_TTY_FRAME_AT_00);
    ctx.register_subr(SPEC_TTY_FRAME_AT_01);
    ctx.register_subr(SPEC_TTY_FRAME_AT_02);
    ctx.register_subr(SPEC_TTY_FRAME_AT_03);
}

/// Canonical display step `window_bottom_divider_width` (12 registrations).
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-bottom-divider-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_bottom_divider_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-lines-pixel-dimensions",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_lines_pixel_dimensions),
        SubrArity::new(0, Some(6)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-new-normal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_new_normal),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_03: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-new-pixel",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_new_pixel),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_04: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-new-total",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_new_total),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_05: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-old-body-pixel-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_body_pixel_height,),
        SubrArity::new(0, Some(1)),
    ))
    .placeholder(NoEvalPlaceholder::FixnumZero);
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_06: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-old-body-pixel-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_body_pixel_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_07: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-old-pixel-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_pixel_height),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_08: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-old-pixel-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_pixel_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_09: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-right-divider-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_right_divider_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_10: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-scroll-bar-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_scroll_bar_height),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_11: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-scroll-bar-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_scroll_bar_width),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_window_bottom_divider_width(ctx: &mut Context) {
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_00);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_01);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_02);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_03);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_04);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_05);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_06);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_07);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_08);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_09);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_10);
    ctx.register_subr(SPEC_WINDOW_BOTTOM_DIVIDER_WIDTH_11);
}

/// Canonical display step `font` (20 registrations).
#[doc(hidden)]
pub fn register_font(ctx: &mut Context) {
    crate::emacs_core::font::register_subrs(ctx);
}

/// Canonical display step `neomacs_frame_edges` (1 registrations).
const SPEC_NEOMACS_FRAME_EDGES_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-frame-edges",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_neomacs_frame_edges),
        SubrArity::new(0, Some(2)),
    ));
#[doc(hidden)]
pub fn register_neomacs_frame_edges(ctx: &mut Context) {
    ctx.register_subr(SPEC_NEOMACS_FRAME_EDGES_00);
}

/// Canonical display step `effects_modifiers_chrome` (8 registrations).
#[doc(hidden)]
pub fn register_effects_modifiers_chrome(ctx: &mut Context) {
    crate::emacs_core::neo::effects::register_subrs(ctx);
    crate::emacs_core::neo::modifiers::register_subrs(ctx);
    crate::emacs_core::neo::chrome::register_subrs(ctx);
}

/// Canonical display step `window_discard_buffer_from_window` (4 registrations).
const SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-discard-buffer-from-window",
        NativeFn::ContextVec(
            crate::emacs_core::window_cmds::builtin_window_discard_buffer_from_window,
        ),
        SubrArity::new(2, Some(3)),
    ));
const SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-cursor-info",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_cursor_info),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "combine-windows",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_combine_windows),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_03: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "uncombine-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_uncombine_window),
        SubrArity::new(1, Some(1)),
    ));
#[doc(hidden)]
pub fn register_window_discard_buffer_from_window(ctx: &mut Context) {
    ctx.register_subr(SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_00);
    ctx.register_subr(SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_01);
    ctx.register_subr(SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_02);
    ctx.register_subr(SPEC_WINDOW_DISCARD_BUFFER_FROM_WINDOW_03);
}

/// Canonical display step `window_text_pixel_size` (3 registrations).
const SPEC_WINDOW_TEXT_PIXEL_SIZE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-text-pixel-size",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_window_text_pixel_size_ctx),
        SubrArity::new(0, Some(7)),
    ));
const SPEC_WINDOW_TEXT_PIXEL_SIZE_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "pos-visible-in-window-p",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_pos_visible_in_window_p_ctx),
        SubrArity::new(0, Some(3)),
    ));
const SPEC_WINDOW_TEXT_PIXEL_SIZE_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame--face-hash-table",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_frame_face_hash_table),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_window_text_pixel_size(ctx: &mut Context) {
    ctx.register_subr(SPEC_WINDOW_TEXT_PIXEL_SIZE_00);
    ctx.register_subr(SPEC_WINDOW_TEXT_PIXEL_SIZE_01);
    ctx.register_subr(SPEC_WINDOW_TEXT_PIXEL_SIZE_02);
}

/// Canonical display step `active_minibuffer_window` (30 registrations).
const SPEC_ACTIVE_MINIBUFFER_WINDOW_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "active-minibuffer-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_active_minibuffer_window),
        SubrArity::new(0, Some(0)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "minibuffer-selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_minibuffer_selected_window),
        SubrArity::new(0, Some(0)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-mode-line-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_mode_line_height),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_03: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-header-line-height",
        NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_header_line_height),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_04: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-tab-line-height",
        NativeFn::ContextSlice(crate::emacs_core::window_cmds::builtin_window_tab_line_height),
        SubrArity::new(0, Some(1)),
    ))
    .placeholder(NoEvalPlaceholder::FixnumZero);
const SPEC_ACTIVE_MINIBUFFER_WINDOW_05: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-display-table",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_display_table),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_06: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-cursor-type",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_cursor_type),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_07: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-scroll-bars",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_scroll_bars),
        SubrArity::new(1, Some(6)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_08: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-next-buffers",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_next_buffers),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_09: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-prev-buffers",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_prev_buffers),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_10: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-dedicated-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_dedicated_p),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_11: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-window-internal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_delete_window_internal),
        SubrArity::new(1, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_12: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "delete-other-windows-internal",
            NativeFn::ContextVec(
                crate::emacs_core::window_cmds::builtin_delete_other_windows_internal,
            ),
            SubrArity::new(0, Some(2)),
        ))
        .interactive(neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(""));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_13: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-combination-limit",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_combination_limit),
        SubrArity::new(1, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_14: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-combination-limit",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_combination_limit),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_15: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-resize-apply-total",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_resize_apply_total),
        SubrArity::new(0, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_16: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "other-window-for-scrolling",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_other_window_for_scrolling),
        SubrArity::new(0, Some(0)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_17: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "modify-frame-parameters",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_modify_frame_parameters),
        SubrArity::new(2, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_18: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_selected_window),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_19: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-old-selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_old_selected_window),
        SubrArity::new(0, Some(1)),
    ))
    .requires_eval_state();
const SPEC_ACTIVE_MINIBUFFER_WINDOW_20: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-frame-selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_frame_selected_window),
        SubrArity::new(2, Some(3)),
    ))
    .requires_eval_state();
const SPEC_ACTIVE_MINIBUFFER_WINDOW_21: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-pixel-width",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_pixel_width),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_22: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-pixel-height",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_pixel_height),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_23: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-server-max-request-size",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_max_request_size),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_24: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-grayscale-p",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_grayscale_p),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_25: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-backing-store",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_backing_store),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_26: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-color-cells",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_color_cells),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_27: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-save-under",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_save_under),
        SubrArity::new(0, Some(1)),
    ));
#[cfg(not(any(windows, target_os = "macos")))]
const SPEC_ACTIVE_MINIBUFFER_WINDOW_28: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-set-last-user-time",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_set_last_user_time),
        SubrArity::new(1, Some(2)),
    ));
const SPEC_ACTIVE_MINIBUFFER_WINDOW_29: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-visual-class",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_visual_class),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_active_minibuffer_window(ctx: &mut Context) {
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_00);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_01);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_02);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_03);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_04);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_05);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_06);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_07);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_08);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_09);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_10);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_11);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_12);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_13);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_14);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_15);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_16);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_17);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_18);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_19);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_20);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_21);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_22);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_23);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_24);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_25);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_26);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_27);
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_28);
    ctx.register_subr(SPEC_ACTIVE_MINIBUFFER_WINDOW_29);
}

/// Canonical display step `internal_lisp_face_equal_p` (2 registrations).
const SPEC_INTERNAL_LISP_FACE_EQUAL_P_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-lisp-face-equal-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_lisp_face_equal_p),
        SubrArity::new(2, Some(3)),
    ));
const SPEC_INTERNAL_LISP_FACE_EQUAL_P_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-lisp-face-empty-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_lisp_face_empty_p),
        SubrArity::new(1, Some(2)),
    ));
#[doc(hidden)]
pub fn register_internal_lisp_face_equal_p(ctx: &mut Context) {
    ctx.register_subr(SPEC_INTERNAL_LISP_FACE_EQUAL_P_00);
    ctx.register_subr(SPEC_INTERNAL_LISP_FACE_EQUAL_P_01);
}

/// Canonical display step `merge_face_attribute` (2 registrations).
const SPEC_MERGE_FACE_ATTRIBUTE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "merge-face-attribute",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_merge_face_attribute_with_eval),
        SubrArity::new(3, Some(3)),
    ));
const SPEC_MERGE_FACE_ATTRIBUTE_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "color-gray-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_color_gray_p),
        SubrArity::new(1, Some(2)),
    ));
#[doc(hidden)]
pub fn register_merge_face_attribute(ctx: &mut Context) {
    ctx.register_subr(SPEC_MERGE_FACE_ATTRIBUTE_00);
    ctx.register_subr(SPEC_MERGE_FACE_ATTRIBUTE_01);
}

/// Canonical display step `color_distance` (1 registrations).
const SPEC_COLOR_DISTANCE_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "color-distance",
    NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_color_distance),
    SubrArity::new(2, Some(4)),
));
#[doc(hidden)]
pub fn register_color_distance(ctx: &mut Context) {
    ctx.register_subr(SPEC_COLOR_DISTANCE_00);
}

/// Canonical display step `internal_copy_lisp_face` (3 registrations).
const SPEC_INTERNAL_COPY_LISP_FACE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-copy-lisp-face",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_copy_lisp_face),
        SubrArity::new(4, Some(4)),
    ));
const SPEC_INTERNAL_COPY_LISP_FACE_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-get-lisp-face-attribute",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_get_lisp_face_attribute),
        SubrArity::new(2, Some(3)),
    ));
const SPEC_INTERNAL_COPY_LISP_FACE_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-merge-in-global-face",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_merge_in_global_face),
        SubrArity::new(2, Some(2)),
    ));
#[doc(hidden)]
pub fn register_internal_copy_lisp_face(ctx: &mut Context) {
    ctx.register_subr(SPEC_INTERNAL_COPY_LISP_FACE_00);
    ctx.register_subr(SPEC_INTERNAL_COPY_LISP_FACE_01);
    ctx.register_subr(SPEC_INTERNAL_COPY_LISP_FACE_02);
}

/// Canonical display step `redraw_display` (1 registrations).
const SPEC_REDRAW_DISPLAY_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "redraw-display",
    NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_redraw_display_in_context,),
    SubrArity::new(0, Some(0)),
))
.interactive(
    neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(""),
);
#[doc(hidden)]
pub fn register_redraw_display(ctx: &mut Context) {
    ctx.register_subr(SPEC_REDRAW_DISPLAY_00);
}

/// Canonical display step `force_window_update` (1 registrations).
const SPEC_FORCE_WINDOW_UPDATE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "force-window-update",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_force_window_update),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_force_window_update(ctx: &mut Context) {
    ctx.register_subr(SPEC_FORCE_WINDOW_UPDATE_00);
}

/// Canonical display step `x_focus_frame` (1 registrations).
const SPEC_X_FOCUS_FRAME_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "x-focus-frame",
    NativeFn::ContextVec(crate::emacs_core::display::builtin_x_focus_frame),
    SubrArity::new(1, Some(2)),
));
#[doc(hidden)]
pub fn register_x_focus_frame(ctx: &mut Context) {
    ctx.register_subr(SPEC_X_FOCUS_FRAME_00);
}

/// Canonical display step `x_popup_dialog` (2 registrations).
const SPEC_X_POPUP_DIALOG_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "x-popup-dialog",
    NativeFn::ContextVec(crate::emacs_core::display::builtin_x_popup_dialog),
    SubrArity::new(2, Some(3)),
));
const SPEC_X_POPUP_DIALOG_01: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "x-popup-menu",
    NativeFn::ContextVec(crate::emacs_core::display::builtin_x_popup_menu),
    SubrArity::new(2, Some(2)),
));
#[doc(hidden)]
pub fn register_x_popup_dialog(ctx: &mut Context) {
    ctx.register_subr(SPEC_X_POPUP_DIALOG_00);
    ctx.register_subr(SPEC_X_POPUP_DIALOG_01);
}

/// Canonical display step `delete_terminal` (9 registrations).
const SPEC_DELETE_TERMINAL_00: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "delete-terminal",
    NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_delete_terminal),
    SubrArity::new(0, Some(2)),
));
const SPEC_DELETE_TERMINAL_01: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "make-terminal-frame",
    NativeFn::ContextVec(crate::emacs_core::frame::builtin_make_terminal_frame),
    SubrArity::new(1, Some(1)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_02: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "image-size",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_image_size_in_context),
    SubrArity::new(1, Some(3)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_03: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "image-mask-p",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_image_mask_p_in_context),
    SubrArity::new(1, Some(2)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_04: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "image-flush",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_image_flush_in_context),
    SubrArity::new(1, Some(2)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_05: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "clear-image-cache",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_clear_image_cache_in_context),
    SubrArity::new(0, Some(2)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_06: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "image-cache-size",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_image_cache_size_in_context),
    SubrArity::new(0, Some(0)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_07: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "image-metadata",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_image_metadata_in_context),
    SubrArity::new(1, Some(2)),
))
.requires_eval_state();
const SPEC_DELETE_TERMINAL_08: SubrSpec = neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
    "neomacs-image-extent",
    NativeFn::ContextVec(crate::emacs_core::image::builtin_neomacs_image_extent_in_context),
    SubrArity::new(1, Some(2)),
))
.requires_eval_state();
#[doc(hidden)]
pub fn register_delete_terminal(ctx: &mut Context) {
    ctx.register_subr(SPEC_DELETE_TERMINAL_00);
    ctx.register_subr(SPEC_DELETE_TERMINAL_01);
    ctx.register_subr(SPEC_DELETE_TERMINAL_02);
    ctx.register_subr(SPEC_DELETE_TERMINAL_03);
    ctx.register_subr(SPEC_DELETE_TERMINAL_04);
    ctx.register_subr(SPEC_DELETE_TERMINAL_05);
    ctx.register_subr(SPEC_DELETE_TERMINAL_06);
    ctx.register_subr(SPEC_DELETE_TERMINAL_07);
    ctx.register_subr(SPEC_DELETE_TERMINAL_08);
}

/// Canonical display step `image_transforms_p` (3 registrations).
const SPEC_IMAGE_TRANSFORMS_P_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "image-transforms-p",
        NativeFn::ContextVec(crate::emacs_core::image::builtin_image_transforms_p),
        SubrArity::new(0, Some(1)),
    ));
const SPEC_IMAGE_TRANSFORMS_P_01: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "invisible-p",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_invisible_p),
        SubrArity::new(1, Some(1)),
    ));
const SPEC_IMAGE_TRANSFORMS_P_02: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "line-pixel-height",
        NativeFn::ContextSlice(crate::emacs_core::xdisp::builtin_line_pixel_height),
        SubrArity::new(0, Some(0)),
    ));
#[doc(hidden)]
pub fn register_image_transforms_p(ctx: &mut Context) {
    ctx.register_subr(SPEC_IMAGE_TRANSFORMS_P_00);
    ctx.register_subr(SPEC_IMAGE_TRANSFORMS_P_01);
    ctx.register_subr(SPEC_IMAGE_TRANSFORMS_P_02);
}

/// Canonical display step `current_bidi_paragraph_direction` (1 registrations).
const SPEC_CURRENT_BIDI_PARAGRAPH_DIRECTION_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-bidi-paragraph-direction",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_current_bidi_paragraph_direction),
        SubrArity::new(0, Some(1)),
    ));
#[doc(hidden)]
pub fn register_current_bidi_paragraph_direction(ctx: &mut Context) {
    ctx.register_subr(SPEC_CURRENT_BIDI_PARAGRAPH_DIRECTION_00);
}

/// Canonical display step `move_to_window_line` (1 registrations).
const SPEC_MOVE_TO_WINDOW_LINE_00: SubrSpec =
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "move-to-window-line",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_move_to_window_line),
        SubrArity::new(1, Some(1)),
    ))
    .interactive(
        neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("P"),
    );
#[doc(hidden)]
pub fn register_move_to_window_line(ctx: &mut Context) {
    ctx.register_subr(SPEC_MOVE_TO_WINDOW_LINE_00);
}
