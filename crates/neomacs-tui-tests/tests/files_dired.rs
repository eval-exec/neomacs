#![cfg(unix)]
//! TUI comparison tests: files dired.

use crate::support;
use neomacs_tui_tests::*;
use std::fs;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

fn make_shared_dired_fixture(label: &str) -> TuiTempDirectory {
    let directory = TuiTempDirectory::new_with_private_parent(
        &format!("neomacs-dired-root-{label}-"),
        "listing",
    );
    fs::create_dir_all(directory.join("nested")).expect("create dired fixture directory");
    fs::write(directory.join("alpha.txt"), "alpha body\n").expect("write alpha fixture");
    fs::write(directory.join("beta.org"), "* beta heading\n").expect("write beta fixture");
    fs::write(directory.join("zeta.log"), "zeta body\n").expect("write zeta fixture");
    directory
}

fn boot_shared_dired_pair() -> (TuiSession, TuiSession) {
    let (mut gnu, mut neo) = boot_pair("");
    use_deterministic_file_system_info(&mut gnu, &mut neo);
    (gnu, neo)
}

fn open_shared_dired(gnu: &mut TuiSession, neo: &mut TuiSession, dir: &std::path::Path) {
    send_both(gnu, neo, "C-x d");
    let dired_path = format!("{}/", dir.display());
    gnu.send(dired_path.as_bytes());
    neo.send(dired_path.as_bytes());
    send_both(gnu, neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Dired by name"))
            && ["alpha.txt", "beta.org", "nested", "zeta.log"]
                .iter()
                .all(|name| grid.iter().any(|row| row.contains(name)))
    };
    gnu.read_until(Duration::from_secs(10), ready);
    neo.read_until(Duration::from_secs(20), ready);
    read_both(gnu, neo, Duration::from_secs(1));
}

fn dired_goto_file(gnu: &mut TuiSession, neo: &mut TuiSession, file: &std::path::Path) {
    send_both(gnu, neo, "j");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Goto file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    let file_name = file
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixture file name should be utf-8")
        .to_string();
    let file_path = file.to_string_lossy().into_owned();
    gnu.send(file_path.as_bytes());
    neo.send(file_path.as_bytes());
    send_both(gnu, neo, "RET");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains(&file_name))
            && grid.last().is_none_or(|row| !row.contains("Goto file:"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(gnu, neo, Duration::from_secs(1));
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn find_file_via_cx_cf() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "common-usage.txt",
        "alpha line\nbeta line\n",
        "C-x C-f",
    );
    assert_pair_exact_display("find_file_via_cx_cf", &gnu, &neo);
}

#[test]
fn find_file_literally_via_mx_visits_file_without_modes() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "literal-visit.txt", "literal visit body\n");
    write_home_file(&neo, "literal-visit.txt", "literal visit body\n");

    invoke_mx_command(&mut gnu, &mut neo, "find-file-literally");
    let prompt_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Find file literally:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/literal-visit.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("literal-visit.txt"))
            && grid.iter().any(|row| row.contains("literal visit body"))
            && grid.iter().any(|row| row.contains("Fundamental"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "find_file_literally_via_mx_visits_file_without_modes",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_literally_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "find-file-literally");
    let prompt_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Find file literally:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Find file literally:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty find-file-literally prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "find_file_literally_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_tab_completion_via_cx_cf_completes_unique_home_file() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(
        &gnu,
        "completion-unique-target.txt",
        "completed file body\n",
    );
    write_home_file(
        &neo,
        "completion-unique-target.txt",
        "completed file body\n",
    );

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "find_file_tab_completion_via_cx_cf_completes_unique_home_file/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/completion-uniq");
    }
    send_both(&mut gnu, &mut neo, "TAB");
    let completed = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("completion-unique-target.txt"))
    };
    gnu.read_until(Duration::from_secs(6), completed);
    neo.read_until(Duration::from_secs(8), completed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "find_file_tab_completion_via_cx_cf_completes_unique_home_file/completed",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("completion-unique-target.txt"))
            && grid.iter().any(|row| row.contains("completed file body"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "find_file_tab_completion_via_cx_cf_completes_unique_home_file",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_minibuffer_del_deletes_previous_character() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "minibuffer-del-target.txt", "minibuffer del body\n");
    write_home_file(&neo, "minibuffer-del-target.txt", "minibuffer del body\n");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/minibuffer-del-target.txX");
    }
    send_both(&mut gnu, &mut neo, "DEL");
    for session in [&mut gnu, &mut neo] {
        session.send(b"t");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("minibuffer-del-target.txt"))
            && grid.iter().any(|row| row.contains("minibuffer del body"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "find_file_minibuffer_del_deletes_previous_character",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_empty_minibuffer_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Find file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty C-x C-f prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "find_file_empty_minibuffer_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_minibuffer_ctrl_h_does_not_delete_previous_character() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/ctrl-h-probeX");
    }
    send_both(&mut gnu, &mut neo, "BS");

    let path_preserved = |grid: &[String]| grid.iter().any(|row| row.contains("~/ctrl-h-probeX"));
    gnu.read_until(Duration::from_secs(6), path_preserved);
    neo.read_until(Duration::from_secs(8), path_preserved);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("~/ctrl-h-probeX")),
            "{label} should keep the previous minibuffer character after terminal C-h\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "find_file_minibuffer_ctrl_h_does_not_delete_previous_character",
        &gnu,
        &neo,
    );
}

#[test]
fn keyboard_quit_after_find_file_ctrl_h_returns_to_scratch() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/ctrl-h-quit-probe");
    }
    send_both(&mut gnu, &mut neo, "BS C-g");

    let scratch_only =
        |grid: &[String]| scratch_ready(grid) && !grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), scratch_only);
    neo.read_until(Duration::from_secs(8), scratch_only);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "keyboard_quit_after_find_file_ctrl_h_returns_to_scratch",
        &gnu,
        &neo,
    );
}

#[test]
fn list_directory_via_cx_cd_lists_entries() {
    let (mut gnu, mut neo) = boot_pair("");
    let dir = TuiTempDirectory::new("neomacs-list-directory-");
    fs::create_dir_all(dir.join("nested")).expect("create list-directory fixture");
    fs::write(dir.join("alpha.txt"), "alpha body\n").expect("write alpha fixture");
    fs::write(dir.join("beta.org"), "* beta heading\n").expect("write beta fixture");
    fs::write(dir.join("zeta.log"), "zeta body\n").expect("write zeta fixture");

    send_both(&mut gnu, &mut neo, "C-x C-d");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List directory (brief):"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    let list_path = format!("{}/", dir.display());
    gnu.send(list_path.as_bytes());
    neo.send(list_path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Directory*"))
            && grid.iter().any(|row| row.contains("Directory "))
            && ["alpha.txt", "beta.org", "nested", "zeta.log"]
                .iter()
                .all(|name| grid.iter().any(|row| row.contains(name)))
    };
    gnu.read_until(Duration::from_secs(10), ready);
    neo.read_until(Duration::from_secs(20), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("list_directory_via_cx_cd_lists_entries", &gnu, &neo);
}

#[test]
fn dired_open_directory_via_cx_d_lists_entries() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("open");

    open_shared_dired(&mut gnu, &mut neo, &dir);

    assert_pair_exact_display("dired_open_directory_via_cx_d_lists_entries", &gnu, &neo);
}

#[test]
fn dired_mark_flag_and_unmark_current_file() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("mark");
    let alpha = dir.join("alpha.txt");
    let beta = dir.join("beta.org");

    open_shared_dired(&mut gnu, &mut neo, &dir);
    dired_goto_file(&mut gnu, &mut neo, &alpha);
    send_both(&mut gnu, &mut neo, "m");
    let alpha_marked = |grid: &[String]| {
        grid.iter()
            .any(|row| row.starts_with('*') && row.contains("alpha.txt"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_marked);
    neo.read_until(Duration::from_secs(8), alpha_marked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("dired_mark_flag_and_unmark_current_file/mark", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "DEL");
    let alpha_unmarked = |grid: &[String]| {
        grid.iter()
            .any(|row| !row.starts_with('*') && row.contains("alpha.txt"))
            && !grid
                .iter()
                .any(|row| row.starts_with('*') && row.contains("alpha.txt"))
    };
    gnu.read_until(Duration::from_secs(6), alpha_unmarked);
    neo.read_until(Duration::from_secs(8), alpha_unmarked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("dired_mark_flag_and_unmark_current_file/unmark", &gnu, &neo);

    dired_goto_file(&mut gnu, &mut neo, &beta);
    send_both(&mut gnu, &mut neo, "d");
    let beta_flagged = |grid: &[String]| {
        grid.iter()
            .any(|row| row.starts_with('D') && row.contains("beta.org"))
    };
    gnu.read_until(Duration::from_secs(6), beta_flagged);
    neo.read_until(Duration::from_secs(8), beta_flagged);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("dired_mark_flag_and_unmark_current_file/flag", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "DEL");
    let beta_unflagged = |grid: &[String]| {
        grid.iter()
            .any(|row| !row.starts_with('D') && row.contains("beta.org"))
            && !grid
                .iter()
                .any(|row| row.starts_with('D') && row.contains("beta.org"))
    };
    gnu.read_until(Duration::from_secs(6), beta_unflagged);
    neo.read_until(Duration::from_secs(8), beta_unflagged);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("dired_mark_flag_and_unmark_current_file/unflag", &gnu, &neo);
}

#[test]
fn dired_find_file_via_ret_visits_current_file() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("find-file");
    let beta = dir.join("beta.org");

    open_shared_dired(&mut gnu, &mut neo, &dir);
    dired_goto_file(&mut gnu, &mut neo, &beta);
    send_both(&mut gnu, &mut neo, "RET");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("* beta heading"))
            && grid.iter().any(|row| row.contains("beta.org"))
            && !grid.iter().any(|row| row.contains("Dired by name"))
    };
    gnu.read_until(Duration::from_secs(60), ready);
    neo.read_until(Duration::from_secs(60), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("dired_find_file_via_ret_visits_current_file", &gnu, &neo);
}

#[test]
fn dired_jump_via_cx_cj_opens_parent_listing_on_current_file() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("jump");
    let beta = dir.join("beta.org");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let beta_path = beta.to_string_lossy().into_owned();
    gnu.send(beta_path.as_bytes());
    neo.send(beta_path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let file_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta heading"))
            && grid.iter().any(|row| row.contains("beta.org"))
    };
    gnu.read_until(Duration::from_secs(6), file_ready);
    neo.read_until(Duration::from_secs(8), file_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-j");
    let dired_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Dired by name"))
            && ["alpha.txt", "beta.org", "nested", "zeta.log"]
                .iter()
                .all(|name| grid.iter().any(|row| row.contains(name)))
    };
    gnu.read_until(Duration::from_secs(10), dired_ready);
    neo.read_until(Duration::from_secs(20), dired_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "dired_jump_via_cx_cj_opens_parent_listing_on_current_file/dired",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    gnu.read_until(Duration::from_secs(6), file_ready);
    neo.read_until(Duration::from_secs(8), file_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "dired_jump_via_cx_cj_opens_parent_listing_on_current_file/revisit",
        &gnu,
        &neo,
    );
}

#[test]
fn dired_jump_other_window_via_cx4_cj_keeps_file_and_listing_visible() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("jump-other-window");
    let beta = dir.join("beta.org");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let beta_path = beta.to_string_lossy().into_owned();
    gnu.send(beta_path.as_bytes());
    neo.send(beta_path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let file_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta heading"))
            && grid.iter().any(|row| row.contains("beta.org"))
    };
    gnu.read_until(Duration::from_secs(6), file_ready);
    neo.read_until(Duration::from_secs(8), file_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x 4 C-j");
    let jump_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Dired by name"))
            && grid.iter().any(|row| row.contains("beta heading"))
            && ["alpha.txt", "beta.org", "nested", "zeta.log"]
                .iter()
                .all(|name| grid.iter().any(|row| row.contains(name)))
    };
    gnu.read_until(Duration::from_secs(10), jump_ready);
    neo.read_until(Duration::from_secs(20), jump_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display(
        "dired_jump_other_window_via_cx4_cj_keeps_file_and_listing_visible",
        &gnu,
        &neo,
    );
}

#[test]
fn dired_copy_current_file_via_c_copies_file_and_updates_listing() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("copy");
    let alpha = dir.join("alpha.txt");
    let alpha_copy = dir.join("alpha-copy.txt");
    let alpha_copy_path = alpha_copy.to_string_lossy().into_owned();

    open_shared_dired(&mut gnu, &mut neo, &dir);
    dired_goto_file(&mut gnu, &mut neo, &alpha);

    gnu.send_keys("C");
    let copy_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Copy") && row.contains("alpha.txt") && row.contains("to:"))
    };
    gnu.read_until(Duration::from_secs(6), copy_prompt);
    gnu.send(alpha_copy_path.as_bytes());
    gnu.send_keys("RET");
    let copy_ready = |grid: &[String]| grid.iter().any(|row| row.contains("alpha-copy.txt"));
    gnu.read_until(Duration::from_secs(10), copy_ready);
    gnu.read(Duration::from_secs(1));
    assert_eq!(
        fs::read_to_string(&alpha_copy).expect("GNU should copy dired file"),
        "alpha body\n"
    );

    fs::remove_file(&alpha_copy).expect("reset copied file before Neomacs operation");

    neo.send_keys("C");
    neo.read_until(Duration::from_secs(8), copy_prompt);
    neo.send(alpha_copy_path.as_bytes());
    neo.send_keys("RET");
    neo.read_until(Duration::from_secs(12), copy_ready);
    neo.read(Duration::from_secs(1));
    assert_eq!(
        fs::read_to_string(&alpha_copy).expect("Neomacs should copy dired file"),
        "alpha body\n"
    );

    assert_pair_exact_display(
        "dired_copy_current_file_via_c_copies_file_and_updates_listing",
        &gnu,
        &neo,
    );
}

#[test]
fn dired_rename_current_file_via_r_moves_file_and_updates_listing() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("rename");
    let beta = dir.join("beta.org");
    let beta_renamed = dir.join("beta-renamed.org");
    let beta_renamed_path = beta_renamed.to_string_lossy().into_owned();

    open_shared_dired(&mut gnu, &mut neo, &dir);
    dired_goto_file(&mut gnu, &mut neo, &beta);

    gnu.send_keys("R");
    let rename_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Rename") && row.contains("beta.org") && row.contains("to:"))
    };
    gnu.read_until(Duration::from_secs(6), rename_prompt);
    gnu.send(beta_renamed_path.as_bytes());
    gnu.send_keys("RET");
    let rename_ready = |grid: &[String]| grid.iter().any(|row| row.contains("beta-renamed.org"));
    gnu.read_until(Duration::from_secs(10), rename_ready);
    gnu.read(Duration::from_secs(1));
    assert!(
        beta_renamed.exists() && !beta.exists(),
        "GNU should rename beta.org to beta-renamed.org"
    );

    fs::rename(&beta_renamed, &beta).expect("reset renamed file before Neomacs operation");

    neo.send_keys("R");
    neo.read_until(Duration::from_secs(8), rename_prompt);
    neo.send(beta_renamed_path.as_bytes());
    neo.send_keys("RET");
    neo.read_until(Duration::from_secs(12), rename_ready);
    neo.read(Duration::from_secs(1));
    assert!(
        beta_renamed.exists() && !beta.exists(),
        "Neomacs should rename beta.org to beta-renamed.org"
    );

    assert_pair_exact_display(
        "dired_rename_current_file_via_r_moves_file_and_updates_listing",
        &gnu,
        &neo,
    );
}

#[test]
fn dired_delete_current_file_via_d_confirms_and_removes_listing() {
    let (mut gnu, mut neo) = boot_shared_dired_pair();
    let dir = make_shared_dired_fixture("delete");
    let zeta = dir.join("zeta.log");

    open_shared_dired(&mut gnu, &mut neo, &dir);
    dired_goto_file(&mut gnu, &mut neo, &zeta);

    gnu.send_keys("D");
    let delete_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Delete") && row.contains("zeta.log"))
    };
    gnu.read_until(Duration::from_secs(6), delete_prompt);
    gnu.send(b"yes");
    gnu.send_keys("RET");
    let delete_ready = |grid: &[String]| !grid.iter().any(|row| row.contains("zeta.log"));
    gnu.read_until(Duration::from_secs(10), delete_ready);
    gnu.read(Duration::from_secs(1));
    assert!(!zeta.exists(), "GNU should delete zeta.log from disk");

    fs::write(&zeta, "zeta body\n").expect("reset deleted file before Neomacs operation");

    neo.send_keys("D");
    neo.read_until(Duration::from_secs(8), delete_prompt);
    neo.send(b"yes");
    neo.send_keys("RET");
    neo.read_until(Duration::from_secs(12), delete_ready);
    neo.read(Duration::from_secs(1));
    assert!(!zeta.exists(), "Neomacs should delete zeta.log from disk");

    assert_pair_exact_display(
        "dired_delete_current_file_via_d_confirms_and_removes_listing",
        &gnu,
        &neo,
    );
}

#[test]
fn make_directory_via_mx_creates_directory_on_disk() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "make-directory");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Make directory:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "make_directory_via_mx_creates_directory_on_disk/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/mx-made-dir");
    }
    let typed_ready = |grid: &[String]| grid.iter().any(|row| row.contains("mx-made-dir"));
    gnu.read_until(Duration::from_secs(6), typed_ready);
    neo.read_until(Duration::from_secs(8), typed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "make_directory_via_mx_creates_directory_on_disk/before-ret",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let gnu_dir = gnu.home_dir().join("mx-made-dir");
    let neo_dir = neo.home_dir().join("mx-made-dir");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        if gnu_dir.is_dir() && neo_dir.is_dir() {
            break;
        }
    }

    assert!(
        gnu_dir.is_dir(),
        "GNU should create directory via M-x make-directory"
    );
    assert!(
        neo_dir.is_dir(),
        "Neomacs should create directory via M-x make-directory"
    );

    send_both(&mut gnu, &mut neo, "C-l");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "make_directory_via_mx_creates_directory_on_disk",
        &gnu,
        &neo,
    );
}

#[test]
fn make_directory_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "make-directory");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Make directory:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Make directory:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty make-directory prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "make_directory_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn rename_file_via_mx_moves_file_on_disk() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "rename-source.txt", "rename me\n");
    write_home_file(&neo, "rename-source.txt", "rename me\n");

    invoke_mx_command(&mut gnu, &mut neo, "rename-file");
    let source_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Rename file:"));
    gnu.read_until(Duration::from_secs(6), source_prompt);
    neo.read_until(Duration::from_secs(8), source_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "rename_file_via_mx_moves_file_on_disk/source-prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/rename-source.txt");
    }
    let source_typed = |grid: &[String]| grid.iter().any(|row| row.contains("rename-source.txt"));
    gnu.read_until(Duration::from_secs(6), source_typed);
    neo.read_until(Duration::from_secs(8), source_typed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "rename_file_via_mx_moves_file_on_disk/source-before-ret",
        &gnu,
        &neo,
    );
    send_both(&mut gnu, &mut neo, "RET");

    let dest_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("to file:"));
    gnu.read_until(Duration::from_secs(6), dest_prompt);
    neo.read_until(Duration::from_secs(8), dest_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "rename_file_via_mx_moves_file_on_disk/dest-prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/rename-dest.txt");
    }
    let dest_typed = |grid: &[String]| grid.iter().any(|row| row.contains("rename-dest.txt"));
    gnu.read_until(Duration::from_secs(6), dest_typed);
    neo.read_until(Duration::from_secs(8), dest_typed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "rename_file_via_mx_moves_file_on_disk/dest-before-ret",
        &gnu,
        &neo,
    );
    send_both(&mut gnu, &mut neo, "RET");

    let gnu_src = gnu.home_dir().join("rename-source.txt");
    let neo_src = neo.home_dir().join("rename-source.txt");
    let gnu_dest = gnu.home_dir().join("rename-dest.txt");
    let neo_dest = neo.home_dir().join("rename-dest.txt");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        if !gnu_src.exists() && !neo_src.exists() && gnu_dest.exists() && neo_dest.exists() {
            break;
        }
    }

    assert!(
        !gnu_src.exists(),
        "GNU rename-file should remove source path"
    );
    assert!(
        !neo_src.exists(),
        "Neomacs rename-file should remove source path"
    );
    assert_eq!(
        fs::read_to_string(&gnu_dest).expect("read GNU renamed file"),
        "rename me\n"
    );
    assert_eq!(
        fs::read_to_string(&neo_dest).expect("read Neomacs renamed file"),
        "rename me\n"
    );

    send_both(&mut gnu, &mut neo, "C-l");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("rename_file_via_mx_moves_file_on_disk", &gnu, &neo);
}

#[test]
fn rename_file_empty_source_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "rename-empty-source.txt", "rename me\n");
    write_home_file(&neo, "rename-empty-source.txt", "rename me\n");

    invoke_mx_command(&mut gnu, &mut neo, "rename-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Rename file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Rename file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty rename-file source prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "rename_file_empty_source_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_file_via_mx_removes_file_from_disk() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "delete-direct.txt", "delete me\n");
    write_home_file(&neo, "delete-direct.txt", "delete me\n");

    invoke_mx_command(&mut gnu, &mut neo, "delete-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Delete file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "delete_file_via_mx_removes_file_from_disk/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/delete-direct.txt");
    }
    let typed_ready = |grid: &[String]| grid.iter().any(|row| row.contains("delete-direct.txt"));
    gnu.read_until(Duration::from_secs(6), typed_ready);
    neo.read_until(Duration::from_secs(8), typed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "delete_file_via_mx_removes_file_from_disk/before-ret",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    let gnu_path = gnu.home_dir().join("delete-direct.txt");
    let neo_path = neo.home_dir().join("delete-direct.txt");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        if !gnu_path.exists() && !neo_path.exists() {
            break;
        }
    }

    assert!(
        !gnu_path.exists(),
        "GNU delete-file should remove target path"
    );
    assert!(
        !neo_path.exists(),
        "Neomacs delete-file should remove target path"
    );

    send_both(&mut gnu, &mut neo, "C-l");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("delete_file_via_mx_removes_file_from_disk", &gnu, &neo);
}

#[test]
fn delete_file_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "delete-empty-del.txt", "delete me\n");
    write_home_file(&neo, "delete-empty-del.txt", "delete me\n");

    invoke_mx_command(&mut gnu, &mut neo, "delete-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Delete file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Delete file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty delete-file prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_file_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_directory_via_mx_removes_empty_directory() {
    let (mut gnu, mut neo) = boot_pair("");
    let gnu_dir = gnu.home_dir().join("delete-empty-dir");
    let neo_dir = neo.home_dir().join("delete-empty-dir");
    fs::create_dir(&gnu_dir).expect("create GNU empty directory");
    fs::create_dir(&neo_dir).expect("create Neo empty directory");

    invoke_mx_command(&mut gnu, &mut neo, "delete-directory");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Delete directory:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "delete_directory_via_mx_removes_empty_directory/prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/delete-empty-dir");
    }
    let typed_ready = |grid: &[String]| grid.iter().any(|row| row.contains("delete-empty-dir"));
    gnu.read_until(Duration::from_secs(6), typed_ready);
    neo.read_until(Duration::from_secs(8), typed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "delete_directory_via_mx_removes_empty_directory/before-ret",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        if !gnu_dir.exists() && !neo_dir.exists() {
            break;
        }
    }

    assert!(
        !gnu_dir.exists(),
        "GNU delete-directory should remove target directory"
    );
    assert!(
        !neo_dir.exists(),
        "Neomacs delete-directory should remove target directory"
    );

    send_both(&mut gnu, &mut neo, "C-l");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "delete_directory_via_mx_removes_empty_directory",
        &gnu,
        &neo,
    );
}

#[test]
fn delete_directory_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    let gnu_dir = gnu.home_dir().join("delete-empty-dir-prompt");
    let neo_dir = neo.home_dir().join("delete-empty-dir-prompt");
    fs::create_dir(&gnu_dir).expect("create GNU empty directory");
    fs::create_dir(&neo_dir).expect("create Neo empty directory");

    invoke_mx_command(&mut gnu, &mut neo, "delete-directory");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Delete directory:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Delete directory:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty delete-directory prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "delete_directory_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn copy_directory_via_mx_copies_nested_tree() {
    let (mut gnu, mut neo) = boot_pair("");
    for session in [&gnu, &neo] {
        let source = session.home_dir().join("copy-source-dir");
        fs::create_dir_all(source.join("nested")).expect("create copy-directory fixture");
        fs::write(source.join("alpha.txt"), "alpha copy\n").expect("write alpha fixture");
        fs::write(source.join("nested").join("beta.txt"), "beta copy\n")
            .expect("write nested beta fixture");
    }

    invoke_mx_command(&mut gnu, &mut neo, "copy-directory");
    let source_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Copy directory:"));
    gnu.read_until(Duration::from_secs(6), source_prompt);
    neo.read_until(Duration::from_secs(8), source_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display(
        "copy_directory_via_mx_copies_nested_tree/source-prompt",
        &gnu,
        &neo,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/copy-source-dir");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let dest_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Copy directory") && row.contains(" to:"))
    };
    gnu.read_until(Duration::from_secs(6), dest_prompt);
    neo.read_until(Duration::from_secs(8), dest_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/copy-dest-dir");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let gnu_dest = gnu.home_dir().join("copy-dest-dir");
    let neo_dest = neo.home_dir().join("copy-dest-dir");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        if gnu_dest.join("alpha.txt").exists()
            && neo_dest.join("alpha.txt").exists()
            && gnu_dest.join("nested").join("beta.txt").exists()
            && neo_dest.join("nested").join("beta.txt").exists()
        {
            break;
        }
    }

    assert_eq!(
        fs::read_to_string(gnu_dest.join("alpha.txt")).expect("read GNU copied alpha"),
        "alpha copy\n"
    );
    assert_eq!(
        fs::read_to_string(neo_dest.join("alpha.txt")).expect("read Neo copied alpha"),
        "alpha copy\n"
    );
    assert_eq!(
        fs::read_to_string(gnu_dest.join("nested").join("beta.txt"))
            .expect("read GNU copied nested beta"),
        "beta copy\n"
    );
    assert_eq!(
        fs::read_to_string(neo_dest.join("nested").join("beta.txt"))
            .expect("read Neo copied nested beta"),
        "beta copy\n"
    );

    send_both(&mut gnu, &mut neo, "C-l");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display("copy_directory_via_mx_copies_nested_tree", &gnu, &neo);
}

#[test]
fn copy_directory_empty_source_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    for session in [&gnu, &neo] {
        let source = session.home_dir().join("copy-empty-source-dir");
        fs::create_dir_all(source.join("nested")).expect("create copy-directory fixture");
        fs::write(source.join("alpha.txt"), "alpha copy\n").expect("write alpha fixture");
    }

    invoke_mx_command(&mut gnu, &mut neo, "copy-directory");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Copy directory:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Copy directory:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty copy-directory source prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "copy_directory_empty_source_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_other_window_via_cx4_cf() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "other-window.txt",
        "window line 1\nwindow line 2\n",
        "C-x 4 C-f",
    );

    let ready = |grid: &[String]| {
        grid.iter().filter(|row| row.contains("*scratch*")).count() >= 1
            && grid.iter().any(|row| row.contains("other-window.txt"))
            && grid.iter().any(|row| row.contains("window line 1"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("find_file_other_window_via_cx4_cf", &gnu, &neo);
}

#[test]
fn find_file_read_only_then_toggle_and_save_via_cx_cr_cx_cq() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "read-only-toggle.txt",
        "original read-only body\n",
        "C-x C-r",
    );

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "readonly:%S" buffer-read-only)"#);
    }
    send_both(&mut gnu, &mut neo, "RET");
    let readonly_ready = |grid: &[String]| grid.iter().any(|row| row.contains("readonly:t"));
    gnu.read_until(Duration::from_secs(6), readonly_ready);
    neo.read_until(Duration::from_secs(8), readonly_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "find_file_read_only_then_toggle_and_save_via_cx_cr_cx_cq/readonly",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-x C-q");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"edited line\n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "find_file_read_only_then_toggle_and_save_via_cx_cr_cx_cq",
        &mut gnu,
        &mut neo,
        "read-only-toggle.txt",
        "edited line\noriginal read-only body\n",
    );
    assert_pair_exact_display(
        "find_file_read_only_then_toggle_and_save_via_cx_cr_cx_cq",
        &gnu,
        &neo,
    );
}

#[test]
fn find_alternate_file_via_cx_cv_replaces_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "alternate-new.txt", "new alternate body\n");
    write_home_file(&neo, "alternate-new.txt", "new alternate body\n");
    open_home_file(
        &mut gnu,
        &mut neo,
        "alternate-old.txt",
        "old alternate body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-v");
    let prompt_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Find alternate file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "find_alternate_file_via_cx_cv_replaces_buffer/prompt",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "C-a C-k");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"~/alternate-new.txt");
    }
    let typed_path = |grid: &[String]| grid.iter().any(|row| row.contains("alternate-new.txt"));
    gnu.read_until(Duration::from_secs(6), typed_path);
    neo.read_until(Duration::from_secs(8), typed_path);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_exact_display(
        "find_alternate_file_via_cx_cv_replaces_buffer/before-ret",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alternate-new.txt"))
            && grid.iter().any(|row| row.contains("new alternate body"))
            && !grid.iter().any(|row| row.contains("old alternate body"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_exact_display("find_alternate_file_via_cx_cv_replaces_buffer", &gnu, &neo);
}

#[test]
fn find_alternate_file_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "alternate-empty-new.txt", "new alternate body\n");
    write_home_file(&neo, "alternate-empty-new.txt", "new alternate body\n");
    open_home_file(
        &mut gnu,
        &mut neo,
        "alternate-empty-old.txt",
        "old alternate body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-v");
    let prompt_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Find alternate file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "C-a C-k DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Find alternate file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty find-alternate-file prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_exact_display(
        "find_alternate_file_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
    );
}

#[test]
fn find_file_via_cx_cf_opens_existing_file_with_contents_visible() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "cx-cf-find-visible.txt";
    let content = "first line of visible content\nsecond line\n";

    open_home_file(&mut gnu, &mut neo, name, content, "C-x C-f");

    // Now re-open the same file via C-x C-f again (should go to same buffer)
    send_both(&mut gnu, &mut neo, "C-x C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(format!("{}\r", name).as_bytes());
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("first line of visible content")),
            "{label}: reopening via C-x C-f should show the file content"
        );
        assert!(
            grid.iter().any(|r| r.contains(name)),
            "{label}: mode-line should show the reopened file name"
        );
    }
    assert_pair_exact_display(
        "find_file_via_cx_cf_opens_existing_file_with_contents_visible",
        &gnu,
        &neo,
    );
}
