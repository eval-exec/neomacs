#![cfg(unix)]
//! N1 (peer 6385, R4): the probe row follows GNU's matrix-reuse predicate.
//!
//! Three arms run from keys bound in a generated init file, so no minibuffer
//! exit or echo-area change precedes the redisplay that is measured:
//!
//! * CONTROL: a plain visible point move.  GNU's predicate holds, so
//!   `try_cursor_movement` succeeds and `partial_line_height` never scans the
//!   probe row.
//! * OUTDATED: make+delete an overlay.  The overlay SET returns to its
//!   previous content, so Neo's digest-keyed reuse still matches, but GNU's
//!   `OVERLAY_MODIFF` moved: `window_outdated (w)` (src/window.c:231-236)
//!   makes the predicate false and `try_window (..., TRY_WINDOW_CHECK_MARGINS)`
//!   scans the first row PAST the window end (src/xdisp.c:11630-11661,
//!   21696-21722).
//! * RESTORED: narrow and widen before the redisplay.  GNU's
//!   `reconsider_clip_changes` (src/xdisp.c:20486, :17271-17277) clears
//!   `clip_changed` because the window matrix still matches BEGV/ZV, so the
//!   predicate holds and no probe row is scanned.
//!
//! The recording fontification function never sets the `fontified` property,
//! so every redisplay re-enters it.  Only a probe entry can sit at or after
//! the published `window-end`; visible text lies before it.  Each arm writes
//! `(WINDOW-END POSITIONS...)` to its own file under the session's HOME.

use crate::support::*;
use neomacs_tui_tests::{TuiSession, TuiTempDirectory};
use std::ffi::OsStr;
use std::fs;
use std::time::Duration;

const INIT: &str = r#";;; -*- lexical-binding: t -*-
(progn
  (defvar p640n1-log nil)
  (defun p640n1-record (pos)
    (setq p640n1-log (cons pos p640n1-log)))
  (setq fontification-functions (list #'p640n1-record))
  ;; Append after startup so the initial *scratch* message still exists (the
  ;; harness ready predicate waits for it) and point stays where startup left
  ;; it, at the end of the scratch message; the control arm's (goto-char 5)
  ;; is then a plain point move, not a recenter.
  (add-hook 'emacs-startup-hook
            (lambda ()
              (with-current-buffer "*scratch*"
                (save-excursion
                  (goto-char (point-max))
                  (dotimes (_ 60) (insert (make-string 150 ?a) "\n"))))))
  (defun p640n1-run (body log-name)
    (setq p640n1-log nil)
    (funcall body)
    (redisplay)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file (expand-file-name log-name (getenv "HOME"))
        (prin1 (list (window-end) p640n1-log) (current-buffer)))))
  ;; The control and overlay arms are identical except for the overlay touch:
  ;; both move point to 5 (control from 1, overlay from the control's 5, so
  ;; its only change is the tick).
  (defun p640n1-arm-control () (interactive)
    (p640n1-run (lambda () (goto-char 5)) "p640n1-control.log"))
  (defun p640n1-arm-outdated () (interactive)
    (p640n1-run (lambda ()
                  (let ((ov (make-overlay 1 2))) (delete-overlay ov))
                  (goto-char 5))
                "p640n1-outdated.log"))
  (defun p640n1-arm-restored () (interactive)
    (p640n1-run (lambda ()
                  (narrow-to-region 1 8000)
                  (widen)
                  (goto-char 6))
                "p640n1-restored.log"))
  ;; Canonicalize the view after startup: the harness's first measured arm
  ;; must be a plain point move inside a top-anchored window, not a scroll or
  ;; a recenter left over from startup.
  (defun p640n1-warm-up ()
    (interactive)
    (goto-char 1)
    (set-window-start nil 1)
    nil)
  (global-set-key (kbd "<f5>") #'p640n1-arm-control)
  (global-set-key (kbd "<f6>") #'p640n1-arm-outdated)
  (global-set-key (kbd "<f7>") #'p640n1-arm-restored)
  (global-set-key (kbd "<f8>") #'p640n1-warm-up)
  nil)
"#;

/// `(window_end, logged_positions)` from an arm's Lisp log.  The first
/// integer is the published window end and is excluded from the positions.
fn parse_log(text: &str) -> Option<(i64, Vec<i64>)> {
    let mut numbers: Vec<i64> = Vec::new();
    let mut number = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
        } else if !number.is_empty() {
            numbers.push(number.parse().ok()?);
            number.clear();
        }
    }
    if !number.is_empty() {
        numbers.push(number.parse().ok()?);
    }
    let window_end = *numbers.first()?;
    Some((window_end, numbers.into_iter().skip(1).collect()))
}

fn read_log(session: &TuiSession, name: &str) -> Option<(i64, Vec<i64>)> {
    let text = fs::read_to_string(session.home_dir().join(name)).ok()?;
    parse_log(&text)
}

/// Press an arm's key and wait until both editors published its log.
fn run_arm(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    key: &str,
    name: &str,
) -> ((i64, Vec<i64>), (i64, Vec<i64>)) {
    send_both(gnu, neo, key);
    let deadline = neomacs_tui_tests::HangCap::new(Duration::from_secs(20));
    loop {
        let gnu_log = read_log(gnu, name);
        let neo_log = read_log(neo, name);
        if let (Some(g), Some(n)) = (&gnu_log, &neo_log) {
            return (g.clone(), n.clone());
        }
        if deadline.is_expired() {
            dump_pair_grids("p640n1_probe_row_arms/timeout", gnu, neo);
            panic!("timed out waiting for arm {name}: gnu={gnu_log:?} neo={neo_log:?}");
        }
        read_both(gnu, neo, Duration::from_millis(200));
    }
}

/// Did the arm enter the probe row?  A visible position can never sit at or
/// past the published window end.
fn probed((window_end, positions): &(i64, Vec<i64>)) -> bool {
    positions.iter().any(|pos| *pos >= *window_end)
}

#[test]
fn probe_row_follows_gnus_matrix_reuse_predicate() {
    let fixture = TuiTempDirectory::new("p640n1-");
    let init_path = fixture.path().join("p640n1-init.el");
    fs::write(&init_path, INIT).expect("write the arm init file");
    let extra_args = format!("-l {}", init_path.display());

    // Neo's GNU redisplay-hook policy owns fontification-functions in this
    // path; GNU needs no knob.
    let (mut gnu, mut neo) = boot_pair_with_envs(
        &extra_args,
        &[],
        &[("NEOMACS_REDISPLAY_GNU_HOOKS", OsStr::new("on"))],
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    // Absorb the startup-hook insertion's redisplay with a no-op command so
    // the first measured arm is a plain point move.
    send_both(&mut gnu, &mut neo, "F8");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let (gnu_control, neo_control) = run_arm(&mut gnu, &mut neo, "F5", "p640n1-control.log");
    assert!(
        !probed(&gnu_control),
        "GNU's control arm keeps its optimizations and must not scan the probe \
         row: {gnu_control:?}"
    );
    assert_eq!(
        probed(&neo_control),
        probed(&gnu_control),
        "the control arm must match GNU: gnu={gnu_control:?} neo={neo_control:?}"
    );

    let (gnu_outdated, neo_outdated) = run_arm(&mut gnu, &mut neo, "F6", "p640n1-outdated.log");
    assert!(
        probed(&gnu_outdated),
        "GNU's outdated arm refuses its optimizations and must scan the probe \
         row: {gnu_outdated:?}"
    );
    assert_eq!(
        probed(&neo_outdated),
        probed(&gnu_outdated),
        "the overlay-tick arm must match GNU: gnu={gnu_outdated:?} neo={neo_outdated:?}"
    );

    let (gnu_restored, neo_restored) = run_arm(&mut gnu, &mut neo, "F7", "p640n1-restored.log");
    assert!(
        !probed(&gnu_restored),
        "GNU's reconsider_clip_changes clears a restored restriction and must \
         not scan the probe row: {gnu_restored:?}"
    );
    assert_eq!(
        probed(&neo_restored),
        probed(&gnu_restored),
        "the restored-restriction arm must match GNU: gnu={gnu_restored:?} neo={neo_restored:?}"
    );
}
