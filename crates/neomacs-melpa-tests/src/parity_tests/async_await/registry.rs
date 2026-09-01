use expect_test::expect;

use super::ParityBatchCase;

fn package_descriptor_records_exact_pin_dependencies_and_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_descriptor_records_exact_pin_dependencies_and_payload",
        r##"(let* ((desc
                 (cadr
                  (assq 'async-await package-alist)))
                (dir (package-desc-dir desc)))
          (list
           (package-version-join
            (package-desc-version desc))
           (package-desc-reqs desc)
           (sort
            (mapcar #'file-name-nondirectory
                    (directory-files dir t "^[^.].*"))
            #'string<)))"##,
        expect![[
            r#"OK ("20220827.437" ((emacs (25 1)) (promise (1 1)) (iter2 (0 9 10))) ("async-await-autoloads.el" "async-await-pkg.el" "async-await.el" "async-await.elc"))"#
        ]],
    )
}

fn installed_source_has_exact_hash_features_and_dependency_versions() -> ParityBatchCase {
    ParityBatchCase::value(
        "installed_source_has_exact_hash_features_and_dependency_versions",
        r##"(let* ((desc
                 (cadr
                  (assq 'async-await package-alist)))
                (dir (package-desc-dir desc))
                (source
                 (expand-file-name
                  "async-await.el" dir)))
          (list
           ;; `secure-hash' on a filename hashes the *string*, not the
           ;; file, so this pinned the installed path -- laundered through
           ;; a digest, where no amount of grepping for the cache
           ;; directory would find it.  It broke when the cache moved from
           ;; package-cache/ to the revision-pinned source-install-cache/,
           ;; and re-capturing would have re-pinned the new path just as
           ;; invisibly.  Hash the contents, which is what this test's
           ;; name claims and what survives a layout change.
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (insert-file-contents-literally source)
             (secure-hash 'sha256 (current-buffer)))
           (featurep 'async-await)
           (featurep 'promise)
           (featurep 'iter2)
           (mapcar
            (lambda (package)
              (let ((installed
                     (cadr
                      (assq package package-alist))))
                (list
                 package
                 (package-version-join
                  (package-desc-version installed)))))
            '(async-await promise iter2))))"##,
        expect![[
            r#"OK ("85797e62ef3e734a5d92c65cd0a4379dd4f07588a3abfc40221aa6fd0ae1d3d6" t t t ((async-await "20220827.437") (promise "20210307.727") (iter2 "20250209.1516")))"#
        ]],
    )
}

fn complete_declared_callable_surface_has_exact_kinds_arities_and_docs() -> ParityBatchCase {
    ParityBatchCase::value(
        "complete_declared_callable_surface_has_exact_kinds_arities_and_docs",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (macrop symbol)
             (commandp symbol)
             (help-function-arglist symbol t)
             (secure-hash
              'sha256
              (or (documentation symbol t) ""))))
          '(async-await--iter-throw
            async-await--awaiter
            async-await--check-return-value
            async-defun
            async-lambda
            async-await-advice-make-autoload))"##,
        expect![[
            r#"OK ((async-await--iter-throw t nil nil (iterator value) "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") (async-await--awaiter t nil nil (iterator) "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") (async-await--check-return-value t nil nil (value) "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") (async-defun t t nil (name arglist &rest body) "ee8bc677c4c77477c2c7aca62b7ad18ca0e7e85635b917bc499bdf7d302cc6d3") (async-lambda t t nil (arglist &rest body) "57495079dedc09b3eff5d831592053380821dfdc3ef3c499d9cba2699bd46522") (async-await-advice-make-autoload t nil nil (fn &rest args) "577695d7ac683e087ec4b6797940e6e77e4585d14106a6f143297511cb1262cd"))"#
        ]],
    )
}

fn complete_declared_variable_surface_has_exact_values_and_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "complete_declared_variable_surface_has_exact_values_and_properties",
        r##"(list
          (list
           'async-await--is-error
           (boundp 'async-await--is-error)
           (symbolp async-await--is-error)
           (string-prefix-p
            "async/await--error"
            (symbol-name async-await--is-error))
           (eq async-await--is-error
               async-await--is-error))
          (list
           'async-await-font-lock-keywords
           (boundp 'async-await-font-lock-keywords)
           async-await-font-lock-keywords)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (get symbol 'doc-string-elt)
              (get symbol 'lisp-indent-function)
              (get symbol 'edebug-form-spec)))
           '(async-defun async-lambda)))"##,
        expect![[
            r#"OK ((async-await--is-error t t t t) (async-await-font-lock-keywords t (("(\\(async-defun\\)\\_>[ \11']*\\(\\(?:\\sw\\|\\s_\\)+\\)?" (1 font-lock-keyword-face) (2 font-lock-function-name-face nil t)))) ((async-defun 3 2 nil) (async-lambda 2 defun nil)))"#
        ]],
    )
}

fn loading_source_registers_advice_font_lock_and_imenu_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_source_registers_advice_font_lock_and_imenu_once",
        r##"(let ((advice-count 0)
              (font-lock-count 0)
              (imenu-count 0)
              (imenu-entry
               (list
                nil
                (concat
                 "^\\s-*(async-defun\\s-+\\("
                 lisp-mode-symbol-regexp
                 "\\)")
                1)))
          (advice-mapc
           (lambda (advice _props)
             (when
                 (eq advice
                     #'async-await-advice-make-autoload)
               (setq advice-count
                     (1+ advice-count))))
           'make-autoload)
          (dolist
              (entry
               (cdr
                (assq
                 'emacs-lisp-mode
                 font-lock-keywords-alist)))
            (when
                (equal
                 (car-safe entry)
                 async-await-font-lock-keywords)
              (setq font-lock-count
                    (1+ font-lock-count))))
          (dolist (entry lisp-imenu-generic-expression)
            (when (equal entry imenu-entry)
              (setq imenu-count
                    (1+ imenu-count))))
          (list
           advice-count
           font-lock-count
           imenu-count
           (not
            (null
             (advice-member-p
              #'async-await-advice-make-autoload
              'make-autoload)))))"##,
        expect!["OK (1 1 1 t)"],
    )
}

fn repeated_source_loading_accumulates_font_lock_specs_but_not_advice_or_imenu() -> ParityBatchCase
{
    ParityBatchCase::value(
        "repeated_source_loading_accumulates_font_lock_specs_but_not_advice_or_imenu",
        r##"(let* ((desc
                  (cadr
                   (assq 'async-await package-alist)))
                 (source
                  (expand-file-name
                   "async-await.el"
                   (package-desc-dir desc))))
          (dotimes (_ 3)
            (load source nil t))
          (let ((advice-count 0)
                (font-lock-count 0)
                (imenu-count 0))
            (advice-mapc
             (lambda (advice _props)
               (when
                   (eq advice
                       #'async-await-advice-make-autoload)
                 (setq advice-count
                       (1+ advice-count))))
             'make-autoload)
            (dolist
                (entry
                 (cdr
                  (assq
                   'emacs-lisp-mode
                   font-lock-keywords-alist)))
              (when
                  (equal
                   (car-safe entry)
                   async-await-font-lock-keywords)
                (setq font-lock-count
                      (1+ font-lock-count))))
            (let ((regexp
                   (concat
                    "^\\s-*(async-defun\\s-+\\("
                    lisp-mode-symbol-regexp
                    "\\)")))
              (dolist
                  (entry
                   lisp-imenu-generic-expression)
                (when
                    (equal
                     entry
                     (list nil regexp 1))
                  (setq imenu-count
                        (1+ imenu-count)))))
            (list
             (featurep 'async-await)
             advice-count
             font-lock-count
             imenu-count)))"##,
        expect!["OK (t 1 4 1)"],
    )
}

fn make_autoload_advice_expands_async_defun_and_delegates_other_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "make_autoload_advice_expands_async_defun_and_delegates_other_forms",
        r##"(let* ((lexical-binding t)
                 (async-result
                  (make-autoload
                   '(async-defun generated-async (value)
                      "Generated."
                      (await value))
                   "fixture.el"))
                 (delegated
                  (async-await-advice-make-autoload
                   (lambda (&rest args)
                     (cons :delegated args))
                   '(defun ordinary (x) x)
                   "ordinary.el"
                   nil)))
          (list
           (car async-result)
           (cadr async-result)
           (nth 2 async-result)
           (nth 4 async-result)
           delegated))"##,
        expect![[
            r#"OK (autoload 'generated-async "fixture.el" nil (:delegated (defun ordinary (x) x) "ordinary.el" nil))"#
        ]],
    )
}

fn font_lock_and_imenu_recognize_real_async_definitions_without_false_positives() -> ParityBatchCase
{
    ParityBatchCase::value(
        "font_lock_and_imenu_recognize_real_async_definitions_without_false_positives",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(async-defun fetch-value (x)\\n"
           "  (await x))\\n"
           "(defun ordinary-value () nil)\\n"
           ";; async-defun comment-only\\n")
          (font-lock-ensure)
          (let ((faces
                 (mapcar
                  (lambda (needle)
                    (goto-char (point-min))
                    (search-forward needle)
                    (list
                     needle
                     (get-text-property
                      (- (point)
                         (length needle))
                      'face)))
                  '("async-defun"
                    "fetch-value"
                    "ordinary-value"
                    "comment-only"))))
            (list
             faces
             (mapcar #'car
                     (imenu--make-index-alist t)))))"##,
        expect![[
            r#"OK ((("async-defun" font-lock-keyword-face) ("fetch-value" font-lock-function-name-face) ("ordinary-value" font-lock-function-name-face) ("comment-only" font-lock-comment-face)) ("*Rescan*" "fetch-value"))"#
        ]],
    )
}

fn autoload_file_exposes_macros_advice_and_exact_source_ownership() -> ParityBatchCase {
    ParityBatchCase::value(
        "autoload_file_exposes_macros_advice_and_exact_source_ownership",
        r##"(list
          (featurep 'async-await)
          (featurep 'async-await-autoloads)
          (mapcar
           (lambda (symbol)
             (let ((definition
                    (symbol-function symbol)))
               (list
                symbol
                (autoloadp definition)
                (macrop symbol)
                (nth 1 definition)
                (nth 4 definition)
                (get symbol 'doc-string-elt))))
           '(async-defun async-lambda))
          (let ((definition
                 (symbol-function
                  'async-await-advice-make-autoload)))
            (list
             (autoloadp definition)
             (nth 1 definition)))
          (not
           (null
            (advice-member-p
             #'async-await-advice-make-autoload
             'make-autoload))))"##,
        expect![[
            r#"OK (nil t ((async-defun t #1=(t) "async-await" t 3) (async-lambda t #1# "async-await" t 2)) (t "async-await") t)"#
        ]],
    )
}

pub(super) fn registry_async_await_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_descriptor_records_exact_pin_dependencies_and_payload(),
        installed_source_has_exact_hash_features_and_dependency_versions(),
        complete_declared_callable_surface_has_exact_kinds_arities_and_docs(),
        complete_declared_variable_surface_has_exact_values_and_properties(),
        loading_source_registers_advice_font_lock_and_imenu_once(),
        repeated_source_loading_accumulates_font_lock_specs_but_not_advice_or_imenu(),
        make_autoload_advice_expands_async_defun_and_delegates_other_forms(),
        font_lock_and_imenu_recognize_real_async_definitions_without_false_positives(),
    ]
}

pub(super) fn registry_async_await_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![autoload_file_exposes_macros_advice_and_exact_source_ownership()]
}
