#![cfg(unix)]
//! TUI comparison tests: menu bar.

mod support;
use std::time::Duration;
use support::*;

fn file_menu_ready(grid: &[String]) -> bool {
    grid.iter().any(|row| row.contains("Visit New File"))
        && grid.iter().any(|row| row.contains("Open File"))
}

#[test]
fn f10_opens_tty_menu_bar_file_menu_and_cg_closes_it() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "F10");
    gnu.read_until(Duration::from_secs(8), file_menu_ready);
    neo.read_until(Duration::from_secs(12), file_menu_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !file_menu_ready(&gnu.text_grid()) || !file_menu_ready(&neo.text_grid()) {
        dump_pair_grids(
            "f10_opens_tty_menu_bar_file_menu_and_cg_closes_it/open",
            &gnu,
            &neo,
        );
    }
    assert!(
        file_menu_ready(&gnu.text_grid()),
        "GNU should show the File menu after F10"
    );
    assert!(
        file_menu_ready(&neo.text_grid()),
        "Neomacs should show the File menu after F10"
    );

    send_both(&mut gnu, &mut neo, "C-g");
    let closed = |grid: &[String]| scratch_ready(grid) && !file_menu_ready(grid);
    gnu.read_until(Duration::from_secs(8), closed);
    neo.read_until(Duration::from_secs(12), closed);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !closed(&gnu.text_grid()) || !closed(&neo.text_grid()) {
        dump_pair_grids(
            "f10_opens_tty_menu_bar_file_menu_and_cg_closes_it/close",
            &gnu,
            &neo,
        );
    }
    assert!(
        closed(&gnu.text_grid()),
        "GNU should close the F10 menu on C-g"
    );
    assert!(
        closed(&neo.text_grid()),
        "Neomacs should close the F10 menu on C-g"
    );
    assert_pair_exact_display(
        "f10_opens_tty_menu_bar_file_menu_and_cg_closes_it",
        &gnu,
        &neo,
    );
}

#[test]
fn f10_can_navigate_to_help_menu_and_open_about_emacs() {
    let (mut gnu, mut neo) = boot_pair("");
    use_deterministic_emacs_version(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "F10");
    gnu.read_until(Duration::from_secs(8), file_menu_ready);
    neo.read_until(Duration::from_secs(12), file_menu_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "RIGHT RIGHT RIGHT RIGHT RIGHT RIGHT");
    let help_menu_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("About Emacs"))
            && grid.iter().any(|row| row.contains("About GNU"))
    };
    gnu.read_until(Duration::from_secs(8), help_menu_ready);
    neo.read_until(Duration::from_secs(12), help_menu_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !help_menu_ready(&gnu.text_grid()) || !help_menu_ready(&neo.text_grid()) {
        dump_pair_grids(
            "f10_can_navigate_to_help_menu_and_open_about_emacs/help",
            &gnu,
            &neo,
        );
    }
    assert!(
        help_menu_ready(&gnu.text_grid()),
        "GNU should navigate from File to Help with Right"
    );
    assert!(
        help_menu_ready(&neo.text_grid()),
        "Neomacs should navigate from File to Help with Right"
    );

    for _ in 0..20 {
        send_both(&mut gnu, &mut neo, "DOWN");
    }
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both(&mut gnu, &mut neo, "RET");

    let about_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*About GNU Emacs*"))
            || grid.iter().any(|row| row.contains("GNU Emacs"))
                && grid.iter().any(|row| row.contains("Copyright"))
    };
    gnu.read_until(Duration::from_secs(8), about_ready);
    neo.read_until(Duration::from_secs(12), about_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !about_ready(&gnu.text_grid()) || !about_ready(&neo.text_grid()) {
        dump_pair_grids(
            "f10_can_navigate_to_help_menu_and_open_about_emacs/about",
            &gnu,
            &neo,
        );
    }
    assert!(
        about_ready(&gnu.text_grid()),
        "GNU should open About Emacs from the TTY menu"
    );
    assert!(
        about_ready(&neo.text_grid()),
        "Neomacs should open About Emacs from the TTY menu"
    );
    assert_pair_exact_display(
        "f10_can_navigate_to_help_menu_and_open_about_emacs",
        &gnu,
        &neo,
    );
}
