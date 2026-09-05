#![cfg(unix)]
//! TUI comparison tests: registers bookmarks.

use crate::support;
use std::time::Duration;
use support::*;

// ── Tests ──────────────────────────────────────────────────
#[test]
fn copy_to_register_and_insert_register_via_cx_r_s_i() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "register-usage.txt",
        "alpha register\nbeta line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC C-e C-x r s");
    let copy_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Copy to register:"));
    gnu.read_until(Duration::from_secs(6), copy_prompt);
    neo.read_until(Duration::from_secs(8), copy_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-> C-x r i");
    let insert_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Insert register:"));
    gnu.read_until(Duration::from_secs(6), insert_prompt);
    neo.read_until(Duration::from_secs(8), insert_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("register-usage.txt"))
            && grid
                .iter()
                .filter(|row| row.contains("alpha register"))
                .count()
                >= 2
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "copy_to_register_and_insert_register_via_cx_r_s_i",
        &gnu,
        &neo,
    );
}

#[test]
fn kill_and_yank_rectangle_via_cx_r_k_y() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "rectangle-usage.txt",
        "aa11xx\nbb22yy\ncc33zz\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-SPC C-n C-n C-f C-f C-x r k");
    let killed = |grid: &[String]| {
        grid.iter().any(|row| row.contains("aaxx"))
            && grid.iter().any(|row| row.contains("bbyy"))
            && grid.iter().any(|row| row.contains("cczz"))
    };
    gnu.read_until(Duration::from_secs(6), killed);
    neo.read_until(Duration::from_secs(8), killed);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("kill_and_yank_rectangle_via_cx_r_k_y/kill", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "C-x r y");
    let yanked = |grid: &[String]| {
        grid.iter().any(|row| row.contains("aa11xx"))
            && grid.iter().any(|row| row.contains("bb22yy"))
            && grid.iter().any(|row| row.contains("cc33zz"))
    };
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("kill_and_yank_rectangle_via_cx_r_k_y/yank", &gnu, &neo);
}

#[test]
fn copy_rectangle_as_kill_then_yank_via_cx_r_mw_y() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "copy-rectangle.txt",
        "aa11xx\nbb22yy\n--\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-SPC C-n C-f C-f C-x r M-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x r y");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("aa11xx"))
            && grid.iter().any(|row| row.contains("bb2211yy"))
            && grid.iter().any(|row| row.contains("--  22"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("copy_rectangle_as_kill_then_yank_via_cx_r_mw_y", &gnu, &neo);
    save_current_file_and_assert_contents(
        "copy_rectangle_as_kill_then_yank_via_cx_r_mw_y",
        &mut gnu,
        &mut neo,
        "copy-rectangle.txt",
        "aa11xx\nbb2211yy\n--  22\n",
    );
}

#[test]
fn open_rectangle_via_cx_r_o_shifts_text_right() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "open-rectangle.txt",
        "ab12\ncd34\nef56\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-SPC C-n C-n C-f C-f C-x r o");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("ab  12"))
            && grid.iter().any(|row| row.contains("cd  34"))
            && grid.iter().any(|row| row.contains("ef  56"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("open_rectangle_via_cx_r_o_shifts_text_right", &gnu, &neo);
    save_current_file_and_assert_contents(
        "open_rectangle_via_cx_r_o_shifts_text_right",
        &mut gnu,
        &mut neo,
        "open-rectangle.txt",
        "ab  12\ncd  34\nef  56\n",
    );
}

#[test]
fn clear_rectangle_via_cx_r_c_blanks_selected_columns() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "clear-rectangle.txt",
        "ab12zz\ncd34zz\nef56zz\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-SPC C-n C-n C-f C-f C-x r c");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("ab  zz"))
            && grid.iter().any(|row| row.contains("cd  zz"))
            && grid.iter().any(|row| row.contains("ef  zz"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "clear_rectangle_via_cx_r_c_blanks_selected_columns",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "clear_rectangle_via_cx_r_c_blanks_selected_columns",
        &mut gnu,
        &mut neo,
        "clear-rectangle.txt",
        "ab  zz\ncd  zz\nef  zz\n",
    );
}

#[test]
fn delete_rectangle_via_cx_r_d_shifts_suffix_left() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-rectangle.txt",
        "ab12zz\ncd34zz\nef56zz\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-SPC C-n C-n C-f C-f C-x r d");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("abzz"))
            && grid.iter().any(|row| row.contains("cdzz"))
            && grid.iter().any(|row| row.contains("efzz"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("delete_rectangle_via_cx_r_d_shifts_suffix_left", &gnu, &neo);
    save_current_file_and_assert_contents(
        "delete_rectangle_via_cx_r_d_shifts_suffix_left",
        &mut gnu,
        &mut neo,
        "delete-rectangle.txt",
        "abzz\ncdzz\nefzz\n",
    );
}

#[test]
fn delete_whitespace_rectangle_via_mx_closes_gaps_from_column() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-whitespace-rectangle.txt",
        "aa  xx\nbb   yy\ncc zz\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-f C-f C-SPC C-n C-n");
    invoke_mx_command(&mut gnu, &mut neo, "delete-whitespace-rectangle");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("aaxx"))
            && grid.iter().any(|row| row.contains("bbyy"))
            && grid.iter().any(|row| row.contains("cczz"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "delete_whitespace_rectangle_via_mx_closes_gaps_from_column",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "delete_whitespace_rectangle_via_mx_closes_gaps_from_column",
        &mut gnu,
        &mut neo,
        "delete-whitespace-rectangle.txt",
        "aaxx\nbbyy\ncczz\n",
    );
}

#[test]
fn string_rectangle_via_cx_r_t_replaces_columns() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "string-rectangle.txt",
        "abcd 1\nefgh 2\nkeep 3\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC C-n C-f C-f C-f C-f C-x r t");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("String rectangle"));
    gnu.read_until(Duration::from_secs(8), prompt_ready);
    neo.read_until(Duration::from_secs(10), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"BOX");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("BOX 1"))
            && grid.iter().any(|row| row.contains("BOX 2"))
            && grid.iter().any(|row| row.contains("keep 3"))
            && !grid.iter().any(|row| row.contains("abcd 1"))
            && !grid.iter().any(|row| row.contains("efgh 2"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(10), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("string_rectangle_via_cx_r_t_replaces_columns", &gnu, &neo);
}

#[test]
fn rectangle_number_lines_via_cx_r_n_numbers_selected_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "rectangle-number-lines.txt",
        "apple\nbanana\ncherry\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-SPC C-n C-n C-x r N");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("1 apple"))
            && grid.iter().any(|row| row.contains("2 banana"))
            && grid.iter().any(|row| row.contains("3 cherry"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "rectangle_number_lines_via_cx_r_n_numbers_selected_lines",
        &gnu,
        &neo,
    );
    save_current_file_and_assert_contents(
        "rectangle_number_lines_via_cx_r_n_numbers_selected_lines",
        &mut gnu,
        &mut neo,
        "rectangle-number-lines.txt",
        "1 apple\n2 banana\n3 cherry\n",
    );
}

#[test]
fn point_to_register_and_jump_to_register_via_cx_r_spc_j() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=70 {
        contents.push_str(&format!("register jump line {line:02}\n"));
    }
    open_home_file(
        &mut gnu,
        &mut neo,
        "point-register.txt",
        &contents,
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x r SPC");
    let point_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Point to register:"));
    gnu.read_until(Duration::from_secs(6), point_prompt);
    neo.read_until(Duration::from_secs(8), point_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M->");
    let at_end = |grid: &[String]| grid.iter().any(|row| row.contains("register jump line 70"));
    gnu.read_until(Duration::from_secs(6), at_end);
    neo.read_until(Duration::from_secs(8), at_end);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x r j");
    let jump_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Jump to register:"));
    gnu.read_until(Duration::from_secs(6), jump_prompt);
    neo.read_until(Duration::from_secs(8), jump_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }

    let at_start = |grid: &[String]| grid.iter().any(|row| row.contains("register jump line 01"));
    gnu.read_until(Duration::from_secs(6), at_start);
    neo.read_until(Duration::from_secs(8), at_start);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "point_to_register_and_jump_to_register_via_cx_r_spc_j",
        &gnu,
        &neo,
    );
}

#[test]
fn view_register_via_mx_displays_saved_text() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(progn (set-register ?a "registered text") (message "register ready"))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    let setup_ready = |grid: &[String]| grid.iter().any(|row| row.contains("register ready"));
    gnu.read_until(Duration::from_secs(6), setup_ready);
    neo.read_until(Duration::from_secs(8), setup_ready);

    invoke_mx_command(&mut gnu, &mut neo, "view-register");
    let view_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("View register:"));
    gnu.read_until(Duration::from_secs(6), view_prompt);
    neo.read_until(Duration::from_secs(8), view_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }

    let output_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Output*"))
            && grid.iter().any(|row| row.contains("Register a contains"))
            && grid.iter().any(|row| row.contains("registered text"))
    };
    gnu.read_until(Duration::from_secs(8), output_ready);
    neo.read_until(Duration::from_secs(12), output_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("view_register_via_mx_displays_saved_text", &gnu, &neo);
}

#[test]
fn list_registers_via_mx_displays_nonempty_register() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(progn (set-register ?b "listed text") (message "register list ready"))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    let setup_ready = |grid: &[String]| grid.iter().any(|row| row.contains("register list ready"));
    gnu.read_until(Duration::from_secs(6), setup_ready);
    neo.read_until(Duration::from_secs(8), setup_ready);

    invoke_mx_command(&mut gnu, &mut neo, "list-registers");

    let output_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Output*"))
            && grid.iter().any(|row| row.contains("Register b contains"))
            && grid.iter().any(|row| row.contains("listed text"))
    };
    gnu.read_until(Duration::from_secs(8), output_ready);
    neo.read_until(Duration::from_secs(12), output_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "list_registers_via_mx_displays_nonempty_register",
        &gnu,
        &neo,
    );
}

#[test]
fn number_to_register_and_increment_register_via_cx_r_n_plus() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "number-register.txt", "7\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "C-x r n");
    let number_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Number to register:"));
    gnu.read_until(Duration::from_secs(6), number_prompt);
    neo.read_until(Duration::from_secs(8), number_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-u 5 C-x r +");
    let increment_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Increment register:"));
    gnu.read_until(Duration::from_secs(6), increment_prompt);
    neo.read_until(Duration::from_secs(8), increment_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "view-register");
    let view_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("View register:"));
    gnu.read_until(Duration::from_secs(6), view_prompt);
    neo.read_until(Duration::from_secs(8), view_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"n");
    }

    let output_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Output*"))
            && grid
                .iter()
                .any(|row| row.contains("Register n contains 12"))
    };
    gnu.read_until(Duration::from_secs(8), output_ready);
    neo.read_until(Duration::from_secs(12), output_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "number_to_register_and_increment_register_via_cx_r_n_plus",
        &gnu,
        &neo,
    );
}

#[test]
fn append_and_prepend_to_register_then_insert_via_mx() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "append-prepend-register.txt",
        "middle\nend\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-a C-SPC C-e");
    invoke_mx_command(&mut gnu, &mut neo, "append-to-register");
    let append_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Append to register:"));
    gnu.read_until(Duration::from_secs(6), append_prompt);
    neo.read_until(Duration::from_secs(8), append_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-n C-a C-SPC C-e");
    invoke_mx_command(&mut gnu, &mut neo, "prepend-to-register");
    let prepend_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Prepend to register:"));
    gnu.read_until(Duration::from_secs(6), prepend_prompt);
    neo.read_until(Duration::from_secs(8), prepend_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-> RET C-x r i");
    let insert_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Insert register:"));
    gnu.read_until(Duration::from_secs(6), insert_prompt);
    neo.read_until(Duration::from_secs(8), insert_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"r");
    }

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("endmiddle"))
            && grid.iter().filter(|row| row.contains("middle")).count() >= 2
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "append_and_prepend_to_register_then_insert_via_mx",
        &gnu,
        &neo,
    );
}

#[test]
fn bookmark_set_and_jump_via_cx_r_m_b() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "bookmark-jump.txt",
        "alpha line\nbeta line\ngamma line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-x r m");
    let set_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Set bookmark named"));
    gnu.read_until(Duration::from_secs(8), set_prompt);
    neo.read_until(Duration::from_secs(10), set_prompt);
    assert!(
        set_prompt(&gnu.text_grid()),
        "GNU should prompt to set bookmark"
    );
    assert!(
        set_prompt(&neo.text_grid()),
        "Neomacs should prompt to set bookmark"
    );
    let gnu_grid = gnu.text_grid();
    let neo_grid = neo.text_grid();
    assert!(
        set_prompt(&gnu_grid),
        "GNU should prompt to set bookmark\n{}",
        gnu_grid.join("\n")
    );
    assert!(
        set_prompt(&neo_grid),
        "Neomacs should prompt to set bookmark\n{}",
        neo_grid.join("\n")
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"jump-spot");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-< C-x r b");
    let jump_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Jump to bookmark"));
    gnu.read_until(Duration::from_secs(8), jump_prompt);
    neo.read_until(Duration::from_secs(10), jump_prompt);
    let gnu_grid = gnu.text_grid();
    let neo_grid = neo.text_grid();
    assert!(
        jump_prompt(&gnu_grid),
        "GNU should prompt to jump to bookmark\n{}",
        gnu_grid.join("\n")
    );
    assert!(
        jump_prompt(&neo_grid),
        "Neomacs should prompt to jump to bookmark\n{}",
        neo_grid.join("\n")
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"jump-spot");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha line"))
            && grid.iter().any(|row| row.contains("Xbeta line"))
            && grid.iter().any(|row| row.contains("gamma line"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(10), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("bookmark_set_and_jump_via_cx_r_m_b", &gnu, &neo);
}

#[test]
fn bookmark_list_via_cx_r_l_shows_saved_bookmark() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "bookmark-list.txt",
        "alpha line\nbeta line\ngamma line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-x r m");
    let set_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Set bookmark named"));
    gnu.read_until(Duration::from_secs(8), set_prompt);
    neo.read_until(Duration::from_secs(10), set_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"list-spot");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x r l");
    let list_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Bookmark List*"))
            && grid.iter().any(|row| row.contains("Bookmark Name"))
            && grid.iter().any(|row| row.contains("list-spot"))
            && grid.iter().any(|row| row.contains("bookmark-list.txt"))
    };
    gnu.read_until(Duration::from_secs(8), list_ready);
    neo.read_until(Duration::from_secs(10), list_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !list_ready(&gnu.text_grid()) || !list_ready(&neo.text_grid()) {
        dump_pair_grids(
            "bookmark_list_via_cx_r_l_shows_saved_bookmark/open",
            &gnu,
            &neo,
        );
    }
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*Bookmark List*")),
            "{label} should display the bookmark list buffer"
        );
        assert!(
            grid.iter().any(|row| row.contains("list-spot")),
            "{label} should list the newly created bookmark"
        );
    }
    assert_pair_exact_display(
        "bookmark_list_via_cx_r_l_shows_saved_bookmark/open",
        &gnu,
        &neo,
    );

    send_both_raw(&mut gnu, &mut neo, b"q");
    let source_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("bookmark-list.txt"))
            && grid.iter().any(|row| row.contains("beta line"))
    };
    gnu.read_until(Duration::from_secs(6), source_ready);
    neo.read_until(Duration::from_secs(8), source_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "bookmark_list_via_cx_r_l_shows_saved_bookmark/quit",
        &gnu,
        &neo,
    );

    // GNU retains the frame's cached Bookmark menu for the quit redisplay.
    // A broad redisplay trigger then rebuilds it from the restored text-mode
    // buffer.  Keep both sides of that cache boundary exact.
    send_both(&mut gnu, &mut neo, "C-l");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "bookmark_list_via_cx_r_l_shows_saved_bookmark/redraw",
        &gnu,
        &neo,
    );
}

#[test]
fn bookmark_rename_and_delete_via_mx_updates_bookmark_list() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "bookmark-rename-delete.txt",
        "alpha line\nbeta line\ngamma line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-n C-x r m");
    let set_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Set bookmark named"));
    gnu.read_until(Duration::from_secs(8), set_prompt);
    neo.read_until(Duration::from_secs(10), set_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"old-bookmark");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "bookmark-rename");
    let old_name_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Old bookmark name"));
    gnu.read_until(Duration::from_secs(8), old_name_prompt);
    neo.read_until(Duration::from_secs(10), old_name_prompt);
    assert!(
        old_name_prompt(&gnu.text_grid()),
        "GNU should prompt for old bookmark name:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        old_name_prompt(&neo.text_grid()),
        "Neomacs should prompt for old bookmark name:\n{}",
        neo.text_grid().join("\n")
    );
    for session in [&mut gnu, &mut neo] {
        session.send(b"old-bookmark");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let new_name_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Rename") && row.contains("old-bookmark"))
    };
    gnu.read_until(Duration::from_secs(8), new_name_prompt);
    neo.read_until(Duration::from_secs(10), new_name_prompt);
    assert!(
        new_name_prompt(&gnu.text_grid()),
        "GNU should prompt for new bookmark name:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        new_name_prompt(&neo.text_grid()),
        "Neomacs should prompt for new bookmark name:\n{}",
        neo.text_grid().join("\n")
    );
    for session in [&mut gnu, &mut neo] {
        session.send(b"new-bookmark");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x r l");
    let renamed_list_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Bookmark List*"))
            && grid.iter().any(|row| row.contains("new-bookmark"))
            && !grid.iter().any(|row| row.contains("old-bookmark"))
    };
    gnu.read_until(Duration::from_secs(8), renamed_list_ready);
    neo.read_until(Duration::from_secs(10), renamed_list_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "bookmark_rename_and_delete_via_mx_updates_bookmark_list/renamed",
        &gnu,
        &neo,
    );

    send_both_raw(&mut gnu, &mut neo, b"q");
    let source_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("bookmark-rename-delete.txt"))
            && grid.iter().any(|row| row.contains("beta line"))
    };
    gnu.read_until(Duration::from_secs(6), source_ready);
    neo.read_until(Duration::from_secs(8), source_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "bookmark-delete");
    let delete_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Delete bookmark"));
    gnu.read_until(Duration::from_secs(8), delete_prompt);
    neo.read_until(Duration::from_secs(10), delete_prompt);
    assert!(
        delete_prompt(&gnu.text_grid()),
        "GNU should prompt to delete bookmark:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        delete_prompt(&neo.text_grid()),
        "Neomacs should prompt to delete bookmark:\n{}",
        neo.text_grid().join("\n")
    );
    for session in [&mut gnu, &mut neo] {
        session.send(b"new-bookmark");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x r l");
    let deleted_list_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("*Bookmark List*"));
    gnu.read_until(Duration::from_secs(8), deleted_list_ready);
    neo.read_until(Duration::from_secs(10), deleted_list_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            !grid.iter().any(|row| row.contains("new-bookmark")),
            "{label} should remove deleted bookmark from bookmark list:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "bookmark_rename_and_delete_via_mx_updates_bookmark_list/deleted",
        &gnu,
        &neo,
    );
}

#[test]
fn recentf_mode_tracks_opened_files_and_lists_them_via_mx() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "recentf-mode");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    open_home_file(
        &mut gnu,
        &mut neo,
        "recentf-first.txt",
        "first recent file\n",
        "C-x C-f",
    );
    open_home_file(
        &mut gnu,
        &mut neo,
        "recentf-second.txt",
        "second recent file\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "recentf-open-files");
    let recentf_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Open Recent*"))
            && grid.iter().any(|row| row.contains("Click on a file"))
            && grid.iter().any(|row| row.contains("recentf-first.txt"))
            && grid.iter().any(|row| row.contains("recentf-second.txt"))
    };
    gnu.read_until(Duration::from_secs(8), recentf_ready);
    neo.read_until(Duration::from_secs(12), recentf_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert!(
        gnu.text_grid()
            .iter()
            .any(|row| row.contains("type q to cancel")),
        "GNU Recentf dialog should advertise q to cancel"
    );
    assert!(
        neo.text_grid()
            .iter()
            .any(|row| row.contains("type q to cancel")),
        "Neomacs Recentf dialog should advertise q to cancel"
    );

    assert_pair_exact_display(
        "recentf_mode_tracks_opened_files_and_lists_them_via_mx",
        &gnu,
        &neo,
    );
}

#[test]
fn bookmark_set_and_jump_via_cx_rm_remembers_file_position() {
    let (mut gnu, mut neo) = boot_pair("");
    let contents = "marker line is here\nline two\nline three\nline four\n";
    let name = "bm-set-jump.txt";
    open_home_file(&mut gnu, &mut neo, name, contents, "C-x C-f");

    // Move to "marker line is here"
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // Set bookmark: C-x r m then name RET
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "r");
    send_both(&mut gnu, &mut neo, "m");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for s in [&mut gnu, &mut neo] {
        s.send(b"bm-test\r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Move to end of file
    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    // Jump to bookmark: C-x r b then name RET
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "r");
    send_both(&mut gnu, &mut neo, "b");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for s in [&mut gnu, &mut neo] {
        s.send(b"bm-test\r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify we're at the marker line
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        // The file has 4 lines; after jumping to bookmark, it should be visible
        assert!(
            grid.iter().any(|r| r.contains(name)),
            "{label} should be in the bookmarked file"
        );
    }
    assert_pair_exact_display(
        "bookmark_set_and_jump_via_cx_rm_remembers_file_position",
        &gnu,
        &neo,
    );
}
