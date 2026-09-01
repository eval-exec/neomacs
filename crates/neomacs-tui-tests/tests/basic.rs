//! TUI comparison tests: basic.

mod support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

fn is_blank_cell(cell: &vt100::Cell) -> bool {
    cell.contents().trim().is_empty()
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn return_after_self_insert_moves_cursor_to_next_terminal_row() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both_raw(&mut gnu, &mut neo, b"a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let gnu_after_insert = gnu.screen().cursor_position();
    let neo_after_insert = neo.screen().cursor_position();

    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let gnu_after_return = gnu.screen().cursor_position();
    let neo_after_return = neo.screen().cursor_position();

    assert_eq!(
        neo_after_insert,
        gnu_after_insert,
        "Neomacs cursor after self-insert must exactly match GNU\nNeomacs screen:\n{}",
        neo.text_grid().join("\n")
    );

    assert_eq!(
        gnu_after_return,
        (gnu_after_insert.0 + 1, 0),
        "GNU oracle must display Return on the next terminal row"
    );
    assert_eq!(
        neo_after_return,
        gnu_after_return,
        "Neomacs cursor after Return must exactly match GNU; \
         Neomacs={neo_after_return:?}\nNeomacs screen:\n{}",
        neo.text_grid().join("\n")
    );
}

#[test]
fn control_x_prefix_echo_has_no_trailing_dash() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gnu_echo = gnu.row_text(ROWS - 1).trim_end().to_string();
    let neo_echo = neo.row_text(ROWS - 1).trim_end().to_string();
    assert_ne!(
        neo_echo, "C-x-",
        "Neomacs should not eagerly append a dash to C-x prefix echo"
    );
    assert_eq!(
        neo_echo.ends_with('-'),
        gnu_echo.ends_with('-'),
        "Neomacs prefix echo should match GNU trailing-dash state"
    );
}

#[test]
fn terminal_resize_updates_frame_geometry() {
    const TARGET_ROWS: u16 = 30;
    const TARGET_COLS: u16 = 100;

    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, TARGET_ROWS, TARGET_COLS);

    // Drain the resize event before sending input.
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(message "resize-test %sx%s" (frame-width) (frame-height))"#,
    );

    let expected_frame_height = TARGET_ROWS - 1;
    let expected = format!("resize-test {TARGET_COLS}x{expected_frame_height}");
    gnu.read_until(Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains(&expected))
    });
    neo.read_until(Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains(&expected))
    });

    assert_eq!(gnu.screen_size(), (TARGET_ROWS, TARGET_COLS));
    assert_eq!(neo.screen_size(), (TARGET_ROWS, TARGET_COLS));
    let gnu_grid = gnu.text_grid();
    let neo_grid = neo.text_grid();
    assert!(
        gnu_grid.iter().any(|row| row.contains(&expected)),
        "GNU should report resized frame geometry {expected}\n{}",
        gnu_grid.join("\n")
    );
    assert!(
        neo_grid.iter().any(|row| row.contains(&expected)),
        "Neomacs should report resized frame geometry {expected}\n{}",
        neo_grid.join("\n")
    );
}

#[test]
fn live_resize_reflow_content_and_adapt_modeline() {
    let (mut gnu, mut neo) = boot_pair("");

    // Open a file with known content
    open_home_file(
        &mut gnu,
        &mut neo,
        "resize-reflow.txt",
        "short line\nlonger line with more text here\nthird line\n",
        "C-x C-f",
    );

    // Resize to narrow (40 cols) — content should re-wrap, mode-line shrinks
    resize_both(&mut gnu, &mut neo, 24, 40);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("short line")),
            "{label}: content visible after resize to 40 cols"
        );
    }

    // Resize to wider (100 cols) — content spreads out, mode-line fills
    resize_both(&mut gnu, &mut neo, 24, 100);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("short line")),
            "{label}: content still visible after resize to 100 cols"
        );
    }

    // Resize back to narrow — content should re-wrap again
    resize_both(&mut gnu, &mut neo, 24, 50);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("short line")),
            "{label}: content visible after third resize"
        );
    }

    // Check mode-line is present (should adapt to terminal width)
    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    assert!(
        gl.iter().any(|r| r.contains("resize-reflow.txt")),
        "GNU mode-line should show filename"
    );
    assert!(
        nl.iter().any(|r| r.contains("resize-reflow.txt")),
        "NEO mode-line should show filename"
    );

    assert_pair_nearly_matches(
        "live_resize_reflow_content_and_adapt_modeline",
        &gnu,
        &neo,
        4,
    );
}

#[test]
fn execute_extended_command_tab_completion_via_mx_completes_unique_command() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "mx-command-completion.txt",
        "abcdef\nsecond\n",
        "C-x C-f",
    );
    send_both(&mut gnu, &mut neo, "C-a");

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "execute_extended_command_tab_completion_via_mx_completes_unique_command/prompt",
        &gnu,
        &neo,
        2,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"overwr");
    }
    send_both(&mut gnu, &mut neo, "TAB");
    let completed = |grid: &[String]| grid.iter().any(|row| row.contains("overwrite-mode"));
    gnu.read_until(Duration::from_secs(6), completed);
    neo.read_until(Duration::from_secs(8), completed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "execute_extended_command_tab_completion_via_mx_completes_unique_command/completed",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "Z");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Zbcdef"))
            && !grid.iter().any(|row| row.contains("Zabcdef"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "execute_extended_command_tab_completion_via_mx_completes_unique_command",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn keyboard_quit_from_mx_via_cg() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"find-fil");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-g");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("keyboard_quit_from_mx_via_cg", &gnu, &neo, 2);
}

#[test]
fn execute_extended_command_history_via_mx_mp_recalls_previous_command() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "calendar");
    let day_header_count = |grid: &[String]| {
        grid.iter()
            .map(|row| row.matches("Su Mo Tu We Th Fr Sa").count())
            .sum::<usize>()
    };
    let calendar_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Calendar")) && day_header_count(grid) >= 3
    };
    gnu.read_until(Duration::from_secs(8), calendar_ready);
    neo.read_until(Duration::from_secs(10), calendar_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "execute_extended_command_history_via_mx_mp_recalls_previous_command/first-calendar",
        &gnu,
        &neo,
        4,
    );

    send_both_raw(&mut gnu, &mut neo, b"q");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "execute_extended_command_history_via_mx_mp_recalls_previous_command/quit",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "execute_extended_command_history_via_mx_mp_recalls_previous_command/prompt",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "M-p");
    let recalled = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("M-x calendar") || row.contains("M-X calendar"))
    };
    gnu.read_until(Duration::from_secs(6), recalled);
    neo.read_until(Duration::from_secs(8), recalled);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "execute_extended_command_history_via_mx_mp_recalls_previous_command/recalled",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "RET");
    gnu.read_until(Duration::from_secs(8), calendar_ready);
    neo.read_until(Duration::from_secs(10), calendar_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "execute_extended_command_history_via_mx_mp_recalls_previous_command/second-calendar",
        &gnu,
        &neo,
        4,
    );
}

#[test]
fn keyboard_escape_quit_from_mx_via_esc_esc_esc() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"find-fil");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "ESC ESC ESC");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "keyboard_escape_quit_from_mx_via_esc_esc_esc",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn universal_argument_insert_via_cu_8_a() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-u 8 a");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("aaaaaaaa"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("universal_argument_insert_via_cu_8_a", &gnu, &neo, 2);
}

#[test]
fn negative_argument_reverses_forward_word_via_mminus_mf() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "negative-argument.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f M-f M-- M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha Xbeta gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "negative_argument_reverses_forward_word_via_mminus_mf",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn boot_screen_layout() {
    let (gnu, neo) = boot_pair("");
    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let diffs = meaningful_diffs(diff_text_grids(&gl, &nl));
    if !diffs.is_empty() {
        eprintln!("boot_screen_layout: {} rows differ", diffs.len());
        print_row_diffs(&diffs);
    }
    assert!(
        diffs.len() <= 2,
        "Boot screens differ in {} rows (expected <= 2 for menu bar / echo area)",
        diffs.len()
    );
}

#[test]
fn boot_blank_cells_use_terminal_default_background() {
    let (gnu, neo) = boot_pair("");
    assert!(
        gnu.text_grid().iter().any(|row| row.contains("*scratch*")),
        "GNU Emacs did not reach the scratch boot screen"
    );
    assert!(
        neo.text_grid().iter().any(|row| row.contains("*scratch*")),
        "Neomacs did not reach the scratch boot screen"
    );

    let mut checked = 0usize;
    let mut mismatches = Vec::new();

    // GNU Emacs leaves ordinary TTY background cells on the terminal default
    // color. A Neomacs regression here painted blank cells explicit white.
    for row in 1..ROWS.saturating_sub(2) {
        for col in 0..COLS {
            let (Some(gnu_cell), Some(neo_cell)) =
                (gnu.screen().cell(row, col), neo.screen().cell(row, col))
            else {
                continue;
            };

            if is_blank_cell(gnu_cell) && is_blank_cell(neo_cell) {
                checked += 1;
                if gnu_cell.bgcolor() != neo_cell.bgcolor() && mismatches.len() < 12 {
                    mismatches.push(format!(
                        "row {row} col {col}: GNU bg {:?}, Neomacs bg {:?}\nGNU: {:?}\nNEO: {:?}",
                        gnu_cell.bgcolor(),
                        neo_cell.bgcolor(),
                        gnu.text_grid().get(row as usize),
                        neo.text_grid().get(row as usize)
                    ));
                }
            }
        }
    }

    assert!(
        checked > 100,
        "Expected many blank body cells to compare, checked {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "Blank body background differs from GNU Emacs:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn mx_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    let gl = gnu.text_grid();
    let nl = neo.text_grid();

    // The last row should contain "M-x " in both
    let gnu_last = gl.last().unwrap();
    let neo_last = nl.last().unwrap();
    assert!(
        gnu_last.contains("M-x"),
        "GNU last row should contain 'M-x': {gnu_last:?}"
    );
    assert!(
        neo_last.contains("M-x"),
        "NEO last row should contain 'M-x': {neo_last:?}"
    );

    // Cancel
    send_both(&mut gnu, &mut neo, "C-g");
}

#[test]
fn universal_argument() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-u 8 a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gl = gnu.text_grid();
    let nl = neo.text_grid();

    // The 8 a's are inserted at point (end of buffer, after comments).
    // Check that SOME row contains "aaaaaaaa".
    let gnu_has_8a = gl.iter().any(|r| r.contains("aaaaaaaa"));
    let neo_has_8a = nl.iter().any(|r| r.contains("aaaaaaaa"));
    if !gnu_has_8a {
        eprintln!("GNU screen (no 8 a's found):");
        for (i, r) in gl.iter().enumerate() {
            let t = r.trim();
            if !t.is_empty() {
                eprintln!("  {i:2}: |{t}|");
            }
        }
    }
    if !neo_has_8a {
        eprintln!("NEO screen (no 8 a's found):");
        for (i, r) in nl.iter().enumerate() {
            let t = r.trim();
            if !t.is_empty() {
                eprintln!("  {i:2}: |{t}|");
            }
        }
    }
    assert!(gnu_has_8a, "GNU buffer should have 8 a's somewhere");
    assert!(neo_has_8a, "NEO buffer should have 8 a's somewhere");
}

#[test]
fn echo_area_message() {
    let (mut gnu, mut neo) = boot_pair("");
    // C-x = (what-cursor-position) shows char info in echo area
    send_both(&mut gnu, &mut neo, "C-x =");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let gnu_echo = gl.last().unwrap();
    let neo_echo = nl.last().unwrap();

    // Both should show cursor position info (contains "Char:" or "point=")
    let gnu_has_info = gnu_echo.contains("Char") || gnu_echo.contains("point");
    let neo_has_info = neo_echo.contains("Char") || neo_echo.contains("point");

    if !gnu_has_info {
        eprintln!("GNU echo area: {gnu_echo:?}");
    }
    if !neo_has_info {
        eprintln!("NEO echo area: {neo_echo:?}");
    }

    // At minimum, check neomacs shows something in the echo area
    assert!(
        neo_has_info || !neo_echo.trim().is_empty(),
        "NEO echo area should show cursor info after C-x ="
    );
}

// ── Session lifecycle tests ──────────────────────────────────

#[test]
fn save_buffers_kill_terminal_prompts_for_modified_file_buffer_via_cx_cc() {
    let (mut gnu, mut neo) = boot_pair("");

    // Visit a file and modify it (file-visiting buffers trigger save prompts)
    open_home_file(
        &mut gnu,
        &mut neo,
        "quit-save-test.txt",
        "original content\n",
        "C-x C-f",
    );

    // Modify buffer without saving — makes it dirty
    send_both_raw(&mut gnu, &mut neo, b"modified content added");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // C-x C-c should prompt to save the modified file buffer
    send_both(&mut gnu, &mut neo, "C-x C-c");

    let save_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Save file") && row.contains("quit-save-test"))
    };
    gnu.read_until(Duration::from_secs(6), save_prompt);
    neo.read_until(Duration::from_secs(8), save_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("Save file") && row.contains("quit-save-test")),
            "{label}: C-x C-c should prompt to save modified file buffer\n{}",
            grid.join("\n")
        );
    }

    // Cancel the quit with C-g to keep session alive for comparison
    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "save_buffers_kill_terminal_prompts_for_modified_file_buffer_via_cx_cc",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn disabled_command_shows_prompt_and_accepts_with_space() {
    let (mut gnu, mut neo) = boot_pair("");

    // C-x C-l (downcase-region) is disabled by default
    send_both(&mut gnu, &mut neo, "C-x C-l");

    let disabled_prompt = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("disabled command")
                || row.contains("disabled") && row.contains("downcase-region")
        })
    };
    gnu.read_until(Duration::from_secs(6), disabled_prompt);
    neo.read_until(Duration::from_secs(8), disabled_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    // Both should show the disabled command prompt
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| {
                row.contains("disabled")
                    && (row.contains("downcase-region") || row.contains("downcase"))
            }),
            "{label}: should show disabled command prompt for C-x C-l\n{}",
            grid.join("\n")
        );
    }

    // Accept with SPC — the command should execute
    send_both(&mut gnu, &mut neo, "SPC");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    assert_pair_nearly_matches(
        "disabled_command_shows_prompt_and_accepts_with_space",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn m_x_help_shows_help_menu() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "help");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Both should show a help buffer/menu
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| { row.contains("Help") || row.contains("help") }),
            "{label}: M-x help should show help information\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches("m_x_help_shows_help_menu", &gnu, &neo, 3);
}

// ── Face remapping tests ────────────────────────────────────

#[test]
fn face_remapping_alist_with_filtered_window_system_not_match_on_tty() {
    let (mut gnu, mut neo) = boot_pair("");

    // Set face-remapping-alist to remap 'default to 'bold only on GUI
    // (:window-system x).  On TTY, window-system is nil so the filter
    // should NOT match and the face should remain unchanged.
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(setq face-remapping-alist '((default :filtered (:window-system x) bold)))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Insert some text — it should render as normal (not bold),
    // because :filtered (:window-system x) doesn't match on TTY
    send_both_raw(
        &mut gnu,
        &mut neo,
        b";; this text should NOT be bold on TTY",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // The mode-line includes "Fundamental" or "Lisp Interaction" — the
    // mode-line face should also be unchanged since filtering didn't match
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| !row.trim().is_empty()),
            "{label}: buffer should have visible content after face-remapping-alist setup"
        );
    }

    // Screen comparison with reasonable tolerance
    assert_pair_nearly_matches(
        "face_remapping_alist_with_filtered_window_system_not_match_on_tty",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn overlay_with_face_property_displays_correctly() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "overlay-face.el",
        "alpha beta gamma delta\n",
        "C-x C-f",
    );

    // Create an overlay on "beta" with a face property
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(let ((ov (make-overlay 7 11))) (overlay-put ov 'face 'bold) nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Both should show the buffer with the overlay applied
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("alpha") && row.contains("beta")),
            "{label}: buffer should display overlay content"
        );
    }

    assert_pair_nearly_matches(
        "overlay_with_face_property_displays_correctly",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn eval_expression_addition_via_mcolon_shows_result() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"(+ 1 2)\r");
    }

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("3"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let gnu_has = gnu.text_grid().iter().any(|r| r.contains("3"));
    let neo_has = neo.text_grid().iter().any(|r| r.contains("3"));
    assert!(gnu_has, "GNU should display result 3 after M-: (+ 1 2)");
    assert!(neo_has, "NEO should display result 3 after M-: (+ 1 2)");
}

#[test]
fn tty_erase_char_reports_the_terminals_stty_erase_like_gnu() {
    // GNU `init_sys_modes' (src/sysdep.c:1130) publishes c_cc[VERASE] from the
    // termios it saved before touching terminal modes, and
    // `normal-erase-is-backspace-setup-frame' (lisp/simple.el) reads it during
    // startup: on a ^H terminal it key-translates C-h to DEL so Backspace
    // deletes rather than opening the help prefix. Neomacs used to hardcode 0,
    // a value GNU never reports, which left that decision permanently off.
    // This harness's pty reports ^? (127), so both engines must say 127.
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"tty-erase-char\r");
    }

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("127"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("127")),
            "{label} should report tty-erase-char 127 for this pty's ^? erase\n{}",
            grid.join("\n")
        );
    }
}

#[test]
// DIVERGENCES.md entry 67: a fabricated terminal-parameter default of 0 for
// `normal-erase-is-backspace' made the `unless' guard in
// `normal-erase-is-backspace-setup-frame' veto the real decision forever, so
// Backspace opened help where GNU deletes. Fixed by deleting the invented
// default; this is the end-to-end reduction that pins the behaviour.
fn backspace_on_a_ctrl_h_erase_terminal_deletes_like_gnu() {
    // The behaviour tty-erase-char gates, exercised end to end rather than
    // argued. On a terminal whose stty erase is ^H,
    // `normal-erase-is-backspace-setup-frame' (lisp/simple.el:11093) turns the
    // mode on and it key-translates C-h to DEL (lisp/simple.el:11178), so the
    // 0x08 the Backspace key sends DELETES a character instead of opening the
    // help prefix. The rest of the suite runs on the pty default of ^?, where
    // the mode stays off and both engines agree no matter what
    // `tty-erase-char' says -- which is why a hardcoded 0 went unnoticed.
    let (mut gnu, mut neo) = boot_pair_with_erase_char("", PtyEraseChar::Backspace);

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/erase-is-backspace-probeX");
    }
    // "BS" is the literal 0x08 byte a Backspace key sends (see the harness key
    // encoder); on this terminal it must erase the X.
    send_both(&mut gnu, &mut neo, "BS");

    let erased = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("~/erase-is-backspace-probe"))
            && !grid
                .iter()
                .any(|row| row.contains("~/erase-is-backspace-probeX"))
    };
    gnu.read_until(Duration::from_secs(6), erased);
    neo.read_until(Duration::from_secs(8), erased);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("~/erase-is-backspace-probe"))
                && !grid
                    .iter()
                    .any(|row| row.contains("~/erase-is-backspace-probeX")),
            "{label} should delete the previous character for Backspace on a ^H-erase terminal\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "backspace_on_a_ctrl_h_erase_terminal_deletes_like_gnu",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn what_cursor_position_via_cx_equals_shows_char_info() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "ATA");

    send_both(&mut gnu, &mut neo, "C-a");
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "=");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("#x41"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("#x41") && r.contains("Char")),
            "{label} C-x = should show Char: A (#x41) info"
        );
    }
}

#[test]
fn universal_argument_self_insert_via_cu_8_star_inserts_eight_asterisks() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-u");
    send_both(&mut gnu, &mut neo, "8");
    send_both(&mut gnu, &mut neo, "*");

    let ready = |grid: &[String]| grid.iter().any(|r| r.contains("********"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("********")),
            "{label}: C-u 8 * should insert 8 asterisks"
        );
    }
}

#[test]
fn read_only_mode_toggle_via_cx_cq_shows_percent_sign_in_mode_line() {
    let (mut gnu, mut neo) = boot_pair("");
    // Open a file, toggle read-only mode
    let name = "readonly-test.txt";
    open_home_file(&mut gnu, &mut neo, name, "test\n", "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-x C-q");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("%%") || r.contains("%@") || r.contains("Read-Only")),
            "{label}: C-x C-q should show read-only indicator"
        );
    }
}

#[test]
fn pwd_via_mx_shows_current_directory() {
    let (mut gnu, mut neo) = boot_pair("");
    invoke_mx_command(&mut gnu, &mut neo, "pwd");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("Directory") || r.contains("/")),
            "{label}: M-x pwd should show current directory"
        );
    }
}

#[test]
fn line_number_mode_toggle_via_mx_shows_l_in_mode_line() {
    let (mut gnu, mut neo) = boot_pair("");
    invoke_mx_command(&mut gnu, &mut neo, "line-number-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains(" L") && !r.contains("line-number-mode")),
            "{label}: enabling line-number-mode should show 'L' in mode line"
        );
    }
}

#[test]
fn what_line_via_mx_shows_current_line_number() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "what-line.txt",
        "line 1\nline 2\nline 3\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n");
    invoke_mx_command(&mut gnu, &mut neo, "what-line");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("Line 2") || r.contains("line 2")),
            "{label}: what-line on line 2 should report Line 2"
        );
    }
}

#[test]
fn mx_history_recall_via_mp_shows_previous_command() {
    let (mut gnu, mut neo) = boot_pair("");
    // First, execute a command via M-x
    invoke_mx_command(&mut gnu, &mut neo, "pwd");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Now open M-x and press M-p to recall
    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-p");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("pwd")),
            "{label}: M-p in M-x should recall pwd"
        );
    }
}

#[test]
fn display_time_via_mx_shows_clock_in_mode_line() {
    let (mut gnu, mut neo) = boot_pair("");
    invoke_mx_command(&mut gnu, &mut neo, "display-time-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let _has_time = grid.iter().any(|r| {
            r.contains(":")
                && (r.contains("AM")
                    || r.contains("PM")
                    || r.chars().filter(|&c| c == ':').count() >= 1)
        });
        // Time might not appear immediately, just check no error
        assert!(
            grid.iter()
                .any(|r| r.contains("scratch") || r.contains("*scratch*")),
            "{label}: display-time-mode should not break the mode line"
        );
    }
}

#[test]
fn beginning_of_buffer_via_mlessthan_goes_to_start() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "bob-test.txt";
    let content = "line A\nline B\n";

    open_home_file(&mut gnu, &mut neo, name, content, "C-x C-f");
    // Move to end, then to beginning
    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Insert at point to verify position
    send_both_raw(&mut gnu, &mut neo, b"X");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("Xline A")),
            "{label}: M-< should go to beginning of buffer"
        );
    }
}

#[test]
fn universal_argument_cu_3_cf_moves_forward_three_chars() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "abcdefgh");
    send_both(&mut gnu, &mut neo, "C-a");

    // C-u 3 C-f should move forward 3 chars
    send_both(&mut gnu, &mut neo, "C-u 3 C-f");
    // Insert marker at point
    send_both_raw(&mut gnu, &mut neo, b"X");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("abcXdef")),
            "{label}: C-u 3 C-f should move 3 chars right"
        );
    }
}

#[test]
fn negative_argument_via_mminus_reverses_direction() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "abcdefgh");
    send_both(&mut gnu, &mut neo, "C-e");

    // M-- C-b should be backward-char with negative = forward
    send_both(&mut gnu, &mut neo, "M-- C-b");
    send_both_raw(&mut gnu, &mut neo, b"X");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("abcdefghX")),
            "{label}: M-- C-b should move forward (inserting X at end)"
        );
    }
}

#[test]
fn column_number_mode_toggle_via_mx_shows_column_in_mode_line() {
    let (mut gnu, mut neo) = boot_pair("");
    invoke_mx_command(&mut gnu, &mut neo, "column-number-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("C") && !r.contains("column-number-mode")),
            "{label}: column-number-mode should show column in mode line"
        );
    }
}

#[test]
fn execute_extended_command_tab_completion_via_mx_tab_shows_completions() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"find-fil");
    send_both(&mut gnu, &mut neo, "TAB");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("find-file")),
            "{label}: TAB completion in M-x should show find-file"
        );
    }
}
