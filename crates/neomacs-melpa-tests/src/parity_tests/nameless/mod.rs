use std::time::Duration;

use crate::{CachedMelpaOracle, NAMELESS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const NAMELESS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const NAMELESS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'nameless)

(defun neomacs-nameless-test-fontify ()
  "Synchronously refresh presentation properties in the current buffer."
  (font-lock-flush (point-min) (point-max))
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-nameless-test-spans ()
  "Describe every Nameless composition or display span in source order."
  (let ((position (point-min))
        result)
    (while (< position (point-max))
      (let* ((composition
              (get-text-property position 'composition))
             (display (get-text-property position 'display))
             (next-composition
              (next-single-property-change
               position 'composition nil (point-max)))
             (next-display
              (next-single-property-change
               position 'display nil (point-max)))
             (next (min next-composition next-display)))
        (when (or composition display)
          (push
           (list
            :range (list (- position (point-min))
                         (- next (point-min)))
            :source (buffer-substring-no-properties position next)
            :composition composition
            :display display
            :face (get-text-property position 'face))
           result))
        (setq position next)))
    (nreverse result)))

(defun neomacs-nameless-test-filtered-copy ()
  "Return copied source and whether presentation-only properties survived."
  (let* ((copy (filter-buffer-substring (point-min) (point-max)))
         (length (length copy)))
    (list
     :text (substring-no-properties copy)
     :composition
     (and (text-property-not-all 0 length 'composition nil copy) t)
     :display
     (and (text-property-not-all 0 length 'display nil copy) t)
     :face (and (text-property-not-all 0 length 'face nil copy) t))))

(defun neomacs-nameless-test-setup (text)
  "Create an Emacs Lisp editing session containing TEXT and enable Nameless."
  (emacs-lisp-mode)
  (font-lock-mode 1)
  (insert text)
  (goto-char (point-min))
  (nameless-mode 1)
  (neomacs-nameless-test-fontify))

"##;

fn nameless_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NAMELESS_MELPA_PIN, "nameless.el")
        .expect("prepare exact shallow Nameless source below ./tmp")
        .with_prelude(NAMELESS_TEST_PRELUDE)
        .with_timeout(NAMELESS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Nameless parity test")
        .into()
}

fn assert_nameless_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        nameless_oracle(),
        &current_test_name(),
        "nameless_parity",
        cases,
    );
}

#[test]
fn nameless_package_batch() {
    assert_nameless_batch(&workflows::workflow_batch_cases());
}
