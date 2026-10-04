#![cfg(unix)]
//! Live GNU callback-transfer oracle for the owned redisplay transaction.
//! Each case compares actual callback order, context, nonlocal result and the
//! subsequent recovery/idle passes. Only frame/window identity is normalized.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar d5-transfer-log nil)
(defvar d5-transfer-recording nil)
(defvar d5-transfer-phase nil)
(defvar d5-transfer-case nil)
(defvar d5-transfer-armed nil)
(defvar d5-transfer-windows nil)
(defvar d5-transfer-main nil)
(defvar d5-transfer-lower nil)

(defun d5-transfer-normalize (value)
  (cond
   ((windowp value)
    (let ((entry (assq value d5-transfer-windows)))
      (unless entry (error "Unanchored fixture window: %S" value))
      (list 'window (cdr entry))))
   ((framep value) '(frame root))
   ((consp value) (cons (d5-transfer-normalize (car value))
                       (d5-transfer-normalize (cdr value))))
   (t value)))

(defun d5-transfer-note (event &rest data)
  (when d5-transfer-recording
    (push (list d5-transfer-phase event (d5-transfer-normalize data)
                (buffer-name) (d5-transfer-normalize (selected-window))
                inhibit-redisplay inhibit-quit)
          d5-transfer-log)))

(defun d5-transfer-query (window)
  ;; Compare actual query values but omit the unrelated historical TTY glyph
  ;; object-height slot. A nested query must not accept/clear a display attempt.
  (let ((end (window-end window t))
        (position (posn-at-point (window-point window) window)))
    (d5-transfer-note 'query window end (nth 1 position) (nth 2 position))))

(defun d5-transfer-pre (windows)
  (d5-transfer-note 'pre windows)
  (when d5-transfer-armed
    (cond
     ((equal d5-transfer-case "pre-signal")
      (setq d5-transfer-armed nil)
      (d5-transfer-note 'pre-before-signal)
      (signal 'error '("D5 pre callback signal")))
     ((equal d5-transfer-case "pre-query-throw")
      (setq d5-transfer-armed nil)
      (d5-transfer-query d5-transfer-main)
      (d5-transfer-note 'pre-before-throw)
      (throw 'd5-transfer 'pre-callback-thrown)))))

(defun d5-transfer-local (event window)
  (d5-transfer-note event window)
  (when d5-transfer-armed
    (cond
     ((and (equal d5-transfer-case "local-signal") (eq event 'local-size))
      (setq d5-transfer-armed nil)
      (d5-transfer-note 'local-before-signal window)
      ;; GNU reads the actual cons cdr after each callback. Append during the
      ;; failing callback so a flattened hook plan cannot pass this case.
      (let ((functions (buffer-local-value 'window-size-change-functions
                                           (current-buffer))))
        (setcdr functions
                (append (cdr functions)
                        (list (lambda (target)
                                (d5-transfer-note 'added-live-tail target))))))
      (signal 'error '("D5 local callback signal")))
     ((and (equal d5-transfer-case "local-throw") (eq event 'local-state))
      (setq d5-transfer-armed nil)
      (d5-transfer-note 'local-before-throw window)
      (throw 'd5-transfer 'local-callback-thrown))
     ((and (equal d5-transfer-case "local-query-reentry") (eq event 'local-size))
      (setq d5-transfer-armed nil)
      (d5-transfer-query window)
      ;; GNU's private redisplaying guard must still prevent reentry when Lisp
      ;; deliberately rebinds the public inhibit variable to nil.
      (let ((inhibit-redisplay nil))
        (d5-transfer-note 'before-nested-redisplay window)
        (redisplay t)
        (d5-transfer-note 'after-nested-redisplay window))))))

(defun d5-transfer-install (buffers)
  (setq pre-redisplay-function #'d5-transfer-pre)
  (dolist (spec '((window-buffer-change-functions . buffer)
                  (window-size-change-functions . size)
                  (window-selection-change-functions . selection)
                  (window-state-change-functions . state)))
    (let ((symbol (car spec)) (event (cdr spec)))
      (set-default symbol
                   (list (lambda (frame) (d5-transfer-note event frame))))
      (dolist (buffer buffers)
        (with-current-buffer buffer
          (let ((local-event (intern (concat "local-" (symbol-name event))))
                (tail-event (intern (concat "tail-" (symbol-name event)))))
            (set (make-local-variable symbol)
                 (list (lambda (window) (d5-transfer-local local-event window))
                       (lambda (window) (d5-transfer-note tail-event window)))))))))
  (set-default 'window-state-change-hook
               (list (lambda () (d5-transfer-note 'state-hook))))
  (set-default 'window-configuration-change-hook
               (list (lambda () (d5-transfer-note 'configuration))))
  (set-default 'window-scroll-functions nil))

(defun d5-transfer-buffer (name)
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (erase-buffer)
      (fundamental-mode)
      (dotimes (i 100) (insert (format "row-%03d abcdefghijklmnop\n" i)))
      (goto-char (point-min))
      (forward-line 8)
      (setq-local mode-line-format nil header-line-format nil
                  tab-line-format nil truncate-lines t
                  bidi-paragraph-direction 'left-to-right))
    buffer))

(defun d5-transfer-redisplay (phase)
  (setq d5-transfer-phase phase)
  (let ((result (catch 'd5-transfer (redisplay t) 'display-returned)))
    (d5-transfer-note 'redisplay-result result)))

(defun d5-transfer-mini ()
  ;; Use the real reader's registered cleanup stack. GNU shrinks before calling
  ;; inactive-mode; a sizing throw skips inactive-mode but later independent
  ;; restore/configuration handlers still run when restore-windows is t.
  (let ((old-inactive (symbol-function 'minibuffer-inactive-mode))
        (old-resize (symbol-function 'window--resize-root-window-vertically))
        (size-throw-armed nil))
    (setq d5-transfer-recording nil)
    (let ((result
           (catch 'd5-transfer
             (unwind-protect
                 (progn
                   (fset 'minibuffer-inactive-mode
                         (lambda (&rest args)
                           (d5-transfer-note 'inactive-geometry
                                             (window-total-height (minibuffer-window))
                                             (minibuffer-depth))
                           (apply old-inactive args)))
                   (fset 'window--resize-root-window-vertically
                         (lambda (&rest args)
                           (when (and size-throw-armed (> (nth 1 args) 0))
                             (setq size-throw-armed nil)
                             (d5-transfer-note 'mini-before-sizing-throw args
                                               (window-total-height (minibuffer-window)))
                             (throw 'd5-transfer 'mini-sizing-thrown))
                           (apply old-resize args)))
                   (let ((read-minibuffer-restore-windows t)
                         (minibuffer-exit-hook nil))
                     (minibuffer-with-setup-hook
                         (lambda ()
                           (setq-local resize-mini-windows t max-mini-window-height 3
                                       truncate-lines nil mode-line-format nil)
                           (insert "first\nsecond\nthird\nfourth\nfifth")
                           (redisplay t)
                           (redisplay t)
                           (setq d5-transfer-recording nil)
                           ;; Setup runs before GNU installs the reader's exit
                           ;; catch. Start the full teardown trace in the timer,
                           ;; excluding intervening reader/idle redisplays.
                           (run-with-timer
                            0 nil
                            (lambda ()
                              (setq d5-transfer-recording t d5-transfer-phase 'mini-exit
                                    size-throw-armed (equal d5-transfer-case "mini-sizing-throw"))
                              (d5-transfer-note 'mini-exit-request
                                                (window-total-height (minibuffer-window)))
                              (exit-minibuffer))))
                       (read-from-minibuffer "transfer: "))))
               (fset 'minibuffer-inactive-mode old-inactive)
               (fset 'window--resize-root-window-vertically old-resize)))))
      (d5-transfer-note 'mini-reader-result result)
      (d5-transfer-note 'mini-restored
                        (window-total-height (minibuffer-window))
                        (minibuffer-depth)
                        (window-start d5-transfer-main)
                        (window-point d5-transfer-main)))))

(defun d5-transfer-main ()
  (let ((output (getenv "D5_HOOK_TRANSFER_OUT")))
    (condition-case error-data
        (let* ((a (d5-transfer-buffer "transfer-a"))
               (b (d5-transfer-buffer "transfer-b")))
          (setq d5-transfer-case (getenv "D5_HOOK_TRANSFER_CASE"))
          (delete-other-windows)
          (switch-to-buffer a)
          (setq d5-transfer-main (selected-window)
                d5-transfer-lower (split-window-below))
          (set-window-buffer d5-transfer-lower b)
          (set-window-start d5-transfer-main 1)
          (set-window-start d5-transfer-lower 1)
          (setq d5-transfer-windows
                (list (cons d5-transfer-main 'main)
                      (cons d5-transfer-lower 'lower)
                      (cons (minibuffer-window) 'mini)))
          (when (window-parent d5-transfer-main)
            (push (cons (window-parent d5-transfer-main) 'tree-root)
                  d5-transfer-windows))
          (d5-transfer-install (list a b))
          (redisplay t)
          (redisplay t)
          (setq d5-transfer-log nil d5-transfer-recording t
                d5-transfer-armed t d5-transfer-phase 'action)
          (cond
           ((equal d5-transfer-case "buffer-local-pre")
            (setq-local pre-redisplay-function
                        (lambda (windows) (d5-transfer-note 'buffer-local-pre windows)))
            (force-mode-line-update t))
           ((equal d5-transfer-case "buffer-local-inhibit")
            (setq-local inhibit-redisplay t)
            (force-mode-line-update t))
           ((member d5-transfer-case '("mini-inactive-order" "mini-sizing-throw"))
            (d5-transfer-mini))
           ((member d5-transfer-case '("pre-signal" "pre-query-throw"))
            (force-mode-line-update t))
           (t (enlarge-window 1)))
          (d5-transfer-redisplay 'action)
          (when (equal d5-transfer-case "buffer-local-inhibit")
            (setq d5-transfer-phase 'inhibit-release)
            (kill-local-variable 'inhibit-redisplay)
            (d5-transfer-note 'inhibit-released))
          ;; A throw aborts paint while the callback's flag has already changed.
          ;; GNU's next pre targets and window records are the live oracle for
          ;; pending ownership, record-on-unwind and guard/binding cleanup.
          (d5-transfer-redisplay 'recovery)
          (d5-transfer-redisplay 'idle)
          (when (equal d5-transfer-case "local-signal")
            ;; Change-function safe_call leaves the signaled callback present.
            ;; Trigger it again with its one-shot error disarmed.
            (setq d5-transfer-phase 'second-change)
            (enlarge-window -1)
            (d5-transfer-redisplay 'second-change)
            (d5-transfer-redisplay 'second-idle))
          (setq d5-transfer-recording nil)
          (with-temp-file output
            (let ((print-length nil) (print-level nil))
              (prin1 (nreverse d5-transfer-log) (current-buffer))
              (insert "\n"))))
      (error
       (setq d5-transfer-recording nil)
       (with-temp-file output
         (insert "ERROR: " (prin1-to-string error-data) "\n"))))
    (kill-emacs 0)))

(setq inhibit-startup-screen t initial-scratch-message nil
      native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent
      redisplay-dont-pause t resize-mini-windows nil)
(when (fboundp 'menu-bar-mode) (menu-bar-mode -1))
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(run-with-timer 0.1 nil #'d5-transfer-main)
"##;

fn run_probe(name: &str, program: PathBuf, scenario: &str, gnu: bool) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-hook-transfer-oracle-");
    let script = home.path().join("hook-transfer.el");
    let out = home.path().join("hook-transfer.out");
    std::fs::write(&script, PROBE_EL).expect("write hook transfer oracle");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch.arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    } else {
        launch = launch.env("NEOMACS_REDISPLAY_GNU_HOOKS", "on");
    }
    launch = launch
        .arg("--load")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("D5_HOOK_TRANSFER_CASE", scenario)
        .env("D5_HOOK_TRANSFER_OUT", &out);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(60)),
        TuiProcessOutcome::Exited,
        "{name} {scenario} transfer did not finish; grid:\n{}",
        session.text_grid().join("\n")
    );
    let result = std::fs::read_to_string(&out)
        .unwrap_or_else(|error| panic!("{name} {scenario} wrote no trace: {error}"));
    assert!(
        !result.starts_with("ERROR:") && result.contains("redisplay-result"),
        "{name} {scenario} has invalid transfer trace: {result}"
    );
    result
}

fn assert_transfer_matches_gnu(scenario: &str) {
    let gnu = run_probe("GNU", PathBuf::from("emacs"), scenario, true);
    let neo = run_probe(
        "Neomacs",
        neomacs_tui_tests::neomacs_binary(),
        scenario,
        false,
    );
    assert_eq!(
        neo, gnu,
        "{scenario} transfer/order/cleanup differs\nGNU:\n{gnu}\nNeomacs:\n{neo}"
    );
}

#[test]
fn pre_signal_is_demoted_and_next_pass_matches_gnu() {
    assert_transfer_matches_gnu("pre-signal");
}

#[test]
fn pre_query_then_throw_preserves_pending_targets_and_recovers_like_gnu() {
    assert_transfer_matches_gnu("pre-query-throw");
}

#[test]
fn local_signal_continues_live_tail_and_remains_installed_like_gnu() {
    assert_transfer_matches_gnu("local-signal");
}

#[test]
fn local_throw_records_on_unwind_then_recovers_like_gnu() {
    assert_transfer_matches_gnu("local-throw");
}

#[test]
fn local_query_and_public_inhibit_rebinding_preserve_private_guard_like_gnu() {
    assert_transfer_matches_gnu("local-query-reentry");
}

#[test]
fn mini_shrink_precedes_inactive_callback_like_gnu() {
    assert_transfer_matches_gnu("mini-inactive-order");
}

#[test]
fn mini_sizing_throw_restores_context_before_next_redisplay_like_gnu() {
    assert_transfer_matches_gnu("mini-sizing-throw");
}

#[test]
fn buffer_local_pre_function_uses_current_buffer_scope_like_gnu() {
    assert_transfer_matches_gnu("buffer-local-pre");
}

#[test]
fn buffer_local_inhibit_defers_targets_until_release_like_gnu() {
    assert_transfer_matches_gnu("buffer-local-inhibit");
}
