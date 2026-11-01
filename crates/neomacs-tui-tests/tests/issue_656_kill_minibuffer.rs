//! Issue #656: `kill-buffer` must refuse to kill the buffer displayed in the
//! minibuffer window (GNU `Fkill_buffer`, src/buffer.c:1998-2000).
//!
//! This form runs inside the active minibuffer (it is typed at the `M-:`
//! prompt), so it exercises the active-minibuffer target. The batch oracle
//! test covers the no-active-minibuffer case with the same contract.

use neomacs_tui_tests::pair::{boot_pair, eval_expression_one};

#[test]
fn kill_buffer_refuses_the_buffer_shown_in_the_active_minibuffer() {
    let (mut gnu, mut neo) = boot_pair("");
    let dir = tempfile::tempdir().expect("temporary directory");
    let gnu_out = dir.path().join("issue-656-gnu.out");
    let neo_out = dir.path().join("issue-656-neomacs.out");
    let form = |path: &std::path::Path| {
        // The minibuffer window is restored before returning so the active
        // minibuffer can exit normally.
        format!(
            r##"(let* ((mini (minibuffer-window))
       (original (window-buffer mini))
       (victim (generate-new-buffer "neb-tui-kill-mini-victim")))
  (unwind-protect
      (progn
        (set-window-buffer mini victim)
        (let ((result (list (kill-buffer victim)
                            (buffer-live-p victim)
                            (buffer-name (window-buffer mini)))))
          (set-window-buffer mini original)
          (with-temp-file {path:?}
            (prin1 result (current-buffer)))))
    (when (buffer-live-p victim)
      (set-window-buffer mini original)
      (kill-buffer victim))))"##,
            path = path
        )
    };
    eval_expression_one(&mut gnu, &form(&gnu_out));
    eval_expression_one(&mut neo, &form(&neo_out));

    let gnu_result = std::fs::read_to_string(&gnu_out).expect("GNU kill-buffer result");
    let neo_result = std::fs::read_to_string(&neo_out).expect("Neomacs kill-buffer result");
    assert_eq!(
        gnu_result, "(nil t \"neb-tui-kill-mini-victim\")",
        "GNU oracle must refuse to kill the minibuffer window's buffer"
    );
    assert_eq!(
        neo_result,
        gnu_result,
        "Neomacs must refuse exactly like GNU; GNU grid:\n{}\nNeomacs grid:\n{}",
        gnu.text_grid().join("\n"),
        neo.text_grid().join("\n")
    );
}
