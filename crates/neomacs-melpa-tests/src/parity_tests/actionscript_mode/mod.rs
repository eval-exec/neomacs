use std::time::Duration;

use crate::{ACTIONSCRIPT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACTIONSCRIPT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// A real ActionScript 3 class plus sandbox helpers.  The fixture is written
/// with every line at column zero so the mode's own indenter has something to
/// do, and it deliberately contains a `}' inside a string and another inside a
/// line comment, because `as3-count-scope-depth' decides whether to count a
/// brace by looking at its *face* - so indentation only comes out right in a
/// fontified buffer.  Files are written into the per-case sandbox and visited
/// with `find-file-noselect', so `auto-mode-alist' picks the mode.
const ACTIONSCRIPT_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst as-test-ticker
  (concat
   "package com.example.game {\n"
   "\n"
   "import flash.display.Sprite;\n"
   "import flash.events.Event;\n"
   "\n"
   "/**\n"
   " * A sprite that counts frames.\n"
   " */\n"
   "public class Ticker extends Sprite implements ITickable {\n"
   "\n"
   "public static const MAX_TICKS:int = 100;\n"
   "\n"
   "private var _label:String = 'ready';\n"
   "private var _count:uint = 0;\n"
   "\n"
   "public function Ticker(label:String = \"ready\") {\n"
   "_label = label;\n"
   "addEventListener(Event.ENTER_FRAME, onEnterFrame);\n"
   "}\n"
   "\n"
   "public function get count():uint {\n"
   "return _count;\n"
   "}\n"
   "\n"
   "private function onEnterFrame(event:Event):void {\n"
   "if (_count < MAX_TICKS) {\n"
   "_count++;\n"
   "trace(\"tick } \" + _count);  // closing brace } in a comment\n"
   "} else {\n"
   "removeEventListener(Event.ENTER_FRAME, onEnterFrame);\n"
   "}\n"
   "}\n"
   "}\n"
   "}\n"))

(defconst as-test-syntax-sample
  (concat
   "package {\n"
   "class Syn {\n"
   "var $mixed_name:String = \"double \\\" quoted\";\n"
   "var other:String = 'single { quoted';\n"
   "/* block { comment */\n"
   "function run():void {\n"
   "if (a && (b || c)) { trace(\"x\"); } // line { comment\n"
   "}\n"
   "}\n"
   "}\n"))

(defun as-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun as-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (as-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun as-test-open (name text)
  "Visit a sandbox file holding TEXT and return its buffer."
  (find-file-noselect (as-test-write name text)))

(defun as-test-face-runs (&optional beginning end)
  "Return the (TEXT . FACE) runs font lock produced in the current buffer."
  (font-lock-ensure)
  (let ((position (or beginning (point-min)))
        (limit (or end (point-max)))
        (runs nil))
    (while (< position limit)
      (let ((next (next-single-property-change position 'face nil limit))
            (face (get-text-property position 'face)))
        (when face
          (push (cons (buffer-substring-no-properties position next) face) runs))
        (setq position next)))
    (nreverse runs)))

(defun as-test-at (needle &optional offset)
  "Return the buffer position of NEEDLE plus OFFSET."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (+ (match-beginning 0) (or offset 0))))

(defun as-test-ppss (position)
  "Return the interesting parts of `syntax-ppss' at POSITION."
  (let ((state (syntax-ppss position)))
    (list :depth (nth 0 state)
          :in-string (nth 3 state)
          :in-comment (and (nth 4 state) t)
          :comment-style (nth 7 state)
          :start (nth 8 state)
          :innermost-open (nth 1 state))))
"##;

fn actionscript_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACTIONSCRIPT_MODE_MELPA_PIN, "actionscript-mode.el")
        .expect("prepare pinned actionscript-mode source below ./tmp")
        .with_prelude(ACTIONSCRIPT_MODE_TEST_PRELUDE)
        .with_timeout(ACTIONSCRIPT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed actionscript-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_actionscript_mode_parity` cases (2a).
pub(crate) fn assert_actionscript_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        actionscript_mode_oracle(),
        &name,
        "actionscript_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn actionscript_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_actionscript_mode_batch(&cases);
}

// END generated package batch tests
