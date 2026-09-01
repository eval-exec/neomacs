use std::time::Duration;

use expect_test::expect;

use crate::{AUTO_YASNIPPET_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'auto-yasnippet)

(defun neomacs-aya-test-state ()
  "Return stable text and active Yasnippet field state."
  (let ((field (and (bound-and-true-p yas--active-field-overlay)
                    (overlay-get yas--active-field-overlay 'yas--field))))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :mark (mark t)
          :active (and (use-region-p) t)
          :snippets (length (yas-active-snippets t))
          :field (and field (yas--field-number field)))))

(defun neomacs-aya-test-finish-field (text)
  "Type TEXT in the current field and advance through Yasnippet."
  (insert text)
  (yas-next-field-or-maybe-expand)
  (neomacs-aya-test-state))

(defun neomacs-aya-test-error (function)
  "Return FUNCTION's value or stable error details."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn package_contract_exposes_disposable_snippet_workflows_and_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'auto-yasnippet package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features
         (mapcar (lambda (feature) (and (featurep feature) t))
                 '(auto-yasnippet yasnippet)))
   :defaults
   (list :directory
         (file-relative-name aya-persist-snippets-dir user-emacs-directory)
         :newline aya-create-with-newline
         :case-fold aya-case-fold
         :trim aya-trim-one-line
         :marker aya-marker
         :field-regex aya-field-regex
         :current aya-current
         :history aya-history)
   :commands
   (mapcar #'commandp
           '(aya-create aya-expand aya-expand-from-history
             aya-delete-from-history aya-clear-history
             aya-next-in-history aya-previous-in-history
             aya-open-line aya-yank-snippet aya-yank-snippet-from-history
             aya-persist-snippet aya-persist-snippet-from-history))))
"###;
    let expected = expect![[
        r#"OK (:package (:name auto-yasnippet :version "20230208.331" :requirements ((yasnippet (0 14 0)) (emacs (25 1))) :features (t t)) :defaults (:directory "snippets" :newline nil :case-fold t :trim nil :marker "~" :field-regex "\\sw\\|\\s_" :current "" :history nil) :commands (t t t t t t t t t t t t))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_disposable_snippet_workflows_and_defaults",
        elisp_form,
        expected,
    )
}

fn mixed_case_template_creation_and_real_expansion_update_all_mirrors() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "")
      (aya-history nil)
      (aya-case-fold t)
      (yas-global-mode nil))
  (with-temp-buffer
    (c-mode)
    (insert "count_of_~red = get_total(\"~Red\");")
    (aya-create)
    (let ((created (neomacs-aya-test-state))
          (snippet aya-current)
          (history (copy-sequence aya-history)))
      (erase-buffer)
      (yas-minor-mode 1)
      (aya-expand 1)
      (let ((expanded (neomacs-aya-test-state))
            (finished (neomacs-aya-test-finish-field "blue")))
        (yas-exit-all-snippets)
        (list :created created
              :snippet snippet
              :history history
              :expanded expanded
              :finished finished
              :settled (neomacs-aya-test-state)
              :current aya-current)))))
"###;
    let expected = expect![[
        r#"OK (:created (:text "count_of_red = get_total(\"Red\");" :point 33 :mark nil :active nil :snippets 0 :field nil) :snippet "count_of_$1 = get_total(\"${1:$(aya--upcase-first-char yas-text)}\");" :history ("count_of_$1 = get_total(\"${1:$(aya--upcase-first-char yas-text)}\");") :expanded (:text "count_of_ = get_total(\"\");" :point 10 :mark nil :active nil :snippets 1 :field 1) :finished (:text "count_of_blue = get_total(\"Blue\");" :point 35 :mark nil :active nil :snippets 1 :field 1) :settled (:text "count_of_blue = get_total(\"Blue\");" :point 35 :mark nil :active nil :snippets 0 :field 1) :current "count_of_$1 = get_total(\"${1:$(aya--upcase-first-char yas-text)}\");")"#
    ]];
    ParityBatchCase::value(
        "mixed_case_template_creation_and_real_expansion_update_all_mirrors",
        elisp_form,
        expected,
    )
}

fn multiline_method_template_expands_two_fields_and_case_preserving_mirrors() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "")
      (aya-history nil)
      (aya-case-fold t))
  (with-temp-buffer
    (java-mode)
    (insert "~FooType get~Foo() {\n"
            "    // Get the ~foo attribute on this.\n"
            "    return this.~foo;\n"
            "}")
    (set-mark (point-min))
    (goto-char (point-max))
    (activate-mark)
    (aya-create)
    (let ((source (neomacs-aya-test-state))
          (snippet aya-current))
      (deactivate-mark)
      (erase-buffer)
      (yas-minor-mode 1)
      (aya-expand 1)
      (let ((field-one (neomacs-aya-test-state)))
        (insert "Widget")
        (yas-next-field-or-maybe-expand)
        (let ((field-two (neomacs-aya-test-state)))
          (insert "bar")
          (yas-next-field-or-maybe-expand)
          (let ((result (neomacs-aya-test-state)))
            (yas-exit-all-snippets)
            (list :source source
                  :snippet snippet
                  :field-one field-one
                  :field-two field-two
                  :result result
                  :settled (neomacs-aya-test-state))))))))
"###;
    let expected = expect![[
        r#"OK (:source (:text "FooType getFoo() {\n    // Get the foo attribute on this.\n    return this.foo;\n}" :point 80 :mark 1 :active t :snippets 0 :field nil) :snippet "$1 get${2:$(aya--upcase-first-char yas-text)}() {\n    // Get the $2 attribute on this.\n    return this.$2;\n}" :field-one (:text "get() {\n    // Get the  attribute on this.\n    return this.;\n}" :point 1 :mark 1 :active nil :snippets 1 :field 1) :field-two (:text "Widgetget() {\n    // Get the  attribute on this.\n    return this.;\n}" :point 30 :mark 1 :active nil :snippets 1 :field 2) :result (:text "WidgetgetBar() {\n    // Get the bar attribute on this.\n    return this.bar;\n}" :point 78 :mark 1 :active nil :snippets 1 :field 2) :settled (:text "WidgetgetBar() {\n    // Get the bar attribute on this.\n    return this.bar;\n}" :point 78 :mark 1 :active nil :snippets 0 :field 2))"#
    ]];
    ParityBatchCase::value(
        "multiline_method_template_expands_two_fields_and_case_preserving_mirrors",
        elisp_form,
        expected,
    )
}

fn active_expression_region_is_wrapped_at_the_selected_snippet_field() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "")
      (aya-history nil))
  (with-temp-buffer
    (prog-mode)
    (insert "print(\"~thing\")")
    (aya-create)
    (let ((snippet aya-current))
      (erase-buffer)
      (insert "subtotal + shipping")
      (set-mark (point-min))
      (goto-char (point-max))
      (activate-mark)
      (let ((selected (neomacs-aya-test-state)))
        (yas-minor-mode 1)
        (aya-expand 1)
        (let ((wrapped (neomacs-aya-test-state)))
          (yas-exit-all-snippets)
          (list :snippet snippet
                :selected selected
                :wrapped wrapped
                :finished (neomacs-aya-test-state)))))))
"###;
    let expected = expect![[
        r#"OK (:snippet "print(\"$1\")" :selected (:text "subtotal + shipping" :point 20 :mark 1 :active t :snippets 0 :field nil) :wrapped (:text "print(\"subtotal + shipping\")" :point 27 :mark 1 :active t :snippets 1 :field nil) :finished (:text "print(\"subtotal + shipping\")" :point 27 :mark 1 :active t :snippets 0 :field nil))"#
    ]];
    ParityBatchCase::value(
        "active_expression_region_is_wrapped_at_the_selected_snippet_field",
        elisp_form,
        expected,
    )
}

fn creation_history_navigation_deletion_and_clear_form_a_complete_session() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "")
      (aya-history nil)
      notices)
  (with-temp-buffer
    (dolist (template '("log.~level(\"~message\")"
                        "metric.~name += ~amount"
                        "queue.push(~item)"))
      (erase-buffer)
      (insert template)
      (aya-create))
    (let ((created (list aya-current (copy-sequence aya-history))))
      (cl-letf (((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (push (apply #'format format-string arguments) notices))))
        (aya-next-in-history)
        (let ((next aya-current))
          (aya-next-in-history)
          (let ((wrapped aya-current))
            (aya-previous-in-history)
            (let ((previous aya-current))
              (cl-letf (((symbol-function 'completing-read-multiple)
                         (lambda (&rest _)
                           (list (nth 1 aya-history) (nth 2 aya-history))))
                        ((symbol-function 'y-or-n-p) (lambda (&rest _) t)))
                (aya-delete-from-history))
              (let ((deleted (list aya-current (copy-sequence aya-history))))
                (aya-clear-history)
                (list :created created
                      :next next
                      :wrapped wrapped
                      :previous previous
                      :deleted deleted
                      :cleared (list aya-current aya-history)
                      :notices (nreverse notices))))))))))
"###;
    let expected = expect![[
        r#"OK (:created ("queue.push($1)" ("log.$1(\"$2\")" "metric.$1 += $2" "queue.push($1)")) :next "log.$1(\"$2\")" :wrapped "metric.$1 += $2" :previous "log.$1(\"$2\")" :deleted ("log.$1(\"$2\")" ("log.$1(\"$2\")")) :cleared (nil ("")) :notices ("aya-current:\nlog.$1(\"$2\")" "aya-current:\nmetric.$1 += $2" "aya-current:\nlog.$1(\"$2\")"))"#
    ]];
    ParityBatchCase::value(
        "creation_history_navigation_deletion_and_clear_form_a_complete_session",
        elisp_form,
        expected,
    )
}

fn trim_newline_default_hook_and_backtick_escaping_shape_reusable_templates() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "")
      (aya-history nil)
      (aya-trim-one-line t)
      (aya-create-with-newline t)
      (aya-case-fold nil)
      (default-calls 0))
  (with-temp-buffer
    (setq-local aya-default-function
                (lambda () (setq default-calls (1+ default-calls))))
    (insert "    const ~port = config.~port;")
    (aya-create)
    (let ((created (list (buffer-string) aya-current
                         (copy-sequence aya-history) default-calls)))
      (erase-buffer)
      (insert "    const port = config.port;")
      (aya-create)
      (let ((unmarked (list (buffer-string) aya-current default-calls)))
        (erase-buffer)
        (insert "```~lang\n~body\n```")
        (set-mark (point-min))
        (goto-char (point-max))
        (activate-mark)
        (aya-create)
        (list :created created
              :unmarked unmarked
              :fenced (list (buffer-string) aya-current
                            (car (last aya-history)) default-calls))))))
"###;
    let expected = expect![[
        r#"OK (:created ("    const port = config.port;" "const $1 = config.$1;\n" ("const $1 = config.$1;\n") 1) :unmarked ("    const port = config.port;" "const $1 = config.$1;\n" 1) :fenced ("```lang\nbody\n```" "\\`\\`\\`$1\n$2\n\\`\\`\\`\n" "\\`\\`\\`$1\n$2\n\\`\\`\\`\n" 2))"#
    ]];
    ParityBatchCase::value(
        "trim_newline_default_hook_and_backtick_escaping_shape_reusable_templates",
        elisp_form,
        expected,
    )
}

fn snippet_yank_exports_current_and_selected_history_entries_as_snippet_files() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "logger.info($1)")
      (aya-history '("metric.increment($1)" "queue.push($1)")))
  (list
   :current
   (with-temp-buffer
     (aya-yank-snippet)
     (list (buffer-string) major-mode (buffer-modified-p)))
   :history
   (with-temp-buffer
     (cl-letf (((symbol-function 'completing-read)
                (lambda (&rest _) (nth 1 aya-history))))
       (aya-yank-snippet-from-history))
     (list (buffer-string) major-mode (buffer-modified-p)))
   :nonempty-error
   (with-temp-buffer
     (insert "existing")
     (neomacs-aya-test-error #'aya-yank-snippet))))
"###;
    let expected = expect![[
        r##"OK (:current ("# -*- mode: snippet -*-\n# name: \n# key: \n# --\nlogger.info($1)" fundamental-mode t) :history ("# -*- mode: snippet -*-\n# name: \n# key: \n# --\nqueue.push($1)" fundamental-mode t) :nonempty-error (:error user-error :data ("Must be called from an empty file") :message "Must be called from an empty file"))"##
    ]];
    ParityBatchCase::value(
        "snippet_yank_exports_current_and_selected_history_entries_as_snippet_files",
        elisp_form,
        expected,
    )
}

fn persistence_writes_a_mode_scoped_loadable_snippet_and_rejects_duplicates() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (expand-file-name "auto-yasnippet-persist" user-emacs-directory))
       (aya-persist-snippets-dir root)
       (aya-current "logger.${1:info}($2)$0")
       (aya-insert-snippet-function #'aya-insert-snippet-function-extra)
       (user-full-name "Parity Engineer")
       (target (expand-file-name "emacs-lisp-mode/logger" root)))
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (with-temp-buffer
        (emacs-lisp-mode)
        (cl-letf (((symbol-function 'read-string)
                   (lambda (&rest _) "log")))
          (aya--persist aya-current "logger"))
        (let ((content
               (with-temp-buffer
                 (insert-file-contents target)
                 (buffer-string)))
              (duplicate
               (neomacs-aya-test-error
                (lambda () (aya--persist aya-current "logger")))))
          (list :relative (file-relative-name target root)
                :exists (file-exists-p target)
                :content content
                :duplicate duplicate)))
    (when (file-directory-p root)
      (delete-directory root t))))
"###;
    let expected = expect![[
        r##"OK (:relative "emacs-lisp-mode/logger" :exists t :content "# -*- mode: snippet -*-\n# contributor: Parity Engineer\n# name: logger\n# key: log\n# --\nlogger.${1:info}($2)$0" :duplicate (:error user-error :data ("A snippet called \"logger\" already exists in \"[ORACLE-HOME]/.emacs.d/auto-yasnippet-persist/emacs-lisp-mode\"") :message "A snippet called \"logger\" already exists in \"[ORACLE-HOME]/.emacs.d/auto-yasnippet-persist/emacs-lisp-mode\""))"##
    ]];
    ParityBatchCase::value(
        "persistence_writes_a_mode_scoped_loadable_snippet_and_rejects_duplicates",
        elisp_form,
        expected,
    )
}

fn command_errors_preserve_session_state_when_required_inputs_are_missing() -> ParityBatchCase {
    let elisp_form = r###"
(let ((aya-current "")
      (aya-history nil))
  (list
   :expand (neomacs-aya-test-error (lambda () (aya-expand 1)))
   :history-expand
   (neomacs-aya-test-error (lambda () (aya-expand-from-history 1)))
   :history-delete (neomacs-aya-test-error #'aya-delete-from-history)
   :history-next (neomacs-aya-test-error #'aya-next-in-history)
   :history-previous (neomacs-aya-test-error #'aya-previous-in-history)
   :persist
   (neomacs-aya-test-error
    (lambda () (call-interactively #'aya-persist-snippet)))
   :state (list aya-current aya-history)))
"###;
    let expected = expect![[
        r#"OK (:expand (:error user-error :data ("There is no aya-current snippet available") :message "There is no aya-current snippet available") :history-expand (:error user-error :data ("Nothing in aya-history to expand") :message "Nothing in aya-history to expand") :history-delete (:error user-error :data ("Nothing in aya-history to delete") :message "Nothing in aya-history to delete") :history-next (:error user-error :data ("Nothing in aya-history") :message "Nothing in aya-history") :history-previous (:error user-error :data ("Nothing in aya-history") :message "Nothing in aya-history") :persist (:error user-error :data ("You don’t have an auto-snippet defined") :message "You don’t have an auto-snippet defined") :state ("" nil))"#
    ]];
    ParityBatchCase::value(
        "command_errors_preserve_session_state_when_required_inputs_are_missing",
        elisp_form,
        expected,
    )
}

#[test]
fn auto_yasnippet_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(AUTO_YASNIPPET_MELPA_PIN, "auto-yasnippet.el")
            .expect("prepare revision-pinned Auto-YASnippet below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "auto-yasnippet-package-batch",
        "Auto-YASnippet",
        &[
            package_contract_exposes_disposable_snippet_workflows_and_defaults(),
            mixed_case_template_creation_and_real_expansion_update_all_mirrors(),
            multiline_method_template_expands_two_fields_and_case_preserving_mirrors(),
            active_expression_region_is_wrapped_at_the_selected_snippet_field(),
            creation_history_navigation_deletion_and_clear_form_a_complete_session(),
            trim_newline_default_hook_and_backtick_escaping_shape_reusable_templates(),
            snippet_yank_exports_current_and_selected_history_entries_as_snippet_files(),
            persistence_writes_a_mode_scoped_loadable_snippet_and_rejects_duplicates(),
            command_errors_preserve_session_state_when_required_inputs_are_missing(),
        ],
    );
}
