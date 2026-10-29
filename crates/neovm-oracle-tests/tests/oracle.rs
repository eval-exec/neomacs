//! Every oracle parity suite in one test binary.
//!
//! Each former per-file integration binary linked neovm-core and Cranelift
//! again; nextest runs every test in its own process, so one binary is
//! equivalent at run time. Keep new tests as #[path] modules listed here.

#[path = "../src/common.rs"]
mod common;

#[path = "oracle_backtrace_local_remake.rs"]
mod oracle_backtrace_local_remake;

#[path = "oracle_backtrace_locals.rs"]
mod oracle_backtrace_locals;

#[path = "oracle_buffer_local_completion.rs"]
mod oracle_buffer_local_completion;

#[path = "oracle_circular_lists.rs"]
mod oracle_circular_lists;

#[path = "oracle_coding_detection.rs"]
mod oracle_coding_detection;

#[path = "oracle_coding_save.rs"]
mod oracle_coding_save;

#[path = "oracle_combination.rs"]
mod oracle_combination;

#[path = "oracle_copy_lisp_face_defaults.rs"]
mod oracle_copy_lisp_face_defaults;

#[path = "oracle_default_threads.rs"]
mod oracle_default_threads;

#[path = "oracle_divergence.rs"]
mod oracle_divergence;

#[path = "oracle_divergence_combo_cl.rs"]
mod oracle_divergence_combo_cl;

#[path = "oracle_divergence_combo_complex.rs"]
mod oracle_divergence_combo_complex;

#[path = "oracle_divergence_combo_general.rs"]
mod oracle_divergence_combo_general;

#[path = "oracle_divergence_combo_strict.rs"]
mod oracle_divergence_combo_strict;

#[path = "oracle_dual_width_text.rs"]
mod oracle_dual_width_text;

#[path = "oracle_eob_overlay_motion.rs"]
mod oracle_eob_overlay_motion;

#[path = "oracle_extended_keysyms.rs"]
mod oracle_extended_keysyms;

#[path = "oracle_face_theme.rs"]
mod oracle_face_theme;

#[path = "oracle_fill_column_stipple.rs"]
mod oracle_fill_column_stipple;

#[path = "oracle_font_coverage.rs"]
mod oracle_font_coverage;

#[path = "oracle_frame_bar_parameters.rs"]
mod oracle_frame_bar_parameters;

#[path = "oracle_frame_focus.rs"]
mod oracle_frame_focus;

#[path = "oracle_gdl.rs"]
mod oracle_gdl;

#[path = "oracle_gdn_buffer.rs"]
mod oracle_gdn_buffer;

#[path = "oracle_gdn_compiled.rs"]
mod oracle_gdn_compiled;

#[path = "oracle_gdn_equality.rs"]
mod oracle_gdn_equality;

#[path = "oracle_green_alias_corners.rs"]
mod oracle_green_alias_corners;

#[path = "oracle_green_threads.rs"]
mod oracle_green_threads;

#[path = "oracle_green_two_owner.rs"]
mod oracle_green_two_owner;

#[path = "oracle_integer_width.rs"]
mod oracle_integer_width;

#[path = "oracle_minibuffer_selected_window.rs"]
mod oracle_minibuffer_selected_window;

#[path = "oracle_mode_line_flow.rs"]
mod oracle_mode_line_flow;

#[path = "oracle_mode_line_live_spine.rs"]
mod oracle_mode_line_live_spine;

#[path = "oracle_org_mode.rs"]
mod oracle_org_mode;

#[path = "oracle_pixel_measurement.rs"]
mod oracle_pixel_measurement;

#[path = "oracle_prefix_help.rs"]
mod oracle_prefix_help;

#[path = "oracle_shift_selection.rs"]
mod oracle_shift_selection;

#[path = "oracle_tab_line.rs"]
mod oracle_tab_line;

#[path = "oracle_tls_loopback.rs"]
mod oracle_tls_loopback;

#[path = "oracle_tty_face_lifecycle.rs"]
mod oracle_tty_face_lifecycle;

#[path = "oracle_tty_redisplay_viewport.rs"]
mod oracle_tty_redisplay_viewport;

#[path = "oracle_tty_terminal_output.rs"]
mod oracle_tty_terminal_output;

#[path = "tls_fixture_cleanup.rs"]
mod tls_fixture_cleanup;

#[path = "treesit_grammar_loading.rs"]
mod treesit_grammar_loading;

#[path = "oracle_startup_frame_lifecycle.rs"]
mod oracle_startup_frame_lifecycle;

#[path = "oracle_category_word_wrap.rs"]
mod oracle_category_word_wrap;
