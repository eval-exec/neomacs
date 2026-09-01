use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_NERD_COMMENTER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EVIL_NERD_COMMENTER_TEST_TIMEOUT: Duration = Duration::from_secs(90);
const EVIL_NERD_COMMENTER_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'evil-nerd-commenter)

(defun neomacs-evilnc-test-position ()
  "Describe point in a stable, user-visible form."
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :line-text (buffer-substring-no-properties
                    (line-beginning-position)
                    (line-end-position))))

(defun neomacs-evilnc-test-select (begin end)
  "Activate the region from BEGIN through END."
  (goto-char begin)
  (set-mark end)
  (setq mark-active t)
  (activate-mark))

(defun neomacs-evilnc-test-index-report (index)
  "Describe Evil Nerd Commenter's INDEX without buffer identities."
  (mapcar
   (lambda (entry)
     (let ((marker (cdr entry)))
       (save-excursion
         (goto-char marker)
         (list :summary (car entry)
               :position (marker-position marker)
               :line (line-number-at-pos)
               :column (current-column)))))
   index))
"###;

fn evil_nerd_commenter_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_NERD_COMMENTER_MELPA_PIN, "evil-nerd-commenter.el")
        .expect("prepare revision-pinned Evil Nerd Commenter source below ./tmp")
        .with_prelude(EVIL_NERD_COMMENTER_TEST_PRELUDE)
        .with_timeout(EVIL_NERD_COMMENTER_TEST_TIMEOUT)
}

fn multi_line_javascript_toggle_preserves_cursor_and_round_trips_source() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (js-mode)
  (insert "function deploy(release) {\n"
          "  validate(release);\n"
          "  publish(release);\n"
          "  notify(release);\n"
          "}\n")
  (goto-char (point-min))
  (search-forward "validate")
  (let ((before (neomacs-evilnc-test-position)))
    (evilnc-comment-or-uncomment-lines 3)
    (let ((commented (buffer-string))
          (after-comment (neomacs-evilnc-test-position)))
      (evilnc-comment-or-uncomment-lines 3)
      (list :before before
            :commented commented
            :after-comment after-comment
            :restored (buffer-string)
            :after-uncomment (neomacs-evilnc-test-position)))))
"###;
    let expected = expect![[
        r#"OK (:before (:point 38 :line 2 :column 10 :line-text "  validate(release);") :commented "function deploy(release) {\n  // validate(release);\n  // publish(release);\n  // notify(release);\n}\n" :after-comment (:point 38 :line 2 :column 10 :line-text "  // validate(release);") :restored "function deploy(release) {\n  validate(release);\n  publish(release);\n  notify(release);\n}\n" :after-uncomment (:point 38 :line 2 :column 10 :line-text "  validate(release);"))"#
    ]];
    ParityBatchCase::value(
        "multi_line_javascript_toggle_preserves_cursor_and_round_trips_source",
        elisp_form,
        expected,
    )
}

fn negative_count_inverts_production_and_fallback_branches_line_by_line() -> ParityBatchCase {
    let elisp_form = r###"
(let ((evilnc-invert-comment-line-by-line t)
      (inhibit-message t))
  (with-temp-buffer
    (js-mode)
    (insert "if (featureEnabled) { deployCanary(); }\n"
            "// if (!featureEnabled) { deployStable(); }\n"
            "recordDecision();\n")
    (goto-char (point-min))
    (forward-line 1)
    (search-forward "deployStable")
    (let ((before (neomacs-evilnc-test-position)))
      (evilnc-comment-or-uncomment-lines -2)
      (list :before before
            :after (neomacs-evilnc-test-position)
            :source (buffer-string)
            :invert-enabled evilnc-invert-comment-line-by-line))))
"###;
    let expected = expect![[
        r#"OK (:before (:point 79 :line 2 :column 38 :line-text "// if (!featureEnabled) { deployStable(); }") :after (:point 82 :line 2 :column 38 :line-text "if (!featureEnabled) { deployStable(); }") :source "// if (featureEnabled) { deployCanary(); }\nif (!featureEnabled) { deployStable(); }\nrecordDecision();\n" :invert-enabled t)"#
    ]];
    ParityBatchCase::value(
        "negative_count_inverts_production_and_fallback_branches_line_by_line",
        elisp_form,
        expected,
    )
}

fn inline_javascript_region_uses_block_comments_and_restores_mode_syntax() -> ParityBatchCase {
    let elisp_form = r###"
(let ((transient-mark-mode t))
  (with-temp-buffer
    (js-mode)
    (insert "if (featureEnabled && region === \"east\") {\n"
            "  deployCanary();\n"
            "}\n")
    (goto-char (point-min))
    (search-forward "featureEnabled")
    (let ((begin (match-beginning 0)))
      (search-forward "\"east\"")
      (let ((end (match-end 0))
            (syntax-before
             (list comment-start comment-end
                   comment-start-skip comment-end-skip)))
        (neomacs-evilnc-test-select begin end)
        (evilnc-comment-or-uncomment-lines 1)
        (list :source (buffer-string)
              :position (neomacs-evilnc-test-position)
              :region (and (region-active-p)
                           (list (region-beginning) (region-end)))
              :syntax-before syntax-before
              :syntax-after
              (list comment-start comment-end
                    comment-start-skip comment-end-skip))))))
"###;
    let expected = expect![[
        r#"OK (:source "if (/* featureEnabled && region === \"east\" */) {\n  deployCanary();\n}\n" :position (:point 5 :line 1 :column 4 :line-text "if (/* featureEnabled && region === \"east\" */) {") :region (5 43) :syntax-before ("// " "" "\\(?://+\\|/\\*+\\)\\s *" nil) :syntax-after ("// " "" "\\(?://+\\|/\\*+\\)\\s *" nil))"#
    ]];
    ParityBatchCase::value(
        "inline_javascript_region_uses_block_comments_and_restores_mode_syntax",
        elisp_form,
        expected,
    )
}

fn copy_comment_and_kill_ring_workflows_keep_an_executable_deployment_copy() -> ParityBatchCase {
    let elisp_form = r###"
(let ((evilnc-original-above-comment-when-copy-and-comment nil)
      (kill-ring nil)
      (kill-ring-yank-pointer nil)
      (interprogram-cut-function nil))
  (let* ((copy-report
          (with-temp-buffer
            (js-mode)
            (insert "prepareRelease(\"REL-2048\");\n"
                    "deployCanary(\"us-east-1\");\n"
                    "auditDeployment();\n")
            (goto-char (point-min))
            (move-to-column 7)
            (evilnc-copy-and-comment-lines 2)
            (list :source (buffer-string)
                  :position (neomacs-evilnc-test-position))))
         (kill-report
          (with-temp-buffer
            (js-mode)
            (insert "prepareRollback(\"REL-2048\");\n"
                    "deployStable(\"us-east-1\");\n"
                    "closeIncident();\n")
            (goto-char (point-min))
            (evilnc-comment-and-kill-ring-save 2)
            (list :source (buffer-string)
                  :position (neomacs-evilnc-test-position)
                  :kill-ring kill-ring
                  :latest-kill (current-kill 0 t)))))
    (list :copy-and-comment copy-report
          :comment-and-save kill-report)))
"###;
    let expected = expect![[
        r#"OK (:copy-and-comment (:source "// prepareRelease(\"REL-2048\");\n// deployCanary(\"us-east-1\");\nprepareRelease(\"REL-2048\");\ndeployCanary(\"us-east-1\");\nauditDeployment();\n" :position (:point 97 :line 4 :column 7 :line-text "deployCanary(\"us-east-1\");")) :comment-and-save (:source "// prepareRollback(\"REL-2048\");\n// deployStable(\"us-east-1\");\ncloseIncident();\n" :position (:point 1 :line 1 :column 0 :line-text "// prepareRollback(\"REL-2048\");") :kill-ring ("prepareRollback(\"REL-2048\");\ndeployStable(\"us-east-1\");") :latest-kill "prepareRollback(\"REL-2048\");\ndeployStable(\"us-east-1\");"))"#
    ]];
    ParityBatchCase::value(
        "copy_comment_and_kill_ring_workflows_keep_an_executable_deployment_copy",
        elisp_form,
        expected,
    )
}

fn paragraph_toggle_preserves_blank_runbook_boundaries_and_round_trips() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (js-mode)
  (insert "prepareRelease();\n"
          "validateArtifacts();\n"
          "\n"
          "deployCanary();\n"
          "observeHealth();\n"
          "\n"
          "promoteStable();\n")
  (goto-char (point-min))
  (evilnc-comment-or-uncomment-paragraphs 2)
  (let ((commented (buffer-string))
        (after-comment (neomacs-evilnc-test-position)))
    (goto-char (point-min))
    (evilnc-comment-or-uncomment-paragraphs 1)
    (goto-char (point-min))
    (search-forward "deployCanary")
    (evilnc-comment-or-uncomment-paragraphs 1)
    (list :commented commented
          :after-comment after-comment
          :restored (buffer-string)
          :after-uncomment (neomacs-evilnc-test-position))))
"###;
    let expected = expect![[
        r#"OK (:commented "// prepareRelease();\n// validateArtifacts();\n\n// deployCanary();\n// observeHealth();\n\npromoteStable();\n" :after-comment (:point 87 :line 7 :column 0 :line-text "promoteStable();") :restored "prepareRelease();\nvalidateArtifacts();\n\ndeployCanary();\nobserveHealth();\n\npromoteStable();\n" :after-uncomment (:point 75 :line 7 :column 0 :line-text "promoteStable();"))"#
    ]];
    ParityBatchCase::value(
        "paragraph_toggle_preserves_blank_runbook_boundaries_and_round_trips",
        elisp_form,
        expected,
    )
}

fn html_and_jsx_tag_workflows_use_contextual_delimiters_and_restore_markup() -> ParityBatchCase {
    let elisp_form = r###"
(let ((html-report
       (with-temp-buffer
         (html-mode)
         (insert "<section id=\"release\">\n"
                 "  <h2>REL-2048</h2>\n"
                 "  <p>Canary ready</p>\n"
                 "</section>")
         (goto-char (point-min))
         (evilnc-comment-or-uncomment-html-tag)
         (let ((commented (buffer-string)))
           (goto-char (point-min))
           (forward-line 1)
           (evilnc-comment-or-uncomment-html-tag)
           (list :commented commented
                 :restored (buffer-string)))))
      (jsx-report
       (with-temp-buffer
         (setq buffer-file-name "release-dashboard.jsx")
         (js-mode)
         (insert "<ReleaseCard status=\"canary\">REL-2048</ReleaseCard>")
         (goto-char (point-min))
         (evilnc-comment-or-uncomment-html-tag)
         (list :comment-start (evilnc-html-comment-start)
               :comment-end (evilnc-html-comment-end)
               :source (buffer-substring-no-properties
                        (point-min) (point-max))))))
  (list :html html-report :jsx jsx-report))
"###;
    let expected = expect![[
        r#"OK (:html (:commented "<!-- <section id=\"release\">\n  <h2>REL-2048</h2>\n  <p>Canary ready</p>\n</section> -->" :restored "<section id=\"release\">\n  <h2>REL-2048</h2>\n  <p>Canary ready</p>\n</section>") :jsx (:comment-start "{/* " :comment-end " */}" :source "{/* <ReleaseCard status=\"canary\">REL-2048</ReleaseCard> */}"))"#
    ]];
    ParityBatchCase::value(
        "html_and_jsx_tag_workflows_use_contextual_delimiters_and_restore_markup",
        elisp_form,
        expected,
    )
}

fn org_python_source_block_uses_language_comments_without_touching_the_document() -> ParityBatchCase
{
    let elisp_form = r###"
(require 'org)
(with-temp-buffer
  (insert "* Release runbook\n"
          "#+BEGIN_SRC python\n"
          "def deploy(release):\n"
          "    validate_release(release)\n"
          "    publish_release(release)\n"
          "#+END_SRC\n"
          "\n"
          "Operational notes stay in Org.\n")
  (org-mode)
  (goto-char (point-min))
  (search-forward "validate_release")
  (let* ((info (evilnc--org-src-block-info))
         (language-mode (evilnc--org-lang-major-mode info)))
    (evilnc-comment-or-uncomment-lines 2)
    (let ((commented (buffer-string))
          (after-comment (neomacs-evilnc-test-position)))
      (goto-char (point-min))
      (search-forward "validate_release")
      (evilnc-comment-or-uncomment-lines 2)
      (list :block
            (list :begin (nth 0 info)
                  :end (nth 1 info)
                  :language (nth 2 info)
                  :mode language-mode)
            :commented commented
            :after-comment after-comment
            :restored (buffer-string)
            :after-uncomment (neomacs-evilnc-test-position)))))
"###;
    let expected = expect![[
        r#"OK (:block (:begin 38 :end 118 :language "python" :mode python-mode) :commented "* Release runbook\n#+BEGIN_SRC python\ndef deploy(release):\n    # validate_release(release)\n    # publish_release(release)\n#+END_SRC\n\nOperational notes stay in Org.\n" :after-comment (:point 79 :line 4 :column 20 :line-text "    # validate_release(release)") :restored "* Release runbook\n#+BEGIN_SRC python\ndef deploy(release):\n    validate_release(release)\n    publish_release(release)\n#+END_SRC\n\nOperational notes stay in Org.\n" :after-uncomment (:point 81 :line 4 :column 22 :line-text "    validate_release(release)"))"#
    ]];
    ParityBatchCase::value(
        "org_python_source_block_uses_language_comments_without_touching_the_document",
        elisp_form,
        expected,
    )
}

fn imenu_comment_index_surfaces_actionable_release_notes_and_skips_noise() -> ParityBatchCase {
    let elisp_form = r###"
(let ((evilnc-min-comment-length-for-imenu 16))
  (with-temp-buffer
    (js-mode)
    (insert "function deploy() {\n"
            "  // Promote REL-2048 to canary after health checks\n"
            "  publishRelease();\n"
            "  // retry\n"
            "  observeCanary();\n"
            "}\n"
            "// Roll back REL-2048 when error budget is exhausted\n"
            "rollbackRelease();\n")
    (font-lock-ensure (point-min) (point-max))
    (list :index
          (neomacs-evilnc-test-index-report
           (evilnc-imenu-create-index-function))
          :source (buffer-substring-no-properties
                   (point-min) (point-max)))))
"###;
    let expected = expect![[
        r#"OK (:index ((:summary "2:Promote REL-2048 to canary after health checks" :position 26 :line 2 :column 5) (:summary "7:Roll back REL-2048 when error budget is exhausted" :position 128 :line 7 :column 3)) :source "function deploy() {\n  // Promote REL-2048 to canary after health checks\n  publishRelease();\n  // retry\n  observeCanary();\n}\n// Roll back REL-2048 when error budget is exhausted\nrollbackRelease();\n")"#
    ]];
    ParityBatchCase::value(
        "imenu_comment_index_surfaces_actionable_release_notes_and_skips_noise",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_nerd_commenter_package_batch() {
    assert_oracle_batch_cases(
        evil_nerd_commenter_oracle(),
        "evil-nerd-commenter-package-batch",
        "evil-nerd-commenter",
        &[
            multi_line_javascript_toggle_preserves_cursor_and_round_trips_source(),
            negative_count_inverts_production_and_fallback_branches_line_by_line(),
            inline_javascript_region_uses_block_comments_and_restores_mode_syntax(),
            copy_comment_and_kill_ring_workflows_keep_an_executable_deployment_copy(),
            paragraph_toggle_preserves_blank_runbook_boundaries_and_round_trips(),
            html_and_jsx_tag_workflows_use_contextual_delimiters_and_restore_markup(),
            org_python_source_block_uses_language_comments_without_touching_the_document(),
            imenu_comment_index_surfaces_actionable_release_notes_and_skips_noise(),
        ],
    );
}
