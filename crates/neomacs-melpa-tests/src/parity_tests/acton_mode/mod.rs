use std::time::Duration;

use crate::{ACTON_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACTON_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// acton-mode is a self-contained major mode for the Acton language: syntax
/// table, comment setup, its own line indenter, font-lock rules, an imenu
/// index and a `post-self-insert-hook' that realigns `else'/`elif'/`except'/
/// `finally'.  It needs no external program and declares no dependencies, so
/// every workflow drives the real thing: real `.act' files written into the
/// sandbox, visited with `find-file-noselect' so `auto-mode-alist' routes them,
/// then real editing commands.
///
/// One isolation detail: the per-case sandbox lives below the Neomacs checkout,
/// whose `.dir-locals.el' sets `tab-width' to 8 for every mode.  Left enabled
/// it silently overrides the mode's own `(setq-local tab-width
/// acton-indent-offset)', so directory-local variables are switched off.
const ACTON_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'imenu)

;; The per-case sandbox lives below the Neomacs checkout, whose
;; `.dir-locals.el' sets `tab-width' to 8 for every mode.  Letting it apply
;; would mask the mode's own `(setq-local tab-width acton-indent-offset)', so
;; directory-local variables are switched off for these files.
(setq enable-dir-local-variables nil)

(defvar actn-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst actn-test-counter
  "# a counter actor, from the Acton tutorial
import acton.rts

actor Counter(name: str):
    var count = 0
    limit: Int = 0x10

    action def bump(step: int) -> int:
        count += step
        if count > limit:
            print(\"over \", name)
        return count

class Point(object):
    def __init__(self, x: float):
        self.x = x

protocol Drawable:
    def draw(self) -> None:
        pass
")

(defun actn-test-write (name text)
  (let ((path (expand-file-name name actn-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun actn-test-visit (name text)
  "Write NAME with TEXT into the sandbox and visit it the way a user does."
  (let ((buffer (find-file-noselect (actn-test-write name text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun actn-test-face-runs ()
  (font-lock-ensure)
  (let ((position (point-min))
        (runs nil))
    (while (< position (point-max))
      (let ((next (next-single-property-change position 'face nil (point-max))))
        (push (cons (buffer-substring-no-properties position next)
                    (get-text-property position 'face))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun actn-test-text ()
  (buffer-substring-no-properties (point-min) (point-max)))


(defconst actn-test-unindented
  "actor Counter():
var count = 0
def bump(step: int) -> int:
count += step
if count > 10:
print(\"over\")
return count
def other():
pass
")

(defun actn-test-imenu ()
  "Return the imenu index with markers resolved to buffer positions."
  (mapcar (lambda (entry)
            (cons (car entry)
                  (mapcar (lambda (item)
                            (cons (car item) (marker-position (cdr item))))
                          (cdr entry))))
          (funcall imenu-create-index-function)))
"##;

fn acton_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACTON_MODE_MELPA_PIN, "acton-mode.el")
        .expect("prepare pinned acton-mode source below ./tmp")
        .with_prelude(ACTON_MODE_TEST_PRELUDE)
        .with_timeout(ACTON_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed acton-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_acton_mode_parity` cases (2a).
pub(crate) fn assert_acton_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(acton_mode_oracle(), &name, "acton_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn acton_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_acton_mode_batch(&cases);
}

// END generated package batch tests
