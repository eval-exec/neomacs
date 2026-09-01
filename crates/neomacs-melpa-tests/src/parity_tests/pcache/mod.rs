use std::time::Duration;

use crate::{CachedMelpaOracle, PCACHE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PCACHE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const PCACHE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'pcache)

(defclass neomacs-pcache-test-artifact ()
  ((name :initarg :name)
   (digest :initarg :digest)
   (labels :initarg :labels)))

(defun neomacs-pcache-test-root (name)
  "Return NAME below this oracle process's deterministic sandbox."
  (file-name-as-directory
   (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-pcache-test-canonical-entries (repository)
  "Return REPOSITORY values sorted by their printed keys."
  (let (rows)
    (pcache-map
     repository
     (lambda (key _entry)
       (push (list key (pcache-get repository key :missing)) rows)))
    (sort rows
          (lambda (left right)
            (string< (prin1-to-string (car left))
                     (prin1-to-string (car right)))))))

(defun neomacs-pcache-test-file-string (file)
  "Return FILE's bytes as an Emacs string."
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))
"##;

fn pcache_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PCACHE_MELPA_PIN, "pcache.el")
        .expect("prepare exact shallow Pcache source below ./tmp")
        .with_prelude(PCACHE_TEST_PRELUDE)
        .with_timeout(PCACHE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Pcache parity test")
        .into()
}

fn assert_pcache_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        pcache_oracle(),
        &current_test_name(),
        "pcache_parity",
        cases,
    );
}

#[test]
fn pcache_package_batch() {
    assert_pcache_batch(&workflows::workflow_batch_cases());
}
