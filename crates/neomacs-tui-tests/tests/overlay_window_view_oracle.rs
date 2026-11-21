#![cfg(unix)]
//! GNU oracle coverage for the window-range overlay view retry (#702).
//!
//! An invisible overlay hides more than four screenfuls of text. The layout
//! walk starts at the buffer top, so its window-range overlay view is
//! exhausted before the visible tail; redisplay must retry with the
//! whole-buffer view and render a face + before-string overlay anchored past
//! the collapsed region. Modifying that overlay must then update the display
//! in the same run. Both steps compare the rendered grid against the attested
//! GNU 31.1 oracle: the engine unit rows only prove Neomacs against itself.

use crate::support;

use std::time::Duration;
use support::{
    assert_pair_exact_display, boot_pair, eval_expression, wait_for_both, write_home_file,
};

const ORACLE_FILE: &str = "overlay-window-view-oracle.el";

const ORACLE_ELISP: &str = r#";;; -*- lexical-binding: t; -*-

(defvar neo-overlay-window-trailing nil)

(defun neo-overlay-window-oracle-setup ()
  (delete-other-windows)
  (let ((text (mapconcat (lambda (line) (format "line %04d abcdefghij" line))
                         (number-sequence 0 3999)
                         "\n")))
    (switch-to-buffer (get-buffer-create " *overlay-window-oracle*"))
    (widen)
    (erase-buffer)
    (insert text "\n")
    (goto-char (point-min))
    (setq-local mode-line-format nil
                header-line-format nil
                tab-line-format nil
                truncate-lines nil
                word-wrap nil
                buffer-invisibility-spec t)
    (set-window-buffer (selected-window) (current-buffer))
    (set-window-start (selected-window) (point-min))
    (set-window-hscroll (selected-window) 0)
    (redisplay t)
    ;; Hide more than four screenfuls. The walk starts at the buffer top, so
    ;; the window-range overlay view ends inside this region and the trailing
    ;; overlay below can only be rendered by the whole-buffer retry.
    (let ((hidden (make-overlay 10 60000)))
      (overlay-put hidden 'invisible t))
    (setq neo-overlay-window-trailing (make-overlay 60400 60401))
    (overlay-put neo-overlay-window-trailing 'face 'bold)
    (overlay-put neo-overlay-window-trailing 'before-string "TRAILING")
    (goto-char (point-min))
    (set-window-start (selected-window) (point-min))
    (redisplay t)))

(neo-overlay-window-oracle-setup)
"#;

#[test]
fn invisible_collapse_retry_reaches_a_trailing_overlay_and_its_update() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, ORACLE_FILE, ORACLE_ELISP);
    write_home_file(&neo, ORACLE_FILE, ORACLE_ELISP);
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(load "~/overlay-window-view-oracle.el" nil t)"#,
    );

    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.contains("TRAILING"))
    });
    assert!(
        gnu.text_grid().iter().any(|row| row.contains("TRAILING")),
        "GNU oracle did not render the collapsed view; grid:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        neo.text_grid().iter().any(|row| row.contains("TRAILING")),
        "Neomacs dropped a trailing overlay past the exhausted view; grid:\n{}",
        neo.text_grid().join("\n")
    );
    assert_pair_exact_display("overlay_window_view_collapse", &gnu, &neo);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(overlay-put neo-overlay-window-trailing 'before-string \"UPDATED\")",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.contains("UPDATED"))
    });
    assert!(
        neo.text_grid().iter().any(|row| row.contains("UPDATED")),
        "the changed trailing overlay did not reach the display; grid:\n{}",
        neo.text_grid().join("\n")
    );
    assert!(
        !neo.text_grid().iter().any(|row| row.contains("TRAILING")),
        "the changed trailing overlay left a stale before-string; grid:\n{}",
        neo.text_grid().join("\n")
    );
    assert_pair_exact_display("overlay_window_view_collapse_update", &gnu, &neo);
}
