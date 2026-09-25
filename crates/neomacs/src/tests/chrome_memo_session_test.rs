//! P3.5 E2 (`NEOMACS_CHROME_MEMO`) in process: typing in the middle of an
//! emacs-lisp-mode buffer with the default mode line, through the real
//! bootstrap evaluator and TTY renderer.
//!
//! The gate evaluates the mode line exactly when GNU would. Typing at the
//! end of a line keeps it (GNU's optimization 1), moving to another line
//! changes `L%l`, so the frames that evaluate AND render the same are the
//! ones something forced: `force-mode-line-update`, which minor modes call
//! from their hooks. The memo installs the previous row on those. Every
//! frame is compared with a fresh runtime's full layout.

use super::edit_sync_session_test::{SOURCE, Session};

#[test]
fn typing_renders_an_unchanged_default_mode_line_from_the_memo() {
    let mut s = Session::with_knobs(&[("NEOMACS_CHROME_MEMO", "on")]);
    s.step("load source", SOURCE);
    s.frame("idle");
    let mut report = Vec::new();
    let mut hits = 0;
    let mut evaluated = 0;
    for (label, lisp) in [
        ("type", "(insert \"x\")"),
        (
            "type and force",
            "(progn (insert \"y\") (force-mode-line-update))",
        ),
        ("force", "(force-mode-line-update)"),
        (
            "move within the line",
            "(progn (backward-char 4) (force-mode-line-update))",
        ),
        (
            "type mid-line",
            "(progn (insert \"z\") (force-mode-line-update))",
        ),
        (
            "delete",
            "(progn (delete-char -1) (force-mode-line-update))",
        ),
        ("move down", "(forward-line 1)"),
        ("force below", "(force-mode-line-update)"),
        ("newline", "(newline)"),
        (
            "type on the new line",
            "(progn (insert \"w\") (force-mode-line-update))",
        ),
    ] {
        let stats = s.step(label, lisp);
        report.push(format!(
            "{label:<22} relaid_chrome={} reused_chrome={} memo={}",
            stats.relaid_chrome_rows, stats.reused_chrome_rows, stats.chrome_memo_hits
        ));
        hits += stats.chrome_memo_hits;
        evaluated += usize::from(stats.relaid_chrome_rows > 0);
    }
    tracing::info!("chrome memo session:\n{}", report.join("\n"));
    assert!(
        evaluated > 0,
        "no frame evaluated the mode line\n{}",
        report.join("\n")
    );
    assert!(
        hits > 0,
        "the default mode line never hit the memo\n{}",
        report.join("\n")
    );
}
