use expect_test::expect;

use super::ParityBatchCase;

fn org_export_preserves_unicode_structure_and_honors_hooks_and_ascii_policy() -> ParityBatchCase {
    ParityBatchCase::value(
        "org_export_preserves_unicode_structure_and_honors_hooks_and_ascii_policy",
        r##"
(let ((org-mime-export-options
       '(:with-toc nil :section-numbers nil :with-author nil))
      (org-mime-plain-text-hook
       (list (lambda () (goto-char (point-max)) (insert "\nPLAIN-HOOK"))))
      (org-mime-html-hook
       (list (lambda ()
               (goto-char (point-min))
               (org-mime-change-element-style
                "table" "border:1px solid #999")
               (goto-char (point-max))
               (insert "<!--HTML-HOOK-->")))))
  (let* ((source
          "* Release λ\nA *bold* update.\n\n| Service | State |\n| api | green |\n")
         (html (org-mime-export-string source))
         (plain-original (org-mime-export-ascii-maybe source))
         (org-mime-export-ascii 'utf-8)
         (plain-exported (org-mime-export-ascii-maybe source)))
    (list :html
          (neomacs-org-mime-test-normalize-html
           (org-mime-apply-html-hook html))
          :plain-original (org-mime-apply-plain-text-hook plain-original)
          :plain-exported (org-mime-apply-plain-text-hook plain-exported))))
"##,
        expect![[
            r#"OK (:html "<div id=\"outline-container-org<ID>\" class=\"outline-2\">\n<h2 id=\"org<ID>\">Release λ</h2>\n<div class=\"outline-text-2\" id=\"text-org<ID>\">\n<p>\nA <b>bold</b> update.\n</p>\n\n<table style=\"border:1px solid #999\" border=\"2\" cellspacing=\"0\" cellpadding=\"6\" rules=\"groups\" frame=\"hsides\">\n\n\n<colgroup>\n<col  class=\"org-left\" />\n\n<col  class=\"org-left\" />\n</colgroup>\n<tbody>\n<tr>\n<td class=\"org-left\">Service</td>\n<td class=\"org-left\">State</td>\n</tr>\n\n<tr>\n<td class=\"org-left\">api</td>\n<td class=\"org-left\">green</td>\n</tr>\n</tbody>\n</table>\n</div>\n</div>\n<!--HTML-HOOK-->" :plain-original "* Release λ\nA *bold* update.\n\n| Service | State |\n| api | green |\n\nPLAIN-HOOK" :plain-exported "Table of Contents\n─────────────────\n\n1. Release λ\n\n\n1 Release λ\n═══════════\n\n  A *bold* update.\n\n  ━━━━━━━━━━━━━━━━\n   Service  State \n   api      green \n  ━━━━━━━━━━━━━━━━\n\nPLAIN-HOOK")"#
        ]],
    )
}

fn multipart_markup_beautifies_nested_quotes_and_wraps_related_images() -> ParityBatchCase {
    ParityBatchCase::value(
        "multipart_markup_beautifies_nested_quotes_and_wraps_related_images",
        r##"
(let ((org-mime-library 'mml)
      (org-mime-beautify-quoted-mail-p t))
  (org-mime-multipart
   "Plain release note\n"
   "<p>Hello</p>\n&gt; first quote\n&gt;&gt; nested quote\nTail\n"
   "<#part type=\"image/png\" filename=\"chart.png\" disposition=inline id=\"<chart>\">\n<#/part>\n"))
"##,
        expect![[
            r#"OK "<#multipart type=alternative>\n<#part type=text/plain>\nPlain release note\n<#multipart type=related><#part type=text/html>\n<p>Hello</p>\n<blockquote class=\"gmail_quote\" style=\"margin:0 0 0 .8ex;border-left:1px #ccc solid;padding-left:1ex\">\n\n<div>first quote\n<blockquote class=\"gmail_quote\" style=\"margin:0 0 0 .8ex;border-left:1px #ccc solid;padding-left:1ex\">\n\n<div>nested quote\n\n</div></blockquote>\n\n</div></blockquote>\nTail\n<#part type=\"image/png\" filename=\"chart.png\" disposition=inline id=\"<chart>\">\n<#/part>\n<#/multipart>\n<#/multipart>\n""#
        ]],
    )
}

fn inline_images_use_cid_markup_and_missing_files_signal_before_mail_creation() -> ParityBatchCase {
    ParityBatchCase::value(
        "inline_images_use_cid_markup_and_missing_files_signal_before_mail_creation",
        r##"
(let* ((root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (image (expand-file-name "chart λ.png" root))
       (mail (expand-file-name "release.org" root)))
  (with-temp-file image
    (set-buffer-multibyte nil)
    (insert (unibyte-string 137 80 78 71 13 10 26 10)))
  (let* ((cid (replace-regexp-in-string "[/\\\\]" "_" image))
         (existing
          (org-mime-replace-images
           (format "<p>Status</p><img src=\"%s\">" image) mail))
        (missing
         (condition-case err
             (org-mime-replace-images
              "<img src=\"missing.png\">" mail)
           (error
            (list :signal (car err)
                  :message (error-message-string err))))))
    (list :html
          (replace-regexp-in-string (regexp-quote cid) "<CID>"
                                    (car existing) t t)
          :parts
          (mapcar
           (lambda (part)
             (replace-regexp-in-string (regexp-quote cid) "<CID>"
                                       part t t))
           (cdr existing))
          :same-id
          (and (string-match-p (regexp-quote (concat "cid:" cid))
                               (car existing))
               (string-match-p (regexp-quote (concat "id=\"<" cid ">\""))
                               (car (cdr existing)))
               t)
          :missing missing)))
"##,
        expect![[
            r#"OK (:html "<p>Status</p><img src=\"cid:<CID>\">" :parts ("<#part type=\"image/png\" filename=\"[ORACLE-SANDBOX]/chart λ.png\" disposition=inline id=\"<<CID>>\">\n<#/part>\n") :same-id t :missing (:signal user-error :message "Path: [ORACLE-SANDBOX]/missing.png does not exist"))"#
        ]],
    )
}

fn message_htmlize_preserves_headers_attachments_and_signature_then_reverts_to_plain()
-> ParityBatchCase {
    ParityBatchCase::value(
        "message_htmlize_preserves_headers_attachments_and_signature_then_reverts_to_plain",
        r##"
(with-temp-buffer
  (insert
   "To: ops@example.test\nSubject: Release λ\n"
   "--text follows this line--\n"
   "* Status\nAPI is *green*.\n"
   "<#part type=\"application/pdf\" filename=\"report.pdf\" disposition=attachment>\n<#/part>\n"
   "-- \nRelease Team\n")
  (message-mode)
  (goto-char (point-min))
  (org-mime-htmlize)
  (let ((htmlized (buffer-substring-no-properties (point-min) (point-max))))
    (org-mime-revert-to-plain-text-mail)
    (list :htmlized
          (neomacs-org-mime-test-normalize-html htmlized)
          :reverted
          (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[
            r##"OK (:htmlized "To: ops@example.test\nSubject: Release λ\n--text follows this line--\n<#multipart type=alternative>\n<#part type=text/plain>\n* Status\nAPI is *green*.\n\n<#part type=text/html>\n<div id=\"table-of-contents\" role=\"doc-toc\">\n<h2>Table of Contents</h2>\n<div id=\"text-table-of-contents\" role=\"doc-toc\">\n<ul>\n<li><a href=\"#org<ID>\">1. Status</a></li>\n</ul>\n</div>\n</div>\n<div id=\"outline-container-org<ID>\" class=\"outline-2\">\n<h2 id=\"org<ID>\"><span class=\"section-number-2\">1.</span> Status</h2>\n<div class=\"outline-text-2\" id=\"text-1\">\n<p>\nAPI is <b>green</b>.\n</p>\n</div>\n</div>\n<#/multipart>\n<#part type=\"application/pdf\" filename=\"report.pdf\" disposition=attachment>\n<#/part>-- \nRelease Team\n" :reverted "To: ops@example.test\nSubject: Release λ\n--text follows this line--\n* Status\nAPI is *green*.\n\n")"##
        ]],
    )
}

fn org_buffer_properties_drive_message_headers_subject_and_multipart_body() -> ParityBatchCase {
    ParityBatchCase::value(
        "org_buffer_properties_drive_message_headers_subject_and_multipart_body",
        r##"
(unwind-protect
    (with-temp-buffer
      (insert
       "#+PROPERTY: MAIL_SUBJECT Deployment report λ\n"
       "#+PROPERTY: MAIL_TO Ops <ops@example.test>\n"
       "#+PROPERTY: MAIL_FROM Bot <bot@example.test>\n"
       "#+PROPERTY: MAIL_CC reviewers@example.test\n"
       "#+PROPERTY: MAIL_BCC audit@example.test\n"
       "#+PROPERTY: MAIL_IN_REPLY_TO <release-42@example.test>\n"
       "* Status\nEverything is green.\n")
      (org-mode)
      (let ((props (org-mime-buffer-properties)))
        (org-mime-org-buffer-htmlize)
        (let ((mail (car (message-buffers))))
          (list :props props
                :mail
                (with-current-buffer mail
                  (neomacs-org-mime-test-normalize-html
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))
  (neomacs-org-mime-test-kill-message-buffers))
"##,
        expect![[r##"OK (:props (:MAIL_SUBJECT "Deployment report λ" :MAIL_TO "Ops <ops@example.test>" :MAIL_FROM "Bot <bot@example.test>" :MAIL_CC "reviewers@example.test" :MAIL_BCC "audit@example.test" :MAIL_IN_REPLY_TO "<release-42@example.test>") :mail "From: Bot <bot@example.test>\nTo: Ops <ops@example.test>\nCc: reviewers@example.test\nSubject: Deployment report λ\nIn-Reply-To: <release-42@example.test>\nBcc: audit@example.test\n--text follows this line--\n<#multipart type=alternative>\n<#part type=text/plain>\n#+PROPERTY: MAIL_SUBJECT Deployment report λ\n#+PROPERTY: MAIL_TO Ops <ops@example.test>\n#+PROPERTY: MAIL_FROM Bot <bot@example.test>\n#+PROPERTY: MAIL_CC reviewers@example.test\n#+PROPERTY: MAIL_BCC audit@example.test\n#+PROPERTY: MAIL_IN_REPLY_TO <release-42@example.test>\n* Status\nEverything is green.\n<#part type=text/html>\n<div id=\"table-of-contents\" role=\"doc-toc\">\n<h2>Table of Contents</h2>\n<div id=\"text-table-of-contents\" role=\"doc-toc\">\n<ul>\n<li><a href=\"#org<ID>\">1. Status</a></li>\n</ul>\n</div>\n</div>\n<div id=\"outline-container-org<ID>\" class=\"outline-2\">\n<h2 id=\"org<ID>\"><span class=\"section-number-2\">1.</span> Status</h2>\n<div class=\"outline-text-2\" id=\"text-1\">\n<p>\nEverything is green.\n</p>\n</div>\n</div>\n<#/multipart>\n")"##]],
    )
    .fresh_process()
}

fn dedicated_org_editor_saves_successive_changes_and_restores_parent_mail() -> ParityBatchCase {
    ParityBatchCase::value(
        "dedicated_org_editor_saves_successive_changes_and_restores_parent_mail",
        r##"
(let ((org-mime-obey-display-buffer-p t)
      (org-mime-instructions-hint "EDIT-HINT\n"))
  (save-window-excursion
    (with-temp-buffer
      (insert
       "To: ops@example.test\nSubject: Draft\n"
       "--text follows this line--\nfoo\n")
      (message-mode)
      (let ((mail (current-buffer)) editor)
        (org-mime-edit-mail-in-org-mode)
        (setq editor (current-buffer))
        (goto-char (point-max))
        (insert "bar\n")
        (org-mime-edit-src-save)
        (let ((first
               (with-current-buffer mail
                 (buffer-substring-no-properties
                  (point-min) (point-max)))))
          (goto-char (point-max))
          (insert "baz\n")
          (org-mime-edit-src-exit)
          (list :editor-mode major-mode
                :editor-live (buffer-live-p editor)
                :first first
                :final
                (with-current-buffer mail
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))
"##,
        expect![[r#"OK (:editor-mode message-mode :editor-live nil :first "To: ops@example.test\nSubject: Draft\n--text follows this line--\nfoo\nbar\n" :final "To: ops@example.test\nSubject: Draft\n--text follows this line--\nfoo\nbar\nbaz\n")"#]],
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        org_export_preserves_unicode_structure_and_honors_hooks_and_ascii_policy(),
        multipart_markup_beautifies_nested_quotes_and_wraps_related_images(),
        inline_images_use_cid_markup_and_missing_files_signal_before_mail_creation(),
        message_htmlize_preserves_headers_attachments_and_signature_then_reverts_to_plain(),
        org_buffer_properties_drive_message_headers_subject_and_multipart_body(),
        dedicated_org_editor_saves_successive_changes_and_restores_parent_mail(),
    ]
}
