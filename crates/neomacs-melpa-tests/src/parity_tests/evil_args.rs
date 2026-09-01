use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_ARGS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-args)

(defun neomacs-evil-args-test-buffer (text body)
  "Run BODY in a temporary Evil buffer containing TEXT."
  (with-temp-buffer
    (insert text)
    (goto-char (point-min))
    (emacs-lisp-mode)
    (evil-local-mode 1)
    (unwind-protect
        (progn
          (evil-normal-state)
          (funcall body))
      (when (evil-visual-state-p) (evil-exit-visual-state))
      (evil-local-mode -1))))

(defun neomacs-evil-args-test-point-state ()
  "Return point with readable context for motion assertions."
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :before (and (> (point) (point-min))
                     (char-to-string (char-before)))
        :after (and (char-after) (char-to-string (char-after)))
        :line-text (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))
        :rest (buffer-substring-no-properties
               (point) (min (+ (point) 18) (point-max)))))

(defun neomacs-evil-args-test-range-state (range)
  "Return RANGE boundaries, type, and exact selected text."
  (let ((begin (evil-range-beginning range))
        (end (evil-range-end range)))
    (list :begin begin
          :end end
          :type (evil-type range)
          :text (buffer-substring-no-properties begin end))))
"####;

fn nested_function_calls_move_by_top_level_arguments_in_both_directions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-args-test-buffer
 "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));"
 (lambda ()
   (search-forward "api")
   (backward-char 2)
   (let ((start (neomacs-evil-args-test-point-state)))
     (evil-forward-arg 1)
     (let ((build (neomacs-evil-args-test-point-state)))
       (evil-forward-arg 1)
       (let ((notify (neomacs-evil-args-test-point-state)))
         (evil-forward-arg 1)
         (let ((closer (neomacs-evil-args-test-point-state)))
           (evil-backward-arg 1)
           (let ((back-notify (neomacs-evil-args-test-point-state)))
             (evil-backward-arg 2)
             (list :start start
                   :build build
                   :notify notify
                   :closer closer
                   :back-notify back-notify
                   :back-api (neomacs-evil-args-test-point-state)))))))))
"####;
    let expected = expect![[
        r#"OK (:start (:point 9 :line 1 :column 8 :before "a" :after "p" :line-text "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));" :rest "pi, build(target(\"") :build (:point 13 :line 1 :column 12 :before " " :after "b" :line-text "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));" :rest "build(target(\"linu") :notify (:point 54 :line 1 :column 53 :before " " :after "n" :line-text "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));" :rest "notify(owner));") :closer (:point 67 :line 1 :column 66 :before ")" :after ")" :line-text "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));" :rest ");") :back-notify (:point 54 :line 1 :column 53 :before " " :after "n" :line-text "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));" :rest "notify(owner));") :back-api (:point 8 :line 1 :column 7 :before "(" :after "a" :line-text "deploy(api, build(target(\"linux\", \"x86_64\"), flags), notify(owner));" :rest "api, build(target("))"#
    ]];
    ParityBatchCase::value(
        "nested_function_calls_move_by_top_level_arguments_in_both_directions",
        elisp_form,
        expected,
    )
}

fn multiline_calls_land_on_indented_argument_starts_and_honor_counts() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-args-test-buffer
 "publish(\n  artifact,\n  checksum(source, algorithm),\n  metadata\n)"
 (lambda ()
   (search-forward "artifact")
   (backward-char 4)
   (let ((artifact (neomacs-evil-args-test-point-state)))
     (evil-forward-arg 1)
     (let ((checksum (neomacs-evil-args-test-point-state)))
       (evil-forward-arg 1)
       (let ((metadata (neomacs-evil-args-test-point-state)))
         (evil-backward-arg 2)
         (list :artifact artifact
               :checksum checksum
               :metadata metadata
               :back-two (neomacs-evil-args-test-point-state)))))))
"####;
    let expected = expect![[
        r#"OK (:artifact (:point 16 :line 2 :column 6 :before "i" :after "f" :line-text "  artifact," :rest "fact,\n  checksum(s") :checksum (:point 24 :line 3 :column 2 :before " " :after "c" :line-text "  checksum(source, algorithm)," :rest "checksum(source, a") :metadata (:point 55 :line 4 :column 2 :before " " :after "m" :line-text "  metadata" :rest "metadata\n)") :back-two (:point 12 :line 2 :column 2 :before " " :after "a" :line-text "  artifact," :rest "artifact,\n  checks"))"#
    ]];
    ParityBatchCase::value(
        "multiline_calls_land_on_indented_argument_starts_and_honor_counts",
        elisp_form,
        expected,
    )
}

fn inner_and_outer_text_objects_select_middle_last_and_multiple_arguments() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :middle
 (neomacs-evil-args-test-buffer
  "call(alpha, beta + gamma, delta)"
  (lambda ()
    (search-forward "beta")
    (backward-char 2)
    (list :inner (neomacs-evil-args-test-range-state (evil-inner-arg 1))
          :outer (neomacs-evil-args-test-range-state (evil-outer-arg 1))
          :inner-two (neomacs-evil-args-test-range-state (evil-inner-arg 2)))))
 :last
 (neomacs-evil-args-test-buffer
  "call(alpha, beta + gamma, delta)"
  (lambda ()
    (search-forward "delta")
    (backward-char 2)
    (list :inner (neomacs-evil-args-test-range-state (evil-inner-arg 1))
          :outer (neomacs-evil-args-test-range-state (evil-outer-arg 1))))))
"####;
    let expected = expect![[
        r#"OK (:middle (:inner (:begin 13 :end 25 :type inclusive :text "beta + gamma") :outer (:begin 13 :end 27 :type inclusive :text "beta + gamma, ") :inner-two (:begin 13 :end 32 :type inclusive :text "beta + gamma, delta")) :last (:inner (:begin 27 :end 32 :type inclusive :text "delta") :outer (:begin 25 :end 32 :type inclusive :text ", delta")))"#
    ]];
    ParityBatchCase::value(
        "inner_and_outer_text_objects_select_middle_last_and_multiple_arguments",
        elisp_form,
        expected,
    )
}

fn deleting_outer_middle_and_last_arguments_preserves_valid_call_syntax() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :middle
 (neomacs-evil-args-test-buffer
  "deploy(alpha, beta, gamma)"
  (lambda ()
    (search-forward "beta")
    (backward-char 2)
    (let* ((range (evil-outer-arg 1))
           (selected (neomacs-evil-args-test-range-state range))
           (kill-ring nil))
      (evil-delete (evil-range-beginning range)
                   (evil-range-end range)
                   (or (evil-type range) 'exclusive) nil nil)
      (list :selected selected
            :buffer (buffer-string)
            :point (point)
            :kill-ring kill-ring))))
 :last
 (neomacs-evil-args-test-buffer
  "deploy(alpha, beta, gamma)"
  (lambda ()
    (search-forward "gamma")
    (backward-char 2)
    (let* ((range (evil-outer-arg 1))
           (selected (neomacs-evil-args-test-range-state range))
           (kill-ring nil))
      (evil-delete (evil-range-beginning range)
                   (evil-range-end range)
                   (or (evil-type range) 'exclusive) nil nil)
      (list :selected selected
            :buffer (buffer-string)
            :point (point)
            :kill-ring kill-ring)))))
"####;
    let expected = expect![[
        r#"OK (:middle (:selected (:begin 15 :end 21 :type inclusive :text "beta, ") :buffer "deploy(alpha, gamma)" :point 15 :kill-ring ("beta, ")) :last (:selected (:begin 19 :end 26 :type inclusive :text ", gamma") :buffer "deploy(alpha, beta)" :point 19 :kill-ring (", gamma")))"#
    ]];
    ParityBatchCase::value(
        "deleting_outer_middle_and_last_arguments_preserves_valid_call_syntax",
        elisp_form,
        expected,
    )
}

fn customized_space_delimiters_navigate_lisp_lists_without_leaking_configuration() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((before
       (list (copy-tree evil-args-openers)
             (copy-tree evil-args-closers)
             (copy-tree evil-args-delimiters))))
  (let ((result
         (let ((evil-args-openers '("("))
               (evil-args-closers '(")"))
               (evil-args-delimiters '(" ")))
           (neomacs-evil-args-test-buffer
            "(function alpha beta gamma)"
            (lambda ()
              (search-forward "function")
              (backward-char 3)
              (let ((function (neomacs-evil-args-test-point-state)))
                (evil-forward-arg 1)
                (let ((alpha (neomacs-evil-args-test-point-state)))
                  (evil-forward-arg 2)
                  (let ((gamma (neomacs-evil-args-test-point-state)))
                    (evil-backward-arg 3)
                    (list :function function
                          :alpha alpha
                          :gamma gamma
                          :back-function
                          (neomacs-evil-args-test-point-state))))))))))
    (list :result result
          :configuration-before before
          :configuration-after
          (list evil-args-openers evil-args-closers evil-args-delimiters))))
"####;
    let expected = expect![[
        r#"OK (:result (:function (:point 7 :line 1 :column 6 :before "t" :after "i" :line-text "(function alpha beta gamma)" :rest "ion alpha beta gam") :alpha (:point 11 :line 1 :column 10 :before " " :after "a" :line-text "(function alpha beta gamma)" :rest "alpha beta gamma)") :gamma (:point 22 :line 1 :column 21 :before " " :after "g" :line-text "(function alpha beta gamma)" :rest "gamma)") :back-function (:point 2 :line 1 :column 1 :before "(" :after "f" :line-text "(function alpha beta gamma)" :rest "function alpha bet")) :configuration-before (("(" "{" "[") (")" "}" "]") ("," ";")) :configuration-after (("(" "{" "[") (")" "}" "]") ("," ";")))"#
    ]];
    ParityBatchCase::value(
        "customized_space_delimiters_navigate_lisp_lists_without_leaking_configuration",
        elisp_form,
        expected,
    )
}

fn repeated_jump_out_moves_from_nested_format_to_call_and_enclosing_form() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-args-test-buffer
 "if (ready) {\n  publish(target, format(item, codec));\n}"
 (lambda ()
   (search-forward "codec")
   (backward-char 2)
   (let ((inside (neomacs-evil-args-test-point-state)))
     (evil-jump-out-args 1)
     (let ((format (neomacs-evil-args-test-point-state)))
       (evil-jump-out-args 1)
       (let ((publish (neomacs-evil-args-test-point-state)))
         (evil-jump-out-args 1)
         (list :inside inside
               :format format
               :publish publish
               :outer (neomacs-evil-args-test-point-state)))))))
"####;
    let expected = expect![[
        r#"OK (:inside (:point 48 :line 2 :column 34 :before "d" :after "e" :line-text "  publish(target, format(item, codec));" :rest "ec));\n}") :format (:point 32 :line 2 :column 18 :before " " :after "f" :line-text "  publish(target, format(item, codec));" :rest "format(item, codec") :publish (:point 16 :line 2 :column 2 :before " " :after "p" :line-text "  publish(target, format(item, codec));" :rest "publish(target, fo") :outer (:point 5 :line 1 :column 4 :before "(" :after "r" :line-text "if (ready) {" :rest "ready) {\n  publish"))"#
    ]];
    ParityBatchCase::value(
        "repeated_jump_out_moves_from_nested_format_to_call_and_enclosing_form",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_args_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(EVIL_ARGS_MELPA_PIN, "evil-args.el")
            .expect("prepare revision-pinned Evil Args source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "evil-args-package-batch",
        "Evil Args",
        &[
            nested_function_calls_move_by_top_level_arguments_in_both_directions(),
            multiline_calls_land_on_indented_argument_starts_and_honor_counts(),
            inner_and_outer_text_objects_select_middle_last_and_multiple_arguments(),
            deleting_outer_middle_and_last_arguments_preserves_valid_call_syntax(),
            customized_space_delimiters_navigate_lisp_lists_without_leaking_configuration(),
            repeated_jump_out_moves_from_nested_format_to_call_and_enclosing_form(),
        ],
    );
}
