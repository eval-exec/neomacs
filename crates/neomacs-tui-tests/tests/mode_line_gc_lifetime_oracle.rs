#![cfg(unix)]
//! Live GNU oracle for GC ownership during mode-line formatting and display.
//! Each test starts fresh 40x120 TTY editors and compares runtime GNU values.
//! The property projection deliberately asks for the fixture's public keys;
//! GNU's additional mode-line-elt-no annotation is outside this lifetime test.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar d5-root-format nil)
(defvar d5-root-events nil)
(put 'd5-root-format 'risky-local-variable t)

(defun d5-root-fresh (label)
  (push (list 'fresh label) d5-root-events)
  (propertize (copy-sequence label)
              'd5-root-payload (vector (list (concat label "-value")))
              'help-echo (concat label "-help")
              'face 'bold))

(defun d5-root-collect (label)
  (push (list 'gc label) d5-root-events)
  (garbage-collect)
  (copy-sequence label))

(defun d5-root-make-format (kind)
  ;; Copy the template's conses: the function's constant pool must not retain
  ;; the active list after a :eval replaces d5-root-format during its walk.
  (pcase kind
    ('copied
     (list "ML[" (d5-root-fresh "Copied")
           '(:eval (d5-root-collect "Tail")) "]"))
    ('siblings
     (copy-tree '("ML[" (:eval (d5-root-fresh "Fresh"))
                  (:eval (d5-root-collect "Tail")) "]") t))
    ('ancestor
     (copy-tree
      '("ML[" (:eval (d5-root-fresh "Outer"))
        (:propertize (-7 (10 (:eval (d5-root-fresh "Inner"))))
                     d5-root-wrapper ("wrap" [("nested")]))
        (:eval (d5-root-collect "Tail")) "]") t))
    ('replace-reenter
     (copy-tree
      '("ML[" (:eval (d5-root-fresh "Outer"))
        (:eval
         (progn
           (setq d5-root-format (list (copy-sequence "Replacement")))
           (format-mode-line
            '("inner[" (:eval (d5-root-fresh "Nested"))
              (:eval (d5-root-collect "NestedTail")) "]"))
           (d5-root-collect "OuterTail")))
        "]") t))
    (_ (error "Unknown D5 GC oracle case: %S" kind))))

(defun d5-root-string-observation (string)
  ;; Preserve every character and every value for the specified keys. This
  ;; exposes fresh strings, nested vectors/lists and each padding/truncation
  ;; boundary without imposing a property-plist iteration order.
  (list (substring-no-properties string)
        (multibyte-string-p string)
        (let ((i 0) (observations nil))
          (while (< i (length string))
            (push (mapcar (lambda (key) (get-text-property i key string))
                          '(d5-root-payload d5-root-wrapper help-echo face))
                  observations)
            (setq i (1+ i)))
          (nreverse observations))))

(let ((out (getenv "NEOMACS_MODE_LINE_GC_ORACLE_OUT"))
      (ready (getenv "NEOMACS_MODE_LINE_GC_ORACLE_READY"))
      (release (getenv "NEOMACS_MODE_LINE_GC_ORACLE_RELEASE"))
      (kind (intern (getenv "NEOMACS_MODE_LINE_GC_ORACLE_CASE")))
      (observations nil)
      (display-events nil))
  (switch-to-buffer (get-buffer-create "d5-gc-root"))
  (erase-buffer)
  (insert "D5-GC-ROOT-BODY\n")
  (goto-char (point-min))
  (setq mode-line-format nil)
  (redisplay t)
  ;; Explicit interactive format-mode-line reaches the property accumulator.
  ;; Inspect after another GC too, while STRING is a Lisp-owned result.
  (dotimes (iteration 12)
    (setq d5-root-format (d5-root-make-format kind)
          d5-root-events nil)
    (let ((string (format-mode-line 'd5-root-format)))
      (garbage-collect)
      (push (list iteration (nreverse d5-root-events)
                  (d5-root-string-observation string))
            observations)))
  ;; Force the same source through actual redisplay/chrome fingerprinting.
  ;; The last displayed mode-line row is captured by Rust before exit.
  (dotimes (iteration 3)
    (setq d5-root-format (d5-root-make-format kind)
          d5-root-events nil
          mode-line-format '("" d5-root-format))
    (force-mode-line-update)
    (redisplay t)
    (push (list iteration (nreverse d5-root-events)) display-events))
  (with-temp-file out
    (let ((print-length nil) (print-level nil))
      (prin1 (list (nreverse observations) (nreverse display-events))
             (current-buffer))
      (insert "\n")))
  ;; Keep the captured frame steady. The Rust driver releases this exact
  ;; process after observing the ready marker and copying its mode-line row.
  (let ((inhibit-redisplay t))
    (with-temp-file ready (insert "ready\n"))
    (while (not (file-exists-p release))
      (accept-process-output nil 0.01))))
(kill-emacs 0)
"##;

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    values: String,
    mode_line: String,
}

fn run_probe(name: &str, program: PathBuf, kind: &str, gnu: bool) -> Observation {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-ml-gc-oracle-");
    let script = home.path().join("ml-gc-oracle.el");
    let out = home.path().join("ml-gc-oracle.out");
    let ready = home.path().join("ml-gc-oracle.ready");
    let release = home.path().join("ml-gc-oracle.release");
    std::fs::write(&script, PROBE_EL).expect("write GC probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch
            .arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_MODE_LINE_GC_ORACLE_OUT", &out)
        .env("NEOMACS_MODE_LINE_GC_ORACLE_READY", &ready)
        .env("NEOMACS_MODE_LINE_GC_ORACLE_RELEASE", &release)
        .env("NEOMACS_MODE_LINE_GC_ORACLE_CASE", kind)
        .env("NEOMACS_MODE_LINE_NUMERIC_PADDING", "on")
        .env("NEOMACS_MODE_LINE_GATE", "gnu");
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    session.read_until(Duration::from_secs(60), |_| ready.exists());
    assert!(
        ready.exists(),
        "{name} did not finish GC case {kind}; grid:\n{}",
        session.text_grid().join("\n")
    );
    session.read(Duration::from_millis(100));
    let mode_line = session.text_grid().get(38).expect("40-row grid").clone();
    let values = std::fs::read_to_string(&out)
        .unwrap_or_else(|error| panic!("{name} wrote no GC case {kind}: {error}"));
    std::fs::write(&release, "release\n").expect("release owned editor");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{name} did not exit GC case {kind}"
    );
    Observation { values, mode_line }
}

fn compare_case(kind: &str) {
    let gnu = run_probe("GNU", PathBuf::from("emacs"), kind, true);
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "neomacs binary missing: {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, kind, false);
    assert_eq!(
        neo, gnu,
        "mode-line GC lifetime diverges in case {kind}; all expected values come from live GNU"
    );
}

#[test]
fn copied_source_plists_survive_later_eval_gc_like_gnu() {
    compare_case("copied");
}

#[test]
fn fresh_eval_source_and_plists_survive_sibling_gc_like_gnu() {
    compare_case("siblings");
}

#[test]
fn ancestor_and_width_property_accumulators_survive_gc_like_gnu() {
    compare_case("ancestor");
}

#[test]
fn replaced_format_and_reentrant_output_survive_gc_like_gnu() {
    compare_case("replace-reenter");
}
