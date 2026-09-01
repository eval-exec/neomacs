//! Regression coverage for issue #254: quitting `M-x` must dismiss its
//! `*Completions*` popup window.

mod support;

use neomacs_tui_tests::{StrictGridOptions, assert_grids_strict};
use std::time::Duration;

use support::{boot_pair, dump_pair_grids, read_both, scratch_ready, send_both, wait_for_both};

#[test]
fn keyboard_quit_dismisses_mx_completions_window() {
    assert_keyboard_quit_dismisses_mx_completions_window("", "issue #254 after keyboard-quit");
}

#[test]
fn keyboard_quit_exit_hook_dismisses_mx_completions_when_window_restoration_is_disabled() {
    assert_keyboard_quit_dismisses_mx_completions_window(
        "--eval=(set'read-minibuffer-restore-windows())",
        "issue #254 exit-hook fallback after keyboard-quit",
    );
}

fn assert_keyboard_quit_dismisses_mx_completions_window(extra_args: &str, label: &str) {
    let (mut gnu, mut neo) = boot_pair(extra_args);

    send_both(&mut gnu, &mut neo, "M-x");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.last().is_some_and(|row| row.contains("M-x"))
    });

    gnu.send(b"find-f");
    neo.send(b"find-f");
    send_both(&mut gnu, &mut neo, "TAB");
    let completions_visible =
        |grid: &[String]| grid.iter().any(|row| row.contains("*Completions*"));
    wait_for_both(
        &mut gnu,
        &mut neo,
        Duration::from_secs(8),
        completions_visible,
    );
    if !completions_visible(&gnu.text_grid()) || !completions_visible(&neo.text_grid()) {
        dump_pair_grids(&format!("{label}/after-tab"), &gnu, &neo);
    }
    assert!(
        completions_visible(&gnu.text_grid()),
        "GNU oracle must display the M-x completions window"
    );
    assert!(
        completions_visible(&neo.text_grid()),
        "Neomacs must display the M-x completions window before the quit"
    );

    send_both(&mut gnu, &mut neo, "C-g");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let gnu_still_visible = completions_visible(&gnu.text_grid());
    let neo_still_visible = completions_visible(&neo.text_grid());
    if gnu_still_visible || neo_still_visible {
        dump_pair_grids(&format!("{label}/after-keyboard-quit"), &gnu, &neo);
    }
    assert!(
        !gnu_still_visible,
        "GNU oracle must dismiss the M-x completions window on C-g"
    );
    assert!(
        !neo_still_visible,
        "Neomacs must dismiss the M-x completions window on C-g"
    );
    assert_grids_strict(
        label,
        gnu.screen(),
        neo.screen(),
        &StrictGridOptions::default(),
    );
}
