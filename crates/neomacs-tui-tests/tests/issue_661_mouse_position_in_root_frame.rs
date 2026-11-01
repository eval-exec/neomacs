//! Issue #660: `mouse-position-in-root-frame` must answer on a tty.
//!
//! GNU `Fmouse_position_in_root_frame` (`src/frame.c:7010-7039`) adds the
//! mouse frame's and each ancestor's `left_pos`/`top_pos` to the value of
//! `mouse-position` (after `mouse-position-function`).  A tty frame's offsets
//! are zero, and `mouse-drag-line` reads the pointer through this primitive
//! on ttys (`lisp/mouse.el:862-871`).  Neomacs previously returned nil, so
//! dragging a mode line or divider never resized a window.
//!
//! The synthetic `mouse-position-function` gives both editors a known frame
//! and coordinates without needing physical pointer input.

use neomacs_tui_tests::pair::{boot_pair, eval_expression_one};

#[test]
fn mouse_position_in_root_frame_answers_on_a_tty() {
    let (mut gnu, mut neo) = boot_pair("");
    let dir = tempfile::tempdir().expect("temporary directory");
    let gnu_out = dir.path().join("issue-661-gnu.out");
    let neo_out = dir.path().join("issue-661-neomacs.out");
    let form = |out: &std::path::Path| {
        format!(
            r##"(let* ((known
                 (let ((mouse-position-function
                        (lambda (_pos)
                          (cons (selected-frame) (cons 19 45)))))
                   (mouse-position-in-root-frame)))
                (unknown
                 (let ((mouse-position-function
                        (lambda (_pos)
                          (cons (selected-frame) (cons nil nil)))))
                   (mouse-position-in-root-frame))))
             (with-temp-file {out:?}
               (prin1 (list known unknown) (current-buffer))))"##,
            out = out
        )
    };
    eval_expression_one(&mut gnu, &form(&gnu_out));
    eval_expression_one(&mut neo, &form(&neo_out));

    let gnu_result = std::fs::read_to_string(&gnu_out).expect("GNU mouse root result");
    let neo_result = std::fs::read_to_string(&neo_out).expect("Neomacs mouse root result");
    assert_eq!(
        gnu_result, "((19 . 45) (0 . 0))",
        "GNU must add the tty frame offsets and read nil coordinates as zero"
    );
    assert_eq!(
        neo_result,
        gnu_result,
        "Neomacs must answer mouse-position-in-root-frame on a tty; \
         GNU grid:\n{}\nNeomacs grid:\n{}",
        gnu.text_grid().join("\n"),
        neo.text_grid().join("\n")
    );
}
