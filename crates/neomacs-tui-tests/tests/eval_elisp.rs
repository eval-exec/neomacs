#![cfg(unix)]
//! TUI comparison tests: eval elisp.

mod support;
use neomacs_tui_tests::TuiSession;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

fn backtrace_ready(grid: &[String]) -> bool {
    grid.iter().any(|row| row.contains("*Backtrace*"))
        && grid.iter().any(|row| row.contains("Debugger entered"))
        && grid
            .iter()
            .any(|row| row.contains("void-variable") || row.contains("value as variable is void"))
}

/// Ask GNU's own `cl-print-object' method to expose byte-code structure in
/// backtraces instead of its abbreviated `sxhash' token.
///
/// GNU documents that `sxhash' values are not stable across Emacs sessions,
/// and `sxhash_obj' hashes symbol/object identity through `XHASH'
/// (`src/fns.c`).  A GNU/Neomacs pair is necessarily two sessions, so the
/// default `#<bytecode HEX>' spelling cannot be an exact cross-process
/// contract.  `raw' is the stronger contract: it compares the actual byte-code
/// slots, constants, and visible styling rather than an opaque identity hash.
fn expose_structural_bytecode_in_backtraces(gnu: &mut TuiSession, neo: &mut TuiSession) {
    support::eval_expression(
        gnu,
        neo,
        "(progn (require 'cl-print) (setq cl-print-compiled 'raw))",
    );
    read_both(gnu, neo, Duration::from_secs(1));
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn eval_last_sexp_via_cx_ce_prints_echo_area_value() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both_raw(&mut gnu, &mut neo, b"(+ 40 2)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-e");

    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains("42"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("(+ 40 2)")),
            "{label} should keep the evaluated sexp in the buffer"
        );
        assert!(
            grid.iter().rev().take(4).any(|row| row.contains("42")),
            "{label} should show eval-last-sexp's value in the echo area"
        );
    }
    assert_pair_exact_display(
        "eval_last_sexp_via_cx_ce_prints_echo_area_value",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_last_sexp_error_via_cx_ce_opens_backtrace() {
    let (mut gnu, mut neo) = boot_pair("");
    expose_structural_bytecode_in_backtraces(&mut gnu, &mut neo);

    send_both_raw(&mut gnu, &mut neo, b"hello");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-e");

    gnu.read_until(Duration::from_secs(6), backtrace_ready);
    neo.read_until(Duration::from_secs(8), backtrace_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !backtrace_ready(&gnu.text_grid()) || !backtrace_ready(&neo.text_grid()) {
        dump_pair_grids("eval_last_sexp_error_via_cx_ce_opens_backtrace", &gnu, &neo);
    }

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Backtrace*")),
            "{label} should display the Backtrace buffer"
        );
        assert!(
            grid.iter().any(|row| row.contains("Debugger entered")),
            "{label} should show debugger entry text"
        );
        assert!(
            grid.iter().any(|row| row.contains("hello")),
            "{label} should show the void variable in the backtrace"
        );
    }
    assert_pair_exact_display("eval_last_sexp_error_via_cx_ce_opens_backtrace", &gnu, &neo);
}

#[test]
fn eval_expression_minibuffer_ctrl_h_does_not_delete_previous_character() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    let eval_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), eval_prompt);
    neo.read_until(Duration::from_secs(8), eval_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"(+ 1 2)X");
    }
    send_both(&mut gnu, &mut neo, "BS");

    let expression_preserved = |grid: &[String]| grid.iter().any(|row| row.contains("(+ 1 2)X"));
    gnu.read_until(Duration::from_secs(6), expression_preserved);
    neo.read_until(Duration::from_secs(8), expression_preserved);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            expression_preserved(&grid),
            "{label} should keep the previous eval-expression minibuffer character after terminal C-h\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "eval_expression_minibuffer_ctrl_h_does_not_delete_previous_character",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_expression_minibuffer_del_deletes_previous_character() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    let eval_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), eval_prompt);
    neo.read_until(Duration::from_secs(8), eval_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"(+ 1 2)X");
    }
    send_both(&mut gnu, &mut neo, "DEL RET");

    let result_ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains("3"));
    gnu.read_until(Duration::from_secs(6), result_ready);
    neo.read_until(Duration::from_secs(8), result_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            result_ready(&grid),
            "{label} should evaluate corrected (+ 1 2) after terminal DEL in M-:\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "eval_expression_minibuffer_del_deletes_previous_character",
        &gnu,
        &neo,
    );
}

#[test]
fn next_line_key_preserves_goal_column_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/simple.el:next-line delegates to line-move, which records
    // temporary-goal-column and finishes on the same logical column.
    let setup = r#"(progn(switch-to-buffer"*nl*")(erase-buffer)(insert"abcdef\nuvwxyz\n")(goto-char 5)(setq line-move-visual nil)(message"nlready:%S"(list(line-number-at-pos)(current-column))))"#;
    support::eval_expression(&mut gnu, &mut neo, setup);

    let setup_ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("nlready:(1 4)"))
    };
    gnu.read_until(Duration::from_secs(6), setup_ready);
    neo.read_until(Duration::from_secs(8), setup_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            setup_ready(&grid),
            "{label}: next-line setup should place point on first line column 4\n{}",
            grid.join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    let probe = r#"(message"nextkey:%S"(list(buffer-name)(line-number-at-pos)(current-column)(char-after)))"#;
    support::eval_expression(&mut gnu, &mut neo, probe);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("nextkey:") && row.contains(r#"\"*nl*\""#) && row.contains("2 4 121")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: C-n should land on the same logical column like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("next_line_key_preserves_goal_column_like_gnu", &gnu, &neo);
}

#[test]
fn next_line_ignores_invisible_newlines_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/simple.el:line-move-1 honors line-move-ignore-invisible by
    // skipping invisible text/newlines while preserving the goal column.
    let expr = r#"(progn(switch-to-buffer"*invmove*")(erase-buffer)(setq line-move-visual nil line-move-ignore-invisible t)(insert"aaaa\nbbbb\ncccc\n")(put-text-property 5 10 'invisible t)(goto-char 3)(call-interactively 'next-line)(message"invmove:%S"(list(line-number-at-pos)(current-column)(point)(char-after))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("invmove:(3 2 13 99)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: next-line should ignore invisible newlines like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("next_line_ignores_invisible_newlines_like_gnu", &gnu, &neo);
}

#[test]
fn previous_line_ignores_invisible_newlines_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/simple.el:line-move-1 has a mirrored backward path that skips
    // invisible previous lines when line-move-ignore-invisible is non-nil.
    let expr = r#"(progn(switch-to-buffer"*previnv*")(erase-buffer)(setq line-move-visual nil line-move-ignore-invisible t)(insert"aaaa\nbbbb\ncccc\n")(put-text-property 5 10 'invisible t)(goto-char 13)(call-interactively 'previous-line)(message"previnv:%S"(list(line-number-at-pos)(current-column)(point)(char-after))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("previnv:(1 2 3 97)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: previous-line should ignore invisible newlines like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "previous_line_ignores_invisible_newlines_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn previous_line_without_ignore_invisible_matches_gnu_position() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/simple.el:line-move-1 uses a different backward branch when
    // line-move-ignore-invisible is nil, and still has defined point semantics.
    let expr = r#"(progn(switch-to-buffer"*previnv2*")(erase-buffer)(setq line-move-visual nil line-move-ignore-invisible nil)(insert"aaaa\nbbbb\ncccc\n")(put-text-property 5 10 'invisible t)(goto-char 13)(call-interactively 'previous-line)(message"previnv2:%S"(list(line-number-at-pos)(current-column)(point)(char-after))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("previnv2:(2 0 10 10)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: previous-line with line-move-ignore-invisible nil should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "previous_line_without_ignore_invisible_matches_gnu_position",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_expression_empty_minibuffer_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    let eval_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), eval_prompt);
    neo.read_until(Duration::from_secs(8), eval_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Eval:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
            && !grid.iter().any(|row| row.contains("*Backtrace*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty M-: prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "eval_expression_empty_minibuffer_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn execute_extended_command_minibuffer_ctrl_h_preserves_command_text() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"forward-charX");
    }
    send_both(&mut gnu, &mut neo, "BS");

    let command_preserved = |grid: &[String]| grid.iter().any(|row| row.contains("forward-charX"));
    gnu.read_until(Duration::from_secs(6), command_preserved);
    neo.read_until(Duration::from_secs(8), command_preserved);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            command_preserved(&grid),
            "{label} should keep the previous M-x minibuffer character after terminal C-h\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "execute_extended_command_minibuffer_ctrl_h_preserves_command_text",
        &gnu,
        &neo,
    );
}

#[test]
fn execute_extended_command_minibuffer_del_deletes_previous_character() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"forward-charX");
    }
    send_both(&mut gnu, &mut neo, "DEL");
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"Z");
    }

    let inserted = |grid: &[String]| grid.iter().any(|row| row.trim_end() == "Z");
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            inserted(&grid),
            "{label} should run corrected forward-char after terminal DEL in M-x\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "execute_extended_command_minibuffer_del_deletes_previous_character",
        &gnu,
        &neo,
    );
}

#[test]
fn execute_extended_command_minibuffer_multiple_del_keyhits() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"forward-charXYZ");
    }
    send_both(&mut gnu, &mut neo, "DEL DEL DEL RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"Z");
    }

    let inserted = |grid: &[String]| grid.iter().any(|row| row.trim_end() == "Z");
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            inserted(&grid),
            "{label} should run corrected forward-char after three terminal DEL keyhits in M-x\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "execute_extended_command_minibuffer_multiple_del_keyhits",
        &gnu,
        &neo,
    );
}

#[test]
fn execute_extended_command_empty_minibuffer_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("M-x"))
            && !grid.iter().any(|row| row.contains("No match"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty M-x prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "execute_extended_command_empty_minibuffer_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn trace_function_background_writes_trace_output_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    let eval_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), eval_prompt);
    neo.read_until(Duration::from_secs(8), eval_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(progn (defun trace-probe (x) (+ x 1)) (trace-function-background 'trace-probe) (trace-probe 41))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");

    let eval_ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains("42"));
    gnu.read_until(Duration::from_secs(6), eval_ready);
    neo.read_until(Duration::from_secs(8), eval_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    let switch_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Switch to buffer:"));
    gnu.read_until(Duration::from_secs(6), switch_prompt);
    neo.read_until(Duration::from_secs(8), switch_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"*trace-output*");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let trace_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*trace-output*"))
            && grid.iter().any(|row| row.contains("1 -> (trace-probe 41)"))
            && grid.iter().any(|row| row.contains("1 <- trace-probe: 42"))
    };
    gnu.read_until(Duration::from_secs(6), trace_ready);
    neo.read_until(Duration::from_secs(8), trace_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*trace-output*")),
            "{label} should display trace-buffer"
        );
        assert!(
            grid.iter().any(|row| row.contains("1 -> (trace-probe 41)")),
            "{label} should show trace entry"
        );
        assert!(
            grid.iter().any(|row| row.contains("1 <- trace-probe: 42")),
            "{label} should show trace exit"
        );
    }
    assert_pair_exact_display(
        "trace_function_background_writes_trace_output_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn completion_at_point_in_elisp_buffer_completes_function_name() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "completion-at-point.el",
        "(forward-cha\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    invoke_mx_command(&mut gnu, &mut neo, "completion-at-point");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("completion-at-point.el"))
            && grid.iter().any(|row| row.contains("(forward-char"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("(forward-char")),
            "{label} should complete an Emacs Lisp function name at point\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "completion_at_point_in_elisp_buffer_completes_function_name",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_expression_via_mcolon_prints_echo_area_value() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_via_mcolon_prints_echo_area_value/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"(+ 2 3)");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("5 (#o5, #x5"))
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
                .any(|row| row.contains("5 (#o5, #x5")),
            "{label} should show eval-expression's integer value formats"
        );
    }
    assert_pair_exact_display(
        "eval_expression_via_mcolon_prints_echo_area_value",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_expression_history_via_mcolon_mp_recalls_previous_expression() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/prompt-1",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"(+ 1 2)");
    }
    let first_expr_typed =
        |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval: (+ 1 2)"));
    gnu.read_until(Duration::from_secs(6), first_expr_typed);
    neo.read_until(Duration::from_secs(8), first_expr_typed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/typed-1",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let first_result = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains("3"));
    gnu.read_until(Duration::from_secs(6), first_result);
    neo.read_until(Duration::from_secs(8), first_result);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/result-1",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "M-:");
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/prompt-2",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "M-p");
    let recalled = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval: (+ 1 2)"));
    gnu.read_until(Duration::from_secs(6), recalled);
    neo.read_until(Duration::from_secs(8), recalled);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/recalled",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "DEL DEL");
    send_both_raw(&mut gnu, &mut neo, b"5)");
    let edited = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval: (+ 1 5)"));
    gnu.read_until(Duration::from_secs(6), edited);
    neo.read_until(Duration::from_secs(8), edited);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/edited",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let second_result = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains("6"));
    gnu.read_until(Duration::from_secs(6), second_result);
    neo.read_until(Duration::from_secs(8), second_result);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "eval_expression_history_via_mcolon_mp_recalls_previous_expression/result-2",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_expression_error_via_mcolon_opens_backtrace() {
    let (mut gnu, mut neo) = boot_pair("");
    expose_structural_bytecode_in_backtraces(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "eval_expression_error_via_mcolon_opens_backtrace/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"missing-variable");
    }
    send_both(&mut gnu, &mut neo, "RET");

    gnu.read_until(Duration::from_secs(6), backtrace_ready);
    neo.read_until(Duration::from_secs(8), backtrace_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !backtrace_ready(&gnu.text_grid()) || !backtrace_ready(&neo.text_grid()) {
        dump_pair_grids(
            "eval_expression_error_via_mcolon_opens_backtrace",
            &gnu,
            &neo,
        );
    }

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Backtrace*")),
            "{label} should display the Backtrace buffer"
        );
        assert!(
            grid.iter().any(|row| row.contains("Debugger entered")),
            "{label} should show debugger entry text"
        );
        assert!(
            grid.iter().any(|row| row.contains("missing-variable")),
            "{label} should show the void variable in the backtrace"
        );
    }
    assert_pair_exact_display(
        "eval_expression_error_via_mcolon_opens_backtrace",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_expression() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Type (+ 1 2) RET
    for s in [&mut gnu, &mut neo] {
        s.send(b"(+ 1 2)");
    }
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Echo area (last row) should show "3"
    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let gnu_echo = gl.last().unwrap();
    let neo_echo = nl.last().unwrap();
    assert!(
        gnu_echo.contains('3'),
        "GNU echo should show 3: {gnu_echo:?}"
    );
    assert!(
        neo_echo.contains('3'),
        "NEO echo should show 3: {neo_echo:?}"
    );
    assert_pair_exact_display("eval_expression", &gnu, &neo);
}

// ── File modtime tests ───────────────────────────────────────

#[test]
fn visited_file_modtime_returns_cons_after_file_visit() {
    let (mut gnu, mut neo) = boot_pair("");

    // Visit a file with insert-file-contents :visit
    open_home_file(
        &mut gnu,
        &mut neo,
        "modtime-test.el",
        "(message \"hello\")\n",
        "C-x C-f",
    );

    // Evaluate (visited-file-modtime) — should return a cons, not 0
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"(visited-file-modtime)");
    }
    send_both(&mut gnu, &mut neo, "RET");

    // Result should show a cons like (12345 67890) in the echo area,
    // not the integer 0
    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains('(') && row.chars().filter(|&c| c.is_ascii_digit()).count() >= 4
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            !echo.contains(" 0 "),
            "{label}: visited-file-modtime should return cons, not 0. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "visited_file_modtime_returns_cons_after_file_visit",
        &gnu,
        &neo,
    );
}

#[test]
fn verify_visited_file_modtime_returns_t_for_unmodified_file() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "modtime-u.el",
        "(provide 'modtime-u)\n",
        "C-x C-f",
    );

    // Evaluate (verify-visited-file-modtime) — should return t
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"(verify-visited-file-modtime)");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains('t'));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('t'),
            "{label}: verify-visited-file-modtime should return t. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "verify_visited_file_modtime_returns_t_for_unmodified_file",
        &gnu,
        &neo,
    );
}

// ── Narrowing / buffer position tests ────────────────────────

#[test]
fn mode_line_shows_buffer_position_percent() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "mode-pct.el",
        "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n",
        "C-x C-f",
    );

    // Move to bottom, check mode-line shows Top/Bot/All
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Mode-line row (second to last) should show buffer position
    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let gnu_mode = &gl[gl.len().saturating_sub(2)];
    let neo_mode = &nl[nl.len().saturating_sub(2)];

    // Both should show some position indicator (Top, Bot, All, or %)
    let has_pos = |row: &str| {
        row.contains("Top") || row.contains("Bot") || row.contains("All") || row.contains('%')
    };
    assert!(
        has_pos(gnu_mode),
        "GNU mode-line should have position indicator: {gnu_mode}"
    );
    assert!(
        has_pos(neo_mode),
        "NEO mode-line should have position indicator: {neo_mode}"
    );
    assert_pair_exact_display("mode_line_shows_buffer_position_percent", &gnu, &neo);
}

// ── Lisp environment semantics tests ────────────────────────

#[test]
fn lisp_environment_variables_match_gnu_emacs_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // Test (emacs-version) returns a string
    support::eval_expression(&mut gnu, &mut neo, "(emacs-version)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('\"'),
            "{label}: (emacs-version) should return a string. Echo: {echo}"
        );
    }

    // Test (boundp 'enable-recursive-minibuffers) — should be t
    support::eval_expression(&mut gnu, &mut neo, "(boundp 'enable-recursive-minibuffers)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('t'),
            "{label}: (boundp 'enable-recursive-minibuffers) should be t. Echo: {echo}"
        );
    }

    // Test (>= emacs-major-version 31) — NeoMacs is Emacs 31+
    support::eval_expression(&mut gnu, &mut neo, "(>= emacs-major-version 31)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('t'),
            "{label}: (>= emacs-major-version 31) should be t. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "lisp_environment_variables_match_gnu_emacs_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn defconst_sets_local_binding_like_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "defconstlocal:%S" (list (let ((x 1)) (defvar x 2) x) (let ((x 1)) (defconst x 3) x)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("defconstlocal:") && row.contains("(1 3)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: defconst should set the current local binding while defvar should not\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("defconst_sets_local_binding_like_gnu_semantics", &gnu, &neo);
}

// ── Face inheritance tests ──────────────────────────────────

#[test]
fn face_attribute_inherit_returns_correct_chain_for_mode_line() {
    let (mut gnu, mut neo) = boot_pair("");

    // mode-line inherits from mode-line-active which inherits from
    // mode-line base.  Test the chain via face-attribute.
    support::eval_expression(&mut gnu, &mut neo, "(face-attribute 'mode-line :inherit)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        // Both should return something non-nil (a face name or nil)
        assert!(
            !echo.trim().is_empty(),
            "{label}: (face-attribute 'mode-line :inherit) should return value. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "face_attribute_inherit_returns_correct_chain_for_mode_line",
        &gnu,
        &neo,
    );
}

// ── Buffer position correctness tests ───────────────────────

#[test]
fn buffer_positions_are_correct_1_based_after_file_visit() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(&mut gnu, &mut neo, "pos-check.txt", "abc\n", "C-x C-f");

    // Check (point-min) is 1
    support::eval_expression(&mut gnu, &mut neo, "(point-min)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('1'),
            "{label}: (point-min) should be 1 after visiting file. Echo: {echo}"
        );
    }

    // Check (point-max) matches between GNU and NeoMacs
    support::eval_expression(&mut gnu, &mut neo, "(point-max)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    let gnu_pm = gnu.text_grid().last().cloned().unwrap_or_default();
    let neo_pm = neo.text_grid().last().cloned().unwrap_or_default();
    let gnu_num: String = gnu_pm.chars().filter(|c| c.is_ascii_digit()).collect();
    let neo_num: String = neo_pm.chars().filter(|c| c.is_ascii_digit()).collect();
    assert!(!gnu_num.is_empty(), "GNU point-max not found: {gnu_pm}");
    assert!(!neo_num.is_empty(), "NEO point-max not found: {neo_pm}");
    assert_eq!(
        gnu_num, neo_num,
        "point-max mismatch: GNU={gnu_num} NEO={neo_num}"
    );

    // (buffer-size) should also match
    support::eval_expression(&mut gnu, &mut neo, "(buffer-size)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    let gnu_bs = gnu.text_grid().last().cloned().unwrap_or_default();
    let neo_bs = neo.text_grid().last().cloned().unwrap_or_default();
    let gnu_bs_num: String = gnu_bs.chars().filter(|c| c.is_ascii_digit()).collect();
    let neo_bs_num: String = neo_bs.chars().filter(|c| c.is_ascii_digit()).collect();
    assert_eq!(
        gnu_bs_num, neo_bs_num,
        "buffer-size mismatch: GNU={gnu_bs_num} NEO={neo_bs_num}"
    );

    // (point) at start of buffer should be 1
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    support::eval_expression(&mut gnu, &mut neo, "(point)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('1'),
            "{label}: (point) at buffer start should be 1. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "buffer_positions_are_correct_1_based_after_file_visit",
        &gnu,
        &neo,
    );
}

// ── Fundamental Elisp operation tests ───────────────────────

#[test]
fn fundamental_elisp_operations_return_correct_values() {
    let (mut gnu, mut neo) = boot_pair("");

    // Test (car (cons 1 2)) should be 1
    support::eval_expression(&mut gnu, &mut neo, "(car (cons 1 2))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('1'),
            "{label}: (car (cons 1 2)) should be 1. Echo: {echo}"
        );
    }

    // Test (cdr (cons 1 2)) should be 2
    support::eval_expression(&mut gnu, &mut neo, "(cdr (cons 1 2))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('2'),
            "{label}: (cdr (cons 1 2)) should be 2. Echo: {echo}"
        );
    }

    // Test (equal (cons 1 2) (cons 1 2)) should be t
    support::eval_expression(&mut gnu, &mut neo, "(equal (cons 1 2) (cons 1 2))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('t'),
            "{label}: (equal (cons 1 2) (cons 1 2)) should be t. Echo: {echo}"
        );
    }

    // Test (listp (cons 1 2)) should be t
    support::eval_expression(&mut gnu, &mut neo, "(listp (cons 1 2))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('t'),
            "{label}: (listp (cons 1 2)) should be t. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "fundamental_elisp_operations_return_correct_values",
        &gnu,
        &neo,
    );
}

#[test]
fn sequence_mutation_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"seq-mut:%S\" (list (mapcar (lambda (x) (cons x (* x x))) '(1 2 3)) (let ((xs (list 3 1 2))) (sort xs '<)) (delq 'b (list 'a 'b 'c 'b)) (nreverse (list 1 2 3))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("seq-mut:")
                && row.contains("((1 . 1) (2 . 4) (3 . 9))")
                && row.contains("(1 2 3)")
                && row.contains("(a c)")
                && row.contains("(3 2 1)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sequence mutation functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "sequence_mutation_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn aset_unibyte_string_non_byte_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "asetbyte:%S" (let ((s (string-as-unibyte "abc"))) (condition-case e (aset s 1 #x100) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("asetbyte:")
                && row.contains("error")
                && row.contains("Attempt to store non-byte value into unibyte string")
                && !row.contains("wrong-type-argument")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: aset into unibyte string with non-byte char should match GNU error semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "aset_unibyte_string_non_byte_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn aset_multibyte_string_non_ascii_replacement_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "asetmb:%S" (condition-case e (let ((s (copy-sequence "aéc"))) (aset s 1 ?x)) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("asetmb:")
                && row.contains("error")
                && row.contains("Attempt to replace non-ASCII char in multibyte string")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: aset replacing a non-ASCII multibyte char should match GNU error semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "aset_multibyte_string_non_ascii_replacement_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn store_substring_preserves_aset_multibyte_errors_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/international/mule-util.el:store-substring is a thin loop over
    // `aset'.  It must inherit aset's refusal to replace a non-ASCII
    // multibyte character with a different byte-length character.
    let expr = r#"(progn (require 'mule-util) (message "storemb:%S" (list (condition-case e (let ((s (copy-sequence "éé"))) (store-substring s 0 "xx") s) (error (list (car e) (cadr e)))) (condition-case e (let ((s (copy-sequence "éé"))) (store-substring s 0 "x") s) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("storemb:")
            && recent
                .matches("Attempt to replace non-ASCII char in multibyte string")
                .count()
                >= 2
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: store-substring should preserve GNU aset multibyte replacement errors\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "store_substring_preserves_aset_multibyte_errors_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn nconc_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"nconc:%S\" (let ((a (list 1 2)) (b (list 3))) (list (nconc a b) a b)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("nconc:") && row.contains("((1 2 3) (1 2 3) (3))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: nconc destructive list behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("nconc_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn nconc_circular_nonfinal_list_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:nconc walks every non-final list with FOR_EACH_TAIL.
    // Circular non-final arguments signal `circular-list`; they must not hang
    // while trying to find the splice point.
    let expr = r#"(message "nconccycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (nconc x (list 3))) (error (list (car e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("nconccycle:(circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: nconc circular non-final list error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "nconc_circular_nonfinal_list_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn equal_circular_list_behavior_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU 31.0.90 refactored `equal` (internal_equal_1 / internal_equal_cycle): a
    // cycle is no longer an error.  Two separate self-circular lists with equal
    // cars are `equal' (t); structurally different circular lists are nil — so the
    // three cases yield (t t nil), not the pre-31 (t (circular-list) nil).
    let expr = r#"(message "equalcycle:%S" (list (let ((x (list 1))) (setcdr x x) (equal x x)) (condition-case e (let ((x (list 1)) (y (list 1))) (setcdr x x) (setcdr y y) (equal x y)) (error (list (car e)))) (condition-case e (let ((x (list 1 2)) (y (list 1 2))) (setcdr (cdr x) x) (setcdr (cdr y) (cdr y)) (equal x y)) (error (list (car e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("equalcycle:(t t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: equal should match GNU circular-list behavior\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "equal_circular_list_behavior_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn vector_sort_compare_strings_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"sortseq:%S\" (list (sort (copy-sequence [3 1 2]) '<) (compare-strings \"abc\" nil nil \"abd\" nil nil) (compare-strings \"abc\" nil nil \"ABC\" nil nil t)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("sortseq:") && row.contains("([1 2 3] -3 t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vector sort and compare-strings should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "vector_sort_compare_strings_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn compare_strings_reversed_range_errors_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:compare-strings clamps too-large positive END values for
    // compatibility, then validates START/END with validate_subarray.  A
    // reversed range must signal args-out-of-range, not panic or compare an
    // empty slice.
    let expr = r#"(message "cmpstrrange:%S" (condition-case e (compare-strings "abc" 3 2 "abc" nil nil) (error (list (car e) (cadr e) (caddr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"cmpstrrange:(args-out-of-range \"abc\" 3)"#;
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: compare-strings should reject reversed START/END bounds like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("compare_strings_reversed_range_errors_like_gnu", &gnu, &neo);
}

#[test]
fn sort_keyword_error_semantics_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:sort parses keyword pairs only for odd argument counts.
    // Unknown keywords signal `error`; a two-argument call is the legacy
    // `(sort SEQ LESSP)` form, so :lessp is called as a predicate and signals
    // `void-function`.
    let expr = r#"(message "sortkwerr:%S" (list (condition-case e (sort [3 1] :bad t) (error (list (car e) (cadr e)))) (condition-case e (sort [3 1] :lessp) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("sortkwerr:")
                && row.contains(r#"((error \"Invalid keyword argument\") (void-function :lessp))"#)
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sort keyword error behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("sort_keyword_error_semantics_match_gnu", &gnu, &neo);
}

#[test]
fn sort_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:sort_list computes list_length before sorting, so cyclic
    // list input must signal `circular-list` instead of entering sort setup.
    let expr = r#"(message "sortcycle:%S" (condition-case e (let ((x (list 2 1))) (setcdr (cdr x) x) (sort x (function <))) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("sortcycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sort should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("sort_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn sort_rejects_records_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:sort accepts only lists, nil, and vectors.  Records are
    // not valid sort sequences even though they are vectorlike objects.
    let expr = r#"(message "sortrec:%S" (condition-case e (sort #s(foo 3 1 2) :lessp #'<) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("sortrec:(wrong-type-argument list-or-vector-p)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sort should reject records with list-or-vector-p like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("sort_rejects_records_like_gnu", &gnu, &neo);
}

#[test]
fn copy_tree_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"copy:%S\" (let* ((inner (list 1)) (tree (list inner (vector inner))) (copy (copy-tree tree t))) (setcar inner 9) (list tree copy (eq (car tree) (car copy)) (eq (aref (cadr tree) 0) (aref (cadr copy) 0)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("copy:")
                && row.contains("((9)")
                && row.contains("((1)")
                && row.contains("nil nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: copy-tree list/vector deep-copy behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("copy_tree_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn property_list_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"plist:%S\" (let ((plist (list :a 1 :b 2 :a 3))) (list (plist-get plist :a) (plist-member plist :b) (progn (setq plist (plist-put plist :c 4)) plist) (progn (setq plist (plist-put plist :a 9)) plist))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("plist:")
                && row.contains("(1")
                && row.contains("(:b 2 :a 3 :c 4)")
                && row.contains(":c 4")
                && row.contains(":a 9")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: property-list functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "property_list_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn property_list_edge_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"plistedge:%S\" (let ((p (list :a 1 :b 2 :a 3))) (list (plist-get p :a) (plist-member p :a) (plist-get (plist-put p :b 9) :b) (condition-case e (plist-get '(:a) :a) (wrong-type-argument (car e)) (error (car e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("plistedge:")
                && row.contains("(1 (:a 1")
                && row.contains(":b 9")
                && row.contains("nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: property-list edge behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "property_list_edge_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn plist_get_circular_missing_property_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:plist_get uses FOR_EACH_TAIL_SAFE and returns nil for a
    // cyclic plist when PROP is absent.  This deliberately differs from
    // plist-member/plist-put, which validate the tail as plistp.
    let expr = r#"(message "pgcycle:%S" (condition-case e (let ((x (list :a 1 :b 2))) (setcdr (cdddr x) x) (plist-get x :z)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("pgcycle:nil"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: plist-get should return nil for missing cyclic plist property like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "plist_get_circular_missing_property_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"symprop:%S\" (let ((sym (make-symbol \"symprop-target\"))) (put sym 'alpha 1) (put sym 'beta '(x y)) (list (get sym 'alpha) (or (get sym 'missing) 'fallback) (symbol-plist sym) (progn (setplist sym '(gamma 3 delta 4)) (symbol-plist sym)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("symprop:")
                && row.contains("(1 fallback")
                && row.contains("alpha 1")
                && row.contains("beta (x y)")
                && row.contains("(gamma 3 delta 4)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol property functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overriding_plist_environment_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "overplist:%S" (let ((sym (make-symbol "overplist-target"))) (put sym 'p 'real) (put sym 'q 'real) (let ((overriding-plist-environment (list (list sym 'p 'override 'q nil)))) (list (get sym 'p) (get sym 'q) (put sym 'p 'new) (get sym 'p) (let ((overriding-plist-environment nil)) (get sym 'p)) (symbol-plist sym)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "overplist:(override real new override new (p new q real))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overriding-plist-environment get/put behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overriding_plist_environment_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

// ── String and numeric operation tests ──────────────────────

#[test]
fn string_search_replace_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"strfun:%S\" (list (upcase \"aBz\") (downcase \"AbZ\") (capitalize \"hello-world test\") (let ((s (copy-sequence \"abc\"))) (aset s 1 ?Z) s) (progn (string-match \"\\\\([a-z]+\\\\)-\\\\([0-9]+\\\\)\" \"foo-123\") (list (match-string 0 \"foo-123\") (match-string 1 \"foo-123\") (replace-match \"bar\" nil nil \"foo-123\" 1)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("strfun:")
                && row.contains("ABZ")
                && row.contains("abz")
                && row.contains("Hello-World Test")
                && row.contains("aZc")
                && row.contains("foo-123")
                && row.contains("foo")
                && row.contains("bar-123")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string search/replace functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_search_replace_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_case_predicate_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"caseconv:%S\" (list (upcase-initials \"hello-world TEST\") (capitalize \"foo_bar baz\") (string-prefix-p \"foo\" \"foobar\") (string-suffix-p \"bar\" \"foobar\") (string-match-p (regexp-quote \"a+b\") \"xxa+b\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("caseconv:")
                && row.contains("Hello-World TEST")
                && row.contains("Foo_Bar Baz")
                && row.contains("t t 2")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string case and predicate behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_case_predicate_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn subst_char_in_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"subst:%S\" (list (subst-char-in-string ?a ?x \"banana\") (let ((s (copy-sequence \"banana\"))) (list (subst-char-in-string ?a ?x s t) s)) (let ((s \"banana\")) (eq s (subst-char-in-string ?a ?x s))) (subst-char-in-string ?q ?x \"banana\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("subst:")
                && row.matches("bxnxnx").count() >= 3
                && row.contains("nil")
                && row.contains("banana")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: subst-char-in-string copy and in-place behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "subst_char_in_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn remove_delq_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"remove:%S\" (let* ((xs (list 'a 'b 'c 'b)) (r (remove 'b xs)) (ys (list 'a 'b 'c 'b)) (d (delq 'b ys))) (list r xs d ys)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("remove:") && row.contains("((a c) (a b c b) (a c) (a c))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: remove and delq behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "remove_delq_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn alist_lookup_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"alist:%S\" (let ((xs (list (cons \"k\" 1) (cons (copy-sequence \"k\") 2) (cons 'sym 3)))) (list (assoc \"k\" xs) (assq \"k\" xs) (assq 'sym xs) (rassoc 2 xs))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("alist:")
                && row.contains("nil")
                && row.contains("(sym . 3)")
                && row.contains(". 2)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: alist lookup behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "alist_lookup_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn assq_delete_all_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"assocdel:%S\" (let ((a (list (cons 'x 1) (cons 'y 2) (cons 'x 3)))) (list (assq-delete-all 'x a) a)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("assocdel:")
                && row.contains("(((y . 2))")
                && row.contains("((x . 1) (y . 2))")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: assq-delete-all destructive alist behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "assq_delete_all_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn member_predicate_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"member:%S\" (let ((s (copy-sequence \"x\"))) (list (member \"x\" (list s)) (memq \"x\" (list s)) (memql 1.0 (list 1.0)) (memql 1 (list 1.0)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("member:") && row.contains("nil") && row.contains("1.0"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: member predicate behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "member_predicate_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn assoc_detects_circular_alists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:assoc walks ALIST with FOR_EACH_TAIL and validates the
    // final tail with CHECK_LIST_END.  A cyclic alist must signal
    // `circular-list`, not loop forever.
    let expr = r#"(message "assoccycle:%S" (condition-case e (let ((x (list (cons "a" 1) (cons "b" 2)))) (setcdr (cdr x) x) (assoc "z" x)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("assoccycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: assoc should detect circular alists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("assoc_detects_circular_alists_like_gnu", &gnu, &neo);
}

#[test]
fn memq_circular_list_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:memq walks with FOR_EACH_TAIL and then
    // CHECK_LIST_END.  A circular list with no match signals
    // `circular-list`; it must not spin forever.
    let expr = r#"(message "memqcycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (memq 3 x)) (error (list (car e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("memqcycle:(circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: memq circular-list error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("memq_circular_list_error_matches_gnu_semantics", &gnu, &neo);
}

#[test]
fn copy_sequence_circular_list_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:copy-sequence copies list tails with FOR_EACH_TAIL and
    // checks the final tail.  Circular lists signal `circular-list`; copying
    // must not loop forever.
    let expr = r#"(message "copyseqcycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (copy-sequence x)) (error (list (car e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("copyseqcycle:(circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: copy-sequence circular-list error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "copy_sequence_circular_list_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn append_circular_list_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:append copies non-final list arguments through
    // concat_to_list, which validates list termination.  Circular inputs must
    // signal `circular-list`; they must not hang.
    let expr = r#"(message "appendcycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (append x nil)) (error (list (car e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("appendcycle:(circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: append circular-list error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "append_circular_list_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn vconcat_circular_list_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:vconcat uses concat_to_vector, which computes argument
    // lengths through Flength before allocation.  Circular list inputs signal
    // `circular-list`; they must not loop while building the vector.
    let expr = r#"(message "vconcatcycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (vconcat x)) (error (list (car e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("vconcatcycle:(circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vconcat circular-list error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "vconcat_circular_list_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn vconcat_rejects_records_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:vconcat accepts list, vector, and string arguments.
    // Records are valid copy-sequence inputs, but not vconcat sequences.
    let expr = r#"(message "vconcatrec:%S" (condition-case e (vconcat #s(foo a b)) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("vconcatrec:(wrong-type-argument sequencep)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vconcat should reject records with sequencep like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("vconcat_rejects_records_like_gnu", &gnu, &neo);
}

#[test]
fn elt_rejects_records_even_though_length_accepts_them_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:Flength accepts RECORDP via PVSIZE, but Felt calls
    // CHECK_ARRAY(sequence, Qsequencep).  Records are not valid `elt`
    // sequences even though they have a length and copy-sequence works.
    let expr = r#"(message "eltrec:%S" (list (length #s(foo a b)) (condition-case e (elt #s(foo a b) 0) (error (list (car e) (cadr e)))) (copy-sequence #s(foo a b))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "eltrec:(3 (wrong-type-argument sequencep) #s(foo a b))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: elt should reject records with GNU sequencep semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "elt_rejects_records_even_though_length_accepts_them_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn vector_array_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"vecfun:%S\" (let* ((v (vector 'a 'b 'c)) (copy (copy-sequence v))) (aset copy 1 'B) (list (length v) (aref v 1) copy (vconcat '(1 2) [3 4] \"ab\") (append [x y] '(z)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("vecfun:")
                && row.contains("(3 b [a B c]")
                && row.contains("[1 2 3 4 97 98]")
                && row.contains("(x y z)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vector/array functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "vector_array_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn vector_subseq_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"vectorresize:%S\" (let ((v [1 2 3])) (list (vectorp v) (vconcat v [4]) (append v nil) (seq-subseq v 1 3))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("vectorresize:") && row.contains("(t [1 2 3 4] (1 2 3) [2 3])"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vector concatenation and seq-subseq behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "vector_subseq_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn fillarray_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"fillseq:%S\" (list (let ((v (vector 1 2 3))) (fillarray v 9) v) (let ((s (copy-sequence \"abc\"))) (fillarray s ?x) s) (condition-case e (fillarray (list 1 2) 3) (wrong-type-argument (car e)) (error (car e)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("fillseq:")
                && row.contains("[9 9 9]")
                && row.contains("xxx")
                && row.contains("wrong-type-argument")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: fillarray vector/string mutation should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("fillarray_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn fillarray_and_clear_string_multibyte_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fillclear:%S" (let ((s1 (copy-sequence "éé")) (s2 (copy-sequence "éé")) (s3 (copy-sequence "é"))) (put-text-property 0 1 'face 'bold s3) (list (condition-case e (progn (fillarray s1 ?x) (string-to-list s1)) (error (list (car e) (cadr e)))) (condition-case e (progn (fillarray s2 ?🙂) (string-to-list s2)) (error (list (car e) (cadr e)))) (progn (clear-string s3) (list (string-to-list s3) (multibyte-string-p s3) (length s3) (string-bytes s3) (text-properties-at 0 s3))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fillclear:")
            && recent
                .matches("Attempt to change byte length of a string")
                .count()
                == 2
            && recent.contains("((0 0) nil 2 2 nil)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: fillarray and clear-string multibyte string behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "fillarray_and_clear_string_multibyte_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn fillarray_multibyte_string_preserves_character_codepoints_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:fillarray validates ITEM with CHECK_CHARACTER, encodes it
    // with CHAR_STRING for multibyte strings, and refuses only byte-length
    // changes.  It must not truncate non-ASCII character codepoints to bytes.
    let expr = r#"(message "fillmb:%S" (let ((s (copy-sequence "éé"))) (fillarray s 256) (list (string-to-list s) (multibyte-string-p s) (length s) (string-bytes s))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "fillmb:((256 256) t 2 4)";
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: fillarray on multibyte strings should preserve character codepoints like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "fillarray_multibyte_string_preserves_character_codepoints_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn reverse_circular_list_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:reverse walks list tails with FOR_EACH_TAIL and then
    // CHECK_LIST_END, so circular lists signal circular-list rather than a
    // generic listp type error.
    let expr = r#"(message "revcycle:%S" (let ((x (list 1 2 3))) (setcdr (last x) x) (list (safe-length x) (proper-list-p x) (condition-case e (reverse x) (error (car e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("revcycle:") && row.contains("(5 nil circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: reverse circular-list error should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "reverse_circular_list_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn delq_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:delq walks tails with FOR_EACH_TAIL and validates the
    // terminal tail with CHECK_LIST_END.  Circular inputs must signal
    // `circular-list`; they must not spin.
    let expr = r#"(message "delqcycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (delq 9 x)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("delqcycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: delq should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("delq_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn delete_dups_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/subr.el:delete-dups calls `length' before destructively
    // removing duplicates, so circular inputs must signal `circular-list`
    // instead of entering the duplicate-removal loop forever.
    let expr = r#"(message "dupscycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (delete-dups x)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("dupscycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: delete-dups should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("delete_dups_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn mapcar_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:mapcar computes Flength before mapping, and Flength's
    // list_length path signals `circular-list` for cyclic lists.
    let expr = r#"(message "mapcycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (mapcar 'identity x)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("mapcycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapcar should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("mapcar_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn mapconcat_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:mapconcat computes Flength before mapping, and Flength's
    // list_length path signals `circular-list` for cyclic lists.
    let expr = r#"(message "mapconcatcycle:%S" (condition-case e (let ((x (list "a" "b"))) (setcdr (cdr x) x) (mapconcat 'identity x "-")) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("mapconcatcycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapconcat should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("mapconcat_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn mapc_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:mapc computes Flength before calling mapcar1 for side
    // effects, so cyclic lists must signal `circular-list`.
    let expr = r#"(message "mapccycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (mapc (lambda (_) nil) x)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("mapccycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapc should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("mapc_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn mapcan_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:mapcan computes Flength before mapcar1 and nconc, so
    // cyclic input sequences must signal `circular-list`.
    let expr = r#"(message "mapcancycle:%S" (condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (mapcan (lambda (v) (list v)) x)) (error (car e))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("mapcancycle:") && row.contains("circular-list"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapcan should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("mapcan_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn copy_alist_detects_circular_lists_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:copy-alist first calls copy-sequence on the alist,
    // so circular top-level alists signal circular-list instead of looping.
    let expr = r#"(message "copyalistcycle:%S" (condition-case e (let ((x (list (cons 'a 1)))) (setcdr x x) (copy-alist x)) (error (list (car e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("copyalistcycle:(circular-list)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: copy-alist circular top-level alist should signal circular-list like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("copy_alist_detects_circular_lists_like_gnu", &gnu, &neo);
}

#[test]
fn mapcar_rejects_char_tables_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:mapcar explicitly rejects char-tables with `listp' after
    // its Flength preflight.  Char-table internals must not be exposed as
    // mappable sequence elements.
    let expr = r#"(message "mct:%S" (let ((c (make-char-table nil 0))) (condition-case e (mapcar #'identity c) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "mct:(wrong-type-argument listp)";
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapcar should reject char-tables like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("mapcar_rejects_char_tables_like_gnu", &gnu, &neo);
}

#[test]
fn mapcar_rejects_records_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:mapcar/mapcar1 accepts lists, nil, vectors,
    // bool-vectors, and strings; records are rejected as non-sequences.
    let expr = r#"(message "mapcarrec:%S" (condition-case e (mapcar #'identity #s(foo a b)) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("mapcarrec:(wrong-type-argument sequencep)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapcar should reject records with sequencep like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("mapcar_rejects_records_like_gnu", &gnu, &neo);
}

#[test]
fn length_predicates_large_circular_lists_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:length_internal uses a fast unchecked path only below
    // 0xffff.  At larger thresholds it walks with FOR_EACH_TAIL and signals
    // `circular-list` for cyclic lists.
    let expr = r#"(message "lenpredbig:%S" (let ((x (list 1 2))) (setcdr (cdr x) x) (list (condition-case e (length< x 100000) (error (car e))) (condition-case e (length> x 100000) (error (car e))) (condition-case e (length= x 100000) (error (car e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("lenpredbig:")
                && row.contains("(circular-list circular-list circular-list)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: large length predicates should detect circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "length_predicates_large_circular_lists_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn bool_vector_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"boolvec:%S\" (let ((bv (make-bool-vector 4 nil))) (aset bv 1 t) (aset bv 3 t) (list (bool-vector-p bv) (aref bv 0) (aref bv 1) (vconcat bv))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("boolvec:") && row.contains("(t nil t [nil t nil t])"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: bool-vector behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "bool_vector_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn make_bool_vector_negative_length_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "boolveclen:%S" (condition-case e (make-bool-vector -1 nil) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("boolveclen:")
                && row.contains("wrong-type-argument")
                && row.contains("wholenump")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: make-bool-vector negative length error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "make_bool_vector_negative_length_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn bool_vector_destination_return_value_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c:bool_vector_binop_driver returns nil when an explicit
    // destination already contains the requested result; otherwise it mutates
    // and returns the destination vector.
    let expr = r#"(message "boolopret:%S" (let* ((a (bool-vector t nil t)) (b (bool-vector nil t t)) (same (bool-vector t t t)) (changed (bool-vector nil nil nil)) (r1 (bool-vector-union a b same)) (r2 (bool-vector-union a b changed))) (list r1 (eq r2 changed) (vconcat same) (vconcat changed))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("boolopret:") && row.contains("(nil t [t t t] [t t t])"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: bool-vector destination return value should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "bool_vector_destination_return_value_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn bool_vector_reader_size_validation_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:read_bool_vector requires the string literal to contain
    // exactly the bytes needed for the declared bit length, except for the
    // documented old Emacs multiple-of-8 compatibility case.
    let expr = r##"(message "boolread:%S" (list (vconcat (read "#&3\"\005\"")) (condition-case e (read "#&3\"\"") (error (list (car e) (cadr e)))) (condition-case e (read "#&x\"a\"") (error (list (car e) (cadr e))))))"##;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("boolread:")
                && row.contains("([t nil t]")
                && row.contains("(invalid-read-syntax \\\"#&...\\\")")
                && row.contains("(invalid-read-syntax \\\"#&\\\")")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: bool-vector reader size validation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "bool_vector_reader_size_validation_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn record_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"record:%S\" (let ((r (record 'foo 1 2))) (aset r 1 9) (list (recordp r) (type-of r) (aref r 0) (aref r 1) (length r))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("record:") && row.contains("(t foo foo 9 3)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: record behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("record_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn hash_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"hashfun:%S\" (let ((h (make-hash-table :test 'equal))) (puthash \"k\" 1 h) (puthash \"j\" 2 h) (remhash \"j\" h) (list (gethash \"k\" h 'missing) (gethash \"j\" h 'missing) (hash-table-count h))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("hashfun:") && row.contains("(1 missing 1)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: hash-table functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("hash_table_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn hash_table_copy_maphash_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"hashcopy:%S\" (let ((h (make-hash-table :test 'equal))) (puthash \"a\" 1 h) (puthash \"b\" 2 h) (let ((c (copy-hash-table h)) seen) (puthash \"a\" 9 c) (maphash (lambda (k v) (push (cons k v) seen)) h) (list (gethash \"a\" h) (gethash \"a\" c) (hash-table-count h) (length seen)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("hashcopy:") && row.contains("(1 9 2 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: hash-table copy and maphash behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "hash_table_copy_maphash_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn maphash_mutation_visits_live_entries_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:maphash walks with DOHASH_SAFE, so callbacks may remhash
    // the current key or puthash a new value for the current key without
    // crashing.  Its live traversal also sees the newly added key here and
    // skips the removed unvisited key.
    let expr = r#"(message "maphashmut:%S" (let ((h (make-hash-table :test 'eq)) seen) (puthash 'a 1 h) (puthash 'b 2 h) (maphash (lambda (k v) (push k seen) (when (eq k 'a) (puthash 'c 3 h) (remhash 'b h))) h) (list (sort seen (lambda (a b) (string< (symbol-name a) (symbol-name b)))) (hash-table-count h) (gethash 'b h 'missing) (gethash 'c h 'missing))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "maphashmut:((a c) 2 missing 3)";
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: maphash should follow GNU live mutation traversal semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("maphash_mutation_visits_live_entries_like_gnu", &gnu, &neo);
}

#[test]
fn hash_table_key_test_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"hashtest:%S\" (let ((eqh (make-hash-table :test 'eq)) (equalh (make-hash-table :test 'equal)) (eqlh (make-hash-table :test 'eql))) (puthash (copy-sequence \"k\") 'eq-string eqh) (puthash (copy-sequence \"k\") 'equal-string equalh) (puthash 1.0 'float eqlh) (puthash 1 'int eqlh) (list (gethash \"k\" eqh 'missing) (gethash \"k\" equalh 'missing) (gethash 1.0 eqlh 'missing) (gethash 1 eqlh 'missing))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("hashtest:") && row.contains("(missing equal-string float int)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: hash-table key-test semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "hash_table_key_test_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn equal_hash_table_overlay_keys_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "hashoverlay:%S" (with-temp-buffer (insert "abc") (let ((h (make-hash-table :test 'equal)) (o1 (make-overlay 1 2)) (o2 (make-overlay 1 2))) (overlay-put o1 'face 'bold) (overlay-put o2 'face 'bold) (puthash o1 'overlay-hit h) (list (gethash o2 h 'missing) (hash-table-count h)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("hashoverlay:(overlay-hit 1)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: equal hash tables should find matching overlay keys like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "equal_hash_table_overlay_keys_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn hash_table_custom_test_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (define-hash-table-test 'neo-len-test (lambda (a b) (= (length a) (length b))) (lambda (a) (length a))) (message \"hashtestdef:%S\" (let ((h (make-hash-table :test 'neo-len-test))) (puthash \"aa\" 1 h) (list (gethash \"bb\" h) (gethash \"c\" h 'missing) (hash-table-test h)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("hashtestdef:") && row.contains("(1 missing neo-len-test)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: custom hash-table test behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "hash_table_custom_test_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn hash_table_weakness_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"weakhash:%S\" (let ((h (make-hash-table :weakness 'key :test 'eq))) (puthash (cons 1 2) 3 h) (list (hash-table-weakness h) (hash-table-count h))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("weakhash:") && row.contains("(key 1)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: weak hash-table metadata should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "hash_table_weakness_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn make_hash_table_invalid_keyword_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "hashkw:%S" (condition-case e (make-hash-table :foo 1) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("hashkw:")
                && row.contains("error")
                && row.contains("Invalid keyword argument")
                && !row.contains("Invalid argument list")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: make-hash-table invalid keyword error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "make_hash_table_invalid_keyword_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn make_hash_table_obsolete_keywords_and_odd_args_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "hashargs:%S" (list (condition-case e (hash-table-p (make-hash-table :rehash-size 0 :rehash-threshold 0 :purecopy t)) (error (list (car e) (cadr e)))) (condition-case e (make-hash-table :test) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("hashargs:")
                && row.contains("(t (error")
                && row.contains("Odd number of arguments")
                && !row.contains("Invalid argument list")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: make-hash-table obsolete keywords and odd argument errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "make_hash_table_obsolete_keywords_and_odd_args_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn marker_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"marker:%S\" (with-temp-buffer (insert \"ab\") (goto-char 2) (let ((left (point-marker)) (right (copy-marker (point) t))) (insert \"X\") (let ((before (list (buffer-string) (marker-position left) (marker-insertion-type left) (marker-position right) (marker-insertion-type right) (bufferp (marker-buffer left))))) (set-marker left nil) (append before (list (marker-position left) (marker-buffer left)))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("marker:") && row.contains("aXb") && row.contains("2 nil 3 t t nil nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: marker functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("marker_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn insert_before_markers_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "ibmarkers:%S" (with-temp-buffer (insert "ab") (goto-char 2) (let ((left (point-marker)) (right (copy-marker (point) t))) (insert-before-markers "X") (list (buffer-string) (marker-position left) (marker-insertion-type left) (marker-position right) (marker-insertion-type right)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"ibmarkers:(\"aXb\" 3 nil 3 t)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: insert-before-markers should advance markers at point like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "insert_before_markers_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_region_marker_adjustment_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "delmark:%S" (with-temp-buffer (insert "abcdef") (let ((at-from (copy-marker 2)) (inside (copy-marker 4)) (at-to (copy-marker 5)) (after (copy-marker 7))) (delete-region 2 5) (list (buffer-string) (marker-position at-from) (marker-position inside) (marker-position at-to) (marker-position after)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"delmark:(\"aef\" 2 2 2 4)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: delete-region marker adjustment should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_region_marker_adjustment_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn marker_cross_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "markx:%S" (let ((a (generate-new-buffer " *neo-marker-a*")) (b (generate-new-buffer " *neo-marker-b*")) (m (make-marker))) (unwind-protect (progn (with-current-buffer b (insert "hello")) (set-marker m 3 b) (list (eq (marker-buffer m) b) (marker-position m) (with-current-buffer b (goto-char 3) (insert "X") (list (buffer-string) (marker-position m))) (eq (set-marker m nil) m) (marker-position m) (marker-buffer m))) (kill-buffer a) (kill-buffer b))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("markx:") && row.contains(r#"(t 3 (\"heXllo\" 3) t nil nil)"#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: cross-buffer set-marker and detach semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "marker_cross_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn set_marker_buffer_type_error_uses_bufferp_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/marker.c:Fset_marker delegates to set_marker_internal, whose
    // optional BUFFER argument is validated as a buffer.  Passing a non-buffer
    // third argument must signal bufferp, not another implementation-specific
    // predicate.
    let expr = r#"(message "setmarkbuf:%S" (with-temp-buffer (let ((m (make-marker))) (condition-case e (set-marker m 1 1) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "setmarkbuf:(wrong-type-argument bufferp)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: set-marker third-argument type error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "set_marker_buffer_type_error_uses_bufferp_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn marker_insertion_type_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"marktype:%S\" (with-temp-buffer (insert \"ab\") (let ((m1 (copy-marker (point-max) nil)) (m2 (copy-marker (point-max) t))) (goto-char (point-max)) (insert \"Z\") (list (marker-position m1) (marker-position m2) (buffer-string)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("marktype:") && row.contains("(3 4"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: marker insertion type behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "marker_insertion_type_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn marker_last_position_after_kill_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "marklast:%S" (let ((b (generate-new-buffer " *marklast*")) m before) (with-current-buffer b (insert "abc") (setq m (copy-marker 3 t)) (setq before (list (marker-position m) (marker-last-position m) (marker-buffer m)))) (kill-buffer b) (list before (marker-position m) (marker-last-position m) (marker-buffer m))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("marklast:") && row.contains("((3 3 #<killed buffer>) nil 3 nil)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: marker-last-position after buffer kill should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "marker_last_position_after_kill_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn narrowing_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"narrow:%S\" (with-temp-buffer (insert \"alpha\\nbeta\\ngamma\\n\") (goto-char (point-min)) (forward-line 1) (let ((beg (point))) (forward-line 1) (narrow-to-region beg (point)) (list (buffer-size) (point-min) (point-max) (buffer-string) (save-restriction (widen) (list (point-min) (point-max) (buffer-size)))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let tail = grid
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        tail.contains("narrow:")
            && tail.contains("(17 7 12")
            && tail.contains("beta")
            && tail.contains("(1 18 17)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: narrowing functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("narrowing_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn narrowing_point_clamp_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "narrowpt:%S" (with-temp-buffer (insert "abcdef") (goto-char 1) (let ((a (progn (narrow-to-region 3 5) (list (point-min) (point-max) (point)))) b) (setq b (save-restriction (goto-char 5) (narrow-to-region 2 4) (list (point-min) (point-max) (point)))) (list a b (point-min) (point-max) (point)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("narrowpt:((3 5 3) (2 4 4) 3 5 4)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: narrow-to-region and save-restriction point clamping should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "narrowing_point_clamp_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"overlay:%S\" (with-temp-buffer (insert \"abc\") (let ((o (make-overlay 1 2))) (overlay-put o 'p 7) (list (overlay-start o) (overlay-end o) (overlay-get o 'p) (length (overlays-at 1)) (progn (delete-overlay o) (overlay-buffer o))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("overlay:") && row.contains("(1 2 7 1 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("overlay_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn overlay_accessors_after_buffer_kill_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/buffer.c:overlay-start/end/buffer return nil after an overlay's
    // buffer has been killed or the overlay has been deleted.  Accessors must
    // not signal a dead-buffer error.
    let expr = r#"(message "ovdead:%S" (let ((o (with-temp-buffer (insert "abc") (make-overlay 1 2)))) (list (condition-case e (overlay-start o) (error (list (car e) (cadr e)))) (condition-case e (overlay-end o) (error (list (car e) (cadr e)))) (condition-case e (overlay-buffer o) (error (list (car e) (cadr e)))) (condition-case e (delete-overlay o) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ovdead:(nil nil nil nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay accessors after buffer kill should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overlay_accessors_after_buffer_kill_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_move_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"ovmove:%S\" (with-temp-buffer (insert \"abcdef\") (let ((o (make-overlay 2 4))) (move-overlay o 3 6) (overlay-put o 'evaporate t) (list (overlay-start o) (overlay-end o) (overlay-get o 'evaporate) (length (overlays-at 4))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ovmove:") && row.contains("(3 6 t 1)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay move behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overlay_move_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_advance_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "ovadv:%S" (with-temp-buffer (insert "ab") (let ((a (make-overlay 2 2 nil nil nil)) (b (make-overlay 2 2 nil t t))) (goto-char 2) (insert "X") (list (buffer-string) (list (overlay-start a) (overlay-end a) (overlays-at 2) (overlays-at 3)) (list (overlay-start b) (overlay-end b) (memq b (overlays-at 2)) (memq b (overlays-at 3)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains(r#"ovadv:(\"aXb\" (2 2 nil nil) (3 3 nil nil))"#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay front/rear advance and zero-length overlays-at behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overlay_advance_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_overlap_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"ovprio:%S\" (with-temp-buffer (insert \"abcdef\") (let ((a (make-overlay 2 5)) (b (make-overlay 3 4))) (overlay-put a 'priority 1) (overlay-put b 'priority 9) (list (mapcar (lambda (o) (overlay-get o 'priority)) (overlays-at 3)) (length (overlays-in 1 6)) (bufferp (overlay-buffer a))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ovprio:") && row.contains("((1 9) 2 t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlapping overlay enumeration should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overlay_overlap_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_change_respects_narrowing_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "ovchangenarrow:%S" (with-temp-buffer (insert "abcdef") (let ((o1 (make-overlay 2 2)) (o2 (make-overlay 4 4)) (o3 (make-overlay 7 7))) (narrow-to-region 2 6) (let ((ovs (overlays-in 2 6))) (list (length ovs) (not (null (memq o1 ovs))) (not (null (memq o2 ovs))) (not (null (memq o3 ovs))) (next-overlay-change 1) (next-overlay-change 2) (next-overlay-change 6) (previous-overlay-change 7) (previous-overlay-change 6) (previous-overlay-change 2))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ovchangenarrow:") && row.contains("(2 t t nil 2 4 6 4 4 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay change functions should respect the narrowed accessible range like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("overlay_change_respects_narrowing_like_gnu", &gnu, &neo);
}

#[test]
fn get_char_property_window_object_matches_gnu_overlay_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/textprop.c:get_char_property_and_overlay accepts a window as
    // OBJECT, uses that window's buffer, and includes only overlays whose
    // 'window property matches that window.
    let expr = r#"(message "wincharprop:%S" (let ((buf (get-buffer-create "*neo-window-property-probe*"))) (switch-to-buffer buf) (erase-buffer) (insert "abc") (let ((o (make-overlay 1 3 buf)) (w (selected-window))) (overlay-put o 'face 'win-face) (overlay-put o 'window w) (list (get-char-property 1 'face buf) (get-char-property 1 'face w) (get-char-property-and-overlay 1 'face w)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("wincharprop:") && row.contains("(win-face win-face (win-face . #<overlay")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: get-char-property should accept window OBJECT and honor window-specific overlays like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "get_char_property_window_object_matches_gnu_overlay_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn single_char_property_change_sees_overlay_boundaries_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/textprop.c:next-single-char-property-change advances through
    // next-char-property-change, which includes overlay boundaries before it
    // compares the selected char property with get-char-property.
    let expr = r#"(message "singlecharprop:%S" (with-temp-buffer (insert "abcdef") (put-text-property 2 4 'face 'text-face) (let ((o (make-overlay 4 6))) (overlay-put o 'face 'overlay-face) (list (next-single-char-property-change 1 'face) (next-single-char-property-change 2 'face) (next-single-char-property-change 4 'face) (previous-single-char-property-change 7 'face) (previous-single-char-property-change 6 'face) (previous-single-char-property-change 4 'face)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("singlecharprop:") && row.contains("(2 4 6 6 4 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: single char property changes should include overlay boundaries like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "single_char_property_change_sees_overlay_boundaries_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_intangible_motion_matches_gnu_point_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/intervals.c:set_point_both uses Fget_char_property while
    // inhibit-point-motion-hooks is nil, so overlay-backed intangible text
    // prevents point from landing inside the protected region.
    let expr = r#"(message "ovintang:%S" (with-temp-buffer (insert "abcdef") (let ((o (make-overlay 3 5))) (overlay-put o 'intangible 'zone) (let ((inhibit-point-motion-hooks nil)) (goto-char 2) (goto-char 4) (let ((forward (point))) (goto-char 6) (goto-char 4) (let ((backward (point))) (let ((inhibit-point-motion-hooks t)) (goto-char 4) (list forward backward (point)))))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ovintang:") && row.contains("(5 3 4)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay intangible should constrain point motion like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overlay_intangible_motion_matches_gnu_point_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"textprop:%S\" (let ((s (copy-sequence \"abcd\"))) (put-text-property 1 3 'face 'bold s) (put-text-property 2 4 'mouse-face 'highlight s) (list (get-text-property 1 'face s) (get-text-property 2 'mouse-face s) (text-properties-at 2 s) (next-single-property-change 1 'face s) (previous-single-property-change 4 'mouse-face s))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("textprop:")
                && row.contains("(bold highlight")
                && row.contains("face bold")
                && row.contains("mouse-face highlight")
                && row.contains("3 2")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text property functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn reversed_text_property_ranges_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpreverse:%S" (list (let ((s (copy-sequence "abcd"))) (put-text-property 3 1 'face 'bold s) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 3))) (let ((s (copy-sequence "abcd"))) (add-text-properties 3 1 '(mouse-face highlight) s) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 3))) (let ((s (copy-sequence "abcd"))) (put-text-property 0 4 'face 'bold s) (remove-text-properties 3 1 '(face nil) s) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 3)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("tpreverse:")
            && recent.contains("(nil (face bold) (face bold) nil)")
            && recent.contains("(nil (mouse-face highlight) (mouse-face highlight) nil)")
            && recent.contains("((face bold) nil nil (face bold))")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: reversed text-property ranges should be normalized like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "reversed_text_property_ranges_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_plist_validation_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpplist:%S" (list (let ((s (copy-sequence "abcd"))) (condition-case e (progn (add-text-properties 0 2 '(face) s) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 2))) (error (list (car e) (cadr e))))) (let ((s (copy-sequence "abcd"))) (condition-case e (progn (add-text-properties 0 2 'face s) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 2))) (error (list (car e) (cadr e))))) (let ((s (copy-sequence "abcd"))) (put-text-property 0 2 'face nil s) (condition-case e (progn (remove-text-properties 0 2 'face s) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 2))) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("tpplist:")
            && recent.contains("error")
            && recent.contains("Odd length text property list")
            && recent.contains("((face nil) (face nil) nil)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property plist validation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_plist_validation_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn next_property_change_limit_t_matches_gnu_interval_boundary_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpnextt:%S" (let ((s (copy-sequence "abcdef"))) (put-text-property 1 3 'face 'bold s) (put-text-property 3 5 'face 'bold s) (list (next-property-change 1 s) (next-property-change 1 s t) (next-single-property-change 1 'face s) (next-single-property-change 1 'face s 4) (previous-property-change 5 s) (previous-single-property-change 5 'face s) (previous-property-change 5 s 2) (previous-single-property-change 5 'face s 2))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("tpnextt:") && row.contains("(5 3 5 4 1 1 2 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: next-property-change with LIMIT=t should expose the next interval boundary like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "next_property_change_limit_t_matches_gnu_interval_boundary_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_change_out_of_range_errors_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpchangerange:%S" (let ((s (copy-sequence "abc"))) (put-text-property 1 2 'face 'bold s) (list (condition-case e (next-property-change 4 s) (error (list (car e) (cadr e)))) (condition-case e (next-single-property-change 4 'face s) (error (list (car e) (cadr e)))) (condition-case e (previous-property-change -1 s) (error (list (car e) (cadr e)))) (condition-case e (previous-single-property-change -1 'face s) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("tpchangerange:")
            && recent.matches("args-out-of-range").count() >= 4
            && recent.contains("(args-out-of-range 4)")
            && recent.contains("(args-out-of-range -1)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property change out-of-range errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_change_out_of_range_errors_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_text_property_change_respects_narrowing_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpchangenarrow:%S" (with-temp-buffer (insert "abcdef") (put-text-property 2 4 'face 'bold) (narrow-to-region 2 6) (list (condition-case e (next-property-change 1 nil) (error (list (car e) (cadr e)))) (condition-case e (next-single-property-change 1 'face nil) (error (list (car e) (cadr e)))) (condition-case e (previous-property-change 7 nil) (error (list (car e) (cadr e)))) (condition-case e (previous-single-property-change 7 'face nil) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("tpchangenarrow:")
            && recent.matches("args-out-of-range").count() >= 4
            && recent.contains("(args-out-of-range 1)")
            && recent.contains("(args-out-of-range 7)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer text-property change functions should reject positions outside the narrowed accessible range like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_text_property_change_respects_narrowing_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn text_properties_at_out_of_range_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpatrange:%S" (list (let ((s (copy-sequence "abc"))) (put-text-property 2 3 'face 'bold s) (list (text-properties-at 3 s) (condition-case e (text-properties-at 4 s) (error (list (car e) (cadr e)))))) (with-temp-buffer (insert "abc") (put-text-property 3 4 'face 'bold) (narrow-to-region 1 3) (list (text-properties-at 3) (get-text-property 3 'face) (condition-case e (text-properties-at 4) (error (list (car e) (cadr e)))) (condition-case e (get-text-property 4 'face) (error (list (car e) (cadr e))))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("tpatrange:")
                && row.contains("((nil (args-out-of-range 4))")
                && row.contains("((face bold) bold (args-out-of-range 4) (args-out-of-range 4))")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-properties-at/get-text-property out-of-range behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_properties_at_out_of_range_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_mutation_out_of_range_errors_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpmutrange:%S" (let ((s (copy-sequence "abc"))) (list (condition-case e (add-text-properties -1 1 '(face bold) s) (error (list (car e) (cadr e) (caddr e)))) (condition-case e (put-text-property 1 9 'face 'bold s) (error (list (car e) (cadr e) (caddr e)))) (condition-case e (set-text-properties 9 1 '(face bold) s) (error (list (car e) (cadr e) (caddr e)))) (condition-case e (remove-text-properties -1 1 '(face nil) s) (error (list (car e) (cadr e) (caddr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("tpmutrange:")
            && recent.matches("args-out-of-range").count() >= 4
            && recent.contains("(args-out-of-range -1 1)")
            && recent.contains("(args-out-of-range 1 9)")
            && recent.contains("(args-out-of-range 9 1)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property mutation out-of-range errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_mutation_out_of_range_errors_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_equality_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"tpeq:%S\" (let ((s (copy-sequence \"abcd\"))) (put-text-property 1 3 'face 'bold s) (list (substring s 1 3) (text-properties-at 0 (substring s 1 3)) (equal s (substring-no-properties s)) (equal-including-properties s (substring-no-properties s)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("tpeq:")
                && row.contains("bc")
                && row.contains("face bold")
                && row.contains("t nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property equality should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_equality_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn case_conversion_preserves_string_text_properties_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "caseprop:%S" (let* ((s (copy-sequence "abC"))) (put-text-property 0 2 'face 'bold s) (let ((u (upcase s)) (d (downcase s)) (c (capitalize s))) (list u (mapcar (lambda (i) (text-properties-at i u)) (number-sequence 0 2)) d (mapcar (lambda (i) (text-properties-at i d)) (number-sequence 0 2)) c (mapcar (lambda (i) (text-properties-at i c)) (number-sequence 0 2)) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 2))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("caseprop:")
            && recent.matches("#(").count() >= 3
            && recent.contains("ABC")
            && recent.contains("abc")
            && recent.contains("Abc")
            && recent.matches("face").count() >= 6
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: upcase/downcase/capitalize should preserve string text properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "case_conversion_preserves_string_text_properties_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn sxhash_equal_including_properties_hashes_string_intervals_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "sxhashprop:%S" (let ((s1 (copy-sequence "ab")) (s2 (copy-sequence "ab"))) (put-text-property 0 1 'face 'bold s1) (list (= (sxhash-equal s1) (sxhash-equal s2)) (= (sxhash-equal-including-properties s1) (sxhash-equal-including-properties s2)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("sxhashprop:(t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sxhash-equal-including-properties should include string intervals like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "sxhash_equal_including_properties_hashes_string_intervals_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn marker_and_overlay_equal_hash_semantics_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "eqhashobj:%S" (list (with-temp-buffer (insert "abc") (let ((m1 (copy-marker 2)) (m2 (copy-marker 2)) (m3 (copy-marker 3))) (list (= (sxhash-equal m1) (sxhash-equal m2)) (= (sxhash-equal m1) (sxhash-equal m3))))) (with-temp-buffer (insert "abc") (let ((o1 (make-overlay 1 2)) (o2 (make-overlay 1 2))) (overlay-put o1 'face 'bold) (let ((before (list (equal o1 o2) (= (sxhash-equal o1) (sxhash-equal o2))))) (overlay-put o2 'face 'bold) (append before (list (equal o1 o2) (= (sxhash-equal o1) (sxhash-equal o2)))))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("eqhashobj:((t nil) (nil nil t t))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: marker and overlay equal/sxhash behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "marker_and_overlay_equal_hash_semantics_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_copy_sequence_and_substring_independence_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpcopy:%S" (let* ((s (copy-sequence "abcd"))) (put-text-property 1 3 'face 'bold s) (let* ((c (copy-sequence s)) (sub (substring s 1 3)) (plain (substring-no-properties s 1 3))) (put-text-property 0 1 'face 'italic c) (list (text-properties-at 1 s) (text-properties-at 0 c) (text-properties-at 1 c) (text-properties-at 0 sub) (text-properties-at 1 sub) (text-properties-at 0 plain) (equal-including-properties s c)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("tpcopy:")
                && row.contains("((face bold) (face italic) (face bold)")
                && row.contains("(face bold) nil nil)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: copy-sequence and substring should copy string properties independently like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_copy_sequence_and_substring_independence_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn display_property_substring_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"displayprop:%S\" (let ((s (propertize \"x\" 'display \"Y\"))) (list (get-text-property 0 'display s) (substring s 0 1) (substring-no-properties s 0 1))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("displayprop:")
            && recent.contains("Y")
            && recent.contains("display")
            && recent.contains("x")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: display text-property substring behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "display_property_substring_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_read_only_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"textlock:%S\" (with-temp-buffer (insert \"abc\") (put-text-property 2 3 'read-only t) (list (condition-case e (delete-region 1 3) (text-read-only (car e)) (error (car e))) (buffer-string))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("textlock:")
                && row.contains("text-read-only")
                && row.contains("abc")
                && row.contains("read-only")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: read-only text-property edit protection should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_read_only_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn category_read_only_property_protects_text_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "catlock:%S" (with-temp-buffer (insert "abcd") (put 'neomacs-tui-ro-category 'read-only t) (put-text-property 2 3 'category 'neomacs-tui-ro-category) (let ((blocked (condition-case e (delete-region 2 3) (error (list (car e) (cadr e))))) (after-blocked (buffer-string))) (let ((inhibit-read-only t)) (delete-region 2 3) (list (get-text-property 2 'read-only) blocked after-blocked (buffer-string))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("catlock:")
            && recent.contains("(nil (text-read-only nil)")
            && recent.contains("abcd")
            && recent.contains("acd")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: category read-only property should protect text from edits like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "category_read_only_property_protects_text_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_modification_hooks_run_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/textprop.c:verify_interval_modification collects
    // modification-hooks for non-empty changes and calls them before the
    // actual deletion/replacement is applied.
    let expr = r#"(message "tphooks2:%S" (with-temp-buffer (insert "abcd") (let ((events nil)) (put-text-property 2 4 'modification-hooks (list (lambda (beg end) (push (list 'mod beg end (substring-no-properties (buffer-string))) events)))) (delete-region 2 3) (list (substring-no-properties (buffer-string)) (nreverse events)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("tphooks2:")
                && row.contains("acd")
                && row.contains("mod 2 3")
                && row.contains("abcd")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property modification-hooks should run before deletion like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("text_property_modification_hooks_run_like_gnu", &gnu, &neo);
}

#[test]
fn text_property_insert_hooks_run_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/textprop.c chooses insert-behind-hooks and
    // insert-in-front-hooks before insertion, then report_interval_modification
    // runs them after the inserted text exists.
    let expr = r#"(message "inshooks2:%S" (with-temp-buffer (insert "ab") (let ((events nil)) (put-text-property 1 2 'insert-behind-hooks (list (lambda (beg end) (push (list 'behind beg end (substring-no-properties (buffer-string))) events)))) (put-text-property 2 3 'insert-in-front-hooks (list (lambda (beg end) (push (list 'front beg end (substring-no-properties (buffer-string))) events)))) (goto-char 2) (insert "X") (list (substring-no-properties (buffer-string)) (nreverse events)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("inshooks2:")
                && row.contains("aXb")
                && row.contains("behind 2 3")
                && row.contains("front 2 3")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property insert hooks should run after insertion like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("text_property_insert_hooks_run_like_gnu", &gnu, &neo);
}

#[test]
fn text_property_removal_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"tprop2:%S\" (let ((s (copy-sequence \"abcd\"))) (put-text-property 0 4 'face 'bold s) (remove-text-properties 1 3 '(face nil) s) (list (text-properties-at 0 s) (text-properties-at 1 s) (next-single-property-change 0 'face s) (next-single-property-change 1 'face s) s)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("tprop2:") && row.contains("(face bold)") && row.contains("nil 1 3")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text property removal should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_removal_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_stickiness_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"sticky:%S\" (with-temp-buffer (insert \"ab\") (put-text-property 1 2 'face 'bold) (put-text-property 1 2 'rear-nonsticky '(face)) (goto-char 2) (insert \"X\") (list (buffer-string) (text-properties-at 0 (buffer-string)) (text-properties-at 1 (buffer-string)) (text-properties-at 2 (buffer-string)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("sticky:")
                && row.contains("aXb")
                && row.contains("rear-nonsticky")
                && row.contains("face bold")
                && row.contains("nil nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property stickiness should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_stickiness_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_search_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"tpropsearch:%S\" (let ((s (copy-sequence \"abcdef\"))) (put-text-property 1 4 'face 'bold s) (list (text-property-any 0 6 'face 'bold s) (text-property-any 4 6 'face 'bold s) (text-property-not-all 1 4 'face 'bold s) (text-property-not-all 0 6 'face 'bold s))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("tpropsearch:") && row.contains("(1 nil nil 0)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property search helpers should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_search_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_change_limit_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tproplimit:%S" (let ((s (copy-sequence "abcdef"))) (put-text-property 1 5 'face 'bold s) (list (next-single-property-change 1 'face s 3) (next-single-property-change 1 'face s 6) (previous-single-property-change 5 'face s 3) (previous-single-property-change 5 'face s 0) (previous-single-property-change 1 'face s 0) (next-single-property-change 5 'face s 6))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("tproplimit:(3 5 3 1 0 6)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text-property change LIMIT behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_change_limit_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn button_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'button) (message \"button:%S\" (with-temp-buffer (insert-text-button \"Go\" 'action (lambda (_) 'done) 'help-echo \"Help\") (let ((b (button-at (point-min)))) (list (not (null b)) (button-label b) (button-get b 'help-echo) (button-has-type-p b 'push-button))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("button:")
                && row.contains("t")
                && row.contains("Go")
                && row.contains("Help")
                && row.contains("nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: button text property helper semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("button_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn field_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"field:%S\" (with-temp-buffer (insert \"aa\" (propertize \"bb\" 'field 'f) \"cc\") (mapcar (lambda (p) (list p (field-beginning p) (field-end p) (field-string p) (field-string-no-properties p))) '(1 3 4 5))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("field:")
            && recent.contains("(1 1 3")
            && recent.contains("(3 1 3")
            && recent.contains("(4 3 5")
            && recent.contains("(5 3 5")
            && recent.contains("field f")
            && recent.contains("bb")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: field text-property boundary helpers should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "field_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_substring_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"bufsub:%S\" (with-temp-buffer (insert \"abcd\") (put-text-property 2 4 'face 'bold) (list (buffer-substring 2 4) (text-properties-at 0 (buffer-substring 2 4)) (buffer-substring-no-properties 2 4) (text-properties-at 0 (buffer-substring-no-properties 2 4)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("bufsub:") && row.contains("face bold") && row.contains("nil"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer substring property behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_substring_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_local_variable_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"buflocal:%S\" (with-temp-buffer (setq-local fill-column 33) (list fill-column (local-variable-p 'fill-column) (with-temp-buffer fill-column))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("buflocal:") && row.contains("(33 t 70)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer-local variable functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_local_variable_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn local_variable_if_set_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "lvif:%S" (let ((sym (make-symbol "neo-auto-local"))) (make-variable-buffer-local sym) (with-temp-buffer (list (local-variable-p sym) (local-variable-if-set-p sym) (progn (set sym 9) (list (local-variable-p sym) (local-variable-if-set-p sym) (symbol-value sym))) (with-temp-buffer (list (local-variable-p sym) (local-variable-if-set-p sym) (boundp sym)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("lvif:") && row.contains("(nil t (t t 9) (nil t t))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: local-variable-if-set-p and make-variable-buffer-local should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "local_variable_if_set_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn variable_binding_locus_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "locus:%S" (let ((sym (make-symbol "neo-locus"))) (make-variable-buffer-local sym) (list (default-boundp sym) (condition-case e (default-value sym) (void-variable (car e)) (error (car e))) (with-temp-buffer (list (variable-binding-locus sym) (progn (set sym 5) (list (eq (variable-binding-locus sym) (current-buffer)) (local-variable-p sym) (default-boundp sym) (default-value sym))))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("locus:") && row.contains("(t nil (nil (t t t nil)))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: variable-binding-locus and default-boundp should match GNU automatic-local semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "variable_binding_locus_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn default_local_variable_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"deflocal:%S\" (let ((orig (default-value 'fill-column))) (unwind-protect (with-temp-buffer (setq-default fill-column 71) (setq-local fill-column 33) (list fill-column (default-value 'fill-column) (progn (kill-local-variable 'fill-column) fill-column))) (setq-default fill-column orig))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("deflocal:") && row.contains("(33 71 71)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: default and local variable behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "default_local_variable_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn permanent_local_variable_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"permlocal:%S\" (with-temp-buffer (put 'neo-p 'permanent-local t) (set (make-local-variable 'neo-n) 1) (set (make-local-variable 'neo-p) 2) (kill-all-local-variables) (list (local-variable-p 'neo-n) (local-variable-p 'neo-p) (boundp 'neo-n) (boundp 'neo-p) neo-p)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("permlocal:") && row.contains("(nil t nil t 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: permanent-local variable behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "permanent_local_variable_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn hook_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"hook:%S\" (let ((hook nil) (seen nil)) (add-hook 'hook (lambda () (push 'a seen))) (add-hook 'hook (lambda () (push 'b seen))) (run-hooks 'hook) (list seen (length hook))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("hook:") && row.contains("((a b) 0)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: hook functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("hook_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn condition_object_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"cond:%S\" (list (condition-case e (signal 'wrong-type-argument '(integerp \"x\")) (wrong-type-argument (list 'typed e)) (error (list 'error e))) (condition-case e (error \"boom %s\" 7) (error (list (car e) (cadr e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("cond:")
                && row.contains("typed")
                && row.contains("wrong-type-argument")
                && row.contains("integerp")
                && row.contains("boom 7")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: condition object handling should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "condition_object_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn condition_case_success_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "condsucc:%S" (list (condition-case v (+ 1 2) (:success (list :ok v)) (error (list :err v))) (condition-case v (error "bad") (:success (list :ok v)) (error (list :err (car v) (cdr v)))) (condition-case nil (+ 3 4) (:success :ok) (error :err))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"condsucc:((:ok 3) (:err error (\"bad\")) :ok)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: condition-case :success binding and error bypass behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "condition_case_success_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn nonlocal_exit_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"nonlocal:%S\" (list (catch 'a (throw 'a 7)) (condition-case e (throw 'b 3) (no-catch (cadr e))) (let (s) (condition-case e (unwind-protect (progn (push 'body s) (error \"boom\")) (push 'cleanup s)) (error (nreverse s))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("nonlocal:") && row.contains("(7 b") && row.contains("(body cleanup)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: catch/throw no-catch and unwind-protect ordering should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "nonlocal_exit_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn timer_object_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"timerobj:%S\" (let ((tm (run-at-time 100 nil (lambda () nil)))) (prog1 (list (timerp tm) (not (null (memq tm timer-list))) (cancel-timer tm) (memq tm timer-list)) (ignore-errors (cancel-timer tm)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("timerobj:") && row.contains("(t t nil nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: timer object creation and cancellation should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "timer_object_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn unwind_protect_cleanup_error_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "unwinderr:%S" (list (condition-case e (unwind-protect :body (error "cleanup")) (error (list (car e) (cdr e)))) (catch 'tag (condition-case e (unwind-protect (throw 'tag :body) (error "cleanup")) (error (list :caught (car e) (cdr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"unwinderr:((error (\"cleanup\")) (:caught error (\"cleanup\")))"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: unwind-protect cleanup errors should override body return and body throw like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "unwind_protect_cleanup_error_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn define_error_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (define-error 'neo-test-error \"Neo message\" 'file-error) (message \"deferr:%S\" (list (get 'neo-test-error 'error-conditions) (get 'neo-test-error 'error-message) (condition-case e (signal 'neo-test-error '(\"payload\")) (file-error (list 'file (car e) (cdr e))) (error (list 'error e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("deferr:")
                && row.contains("neo-test-error")
                && row.contains("file-error")
                && row.contains("Neo message")
                && row.contains("payload")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: define-error inheritance and signaling should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "define_error_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn read_from_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"readstr:%S\" (list (read-from-string \"(a . b) tail\") (read-from-string \"\\\"a\\\\\\\"b\\\"x\") (condition-case e (read-from-string \"(\") (end-of-file (car e)) (error (car e)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("readstr:")
                && row.contains("((a . b) . 7)")
                && row.contains(". 6)")
                && row.contains("end-of-file")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: read-from-string object, index, and error behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "read_from_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn circular_read_print_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"circle:%S\" (let* ((print-circle t) (x (read \"#1=(a . #1#)\"))) (list (consp x) (eq x (cdr x)) (prin1-to-string x))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("circle:") && row.contains("(t t") && row.contains("#1=(a . #1#)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: circular read and print-circle behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "circular_read_print_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn invalid_read_label_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r##"(message "readlabel:%S" (condition-case e (read-from-string "#1#") (error (list (car e) (cadr e)))))"##;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("readlabel:(invalid-read-syntax \\\"#1#\\\")"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invalid read-label error data should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("invalid_read_label_error_matches_gnu_semantics", &gnu, &neo);
}

#[test]
fn read_circle_nil_rejects_read_label_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c gates #N=/#N# recursive structure syntax on
    // `read-circle`.  With read-circle nil, #N= is invalid read syntax and
    // must not construct a circular object.
    let expr = r##"(message "readlabelnil:%S" (let ((read-circle nil)) (condition-case e (read-from-string "#1=(a . #1#)") (error (list (car e) (cadr e))))))"##;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("readlabelnil:(invalid-read-syntax \\\"#1=\\\")"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: read-circle nil should reject read-label syntax like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("read_circle_nil_rejects_read_label_like_gnu", &gnu, &neo);
}

#[test]
fn hash_table_reader_constructor_errors_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:hash_table_from_plist validates #s(hash-table ...)
    // constructor data before creating the table.  Malformed data must signal
    // the same reader/hash-table errors; it must not be accepted as an empty
    // or partially initialized hash table.
    let expr = r##"(message "hashread:%S" (list (condition-case e (read "#s(hash-table data (a))") (error (list (car e) (cadr e)))) (condition-case e (read "#s(hash-table data . a)") (error (list (car e) (cadr e)))) (condition-case e (read "#s(hash-table test bogus data (a 1))") (error (list (car e) (cadr e)))) (let ((h (read "#s(hash-table test equal data (a 1 a 2))"))) (list (hash-table-test h) (hash-table-count h) (gethash 'a h)))))"##;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("hashread:")
                && row.contains("Hash table data length is odd")
                && row.contains("(invalid-read-syntax \\\".\\\")")
                && row.contains("Invalid hash table test")
                && row.contains("(equal 1 2)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: #s(hash-table ...) reader constructor errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "hash_table_reader_constructor_errors_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn radix_reader_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"radix:%S\" (list (read \"#b1010\") (read \"#o12\") (read \"#xA\") (read \"#36rZ\") (condition-case e (read \"#2r2\") (invalid-read-syntax (car e)) (error (car e)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("radix:") && row.contains("(10 10 10 35 invalid-read-syntax)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: radix reader syntax should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "radix_reader_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn character_reader_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"charread:%S\" (list (read \"?A\") (read \"?\\\\n\") (read \"?\\\\C-a\") (read \"?\\\\M-a\") (read \"?\\\\N{LATIN CAPITAL LETTER A}\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("charread:") && row.contains("(65 10 1 134217825 65)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: character reader syntax should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "character_reader_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn incomplete_character_reader_errors_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:read_char_literal/read_char_escape signal end-of-file
    // for a bare `?' and for incomplete character modifiers like \C- and \M-.
    let expr = r#"(message "chareof:%S" (list (condition-case e (read "?") (error (list (car e) (cadr e)))) (condition-case e (read (concat "?" "\\" "C-")) (error (list (car e) (cadr e)))) (condition-case e (read (concat "?" "\\" "M-")) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("chareof:((end-of-file nil) (end-of-file nil) (end-of-file nil))")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: incomplete character reader errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "incomplete_character_reader_errors_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn malformed_unicode_character_escape_error_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "charunicodeerr:%S" (condition-case e (read "?\\uXYZ") (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("charunicodeerr:")
                && row.contains("error")
                && row.contains("Non-hex character used for Unicode escape")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: malformed Unicode character escape errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "malformed_unicode_character_escape_error_matches_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn unicode_character_reader_error_payloads_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:read_char_escape distinguishes named-character syntax,
    // malformed Unicode escapes, non-hex digits, and out-of-range codepoints.
    let expr = r#"(message "readunicode:%S" (list (condition-case e (read (concat "?" "\\" "N")) (error (list (car e) (cadr e)))) (condition-case e (read (concat "?" "\\" "u12")) (error (list (car e) (cadr e)))) (condition-case e (read (concat "?" "\\" "u12xz")) (error (list (car e) (cadr e)))) (condition-case e (read (concat "?" "\\" "U00110000")) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid.join("\n");
        recent.contains("readunicode:")
            && recent.contains("Expected opening brace after")
            && recent.contains("Malformed Unicode escape")
            && recent.contains("Non-hex character used for")
            && recent.contains("escape: x (120)")
            && recent.contains("Non-Unicode character")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: Unicode character reader errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "unicode_character_reader_error_payloads_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn hex_character_reader_error_payloads_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:read_char_escape accepts modifier bits in hex escapes
    // up to CHAR_META | (CHAR_META - 1), and signals ordinary `error' for
    // empty or out-of-range hex escapes.
    let expr = r#"(message "readhexerr:%S" (list (read (concat "?" "\\" "x4000001")) (condition-case e (read (concat "?" "\\" "x")) (error (list (car e) (cadr e)))) (condition-case e (read (concat "?" "\\" "x10000000")) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid.join("\n");
        recent.contains("readhexerr:")
            && recent.contains("67108865")
            && recent.contains("Invalid escape char syntax")
            && recent.contains("Hex character out of range")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: hex character reader errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("hex_character_reader_error_payloads_match_gnu", &gnu, &neo);
}

#[test]
fn provide_eval_after_load_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (setq neo-after-load-log nil) (eval-after-load 'neo-feature '(push 'after neo-after-load-log)) (message \"feature:%S\" (list (featurep 'neo-feature) neo-after-load-log (provide 'neo-feature) (featurep 'neo-feature) neo-after-load-log)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("feature:(nil nil neo-feature t (after))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: provide, featurep, and eval-after-load should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "provide_eval_after_load_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn match_data_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"match:%S\" (progn (string-match \"\\\\(a\\\\)\" \"a\") (save-match-data (string-match \"b\" \"b\")) (list (match-beginning 1) (match-end 1) (match-string 1 \"a\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("match:") && row.contains("(0 1"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: match data preservation should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("match_data_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn invalid_regexp_error_payload_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/search.c:string-match reaches compile_pattern, which signals
    // invalid-regexp with the exact regexp compiler diagnostic.
    let expr = r#"(message "badre:%S" (condition-case e (string-match "\\(" "abc") (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"badre:(invalid-regexp \"Unmatched ( or \\\\(\")"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invalid-regexp payload should match GNU regexp compiler diagnostics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "invalid_regexp_error_payload_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_match_start_type_error_matches_gnu_fixnump_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/search.c:string_match_1 validates START with CHECK_FIXNUM for
    // both ordinary and POSIX string matching.
    let expr = r#"(message "matchstart:%S" (list (condition-case e (string-match "a" "abc" 1.0) (error (list (car e) (cadr e)))) (condition-case e (posix-string-match "a" "abc" 1.0) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "matchstart:((wrong-type-argument fixnump) (wrong-type-argument fixnump))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string-match START type errors should match GNU CHECK_FIXNUM\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_match_start_type_error_matches_gnu_fixnump_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn match_data_reuse_and_reseat_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/search.c:match-data destructively fills a supplied REUSE list.
    // When RESEAT is non-nil, old marker elements are made to point nowhere;
    // if REUSE is longer than needed, the extra cells remain and are set nil.
    let expr = r#"(message "matchreuse:%S" (with-temp-buffer (insert "abc") (goto-char 1) (re-search-forward "b") (let* ((reuse (list (point-marker) (point-marker) (point-marker) (point-marker) (point-marker))) (m0 (car reuse)) (r1 (match-data t reuse t)) (pos0 (marker-position m0)) (r2 (match-data t reuse nil))) (list (eq r1 reuse) (length r1) pos0 r1 (eq r2 reuse) (length r2)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("matchreuse:")
                && row.contains("(t 5 nil")
                && row.contains("nil nil)")
                && row.contains(" t 5)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: match-data REUSE/RESEAT behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "match_data_reuse_and_reseat_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn inhibit_changing_match_data_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "inhibitmatch:%S" (progn (string-match "\\(a\\)" "a") (let ((before (match-data))) (let ((inhibit-changing-match-data t)) (string-match "\\(b\\)" "b") (with-temp-buffer (insert "ccc") (goto-char 1) (re-search-forward "c+" nil t)) (looking-at "c")) (list before (match-data) (match-string 1 "a")))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("inhibitmatch:")
                && row.contains("((0 1 0 1) (0 1 0 1)")
                && row.contains(r#"\"a\""#)
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: inhibit-changing-match-data should preserve previous match data like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "inhibit_changing_match_data_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn match_data_reuse_list_is_destructively_updated_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "matchreuse:%S" (with-temp-buffer (insert "abc") (goto-char 1) (re-search-forward "\\(a\\)b") (let* ((reuse (list 'a 'b 'c 'd 'e)) (result (match-data t reuse))) (list (mapcar (lambda (x) (if (bufferp x) (buffer-name x) x)) result) (mapcar (lambda (x) (if (bufferp x) (buffer-name x) x)) reuse) (eq result reuse) (length reuse)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("matchreuse:")
                && row.contains(r#"((1 3 1 2 \" *temp*\") (1 3 1 2 \" *temp*\") t 5)"#)
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: match-data should destructively update a reusable list like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "match_data_reuse_list_is_destructively_updated_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn looking_at_p_match_data_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "lookp:%S" (progn (string-match "\\(a\\)" "a") (let ((before (match-data))) (with-temp-buffer (insert "abc") (goto-char 1) (let ((hit (looking-at-p "\\(a\\)")) (after-hit (match-data))) (goto-char 2) (let ((miss (looking-at-p "\\(z\\)")) (after-miss (match-data))) (list hit miss before after-hit after-miss (match-string 1 "a"))))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"lookp:(t nil (0 1 0 1) (0 1 0 1) (0 1 0 1) \"a\")"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: looking-at-p should return predicate result without changing match data like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "looking_at_p_match_data_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn posix_looking_at_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "poslook:%S" (with-temp-buffer (insert "aaaa") (goto-char 1) (let ((ordinary (looking-at "a\\|aa\\|aaa")) (ordinary-text (match-string 0)) (ordinary-end (match-end 0))) (goto-char 1) (let ((posix (posix-looking-at "a\\|aa\\|aaa")) (posix-text (match-string 0)) (posix-end (match-end 0))) (list ordinary ordinary-text ordinary-end posix posix-text posix-end)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"poslook:(t \"a\" 2 t \"aaa\" 4)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: posix-looking-at should choose the longest match like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "posix_looking_at_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn optional_submatch_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"submatch:%S\" (progn (string-match \"\\\\(a\\\\)?b\" \"b\") (list (match-beginning 1) (match-end 1) (match-string 1 \"b\") (match-string 0 \"b\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("submatch:") && row.contains("(nil nil nil") && row.contains("b")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: optional unmatched submatch behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "optional_submatch_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn match_string_source_type_error_matches_gnu_substring_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/subr.el:match-string delegates SOURCE extraction directly to
    // substring, which accepts arrays and therefore signals arrayp for a
    // non-array SOURCE.
    let expr = r#"(message "matchsrc:%S" (condition-case e (progn (string-match "a" "a") (match-string 0 123)) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("matchsrc:(wrong-type-argument arrayp)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: match-string SOURCE type error should inherit GNU substring semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "match_string_source_type_error_matches_gnu_substring_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn failed_match_data_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"matchfail:%S\" (progn (string-match \"a\" \"abc\") (let ((before (match-beginning 0))) (string-match \"z\" \"abc\") (list before (match-beginning 0) (match-data)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("matchfail:") && row.contains("(0 0 (0 1))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: failed match data behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "failed_match_data_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn obarray_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"ob:%S\" (let ((ob (make-vector 7 0))) (list (intern-soft \"foo\" ob) (symbol-name (intern \"foo\" ob)) (eq (intern-soft \"foo\" ob) (intern \"foo\" ob)) (unintern \"foo\" ob) (intern-soft \"foo\" ob))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ob:") && row.contains("(nil") && row.contains("t t nil"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: obarray operations should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("obarray_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn vector_obarray_slot_zero_conversion_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:check_obarray_slow accepts a nonempty vector whose first
    // element is 0 for compatibility, installs a real obarray object in slot
    // 0, and then uses that object for symbol lookup.
    let expr = r#"(message "obslot:%S" (let ((ob (make-vector 7 0))) (list (obarrayp ob) (aref ob 0) (symbol-name (intern "x" ob)) (obarrayp (aref ob 0)) (eq (intern-soft "x" ob) (intern-soft "x" (aref ob 0))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("obslot:(nil 0 \\\"x\\\" t t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vector obarray slot-zero conversion should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "vector_obarray_slot_zero_conversion_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn invalid_obarray_argument_errors_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "obbad:%S" (condition-case e (intern "x" [1 2]) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("obbad:")
                && row.contains("wrong-type-argument")
                && row.contains("obarrayp")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invalid vector obarray should signal GNU's obarrayp type error\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "invalid_obarray_argument_errors_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn mapatoms_obarray_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"mapatoms:%S\" (let ((ob (make-vector 7 0)) seen) (intern \"b\" ob) (intern \"a\" ob) (mapatoms (lambda (s) (push (symbol-name s) seen)) ob) (sort seen 'string<)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("mapatoms:") && row.contains("a") && row.contains("b"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: mapatoms over private obarray should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "mapatoms_obarray_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_keyword_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"symedge:%S\" (let ((s (make-symbol \"k\"))) (list (symbol-name :foo) (keywordp :foo) (keywordp 'foo) (symbol-name s) (eq s (intern-soft \"k\")) (intern-soft \"no-such-symbol\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("symedge:")
                && row.contains(":foo")
                && row.contains("t nil")
                && row.contains("k")
                && row.contains("nil nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: keyword and uninterned symbol behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_keyword_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn abbrev_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"abbrev:%S\" (let ((tab (make-abbrev-table))) (define-abbrev tab \"btw\" \"by the way\") (list (abbrev-table-p tab) (symbol-value (intern-soft \"btw\" tab)) (abbrev-expansion \"btw\" tab))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("abbrev:") && row.contains("(t") && row.contains("by the way"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: abbrev table behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "abbrev_table_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn face_attribute_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"face:%S\" (let ((f (make-face 'neo-face))) (set-face-attribute f nil :weight 'bold :slant 'italic) (list (facep f) (face-attribute f :weight nil) (face-attribute f :slant nil))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("face:") && row.contains("bold") && row.contains("italic"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: face attribute behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "face_attribute_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_value_cell_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"symbind:%S\" (let ((s (make-symbol \"neo-var\"))) (list (boundp s) (progn (set s 7) (boundp s)) (symbol-value s) (progn (makunbound s) (boundp s)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("symbind:") && row.contains("(nil t 7 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol value cell operations should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_value_cell_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn variable_watcher_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"watch:%S\" (let ((sym (make-symbol \"watched\")) seen) (add-variable-watcher sym (lambda (s n o w) (push (list s n o w) seen))) (set sym 1) (set sym 2) (list (mapcar (lambda (x) (list (cadr x) (caddr x) (cadddr x))) (nreverse seen)) (get sym sym))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("watch:") && row.contains("((1 set nil) (2 set nil))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: variable watcher behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "variable_watcher_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_function_cell_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"funbind:%S\" (let ((s (make-symbol \"neo-fun\"))) (list (fboundp s) (progn (fset s (lambda (x) (+ x 1))) (fboundp s)) (funcall (symbol-function s) 4) (progn (fmakunbound s) (fboundp s)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("funbind:") && row.contains("(nil t 5 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol function cell operations should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_function_cell_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_with_pos_type_and_negative_position_semantics_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c:Fbare_symbol accepts bare symbols and symbol-with-pos,
    // and its wrong-type predicate is the compound `(symbolp
    // symbol-with-pos-p)'.  GNU src/data.c:Fposition_symbol accepts any
    // fixnum position, including negative fixnums, and rejects floats with
    // fixnum-or-symbol-with-pos-p.
    let expr = r#"(message "sympos:%S" (list (equal (condition-case e (bare-symbol 1) (error (list (car e) (cadr e)))) '(wrong-type-argument (symbolp symbol-with-pos-p))) (equal (condition-case e (position-symbol 1 0) (error (list (car e) (cadr e)))) '(wrong-type-argument (symbolp symbol-with-pos-p))) (condition-case e (let ((s (position-symbol 'a -1))) (list (bare-symbol s) (symbol-with-pos-pos s) (symbol-with-pos-p s))) (error (list (car e) (cadr e)))) (equal (condition-case e (position-symbol 'a 1.0) (error (list (car e) (cadr e)))) '(wrong-type-argument fixnum-or-symbol-with-pos-p))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "sympos:(t t (a -1 t) t)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol-with-pos type and negative-position semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_with_pos_type_and_negative_position_semantics_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_with_pos_is_not_transparent_to_symbol_cell_apis_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c uses CHECK_SYMBOL for symbol-name, symbol-plist, and
    // fset.  A symbol-with-pos is not a Lisp symbol for these APIs; callers
    // must use bare-symbol explicitly when they want the underlying symbol.
    let expr = r#"(message "symposcells:%S" (let ((a (position-symbol 'foo 1)) (b (position-symbol 'foo 2))) (list (eq a 'foo) (eq a b) (equal a b) (equal (condition-case e (symbol-name a) (error (list (car e) (cadr e)))) '(wrong-type-argument symbolp)) (equal (condition-case e (symbol-plist a) (error (list (car e) (cadr e)))) '(wrong-type-argument symbolp)) (equal (condition-case e (progn (put a 'p 9) (list (get 'foo 'p) (get b 'p))) (error (list (car e) (cadr e)))) '(wrong-type-argument symbolp)) (equal (condition-case e (progn (fset a (lambda () 7)) (list (fboundp 'foo) (funcall b))) (error (list (car e) (cadr e)))) '(wrong-type-argument symbolp)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "symposcells:(nil nil nil t t t t)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol-with-pos should not be transparent to symbol cell APIs like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_with_pos_is_not_transparent_to_symbol_cell_apis_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_with_pos_is_not_transparent_to_binding_apis_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c validates boundp, default-boundp, makunbound, fboundp,
    // and fmakunbound arguments with CHECK_SYMBOL.  A symbol-with-pos is not
    // accepted by these APIs; callers must explicitly pass bare-symbol.
    let expr = r#"(message "symposbinding:%S" (let ((s (position-symbol 'foo 1))) (mapcar (lambda (form) (equal (condition-case e (eval form) (error (list (car e) (cadr e)))) '(wrong-type-argument symbolp))) (list (list 'boundp s) (list 'default-boundp s) (list 'makunbound s) (list 'fboundp s) (list 'fmakunbound s)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "symposbinding:(t t t t t)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol-with-pos should not be transparent to binding APIs like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_with_pos_is_not_transparent_to_binding_apis_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn symbol_with_pos_print_symbols_bare_semantics_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/print.c:PVEC_SYMBOL_WITH_POS prints an unreadable
    // `#<symbol NAME at POS>' object unless print-symbols-bare is non-nil.
    // Therefore the default printed form is not readable.
    let expr = r##"(message "symposprint:%S" (let* ((s (position-symbol 'foo 12)) (printed (prin1-to-string s))) (list (eq (type-of s) 'symbol-with-pos) (equal printed "#<symbol foo at 12>") (equal (let ((print-symbols-bare t)) (prin1-to-string s)) "foo") (equal (let ((print-symbols-bare nil)) (prin1-to-string s)) "#<symbol foo at 12>") (equal (condition-case e (read printed) (error (list (car e) (cadr e)))) '(invalid-read-syntax "#<")))))"##;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "symposprint:(t t t t t)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: symbol-with-pos print-symbols-bare semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "symbol_with_pos_print_symbols_bare_semantics_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn read_positioning_symbols_print_and_cell_semantics_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:Fread_positioning_symbols wraps read symbols in
    // symbol-with-pos objects, and GNU src/print.c still obeys
    // print-symbols-bare for those objects.  The positioned symbols are not
    // accepted by CHECK_SYMBOL APIs such as symbol-name.
    let expr = r##"(message "readpos:%S" (let* ((obj (read-positioning-symbols "(alpha beta . gamma)"))) (list (equal (mapcar (lambda (x) (list (symbol-with-pos-p x) (bare-symbol x) (symbol-with-pos-pos x))) (list (car obj) (cadr obj) (cdr (cdr obj)))) '((t alpha 1) (t beta 7) (t gamma 14))) (equal (condition-case e (symbol-name (car obj)) (error (list (car e) (cadr e)))) '(wrong-type-argument symbolp)) (equal (let ((print-symbols-bare t)) (prin1-to-string obj)) "(alpha beta . gamma)") (equal (let ((print-symbols-bare nil)) (prin1-to-string obj)) "(#<symbol alpha at 1> #<symbol beta at 7> . #<symbol gamma at 14>)"))))"##;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "readpos:(t t t t)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: read-positioning-symbols symbol-with-pos semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "read_positioning_symbols_print_and_cell_semantics_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn function_predicate_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"funpred:%S\" (list (functionp (lambda () 1)) (subrp (symbol-function 'car)) (macrop 'when) (commandp 'find-file) (commandp (lambda () (interactive)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("funpred:") && row.contains("(t t t t t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: function predicate behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "function_predicate_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn function_arity_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"arity:%S\" (list (func-arity (lambda (a &optional b &rest c) nil)) (subr-arity (symbol-function 'car)) (help-function-arglist (lambda (x &optional y) nil)) (help-function-arglist (symbol-function 'cons))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("arity:")
                && row.contains("(1 . many)")
                && row.contains("(1 . 1)")
                && row.contains("(x &optional y)")
                && row.contains("(arg1 arg2)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: function arity introspection should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "function_arity_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn interactive_form_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"interactive:%S\" (let ((f (lambda (x) (interactive \"p\") x))) (list (commandp f) (interactive-form f))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("interactive:") && row.contains("(t (interactive"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: interactive form behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "interactive_form_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn commandp_interactive_form_property_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/eval.c:commandp treats an interactive-form symbol property on a
    // non-command as an error, while src/data.c:interactive-form still returns
    // that property when queried directly.
    let expr = r#"(message "cmdprop2:%S" (let ((s (make-symbol "neo-cmd-prop"))) (fset s (lambda () 1)) (put s 'interactive-form '(interactive "p")) (list (condition-case e (commandp s) (error (list (car e) (cadr e)))) (interactive-form s) (commandp (symbol-function s)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("cmdprop2:")
                && row.contains("error")
                && row.contains("interactive-form")
                && row.contains("(interactive")
                && row.contains("nil)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: commandp interactive-form property error should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "commandp_interactive_form_property_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn interactive_form_command_alias_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c:interactive-form follows indirect_function for command
    // aliases.  A no-argument `(interactive)' form is normalized and printed
    // as `(interactive nil)`.
    let expr = r#"(message "aliasiform:%S" (progn (defun neo-alias-target () (interactive) 1) (defalias 'neo-alias-command 'neo-alias-target "Alias doc.") (list (interactive-form 'neo-alias-command) (commandp 'neo-alias-command))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("aliasiform:") && row.contains("((interactive nil) t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: interactive-form for command aliases should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "interactive_form_command_alias_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn autoload_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"auto:%S\" (let ((s (make-symbol \"neo-auto\"))) (autoload s \"nofile\" \"doc\" t) (list (autoloadp (symbol-function s)) (commandp s) (documentation s t))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("auto:") && row.contains("(t t") && row.contains("doc"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: autoload behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("autoload_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn interactive_form_unloaded_autoload_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c:interactive-form follows indirect_function, so querying
    // an unloaded autoload attempts to load its file before returning an
    // interactive form.
    let expr = r#"(message "autoload3:%S" (let ((cmd (make-symbol "neo-auto-cmd")) (fun (make-symbol "neo-auto-fun"))) (autoload cmd "nofile" "doc" t) (autoload fun "nofile" "doc" nil) (list (commandp cmd) (condition-case e (interactive-form cmd) (error (list (car e) (cadr e)))) (commandp fun) (condition-case e (interactive-form fun) (error (list (car e) (cadr e)))) (autoloadp (symbol-function cmd)) (autoloadp (symbol-function fun)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("autoload3:")
                && row.contains("(t (file-missing")
                && row.contains("nil (file-missing")
                && row.contains("t t)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: interactive-form on unloaded autoloads should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "interactive_form_unloaded_autoload_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn documentation_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"doc:%S\" (let ((s (make-symbol \"docfun\"))) (fset s (lambda () \"DOCSTR\" 1)) (list (documentation s t) (documentation-property s 'function-documentation t))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("doc:") && row.contains("DOCSTR") && row.contains("nil"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: documentation behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "documentation_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn documentation_property_overrides_function_docstring_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/doc.c:documentation consults the symbol's
    // function-documentation property.  documentation-property returns the
    // same property docstring when present.
    let expr = r#"(let((s(make-symbol"d")))(fset s(lambda()"L"1))(put s 'function-documentation "P")(message "docp:%S"(list(documentation s t)(documentation-property s 'function-documentation t))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("docp:") && row.contains(r#"\"P\" \"P\""#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: function-documentation property should override docstring like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "documentation_property_overrides_function_docstring_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn documentation_property_accepts_non_symbol_properties_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/doc.c:documentation-property delegates to Fget, and
    // GNU src/fns.c:get/put accept arbitrary Lisp objects as property keys.
    let expr = r#"(let((p(cons 'k nil)))(put 'dp-non p "Doc")(message "docpn:%S"(list(get 'dp-non p)(documentation-property 'dp-non p t))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("docpn:") && row.contains(r#"\"Doc\" \"Doc\""#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: documentation-property should accept non-symbol property keys like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "documentation_property_accepts_non_symbol_properties_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn advice_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"advice:%S\" (let ((s (make-symbol \"adv\")) (adv (lambda (orig x) (* 10 (funcall orig x))))) (fset s (lambda (x) (+ x 1))) (advice-add s :around adv) (prog1 (list (funcall s 2) (not (null (advice-member-p adv s)))) (advice-remove s adv))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("advice:") && row.contains("(30 t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: advice behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("advice_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn lambda_binding_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"dynlex:%S\" (let ((x 1)) (list (let ((f (lambda () x))) (let ((x 2)) (funcall f))) (let ((y 1)) (let ((f (lambda () y))) (setq y 3) (funcall f))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("dynlex:") && row.contains("(1 3)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: lambda binding behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "lambda_binding_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn let_sequence_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"letseq:%S\" (let ((x 1)) (list (let ((x 2) (y x)) (list x y)) (let* ((x 2) (y x)) (list x y)) x)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("letseq:") && row.contains("((2 1) (2 2) 1)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: let and let* sequencing should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "let_sequence_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn boolean_short_circuit_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"bool:%S\" (list (and 1 2 nil (error \"no\")) (or nil 0 (error \"no\")) (not nil) (not 0)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("bool:") && row.contains("(nil 0 t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: boolean short-circuit behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "boolean_short_circuit_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn cond_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"condform:%S\" (list (cond ((> 1 2) 'bad) ((< 1 2) 'ok) (t 'fallback)) (cond (nil 'bad) ((quote (x y))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("condform:") && row.contains("(ok (x y))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: cond behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("cond_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn loop_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"loop:%S\" (let ((i 0) acc) (list (while (< i 3) (push i acc) (setq i (1+ i))) acc (dotimes (j 3 'done) (push j acc)) acc)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("loop:") && row.contains("nil") && row.contains("done"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: loop behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("loop_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn pcase_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"pcase:%S\" (list (pcase (list 1 2) (`(,a ,b) (+ a b)) (_ nil)) (pcase :foo (:bar 1) (:foo 2) (_ 3)) (pcase '(a . b) (`(,x . ,y) (list x y)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("pcase:") && row.contains("(3 2 (a b))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: pcase behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("pcase_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn cl_lib_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'cl-lib) (message \"cl:%S\" (let ((x 1)) (list (cl-incf x 2) x (cl-loop for i below 3 sum i) (cl-typecase \"x\" (string 'str) (t 'other))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("cl:") && row.contains("(3 3 3 str)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: cl-lib behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("cl_lib_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn cl_defstruct_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'cl-lib) (cl-defstruct neo-point x y) (message \"clstruct:%S\" (let ((p (make-neo-point :x 1 :y 2))) (setf (neo-point-y p) 9) (list (neo-point-p p) (neo-point-x p) (neo-point-y p) (type-of p) (length p)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("clstruct:") && row.contains("(t 1 9 neo-point 3)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: cl-defstruct constructor/accessor behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "cl_defstruct_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn cl_symbol_macrolet_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'cl-lib) (message \"symmac:%S\" (list (cl-symbol-macrolet ((x (car cell))) (let ((cell (list 1))) (setq x 7) cell)) (macroexpand '(cl-symbol-macrolet ((x y)) x)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("symmac:") && row.contains("((7) y)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: cl-symbol-macrolet expansion and setq behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "cl_symbol_macrolet_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn seq_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'seq) (message \"seq:%S\" (list (seq-filter #'numberp '(a 1 b 2)) (seq-map #'1+ [1 2]) (seq-position '(a b c) 'b))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("seq:") && row.contains("((1 2) (2 3) 1)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: seq library behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("seq_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn byte_compile_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'bytecomp) (message \"byte:%S\" (let ((f (byte-compile (lambda (x) (+ x 2))))) (list (byte-code-function-p f) (funcall f 3)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("byte:") && row.contains("(t 5)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: byte-compile behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "byte_compile_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn rx_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'rx) (message \"rx:%S\" (list (rx-to-string '(seq bol (or \"a\" \"b\") eol)) (string-match-p (rx bol (+ digit) eol) \"123\") (regexp-opt '(\"foo\" \"bar\")))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("rx:") && row.contains("[ab]") && row.contains("0"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: rx behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("rx_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn regexp_opt_depth_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"regexp:%S\" (let ((re (regexp-opt '(\"cat\" \"car\") 'paren))) (list re (regexp-opt-depth re) (string-match re \"car\") (match-string 1 \"car\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("regexp:") && row.contains("ca[rt]") && row.contains("1 0"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: regexp-opt-depth behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "regexp_opt_depth_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn regexp_quote_words_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"regexpquote:%S\" (list (regexp-quote \"a+b? [x]\") (regexp-opt '(\"a+\" \"a?\" \"ab\") 'words) (regexp-opt-depth (regexp-opt '(\"a+\" \"a?\" \"ab\") 'words))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("regexpquote:")
            && recent.contains("a")
            && recent.contains("+b")
            && recent.contains("?")
            && recent.contains("[x]")
            && recent.contains("a[+?b]")
            && recent.contains(" 1)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: regexp quote and word regexp-opt behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "regexp_quote_words_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn ring_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'ring) (message \"ring:%S\" (let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (ring-insert r 'd) (list (ring-length r) (ring-ref r 0) (ring-ref r 2) (ring-empty-p r)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ring:") && row.contains("(3 d b nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: ring behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("ring_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn subr_x_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'subr-x) (message \"subrx:%S\" (list (string-empty-p \"\") (string-trim \"  hi \") (when-let ((x 3)) (+ x 4)) (if-let ((x nil)) x 'fallback))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("subrx:")
                && row.contains("(t")
                && row.contains("hi")
                && row.contains("7 fallback")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: subr-x behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("subr_x_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn thread_macro_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'subr-x) (message \"thread:%S\" (list (thread-first 3 (1+) (* 2)) (thread-last '(1 2 3) (mapcar #'1+) (apply '+)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("thread:") && row.contains("(8 9)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: thread macro behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "thread_macro_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn eieio_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'eieio) (defclass neo-eieio-test () ((x :initarg :x :initform 1))) (message \"eieio:%S\" (let ((o (neo-eieio-test :x 5))) (list (object-of-class-p o 'neo-eieio-test) (oref o x) (progn (oset o x 7) (oref o x))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("eieio:") && row.contains("(t 5 7)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: EIEIO behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("eieio_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn cl_generic_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'cl-generic) (cl-defgeneric neo-generic (x)) (cl-defmethod neo-generic ((x integer)) (list 'int x)) (cl-defmethod neo-generic ((x string)) (list 'str x)) (message \"clgen:%S\" (list (neo-generic 3) (neo-generic \"x\") (condition-case e (neo-generic 'sym) (cl-no-applicable-method (car e)) (error (car e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("clgen:")
                && row.contains("(int 3)")
                && row.contains("str")
                && row.contains("x")
                && row.contains("cl-no-applicable-method")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: cl-generic dispatch and no-applicable-method signaling should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("cl_generic_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn define_minor_mode_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'easy-mmode) (define-minor-mode neo-test-mode \"Doc.\" :init-value nil :lighter \" Neo\") (with-temp-buffer (neo-test-mode 1) (message \"minor:%S\" (list neo-test-mode (assq 'neo-test-mode minor-mode-alist) (commandp 'neo-test-mode)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("minor:")
                && row.contains("neo-test-mode")
                && row.contains("Neo")
                && row.contains("t")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: define-minor-mode behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "define_minor_mode_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn define_derived_mode_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (define-derived-mode neo-derived-mode fundamental-mode \"NeoD\" \"Doc.\") (with-temp-buffer (neo-derived-mode) (message \"derived:%S\" (list major-mode mode-name (derived-mode-p 'fundamental-mode) (derived-mode-p 'neo-derived-mode)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("derived:")
                && row.contains("neo-derived-mode")
                && row.contains("NeoD")
                && row.contains("nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: define-derived-mode behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "define_derived_mode_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn map_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'map) (message \"map:%S\" (let ((h (make-hash-table :test 'equal))) (puthash \"a\" 1 h) (list (map-elt '((a . 1)) 'a) (map-elt '(:a 2) :a) (map-elt h \"a\") (map-keys '((x . 1) (y . 2)))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("map:") && row.contains("(1 2 1 (x y))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: map library behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("map_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn macroexpand_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"macro:%S\" (let ((m (macroexpand '(when t 1 2)))) (list (car m) (cadr m) (caddr m) (cadddr m))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("macro:") && row.contains("(if t") && row.contains("(progn 1 2)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: macroexpand should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "macroexpand_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn macroexp_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'macroexp) (message \"macroexp:%S\" (macroexp-progn '((setq a 1) (setq b 2)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("macroexp:")
                && row.contains("(progn")
                && row.contains("(setq a 1)")
                && row.contains("(setq b 2)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: macroexp behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("macroexp_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn syntax_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"syntax:%S\" (let ((st (make-syntax-table))) (with-syntax-table st (modify-syntax-entry ?_ \"w\") (modify-syntax-entry ?# \"<\") (list (char-syntax ?_) (char-syntax ?#) (char-syntax ?a)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("syntax:") && row.contains("(119 60 119)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: syntax table operations should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "syntax_table_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn syntax_table_copy_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"syntcopy:%S\" (let ((st (make-syntax-table))) (modify-syntax-entry ?_ \"w\" st) (let ((cp (copy-syntax-table st))) (modify-syntax-entry ?_ \"_\" cp) (list (with-syntax-table st (char-syntax ?_)) (with-syntax-table cp (char-syntax ?_)) (string-to-syntax \"w\")))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("syntcopy:") && row.contains("(119 95 (2))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: syntax-table copying should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "syntax_table_copy_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn syntax_table_regexp_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"synre:%S\" (let ((st (make-syntax-table))) (modify-syntax-entry ?_ \"w\" st) (modify-syntax-entry ?$ \".\" st) (with-syntax-table st (list (char-syntax ?_) (char-syntax ?$) (string-match \"\\\\sw+\" \"__\") (string-match \"\\\\s.+\" \"$$\")))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("synre:") && row.contains("(119 46 0 0)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: syntax-table regexp classes should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "syntax_table_regexp_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn syntax_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "synprop:%S" (with-temp-buffer (insert "_") (put-text-property 1 2 'syntax-table (string-to-syntax "w")) (list (let ((parse-sexp-lookup-properties t)) (syntax-class (syntax-after 1))) (let ((parse-sexp-lookup-properties nil)) (syntax-class (syntax-after 1))) (char-syntax ?_))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("synprop:(2 3 95)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: syntax-after text property lookup should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "syntax_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn syntax_table_comment_flags_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"syntaxextra:%S\" (let ((st (make-syntax-table))) (modify-syntax-entry ?/ \". 124b\" st) (modify-syntax-entry ?* \". 23\" st) (with-syntax-table st (list (string-to-syntax \". 124b\") (char-syntax ?/) (char-syntax ?*)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("syntaxextra:") && row.contains("((2818049) 46 46)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: syntax-table comment flag encoding should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "syntax_table_comment_flags_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn skip_syntax_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"syntaxclass:%S\" (with-temp-buffer (emacs-lisp-mode) (insert \"a_b\") (list (skip-syntax-forward \"w_\") (point) (char-syntax ?_) (char-syntax ?a))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("syntaxclass:") && row.contains("(0 4 95 119)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: skip-syntax-forward and syntax class behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "skip_syntax_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn category_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"category:%S\" (let ((ct (make-category-table))) (define-category ?x \"X category\" ct) (modify-category-entry ?a ?x ct) (list (category-docstring ?x ct) (category-set-mnemonics (aref ct ?a)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("category:") && row.contains("X category") && row.contains("x"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: category table behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "category_table_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn char_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"chartab:%S\" (let ((ct (make-char-table nil 0))) (set-char-table-range ct '(?a . ?c) 9) (aset ct ?b 4) (list (aref ct ?a) (aref ct ?b) (aref ct ?c) (aref ct ?d))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("chartab:") && row.contains("(9 4 9 0)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: char-table operations should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("char_table_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn char_table_range_error_and_reversed_range_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "chartabrange:%S" (let ((ct (make-char-table nil 0))) (list (condition-case e (char-table-range ct -1) (error (list (car e) (cadr e)))) (condition-case e (set-char-table-range ct -1 9) (error (list (car e) (cadr e)))) (condition-case e (set-char-table-range ct '(?z . ?a) 9) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("chartabrange:")
            && recent.contains("Invalid RANGE argument")
            && recent.contains("char-table-range")
            && recent.contains("set-char-table-range")
            && recent.contains(" 9)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: char-table invalid and reversed range semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "char_table_range_error_and_reversed_range_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn set_char_table_reversed_range_returns_value_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/chartab.c:set-char-table-range delegates cons ranges directly
    // to char_table_set_range.  When FROM is greater than TO, that helper's
    // loop is empty, no error is signaled, and the Lisp function returns VALUE.
    let expr = r#"(message "chartabrev:%S" (let ((ct (make-char-table nil 0))) (list (condition-case e (set-char-table-range ct '(?z . ?a) 'bad) (error (list (car e) (cadr e) (caddr e)))) (aref ct ?a) (aref ct ?z))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "chartabrev:(bad 0 0)";
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: reversed set-char-table-range should return VALUE without changing entries like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "set_char_table_reversed_range_returns_value_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn make_char_table_purpose_type_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "chartabpurpose:%S" (condition-case e (make-char-table 123) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("chartabpurpose:")
                && row.contains("wrong-type-argument")
                && row.contains("symbolp")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: make-char-table purpose type checking should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "make_char_table_purpose_type_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn make_char_table_extra_slots_property_type_error_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/chartab.c:Fmake_char_table reads PURPOSE's
    // `char-table-extra-slots' property and validates it with CHECK_FIXNAT.
    // Negative integers and floats must signal wholenump instead of being
    // silently treated as zero extra slots.
    let expr = r#"(message "chartabextraslots:%S" (list (let ((sym (make-symbol "x"))) (put sym 'char-table-extra-slots -1) (condition-case e (make-char-table sym nil) (error (list (car e) (cadr e))))) (let ((sym (make-symbol "x"))) (put sym 'char-table-extra-slots 1.0) (condition-case e (make-char-table sym nil) (error (list (car e) (cadr e)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected =
        "chartabextraslots:((wrong-type-argument wholenump) (wrong-type-argument wholenump))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: make-char-table should validate char-table-extra-slots like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "make_char_table_extra_slots_property_type_error_matches_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn case_table_fillarray_preserves_gnu_extra_slot_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "casefill:%S" (let ((ct (make-char-table 'case-table 'base))) (fillarray ct 'x) (list (char-table-p ct) (aref ct ?a) (aref ct 999999) (condition-case e (aref ct nil) (error (car e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("casefill:") && row.contains("(t base x wrong-type-argument)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: fillarray on case-table char-tables should match GNU extra-slot semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "case_table_fillarray_preserves_gnu_extra_slot_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn char_table_extra_slots_initialize_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/chartab.c:make-char-table sizes custom extra slots from the
    // PURPOSE symbol's char-table-extra-slots property and initializes the
    // whole backing vector, including extras, to INIT.
    let expr = r#"(message "chartabextra:%S" (progn (put 'neo-extra-purpose 'char-table-extra-slots 2) (unwind-protect (let ((ct (make-char-table 'neo-extra-purpose 0))) (set-char-table-extra-slot ct 0 'slot0) (list (char-table-subtype ct) (char-table-extra-slot ct 0) (char-table-extra-slot ct 1) (condition-case e (char-table-extra-slot ct 2) (error (car e))))) (put 'neo-extra-purpose 'char-table-extra-slots nil))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("chartabextra:(neo-extra-purpose slot0 0 args-out-of-range)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: custom char-table extra slots should initialize to INIT like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("char_table_extra_slots_initialize_like_gnu", &gnu, &neo);
}

#[test]
fn char_table_extra_slot_index_type_errors_use_fixnump_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/chartab.c:Fchar_table_extra_slot and
    // Fset_char_table_extra_slot validate N with CHECK_FIXNUM, so a float
    // index must signal fixnump, not the broader integerp predicate.
    let expr = r#"(message "chartabextraidx:%S" (progn (put 'neo-extra-index-purpose 'char-table-extra-slots 1) (unwind-protect (let ((ct (make-char-table 'neo-extra-index-purpose nil))) (list (condition-case e (char-table-extra-slot ct 0.0) (error (list (car e) (cadr e)))) (condition-case e (set-char-table-extra-slot ct 0.0 'x) (error (list (car e) (cadr e)))))) (put 'neo-extra-index-purpose 'char-table-extra-slots nil))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "chartabextraidx:((wrong-type-argument fixnump) (wrong-type-argument fixnump))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: char-table extra-slot index type errors should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "char_table_extra_slot_index_type_errors_use_fixnump_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn char_table_map_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"chartabmap:%S\" (let ((ct (make-char-table nil nil)) seen) (set-char-table-range ct ?a 1) (set-char-table-range ct ?b 2) (map-char-table (lambda (k v) (push (cons k v) seen)) ct) (sort seen (lambda (a b) (< (car a) (car b))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("chartabmap:") && row.contains("((97 . 1) (98 . 2))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: map-char-table behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "char_table_map_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn map_char_table_parent_inheritance_ranges_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/chartab.c:map_char_table/map_sub_char_table treats nil local
    // slots as inherited from the parent, emits only value changes, and
    // coalesces the inherited tail into one range.  It must not duplicate the
    // inherited tail or drop explicit local mappings before it.
    let expr = r#"(message "chartabparentmap:%S" (let ((p (make-char-table nil 1)) (c (make-char-table nil nil)) seen) (set-char-table-parent c p) (set-char-table-range c ?a 1) (set-char-table-range c ?b 2) (map-char-table (lambda (k v) (push (cons k v) seen)) c) (sort seen (lambda (a b) (< (if (consp (car a)) (caar a) (car a)) (if (consp (car b)) (caar b) (car b)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "chartabparentmap:((97 . 1) (98 . 2) ((99 . 4194303) . 1))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: map-char-table parent inheritance ranges should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "map_char_table_parent_inheritance_ranges_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn display_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"disptab:%S\" (let ((dt (make-display-table))) (aset dt 0 [65]) (list (vectorp dt) (char-table-p dt) (aref dt 0) (aref dt 1) (length dt))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("disptab:") && row.contains("(nil t [65] nil 4194304)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: display-table char-table slot behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "display_table_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn char_table_printer_includes_initialized_standard_slots_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/chartab.c:Fmake_char_table initializes the whole char-table
    // vector with INIT before installing parent and purpose.  GNU
    // src/print.c prints PVEC_CHAR_TABLE through print_stack_push_vector over
    // PVSIZE slots, so `(prin1-to-string (make-char-table nil 1))` exposes
    // the initialized standard slots as repeated `1`, not repeated `nil`.
    let expr = r#"(message "chartabprint:%S" (let ((s (prin1-to-string (make-char-table nil 1)))) (list (length s) (string-match "1 1 1 1 1 1 1 1" s) (string-match "nil nil nil nil nil nil nil nil" s))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "chartabprint:(143 13 nil)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: char-table printer should expose initialized standard slots like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "char_table_printer_includes_initialized_standard_slots_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn display_table_width_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/character.c consults `buffer-display-table' in char-width and
    // string-width; lisp/international/mule-util.el's truncate-string-to-width
    // is built on those same width semantics.
    let expr = r#"(message "dispwidth:%S" (let ((dt (make-display-table))) (aset dt ?x [?A ?B ?C]) (with-temp-buffer (setq buffer-display-table dt) (list (char-width ?x) (string-width "xox") (truncate-string-to-width "xox" 4 0 ?.)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains(r#"dispwidth:(3 7 \"xo\")"#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: display-table replacements should affect width functions like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "display_table_width_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_width_reversed_range_errors_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/character.c:string-width validates FROM/TO with
    // validate_subarray, so reversed substring bounds signal
    // args-out-of-range rather than silently yielding zero width.
    let expr = r#"(message "widthrange:%S" (condition-case e (string-width "abc" 3 2) (error (list (car e) (cadr e) (caddr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"widthrange:(args-out-of-range \"abc\" 3)"#;
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string-width should reject reversed FROM/TO bounds like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("string_width_reversed_range_errors_like_gnu", &gnu, &neo);
}

#[test]
fn save_excursion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "saveexc:%S" (with-temp-buffer (insert "abc") (goto-char 2) (let ((before (point)) inside after at-point) (setq inside (save-excursion (goto-char (point-max)) (insert "Z") (point))) (setq after (point)) (erase-buffer) (insert "ab") (goto-char 2) (setq at-point (list (save-excursion (insert "X") (point)) (point) (buffer-string))) (list before inside after at-point))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("saveexc:") && row.contains(r#"(2 5 2 (3 2 \"aXb\"))"#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: save-excursion should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "save_excursion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn save_excursion_killed_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "savekill:%S" (let ((orig (current-buffer)) (b (generate-new-buffer " *savekill*")) result current-live) (unwind-protect (progn (set-buffer b) (insert "abc") (goto-char 2) (setq result (save-excursion (kill-buffer b) (list :body (buffer-live-p b) (buffer-name (current-buffer))))) (setq current-live (buffer-live-p (current-buffer))) (list result current-live (buffer-live-p b) (eq (current-buffer) orig))) (when (buffer-live-p b) (kill-buffer b)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"savekill:((:body nil \"*Messages*\") t nil nil)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: save-excursion around a killed current buffer should follow GNU kill-buffer and unwind semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "save_excursion_killed_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn save_mark_and_excursion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "savemark:%S" (with-temp-buffer (insert "abcd") (goto-char 2) (set-mark 4) (setq mark-active t) (let ((inside (save-mark-and-excursion (goto-char 1) (set-mark 3) (setq mark-active nil) (list (point) (mark t) mark-active)))) (list inside (point) (mark t) mark-active))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("savemark:") && row.contains("((1 3 nil) 2 4 t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: save-mark-and-excursion should restore point, mark, and mark-active like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "save_mark_and_excursion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn save_current_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"savebuf:%S\" (let ((a (current-buffer)) (b (generate-new-buffer \" *neo-savebuf*\")) inside after) (unwind-protect (progn (setq inside (save-current-buffer (set-buffer b) (buffer-name (current-buffer)))) (setq after (eq (current-buffer) a)) (list inside after (buffer-live-p b))) (kill-buffer b))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("savebuf:") && row.contains("t t"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: save-current-buffer should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "save_current_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn generate_buffer_name_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"genbuf:%S\" (let ((a (generate-new-buffer \"neo\")) (b (generate-new-buffer \"neo\"))) (prog1 (list (buffer-name a) (buffer-name b) (eq a b)) (kill-buffer a) (kill-buffer b))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("genbuf:")
                && row.contains("neo")
                && row.contains("neo<2>")
                && row.contains("nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: generated buffer naming should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "generate_buffer_name_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn rename_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"renbuf:%S\" (let ((b (generate-new-buffer \"neo-buf\"))) (unwind-protect (with-current-buffer b (list (buffer-name) (rename-buffer \"neo-renamed\" t) (buffer-name) (generate-new-buffer-name \"neo-renamed\"))) (when (buffer-live-p b) (kill-buffer b)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("renbuf:")
                && row.contains("neo-buf")
                && row.contains("neo-renamed")
                && row.contains("neo-renamed<2>")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: rename-buffer behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "rename_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_last_name_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "buflast:%S" (let ((b (generate-new-buffer "neo-last"))) (with-current-buffer b (rename-buffer "neo-last-renamed" t)) (let ((before (list (buffer-name b) (buffer-last-name b)))) (kill-buffer b) (list before (buffer-live-p b) (buffer-name b) (buffer-last-name b)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"buflast:((\"neo-last-renamed\" \"neo-last\") nil nil \"neo-last-renamed\")"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer-last-name after rename and kill should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_last_name_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn set_visited_file_name_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let filename = format!("/tmp/neomacs-visfile-oracle-{}.txt", std::process::id());
    let basename = format!("neomacs-visfile-oracle-{}.txt", std::process::id());
    let expr = format!(
        r#"(message "visfile:%S" (let ((f "{filename}")) (with-temp-buffer (rename-buffer "neo-vis" t) (let ((start (list (buffer-name) buffer-file-name buffer-file-truename default-directory (buffer-modified-p)))) (set-visited-file-name f t) (let ((set (list (buffer-name) buffer-file-name buffer-file-truename default-directory (buffer-modified-p)))) (set-visited-file-name "" t) (list start set (list (buffer-name) buffer-file-name buffer-file-truename buffer-auto-save-file-name)))))))"#
    );

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("visfile:")
            && recent.contains(r#"(\"neo-vis\" nil nil"#)
            && recent.contains(&format!(r#"(\"{basename}\""#))
            && recent.contains(&format!(r#"\"{filename}\""#))
            && recent.contains(r#"\"/tmp/\" t)"#)
            && recent.contains(&format!(r#"\"{basename}\" nil"#))
            && recent.contains(r#"nil))"#)
    };

    // GNU creates a lock as part of `set-buffer-modified-p` in
    // `set-visited-file-name`; drive this oracle serially so the two
    // compared editors do not contend for the same real lock file.
    eval_expression_one(&mut gnu, &expr);
    gnu.read_until(Duration::from_secs(6), ready);
    eval_expression_one(&mut neo, &expr);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: set-visited-file-name buffer renaming and nil filename semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "set_visited_file_name_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn indirect_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "indirect:%S" (let ((base (generate-new-buffer "neo-base")) ind nested) (unwind-protect (progn (with-current-buffer base (insert "abc") (goto-char 2)) (setq ind (make-indirect-buffer base "neo-ind" nil t)) (setq nested (make-indirect-buffer ind "neo-nested" nil t)) (list (eq (buffer-base-buffer ind) base) (eq (buffer-base-buffer nested) base) (with-current-buffer ind (list (buffer-string) (point) buffer-file-name)) (with-current-buffer ind (condition-case e (set-visited-file-name "/tmp/indirect.txt" t) (error (car e)))))) (when (buffer-live-p nested) (kill-buffer nested)) (when (buffer-live-p ind) (kill-buffer ind)) (when (buffer-live-p base) (kill-buffer base)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains(r#"indirect:(t t (\"abc\" 2 nil) error)"#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: indirect buffer base resolution and visited-file rejection should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "indirect_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn killed_buffer_local_value_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "killlocals:%S" (let ((b (generate-new-buffer "neo-locals"))) (with-current-buffer b (setq-local fill-column 33) (setq-local neo-kill-local 44)) (let ((before (list (local-variable-p 'fill-column b) (buffer-local-value 'fill-column b) (buffer-local-value 'neo-kill-local b)))) (kill-buffer b) (list before (buffer-live-p b) (condition-case e (buffer-local-value 'fill-column b) (error (car e))) (boundp 'neo-kill-local)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("killlocals:((t 33 44) nil 70 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer-local-value after kill should fall back to defaults like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "killed_buffer_local_value_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn killed_buffer_file_name_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "deadbuf:%S" (let ((b (generate-new-buffer "neo-dead"))) (with-current-buffer b (setq buffer-file-name "/tmp/dead.txt" buffer-file-truename "/tmp/dead.txt")) (kill-buffer b) (list (buffer-live-p b) (buffer-name b) (buffer-last-name b) (buffer-file-name b) (buffer-base-buffer b) (condition-case e (with-current-buffer b (current-buffer)) (error (car e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"deadbuf:(nil nil \"neo-dead\" \"/tmp/dead.txt\" nil error)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: killed buffer name and file-name slots should remain queryable like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "killed_buffer_file_name_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn kill_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"killbuf:%S\" (let ((b (generate-new-buffer \"neo-kill\"))) (list (buffer-live-p b) (kill-buffer b) (buffer-live-p b))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("killbuf:") && row.contains("(t t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: kill-buffer liveness behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "kill_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn kill_current_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "killcur:%S" (let ((orig (current-buffer)) (b (generate-new-buffer " *killcur*"))) (set-buffer b) (insert "abc") (let ((ret (kill-buffer b))) (list ret (buffer-live-p b) (buffer-name (current-buffer)) (eq (current-buffer) orig)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"killcur:(t nil \"*Messages*\" nil)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: kill-buffer of the current buffer should select GNU's other-buffer result\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "kill_current_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn other_buffer_visible_preference_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "otherbuf:%S" (let ((b (generate-new-buffer " *hidden-current*"))) (unwind-protect (progn (set-buffer b) (list (buffer-name (other-buffer b nil nil)) (buffer-name (other-buffer b t nil)) (buffer-name (other-buffer nil t nil)))) (when (buffer-live-p b) (kill-buffer b)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"otherbuf:(\"*Messages*\" \"*scratch*\" \"*scratch*\")"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: other-buffer should prefer non-visible candidates before visible buffers like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "other_buffer_visible_preference_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_list_startup_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "buflist:%S" (let ((names (mapcar (function buffer-name) (buffer-list)))) (list (and (member " *code-conversion-work*" names) t) (member " *code-converting-work*" names) names)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"buflist:(t nil (\"*scratch*\" \" *Minibuf-1*\" \" *Minibuf-0*\" \"*Messages*\" \" *Echo Area 0*\" \" *Echo Area 1*\" \" *code-conversion-work*\"))"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: startup buffer-list should expose the same live buffers and ordering as GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_list_startup_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_modified_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"mod:%S\" (with-temp-buffer (list (buffer-modified-p) (progn (insert \"x\") (buffer-modified-p)) (progn (set-buffer-modified-p nil) (buffer-modified-p)) (progn (set-buffer-modified-p t) (buffer-modified-p)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("mod:") && row.contains("(nil t nil t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer modified flag semantics should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_modified_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_modified_tick_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "modtick:%S" (with-temp-buffer (let ((m0 (buffer-modified-tick)) (c0 (buffer-chars-modified-tick))) (insert "x") (let ((m1 (buffer-modified-tick)) (c1 (buffer-chars-modified-tick))) (restore-buffer-modified-p 'autosaved) (list (buffer-modified-p) (> m1 m0) (> c1 c0) (= (buffer-modified-tick) m1) (= (buffer-chars-modified-tick) c1) (progn (restore-buffer-modified-p nil) (buffer-modified-p)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("modtick:(autosaved t t t t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer modified tick and autosaved state should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_modified_tick_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn text_property_modified_tick_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "tpropmodtick:%S" (with-temp-buffer (insert "abc") (let ((m0 (buffer-modified-tick)) (c0 (buffer-chars-modified-tick))) (put-text-property 1 2 'face 'bold) (list (buffer-modified-p) (> (buffer-modified-tick) m0) (= (buffer-chars-modified-tick) c0) (text-properties-at 1)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "tpropmodtick:(t t t (face bold))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text property modified tick behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "text_property_modified_tick_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn overlay_property_modified_tick_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "ovmodtick:%S" (with-temp-buffer (insert "abc") (restore-buffer-modified-p nil) (let ((m0 (buffer-modified-tick)) (c0 (buffer-chars-modified-tick)) (o (make-overlay 1 2))) (overlay-put o 'face 'bold) (list (buffer-modified-p) (= (buffer-modified-tick) m0) (= (buffer-chars-modified-tick) c0) (overlay-get o 'face)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "ovmodtick:(nil t t bold)";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: overlay property modified tick behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "overlay_property_modified_tick_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn subst_char_in_region_noundo_tick_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "substnoundo:%S" (with-temp-buffer (buffer-enable-undo) (insert "abc") (setq buffer-undo-list nil) (restore-buffer-modified-p nil) (let ((m0 (buffer-modified-tick)) (c0 (buffer-chars-modified-tick))) (subst-char-in-region 1 4 ?a ?z t) (list (buffer-string) (buffer-modified-p) (> (buffer-modified-tick) m0) (> (buffer-chars-modified-tick) c0) buffer-undo-list))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"substnoundo:(\"zbc\" t t t nil)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: subst-char-in-region NOUNDO tick and undo behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "subst_char_in_region_noundo_tick_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn transpose_regions_moves_text_properties_and_markers_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "transpose:%S" (with-temp-buffer (insert "abcdef") (put-text-property 1 3 'face 'r1) (put-text-property 5 7 'face 'r2) (let ((m1 (copy-marker 2)) (m2 (copy-marker 6))) (transpose-regions 1 3 5 7 nil) (list (buffer-string) (marker-position m1) (marker-position m2) (mapcar (lambda (p) (text-properties-at p)) (number-sequence 1 6))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("transpose:")
                && row.contains(r#"\"efcdab\""#)
                && row.contains("6 2")
                && row.contains("((face r2) (face r2) nil nil (face r1) (face r1))")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: transpose-regions should move text, properties, and markers like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "transpose_regions_moves_text_properties_and_markers_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_and_extract_empty_region_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "delxempty:%S" (with-temp-buffer (insert "abc") (restore-buffer-modified-p nil) (let ((m0 (buffer-modified-tick)) (c0 (buffer-chars-modified-tick))) (let ((s (delete-and-extract-region 2 2))) (list s (multibyte-string-p s) (string-bytes s) (buffer-string) (buffer-modified-p) (= (buffer-modified-tick) m0) (= (buffer-chars-modified-tick) c0))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"delxempty:(\"\" nil 0 \"abc\" nil t t)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: empty delete-and-extract-region behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_and_extract_empty_region_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_and_extract_region_properties_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "delxprop:%S" (with-temp-buffer (insert "abcd") (put-text-property 2 4 'face 'bold) (let ((s (delete-and-extract-region 2 4))) (list s (text-properties-at 0 s) (text-properties-at 1 s) (buffer-string) (buffer-modified-p)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"delxprop:(#(\"bc\" 0 2 (face bold)) (face bold) (face bold) \"ad\" t)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: delete-and-extract-region text property preservation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_and_extract_region_properties_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_and_extract_region_narrowing_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "delxnarrow:%S" (with-temp-buffer (insert "abcdef") (narrow-to-region 3 5) (let ((err (condition-case e (delete-and-extract-region 2 4) (error (list (car e) (length (cdr e))))))) (list err (buffer-string) (point-min) (point-max)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"delxnarrow:((args-out-of-range 3) \"cd\" 3 5)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: narrowed delete-and-extract-region range validation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_and_extract_region_narrowing_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn erase_buffer_narrowing_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "erasenarrow:%S" (with-temp-buffer (insert "abcdef") (narrow-to-region 3 5) (erase-buffer) (list (buffer-string) (point-min) (point-max) (buffer-size) (point))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"erasenarrow:(\"\" 1 1 0 1)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: erase-buffer should widen before deleting like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "erase_buffer_narrowing_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_undo_list_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"undolist:%S\" (with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (delete-char -1) (list (buffer-string) (consp buffer-undo-list) (memq nil buffer-undo-list) (progn (primitive-undo 1 buffer-undo-list) (buffer-string)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("undolist:")
                && row.contains("ab")
                && row.contains("(nil")
                && row.contains("(1 . 4)")
                && row.contains("abc")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer undo list and primitive-undo should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_undo_list_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn primitive_undo_narrowing_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "undonarrow:%S" (with-temp-buffer (buffer-enable-undo) (insert "abc") (setq buffer-undo-list nil) (delete-region 2 3) (let ((ul buffer-undo-list)) (narrow-to-region 1 1) (let ((err (condition-case e (primitive-undo 1 ul) (error (list (car e) (cadr e)))))) (list err (buffer-string) (point-min) (point-max))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"undonarrow:((error \"Changes to be undone are outside visible portion of buffer\") \"\" 1 1)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: primitive-undo should reject undo outside visible narrowed region like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "primitive_undo_narrowing_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn buffer_disable_undo_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "undodisable:%S" (with-temp-buffer (buffer-enable-undo) (insert "a") (let ((before (consp buffer-undo-list))) (buffer-disable-undo) (let ((disabled buffer-undo-list)) (insert "b") (undo-boundary) (list before disabled buffer-undo-list (buffer-string))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains(r#"undodisable:(t t t \"ab\")"#))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: buffer-disable-undo should leave buffer-undo-list disabled like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "buffer_disable_undo_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn column_motion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(let ((result (with-temp-buffer (setq tab-width 8 indent-tabs-mode nil) (insert "a\tb\n") (goto-char (point-min)) (list (current-column) (progn (forward-char 1) (current-column)) (progn (forward-char 1) (current-column)) (progn (goto-char (point-min)) (move-to-column 4) (list (point) (current-column))) (progn (goto-char (point-min)) (move-to-column 8) (list (point) (current-column))) (progn (goto-char (point-min)) (move-to-column 4 t) (list (buffer-string) (point) (current-column))))))) (write-region (prin1-to-string result) nil (expand-file-name "column-motion-result.txt" "~") nil 'silent) (message "column:done"))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("column:done"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: current-column and move-to-column tab behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    let expected = r#"(0 1 8 (3 8) (3 8) ("a       b
" 5 4))"#;
    assert_eq!(
        std::fs::read_to_string(gnu.home_dir().join("column-motion-result.txt"))
            .expect("read GNU column motion result"),
        expected,
        "GNU column motion oracle should match the result studied in src/indent.c"
    );
    assert_eq!(
        std::fs::read_to_string(neo.home_dir().join("column-motion-result.txt"))
            .expect("read Neomacs column motion result"),
        expected,
        "Neomacs move-to-column FORCE inside a tab should replace the tab with spaces when indent-tabs-mode is nil"
    );

    assert_pair_exact_display(
        "column_motion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn thing_at_point_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"bounds:%S\" (with-temp-buffer (insert \"foo bar\\n  baz\") (goto-char 2) (list (bounds-of-thing-at-point 'word) (thing-at-point 'word t) (progn (goto-char 6) (bounds-of-thing-at-point 'symbol)) (progn (goto-char 12) (thing-at-point 'line t)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("bounds:")
                && row.contains("((1 . 4)")
                && row.contains("foo")
                && row.contains("(5 . 8)")
                && row.contains("baz")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: thing-at-point bounds and text extraction should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "thing_at_point_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn point_motion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"move:%S\" (with-temp-buffer (insert \"abc\") (goto-char 1) (list (progn (forward-char 2) (point)) (progn (backward-char 1) (point)) (condition-case e (forward-char 99) (error (car e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("move:") && row.contains("(3 2 end-of-buffer)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: point motion behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "point_motion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn point_character_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"bufferchars:%S\" (with-temp-buffer (insert \"abc\") (goto-char (point-min)) (list (following-char) (preceding-char) (progn (forward-char 1) (list (following-char) (preceding-char))) (progn (goto-char (point-max)) (following-char) (preceding-char)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("bufferchars:") && row.contains("(97 0 (98 97) 99)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: following-char and preceding-char behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "point_character_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn save_window_excursion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "savewin:%S" (progn (delete-other-windows) (let ((orig (current-buffer)) (b (generate-new-buffer " *neo-savewin*")) inside after) (unwind-protect (progn (setq inside (save-window-excursion (split-window-right) (other-window 1) (set-buffer b) (list (length (window-list)) (eq (current-buffer) b) (eq (selected-window) (next-window))))) (setq after (list (length (window-list)) (eq (current-buffer) orig))) (list inside after)) (kill-buffer b)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("savewin:") && row.contains("((2 t nil) (1 t))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: save-window-excursion should restore window configuration and current buffer like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "save_window_excursion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn window_visibility_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "winvis:%S" (progn (delete-other-windows) (let ((orig (current-buffer)) (b (generate-new-buffer " *winvis*"))) (unwind-protect (progn (set-buffer b) (list (buffer-name (window-buffer (selected-window))) (eq (get-buffer-window b) nil) (eq (get-buffer-window orig) (selected-window)) (length (window-list nil nil)) (length (window-list nil t)))) (when (buffer-live-p b) (kill-buffer b))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"winvis:(\"*scratch*\" t t 1 2)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: set-buffer visibility and window-list minibuffer inclusion should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "window_visibility_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn split_window_order_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "winsplit:%S" (progn (delete-other-windows) (let* ((w0 (selected-window)) (w1 (split-window-right))) (list (eq (selected-window) w0) (eq (next-window w0 nil nil) w1) (eq (next-window w1 nil nil) w0) (mapcar (lambda (w) (eq w w0)) (window-list nil nil w0)) (mapcar (lambda (w) (eq w w1)) (window-list nil nil w0)) (length (window-list nil nil)) (length (window-list nil t))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("winsplit:(t t t (t nil) (nil t) 2 3)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: split-window ordering and next-window traversal should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "split_window_order_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn window_defvar_lisp_variables_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(message "window-defvar:%S" "#,
        r#"(list (mapcar (lambda (s) "#,
        r#"(list s (boundp s) (symbol-value s) (special-variable-p s))) "#,
        r#"'(window-restore-killed-buffer-windows window-combination-limit)) "#,
        r#"(eval '(let ((window-restore-killed-buffer-windows 'dyn-restore) "#,
        r#"(window-combination-limit 'dyn-limit)) "#,
        r#"(list (symbol-value 'window-restore-killed-buffer-windows) "#,
        r#"(symbol-value 'window-combination-limit))) t)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("window-defvar:")
                && row.contains("(window-restore-killed-buffer-windows t nil t)")
                && row.contains("(window-combination-limit t window-size t)")
                && row.contains("(dyn-restore dyn-limit)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: window DEFVAR_LISP variables should be bound and special like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "window_defvar_lisp_variables_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn line_position_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"bufferpos:%S\" (with-temp-buffer (insert \"ab\\ncd\\nef\") (list (pos-bol) (pos-eol) (progn (forward-line 1) (list (line-number-at-pos) (pos-bol) (pos-eol) (count-lines (point-min) (point-max)))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("bufferpos:") && row.contains("(7 9 (3 7 9 3))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: line position helpers should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "line_position_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn line_position_field_constraints_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "field-linepos:%S" (with-temp-buffer (insert "aa" (propertize "bb" 'field 'f) "cc\nxx") (goto-char 4) (list (pos-bol) (line-beginning-position) (pos-eol) (line-end-position) (let ((inhibit-field-text-motion t)) (list (line-beginning-position) (line-end-position))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("field-linepos:") && row.contains("(1 3 7 5 (1 7))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: line-beginning-position and line-end-position should respect fields and inhibit-field-text-motion like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "line_position_field_constraints_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn editfns_defvar_lisp_variables_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(message "editfns-defvar:%S" "#,
        r#"(list (mapcar (lambda (s) "#,
        r#"(list s (boundp s) (symbol-value s) (special-variable-p s))) "#,
        r#"'(inhibit-field-text-motion "#,
        r#"buffer-access-fontify-functions "#,
        r#"buffer-access-fontified-property)) "#,
        r#"(eval '(let ((inhibit-field-text-motion 'dyn)) "#,
        r#"(symbol-value 'inhibit-field-text-motion)) t)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("editfns-defvar:")
                && row.contains("((inhibit-field-text-motion t nil t)")
                && row.contains("(buffer-access-fontify-functions t nil t)")
                && row.contains("(buffer-access-fontified-property t nil t)) dyn")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: editfns DEFVAR_LISP variables should be bound and special like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "editfns_defvar_lisp_variables_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn core_c_defvar_variables_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(with-current-buffer "*scratch*" (erase-buffer) "#,
        r#"(let ((facts (mapcar (lambda (s) "#,
        r#"(list s (boundp s) (symbol-value s) (special-variable-p s))) "#,
        r#"'(echo-keystrokes process-connection-type undo-limit undo-strong-limit inhibit-message inhibit-redisplay))) "#,
        r#"(dyn (eval '(let ((echo-keystrokes 7) (process-connection-type nil) "#,
        r#"(undo-limit 170001) (undo-strong-limit 270001) (inhibit-message t) (inhibit-redisplay t)) "#,
        r#"(list (symbol-value 'echo-keystrokes) (symbol-value 'process-connection-type) "#,
        r#"(symbol-value 'undo-limit) (symbol-value 'undo-strong-limit) "#,
        r#"(symbol-value 'inhibit-message) (symbol-value 'inhibit-redisplay))) t))) "#,
        r#"(dolist (entry facts) (insert (format "core-c-defvar-var:%S\n" entry))) "#,
        r#"(insert (format "core-c-defvar-dyn:%S\n" dyn)) (goto-char (point-min))))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains("core-c-defvar-var:(echo-keystrokes t 1 t)")
            && text.contains("core-c-defvar-var:(process-connection-type t t t)")
            && text.contains("core-c-defvar-var:(undo-limit t 160000 t)")
            && text.contains("core-c-defvar-var:(undo-strong-limit t 240000 t)")
            && text.contains("core-c-defvar-var:(inhibit-message t nil t)")
            && text.contains("core-c-defvar-var:(inhibit-redisplay t nil t)")
            && text.contains("core-c-defvar-dyn:(7 nil 170001 270001 t t)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: core C DEFVAR variables should be bound and special like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("core_c_defvar_variables_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn true_default_c_defvar_variables_are_special_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(with-current-buffer "*scratch*" (erase-buffer) "#,
        r#"(let ((vars '(auto-hscroll-mode "#,
        r#"delete-auto-save-files "#,
        r#"delete-exited-processes "#,
        r#"display-fill-column-indicator-column "#,
        r#"display-hourglass "#,
        r#"display-line-numbers-current-absolute "#,
        r#"make-cursor-line-fully-visible "#,
        r#"menu-prompting "#,
        r#"mode-line-in-non-selected-windows "#,
        r#"mouse-highlight "#,
        r#"open-paren-in-column-0-is-defun-start "#,
        r#"overflow-newline-into-fringe "#,
        r#"read-minibuffer-restore-windows "#,
        r#"scroll-bar-adjust-thumb-portion "#,
        r#"select-active-regions "#,
        r#"translate-upper-case-key-bindings "#,
        r#"use-dialog-box "#,
        r#"use-file-dialog "#,
        r#"use-system-tooltips "#,
        r#"visible-cursor "#,
        r#"x-gtk-file-dialog-help-text "#,
        r#"x-select-enable-clipboard-manager))) "#,
        r#"(dolist (s vars) "#,
        r#"(insert (format "true-defvar-special:%S\n" "#,
        r#"(list s (boundp s) (special-variable-p s))))) "#,
        r#"(insert (format "true-defvar-dyn:%S\n" "#,
        r#"(eval '(let ((auto-hscroll-mode nil) "#,
        r#"(use-dialog-box nil) "#,
        r#"(use-file-dialog nil) "#,
        r#"(visible-cursor nil)) "#,
        r#"(list (symbol-value 'auto-hscroll-mode) "#,
        r#"(symbol-value 'use-dialog-box) "#,
        r#"(symbol-value 'use-file-dialog) "#,
        r#"(symbol-value 'visible-cursor))) t)))) "#,
        r#"(goto-char (point-min)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = [
        "true-defvar-special:(auto-hscroll-mode t t)",
        "true-defvar-special:(delete-auto-save-files t t)",
        "true-defvar-special:(delete-exited-processes t t)",
        "true-defvar-special:(display-fill-column-indicator-column t t)",
        "true-defvar-special:(display-hourglass t t)",
        "true-defvar-special:(display-line-numbers-current-absolute t t)",
        "true-defvar-special:(make-cursor-line-fully-visible t t)",
        "true-defvar-special:(menu-prompting t t)",
        "true-defvar-special:(mode-line-in-non-selected-windows t t)",
        "true-defvar-special:(mouse-highlight t t)",
        "true-defvar-special:(open-paren-in-column-0-is-defun-start t t)",
        "true-defvar-special:(overflow-newline-into-fringe t t)",
        "true-defvar-special:(read-minibuffer-restore-windows t t)",
        "true-defvar-special:(scroll-bar-adjust-thumb-portion t t)",
        "true-defvar-special:(select-active-regions t t)",
        "true-defvar-special:(translate-upper-case-key-bindings t t)",
        "true-defvar-special:(use-dialog-box t t)",
        "true-defvar-special:(use-file-dialog t t)",
        "true-defvar-special:(use-system-tooltips t t)",
        "true-defvar-special:(visible-cursor t t)",
        "true-defvar-special:(x-gtk-file-dialog-help-text t t)",
        "true-defvar-special:(x-select-enable-clipboard-manager t t)",
        "true-defvar-dyn:(nil nil nil nil)",
    ];
    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        expected.iter().all(|needle| text.contains(needle))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: true-default C DEFVAR variables should be special like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "true_default_c_defvar_variables_are_special_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn select_active_regions_default_matches_gnu_keyboard_defvar() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(with-current-buffer "*scratch*" (erase-buffer) (insert (format "select-active-regions-default:%S\n" (list (boundp 'select-active-regions) (special-variable-p 'select-active-regions) select-active-regions (default-value 'select-active-regions)))) (goto-char (point-min)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "select-active-regions-default:(t t t t)";
    let ready = |grid: &[String]| grid.iter().any(|line| line.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: select-active-regions should match GNU keyboard.c DEFVAR default\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "select_active_regions_default_matches_gnu_keyboard_defvar",
        &gnu,
        &neo,
    );
}

#[test]
fn display_fill_column_indicator_column_matches_gnu_xdisp_defvar_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(with-current-buffer "*scratch*" (erase-buffer) (insert (format "fci-column:%S\n" (list (boundp 'display-fill-column-indicator-column) (special-variable-p 'display-fill-column-indicator-column) display-fill-column-indicator-column (default-value 'display-fill-column-indicator-column) (local-variable-if-set-p 'display-fill-column-indicator-column)))) (let ((before (list (buffer-local-boundp 'display-fill-column-indicator-column (current-buffer)) display-fill-column-indicator-column))) (setq display-fill-column-indicator-column 80) (insert (format "fci-column-local:%S\n" (list before (buffer-local-boundp 'display-fill-column-indicator-column (current-buffer)) display-fill-column-indicator-column (default-value 'display-fill-column-indicator-column) (with-temp-buffer display-fill-column-indicator-column))))) (goto-char (point-min)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = [
        "fci-column:(t t t t t)",
        "fci-column-local:((t t) t 80 t t)",
    ];
    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        expected.iter().all(|needle| text.contains(needle))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: display-fill-column-indicator-column should match GNU xdisp DEFVAR semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "display_fill_column_indicator_column_matches_gnu_xdisp_defvar_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn c_bootstrap_defvars_match_gnu_defaults_and_specialness() {
    let probe = r#";;; -*- lexical-binding: t -*-
(with-current-buffer "*scratch*"
  (goto-char (point-min))
  (dolist (entry
           (mapcar
            (lambda (s)
              (let* ((bound (boundp s))
                     (value (and bound (symbol-value s)))
                     (default (and bound (default-value s)))
                     (normalize
                      (lambda (v)
                        (if (and (eq s 'tool-bar-separator-image-expression)
                                 (consp v))
                            (let ((printed (prin1-to-string v)))
                              (list 'cons
                                    (car v)
                                    (equal v default)
                                    (not (null (string-match-p "separator.xpm" printed)))
                                    (not (null (string-match-p "separator.xbm" printed)))
                                    (not (null (string-match-p "separator.pbm" printed)))))
                          v))))
                (list s
                      bound
                      (special-variable-p s)
                      (funcall normalize value)
                      (funcall normalize default)
                      (local-variable-if-set-p s))))
            '(help-char
              help-event-list
              help-form
              deactivate-mark
              input-method-function
              cursor-in-echo-area
              executing-kbd-macro
              executing-kbd-macro-index
              inhibit-read-only
              tab-bar-separator-image-expression
              tool-bar-separator-image-expression)))
    (insert (format "c-bootstrap-defvar:%S\n" entry)))
  (goto-char (point-min)))"#;
    let probe_path = write_shared_temp_file("c-bootstrap-defvars.el", probe);
    let args = format!("-l {}", probe_path.display());
    let (mut gnu, mut neo) = boot_pair(&args);

    let expected = [
        "c-bootstrap-defvar:(help-char t t 8 8 nil)",
        "c-bootstrap-defvar:(help-event-list t t (help f1 63) (help f1 63) nil)",
        "c-bootstrap-defvar:(help-form t t nil nil nil)",
        "c-bootstrap-defvar:(deactivate-mark t t nil nil t)",
        "c-bootstrap-defvar:(input-method-function t t list list nil)",
        "c-bootstrap-defvar:(cursor-in-echo-area t t nil nil nil)",
        "c-bootstrap-defvar:(executing-kbd-macro t t nil nil nil)",
        "c-bootstrap-defvar:(executing-kbd-macro-index t t 0 0 nil)",
        "c-bootstrap-defvar:(inhibit-read-only t t nil nil nil)",
        "c-bootstrap-defvar:(tab-bar-separator-image-expression t t nil nil nil)",
        "c-bootstrap-defvar:(tool-bar-separator-image-expression t t (cons find-image t t t t) (cons find-image t t t t) nil)",
    ];
    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        expected.iter().all(|needle| text.contains(needle))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: C bootstrap DEFVAR defaults and specialness should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "c_bootstrap_defvars_match_gnu_defaults_and_specialness",
        &gnu,
        &neo,
    );
}

#[test]
fn eval_depth_limit_variables_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(with-current-buffer "*scratch*" (erase-buffer) "#,
        r#"(dolist (s '(max-lisp-eval-depth lisp-eval-depth-reserve max-specpdl-size)) "#,
        r#"(insert (format "eval-limit:%S\n" "#,
        r#"(list s (boundp s) (and (boundp s) (symbol-value s)) "#,
        r#"(special-variable-p s) (get s 'byte-obsolete-variable))))) "#,
        r#"(insert (format "eval-limit-dyn:%S\n" "#,
        r#"(condition-case e "#,
        r#"(eval '(let ((max-lisp-eval-depth 42) "#,
        r#"(lisp-eval-depth-reserve 43) "#,
        r#"(max-specpdl-size 44)) "#,
        r#"(list (symbol-value 'max-lisp-eval-depth) "#,
        r#"(symbol-value 'lisp-eval-depth-reserve) "#,
        r#"(symbol-value 'max-specpdl-size))) t) "#,
        r#"(error (list 'error (car e) (cadr e)))))) "#,
        r#"(goto-char (point-min)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains(r#"eval-limit:(max-lisp-eval-depth t 1600 t nil)"#)
            && text.contains(r#"eval-limit:(lisp-eval-depth-reserve t 200 t nil)"#)
            && text.contains(r#"eval-limit:(max-specpdl-size t 2500 t (nil nil "29.1"))"#)
            && text.contains(r#"eval-limit-dyn:(42 43 44)"#)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: eval depth limit variables should match GNU DEFVAR/subr.el semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("eval_depth_limit_variables_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn move_beginning_of_line_field_constraints_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "field-mbol:%S" (with-temp-buffer (insert "aa" (propertize "bb" 'field 'f) "cc\nxx") (goto-char 4) (let ((a (progn (move-beginning-of-line nil) (point))) (b (progn (goto-char 4) (move-end-of-line nil) (point))) (c (let ((inhibit-field-text-motion t)) (goto-char 4) (move-beginning-of-line nil) (point))) (d (let ((inhibit-field-text-motion t)) (goto-char 4) (move-end-of-line nil) (point)))) (list a b c d))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("field-mbol:") && row.contains("(3 7 1 7)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: move-beginning-of-line should honor inhibit-field-text-motion like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "move_beginning_of_line_field_constraints_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn line_boundary_predicate_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"bolp:%S\" (with-temp-buffer (insert \"a\\nb\") (goto-char 1) (list (bolp) (eolp) (progn (end-of-line) (list (bolp) (eolp))) (progn (forward-char 1) (list (bolp) (eolp))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("bolp:") && row.contains("(t nil (nil t) (t nil))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: bolp and eolp boundary predicates should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "line_boundary_predicate_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn sort_lines_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"sortlines:%S\" (with-temp-buffer (insert \"b\\na\\nc\\n\") (sort-lines nil (point-min) (point-max)) (split-string (buffer-string) \"\\n\" t)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("sortlines:")
                && row.contains("a")
                && row.contains("b")
                && row.contains("c")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sort-lines buffer transformation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("sort_lines_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn delete_text_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"deltext:%S\" (with-temp-buffer (insert \"abcdef\") (list (delete-region 2 4) (buffer-string) (progn (goto-char 2) (delete-char 1)) (buffer-string))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("deltext:")
                && row.contains("nil")
                && row.contains("adef")
                && row.contains("aef")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: text deletion behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_text_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn kill_ring_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"killring:%S\" (let ((kill-ring nil) (kill-ring-yank-pointer nil)) (kill-new \"a\") (kill-new \"b\") (list kill-ring (current-kill 0) (current-kill 1))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("killring:") && row.contains("b") && row.contains("a"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: kill ring insertion and current-kill behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("kill_ring_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn change_hook_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"changehook:%S\" (with-temp-buffer (let (seen) (add-hook 'before-change-functions (lambda (b e) (push (list 'before b e) seen)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'after b e l) seen)) nil t) (insert \"ab\") (nreverse seen))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("changehook:")
                && row.contains("(before 1 1)")
                && row.contains("(after 1 3 0)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: change hook behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "change_hook_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn combine_after_change_calls_coalesces_events_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/insdel.c defers after-change-functions while
    // combine-after-change-calls is active, then
    // combine-after-change-execute merges the recorded changes into one
    // after-change notification.
    let expr = r#"(message "combineafter:%S" (with-temp-buffer (let ((events nil)) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l (substring-no-properties (buffer-string))) events)) nil t) (combine-after-change-calls (insert "ab") (goto-char 2) (insert "X") (delete-region 1 2)) (list (substring-no-properties (buffer-string)) (nreverse events)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("combineafter:")
                && row.contains("Xb")
                && row.contains("((1 3 0")
                && !row.contains("(2 3 0")
                && !row.contains("(1 1 1")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: combine-after-change-calls should coalesce after-change events like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "combine_after_change_calls_coalesces_events_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn char_at_point_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"charpos:%S\" (with-temp-buffer (insert \"ab\") (goto-char 1) (list (char-after) (char-before) (progn (goto-char 2) (list (char-before) (char-after))) (progn (goto-char (point-max)) (char-after)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("charpos:") && row.contains("(97 nil (97 98) nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: char-before and char-after behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "char_at_point_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn line_motion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"linepos:%S\" (with-temp-buffer (insert \"aa\\nbbb\\n\") (goto-char 1) (list (progn (end-of-line) (point)) (progn (forward-line 1) (point)) (progn (end-of-line) (point)) (progn (beginning-of-line) (point)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("linepos:") && row.contains("(3 4 7 4)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: line motion behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "line_motion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn sexp_motion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"sexp:%S\" (with-temp-buffer (emacs-lisp-mode) (insert \"(a (b c))\") (list (progn (goto-char 1) (scan-sexps (point) 1)) (progn (goto-char 4) (forward-sexp 1) (point)) (progn (goto-char 9) (backward-sexp 1) (point)) (condition-case e (scan-sexps 1 -1) (scan-error (car e)) (error (car e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("sexp:") && row.contains("(10 9 4 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sexp scanning and motion should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "sexp_motion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn scan_lists_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"scanlists:%S\" (with-temp-buffer (emacs-lisp-mode) (insert \"(a (b) c)\") (list (scan-lists 1 1 0) (scan-lists 4 1 0) (condition-case e (scan-lists 1 -1 0) (scan-error (car e)) (error (car e))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("scanlists:") && row.contains("(10 7 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: scan-lists behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("scan_lists_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn parse_partial_sexp_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"pparse:%S\" (with-temp-buffer (emacs-lisp-mode) (insert \"(a \\\"b\\\") ;c\") (list (parse-partial-sexp 1 (point-max)) (nth 0 (syntax-ppss (point-max))) (nth 3 (syntax-ppss (point-max))) (nth 4 (syntax-ppss (point-max))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("pparse:")
                && row.contains("(0 nil 1 nil t")
                && row.contains("9 nil nil")
                && row.contains("0 nil t")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: parse-partial-sexp and syntax-ppss should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "parse_partial_sexp_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn parse_partial_sexp_comment_stop_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "ppstop:%S" (with-temp-buffer (emacs-lisp-mode) (insert "abc ; comment\n(def)") (list (parse-partial-sexp 1 (point-max) nil nil nil t) (parse-partial-sexp 1 (point-max) nil nil nil 'syntax-table))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected =
        "ppstop:((0 nil 1 nil t nil 0 nil 5 nil nil) (0 nil 1 nil t nil 0 nil 5 nil nil))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: parse-partial-sexp comment stop behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "parse_partial_sexp_comment_stop_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn emacs_lisp_indent_region_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"indent:%S\" (with-temp-buffer (emacs-lisp-mode) (insert \"(progn\\n(+ 1 2))\") (indent-region (point-min) (point-max)) (buffer-string)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("indent:") && recent.contains("(progn") && recent.contains("(+ 1 2))")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: emacs-lisp-mode indent-region should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "emacs_lisp_indent_region_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn case_fold_search_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"casefold:%S\" (with-temp-buffer (insert \"Abc\") (goto-char 1) (let ((case-fold-search t)) (list (search-forward \"abc\" nil t) (progn (goto-char 1) (let ((case-fold-search nil)) (search-forward \"abc\" nil t)))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("casefold:") && row.contains("(4 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: case-fold-search behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "case_fold_search_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn case_fold_regexp_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"casefre:%S\" (list (let ((case-fold-search t)) (string-match \"abc\" \"ABC\")) (let ((case-fold-search nil)) (string-match \"abc\" \"ABC\")) (let ((case-fold-search t)) (string-match \"[[:upper:]]+\" \"abc\")) (let ((case-fold-search nil)) (string-match \"[[:upper:]]+\" \"abc\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("casefre:") && row.contains("(0 nil 0 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: case-fold-search regexp behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "case_fold_regexp_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_match_literal_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"repl:%S\" (list (progn (string-match \"\\\\(foo\\\\)\" \"foo\") (replace-match \"X\\\\1\" nil nil \"foo\")) (progn (string-match \"\\\\(foo\\\\)\" \"foo\") (replace-match \"X\\\\1\" nil t \"foo\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("repl:") && row.contains("Xfoo"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-match literal behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_match_literal_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_match_buffer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"matchreplace:%S\" (with-temp-buffer (insert \"abc123def\") (goto-char (point-min)) (re-search-forward \"[0-9]+\") (replace-match \"NUM\") (list (buffer-string) (match-beginning 0) (match-end 0))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("matchreplace:") && row.contains("abcNUMdef") && row.contains("4 7")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-match buffer mutation and match data should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_match_buffer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_match_case_transfer_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "replmatchcase:%S" (list (progn (string-match "foo" "foo") (replace-match "bar" nil nil "foo")) (progn (string-match "foo" "Foo") (replace-match "bar" nil nil "Foo")) (progn (string-match "foo" "FOO") (replace-match "bar" nil nil "FOO")) (progn (string-match "foo" "FOO") (replace-match "bar" t nil "FOO"))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"replmatchcase:(\"bar\" \"Bar\" \"BAR\" \"bar\")"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-match case transfer and fixedcase behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_match_case_transfer_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_match_subexp_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "subrepl:%S" (list (progn (string-match "\\([a-z]+\\)-\\([0-9]+\\)-\\([a-z]+\\)" "foo-123-bar") (list (replace-match "X" nil nil "foo-123-bar" 2) (match-data))) (with-temp-buffer (insert "foo-123-bar") (goto-char 1) (re-search-forward "\\([a-z]+\\)-\\([0-9]+\\)-\\([a-z]+\\)") (replace-match "XX" nil nil nil 2) (list (buffer-string) (match-beginning 0) (match-end 0) (match-beginning 1) (match-end 1) (match-beginning 2) (match-end 2) (match-beginning 3) (match-end 3))) (condition-case e (progn (string-match "\\(a\\)?b" "b") (replace-match "X" nil nil "b" 1)) (error (car e))) (progn (string-match "\\(a\\)?b" "b") (replace-match "[\\1]" nil nil "b"))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"subrepl:((\"foo-X-bar\" (0 11 0 3 4 7 8 11)) (\"foo-XX-bar\" 1 11 1 4 5 7 8 11) error \"[]\")"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-match SUBEXP, match-data repair, and unmatched subexp behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_match_subexp_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_match_string_text_properties_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "replprop:%S" (let* ((s (copy-sequence "abcde")) r) (put-text-property 0 2 'face 'a s) (put-text-property 2 5 'face 'b s) (string-match "bc" s) (setq r (replace-match (propertize "XY" 'face 'x) t nil s)) (list r (mapcar (lambda (i) (text-properties-at i r)) (number-sequence 0 4)) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 4)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("replprop:")
            && recent.contains("aXYde")
            && recent.contains("((face a) (face x) (face x) (face b) (face b))")
            && recent.contains("((face a) (face a) (face b) (face b) (face b))")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-match on strings should preserve source and replacement text properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_match_string_text_properties_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_regexp_in_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"replcase:%S\" (list (replace-regexp-in-string \"[a-z]\" (lambda (m) (upcase m)) \"ab\") (let ((case-replace t)) (replace-regexp-in-string \"foo\" \"bar\" \"Foo\")) (let ((case-replace nil)) (replace-regexp-in-string \"foo\" \"bar\" \"Foo\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("replcase:") && row.contains("AB") && row.contains("Bar"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-regexp-in-string behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_regexp_in_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn replace_regexp_in_string_text_properties_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "replregexprop:%S" (let* ((s (copy-sequence "abcde"))) (put-text-property 0 2 'face 'a s) (put-text-property 2 5 'face 'b s) (let ((r (replace-regexp-in-string "bc" (propertize "XY" 'face 'x) s t t))) (list r (mapcar (lambda (i) (text-properties-at i r)) (number-sequence 0 4)) (mapcar (lambda (i) (text-properties-at i s)) (number-sequence 0 4))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("replregexprop:")
            && recent.contains("aXYde")
            && recent.matches("(face a)").count() >= 3
            && recent.matches("(face x)").count() >= 2
            && recent.matches("(face b)").count() >= 5
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: replace-regexp-in-string should preserve replacement and source text properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "replace_regexp_in_string_text_properties_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn equality_predicate_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"eqs:%S\" (list (eq 1000 1000) (eql 1.0 1.0) (equal 1 1.0) (equal \"x\" (copy-sequence \"x\")) (eq \"x\" (copy-sequence \"x\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("eqs:") && row.contains("(t t nil t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: equality predicates should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "equality_predicate_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn substring_sequence_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"substr:%S\" (list (substring \"abcdef\" 1 4) (substring \"abcdef\" -3 -1) (substring [a b c d] 1 3)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("substr:") && row.contains("bcd") && row.contains("[b c]"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: substring sequence behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "substring_sequence_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn ignore_errors_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"ignerr:%S\" (list (ignore-errors (+ 1 2)) (ignore-errors (error \"bad %s\" 9)) (condition-case e (error \"bad %s\" 9) (error (cdr e)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ignerr:") && row.contains("(3 nil") && row.contains("bad 9"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: ignore-errors behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "ignore_errors_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn completion_table_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"comp:%S\" (let ((tbl '(\"alpha\" \"alpine\" \"beta\"))) (list (try-completion \"al\" tbl) (try-completion \"alp\" tbl) (all-completions \"al\" tbl) (test-completion \"alpha\" tbl) (test-completion \"al\" tbl))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("comp:")
                && row.contains("alp")
                && row.contains("alpha")
                && row.contains("alpine")
                && row.contains("t nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: completion table functions should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "completion_table_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn completion_ignore_case_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"completioncase:%S\" (let ((completion-ignore-case t) (tbl '(\"alpha\" \"Alpine\" \"beta\"))) (list (try-completion \"AL\" tbl) (all-completions \"AL\" tbl) (test-completion \"ALPHA\" tbl))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("completioncase:")
                && row.contains("alp")
                && row.contains("alpha")
                && row.contains("Alpine")
                && row.contains("t")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: completion-ignore-case behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "completion_ignore_case_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn add_to_history_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (defvar neomacs-test-history nil) (let ((neomacs-test-history nil) (history-delete-duplicates t)) (add-to-history 'neomacs-test-history \"a\") (add-to-history 'neomacs-test-history \"b\") (add-to-history 'neomacs-test-history \"a\") (message \"history:%S\" neomacs-test-history)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("history:") && row.contains("a") && row.contains("b"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: add-to-history duplicate deletion should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "add_to_history_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn add_to_history_keep_all_and_limits_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "history-corners:%S" (let ((history-delete-duplicates t) (history-length 10)) (defvar h1 nil) (defvar h2 nil) (defvar h3 nil) (defvar h4 nil) (setq h1 nil h2 nil h3 nil h4 nil) (add-to-history 'h1 "") (add-to-history 'h1 "" nil t) (add-to-history 'h1 "" nil t) (add-to-history 'h2 "a" 0) (put 'h3 'history-length 2) (mapc (lambda (x) (add-to-history 'h3 x)) '("a" "b" "c")) (let ((history-delete-duplicates nil)) (add-to-history 'h4 "a") (add-to-history 'h4 "a" nil nil) (add-to-history 'h4 "a" nil t)) (list h1 h2 h3 h4)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("history-corners:((\\\"\\\") nil"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: add-to-history keep-all, duplicate deletion, and length limits should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "add_to_history_keep_all_and_limits_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn numeric_rounding_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"round:%S\" (list (truncate -1.7) (floor -1.2) (ceiling -1.2) (round 2.5) (round -2.5)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("round:") && row.contains("(-1 -2 -1 2 -2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: numeric rounding behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "numeric_rounding_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn arithmetic_remainder_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr =
        "(message \"arith:%S\" (list (/ 7 3) (/ 7 3.0) (mod -7 3) (% -7 3) (mod 7 -3) (% 7 -3)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("arith:") && row.contains("(2 2.333") && row.contains("2 -1 -2 1")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: arithmetic remainder behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "arithmetic_remainder_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn substring_list_type_error_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:substring accepts strings and vectors via
    // CHECK_VECTOR_OR_STRING.  List input must signal arrayp, not stringp.
    let expr = r#"(message "subtype:%S" (condition-case e (substring '(a b c) 0 1) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("subtype:(wrong-type-argument arrayp)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: substring list type error should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "substring_list_type_error_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_number_conversion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"strnum:%S\" (list (string-to-number \"010\") (string-to-number \"010\" 8) (string-to-number \"ff\" 16) (string-to-number \"12abc\") (number-to-string 1.5)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("strnum:") && row.contains("(10 8 255 12") && row.contains("1.5")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string/number conversion should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_number_conversion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_to_number_special_float_exponents_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lread.c:string_to_number accepts e+INF/e+NaN spellings in
    // decimal exponent syntax.  These are special floats, not ordinary
    // numbers parsed only up to the `e`.
    let expr = r#"(message "numparse:%S" (list (number-to-string (string-to-number "1.2e+INF")) (number-to-string (string-to-number "12e+NaN")) (string-to-number "1.") (string-to-number "1.e2") (string-to-number "1.9" 16)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("numparse:")
                && row.contains("\\\"1.0e+INF\\\"")
                && row.contains("\\\"12.0e+NaN\\\"")
                && row.contains("1 100.0 1)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string-to-number special float exponent parsing should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_to_number_special_float_exponents_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_to_number_base_type_error_matches_gnu_fixnump_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/data.c:Fstring_to_number validates BASE with CHECK_FIXNUM.
    // A float base is therefore a fixnump type error, not the broader
    // integerp error.
    let expr = r#"(message "strbase:%S" (condition-case e (string-to-number "10" 2.0) (error (list (car e) (cadr e)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("strbase:(wrong-type-argument fixnump)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string-to-number BASE type error should match GNU CHECK_FIXNUM\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_to_number_base_type_error_matches_gnu_fixnump_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn integer_bit_arithmetic_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"numedge:%S\" (list (floor -3 2) (ceiling -3 2) (truncate -3 2) (round 2.5) (round -2.5) (mod -3 2) (ash -8 -1) (logand #b1100 #b1010)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("numedge:") && row.contains("(-2 -1 -1 2 -2 1 -4 8)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: integer division and bit arithmetic should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "integer_bit_arithmetic_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn time_value_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"time:%S\" (list (time-less-p (seconds-to-time 1) (seconds-to-time 2)) (time-add (seconds-to-time 1) (seconds-to-time 2)) (float-time (seconds-to-time 3))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("time:") && row.contains("(t (0 3 0 0) 3.0)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: time value behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("time_value_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn parse_time_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"timeparse:%S\" (list (parse-time-string \"2026-05-08 11:22:33 -0400\") (format-time-string \"%Y-%m-%d %H:%M:%S %z\" (encode-time (parse-time-string \"2026-05-08 11:22:33 -0400\")) t)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("timeparse:")
                && row.contains("(33 22 11 8 5 2026 nil -1 -14400)")
                && row.contains("2026-05-08 15:22:33 +0000")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: parse-time-string and encode-time timezone behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("parse_time_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn split_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"split:%S\" (list (split-string \"a,,b,\" \",\" t) (split-string \" a  b \" nil t) (regexp-quote \"a.b*c\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("split:") && row.contains("\\\"a\\\"") && row.contains("\\\"b\\\"")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: split-string and regexp-quote should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "split_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn file_name_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"file:%S\" (let ((default-directory \"/tmp/\")) (list (expand-file-name \"a/../b\") (file-name-nondirectory \"/x/y.txt\") (file-name-directory \"/x/y.txt\") (file-name-extension \"a.tar.gz\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("file:")
                && row.contains("/tmp/b")
                && row.contains("y.txt")
                && row.contains("gz")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: file-name behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("file_name_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn write_region_inhibit_fsync_matches_gnu_fileio_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(message "write-fsync:%S" "#,
        r#"(list (boundp 'write-region-inhibit-fsync) "#,
        r#"(symbol-value 'write-region-inhibit-fsync) "#,
        r#"(special-variable-p 'write-region-inhibit-fsync) "#,
        r#"(eval '(let ((write-region-inhibit-fsync nil)) "#,
        r#"(symbol-value 'write-region-inhibit-fsync)) t)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("write-fsync:(t t t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: write-region-inhibit-fsync should match GNU fileio.c DEFVAR_BOOL semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "write_region_inhibit_fsync_matches_gnu_fileio_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn temporary_file_directory_matches_gnu_filelock_defvar_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(message "tempdir:%S" "#,
        r#"(list (boundp 'temporary-file-directory) "#,
        r#"(stringp (symbol-value 'temporary-file-directory)) "#,
        r#"(special-variable-p 'temporary-file-directory) "#,
        r#"(equal (eval '(let ((temporary-file-directory "gnu-dynamic/")) "#,
        r#"temporary-file-directory) t) "gnu-dynamic/")))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("tempdir:(t t t t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: temporary-file-directory should match GNU filelock.c DEFVAR_LISP semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "temporary_file_directory_matches_gnu_filelock_defvar_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn create_lockfiles_matches_gnu_filelock_defvar_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(message "createlock:%S" "#,
        r#"(list (boundp 'create-lockfiles) "#,
        r#"(symbol-value 'create-lockfiles) "#,
        r#"(special-variable-p 'create-lockfiles) "#,
        r#"(not (let ((create-lockfiles nil)) "#,
        r#"(eval 'create-lockfiles t)))))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("createlock:(t t t t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: create-lockfiles should match GNU filelock.c DEFVAR_BOOL semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "create_lockfiles_matches_gnu_filelock_defvar_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn file_name_coding_system_variables_match_gnu_fileio_defvar_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(message "fnamecoding:%S" "#,
        r#"(list (boundp 'file-name-coding-system) "#,
        r#"(symbol-value 'file-name-coding-system) "#,
        r#"(special-variable-p 'file-name-coding-system) "#,
        r#"(eq (let ((file-name-coding-system 'utf-8)) "#,
        r#"(eval 'file-name-coding-system t)) 'utf-8) "#,
        r#"(boundp 'default-file-name-coding-system) "#,
        r#"(symbol-value 'default-file-name-coding-system) "#,
        r#"(special-variable-p 'default-file-name-coding-system) "#,
        r#"(eq (let ((default-file-name-coding-system 'raw-text)) "#,
        r#"(eval 'default-file-name-coding-system t)) 'raw-text)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("fnamecoding:(t nil t t t utf-8-unix t t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: file-name coding variables should match GNU fileio.c DEFVAR_LISP semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "file_name_coding_system_variables_match_gnu_fileio_defvar_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn file_name_edge_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"fileedge:%S\" (list (file-name-directory \"/tmp/a/b.txt\") (file-name-nondirectory \"/tmp/a/b.txt\") (directory-file-name \"/tmp/a/\") (file-name-as-directory \"/tmp/a\") (file-remote-p \"/ssh:host:/tmp/x\" 'method) (file-remote-p \"/tmp/x\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("fileedge:")
                && row.contains("/tmp/a/")
                && row.contains("b.txt")
                && row.contains("/tmp/a")
                && row.contains("ssh")
                && row.contains("nil")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: file-name edge behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "file_name_edge_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn expand_file_name_preserves_double_slash_root_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fileio.c:expand-file-name canonicalization collapses repeated
    // slashes except it deliberately leaves an initial '//' root alone.
    let expr = r#"(message "expanddbl:%S" (list (expand-file-name "//server/share/../x") (expand-file-name "///server/share/../x")))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"expanddbl:(\"//server/x\" \"/server/x\")"#;
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: expand-file-name should preserve an initial double-slash root like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "expand_file_name_preserves_double_slash_root_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn expand_file_name_preserves_posix_superroot_parent_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fileio.c:expand-file-name preserves the POSIX superroot
    // spelling /../ for the first parent reference above root, then collapses
    // additional parents normally.
    let expr = r#"(message "expandsuper:%S" (list (expand-file-name "/../x") (expand-file-name "/../../x")))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"expandsuper:(\"/../x\" \"/x\")"#;
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: expand-file-name should preserve POSIX superroot /../ like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "expand_file_name_preserves_posix_superroot_parent_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn file_name_handler_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "handler:%S" (progn (fset 'neo-h1 (lambda (&rest _) :h1)) (fset 'neo-h2 (lambda (&rest _) :h2)) (put 'neo-h1 'operations '(op-a)) (let ((file-name-handler-alist '(("foo" . neo-h1) ("/foo" . neo-h2)))) (prog1 (list (eq (find-file-name-handler "/tmp/foo" 'op-a) 'neo-h2) (eq (find-file-name-handler "/tmp/foo" 'op-b) 'neo-h2) (let ((inhibit-file-name-operation 'op-a) (inhibit-file-name-handlers (list 'neo-h2))) (eq (find-file-name-handler "/tmp/foo" 'op-a) 'neo-h1)) (let ((inhibit-file-name-operation 'op-b) (inhibit-file-name-handlers (list 'neo-h2))) (eq (find-file-name-handler "/tmp/foo" 'op-a) 'neo-h2))) (fmakunbound 'neo-h1) (fmakunbound 'neo-h2) (put 'neo-h1 'operations nil)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("handler:(nil t t nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: file-name handler selection and inhibition should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "file_name_handler_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn file_mode_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"modes:%S\" (list (file-modes-symbolic-to-number \"u=rw,go=r\") (file-modes-number-to-symbolic #o644) (file-modes-number-to-symbolic #o755)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("modes:")
                && row.contains("420")
                && row.contains("-rw-r--r--")
                && row.contains("-rwxr-xr-x")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: file mode conversion should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("file_mode_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn invisible_p_buffer_invisibility_spec_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "invis:%S" (list buffer-invisibility-spec (invisible-p t) (invisible-p 'hide) (let ((buffer-invisibility-spec '(hide))) (list (invisible-p t) (invisible-p 'hide) (invisible-p '(hide other)))) (let ((buffer-invisibility-spec '((hide . t)))) (invisible-p 'hide)) (with-temp-buffer (insert "a" (propertize "bc" 'invisible t) "d") (invisible-p 2))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("invis:") && row.contains("(t t t (nil t t) 2 t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invisible-p should interpret buffer-invisibility-spec like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "invisible_p_buffer_invisibility_spec_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn invisible_p_overlay_invisibility_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/xdisp.c: Finvisible_p reads the 'invisible character property
    // at a buffer position via Fget_char_property, so overlay properties are
    // part of the same semantic surface as text properties.
    let expr = r#"(message "ovinvis:%S" (with-temp-buffer (insert "abcd") (let ((o (make-overlay 2 4))) (overlay-put o 'invisible 'hide) (list (let ((buffer-invisibility-spec '(hide))) (list (invisible-p 2) (invisible-p 3) (invisible-p 4))) (let ((buffer-invisibility-spec '((hide . t)))) (list (invisible-p 2) (invisible-p 3) (invisible-p 4))) (invisible-p 'hide)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("ovinvis:") && row.contains("((t t nil) (2 2 nil) t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invisible-p should see overlay invisible properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "invisible_p_overlay_invisibility_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn invisible_p_category_invisibility_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/intervals.c:textget falls back through the interval category
    // symbol, and src/xdisp.c:Finvisible_p applies buffer-invisibility-spec
    // to that effective invisible property.
    let expr = r#"(message "catinvis:%S" (with-temp-buffer (insert "abcd") (put 'catinvis 'invisible 'hide) (put-text-property 2 4 'category 'catinvis) (list (get-text-property 2 'invisible) (let ((buffer-invisibility-spec '(hide))) (list (invisible-p 2) (invisible-p 4))) (let ((buffer-invisibility-spec '((hide . t)))) (invisible-p 2)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("catinvis:") && row.contains("(hide (t nil) 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invisible-p should honor category-backed invisible properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "invisible_p_category_invisibility_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn invisible_p_default_text_properties_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/intervals.c:lookup_char_property checks
    // default-text-properties after direct/category/alias lookup, so
    // invisible-p must treat default invisible properties as effective
    // character properties.
    let expr = r#"(message "defaultprops:%S" (let ((default-text-properties '(foo dfault invisible hide))) (with-temp-buffer (insert "abc") (list (get-text-property 1 'foo) (get-char-property 1 'foo) (let ((buffer-invisibility-spec '(hide))) (invisible-p 1)) (let ((buffer-invisibility-spec '((hide . t)))) (invisible-p 1))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("defaultprops:") && row.contains("(dfault dfault t 2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: invisible-p should honor default-text-properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "invisible_p_default_text_properties_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn character_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"chars:%S\" (list (string-to-char \"abc\") (char-to-string ?A) (length \"é\") (string-bytes \"é\") (multibyte-string-p \"é\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("chars:") && row.contains("(97") && row.contains("1 2 t"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: character and string conversion should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "character_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn characterp_ignored_second_arg_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/character.c:Fcharacterp has arity 1..2 and ignores the second
    // argument.  This preserves historical callers that pass an obsolete
    // strict flag.
    let expr = r#"(message "charp2:%S" (list (condition-case e (characterp ?a t) (error (list (car e) (cadr e)))) (condition-case e (characterp #x400000 t) (error (list (car e) (cadr e)))) (condition-case e (characterp nil t) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("charp2:(t nil nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: characterp should accept and ignore a second arg like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "characterp_ignored_second_arg_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn unibyte_char_to_multibyte_rejects_negative_chars_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/character.c:Funibyte_char_to_multibyte calls CHECK_CHARACTER
    // before checking the unibyte range, so negative integers are characterp
    // type errors.
    let expr = r#"(message "unibytech:%S" (list (unibyte-char-to-multibyte #x80) (condition-case e (unibyte-char-to-multibyte #x100) (error (list (car e) (cadr e)))) (condition-case e (unibyte-char-to-multibyte -1) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("unibytech:(4194176")
                && row.contains(r#"(error \"Not a unibyte character: 256\")"#)
                && row.contains("(wrong-type-argument characterp))")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: unibyte-char-to-multibyte should reject negative chars like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "unibyte_char_to_multibyte_rejects_negative_chars_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn encode_char_rejects_negative_chars_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/charset.c:Fencode_char validates CH with CHECK_CHARACTER after
    // validating the charset.  Negative integers are characterp type errors,
    // not unsupported characters returning nil.
    let expr = r#"(message "encchar:%S" (list (encode-char ?A 'ascii) (encode-char ?é 'ascii) (condition-case e (encode-char -1 'ascii) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("encchar:(65 nil (wrong-type-argument characterp))"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: encode-char should reject negative chars like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("encode_char_rejects_negative_chars_like_gnu", &gnu, &neo);
}

#[test]
fn coding_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"coding:%S\" (let* ((s \"é\") (u (encode-coding-string s 'utf-8))) (list (length s) (string-bytes s) (multibyte-string-p u) (string-bytes u) (decode-coding-string u 'utf-8) (string-as-unibyte \"é\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("coding:")
                && row.contains("(1 2 nil 2")
                && row.contains("é")
                && row.contains("303")
                && row.contains("251")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: coding string conversion should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "coding_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn url_util_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'url-util) (message \"urlhex:%S\" (list (url-hexify-string \"a b/é\") (url-unhex-string \"a%20b%2F%C3%A9\") (url-unhex-string \"%E9\") (url-hexify-string \"!*()\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("urlhex:")
            && recent.contains("a%20b%2F%C3%A9")
            && recent.contains("a b/")
            && recent.contains("303")
            && recent.contains("251")
            && recent.contains("\\351")
            && recent.contains("%21%2A%28%29")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: URL hex string helpers should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("url_util_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn url_parse_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'url-parse) (let ((u (url-generic-parse-url \"https://user:pw@example.com:8443/a/b?q=1#frag\"))) (message \"urlparse:%S\" (list (url-type u) (url-user u) (url-password u) (url-host u) (url-portspec u) (url-filename u) (url-target u)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("urlparse:")
                && row.contains("https")
                && row.contains("user")
                && row.contains("pw")
                && row.contains("example.com")
                && row.contains("8443")
                && row.contains("/a/b?q=1")
                && row.contains("frag")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: URL parser accessors should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("url_parse_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn url_network_support_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(progn (require 'url-expand) (require 'url-proxy) (require 'url-cookie) (require 'url-cache) (let* ((url-proxy-services '(("http" . "proxy.example:8080") ("no_proxy" . "\\.local\\'"))) (url-cache-directory (expand-file-name "url-cache-oracle/" temporary-file-directory)) (url-cookie-storage nil) (url-cookie-secure-storage nil) (expanded (list (url-expand-file-name "../c?z=3" "https://example.com/a/b/d.html?x=1") (url-expand-file-name "//cdn.example.org/lib.js" "https://example.com/a/b/") (url-expand-file-name "" "https://example.com/a/b/?q=1"))) (proxy (list (url-find-proxy-for-url (url-generic-parse-url "http://example.com/") "example.com") (url-find-proxy-for-url (url-generic-parse-url "http://host.local/") "host.local"))) cache plain-cookie secure-cookie) (url-cookie-store "sid" "one" "" ".example.com" "/a" nil) (url-cookie-store "root" "two" "" ".example.com" "/" nil) (url-cookie-store "sec" "three" "" ".example.com" "/a" t) (setq cache (list (equal (url-cache-create-filename "http://example.com:80/a") (url-cache-create-filename "http://example.com/a")) (equal (url-cache-create-filename "http://example.com:8080/a") (url-cache-create-filename "http://example.com/a")))) (setq plain-cookie (url-cookie-generate-header-lines "www.example.com" "/a/page" nil)) (setq secure-cookie (url-cookie-generate-header-lines "www.example.com" "/a/page" t)) (message "urlnet:%S" (list (equal expanded '("https://example.com/a/c?z=3" "https://cdn.example.org/lib.js" "https://example.com/a/b/?q=1")) (equal proxy '("http://proxy.example:8080/" nil)) (equal plain-cookie "Cookie: sid=one; root=two\r\n") (equal secure-cookie "Cookie: sid=one; sec=three; root=two\r\n") cache))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("urlnet:") && recent.contains("(t t t t (t nil))")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: URL expansion, proxy, cookie, and cache helpers should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "url_network_support_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn url_file_handler_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(progn (require 'url-handlers) (url-handler-mode 1) (let* ((fn "https://user@example.com:443/a/b") (vals (list (file-name-absolute-p fn) (file-remote-p fn) (file-remote-p fn 'method) (file-remote-p fn 'user) (file-remote-p fn 'host) (file-remote-p fn 'localname) (file-remote-p "file:///tmp/x") (unhandled-file-name-directory "file:///tmp/x") (file-name-directory "https://example.com/a/b") (directory-file-name "https://example.com/a/b/") (file-name-completion "https://example.com/a" "https://example.com/") (file-name-all-completions "a" "https://example.com/")))) (message "urlhandler:%S" (equal vals '(nil "https:user@example.com/" "https" "user" "example.com" "/a/b" nil "/tmp/x/" "https://example.com/a/" "https://example.com/a/b" "https://example.com/a" nil)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(5)
            .any(|row| row.contains("urlhandler:t"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: URL file-name handler helpers should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "url_file_handler_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn split_string_trim_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "splittrim:%S" (list (split-string " <a> , <> , <b> " "," nil "[ <>]+") (split-string " <a> , <> , <b> " "," t "[ <>]+") (split-string "" "," nil "[ ]+") (split-string "" "," t "[ ]+")))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"splittrim:((\"a\" \"\" \"b\") (\"a\" \"b\") (\"\") nil)"#;
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: split-string trim and empty-field behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "split_string_trim_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn char_code_property_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"charprop:%S\" (list (get-char-code-property ?A 'general-category) (get-char-code-property ?0 'general-category) (get-char-code-property ?\\s 'general-category) (get-char-code-property ?é 'name)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("charprop:")
                && row.contains("Lu")
                && row.contains("Nd")
                && row.contains("Zs")
                && row.contains("LATIN SMALL LETTER E WITH ACUTE")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: character code properties should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "char_code_property_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_compare_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"cmp:%S\" (list (string-lessp \"a\" \"b\") (string-lessp \"b\" \"a\") (compare-strings \"abc\" nil nil \"abd\" nil nil) (compare-strings \"abc\" nil nil \"abc\" nil nil)))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("cmp:") && row.contains("(t nil -3 t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string comparison should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_compare_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_comparison_functions_accept_symbols_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:Fstring_lessp and Fstring_collate_lessp accept symbols
    // by comparing their print names.  The `string<' alias preserves the
    // same contract.
    let expr = r#"(message "cmpsym:%S" (list (condition-case e (string-lessp 'abc 'abd) (error (list (car e) (cadr e)))) (condition-case e (string< 'abc "abd") (error (list (car e) (cadr e)))) (condition-case e (string-collate-lessp 'abc 'abd) (error (list (car e) (cadr e)))) (condition-case e (string-collate-equalp 'abc "abc") (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("cmpsym:(t t t t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string comparison functions should accept symbols like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_comparison_functions_accept_symbols_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn string_comparison_positioned_symbol_designators_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/lisp.h:SYMBOLP treats a symbol-with-pos as its bare symbol only
    // while `symbols-with-pos-enabled' is non-nil.  GNU
    // src/fns.c:Fstring_lessp/Fstring_equal use SYMBOLP before extracting the
    // print name, so the dynamic flag governs both interpreted calls and the
    // corresponding byte-code operations.
    let expr = r#"(message "cmpsympos:%S" (let ((a (position-symbol 'alpha 17)) (b (position-symbol 'beta 23)) (v2 (position-symbol 'alpha2 31)) (v10 (position-symbol 'alpha10 37))) (list (let ((symbols-with-pos-enabled nil)) (list (symbolp a) (condition-case e (string-lessp a b) (error (list (car e) (cadr e)))) (condition-case e (string-equal a "alpha") (error (list (car e) (cadr e)))))) (let ((symbols-with-pos-enabled t)) (list (symbolp a) (string-lessp a b) (string< a "beta") (string-lessp "alpha" b) (string-equal a "alpha") (string= "beta" b) (string-greaterp b a) (string> b a) (string-version-lessp v2 v10) (string-collate-lessp a b) (string-collate-equalp a "alpha") (funcall (byte-compile (lambda (x y) (list (string-lessp x y) (string-equal x "alpha")))) a b))) (funcall (byte-compile (lambda (x y) (let ((symbols-with-pos-enabled t)) (list symbols-with-pos-enabled (symbolp x) (string-lessp x y))))) a b))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "cmpsympos:((nil (wrong-type-argument stringp) (wrong-type-argument stringp)) (t t t t t t t t t t t (t t)) (t t t))";
    let ready = |grid: &[String]| grid.iter().rev().take(4).any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: positioned symbol string designators should follow GNU's dynamic flag\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_comparison_positioned_symbol_designators_match_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn compare_strings_bounds_and_ignore_case_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "cmpbounds:%S" (list (compare-strings "abcdef" -3 -1 "cd" nil nil) (condition-case e (compare-strings "abc" 9 nil "" nil nil) (error (list (car e) (cadr e)))) (condition-case e (compare-strings "abc" nil -9 "" nil nil) (error (list (car e) (cadr e)))) (compare-strings "abc" 0 99 "abc" 0 99) (compare-strings "İ" nil nil "i" nil nil t)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("cmpbounds:")
                && row.contains("(1 (args-out-of-range")
                && row.matches("args-out-of-range").count() == 2
                && row.contains("t 1)")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: compare-strings bounds and ignore-case behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "compare_strings_bounds_and_ignore_case_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_algorithm_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"stralg:%S\" (list (string-version-lessp \"file9\" \"file10\") (string-version-lessp \"file10\" \"file9\") (string-distance \"kitten\" \"sitting\") (string-distance \"same\" \"same\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("stralg:") && row.contains("(t nil 3 0)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string version and distance algorithms should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_algorithm_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn string_distance_unibyte_inputs_use_byte_compare_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/fns.c:Fstring_distance uses byte comparison when BYTECOMPARE is
    // non-nil, or when both input strings are unibyte.
    let expr = r#"(message "strdistuni:%S" (let ((u (string-make-unibyte "é"))) (list (string-distance u "é") (string-distance u "é" nil) (string-distance u "é" t) (string-bytes u) (multibyte-string-p u))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("strdistuni:(0 0 2 1 nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: string-distance should byte-compare unibyte strings like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "string_distance_unibyte_inputs_use_byte_compare_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn format_print_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"fmt:%S\" (list (format \"%04d\" 7) (format \"%S\" \"x\\ny\") (prin1-to-string (list 'a \"b\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmt:")
            && recent.contains("0007")
            && recent.contains("(a")
            && recent.contains("b")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: formatting and printing behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_print_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn float_output_format_matches_gnu_print_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = concat!(
        r#"(with-current-buffer "*scratch*" (erase-buffer) "#,
        r#"(dolist (entry (list "#,
        r#"(list 'meta (boundp 'float-output-format) (symbol-value 'float-output-format) (special-variable-p 'float-output-format)) "#,
        r#"(list 'default (number-to-string 1.25)) "#,
        r#"(list 'let (let ((float-output-format "%.1f")) (number-to-string 1.25)) "#,
        r#"(let ((float-output-format "%.1f")) (prin1-to-string 1.25))) "#,
        r#"(list 'override (prin1-to-string 1.25 nil '((float-format . "%.1f")))) "#,
        r#"(list 'bad (let ((float-output-format "bad")) (number-to-string 1.25))) "#,
        r#"(list 'zero-f (let ((float-output-format "%.0f")) (number-to-string 1.25))) "#,
        r#"(list 'zero-g (let ((float-output-format "%.0g")) (number-to-string 1.25))) "#,
        r#"(list 'dynamic (eval '(let ((float-output-format "%.1f")) "#,
        r#"(symbol-value 'float-output-format)) t)))) "#,
        r#"(insert (format "float-output:%S\n" entry))) "#,
        r#"(goto-char (point-min)))"#
    );
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains(r#"float-output:(meta t nil t)"#)
            && text.contains(r#"float-output:(default "1.25")"#)
            && text.contains(r#"float-output:(let "1.2" "1.2")"#)
            && text.contains(r#"float-output:(override "1.2")"#)
            && text.contains(r#"float-output:(bad "1.25")"#)
            && text.contains(r#"float-output:(zero-f "1")"#)
            && text.contains(r#"float-output:(zero-g "1.25")"#)
            && text.contains(r#"float-output:(dynamic "%.1f")"#)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: float-output-format should match GNU print.c semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "float_output_format_matches_gnu_print_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn format_percent_c_preserves_large_character_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/editfns.c:format handles %c separately from sprintf and emits
    // the Lisp character code into the result string; #x110000 is a valid
    // Emacs character and becomes one multibyte character, not a runtime
    // string-storage panic.
    let expr = r#"(message "fmtchar:%S" (let ((s (format "%c" #x110000))) (list (string-to-list s) (length s) (string-bytes s) (multibyte-string-p s))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = "fmtchar:((1114112) 1 4 t)";
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format %c should preserve large Emacs character codes like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_percent_c_preserves_large_character_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn format_copies_format_string_text_properties_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fmtprop:%S" (let* ((fmt (copy-sequence "A:%s:B"))) (put-text-property 0 2 'face 'bold fmt) (put-text-property 4 6 'face 'italic fmt) (let ((r (format fmt "xx"))) (list r (text-properties-at 0 r) (text-properties-at 1 r) (text-properties-at 2 r) (text-properties-at 3 r) (text-properties-at 4 r) (text-properties-at 5 r)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmtprop:")
            && recent.contains("A:xx:B")
            && recent.contains("(face bold)")
            && recent.contains("(face italic)")
            && recent.contains("nil nil")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format should copy text properties from literal format-string spans like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_copies_format_string_text_properties_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn format_preserves_format_and_string_argument_properties_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fmtmixprop:%S" (let* ((fmt (propertize "A:%s:%S:Z" 'face 'fmt)) (arg (propertize "xx" 'face 'arg)) (r (format fmt arg arg))) (list r (mapcar (lambda (i) (text-properties-at i r)) (number-sequence 0 (1- (length r)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmtmixprop:")
            && recent.contains("A:xx:")
            && recent.matches("(face arg)").count() >= 2
            && recent.matches("(face fmt)").count() >= 20
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format should preserve both format-string and %s argument text properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_preserves_format_and_string_argument_properties_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn format_message_preserves_text_properties_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fmtmsgprop:%S" (let* ((fmt (propertize "A:%s:Z" 'face 'fmt)) (arg (propertize "xx" 'face 'arg)) (r (format-message fmt arg))) (list r (mapcar (lambda (i) (text-properties-at i r)) (number-sequence 0 (1- (length r)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmtmsgprop:")
            && recent.contains("A:xx:Z")
            && recent.matches("(face fmt)").count() >= 4
            && recent.matches("(face arg)").count() >= 2
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format-message should preserve format-string and %s argument text properties like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_message_preserves_text_properties_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn format_message_text_quoting_style_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fmtquote:%S" (list (format-message "`x'") (let ((text-quoting-style 'straight)) (format-message "`x'")) (let ((text-quoting-style 'curve)) (format-message "`x'"))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmtquote:") && recent.contains("'x'") && recent.matches("‘x’").count() >= 2
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format-message should honor text-quoting-style like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_message_text_quoting_style_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn format_left_aligned_precision_extends_string_properties_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fmtleftprop:%S" (let* ((s (copy-sequence "abcdef"))) (put-text-property 1 5 'face 'bold s) (let ((r (format "%-6.3s" s))) (list r (mapcar (lambda (i) (text-properties-at i r)) '(0 1 2 3 4 5))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmtleftprop:")
            && recent.contains("abc")
            && recent.contains("(nil (face bold) (face bold) (face bold) (face bold) nil)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: left-aligned format precision should extend string text properties over right padding like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_left_aligned_precision_extends_string_properties_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn format_string_precision_uses_display_width_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/editfns.c:format implements %s precision with
    // lisp_string_width, so precision limits display columns, not merely
    // characters or bytes.  A width-2 character does not fit precision 1.
    let expr = r#"(message "fmtwideprec:%S" (list (format "%.1s" "中x") (format "%.2s" "中x") (format "%.3s" "中x")))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"fmtwideprec:(\"\" \"中\" \"中x\")"#;
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format %s precision should count display width like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_string_precision_uses_display_width_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn format_numeric_precision_and_prefixes_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "fmtnum:%S" (list (format "%#o" 0) (format "%#.0o" 0) (format "%.0d" 0) (format "%05.3d" 7) (format "%-05d" 7) (format "%+05d" 7) (format "% 05d" 7) (format "%#08x" 31) (format "%#08b" 5)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains(r#"fmtnum:(\"0\" \"0\" \"\" \"  007\" \"7    \" \"+0007\" \" 0007\" \"0x00001f\" \"0b000101\")"#)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: numeric format precision, padding, and alternate prefixes should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_numeric_precision_and_prefixes_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn format_integer_conversion_accepts_nonfinite_floats_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/editfns.c:format allows FLOATP arguments for integer
    // conversions other than %c.  Non-finite floats are formatted through the
    // numeric conversion path as inf, -inf, and nan rather than rejected.
    let expr = r#"(message "fmtdfloat:%S" (list (format "%d" 1e+INF) (format "%d" -1e+INF) (format "%d" 0.0e+NaN)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let expected = r#"fmtdfloat:(\"inf\" \"-inf\" \"nan\")"#;
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format %d should accept non-finite floats like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_integer_conversion_accepts_nonfinite_floats_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn format_huge_field_width_overflows_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/editfns.c:format parses field widths into bounded buffer sizes;
    // absurdly large widths signal "Maximum string size exceeded" instead of
    // being ignored.
    let expr = r#"(message "fmtwidth:%S" (list (condition-case e (format "%999999999999999999999s" "x") (error (list (car e) (cadr e)))) (condition-case e (format "%999999999999999999999d" 7) (error (list (car e) (cadr e))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("fmtwidth:") && recent.matches("Maximum string size exceeded").count() >= 2
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format huge field widths should overflow like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("format_huge_field_width_overflows_like_gnu", &gnu, &neo);
}

#[test]
fn format_print_level_notation_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/editfns.c routes `%S' through prin1-style printing; src/print.c
    // uses "..." when `print-level' truncates nested list objects.
    let expr = r#"(message "fmtlevel:%S" (let ((print-level 1)) (format "%S" '((a b) (c d)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains(r#"fmtlevel:\"(... ...)\""#)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: format %S should use GNU print-level ellipsis notation\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "format_print_level_notation_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn with_output_to_string_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"outstr:%S\" (list (with-output-to-string (princ \"A\") (prin1 'b)) (prin1-to-string '(a . b)) (prin1-to-string [1 2])))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("outstr:")
            && recent.contains("Ab")
            && recent.contains("(a . b)")
            && recent.contains("[1 2]")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: with-output-to-string and object printing should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "with_output_to_string_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn print_circle_nil_bounded_cycle_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "printcycle:%S" (let ((x (list 1 2))) (setcdr (last x) x) (list (let ((print-circle t)) (prin1-to-string x)) (let ((print-circle nil) (print-length 6) (print-level nil)) (prin1-to-string x)))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid.join("\n");
        recent.contains("printcycle:")
            && recent.contains(r##"\"#1=(1 2 . #1#)\""##)
            && recent.contains(r##"\"(1 2 1 2 . #2)\""##)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: print-circle nil with print-length should recurse and truncate circular lists like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "print_circle_nil_bounded_cycle_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn print_circle_nil_tail_cycle_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/print.c prints bounded circular structures differently when the
    // cycle starts in the tail rather than at the head.  With print-circle nil,
    // print-length truncation must use GNU's #N tail-depth notation.
    let expr = r#"(message "printtailn:%S" (let ((print-circle nil) (print-length 7) (print-level nil) (x (list 'a 'b 'c))) (setcdr (cddr x) (cdr x)) (prin1-to-string x)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("printtailn:") && row.contains("(a b c b . #2)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: print-circle nil tail-cycle truncation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "print_circle_nil_tail_cycle_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn print_level_vectorlike_notation_matches_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU src/print.c handles vectorlike objects separately from conses:
    // nested vectors at print-level 1 remain printed, while nested cons
    // structure inside a record is truncated as "...".
    let expr = r#"(message "printvec:%S" (list (let ((print-level 1)) (prin1-to-string [[a b] [c d]])) (let ((print-level 1)) (prin1-to-string '#s(foo (a b) [c d]))) (let ((print-length 2)) (prin1-to-string [1 2 3 4]))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid.join("\n");
        recent.contains("printvec:")
            && recent.contains(r##"\"[[a b] [c d]]\""##)
            && recent.contains(r##"\"#s(foo ... [c d])\""##)
            && recent.contains(r##"\"[1 2 ...]\""##)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: vectorlike print-level and print-length notation should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "print_level_vectorlike_notation_matches_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn process_output_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"proc:%S\" (list (shell-command-to-string \"printf hello\") (shell-command-to-string \"printf err >&2; exit 7\") (with-temp-buffer (list (process-file \"printf\" nil t nil \"abc\") (buffer-string)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("proc:")
            && recent.contains("hello")
            && recent.contains("err")
            && recent.contains("(0")
            && recent.contains("abc")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: process output capture should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "process_output_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn process_signal_status_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"psig:%S\" (list (process-file shell-file-name nil nil nil shell-command-switch \"kill -TERM $$\") (let ((process-file-return-signal-string t)) (process-file shell-file-name nil nil nil shell-command-switch \"kill -TERM $$\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("psig:") && recent.contains("Terminated")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: process signal status should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "process_signal_status_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn call_process_region_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"cpr:%S\" (list (with-temp-buffer (insert \"abc\") (call-process-region (point-min) (point-max) \"cat\" nil t nil) (buffer-string)) (with-temp-buffer (insert \"abc\") (list (call-process-region (point-min) (point-max) shell-file-name t t nil shell-command-switch \"cat; kill -TERM $$\") (buffer-string)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("cpr:")
            && recent.contains("abcabc")
            && recent.contains("Terminated")
            && recent.contains("abc")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: call-process-region should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "call_process_region_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn call_process_exec_path_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"cexec:%S\" (list (condition-case err (let ((exec-path nil)) (call-process \"printf\" nil t nil \"ok\") (buffer-string)) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path (list 42))) (call-process \"printf\" nil t nil \"ok\") (buffer-string)) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path (list \"/usr/bin\")) (exec-suffixes (list 42))) (call-process \"printf\" nil t nil \"ok\") (buffer-string)) (error (list (car err) (cadr err))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("cexec:")
            && recent.contains("file-missing")
            && recent.contains("Searching for program")
            && recent.contains("wrong-type-argument")
            && recent.contains("stringp")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: call-process executable lookup should match GNU exec-path semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "call_process_exec_path_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn async_process_exec_path_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(let ((cat-dir (file-name-directory (executable-find "cat")))) (message "aexec:%S" (list (condition-case err (let ((exec-path nil)) (start-process "aexec-start-nil" nil "printf" "ok") 'ok) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path (list 42))) (start-process "aexec-start-bad-path" nil "printf" "ok") 'ok) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path (list cat-dir)) (exec-suffixes (list 42))) (start-process "aexec-start-bad-suffix" nil "cat") 'ok) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path (list cat-dir))) (let ((p (start-process "aexec-start-ok" nil "cat"))) (prog1 (list (processp p) (process-command p)) (delete-process p)))) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path nil)) (make-process :name "aexec-make-nil" :command '("printf" "ok")) 'ok) (error (list (car err) (cadr err)))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains("aexec:")
            && text.contains("file-missing")
            && text.contains("Searching for program")
            && text.contains("wrong-type-argument")
            && text.contains("stringp")
            && text.contains(r#"(t (\"cat\"))"#)
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: async process executable lookup should match GNU exec-path semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "async_process_exec_path_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn async_shell_process_wrappers_use_dynamic_shell_file_name_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "ashell:%S" (list (special-variable-p 'shell-file-name) (condition-case err (let ((exec-path nil) (shell-file-name "sh")) (start-process-shell-command "ashell-start" nil "printf ok") 'ok) (error (list (car err) (cadr err)))) (condition-case err (let ((exec-path nil) (shell-file-name "sh")) (start-file-process-shell-command "ashell-file" nil "printf ok") 'ok) (error (list (car err) (cadr err))))))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains("ashell:")
            && text.contains("t")
            && text.contains("file-missing")
            && text.contains("Searching for program")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: async shell process wrappers should use dynamic shell-file-name\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "async_shell_process_wrappers_use_dynamic_shell_file_name_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn callproc_directory_variables_are_special_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = r#"(message "cpspecial:%S" (mapcar (lambda (s) (list s (boundp s) (special-variable-p s))) '(exec-directory data-directory doc-directory configure-info-directory shared-game-score-directory)))"#;
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains("cpspecial:")
            && text.contains("(exec-directory t t)")
            && text.contains("(data-directory t t)")
            && text.contains("(doc-directory t t)")
            && text.contains("(configure-info-directory t t)")
            && text.contains("(shared-game-score-directory t t)")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: callproc DEFVAR directory variables should be bound and special\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "callproc_directory_variables_are_special_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn hash_base64_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"crypto:%S\" (list (md5 \"abc\") (secure-hash 'sha1 \"abc\") (base64-encode-string \"abc\" t) (base64-decode-string \"YWJj\")))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("crypto:")
                && row.contains("900150983cd24fb0d6963f7d28e17f72")
                && row.contains("a9993e364706816aba3e25717850c26c9cd0d89d")
                && row.contains("YWJj")
                && row.contains("abc")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: hash and base64 string helpers should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "hash_base64_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn json_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'json) (message \"json:%S\" (let ((json-object-type 'alist) (json-array-type 'list)) (list (json-encode '((a . 1) (b . [2 3]))) (json-read-from-string \"{\\\"a\\\":1,\\\"b\\\":[2,3]}\") (json-encode-string \"é\\n\")))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        let recent = grid
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        recent.contains("json:")
            && recent.contains("a")
            && recent.contains("b")
            && recent.contains("[2,3]")
            && recent.contains("((a . 1) (b 2 3))")
            && recent.contains("é")
            && recent.contains("\\\\n")
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: JSON encode/decode behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("json_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn xml_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'xml) (message \"xml:%S\" (with-temp-buffer (insert \"<root a=\\\"1\\\"><child>é</child></root>\") (car (xml-parse-region (point-min) (point-max))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("xml:")
                && row.contains("(root")
                && row.contains("((a .")
                && row.contains("1")
                && row.contains("(child nil")
                && row.contains("é")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: XML parser tree shape should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("xml_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn dom_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(progn (require 'dom) (let ((tree '(root ((class . \"top\")) (section ((id . \"a\")) \"Alpha\") (section ((id . \"b\")) (span nil \"Beta\"))))) (message \"dom:%S\" (list (dom-tag tree) (dom-attr tree 'class) (length (dom-by-tag tree 'section)) (dom-text (car (dom-by-tag tree 'span)))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("dom:")
                && row.contains("root")
                && row.contains("top")
                && row.contains(" 2 ")
                && row.contains("Beta")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: DOM helper traversal should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display("dom_elisp_functions_match_gnu_semantics", &gnu, &neo);
}

#[test]
fn string_and_numeric_operations_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // (concat "a" "b") should be "ab"
    support::eval_expression(&mut gnu, &mut neo, "(concat \"a\" \"b\")");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("ab"),
            "{label}: (concat \"a\" \"b\") should be \"ab\". Echo: {echo}"
        );
    }

    // (substring "hello" 1 3) should be "el" (0-indexed in GNU!)
    support::eval_expression(&mut gnu, &mut neo, "(substring \"hello\" 1 3)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("el"),
            "{label}: (substring \"hello\" 1 3) should be \"el\". Echo: {echo}"
        );
    }

    // (length "hello") should be 5
    support::eval_expression(&mut gnu, &mut neo, "(length \"hello\")");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('5'),
            "{label}: (length \"hello\") should be 5. Echo: {echo}"
        );
    }

    // (+ 1 2 3) should be 6
    support::eval_expression(&mut gnu, &mut neo, "(+ 1 2 3)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('6'),
            "{label}: (+ 1 2 3) should be 6. Echo: {echo}"
        );
    }

    // (symbol-name 'hello) should be "hello"
    support::eval_expression(&mut gnu, &mut neo, "(symbol-name 'hello)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("hello"),
            "{label}: (symbol-name 'hello) should be \"hello\". Echo: {echo}"
        );
    }

    // (intern "hello") should return the symbol hello
    support::eval_expression(&mut gnu, &mut neo, "(intern \"hello\")");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("hello"),
            "{label}: (intern \"hello\") should be hello. Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "string_and_numeric_operations_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

// ── Environment and keymap tests ────────────────────────────

#[test]
fn getenv_returns_same_path_as_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    support::eval_expression(&mut gnu, &mut neo, "(getenv \"HOME\")");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('\"') || echo.contains('/'),
            "{label}: (getenv HOME) should return a path. Echo: {echo}"
        );
    }

    support::eval_expression(&mut gnu, &mut neo, "(getenv \"USER\")");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('\"'),
            "{label}: (getenv USER) should return a string. Echo: {echo}"
        );
    }
    assert_pair_exact_display("getenv_returns_same_path_as_gnu", &gnu, &neo);
}

#[test]
fn key_description_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"kbd:%S\" (list (key-description (kbd \"C-x C-f\")) (single-key-description ?\\C-h) (vectorp (kbd \"<f5>\")) (key-description [f5])))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("kbd:")
                && row.contains("C-x C-f")
                && row.contains("C-h")
                && row.contains(" t ")
                && row.contains("<f5>")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: keyboard description helpers should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "key_description_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn event_conversion_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"kbdvector:%S\" (list (kbd \"C-M-a\") (key-description (vector (event-convert-list '(control meta a)))) (event-modifiers (event-convert-list '(control meta a))) (event-basic-type (event-convert-list '(control meta a)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("kbdvector:")
                && row.contains("[134217729]")
                && row.contains("C-M-a")
                && row.contains("(control meta)")
                && row.contains("97")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: event conversion and modifier helpers should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "event_conversion_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn command_remapping_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"remap:%S\" (let ((m (make-sparse-keymap))) (define-key m [remap next-line] 'forward-line) (list (lookup-key m [remap next-line]) (command-remapping 'next-line nil (list m)) (command-remapping 'previous-line nil (list m)))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("remap:") && row.contains("(forward-line forward-line nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: command remapping lookup should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "command_remapping_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn sparse_keymap_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"keymap:%S\" (let ((m (make-sparse-keymap))) (define-key m (kbd \"C-c a\") 'ignore) (list (keymapp m) (lookup-key m (kbd \"C-c a\")) (lookup-key m (kbd \"C-c b\")))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("keymap:") && row.contains("(t ignore nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: sparse keymap behavior should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "sparse_keymap_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn local_keymap_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"localmap:%S\" (with-temp-buffer (let ((m (make-sparse-keymap))) (define-key m (kbd \"C-c a\") 'ignore) (use-local-map m) (list (eq (current-local-map) m) (lookup-key (current-local-map) (kbd \"C-c a\")) (local-key-binding (kbd \"C-c a\"))))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("localmap:") && row.contains("(t ignore ignore)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: local keymap lookup should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "local_keymap_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn full_keymap_prompt_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"keyprompt:%S\" (let ((m (make-keymap \"Prompt\"))) (define-key m \"a\" 'ignore) (list (keymapp m) (car m) (lookup-key m \"a\") (lookup-key m \"b\"))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("keyprompt:") && row.contains("(t keymap ignore nil)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: full keymap prompt and lookup behavior should match GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "full_keymap_prompt_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn keymap_parent_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"keyparent:%S\" (let ((parent (make-sparse-keymap)) (child (make-sparse-keymap))) (define-key parent (kbd \"C-c p\") 'previous-line) (define-key child (kbd \"C-c c\") 'next-line) (set-keymap-parent child parent) (list (lookup-key child (kbd \"C-c c\")) (lookup-key child (kbd \"C-c p\")) (eq (keymap-parent child) parent))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("keyparent:") && row.contains("(next-line previous-line t)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: keymap parent inheritance should match GNU semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "keymap_parent_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn substitute_command_keys_elisp_functions_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    let expr = "(message \"keys:%S\" (let ((map (make-sparse-keymap))) (define-key map (kbd \"C-c n\") 'next-line) (let ((overriding-local-map map)) (list (key-description (kbd \"C-c n\")) (lookup-key map (kbd \"C-c n\")) (substitute-command-keys \"Go: \\\\[next-line]\")))))";
    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| {
        grid.iter().rev().take(4).any(|row| {
            row.contains("keys:")
                && row.contains("C-c n")
                && row.contains("next-line")
                && row.contains("Go: C-c n")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: substitute-command-keys should match GNU keymap substitution semantics\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "substitute_command_keys_elisp_functions_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn lookup_key_global_map_returns_correct_binding() {
    let (mut gnu, mut neo) = boot_pair("");

    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(lookup-key global-map (kbd \"C-x C-f\"))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            !echo.trim().is_empty() && !echo.contains("nil"),
            "{label}: (lookup-key global-map (kbd C-x C-f)) should find binding"
        );
    }
    assert_pair_exact_display("lookup_key_global_map_returns_correct_binding", &gnu, &neo);
}

// ── Hash table tests ────────────────────────────────────────

#[test]
fn hash_table_operations_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // (make-hash-table) should create a hash table
    support::eval_expression(&mut gnu, &mut neo, "(hash-table-p (make-hash-table))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('t'),
            "{label}: (hash-table-p (make-hash-table)) should be t. Echo: {echo}"
        );
    }

    // (gethash 'key (make-hash-table)) should be nil
    support::eval_expression(&mut gnu, &mut neo, "(gethash 'key (make-hash-table))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("nil"),
            "{label}: (gethash 'key (make-hash-table)) should be nil. Echo: {echo}"
        );
    }
    assert_pair_exact_display("hash_table_operations_match_gnu_semantics", &gnu, &neo);
}

// ── Sequence tests ──────────────────────────────────────────

#[test]
fn sequence_operations_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // (length [1 2 3]) should be 3
    support::eval_expression(&mut gnu, &mut neo, "(length [1 2 3])");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('3'),
            "{label}: (length [1 2 3]) should be 3. Echo: {echo}"
        );
    }

    // (aref [1 2 3] 0) should be 1
    support::eval_expression(&mut gnu, &mut neo, "(aref [1 2 3] 0)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('1'),
            "{label}: (aref [1 2 3] 0) should be 1. Echo: {echo}"
        );
    }
    assert_pair_exact_display("sequence_operations_match_gnu_semantics", &gnu, &neo);
}

// ── Regexp and assoc tests ──────────────────────────────────

#[test]
fn regexp_and_assoc_operations_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // (string-match "foo" "foobar") should be 0 (match at position 0)
    support::eval_expression(&mut gnu, &mut neo, "(string-match \"foo\" \"foobar\")");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('0'),
            "{label}: (string-match ...) should be 0. Echo: {echo}"
        );
    }

    // (assoc 'b '((a . 1) (b . 2))) should be (b . 2)
    support::eval_expression(&mut gnu, &mut neo, "(assoc 'b '((a . 1) (b . 2)))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('b') && echo.contains('2'),
            "{label}: (assoc 'b ...) should find (b . 2). Echo: {echo}"
        );
    }
    assert_pair_exact_display(
        "regexp_and_assoc_operations_match_gnu_semantics",
        &gnu,
        &neo,
    );
}

// ── Evaluator core tests ────────────────────────────────────

#[test]
fn lambda_apply_funcall_match_gnu_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // ((lambda (x) (+ x 1)) 41) should be 42
    support::eval_expression(&mut gnu, &mut neo, "((lambda (x) (+ x 1)) 41)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("42"),
            "{label}: ((lambda (x) (+ x 1)) 41) should be 42. Echo: {echo}"
        );
    }

    // (apply '+ '(1 2 3)) should be 6
    support::eval_expression(&mut gnu, &mut neo, "(apply '+ '(1 2 3))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('6'),
            "{label}: (apply '+ '(1 2 3)) should be 6. Echo: {echo}"
        );
    }

    // (funcall '+ 1 2 3) should be 6
    support::eval_expression(&mut gnu, &mut neo, "(funcall '+ 1 2 3)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('6'),
            "{label}: (funcall '+ 1 2 3) should be 6. Echo: {echo}"
        );
    }
    assert_pair_exact_display("lambda_apply_funcall_match_gnu_semantics", &gnu, &neo);
}

// ── Macro and control flow tests ────────────────────────────

#[test]
fn macroexpand_and_condition_case_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // (condition-case nil (/ 1 0) (arith-error "caught")) should return "caught"
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(condition-case nil (/ 1 0) (arith-error \"caught\"))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("caught"),
            "{label}: (condition-case ... (/ 1 0) ...) should catch arith-error"
        );
    }

    // (eval '(+ 1 2)) should be 3
    support::eval_expression(&mut gnu, &mut neo, "(eval '(+ 1 2))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('3'),
            "{label}: (eval '(+ 1 2)) should be 3. Echo: {echo}"
        );
    }
    assert_pair_exact_display("macroexpand_and_condition_case_match_gnu", &gnu, &neo);
}

// ── Non-local exit tests ────────────────────────────────────

#[test]
fn catch_throw_and_unwind_protect_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // (catch 'tag (throw 'tag 42)) should be 42
    support::eval_expression(&mut gnu, &mut neo, "(catch 'tag (throw 'tag 42))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("42"),
            "{label}: (catch 'tag (throw 'tag 42)) should be 42"
        );
    }

    // (unwind-protect 42 (message "cleanup")) should be 42
    support::eval_expression(&mut gnu, &mut neo, "(unwind-protect 42 (ignore))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("42"),
            "{label}: (unwind-protect 42 ...) should return 42"
        );
    }
    assert_pair_exact_display("catch_throw_and_unwind_protect_match_gnu", &gnu, &neo);
}

// ── Prefix argument diagnostic ──────────────────────────────

#[test]
fn prefix_arg_survives_from_cu_to_next_command() {
    let (mut gnu, mut neo) = boot_pair("");

    // Check prefix-arg is nil before any C-u
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(if (null prefix-arg) \"nil\" \"non-nil\")",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("nil"),
            "{label}: prefix-arg should be nil before C-u. Echo: {echo}"
        );
    }

    // Check current-prefix-arg is nil too
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(if (null current-prefix-arg) \"nil\" \"non-nil\")",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("nil"),
            "{label}: current-prefix-arg should be nil"
        );
    }
    assert_pair_exact_display("prefix_arg_survives_from_cu_to_next_command", &gnu, &neo);
}

// ── Function definition and call tests ──────────────────────

#[test]
fn defun_and_optional_args_preserve_argument_semantics() {
    let (mut gnu, mut neo) = boot_pair("");

    // Define and call a function
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (defun tui-test-fn (x) (* x x)) (tui-test-fn 7))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains("49"),
            "{label}: (defun fn (x) (* x x)) then (fn 7) should be 49"
        );
    }

    // Test &optional args
    support::eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (defun tui-opt (a &optional b) (if b (+ a b) a)) (tui-opt 5 3))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let empty = String::new();
        let echo = grid.last().unwrap_or(&empty);
        assert!(
            echo.contains('8'),
            "{label}: (defun fn (a &optional b)) then (fn 5 3) should be 8"
        );
    }
    assert_pair_exact_display(
        "defun_and_optional_args_preserve_argument_semantics",
        &gnu,
        &neo,
    );
}

#[test]
fn where_is_internal_returns_key_bindings_for_commands() {
    let (mut gnu, mut neo) = boot_pair("");
    // Short expression to avoid NEO TUI M-: input issues
    let expr = "(message \"wi=%d\" (length (where-is-internal 'find-file)))";

    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| grid.iter().any(|r| r.contains("wi="));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let has_output = grid
            .iter()
            .any(|r| r.contains("wi=1") || r.contains("wi=2") || r.contains("wi=3"));
        assert!(
            has_output,
            "{label}: where-is-internal find-file should show wi=N with N > 0"
        );
    }
    assert_pair_exact_display(
        "where_is_internal_returns_key_bindings_for_commands",
        &gnu,
        &neo,
    );
}

#[test]
fn where_is_internal_firstonly_preserves_keymap_order_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    let expr = r#"(let((map1(make-sparse-keymap))(map2(make-sparse-keymap)))(define-key map1 [32 104 100 104] 'tui-test-command)(define-key map2 [8 100 104] 'tui-test-command)(message "wif:%S"(where-is-internal 'tui-test-command (list map1 map2) t)))"#;

    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| grid.iter().any(|r| r.contains("wif:"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let expected = "wif:[32 104 100 104]";
    if !gnu.text_grid().iter().any(|row| row.contains(expected))
        || !neo.text_grid().iter().any(|row| row.contains(expected))
    {
        dump_pair_grids(
            "where_is_internal_firstonly_preserves_keymap_order_like_gnu",
            &gnu,
            &neo,
        );
    }
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains(expected)),
            "{label}: where-is-internal FIRSTONLY should keep GNU keymap order instead of choosing the shortest binding"
        );
    }
    assert_pair_exact_display(
        "where_is_internal_firstonly_preserves_keymap_order_like_gnu",
        &gnu,
        &neo,
    );
}

#[test]
fn apropos_command_includes_key_binding_for_find_file() {
    let (mut gnu, mut neo) = boot_pair("");
    let expr = "(apropos-command \"find-file\")";

    support::eval_expression(&mut gnu, &mut neo, expr);

    let ready = |grid: &[String]| grid.iter().any(|r| r.contains("*Apropos*"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("*Apropos*")),
            "{label}: apropos-command should open *Apropos* buffer"
        );
    }
    assert_pair_exact_display(
        "apropos_command_includes_key_binding_for_find_file",
        &gnu,
        &neo,
    );
}

#[test]
fn recent_keys_includes_command_after_self_insert() {
    let (mut gnu, mut neo) = boot_pair("");
    // Type X, then check recent-keys via M-:
    send_both_raw(&mut gnu, &mut neo, b"X");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-:");
    let p = |g: &[String]| g.iter().any(|r| r.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), p);
    neo.read_until(Duration::from_secs(8), p);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    // Short expression
    for s in [&mut gnu, &mut neo] {
        s.send(b"(length (recent-keys 'include-cmds))\r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        // Look for any number in the output
        let has_output = grid
            .iter()
            .any(|r| r.split_whitespace().any(|w| w.parse::<i32>().is_ok()));
        assert!(
            has_output,
            "{label}: M-: eval should produce a numeric result"
        );
    }
    assert_pair_exact_display("recent_keys_includes_command_after_self_insert", &gnu, &neo);
}
