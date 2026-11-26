#![cfg(unix)]
//! Issue #702 Slice 1: truncated long lines in a window with display margins
//! must place the `$` truncation glyph at the last column of `TEXT_AREA`
//! (flush against the right margin), matching GNU Emacs `src/xdisp.c:26608-26650`.
//!
//! Before the fix, `DisplayRowColumnCount::from_row` summed `LeftMargin + Text +
//! RightMargin` against `TEXT_AREA` `target_col`, popping `right_margin_width`
//! real text columns and leaving a blank hole before the right margin.

use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};
use std::{path::PathBuf, time::Duration};

fn render_truncated_margin_rows(
    binary: PathBuf,
    left_margin: usize,
    right_margin: usize,
) -> Vec<String> {
    let directory = TuiTempDirectory::new("issue-702-margin-trunc-");
    let init = directory.path().join("init.el");
    let elisp = format!(
        r#"(progn
  (menu-bar-mode -1)
  (switch-to-buffer (get-buffer-create "*trunc-margin*"))
  (delete-other-windows)
  (erase-buffer)
  (setq-local truncate-lines t)
  (setq-local word-wrap nil)
  (setq-local left-margin-width {left_margin})
  (setq-local right-margin-width {right_margin})
  (set-window-margins (selected-window) {left_margin} {right_margin})
  (let* ((p0 (point))
         (line0 (concat "L0:" (apply #'concat (mapcar (lambda (i) (format "%02d|" i)) (number-sequence 0 39))))))
    (insert line0 "\n")
    (let ((ov (make-overlay p0 (point))))
      (overlay-put ov 'before-string
                   (concat
                    (when (> {left_margin} 0)
                      (propertize "l" 'display (list '(margin left-margin) (make-string {left_margin} ?L))))
                    (propertize "r" 'display (list '(margin right-margin) (concat "AuthorName" (make-string (max 0 (- {right_margin} 10)) ?R))))))))
  (let* ((p1 (point))
         (text-cols (- 80 {left_margin} {right_margin}))
         ;; Put a double-width CJK char spanning the last two text columns.
         (prefix (make-string (max 0 (- text-cols 2)) ?a)))
    (insert prefix "日TAIL-BEYOND-EDGE\n")
    (let ((ov (make-overlay p1 (point))))
      (overlay-put ov 'before-string
                   (concat
                    (when (> {left_margin} 0)
                      (propertize "l" 'display (list '(margin left-margin) (make-string {left_margin} ?L))))
                    (propertize "r" 'display (list '(margin right-margin) (concat "WideMargin" (make-string (max 0 (- {right_margin} 10)) ?W))))))))
  (goto-char (point-min))
  (redisplay t)
  (message "ISSUE702-READY"))
"#
    );
    std::fs::write(&init, elisp).expect("write init.el");

    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--load")
        .arg(&init)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "ISSUE702-READY",
        TuiTerminalConfig::new("xterm-256color", 24, 80),
    );
    session.read_until_screen(Duration::from_secs(15), |_lines, screen| {
        screen.contents().contains("ISSUE702-READY")
    });
    let rows = vec![session.row_text(0), session.row_text(1)];
    session.send_key("C-x");
    session.send_key("C-c");
    let _ = session.run_to_completion(Duration::from_secs(5));
    assert_ne!(
        rows[0].trim(),
        "",
        "row 0 must be non-empty after redisplay"
    );
    rows
}

#[test]
fn truncated_long_line_with_right_margin_places_dollar_flush_like_gnu() {
    let gnu = render_truncated_margin_rows(neomacs_tui_tests::oracle::gnu_oracle_program(), 0, 20);
    let neo = render_truncated_margin_rows(neomacs_tui_tests::neomacs_binary(), 0, 20);

    // Precondition: GNU puts `$` at col 59 flush against `"AuthorNameRRRRRRRRRR"`.
    assert!(
        gnu[0].contains("$AuthorNameRRRRRRRRRR"),
        "expected GNU row 0 to draw `$` flush against right margin, got: {:?}",
        gnu[0]
    );
    assert!(
        gnu[1].contains("$$WideMarginWWWWWWWWWW"),
        "expected GNU row 1 to overwrite cut wide char with `$$` flush against right margin, got: {:?}",
        gnu[1]
    );
    assert_eq!(
        neo[0], gnu[0],
        "neomacs truncated row with right margin must match GNU Emacs 31.1 verbatim"
    );
    assert_eq!(
        neo[1], gnu[1],
        "neomacs truncated wide-char row with right margin must match GNU Emacs 31.1 verbatim"
    );
}

#[test]
fn truncated_long_line_with_left_and_right_margins_matches_gnu() {
    let gnu = render_truncated_margin_rows(neomacs_tui_tests::oracle::gnu_oracle_program(), 6, 16);
    let neo = render_truncated_margin_rows(neomacs_tui_tests::neomacs_binary(), 6, 16);

    assert!(
        gnu[0].starts_with("LLLLLL") && gnu[0].contains("$AuthorNameRRRRRR"),
        "expected GNU row 0 with left+right margins to place `$` flush against right margin, got: {:?}",
        gnu[0]
    );
    assert_eq!(
        neo[0], gnu[0],
        "neomacs truncated row with both margins must match GNU Emacs 31.1 verbatim"
    );
    assert_eq!(
        neo[1], gnu[1],
        "neomacs truncated wide-char row with both margins must match GNU Emacs 31.1 verbatim"
    );
}
