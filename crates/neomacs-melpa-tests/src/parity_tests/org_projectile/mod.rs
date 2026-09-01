use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_PROJECTILE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORG_PROJECTILE_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const ORG_PROJECTILE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'org)
(require 'org-capture)
(require 'org-projectile)

(defvar neomacs-org-projectile-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar neomacs-org-projectile-test-case-root nil)

(defun neomacs-org-projectile-test-write (path text)
  "Write TEXT to PATH inside the current case sandbox."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region text nil path nil 'silent))
  path)

(defun neomacs-org-projectile-test-project (relative)
  "Create a real Projectile project below RELATIVE and return its root."
  (let ((root (file-name-as-directory
               (expand-file-name
                relative neomacs-org-projectile-test-case-root))))
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    (neomacs-org-projectile-test-write
     (expand-file-name ".projectile" root)
     "# Isolate this fixture from the enclosing Neomacs project.\n")
    (neomacs-org-projectile-test-write
     (expand-file-name "README.md" root)
     (format "# %s\n" (file-name-nondirectory (directory-file-name root))))
    root))

(defun neomacs-org-projectile-test-file-text (path)
  "Return PATH's exact bytes decoded as UTF-8 text."
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path))
    (buffer-string)))

(defun neomacs-org-projectile-test-heading-record ()
  "Describe the current Org heading's visible planning state."
  (let ((todo (org-get-todo-state))
        (heading (org-get-heading t t t t)))
    (list :level (org-current-level)
          :todo (and todo (substring-no-properties todo))
          :heading (substring-no-properties heading)
          :stats (get-text-property 0 'org-stats heading)
          :category (org-get-category)
          :owner (org-entry-get (point) "OWNER"))))

(defun neomacs-org-projectile-test-capture-buffers ()
  "Return names of live Org indirect capture buffers."
  (sort
   (delq nil
         (mapcar
          (lambda (buffer)
            (and (string-prefix-p "CAPTURE-" (buffer-name buffer))
                 (buffer-name buffer)))
          (buffer-list)))
   #'string<))

(defmacro neomacs-org-projectile-test-with-sandbox (name &rest body)
  "Run BODY with isolated Projectile, Org Capture, files, and buffers."
  (declare (indent 1) (debug (form body)))
  `(save-window-excursion
     (let* ((case-root
             (file-name-as-directory
              (expand-file-name ,name neomacs-org-projectile-test-root)))
            (buffers-before (buffer-list))
            (origin-buffer (window-buffer (selected-window)))
            (neomacs-org-projectile-test-case-root case-root)
            (projectile-known-projects nil)
            (projectile-known-projects-on-file nil)
            (projectile-project-root-cache (make-hash-table :test 'equal))
            (projectile-project-name nil)
            (projectile-project-name-function #'projectile-default-project-name)
            (projectile-track-known-projects-automatically nil)
            (projectile-enable-caching nil)
            (org-projectile-strategy
             (make-instance 'org-projectile-combine-strategies))
            (org-project-capture-strategy
             (make-instance 'org-projectile-per-project-strategy))
            (org-project-capture-projects-file
             (expand-file-name "portfolio.org" case-root))
            (org-project-capture-projects-directory nil)
            (org-project-capture-per-project-filepath "TODO.org")
            (org-project-capture-capture-template "* TODO %?\n")
            (org-project-capture-force-linked t)
            (org-project-capture-counts-in-heading t)
            (org-project-capture-subheading-selection nil)
            (org-project-capture-add-category-to-new-files t)
            (org-todo-keywords '((sequence "TODO" "NEXT" "WAIT" "|" "DONE")))
            (org-capture-plist nil)
            (org-capture-current-plist nil)
            (org-capture-templates nil)
            (org-capture-last-stored-marker (make-marker))
            (org-store-link-plist nil)
            (org-capture-after-finalize-hook nil)
            (org-capture-before-finalize-hook nil)
            (enable-dir-local-variables nil))
       (when (file-directory-p case-root)
         (delete-directory case-root t))
       (make-directory case-root t)
       (unwind-protect
           (progn ,@body)
         (dolist (buffer (buffer-list))
           (when (and (not (memq buffer buffers-before))
                      (buffer-live-p buffer))
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer)))
         (when (markerp org-capture-last-stored-marker)
           (set-marker org-capture-last-stored-marker nil))
         (when (file-directory-p case-root)
           (delete-directory case-root t))))))
"####;

fn org_projectile_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_PROJECTILE_MELPA_PIN, "org-projectile.el")
        .expect("prepare exact shallow Org-Projectile source graph below ./tmp")
        .with_prelude(ORG_PROJECTILE_TEST_PRELUDE)
        .with_timeout(ORG_PROJECTILE_TEST_TIMEOUT)
}

fn assert_org_projectile_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        org_projectile_oracle(),
        "org-projectile-package-batch",
        "Org Projectile",
        cases,
    );
}

#[test]
fn org_projectile_package_batch() {
    assert_org_projectile_batch(&workflows::workflow_batch_cases());
}
