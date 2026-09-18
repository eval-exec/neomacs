#![cfg(unix)]
//! TUI comparisons for common source-navigation workflows.
//!
//! GNU behavior here is driven by `lisp/progmodes/xref.el`,
//! `lisp/progmodes/elisp-mode.el`, and `lisp/emacs-lisp/find-func.el`.

use crate::support;

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
    assert_pair_exact_display(
        "xref_find_definitions_and_go_back_from_elisp_symbol",
        &gnu,
        &neo,
    );
}

#[test]
fn find_function_via_mx_opens_lisp_definition() {
    let (mut gnu, mut neo) = boot_pair("");
    disable_vc_backends(&mut gnu, &mut neo);

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
    wait_for_both_mx_suggestion(&mut gnu, &mut neo, "find-function", Duration::from_secs(8));

    // The two executables necessarily open their own copy of the defining
    // file, and a `make install` oracle keeps its Lisp compressed: GNU
    // visits `newcomment.el.gz` while Neomacs visits the tree's
    // `newcomment.el`, and the buffer name reaches the mode line.  Declare
    // the two spellings as one logical resource; on a checkout-running GNU
    // the names already agree and the pair is inert.
    let buffer_name = |session: &mut neomacs_tui_tests::TuiSession| -> String {
        eval_expression_one(session, "(message \"BNXX%sXX\" (buffer-name))");
        session.read(Duration::from_millis(600));
        let (rows, _) = session.screen_size();
        let mut found = String::new();
        for r in (0..rows).rev() {
            let t = session.row_text(r);
            if let Some(i) = t.find("BNXX") {
                let tail: String = t[i + 4..].chars().take_while(|c| *c != 'X').collect();
                if !tail.is_empty() {
                    found = tail;
                    break;
                }
            }
        }
        found
    };
    let gnu_name = buffer_name(&mut gnu);
    let neo_name = buffer_name(&mut neo);
    assert!(
        !gnu_name.is_empty() && !neo_name.is_empty(),
        "buffer-name probe failed"
    );
    assert_pair_exact_display_with_path_pairs(
        "find_function_via_mx_opens_lisp_definition",
        &gnu,
        &neo,
        &[(gnu_name, neo_name)],
    );
}
