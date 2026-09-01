use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EXPAND_REGION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EXPAND_REGION_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const EXPAND_REGION_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'expand-region)

(defun neomacs-er-test-reset-command-state ()
  "Reset Expand Region's command protocol in the current buffer."
  (setq last-command nil
        this-command nil)
  (deactivate-mark t))

(defun neomacs-er-test-state (label)
  "Capture the user-visible region and rollback state under LABEL."
  (let ((active (region-active-p)))
    (list label
          :point (point)
          :mark (mark t)
          :active active
          :bounds (and active (list (region-beginning) (region-end)))
          :text (and active
                     (buffer-substring-no-properties
                      (region-beginning) (region-end))))))

(defun neomacs-er-test-expand (label)
  "Expand once and return a checkpoint under LABEL."
  (setq this-command 'er/expand-region)
  (er/expand-region 1)
  (setq last-command 'er/expand-region)
  (neomacs-er-test-state label))

(defun neomacs-er-test-contract (count label)
  "Contract COUNT times and return a checkpoint under LABEL."
  (setq this-command 'er/contract-region)
  (er/contract-region count)
  (setq last-command 'er/contract-region)
  (neomacs-er-test-state label))

(defun neomacs-er-test-expand-times (count)
  "Expand COUNT times and return all checkpoints."
  (let (states)
    (dotimes (index count)
      (push (neomacs-er-test-expand (1+ index)) states))
    (nreverse states)))
"##;

fn expand_region_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EXPAND_REGION_MELPA_PIN, "expand-region.el")
        .expect("prepare revision-pinned Expand Region source below ./tmp")
        .with_prelude(EXPAND_REGION_TEST_PRELUDE)
        .with_timeout(EXPAND_REGION_TEST_TIMEOUT)
}

fn elisp_release_message_expands_from_word_to_string_expression_and_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(setq release-message \"Deploy preview now\")")
  (goto-char (point-min))
  (search-forward "preview")
  (backward-char 3)
  (neomacs-er-test-reset-command-state)
  (neomacs-er-test-expand-times 6))
"##;
    let expected = expect![[
        r####"OK ((1 :point 31 :mark 38 :active t :bounds (31 38) :text "preview") (2 :point 24 :mark 42 :active t :bounds (24 42) :text "Deploy preview now") (3 :point 23 :mark 43 :active t :bounds (23 43) :text "\"Deploy preview now\"") (4 :point 2 :mark 43 :active t :bounds (2 43) :text "setq release-message \"Deploy preview now\"") (5 :point 1 :mark 44 :active t :bounds (1 44) :text "(setq release-message \"Deploy preview now\")") (6 :point 1 :mark 44 :active t :bounds (1 44) :text "(setq release-message \"Deploy preview now\")"))"####
    ]];
    ParityBatchCase::value(
        "elisp_release_message_expands_from_word_to_string_expression_and_buffer",
        elisp_form,
        expected,
    )
}

fn overshoot_can_contract_stepwise_and_reset_to_the_exact_starting_cursor() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(((checksum)))")
  (goto-char 8)
  (neomacs-er-test-reset-command-state)
  (let ((expanded (neomacs-er-test-expand-times 4))
        contracted reset)
    (setq contracted
          (list (neomacs-er-test-contract 1 :contract-one)
                (neomacs-er-test-contract 1 :contract-two)))
    (setq reset (neomacs-er-test-contract 0 :reset))
    (list :expanded expanded
          :contracted contracted
          :reset reset)))
"##;
    let expected = expect![[
        r####"OK (:expanded ((1 :point 4 :mark 12 :active t :bounds (4 12) :text "checksum") (2 :point 3 :mark 13 :active t :bounds (3 13) :text "(checksum)") (3 :point 2 :mark 14 :active t :bounds (2 14) :text "((checksum))") (4 :point 1 :mark 15 :active t :bounds (1 15) :text "(((checksum)))")) :contracted ((:contract-one :point 2 :mark 14 :active t :bounds (2 14) :text "((checksum))") (:contract-two :point 3 :mark 13 :active t :bounds (3 13) :text "(checksum)")) :reset (:reset :point 8 :mark 8 :active nil :bounds nil :text nil))"####
    ]];
    ParityBatchCase::value(
        "overshoot_can_contract_stepwise_and_reset_to_the_exact_starting_cursor",
        elisp_form,
        expected,
    )
}

fn existing_selection_expands_to_its_pair_and_autocopies_each_region() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (text-mode)
  (insert "Deploy (preview artifact) after verification")
  (neomacs-er-test-reset-command-state)
  (goto-char (point-min))
  (search-forward "preview artifact")
  (let ((selection-start (match-beginning 0))
        (selection-end (match-end 0))
        (expand-region-autocopy-register "r"))
    (set-register ?r nil)
    (goto-char selection-end)
    (set-mark selection-start)
    (activate-mark)
    (let ((expanded (neomacs-er-test-expand :expanded-to-pair))
          expanded-register contracted contracted-register)
      (setq expanded-register (get-register ?r))
      (setq contracted
            (neomacs-er-test-contract 1 :contracted-to-selection))
      (setq contracted-register (get-register ?r))
      (list :initial-selection (list selection-start selection-end)
            :expanded expanded
            :expanded-register expanded-register
            :contracted contracted
            :contracted-register contracted-register))))
"##;
    let expected = expect![[
        r####"OK (:initial-selection (9 25) :expanded (:expanded-to-pair :point 8 :mark 26 :active t :bounds (8 26) :text "(preview artifact)") :expanded-register "(preview artifact)" :contracted (:contracted-to-selection :point 25 :mark 9 :active t :bounds (9 25) :text "preview artifact") :contracted-register "preview artifact")"####
    ]];
    ParityBatchCase::value(
        "existing_selection_expands_to_its_pair_and_autocopies_each_region",
        elisp_form,
        expected,
    )
}

fn editing_after_expansion_invalidates_history_instead_of_restoring_stale_bounds() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (text-mode)
  (insert "Deploy preview artifact")
  (goto-char (point-min))
  (search-forward "preview")
  (backward-char 3)
  (neomacs-er-test-reset-command-state)
  (let ((before-edit (neomacs-er-test-expand-times 2)))
    (deactivate-mark t)
    (goto-char (point-max))
    (insert " now")
    (let ((after-contract (neomacs-er-test-contract 1 :after-contract)))
      (list :before-edit before-edit
            :buffer (buffer-string)
            :contract-after-edit after-contract))))
"##;
    let expected = expect![[
        r####"OK (:before-edit ((1 :point 8 :mark 15 :active t :bounds (8 15) :text "preview") (2 :point 1 :mark 24 :active t :bounds (1 24) :text "Deploy preview artifact")) :buffer "Deploy preview artifact now" :contract-after-edit (:after-contract :point 28 :mark 24 :active nil :bounds nil :text nil))"####
    ]];
    ParityBatchCase::value(
        "editing_after_expansion_invalidates_history_instead_of_restoring_stale_bounds",
        elisp_form,
        expected,
    )
}

fn html_release_card_grows_from_attribute_value_to_attribute_and_nested_tags() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (html-mode)
  (insert "<section><button class=\"deploy preview\"><span>Ship</span></button></section>")
  (goto-char (point-min))
  (search-forward "preview")
  (backward-char 3)
  (neomacs-er-test-reset-command-state)
  (neomacs-er-test-expand-times 8))
"##;
    let expected = expect![[
        r####"OK ((1 :point 32 :mark 39 :active t :bounds (32 39) :text "preview") (2 :point 25 :mark 39 :active t :bounds (25 39) :text "deploy preview") (3 :point 24 :mark 40 :active t :bounds (24 40) :text "\"deploy preview\"") (4 :point 18 :mark 40 :active t :bounds (18 40) :text "class=\"deploy preview\"") (5 :point 11 :mark 40 :active t :bounds (11 40) :text "button class=\"deploy preview\"") (6 :point 10 :mark 41 :active t :bounds (10 41) :text "<button class=\"deploy preview\">") (7 :point 10 :mark 67 :active t :bounds (10 67) :text "<button class=\"deploy preview\"><span>Ship</span></button>") (8 :point 1 :mark 77 :active t :bounds (1 77) :text "<section><button class=\"deploy preview\"><span>Ship</span></button></section>"))"####
    ]];
    ParityBatchCase::value(
        "html_release_card_grows_from_attribute_value_to_attribute_and_nested_tags",
        elisp_form,
        expected,
    )
}

fn python_deployment_grows_from_message_word_to_statement_and_function_block() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (python-mode)
  (insert
   "def deploy(environment):\n"
   "    message = \"Deploy preview now\"\n"
   "    return message\n")
  (goto-char (point-min))
  (search-forward "preview")
  (backward-char 3)
  (neomacs-er-test-reset-command-state)
  (neomacs-er-test-expand-times 7))
"##;
    let expected = expect![[
        r####"OK ((1 :point 48 :mark 55 :active t :bounds (48 55) :text "preview") (2 :point 41 :mark 59 :active t :bounds (41 59) :text "Deploy preview now") (3 :point 40 :mark 60 :active t :bounds (40 60) :text "\"Deploy preview now\"") (4 :point 30 :mark 60 :active t :bounds (30 60) :text "message = \"Deploy preview now\"") (5 :point 1 :mark 79 :active t :bounds (1 79) :text "def deploy(environment):\n    message = \"Deploy preview now\"\n    return message") (6 :point 1 :mark 80 :active t :bounds (1 80) :text "def deploy(environment):\n    message = \"Deploy preview now\"\n    return message\n") (7 :point 1 :mark 80 :active t :bounds (1 80) :text "def deploy(environment):\n    message = \"Deploy preview now\"\n    return message\n"))"####
    ]];
    ParityBatchCase::value(
        "python_deployment_grows_from_message_word_to_statement_and_function_block",
        elisp_form,
        expected,
    )
}

fn cpp_publish_call_grows_through_qualified_name_arguments_statement_and_block() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (c++-mode)
  (insert
   "void release() {\n"
   "  release::pipeline::publish(artifacts[index]);\n"
   "}\n")
  (goto-char (point-min))
  (search-forward "artifacts")
  (backward-char 3)
  (neomacs-er-test-reset-command-state)
  (neomacs-er-test-expand-times 8))
"##;
    let expected = expect![[
        r####"OK ((1 :point 47 :mark 56 :active t :bounds (47 56) :text "artifacts") (2 :point 47 :mark 63 :active t :bounds (47 63) :text "artifacts[index]") (3 :point 46 :mark 64 :active t :bounds (46 64) :text "(artifacts[index])") (4 :point 20 :mark 64 :active nil :bounds nil :text nil) (5 :point 20 :mark 65 :active t :bounds (20 65) :text "release::pipeline::publish(artifacts[index]);") (6 :point 16 :mark 67 :active t :bounds (16 67) :text "{\n  release::pipeline::publish(artifacts[index]);\n}") (7 :point 1 :mark 67 :active nil :bounds nil :text nil) (8 :point 1 :mark 68 :active t :bounds (1 68) :text "void release() {\n  release::pipeline::publish(artifacts[index]);\n}\n"))"####
    ]];
    ParityBatchCase::value(
        "cpp_publish_call_grows_through_qualified_name_arguments_statement_and_block",
        elisp_form,
        expected,
    )
}

#[test]
fn expand_region_package_batch() {
    assert_oracle_batch_cases(
        expand_region_oracle(),
        "expand-region-package-batch",
        "Expand Region",
        &[
            elisp_release_message_expands_from_word_to_string_expression_and_buffer(),
            overshoot_can_contract_stepwise_and_reset_to_the_exact_starting_cursor(),
            existing_selection_expands_to_its_pair_and_autocopies_each_region(),
            editing_after_expansion_invalidates_history_instead_of_restoring_stale_bounds(),
            html_release_card_grows_from_attribute_value_to_attribute_and_nested_tags(),
            python_deployment_grows_from_message_word_to_statement_and_function_block(),
            cpp_publish_call_grows_through_qualified_name_arguments_statement_and_block(),
        ],
    );
}
