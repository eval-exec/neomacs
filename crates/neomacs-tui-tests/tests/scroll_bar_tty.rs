#![cfg(unix)]
//! GNU oracle coverage for scroll bars on a terminal frame.
//!
//! GNU gives a TTY frame no scroll bar at all.  `set-window-scroll-bars`
//! explicitly does nothing there ("Do nothing on a tty", src/window.c), the
//! `vertical-scroll-bars` / `horizontal-scroll-bars` frame parameters have no
//! effect on a frame without a window system, and every query stays at zero:
//! `window-scroll-bar-width` 0, `frame-scroll-bar-width` 0, and
//! `(window-scroll-bars)` = `(nil 0 t nil 0 t nil)`.  `scroll-bar-mode` writes
//! the frame parameter through `modify-all-frames-parameters`, so on a tty it
//! is a no-op too, and `window-body-width` never moves off the terminal width.
//!
//! Neomacs reserves the column instead.  `frame_vertical_scroll_bar_side`
//! (crates/neovm-core/src/window/display.rs) trusts the frame parameter
//! whenever it is present, and consults the window system only for the value
//! that is *absent*.  After `(scroll-bar-mode 1)` -- or after any
//! `(set-frame-parameter nil 'vertical-scroll-bars 'right)`, which the
//! `-vb` / `--vertical-scroll-bars` switches also write into
//! `default-frame-alist` -- a terminal frame reports WIDTH 1 column and
//! `window-scroll-bar-width` 1, and `window-body-width` shrinks 80 -> 79 for
//! a bar the TTY backend never paints.
//!
//! `set-window-scroll-bars` on a tty is a no-op in both editors (the tty
//! early-return in `FrameManager::set_window_scroll_bars` mirrors GNU's), so
//! that case is expected to stay green: it is the frame-parameter path that
//! diverges, and the oracle separates the two.
//!
//! This case is meant to stay RED: it is a real divergence and it is not to be
//! blessed from Neomacs, weakened, or skipped.  Fixing it means gating the
//! resolver on the frame having a window system, as GNU gates on
//! `FRAME_WINDOW_P` / `FRAME_HAS_VERTICAL_SCROLL_BARS`.

use crate::support;

use neomacs_tui_tests::TuiSession;
use std::fs;
use std::time::{Duration, Instant};
use support::{boot_pair, eval_expression, read_both, write_home_file};

const ORACLE_FILE: &str = "scroll-bar-tty-oracle.el";
const RESULT_FILE: &str = "scroll-bar-tty-oracle.out";
const DONE_FILE: &str = "scroll-bar-tty-oracle.done";

/// One `(NAME :ok STATE)` line per case, where STATE is
/// `(WSB-WIDTH WSB-HEIGHT WSBS BODY-WIDTH VSB-PARAM HSB-PARAM
///   FRAME-SB-WIDTH FRAME-SB-HEIGHT)`.
///
/// The frame-parameter cases are the divergence; `default-tty` and
/// `set-window-scroll-bars-width-4` are the controls that must agree.
const ORACLE_ELISP: &str = r#";;; -*- lexical-binding: t; -*-

(defvar neo-scroll-bar-tty-oracle-results nil)

(defun neo-scroll-bar-tty-oracle--flush ()
  (with-temp-file
      (expand-file-name "scroll-bar-tty-oracle.out" (getenv "HOME"))
    (dolist (result (reverse neo-scroll-bar-tty-oracle-results))
      (prin1 result (current-buffer))
      (terpri (current-buffer)))))

(defun neo-scroll-bar-tty-oracle--state ()
  (list (window-scroll-bar-width)
        (window-scroll-bar-height)
        (window-scroll-bars)
        (window-body-width)
        (frame-parameter nil 'vertical-scroll-bars)
        (frame-parameter nil 'horizontal-scroll-bars)
        (frame-scroll-bar-width)
        (frame-scroll-bar-height)))

(defun neo-scroll-bar-tty-oracle--record (name thunk)
  (push (condition-case error-data
            (list name :ok (funcall thunk))
          (error (list name :error error-data)))
        neo-scroll-bar-tty-oracle-results)
  (neo-scroll-bar-tty-oracle--flush))

(setq neo-scroll-bar-tty-oracle-results nil)

;; Control: a fresh terminal frame has no scroll bar in either editor.
(neo-scroll-bar-tty-oracle--record
 'default-tty
 (lambda () (neo-scroll-bar-tty-oracle--state)))

;; Divergence: GNU's `scroll-bar-mode' only writes frame parameters, and a
;; frame parameter cannot put a scroll bar on a terminal frame.
(neo-scroll-bar-tty-oracle--record
 'scroll-bar-mode-on
 (lambda ()
   (scroll-bar-mode 1)
   (redisplay t)
   (neo-scroll-bar-tty-oracle--state)))

(neo-scroll-bar-tty-oracle--record
 'scroll-bar-mode-off
 (lambda ()
   (scroll-bar-mode -1)
   (redisplay t)
   (neo-scroll-bar-tty-oracle--state)))

;; Divergence: the raw frame-parameter path, reachable without the mode.
(neo-scroll-bar-tty-oracle--record
 'frame-param-vertical-right
 (lambda ()
   (set-frame-parameter nil 'vertical-scroll-bars 'right)
   (redisplay t)
   (neo-scroll-bar-tty-oracle--state)))

(neo-scroll-bar-tty-oracle--record
 'frame-param-horizontal-bottom
 (lambda ()
   (set-frame-parameter nil 'horizontal-scroll-bars 'bottom)
   (redisplay t)
   (neo-scroll-bar-tty-oracle--state)))

;; Control: clearing both parameters must restore the opening state.
(neo-scroll-bar-tty-oracle--record
 'frame-params-cleared
 (lambda ()
   (set-frame-parameter nil 'vertical-scroll-bars nil)
   (set-frame-parameter nil 'horizontal-scroll-bars nil)
   (redisplay t)
   (neo-scroll-bar-tty-oracle--state)))

;; Control: GNU's "Do nothing on a tty" (src/window.c) also guards Neomacs's
;; window-local setter.  Signature is (WINDOW WIDTH VERTICAL-TYPE HEIGHT
;; HORIZONTAL-TYPE &optional PERSISTENT).
(neo-scroll-bar-tty-oracle--record
 'set-window-scroll-bars-width-4
 (lambda ()
   (set-window-scroll-bars (selected-window) 4 'right)
   (redisplay t)
   (neo-scroll-bar-tty-oracle--state)))

(with-temp-file
    (expand-file-name "scroll-bar-tty-oracle.done" (getenv "HOME"))
  (insert "done\n"))
"SCROLL-BAR-TTY-ORACLE-DONE"
"#;

/// Poll both result files; the load writes each case as it completes, so the
/// done marker is the only synchronization this oracle needs.
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
fn scroll_bar_tty_frame_parameter_reserves_no_column() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, ORACLE_FILE, ORACLE_ELISP);
    write_home_file(&neo, ORACLE_FILE, ORACLE_ELISP);

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(load "~/scroll-bar-tty-oracle.el" nil t)"#,
    );
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
        "scroll-bar TTY state diverged from GNU in {} case(s):\n\n{}",
        divergences.len(),
        divergences.join("\n\n")
    );
}
