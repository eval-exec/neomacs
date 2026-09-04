#![cfg(unix)]
//! TUI comparison tests: help describe.

mod support;
use neomacs_tui_tests::*;
use std::{fs, time::Duration};
use support::*;

fn assert_describe_mode_help_content(label: &str, gnu: &TuiSession, neo: &TuiSession) {
    for (editor, session) in [("GNU", gnu), ("NEO", neo)] {
        let grid = session.text_grid();
        for needle in [
            "*Help*",
            "Major mode lisp-interaction-mode",
            "eval-print-last-sexp",
            "lisp-interaction-mode-hook",
        ] {
            assert!(
                grid.iter().any(|row| row.contains(needle)),
                "{label}: {editor} help buffer should contain {needle:?}"
            );
        }
    }
}

fn dump_named_buffer_to_home_file(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    buffer_name: &str,
    file_name: &str,
) {
    let home_file_name = format!("~/{file_name}");
    let expression = format!(
        r#"(with-current-buffer {buffer_name:?} (write-region (point-min) (point-max) {home_file_name:?} nil 'silent))"#
    );
    eval_expression(gnu, neo, &expression);

    let gnu_path = gnu.home_dir().join(file_name);
    let neo_path = neo.home_dir().join(file_name);
    for _ in 0..20 {
        read_both(gnu, neo, Duration::from_millis(300));
        if gnu_path.exists() && neo_path.exists() {
            return;
        }
    }

    panic!(
        "timed out waiting for {buffer_name:?} dumps at {} and {}",
        gnu_path.display(),
        neo_path.display()
    );
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn describe_mode_on_scratch_via_ch_m() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "m");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("Fundamental mode"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_describe_mode_help_content("describe_mode_on_scratch_via_ch_m", &gnu, &neo);
    assert_pair_exact_display("describe_mode_on_scratch_via_ch_m", &gnu, &neo);
}

#[test]
fn describe_mode_outline_heading_via_ch_m() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "m");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid
                .iter()
                .any(|row| row.contains("Major mode fundamental-mode"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_describe_mode_help_content("describe_mode_outline_heading_via_ch_m", &gnu, &neo);
    assert_pair_exact_display("describe_mode_outline_heading_via_ch_m", &gnu, &neo);
}

#[test]
fn quit_help_buffer_via_q() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "m");
    let help_ready = |grid: &[String]| grid.iter().any(|row| row.contains("*Help*"));
    gnu.read_until(Duration::from_secs(10), help_ready);
    neo.read_until(Duration::from_secs(20), help_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "q");
    let scratch_only_ready =
        |grid: &[String]| scratch_ready(grid) && !grid.iter().any(|row| row.contains("*Help*"));
    gnu.read_until(Duration::from_secs(6), scratch_only_ready);
    neo.read_until(Duration::from_secs(8), scratch_only_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("quit_help_buffer_via_q", &gnu, &neo);
}

#[test]
fn help_for_help_via_ch_ch_lists_help_options() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "C-h");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Commands, Keys and Functions"))
            && grid.iter().any(|row| row.contains("Manuals"))
            && grid.iter().any(|row| row.contains("Show help for key"))
            && grid.iter().any(|row| row.contains("Show all key bindings"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids(
            "help_for_help_via_ch_ch_lists_help_options/not-ready",
            &gnu,
            &neo,
        );
    }

    assert_pair_exact_display("help_for_help_via_ch_ch_lists_help_options", &gnu, &neo);
}

#[test]
fn describe_key_find_file_via_chk() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "k");
    send_both(&mut gnu, &mut neo, "C-x C-f");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("find-file"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} should show *Help* after C-h k"
        );
        assert!(
            grid.iter().any(|row| row.contains("find-file")),
            "{label} help buffer should mention find-file"
        );
        assert!(
            grid.iter().any(|row| row.contains("C-x C-f")),
            "{label} help buffer should mention C-x C-f"
        );
    }
    assert_pair_exact_display("describe_key_find_file_via_chk", &gnu, &neo);
}

#[test]
fn help_with_tutorial_via_ch_t_opens_tutorial_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "t");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("TUTORIAL"))
            && grid.iter().any(|row| row.contains("Emacs tutorial"))
            && grid.iter().any(|row| row.contains("CONTROL key"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("TUTORIAL")),
            "{label} should show the tutorial buffer name"
        );
        assert!(
            grid.iter().any(|row| row.contains("Emacs tutorial")),
            "{label} should show the tutorial heading"
        );
        assert!(
            grid.iter().any(|row| row.contains("CONTROL key")),
            "{label} should show the tutorial contents"
        );
    }
    assert_pair_exact_display(
        "help_with_tutorial_via_ch_t_opens_tutorial_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn info_directory_via_ch_i_opens_info_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "i");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("*info*") || row.contains("*Info*"))
            && grid
                .iter()
                .any(|row| row.contains("INFO tree") || row.contains("Directory node"))
            && grid.iter().any(|row| row.contains("Emacs"))
    };
    gnu.read_until(Duration::from_secs(12), ready);
    neo.read_until(Duration::from_secs(20), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids("info_directory_via_ch_i_opens_info_buffer", &gnu, &neo);
    }

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("*info*") || row.contains("*Info*")),
            "{label} should show the Info buffer name"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("INFO tree") || row.contains("Directory node")),
            "{label} should show the Info directory"
        );
        assert!(
            grid.iter().any(|row| row.contains("Emacs")),
            "{label} should show Emacs entries in the Info directory"
        );
    }
    assert_pair_exact_display("info_directory_via_ch_i_opens_info_buffer", &gnu, &neo);
}

#[test]
fn calendar_via_mx_opens_calendar_and_q_quits() {
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

    if !calendar_ready(&gnu.text_grid()) || !calendar_ready(&neo.text_grid()) {
        dump_pair_grids(
            "calendar_via_mx_opens_calendar_and_q_quits/open",
            &gnu,
            &neo,
        );
    }
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("Calendar")),
            "{label} should display the Calendar mode line"
        );
        assert!(
            day_header_count(&grid) >= 3,
            "{label} should show Gregorian calendar day headers"
        );
    }
    assert_pair_exact_display(
        "calendar_via_mx_opens_calendar_and_q_quits/open",
        &gnu,
        &neo,
    );

    send_both_raw(&mut gnu, &mut neo, b"q");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "calendar_via_mx_opens_calendar_and_q_quits/quit",
        &gnu,
        &neo,
    );
}

#[test]
fn view_hello_file_pages_down_and_up_via_cv_mv() {
    let (mut gnu, mut neo) = boot_pair("");
    use_backend_only_vc_mode_line(&mut gnu, &mut neo);

    invoke_mx_command(&mut gnu, &mut neo, "view-hello-file");
    let hello_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("This is a list of ways"))
            && grid.iter().any(|row| row.contains("HELLO"))
    };
    gnu.read_until(Duration::from_secs(8), hello_ready);
    neo.read_until(Duration::from_secs(12), hello_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "view_hello_file_pages_down_and_up_via_cv_mv/open",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-v");
    let paged_down = |grid: &[String]| {
        grid.iter().any(|row| row.contains("LANGUAGE"))
            || grid.iter().any(|row| row.contains("Adlam"))
            || grid.iter().any(|row| row.contains("Braille"))
    };
    gnu.read_until(Duration::from_secs(8), paged_down);
    neo.read_until(Duration::from_secs(12), paged_down);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "view_hello_file_pages_down_and_up_via_cv_mv/page-down",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "M-v");
    gnu.read_until(Duration::from_secs(8), hello_ready);
    neo.read_until(Duration::from_secs(12), hello_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "view_hello_file_pages_down_and_up_via_cv_mv/page-up",
        &gnu,
        &neo,
    );
}

#[test]
fn view_hello_file_via_ch_h_opens_hello_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    use_backend_only_vc_mode_line(&mut gnu, &mut neo);

    send_help_sequence(&mut gnu, &mut neo, "h");
    let hello_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("This is a list of ways"))
            && grid.iter().any(|row| row.contains("HELLO"))
    };
    gnu.read_until(Duration::from_secs(8), hello_ready);
    neo.read_until(Duration::from_secs(12), hello_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("view_hello_file_via_ch_h_opens_hello_buffer", &gnu, &neo);
}

#[test]
fn describe_copying_via_ch_cc_opens_copying_file() {
    let (mut gnu, mut neo) = boot_pair("");

    // `describe-copying' visits each executable's own source-tree COPYING
    // file. Those fixtures intentionally belong to different Git checkouts,
    // so their branch labels are environmental data rather than editor
    // behavior. Suppress VC for this workflow at the source; the rest of the
    // suite retains its explicit VC/modeline coverage.
    support::eval_expression(&mut gnu, &mut neo, "(setq vc-handled-backends nil)");

    send_help_sequence(&mut gnu, &mut neo, "C-c");
    let copying_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("COPYING"))
            && grid
                .iter()
                .any(|row| row.contains("GNU GENERAL PUBLIC LICENSE"))
    };
    gnu.read_until(Duration::from_secs(8), copying_ready);
    neo.read_until(Duration::from_secs(12), copying_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("COPYING")),
            "{label} should show the COPYING help file"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("GNU GENERAL PUBLIC LICENSE")),
            "{label} should show GPL text from the COPYING file"
        );
    }
    assert_pair_exact_display("describe_copying_via_ch_cc_opens_copying_file", &gnu, &neo);
}

#[test]
fn describe_no_warranty_via_ch_cw_jumps_to_warranty_section() {
    let (mut gnu, mut neo) = boot_pair("");

    // As above, keep the paired display independent of the two source
    // checkouts' branch names without disabling VC in unrelated tests.
    support::eval_expression(&mut gnu, &mut neo, "(setq vc-handled-backends nil)");

    send_help_sequence(&mut gnu, &mut neo, "C-w");
    let warranty_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("COPYING"))
            && grid
                .iter()
                .any(|row| row.contains("15. Disclaimer of Warranty"))
            && grid
                .iter()
                .any(|row| row.contains("THERE IS NO WARRANTY FOR THE PROGRAM"))
    };
    gnu.read_until(Duration::from_secs(8), warranty_ready);
    neo.read_until(Duration::from_secs(12), warranty_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("COPYING")),
            "{label} should show the COPYING help file"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("15. Disclaimer of Warranty")),
            "{label} should jump to the warranty disclaimer"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("THERE IS NO WARRANTY FOR THE PROGRAM")),
            "{label} should show the warranty disclaimer body"
        );
    }
    assert_pair_exact_display(
        "describe_no_warranty_via_ch_cw_jumps_to_warranty_section",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_bindings_via_ch_b() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "b");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| {
                row.contains("Key translations")
                    || row.contains("Major Mode Bindings")
                    || row.contains("lisp-interaction-mode")
            })
    };
    gnu.read_until(Duration::from_secs(15), ready);
    neo.read_until(Duration::from_secs(30), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} should show *Help* after C-h b\n{}",
            grid.join("\n")
        );
        assert!(
            grid.iter().any(|row| row.contains("Key translations")
                || row.contains("Major Mode Bindings")
                || row.contains("lisp-interaction-mode")),
            "{label} describe-bindings should show a GNU-visible heading\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display("describe_bindings_via_ch_b", &gnu, &neo);
}

#[test]
fn quit_describe_bindings_via_q() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "b");
    let help_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| {
                row.contains("Key translations")
                    || row.contains("Major Mode Bindings")
                    || row.contains("lisp-interaction-mode")
            })
    };
    gnu.read_until(Duration::from_secs(15), help_ready);
    neo.read_until(Duration::from_secs(30), help_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "q");
    let scratch_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*scratch*")),
            "{label} should return to *scratch* after q"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("This buffer is for text that is not saved")),
            "{label} should show the scratch buffer contents after q"
        );
    }
    assert_pair_exact_display("quit_describe_bindings_via_q", &gnu, &neo);
}

#[test]
fn apropos_command_find_file_via_ch_a_lists_matches() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "a");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Search for command"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "apropos_command_find_file_via_ch_a_lists_matches/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"find-file");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Apropos*"))
            && grid.iter().any(|row| row.contains("find-file"))
            && grid.iter().any(|row| row.contains("C-x C-f"))
    };
    gnu.read_until(Duration::from_secs(10), ready);
    neo.read_until(Duration::from_secs(15), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Apropos*")),
            "{label} should show *Apropos* after C-h a"
        );
        assert!(
            grid.iter().any(|row| row.contains("find-file")),
            "{label} apropos-command should list find-file"
        );
        assert!(
            grid.iter().any(|row| row.contains("C-x C-f")),
            "{label} apropos-command should show find-file's default binding"
        );
    }
    assert_pair_exact_display(
        "apropos_command_find_file_via_ch_a_lists_matches",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_function_find_file_via_ch_f() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Describe function"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"find-file");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("find-file is"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} should show *Help* after C-h f"
        );
        assert!(
            grid.iter().any(|row| row.contains("find-file is")),
            "{label} describe-function should mention find-file"
        );
        assert!(
            grid.iter().any(|row| row.contains("C-x C-f")),
            "{label} describe-function should mention C-x C-f"
        );
    }
    assert_pair_exact_display("describe_function_find_file_via_ch_f", &gnu, &neo);
}

#[test]
fn describe_variable_fill_column_via_ch_v() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "v");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Describe variable"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("describe_variable_fill_column_via_ch_v/prompt", &gnu, &neo);

    for session in [&mut gnu, &mut neo] {
        session.send(b"fill-column");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid
                .iter()
                .any(|row| row.contains("fill-column is a variable"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} should show *Help* after C-h v"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("fill-column is a variable")),
            "{label} describe-variable should mention fill-column"
        );
        assert!(
            grid.iter().any(|row| row.contains("70")),
            "{label} describe-variable should show fill-column's default value"
        );
    }
    assert_pair_exact_display("describe_variable_fill_column_via_ch_v", &gnu, &neo);
}

#[test]
fn describe_symbol_fill_column_via_ch_o() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "o");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Describe symbol"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("describe_symbol_fill_column_via_ch_o/prompt", &gnu, &neo);

    for session in [&mut gnu, &mut neo] {
        session.send(b"fill-column");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid
                .iter()
                .any(|row| row.contains("fill-column is a variable"))
            && grid
                .iter()
                .any(|row| row.contains("Automatically becomes buffer-local"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids("describe_symbol_fill_column_via_ch_o/not-ready", &gnu, &neo);
    }

    assert_pair_exact_display("describe_symbol_fill_column_via_ch_o", &gnu, &neo);
}

#[test]
fn describe_syntax_via_ch_s_shows_syntax_table() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "s");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("syntax table"))
            && grid.iter().any(|row| row.contains("whitespace"))
            && grid.iter().any(|row| row.contains("word"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids(
            "describe_syntax_via_ch_s_shows_syntax_table/not-ready",
            &gnu,
            &neo,
        );
    }

    assert_pair_exact_display("describe_syntax_via_ch_s_shows_syntax_table", &gnu, &neo);
}

#[test]
fn describe_face_default_via_mx_shows_face_attributes() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "describe-face");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Describe face"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "describe_face_default_via_mx_shows_face_attributes/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"default");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("Face: default"))
            && grid.iter().any(|row| row.contains("Documentation:"))
            && grid.iter().any(|row| row.contains("Family"))
            && grid.iter().any(|row| row.contains("Foreground"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids(
            "describe_face_default_via_mx_shows_face_attributes/not-ready",
            &gnu,
            &neo,
        );
    }

    assert_pair_exact_display(
        "describe_face_default_via_mx_shows_face_attributes",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_key_briefly_find_file_via_ch_c() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "c");
    send_both(&mut gnu, &mut neo, "C-x C-f");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("C-x C-f"))
            && grid.iter().any(|row| row.contains("find-file"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("C-x C-f")),
            "{label} should show the described key after C-h c"
        );
        assert!(
            grid.iter().any(|row| row.contains("find-file")),
            "{label} describe-key-briefly should mention find-file"
        );
    }
    assert_pair_exact_display("describe_key_briefly_find_file_via_ch_c", &gnu, &neo);
}

#[test]
fn where_is_find_file_via_ch_w_reports_key_binding() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Where is command"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "where_is_find_file_via_ch_w_reports_key_binding/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"find-file");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("find-file is on") && row.contains("C-x C-f"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .rev()
                .take(4)
                .any(|row| { row.contains("find-file is on") && row.contains("C-x C-f") }),
            "{label} where-is should report the default find-file binding"
        );
    }
    assert_pair_exact_display(
        "where_is_find_file_via_ch_w_reports_key_binding",
        &gnu,
        &neo,
    );
}

#[test]
fn where_is_prompt_ctrl_h_preserves_command_text() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Where is command"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"find-fileX");
    }
    send_both(&mut gnu, &mut neo, "BS");

    let command_preserved = |grid: &[String]| grid.iter().any(|row| row.contains("find-fileX"));
    gnu.read_until(Duration::from_secs(6), command_preserved);
    neo.read_until(Duration::from_secs(8), command_preserved);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            command_preserved(&grid),
            "{label} should keep the where-is minibuffer text after terminal C-h\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("where_is_prompt_ctrl_h_preserves_command_text", &gnu, &neo);
}

#[test]
fn where_is_prompt_del_deletes_previous_command_character() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Where is command"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"find-fileX");
    }
    send_both(&mut gnu, &mut neo, "DEL RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("find-file is on") && row.contains("C-x C-f"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label} should run where-is for corrected find-file after terminal DEL\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "where_is_prompt_del_deletes_previous_command_character",
        &gnu,
        &neo,
    );
}

#[test]
fn where_is_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Where is command"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Where is command"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty where-is prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "where_is_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn view_lossage_via_ch_l_shows_recent_keys_and_commands() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-f C-b C-h l");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("C-f"))
            && grid.iter().any(|row| row.contains("forward-char"))
            && grid.iter().any(|row| row.contains("C-b"))
            && grid.iter().any(|row| row.contains("backward-char"))
            && grid.iter().any(|row| row.contains("C-h l"))
            && grid.iter().any(|row| row.contains("view-lossage"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids(
            "view_lossage_via_ch_l_shows_recent_keys_and_commands/not-ready",
            &gnu,
            &neo,
        );
    }

    assert_pair_exact_display(
        "view_lossage_via_ch_l_shows_recent_keys_and_commands",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_char_on_ascii_character_matches_gnu_help_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "describe-char-ascii.txt",
        "ASCII target\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-a");
    invoke_mx_command(&mut gnu, &mut neo, "describe-char");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid
                .iter()
                .any(|row| row.contains("LATIN CAPITAL LETTER A"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("character") || row.contains("LATIN")),
            "{label} describe-char should show character info\n{}",
            grid.join("\n")
        );
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} describe-char should open a Help buffer\n{}",
            grid.join("\n")
        );
    }

    dump_named_buffer_to_home_file(&mut gnu, &mut neo, "*Help*", "describe-char-ascii-help.txt");
    let gnu_help = fs::read_to_string(gnu.home_dir().join("describe-char-ascii-help.txt"))
        .expect("read GNU describe-char help dump");
    let neo_help = fs::read_to_string(neo.home_dir().join("describe-char-ascii-help.txt"))
        .expect("read Neomacs describe-char help dump");
    assert_eq!(
        gnu_help, neo_help,
        "describe-char help buffer for ASCII character should match GNU exactly"
    );
    assert_pair_exact_display(
        "describe_char_on_ascii_character_matches_gnu_help_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_variable_fill_column_via_ch_v_shows_docstring() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "v");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"fill-column\r");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("fill-column"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} C-h v should open a Help buffer"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("fill-column") && row.contains("column")),
            "{label} C-h v fill-column should show variable info"
        );
    }
    assert_pair_exact_display(
        "describe_variable_fill_column_via_ch_v_shows_docstring",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_function_forward_char_via_ch_f_shows_docstring() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"forward-char\r");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("forward-char"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} C-h f should open a Help buffer"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("forward-char") && !row.contains("C-h f")),
            "{label} C-h f forward-char should show function doc"
        );
    }
    assert_pair_exact_display(
        "describe_function_forward_char_via_ch_f_shows_docstring",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_mode_via_ch_m_shows_lisp_interaction_bindings() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "m");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("Lisp Interaction"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} C-h m should open a Help buffer"
        );
        assert!(
            grid.iter().any(|row| row.contains("Lisp Interaction")),
            "{label} C-h m should show Lisp Interaction mode info"
        );
        assert!(
            grid.iter().any(|row| row.contains("eval-print-last-sexp")),
            "{label} C-h m should show key bindings like eval-print-last-sexp"
        );
    }
    assert_pair_exact_display(
        "describe_mode_via_ch_m_shows_lisp_interaction_bindings",
        &gnu,
        &neo,
    );
}

#[test]
fn describe_key_cx_cf_via_ch_k_shows_find_file_doc() {
    let (mut gnu, mut neo) = boot_pair("");
    send_help_sequence(&mut gnu, &mut neo, "k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    // Send the key to describe: C-x C-f
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "C-f");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Help*"))
            && grid.iter().any(|row| row.contains("find-file"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Help*")),
            "{label} C-h k should open a Help buffer"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("find-file") && row.contains("C-x C-f")),
            "{label} C-h k C-x C-f should show find-file binding"
        );
    }
    assert_pair_exact_display(
        "describe_key_cx_cf_via_ch_k_shows_find_file_doc",
        &gnu,
        &neo,
    );
}
