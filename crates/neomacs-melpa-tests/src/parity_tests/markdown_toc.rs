use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MARKDOWN_TOC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'markdown-toc)

(defun neomacs-markdown-toc-test-state ()
  "Return stable document and editor state for a TOC workflow."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :modified (buffer-modified-p)
        :toc-present (and (markdown-toc--toc-already-present-p) t)))

(defun neomacs-markdown-toc-test-generate (source marker &optional preset)
  "Generate a TOC in SOURCE after MARKER using PRESET."
  (with-temp-buffer
    (markdown-mode)
    (insert source)
    (font-lock-ensure (point-min) (point-max))
    (goto-char (point-min))
    (search-forward marker)
    (let ((markdown-toc-preset (or preset 'legacy)))
      (markdown-toc-generate-toc))
    (neomacs-markdown-toc-test-state)))
"###;

fn package_and_minor_mode_contract_expose_the_documentation_workflow() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'markdown-toc package-alist))))
  (with-temp-buffer
    (markdown-mode)
    (let ((before (list markdown-toc-mode
                        (assq 'markdown-toc-mode minor-mode-alist))))
      (markdown-toc-mode 1)
      (let ((enabled (list markdown-toc-mode
                           (assq 'markdown-toc-mode minor-mode-alist))))
        (markdown-toc-mode -1)
        (list
         :package
         (list :name (package-desc-name descriptor)
               :version (package-version-join (package-desc-version descriptor))
               :requirements (package-desc-reqs descriptor)
               :features (mapcar #'featurep
                                  '(markdown-toc markdown-toc-pandoc)))
         :commands
         (mapcar #'commandp
                 '(markdown-toc-generate-toc
                   markdown-toc-generate-or-refresh-toc
                   markdown-toc-refresh-toc
                   markdown-toc-delete-toc
                   markdown-toc-follow-link-at-point
                   markdown-toc-version))
         :aliases
         (list (eq (indirect-function 'markdown-toc/generate-toc)
                   (indirect-function 'markdown-toc-generate-toc))
               (eq (indirect-function 'markdown-toc/version)
                   (indirect-function 'markdown-toc-version)))
         :defaults
         (list markdown-toc--toc-version markdown-toc-list-item-marker
               markdown-toc-indentation-space markdown-toc-preset
               markdown-toc-header-toc-start markdown-toc-header-toc-title
               markdown-toc-header-toc-end)
         :mode (list :before before :enabled enabled
                     :disabled markdown-toc-mode
                     :lighter (assq 'markdown-toc-mode minor-mode-alist)))))))
"###;
    let expected = expect![[
        r#"OK (:package (:name markdown-toc :version "20260131.1444" :requirements ((emacs (28 1)) (markdown-mode (2 1)) (dash (2 11 0)) (s (1 9 0))) :features (t t)) :commands (t t t t t t) :aliases (t t) :defaults ("0.1.6" "-" 2 legacy "<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->" "**Table of Contents**" "<!-- markdown-toc end -->") :mode (:before (nil #1=(markdown-toc-mode " mt")) :enabled (t #1#) :disabled nil :lighter #1#))"#
    ]];
    ParityBatchCase::value(
        "package_and_minor_mode_contract_expose_the_documentation_workflow",
        elisp_form,
        expected,
    )
}

fn production_readme_generates_nested_toc_at_the_authors_insertion_point() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-markdown-toc-test-generate
 "---
title: Checkout Service
---
# Checkout Service

Operational release notes.

## Installation

### Container image

```markdown
# Example only
## Do not publish
```

Configuration
=============

### Retry policy

## Operations

### Canary rollout
#### Rollback checklist
"
 "Operational release notes.\n")
"###;
    let expected = expect![[
        r#"OK (:text "---\ntitle: Checkout Service\n---\n# Checkout Service\n\nOperational release notes.\n<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->\n**Table of Contents**\n\n- [Checkout Service](#checkout-service)\n  - [Installation](#installation)\n    - [Container image](#container-image)\n- [Configuration](#configuration)\n  - [-](#-)\n  - [Operations](#operations)\n    - [Canary rollout](#canary-rollout)\n      - [Rollback checklist](#rollback-checklist)\n\n<!-- markdown-toc end -->\n\n## Installation\n\n### Container image\n\n```markdown\n# Example only\n## Do not publish\n```\n\nConfiguration\n=============\n\n### Retry policy\n\n## Operations\n\n### Canary rollout\n#### Rollback checklist\n" :point 80 :line 7 :column 0 :modified t :toc-present t)"#
    ]];
    ParityBatchCase::value(
        "production_readme_generates_nested_toc_at_the_authors_insertion_point",
        elisp_form,
        expected,
    )
}

fn refreshing_after_heading_edits_is_idempotent_and_preserves_the_working_point() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (markdown-mode)
  (insert "# Release Handbook\n\n## Build\n\n### Linux\n\n## Deploy\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "Release Handbook\n")
  (markdown-toc-generate-toc)
  (goto-char (point-max))
  (insert "\n### Canary\n### Rollback\n")
  (goto-char (point-min))
  (search-forward "## Build")
  (replace-match "## Compile & Package" t t)
  (goto-char (point-max))
  (backward-char 2)
  (let ((point-before (point)))
    (markdown-toc-refresh-toc)
    (let ((first (buffer-substring-no-properties (point-min) (point-max)))
          (point-after (point)))
      (set-buffer-modified-p nil)
      (markdown-toc-generate-or-refresh-toc)
      (list :first first
            :second (buffer-substring-no-properties (point-min) (point-max))
            :point-before point-before
            :point-after point-after
            :point-after-second (point)
            :same-content
            (equal first (buffer-substring-no-properties (point-min) (point-max)))
            :second-modified (buffer-modified-p)))))
"###;
    let expected = expect![[
        r##"OK (:first "# Release Handbook\n<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->\n**Table of Contents**\n\n- [Release Handbook](#release-handbook)\n  - [Compile & Package](#compile--package)\n    - [Linux](#linux)\n  - [Deploy](#deploy)\n    - [Canary](#canary)\n    - [Rollback](#rollback)\n\n<!-- markdown-toc end -->\n\n## Compile & Package\n\n### Linux\n\n## Deploy\n\n### Canary\n### Rollback\n" :second "# Release Handbook\n<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->\n**Table of Contents**\n\n- [Release Handbook](#release-handbook)\n  - [Compile & Package](#compile--package)\n    - [Linux](#linux)\n  - [Deploy](#deploy)\n    - [Canary](#canary)\n    - [Rollback](#rollback)\n\n<!-- markdown-toc end -->\n\n## Compile & Package\n\n### Linux\n\n## Deploy\n\n### Canary\n### Rollback\n" :point-before 329 :point-after 404 :point-after-second 404 :same-content t :second-modified t)"##
    ]];
    ParityBatchCase::value(
        "refreshing_after_heading_edits_is_idempotent_and_preserves_the_working_point",
        elisp_form,
        expected,
    )
}

fn custom_publishing_policy_changes_headers_numbering_indentation_and_scope() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (markdown-mode)
  (insert "# Internal Runbook\n\n## Public API\n### Authentication\n### Errors\n\n## Operations\n### On-call\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "Internal Runbook\n")
  (let ((markdown-toc-header-toc-start "<!-- generated navigation -->")
        (markdown-toc-header-toc-title "**Runbook navigation**")
        (markdown-toc-header-toc-end "<!-- end generated navigation -->")
        (markdown-toc-list-item-marker "1.")
        (markdown-toc-indentation-space 4)
        (markdown-toc-user-toc-structure-manipulation-fn
         (lambda (structure)
           (mapcar (lambda (entry) (cons (max 0 (1- (car entry))) (cdr entry)))
                   (cdr structure)))))
    (markdown-toc-generate-toc))
  (neomacs-markdown-toc-test-state))
"###;
    let expected = expect![[
        r##"OK (:text "# Internal Runbook\n<!-- generated navigation -->\n**Runbook navigation**\n\n1. [Public API](#public-api)\n    1. [Authentication](#authentication)\n    1. [Errors](#errors)\n1. [Operations](#operations)\n    1. [On-call](#on-call)\n\n<!-- end generated navigation -->\n\n## Public API\n### Authentication\n### Errors\n\n## Operations\n### On-call\n" :point 20 :line 2 :column 0 :modified t :toc-present nil)"##
    ]];
    ParityBatchCase::value(
        "custom_publishing_policy_changes_headers_numbering_indentation_and_scope",
        elisp_form,
        expected,
    )
}

fn legacy_github_anchors_cover_duplicates_punctuation_underscores_and_unicode() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-markdown-toc-test-generate
 "# API v2.0: Orders & Payments\n
## Retry-policy_status\n
## Retry-policy_status\n
### `POST /orders/{id}`\n
## 配置 SPF Sender Policy Framework 记录\n
## Ship (~snapshot)!\n"
 "API v2.0: Orders & Payments\n"
 'legacy)
"###;
    let expected = expect![[
        r##"OK (:text "# API v2.0: Orders & Payments\n<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->\n**Table of Contents**\n\n- [API v2.0: Orders & Payments](#api-v20-orders--payments)\n  - [Retry-policy_status](#retry-policy_status)\n  - [Retry-policy_status](#retry-policy_status-1)\n    - [`POST /orders/{id}`](#post-ordersid)\n  - [配置 SPF Sender Policy Framework 记录](#-spf-sender-policy-framework-)\n  - [Ship (~snapshot)!](#ship-snapshot)\n\n<!-- markdown-toc end -->\n\n## Retry-policy_status\n\n## Retry-policy_status\n\n### `POST /orders/{id}`\n\n## 配置 SPF Sender Policy Framework 记录\n\n## Ship (~snapshot)!\n" :point 31 :line 2 :column 0 :modified t :toc-present t)"##
    ]];
    ParityBatchCase::value(
        "legacy_github_anchors_cover_duplicates_punctuation_underscores_and_unicode",
        elisp_form,
        expected,
    )
}

fn pandoc_preset_builds_stable_unicode_formatted_and_duplicate_slugs() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-markdown-toc-test-generate
 "# Release Notes 🚀\n
## **API** [Migration](https://example.invalid/guide)\n
## Café déjà vu\n
## Café déjà vu\n
### `GET /v2/orders` and escaped \\*literal\\*\n
## <span>HTML</span> -- smart... punctuation\n
## 2026 roadmap\n"
 "Release Notes 🚀\n"
 'pandoc)
"###;
    let expected = expect![[
        r##"OK (:text "# Release Notes 🚀\n<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->\n**Table of Contents**\n\n- [Release Notes 🚀](#release-notes)\n  - [**API** [Migration](https://example.invalid/guide)](#api-migration)\n  - [Café déjà vu](#café-déjà-vu)\n  - [Café déjà vu](#café-déjà-vu-1)\n    - [`GET /v2/orders` and escaped \\*literal\\*](#get-v2orders-and-escaped-literal)\n  - [<span>HTML</span> -- smart... punctuation](#html-smart-punctuation)\n  - [2026 roadmap](#roadmap)\n\n<!-- markdown-toc end -->\n\n## **API** [Migration](https://example.invalid/guide)\n\n## Café déjà vu\n\n## Café déjà vu\n\n### `GET /v2/orders` and escaped \\*literal\\*\n\n## <span>HTML</span> -- smart... punctuation\n\n## 2026 roadmap\n" :point 19 :line 2 :column 0 :modified t :toc-present t)"##
    ]];
    ParityBatchCase::value(
        "pandoc_preset_builds_stable_unicode_formatted_and_duplicate_slugs",
        elisp_form,
        expected,
    )
}

fn generated_nested_links_navigate_to_headings_and_reject_misaligned_rows() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (markdown-mode)
  (insert "# Service\n\n## Deploy\n\n### Canary rollout\n\n### Rollback checklist\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "Service\n")
  (markdown-toc-generate-toc)
  (goto-char (point-min))
  (search-forward "[Canary rollout]")
  (markdown-toc-follow-link-at-point)
  (let ((followed (list :point (point)
                        :line (buffer-substring-no-properties
                               (line-beginning-position) (line-end-position))))
        rejected-point rejected-message)
    (goto-char (point-min))
    (search-forward "    - [Rollback checklist]")
    (beginning-of-line)
    (delete-char 1)
    (forward-char 4)
    (setq rejected-point (point))
    (setq rejected-message (markdown-toc-follow-link-at-point))
    (list :followed followed
          :rejected (list :before rejected-point :after (point)
                          :line (buffer-substring-no-properties
                                 (line-beginning-position) (line-end-position))
                          :message rejected-message))))
"###;
    let expected = expect![[
        r####"OK (:followed (:point 311 :line "### Canary rollout") :rejected (:before 210 :after 210 :line "   - [Rollback checklist](#rollback-checklist)" :message "markdown-toc: Not on a link (or misindented), nothing to do"))"####
    ]];
    ParityBatchCase::value(
        "generated_nested_links_navigate_to_headings_and_reject_misaligned_rows",
        elisp_form,
        expected,
    )
}

fn delete_and_noop_refresh_preserve_surrounding_document_content_and_point() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (markdown-mode)
  (insert "Preface for operators.\n\n# Service\n\n## Deploy\n\nAppendix notes.\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "Preface for operators.\n")
  (let ((original (buffer-substring-no-properties (point-min) (point-max)))
        (refresh-point (point)))
    (set-buffer-modified-p nil)
    (markdown-toc-refresh-toc)
    (let ((noop (list :text (buffer-substring-no-properties
                             (point-min) (point-max))
                      :point (point)
                      :modified (buffer-modified-p))))
      (markdown-toc-generate-or-refresh-toc)
      (goto-char (point-max))
      (search-backward "Appendix notes.")
      (let ((delete-point (point))
            (generated (buffer-substring-no-properties
                        (point-min) (point-max))))
        (markdown-toc-delete-toc)
        (list :original original :refresh-point refresh-point :noop noop
              :generated generated :delete-point delete-point
              :after-delete (neomacs-markdown-toc-test-state))))))
"###;
    let expected = expect![[
        r#"OK (:original "Preface for operators.\n\n# Service\n\n## Deploy\n\nAppendix notes.\n" :refresh-point 24 :noop (:text "Preface for operators.\n\n# Service\n\n## Deploy\n\nAppendix notes.\n" :point 24 :modified nil) :generated "Preface for operators.\n<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->\n**Table of Contents**\n\n- [Service](#service)\n  - [Deploy](#deploy)\n\n<!-- markdown-toc end -->\n\n# Service\n\n## Deploy\n\nAppendix notes.\n" :delete-point 229 :after-delete (:text "Preface for operators.\n\n# Service\n\n## Deploy\n\nAppendix notes.\n" :point 47 :line 7 :column 0 :modified t :toc-present nil))"#
    ]];
    ParityBatchCase::value(
        "delete_and_noop_refresh_preserve_surrounding_document_content_and_point",
        elisp_form,
        expected,
    )
}

fn pandoc_cli_selection_reports_missing_tool_without_mutating_the_readme() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (markdown-mode)
  (insert "# Deployment\n\n## Canary\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "Deployment\n")
  (let ((markdown-toc-preset 'pandoc-cli)
        (before (buffer-substring-no-properties (point-min) (point-max)))
        result)
    (cl-letf (((symbol-function 'executable-find) (lambda (_) nil)))
      (setq result
            (condition-case error
                (progn (markdown-toc-generate-toc) :unexpected-success)
              (user-error (list :type (car error) :message (cadr error))))))
    (list :result result
          :unchanged (equal before
                            (buffer-substring-no-properties
                             (point-min) (point-max)))
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point))))
"###;
    let expected = expect![[
        r##"OK (:result (:type user-error :message "Pandoc executable not found.") :unchanged t :text "# Deployment\n\n## Canary\n" :point 14)"##
    ]];
    ParityBatchCase::value(
        "pandoc_cli_selection_reports_missing_tool_without_mutating_the_readme",
        elisp_form,
        expected,
    )
}

#[test]
fn markdown_toc_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(MARKDOWN_TOC_MELPA_PIN, "markdown-toc.el")
            .expect("prepare revision-pinned Markdown-Toc below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "markdown-toc-package-batch",
        "Markdown-Toc",
        &[
            package_and_minor_mode_contract_expose_the_documentation_workflow(),
            production_readme_generates_nested_toc_at_the_authors_insertion_point(),
            refreshing_after_heading_edits_is_idempotent_and_preserves_the_working_point(),
            custom_publishing_policy_changes_headers_numbering_indentation_and_scope(),
            legacy_github_anchors_cover_duplicates_punctuation_underscores_and_unicode(),
            pandoc_preset_builds_stable_unicode_formatted_and_duplicate_slugs(),
            generated_nested_links_navigate_to_headings_and_reject_misaligned_rows(),
            delete_and_noop_refresh_preserve_surrounding_document_content_and_point(),
            pandoc_cli_selection_reports_missing_tool_without_mutating_the_readme(),
        ],
    );
}
