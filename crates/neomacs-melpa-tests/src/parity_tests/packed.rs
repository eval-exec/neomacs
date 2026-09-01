use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PACKED_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'packed)

(defvar neomacs-packed-test-root nil)

(defun neomacs-packed-test-write (relative content)
  (let ((file (expand-file-name relative neomacs-packed-test-root)))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert content))
    file))

(defun neomacs-packed-test-relative (file)
  (file-relative-name file neomacs-packed-test-root))

(defun neomacs-packed-test-normalize (value)
  (cond ((stringp value)
         (replace-regexp-in-string
          (regexp-quote neomacs-packed-test-root) "<package>/" value t t))
        ((consp value)
         (cons (neomacs-packed-test-normalize (car value))
               (neomacs-packed-test-normalize (cdr value))))
        (t value)))

(defun neomacs-packed-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (neomacs-packed-test-normalize
      (list :error (car error-data)
            :data (cdr error-data)
            :message (error-message-string error-data))))))

(defun neomacs-packed-test-in-tree (name function)
  (let* ((parent (file-name-as-directory (getenv "TMPDIR")))
         (root (make-temp-file
                (expand-file-name (format "packed-%s-" name) parent) t))
         (neomacs-packed-test-root (file-name-as-directory root)))
    (unwind-protect
        (funcall function root)
      (when (file-directory-p root) (delete-directory root t)))))
"####;

fn package_tree_discovery_keeps_real_libraries_and_ignores_non_package_content() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-packed-test-in-tree
 "discovery"
 (lambda (root)
   (neomacs-packed-test-write "acme.el" "(provide 'acme)\n")
   (neomacs-packed-test-write "acme-tools.el" "(provide 'acme-tools)\n")
   (neomacs-packed-test-write "notes.el" "(message \"documentation example\")\n")
   (neomacs-packed-test-write
    "extensions/acme-report.el" "(provide 'acme-report)\n")
   (neomacs-packed-test-write
    ".internal/acme-secret.el" "(provide 'acme-secret)\n")
   (neomacs-packed-test-write "vendor/.nosearch" "")
   (neomacs-packed-test-write "vendor/acme-vendor.el" "(provide 'acme-vendor)\n")
   (list
    :libraries (sort (packed-libraries root) #'string-lessp)
    :inventory
    (sort
     (mapcar (lambda (entry)
               (list (neomacs-packed-test-relative (car entry)) (cdr entry)))
             (packed-libraries-1 root))
     (lambda (left right) (string-lessp (car left) (car right))))
    :root-library (packed-library-p (expand-file-name "acme.el" root))
    :non-library (packed-library-p (expand-file-name "notes.el" root))
    :ignored-directory (packed-ignore-directory-p
                        (expand-file-name "vendor" root)))))
"####;
    let expected = expect![[
        r#"OK (:libraries ("acme-tools.el" "acme.el" "extensions/acme-report.el") :inventory (("acme-tools.el" acme-tools) ("acme.el" acme) ("extensions/acme-report.el" acme-report) ("notes.el" nil)) :root-library acme :non-library nil :ignored-directory t)"#
    ]];
    ParityBatchCase::value(
        "package_tree_discovery_keeps_real_libraries_and_ignores_non_package_content",
        elisp_form,
        expected,
    )
}

fn main_library_inference_handles_mode_packages_singletons_and_broken_features() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-packed-test-in-tree
 "main-library"
 (lambda (root)
   (neomacs-packed-test-write "service/service-mode.el" "(provide 'service-mode)\n")
   (neomacs-packed-test-write "service/service-tools.el" "(provide 'service-tools)\n")
   (neomacs-packed-test-write "singleton/odd-name.el" "(provide 'odd-name)\n")
   (neomacs-packed-test-write "broken/broken.el" "(provide 'different-feature)\n")
   (list
    :mode-package
    (neomacs-packed-test-relative
     (packed-main-library (expand-file-name "service" root) "service"))
    :singleton
    (neomacs-packed-test-relative
     (packed-main-library (expand-file-name "singleton" root) "product"))
    :broken
    (neomacs-packed-test-capture
     (lambda ()
       (packed-main-library (expand-file-name "broken" root) "broken" nil t)))
    :missing
    (packed-main-library-1 "missing" '("one.el" "two.el") t t))))
"####;
    let expected = expect![[
        r#"OK (:mode-package "service/service-mode.el" :singleton "singleton/odd-name.el" :broken (:error error :data ("Main library <package>/broken/broken.el provides no or wrong feature") :message "Main library <package>/broken/broken.el provides no or wrong feature") :missing nil)"#
    ]];
    ParityBatchCase::value(
        "main_library_inference_handles_mode_packages_singletons_and_broken_features",
        elisp_form,
        expected,
    )
}

fn source_metadata_parser_excludes_comments_and_strings_and_preserves_optional_requires()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-packed-test-in-tree
 "metadata"
 (lambda (_root)
   (let ((source
          (neomacs-packed-test-write
           "release-tools.el"
           ";; (require 'comment-only)\n\
(defconst release-example \"(require 'string-only)\")\n\
(require 'json)\n\
(require 'project nil t)\n\
(cc-require 'cl-lib)\n\
(provide 'release-tools '(release-api release-legacy))\n"))
         (theme
          (neomacs-packed-test-write
           "aurora-theme.el" "(provide-theme 'aurora)\n")))
     (with-temp-buffer
       (insert "scratch")
       (goto-char 4)
       (string-match "ra" "scratch")
       (let ((before-point (point))
             (before-match (match-data))
             parsed)
         (setq parsed
               (packed-with-file source
                 (list :point (point)
                       :file (file-name-nondirectory buffer-file-name)
                       :modified (buffer-modified-p)
                       :provided (packed-provided)
                       :required (packed-required))))
         (list :parsed parsed
               :library-feature (packed-library-feature source)
               :theme-feature (packed-library-feature theme)
               :caller-point-preserved (= before-point (point))
               :caller-match-preserved (equal before-match (match-data))))))))
"####;
    let expected = expect![[
        r#"OK (:parsed (:point 1 :file "release-tools.el" :modified nil :provided (release-legacy release-api release-tools) :required ((cl-lib json) (project))) :library-feature release-tools :theme-feature aurora-theme :caller-point-preserved t :caller-match-preserved t)"#
    ]];
    ParityBatchCase::value(
        "source_metadata_parser_excludes_comments_and_strings_and_preserves_optional_requires",
        elisp_form,
        expected,
    )
}

fn load_path_management_adds_only_library_directories_and_restores_the_environment()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-packed-test-in-tree
 "load-path"
 (lambda (root)
   (neomacs-packed-test-write "product.el" "(provide 'product)\n")
   (neomacs-packed-test-write "extensions/report.el" "(provide 'report)\n")
   (neomacs-packed-test-write "assets/readme.el" "No Lisp metadata here.\n")
   (neomacs-packed-test-write "vendor/.nosearch" "")
   (neomacs-packed-test-write "vendor/private.el" "(provide 'private)\n")
   (let ((load-path '("/external/site-lisp")))
     (let ((discovered
            (sort (mapcar #'neomacs-packed-test-relative
                          (packed-load-path root))
                  #'string-lessp)))
       (packed-add-to-load-path root)
       (let ((added
              (mapcar (lambda (directory)
                        (if (string-prefix-p root directory)
                            (neomacs-packed-test-relative directory)
                          directory))
                      load-path)))
         (packed-remove-from-load-path root)
         (list :discovered discovered :added added :restored load-path))))))
"####;
    let expected = expect![[
        r#"OK (:discovered ("." "extensions") :added ("extensions" "." "/external/site-lisp") :restored ("/external/site-lisp"))"#
    ]];
    ParityBatchCase::value(
        "load_path_management_adds_only_library_directories_and_restores_the_environment",
        elisp_form,
        expected,
    )
}

fn source_suffix_and_lookup_helpers_prefer_editable_lisp_over_compiled_artifacts() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-packed-test-in-tree
 "lookup"
 (lambda (root)
   (let ((load-suffixes '(".elc" ".el"))
         (load-file-rep-suffixes '("" ".gz")))
     (neomacs-packed-test-write "widget.el" "(provide 'widget)\n")
     (neomacs-packed-test-write "widget.elc" "compiled-placeholder")
     (neomacs-packed-test-write "compressed.el.gz" "compressed-placeholder")
     (list
      :el-suffixes (copy-tree (packed-el-suffixes))
      :elc-suffixes (copy-tree (packed-elc-suffixes))
      :el-nosuffix (copy-tree (packed-el-suffixes t nil))
      :el-must-suffix (copy-tree (packed-el-suffixes nil t))
      :el-regexp
      (mapcar (lambda (file)
                (not (null (string-match-p (packed-el-regexp) file))))
              '("widget.el" "widget.el.gz" "widget.elc"))
      :located (file-name-nondirectory
                (packed-locate-library "widget" nil (list root)))
      :source (file-name-nondirectory
               (packed-el-file (expand-file-name "widget.elc" root)))
      :compressed-source
      (file-name-nondirectory
       (packed-el-file (expand-file-name "compressed.elc" root)))
      :destination
      (file-name-nondirectory
       (packed-elc-file (expand-file-name "widget.el" root)))))))
"####;
    let expected = expect![[
        r#"OK (:el-suffixes (".el" ".el.gz" "" ".gz") :elc-suffixes (".elc" ".elc.gz" "" ".gz") :el-nosuffix ("" ".gz") :el-must-suffix (".el" ".el.gz") :el-regexp (t t nil) :located "widget.el" :source "widget.el" :compressed-source "compressed.el.gz" :destination "widget.elc")"#
    ]];
    ParityBatchCase::value(
        "source_suffix_and_lookup_helpers_prefer_editable_lisp_over_compiled_artifacts",
        elisp_form,
        expected,
    )
}

fn byte_compile_and_autoload_workflow_produces_loadable_package_artifacts_without_mode_hooks()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-packed-test-in-tree
 "build"
 (lambda (root)
   (let* ((source
           (neomacs-packed-test-write
            "deploy.el"
            ";;;###autoload\n(defun deploy-release () (interactive) :released)\n\
(provide 'deploy)\n"))
          (destination (expand-file-name "deploy.elc" root))
          (autoloads (expand-file-name "deploy-autoloads.el" root))
          (nested (expand-file-name "nested/leaf" root))
          (after-change-count 0)
          (prog-count 0)
          (elisp-count 0)
          (after-change-major-mode-hook
           (list (lambda () (setq after-change-count (1+ after-change-count)))))
          (prog-mode-hook (list (lambda () (setq prog-count (1+ prog-count)))))
          (emacs-lisp-mode-hook
           (list (lambda () (setq elisp-count (1+ elisp-count)))))
          (byte-compile-warnings nil))
     (make-directory nested t)
     (let ((compile-result (packed-byte-compile-file source)))
       (packed-update-autoloads autoloads root)
       (let ((autoload-text
              (with-temp-buffer
                (insert-file-contents autoloads)
                (buffer-string))))
         (list :compile-result compile-result
               :compiled (file-exists-p destination)
               :mode-hook-counts
               (list after-change-count prog-count elisp-count)
               :autoload-file (file-name-nondirectory autoloads)
               :autoload-present
               (not (null (string-match-p
                           "(autoload 'deploy-release" autoload-text)))
               :loaddefs
               (file-name-nondirectory (packed-loaddefs-file nested))))))))
"####;
    let expected = expect![[
        r#"OK (:compile-result t :compiled t :mode-hook-counts (0 0 0) :autoload-file "deploy-autoloads.el" :autoload-present t :loaddefs "deploy-autoloads.el")"#
    ]];
    ParityBatchCase::value(
        "byte_compile_and_autoload_workflow_produces_loadable_package_artifacts_without_mode_hooks",
        elisp_form,
        expected,
    )
}

#[test]
fn packed_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(PACKED_MELPA_PIN, "packed.el")
            .expect("prepare revision-pinned Packed source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "packed-package-batch",
        "Packed",
        &[
            package_tree_discovery_keeps_real_libraries_and_ignores_non_package_content(),
            main_library_inference_handles_mode_packages_singletons_and_broken_features(),
            source_metadata_parser_excludes_comments_and_strings_and_preserves_optional_requires(),
            load_path_management_adds_only_library_directories_and_restores_the_environment(),
            source_suffix_and_lookup_helpers_prefer_editable_lisp_over_compiled_artifacts(),
            byte_compile_and_autoload_workflow_produces_loadable_package_artifacts_without_mode_hooks(),
        ],
    );
}
