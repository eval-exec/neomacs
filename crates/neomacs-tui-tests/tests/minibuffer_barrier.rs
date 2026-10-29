//! `eval_expression` must return only after the eval minibuffer exits.
//!
//! A form longer than the minibuffer scrolls its `Eval:` prompt row out of
//! view while the minibuffer is still open, so "the prompt text is gone" is
//! not by itself a usable exit signal.  The pair harness also requires the
//! menu bar's `Minibuf` item to drop and the form's tail to leave the last
//! row; this test keeps a form under the cursor long enough to scroll the
//! prompt away and then asserts the barrier held.

use crate::support::*;
use std::time::Duration;

#[test]
fn eval_expression_waits_out_a_scrolled_minibuffer() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut form = String::from("(progn\n");
    for index in 0..60 {
        form.push_str(&format!("  ;; filler line {index}\n"));
    }
    form.push_str("  (insert \"SCROLLED-MINIBUFFER-EXITED\")\n  (goto-char (point-min)))\n");
    eval_expression(&mut gnu, &mut neo, &form);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("SCROLLED-MINIBUFFER-EXITED")),
            "{label} returned from eval_expression before the form ran:\n{}",
            grid.join("\n")
        );
        assert!(
            !grid
                .iter()
                .any(|row| row.contains("File Edit Options Buffers") && row.contains("Minibuf")),
            "{label} returned from eval_expression while the minibuffer was still open:\n{}",
            grid.join("\n")
        );
    }
}

/// The barrier's menu-bar observable is GNU's, not an invention: the item
/// `minibuffer-local-map` adds is visible in both editors exactly while a
/// minibuffer is active.
#[test]
fn the_tty_menu_bar_shows_minibuf_only_while_a_minibuffer_is_active() {
    let (mut gnu, mut neo) = boot_pair("");
    let menu_row = |grid: &[String]| {
        grid.iter()
            .find(|row| row.contains("File Edit Options Buffers"))
            .cloned()
            .unwrap_or_default()
    };
    let shows_minibuf = |grid: &[String]| menu_row(grid).contains("Minibuf");
    let minibuf_gone = |grid: &[String]| {
        !grid
            .iter()
            .any(|row| row.contains("File Edit Options Buffers") && row.contains("Minibuf"))
    };
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            !shows_minibuf(&session.text_grid()),
            "{label} shows the Minibuf menu item at rest"
        );
    }
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            shows_minibuf(&session.text_grid()),
            "{label} does not show the Minibuf menu item inside M-:"
        );
    }
    send_both(&mut gnu, &mut neo, "C-g");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), minibuf_gone);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            minibuf_gone(&session.text_grid()),
            "{label} kept the Minibuf menu item after C-g"
        );
    }
}
