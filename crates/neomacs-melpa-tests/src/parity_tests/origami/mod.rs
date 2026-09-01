use std::time::Duration;

use crate::{CachedMelpaOracle, ORIGAMI_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORIGAMI_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ORIGAMI_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; Origami's fold-header defface evaluates (face-attribute 'highlight :background)
;; at load time and puts it in a :box. Batch/TTY frames leave that attribute
;; unspecified, which modern Emacs rejects. Force a concrete color first.
(set-face-attribute 'highlight nil :background "gray80")
(require 'origami)
(face-spec-set
 'origami-fold-header-face
 '((t (:box (:line-width 1 :color "gray80") :background "gray80")))
 'face-defface-spec)

(defun neomacs-origami-test-with-buffer (body)
  "Call BODY in a buffer with nested braces suitable for folding."
  (with-temp-buffer
    (insert
     "function outer() {\n"
     "  function inner() {\n"
     "    return 1;\n"
     "  }\n"
     "  return inner();\n"
     "}\n")
    (js-mode)
    (origami-mode 1)
    (goto-char (point-min))
    (funcall body (current-buffer))))
"####;

fn origami_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORIGAMI_MELPA_PIN, "origami.el")
        .expect("prepare exact shallow origami source below ./tmp")
        .with_prelude(ORIGAMI_TEST_PRELUDE)
        .with_timeout(ORIGAMI_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed origami parity test")
        .into()
}

fn assert_origami_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        origami_oracle(),
        &current_test_name(),
        "origami_parity",
        cases,
    );
}

#[test]
fn origami_package_batch() {
    assert_origami_batch(&workflows::workflow_batch_cases());
}
