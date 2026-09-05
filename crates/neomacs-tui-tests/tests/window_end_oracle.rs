#![cfg(unix)]
//! GNU Emacs oracle coverage for synchronous `window-end` layout queries.
//!
//! The probes run inside real interactive TTY frames.  Batch mode cannot
//! exercise the same redisplay/query boundary, and a Rust-only layout test
//! cannot establish GNU compatibility.

use crate::support;

use neomacs_tui_tests::TuiSession;
use std::fs;
use std::time::{Duration, Instant};
use support::{
    assert_pair_exact_display, boot_pair, eval_expression, read_both, send_both, wait_for_both,
    write_home_file,
};

const ORACLE_FILE: &str = "window-end-oracle.el";
const RESULT_FILE: &str = "window-end-oracle.out";
const DONE_FILE: &str = "window-end-oracle.done";

const ORACLE_ELISP: &str = r#";;; -*- lexical-binding: t; -*-

(defvar neo-window-end-oracle-results nil)

(defun neo-window-end-oracle--flush ()
  (with-temp-file
      (expand-file-name "window-end-oracle.out" (getenv "HOME"))
    (dolist (result (reverse neo-window-end-oracle-results))
      (prin1 result (current-buffer))
      (terpri (current-buffer)))))

(defun neo-window-end-oracle--text (line count)
  (let ((text ""))
    (dotimes (_ count text)
      (setq text (concat text line "\n")))))

(defun neo-window-end-oracle--setup (text truncate)
  (delete-other-windows)
  (let ((buffer (get-buffer-create " *window-end-oracle*")))
    (switch-to-buffer buffer)
    (widen)
    (erase-buffer)
    (insert text)
    (goto-char (point-min))
    (setq-local mode-line-format nil)
    (setq-local header-line-format nil)
    (setq-local tab-line-format nil)
    (setq-local truncate-lines truncate)
    (setq-local word-wrap nil)
    (setq-local buffer-invisibility-spec t)
    (set-window-buffer (selected-window) buffer)
    (set-window-start (selected-window) (point-min))
    (set-window-hscroll (selected-window) 0)
    (redisplay t)
    buffer))

(defun neo-window-end-oracle--probe (&optional window)
  (let* ((window (or window (selected-window)))
         (buffer (window-buffer window))
         (end (window-end window t)))
    (with-current-buffer buffer
      (list (window-start window)
            end
            (position-bytes end)
            (point-min)
            (position-bytes (point-min))
            (point-max)
            (position-bytes (point-max))))))

(defun neo-window-end-oracle--record (name thunk)
  (push
   (condition-case error-data
       (list name :ok (funcall thunk))
     (error (list name :error error-data)))
   neo-window-end-oracle-results)
  (neo-window-end-oracle--flush))

(setq neo-window-end-oracle-results nil)

(neo-window-end-oracle--record
 'ascii-wrap
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text
     "alpha bravo charlie delta echo foxtrot golf hotel india" 80)
    nil)
   (neo-window-end-oracle--probe)))

(neo-window-end-oracle--record
 'utf8-wrap
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text
     (concat "A" (string #x597d #x1f642 #x03b2)
             " cafe" (string #x0301) " long-tail-for-wrapping")
     80)
    nil)
   (neo-window-end-oracle--probe)))

(neo-window-end-oracle--record
 'bidi-and-combining
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text
     (concat "left " (string #x05d0 #x05d1 #x05d2)
             " mid " (string #x0645 #x0631 #x062d #x0628 #x0627)
             " e" (string #x0301) " right")
     100)
    nil)
   (neo-window-end-oracle--probe)))

(neo-window-end-oracle--record
 'invisible-multibyte
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text
     (concat "shown-" (string #x597d #x03b2) "-hidden-tail-0123456789")
     100)
    nil)
   (put-text-property 20 180 'invisible t)
   (redisplay t)
   (neo-window-end-oracle--probe)))

(neo-window-end-oracle--record
 'display-replacement
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text "source-abcdefghijklmnopqrstuvxyz" 100)
    nil)
   (put-text-property
    20 160 'display (concat "<" (string #x597d #x1f642) ">"))
   (redisplay t)
   (neo-window-end-oracle--probe)))

(neo-window-end-oracle--record
 'narrowed-multibyte
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text
     (concat "prefix-" (string #x597d #x03b2) "-suffix-0123456789")
     120)
    nil)
   (narrow-to-region 37 (- (point-max) 73))
   (goto-char (point-min))
   (set-window-start nil (point-min))
   (redisplay t)
   (neo-window-end-oracle--probe)))

(neo-window-end-oracle--record
 'truncated-hscroll
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text
     "0123456789-abcdefghijklmnopqrstuvwxyz-ABCDEFGHIJKLMNOPQRSTUVWXYZ-tail" 100)
    t)
   (set-window-hscroll nil 17)
   (redisplay t)
   (list (window-hscroll) (neo-window-end-oracle--probe))))

(neo-window-end-oracle--record
 'stale-after-multibyte-edit
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text "before-edit-abcdefghijklmnopqrstuvwxyz" 100)
    nil)
   (let ((before (neo-window-end-oracle--probe)))
     (goto-char (point-min))
     (insert (string #x597d #x1f642 #x03b2) "-")
     ;; Do not redisplay here.  UPDATE=t must synchronously answer from the
     ;; current buffer rather than mixing stale char and byte companions.
     (list before (neo-window-end-oracle--probe)))))

(neo-window-end-oracle--record
 'explicit-start-side-effects
 (lambda ()
   (neo-window-end-oracle--setup
    (neo-window-end-oracle--text "explicit-start-abcdefghijklmnopqrstuvwxyz" 120)
    nil)
   (goto-char (point-min))
   (set-window-start nil 240)
   (let ((start-before (window-start))
         (point-before (point))
         (answer (neo-window-end-oracle--probe)))
     (list start-before point-before answer (window-start) (point)))))

(neo-window-end-oracle--record
 'split-selected-and-nonselected
 (lambda ()
   (let* ((left-buffer
           (neo-window-end-oracle--setup
            (neo-window-end-oracle--text "left-abcdefghijklmnopqrstuvwxyz" 100)
            nil))
          (left (selected-window))
          (right (split-window-right))
          (right-buffer (get-buffer-create " *window-end-oracle-right*")))
     (with-current-buffer right-buffer
       (widen)
       (erase-buffer)
       (insert
        (neo-window-end-oracle--text
         (concat "right-" (string #x597d #x03b2) "-abcdefghijklmnop")
         100))
       (goto-char (point-min)))
     (set-window-buffer right right-buffer)
     (set-window-point right
                       (with-current-buffer right-buffer (point-min)))
     (set-window-start left
                       (with-current-buffer left-buffer (point-min)))
     (set-window-start right
                       (with-current-buffer right-buffer (point-min)))
     (redisplay t)
     (list (neo-window-end-oracle--probe left)
           (neo-window-end-oracle--probe right)))))

(neo-window-end-oracle--record
 'active-minibuffer
 (lambda ()
   (let (answer)
     (minibuffer-with-setup-hook
         (lambda ()
           (insert
            (neo-window-end-oracle--text
             (concat "candidate-" (string #x597d #x03b2) "-abcdefghij")
             8))
           (redisplay t)
           (setq answer
                 (neo-window-end-oracle--probe
                  (active-minibuffer-window))))
       (read-from-minibuffer "Oracle: "))
     answer)))

(with-temp-file
    (expand-file-name "window-end-oracle.done" (getenv "HOME"))
  (insert "done\n"))
"WINDOW-END-ORACLE-DONE"
"#;

fn wait_for_results(gnu: &mut TuiSession, neo: &mut TuiSession) -> (String, String) {
    let gnu_path = gnu.home_dir().join(RESULT_FILE);
    let neo_path = neo.home_dir().join(RESULT_FILE);
    let gnu_done = gnu.home_dir().join(DONE_FILE);
    let neo_done = neo.home_dir().join(DONE_FILE);
    let deadline = Instant::now() + Duration::from_secs(20);

    while Instant::now() < deadline {
        if gnu_done.exists() && neo_done.exists() {
            break;
        }
        read_both(gnu, neo, Duration::from_millis(250));
    }

    let gnu_result = fs::read_to_string(&gnu_path)
        .unwrap_or_else(|error| panic!("GNU oracle did not write {}: {error}", gnu_path.display()));
    let neo_result = fs::read_to_string(&neo_path).unwrap_or_default();
    assert!(
        neo_done.exists(),
        "Neomacs oracle did not finish; completed cases:\n{neo_result}\n\
         grid:\n{}\nrecent PTY output:\n{}",
        neo.text_grid().join("\n"),
        String::from_utf8_lossy(neo.recent_output())
    );
    assert!(
        gnu_done.exists(),
        "GNU oracle did not finish; completed cases:\n{gnu_result}\n\
         grid:\n{}\nrecent PTY output:\n{}",
        gnu.text_grid().join("\n"),
        String::from_utf8_lossy(gnu.recent_output())
    );
    (gnu_result, neo_result)
}

#[test]
fn window_end_update_matches_gnu_across_display_boundaries() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, ORACLE_FILE, ORACLE_ELISP);
    write_home_file(&neo, ORACLE_FILE, ORACLE_ELISP);

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(load "~/window-end-oracle.el" nil t)"#,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), |grid| {
        grid.iter()
            .any(|row| row.contains("candidate-") || row.contains("Oracle:"))
    });
    send_both(&mut gnu, &mut neo, "RET");
    let (gnu_result, neo_result) = wait_for_results(&mut gnu, &mut neo);

    let gnu_lines = gnu_result.lines().collect::<Vec<_>>();
    let neo_lines = neo_result.lines().collect::<Vec<_>>();
    assert_eq!(
        neo_lines.len(),
        gnu_lines.len(),
        "oracle case count differs\nGNU:\n{gnu_result}\nNeomacs:\n{neo_result}"
    );
    let mut divergences = Vec::new();
    for (gnu_line, neo_line) in gnu_lines.iter().zip(&neo_lines) {
        assert!(
            !gnu_line.contains(":error"),
            "GNU oracle setup is invalid: {gnu_line}"
        );
        assert!(
            !neo_line.contains(":error"),
            "Neomacs signalled during oracle case: {neo_line}"
        );
        if neo_line != gnu_line {
            divergences.push(format!("GNU:     {gnu_line}\nNeomacs: {neo_line}"));
        }
    }
    assert!(
        divergences.is_empty(),
        "window-end diverged from GNU in {} case(s):\n\n{}",
        divergences.len(),
        divergences.join("\n\n")
    );
    assert_pair_exact_display(
        "window_end_update_matches_gnu_across_display_boundaries",
        &gnu,
        &neo,
    );
}
