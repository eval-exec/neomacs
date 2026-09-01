use std::time::Duration;

use crate::{APPLE_CONTAINER_TRAMP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APPLE_CONTAINER_TRAMP_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APPLE_CONTAINER_TRAMP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-apple-container-test-root (name)
  (file-name-as-directory
   (expand-file-name
    name
    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-apple-container-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-apple-container-test-prepare (name)
  (let* ((root
          (neomacs-apple-container-test-root name))
         (bin (expand-file-name "bin/" root))
         (program (expand-file-name "container" bin))
         (calls
          (expand-file-name
           "container-calls.log"
           root))
         (containers
          (expand-file-name
           "containers/"
           root)))
    (neomacs-apple-container-test-cleanup root)
    (make-directory bin t)
    (make-directory containers t)
    (with-temp-file program
      (insert
       "#!/bin/sh\n"
       "set -eu\n"
       "log=${APPLE_CONTAINER_TEST_LOG:?}\n"
       "printf '%s\\n' \"$*\" >> \"$log\"\n"
       "while [ \"$#\" -gt 0 ]; do\n"
       "  case \"$1\" in\n"
       "    --context|--url) shift 2 ;;\n"
       "    ls)\n"
       "      printf '%s\\n' 'ID IMAGE STATUS' 'payments app:v1 running' 'worker job:v2 running'\n"
       "      exit 0 ;;\n"
       "    exec) shift; break ;;\n"
       "    *) shift ;;\n"
       "  esac\n"
       "done\n"
       "while [ \"$#\" -gt 0 ]; do\n"
       "  case \"$1\" in\n"
       "    -it) shift ;;\n"
       "    -u) shift 2 ;;\n"
       "    sh|/bin/sh) shift; exec /bin/sh \"$@\" ;;\n"
       "    *) shift ;;\n"
       "  esac\n"
       "done\n"
       "exit 64\n"))
    (set-file-modes program #o755)
    (list root bin calls containers)))

(defun neomacs-apple-container-test-cleanup (root)
  (ignore-errors
    (tramp-cleanup-all-connections))
  (dolist (buffer (buffer-list))
    (let* ((file (buffer-file-name buffer))
           (localname
            (and
             file
             (or
              (ignore-errors
                (file-remote-p
                 file
                 'localname
                 'never))
              file))))
      (when
          (and
           localname
           (string-prefix-p root localname))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (file-exists-p root)
    (delete-directory root t)))
"####;

fn apple_container_tramp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APPLE_CONTAINER_TRAMP_MELPA_PIN, "apple-container-tramp.el")
        .expect("prepare pinned apple-container-tramp source below ./tmp")
        .with_prelude(APPLE_CONTAINER_TRAMP_TEST_PRELUDE)
        .with_timeout(APPLE_CONTAINER_TRAMP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apple-container-tramp parity test")
        .into()
}

/// Multi-probe batch for `assert_apple_container_tramp_parity` cases (2a).
pub(crate) fn assert_apple_container_tramp_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        apple_container_tramp_oracle(),
        &name,
        "apple_container_tramp_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn apple_container_tramp_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apple_container_tramp_batch(&cases);
}

// END generated package batch tests
