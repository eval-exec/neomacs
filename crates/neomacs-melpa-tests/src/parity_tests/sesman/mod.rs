use std::time::Duration;

use crate::{CachedMelpaOracle, SESMAN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SESMAN_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SESMAN_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'sesman)

(defmacro neomacs-sesman-test-with-empty (&rest body)
  "Run BODY with empty sesman session/link tables."
  `(let ((sesman-links-alist nil)
         (sesman-sessions-hashmap (make-hash-table :test #'equal)))
     ,@body))

(cl-defmethod sesman-start-session ((system (eql NeoParity)))
  "Start a deterministic NeoParity session."
  (let* ((name (format "neo-%s" (hash-table-count sesman-sessions-hashmap)))
         (session (list name (format "object-%s" name))))
    (sesman-register 'NeoParity session)
    session))

(cl-defmethod sesman-quit-session ((system (eql NeoParity)) session)
  "Mark SESSION as quit for NeoParity."
  (setcdr session (list "[quit]")))

(cl-defmethod sesman-project ((system (eql NeoParity)))
  "Use the parent of default-directory as the project root."
  (file-name-as-directory
   (expand-file-name
    (or (locate-dominating-file default-directory ".git")
        (file-name-directory (directory-file-name default-directory))))))

(defun neomacs-sesman-test-session-names (system)
  "Return sorted session names for SYSTEM."
  (sort (mapcar #'car (sesman-sessions system)) #'string<))
"####;

fn sesman_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SESMAN_MELPA_PIN, "sesman.el")
        .expect("prepare exact shallow sesman source below ./tmp")
        .with_prelude(SESMAN_TEST_PRELUDE)
        .with_timeout(SESMAN_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed sesman parity test")
        .into()
}

fn assert_sesman_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        sesman_oracle(),
        &current_test_name(),
        "sesman_parity",
        cases,
    );
}

#[test]
fn sesman_package_batch() {
    assert_sesman_batch(&workflows::workflow_batch_cases());
}
