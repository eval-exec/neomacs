#![cfg(unix)]
//! TUI comparisons for common programming-buffer workflows.
//!
//! These cover GNU Emacs behavior from `lisp/indent.el`,
//! `lisp/newcomment.el`, `lisp/imenu.el`, and
//! `lisp/progmodes/elisp-mode.el`.

mod support;

use std::time::Duration;
use support::*;

#[test]
fn indent_for_tab_command_indents_current_elisp_line() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "tab-indent-probe.el";
    let initial = "(defun neo-tab-probe ()\n(message \"alpha\")\n)\n";
    let expected = "(defun neo-tab-probe ()\n  (message \"alpha\")\n)\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-n TAB");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(4), |grid| {
        grid.iter().any(|row| row.contains("  (message \"alpha\")"))
    });

    save_current_file_and_assert_contents(
        "indent-for-tab-command",
        &mut gnu,
        &mut neo,
        name,
        expected,
    );
    assert_pair_exact_display(
        "indent_for_tab_command_indents_current_elisp_line",
        &gnu,
        &neo,
    );
}

#[test]
fn comment_dwim_on_blank_elisp_line_inserts_indented_comment() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "comment-dwim-blank-probe.el";
    let initial = "(defun neo-comment-probe ()\n\n  (message \"alpha\"))\n";
    let expected = "(defun neo-comment-probe ()\n  ;; \n  (message \"alpha\"))\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-n M-;");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(4), |grid| {
        grid.iter().any(|row| row.contains("  ;;"))
    });

    save_current_file_and_assert_contents("comment-dwim", &mut gnu, &mut neo, name, expected);
    assert_pair_exact_display(
        "comment_dwim_on_blank_elisp_line_inserts_indented_comment",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_defun_via_cmeta_x_defines_current_elisp_function() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "eval-defun-probe.el";
    let initial = "(defun neo-eval-defun-probe ()\n  \"value-from-eval-defun\")\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    eval_expression(&mut gnu, &mut neo, "(neo-eval-defun-probe)");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(6), |grid| {
        grid.iter().any(|row| row.contains("value-from-eval-defun"))
    });
    assert_pair_exact_display(
        "eval_defun_via_cmeta_x_defines_current_elisp_function",
        &gnu,
        &neo,
    );
}

#[test]
fn imenu_via_mx_jumps_to_named_elisp_defun() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "imenu-probe.el";
    let initial = "(defun neo-imenu-alpha ()\n  1)\n\n(defun neo-imenu-beta ()\n  2)\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    invoke_mx_command(&mut gnu, &mut neo, "imenu");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(6), |grid| {
        grid.last().is_some_and(|row| row.contains("Index item:"))
    });
    gnu.send(b"neo-imenu-beta");
    neo.send(b"neo-imenu-beta");
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(message "imenu-at-beta %s" (save-excursion (beginning-of-line) (looking-at "(defun neo-imenu-beta")))"#,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(6), |grid| {
        grid.iter().any(|row| row.contains("imenu-at-beta t"))
    });
    assert_pair_exact_display("imenu_via_mx_jumps_to_named_elisp_defun", &gnu, &neo);
}

#[test]
fn indent_region_via_mx_indents_elisp_defun() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "indent-region-probe.el";
    let initial = "(defun indent-region-probe ()\n(message \"hello\"))\n";
    let expected = "(defun indent-region-probe ()\n  (message \"hello\"))\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    // Select whole buffer
    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Use M-x indent-region (C-M-\ may not transmit over PTY)
    invoke_mx_command(&mut gnu, &mut neo, "indent-region");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "indent-region-via-mx",
        &mut gnu,
        &mut neo,
        name,
        expected,
    );
    assert_pair_exact_display("indent_region_via_mx_indents_elisp_defun", &gnu, &neo);
}

#[test]
fn forward_sexp_via_mx_moves_past_balanced_expression() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "forward-sexp.el";
    let initial = "(foo bar) baz\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-a");
    invoke_mx_command(&mut gnu, &mut neo, "forward-sexp");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // After moving past (foo bar), insert X to mark position
    for s in [&mut gnu, &mut neo] {
        s.send(b"X");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("(foo bar) X") || r.contains("(foo bar)X")),
            "{label}: forward-sexp should move past (foo bar)"
        );
    }
    assert_pair_exact_display(
        "forward_sexp_via_mx_moves_past_balanced_expression",
        &gnu,
        &neo,
    );
}
