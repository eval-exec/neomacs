#![cfg(unix)]
//! TUI regression for the `minibuffer-line' GNU ELPA package (issue #414).
//!
//! `minibuffer-line' displays status information in the minibuffer window
//! when no minibuffer is active.  Its whole mechanism rests on one engine
//! behaviour: the miniwindow displays the permanently-inactive minibuffer
//! buffer `" *Minibuf-0*"' (GNU: `init_minibuf_once' creates it at startup,
//! `minibuffer_unwind' points the miniwindow back at it after every
//! minibuffer exit), so writing formatted text into that buffer — which is
//! all `minibuffer-line--update' does — must become visible in the
//! miniwindow's grid.
//!
//! These tests load the real package source and set a deterministic
//! `minibuffer-line-format' (a fixed marker string, no system-name or
//! wall-clock constructs) so both editors must render the same text in the
//! same screen row.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

/// The package source, verbatim from GNU ELPA (minibuffer-line 0.1).
const MINIBUFFER_LINE_SOURCE: &str = r#"
;;; minibuffer-line.el --- Display status info in the minibuffer window  -*- lexical-binding: t; -*-

(defgroup minibuffer-line ()
  "Use the idle minibuffer window to display status information."
  :group 'mode-line)

(defcustom minibuffer-line-format
  '("" (:eval system-name) " | " (:eval (format-time-string "%F %R")))
  "Specification of the contents of the minibuffer-line.
Uses the same format as `mode-line-format'."
  :type 'sexp)

(defface minibuffer-line
  '((t :inherit mode-line-inactive))
  "Face to use for the minibuffer-line.")

(defcustom minibuffer-line-refresh-interval 60
  "The frequency at which the minibuffer-line is updated, in seconds."
  :type 'integer)

(defconst minibuffer-line--buffer " *Minibuf-0*")

(defvar minibuffer-line--timer nil)

(define-minor-mode minibuffer-line-mode
  "Display status info in the minibuffer window."
  :global t
  (with-current-buffer minibuffer-line--buffer
    (erase-buffer))
  (when minibuffer-line--timer
    (cancel-timer minibuffer-line--timer)
    (setq minibuffer-line--timer nil))
  (when minibuffer-line-mode
    (setq minibuffer-line--timer
          (run-with-timer t minibuffer-line-refresh-interval
                          #'minibuffer-line--update))
    (minibuffer-line--update)))

(defun minibuffer-line--update ()
  (with-current-buffer minibuffer-line--buffer
    (erase-buffer)
    (insert (format-mode-line minibuffer-line-format 'minibuffer-line))))

(provide 'minibuffer-line)
;;; minibuffer-line.el ends here
"#;

/// Enable `minibuffer-line-mode' in both sessions with a deterministic
/// `(:eval ...)' format (the construct class the package's default format
/// uses), so the marker — not the hostname or wall clock — is what must
/// appear in the miniwindow.
fn enable_minibuffer_line_with_marker(gnu: &mut TuiSession, neo: &mut TuiSession, marker: &str) {
    let package = write_shared_temp_file("minibuffer-line.el", MINIBUFFER_LINE_SOURCE);
    let expression = format!(
        "(progn (load-file {:?})\
 (setq minibuffer-line-format '(\"\" (:eval (concat \"{marker}\" \"-MARKER\"))))\
 (minibuffer-line-mode 1))",
        package.path()
    );
    eval_expression(gnu, neo, &expression);
}

#[test]
fn minibuffer_line_mode_displays_the_format_in_the_miniwindow() {
    let (mut gnu, mut neo) = boot_pair("");
    enable_minibuffer_line_with_marker(&mut gnu, &mut neo, "MBL-STATUS");

    // `minibuffer-line-mode' calls `minibuffer-line--update' once at enable
    // time (no waiting for the 60s refresh timer), so the marker must appear
    // without any further input.
    let marker_ready = |grid: &[String]| grid.iter().any(|row| row.contains("MBL-STATUS-MARKER"));
    gnu.read_until(Duration::from_secs(6), marker_ready);
    neo.read_until(Duration::from_secs(8), marker_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_exact_display(
        "minibuffer_line_mode_displays_the_format_in_the_miniwindow",
        &gnu,
        &neo,
    );
}

#[test]
fn minibuffer_line_updates_on_its_refresh_timer() {
    let (mut gnu, mut neo) = boot_pair("");
    // A 1s refresh interval with a counting marker proves the refresh timer
    // re-renders the miniwindow (erase + re-insert), not just the first draw.
    // The counter lives in a single well-formed `(:eval ...)' form.
    let package = write_shared_temp_file("minibuffer-line.el", MINIBUFFER_LINE_SOURCE);
    let expression = format!(
        "(progn (load-file {:?})\
 (defvar mbl-tick 0)\
 (setq minibuffer-line-refresh-interval 1 \
 minibuffer-line-format '((:eval (concat \"TICK\" (number-to-string (setq mbl-tick (1+ mbl-tick)))))))\
 (minibuffer-line-mode 1))",
        package.path()
    );
    eval_expression(&mut gnu, &mut neo, &expression);

    // The first update runs at enable time (TICK1); the refresh timer must
    // push the count upward without any user input.
    let tick_ready = |grid: &[String]| grid.iter().any(|row| row.contains("TICK1"));
    gnu.read_until(Duration::from_secs(6), tick_ready);
    neo.read_until(Duration::from_secs(8), tick_ready);
    let tick_bumped = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("TICK2") || row.contains("TICK3"))
    };
    gnu.read_until(Duration::from_secs(8), tick_bumped);
    neo.read_until(Duration::from_secs(10), tick_bumped);

    // Live counters make an exact cross-engine grid comparison racy, so
    // settle both engines into the same static state: stop the refresh
    // timer and force one final update with a fixed marker.  This still
    // exercises the same `minibuffer-line--update' re-render path the
    // timer used, and gives the parity assertion a deterministic screen.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (cancel-timer minibuffer-line--timer)\
 (setq minibuffer-line--timer nil \
 minibuffer-line-format '(\"MBL-SETTLED-MARKER\"))\
 (minibuffer-line--update))",
    );
    let settled_ready = |grid: &[String]| grid.iter().any(|row| row.contains("MBL-SETTLED-MARKER"));
    gnu.read_until(Duration::from_secs(6), settled_ready);
    neo.read_until(Duration::from_secs(8), settled_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_exact_display("minibuffer_line_updates_on_its_refresh_timer", &gnu, &neo);
}
