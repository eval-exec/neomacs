//! Practical parity for python-environment virtualenv paths.
//!
//! These cases resolve root/bin/lib layouts (Unix and Windows), detect a
//! planted environment, and signal when `virtualenv` is missing.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DEFERRED_MELPA_PIN, PYTHON_ENVIRONMENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'python-environment)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst pe469-test-tree
  "b7ddedbf19c14e6ab6c5d541cb15917d377e9a12")
(defconst pe469-test-manifest
  '(("python-environment-pkg.el" . "2510db4f351113b28febc7d06398424d142d465a4f4f036e7f6270e56fea367d")
    ("python-environment.el" . "620264a34093078e88ddb42b52da66635be2db5dfd323ce359839f91b5891549")
    ("test-python-environment.el" . "cce56613a99ffa9e539818f322808de8c02f54f4d5bab771b7a4be34d4f95e83")))

(defun pe469-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun pe469-test-source-state ()
  (let* ((located (locate-library "python-environment.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (pe469-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/python-environment.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car pe469-test-manifest)))
      (error "Unexpected installed python-environment payload: %S"
             (or manifest files)))
    (dolist (entry pe469-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (pe469-test-sha file) expected))
          (error "Unexpected installed python-environment source: %S"
                 (cons entry manifest)))))
    (list :tree pe469-test-tree
          :manifest manifest
          :feature (featurep 'python-environment)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'python-environment package-alist)))))))

(defun pe469-test-write (path text)
  (make-directory (file-name-directory path) t)
  (write-region text nil path nil 'silent)
  path)

(defun pe469-test-rel (root path)
  (and path (file-relative-name path root)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYTHON_ENVIRONMENT_MELPA_PIN, "python-environment.el")
        .expect("prepare pinned python-environment source below ./tmp")
        .with_melpa_dependency(DEFERRED_MELPA_PIN)
        .expect("prepare pinned deferred dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn root_path_uses_custom_directory_and_default_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "root_path_uses_custom_directory_and_default_name",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "pe-home"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (python-environment-directory root)
       (python-environment-default-root-name "café-env"))
  (list :source (pe469-test-source-state)
        :default (pe469-test-rel root (python-environment-root-path))
        :named (pe469-test-rel root (python-environment-root-path "other"))
        :exists (and (python-environment-exists-p) t)))
"####,
        expect![[
            r#"OK (:source (:tree "b7ddedbf19c14e6ab6c5d541cb15917d377e9a12" :manifest (("python-environment-pkg.el" . "2510db4f351113b28febc7d06398424d142d465a4f4f036e7f6270e56fea367d") ("python-environment.el" . "620264a34093078e88ddb42b52da66635be2db5dfd323ce359839f91b5891549") ("test-python-environment.el" . "cce56613a99ffa9e539818f322808de8c02f54f4d5bab771b7a4be34d4f95e83")) :feature t :version "20150310.853") :default "café-env" :named "other" :exists nil)"#
        ]],
    )
}

fn unix_bin_and_lib_detect_a_planted_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "unix_bin_and_lib_detect_a_planted_environment",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "pe-unix"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (python-environment-directory root)
       (python-environment-default-root-name "venv"))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (pe469-test-write (expand-file-name "venv/bin/python" root) "#!/bin/sh\n")
        (pe469-test-write (expand-file-name "venv/lib/site.py" root) "# café\n")
        (list :source (pe469-test-source-state)
              :exists (and (python-environment-exists-p) t)
              :bin (pe469-test-rel root (python-environment-bin "python"))
              :lib (pe469-test-rel root (python-environment-lib "site.py"))
              :missing-bin (python-environment-bin "no-such-tool")
              :missing-lib (python-environment-lib "no-such.py")))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "b7ddedbf19c14e6ab6c5d541cb15917d377e9a12" :manifest (("python-environment-pkg.el" . "2510db4f351113b28febc7d06398424d142d465a4f4f036e7f6270e56fea367d") ("python-environment.el" . "620264a34093078e88ddb42b52da66635be2db5dfd323ce359839f91b5891549") ("test-python-environment.el" . "cce56613a99ffa9e539818f322808de8c02f54f4d5bab771b7a4be34d4f95e83")) :feature t :version "20150310.853") :exists t :bin "venv/bin/python" :lib "venv/lib/site.py" :missing-bin nil :missing-lib nil)"#
        ]],
    )
}

fn windows_layout_falls_back_to_scripts_and_lib() -> ParityBatchCase {
    ParityBatchCase::value(
        "windows_layout_falls_back_to_scripts_and_lib",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "pe-win"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (python-environment-directory root)
       (python-environment-default-root-name "venv"))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (pe469-test-write (expand-file-name "venv/Scripts/python.exe" root) "MZ\n")
        (pe469-test-write (expand-file-name "venv/Lib/site.py" root) "# win\n")
        (list :source (pe469-test-source-state)
              :exists (and (python-environment-exists-p) t)
              :bin (pe469-test-rel root (python-environment-bin "python"))
              :lib (pe469-test-rel root (python-environment-lib "site.py"))))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "b7ddedbf19c14e6ab6c5d541cb15917d377e9a12" :manifest (("python-environment-pkg.el" . "2510db4f351113b28febc7d06398424d142d465a4f4f036e7f6270e56fea367d") ("python-environment.el" . "620264a34093078e88ddb42b52da66635be2db5dfd323ce359839f91b5891549") ("test-python-environment.el" . "cce56613a99ffa9e539818f322808de8c02f54f4d5bab771b7a4be34d4f95e83")) :feature t :version "20150310.853") :exists t :bin "venv/Scripts/python.exe" :lib "venv/Lib/site.py")"#
        ]],
    )
}

fn make_block_signals_when_virtualenv_is_missing() -> ParityBatchCase {
    ParityBatchCase::value(
        "make_block_signals_when_virtualenv_is_missing",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "pe-missing"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (python-environment-directory root)
       (python-environment-virtualenv '("pe469-no-such-virtualenv" "--quiet")))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (list :source (pe469-test-source-state)
              :missing
              (condition-case err
                  (python-environment-make-block)
                (error (list (car err)
                             (error-message-string err))))))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "b7ddedbf19c14e6ab6c5d541cb15917d377e9a12" :manifest (("python-environment-pkg.el" . "2510db4f351113b28febc7d06398424d142d465a4f4f036e7f6270e56fea367d") ("python-environment.el" . "620264a34093078e88ddb42b52da66635be2db5dfd323ce359839f91b5891549") ("test-python-environment.el" . "cce56613a99ffa9e539818f322808de8c02f54f4d5bab771b7a4be34d4f95e83")) :feature t :version "20150310.853") :missing (error "Program named \"pe469-no-such-virtualenv\" does not exist."))"#
        ]],
    )
}

#[test]
fn python_environment_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        root_path_uses_custom_directory_and_default_name(),
        unix_bin_and_lib_detect_a_planted_environment(),
        windows_layout_falls_back_to_scripts_and_lib(),
        make_block_signals_when_virtualenv_is_missing(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "python-environment-rank469",
        "python_environment_parity",
        &cases,
    );
}
