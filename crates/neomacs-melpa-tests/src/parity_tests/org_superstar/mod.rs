use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_SUPERSTAR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORG_SUPERSTAR_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ORG_SUPERSTAR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'org-superstar)

(defvar neomacs-org-superstar-test-hook-calls nil)

(defun neomacs-org-superstar-test-heading-hook ()
  "Record the current heading and add a visible test property."
    (let ((level (org-superstar-heading-level))
        (position (match-beginning 1)))
    (push (list level (line-number-at-pos position))
          neomacs-org-superstar-test-hook-calls)
    (put-text-property (line-beginning-position) (line-end-position)
                       'neomacs-superstar-level level)))

(defun neomacs-org-superstar-test-fontify (text)
  "Insert TEXT, enable Org Superstar, and fully fontify the buffer."
  (insert text)
  (org-mode)
  (org-superstar-mode 1)
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-org-superstar-test-char-property (position property)
  "Describe PROPERTY at POSITION without unstable overlay state."
  (let ((value (get-text-property position property)))
    (cond
     ((eq property 'composition)
      (and value (substring-no-properties (format "%S" value))))
     (t value))))

(defun neomacs-org-superstar-test-heading-state ()
  "Describe the current Org heading's raw text and rendered stars."
  (save-excursion
    (beginning-of-line)
    (unless (looking-at "^\\(\\*+\\) \\(.*\\)$")
      (error "Point is not on an Org heading"))
    (let* ((start (match-beginning 1))
           (end (match-end 1))
           (level (- end start))
           (bullet (1- end)))
      (list
       :raw (match-string-no-properties 0)
       :level level
       :leading
       (let (result)
         (dotimes (offset (max 0 (1- level)) (nreverse result))
           (let ((position (+ start offset)))
             (push
              (list :composition
                    (neomacs-org-superstar-test-char-property
                     position 'composition)
                    :face (get-text-property position 'face)
                    :invisible (get-text-property position 'invisible))
              result))))
       :bullet
       (list :composition
             (neomacs-org-superstar-test-char-property bullet 'composition)
             :face (get-text-property bullet 'face)
             :invisible (get-text-property bullet 'invisible))))))

(defun neomacs-org-superstar-test-headings ()
  "Describe every heading in document order."
  (save-excursion
    (goto-char (point-min))
    (let (result)
      (while (re-search-forward "^\\*+ " nil t)
        (push (neomacs-org-superstar-test-heading-state) result))
      (nreverse result))))

(defun neomacs-org-superstar-test-list-state ()
  "Describe every list-looking bullet in document order."
  (save-excursion
    (goto-char (point-min))
    (let (result)
      (while (re-search-forward
              "^\\([ \t]*\\)\\([-+*]\\|[[:alnum:]]+[.)]\\) "
              nil t)
        (let ((position (match-beginning 2)))
          (push
           (list :line (line-number-at-pos)
                 :indent (length (match-string-no-properties 1))
                 :raw (match-string-no-properties 2)
                 :display (get-text-property position 'display)
                 :face (get-text-property position 'face))
           result)))
      (nreverse result))))
"##;

fn org_superstar_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_SUPERSTAR_MELPA_PIN, "org-superstar.el")
        .expect("prepare exact shallow Org Superstar source below ./tmp")
        .with_prelude(ORG_SUPERSTAR_TEST_PRELUDE)
        .with_timeout(ORG_SUPERSTAR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Org Superstar parity test")
        .into()
}

fn assert_org_superstar_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        org_superstar_oracle(),
        &current_test_name(),
        "org_superstar_parity",
        cases,
    );
}

#[test]
fn org_superstar_package_batch() {
    assert_org_superstar_batch(&workflows::workflow_batch_cases());
}
