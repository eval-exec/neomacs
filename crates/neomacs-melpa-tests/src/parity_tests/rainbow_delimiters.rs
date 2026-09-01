use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, RAINBOW_DELIMITERS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const RAINBOW_DELIMITERS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const RAINBOW_DELIMITERS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'rainbow-delimiters)

(defun neomacs-rainbow-test-fontify ()
  "Flush and synchronously fontify the accessible buffer."
  (font-lock-flush (point-min) (point-max))
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-rainbow-test-delimiters ()
  "Describe every delimiter-looking character in the accessible buffer."
  (save-excursion
    (goto-char (point-min))
    (let (result)
      (while (re-search-forward "[][(){}<>]" nil t)
        (let* ((position (match-beginning 0))
               (token (match-string-no-properties 0))
               (ppss (save-excursion
                       (save-match-data
                         (syntax-ppss position))))
               (face (get-text-property position 'face)))
          (push
           (list token
                 (- position (point-min))
                 (nth 0 ppss)
                 (cond ((nth 3 ppss) :string)
                       ((nth 4 ppss) :comment)
                       (t :code))
                 face)
           result)))
      (nreverse result))))

(defun neomacs-rainbow-test-angle-syntax ()
  "Describe cc-mode's observable syntax metadata for template angles."
  (save-excursion
    (goto-char (point-min))
    (let (result)
      (while (re-search-forward "[<>]" nil t)
        (let ((position (match-beginning 0))
              (token (match-string-no-properties 0)))
          (push
           (list token
                 (- position (point-min))
                 :category (get-text-property position 'category)
                 :syntax-table (get-text-property position 'syntax-table)
                 :c-type (get-text-property position 'c-type)
                 :effective-syntax (syntax-after position))
           result)))
      (nreverse result))))

(defun neomacs-rainbow-test-setup (mode text)
  "Initialize MODE with TEXT and Rainbow Delimiters enabled."
  (funcall mode)
  (font-lock-mode 1)
  (insert text)
  (rainbow-delimiters-mode 1)
  (neomacs-rainbow-test-fontify))
"##;

fn rainbow_delimiters_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RAINBOW_DELIMITERS_MELPA_PIN, "rainbow-delimiters.el")
        .expect("prepare revision-pinned Rainbow Delimiters source below ./tmp")
        .with_prelude(RAINBOW_DELIMITERS_TEST_PRELUDE)
        .with_timeout(RAINBOW_DELIMITERS_TEST_TIMEOUT)
}

fn elisp_deployment_workflow_colors_code_depth_but_not_examples_in_prose() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-rainbow-test-setup
   #'emacs-lisp-mode
   (concat
    "(defun deploy (environment)\n"
    "  ;; Example delimiters are prose: ([ignored]).\n"
    "  (let ((payload (list :environment environment\n"
    "                       :message \"ready (preview)\")))\n"
    "    (when (and environment (> (length payload) 0))\n"
    "      [payload (cons 'audit payload)])))"))
  (list :mode rainbow-delimiters-mode
        :balanced (condition-case nil
                      (progn (check-parens) t)
                    (error nil))
        :delimiters (neomacs-rainbow-test-delimiters)))
"##;
    let expected = expect![[
        r####"OK (:mode t :balanced t :delimiters (("(" 0 0 :code (rainbow-delimiters-depth-1-face)) ("(" 14 1 :code (rainbow-delimiters-depth-2-face)) (")" 26 2 :code (rainbow-delimiters-depth-2-face)) ("(" 63 1 :comment font-lock-comment-face) ("[" 64 1 :comment font-lock-comment-face) ("]" 72 1 :comment font-lock-comment-face) (")" 73 1 :comment font-lock-comment-face) ("(" 78 1 :code (rainbow-delimiters-depth-2-face)) ("(" 83 2 :code (rainbow-delimiters-depth-3-face)) ("(" 84 3 :code (rainbow-delimiters-depth-4-face)) ("(" 93 4 :code (rainbow-delimiters-depth-5-face)) ("(" 163 5 :string font-lock-string-face) (")" 171 5 :string font-lock-string-face) (")" 173 5 :code (rainbow-delimiters-depth-5-face)) (")" 174 4 :code (rainbow-delimiters-depth-4-face)) (")" 175 3 :code (rainbow-delimiters-depth-3-face)) ("(" 181 2 :code (rainbow-delimiters-depth-3-face)) ("(" 187 3 :code (rainbow-delimiters-depth-4-face)) ("(" 204 4 :code (rainbow-delimiters-depth-5-face)) (">" 205 5 :code nil) ("(" 207 5 :code (rainbow-delimiters-depth-6-face)) (")" 222 6 :code (rainbow-delimiters-depth-6-face)) (")" 225 5 :code (rainbow-delimiters-depth-5-face)) (")" 226 4 :code (rainbow-delimiters-depth-4-face)) ("[" 234 3 :code (rainbow-delimiters-depth-4-face)) ("(" 243 4 :code (rainbow-delimiters-depth-5-face)) (")" 263 5 :code (rainbow-delimiters-depth-5-face)) ("]" 264 4 :code (rainbow-delimiters-depth-4-face)) (")" 265 3 :code (rainbow-delimiters-depth-3-face)) (")" 266 2 :code (rainbow-delimiters-depth-2-face)) (")" 267 1 :code (rainbow-delimiters-depth-1-face))))"####
    ]];
    ParityBatchCase::value(
        "elisp_deployment_workflow_colors_code_depth_but_not_examples_in_prose",
        elisp_form,
        expected,
    )
}

fn repairing_a_mismatched_form_recomputes_faces_for_the_expanded_expression() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-rainbow-test-setup #'emacs-lisp-mode "(deploy [artifact))")
  (let ((broken (neomacs-rainbow-test-delimiters)))
    (goto-char (point-min))
    (search-forward "artifact")
    (delete-char 1)
    (insert "]")
    (insert " (checksum artifact)")
    (neomacs-rainbow-test-fontify)
    (list :broken broken
          :repaired-buffer
          (buffer-substring-no-properties (point-min) (point-max))
          :repaired (neomacs-rainbow-test-delimiters)
          :balanced (condition-case nil
                        (progn (check-parens) t)
                      (error nil)))))
"##;
    let expected = expect![[
        r####"OK (:broken (("(" 0 0 :code (rainbow-delimiters-depth-1-face)) ("[" 8 1 :code (rainbow-delimiters-depth-2-face)) (")" 17 2 :code (rainbow-delimiters-mismatched-face)) (")" 18 1 :code (rainbow-delimiters-depth-1-face))) :repaired-buffer "(deploy [artifact] (checksum artifact))" :repaired (("(" 0 0 :code (rainbow-delimiters-depth-1-face)) ("[" 8 1 :code (rainbow-delimiters-depth-2-face)) ("]" 17 2 :code (rainbow-delimiters-depth-2-face)) ("(" 19 1 :code (rainbow-delimiters-depth-2-face)) (")" 37 2 :code (rainbow-delimiters-depth-2-face)) (")" 38 1 :code (rainbow-delimiters-depth-1-face))) :balanced t)"####
    ]];
    ParityBatchCase::value(
        "repairing_a_mismatched_form_recomputes_faces_for_the_expanded_expression",
        elisp_form,
        expected,
    )
}

fn cpp_release_manifest_colors_templates_calls_and_initializer_lists() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-rainbow-test-setup
   #'c++-mode
   (concat
    "std::vector<std::pair<int, std::string>> build_manifest() {\n"
    "  return {{1, \"app.tar\"}, {2, \"symbols.tar\"}};\n"
    "}"))
  (list :buffer (buffer-substring-no-properties (point-min) (point-max))
        :angle-syntax (neomacs-rainbow-test-angle-syntax)
        :delimiters (neomacs-rainbow-test-delimiters)))
"##;
    let expected = expect![[
        r####"OK (:buffer "std::vector<std::pair<int, std::string>> build_manifest() {\n  return {{1, \"app.tar\"}, {2, \"symbols.tar\"}};\n}" :angle-syntax (("<" 11 :category c-<-as-paren-syntax :syntax-table #1=(4 . 62) :c-type nil :effective-syntax #1#) ("<" 21 :category c-<-as-paren-syntax :syntax-table #1# :c-type nil :effective-syntax #1#) (">" 38 :category c->-as-paren-syntax :syntax-table #2=(5 . 60) :c-type nil :effective-syntax #2#) (">" 39 :category c->-as-paren-syntax :syntax-table #2# :c-type c-decl-id-start :effective-syntax #2#)) :delimiters (("<" 11 0 :code (rainbow-delimiters-depth-1-face)) ("<" 21 1 :code (rainbow-delimiters-depth-2-face)) (">" 38 2 :code (rainbow-delimiters-depth-2-face)) (">" 39 1 :code (rainbow-delimiters-depth-1-face)) ("(" 55 0 :code (rainbow-delimiters-depth-1-face)) (")" 56 1 :code (rainbow-delimiters-depth-1-face)) ("{" 58 0 :code (rainbow-delimiters-depth-1-face)) ("{" 69 1 :code (rainbow-delimiters-depth-2-face)) ("{" 70 2 :code (rainbow-delimiters-depth-3-face)) ("}" 83 3 :code (rainbow-delimiters-depth-3-face)) ("{" 86 2 :code (rainbow-delimiters-depth-3-face)) ("}" 103 3 :code (rainbow-delimiters-depth-3-face)) ("}" 104 2 :code (rainbow-delimiters-depth-2-face)) ("}" 107 1 :code (rainbow-delimiters-depth-1-face))))"####
    ]];
    ParityBatchCase::value(
        "cpp_release_manifest_colors_templates_calls_and_initializer_lists",
        elisp_form,
        expected,
    )
}

fn diff_review_prepends_depth_faces_without_erasing_added_line_faces() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-rainbow-test-setup
   #'diff-mode
   (concat
    "diff --git a/release.el b/release.el\n"
    "@@ -1 +1 @@\n"
    "-(deploy artifact)\n"
    "+(deploy (verify artifact))\n"))
  (list :delimiters (neomacs-rainbow-test-delimiters)
        :added-line-face
        (save-excursion
          (goto-char (point-min))
          (search-forward "+(deploy")
          (get-text-property (match-beginning 0) 'face))))
"##;
    let expected = expect![[
        r####"OK (:delimiters (("(" 50 0 :code (rainbow-delimiters-depth-1-face diff-removed)) (")" 66 1 :code (rainbow-delimiters-depth-1-face diff-removed)) ("(" 69 0 :code (rainbow-delimiters-depth-1-face diff-added)) ("(" 77 1 :code (rainbow-delimiters-depth-2-face diff-added)) (")" 93 2 :code (rainbow-delimiters-depth-2-face diff-added)) (")" 94 1 :code (rainbow-delimiters-depth-1-face diff-added))) :added-line-face diff-indicator-added)"####
    ]];
    ParityBatchCase::value(
        "diff_review_prepends_depth_faces_without_erasing_added_line_faces",
        elisp_form,
        expected,
    )
}

fn deep_validation_pipeline_reserves_outer_faces_and_cycles_inner_levels() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((rainbow-delimiters-max-face-count 4)
        (rainbow-delimiters-outermost-only-face-count 2))
    (neomacs-rainbow-test-setup
     #'emacs-lisp-mode
     "(release (validate (sign (compress (bundle artifact)))))")
    (neomacs-rainbow-test-delimiters)))
"##;
    let expected = expect![[
        r####"OK (("(" 0 0 :code (rainbow-delimiters-depth-1-face)) ("(" 9 1 :code (rainbow-delimiters-depth-2-face)) ("(" 19 2 :code (rainbow-delimiters-depth-3-face)) ("(" 25 3 :code (rainbow-delimiters-depth-4-face)) ("(" 35 4 :code (rainbow-delimiters-depth-3-face)) (")" 51 5 :code (rainbow-delimiters-depth-3-face)) (")" 52 4 :code (rainbow-delimiters-depth-4-face)) (")" 53 3 :code (rainbow-delimiters-depth-3-face)) (")" 54 2 :code (rainbow-delimiters-depth-2-face)) (")" 55 1 :code (rainbow-delimiters-depth-1-face)))"####
    ]];
    ParityBatchCase::value(
        "deep_validation_pipeline_reserves_outer_faces_and_cycles_inner_levels",
        elisp_form,
        expected,
    )
}

fn custom_visual_policy_marks_outer_scope_and_keeps_mismatch_errors() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((rainbow-delimiters-pick-face-function
         (lambda (depth match loc)
           (if (and (= depth 1) match)
               'font-lock-keyword-face
             (rainbow-delimiters-default-pick-face depth match loc)))))
    (neomacs-rainbow-test-setup
     #'emacs-lisp-mode
     "(release [artifact))")
    (neomacs-rainbow-test-delimiters)))
"##;
    let expected = expect![[
        r####"OK (("(" 0 0 :code (font-lock-keyword-face)) ("[" 9 1 :code (rainbow-delimiters-depth-2-face)) (")" 18 2 :code (rainbow-delimiters-mismatched-face)) (")" 19 1 :code (font-lock-keyword-face)))"####
    ]];
    ParityBatchCase::value(
        "custom_visual_policy_marks_outer_scope_and_keeps_mismatch_errors",
        elisp_form,
        expected,
    )
}

fn disabling_the_mode_removes_only_rainbow_faces_after_refontification() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-rainbow-test-setup
   #'emacs-lisp-mode
   "(when ready (message \"ship\"))")
  (let ((enabled (neomacs-rainbow-test-delimiters)))
    (rainbow-delimiters-mode 0)
    (neomacs-rainbow-test-fontify)
    (list :enabled enabled
          :mode rainbow-delimiters-mode
          :disabled (neomacs-rainbow-test-delimiters)
          :keyword-face
          (save-excursion
            (goto-char (point-min))
            (search-forward "when")
            (get-text-property (match-beginning 0) 'face)))))
"##;
    let expected = expect![[
        r####"OK (:enabled (("(" 0 0 :code (rainbow-delimiters-depth-1-face)) ("(" 12 1 :code (rainbow-delimiters-depth-2-face)) (")" 27 2 :code (rainbow-delimiters-depth-2-face)) (")" 28 1 :code (rainbow-delimiters-depth-1-face))) :mode nil :disabled (("(" 0 0 :code nil) ("(" 12 1 :code nil) (")" 27 2 :code nil) (")" 28 1 :code nil)) :keyword-face font-lock-keyword-face)"####
    ]];
    ParityBatchCase::value(
        "disabling_the_mode_removes_only_rainbow_faces_after_refontification",
        elisp_form,
        expected,
    )
}

#[test]
fn rainbow_delimiters_package_batch() {
    let cases = vec![
        elisp_deployment_workflow_colors_code_depth_but_not_examples_in_prose(),
        repairing_a_mismatched_form_recomputes_faces_for_the_expanded_expression(),
        cpp_release_manifest_colors_templates_calls_and_initializer_lists(),
        diff_review_prepends_depth_faces_without_erasing_added_line_faces(),
        deep_validation_pipeline_reserves_outer_faces_and_cycles_inner_levels(),
        custom_visual_policy_marks_outer_scope_and_keeps_mismatch_errors(),
        disabling_the_mode_removes_only_rainbow_faces_after_refontification(),
    ];
    assert_oracle_batch_cases(
        rainbow_delimiters_oracle(),
        "parity_tests::rainbow_delimiters::rainbow_delimiters_package_batch",
        "rainbow_delimiters_parity",
        &cases,
    );
}
