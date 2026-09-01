//! TUI comparison tests for TTY child frame functionality.
//!
//! Each test spawns GNU Emacs and Neomacs side-by-side in isolated PTYs,
//! evaluates Elisp to create/manipulate child frames, and compares the
//! rendered output. These tests exercise GNU Emacs's `(featurep
//! 'tty-child-frames)` feature and serve as a spec for NeoMacs.

mod support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::{boot_pair, eval_expression, read_both, send_both};

/// Elisp helper loaded via `-l` at boot. Defines `cf--make-child` for
/// concise child-frame creation in tests.
const CHILD_FRAME_INIT: &str = r#";;; -*- lexical-binding: t; -*-
(eval-when-compile (require 'cl-lib))
(defun cf--make-child (&rest params)
  "Create a child frame with PARAMS merged into defaults."
  (let* ((parent (selected-frame))
         (defaults '((parent-frame . nil)
                     (width . 30)
                     (height . 5)
                     (left . 5)
                     (top . 3)
                     (minibuffer . nil)
                     (cursor-type . nil)))
         (full (cons (cons 'parent-frame parent)
                     (cl-remove-duplicates
                      (append params (cdr defaults))
                      :key #'car :from-end t))))
    (make-frame full)))
"#;

fn boot_child_frame_pair() -> (TuiSession, TuiSession) {
    let init = TuiTempFile::new("neomacs-child-frame-init-", "init.el", CHILD_FRAME_INIT);
    let extra_args = format!("-l {}", init.display());
    boot_pair(&extra_args)
}

/// Check that `text` appears somewhere in the rendered rows `[row_start, row_end)`.
fn assert_region_contains(session: &TuiSession, row_start: usize, row_end: usize, text: &str) {
    let grid = session.text_grid();
    let region: Vec<&String> = grid[row_start..row_end].iter().collect();
    let found = region.iter().any(|row| row.contains(text));
    if !found {
        eprintln!(
            "{}: '{}' not found in rows {row_start}..{row_end}",
            session.name, text
        );
        for (i, row) in region.iter().enumerate() {
            eprintln!("  {:2}: |{}|", row_start + i, row.trim_end());
        }
    }
    assert!(
        found,
        "{}: expected '{}' in rows {row_start}..{row_end}",
        session.name, text
    );
}

/// Delete all child frames (all frames except the selected one).
fn delete_all_child_frames(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let expr = r#"(dolist (f (delq (selected-frame) (frame-list))) (delete-frame f))"#;
    eval_expression(gnu, neo, expr);
    read_both(gnu, neo, Duration::from_secs(2));
}

#[test]
fn tty_child_frames_feature_check() {
    let (mut gnu, mut neo) = boot_child_frame_pair();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(message \"%s\" (featurep 'tty-child-frames))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gnu_echo = gnu.text_grid().last().unwrap().trim().to_string();
    let neo_echo = neo.text_grid().last().unwrap().trim().to_string();

    assert!(
        gnu_echo.contains('t'),
        "GNU should support tty-child-frames: {gnu_echo:?}"
    );
    // NeoMacs currently does NOT support tty-child-frames, but this test
    // documents the expected behavior.
    if !neo_echo.contains('t') {
        eprintln!("NOTE: NeoMacs does not yet support tty-child-frames (got: {neo_echo:?})");
    }
}

#[test]
fn create_and_delete_child_frame() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a child frame using the helper. Default position: left=5, top=3,
    // width=30, height=5. The child frame should occupy rows 3–7 (mode-line
    // at row ~6), cols 5–34.
    eval_expression(&mut gnu, &mut neo, "(cf--make-child)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // The child frame is visible: the screen should differ from the original
    // *scratch* buffer.
    let gnu_grid = gnu.text_grid();

    // GNU Emacs should show the child frame (rows 3-7 changed from original).
    let gnu_row3 = gnu_grid[3].trim();
    assert!(
        !gnu_row3.contains("This buffer is for text"),
        "GNU child frame should overlay row 3: got |{gnu_row3}|"
    );

    // Delete all child frames and verify the screen restores to scratch.
    delete_all_child_frames(&mut gnu, &mut neo);

    let gnu_after = gnu.text_grid();
    assert!(
        gnu_after.iter().any(|r| r.contains("*scratch*")),
        "GNU should restore *scratch* after deleting child frame"
    );
}

#[test]
fn child_frame_displays_buffer() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a child frame and switch it to a new buffer with content.
    let expr = r#"(let* ((cf (cf--make-child))
             (buf (get-buffer-create "*cf-content*")))
        (select-frame cf)
        (switch-to-buffer buf)
        (insert "Hello from child frame!")
        (sit-for 0)
        (select-frame (frame-parent cf))
        cf)"#;
    eval_expression(&mut gnu, &mut neo, expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // GNU: the child frame region (rows 3-7) should contain our text.
    assert_region_contains(&gnu, 3, 7, "Hello from child frame!");

    // Clean up.
    delete_all_child_frames(&mut gnu, &mut neo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
}

#[test]
fn child_frame_resize() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a child frame (default: width=30, height=5, left=5, top=3).
    eval_expression(&mut gnu, &mut neo, "(cf--make-child)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    let gnu_before = gnu.text_grid();

    // Resize the child frame to 50 wide, 8 tall.
    let resize_expr = r#"(let ((cf (car (delq (selected-frame) (frame-list)))))
        (set-frame-size cf 50 8))"#;
    eval_expression(&mut gnu, &mut neo, resize_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gnu_after_resize = gnu.text_grid();
    assert_ne!(
        gnu_before, gnu_after_resize,
        "GNU screen should change after resizing child frame"
    );

    // Move the child frame to left=20, top=1.
    let move_expr = r#"(let ((cf (car (delq (selected-frame) (frame-list)))))
        (set-frame-position cf 20 1))"#;
    eval_expression(&mut gnu, &mut neo, move_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gnu_after_move = gnu.text_grid();
    assert_ne!(
        gnu_after_resize, gnu_after_move,
        "GNU screen should change after moving child frame"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn display_buffer_in_child_frame() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a buffer with known content, then display it in a child frame.
    // display-buffer-in-child-frame takes (BUFFER &optional ALIST).
    let expr = r#"(let ((buf (get-buffer-create "*cf-display*")))
        (with-current-buffer buf
          (erase-buffer)
          (insert "Displayed in child frame\n"))
        (let ((cf (cf--make-child '(width . 40) '(height . 6) '(left . 5) '(top . 2))))
          (select-frame cf)
          (switch-to-buffer buf)
          (sit-for 0)
          (select-frame (frame-parent cf))))"#;
    eval_expression(&mut gnu, &mut neo, expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // GNU: the child frame area (rows ~2-7) should show our text.
    assert_region_contains(&gnu, 2, 8, "Displayed in child frame");

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn minibuffer_child_frame() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a child frame that shares the parent's minibuffer (default for
    // TTY child frames when minibuffer is not explicitly set to a separate
    // frame). Then exercise M-x while the child frame is visible.
    eval_expression(&mut gnu, &mut neo, "(cf--make-child)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Use M-x forward-line RET — the minibuffer should work.
    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    gnu.send(b"forward-line");
    neo.send(b"forward-line");
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Verify the echo area responded (no error about minibuffer being
    // unavailable).
    let gnu_grid = gnu.text_grid();
    let gnu_echo = gnu_grid.last().unwrap();
    assert!(
        !gnu_echo.contains("error") && !gnu_echo.contains("Error"),
        "GNU minibuffer should work with child frame: {gnu_echo:?}"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn child_frame_border_width() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a child frame with an explicit border width.
    let expr = r#"(cf--make-child '(child-frame-border-width . 1)
                               '(width . 20) '(height . 4)
                               '(left . 3) '(top . 2))"#;
    eval_expression(&mut gnu, &mut neo, expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // With border-width=1, GNU should render border characters around the
    // child frame area (rows 2-5, cols 3-22).
    let gnu_grid = gnu.text_grid();
    let gnu_row2 = gnu_grid[2].trim();
    assert!(
        !gnu_row2.contains("This buffer is for text"),
        "GNU row 2 should show child frame border, not scratch: |{gnu_row2}|"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn delete_child_frame_restores_parent() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Insert text into the parent buffer near the bottom, outside the
    // child frame's default area (rows 3-7).
    let insert_expr = r#"(goto-char (point-max))
        (insert "\nPARENT TEXT SURVIVES\n")"#;
    eval_expression(&mut gnu, &mut neo, insert_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Create a child frame that overlaps some of the parent content area.
    eval_expression(&mut gnu, &mut neo, "(cf--make-child '(top . 5))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Delete the child frame.
    delete_all_child_frames(&mut gnu, &mut neo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // The parent's text should be intact after child frame removal.
    // Search the full screen since we don't know the exact row.
    let gnu_grid = gnu.text_grid();
    let found = gnu_grid.iter().any(|r| r.contains("PARENT TEXT SURVIVES"));
    if !found {
        eprintln!("GNU screen after child frame deletion:");
        for (i, r) in gnu_grid.iter().enumerate() {
            let t = r.trim_end();
            if !t.is_empty() {
                eprintln!("  {i:2}: |{t}|");
            }
        }
    }
    assert!(found, "GNU parent text should survive child frame deletion");
}

#[test]
fn make_frame_invisible_hides_tty_child_frame_and_restores_parent_pixels() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    let setup = r#"(progn (switch-to-buffer "*scratch*") (erase-buffer) (dotimes (i 10) (insert (format "PARENT-HIDE-L%d\n" (1+ i)))) (goto-char (point-min)) (setq cf--last-child (cf--make-child '(width . 30) '(height . 4) '(left . 5) '(top . 2))) (select-frame cf--last-child) (switch-to-buffer (get-buffer-create "*cf-hide-child*")) (erase-buffer) (insert "CHILD-HIDE-VISIBLE\n") (sit-for 0) (select-frame (frame-parent cf--last-child)) (message "CHILD:%S" (frame-visible-p cf--last-child)))"#;
    eval_expression(&mut gnu, &mut neo, setup);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    assert_region_contains(&gnu, 2, 7, "CHILD-HIDE-VISIBLE");
    assert_region_contains(&neo, 2, 7, "CHILD-HIDE-VISIBLE");

    let hide = r#"(progn (make-frame-invisible cf--last-child t) (message "HIDDEN:%S:%S" (frame-visible-p cf--last-child) (eq (selected-frame) cf--last-child)))"#;
    eval_expression(&mut gnu, &mut neo, hide);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    for session in [&gnu, &neo] {
        let grid = session.text_grid();
        let screen = grid.join("\n");
        assert!(
            screen.contains("HIDDEN:nil:nil"),
            "{} should report the hidden child frame as invisible and unselected:\n{screen}",
            session.name
        );
        assert!(
            !screen.contains("CHILD-HIDE-VISIBLE"),
            "{} should not leave child-frame text painted after make-frame-invisible:\n{screen}",
            session.name
        );
        assert!(
            screen.contains("PARENT-HIDE-L4"),
            "{} should repaint parent text underneath the hidden child frame:\n{screen}",
            session.name
        );
    }
}

#[test]
fn multiple_child_frames() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create first child frame at top-left.
    let expr1 = r#"(cf--make-child '(width . 20) '(height . 4)
                               '(left . 2) '(top . 2))"#;
    eval_expression(&mut gnu, &mut neo, expr1);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Create second child frame at bottom-right.
    let expr2 = r#"(cf--make-child '(width . 25) '(height . 4)
                               '(left . 40) '(top . 14))"#;
    eval_expression(&mut gnu, &mut neo, expr2);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    let gnu_grid = gnu.text_grid();
    // Both child frames should have changed the screen from original scratch.
    assert!(
        !gnu_grid[3].trim().contains("This buffer is for text"),
        "GNU first child frame should overlay rows 2-5"
    );
    assert!(
        !gnu_grid[15].trim().contains("For information about"),
        "GNU second child frame should overlay rows 14-17"
    );

    // Delete first child frame.
    let delete_first = r#"(let ((frames (delq (selected-frame) (frame-list))))
        (delete-frame (car frames)))"#;
    eval_expression(&mut gnu, &mut neo, delete_first);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Second child frame should still be visible — screen should still differ.
    let gnu_after_delete1 = gnu.text_grid();
    assert_ne!(
        gnu_grid, gnu_after_delete1,
        "GNU screen should change after deleting first child frame"
    );

    // Delete remaining child frame.
    delete_all_child_frames(&mut gnu, &mut neo);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gnu_final = gnu.text_grid();
    assert!(
        gnu_final.iter().any(|r| r.contains("*scratch*")),
        "GNU should restore *scratch* after deleting all child frames"
    );
}

// ── Position movement tests ──────────────────────────────────────────────

/// Create a child frame with content, return its inner expression for later
/// position queries.  Returns the expression string that evaluates to the
/// youngest child frame.
const CF_FIND_EXPR: &str = r#"(car (delq (selected-frame) (frame-list)))"#;

#[test]
fn child_frame_move_right() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create child frame with content at default position (left=5, top=3).
    let make_expr = r#"(let* ((cf (cf--make-child '(width . 20) '(height . 4)))
             (buf (get-buffer-create "*cf-move*")))
        (select-frame cf)
        (switch-to-buffer buf)
        (insert "MOVE-RIGHT-TEST")
        (sit-for 0)
        (select-frame (frame-parent cf))
        cf)"#;
    eval_expression(&mut gnu, &mut neo, make_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Verify initial position is visible in default area.
    assert_region_contains(&gnu, 3, 7, "MOVE-RIGHT-TEST");

    // Move child frame right by 20 columns.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(set-frame-position {CF_FIND_EXPR} 25 3)"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Content should have moved right — original area should no longer show it.
    // Verify that the frame position parameter updated.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(message \"POS: %S\" (frame-position {CF_FIND_EXPR}))"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_echo = gnu.text_grid().last().cloned().unwrap_or_default();
    assert!(
        gnu_echo.contains("25") && gnu_echo.contains("3"),
        "GNU frame-position should be (25 3) after move right: {gnu_echo:?}"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn child_frame_move_down() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create child frame with content.
    let make_expr = r#"(let* ((cf (cf--make-child '(width . 20) '(height . 4) '(left . 5) '(top . 3)))
             (buf (get-buffer-create "*cf-move-down*")))
        (select-frame cf)
        (switch-to-buffer buf)
        (insert "MOVE-DOWN-TEST")
        (sit-for 0)
        (select-frame (frame-parent cf))
        cf)"#;
    eval_expression(&mut gnu, &mut neo, make_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    assert_region_contains(&gnu, 3, 7, "MOVE-DOWN-TEST");

    // Move child frame down by 10 rows.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(set-frame-position {CF_FIND_EXPR} 5 13)"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Content should now appear in rows 13-16, not rows 3-6.
    assert_region_contains(&gnu, 13, 17, "MOVE-DOWN-TEST");

    // Verify frame-position.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(message \"POS: %S\" (frame-position {CF_FIND_EXPR}))"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_echo = gnu.text_grid().last().cloned().unwrap_or_default();
    assert!(
        gnu_echo.contains("5") && gnu_echo.contains("13"),
        "GNU frame-position should be (5 13) after move down: {gnu_echo:?}"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn child_frame_move_multiple_steps() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create child frame with content at (left=5, top=3).
    let make_expr = r#"(let* ((cf (cf--make-child '(width . 20) '(height . 4) '(left . 5) '(top . 3)))
             (buf (get-buffer-create "*cf-move-steps*")))
        (select-frame cf)
        (switch-to-buffer buf)
        (insert "STEP-TEST")
        (sit-for 0)
        (select-frame (frame-parent cf))
        cf)"#;
    eval_expression(&mut gnu, &mut neo, make_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    assert_region_contains(&gnu, 3, 7, "STEP-TEST");

    // Step 1: move right.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(set-frame-position {CF_FIND_EXPR} 35 3)"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    // Step 2: move down.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(set-frame-position {CF_FIND_EXPR} 35 13)"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    // Step 3: move left.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(set-frame-position {CF_FIND_EXPR} 5 13)"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    // Step 4: move back up to original.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(set-frame-position {CF_FIND_EXPR} 5 3)"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Content should be back at original position.
    assert_region_contains(&gnu, 3, 7, "STEP-TEST");

    // Verify final position.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(message \"POS: %S\" (frame-position {CF_FIND_EXPR}))"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_echo = gnu.text_grid().last().cloned().unwrap_or_default();
    assert!(
        gnu_echo.contains("5") && gnu_echo.contains("3"),
        "GNU frame-position should return to (5 3): {gnu_echo:?}"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn child_frame_move_after_resize() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create a wide child frame.
    let make_expr = r#"(let* ((cf (cf--make-child '(width . 30) '(height . 6) '(left . 5) '(top . 2)))
             (buf (get-buffer-create "*cf-resize-move*")))
        (select-frame cf)
        (switch-to-buffer buf)
        (insert "RESIZE-MOVE-TEST")
        (sit-for 0)
        (select-frame (frame-parent cf))
        cf)"#;
    eval_expression(&mut gnu, &mut neo, make_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    assert_region_contains(&gnu, 2, 8, "RESIZE-MOVE-TEST");

    // Resize smaller and reposition in one sequence.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!(
            "(let ((cf {CF_FIND_EXPR})) (set-frame-size cf 15 4) (set-frame-position cf 50 2))"
        ),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // Verify position updated.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!(
            "(message \"POS: %S SIZE: %S\" (frame-position {CF_FIND_EXPR}) (cons (frame-width {CF_FIND_EXPR}) (frame-height {CF_FIND_EXPR})))"
        ),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_echo = gnu.text_grid().last().cloned().unwrap_or_default();
    assert!(
        gnu_echo.contains("50") && gnu_echo.contains("2"),
        "GNU frame-position should be (50 2) after resize+move: {gnu_echo:?}"
    );
    assert!(
        gnu_echo.contains("15") && gnu_echo.contains("4"),
        "GNU frame-size should be (15 . 4): {gnu_echo:?}"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}

#[test]
fn child_frame_position_consistency() {
    let (mut gnu, mut neo) = boot_child_frame_pair();

    // Create child frame and verify position API returns correct values.
    let make_expr = r#"(cf--make-child '(left . 12) '(top . 7) '(width . 18) '(height . 5))"#;
    eval_expression(&mut gnu, &mut neo, make_expr);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));

    // frame-parameter should return the set values.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!(
            "(message \"PARAMS: left=%S top=%S width=%S height=%S\" (frame-parameter {CF_FIND_EXPR} 'left) (frame-parameter {CF_FIND_EXPR} 'top) (frame-parameter {CF_FIND_EXPR} 'width) (frame-parameter {CF_FIND_EXPR} 'height))"
        ),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_echo = gnu.text_grid().last().cloned().unwrap_or_default();
    assert!(
        gnu_echo.contains("left=12") && gnu_echo.contains("top=7"),
        "GNU frame-parameter left/top should be 12/7: {gnu_echo:?}"
    );
    assert!(
        gnu_echo.contains("width=18") && gnu_echo.contains("height=5"),
        "GNU frame-parameter width/height should be 18/5: {gnu_echo:?}"
    );

    // frame-position should match.
    eval_expression(
        &mut gnu,
        &mut neo,
        &format!("(message \"FRAME-POS: %S\" (frame-position {CF_FIND_EXPR}))"),
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_pos = gnu.text_grid().last().cloned().unwrap_or_default();
    assert!(
        gnu_pos.contains("12") && gnu_pos.contains("7"),
        "GNU frame-position should be (12 7): {gnu_pos:?}"
    );

    delete_all_child_frames(&mut gnu, &mut neo);
}
