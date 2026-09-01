;;; build-package-from-source.el --- Build one locked package  -*- lexical-binding: t; -*-

(let ((package-build-dir (getenv "NEOMACS_PACKAGE_BUILD_DIR"))
      (recipes-dir (getenv "NEOMACS_PACKAGE_RECIPES_DIR"))
      (build-root (getenv "NEOMACS_PACKAGE_BUILD_ROOT"))
      (package-name (getenv "NEOMACS_PACKAGE_NAME"))
      (expected-version (getenv "NEOMACS_PACKAGE_VERSION"))
      (expected-revision (getenv "NEOMACS_PACKAGE_REVISION"))
      (commit-time (string-to-number
                    (getenv "NEOMACS_PACKAGE_COMMIT_TIME"))))
  (add-to-list 'load-path package-build-dir)
  (require 'package-build)
  (setq package-build-directory build-root
        package-build-archive-dir (expand-file-name "packages" build-root)
        package-build-recipes-dir recipes-dir
        package-build-working-dir (expand-file-name "working" build-root)
        package-build-releases nil
        package-build-build-function 'package-build--build-multi-file-package
        package-build-snapshot-version-functions
        '(package-build-timestamp-version)
        package-build-badge-data nil
        package-build--inhibit-fetch 'strict)
  (make-directory package-build-archive-dir t)
  (let ((recipe (package-recipe-lookup package-name)))
    ;; The source lock is authoritative.  Do not ask a branch, tag, current
    ;; catalog, or the current repository head which revision/version to use.
    (oset recipe commit expected-revision)
    (oset recipe time commit-time)
    (oset recipe version expected-version)
    (oset recipe revdesc (substring expected-revision 0 12))
    (package-build--package recipe)
    (princ (format "NEOMACS-SOURCE-PACKAGE:ready:%s:%s\n"
                   package-name expected-version))))

;;; build-package-from-source.el ends here
