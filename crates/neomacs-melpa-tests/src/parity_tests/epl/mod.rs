use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EPL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EPL_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const EPL_TEST_PRELUDE: &str = r##"
(defun epl-test-root (name)
  "Return a clean case directory NAME inside the oracle sandbox."
  (let ((root
         (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun epl-test-package-snapshot (package)
  "Return PACKAGE's stable public EPL metadata."
  (list :name (epl-package-name package)
        :version (epl-package-version package)
        :version-string (epl-package-version-string package)
        :summary (epl-package-summary package)
        :requirements
        (mapcar
         (lambda (requirement)
           (list (epl-requirement-name requirement)
                 (epl-requirement-version requirement)
                 (epl-requirement-version-string requirement)))
         (epl-package-requirements package))))

(defun epl-test-desc (name version summary directory &optional requirements)
  "Construct a package.el descriptor for a realistic EPL database entry."
  (package-desc-create
   :name name
   :version (version-to-list version)
   :summary summary
   :reqs requirements
   :kind 'single
   :archive "stable"
   :dir directory))
"##;

fn epl_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EPL_MELPA_PIN, "epl.el")
        .expect("prepare pinned EPL source below ./tmp")
        .with_prelude(EPL_TEST_PRELUDE)
        .with_timeout(EPL_TEST_TIMEOUT)
}

fn package_source_metadata_drives_dependency_and_validation_workflows() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_source_metadata_drives_dependency_and_validation_workflows",
        r##"
(let* ((root (epl-test-root "epl-source-metadata"))
       (source (expand-file-name "checkout-tools.el" root))
       (invalid (expand-file-name "broken-package.el" root)))
  (with-temp-file source
    (insert
     ";;; checkout-tools.el --- Checkout release helpers  -*- lexical-binding: t; -*-\n"
     ";; Version: 2.4.1-beta\n"
     ";; Package-Requires: ((emacs \"27.1\") (cl-lib \"0.6\"))\n"
     ";; Keywords: tools\n"
     ";;; Commentary:\n;; Release helpers used by the checkout team.\n"
     ";;; Code:\n(provide 'checkout-tools)\n"
     ";;; checkout-tools.el ends here\n"))
  (with-temp-file invalid
    (insert
     ";;; broken-package.el --- Missing version metadata\n"
     ";;; Code:\n(provide 'broken-package)\n"))
  (let* ((package (epl-package-from-file source))
         (invalid-result
          (condition-case err
              (progn (epl-package-from-file invalid) 'unexpected-success)
            (error
             (list (car err)
                   (file-name-nondirectory (cadr err))
                   (and (string-match-p "Version" (error-message-string err))
                        t))))))
    (list :metadata (epl-test-package-snapshot package)
          :directory (epl-package-directory package)
          :installed (epl-package-installed-p package)
          :invalid invalid-result)))
"##,
        expect![[
            r##"OK (:metadata (:name checkout-tools :version (2 4 1 -2) :version-string "2.4.1beta" :summary "Checkout release helpers" :requirements ((emacs (27 1) "27.1") (cl-lib (0 6) "0.6"))) :directory nil :installed nil :invalid (epl-invalid-package-file "broken-package.el" t))"##
        ]],
    )
}

fn descriptor_file_is_parsed_without_mutating_the_package_database() -> ParityBatchCase {
    ParityBatchCase::value(
        "descriptor_file_is_parsed_without_mutating_the_package_database",
        r##"
(let* ((root (epl-test-root "epl-descriptor"))
       (descriptor (expand-file-name "checkout-tools-pkg.el" root))
       (invalid (expand-file-name "invalid-pkg.el" root))
       (package-alist '((already-installed . sentinel))))
  (with-temp-file descriptor
    (insert
     "(define-package \"checkout-tools\" \"3.7.0-rc1\"\n"
     "  \"Checkout deployment workflow\"\n"
     "  '((emacs \"27.1\") (transient \"0.6.0\")))\n"))
  (with-temp-file invalid
    (insert "(checkout-package \"broken\" \"1.0\")\n"))
  (let* ((before (copy-tree package-alist))
         (package (epl-package-from-descriptor-file descriptor))
         (invalid-result
          (condition-case err
              (progn
                (epl-package-from-descriptor-file invalid)
                'unexpected-success)
            (error
             (list (car err)
                   (and (string-match-p "no valid package descriptor"
                                        (error-message-string err))
                        t))))))
    (list :package (epl-test-package-snapshot package)
          :database-unchanged (equal package-alist before)
          :database package-alist
          :invalid invalid-result)))
"##,
        expect![[
            r##"OK (:package (:name checkout-tools :version (3 7 0 -1 1) :version-string "3.7.0pre1" :summary "Checkout deployment workflow" :requirements ((emacs (27 1) "27.1") (transient (0 6 0) "0.6.0"))) :database-unchanged t :database ((already-installed . sentinel)) :invalid (error t))"##
        ]],
    )
}

fn installed_and_available_databases_select_versions_and_plan_upgrades() -> ParityBatchCase {
    ParityBatchCase::value(
        "installed_and_available_databases_select_versions_and_plan_upgrades",
        r##"
(let* ((root (epl-test-root "epl-package-database"))
       (checkout-v1-dir (expand-file-name "checkout-tools-1.8.0" root))
       (checkout-v2-dir (expand-file-name "checkout-tools-2.0.0" root))
       (shipping-dir (expand-file-name "shipping-mode-1.5.0" root)))
  (dolist (directory (list checkout-v1-dir checkout-v2-dir shipping-dir))
    (make-directory directory t))
  (let* ((checkout-v1
          (epl-test-desc 'checkout-tools "1.8.0" "Legacy checkout" checkout-v1-dir))
         (checkout-v2
          (epl-test-desc
           'checkout-tools "2.0.0" "Current checkout" checkout-v2-dir
           '((transient (0 5 0)))))
         (checkout-v21
          (epl-test-desc
           'checkout-tools "2.1.0" "Candidate checkout" nil
           '((transient (0 6 0)))))
         (checkout-v3
          (epl-test-desc
           'checkout-tools "3.0.0" "Next checkout" nil
           '((transient (0 7 0)))))
         (shipping
          (epl-test-desc 'shipping-mode "1.5.0" "Shipping tools" shipping-dir))
         (shipping-available
          (epl-test-desc 'shipping-mode "1.5.0" "Shipping tools" nil))
         (package-alist
          `((shipping-mode ,shipping)
            (checkout-tools ,checkout-v2 ,checkout-v1)))
         (package-archive-contents
          `((checkout-tools ,checkout-v3 ,checkout-v21)
            (shipping-mode ,shipping-available)))
         (installed (epl-installed-packages))
         (checkout-installed (epl-find-installed-packages 'checkout-tools))
         (checkout-available (epl-find-available-packages 'checkout-tools))
         (upgrades (epl-find-upgrades (list (car checkout-installed)))))
    (list
     :installed
     (mapcar (lambda (package)
               (list (epl-package-name package)
                     (epl-package-version-string package)
                     (epl-package-summary package)))
             installed)
     :checkout-installed
     (mapcar #'epl-package-version-string checkout-installed)
     :checkout-available
     (mapcar #'epl-package-version-string checkout-available)
     :minimums
     (mapcar (lambda (version)
               (list version
                     (and (epl-package-installed-p
                           'checkout-tools (version-to-list version))
                          t)))
             '("1.0" "2.0" "2.0.1" "3.0"))
     :outdated
     (list (epl-package-outdated-p 'checkout-tools)
           (epl-package-outdated-p (car checkout-installed))
           (epl-package-outdated-p 'shipping-mode)
           (mapcar (lambda (package)
                     (list (epl-package-name package)
                           (epl-package-version-string package)))
                   (epl-outdated-packages)))
     :upgrades
     (mapcar
      (lambda (upgrade)
        (list (epl-package-name (epl-upgrade-installed upgrade))
              (epl-package-version-string (epl-upgrade-installed upgrade))
              (epl-package-version-string (epl-upgrade-available upgrade))))
      upgrades))))
"##,
        expect![[
            r##"OK (:installed ((shipping-mode "1.5.0" "Shipping tools") (checkout-tools "2.0.0" "Current checkout") (checkout-tools "1.8.0" "Legacy checkout")) :checkout-installed ("2.0.0" "1.8.0") :checkout-available ("3.0.0" "2.1.0") :minimums (("1.0" t) ("2.0" t) ("2.0.1" nil) ("3.0" nil)) :outdated (t t nil ((checkout-tools "2.0.0") (checkout-tools "1.8.0"))) :upgrades ((checkout-tools "2.0.0" "3.0.0")))"##
        ]],
    )
}

fn local_package_install_load_query_and_delete_complete_a_real_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "local_package_install_load_query_and_delete_complete_a_real_lifecycle",
        r##"
(let* ((root (epl-test-root "epl-install-lifecycle"))
       (source (expand-file-name "checkout-helper.el" root))
       (package-user-dir (expand-file-name "packages" root))
       (package-directory-list nil)
       (package-alist nil)
       (package-archive-contents nil)
       (package-archives nil)
       (package-selected-packages nil)
       (package-check-signature nil)
       (package-native-compile nil)
       (native-comp-jit-compilation nil)
       (load-path (copy-sequence load-path)))
  (make-directory package-user-dir t)
  (with-temp-file source
    (insert
     ";;; checkout-helper.el --- Checkout price calculations  -*- lexical-binding: t; -*-\n"
     ";; Version: 1.2.3\n"
     ";; Package-Requires: ((emacs \"27.1\"))\n"
     ";;; Commentary:\n;; Deterministic checkout calculations.\n"
     ";;; Code:\n"
     "(defun checkout-helper-total (subtotal discount shipping)\n"
     "  (+ (- subtotal discount) shipping))\n"
     "(provide 'checkout-helper)\n"
     ";;; checkout-helper.el ends here\n"))
  (epl-install-file source)
  (let* ((packages (epl-find-installed-packages 'checkout-helper))
         (package (car packages))
         (directory (epl-package-directory package))
         (installed-files
          (sort (directory-files directory nil "\\`[^.]") #'string<)))
    (require 'checkout-helper)
    (let ((installed
           (list :count (length packages)
                 :metadata (epl-test-package-snapshot package)
                 :directory (file-name-nondirectory
                             (directory-file-name directory))
                 :files installed-files
                 :calculation (checkout-helper-total 12900 1500 799)
                 :queries
                 (mapcar
                  (lambda (version)
                    (list version
                          (and (epl-package-installed-p
                                'checkout-helper (version-to-list version))
                               t)))
                  '("1.0" "1.2.3" "1.2.4"))
                 :object-installed (and (epl-package-installed-p package) t))))
      (epl-package-delete package)
      (list :installed installed
            :deleted
            (list :directory-exists (file-exists-p directory)
                  :database (epl-find-installed-packages 'checkout-helper)
                  :symbol-installed (epl-package-installed-p 'checkout-helper)
                  :object-installed (epl-package-installed-p package)
                  :feature-still-loaded (featurep 'checkout-helper))))))
"##,
        expect![[
            r##"OK (:installed (:count 1 :metadata (:name checkout-helper :version (1 2 3) :version-string "1.2.3" :summary "Checkout price calculations" :requirements ((emacs (27 1) "27.1"))) :directory "checkout-helper-1.2.3" :files ("checkout-helper-autoloads.el" "checkout-helper-pkg.el" "checkout-helper.el" "checkout-helper.elc") :calculation 12199 :queries (("1.0" t) ("1.2.3" t) ("1.2.4" nil)) :object-installed t) :deleted (:directory-exists nil :database nil :symbol-installed nil :object-installed nil :feature-still-loaded t))"##
        ]],
    )
}

fn built_in_discovery_resolves_real_editor_libraries_and_version_queries() -> ParityBatchCase {
    ParityBatchCase::value(
        "built_in_discovery_resolves_real_editor_libraries_and_version_queries",
        r##"
(let* ((built-ins (epl-built-in-packages))
       (built-in-names (mapcar #'epl-package-name built-ins))
       (selected
        (delq
         nil
         (mapcar
          (lambda (name)
            (let ((package (epl-find-built-in-package name)))
              (when package
                (list :package (epl-test-package-snapshot package)
                      :listed (and (memq name built-in-names) t)
                      :directory (epl-package-directory package)
                      :built-in (and (epl-built-in-p name) t)
                      :installed-by-name
                      (and (epl-package-installed-p name) t)
                      :installed-as-object
                      (and (epl-package-installed-p package) t)))))
          '(cl-lib seq project package)))))
  (list :selected selected
        :selected-names-unique
        (= (length selected)
           (length (delete-dups (mapcar
                                 (lambda (entry)
                                   (plist-get (plist-get entry :package) :name))
                                 selected))))
        :cl-lib-minimums
        (mapcar
         (lambda (version)
           (list version
                 (and (epl-package-installed-p
                       'cl-lib (version-to-list version))
                      t)))
         '("0.1" "1.0" "999.0"))
        :missing (epl-find-built-in-package 'not-a-real-editor-library)))
"##,
        expect![[
            r##"OK (:selected ((:package (:name cl-lib :version (1 0) :version-string "1.0" :summary "Common Lisp extensions for Emacs" :requirements nil) :listed t :directory builtin :built-in t :installed-by-name t :installed-as-object t) (:package (:name seq :version (2 24) :version-string "2.24" :summary "Sequence manipulation functions" :requirements nil) :listed t :directory builtin :built-in t :installed-by-name t :installed-as-object t) (:package (:name project :version (0 11 2) :version-string "0.11.2" :summary "Operations on the current project" :requirements nil) :listed t :directory builtin :built-in t :installed-by-name t :installed-as-object t) (:package (:name package :version (1 1 0) :version-string "1.1.0" :summary "Simple package system for Emacs" :requirements nil) :listed t :directory builtin :built-in t :installed-by-name t :installed-as-object t)) :selected-names-unique t :cl-lib-minimums (("0.1" t) ("1.0" t) ("999.0" nil)) :missing nil)"##
        ]],
    )
}

#[test]
fn epl_package_batch() {
    let cases = vec![
        package_source_metadata_drives_dependency_and_validation_workflows(),
        descriptor_file_is_parsed_without_mutating_the_package_database(),
        installed_and_available_databases_select_versions_and_plan_upgrades(),
        local_package_install_load_query_and_delete_complete_a_real_lifecycle(),
        built_in_discovery_resolves_real_editor_libraries_and_version_queries(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed EPL parity test");
    assert_oracle_batch_cases(epl_oracle(), test_name, "epl_parity", &cases);
}
