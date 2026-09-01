use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TOC_ORG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TOC_ORG_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const TOC_ORG_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'toc-org)

(unless (fboundp 'markdown-mode)
  (define-derived-mode markdown-mode text-mode "Markdown"
    "Minimal Markdown mode for Toc-Org parity workflows."))

(defun neomacs-toc-org-test-hash-entries ()
  "Return Toc-Org's link translation table in deterministic order."
  (when toc-org-hrefify-hash
    (sort (cl-loop for key being the hash-keys of toc-org-hrefify-hash
                   using (hash-values value)
                   collect (cons key value))
          (lambda (left right) (string< (car left) (car right))))))

(defun neomacs-toc-org-test-state ()
  "Return the current document and editing state."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :modified (buffer-modified-p)
        :hash (neomacs-toc-org-test-hash-entries)))
"####;

fn toc_org_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TOC_ORG_MELPA_PIN, "toc-org.el")
        .expect("prepare revision-pinned Toc-Org source below ./tmp")
        .with_prelude(TOC_ORG_TEST_PRELUDE)
        .with_timeout(TOC_ORG_TEST_TIMEOUT)
}

fn org_release_document_generates_a_depth_limited_idempotent_toc() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Release handbook\n"
          "* Contents :TOC_2:\n"
          "stale entry\n"
          "* Release Overview\n"
          "** API Contract\n"
          "*** Failure Modes\n"
          "* Deployment Guide\n"
          "** Canary Rollout\n"
          "*** Rollback Checklist\n")
  (toc-org-insert-toc)
  (let ((first (buffer-substring-no-properties (point-min) (point-max))))
    (set-buffer-modified-p nil)
    (toc-org-insert-toc)
    (list :first first
          :second (buffer-substring-no-properties (point-min) (point-max))
          :idempotent (not (buffer-modified-p)))))
"####;
    let expected = expect![[
        r####"OK (:first "#+TITLE: Release handbook\n* Contents :TOC_2:\n- [[#release-overview][Release Overview]]\n  - [[#api-contract][API Contract]]\n- [[#deployment-guide][Deployment Guide]]\n  - [[#canary-rollout][Canary Rollout]]\n\n* Release Overview\n** API Contract\n*** Failure Modes\n* Deployment Guide\n** Canary Rollout\n*** Rollback Checklist\n" :second "#+TITLE: Release handbook\n* Contents :TOC_2:\n- [[#release-overview][Release Overview]]\n  - [[#api-contract][API Contract]]\n- [[#deployment-guide][Deployment Guide]]\n  - [[#canary-rollout][Canary Rollout]]\n\n* Release Overview\n** API Contract\n*** Failure Modes\n* Deployment Guide\n** Canary Rollout\n*** Rollback Checklist\n" :idempotent t)"####
    ]];
    ParityBatchCase::value(
        "org_release_document_generates_a_depth_limited_idempotent_toc",
        elisp_form,
        expected,
    )
}

fn publication_filters_strip_workflow_metadata_and_private_subtrees() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (org-mode)
  (insert "#+TODO: PLAN(p) REVIEW(r) | SHIPPED(s)\n"
          "* Contents :TOC_3:\n"
          "* PLAN [[https://example.invalid/release][Release Plan]] [40%] :public:\n"
          "** [#A] API Contract [2/4] :owners:\n"
          "*** Error Semantics\n"
          "* COMMENT Internal Draft\n"
          "** Secret Token Rotation\n"
          "* Operations :noexport:\n"
          "** Production Credentials\n"
          "* REVIEW Deployment\n"
          "** Canary\n")
  (let* ((raw (toc-org-raw-toc nil))
         (depth-two (toc-org-flush-subheadings raw 2))
         (formatted
          (toc-org-hrefify-toc depth-two #'toc-org-hrefify-gh nil)))
    (list :raw raw :depth-two depth-two :formatted formatted)))
"####;
    let expected = expect![[
        r####"OK (:raw "* Release Plan [40%]\n** API Contract [2/4]\n*** Error Semantics\n* Deployment\n** Canary\n" :depth-two "* Release Plan [40%]\n** API Contract [2/4]\n* Deployment\n** Canary\n" :formatted "- [[#release-plan-40][Release Plan]]\n  - [[#api-contract-24][API Contract]]\n- [[#deployment][Deployment]]\n  - [[#canary][Canary]]\n")"####
    ]];
    ParityBatchCase::value(
        "publication_filters_strip_workflow_metadata_and_private_subtrees",
        elisp_form,
        expected,
    )
}

fn markdown_readme_ignores_fenced_examples_and_deduplicates_github_anchors() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (markdown-mode)
  (insert "# Project\n\n"
          "```markdown\n"
          "# Example Heading\n"
          "## Nested Example\n"
          "```\n\n"
          "## Install ##\n"
          "## Install\n"
          "### Linux\n\n"
          "# Contents <-- :TOC_3: -->\n"
          "old\n")
  (setq toc-org-hrefify-hash (make-hash-table :test 'equal))
  (toc-org-insert-toc)
  (neomacs-toc-org-test-state))
"####;
    let expected = expect![[
        r####"OK (:text "# Project\n\n```markdown\n# Example Heading\n## Nested Example\n```\n\n## Install ##\n## Install\n### Linux\n\n# Contents <-- :TOC_3: -->\n- [Project](#project)\n  - [Install](#install)\n  - [Install](#install-1)\n    - [Linux](#linux)\n" :point 128 :modified t :hash (("#install" . "Install") ("#install-1" . "Install") ("#linux" . "Linux") ("#project" . "Project")))"####
    ]];
    ParityBatchCase::value(
        "markdown_readme_ignores_fenced_examples_and_deduplicates_github_anchors",
        elisp_form,
        expected,
    )
}

fn dry_run_builds_org_link_translation_without_modifying_the_document() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (org-mode)
  (insert "* Contents :TOC_2:\n"
          "- [[#deployment][Deployment]]\n"
          "- [[#deployment-1][Deployment]]\n"
          "* Deployment\n"
          "* Deployment\n")
  (setq toc-org-hrefify-hash (make-hash-table :test 'equal))
  (set-buffer-modified-p nil)
  (toc-org-insert-toc t)
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :modified (buffer-modified-p)
        :hash (neomacs-toc-org-test-hash-entries)
        :custom-id (toc-org-unhrefify "custom-id" "deployment-1")
        :legacy (toc-org-unhrefify "thisfile" "#deployment")
        :disabled
        (let ((toc-org-enable-links-opening nil))
          (toc-org-unhrefify "custom-id" "deployment"))))
"####;
    let expected = expect![[
        r####"OK (:text "* Contents :TOC_2:\n- [[#deployment][Deployment]]\n- [[#deployment-1][Deployment]]\n* Deployment\n* Deployment\n" :modified nil :hash (("#deployment" . "Deployment") ("#deployment-1" . "Deployment")) :custom-id ("fuzzy" . "Deployment") :legacy ("thisfile" . "Deployment") :disabled ("custom-id" . "deployment"))"####
    ]];
    ParityBatchCase::value(
        "dry_run_builds_org_link_translation_without_modifying_the_document",
        elisp_form,
        expected,
    )
}

fn mode_save_hook_updates_live_headings_and_disable_stops_future_rewrites() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (org-mode)
  (insert "* Contents :TOC_1:\n* Overview\n")
  (toc-org-mode 1)
  (goto-char (point-max))
  (insert "* Deployment\n")
  (run-hooks 'before-save-hook)
  (let ((enabled
         (list :mode toc-org-mode
               :hook (and (memq #'toc-org-insert-toc before-save-hook) t)
               :text (buffer-substring-no-properties
                      (point-min) (point-max))
               :translation org-link-translation-function)))
    (toc-org-mode -1)
    (goto-char (point-max))
    (insert "* Rollback\n")
    (run-hooks 'before-save-hook)
    (list :enabled enabled
          :disabled
          (list :mode toc-org-mode
                :hook (and (memq #'toc-org-insert-toc before-save-hook) t)
                :text (buffer-substring-no-properties
                       (point-min) (point-max))
                :translation org-link-translation-function))))
"####;
    let expected = expect![[
        r####"OK (:enabled (:mode t :hook t :text "* Contents :TOC_1:\n- [[#overview][Overview]]\n- [[#deployment][Deployment]]\n\n* Overview\n* Deployment\n" :translation toc-org-unhrefify) :disabled (:mode nil :hook nil :text "* Contents :TOC_1:\n- [[#overview][Overview]]\n- [[#deployment][Deployment]]\n\n* Overview\n* Deployment\n* Rollback\n" :translation nil))"####
    ]];
    ParityBatchCase::value(
        "mode_save_hook_updates_live_headings_and_disable_stops_future_rewrites",
        elisp_form,
        expected,
    )
}

fn markdown_links_navigate_to_real_headings_and_fallback_when_not_on_a_link() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (markdown-mode)
  (insert "# Contents <-- :TOC_2: -->\n"
          "- [Deployment](#deployment)\n"
          "\n# Overview\n"
          "\n# Deployment\n"
          "Ship safely.\n")
  (goto-char (point-min))
  (search-forward "Deployment](")
  (backward-char 3)
  (toc-org-follow-markdown-link)
  (let ((followed
         (list :point (point)
               :line (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position))))
        fallback-arg)
    (goto-char (point-max))
    (cl-letf (((symbol-function 'markdown-follow-thing-at-point)
               (lambda (arg) (setq fallback-arg arg))))
      (toc-org-markdown-follow-thing-at-point '(4)))
    (list :followed followed
          :fallback fallback-arg
          :fallback-point (point))))
"####;
    let expected = expect![[
        r####"OK (:followed (:point 69 :line "# Deployment") :fallback (4) :fallback-point 95)"####
    ]];
    ParityBatchCase::value(
        "markdown_links_navigate_to_real_headings_and_fallback_when_not_on_a_link",
        elisp_form,
        expected,
    )
}

fn quoted_org_style_toc_respects_depth_and_list_indentation() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (org-mode)
  (insert "* Contents :TOC_3_org:QUOTE:\n"
          "* Release [75%]\n"
          "** Build\n"
          "*** Linux\n"
          "**** Package\n"
          "* Operations\n")
  (let ((org-list-indent-offset 1))
    (toc-org-insert-toc))
  (buffer-substring-no-properties (point-min) (point-max)))
"####;
    let expected = expect![[
        r####"OK "* Contents :TOC_3_org:QUOTE:\n#+BEGIN_QUOTE\n- [[Release][Release]]\n   - [[Build][Build]]\n      - [[Linux][Linux]]\n- [[Operations][Operations]]\n#+END_QUOTE\n\n* Release [75%]\n** Build\n*** Linux\n**** Package\n* Operations\n""####
    ]];
    ParityBatchCase::value(
        "quoted_org_style_toc_respects_depth_and_list_indentation",
        elisp_form,
        expected,
    )
}

#[test]
fn toc_org_package_batch() {
    let cases = vec![
        org_release_document_generates_a_depth_limited_idempotent_toc(),
        publication_filters_strip_workflow_metadata_and_private_subtrees(),
        markdown_readme_ignores_fenced_examples_and_deduplicates_github_anchors(),
        dry_run_builds_org_link_translation_without_modifying_the_document(),
        mode_save_hook_updates_live_headings_and_disable_stops_future_rewrites(),
        markdown_links_navigate_to_real_headings_and_fallback_when_not_on_a_link(),
        quoted_org_style_toc_respects_depth_and_list_indentation(),
    ];
    assert_oracle_batch_cases(toc_org_oracle(), "toc-org-package-batch", "Toc-Org", &cases);
}
