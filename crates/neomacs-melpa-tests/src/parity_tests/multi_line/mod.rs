use std::time::Duration;

use crate::{
    CLOJURE_MODE_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, GO_MODE_MELPA_PIN,
    HASKELL_MODE_MELPA_PIN, MULTI_LINE_MELPA_PIN, RUST_MODE_MELPA_PIN, S_MELPA_PIN,
    SCALA_MODE_MELPA_PIN, SHUT_UP_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'clojure-mode)
(require 'go-mode)
(require 'haskell-mode)
(require 'multi-line)
(require 'multi-line-highlight)
(require 'rust-mode)
(require 'scala-mode)

(defmacro neomacs-multi-line-test-with-restored-cycle (&rest body)
  "Run BODY while restoring Multi-Line's mutable cycle state afterward."
  (declare (indent 0) (debug t))
  `(let* ((cycler (oref multi-line-current-strategy respace))
          (saved-last-cycler multi-line-last-cycler)
          (saved-marker (oref cycler last-cycle-marker))
          (saved-index (oref cycler cycle-index))
          (saved-command (oref cycler command-at-last-cycle)))
     (unwind-protect
         (progn ,@body)
       (setq multi-line-last-cycler saved-last-cycler)
       (oset cycler last-cycle-marker saved-marker)
       (oset cycler cycle-index saved-index)
       (oset cycler command-at-last-cycle saved-command))))

(defun neomacs-multi-line-test-prepare
    (mode contents point-pattern &optional fill)
  "Prepare a real MODE buffer with CONTENTS and point after POINT-PATTERN."
  (switch-to-buffer (current-buffer))
  (funcall mode)
  (use-local-map (copy-keymap (current-local-map)))
  (setq-local fill-column (or fill 80)
              indent-tabs-mode nil)
  (insert contents)
  (goto-char (point-min))
  (search-forward point-pattern)
  (local-set-key (kbd "C-c d") #'multi-line))

(defun neomacs-multi-line-test-buffer-state ()
  "Return the exact user-visible source and cursor state."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :modified (buffer-modified-p)))

(defun neomacs-multi-line-test-format-once
    (mode contents point-pattern &optional fill)
  "Format CONTENTS once through MODE's real public Multi-Line command."
  (with-temp-buffer
    (neomacs-multi-line-test-prepare
     mode contents point-pattern fill)
    (neomacs-multi-line-test-with-restored-cycle
      (execute-kbd-macro (kbd "C-c d"))
      (neomacs-multi-line-test-buffer-state))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MULTI_LINE_MELPA_PIN, "multi-line.el")
        .expect("prepare exact shallow Multi-Line source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow Dash dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare exact shallow s dependency below ./tmp")
        .with_melpa_dependency(SHUT_UP_MELPA_PIN)
        .expect("prepare exact shallow shut-up dependency below ./tmp")
        .with_melpa_dependency(CLOJURE_MODE_MELPA_PIN)
        .expect("prepare exact shallow Clojure Mode dependency below ./tmp")
        .with_melpa_dependency(GO_MODE_MELPA_PIN)
        .expect("prepare exact shallow Go Mode dependency below ./tmp")
        .with_melpa_dependency(HASKELL_MODE_MELPA_PIN)
        .expect("prepare exact shallow Haskell Mode dependency below ./tmp")
        .with_melpa_dependency(RUST_MODE_MELPA_PIN)
        .expect("prepare exact shallow Rust Mode dependency below ./tmp")
        .with_melpa_dependency(SCALA_MODE_MELPA_PIN)
        .expect("prepare exact shallow Scala Mode dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn multi_line_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "multi-line-package-batch",
        "multi_line_parity",
        &workflows::workflow_batch_cases(),
    );
}
