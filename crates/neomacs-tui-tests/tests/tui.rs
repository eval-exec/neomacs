//! Every TUI pair test in one binary.
//!
//! Each former per-file integration binary became a module here: 37 separate
//! static links of neovm-core + Cranelift bought nothing that nextest needs -
//! it runs every test in its own process regardless. Keep new tests as modules
//! under tests/ and list them below.
#![cfg(unix)]

mod support;

#[path = "basic.rs"]
mod basic;
#[path = "buffers.rs"]
mod buffers;
#[path = "child_frames.rs"]
mod child_frames;
#[path = "command_loop_subreads.rs"]
mod command_loop_subreads;
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
#[path = "frame_visibility.rs"]
mod frame_visibility;
#[path = "help_describe.rs"]
mod help_describe;
#[path = "issue_140_hscroll.rs"]
mod issue_140_hscroll;
#[path = "issue_170_centered_buffer.rs"]
mod issue_170_centered_buffer;
#[path = "issue_254.rs"]
mod issue_254;
#[path = "mark_region_fill.rs"]
mod mark_region_fill;
#[path = "menu_bar.rs"]
mod menu_bar;
#[path = "modes.rs"]
mod modes;
#[path = "org.rs"]
mod org;
#[path = "programming.rs"]
mod programming;
#[path = "project.rs"]
mod project;
#[path = "raw_terminal_snapshot_test.rs"]
mod raw_terminal_snapshot_test;
#[path = "redisplay_display_vars.rs"]
mod redisplay_display_vars;
#[path = "registers_bookmarks.rs"]
mod registers_bookmarks;
#[path = "replace_sort.rs"]
mod replace_sort;
#[path = "saving_insert.rs"]
mod saving_insert;
#[path = "search.rs"]
mod search;
#[path = "shell_compile.rs"]
mod shell_compile;
#[path = "source_navigation.rs"]
mod source_navigation;
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
