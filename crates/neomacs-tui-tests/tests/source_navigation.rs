//! TUI comparisons for common source-navigation workflows.
//!
//! GNU behavior here is driven by `lisp/progmodes/xref.el`,
//! `lisp/progmodes/elisp-mode.el`, and `lisp/emacs-lisp/find-func.el`.

mod support;

use std::time::Duration;
use support::*;

fn search_forward_both(
    gnu: &mut neomacs_tui_tests::TuiSession,
    neo: &mut neomacs_tui_tests::TuiSession,
    needle: &str,
) {
    send_both(gnu, neo, "C-s");
    gnu.send(needle.as_bytes());
    neo.send(needle.as_bytes());
    send_both(gnu, neo, "RET");
    read_both(gnu, neo, Duration::from_secs(1));
}

#[test]
fn xref_find_definitions_and_go_back_from_elisp_symbol() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "xref-navigation-probe.el";
    let initial = "(defun neo-xref-caller ()\n  (comment-dwim nil))\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    search_forward_both(&mut gnu, &mut neo, "comment-dwim");
    send_both(&mut gnu, &mut neo, "M-.");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("newcomment.el"))
            && grid.iter().any(|row| row.contains("(defun comment-dwim"))
    });

    send_both(&mut gnu, &mut neo, "M-,");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains(name))
            && grid.iter().any(|row| row.contains("(comment-dwim nil)"))
    });
}

#[test]
fn find_function_via_mx_opens_lisp_definition() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "find-function");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.last()
            .is_some_and(|row| row.contains("Find function:"))
    });
    gnu.send(b"comment-dwim");
    neo.send(b"comment-dwim");
    send_both(&mut gnu, &mut neo, "RET");

    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("newcomment.el"))
            && grid.iter().any(|row| row.contains("(defun comment-dwim"))
    });
}
