use expect_test::expect;

use super::ParityBatchCase;

fn auto_read_only_default_regexps_match_exact_extension_and_directory_boundaries() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_read_only_default_regexps_match_exact_extension_and_directory_boundaries",
        r##"(let ((quoted-user-directory
                (regexp-quote
                 (file-name-as-directory
                  (expand-file-name
                   user-emacs-directory)))))
         (list
          (mapcar
           (lambda (regexp)
             (replace-regexp-in-string
              (regexp-quote
               quoted-user-directory)
              "[USER-EMACS-DIRECTORY]/"
              regexp
              t
              t))
           auto-read-only-file-regexps)
          (mapcar
           (lambda (file)
             (list
              file
              (and
               (seq-some
                (lambda (regexp)
                  (string-match-p regexp file))
                auto-read-only-file-regexps)
               t)))
           (list
            "/workspace/cache/module.elc"
            "/workspace/cache/module.pyc"
            "/workspace/cache/module.elc.gz"
            "/workspace/cache/MODULE.ELC"
            "/opt/share/emacs/site-lisp/library.el"
            "/opt/share/site-lisp/library.el"
            (expand-file-name
             "elpa/pkg/pkg.el"
             user-emacs-directory)
            (expand-file-name
             "el-get/pkg/pkg.el"
             user-emacs-directory)
            (expand-file-name
             "packages/pkg.el"
             user-emacs-directory)
            "/workspace/.bundle/ruby/tool.rb"
            "/workspace/.cask/29.1/elpa/pkg.el"
            "/workspace/.casket/pkg.el"
            "/workspace/vendor/pkg.el"))))"##,
        expect![[
            r#"OK (("\\(?:\\.\\(?:\\(?:el\\|py\\)c\\)\\)\\'" "/share/.+/site-lisp/" "[USER-EMACS-DIRECTORY]/\\(?:el\\(?:-get\\|pa\\)\\)/" "/\\(?:\\.\\(?:bundle\\|cask\\)\\)/") (("/workspace/cache/module.elc" t) ("/workspace/cache/module.pyc" t) ("/workspace/cache/module.elc.gz" nil) ("/workspace/cache/MODULE.ELC" t) ("/opt/share/emacs/site-lisp/library.el" t) ("/opt/share/site-lisp/library.el" nil) ("[ORACLE-HOME]/.emacs.d/elpa/pkg/pkg.el" t) ("[ORACLE-HOME]/.emacs.d/el-get/pkg/pkg.el" t) ("[ORACLE-HOME]/.emacs.d/packages/pkg.el" nil) ("/workspace/.bundle/ruby/tool.rb" t) ("/workspace/.cask/29.1/elpa/pkg.el" t) ("/workspace/.casket/pkg.el" nil) ("/workspace/vendor/pkg.el" nil)))"#
        ]],
    )
}

fn auto_read_only_without_filename_or_without_match_is_a_strict_noop() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_without_filename_or_without_match_is_a_strict_noop",
        r##"(mapcar
         (lambda (file)
           (with-temp-buffer
             (insert "editable")
             (set-buffer-modified-p nil)
             (setq buffer-file-name file)
             (let ((auto-read-only-file-regexps
                    '("/vendor/" "\\.elc\\'"))
                   (auto-read-only-function
                    (lambda ()
                      (error
                       "unexpected action"))))
               (list
                file
                (auto-read-only)
                (auto-read-only-test-buffer-state)))))
         '(nil
           "/workspace/src/main.el"
           "/workspace/vendorish/main.el"
           "/workspace/output.elc.gz"))"##,
        expect![[
            r#"OK ((nil nil (" *temp*" nil "editable" 9 nil nil nil)) ("/workspace/src/main.el" nil (" *temp*" "/workspace/src/main.el" "editable" 9 nil nil nil)) ("/workspace/vendorish/main.el" nil (" *temp*" "/workspace/vendorish/main.el" "editable" 9 nil nil nil)) ("/workspace/output.elc.gz" nil (" *temp*" "/workspace/output.elc.gz" "editable" 9 nil nil nil)))"#
        ]],
    )
}

fn auto_read_only_default_action_enters_real_view_mode_and_returns_its_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_default_action_enters_real_view_mode_and_returns_its_value",
        r##"(with-temp-buffer
         (insert "compiled payload")
         (set-buffer-modified-p nil)
         (setq buffer-file-name
               "/workspace/build/module.elc")
         (let ((auto-read-only-file-regexps
                '("\\.elc\\'"))
               (auto-read-only-function nil))
           (list
            (auto-read-only)
            (auto-read-only-test-buffer-state)
            (condition-case error-data
                (progn
                  (goto-char (point-max))
                  (insert "!"))
              (error
               (list
                (car error-data)
                (cdr error-data))))
            (buffer-string))))"##,
        expect![[
            r#"OK (t (" *temp*" "/workspace/build/module.elc" "compiled payload" 17 t t nil) (buffer-read-only ((:buffer nil))) "compiled payload")"#
        ]],
    )
}

fn auto_read_only_custom_action_runs_once_in_original_buffer_with_exact_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_custom_action_runs_once_in_original_buffer_with_exact_state",
        r##"(with-temp-buffer
         (rename-buffer
          " *auto-read-only-action*")
         (insert "third-party source")
         (goto-char 7)
         (set-buffer-modified-p nil)
         (setq buffer-file-name
               "/workspace/vendor/library.el")
         (let* ((auto-read-only-file-regexps
                 '("/vendor/"))
                calls
                (auto-read-only-function
                 (lambda ()
                   (push
                    (auto-read-only-test-buffer-state)
                    calls)
                   :protected)))
           (list
            (auto-read-only)
            (nreverse calls)
            (auto-read-only-test-buffer-state))))"##,
        expect![[
            r#"OK (:protected ((" *auto-read-only-action*" "/workspace/vendor/library.el" "third-party source" 7 nil nil nil)) (" *auto-read-only-action*" "/workspace/vendor/library.el" "third-party source" 7 nil nil nil))"#
        ]],
    )
}

fn auto_read_only_stops_at_first_match_and_never_evaluates_later_invalid_regexp() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_read_only_stops_at_first_match_and_never_evaluates_later_invalid_regexp",
        r##"(with-temp-buffer
         (setq buffer-file-name
               "/workspace/vendor/library.el")
         (let* ((auto-read-only-file-regexps
                 '("/vendor/" "["))
                calls
                (auto-read-only-function
                 (lambda ()
                   (push :action calls)
                   :done)))
           (list
            (auto-read-only-test-error-data
             #'auto-read-only)
            (nreverse calls))))"##,
        expect!["OK ((:ok :done) (:action))"],
    )
}

fn auto_read_only_reaches_and_propagates_invalid_later_regexp_after_misses() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_reaches_and_propagates_invalid_later_regexp_after_misses",
        r##"(with-temp-buffer
         (setq buffer-file-name
               "/workspace/src/library.el")
         (let ((auto-read-only-file-regexps
                '("/vendor/" "["))
               (auto-read-only-function
                (lambda ()
                  :unexpected)))
           (list
            (auto-read-only-test-error-data
             #'auto-read-only)
            buffer-read-only
            (bound-and-true-p view-mode))))"##,
        expect![[r#"OK ((:error invalid-regexp ("Unmatched [ or [^")) nil nil)"#]],
    )
}

fn auto_read_only_preserves_preexisting_match_data_for_match_and_miss_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_preserves_preexisting_match_data_for_match_and_miss_paths",
        r##"(progn
         (string-match
          "\\(seed\\)-\\([0-9]+\\)"
          "seed-42")
         (let ((before
                (match-data))
               outcomes)
           (dolist
               (file
                '("/workspace/vendor/pkg.el"
                  "/workspace/src/pkg.el"))
             (with-temp-buffer
               (setq buffer-file-name file)
               (let ((auto-read-only-file-regexps
                      '("/vendor/"))
                     (auto-read-only-function
                      #'ignore))
                 (push
                  (list
                   file
                   (auto-read-only)
                   (match-data)
                   (equal
                    before
                    (match-data)))
                  outcomes))))
           (list before
                 (nreverse outcomes))))"##,
        expect![[
            r#"OK ((0 7 0 4 5 7) (("/workspace/vendor/pkg.el" nil (0 7 0 4 5 7) t) ("/workspace/src/pkg.el" nil (0 7 0 4 5 7) t)))"#
        ]],
    )
}

fn auto_read_only_matcher_uses_one_filename_and_stops_calling_after_success() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_matcher_uses_one_filename_and_stops_calling_after_success",
        r##"(with-temp-buffer
         (setq buffer-file-name
               "/workspace/vendor/pkg.el")
         (let ((auto-read-only-file-regexps
                '("first" "second" "third"))
               calls
               (auto-read-only-function
                (lambda ()
                  :applied)))
           (cl-letf
               (((symbol-function
                  'string-match-p)
                 (lambda (regexp string)
                   (push
                    (list regexp string)
                    calls)
                   (equal regexp "second"))))
             (list
              (auto-read-only)
              (nreverse calls)))))"##,
        expect![[
            r#"OK (:applied (("first" "/workspace/vendor/pkg.el") ("second" "/workspace/vendor/pkg.el")))"#
        ]],
    )
}

fn auto_read_only_buffer_local_patterns_and_actions_are_isolated_in_practical_use()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_buffer_local_patterns_and_actions_are_isolated_in_practical_use",
        r##"(let ((first
                (generate-new-buffer
                 " *auto-read-only-first*"))
               (second
                (generate-new-buffer
                 " *auto-read-only-second*"))
               events)
         (unwind-protect
             (progn
               (with-current-buffer first
                 (setq buffer-file-name
                       "/workspace/vendor/first.el")
                 (setq-local
                  auto-read-only-file-regexps
                  '("/vendor/"))
                 (setq-local
                  auto-read-only-function
                  (lambda ()
                    (push
                     (list :first
                           (current-buffer))
                     events)
                    :first-result)))
               (with-current-buffer second
                 (setq buffer-file-name
                       "/workspace/generated/second.el")
                 (setq-local
                  auto-read-only-file-regexps
                  '("/generated/"))
                 (setq-local
                  auto-read-only-function
                  (lambda ()
                    (push
                     (list :second
                           (current-buffer))
                     events)
                    :second-result)))
               (list
                (with-current-buffer first
                  (auto-read-only))
                (with-current-buffer second
                  (auto-read-only))
                (mapcar
                 (lambda (event)
                   (list
                    (car event)
                    (buffer-name
                     (cadr event))))
                 (nreverse events))
                (with-current-buffer first
                  auto-read-only-file-regexps)
                (with-current-buffer second
                  auto-read-only-file-regexps)))
           (when (buffer-live-p first)
             (kill-buffer first))
           (when (buffer-live-p second)
             (kill-buffer second))))"##,
        expect![[
            r#"OK (:first-result :second-result ((:first " *auto-read-only-first*") (:second " *auto-read-only-second*")) ("/vendor/") ("/generated/"))"#
        ]],
    )
}

fn auto_read_only_custom_function_arity_and_errors_propagate_without_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_custom_function_arity_and_errors_propagate_without_fallback",
        r##"(mapcar
         (lambda (function)
           (with-temp-buffer
             (setq buffer-file-name
                   "/workspace/vendor/pkg.el")
             (let ((auto-read-only-file-regexps
                    '("/vendor/"))
                   (auto-read-only-function
                    function))
               (let ((outcome
                      (auto-read-only-test-error-data
                       #'auto-read-only)))
                 (list
                  (if (eq
                       (cadr outcome)
                       'wrong-number-of-arguments)
                      (list
                       :error
                       'wrong-number-of-arguments
                       (car
                        (last
                         (caddr outcome))))
                    outcome)
                  buffer-read-only
                  (bound-and-true-p
                   view-mode))))))
         (list
          (lambda (_argument)
            :wrong-arity)
          (lambda ()
            (error "custom failure"))))"##,
        expect![[
            r#"OK (((:error wrong-number-of-arguments 0) nil nil) ((:error error ("custom failure")) nil nil))"#
        ]],
    )
}

pub(super) fn matching_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_read_only_default_regexps_match_exact_extension_and_directory_boundaries(),
        auto_read_only_without_filename_or_without_match_is_a_strict_noop(),
        auto_read_only_default_action_enters_real_view_mode_and_returns_its_value(),
        auto_read_only_custom_action_runs_once_in_original_buffer_with_exact_state(),
        auto_read_only_stops_at_first_match_and_never_evaluates_later_invalid_regexp(),
        auto_read_only_reaches_and_propagates_invalid_later_regexp_after_misses(),
        auto_read_only_preserves_preexisting_match_data_for_match_and_miss_paths(),
        auto_read_only_matcher_uses_one_filename_and_stops_calling_after_success(),
        auto_read_only_buffer_local_patterns_and_actions_are_isolated_in_practical_use(),
        auto_read_only_custom_function_arity_and_errors_propagate_without_fallback(),
    ]
}
