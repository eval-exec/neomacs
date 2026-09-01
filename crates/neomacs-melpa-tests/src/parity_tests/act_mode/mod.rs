use std::time::Duration;

use crate::{ACT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// act-mode is a small major mode for the ACT hardware description language
/// whose own Commentary says it "currently only supports syntax highlighting",
/// so the workflows are about what a user sees: real `.act' files written into
/// the sandbox, visited with `find-file-noselect' so `auto-mode-alist' does the
/// routing, then `font-lock-ensure' and the resulting faces.  Nothing is
/// stubbed; the package needs no external program.
///
/// The fixture text is a two-stage buffer of the kind the ACT tutorial opens
/// with, so every category the mode knows about -- comment, keyword, function,
/// type and the `<N>' constant -- occurs in a realistic position.
///
/// Emacs's comment commands cannot be exercised here: with no `comment-start',
/// `comment-normalize-vars' prompts with `read-string', which in batch reads
/// stdin.  The workflows pin the comment *state* instead.
const ACT_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar actm-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst actm-test-design
  "// a two-stage buffer, from the ACT tutorial
import \"globals.act\";
export defproc buffer (bool? in; bool! out) {
  bool _x;
  prs {
    in => _x-
    _x => out-
  }
}
deftype e1of<3> onehot;
defchan handshake (int width) { pint w = width; }
")

(defun actm-test-write (name text)
  (let ((path (expand-file-name name actm-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun actm-test-visit (name text)
  "Write NAME with TEXT into the sandbox and visit it the way a user does."
  (let ((buffer (find-file-noselect (actm-test-write name text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun actm-test-face-runs ()
  "Return the buffer as (TEXT . FACE) runs, after fontification."
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

(defun actm-test-faces-of (&rest words)
  "Return (WORD FACE) for the first occurrence of each of WORDS."
  (font-lock-ensure)
  (mapcar (lambda (word)
            (save-excursion
              (goto-char (point-min))
              (if (let ((case-fold-search nil)) (search-forward word nil t))
                  (list word (get-text-property (match-beginning 0) 'face))
                (list word 'not-found))))
          words))
"##;

fn act_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACT_MODE_MELPA_PIN, "act-mode.el")
        .expect("prepare pinned act-mode source below ./tmp")
        .with_prelude(ACT_MODE_TEST_PRELUDE)
        .with_timeout(ACT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed act-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_act_mode_parity` cases (2a).
pub(crate) fn assert_act_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(act_mode_oracle(), &name, "act_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn act_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_act_mode_batch(&cases);
}

// END generated package batch tests
