#![cfg(unix)]
//! Exact interactive GNU oracle for redisplay callback ordering and context.
//!
//! GNU xdisp.c calls prepare_menu_bars/pre-redisplay-function before
//! run_window_change_functions. window.c runs each window's local change
//! callbacks before all global change callbacks, then configuration callbacks
//! and the final state hook. Scroll callbacks run as redisplay commits a start.
//! Every scenario has fresh processes and compares the sequence GNU actually
//! produces. Only window/frame identities are replaced with stable labels;
//! no callback, ordering, argument, or context difference is accepted.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

#[derive(Clone, Copy)]
enum Scenario {
    Idle,
    Edit,
    Point,
    Scroll,
    Split,
    Selection,
    Resize,
    ForcedState,
    ActiveMini,
    OverlayMini,
    MiniShrinkPrefix,
    EchoGrow,
    EchoClear,
}

impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Edit => "edit",
            Self::Point => "point",
            Self::Scroll => "scroll",
            Self::Split => "split",
            Self::Selection => "selection",
            Self::Resize => "resize",
            Self::ForcedState => "forced-state",
            Self::ActiveMini => "active-mini",
            Self::OverlayMini => "overlay-mini",
            Self::MiniShrinkPrefix => "mini-shrink-prefix",
            Self::EchoGrow => "echo-grow",
            Self::EchoClear => "echo-clear",
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
      (setq-local mode-line-format nil)
      (setq-local header-line-format nil)
      (setq-local tab-line-format nil)
      (setq-local truncate-lines t)
      (setq-local bidi-paragraph-direction 'left-to-right))
    buffer))

(defun d5-hook-order-mini (scenario)
  ;; Let the real minibuffer reader own entry/exit. Only the action redisplay
  ;; is recorded, after initial active-window geometry has been accepted.
  (setq d5-hook-order-recording nil)
  (minibuffer-with-setup-hook
      (lambda ()
        (setq-local resize-mini-windows t max-mini-window-height 3
                    truncate-lines nil mode-line-format nil)
        (d5-hook-order-install (list (current-buffer)))
        (when (equal scenario "mini-shrink-prefix")
          (setq-local line-prefix "prefix ")
          (insert "first\nsecond\nthird\nfourth\nfifth"))
        (redisplay t)
        (redisplay t)
        (setq d5-hook-order-log nil d5-hook-order-recording t)
        (cond
         ((equal scenario "mini-shrink-prefix")
          (delete-region (minibuffer-prompt-end) (point-max))
          (goto-char (point-max))
          (insert "x"))
         ((equal scenario "overlay-mini")
          (let ((overlay (make-overlay (point-max) (point-max))))
            (overlay-put overlay 'after-string "\nfirst\nsecond\nthird\nfourth\nfifth")))
         (t (insert "first\nsecond\nthird\nfourth\nfifth")))
        (redisplay t)
        (push (list 'observation 'mini-geometry
                    (window-total-height (minibuffer-window))
                    (window-start (minibuffer-window))
                    (point) (buffer-name))
              d5-hook-order-log)
        (setq d5-hook-order-recording nil)
        ;; GNU installs the exit catch only after minibuffer-setup-hook.
        (run-with-timer 0 nil #'exit-minibuffer))
    (read-from-minibuffer "probe: ")))

(defun d5-hook-order-main ()
  (let ((output (getenv "D5_HOOK_ORDER_OUT"))
        (scenario (getenv "D5_HOOK_ORDER_CASE")))
    (condition-case error-data
        (let* ((buffer-a (d5-hook-order-buffer "hook-order-a"))
               (buffer-b (d5-hook-order-buffer "hook-order-b")))
          (delete-other-windows)
          (switch-to-buffer buffer-a)
          (setq d5-hook-order-main (selected-window)
                d5-hook-order-windows
                (list (cons (selected-window) 0) (cons (minibuffer-window) 'mini))
                d5-hook-order-next-window 1)
          (set-window-start d5-hook-order-main 1)
          (when (member scenario '("selection" "resize"))
            (setq d5-hook-order-lower (split-window-below))
            (set-window-buffer d5-hook-order-lower buffer-b)
            (push (cons d5-hook-order-lower 1) d5-hook-order-windows)
            (setq d5-hook-order-next-window 2))
          (d5-hook-order-install (list buffer-a buffer-b))
          ;; Accept initial geometry/change records before observing the action.
          (redisplay t)
          (redisplay t)
          (when (equal scenario "echo-clear")
            (setq resize-mini-windows t max-mini-window-height 3)
            (message "first\nsecond\nthird")
            (redisplay t))
          (setq d5-hook-order-log nil d5-hook-order-recording t)
          (cond
           ((member scenario '("active-mini" "overlay-mini" "mini-shrink-prefix"))
            (d5-hook-order-mini scenario))
           ((equal scenario "echo-grow")
            (setq resize-mini-windows t max-mini-window-height 3)
            (message "first\nsecond\nthird\nfourth"))
           ((equal scenario "echo-clear") (message nil))
           ((equal scenario "idle") nil)
           ((equal scenario "edit") (insert "x"))
           ((equal scenario "point") (forward-char 1))
           ((equal scenario "scroll")
            (set-window-start d5-hook-order-main
                              (save-excursion
                                (goto-char (point-min)) (forward-line 7) (point))))
           ((equal scenario "split") (split-window-below))
           ((equal scenario "selection") (select-window d5-hook-order-lower))
           ((equal scenario "resize") (enlarge-window 2))
           ((equal scenario "forced-state") (set-frame-window-state-change nil t))
           (t (error "Unknown hook-order scenario %S" scenario)))
          (redisplay t)
          (when (member scenario '("echo-grow" "echo-clear"))
            (push (list 'observation 'echo-geometry
                        (window-total-height (minibuffer-window))
                        (window-start (minibuffer-window))
                        (window-point (minibuffer-window))
                        (buffer-name (window-buffer (minibuffer-window))))
                  d5-hook-order-log))
          (setq d5-hook-order-recording nil)
          (with-temp-file output
            (prin1 (nreverse d5-hook-order-log) (current-buffer))
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
;; Run after startup has entered the real command loop, not batch emulation.
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
        launch = launch.env("NEOMACS_REDISPLAY_GNU_HOOKS", "on");
    }
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
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
        PathBuf::from("emacs"),
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
fn idle_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Idle);
}

#[test]
fn edit_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Edit);
}

#[test]
fn point_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Point);
}

#[test]
fn scroll_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Scroll);
}

#[test]
fn split_local_and_global_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Split);
}

#[test]
fn selection_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Selection);
}

#[test]
fn resize_local_and_global_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::Resize);
}

#[test]
fn forced_state_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::ForcedState);
}

#[test]
fn active_mini_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::ActiveMini);
}

#[test]
fn overlay_mini_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::OverlayMini);
}

#[test]
fn echo_grow_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::EchoGrow);
}

#[test]
fn echo_clear_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::EchoClear);
}

#[test]
fn active_mini_prefix_shrink_hook_order_matches_gnu() {
    assert_order_matches_gnu(Scenario::MiniShrinkPrefix);
}
