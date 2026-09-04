#![cfg(unix)]
//! TUI comparison tests: modes.

mod support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

fn boot_fido_vertical_pair() -> (TuiSession, TuiSession) {
    let init = TuiTempFile::new(
        "neomacs-common-usage-fido-vertical-",
        "init.el",
        ";;; -*- lexical-binding: t; -*-\n\
         (setq max-mini-window-height 8\n\
               resize-mini-windows t\n\
               icomplete-prospects-height 8)\n\
         (fido-vertical-mode 1)\n",
    );
    let extra_args = format!("-l {}", init.display());
    boot_pair(&extra_args)
}

fn fido_bottom_start() -> usize {
    (ROWS as usize).saturating_sub(8)
}

fn bottom_nonempty_rows(session: &TuiSession, first_row: usize) -> Vec<String> {
    bottom_nonempty_rows_from_grid(&session.text_grid(), first_row)
}

fn bottom_nonempty_rows_from_grid(grid: &[String], first_row: usize) -> Vec<String> {
    grid.iter()
        .skip(first_row)
        .map(|row| row.trim().to_string())
        .filter(|row| !row.is_empty())
        .collect()
}

fn assert_fido_prompt_matches_stable_behavior(label: &str, gnu: &TuiSession, neo: &TuiSession) {
    let first_row = fido_bottom_start();
    let gnu_rows = bottom_nonempty_rows(gnu, first_row);
    let neo_rows = bottom_nonempty_rows(neo, first_row);

    assert!(
        !gnu_rows.is_empty() && !neo_rows.is_empty(),
        "{label} should show non-empty minibuffer rows"
    );
    assert_eq!(
        gnu_rows[0], neo_rows[0],
        "{label} should show the same prompt header"
    );
    assert_eq!(
        gnu_rows.len(),
        neo_rows.len(),
        "{label} should use the same number of visible minibuffer rows"
    );

    let gnu_find_file = gnu_rows
        .iter()
        .find(|row| row.contains("find-file"))
        .cloned()
        .expect("GNU should show find-file");
    let neo_find_file = neo_rows
        .iter()
        .find(|row| row.contains("find-file"))
        .cloned()
        .expect("NEO should show find-file");
    assert_eq!(
        gnu_find_file, neo_find_file,
        "{label} should agree on the top find-file candidate"
    );

    // With `max-mini-window-height 8` only the top ~7 flex-sorted candidates
    // for "find-f" are visible; assert against the commands GNU actually
    // renders in that vertical view (lower-ranked matches like `ido-find-file`
    // never appear, in GNU either).
    for stable in [
        "find-file",
        "find-function",
        "find-file-existing",
        "find-file-at-point",
        "find-file-other-tab",
        "find-file-read-only",
    ] {
        assert!(
            gnu_rows.iter().any(|row| row.contains(stable)),
            "{label} GNU should show {stable}"
        );
        assert!(
            neo_rows.iter().any(|row| row.contains(stable)),
            "{label} NEO should show {stable}"
        );
    }
}

fn wait_for_fido_mx_candidates(gnu: &mut TuiSession, neo: &mut TuiSession, query: &str) {
    send_both(gnu, neo, "M-x");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(gnu, neo, Duration::from_millis(300));

    gnu.send(query.as_bytes());
    neo.send(query.as_bytes());
    let candidates_ready = |grid: &[String]| {
        let bottom_start = fido_bottom_start();
        grid.iter().any(|row| row.contains("find-file"))
            && grid[bottom_start..]
                .iter()
                .filter(|row| !row.trim().is_empty())
                .count()
                >= 3
    };
    gnu.read_until(Duration::from_secs(6), candidates_ready);
    neo.read_until(Duration::from_secs(8), candidates_ready);
    read_both(gnu, neo, Duration::from_secs(1));
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn overwrite_mode_via_mx_replaces_character_at_point() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "overwrite-usage.txt",
        "abcdef\nsecond\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-a");
    invoke_mx_command(&mut gnu, &mut neo, "overwrite-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "Z");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Zbcdef"))
            && !grid.iter().any(|row| row.contains("Zabcdef"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "overwrite_mode_via_mx_replaces_character_at_point",
        &gnu,
        &neo,
    );
}

#[test]
fn column_number_mode_via_mx_shows_line_and_column_in_mode_line() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "column-number-mode.txt",
        "abcde\nsecond line\n",
        "C-x C-f",
    );
    send_both(&mut gnu, &mut neo, "C-n C-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let line_only = |grid: &[String]| {
        grid.get(usize::from(ROWS - 2))
            .is_some_and(|row| row.contains(" L2 ") && !row.contains("(2,2)"))
    };
    gnu.read_until(Duration::from_secs(6), line_only);
    neo.read_until(Duration::from_secs(8), line_only);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "column-number-mode");
    let line_and_column = |grid: &[String]| {
        grid.get(usize::from(ROWS - 2))
            .is_some_and(|row| row.contains("(2,2)"))
    };
    gnu.read_until(Duration::from_secs(6), line_and_column);
    neo.read_until(Duration::from_secs(8), line_and_column);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            line_and_column(&grid),
            "{label} should show line and column in the mode line:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "column_number_mode_via_mx_shows_line_and_column_in_mode_line",
        &gnu,
        &neo,
    );
}

#[test]
fn display_line_numbers_mode_shows_buffer_line_numbers() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "display-line-numbers.txt",
        "alpha\nbeta\ngamma\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "display-line-numbers-mode");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("1 alpha"))
            && grid.iter().any(|row| row.contains("2 beta"))
            && grid.iter().any(|row| row.contains("3 gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            ready(&session.text_grid()),
            "{label} must visibly number all buffer lines:\n{}",
            session.text_grid().join("\n")
        );
    }
    assert_pair_exact_display(
        "display_line_numbers_mode_shows_buffer_line_numbers",
        &gnu,
        &neo,
    );
}

#[test]
fn global_display_line_numbers_mode_numbers_new_file_buffers() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "global-display-line-numbers-mode");
    open_home_file(
        &mut gnu,
        &mut neo,
        "global-display-line-numbers.txt",
        "alpha\nbeta\ngamma\n",
        "C-x C-f",
    );

    let has_numbered_lines = |grid: &[String]| {
        grid.iter().any(|row| row.contains("1 alpha"))
            && grid.iter().any(|row| row.contains("2 beta"))
            && grid.iter().any(|row| row.contains("3 gamma"))
    };
    gnu.read_until(Duration::from_secs(6), has_numbered_lines);
    neo.read_until(Duration::from_secs(8), has_numbered_lines);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert!(
        has_numbered_lines(&gnu.text_grid()),
        "GNU oracle must number a new file buffer after enabling the global mode:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        has_numbered_lines(&neo.text_grid()),
        "Neomacs must number a new file buffer after enabling the global mode:\n{}",
        neo.text_grid().join("\n")
    );
    assert_pair_exact_display(
        "global_display_line_numbers_mode_numbers_new_file_buffers",
        &gnu,
        &neo,
    );
}

#[test]
fn whitespace_mode_shows_ws_lighter_and_space_marks() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "whitespace-mode.txt",
        "alpha beta\ngamma delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(progn (setq-local whitespace-style '(face spaces space-mark)) (setq-local whitespace-display-mappings '((space-mark ?\s [?~] [?~]))) (message "whitespace ready"))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");
    let setup_ready = |grid: &[String]| grid.iter().any(|row| row.contains("whitespace ready"));
    gnu.read_until(Duration::from_secs(6), setup_ready);
    neo.read_until(Duration::from_secs(8), setup_ready);

    invoke_mx_command(&mut gnu, &mut neo, "whitespace-mode");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha~beta"))
            && grid.iter().any(|row| row.contains("gamma~delta"))
            && grid.iter().any(|row| row.contains(" ws"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "whitespace_mode_shows_ws_lighter_and_space_marks",
        &gnu,
        &neo,
    );
}

#[test]
fn display_fill_column_indicator_mode_shows_indicator_character() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "fill-column-indicator.txt",
        "alpha\nbeta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(progn (setq-local fill-column 8 display-fill-column-indicator-character ?|) (message "fci ready"))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    let setup_ready = |grid: &[String]| grid.iter().any(|row| row.contains("fci ready"));
    gnu.read_until(Duration::from_secs(6), setup_ready);
    neo.read_until(Duration::from_secs(8), setup_ready);

    invoke_mx_command(&mut gnu, &mut neo, "display-fill-column-indicator-mode");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("alpha") && row.contains("|"))
            && grid
                .iter()
                .any(|row| row.contains("beta") && row.contains("|"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "display_fill_column_indicator_mode_shows_indicator_character",
        &gnu,
        &neo,
    );
}

#[test]
fn toggle_truncate_lines_reports_enabled_and_disabled() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "truncate-lines.txt",
        "a very long line that can be wrapped or truncated depending on buffer display settings\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "toggle-truncate-lines");
    let enabled = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("Truncate long lines enabled"))
    };
    gnu.read_until(Duration::from_secs(6), enabled);
    neo.read_until(Duration::from_secs(8), enabled);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "toggle_truncate_lines_reports_enabled_and_disabled/enabled",
        &gnu,
        &neo,
    );

    invoke_mx_command(&mut gnu, &mut neo, "toggle-truncate-lines");
    let disabled = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("Truncate long lines disabled"))
    };
    gnu.read_until(Duration::from_secs(6), disabled);
    neo.read_until(Duration::from_secs(8), disabled);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "toggle_truncate_lines_reports_enabled_and_disabled/disabled",
        &gnu,
        &neo,
    );
}

#[test]
fn auto_revert_mode_toggles_file_buffer_lighter() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "auto-revert-mode.txt",
        "watched\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "auto-revert-mode");
    let enabled_ready = |grid: &[String]| grid.iter().any(|row| row.contains(" ARev"));
    gnu.read_until(Duration::from_secs(6), enabled_ready);
    neo.read_until(Duration::from_secs(8), enabled_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "auto_revert_mode_toggles_file_buffer_lighter/enabled",
        &gnu,
        &neo,
    );

    invoke_mx_command(&mut gnu, &mut neo, "auto-revert-mode");
    let disabled_ready = |grid: &[String]| !grid.iter().any(|row| row.contains(" ARev"));
    gnu.read_until(Duration::from_secs(6), disabled_ready);
    neo.read_until(Duration::from_secs(8), disabled_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "auto_revert_mode_toggles_file_buffer_lighter/disabled",
        &gnu,
        &neo,
    );
}

#[test]
fn visual_line_mode_shows_wrap_lighter() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "visual-line.txt",
        "a long prose line that visual line mode treats as a display line for ordinary movement commands\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "visual-line-mode");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("visual-line.txt") && row.contains("Wrap"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("visual_line_mode_shows_wrap_lighter", &gnu, &neo);
}

#[test]
fn outline_minor_mode_hide_sublevels_and_show_all() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "outline-minor.txt",
        "* Top\nbody under top\n** Child\nchild body\n* Next\nnext body\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "outline-minor-mode");
    let lighter_ready = |grid: &[String]| grid.iter().any(|row| row.contains(" Outl"));
    gnu.read_until(Duration::from_secs(6), lighter_ready);
    neo.read_until(Duration::from_secs(8), lighter_ready);

    invoke_mx_command(&mut gnu, &mut neo, "outline-hide-sublevels");
    let hidden_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("* Top"))
            && grid.iter().any(|row| row.contains("* Next"))
            && !grid.iter().any(|row| row.contains("body under top"))
            && !grid.iter().any(|row| row.contains("** Child"))
            && !grid.iter().any(|row| row.contains("child body"))
    };
    gnu.read_until(Duration::from_secs(6), hidden_ready);
    neo.read_until(Duration::from_secs(8), hidden_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "outline_minor_mode_hide_sublevels_and_show_all/hidden",
        &gnu,
        &neo,
    );

    invoke_mx_command(&mut gnu, &mut neo, "outline-show-all");
    let shown_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("body under top"))
            && grid.iter().any(|row| row.contains("** Child"))
            && grid.iter().any(|row| row.contains("child body"))
    };
    gnu.read_until(Duration::from_secs(6), shown_ready);
    neo.read_until(Duration::from_secs(8), shown_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "outline_minor_mode_hide_sublevels_and_show_all/shown",
        &gnu,
        &neo,
    );
}

#[test]
fn abbrev_mode_expands_defined_global_abbrev() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(&mut gnu, &mut neo, "abbrev-mode.txt", "seed\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(progn (define-abbrev global-abbrev-table "btw" "by the way") (message "abbrev ready"))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    let setup_ready = |grid: &[String]| grid.iter().any(|row| row.contains("abbrev ready"));
    gnu.read_until(Duration::from_secs(6), setup_ready);
    neo.read_until(Duration::from_secs(8), setup_ready);

    invoke_mx_command(&mut gnu, &mut neo, "abbrev-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-< C-a");
    send_both_raw(&mut gnu, &mut neo, b"btw ");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("by the way seed"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("abbrev_mode_expands_defined_global_abbrev", &gnu, &neo);
}

#[test]
fn define_global_abbrev_then_list_abbrevs_via_mx() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "define-global-abbrev");
    let abbrev_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Define global abbrev:"));
    gnu.read_until(Duration::from_secs(6), abbrev_prompt);
    neo.read_until(Duration::from_secs(8), abbrev_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"omw");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let expansion_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Expansion for omw:"));
    gnu.read_until(Duration::from_secs(6), expansion_prompt);
    neo.read_until(Duration::from_secs(8), expansion_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"on my way");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "list-abbrevs");
    let list_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Abbrevs*"))
            && grid.iter().any(|row| row.contains("global-abbrev-table"))
            && grid.iter().any(|row| row.contains("omw"))
            && grid.iter().any(|row| row.contains("on my way"))
    };
    gnu.read_until(Duration::from_secs(8), list_ready);
    neo.read_until(Duration::from_secs(10), list_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            list_ready(&grid),
            "{label} should list the defined global abbrev:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display("define_global_abbrev_then_list_abbrevs_via_mx", &gnu, &neo);
}

#[test]
fn subword_mode_moves_through_camel_case_parts() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "subword-mode.txt",
        "camelCaseIdentifier\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "subword-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-a M-f");
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("camelXCaseIdentifier"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("subword_mode_moves_through_camel_case_parts", &gnu, &neo);
}

#[test]
fn auto_save_mode_toggles_buffer_auto_save_file_name() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "auto-save-mode.txt",
        "autosave body\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "auto-save-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "auto-save=%S" (not (null buffer-auto-save-file-name)))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let off_ready = |grid: &[String]| grid.iter().any(|row| row.contains("auto-save=nil"));
    gnu.read_until(Duration::from_secs(6), off_ready);
    neo.read_until(Duration::from_secs(8), off_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "auto-save-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "auto-save=%S" (not (null buffer-auto-save-file-name)))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let on_ready = |grid: &[String]| grid.iter().any(|row| row.contains("auto-save=t"));
    gnu.read_until(Duration::from_secs(6), on_ready);
    neo.read_until(Duration::from_secs(8), on_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "auto_save_mode_toggles_buffer_auto_save_file_name",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_selection_mode_replaces_active_region_with_typed_text() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-selection.txt",
        "alpha beta\nsecond\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "delete-selection-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-a C-SPC M-f");
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("X beta"))
            && !grid.iter().any(|row| row.contains("Xalpha beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "delete_selection_mode_replaces_active_region_with_typed_text",
        &gnu,
        &neo,
    );
}

#[test]
fn electric_pair_mode_inserts_matching_delimiter() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(&mut gnu, &mut neo, "electric-pair.txt", "seed\n", "C-x C-f");

    invoke_mx_command(&mut gnu, &mut neo, "electric-pair-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-a");
    send_both_raw(&mut gnu, &mut neo, b"(");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("()seed"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("electric_pair_mode_inserts_matching_delimiter", &gnu, &neo);
}

#[test]
fn fido_vertical_mode_mx_find_f_matches_gnu_then_cg() {
    let (mut gnu, mut neo) = boot_fido_vertical_pair();

    wait_for_fido_mx_candidates(&mut gnu, &mut neo, "find-f");
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let bottom_start = fido_bottom_start();
        assert!(
            grid.iter().any(|row| row.contains("find-file")),
            "{label} should show find-file in fido candidates"
        );
        assert!(
            grid[bottom_start..]
                .iter()
                .filter(|row| !row.trim().is_empty())
                .count()
                >= 3,
            "{label} should expand the minibuffer into a vertical candidate list"
        );
    }
    assert_pair_exact_display(
        "fido_vertical_mode_mx_find_f_matches_gnu_then_cg/prompt-layout",
        &gnu,
        &neo,
    );
    assert_fido_prompt_matches_stable_behavior(
        "fido_vertical_mode_mx_find_f_matches_gnu_then_cg/prompt",
        &gnu,
        &neo,
    );

    abort_minibuffer_and_wait_for_scratch(&mut gnu, &mut neo);
    assert_pair_exact_display(
        "fido_vertical_mode_mx_find_f_matches_gnu_then_cg/abort",
        &gnu,
        &neo,
    );
}

#[test]
fn fido_vertical_mode_enabled_interactively_mx_shows_initial_candidates() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "fido-vertical-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-x");
    let candidates_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("M-x"))
            && bottom_nonempty_rows_from_grid(grid, fido_bottom_start()).len() >= 6
            && ["cd", "5x5", "gdb"]
                .iter()
                .all(|candidate| grid.iter().any(|row| row.contains(candidate)))
    };
    gnu.read_until(Duration::from_secs(6), candidates_ready);
    neo.read_until(Duration::from_secs(8), candidates_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !candidates_ready(&gnu.text_grid()) || !candidates_ready(&neo.text_grid()) {
        dump_pair_grids(
            "fido_vertical_mode_enabled_interactively_mx_shows_initial_candidates",
            &gnu,
            &neo,
        );
    }

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("M-x")),
            "{label} should leave the M-x prompt active"
        );
        assert!(
            bottom_nonempty_rows(session, fido_bottom_start()).len() >= 6,
            "{label} should show vertical command candidates for an empty M-x query"
        );
        for candidate in ["cd", "5x5", "gdb"] {
            assert!(
                grid.iter().any(|row| row.contains(candidate)),
                "{label} should include {candidate} in the initial M-x candidates"
            );
        }
    }

    abort_minibuffer_and_wait_for_scratch(&mut gnu, &mut neo);
    assert_pair_exact_display(
        "fido_vertical_mode_enabled_interactively_mx_shows_initial_candidates/abort",
        &gnu,
        &neo,
    );
}

#[test]
fn fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu() {
    let (mut gnu, mut neo) = boot_fido_vertical_pair();

    wait_for_fido_mx_candidates(&mut gnu, &mut neo, "find-f");
    assert_pair_exact_display(
        "fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu/first-prompt-layout",
        &gnu,
        &neo,
    );
    assert_fido_prompt_matches_stable_behavior(
        "fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu/first-prompt",
        &gnu,
        &neo,
    );
    abort_minibuffer_and_wait_for_scratch(&mut gnu, &mut neo);
    assert_pair_exact_display(
        "fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu/first-abort",
        &gnu,
        &neo,
    );

    wait_for_fido_mx_candidates(&mut gnu, &mut neo, "find-f");
    assert_pair_exact_display(
        "fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu/second-prompt-layout",
        &gnu,
        &neo,
    );
    assert_fido_prompt_matches_stable_behavior(
        "fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu/second-prompt",
        &gnu,
        &neo,
    );
    abort_minibuffer_and_wait_for_scratch(&mut gnu, &mut neo);
    assert_pair_exact_display(
        "fido_vertical_mode_mx_find_f_abort_then_repeat_matches_gnu/second-abort",
        &gnu,
        &neo,
    );
}

#[test]
fn fido_vertical_mode_completions() {
    // Create init file
    let init = "/tmp/tui-cmp-fido-test.el";
    std::fs::write(
        init,
        ";;; -*- lexical-binding: t; -*-\n(fido-vertical-mode 1)\n",
    )
    .expect("write init file");

    let init_arg = format!("-l {init}");
    let (mut gnu, mut neo) = boot_pair(&init_arg);

    // M-x then type to trigger completions
    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for s in [&mut gnu, &mut neo] {
        s.send(b"forw");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(4));

    let gl = gnu.text_grid();
    let nl = neo.text_grid();

    // Check that completions are visible (multiple non-empty rows near bottom)
    let bottom_6_start = (ROWS as usize).saturating_sub(6);
    let gnu_nonempty_bottom = gl[bottom_6_start..]
        .iter()
        .filter(|r| !r.trim().is_empty())
        .count();
    let neo_nonempty_bottom = nl[bottom_6_start..]
        .iter()
        .filter(|r| !r.trim().is_empty())
        .count();

    eprintln!("GNU bottom 6 rows with content: {gnu_nonempty_bottom}");
    eprintln!("NEO bottom 6 rows with content: {neo_nonempty_bottom}");
    for (i, (g, n)) in gl.iter().zip(nl.iter()).enumerate() {
        let gt = g.trim();
        let nt = n.trim();
        if !gt.is_empty() || !nt.is_empty() {
            eprintln!("  {i:2}: GNU=|{gt}| NEO=|{nt}|");
        }
    }

    assert!(
        neo_nonempty_bottom >= 2,
        "Neomacs should show fido completion candidates (got {neo_nonempty_bottom} non-empty rows)"
    );

    // C-g — minibuffer should shrink back
    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    let _gl2 = gnu.text_grid();
    let nl2 = neo.text_grid();

    // After C-g, the bottom area should be mostly empty again
    let bottom_4_start = (ROWS as usize).saturating_sub(4);
    let neo_nonempty_after = nl2[bottom_4_start..]
        .iter()
        .filter(|r| !r.trim().is_empty())
        .count();
    eprintln!("NEO bottom 4 rows after C-g: {neo_nonempty_after}");
    assert!(
        neo_nonempty_after <= 2,
        "Neomacs minibuffer should shrink after C-g (got {neo_nonempty_after} non-empty rows)"
    );
    assert_pair_exact_display("fido_vertical_mode_completions", &gnu, &neo);
}

// ── Major mode switching tests ──────────────────────────────

#[test]
fn text_mode_and_fundamental_mode_switching_updates_modeline() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "text-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let mode_line = &grid[grid.len().saturating_sub(2)];
        assert!(
            mode_line.contains("Text"),
            "{label}: mode-line should show Text after M-x text-mode. Mode-line: {mode_line}"
        );
    }

    invoke_mx_command(&mut gnu, &mut neo, "fundamental-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let mode_line = &grid[grid.len().saturating_sub(2)];
        assert!(
            mode_line.contains("Fundamental") || mode_line.contains("Lisp Interaction"),
            "{label}: mode-line should update after M-x fundamental-mode. Mode-line: {mode_line}"
        );
    }
    assert_pair_exact_display(
        "text_mode_and_fundamental_mode_switching_updates_modeline",
        &gnu,
        &neo,
    );
}

#[test]
fn header_line_format_displays_custom_header_above_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "header-line.txt",
        "buffer content line\n",
        "C-x C-f",
    );

    // Set header-line-format to display a custom string
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(setq header-line-format \"== HEADER ==\")",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let has_header = grid.iter().any(|row| row.contains("== HEADER =="));
        assert!(
            has_header,
            "{label}: should show header-line with custom text"
        );
    }

    assert_pair_exact_display(
        "header_line_format_displays_custom_header_above_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn tab_line_format_displays_custom_tab_line() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(&mut gnu, &mut neo, "tab-line.txt", "content\n", "C-x C-f");

    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (setq tab-line-format \"== TAB LINE ==\") (tab-line-mode 1))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("== TAB LINE ==")),
            "{label}: should show tab-line with custom text"
        );
    }

    assert_pair_exact_display("tab_line_format_displays_custom_tab_line", &gnu, &neo);
}

#[test]
fn left_margin_width_preserves_buffer_content() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "left-margin.txt",
        "hello world\nfoo bar\n",
        "C-x C-f",
    );

    support::eval_expression(&mut gnu, &mut neo, "(setq left-margin-width 4)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("hello")),
            "{label}: buffer visible with left-margin-width 4"
        );
    }
    assert_pair_exact_display("left_margin_width_preserves_buffer_content", &gnu, &neo);
}

#[test]
fn abbrev_expand_interactively_via_cx_apostrophe_after_typing() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "abbrev-expand.txt";
    let initial = "";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    // Define abbrev via M-x define-global-abbrev
    invoke_mx_command(&mut gnu, &mut neo, "define-global-abbrev");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"omwexp\r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"on my way expanded\r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Type the abbrev name and expand
    for s in [&mut gnu, &mut neo] {
        s.send(b"omwexp");
    }
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Expand via C-x '
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "'");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("on my way expanded")),
            "{label}: abbrev expand should replace omwexp with expansion"
        );
    }
    assert_pair_exact_display(
        "abbrev_expand_interactively_via_cx_apostrophe_after_typing",
        &gnu,
        &neo,
    );
}
