//! Every GUI integration suite in one test binary.
//!
//! native_display stays a separate target: the native display contract
//! selects it with --test. Each former per-file binary linked the display
//! stack again; nextest runs every test in its own process.

#[path = "buffer_local_completion.rs"]
mod buffer_local_completion;

#[path = "child_frame_animation.rs"]
mod child_frame_animation;

#[path = "child_frame_position.rs"]
mod child_frame_position;

#[path = "child_frame_sizing.rs"]
mod child_frame_sizing;

#[path = "clatter_notification.rs"]
mod clatter_notification;

#[path = "close_confirmation.rs"]
mod close_confirmation;

#[path = "coding_detection.rs"]
mod coding_detection;

#[path = "coding_save.rs"]
mod coding_save;

#[path = "colrv1_emoji.rs"]
mod colrv1_emoji;

#[path = "compact_frame_geometry.rs"]
mod compact_frame_geometry;

#[path = "config_env_boot.rs"]
mod config_env_boot;

#[path = "desktop_font_startup.rs"]
mod desktop_font_startup;

#[path = "desktop_font_updates.rs"]
mod desktop_font_updates;

#[path = "desktop_font_pins.rs"]
mod desktop_font_pins;

#[path = "display_line_numbers.rs"]
mod display_line_numbers;

#[path = "display_window_geometry.rs"]
mod display_window_geometry;

#[path = "dual_width_fixed_pitch.rs"]
mod dual_width_fixed_pitch;

#[path = "eob_overlay_motion.rs"]
mod eob_overlay_motion;

#[path = "eval_buffer_shorthands_gui_oracle.rs"]
mod eval_buffer_shorthands_gui_oracle;

#[path = "extended_keysyms.rs"]
mod extended_keysyms;

#[path = "face_background_mode_gui_oracle.rs"]
mod face_background_mode_gui_oracle;

#[path = "fill_column_stipple.rs"]
mod fill_column_stipple;

#[path = "font_coverage.rs"]
mod font_coverage;

#[path = "fontset_han_repertory.rs"]
mod fontset_han_repertory;

#[path = "frame_bar_parameters.rs"]
mod frame_bar_parameters;

#[path = "frame_resize_oracle.rs"]
mod frame_resize_oracle;

#[path = "glyph_coverage.rs"]
mod glyph_coverage;

#[path = "golden_emoji_sheet.rs"]
mod golden_emoji_sheet;

#[path = "harness_contract.rs"]
mod harness_contract;

#[path = "idle_timer_output.rs"]
mod idle_timer_output;

#[path = "integer_width.rs"]
mod integer_width;

#[path = "recenter_box_drawing.rs"]
mod recenter_box_drawing;

#[path = "interaction_geometry.rs"]
mod interaction_geometry;

#[path = "issue_417_startup.rs"]
mod issue_417_startup;

#[path = "issue_582_fullscreen.rs"]
mod issue_582_fullscreen;

#[path = "issue_621_mouse_drag_selection_gui_oracle.rs"]
mod issue_621_mouse_drag_selection_gui_oracle;

#[path = "kill_minibuffer_window_gui_oracle.rs"]
mod kill_minibuffer_window_gui_oracle;

#[path = "ligature_compose.rs"]
mod ligature_compose;

#[path = "macos_native_menus.rs"]
mod macos_native_menus;

#[path = "minibuffer_line_gui.rs"]
mod minibuffer_line_gui;

#[path = "minibuffer_selected_window.rs"]
mod minibuffer_selected_window;

#[path = "mouse_position_in_root_frame_gui_oracle.rs"]
mod mouse_position_in_root_frame_gui_oracle;

#[path = "native_frame_focus.rs"]
mod native_frame_focus;

#[path = "native_menus.rs"]
mod native_menus;

#[path = "native_scrolling.rs"]
mod native_scrolling;

#[path = "org_line_spacing.rs"]
mod org_line_spacing;

#[path = "org_silent_selection.rs"]
mod org_silent_selection;

#[path = "pixel_measurement.rs"]
mod pixel_measurement;

#[path = "prefix_help.rs"]
mod prefix_help;

#[path = "present_contract.rs"]
mod present_contract;

#[path = "process_plist_isolation.rs"]
mod process_plist_isolation;

#[path = "process_send_encoding.rs"]
mod process_send_encoding;

#[path = "real_gui_smoke.rs"]
mod real_gui_smoke;

#[path = "resize_presentation.rs"]
mod resize_presentation;

#[path = "scroll_bar_gui_oracle.rs"]
mod scroll_bar_gui_oracle;

#[path = "shift_selection.rs"]
mod shift_selection;

#[path = "stalled_tls.rs"]
mod stalled_tls;

#[path = "startup_failure.rs"]
mod startup_failure;

#[path = "startup_user_settings.rs"]
mod startup_user_settings;

#[path = "svg_animation.rs"]
mod svg_animation;

#[path = "svg_tag_font_remapping.rs"]
mod svg_tag_font_remapping;

#[path = "telega_fixture_gui.rs"]
mod telega_fixture_gui;

#[path = "terminal_identity.rs"]
mod terminal_identity;

#[path = "thin_only_font.rs"]
mod thin_only_font;

#[path = "treesit_debug_mode_line.rs"]
mod treesit_debug_mode_line;

#[path = "window_chrome.rs"]
mod window_chrome;

#[path = "x11_presentation.rs"]
mod x11_presentation;

#[path = "xpm_named_colors.rs"]
mod xpm_named_colors;

#[cfg(target_os = "linux")]
#[path = "wayland_readiness.rs"]
mod wayland_readiness;

#[cfg(target_os = "linux")]
#[path = "wayland_shm_pool_growth.rs"]
mod wayland_shm_pool_growth;

#[path = "category_word_wrap.rs"]
mod category_word_wrap;
