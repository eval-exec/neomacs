#![cfg(unix)]
//! TUI comparison tests: buffers.

mod support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Tests ──────────────────────────────────────────────────
#[test]
fn switch_buffer_via_cx_b_visits_existing_file_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "switch-alpha.txt",
        "alpha buffer body\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "switch-beta.txt",
        "beta buffer body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Switch to buffer:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "switch_buffer_via_cx_b_visits_existing_file_buffer/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"switch-alpha.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let alpha_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("switch-alpha.txt"))
            && grid.iter().any(|row| row.contains("alpha buffer body"))
            && !grid.iter().any(|row| row.contains("beta buffer body"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_ready);
    neo.read_until(Duration::from_secs(8), alpha_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "switch_buffer_via_cx_b_visits_existing_file_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn switch_buffer_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "switch-empty-del.txt",
        "alpha buffer body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Switch to buffer"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Switch to buffer"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty switch-buffer prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "switch_buffer_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn rename_buffer_via_cx_x_r_updates_current_buffer_name() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x x r");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Rename buffer") && row.contains("to new name"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "rename_buffer_via_cx_x_r_updates_current_buffer_name/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"renamed-scratch");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let renamed_ready = |grid: &[String]| grid.iter().any(|row| row.contains("renamed-scratch"));
    gnu.read_until(Duration::from_secs(6), renamed_ready);
    neo.read_until(Duration::from_secs(8), renamed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "rename_buffer_via_cx_x_r_updates_current_buffer_name/renamed",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-x C-b");
    let list_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Buffer List*"))
            && grid.iter().any(|row| row.contains("renamed-scratch"))
    };
    gnu.read_until(Duration::from_secs(6), list_ready);
    neo.read_until(Duration::from_secs(8), list_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "rename_buffer_via_cx_x_r_updates_current_buffer_name/list-buffers",
        &gnu,
        &neo,
    );
}

#[test]
fn rename_buffer_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x x r");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Rename buffer") && row.contains("to new name"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Rename buffer") && row.contains("to new name"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty rename-buffer prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "rename_buffer_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn list_buffers_after_find_file() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "common-usage.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-b");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Buffer List*"))
            && grid.iter().any(|row| row.contains("common-usage.txt"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("list_buffers_after_find_file", &gnu, &neo);
}

#[test]
fn buffer_menu_search_and_select_file_buffer_via_ret() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "buffer-menu-select.txt",
        "selected buffer body\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "buffer-menu");
    let menu_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Buffer List*"))
            && grid.iter().any(|row| row.contains("Buffer Menu"))
            && grid
                .iter()
                .any(|row| row.contains("buffer-menu-select.txt"))
    };
    gnu.read_until(Duration::from_secs(6), menu_ready);
    neo.read_until(Duration::from_secs(8), menu_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-s");
    for session in [&mut gnu, &mut neo] {
        session.send(b"buffer-menu-select.txt");
    }
    let search_ready = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("I-search") && row.contains("buffer-menu-select.txt"))
    };
    gnu.read_until(Duration::from_secs(6), search_ready);
    neo.read_until(Duration::from_secs(8), search_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "C-g RET");
    let selected = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("buffer-menu-select.txt"))
            && grid.iter().any(|row| row.contains("selected buffer body"))
            && !grid.iter().any(|row| row.contains("*Buffer List*"))
    };
    gnu.read_until(Duration::from_secs(6), selected);
    neo.read_until(Duration::from_secs(8), selected);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "buffer_menu_search_and_select_file_buffer_via_ret",
        &gnu,
        &neo,
    );
}

#[test]
fn clone_indirect_buffer_other_window_via_cx4_c() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "clone-indirect.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x 4 c");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("clone-indirect.txt<2>"))
            && grid.iter().filter(|row| row.contains("alpha line")).count() >= 2
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("clone_indirect_buffer_other_window_via_cx4_c", &gnu, &neo);
}

#[test]
fn ibuffer_via_mx_lists_file_buffer_and_q_quits() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "ibuffer-usage.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "ibuffer");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Ibuffer*"))
            && grid.iter().any(|row| row.contains("ibuffer-usage.txt"))
            && grid.iter().any(|row| row.contains("Commands:"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(10), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids("ibuffer_via_mx_lists_file_buffer_and_q_quits", &gnu, &neo);
    }
    assert_pair_exact_display(
        "ibuffer_via_mx_lists_file_buffer_and_q_quits/list",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "q");
    let quit_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("ibuffer-usage.txt"))
            && grid.iter().any(|row| row.contains("alpha line"))
            && !grid.iter().any(|row| row.contains("*Ibuffer*"))
    };
    gnu.read_until(Duration::from_secs(6), quit_ready);
    neo.read_until(Duration::from_secs(8), quit_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "ibuffer_via_mx_lists_file_buffer_and_q_quits/quit",
        &gnu,
        &neo,
    );
}

#[test]
fn switch_to_messages_buffer_via_cx_b() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "common usage smoke")"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"*Messages*");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Messages*"))
            && grid.iter().any(|row| row.contains("common usage smoke"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("switch_to_messages_buffer_via_cx_b", &gnu, &neo);
}

#[test]
fn view_echo_area_messages_via_ch_e_shows_messages_buffer_tail() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "common usage echo log")"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    send_help_sequence(&mut gnu, &mut neo, "e");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Messages*"))
            && grid.iter().any(|row| row.contains("common usage echo log"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !ready(&gnu.text_grid()) || !ready(&neo.text_grid()) {
        dump_pair_grids(
            "view_echo_area_messages_via_ch_e_shows_messages_buffer_tail/not-ready",
            &gnu,
            &neo,
        );
    }

    assert_pair_exact_display(
        "view_echo_area_messages_via_ch_e_shows_messages_buffer_tail",
        &gnu,
        &neo,
    );
}

#[test]
fn switch_to_file_buffer_via_cx_b_restores_existing_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "switch-alpha.txt",
        "alpha first\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "switch-beta.txt",
        "beta second\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    let prompt_ready = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("Switch to buffer:"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"switch-alpha.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("switch-alpha.txt"))
            && grid.iter().any(|row| row.contains("alpha first"))
            && !grid.iter().any(|row| row.contains("beta second"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "switch_to_file_buffer_via_cx_b_restores_existing_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn switch_to_buffer_empty_input_uses_default_previous_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "default-buffer-alpha.txt",
        "default alpha body\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "default-buffer-beta.txt",
        "default beta body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    let prompt_ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("Switch to buffer")
                && row.contains("default")
                && row.contains("default-buffer-alpha.txt")
        })
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "RET");
    let alpha_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("default-buffer-alpha.txt"))
            && grid.iter().any(|row| row.contains("default alpha body"))
            && !grid.iter().any(|row| row.contains("default beta body"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_ready);
    neo.read_until(Duration::from_secs(8), alpha_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "switch_to_buffer_empty_input_uses_default_previous_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn switch_to_buffer_tab_completion_via_cx_b_completes_existing_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "buffer-completion-target.txt",
        "buffer completion body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    let switch_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Switch to buffer:"));
    gnu.read_until(Duration::from_secs(6), switch_prompt);
    neo.read_until(Duration::from_secs(8), switch_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"*scratch*");
    }
    send_both(&mut gnu, &mut neo, "RET");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    gnu.read_until(Duration::from_secs(6), switch_prompt);
    neo.read_until(Duration::from_secs(8), switch_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "switch_to_buffer_tab_completion_via_cx_b_completes_existing_buffer/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"buffer-completion-tar");
    }
    send_both(&mut gnu, &mut neo, "TAB");
    let completed = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("buffer-completion-target.txt"))
    };
    gnu.read_until(Duration::from_secs(6), completed);
    neo.read_until(Duration::from_secs(8), completed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "switch_to_buffer_tab_completion_via_cx_b_completes_existing_buffer/completed",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("buffer-completion-target.txt"))
            && grid
                .iter()
                .any(|row| row.contains("buffer completion body"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "switch_to_buffer_tab_completion_via_cx_b_completes_existing_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn previous_and_next_buffer_via_mx_cycle_recent_file_buffers() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "cycle-alpha.txt",
        "alpha cycle\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "cycle-beta.txt",
        "beta cycle\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "previous-buffer");
    let alpha_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("cycle-alpha.txt"))
            && grid.iter().any(|row| row.contains("alpha cycle"))
            && !grid.iter().any(|row| row.contains("beta cycle"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_ready);
    neo.read_until(Duration::from_secs(8), alpha_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "previous_and_next_buffer_via_mx_cycle_recent_file_buffers/previous",
        &gnu,
        &neo,
    );

    invoke_mx_command(&mut gnu, &mut neo, "next-buffer");
    let beta_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("cycle-beta.txt"))
            && grid.iter().any(|row| row.contains("beta cycle"))
            && !grid.iter().any(|row| row.contains("alpha cycle"))
    };
    gnu.read_until(Duration::from_secs(6), beta_ready);
    neo.read_until(Duration::from_secs(8), beta_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "previous_and_next_buffer_via_mx_cycle_recent_file_buffers/next",
        &gnu,
        &neo,
    );
}

#[test]
fn bury_and_unbury_buffer_via_mx_moves_current_buffer_to_end() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "bury-alpha.txt",
        "alpha bury\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "bury-beta.txt",
        "beta bury\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "bury-buffer");
    let alpha_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("bury-alpha.txt"))
            && grid.iter().any(|row| row.contains("alpha bury"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("F1  bury-alpha.txt"))
            && !grid.iter().any(|row| row.contains("beta bury"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_ready);
    neo.read_until(Duration::from_secs(8), alpha_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            alpha_ready(&grid),
            "{label} should settle on the previous buffer after bury-buffer:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "bury_and_unbury_buffer_via_mx_moves_current_buffer_to_end/buried",
        &gnu,
        &neo,
    );

    invoke_mx_command(&mut gnu, &mut neo, "unbury-buffer");
    let beta_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("bury-beta.txt"))
            && grid.iter().any(|row| row.contains("beta bury"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("F1  bury-beta.txt"))
            && !grid.iter().any(|row| row.contains("alpha bury"))
    };
    gnu.read_until(Duration::from_secs(6), beta_ready);
    neo.read_until(Duration::from_secs(8), beta_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            beta_ready(&grid),
            "{label} should settle on the buried buffer after unbury-buffer:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "bury_and_unbury_buffer_via_mx_moves_current_buffer_to_end",
        &gnu,
        &neo,
    );
}

#[test]
fn clone_buffer_via_mx_creates_independent_scratch_copy() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"original clone body");
    }
    let original_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("original clone body"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("*scratch*"))
    };
    gnu.read_until(Duration::from_secs(6), original_ready);
    neo.read_until(Duration::from_secs(8), original_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "clone-buffer");
    let clone_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("original clone body"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("*scratch*<2>"))
    };
    gnu.read_until(Duration::from_secs(8), clone_ready);
    neo.read_until(Duration::from_secs(12), clone_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    if !clone_ready(&gnu.text_grid()) || !clone_ready(&neo.text_grid()) {
        dump_pair_grids(
            "clone_buffer_via_mx_creates_independent_scratch_copy/clone-ready",
            &gnu,
            &neo,
        );
    }

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            clone_ready(&grid),
            "{label} should show an independent cloned scratch buffer:\n{}",
            grid.join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"clone-only edit");
    }
    let clone_edited = |grid: &[String]| {
        grid.iter().any(|row| row.contains("clone-only edit"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("*scratch*<2>"))
    };
    gnu.read_until(Duration::from_secs(6), clone_edited);
    neo.read_until(Duration::from_secs(8), clone_edited);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(format "clone-check %S/%S" (buffer-name) (with-current-buffer "*scratch*" (save-excursion (goto-char (point-min)) (search-forward "clone-only edit" nil t))))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let clone_check = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("clone-check") && row.contains("*scratch*<2>") && row.contains("/nil")
        })
    };
    gnu.read_until(Duration::from_secs(8), clone_check);
    neo.read_until(Duration::from_secs(12), clone_check);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            clone_check(&grid),
            "{label} should keep clone edits out of the original scratch buffer:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "clone_buffer_via_mx_creates_independent_scratch_copy",
        &gnu,
        &neo,
    );
}

#[test]
fn switch_to_buffer_other_window_via_cx4_b_displays_messages() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "other window buffer switch")"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    send_both(&mut gnu, &mut neo, "C-x 4 b");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Switch to buffer in other window"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let expected_prompt = "Switch to buffer in other window (default *Messages*): ";
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains(expected_prompt)),
            "{label} should show read-buffer's default in the prompt\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "switch_to_buffer_other_window_via_cx4_b_displays_messages/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"*Messages*");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid.iter().any(|row| row.contains("*Messages*"))
            && grid
                .iter()
                .any(|row| row.contains("other window buffer switch"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "switch_to_buffer_other_window_via_cx4_b_displays_messages",
        &gnu,
        &neo,
    );
}

#[test]
fn kill_buffer_after_find_file_via_cx_k() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-buffer.txt",
        "temporary buffer\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("kill-buffer.txt"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("kill_buffer_after_find_file_via_cx_k", &gnu, &neo);
}

// ── Frame-specific buffer-list / buried-buffer-list tests ────────────────

#[test]
fn frame_parameter_buffer_list_returns_buffers_in_order() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "frm-buf-a.txt",
        "frame buffer a\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "frm-buf-b.txt",
        "frame buffer b\n",
        "C-x C-f",
    );

    // Eval (mapcar #'buffer-name (frame-parameter nil 'buffer-list))
    // to verify the frame parameter returns buffer objects in order.
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    let expr = "(mapcar #'buffer-name (frame-parameter nil 'buffer-list))";
    gnu.send(expr.as_bytes());
    neo.send(expr.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let has_frm_buf_b = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("frm-buf-b.txt") && row.contains("frm-buf-a.txt"))
    };
    gnu.read_until(Duration::from_secs(8), has_frm_buf_b);
    neo.read_until(Duration::from_secs(10), has_frm_buf_b);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "frame_parameter_buffer_list_returns_buffers_in_order",
        &gnu,
        &neo,
    );
}

#[test]
fn frame_parameter_buried_buffer_list_after_bury_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "bury-fp-a.txt",
        "bury fp alpha\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "bury-fp-b.txt",
        "bury fp beta\n",
        "C-x C-f",
    );

    // Bury the current buffer (bury-fp-b.txt).  This should move it to
    // the frame's buried-buffer-list frame parameter.
    invoke_mx_command(&mut gnu, &mut neo, "bury-buffer");

    // Wait for the previous buffer (bury-fp-a.txt) to become visible.
    let alpha_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("bury-fp-a.txt"))
            && grid.iter().any(|row| row.contains("bury fp alpha"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_ready);
    neo.read_until(Duration::from_secs(8), alpha_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Eval (frame-parameter nil 'buried-buffer-list) and verify the
    // buried buffer is listed.
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    let expr = "(mapcar #'buffer-name (frame-parameter nil 'buried-buffer-list))";
    gnu.send(expr.as_bytes());
    neo.send(expr.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let has_buried = |grid: &[String]| grid.iter().any(|row| row.contains("bury-fp-b.txt"));
    gnu.read_until(Duration::from_secs(8), has_buried);
    neo.read_until(Duration::from_secs(10), has_buried);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "frame_parameter_buried_buffer_list_after_bury_buffer",
        &gnu,
        &neo,
    );
}

#[test]
fn multiple_bury_then_unbury_restores_in_lifo_order() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "multi-aaa.txt",
        "buffer aaa\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "multi-bbb.txt",
        "buffer bbb\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "multi-ccc.txt",
        "buffer ccc\n",
        "C-x C-f",
    );

    // Bury multi-ccc.txt → should switch to multi-bbb.txt.
    invoke_mx_command(&mut gnu, &mut neo, "bury-buffer");
    let bbb_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("multi-bbb.txt"))
            && grid.iter().any(|row| row.contains("buffer bbb"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("multi-bbb.txt"))
            && !grid.iter().any(|row| row.contains("buffer ccc"))
    };
    gnu.read_until(Duration::from_secs(6), bbb_ready);
    neo.read_until(Duration::from_secs(8), bbb_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "multiple_bury_then_unbury_restores_in_lifo_order/buried-ccc",
        &gnu,
        &neo,
    );

    // Bury multi-bbb.txt → should switch to multi-aaa.txt.
    invoke_mx_command(&mut gnu, &mut neo, "bury-buffer");
    let aaa_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("multi-aaa.txt"))
            && grid.iter().any(|row| row.contains("buffer aaa"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("multi-aaa.txt"))
            && !grid.iter().any(|row| row.contains("buffer bbb"))
    };
    gnu.read_until(Duration::from_secs(6), aaa_ready);
    neo.read_until(Duration::from_secs(8), aaa_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "multiple_bury_then_unbury_restores_in_lifo_order/buried-bbb",
        &gnu,
        &neo,
    );

    // Unbury → should restore multi-bbb.txt (most recently buried = LIFO).
    invoke_mx_command(&mut gnu, &mut neo, "unbury-buffer");
    let unbury_bbb = |grid: &[String]| {
        grid.iter().any(|row| row.contains("multi-bbb.txt"))
            && grid.iter().any(|row| row.contains("buffer bbb"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("multi-bbb.txt"))
    };
    gnu.read_until(Duration::from_secs(6), unbury_bbb);
    neo.read_until(Duration::from_secs(8), unbury_bbb);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "multiple_bury_then_unbury_restores_in_lifo_order/unburied-bbb",
        &gnu,
        &neo,
    );

    // Unbury again → should restore multi-ccc.txt.
    invoke_mx_command(&mut gnu, &mut neo, "unbury-buffer");
    let unbury_ccc = |grid: &[String]| {
        grid.iter().any(|row| row.contains("multi-ccc.txt"))
            && grid.iter().any(|row| row.contains("buffer ccc"))
            && grid
                .get(usize::from(ROWS - 2))
                .is_some_and(|row| row.contains("multi-ccc.txt"))
    };
    gnu.read_until(Duration::from_secs(6), unbury_ccc);
    neo.read_until(Duration::from_secs(8), unbury_ccc);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "multiple_bury_then_unbury_restores_in_lifo_order/unburied-ccc",
        &gnu,
        &neo,
    );
}

#[test]
fn kill_buffer_removes_from_frame_buried_buffer_list() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-buried-a.txt",
        "kill buried a\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-buried-b.txt",
        "kill buried b\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-buried-c.txt",
        "kill buried c\n",
        "C-x C-f",
    );

    // Bury kill-buried-c.txt so it goes into the frame's buried list.
    invoke_mx_command(&mut gnu, &mut neo, "bury-buffer");

    // Wait for the previous buffer to become visible (kill-buried-b.txt).
    let prev_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("kill-buried-b.txt"))
            && !grid.iter().any(|row| row.contains("kill buried c"))
    };
    gnu.read_until(Duration::from_secs(6), prev_ready);
    neo.read_until(Duration::from_secs(8), prev_ready);
    // Extra settle: keep reading until both are truly stable.
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Kill the buried buffer via M-: eval — more direct than C-x k,
    // avoids minibuffer timing issues.
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Eval:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    let kill_expr = "(progn (kill-buffer \"kill-buried-c.txt\") (length (frame-parameter nil 'buried-buffer-list)))";
    gnu.send(kill_expr.as_bytes());
    neo.send(kill_expr.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    // Wait for "0" in the echo area (buried list should be empty).
    let shows_zero = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("#o0") || row.contains(" 0"))
    };
    gnu.read_until(Duration::from_secs(8), shows_zero);
    neo.read_until(Duration::from_secs(10), shows_zero);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "kill_buffer_removes_from_frame_buried_buffer_list",
        &gnu,
        &neo,
    );
}

#[test]
fn list_buffers_via_cx_cb_shows_scratch_and_messages_buffers() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-x C-b");

    let ready = |grid: &[String]| {
        grid.iter().any(|r| r.contains("*Buffer List*"))
            && grid.iter().any(|r| r.contains("*scratch*"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("*Buffer List*")),
            "{label}: C-x C-b should open Buffer List"
        );
        assert!(
            grid.iter().any(|r| r.contains("*scratch*")),
            "{label}: Buffer List should show *scratch*"
        );
    }
    assert_pair_exact_display(
        "list_buffers_via_cx_cb_shows_scratch_and_messages_buffers",
        &gnu,
        &neo,
    );
}
