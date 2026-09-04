#![cfg(unix)]
//! TUI comparison tests: mark region fill.

mod support;
use std::time::Duration;
use support::*;

// ── Tests ──────────────────────────────────────────────────
#[test]
fn mark_sexp_via_cmeta_spc_then_kill_region() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "mark-sexp.el",
        "(alpha beta) gamma\n",
        "C-x C-f",
    );

    // C-M-SPC is ESC followed by C-@ (NUL) in a terminal.
    send_both_raw(&mut gnu, &mut neo, b"\x1b\x00");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("! gamma"))
            && !grid.iter().any(|row| row.contains("(alpha beta) gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("mark_sexp_via_cmeta_spc_then_kill_region", &gnu, &neo);
}

#[test]
fn mark_defun_via_cmeta_h_then_kill_region() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "mark-defun.el",
        "(defun first ()\n  1)\n\n(defun second ()\n  2)\n\nafter\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> C-M-a C-M-h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(defun first ()"))
            && grid.iter().any(|row| row.contains("!"))
            && grid.iter().any(|row| row.contains("after"))
            && !grid.iter().any(|row| row.contains("defun second"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("mark_defun_via_cmeta_h_then_kill_region", &gnu, &neo);
}

#[test]
fn pop_global_mark_via_cx_cspc_returns_to_marked_buffer_position() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "global-mark-ring.txt",
        "alpha one\nbeta two\ngamma three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-a C-SPC M-> C-x C-SPC");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha one"))
            && grid.iter().any(|row| row.contains("!beta two"))
            && grid.iter().any(|row| row.contains("gamma three"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "pop_global_mark_via_cx_cspc_returns_to_marked_buffer_position",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "pop_global_mark_via_cx_cspc_returns_to_marked_buffer_position",
        &mut gnu,
        &mut neo,
        "global-mark-ring.txt",
        "alpha one\n!beta two\ngamma three\n",
    );
}

#[test]
fn pop_local_mark_via_cu_cspc_returns_to_current_buffer_mark() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "local-mark-ring.txt",
        "alpha one\nbeta two\ngamma three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC C-n C-a C-SPC M-> C-u C-SPC");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha one"))
            && grid.iter().any(|row| row.contains("!beta two"))
            && grid.iter().any(|row| row.contains("gamma three"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "pop_local_mark_via_cu_cspc_returns_to_current_buffer_mark",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "pop_local_mark_via_cu_cspc_returns_to_current_buffer_mark",
        &mut gnu,
        &mut neo,
        "local-mark-ring.txt",
        "alpha one\n!beta two\ngamma three\n",
    );
}

#[test]
fn set_mark_command_repeat_pop_repeats_local_mark_pop_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "repeat-pop-mark-ring.txt",
        "alpha one\nbeta two\ngamma three\ndelta four\n",
        "C-x C-f",
    );
    eval_expression(&mut gnu, &mut neo, "(setq set-mark-command-repeat-pop t)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(
        &mut gnu,
        &mut neo,
        "C-SPC C-n C-a C-SPC C-n C-a C-SPC M-> C-u C-SPC C-SPC",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha one"))
            && grid.iter().any(|row| row.contains("!beta two"))
            && grid.iter().any(|row| row.contains("gamma three"))
            && grid.iter().any(|row| row.contains("delta four"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "set_mark_command_repeat_pop_repeats_local_mark_pop_like_gnu",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "set_mark_command_repeat_pop_repeats_local_mark_pop_like_gnu",
        &mut gnu,
        &mut neo,
        "repeat-pop-mark-ring.txt",
        "alpha one\n!beta two\ngamma three\ndelta four\n",
    );
}

#[test]
fn pop_global_mark_widens_to_reach_mark_outside_narrowing_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "global-mark-widen.txt",
        "alpha one\nbeta two\ngamma three\ndelta four\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC C-n C-n C-a C-SPC M-> C-x n n");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Use this command?"))
            || grid
                .iter()
                .any(|row| row.contains("disabled command narrow-to-region"))
    };
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(12), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("narrow-to-region")),
            "{label} should show disabled-command help for narrow-to-region"
        );
        assert!(
            grid.iter().any(|row| row.contains("Use this command?")),
            "{label} should ask before running disabled narrow-to-region"
        );
    }

    send_both_raw(&mut gnu, &mut neo, b" ");
    let narrowed_ready = |grid: &[String]| {
        !grid.iter().any(|row| row.contains("alpha one"))
            && !grid.iter().any(|row| row.contains("beta two"))
            && grid.iter().any(|row| row.contains("gamma three"))
            && grid.iter().any(|row| row.contains("delta four"))
    };
    gnu.read_until(Duration::from_secs(6), narrowed_ready);
    neo.read_until(Duration::from_secs(8), narrowed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-SPC");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("!alpha one"))
            && grid.iter().any(|row| row.contains("beta two"))
            && grid.iter().any(|row| row.contains("gamma three"))
            && grid.iter().any(|row| row.contains("delta four"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "pop_global_mark_widens_to_reach_mark_outside_narrowing_like_gnu",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "pop_global_mark_widens_to_reach_mark_outside_narrowing_like_gnu",
        &mut gnu,
        &mut neo,
        "global-mark-widen.txt",
        "!alpha one\nbeta two\ngamma three\ndelta four\n",
    );
}

#[test]
fn narrow_to_defun_once_then_widen_via_cx_n_d_w() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "narrow-defun.el",
        "(defun first ()\n  1)\n\n(defun second ()\n  2)\n\nafter\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> C-M-a C-n C-x n d");
    let narrowed_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(defun second ()"))
            && grid.iter().any(|row| row.contains("  2)"))
            && !grid.iter().any(|row| row.contains("(defun first ()"))
            && !grid.iter().any(|row| row.contains("after"))
    };
    gnu.read_until(Duration::from_secs(8), narrowed_ready);
    neo.read_until(Duration::from_secs(12), narrowed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "narrow_to_defun_once_then_widen_via_cx_n_d_w/narrowed",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-x n w M-<");
    let widened_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(defun first ()"))
            && grid.iter().any(|row| row.contains("(defun second ()"))
            && grid.iter().any(|row| row.contains("after"))
    };
    gnu.read_until(Duration::from_secs(6), widened_ready);
    neo.read_until(Duration::from_secs(8), widened_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("narrow_to_defun_once_then_widen_via_cx_n_d_w", &gnu, &neo);
}

#[test]
fn set_fill_column_then_fill_paragraph_via_cx_f_mq() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "fill-paragraph.txt",
        "alpha beta gamma delta epsilon zeta eta theta iota kappa\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x f");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "set_fill_column_then_fill_paragraph_via_cx_f_mq/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"20");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-q");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta gamma"))
            && grid.iter().any(|row| row.contains("delta epsilon zeta"))
            && grid.iter().any(|row| row.contains("eta theta iota kappa"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "set_fill_column_then_fill_paragraph_via_cx_f_mq",
        &gnu,
        &neo,
    );
}

#[test]
fn set_fill_column_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "fill-column-empty-del.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x f");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty set-fill-column prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "set_fill_column_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn auto_fill_mode_wraps_inserted_text_after_fill_column() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "auto-fill.txt", "seed\n", "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-x h C-w");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-x f");
    let fill_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
    };
    gnu.read_until(Duration::from_secs(6), fill_prompt);
    neo.read_until(Duration::from_secs(8), fill_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"20");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    invoke_mx_command(&mut gnu, &mut neo, "auto-fill-mode");
    let enabled = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Auto-Fill mode enabled"))
    };
    gnu.read_until(Duration::from_secs(6), enabled);
    neo.read_until(Duration::from_secs(8), enabled);
    assert!(
        enabled(&gnu.text_grid()),
        "GNU should report Auto Fill mode enabled\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        enabled(&neo.text_grid()),
        "Neomacs should report Auto Fill mode enabled\n{}",
        neo.text_grid().join("\n")
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha beta gamma delta epsilon zeta ");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta gamma"))
            && grid.iter().any(|row| row.contains("delta epsilon zeta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    assert!(
        ready(&gnu.text_grid()),
        "GNU should auto-fill inserted text\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        ready(&neo.text_grid()),
        "Neomacs should auto-fill inserted text\n{}",
        neo.text_grid().join("\n")
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "auto_fill_mode_wraps_inserted_text_after_fill_column",
        &gnu,
        &neo,
    );
}

#[test]
fn set_fill_prefix_then_fill_paragraph_via_cx_dot_mq() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "fill-prefix.txt",
        "> alpha beta gamma delta epsilon zeta eta theta iota kappa lambda\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x f");
    let fill_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
    };
    gnu.read_until(Duration::from_secs(6), fill_prompt);
    neo.read_until(Duration::from_secs(8), fill_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"26");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-< C-f C-f C-x .");
    let prefix_ready = |grid: &[String]| grid.iter().any(|row| row.contains("fill-prefix:"));
    gnu.read_until(Duration::from_secs(6), prefix_ready);
    neo.read_until(Duration::from_secs(8), prefix_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-q");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("> alpha beta gamma"))
            && grid.iter().any(|row| row.contains("> delta epsilon zeta"))
            && grid
                .iter()
                .any(|row| row.contains("> eta theta iota kappa"))
            && grid.iter().any(|row| row.contains("> lambda"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "set_fill_prefix_then_fill_paragraph_via_cx_dot_mq",
        &gnu,
        &neo,
    );
}

#[test]
fn center_line_via_mx_uses_fill_column() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "center-line.txt", "title\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "C-x f");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"20");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "center-line");

    let ready = |grid: &[String]| grid.iter().any(|row| row.starts_with("       title"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("center_line_via_mx_uses_fill_column", &gnu, &neo);
    save_current_file_and_assert_contents(
        "center_line_via_mx_uses_fill_column",
        &mut gnu,
        &mut neo,
        "center-line.txt",
        "       title\n",
    );
}

#[test]
fn fill_region_via_mx_wraps_active_region() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "fill-region.txt",
        "alpha beta gamma delta epsilon zeta eta theta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x f");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Change fill-column from"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"20");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "fill-region");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.trim() == "alpha beta gamma")
            && grid.iter().any(|row| row.trim() == "delta epsilon zeta")
            && grid.iter().any(|row| row.trim() == "eta theta")
            && !grid
                .iter()
                .any(|row| row.contains("alpha beta gamma delta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("fill_region_via_mx_wraps_active_region", &gnu, &neo);
    save_current_file_and_assert_contents(
        "fill_region_via_mx_wraps_active_region",
        &mut gnu,
        &mut neo,
        "fill-region.txt",
        "alpha beta gamma\ndelta epsilon zeta\neta theta\n",
    );
}

#[test]
fn set_variable_fill_column_via_mx_updates_buffer_local_value() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "set-variable");
    let variable_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Set variable"));
    gnu.read_until(Duration::from_secs(6), variable_prompt);
    neo.read_until(Duration::from_secs(8), variable_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "set_variable_fill_column_via_mx_updates_buffer_local_value/variable_prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"fill-column");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let value_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Set fill-column") && row.contains("to value"))
    };
    gnu.read_until(Duration::from_secs(6), value_prompt);
    neo.read_until(Duration::from_secs(8), value_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("Set fill-column") && row.contains("to value")),
            "{label} should prompt for fill-column's new value\n{}",
            grid.join("\n")
        );
    }

    for session in [&mut gnu, &mut neo] {
        session.send(b"55");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-:");
    let eval_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), eval_prompt);
    neo.read_until(Duration::from_secs(8), eval_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "fill-column=%S" fill-column)"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("fill-column=55"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "set_variable_fill_column_via_mx_updates_buffer_local_value",
        &gnu,
        &neo,
    );
}

#[test]
fn recenter_top_bottom_cycle_via_cl() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=80 {
        contents.push_str(&format!("recenter line {line:02}\n"));
    }
    open_home_file(
        &mut gnu,
        &mut neo,
        "recenter-usage.txt",
        &contents,
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-s");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"recenter line 40");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");

    let found = |grid: &[String]| grid.iter().any(|row| row.contains("recenter line 40"));
    gnu.read_until(Duration::from_secs(6), found);
    neo.read_until(Duration::from_secs(8), found);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-l");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("recenter_top_bottom_cycle_via_cl/middle", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "C-l");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("recenter_top_bottom_cycle_via_cl/top", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "C-l");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("recenter_top_bottom_cycle_via_cl/bottom", &gnu, &neo);
}

#[test]
fn delete_trailing_whitespace_via_mx_cleans_buffer_before_save() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-trailing-whitespace.txt",
        "alpha   \nbeta\t \n\n\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "delete-trailing-whitespace");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha"))
            && grid.iter().any(|row| row.contains("beta"))
            && grid
                .iter()
                .any(|row| row.contains("delete-trailing-whitespace.txt"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "delete_trailing_whitespace_via_mx_cleans_buffer_before_save",
        &mut gnu,
        &mut neo,
        "delete-trailing-whitespace.txt",
        "alpha\nbeta\n",
    );
    assert_pair_exact_display(
        "delete_trailing_whitespace_via_mx_cleans_buffer_before_save",
        &gnu,
        &neo,
    );
}

#[test]
fn untabify_region_via_mx_expands_tabs_before_save() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "untabify-region.txt",
        "a\tb\n\tindent\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "untabify");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("a       b"))
            && grid.iter().any(|row| row.contains("        indent"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "untabify_region_via_mx_expands_tabs_before_save",
        &mut gnu,
        &mut neo,
        "untabify-region.txt",
        "a       b\n        indent\n",
    );
    assert_pair_exact_display(
        "untabify_region_via_mx_expands_tabs_before_save",
        &gnu,
        &neo,
    );
}

#[test]
fn tabify_region_via_mx_converts_spaces_before_save() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "tabify-region.txt",
        "a       b\n        indent\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "tabify");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("a       b"))
            && grid.iter().any(|row| row.contains("        indent"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "tabify_region_via_mx_converts_spaces_before_save",
        &mut gnu,
        &mut neo,
        "tabify-region.txt",
        "a\tb\n\tindent\n",
    );
    assert_pair_exact_display(
        "tabify_region_via_mx_converts_spaces_before_save",
        &gnu,
        &neo,
    );
}

#[test]
fn indent_rigidly_with_prefix_via_mx_indents_region() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "indent-rigidly.txt",
        "alpha\nbeta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h C-u 4");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "indent-rigidly");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("    alpha"))
            && grid.iter().any(|row| row.contains("    beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "indent_rigidly_with_prefix_via_mx_indents_region",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "indent_rigidly_with_prefix_via_mx_indents_region",
        &mut gnu,
        &mut neo,
        "indent-rigidly.txt",
        "    alpha\n    beta\n",
    );
}

#[test]
fn indent_region_elisp_via_cmeta_backslash() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "indent-region.el",
        "(defun sample ()\n(message \"alpha\")\n(when t\n(message \"beta\")))\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    // C-M-\ is ESC followed by C-\ (0x1c).
    send_both_raw(&mut gnu, &mut neo, b"\x1b\x1c");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("  (message \"alpha\")"))
            && grid.iter().any(|row| row.contains("  (when t"))
            && grid
                .iter()
                .any(|row| row.contains("    (message \"beta\")"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "indent_region_elisp_via_cmeta_backslash",
        &mut gnu,
        &mut neo,
        "indent-region.el",
        "(defun sample ()\n  (message \"alpha\")\n  (when t\n    (message \"beta\")))\n",
    );
    assert_pair_exact_display("indent_region_elisp_via_cmeta_backslash", &gnu, &neo);
}

#[test]
fn set_mark_command_then_kill_region_via_cspc_mf_cw() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "set-mark-kill-region.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both_raw(&mut gnu, &mut neo, &[0]);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains(" beta gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "set_mark_command_then_kill_region_via_cspc_mf_cw",
        &gnu,
        &neo,
    );
}

#[test]
fn mark_word_then_kill_region_via_mat_cw() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "mark-word.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-@ C-w");
    let killed = |grid: &[String]| {
        grid.iter().any(|row| row.contains(" beta gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-e SPC C-y");
    let yanked = |grid: &[String]| grid.iter().any(|row| row.contains(" beta gamma alpha"));
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("mark_word_then_kill_region_via_mat_cw", &gnu, &neo);
}

#[test]
fn mark_whole_buffer_then_kill_and_yank_via_cx_h_cw_cy() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "mark-whole-buffer.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");

    let killed = |grid: &[String]| {
        !grid.iter().any(|row| row.contains("alpha line"))
            && !grid.iter().any(|row| row.contains("beta line"))
    };
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-y");
    let restored = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha line"))
            && grid.iter().any(|row| row.contains("beta line"))
    };
    gnu.read_until(Duration::from_secs(6), restored);
    neo.read_until(Duration::from_secs(8), restored);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "mark_whole_buffer_then_kill_and_yank_via_cx_h_cw_cy",
        &gnu,
        &neo,
    );
}

#[test]
fn mark_page_then_kill_region_via_cx_cp_cw() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "mark-page.txt",
        "first-a\nfirst-b\n\x0c\nsecond-a\nsecond-b\n\x0c\nthird-a\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-< C-s second-a RET C-x C-p");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("first-a"))
            && grid.iter().any(|row| row.contains("third-a"))
            && !grid.iter().any(|row| row.contains("second-a"))
            && !grid.iter().any(|row| row.contains("second-b"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("mark_page_then_kill_region_via_cx_cp_cw", &gnu, &neo);
}

#[test]
fn narrow_to_region_once_then_widen_via_cx_n_n_w() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "narrow-widen.txt",
        "top line\nmiddle visible\nbottom line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-SPC C-n C-x n n");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Use this command?"))
            || grid
                .iter()
                .any(|row| row.contains("disabled command narrow-to-region"))
    };
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(12), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("narrow-to-region")),
            "{label} should show disabled-command help for narrow-to-region"
        );
        assert!(
            grid.iter().any(|row| row.contains("Use this command?")),
            "{label} should ask before running disabled narrow-to-region"
        );
    }

    send_both_raw(&mut gnu, &mut neo, b" ");
    let narrowed_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("middle visible"))
            && !grid.iter().any(|row| row.contains("top line"))
            && !grid.iter().any(|row| row.contains("bottom line"))
    };
    gnu.read_until(Duration::from_secs(8), narrowed_ready);
    neo.read_until(Duration::from_secs(12), narrowed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "narrow_to_region_once_then_widen_via_cx_n_n_w/narrowed",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-x n w");
    let widened_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("top line"))
            && grid.iter().any(|row| row.contains("middle visible"))
            && grid.iter().any(|row| row.contains("bottom line"))
    };
    gnu.read_until(Duration::from_secs(6), widened_ready);
    neo.read_until(Duration::from_secs(8), widened_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("narrow_to_region_once_then_widen_via_cx_n_n_w", &gnu, &neo);
}

#[test]
fn narrow_to_page_once_then_widen_via_cx_n_p_w() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "narrow-page.txt",
        "first page a\nfirst page b\n\x0c\nsecond page a\nsecond page b\n\x0c\nthird page a\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-< C-s second page a RET C-x n p");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Use this command?"))
            || grid
                .iter()
                .any(|row| row.contains("disabled command narrow-to-page"))
    };
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(12), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("narrow-to-page")),
            "{label} should show disabled-command help for narrow-to-page"
        );
        assert!(
            grid.iter().any(|row| row.contains("Use this command?")),
            "{label} should ask before running disabled narrow-to-page"
        );
    }

    send_both_raw(&mut gnu, &mut neo, b" ");
    let narrowed_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("second page a"))
            && grid.iter().any(|row| row.contains("second page b"))
            && !grid.iter().any(|row| row.contains("first page a"))
            && !grid.iter().any(|row| row.contains("third page a"))
    };
    gnu.read_until(Duration::from_secs(8), narrowed_ready);
    neo.read_until(Duration::from_secs(12), narrowed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "narrow_to_page_once_then_widen_via_cx_n_p_w/narrowed",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-x n w");
    send_both(&mut gnu, &mut neo, "M-<");
    let widened_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("first page a"))
            && grid.iter().any(|row| row.contains("second page a"))
            && grid.iter().any(|row| row.contains("third page a"))
    };
    gnu.read_until(Duration::from_secs(6), widened_ready);
    neo.read_until(Duration::from_secs(8), widened_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("narrow_to_page_once_then_widen_via_cx_n_p_w", &gnu, &neo);
}
