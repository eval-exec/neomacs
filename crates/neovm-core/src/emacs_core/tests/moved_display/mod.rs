//! Re-homed display library tests that need system/core or core boot.
//! Each file moves WHOLE tests plus their private helpers; counts reconcile
//! display -N == core +N. See DISPLAY-TESTS-oracleb.tsv.

#[cfg(test)]
#[path = "popup_reader_test.rs"]
mod popup_reader_test;

#[cfg(test)]
#[path = "font_face_boot_test.rs"]
mod font_face_boot_test;

#[cfg(test)]
#[path = "tsb_boundary_numbers.rs"]
mod tsb_boundary_numbers;

#[cfg(test)]
#[path = "window_system_preload_test.rs"]
mod window_system_preload_test;

#[cfg(test)]
#[path = "text_measurement_invisible_test.rs"]
mod text_measurement_invisible_test;

#[path = "buffer_symbol_properties_test.rs"]
mod buffer_symbol_properties_test;
#[path = "face_inheritance_cycle_test.rs"]
mod face_inheritance_cycle_test;
#[cfg(test)]
#[path = "tsb_face_font_validation.rs"]
mod tsb_face_font_validation;

#[cfg(test)]
#[path = "tsb_face_height.rs"]
mod tsb_face_height;

#[cfg(test)]
#[path = "dispnew_test.rs"]
mod dispnew_test;

#[cfg(test)]
#[path = "idle_redraw_test.rs"]
mod idle_redraw_test;

#[cfg(test)]
#[path = "cycling_test.rs"]
mod cycling_test;

#[cfg(test)]
#[path = "selection_test.rs"]
mod selection_test;

#[cfg(test)]
#[path = "xwidget_runtime_test.rs"]
mod xwidget_runtime_test;

#[cfg(test)]
#[path = "core_hook_restore_test.rs"]
mod core_hook_restore_test;

#[cfg(test)]
#[path = "window_cmds_runtime_eval_test.rs"]
mod window_cmds_runtime_eval_test;

#[cfg(test)]
#[path = "xdisp_runtime_eval_test.rs"]
mod xdisp_runtime_eval_test;

#[cfg(test)]
#[path = "image_lisp_playback_test.rs"]
mod image_lisp_playback_test;

#[cfg(test)]
#[path = "tooltip_test.rs"]
mod tooltip_test;

#[cfg(test)]
#[path = "mode_line_outer_flow_test.rs"]
mod mode_line_outer_flow_test;

#[cfg(test)]
#[path = "mode_line_outer_handlers_test.rs"]
mod mode_line_outer_handlers_test;

#[cfg(test)]
#[path = "mode_line_point_test.rs"]
mod mode_line_point_test;

#[cfg(test)]
#[path = "frame_resize_scroll_bar_test.rs"]
mod frame_resize_scroll_bar_test;

#[cfg(test)]
#[path = "terminal_frame_deletion_test.rs"]
mod terminal_frame_deletion_test;

#[cfg(test)]
#[path = "fontset_freshness_test.rs"]
mod fontset_freshness_test;

#[cfg(test)]
#[path = "initial_gui_lifecycle_test.rs"]
mod initial_gui_lifecycle_test;

#[cfg(test)]
#[path = "attempt_freshness_reuse_upper_layer_test.rs"]
mod attempt_freshness_reuse_upper_layer_test;

#[cfg(test)]
#[path = "buffer_layout_projection_upper_layer_test.rs"]
mod buffer_layout_projection_upper_layer_test;

#[cfg(test)]
#[path = "window_cmds_upper_layer_test.rs"]
mod window_cmds_upper_layer_test;

#[cfg(test)]
#[path = "body_geometry_upper_layer_test.rs"]
mod body_geometry_upper_layer_test;

#[cfg(test)]
#[path = "frame_resize_upper_layer_test.rs"]
mod frame_resize_upper_layer_test;

#[cfg(test)]
#[path = "font_size_upper_layer_test.rs"]
mod font_size_upper_layer_test;

#[cfg(test)]
#[path = "builtins_upper_layer_test.rs"]
mod builtins_upper_layer_test;

#[cfg(test)]
#[path = "font_remapping_upper_layer_test.rs"]
mod font_remapping_upper_layer_test;

#[cfg(test)]
#[path = "gc_tls_ownership_upper_layer_test.rs"]
mod gc_tls_ownership_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_gc_roots_upper_layer_test.rs"]
mod mode_line_gc_roots_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_nested_point_upper_layer_test.rs"]
mod mode_line_nested_point_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_prop_borrow_upper_layer_test.rs"]
mod mode_line_prop_borrow_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_flow_upper_layer_test.rs"]
mod mode_line_flow_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_incremental_lifetimes_upper_layer_test.rs"]
mod mode_line_incremental_lifetimes_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_element_attribution_upper_layer_test.rs"]
mod mode_line_element_attribution_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_live_spine_upper_layer_test.rs"]
mod mode_line_live_spine_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_plain_field_upper_layer_test.rs"]
mod mode_line_plain_field_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_min_width_boundary_upper_layer_test.rs"]
mod mode_line_min_width_boundary_upper_layer_test;

#[cfg(test)]
#[path = "mode_line_dynamic_symbols_upper_layer_test.rs"]
mod mode_line_dynamic_symbols_upper_layer_test;

#[cfg(test)]
#[path = "xdisp_upper_layer_test.rs"]
mod xdisp_upper_layer_test;

#[cfg(test)]
#[path = "fontification_demand_upper_layer_test.rs"]
mod fontification_demand_upper_layer_test;

#[cfg(test)]
#[path = "fontification_boundary_upper_layer_test.rs"]
mod fontification_boundary_upper_layer_test;

#[cfg(test)]
#[path = "fontification_admission_upper_layer_test.rs"]
mod fontification_admission_upper_layer_test;

#[cfg(test)]
#[path = "position_chrome_query_upper_layer_test.rs"]
mod position_chrome_query_upper_layer_test;

#[cfg(test)]
#[path = "motion_upper_layer_test.rs"]
mod motion_upper_layer_test;

#[cfg(test)]
#[path = "resources_upper_layer_test.rs"]
mod resources_upper_layer_test;

#[cfg(test)]
#[path = "display_upper_layer_test.rs"]
mod display_upper_layer_test;

#[cfg(test)]
#[path = "display_when_upper_layer_test.rs"]
mod display_when_upper_layer_test;

#[cfg(test)]
#[path = "system_font_upper_layer_test.rs"]
mod system_font_upper_layer_test;

#[cfg(test)]
#[path = "scroll_upper_layer_test.rs"]
mod scroll_upper_layer_test;

#[cfg(test)]
#[path = "idle_redisplay_upper_layer_test.rs"]
mod idle_redisplay_upper_layer_test;

#[cfg(test)]
#[path = "image_identity_upper_layer_test.rs"]
mod image_identity_upper_layer_test;
