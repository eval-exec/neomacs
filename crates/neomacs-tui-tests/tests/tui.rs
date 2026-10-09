//! Every TUI pair test in one binary.
//!
//! Each former per-file integration binary became a module here: 37 separate
//! static links of neovm-core + Cranelift bought nothing that nextest needs -
//! it runs every test in its own process regardless. Keep new tests as modules
//! under tests/ and list them below.
#![cfg(unix)]

mod support;

mod coding_save;
mod dual_width_fixed_pitch;
#[path = "fontset_family_only_test.rs"]
mod fontset_family_only;
mod frame_bar_parameters;
mod idle_timer_output;
mod pixel_measurement;
mod process_plist_isolation;
mod process_send_encoding;

#[cfg(test)]
#[path = "mode_line_min_width_boundary_oracle.rs"]
mod mode_line_min_width_boundary_oracle;

#[cfg(test)]
#[path = "redisplay_transitions.rs"]
mod redisplay_transitions;

#[path = "overlay_face_render.rs"]
mod overlay_face_render;
#[path = "package_tui/mod.rs"]
mod package_tui;

#[path = "basic.rs"]
mod basic;
#[path = "buffers.rs"]
mod buffers;
#[path = "child_frames.rs"]
mod child_frames;
#[path = "command_loop_subreads.rs"]
mod command_loop_subreads;
#[path = "display_line_numbers.rs"]
mod display_line_numbers;
#[path = "editing.rs"]
mod editing;
#[path = "editing_motion.rs"]
mod editing_motion;
#[path = "eval_elisp.rs"]
mod eval_elisp;
#[path = "event_loop.rs"]
mod event_loop;
#[path = "face_color_test.rs"]
mod face_color_test;
#[path = "face_parity.rs"]
mod face_parity;
#[path = "files_dired.rs"]
mod files_dired;
#[path = "force_window_update.rs"]
mod force_window_update;
#[path = "frame_visibility.rs"]
mod frame_visibility;
#[path = "help_describe.rs"]
mod help_describe;
#[path = "ibuffer.rs"]
mod ibuffer;
#[path = "image_svg_animation.rs"]
mod image_svg_animation;
#[path = "input_methods.rs"]
mod input_methods;
#[path = "issue_140_hscroll.rs"]
mod issue_140_hscroll;
#[path = "issue_170_centered_buffer.rs"]
mod issue_170_centered_buffer;
#[path = "issue_254.rs"]
mod issue_254;
#[path = "issue_383_dashboard_banner.rs"]
mod issue_383_dashboard_banner;
#[path = "issue_445.rs"]
mod issue_445;
#[path = "issue_445_ibuffer_filter_groups.rs"]
mod issue_445_ibuffer_filter_groups;
#[path = "issue_446_align_to_hscroll.rs"]
mod issue_446_align_to_hscroll;
#[path = "issue_470.rs"]
mod issue_470;
#[path = "mark_region_fill.rs"]
mod mark_region_fill;
#[path = "menu_bar.rs"]
mod menu_bar;
#[path = "minibuffer_line.rs"]
mod minibuffer_line;
#[path = "minor_mode_order_repro.rs"]
mod minor_mode_order_repro;
#[path = "mode_line_eval_count_oracle.rs"]
mod mode_line_eval_count_oracle;
#[cfg(test)]
#[path = "mode_line_gc_lifetime_oracle.rs"]
mod mode_line_gc_lifetime_oracle;
#[cfg(test)]
#[path = "mode_line_numeric_padding_oracle.rs"]
mod mode_line_numeric_padding_oracle;
#[path = "modes.rs"]
mod modes;
#[path = "org.rs"]
mod org;
#[cfg(test)]
#[path = "posn_current_matrix_clear_oracle.rs"]
mod posn_current_matrix_clear_oracle;
#[cfg(test)]
#[path = "posn_object_extent_oracle.rs"]
mod posn_object_extent_oracle;
#[path = "pre_redisplay_function_oracle.rs"]
mod pre_redisplay_function_oracle;
#[path = "programming.rs"]
mod programming;
#[path = "project.rs"]
mod project;
#[path = "raw_terminal_snapshot_test.rs"]
mod raw_terminal_snapshot_test;
#[path = "redisplay_display_vars.rs"]
mod redisplay_display_vars;
#[cfg(test)]
#[path = "redisplay_hook_order_oracle.rs"]
mod redisplay_hook_order_oracle;
#[cfg(test)]
#[path = "redisplay_hook_transfer_oracle.rs"]
mod redisplay_hook_transfer_oracle;
#[path = "registers_bookmarks.rs"]
mod registers_bookmarks;
#[path = "replace_sort.rs"]
mod replace_sort;
#[path = "saving_insert.rs"]
mod saving_insert;
#[path = "scroll_bar_tty.rs"]
mod scroll_bar_tty;
#[path = "search.rs"]
mod search;
#[path = "send_string_to_terminal.rs"]
mod send_string_to_terminal;
#[path = "shell_compile.rs"]
mod shell_compile;
#[path = "source_navigation.rs"]
mod source_navigation;
mod spacemacs_boot;
#[path = "startup_terminal_initialization.rs"]
mod startup_terminal_initialization;
#[path = "strict_grid.rs"]
mod strict_grid;
#[path = "tty_color_index.rs"]
mod tty_color_index;
#[path = "tty_input.rs"]
mod tty_input;
#[path = "window_divider_overlay_arrow.rs"]
mod window_divider_overlay_arrow;
#[path = "window_end_oracle.rs"]
mod window_end_oracle;
#[path = "windows_tabs.rs"]
mod windows_tabs;

#[path = "gnu_redisplay_mutation_oracle.rs"]
mod gnu_redisplay_mutation_oracle;

#[cfg(test)]
#[path = "redisplay_mini_source_start_oracle.rs"]
mod redisplay_mini_source_start_oracle;
