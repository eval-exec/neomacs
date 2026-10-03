#![cfg(unix)]
//! Source-backed mutation owners, refreshed against live GNU31.1 per test.
//! Only object identity is normalized; hook sequence/context, pre arguments,
//! geometry, window-end, posn and pre-observer mode-line :eval counts are exact.
//! These six tests are proposed and unexecuted. This is not a parity claim.

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy)]
enum Scenario {
    TtyHeight,
    TtyWidth,
    RedrawFrame,
    RedrawDisplay,
    HiddenRename,
    KillLocal,
}
impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::TtyHeight => "tty-height",
            Self::TtyWidth => "tty-width",
            Self::RedrawFrame => "redraw-frame",
            Self::RedrawDisplay => "redraw-display",
            Self::HiddenRename => "hidden-rename",
            Self::KillLocal => "kill-local",
        }
    }
}

const PROBE_EL: &str = r##";;; -*- lexical-binding: t; -*-
(defvar d5-hook-order-log nil)
(defvar d5-hook-order-recording nil)
(defvar d5-hook-order-windows nil)
(defvar d5-hook-order-next-window 1)
(defvar d5-hook-order-main nil)
(defvar d5-hook-order-lower nil)

(defun d5-hook-order-normalize (value)
  ;; Identity normalization only. Initial main/minibuffer labels are anchored
  ;; before tracing. A measured action creates at most one unknown live window,
  ;; so encounter labels cannot hide a permutation among newly created windows.
  (cond
   ((windowp value)
    (let ((entry (assq value d5-hook-order-windows)))
      (unless entry
        (setq entry (cons value d5-hook-order-next-window)
              d5-hook-order-next-window (1+ d5-hook-order-next-window))
        (push entry d5-hook-order-windows))
      (list 'window (cdr entry))))
   ((framep value) '(frame root))
   ((consp value) (cons (d5-hook-order-normalize (car value))
                       (d5-hook-order-normalize (cdr value))))
   (t value)))

(defun d5-hook-order-record (scope event args)
  (when d5-hook-order-recording
    (push (list scope event (d5-hook-order-normalize args)
                (buffer-name)
                (d5-hook-order-normalize (selected-window))
                inhibit-redisplay)
          d5-hook-order-log)))

(defun d5-hook-order-callback (scope event)
  (lambda (&rest args) (d5-hook-order-record scope event args)))

(defun d5-hook-order-install (buffers)
  ;; Replace the observer lists explicitly so package/startup callbacks cannot
  ;; obscure the order under test. Local scroll's t marker also exercises the
  ;; global scroll callback; window-change functions invoke defaults separately.
  (setq pre-redisplay-function
        (lambda (windows)
          (d5-hook-order-record 'direct 'pre-redisplay (list windows))))
  (dolist (entry '((window-scroll-functions . scroll)
                   (window-buffer-change-functions . buffer)
                   (window-size-change-functions . size)
                   (window-selection-change-functions . selection)
                   (window-state-change-functions . state)
                   (window-configuration-change-hook . configuration)))
    (let ((symbol (car entry)) (event (cdr entry)))
      (set-default symbol (list (d5-hook-order-callback 'global event)))
      (dolist (buffer buffers)
        (with-current-buffer buffer
          (set (make-local-variable symbol)
               (if (eq symbol 'window-scroll-functions)
                   (list (d5-hook-order-callback 'local event) t)
                 (list (d5-hook-order-callback 'local event))))))))
  (set-default 'window-state-change-hook
               (list (d5-hook-order-callback 'global 'state-hook))))

(defun d5-hook-order-buffer (name)
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (erase-buffer)
      (fundamental-mode)
      (dotimes (i 100) (insert (format "line-%03d abcdefghijklmnop\n" i)))
      (goto-char (point-min))
      (forward-line 10)
      (setq-local mode-line-format
                  (list (list :eval
                              (list 'd5-mutation-mode-line
                                    (list 'quote (intern name))))))
      (setq-local header-line-format nil)
      (setq-local tab-line-format nil)
      (setq-local truncate-lines t)
      (setq-local bidi-paragraph-direction 'left-to-right))
    buffer))


(defvar d5-mutation-counts nil)
(defun d5-mutation-mode-line (name)
  (let ((entry (assq name d5-mutation-counts)))
    (if entry (setcdr entry (1+ (cdr entry)))
      (push (cons name 1) d5-mutation-counts)))
  "ml")

(defun d5-mutation-geometry (window)
  (let* ((buffer (window-buffer window))
         (position (with-current-buffer buffer (point)))
         (posn (posn-at-point position window)))
    (list (d5-hook-order-normalize window)
          (buffer-name buffer)
          (window-edges window) (window-pixel-edges window)
          (window-body-width window) (window-body-height window)
          (window-start window) (window-end window t)
          (and posn (posn-point posn))
          (and posn (posn-x-y posn))
          (and posn (nth 8 posn)))))

(defun d5-mutation-snapshot ()
  ;; Record only the action's redisplay evaluations. GNU posn-at-point below
  ;; separately evaluates visible chrome in pos_visible_p, even on idle rows.
  ;; Keep all positional observers, but freeze counts before invoking them.
  (let ((redisplay-counts
         (mapcar (lambda (name)
                   (list name (or (cdr (assq name d5-mutation-counts)) 0)))
                 '(hook-order-a hook-order-b hook-order-hidden))))
    (list (mapcar #'d5-mutation-geometry
                  (list d5-hook-order-main d5-hook-order-lower
                        (minibuffer-window)))
          redisplay-counts
          (buffer-name) (d5-hook-order-normalize (selected-window)))))

(defun d5-mutation-stage (label action)
  (setq d5-hook-order-log nil d5-mutation-counts nil
        d5-hook-order-recording t)
  (funcall action)
  (redisplay t)
  (setq d5-hook-order-recording nil)
  (list label (nreverse d5-hook-order-log) (d5-mutation-snapshot)))

(defun d5-hook-order-main ()
  (let ((output (getenv "D5_HOOK_ORDER_OUT"))
        (scenario (getenv "D5_HOOK_ORDER_CASE")))
    (condition-case error-data
        (let* ((buffer-a (d5-hook-order-buffer "hook-order-a"))
               (buffer-b (d5-hook-order-buffer "hook-order-b"))
               (buffer-hidden (d5-hook-order-buffer "hook-order-hidden"))
               (result nil))
          (set-default 'mode-line-format nil)
          (delete-other-windows)
          (switch-to-buffer buffer-a)
          (setq d5-hook-order-main (selected-window)
                d5-hook-order-lower (split-window-below)
                d5-hook-order-windows
                (list (cons d5-hook-order-main 0)
                      (cons d5-hook-order-lower 1)
                      (cons (minibuffer-window) 'mini))
                d5-hook-order-next-window 2)
          (set-window-buffer d5-hook-order-lower buffer-b)
          (set-window-start d5-hook-order-main 1)
          (set-window-start d5-hook-order-lower 1)
          (d5-hook-order-install (list buffer-a buffer-b buffer-hidden))
          (redisplay t)
          (redisplay t)
          (cond
           ((member scenario '("tty-height" "tty-width"))
            (let* ((frame (selected-frame))
                   (height-p (equal scenario "tty-height"))
                   (setter (if height-p #'set-frame-height #'set-frame-width))
                   (original (if height-p (frame-height frame) (frame-width frame)))
                   ;; Even root heights avoid the existing odd-remainder
                   ;; redistribution difference. Width grows beyond native
                   ;; terminal width to keep minibuffer separator geometry stable.
                   (changed (if height-p (- original 4) (+ original 7))))
              (push (d5-mutation-stage 'changed
                       (lambda () (funcall setter frame changed t))) result)
              (push (d5-mutation-stage 'same-size
                       (lambda () (funcall setter frame changed t))) result)
              (push (d5-mutation-stage 'restored
                       (lambda () (funcall setter frame original t))) result)))
           ((equal scenario "redraw-frame")
            (push (d5-mutation-stage 'redraw-frame
                     (lambda () (redraw-frame (selected-frame)))) result)
            (push (d5-mutation-stage 'idle-after-redraw #'ignore) result))
           ((equal scenario "redraw-display")
            (push (d5-mutation-stage 'redraw-display #'redraw-display) result)
            (push (d5-mutation-stage 'idle-after-redraw #'ignore) result))
           ((equal scenario "hidden-rename")
            (push (d5-mutation-stage 'hidden-rename
                     (lambda ()
                       (with-current-buffer buffer-hidden
                         (rename-buffer "hook-order-hidden-renamed")))) result)
            (push (d5-mutation-stage 'idle-after-hidden-rename #'ignore) result))
           ((equal scenario "kill-local")
            (push (d5-mutation-stage 'kill-local
                     (lambda () (with-current-buffer buffer-a
                                  (kill-all-local-variables)))) result)
            (push (d5-mutation-stage 'idle-after-local-reset #'ignore) result))
           (t (error "Unknown mutation scenario %S" scenario)))
          (with-temp-file output
            (prin1 (nreverse result) (current-buffer))
            (terpri (current-buffer))))
      (error
       (setq d5-hook-order-recording nil)
       (with-temp-file output
         (insert "ERROR: " (prin1-to-string error-data) "\n"))))
    (kill-emacs 0)))

(setq inhibit-startup-screen t initial-scratch-message nil
      native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent
      redisplay-dont-pause t)
(when (fboundp 'menu-bar-mode) (menu-bar-mode -1))
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(run-with-timer 0.1 nil #'d5-hook-order-main)
"##;

fn run_probe(name: &str, program: PathBuf, scenario: Scenario, extra: &[&str]) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-hook-order-oracle-");
    let script = home.path().join("hook-order-oracle.el");
    std::fs::write(&script, PROBE_EL).expect("write hook-order probe");
    let out = home.path().join("hook-order-oracle.out");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    for arg in extra {
        launch = launch.arg(*arg);
    }
    launch = launch
        .arg("--load")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("D5_HOOK_ORDER_CASE", scenario.name())
        .env("D5_HOOK_ORDER_OUT", &out);
    if name == "Neomacs" {
        launch = launch
            .env("NEOMACS_REDISPLAY_GNU_HOOKS", "on")
            .env("NEOMACS_MODE_LINE_ACCUM_ROOTS", "on")
            .env("NEOMACS_POSN_OBJECT_EXTENT", "on")
            .env("NEOMACS_MODE_LINE_GATE", "gnu");
    }
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new(
            "xterm-256color",
            if matches!(scenario, Scenario::TtyHeight) {
                41
            } else {
                40
            },
            120,
        ),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(60)),
        TuiProcessOutcome::Exited,
        "{name} {} probe did not finish; grid:\n{}",
        scenario.name(),
        session.text_grid().join("\n")
    );
    let result = std::fs::read_to_string(&out)
        .unwrap_or_else(|error| panic!("{name} {} wrote no trace: {error}", scenario.name()));
    assert!(
        !result.starts_with("ERROR:") && result.contains("pre-redisplay"),
        "{name} {} probe did not produce a valid callback trace: {result}",
        scenario.name()
    );
    result
}

fn assert_order_matches_gnu(scenario: Scenario) {
    let gnu = run_probe(
        "GNU",
        PathBuf::from(std::env::var_os("HOME").expect("HOME for GNU31.1")).join(".local/bin/emacs"),
        scenario,
        &[
            "-no-comp-spawn",
            "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))",
        ],
    );
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "neomacs binary missing: {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, scenario, &[]);
    assert_eq!(
        neo,
        gnu,
        "{} callback order/context differs\nGNU:\n{gnu}\nNeomacs:\n{neo}",
        scenario.name()
    );
}

#[test]
fn tty_height_preserves_width_and_same_size_hook_counts_match_gnu() {
    assert_order_matches_gnu(Scenario::TtyHeight);
}

#[test]
fn tty_width_preserves_height_and_same_size_hook_counts_match_gnu() {
    assert_order_matches_gnu(Scenario::TtyWidth);
}

#[test]
fn redraw_frame_targeted_hooks_and_idle_counts_match_gnu() {
    assert_order_matches_gnu(Scenario::RedrawFrame);
}

#[test]
fn redraw_display_targeted_hooks_and_idle_counts_match_gnu() {
    assert_order_matches_gnu(Scenario::RedrawDisplay);
}

#[test]
fn hidden_buffer_rename_preserves_other_mode_line_eval_counts_like_gnu() {
    assert_order_matches_gnu(Scenario::HiddenRename);
}

#[test]
fn kill_all_local_variables_preserves_other_mode_line_eval_counts_like_gnu() {
    assert_order_matches_gnu(Scenario::KillLocal);
}
