//! Cell-level display assertion ratchet for `crates/neomacs-tui-tests`.
//!
//! # Purpose
//!
//! A TUI regression test whose subject is a face, cursor, or chrome element
//! (menu bar, tab bar, mode line, line-number gutter) cannot catch a color or
//! attribute defect (such as #856) if it only asserts on plain screen text.
//!
//! This guard scans every `*.rs` file under `tests/` with `syn`, classifies
//! every `#[test]` function into:
//! - [`AssertionStrength::TextAndCells`] — asserts full cell-level parity
//!   (colors, style flags, cursor, or raw terminal cell snapshots) against GNU;
//! - [`AssertionStrength::TextAndFace`] — asserts partial cell/wire face color
//!   attributes (e.g. background-only or wire SGR color codes);
//! - [`AssertionStrength::TextOnly`] — asserts no terminal cell attributes
//!   (buffer text, Lisp/JSON oracles, PTY byte streams, lifecycle, or routing).
//!
//! Every non-`TextAndCells` test must have an explicit entry in
//! [`NON_CELL_ALLOWLIST`] with a typed [`NonCellAssertionKind`] and reason,
//! and the exception counts (`text_and_face`, `text_only`,
//! `blocked_chrome_divergence`) must equal [`CELL_ASSERTION_PINS`] **exactly**
//! (same shape as `SLEEP_PINS` in `wall_clock_guard_test.rs`), while
//! `text_and_cells` and `total` are non-shrinking **floors**
//! (`min_text_and_cells`, `min_total`). Adding a new strong `TextAndCells`
//! test therefore passes freely without touching this file, whereas adding a
//! new text-only test, downgrading a strong test, or silently losing test
//! discovery fails this guard until reviewed.
//!
//! # Artifact-only `RawTerminalSnapshot` defense
//!
//! Calling `RawTerminalSnapshot::capture_*` merely to write a debug `.ansi`
//! file to disk (with no assertion or comparison on the snapshot) is NOT
//! counted as `TextAndCells`. A `RawTerminalSnapshot` counts as `TextAndCells`
//! only when the test (or its helper chain) asserts or compares on it
//! (`assert_raw_terminal_snapshots_eq`, `.exact_differences(...)`,
//! `assert_eq!`/`assert_ne!`/`assert_snapshot`/`expect`/`expect_file` with the
//! snapshot or its `ansi_grid()`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AssertionStrength {
    TextAndCells,
    TextAndFace,
    TextOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NonCellAssertionKind {
    /// Partial cell/wire face attributes (`TextAndFace`).
    PartialFaceOrSgr,
    /// Lisp/JSON/hook/posn/mode-line-eval/long-line/RGB-sweep oracle (`TextOnly`).
    LispOracleOrProperty,
    /// PTY byte-stream, `tty-mode-reset-strings`, signals, shutdown, relay (`TextOnly`).
    PtyStreamOrLifecycle,
    /// Event loop, timers, TLS, process plist/encoding, key/focus routing (`TextOnly`).
    EventOrProcessRouting,
    /// Buffer text, coding, completion, workflow, or harness unit check (`TextOnly`).
    BufferTextOrWorkflow,
    /// Chrome test blocked on a documented pre-existing divergence (`TextOnly`).
    BlockedChromeDivergence,
}

impl NonCellAssertionKind {
    fn expected_strength(self) -> AssertionStrength {
        match self {
            Self::PartialFaceOrSgr => AssertionStrength::TextAndFace,
            Self::LispOracleOrProperty
            | Self::PtyStreamOrLifecycle
            | Self::EventOrProcessRouting
            | Self::BufferTextOrWorkflow
            | Self::BlockedChromeDivergence => AssertionStrength::TextOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellAssertionPin {
    /// Non-shrinking floor on total discovered integration tests (guards
    /// against silent loss of test discovery or dropped modules).
    min_total: usize,
    /// Non-shrinking floor on `TextAndCells` tests (adding new strong tests
    /// passes freely; shrinking fails).
    min_text_and_cells: usize,
    /// Exact pin on partial-face (`TextAndFace`) exceptions.
    text_and_face: usize,
    /// Exact pin on `TextOnly` exceptions.
    text_only: usize,
    /// Exact pin on `BlockedChromeDivergence` exceptions.
    blocked_chrome_divergence: usize,
}

/// Crate-wide ratchet pins (same exception-pin shape as `SLEEP_PINS`, plus
/// non-shrinking floors on `total` and `text_and_cells`).
///
/// Baseline on `c2ab78b0e9` before targeted conversions:
/// `total = 1164, text_and_cells = 990 (85.1%), text_and_face = 2, text_only = 172`.
/// After converting 13 attribute-subject rows across 6 files and rebasing onto
/// `d6f1de3ca8` (+2 text-only margin-truncation rows in `issue_702_truncated_margin_parity.rs`):
/// `min_total = 1166, min_text_and_cells = 1003 (86.0%), text_and_face = 2, text_only = 161`.
const CELL_ASSERTION_PINS: &[(&str, CellAssertionPin)] = &[(
    "neomacs-tui-tests",
    CellAssertionPin {
        min_total: 1166,
        min_text_and_cells: 1003,
        text_and_face: 2,
        text_only: 161,
        blocked_chrome_divergence: 1,
    },
)];

#[derive(Debug, Clone, Copy)]
struct NonCellAllowance {
    file: &'static str,
    test: &'static str,
    kind: NonCellAssertionKind,
    reason: &'static str,
}

const NON_CELL_ALLOWLIST: &[NonCellAllowance] = &[
    NonCellAllowance {
        file: "tests/bidi_paragraph_direction_tui_oracle.rs",
        test: "tty_rtl_rows_match_gnu_cells_cursor_and_extend_face",
        kind: NonCellAssertionKind::PartialFaceOrSgr,
        reason: "compares cell contents, bgcolor, and cursor against GNU; does not assert fg/bold/underline",
    },
    NonCellAllowance {
        file: "tests/buffer_local_completion.rs",
        test: "tab_completion_observes_a_buffer_local_case_policy",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "buffer-local completion-ignore-case text completion",
    },
    NonCellAllowance {
        file: "tests/coding_detection.rs",
        test: "preferred_gbk_and_windows_1252_files_match_gnu_in_terminal",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "decoded GBK/Windows-1252 buffer text parity against GNU",
    },
    NonCellAllowance {
        file: "tests/coding_save.rs",
        test: "shift_jis_save_matches_gnu_after_keyboard_edit_and_save",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "Shift-JIS edited file round-trip byte parity against GNU",
    },
    NonCellAllowance {
        file: "tests/dual_width_fixed_pitch.rs",
        test: "cjk_position_advances_match_gnu_in_a_real_terminal",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp CJK posn advance geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/encoded_file.rs",
        test: "oversized_compiled_file_recovers_and_accepts_terminal_input",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "Neomacs-specific oversized .elc recovery and input liveness",
    },
    NonCellAllowance {
        file: "tests/eob_overlay_motion.rs",
        test: "motion_from_accessible_eob_with_virtual_overlay_rows_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp vertical motion distance oracle across EOB overlays",
    },
    NonCellAllowance {
        file: "tests/eob_overlay_motion.rs",
        test: "motion_into_eob_overlay_from_source_line_preserves_gnu_distance",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp vertical motion distance oracle across EOB overlays",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "async_subprocess_output_drives_redisplay",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "interactive_switch_synchronous_shell_command_stays_responsive",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "keyboard_input_echoes_into_scratch",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "keyboard_quit_interrupts_blocking_sleep_for",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "synchronous_shell_command_completes_and_editor_stays_responsive",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "timer_fires_and_redisplays_while_idle",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "timer_queued_unread_event_wakes_interactive_read_event_gnu",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "timer_queued_unread_event_wakes_interactive_read_event_neomacs",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/event_loop.rs",
        test: "unbound_key_echoes_is_undefined",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "command-loop input, timer, subprocess, and C-g responsiveness",
    },
    NonCellAllowance {
        file: "tests/extended_keysyms.rs",
        test: "configured_terminal_sequences_dispatch_extended_function_keys",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "extended function-key terminal escape sequence dispatch",
    },
    NonCellAllowance {
        file: "tests/font_coverage.rs",
        test: "font_coverage_validates_non_unicode_characters_before_frame_in_terminal",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "non-Unicode character validation text check before frame open",
    },
    NonCellAllowance {
        file: "tests/frame_bar_parameters.rs",
        test: "frame_bar_parameters_and_root_geometry_match_gnu_on_tty",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp/JSON frame-bar parameter and root-window geometry oracle",
    },
    NonCellAllowance {
        file: "tests/frame_focus.rs",
        test: "tty_new_and_reselected_frames_route_keys_to_their_own_buffers",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "virtual TTY frame key-routing to per-frame buffers",
    },
    NonCellAllowance {
        file: "tests/gnu_redisplay_mutation_oracle.rs",
        test: "hidden_buffer_rename_preserves_other_mode_line_eval_counts_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay mutation/hook-count trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/gnu_redisplay_mutation_oracle.rs",
        test: "kill_all_local_variables_preserves_other_mode_line_eval_counts_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay mutation/hook-count trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/gnu_redisplay_mutation_oracle.rs",
        test: "redraw_display_targeted_hooks_and_idle_counts_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay mutation/hook-count trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/gnu_redisplay_mutation_oracle.rs",
        test: "redraw_frame_targeted_hooks_and_idle_counts_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay mutation/hook-count trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/gnu_redisplay_mutation_oracle.rs",
        test: "tty_height_preserves_width_and_same_size_hook_counts_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay mutation/hook-count trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/gnu_redisplay_mutation_oracle.rs",
        test: "tty_width_preserves_height_and_same_size_hook_counts_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay mutation/hook-count trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/ibuffer.rs",
        test: "ibuffer_auto_mode_initializes_its_state_vector",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "Lisp ibuffer-auto-mode state vector initialization check",
    },
    NonCellAllowance {
        file: "tests/idle_timer_output.rs",
        test: "idle_timer_fires_amid_ordinary_timers_and_subprocess_output",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "idle-timer scheduling amid repeating timers and subprocess output",
    },
    NonCellAllowance {
        file: "tests/issue_249_special_key_bindings.rs",
        test: "disabled_command_declines_on_n_without_inserting_it",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "disabled-command decline and M-<down> *Completions* point motion",
    },
    NonCellAllowance {
        file: "tests/issue_249_special_key_bindings.rs",
        test: "meta_down_moves_completions_point_to_the_first_candidate",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "disabled-command decline and M-<down> *Completions* point motion",
    },
    NonCellAllowance {
        file: "tests/issue_621_mouse_drag_selection.rs",
        test: "a_tty_mouse_drag_leaves_the_region_active_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp post-drag region-active-p/mark/point oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/columns.rs",
        test: "current_column_approximates_long_lines_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/columns.rs",
        test: "current_column_approximation_is_not_cached_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/columns.rs",
        test: "current_column_below_the_long_line_threshold_is_exact",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/columns.rs",
        test: "current_column_with_nil_threshold_is_exact_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/columns.rs",
        test: "long_line_hscroll_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/columns.rs",
        test: "move_to_column_long_truncated_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/current_column_1.rs",
        test: "current_column_1_long_truncated_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/current_column_1.rs",
        test: "current_column_1_removed_property_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/current_column_1.rs",
        test: "current_column_after_inserting_propertized_text_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/current_column_1.rs",
        test: "current_column_stops_at_begv_in_a_narrowed_buffer",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/declining.rs",
        test: "declining_fontification_probe_short_lines_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/declining.rs",
        test: "declining_fontification_probe_skips_forced_start_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/declining.rs",
        test: "declining_fontification_probe_stays_in_its_row_for_wrapped_lines",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/declining.rs",
        test: "declining_fontification_probes_face_stops_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/declining.rs",
        test: "declining_fontification_probes_truncated_tail_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/declining.rs",
        test: "declining_fontification_visits_gnu_stops_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/fontification.rs",
        test: "fontification_functions_accessible_bounds_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/fontification.rs",
        test: "fontification_narrowing_centres_on_window_point",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/fontification.rs",
        test: "long_line_optimizations_p_after_redisplay_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/fontification.rs",
        test: "long_line_optimizations_p_interleaved_unchanged_modified_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/fontification.rs",
        test: "long_line_optimizations_p_text_props_then_char_insert_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/fontification.rs",
        test: "pre_command_hook_accessible_bounds_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/partial_width.rs",
        test: "move_to_column_other_frame_window_stays_exact_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/partial_width.rs",
        test: "move_to_column_partial_width_window_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/scroll.rs",
        test: "recenter_long_line_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/scroll.rs",
        test: "scroll_up_long_truncated_line_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/scroll.rs",
        test: "scroll_up_numeric_arg_is_not_the_long_line_path",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/scroll.rs",
        test: "scroll_up_starts_from_window_start_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_640_long_lines/scroll.rs",
        test: "whole_scroll_runs_no_fontification",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp long-line column/scroll/fontification-bounds oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_654_centered_cursor.rs",
        test: "centered_cursor_keeps_point_on_gnu_s_scroll_margin_row",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp per-motion count-lines scroll-margin row oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_656_kill_minibuffer.rs",
        test: "kill_buffer_refuses_the_buffer_shown_in_the_active_minibuffer",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "kill-buffer refusal on active minibuffer buffer",
    },
    NonCellAllowance {
        file: "tests/issue_658_eval_buffer_shorthands.rs",
        test: "eval_buffer_uses_the_visited_files_symbol_shorthands",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "eval-buffer read-symbol-shorthands evaluation result",
    },
    NonCellAllowance {
        file: "tests/issue_660_mouse_position_in_root_frame.rs",
        test: "mouse_position_in_root_frame_answers_on_a_tty",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mouse-position coordinate oracle on TTY",
    },
    NonCellAllowance {
        file: "tests/issue_702_truncated_margin_parity.rs",
        test: "truncated_long_line_with_left_and_right_margins_matches_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "truncated line and display-margin text-column placement parity against GNU",
    },
    NonCellAllowance {
        file: "tests/issue_702_truncated_margin_parity.rs",
        test: "truncated_long_line_with_right_margin_places_dollar_flush_like_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "truncated line and display-margin text-column placement parity against GNU",
    },
    NonCellAllowance {
        file: "tests/kill_emacs_shutdown.rs",
        test: "interactive_kill_emacs_calls_the_query_symbol_and_continues_after_yes",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "interactive kill-emacs query-symbol shutdown lifecycle",
    },
    NonCellAllowance {
        file: "tests/kill_emacs_shutdown.rs",
        test: "interactive_kill_emacs_keeps_the_original_query_symbol_after_unintern",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "interactive kill-emacs query-symbol shutdown lifecycle",
    },
    NonCellAllowance {
        file: "tests/kill_emacs_shutdown.rs",
        test: "interactive_query_nonlocal_exit_aborts_shutdown_and_allows_a_second_kill",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "interactive kill-emacs query-symbol shutdown lifecycle",
    },
    NonCellAllowance {
        file: "tests/minibuffer_barrier.rs",
        test: "eval_expression_waits_out_a_scrolled_minibuffer",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "eval_expression scrolled-minibuffer exit barrier",
    },
    NonCellAllowance {
        file: "tests/minibuffer_quit_cleanup.rs",
        test: "minibuffer_quit_cleanup_from_command_matches_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "minibuffer quit cleanup state parity against GNU",
    },
    NonCellAllowance {
        file: "tests/minibuffer_quit_cleanup.rs",
        test: "minibuffer_quit_cleanup_from_startup_load_matches_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "minibuffer quit cleanup state parity against GNU",
    },
    NonCellAllowance {
        file: "tests/minibuffer_selected_window.rs",
        test: "completion_renders_and_accepts_after_the_invoking_window_is_deleted",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "completion acceptance after deleting invoking window",
    },
    NonCellAllowance {
        file: "tests/minor_mode_order_repro.rs",
        test: "doom_minor_mode_order_matches_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "doom-repro diagnostic comparing minor-mode-alist load order",
    },
    NonCellAllowance {
        file: "tests/mode_line_element_attribution_oracle.rs",
        test: "mode_line_coding_fields_read_live_values_after_callbacks_equal_gnu_properties",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_element_attribution_oracle.rs",
        test: "mode_line_element_attribution_all_22_default_w100_outputs_equal_gnu_properties",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_eval_count_oracle.rs",
        test: "mode_line_eval_counts_match_gnu_under_the_gnu_gate",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_eval_count_oracle.rs",
        test: "mode_line_eval_counts_under_the_legacy_gate_diverge_only_where_recorded",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_gc_lifetime_oracle.rs",
        test: "ancestor_and_width_property_accumulators_survive_gc_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_gc_lifetime_oracle.rs",
        test: "copied_source_plists_survive_later_eval_gc_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_gc_lifetime_oracle.rs",
        test: "fresh_eval_source_and_plists_survive_sibling_gc_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_gc_lifetime_oracle.rs",
        test: "replaced_format_and_reentrant_output_survive_gc_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_min_width_boundary_oracle.rs",
        test: "min_width_eol_indicator_percent_keeps_run_open_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_min_width_boundary_oracle.rs",
        test: "min_width_equal_nonidentical_predecessor_keeps_run_open_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_min_width_boundary_oracle.rs",
        test: "min_width_frame_name_percent_keeps_run_open_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_min_width_boundary_oracle.rs",
        test: "min_width_mode_name_percent_keeps_run_open_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_min_width_boundary_oracle.rs",
        test: "min_width_other_source_nonzero_literal_keeps_run_open_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_min_width_boundary_oracle.rs",
        test: "min_width_same_source_predecessor_closes_run_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_numeric_padding_oracle.rs",
        test: "numeric_padding_gaps_precision_and_width_identity_match_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_numeric_padding_oracle.rs",
        test: "numeric_padding_only_child_has_no_outer_min_width_run_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_numeric_padding_oracle.rs",
        test: "numeric_padding_prefix_does_not_start_outer_min_width_run_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_percent_input_oracle.rs",
        test: "percent_inputs_follow_live_eval_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/mode_line_point_oracle.rs",
        test: "point_percent_specs_follow_eval_moves_and_restrictions_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp mode-line evaluation/attribution/GC/width oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/p640n1_probe_row_oracle.rs",
        test: "probe_row_follows_gnus_matrix_reuse_predicate",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp matrix-reuse probe oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/package_tui/ace_window_test/mod.rs",
        test: "ace_window_tui_selects_labelled_windows_and_cancels_without_editing",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "ace-window (aw-background nil) window selection and C-g dispatch",
    },
    NonCellAllowance {
        file: "tests/package_tui/ace_window_test/mod.rs",
        test: "ace_window_tui_two_windows_switch_without_a_label_prompt",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "ace-window (aw-background nil) window selection and C-g dispatch",
    },
    NonCellAllowance {
        file: "tests/package_tui/clatter_test/mod.rs",
        test: "clatter_process_filter_notification_completes_and_remains_visible",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "Clatter process-filter D-Bus notification payload and completion",
    },
    NonCellAllowance {
        file: "tests/package_tui/helm_gitignore_test/mod.rs",
        test: "helm_gitignore_public_workflows_match_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "helm-gitignore HTTP request/cache/generated-buffer workflow parity",
    },
    NonCellAllowance {
        file: "tests/package_tui/mwim_test/mod.rs",
        test: "mwim_real_visual_and_logical_line_keys_match_gnu",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "MWIM visual/logical point-motion snapshot parity against GNU",
    },
    NonCellAllowance {
        file: "tests/pixel_measurement.rs",
        test: "pixel_measurements_match_gnu_terminal_glyph_positions",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp terminal glyph pixel-measurement oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_current_matrix_clear_oracle.rs",
        test: "tty_default_same_buffer_setter_clears_current_matrix_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn/current-matrix clear oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_current_matrix_clear_oracle.rs",
        test: "tty_keep_margins_same_buffer_setter_preserves_current_matrix_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn/current-matrix clear oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_object_extent_oracle.rs",
        test: "tty_actual_mouse_and_query_current_matrix_dimensions_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn-at-x-y/object-extent geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_object_extent_oracle.rs",
        test: "tty_canonical_company_corfu_popup_queries_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn-at-x-y/object-extent geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_object_extent_oracle.rs",
        test: "tty_posn_cold_unavailable_and_split_top_chrome_origins_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn-at-x-y/object-extent geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_object_extent_oracle.rs",
        test: "tty_posn_line_end_newline_empty_and_nonempty_eob_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn-at-x-y/object-extent geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_object_extent_oracle.rs",
        test: "tty_posn_stale_short_matrix_default_and_colored_tails_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn-at-x-y/object-extent geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/posn_object_extent_oracle.rs",
        test: "tty_posn_tab_wide_and_composition_cells_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp posn-at-x-y/object-extent geometry oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/pre_redisplay_function_oracle.rs",
        test: "pre_redisplay_functions_run_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp pre-redisplay-functions hook trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/process_plist_isolation.rs",
        test: "accepted_tcp_process_plists_match_gnu_shallow_isolation",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "accepted TCP process plist shallow isolation oracle",
    },
    NonCellAllowance {
        file: "tests/process_send_encoding.rs",
        test: "japanese_process_string_and_region_sends_match_gnu_bytes",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "process-send-string/region raw byte encoding oracle",
    },
    NonCellAllowance {
        file: "tests/project.rs",
        test: "git_project_fixture_removes_its_tree_when_its_owner_drops",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "GitProjectFixture tempdir cleanup unit check",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "active_mini_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "active_mini_prefix_shrink_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "echo_clear_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "echo_grow_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "edit_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "forced_state_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "idle_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "overlay_mini_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "point_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "resize_local_and_global_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "scroll_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "selection_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_order_oracle.rs",
        test: "split_local_and_global_hook_order_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook ordering trace oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "buffer_local_inhibit_defers_targets_until_release_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "buffer_local_pre_function_uses_current_buffer_scope_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "local_query_and_public_inhibit_rebinding_preserve_private_guard_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "local_signal_continues_live_tail_and_remains_installed_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "local_throw_records_on_unwind_then_recovers_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "mini_shrink_precedes_inactive_callback_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "mini_sizing_throw_restores_context_before_next_redisplay_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "pre_query_then_throw_preserves_pending_targets_and_recovers_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_hook_transfer_oracle.rs",
        test: "pre_signal_is_demoted_and_next_pass_matches_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp redisplay hook unwind/transfer oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_mini_source_start_oracle.rs",
        test: "after_string_after_final_newline_uses_source_line_start_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp minibuffer source window-start oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_mini_source_start_oracle.rs",
        test: "after_string_at_eob_keeps_prompt_line_start_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp minibuffer source window-start oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_mini_source_start_oracle.rs",
        test: "after_string_at_wrapped_eob_uses_source_screen_line_start_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp minibuffer source window-start oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/redisplay_mini_source_start_oracle.rs",
        test: "ordinary_buffer_overflow_keeps_last_source_screenful_like_gnu",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp minibuffer source window-start oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/scroll_bar_tty.rs",
        test: "scroll_bar_tty_frame_parameter_reserves_no_column",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp scroll-bar TTY frame-parameter no-op oracle against GNU",
    },
    NonCellAllowance {
        file: "tests/send_string_to_terminal.rs",
        test: "send_string_to_terminal_reaches_the_pty_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY byte-stream delivery of send-string-to-terminal",
    },
    NonCellAllowance {
        file: "tests/stalled_tls.rs",
        test: "stalled_tls_keeps_timers_and_terminal_redisplay_alive",
        kind: NonCellAssertionKind::EventOrProcessRouting,
        reason: "stalled TLS handshake non-blocking timer/redisplay liveness",
    },
    NonCellAllowance {
        file: "tests/startup_terminal_initialization.rs",
        test: "gnu_editor_uses_attested_oracle_program",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "startup terminal init file loading and oracle program check",
    },
    NonCellAllowance {
        file: "tests/startup_terminal_initialization.rs",
        test: "startup_runs_dash_l_on_every_terminal_gnu_does",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "startup terminal init file loading and oracle program check",
    },
    NonCellAllowance {
        file: "tests/treesit_debug_mode_line.rs",
        test: "go_grammar_loads_and_multibyte_mode_line_renders_with_debug_logging",
        kind: NonCellAssertionKind::BlockedChromeDivergence,
        reason: "blocked on pre-existing TTY set-frame-size divergence (mode line lands on row 29 vs GNU row 28)",
    },
    NonCellAllowance {
        file: "tests/tty_color_index.rs",
        test: "face_colours_reach_the_wire_as_the_index_lisp_computed",
        kind: NonCellAssertionKind::PartialFaceOrSgr,
        reason: "compares raw PTY wire SGR color codes before each face marker against GNU",
    },
    NonCellAllowance {
        file: "tests/tty_color_index.rs",
        test: "tty_color_approximate_matches_gnu_over_the_whole_rgb_cube",
        kind: NonCellAssertionKind::LispOracleOrProperty,
        reason: "Lisp tty-color-approximate 5,832-sample RGB sweep against GNU",
    },
    NonCellAllowance {
        file: "tests/tty_emergency_escape.rs",
        test: "controlling_tty_second_ctrl_g_abort_terminates_with_sigabrt",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "controlling-TTY double C-g SIGTSTP/SIGABRT/reset-strings lifecycle",
    },
    NonCellAllowance {
        file: "tests/tty_emergency_escape.rs",
        test: "controlling_tty_second_ctrl_g_emits_tty_mode_reset_and_set_strings",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "controlling-TTY double C-g SIGTSTP/SIGABRT/reset-strings lifecycle",
    },
    NonCellAllowance {
        file: "tests/tty_emergency_escape.rs",
        test: "controlling_tty_second_ctrl_g_suspends_autosaves_and_continues",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "controlling-TTY double C-g SIGTSTP/SIGABRT/reset-strings lifecycle",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "secondary_tty_delete_terminal_and_exit_isolate_reset_strings_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "suspend_emacs_errors_with_suspended_second_tty_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "suspend_emacs_hooks_cannot_suspend_and_bad_arg_match_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_cannot_suspend_real_subshell_survives_sigint_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_reset_strings_emitted_on_exit_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_strings_emitted_on_emergency_escape_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_strings_emitted_on_suspend_emacs_and_resume_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_strings_emitted_on_suspend_tty_and_resume_tty_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_strings_real_sigtstp_stop_and_sigcont_resume_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_mode_strings.rs",
        test: "tty_mode_suspend_emacs_applies_resize_before_resume_hook_like_gnu",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "raw PTY tty-mode-reset/set-strings byte order, signals, and hooks",
    },
    NonCellAllowance {
        file: "tests/tty_relay.rs",
        test: "auxiliary_frame_draws_past_its_opening_width_after_resize",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "auxiliary TTY input relay, C-g quit, and post-resize width",
    },
    NonCellAllowance {
        file: "tests/tty_relay.rs",
        test: "auxiliary_input_survives_busy_primary",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "auxiliary TTY input relay, C-g quit, and post-resize width",
    },
    NonCellAllowance {
        file: "tests/tty_relay.rs",
        test: "busy_lisp_quits_from_raw_cg",
        kind: NonCellAssertionKind::PtyStreamOrLifecycle,
        reason: "auxiliary TTY input relay, C-g quit, and post-resize width",
    },
];

/// Function identifiers whose invocation directly establishes `TextAndCells`.
const DIRECT_CELL_CALLEES: &[&str] = &[
    "assert_pair_exact_display",
    "assert_pair_exact_display_with_path_pair",
    "assert_pair_exact_display_with_path_pairs",
    "assert_raw_terminal_snapshots_eq",
    "compare_session_displays",
    "compare_displays",
    "compare_displays_with_color_contract",
    "compare_displays_in_environment",
    "diff_screens",
];

/// Method identifiers whose invocation directly establishes `TextAndCells`.
const DIRECT_CELL_METHODS: &[&str] = &["assert_display", "exact_differences"];

/// Function identifiers whose invocation establishes partial `TextAndFace`.
const DIRECT_FACE_CALLEES: &[&str] = &["color_diffs_in", "color_sgr_before"];

/// Method identifiers whose invocation establishes partial `TextAndFace`.
const DIRECT_FACE_METHODS: &[&str] = &[
    "fgcolor",
    "bgcolor",
    "bold",
    "dim",
    "italic",
    "underline",
    "inverse",
];

/// Assertion macros that count a `RawTerminalSnapshot` in the same function
/// as an actual cell assertion (rather than a debug artifact write).
const ASSERT_MACROS: &[&str] = &["assert_eq", "assert_ne", "assert", "expect", "expect_file"];

/// Method identifiers indicating a snapshot/string is asserted upon.
const ASSERT_METHODS: &[&str] = &["assert_eq", "assert_debug_eq", "assert_snapshot"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FnKey {
    file: String,
    name: String,
}

#[derive(Debug, Default)]
struct FnFacts {
    is_test: bool,
    has_non_unit_return: bool,
    direct_cell: bool,
    direct_face: bool,
    uses_raw_snapshot: bool,
    has_any_assertion: bool,
    /// Unqualified calls `foo(...)`.
    unqualified_calls: BTreeSet<String>,
    /// Qualified single-segment module calls `submod::foo(...)`.
    qualified_calls: BTreeSet<(String, String)>,
}

struct FnBodyVisitor {
    facts: FnFacts,
    /// Nested `fn` items declared inside a test body (e.g. `fn capture(...)`).
    nested_fns: Vec<(String, FnFacts)>,
}

impl FnBodyVisitor {
    fn new(is_test: bool, has_non_unit_return: bool) -> Self {
        Self {
            facts: FnFacts {
                is_test,
                has_non_unit_return,
                ..FnFacts::default()
            },
            nested_fns: Vec::new(),
        }
    }

    fn inspect_path(&mut self, path: &syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|seg| seg.ident.to_string())
            .collect();
        if segments.iter().any(|seg| seg == "RawTerminalSnapshot") {
            self.facts.uses_raw_snapshot = true;
        }
        if let Some(last) = segments.last() {
            if DIRECT_CELL_CALLEES.contains(&last.as_str()) {
                self.facts.direct_cell = true;
            }
            if DIRECT_FACE_CALLEES.contains(&last.as_str()) {
                self.facts.direct_face = true;
            }
        }
    }

    fn inspect_call_path(&mut self, path: &syn::Path) {
        self.inspect_path(path);
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|seg| seg.ident.to_string())
            .collect();
        match segments.as_slice() {
            [name] => {
                self.facts.unqualified_calls.insert(name.clone());
            }
            [module, name] => {
                self.facts
                    .qualified_calls
                    .insert((module.clone(), name.clone()));
                self.facts.unqualified_calls.insert(name.clone());
            }
            [.., name] => {
                self.facts.unqualified_calls.insert(name.clone());
            }
            [] => {}
        }
    }
}

impl<'ast> Visit<'ast> for FnBodyVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        // Analyze nested helper functions separately so an artifact-only helper
        // (like `fn capture(...)` writing `.grid.ansi`) does not pollute its
        // enclosing test's `has_any_assertion` flag.
        let nested_non_unit = !matches!(node.sig.output, syn::ReturnType::Default);
        let mut child = FnBodyVisitor::new(has_test_attr(&node.attrs), nested_non_unit);
        visit::visit_block(&mut child, &node.block);
        self.nested_fns
            .push((node.sig.ident.to_string(), child.facts));
        self.nested_fns.extend(child.nested_fns);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(expr_path) = node.func.as_ref() {
            self.inspect_call_path(&expr_path.path);
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        self.inspect_path(&node.path);
        visit::visit_expr_path(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method = node.method.to_string();
        if DIRECT_CELL_METHODS.contains(&method.as_str()) {
            self.facts.direct_cell = true;
        }
        if DIRECT_FACE_METHODS.contains(&method.as_str()) {
            self.facts.direct_face = true;
        }
        if ASSERT_METHODS.contains(&method.as_str()) {
            self.facts.has_any_assertion = true;
        }
        self.facts.unqualified_calls.insert(method);
        visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if let Some(last) = node.path.segments.last() {
            let name = last.ident.to_string();
            if ASSERT_MACROS.contains(&name.as_str()) {
                self.facts.has_any_assertion = true;
            }
        }
        // Scan macro token trees for calls/paths inside `assert_eq!(RawTerminalSnapshot::..., ...)`.
        scan_tokens_for_facts(node.tokens.clone(), &mut self.facts);
        visit::visit_macro(self, node);
    }
}

fn scan_tokens_for_facts(tokens: proc_macro2::TokenStream, facts: &mut FnFacts) {
    let mut iter = tokens.into_iter().peekable();
    while let Some(tree) = iter.next() {
        match tree {
            proc_macro2::TokenTree::Ident(ident) => {
                let name = ident.to_string();
                if name == "RawTerminalSnapshot" {
                    facts.uses_raw_snapshot = true;
                }
                if DIRECT_CELL_CALLEES.contains(&name.as_str())
                    || DIRECT_CELL_METHODS.contains(&name.as_str())
                {
                    facts.direct_cell = true;
                }
                if DIRECT_FACE_CALLEES.contains(&name.as_str())
                    || DIRECT_FACE_METHODS.contains(&name.as_str())
                {
                    facts.direct_face = true;
                }
                if ASSERT_METHODS.contains(&name.as_str()) {
                    facts.has_any_assertion = true;
                }
                if matches!(iter.peek(), Some(proc_macro2::TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis)
                {
                    facts.unqualified_calls.insert(name);
                }
            }
            proc_macro2::TokenTree::Group(group) => {
                scan_tokens_for_facts(group.stream(), facts);
            }
            _ => {}
        }
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "test")
            && matches!(attr.meta, syn::Meta::Path(_))
    })
}

struct FileCollector {
    file: String,
    functions: Vec<(FnKey, FnFacts)>,
}

impl FileCollector {
    fn record_fn(
        &mut self,
        name: String,
        attrs: &[syn::Attribute],
        sig: &syn::Signature,
        block: &syn::Block,
    ) {
        let has_non_unit_return = !matches!(sig.output, syn::ReturnType::Default);
        let mut visitor = FnBodyVisitor::new(has_test_attr(attrs), has_non_unit_return);
        visit::visit_block(&mut visitor, block);
        // Nested helper functions live under `<outer>::<nested>` in the same file
        // so direct calls inside `<outer>` resolve to them cleanly.
        for (nested_name, nested_facts) in visitor.nested_fns {
            let nested_key = FnKey {
                file: self.file.clone(),
                name: format!("{name}::{nested_name}"),
            };
            self.functions.push((nested_key, nested_facts));
        }
        self.functions.push((
            FnKey {
                file: self.file.clone(),
                name,
            },
            visitor.facts,
        ));
    }
}

impl<'ast> Visit<'ast> for FileCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.record_fn(
            node.sig.ident.to_string(),
            &node.attrs,
            &node.sig,
            &node.block,
        );
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.record_fn(
            node.sig.ident.to_string(),
            &node.attrs,
            &node.sig,
            &node.block,
        );
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn classify_sources(sources: &[(String, String)]) -> BTreeMap<FnKey, AssertionStrength> {
    let mut all_fns: BTreeMap<FnKey, FnFacts> = BTreeMap::new();
    let mut file_fns: BTreeMap<String, BTreeMap<String, Vec<FnKey>>> = BTreeMap::new();
    let mut subdir_fns: BTreeMap<String, BTreeMap<String, Vec<FnKey>>> = BTreeMap::new();
    let mut file_stem_map: BTreeMap<(String, String), String> = BTreeMap::new();

    for (rel_path, content) in sources {
        let syntax = syn::parse_file(content)
            .unwrap_or_else(|err| panic!("failed to parse {rel_path}: {err}"));
        let mut collector = FileCollector {
            file: rel_path.clone(),
            functions: Vec::new(),
        };
        visit::visit_file(&mut collector, &syntax);
        let parent = Path::new(rel_path)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if let Some(stem) = Path::new(rel_path).file_stem().and_then(|s| s.to_str()) {
            file_stem_map.insert((parent.clone(), stem.to_owned()), rel_path.clone());
        }
        for (key, facts) in collector.functions {
            let short_name = key
                .name
                .rsplit("::")
                .next()
                .unwrap_or(key.name.as_str())
                .to_owned();
            file_fns
                .entry(key.file.clone())
                .or_default()
                .entry(short_name.clone())
                .or_default()
                .push(key.clone());
            if parent != "tests" && !parent.is_empty() {
                subdir_fns
                    .entry(parent.clone())
                    .or_default()
                    .entry(short_name)
                    .or_default()
                    .push(key.clone());
            }
            all_fns.insert(key, facts);
        }
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    struct Eff {
        direct_cell: bool,
        direct_face: bool,
        returns_raw_snapshot: bool,
        asserts_raw_snapshot: bool,
        has_any_assertion: bool,
    }

    let mut eff: BTreeMap<FnKey, Eff> = all_fns
        .iter()
        .map(|(key, facts)| {
            // A function that uses `RawTerminalSnapshot` AND directly asserts
            // inside itself counts as asserting on raw terminal cells.
            // A helper that constructs `RawTerminalSnapshot` and returns a
            // non-unit value (like `fn painted_ansi_rows(...) -> String`) marks
            // `returns_raw_snapshot`, which upgrades to `asserts_raw_snapshot`
            // ONLY when called by a function that actually asserts.
            // An artifact-only helper that writes `raw.ansi_grid()` to disk
            // without returning it or asserting on it stays `false` for both!
            let asserts_raw = facts.uses_raw_snapshot && facts.has_any_assertion;
            let returns_raw = facts.uses_raw_snapshot && facts.has_non_unit_return;
            (
                key.clone(),
                Eff {
                    direct_cell: facts.direct_cell,
                    direct_face: facts.direct_face,
                    returns_raw_snapshot: returns_raw,
                    asserts_raw_snapshot: asserts_raw,
                    has_any_assertion: facts.has_any_assertion,
                },
            )
        })
        .collect();

    let resolve_targets = |caller: &FnKey, facts: &FnFacts| -> Vec<FnKey> {
        let mut targets = Vec::new();
        let parent = Path::new(&caller.file)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        // 1. Qualified module calls `submod::fn_name(...)` resolve specifically
        //    to `<parent>/<submod>.rs :: fn_name`.
        let mut qualified_names = BTreeSet::new();
        for (module, name) in &facts.qualified_calls {
            if let Some(target_file) = file_stem_map.get(&(parent.clone(), module.clone())) {
                if let Some(keys) = file_fns.get(target_file).and_then(|m| m.get(name)) {
                    targets.extend(keys.iter().cloned());
                    qualified_names.insert(name.clone());
                }
            }
        }
        // 2. Unqualified calls resolve first to nested/same-file functions,
        //    then (only for subdirectory modules like `package_tui/<pkg>/`) to
        //    same-directory helpers.
        for name in &facts.unqualified_calls {
            if qualified_names.contains(name) {
                continue;
            }
            let nested_key = FnKey {
                file: caller.file.clone(),
                name: format!("{}::{name}", caller.name),
            };
            if all_fns.contains_key(&nested_key) {
                targets.push(nested_key);
                continue;
            }
            if let Some(keys) = file_fns.get(&caller.file).and_then(|m| m.get(name)) {
                let non_nested: Vec<_> = keys
                    .iter()
                    .filter(|k| !k.name.contains("::"))
                    .cloned()
                    .collect();
                if !non_nested.is_empty() {
                    targets.extend(non_nested);
                    continue;
                }
            }
            if parent != "tests" && !parent.is_empty() {
                if let Some(keys) = subdir_fns.get(&parent).and_then(|m| m.get(name)) {
                    targets.extend(keys.iter().filter(|k| !k.name.contains("::")).cloned());
                }
            }
        }
        targets
    };

    let call_graph: BTreeMap<FnKey, Vec<FnKey>> = all_fns
        .iter()
        .map(|(key, facts)| (key.clone(), resolve_targets(key, facts)))
        .collect();

    loop {
        let mut changed = false;
        for (key, targets) in &call_graph {
            let mut next = eff[key];
            for target in targets {
                if target == key {
                    continue;
                }
                let callee = eff[target];
                next.direct_cell |= callee.direct_cell;
                next.direct_face |= callee.direct_face;
                next.asserts_raw_snapshot |= callee.asserts_raw_snapshot;
                next.returns_raw_snapshot |= callee.returns_raw_snapshot;
                if callee.returns_raw_snapshot && next.has_any_assertion {
                    next.asserts_raw_snapshot = true;
                }
            }
            if next != eff[key] {
                eff.insert(key.clone(), next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    all_fns
        .iter()
        .filter(|(_, facts)| facts.is_test)
        .map(|(key, _)| {
            let e = eff[key];
            let strength = if e.direct_cell || e.asserts_raw_snapshot {
                AssertionStrength::TextAndCells
            } else if e.direct_face {
                AssertionStrength::TextAndFace
            } else {
                AssertionStrength::TextOnly
            };
            (key.clone(), strength)
        })
        .collect()
}

fn validate_against_allowlist_and_pin(
    classified: &BTreeMap<FnKey, AssertionStrength>,
    allowlist: &[NonCellAllowance],
    pin: CellAssertionPin,
) -> Result<(), Vec<String>> {
    let mut violations = Vec::new();
    let mut allow_map: BTreeMap<FnKey, NonCellAllowance> = BTreeMap::new();
    let mut blocked_count = 0usize;

    for entry in allowlist {
        assert!(
            !entry.reason.trim().is_empty(),
            "allowlist entry {}::{} must carry a non-empty reason",
            entry.file,
            entry.test
        );
        if entry.kind == NonCellAssertionKind::BlockedChromeDivergence {
            blocked_count += 1;
        }
        let key = FnKey {
            file: entry.file.to_owned(),
            name: entry.test.to_owned(),
        };
        if allow_map.insert(key.clone(), *entry).is_some() {
            violations.push(format!(
                "duplicate NON_CELL_ALLOWLIST entry: {}::{}",
                key.file, key.name
            ));
        }
    }

    let mut actual_cells = 0usize;
    let mut actual_face = 0usize;
    let mut actual_text = 0usize;

    for (key, &strength) in classified {
        match strength {
            AssertionStrength::TextAndCells => {
                actual_cells += 1;
                if allow_map.contains_key(key) {
                    violations.push(format!(
                        "stale NON_CELL_ALLOWLIST entry (row now asserts TextAndCells): {}::{}",
                        key.file, key.name
                    ));
                }
            }
            AssertionStrength::TextAndFace => {
                actual_face += 1;
                match allow_map.get(key) {
                    Some(entry) if entry.kind.expected_strength() == strength => {}
                    Some(entry) => violations.push(format!(
                        "mismatched kind for {}::{}: classified TextAndFace but allowlisted as {:?}",
                        key.file, key.name, entry.kind
                    )),
                    None => violations.push(format!(
                        "unclassified TextAndFace test (upgrade to TextAndCells or classify in NON_CELL_ALLOWLIST): {}::{}",
                        key.file, key.name
                    )),
                }
            }
            AssertionStrength::TextOnly => {
                actual_text += 1;
                match allow_map.get(key) {
                    Some(entry) if entry.kind.expected_strength() == strength => {}
                    Some(entry) => violations.push(format!(
                        "mismatched kind for {}::{}: classified TextOnly but allowlisted as {:?}",
                        key.file, key.name, entry.kind
                    )),
                    None => violations.push(format!(
                        "unclassified TextOnly test (upgrade to TextAndCells or classify in NON_CELL_ALLOWLIST): {}::{}",
                        key.file, key.name
                    )),
                }
            }
        }
    }

    for key in allow_map.keys() {
        if !classified.contains_key(key) {
            violations.push(format!(
                "stale NON_CELL_ALLOWLIST entry (test no longer exists): {}::{}",
                key.file, key.name
            ));
        }
    }

    let actual_total = classified.len();
    if actual_total < pin.min_total {
        violations.push(format!(
            "CELL_ASSERTION_PINS total floor regressed: actual total={actual_total} < min_total={}",
            pin.min_total
        ));
    }
    if actual_cells < pin.min_text_and_cells {
        violations.push(format!(
            "CELL_ASSERTION_PINS text_and_cells floor regressed: actual text_and_cells={actual_cells} < min_text_and_cells={}",
            pin.min_text_and_cells
        ));
    }
    if actual_face != pin.text_and_face
        || actual_text != pin.text_only
        || blocked_count != pin.blocked_chrome_divergence
    {
        violations.push(format!(
            "CELL_ASSERTION_PINS exception mismatch: actual=(text_and_face={actual_face}, text_only={actual_text}, blocked_chrome_divergence={blocked_count}) != pinned=(text_and_face={}, text_only={}, blocked_chrome_divergence={})",
            pin.text_and_face, pin.text_only, pin.blocked_chrome_divergence
        ));
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn crate_cell_assertion_pin() -> CellAssertionPin {
    CELL_ASSERTION_PINS
        .iter()
        .find(|(crate_name, _)| *crate_name == env!("CARGO_PKG_NAME"))
        .map(|(_, pin)| *pin)
        .expect("current crate must have a CELL_ASSERTION_PINS entry")
}

#[test]
fn cell_assertion_ratchet_matches_pinned_inventory() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = manifest_dir.join("tests");
    let mut files = Vec::new();
    collect_rs_files(&tests_dir, &mut files);
    assert!(
        !files.is_empty(),
        "expected integration test files under {}",
        tests_dir.display()
    );
    let sources: Vec<(String, String)> = files
        .into_iter()
        .map(|path| {
            let rel = path
                .strip_prefix(&manifest_dir)
                .expect("test path under manifest dir")
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("failed to read {rel}: {err}"));
            (rel, content)
        })
        .collect();

    let classified = classify_sources(&sources);
    if let Err(violations) = validate_against_allowlist_and_pin(
        &classified,
        NON_CELL_ALLOWLIST,
        crate_cell_assertion_pin(),
    ) {
        panic!(
            "TUI cell-assertion ratchet violated ({} issue(s)):\n  {}",
            violations.len(),
            violations.join("\n  ")
        );
    }
}

#[test]
fn cell_assertion_guard_rejects_unclassified_text_only_and_artifact_only_raw_snapshot() {
    let synthetic = vec![(
        "tests/synthetic.rs".to_owned(),
        r#"
#[test]
fn strong_exact_display_row() {
    assert_pair_exact_display("ok", &gnu, &neo);
}

#[test]
fn strong_raw_snapshot_row() {
    let gnu_snap = RawTerminalSnapshot::capture_rows(gnu.screen(), 0..1);
    let neo_snap = RawTerminalSnapshot::capture_rows(neo.screen(), 0..1);
    assert_raw_terminal_snapshots_eq("ok", &gnu_snap, &neo_snap);
}

#[test]
fn partial_face_only_row() {
    assert_ne!(gnu.screen().cell(0, 0).unwrap().bgcolor(), vt100::Color::Default);
}

#[test]
fn artifact_only_raw_snapshot_is_not_cell_assertion() {
    fn dump_artifact(session: &TuiSession) {
        let raw = RawTerminalSnapshot::capture_full_screen(session.screen());
        std::fs::write("debug.ansi", raw.ansi_grid()).unwrap();
    }
    dump_artifact(&gnu);
    assert_eq!(gnu.text_grid(), neo.text_grid());
}

#[test]
fn plain_text_only_row() {
    assert!(gnu.text_grid()[0].contains("hello"));
}
"#
        .to_owned(),
    )];

    let classified = classify_sources(&synthetic);
    let get = |name: &str| {
        classified[&FnKey {
            file: "tests/synthetic.rs".to_owned(),
            name: name.to_owned(),
        }]
    };
    assert_eq!(
        get("strong_exact_display_row"),
        AssertionStrength::TextAndCells
    );
    assert_eq!(
        get("strong_raw_snapshot_row"),
        AssertionStrength::TextAndCells
    );
    assert_eq!(get("partial_face_only_row"), AssertionStrength::TextAndFace);
    assert_eq!(
        get("artifact_only_raw_snapshot_is_not_cell_assertion"),
        AssertionStrength::TextOnly
    );
    assert_eq!(get("plain_text_only_row"), AssertionStrength::TextOnly);

    let pin = CellAssertionPin {
        min_total: 5,
        min_text_and_cells: 2,
        text_and_face: 1,
        text_only: 2,
        blocked_chrome_divergence: 0,
    };
    // Unclassified non-cell rows must be rejected.
    let err = validate_against_allowlist_and_pin(&classified, &[], pin)
        .expect_err("empty allowlist must fail on non-cell rows");
    assert!(
        err.iter()
            .any(|msg| msg.contains("artifact_only_raw_snapshot_is_not_cell_assertion")),
        "expected artifact-only snapshot row to be flagged: {err:?}"
    );
    assert!(
        err.iter().any(|msg| msg.contains("plain_text_only_row")),
        "expected text-only row to be flagged: {err:?}"
    );

    // Properly classified allowlist + pin passes.
    let allowlist = [
        NonCellAllowance {
            file: "tests/synthetic.rs",
            test: "partial_face_only_row",
            kind: NonCellAssertionKind::PartialFaceOrSgr,
            reason: "partial bg check",
        },
        NonCellAllowance {
            file: "tests/synthetic.rs",
            test: "artifact_only_raw_snapshot_is_not_cell_assertion",
            kind: NonCellAssertionKind::BufferTextOrWorkflow,
            reason: "text only",
        },
        NonCellAllowance {
            file: "tests/synthetic.rs",
            test: "plain_text_only_row",
            kind: NonCellAssertionKind::BufferTextOrWorkflow,
            reason: "text only",
        },
    ];
    validate_against_allowlist_and_pin(&classified, &allowlist, pin)
        .expect("exact classification and pin must pass");

    // Adding a brand-new strong TextAndCells test above min_total / min_text_and_cells
    // must pass freely without requiring a pin bump.
    let mut with_extra_strong = classified.clone();
    with_extra_strong.insert(
        FnKey {
            file: "tests/synthetic.rs".to_owned(),
            name: "added_strong_row".to_owned(),
        },
        AssertionStrength::TextAndCells,
    );
    validate_against_allowlist_and_pin(&with_extra_strong, &allowlist, pin)
        .expect("adding a new TextAndCells row must pass without bumping floors");

    // Losing a TextAndCells test below min_text_and_cells or dropping total
    // below min_total must fail.
    let mut missing_strong = classified.clone();
    missing_strong.remove(&FnKey {
        file: "tests/synthetic.rs".to_owned(),
        name: "strong_raw_snapshot_row".to_owned(),
    });
    let floor_err = validate_against_allowlist_and_pin(&missing_strong, &allowlist, pin)
        .expect_err("dropping a TextAndCells row below the floor must fail");
    assert!(
        floor_err
            .iter()
            .any(|msg| msg.contains("total floor regressed")),
        "expected total floor diagnostic: {floor_err:?}"
    );
    assert!(
        floor_err
            .iter()
            .any(|msg| msg.contains("text_and_cells floor regressed")),
        "expected text_and_cells floor diagnostic: {floor_err:?}"
    );

    // Stale allowlist entry for a TextAndCells row must be rejected.
    let mut stale = allowlist.to_vec();
    stale.push(NonCellAllowance {
        file: "tests/synthetic.rs",
        test: "strong_exact_display_row",
        kind: NonCellAssertionKind::BufferTextOrWorkflow,
        reason: "stale",
    });
    let stale_err = validate_against_allowlist_and_pin(&classified, &stale, pin)
        .expect_err("stale allowlist entry must fail");
    assert!(
        stale_err
            .iter()
            .any(|msg| msg.contains("stale NON_CELL_ALLOWLIST")),
        "expected stale allowlist diagnostic: {stale_err:?}"
    );

    // Exception pin mismatch must be rejected in either direction even if allowlist matches.
    for bad_pin in [
        CellAssertionPin {
            text_only: 1,
            ..pin
        },
        CellAssertionPin {
            text_only: 3,
            ..pin
        },
        CellAssertionPin {
            text_and_face: 0,
            ..pin
        },
    ] {
        let pin_err = validate_against_allowlist_and_pin(&classified, &allowlist, bad_pin)
            .expect_err("mismatched exception pin must fail");
        assert!(
            pin_err
                .iter()
                .any(|msg| msg.contains("CELL_ASSERTION_PINS exception mismatch")),
            "expected exception pin mismatch diagnostic: {pin_err:?}"
        );
    }
}
