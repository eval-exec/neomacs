use expect_test::expect;

use super::ParityBatchCase;

fn variable_annotations_show_class_value_and_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "variable_annotations_show_class_value_and_documentation",
        r##"
(progn
  (defvar neomacs-marg-num 42 "Number variable.")
  (defvar neomacs-marg-str "hello world" "String variable.")
  (defvar neomacs-marg-nil nil "Nil variable.")
  (defvar neomacs-marg-list (list 1 2 3) "List variable.")
  (let ((marginalia-field-width 80))
    (neomacs-marginalia-test-plain
     (list :number (marginalia-annotate-variable "neomacs-marg-num")
           :string (marginalia-annotate-variable "neomacs-marg-str")
           :nil (marginalia-annotate-variable "neomacs-marg-nil")
           :list (marginalia-annotate-variable "neomacs-marg-list")
           :unknown (marginalia-annotate-variable "neomacs-marg-not-a-symbol")))))
"##,
        expect![[
            r#"OK (:number "   v       42                                        Number variable.                                                                " :string "   v       \"hello world\"                             String variable.                                                                " :nil "   v       nil                                       Nil variable.                                                                   " :list "   v       (1 2 3)                                   List variable.                                                                  " :unknown nil)"#
        ]],
    )
}

fn function_annotations_show_class_arguments_and_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "function_annotations_show_class_arguments_and_documentation",
        r##"
(progn
  (defun neomacs-marg-fun (a b &optional c) "Sample function doc." (list a b c))
  (defmacro neomacs-marg-macro (x) "Sample macro doc." x)
  (let ((marginalia-field-width 80))
    (neomacs-marginalia-test-plain
     (list :function (marginalia-annotate-function "neomacs-marg-fun")
           :macro (marginalia-annotate-function "neomacs-marg-macro")
           :missing (marginalia-annotate-function "neomacs-marg-undefined")))))
"##,
        expect![[
            r#"OK (:function "   f       (A B &optional C)                         Sample function doc.                                                            " :macro "   m       (X)                                       Sample macro doc.                                                               " :missing nil)"#
        ]],
    )
}

fn command_annotations_include_binding_and_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "command_annotations_include_binding_and_documentation",
        r##"
(progn
  (defun neomacs-marg-command () "Interactive command doc." (interactive) nil)
  (defun neomacs-marg-plain () "Non-command doc." nil)
  (let ((marginalia-field-width 80))
    (neomacs-marginalia-test-plain
     (list :command (marginalia-annotate-command "neomacs-marg-command")
           :non-command (marginalia-annotate-command "neomacs-marg-plain")))))
"##,
        expect![[
            r#"OK (:command "   Interactive command doc.                                                        " :non-command "   Non-command doc.                                                                ")"#
        ]],
    )
}

fn symbol_class_reflects_obsolete_alias_and_group_facets() -> ParityBatchCase {
    ParityBatchCase::value(
        "symbol_class_reflects_obsolete_alias_and_group_facets",
        r##"
(progn
  (defun neomacs-marg-live () "Replacement." (interactive) nil)
  (defun neomacs-marg-old () "Old command." (interactive) nil)
  (make-obsolete 'neomacs-marg-old 'neomacs-marg-live "9.9")
  (defalias 'neomacs-marg-alias 'neomacs-marg-live)
  (defvar neomacs-marg-group-var nil "Group carrier.")
  (put 'neomacs-marg-group-var 'group-documentation "A custom group.")
  (let ((marginalia-field-width 80))
    (neomacs-marginalia-test-plain
     (list :obsolete (marginalia-annotate-symbol "neomacs-marg-old")
           :alias (marginalia-annotate-symbol "neomacs-marg-alias")
           :group (marginalia-annotate-symbol "neomacs-marg-group-var")))))
"##,
        expect![[
            r#"OK (:obsolete "   c-      Old command.                                                                                                              " :alias "   c&      Replacement.                                                                                                              " :group "   vG      Group carrier.                                                                                                            ")"#
        ]],
    )
}

fn character_annotations_describe_code_and_category() -> ParityBatchCase {
    ParityBatchCase::value(
        "character_annotations_describe_code_and_category",
        r##"
(let ((marginalia-field-width 80))
  (neomacs-marginalia-test-plain
   (list :latin (marginalia-annotate-char "LATIN SMALL LETTER A")
         :digit (marginalia-annotate-char "DIGIT ZERO")
         :unknown (marginalia-annotate-char "DEFINITELY NOT A CHARACTER NAME"))))
"##,
        expect![[
            r#"OK (:latin " (a)   000061  Letter, Lowercase             " :digit " (0)   000030  Number, Decimal Digit         " :unknown nil)"#
        ]],
    )
}

fn environment_variable_annotation_reads_current_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "environment_variable_annotation_reads_current_value",
        r##"
(let ((process-environment (copy-sequence process-environment))
      (marginalia-field-width 80))
  (setenv "NEOMACS_MARG_ENV" "deterministic-value")
  (neomacs-marginalia-test-plain
   (list :set (marginalia-annotate-environment-variable "NEOMACS_MARG_ENV")
         :unset (marginalia-annotate-environment-variable "NEOMACS_MARG_ENV_ABSENT"))))
"##,
        expect![[
            r#"OK (:set "   deterministic-value                                                             " :unset nil)"#
        ]],
    )
}

fn censored_and_truncated_values_stay_within_field_limits() -> ParityBatchCase {
    ParityBatchCase::value(
        "censored_and_truncated_values_stay_within_field_limits",
        r##"
(progn
  (defvar neomacs-marg-secret-api-key "super-secret-token" "Holds a key.")
  (defvar neomacs-marg-long-value
    "0123456789012345678901234567890123456789ABCDEFGHIJKLMNOPQRST"
    "Long value variable.")
  (let ((marginalia-field-width 80)
        (marginalia--ellipsis "…")
        (marginalia-censor-variables '("api-?key")))
    (neomacs-marginalia-test-plain
     (list :censored (marginalia-annotate-variable "neomacs-marg-secret-api-key")
           :truncated (marginalia-annotate-variable "neomacs-marg-long-value")))))
"##,
        expect![[
            r#"OK (:censored "   v       *****                                     Holds a key.                                                                    " :truncated "   v       \"01234567890123456789012345678901234567…  Long value variable.                                                            ")"#
        ]],
    )
}

fn cycle_without_active_minibuffer_signals_user_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "cycle_without_active_minibuffer_signals_user_error",
        r##"
(neomacs-marginalia-test-outcome (lambda () (marginalia-cycle)))
"##,
        expect![[r#"OK (:signal user-error :message "Marginalia: No active minibuffer")"#]],
    )
}

fn global_minor_mode_installs_and_removes_its_advice_and_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_minor_mode_installs_and_removes_its_advice_and_hooks",
        r##"
(let ((was (bound-and-true-p marginalia-mode)))
  (unwind-protect
      (progn
        (marginalia-mode 1)
        (let ((enabled
               (list :metadata
                     (and (advice-member-p
                           #'marginalia--completion-metadata-get
                           #'completion-metadata-get)
                          t)
                     :base
                     (and (advice-member-p
                           #'marginalia--base-position
                           #'completion-all-completions)
                          t)
                     :hook
                     (and (memq #'marginalia--minibuffer-setup
                                minibuffer-setup-hook)
                          t))))
          (marginalia-mode -1)
          (list :enabled enabled
                :disabled
                (list :metadata
                      (and (advice-member-p
                            #'marginalia--completion-metadata-get
                            #'completion-metadata-get)
                           t)
                      :base
                      (and (advice-member-p
                            #'marginalia--base-position
                            #'completion-all-completions)
                           t)
                      :hook
                      (and (memq #'marginalia--minibuffer-setup
                                 minibuffer-setup-hook)
                           t)))))
    (if was (marginalia-mode 1) (marginalia-mode -1))))
"##,
        expect!["OK (:enabled (:metadata t :base t :hook t) :disabled (:metadata nil :base nil :hook nil))"],
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        variable_annotations_show_class_value_and_documentation(),
        function_annotations_show_class_arguments_and_documentation(),
        command_annotations_include_binding_and_documentation(),
        symbol_class_reflects_obsolete_alias_and_group_facets(),
        character_annotations_describe_code_and_category(),
        environment_variable_annotation_reads_current_value(),
        censored_and_truncated_values_stay_within_field_limits(),
        cycle_without_active_minibuffer_signals_user_error(),
        global_minor_mode_installs_and_removes_its_advice_and_hooks(),
    ]
}
