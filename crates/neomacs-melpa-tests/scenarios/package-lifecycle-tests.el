;;; package-lifecycle-tests.el --- Package lifecycle contracts  -*- lexical-binding: t; -*-

(load
 (expand-file-name "test/lisp/emacs-lisp/package-tests.el"
                   (getenv "NEOMACS_RUNTIME_ROOT"))
 nil nil t)

(ert-deftest neomacs-package-autoremove-removes-unused-dependencies ()
  (with-package-test ()
    (package-initialize)
    (package-refresh-contents)
    (package-install 'simple-depend)
    (should (package-installed-p 'simple-depend))
    (should (package-installed-p 'simple-single))
    (package-delete (cadr (assq 'simple-depend package-alist)))
    (should-not (package-installed-p 'simple-depend))
    (should (package-installed-p 'simple-single))
    (package-autoremove t)
    (should-not (package-installed-p 'simple-single))))

(ert-deftest neomacs-package-rejects-incompatible-emacs-requirement ()
  (with-package-test
      (:location
       (expand-file-name
        "crates/neomacs-melpa-tests/fixtures/lifecycle-archive"
        (getenv "NEOMACS_RUNTIME_ROOT")))
    (package-initialize)
    (package-refresh-contents)
    (should-error (package-install 'future-only))
    (should-not (package-installed-p 'future-only))
    (should-not (fboundp 'future-only-command))))

(provide 'package-lifecycle-tests)

;;; package-lifecycle-tests.el ends here
