//! TUI comparison tests: editing motion.

mod support;
use std::fs;
use std::time::Duration;
use support::*;

// ── Tests ──────────────────────────────────────────────────
#[test]
fn region_kill_yank_and_undo_round_trip() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-yank-undo.txt",
        "one two three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC M-f C-w");
    let killed = |grid: &[String]| {
        grid.iter().any(|row| row.contains(" two three"))
            && !grid.iter().any(|row| row.contains("one two three"))
    };
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_nearly_matches("region_kill_yank_and_undo_round_trip/killed", &gnu, &neo, 2);

    send_both(&mut gnu, &mut neo, "C-y");
    let yanked = |grid: &[String]| grid.iter().any(|row| row.contains("one two three"));
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_nearly_matches("region_kill_yank_and_undo_round_trip/yanked", &gnu, &neo, 2);

    send_both(&mut gnu, &mut neo, "C-/");
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_nearly_matches(
        "region_kill_yank_and_undo_round_trip/undo-yank",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "C-y");
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    save_current_file_and_assert_contents(
        "region_kill_yank_and_undo_round_trip",
        &mut gnu,
        &mut neo,
        "kill-yank-undo.txt",
        "one two three\n",
    );
}

#[test]
fn dabbrev_expand_via_mslash_expands_previous_word() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "dabbrev-usage.txt",
        "dynamic-expansion\n\n dyn",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> M-/");
    let ready = |grid: &[String]| {
        grid.iter()
            .filter(|row| row.contains("dynamic-expansion"))
            .count()
            >= 2
            && !grid.iter().any(|row| row.trim_end().ends_with(" dyn"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "dabbrev_expand_via_mslash_expands_previous_word",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn kill_region_and_yank_via_cw_cy() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "cut-yank.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-@");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-y");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha line"))
            && grid.iter().any(|row| row.contains("beta line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("kill_region_and_yank_via_cw_cy", &gnu, &neo, 2);
}

#[test]
fn next_line_add_newlines_inserts_newline_at_eob_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "next-line-add-newlines.txt",
        "alpha",
        "C-x C-f",
    );
    eval_expression(&mut gnu, &mut neo, "(setq next-line-add-newlines t)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-> C-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha")) && grid.iter().any(|row| row.contains("!"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "next_line_add_newlines_inserts_newline_at_eob_like_gnu",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "next_line_add_newlines_inserts_newline_at_eob_like_gnu",
        &mut gnu,
        &mut neo,
        "next-line-add-newlines.txt",
        "alpha\n!\n",
    );
}

#[test]
fn undo_edit_via_cx_u() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "undo-usage.txt",
        "alpha line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"omega line");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x u");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("undo-usage.txt"))
            && grid.iter().any(|row| row.contains("alpha line"))
            && !grid.iter().any(|row| row.contains("omega line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("undo_edit_via_cx_u", &gnu, &neo, 2);
}

#[test]
fn repeated_cx_u_continues_undo_chain_across_edit_boundaries() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "undo-cx-u-chain.txt";
    let original = "one\ntwo\n";

    open_home_file(&mut gnu, &mut neo, name, original, "C-x C-f");

    for session in [&mut gnu, &mut neo] {
        session.send(b"AAA ");
    }
    let first_edit = |grid: &[String]| grid.iter().any(|row| row.contains("AAA one"));
    gnu.read_until(Duration::from_secs(6), first_edit);
    neo.read_until(Duration::from_secs(8), first_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-a C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"BBB ");
    }
    let second_edit = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), second_edit);
    neo.read_until(Duration::from_secs(8), second_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-x u");
    let first_undo = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.trim_end() == "two")
            && !grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), first_undo);
    neo.read_until(Duration::from_secs(8), first_undo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "repeated_cx_u_continues_undo_chain_across_edit_boundaries/first",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "C-x u");
    let second_undo = |grid: &[String]| {
        grid.iter().any(|row| row.trim_end() == "one")
            && grid.iter().any(|row| row.trim_end() == "two")
            && !grid.iter().any(|row| row.contains("AAA one"))
            && !grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), second_undo);
    neo.read_until(Duration::from_secs(8), second_undo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "repeated_cx_u_continues_undo_chain_across_edit_boundaries/second",
        &gnu,
        &neo,
        2,
    );

    save_current_file_and_assert_contents(
        "repeated_cx_u_continues_undo_chain_across_edit_boundaries",
        &mut gnu,
        &mut neo,
        name,
        original,
    );
}

#[test]
fn cx_u_with_active_region_undoes_only_region_change() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "undo-region.txt";

    open_home_file(&mut gnu, &mut neo, name, "one\ntwo\n", "C-x C-f");

    for session in [&mut gnu, &mut neo] {
        session.send(b"AAA ");
    }
    let first_edit = |grid: &[String]| grid.iter().any(|row| row.contains("AAA one"));
    gnu.read_until(Duration::from_secs(6), first_edit);
    neo.read_until(Duration::from_secs(8), first_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-a C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"BBB ");
    }
    let second_edit = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), second_edit);
    neo.read_until(Duration::from_secs(8), second_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-a C-SPC C-e C-x u");
    let region_undo = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.trim_end() == "two")
            && !grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), region_undo);
    neo.read_until(Duration::from_secs(8), region_undo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "cx_u_with_active_region_undoes_only_region_change",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    save_current_file_and_assert_contents(
        "cx_u_with_active_region_undoes_only_region_change",
        &mut gnu,
        &mut neo,
        name,
        "AAA one\ntwo\n",
    );
}

#[test]
fn non_undo_command_after_cx_u_breaks_consecutive_undo_chain() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "undo-chain-break.txt";

    open_home_file(&mut gnu, &mut neo, name, "one\ntwo\n", "C-x C-f");

    for session in [&mut gnu, &mut neo] {
        session.send(b"AAA ");
    }
    let first_edit = |grid: &[String]| grid.iter().any(|row| row.contains("AAA one"));
    gnu.read_until(Duration::from_secs(6), first_edit);
    neo.read_until(Duration::from_secs(8), first_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-a C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"BBB ");
    }
    let second_edit = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), second_edit);
    neo.read_until(Duration::from_secs(8), second_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-x u");
    let first_undo = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.trim_end() == "two")
            && !grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), first_undo);
    neo.read_until(Duration::from_secs(8), first_undo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-f C-x u");
    gnu.read_until(Duration::from_secs(6), second_edit);
    neo.read_until(Duration::from_secs(8), second_edit);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "non_undo_command_after_cx_u_breaks_consecutive_undo_chain",
        &gnu,
        &neo,
        2,
    );

    save_current_file_and_assert_contents(
        "non_undo_command_after_cx_u_breaks_consecutive_undo_chain",
        &mut gnu,
        &mut neo,
        name,
        "AAA one\nBBB two\n",
    );
}

fn send_undo_key_both(
    gnu: &mut neomacs_tui_tests::TuiSession,
    neo: &mut neomacs_tui_tests::TuiSession,
    key: &str,
) {
    send_both(gnu, neo, key);
}

fn assert_repeated_undo_in_scratch_buffer_matches_gnu(label: &str, undo_key: &str) {
    let (mut gnu, mut neo) = boot_pair("");
    let original = "one\ntwo";

    send_both(&mut gnu, &mut neo, "C-x h C-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"one\rtwo");
    }
    let original_ready = |grid: &[String]| {
        grid.iter().any(|row| row.trim_end() == "one")
            && grid.iter().any(|row| row.trim_end() == "two")
    };
    gnu.read_until(Duration::from_secs(6), original_ready);
    neo.read_until(Duration::from_secs(8), original_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"AAA ");
    }
    let first_edit = |grid: &[String]| grid.iter().any(|row| row.contains("AAA one"));
    gnu.read_until(Duration::from_secs(6), first_edit);
    neo.read_until(Duration::from_secs(8), first_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-a C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"BBB ");
    }
    let second_edit = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), second_edit);
    neo.read_until(Duration::from_secs(8), second_edit);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_undo_key_both(&mut gnu, &mut neo, undo_key);
    let first_undo = |grid: &[String]| {
        grid.iter().any(|row| row.contains("AAA one"))
            && grid.iter().any(|row| row.trim_end() == "two")
            && !grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), first_undo);
    neo.read_until(Duration::from_secs(8), first_undo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(&format!("{label}/first"), &gnu, &neo, 2);

    send_undo_key_both(&mut gnu, &mut neo, undo_key);
    let second_undo = |grid: &[String]| {
        grid.iter().any(|row| row.trim_end() == "one")
            && grid.iter().any(|row| row.trim_end() == "two")
            && !grid.iter().any(|row| row.contains("AAA one"))
            && !grid.iter().any(|row| row.contains("BBB two"))
    };
    gnu.read_until(Duration::from_secs(6), second_undo);
    neo.read_until(Duration::from_secs(8), second_undo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(&format!("{label}/second"), &gnu, &neo, 2);

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(with-current-buffer "*scratch*" (write-region (point-min) (point-max) "~/scratch-cx-u-chain.txt" nil 'silent))"#,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_eq!(
        fs::read_to_string(gnu.home_dir().join("scratch-cx-u-chain.txt")).expect("read GNU dump"),
        original,
        "GNU scratch buffer contents after repeated C-x u"
    );
    assert_eq!(
        fs::read_to_string(neo.home_dir().join("scratch-cx-u-chain.txt"))
            .expect("read Neomacs dump"),
        original,
        "Neomacs scratch buffer contents after repeated C-x u"
    );
}

#[test]
fn repeated_cx_u_in_scratch_buffer_keeps_non_file_undo_semantics() {
    assert_repeated_undo_in_scratch_buffer_matches_gnu(
        "repeated_cx_u_in_scratch_buffer_keeps_non_file_undo_semantics",
        "C-x u",
    );
}

#[test]
fn repeated_cslash_in_scratch_buffer_keeps_non_file_undo_semantics() {
    assert_repeated_undo_in_scratch_buffer_matches_gnu(
        "repeated_cslash_in_scratch_buffer_keeps_non_file_undo_semantics",
        "C-/",
    );
}

#[test]
fn repeated_cunderscore_in_scratch_buffer_keeps_non_file_undo_semantics() {
    assert_repeated_undo_in_scratch_buffer_matches_gnu(
        "repeated_cunderscore_in_scratch_buffer_keeps_non_file_undo_semantics",
        "C-_",
    );
}

#[test]
fn scroll_page_down_and_up_via_cv_mv() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=80 {
        contents.push_str(&format!("scroll line {line:02}\n"));
    }
    open_home_file(&mut gnu, &mut neo, "scroll-usage.txt", &contents, "C-x C-f");

    send_both(&mut gnu, &mut neo, "C-v");
    let paged = |grid: &[String]| grid.iter().any(|row| row.contains("scroll line 20"));
    gnu.read_until(Duration::from_secs(6), paged);
    neo.read_until(Duration::from_secs(8), paged);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-v");
    let returned = |grid: &[String]| grid.iter().any(|row| row.contains("scroll line 01"));
    gnu.read_until(Duration::from_secs(6), returned);
    neo.read_until(Duration::from_secs(8), returned);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("scroll_page_down_and_up_via_cv_mv", &gnu, &neo, 2);
}

#[test]
fn goto_buffer_end_and_beginning_via_mgt_mlt() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=60 {
        contents.push_str(&format!("edge line {line:02}\n"));
    }
    open_home_file(&mut gnu, &mut neo, "edge-usage.txt", &contents, "C-x C-f");

    send_both(&mut gnu, &mut neo, "M->");
    let at_end = |grid: &[String]| grid.iter().any(|row| row.contains("edge line 60"));
    gnu.read_until(Duration::from_secs(6), at_end);
    neo.read_until(Duration::from_secs(8), at_end);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-<");
    let at_start = |grid: &[String]| grid.iter().any(|row| row.contains("edge line 01"));
    gnu.read_until(Duration::from_secs(6), at_start);
    neo.read_until(Duration::from_secs(8), at_start);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("goto_buffer_end_and_beginning_via_mgt_mlt", &gnu, &neo, 2);
}

#[test]
fn move_beginning_and_end_of_line_via_ca_ce() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "line-motion.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b" END");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"BEGIN ");
    }

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("BEGIN alpha beta gamma END"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("move_beginning_and_end_of_line_via_ca_ce", &gnu, &neo, 2);
}

#[test]
fn delete_char_and_delete_backward_char_via_cd_del() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "delete-char.txt", "alpha\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "C-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-d");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "DEL");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("aha")) && !grid.iter().any(|row| row.contains("alpha"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "delete_char_and_delete_backward_char_via_cd_del",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn next_and_previous_line_via_cn_cp() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "line-step.txt",
        "line one\nline two\nline three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-n C-a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"THREE ");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-p C-a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"TWO ");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("line one"))
            && grid.iter().any(|row| row.contains("TWO line two"))
            && grid.iter().any(|row| row.contains("THREE line three"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("next_and_previous_line_via_cn_cp", &gnu, &neo, 2);
}

#[test]
fn next_line_from_fifth_scratch_character_preserves_column() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x h C-w");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"abcdef\r123456");
    }
    let text_ready = |grid: &[String]| {
        grid.iter().any(|row| row.trim_end() == "abcdef")
            && grid.iter().any(|row| row.trim_end() == "123456")
    };
    gnu.read_until(Duration::from_secs(6), text_ready);
    neo.read_until(Duration::from_secs(8), text_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-< C-f C-f C-f C-f C-n M-:");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(message "scratch-c-n:%S" (list (line-number-at-pos) (current-column) temporary-goal-column (point)))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("scratch-c-n:(2 4") || row.contains("scratch-c-n: (2 4"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: C-n from the fifth visible scratch character should land on line 2 column 4\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "next_line_from_fifth_scratch_character_preserves_column",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn repeated_next_line_restores_goal_column_after_short_line() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x h C-w");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"abcdef\rxy\r123456");
    }
    let text_ready = |grid: &[String]| {
        grid.iter().any(|row| row.trim_end() == "abcdef")
            && grid.iter().any(|row| row.trim_end() == "xy")
            && grid.iter().any(|row| row.trim_end() == "123456")
    };
    gnu.read_until(Duration::from_secs(6), text_ready);
    neo.read_until(Duration::from_secs(8), text_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-< C-f C-f C-f C-f C-n C-n M-:");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(message "scratch-c-n-short:%S" (list (line-number-at-pos) (current-column) temporary-goal-column (point)))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("scratch-c-n-short:(3 4") || row.contains("scratch-c-n-short: (3 4")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: repeated C-n should preserve the original goal column through a short line\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "repeated_next_line_restores_goal_column_after_short_line",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn repeated_previous_line_restores_goal_column_after_short_line() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x h C-w");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"123456\rxy\rabcdef");
    }
    let text_ready = |grid: &[String]| {
        grid.iter().any(|row| row.trim_end() == "123456")
            && grid.iter().any(|row| row.trim_end() == "xy")
            && grid.iter().any(|row| row.trim_end() == "abcdef")
    };
    gnu.read_until(Duration::from_secs(6), text_ready);
    neo.read_until(Duration::from_secs(8), text_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-> C-a C-f C-f C-f C-f C-p C-p M-:");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(message "scratch-c-p-short:%S" (list (line-number-at-pos) (current-column) temporary-goal-column (point)))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("scratch-c-p-short:(1 4") || row.contains("scratch-c-p-short: (1 4")
        })
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: repeated C-p should preserve the original goal column through a short line\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "repeated_previous_line_restores_goal_column_after_short_line",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn set_goal_column_via_cx_cn_guides_vertical_motion() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "goal-column.txt",
        "abcdef\nxy\n123456\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-f C-f C-x C-n");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Use this command?"))
            || grid
                .iter()
                .any(|row| row.contains("disabled command set-goal-column"))
    };
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(12), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("set-goal-column")),
            "{label} should show disabled-command help for set-goal-column"
        );
        assert!(
            grid.iter().any(|row| row.contains("Use this command?")),
            "{label} should ask before running disabled set-goal-column"
        );
    }

    send_both_raw(&mut gnu, &mut neo, b" ");
    let goal_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Goal column 4"));
    gnu.read_until(Duration::from_secs(8), goal_ready);
    neo.read_until(Duration::from_secs(12), goal_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-n C-n M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "goal-column-motion %S/%S" goal-column (current-column))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let motion_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("goal-column-motion 4/4"))
    };
    gnu.read_until(Duration::from_secs(6), motion_ready);
    neo.read_until(Duration::from_secs(8), motion_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            motion_ready(&grid),
            "{label} should preserve the goal column across vertical motion:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_nearly_matches(
        "set_goal_column_via_cx_cn_guides_vertical_motion",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn forward_and_backward_char_via_cf_cb() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "char-motion.txt", "alpha\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "C-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"X");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-b C-b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"Y");
    }

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("aYlXpha"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("forward_and_backward_char_via_cf_cb", &gnu, &neo, 2);
}

#[test]
fn transpose_chars_via_ct() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "transpose-chars.txt",
        "acb\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-t");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("abc")) && !grid.iter().any(|row| row.contains("acb"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("transpose_chars_via_ct", &gnu, &neo, 2);
}

#[test]
fn forward_and_backward_word_via_mf_mb() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "word-motion.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b" END");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-b M-b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"MID ");
    }

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("alpha MID beta END gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("forward_and_backward_word_via_mf_mb", &gnu, &neo, 2);
}

#[test]
fn backward_kill_word_via_esc_del() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "backward-kill-word.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f M-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, &[0x1b, 0x7f]);

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("backward_kill_word_via_esc_del", &gnu, &neo, 2);
}

#[test]
fn alt_backspace_raw_escape_delete_kills_previous_word() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "alt-backspace.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both_raw(&mut gnu, &mut neo, &[0x1b, 0x7f]);

    let killed = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    save_current_file_and_assert_contents(
        "alt_backspace_raw_escape_delete_kills_previous_word",
        &mut gnu,
        &mut neo,
        "alt-backspace.txt",
        "alpha beta \n",
    );
}

#[test]
fn forward_and_backward_sentence_via_me_ma() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sentence-motion.txt",
        "Alpha one. Beta two. Gamma three.\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"[[E]]");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"[[A]]");
    }

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("[[A]]") && row.contains("[[E]]"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("forward_and_backward_sentence_via_me_ma", &gnu, &neo, 2);
}

#[test]
fn forward_and_backward_sexp_via_cmeta_f_b() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sexp-motion.el",
        "(alpha beta) gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let forward_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("(alpha beta)X gamma"));
    gnu.read_until(Duration::from_secs(6), forward_ready);
    neo.read_until(Duration::from_secs(8), forward_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "DEL C-M-b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"^");

    let backward_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("^(alpha beta) gamma"))
            && !grid.iter().any(|row| row.contains("(alpha beta)X gamma"))
    };
    gnu.read_until(Duration::from_secs(6), backward_ready);
    neo.read_until(Duration::from_secs(8), backward_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("forward_and_backward_sexp_via_cmeta_f_b", &gnu, &neo, 2);
}

#[test]
fn kill_sexp_via_cmeta_k_kills_following_expression() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-sexp.el",
        "(alpha beta) gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-M-k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("! gamma"))
            && !grid.iter().any(|row| row.contains("(alpha beta) gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "kill_sexp_via_cmeta_k_kills_following_expression",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn transpose_sexps_via_cmeta_t_swaps_adjacent_expressions() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "transpose-sexps.el",
        "(foo) (bar)\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-< C-M-f C-M-t");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(bar) (foo)"))
            && !grid.iter().any(|row| row.contains("(foo) (bar)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "transpose_sexps_via_cmeta_t_swaps_adjacent_expressions",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "transpose_sexps_via_cmeta_t_swaps_adjacent_expressions",
        &mut gnu,
        &mut neo,
        "transpose-sexps.el",
        "(bar) (foo)\n",
    );
}

#[test]
fn down_list_and_backward_up_list_via_cmeta_d_u() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "list-depth.el",
        "(outer (inner item) tail)\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-M-d");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"^");

    let down_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("(^outer (inner item) tail)"))
    };
    gnu.read_until(Duration::from_secs(6), down_ready);
    neo.read_until(Duration::from_secs(8), down_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "DEL C-M-u");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let up_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("!(outer (inner item) tail)"))
            && !grid
                .iter()
                .any(|row| row.contains("(^outer (inner item) tail)"))
    };
    gnu.read_until(Duration::from_secs(6), up_ready);
    neo.read_until(Duration::from_secs(8), up_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "down_list_and_backward_up_list_via_cmeta_d_u",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn forward_and_backward_list_via_cmeta_n_p() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "list-motion.el",
        "(one (nested)) (two)\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-M-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"^");

    let forward_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("(one (nested))^ (two)"));
    gnu.read_until(Duration::from_secs(6), forward_ready);
    neo.read_until(Duration::from_secs(8), forward_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "DEL C-M-p");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let backward_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("!(one (nested)) (two)"))
            && !grid.iter().any(|row| row.contains("(one (nested))^ (two)"))
    };
    gnu.read_until(Duration::from_secs(6), backward_ready);
    neo.read_until(Duration::from_secs(8), backward_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("forward_and_backward_list_via_cmeta_n_p", &gnu, &neo, 2);
}

#[test]
fn insert_parentheses_via_m_open_paren() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "insert-parentheses.el",
        "alpha\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-(");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"beta");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(beta) alpha"))
            && !grid.iter().any(|row| row.contains("betaalpha"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("insert_parentheses_via_m_open_paren", &gnu, &neo, 2);
}

#[test]
fn move_past_close_and_reindent_via_m_close_paren() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "move-past-close.el",
        "(progn\n  (message \"x\")\n  )tail\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-< C-n C-n M-)");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(progn"))
            && grid.iter().any(|row| row.contains("  (message \"x\"))"))
            && grid.iter().any(|row| row.contains("tail"))
            && !grid.iter().any(|row| row.contains("  )tail"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "move_past_close_and_reindent_via_m_close_paren",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn beginning_and_end_of_defun_via_cmeta_a_e() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "defun-motion.el",
        "(defun first ()\n  1)\n\n(defun second ()\n  2)\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> C-M-a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"^");

    let beginning_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("^(defun second ()"));
    gnu.read_until(Duration::from_secs(6), beginning_ready);
    neo.read_until(Duration::from_secs(8), beginning_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "DEL C-M-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let end_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(defun first ()"))
            && grid.iter().any(|row| row.contains("(defun second ()"))
            && grid.iter().any(|row| row.contains("  2)!"))
    };
    gnu.read_until(Duration::from_secs(6), end_ready);
    neo.read_until(Duration::from_secs(8), end_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("beginning_and_end_of_defun_via_cmeta_a_e", &gnu, &neo, 2);
}

#[test]
fn open_line_via_co() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "open-line.txt",
        "beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha"))
            && grid.iter().any(|row| row.contains("beta gamma"))
            && !grid.iter().any(|row| row.contains("alphabeta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("open_line_via_co", &gnu, &neo, 2);
}

#[test]
fn split_line_via_cmeta_o_moves_tail_down() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "split-line.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-< M-f C-M-o");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha"))
            && grid.iter().any(|row| row.contains("      beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("split_line_via_cmeta_o_moves_tail_down", &gnu, &neo, 2);
    save_current_file_and_assert_contents(
        "split_line_via_cmeta_o_moves_tail_down",
        &mut gnu,
        &mut neo,
        "split-line.txt",
        "alpha \n      beta gamma\n",
    );
}

#[test]
fn tab_to_tab_stop_via_mi_inserts_to_next_tab_column() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "tab-to-tab-stop.txt", "ab\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "C-f C-f M-i");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"cd");
    }

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("ab") && row.contains("cd"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "tab_to_tab_stop_via_mi_inserts_to_next_tab_column",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "tab_to_tab_stop_via_mi_inserts_to_next_tab_column",
        &mut gnu,
        &mut neo,
        "tab-to-tab-stop.txt",
        "ab\tcd\n",
    );
}

#[test]
fn newline_via_cm() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "newline-usage.txt",
        "alpha gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-m");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"beta");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha"))
            && grid.iter().any(|row| row.contains("beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("newline_via_cm", &gnu, &neo, 2);
}

#[test]
fn newline_and_indent_via_cj_trims_trailing_space_and_indents() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "newline-and-indent.el",
        "(let ((alpha 1))   beta)\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-j");
    for session in [&mut gnu, &mut neo] {
        session.send(b"gamma");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(let ((alpha 1))"))
            && grid.iter().any(|row| row.contains("gamma   beta)"))
            && !grid.iter().any(|row| row.contains("1))   "))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "newline_and_indent_via_cj_trims_trailing_space_and_indents",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn kill_sentence_via_mk() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-sentence.txt",
        "Alpha one. Beta two.\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-k");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Beta two."))
            && !grid.iter().any(|row| row.contains("Alpha one."))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("kill_sentence_via_mk", &gnu, &neo, 2);
}

#[test]
fn kill_ring_save_region_then_yank_via_mw_cy() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-ring-save.txt",
        "alpha beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-@");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "SPC C-y");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("alpha beta alpha"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("kill_ring_save_region_then_yank_via_mw_cy", &gnu, &neo, 2);
}

#[test]
fn kill_word_via_md() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-word.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "SPC");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-d");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("kill_word_via_md", &gnu, &neo, 2);
}

#[test]
fn kill_line_twice_then_yank_via_ck_ck_cy() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-line.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-y");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha line"))
            && grid.iter().any(|row| row.contains("beta line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("kill_line_twice_then_yank_via_ck_ck_cy", &gnu, &neo, 2);
}

#[test]
fn yank_pop_after_yank_via_my() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "yank-pop.txt",
        "alpha line\nbeta line\ngamma line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-y");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-y");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta line"))
            && grid.iter().any(|row| row.contains("alpha line"))
            && !grid.iter().any(|row| row.contains("gamma line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("yank_pop_after_yank_via_my", &gnu, &neo, 2);
}

#[test]
fn keyboard_macro_record_and_replay_via_cx_parens_cx_e() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "keyboard-macro.txt",
        "one\ntwo\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x (");
    let recording_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Defining kbd macro"));
    gnu.read_until(Duration::from_secs(6), recording_ready);
    neo.read_until(Duration::from_secs(8), recording_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both_raw(&mut gnu, &mut neo, b"\x05!");
    send_both(&mut gnu, &mut neo, "C-n C-a C-x )");
    let macro_defined = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Keyboard macro defined") || row.contains("End defining"))
    };
    gnu.read_until(Duration::from_secs(6), macro_defined);
    neo.read_until(Duration::from_secs(8), macro_defined);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x e");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("one!")) && grid.iter().any(|row| row.contains("two!"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "keyboard_macro_record_and_replay_via_cx_parens_cx_e",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn keyboard_macro_repeats_via_trailing_e_after_cx_e() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "keyboard-macro-repeat.txt",
        "one\ntwo\nthree\nfour\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x (");
    let recording_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Defining kbd macro"));
    gnu.read_until(Duration::from_secs(6), recording_ready);
    neo.read_until(Duration::from_secs(8), recording_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both_raw(&mut gnu, &mut neo, b"\x05!");
    send_both(&mut gnu, &mut neo, "C-n C-a C-x )");
    let macro_defined = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Keyboard macro defined") || row.contains("End defining"))
    };
    gnu.read_until(Duration::from_secs(6), macro_defined);
    neo.read_until(Duration::from_secs(8), macro_defined);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x e e");
    let repeated = |grid: &[String]| {
        grid.iter().any(|row| row.contains("one!"))
            && grid.iter().any(|row| row.contains("two!"))
            && grid.iter().any(|row| row.contains("three!"))
    };
    gnu.read_until(Duration::from_secs(6), repeated);
    neo.read_until(Duration::from_secs(8), repeated);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both_raw(&mut gnu, &mut neo, b"X");
    let inserted = |grid: &[String]| grid.iter().any(|row| row.contains("Xfour"));
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "keyboard_macro_repeats_via_trailing_e_after_cx_e",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn transpose_words_via_mt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "transpose-words.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-t");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta alpha gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("transpose_words_via_mt", &gnu, &neo, 2);
}

#[test]
fn transpose_lines_via_cx_ct() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "transpose-lines.txt",
        "alpha line\nbeta line\ngamma line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-t");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta line"))
            && grid.iter().any(|row| row.contains("alpha line"))
            && grid.iter().any(|row| row.contains("gamma line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("transpose_lines_via_cx_ct", &gnu, &neo, 2);
}

#[test]
fn just_one_space_via_mspc() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "just-one-space.txt",
        "alpha   beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-SPC");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta gamma"))
            && !grid.iter().any(|row| row.contains("alpha   beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("just_one_space_via_mspc", &gnu, &neo, 2);
}

#[test]
fn cycle_spacing_via_repeated_mspc_collapses_deletes_and_restores() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "cycle-spacing.txt",
        "alpha   beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-SPC");
    let collapsed = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta"))
            && !grid.iter().any(|row| row.contains("alpha   beta"))
    };
    gnu.read_until(Duration::from_secs(6), collapsed);
    neo.read_until(Duration::from_secs(8), collapsed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-SPC");
    let deleted = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alphabeta"))
            && !grid.iter().any(|row| row.contains("alpha beta"))
    };
    gnu.read_until(Duration::from_secs(6), deleted);
    neo.read_until(Duration::from_secs(8), deleted);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-SPC");
    let restored = |grid: &[String]| grid.iter().any(|row| row.contains("alpha   beta"));
    gnu.read_until(Duration::from_secs(6), restored);
    neo.read_until(Duration::from_secs(8), restored);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "cycle_spacing_via_repeated_mspc_collapses_deletes_and_restores",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn delete_horizontal_space_via_mbackslash() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-horizontal-space.txt",
        "alpha   beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-\\");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alphabeta gamma"))
            && !grid.iter().any(|row| row.contains("alpha   beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("delete_horizontal_space_via_mbackslash", &gnu, &neo, 2);
}

#[test]
fn delete_blank_lines_after_current_line_via_cx_co() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-blank-lines.txt",
        "alpha\n\n\nbeta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-o");

    let ready = |grid: &[String]| {
        grid.windows(2)
            .any(|rows| rows[0].contains("alpha") && rows[1].contains("beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "delete_blank_lines_after_current_line_via_cx_co",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn append_next_kill_via_cmeta_w_combines_following_region_kill() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "append-next-kill.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-@ C-w");
    let first_killed = |grid: &[String]| grid.iter().any(|row| row.contains(" beta gamma"));
    gnu.read_until(Duration::from_secs(6), first_killed);
    neo.read_until(Duration::from_secs(8), first_killed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-@ C-M-w C-w");
    let second_killed = |grid: &[String]| {
        grid.iter().any(|row| row.contains(" gamma"))
            && !grid.iter().any(|row| row.contains(" beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), second_killed);
    neo.read_until(Duration::from_secs(8), second_killed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-e SPC C-y");
    let yanked = |grid: &[String]| grid.iter().any(|row| row.contains(" gamma alpha beta"));
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "append_next_kill_via_cmeta_w_combines_following_region_kill",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn delete_indentation_via_mcaret() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-indentation.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-^");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha line beta line"))
            && !grid.iter().any(|row| row.contains("alpha line\n"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("delete_indentation_via_mcaret", &gnu, &neo, 2);
}

#[test]
fn back_to_indentation_then_insert_via_mm() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "back-to-indentation.txt",
        "    alpha beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-m");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("    Xalpha beta"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("back_to_indentation_then_insert_via_mm", &gnu, &neo, 2);
}

#[test]
fn zap_to_char_via_mz_spc() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "zap-to-char.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-z SPC");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta gamma"))
            && !grid.iter().any(|row| row.contains("alpha beta gamma"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("zap_to_char_via_mz_spc", &gnu, &neo, 2);
}

#[test]
fn forward_paragraph_via_m_close_brace() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "forward-paragraph.txt",
        "alpha one\nalpha two\n\nbeta one\nbeta two\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-}");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Xbeta one"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("forward_paragraph_via_m_close_brace", &gnu, &neo, 2);
}

#[test]
fn backward_paragraph_via_m_open_brace() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "backward-paragraph.txt",
        "alpha one\nalpha two\n\nbeta one\nbeta two\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-{");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Xbeta one"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("backward_paragraph_via_m_open_brace", &gnu, &neo, 2);
}

#[test]
fn downcase_region_once_via_disabled_cx_cl() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "downcase-region.txt",
        "ALPHA BETA\nGAMMA DELTA\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-l");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Use this command?"))
            || grid
                .iter()
                .any(|row| row.contains("disabled command downcase-region"))
    };
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(12), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("downcase-region")),
            "{label} should show disabled-command help for downcase-region"
        );
        assert!(
            grid.iter().any(|row| row.contains("Use this command?")),
            "{label} should show disabled-command prompt for downcase-region"
        );
    }

    send_both_raw(&mut gnu, &mut neo, b" ");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta"))
            && grid.iter().any(|row| row.contains("gamma delta"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    settle_session(&mut gnu);
    settle_session(&mut neo);

    assert_pair_nearly_matches("downcase_region_once_via_disabled_cx_cl", &gnu, &neo, 2);
}

#[test]
fn upcase_region_once_via_disabled_cx_cu() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "upcase-region.txt",
        "alpha beta\ngamma delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-u");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Use this command?"))
            || grid
                .iter()
                .any(|row| row.contains("disabled command upcase-region"))
    };
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(12), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("upcase-region")),
            "{label} should show disabled-command help for upcase-region"
        );
        assert!(
            grid.iter().any(|row| row.contains("Use this command?")),
            "{label} should show disabled-command prompt for upcase-region"
        );
    }

    send_both_raw(&mut gnu, &mut neo, b" ");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("ALPHA BETA"))
            && grid.iter().any(|row| row.contains("GAMMA DELTA"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    settle_session(&mut gnu);
    settle_session(&mut neo);

    assert_pair_nearly_matches("upcase_region_once_via_disabled_cx_cu", &gnu, &neo, 2);
}

#[test]
fn upcase_word_via_mu() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "upcase-word.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-u");
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("ALPHA beta gamma"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("upcase_word_via_mu", &gnu, &neo, 2);
}

#[test]
fn downcase_word_via_ml() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "downcase-word.txt",
        "ALPHA beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-l");
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("alpha beta gamma"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("downcase_word_via_ml", &gnu, &neo, 2);
}

#[test]
fn capitalize_word_via_mc() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "capitalize-word.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-c");
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Alpha beta gamma"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("capitalize_word_via_mc", &gnu, &neo, 2);
}

#[test]
fn exchange_point_and_mark_via_cx_cx() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "exchange-point-and-mark.txt",
        "alpha beta gamma\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-@");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-f M-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("alphaX beta gamma"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("exchange_point_and_mark_via_cx_cx", &gnu, &neo, 2);
}

#[test]
fn mark_paragraph_then_kill_and_yank_via_mh_cw_cy() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "mark-paragraph.txt",
        "alpha one\nalpha two\n\nbeta one\nbeta two\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-w");

    let killed = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta one"))
            && grid.iter().any(|row| row.contains("beta two"))
            && !grid.iter().any(|row| row.contains("alpha one"))
            && !grid.iter().any(|row| row.contains("alpha two"))
    };
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-y");
    let restored = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha one"))
            && grid.iter().any(|row| row.contains("alpha two"))
            && grid.iter().any(|row| row.contains("beta one"))
            && grid.iter().any(|row| row.contains("beta two"))
    };
    gnu.read_until(Duration::from_secs(6), restored);
    neo.read_until(Duration::from_secs(8), restored);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "mark_paragraph_then_kill_and_yank_via_mh_cw_cy",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn goto_line_via_mg_g() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "goto-line.txt",
        "alpha line\nbeta line\ngamma line\ndelta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-g g 3 RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Xgamma line"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("goto_line_via_mg_g", &gnu, &neo, 2);
}

#[test]
fn repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line() {
    let (mut gnu, mut neo) = boot_pair("");
    let contents = (1..=50)
        .map(|line| match line {
            2 => "target-two\n".to_string(),
            35 => "target-thirty-five\n".to_string(),
            _ => format!("plain numbered line {line:02}\n"),
        })
        .collect::<String>();
    open_home_file(
        &mut gnu,
        &mut neo,
        "repeat-complex.txt",
        &contents,
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-g g");
    let goto_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Goto line:"));
    gnu.read_until(Duration::from_secs(6), goto_prompt);
    neo.read_until(Duration::from_secs(8), goto_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/goto-prompt",
        &gnu,
        &neo,
        2,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"2");
    }
    let goto_typed = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Goto line: 2"));
    gnu.read_until(Duration::from_secs(6), goto_typed);
    neo.read_until(Duration::from_secs(8), goto_typed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/goto-typed",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let first_target = |grid: &[String]| grid.iter().any(|row| row.contains("target-two"));
    gnu.read_until(Duration::from_secs(6), first_target);
    neo.read_until(Duration::from_secs(8), first_target);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/first-target",
        &gnu,
        &neo,
        3,
    );

    send_both(&mut gnu, &mut neo, "C-x ESC ESC");
    let redo_prompt = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Redo:"))
            && grid.iter().any(|row| row.contains("goto-line"))
            && grid.iter().any(|row| row.contains("2"))
    };
    gnu.read_until(Duration::from_secs(6), redo_prompt);
    neo.read_until(Duration::from_secs(8), redo_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/redo-prompt",
        &gnu,
        &neo,
        3,
    );

    send_both(&mut gnu, &mut neo, "C-b DEL 3 5");
    let redo_edited = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Redo:"))
            && grid.iter().any(|row| row.contains("goto-line"))
            && grid.iter().any(|row| row.contains("35"))
    };
    gnu.read_until(Duration::from_secs(6), redo_edited);
    neo.read_until(Duration::from_secs(8), redo_edited);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/redo-edited",
        &gnu,
        &neo,
        3,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let second_target = |grid: &[String]| grid.iter().any(|row| row.contains("target-thirty-five"));
    gnu.read_until(Duration::from_secs(6), second_target);
    neo.read_until(Duration::from_secs(8), second_target);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/second-target",
        &gnu,
        &neo,
        4,
    );

    send_both_raw(&mut gnu, &mut neo, b"X");
    let inserted = |grid: &[String]| grid.iter().any(|row| row.contains("Xtarget-thirty-five"));
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "repeat_complex_command_via_cx_esc_esc_edits_and_replays_goto_line/final-insert",
        &gnu,
        &neo,
        4,
    );
}

#[test]
fn repeat_last_command_via_cx_z_z_replays_previous_motion() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "repeat-last-command.txt",
        "alpha\nbeta\ngamma\ndelta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-x z z");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Xdelta"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "repeat_last_command_via_cx_z_z_replays_previous_motion",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn goto_char_via_mg_c() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "goto-char.txt", "abcdefg\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "M-g c 4 RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("abcXdefg"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("goto_char_via_mg_c", &gnu, &neo, 2);
}

#[test]
fn goto_char_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "goto-char-empty-del.txt",
        "abcdefg\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-g c");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Go to char:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Go to char:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty goto-char prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "goto_char_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn beginning_and_end_of_buffer_via_mless_mgreater() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "buffer-boundaries.txt",
        "alpha line\nbeta line\ngamma line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let at_end = |grid: &[String]| grid.iter().any(|row| row.contains("gamma lineX"));
    gnu.read_until(Duration::from_secs(6), at_end);
    neo.read_until(Duration::from_secs(8), at_end);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "beginning_and_end_of_buffer_via_mless_mgreater/end",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"Y");

    let at_beginning = |grid: &[String]| grid.iter().any(|row| row.contains("Yalpha line"));
    gnu.read_until(Duration::from_secs(6), at_beginning);
    neo.read_until(Duration::from_secs(8), at_beginning);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "beginning_and_end_of_buffer_via_mless_mgreater",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn count_words_region_via_mequals() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "count-words.txt",
        "alpha beta\ngamma delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC M-> M-=");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Region has"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("count_words_region_via_mequals", &gnu, &neo, 2);
}

#[test]
fn count_words_buffer_via_mx_reports_buffer_totals() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "count-words-buffer.txt",
        "Alpha beta.\nGamma delta.\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "count-words");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Buffer has") && row.contains("4 words"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "count_words_buffer_via_mx_reports_buffer_totals",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn count_lines_page_via_cx_l_reports_current_page() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "count-lines-page.txt",
        "one\ntwo\nthree\nfour\nfive\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-n C-x l");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Page has"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "count_lines_page_via_cx_l_reports_current_page",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn what_line_via_mx_reports_current_line() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "what-line.txt",
        "alpha\nbeta\ngamma\ndelta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-n");
    invoke_mx_command(&mut gnu, &mut neo, "what-line");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("Line 3"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("what_line_via_mx_reports_current_line", &gnu, &neo, 2);
}

#[test]
fn what_cursor_position_via_cx_equals() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "what-cursor-position.txt",
        "alpha beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-x =");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Char:"))
            || grid.iter().any(|row| row.contains("character:"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("what_cursor_position_via_cx_equals", &gnu, &neo, 2);
}

#[test]
fn quoted_insert_newline_via_cq_cj() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "quoted-insert.txt",
        "alpha beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"\x11\x0a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.trim() == "al")
            && grid.iter().any(|row| row.contains("Xpha beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("quoted_insert_newline_via_cq_cj", &gnu, &neo, 2);
}

#[test]
fn undo_edit_via_cslash() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "undo-cslash.txt",
        "alpha beta\n",
        "C-x C-f",
    );

    send_both_raw(&mut gnu, &mut neo, b"X");
    let inserted = |grid: &[String]| grid.iter().any(|row| row.contains("Xalpha beta"));
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both_raw(&mut gnu, &mut neo, b"\x1f");
    let undone = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta"))
            && !grid.iter().any(|row| row.contains("Xalpha beta"))
    };
    gnu.read_until(Duration::from_secs(6), undone);
    neo.read_until(Duration::from_secs(8), undone);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("undo_edit_via_cslash", &gnu, &neo, 2);
}

#[test]
fn undo_redo_via_cmeta_underscore_restores_undone_edit() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "undo-redo.txt",
        "alpha beta\n",
        "C-x C-f",
    );

    send_both_raw(&mut gnu, &mut neo, b"X");
    let inserted = |grid: &[String]| grid.iter().any(|row| row.contains("Xalpha beta"));
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both_raw(&mut gnu, &mut neo, b"\x1f");
    let undone = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha beta"))
            && !grid.iter().any(|row| row.contains("Xalpha beta"))
    };
    gnu.read_until(Duration::from_secs(6), undone);
    neo.read_until(Duration::from_secs(8), undone);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both_raw(&mut gnu, &mut neo, b"\x1b\x1f");
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "undo_redo_via_cmeta_underscore_restores_undone_edit",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn comment_region_via_msemicolon() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "comment-dwim.el",
        "(message \"alpha\")\n(message \"beta\")\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC M-> M-;");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains(";; (message \"alpha\")"))
            && grid.iter().any(|row| row.contains(";; (message \"beta\")"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("comment_region_via_msemicolon", &gnu, &neo, 2);
}

#[test]
fn comment_line_via_mx_comments_current_line_and_moves_to_next() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "comment-line.el",
        "(message \"alpha\")\n(message \"beta\")\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "comment-line");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains(";; (message \"alpha\")"))
            && grid.iter().any(|row| row.contains("(message \"beta\")"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "comment_line_via_mx_comments_current_line_and_moves_to_next",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn comment_line_via_cx_csemicolon_toggles_current_line() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "comment-line-toggle.el",
        "(message \"alpha\")\n(message \"beta\")\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-;");
    let commented = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains(";; (message \"alpha\")"))
            && grid.iter().any(|row| row.contains("(message \"beta\")"))
    };
    gnu.read_until(Duration::from_secs(6), commented);
    neo.read_until(Duration::from_secs(8), commented);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "C-p C-x C-;");
    let uncommented = |grid: &[String]| {
        grid.iter().any(|row| row.contains("(message \"alpha\")"))
            && !grid
                .iter()
                .any(|row| row.contains(";; (message \"alpha\")"))
            && grid.iter().any(|row| row.contains("(message \"beta\")"))
    };
    gnu.read_until(Duration::from_secs(6), uncommented);
    neo.read_until(Duration::from_secs(8), uncommented);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "comment_line_via_cx_csemicolon_toggles_current_line",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn goto_line_via_mg_g_jumps_to_line_number() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=20 {
        contents.push_str(&format!("line {:02}\n", line));
    }
    open_home_file(
        &mut gnu,
        &mut neo,
        "goto-line-usage.txt",
        &contents,
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-g");
    send_both(&mut gnu, &mut neo, "g");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"10\r");
    }

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("line 10"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("goto_line_via_mg_g", &gnu, &neo, 2);
}

#[test]
fn keyboard_macro_record_line_kill_and_replay_via_cx_paren() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kmacro-replay.txt",
        "line one\nline two\nline three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-a");
    // Start recording: C-x (
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "(");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Record actions: C-k C-n (kill line and move down)
    send_both(&mut gnu, &mut neo, "C-k");
    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Stop recording: C-x )
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, ")");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Replay once: C-x e
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // After replay: line two should be killed, cursor on line three
    let gnug = gnu.text_grid();
    let neog = neo.text_grid();
    let gnu_ok = gnug.iter().any(|r| r.contains("line three"));
    let neo_ok = neog.iter().any(|r| r.contains("line three"));
    let gnu_not_two = !gnug.iter().any(|r| r.contains("line two"));
    let neo_not_two = !neog.iter().any(|r| r.contains("line two"));
    assert!(gnu_ok && gnu_not_two, "GNU should have killed line two");
    assert!(neo_ok && neo_not_two, "NEO should have killed line two");
}

#[test]
fn universal_argument_move_down_via_cu_5_cn() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=10 {
        contents.push_str(&format!("line {:02}\n", line));
    }
    open_home_file(&mut gnu, &mut neo, "move-down-5.txt", &contents, "C-x C-f");

    // C-u 5 C-n should move down 5 lines
    send_both(&mut gnu, &mut neo, "C-u");
    send_both(&mut gnu, &mut neo, "5");
    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify we're on line 6 by using C-x = to check cursor position
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "=");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("line 06")),
            "{label}: C-u 5 C-n should move to line 6"
        );
    }
}
