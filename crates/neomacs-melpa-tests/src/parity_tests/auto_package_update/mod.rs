use std::time::Duration;

use crate::{AUTO_PACKAGE_UPDATE_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod buffers;
mod install;
mod registry;
mod schedule;
mod selection;
mod updates;
mod workflows;

const AUTO_PACKAGE_UPDATE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_PACKAGE_UPDATE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'package)
(require 'seq)

(defun auto-package-update-test-root (name)
  (let ((root
         (file-name-as-directory
          (expand-file-name
           name
           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory root t)
    root))

(defun auto-package-update-test-path (root name)
  (expand-file-name name root))

(defun auto-package-update-test-write (file contents)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents))
  file)

(defun auto-package-update-test-read (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun auto-package-update-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list
      :signal
      (car error-data)
      (cdr error-data)))))

(defun auto-package-update-test-desc
    (name version &optional requirements directory archive)
  (package-desc-create
   :name name
   :version version
   :summary (format "Fixture %s" name)
   :reqs requirements
   :kind 'tar
   :archive (or archive "fixture")
   :dir directory))

(defun auto-package-update-test-kill-buffers (&rest names)
  (dolist (name names)
    (let ((buffer (get-buffer name)))
      (when buffer
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun auto-package-update-test-package-source
    (name version requirements body)
  (format
   ";;; %s.el --- Local update fixture -*- lexical-binding: t; -*-\n\
\n\
;; Package-Version: %s\n\
;; Package-Requires: %S\n\
\n\
;;; Code:\n\
%s\n\
(provide '%s)\n\
;;; %s.el ends here\n"
   name
   (package-version-join version)
   (mapcar
    (lambda (requirement)
      (list
       (car requirement)
       (package-version-join
        (cadr requirement))))
    requirements)
   body
   name
   name))

(defun auto-package-update-test-configure-local-world (name)
  (let* ((root
          (auto-package-update-test-root name))
         (archive
          (auto-package-update-test-path
           root
           "archive/"))
         (day-file
          (auto-package-update-test-path
           root
           "state/last-day")))
    (setq
     package-user-dir
     (auto-package-update-test-path
      root
      "elpa/")
     package-archives
     (list
      (cons "fixture" archive))
     package-check-signature nil
     package-unsigned-archives
     '("fixture")
     package-alist nil
     package-archive-contents nil
     package-activated-list nil
     package-selected-packages nil)
    (make-directory
     (file-name-directory day-file)
     t)
    (list
     :root root
     :archive archive
     :package-user-dir package-user-dir
     :day-file day-file)))

(defun auto-package-update-test-write-local-archive
    (directory package-specs)
  (setq directory
        (file-name-as-directory directory))
  (make-directory directory t)
  (dolist (package package-specs)
    (let ((name (plist-get package :name))
          (version (plist-get package :version))
          (requirements
           (plist-get package :requirements))
          (body (plist-get package :body)))
      (auto-package-update-test-write
       (expand-file-name
        (format
         "%s-%s.el"
         name
         (package-version-join version))
        directory)
       (auto-package-update-test-package-source
        name version requirements body))))
  (with-temp-file
      (expand-file-name "archive-contents" directory)
    (let ((print-length nil)
          (print-level nil))
      (prin1
       (cons
        1
        (mapcar
         (lambda (package)
           (cons
            (plist-get package :name)
            (vector
             (plist-get package :version)
             (plist-get package :requirements)
             (format
              "Local fixture %s"
              (plist-get package :name))
             'single
             nil)))
         package-specs))
       (current-buffer))))
  directory)

(defun auto-package-update-test-install-local-version
    (root name version requirements body)
  (let ((source
         (expand-file-name
          (format "sources/%s.el" name)
          root)))
    (auto-package-update-test-write
     source
     (auto-package-update-test-package-source
      name version requirements body))
    (package-install-file source)))

(defun auto-package-update-test-installed-description (name)
  (cadr (assq name package-alist)))

(defun auto-package-update-test-installed-version (name)
  (let ((description
         (auto-package-update-test-installed-description
          name)))
    (and
     description
     (package-version-join
      (package-desc-version description)))))

(defun auto-package-update-test-installed-source (name)
  (let ((description
         (auto-package-update-test-installed-description
          name)))
    (and
     description
     (expand-file-name
      (format "%s.el" name)
      (package-desc-dir description)))))

(defun auto-package-update-test-installed-source-contains-p
    (name needle)
  (let ((source
         (auto-package-update-test-installed-source
          name)))
    (and
     source
     (file-readable-p source)
     (with-temp-buffer
       (insert-file-contents source)
       (search-forward needle nil t)))))
"##;

fn auto_package_update_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_PACKAGE_UPDATE_MELPA_PIN, source_file)
        .expect("prepare pinned auto-package-update source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(AUTO_PACKAGE_UPDATE_TEST_PRELUDE)
        .with_timeout(AUTO_PACKAGE_UPDATE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-package-update parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_package_update_autoload_parity` cases (2a).
pub(crate) fn assert_auto_package_update_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_package_update_oracle("auto-package-update-autoloads.el"),
        &name,
        "auto_package_update_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_package_update_parity` cases (2a).
pub(crate) fn assert_auto_package_update_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_package_update_oracle("auto-package-update.el"),
        &name,
        "auto_package_update_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_package_update_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_package_update_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_package_update_autoload_batch(&cases);
}

#[test]
fn auto_package_update_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        buffers::buffers_public_surface_batch_cases(),
        install::install_public_surface_batch_cases(),
        registry::registry_auto_package_update_batch_cases(),
        schedule::schedule_public_surface_batch_cases(),
        selection::selection_public_surface_batch_cases(),
        updates::updates_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_package_update_batch(&cases);
}

// END generated package batch tests
