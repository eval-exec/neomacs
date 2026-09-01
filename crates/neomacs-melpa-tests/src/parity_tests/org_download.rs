use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_DOWNLOAD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'org-download)

(defun neomacs-org-download-test-directory (name)
  "Create and return a clean deterministic sandbox directory named NAME."
  (let ((directory
         (file-name-as-directory
          (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p directory)
      (delete-directory directory t))
    (make-directory directory t)
    directory))

(defun neomacs-org-download-test-write (filename contents)
  "Write literal CONTENTS to FILENAME and return FILENAME."
  (make-directory (file-name-directory filename) t)
  (let ((coding-system-for-write 'no-conversion))
    (with-temp-file filename
      (set-buffer-multibyte nil)
      (insert contents)))
  filename)

(defun neomacs-org-download-test-read (filename)
  "Read FILENAME literally as a unibyte string."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally filename)
    (buffer-string)))
"###;

fn package_contract_exposes_commands_storage_policies_and_dnd_handlers() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'org-download package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features (mapcar #'featurep '(org-download async org org-attach)))
   :commands
   (mapcar #'commandp
           '(org-download-yank org-download-screenshot
             org-download-clipboard org-download-image
             org-download-rename-at-point org-download-rename-last-file
             org-download-delete org-download-edit))
   :functions (mapcar #'fboundp '(org-download-enable org-download-disable))
   :defaults
   (list org-download-method org-download-image-dir
         org-download-heading-lvl org-download-backend
         org-download-timestamp org-download-screenshot-basename
         org-download-image-html-width org-download-image-latex-width
         org-download-image-org-width org-download-image-attr-list
         org-download-delete-image-after-download
         org-download-display-inline-images org-download-link-format
         org-download-abbreviate-filename-function)
   :buffer-local
   (with-temp-buffer
     (setq org-download-image-dir "assets-a"
           org-download-heading-lvl 2)
     (list (local-variable-p 'org-download-image-dir)
           (local-variable-p 'org-download-heading-lvl)
           org-download-image-dir org-download-heading-lvl))
   :dnd
   (let (handlers)
     (dolist (entry dnd-protocol-alist)
       (when (memq (cdr entry) '(org-download-dnd org-download-dnd-base64))
         (push entry handlers)))
     (nreverse handlers))))
"###;
    let expected = expect![[
        r#"OK (:package (:name org-download :version "20241118.1846" :requirements ((emacs (24 3)) (async (1 2))) :features (t t t t)) :commands (t t t t t t t t) :functions (t t) :defaults (directory nil 0 t "%Y-%m-%d_%H-%M-%S_" "screenshot.png" 0 0 0 nil nil t "[[file:%s]]\n" file-relative-name) :buffer-local (t t "assets-a" 2) :dnd (("^\\(https?\\|ftp\\|file\\|nfs\\):" . org-download-dnd) ("^data:" . org-download-dnd-base64)))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_commands_storage_policies_and_dnd_handlers",
        elisp_form,
        expected,
    )
}

fn nested_org_headings_drive_deterministic_directories_and_url_filenames() -> ParityBatchCase {
    let elisp_form = r###"
(let ((sandbox (neomacs-org-download-test-directory "org-download-headings")))
  (unwind-protect
      (with-temp-buffer
        (setq default-directory sandbox)
        (insert "* Release Plan\n"
                "** Payments API\n"
                "*** Canary Rollout\n"
                "Evidence goes here.\n")
        (org-mode)
        (goto-char (point-max))
        (let ((org-download-image-dir "assets")
              (org-download-timestamp ""))
          (list
           :headings
           (mapcar #'org-download-get-heading '(0 1 2 3))
           :directories
           (mapcar
            (lambda (level)
              (let ((org-download-heading-lvl level))
                (file-relative-name
                 (expand-file-name (org-download--dir)) sandbox)))
            '(0 1 2 nil))
           :filenames
           (let ((org-download-heading-lvl 1))
             (mapcar
              (lambda (request)
                (file-relative-name (apply #'org-download--fullname request)
                                    sandbox))
              '(("https://cdn.example.test/release%20chart.png?token=redacted")
                ("https://cdn.example.test/export?id=481" "jpg")
                ("file:///srv/screenshots/canary.jpg#preview"))))
           :created
           (sort
            (mapcar (lambda (path) (file-relative-name path sandbox))
                    (directory-files-recursively sandbox "." t))
            #'string<))))
    (when (file-exists-p sandbox) (delete-directory sandbox t))))
"###;
    let expected = expect![[
        r#"OK (:headings ("Release_Plan" "Payments_API" "Canary_Rollout" "Canary_Rollout") :directories ("assets/Release_Plan" "assets/Payments_API" "assets/Canary_Rollout" "assets") :filenames ("assets/Payments_API/release chart.png" "assets/Payments_API/export.jpg" "assets/Payments_API/canary.jpg") :created ("assets" "assets/Canary_Rollout" "assets/Payments_API" "assets/Release_Plan"))"#
    ]];
    ParityBatchCase::value(
        "nested_org_headings_drive_deterministic_directories_and_url_filenames",
        elisp_form,
        expected,
    )
}

fn local_image_workflow_copies_bytes_and_inserts_annotated_indented_org_link() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((sandbox (neomacs-org-download-test-directory "org-download-local-image"))
       (source (expand-file-name "incoming/release [blue].png" sandbox))
       (target (expand-file-name "assets/release [blue].png" sandbox)))
  (neomacs-org-download-test-write source "PNG:release-blue\0payload")
  (unwind-protect
      (with-temp-buffer
        (setq default-directory sandbox
              buffer-file-name (expand-file-name "release-runbook.org" sandbox))
        (insert "* Release\n- Evidence:")
        (org-mode)
        (goto-char (point-max))
        (let ((org-download-method 'directory)
              (org-download-image-dir "assets")
              (org-download-heading-lvl nil)
              (org-download-timestamp "")
              (org-download-image-attr-list
               '("#+caption: Canary dashboard" "#+name: release-dashboard"))
              (org-download-image-html-width 960)
              (org-download-image-latex-width 12)
              (org-download-image-org-width 640)
              (org-download-display-inline-images t)
              (org-download-annotate-function
               (lambda (link)
                 (format "#+DOWNLOADED: %s @ deterministic\n"
                         (file-name-nondirectory link))))
              (display-count 0))
          (cl-letf (((symbol-function 'org-download--display-inline-images)
                     (lambda () (setq display-count (1+ display-count)))))
            (org-download-image source))
          (list :buffer (buffer-substring-no-properties (point-min) (point-max))
                :point (point)
                :target (file-relative-name org-download-path-last-file sandbox)
                :target-exists (file-exists-p target)
                :target-bytes (neomacs-org-download-test-read target)
                :source-exists (file-exists-p source)
                :display-count display-count)))
    (when (file-exists-p sandbox) (delete-directory sandbox t))))
"###;
    let expected = expect![[
        r#"OK (:buffer "* Release\n\11   - Evidence:\n\11   #+DOWNLOADED: release [blue].png @ deterministic\n\11   #+caption: Canary dashboard\n\11   #+name: release-dashboard\n\11   #+attr_html: :width 960px\n\11   #+attr_latex: :width 12cm\n\11   #+attr_org: :width 640px\n\11   [[file:assets/release \\[blue\\].png]]\n" :point 272 :target "assets/release [blue].png" :target-exists t :target-bytes "PNG:release-blue\0payload" :source-exists t :display-count 1)"#
    ]];
    ParityBatchCase::value(
        "local_image_workflow_copies_bytes_and_inserts_annotated_indented_org_link",
        elisp_form,
        expected,
    )
}

fn org_attachment_workflow_uses_heading_dir_copies_file_and_emits_attachment_link()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((sandbox (neomacs-org-download-test-directory "org-download-attach"))
       (source (expand-file-name "incoming/incident.png" sandbox))
       (target (expand-file-name "attachments/incident.png" sandbox)))
  (neomacs-org-download-test-write source "PNG:incident-481")
  (unwind-protect
      (with-temp-buffer
        (setq default-directory sandbox
              buffer-file-name (expand-file-name "incident.org" sandbox))
        (insert "* Incident 481\n"
                ":PROPERTIES:\n"
                ":DIR: attachments\n"
                ":END:\n"
                "Evidence:\n")
        (org-mode)
        (goto-char (point-max))
        (let ((org-download-method 'attach)
              (org-download-timestamp "")
              (org-download-display-inline-images nil)
              (org-download-annotate-function
               (lambda (_) "#+DOWNLOADED: attachment fixture\n")))
          (org-download-image source)
          (goto-char (point-min))
          (list :buffer (buffer-substring-no-properties (point-min) (point-max))
                :dir (org-entry-get nil "DIR")
                :tags (org-get-tags nil t)
                :last-file (file-relative-name org-download-path-last-file sandbox)
                :target-exists (file-exists-p target)
                :target-bytes (neomacs-org-download-test-read target)
                :source-exists (file-exists-p source)
                :attachment-files
                (sort (directory-files (file-name-directory target) nil
                                       directory-files-no-dot-files-regexp)
                      #'string<))))
    (when (file-exists-p sandbox) (delete-directory sandbox t))))
"###;
    let expected = expect![[
        r#"OK (:buffer "* Incident 481                                                       :ATTACH:\n:PROPERTIES:\n:DIR: attachments\n:END:\nEvidence:\n\n#+DOWNLOADED: attachment fixture\n[[attachment:incident.png]]\n" :dir "attachments" :tags ("ATTACH") :last-file "attachments/incident.png" :target-exists t :target-bytes "PNG:incident-481" :source-exists t :attachment-files ("incident.png"))"#
    ]];
    ParityBatchCase::value(
        "org_attachment_workflow_uses_heading_dir_copies_file_and_emits_attachment_link",
        elisp_form,
        expected,
    )
}

fn custom_screenshot_workflow_captures_imports_and_removes_transient_source() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((sandbox (neomacs-org-download-test-directory "org-download-screenshot"))
       (default-capture (expand-file-name "capture/default.png" sandbox))
       (actual-capture (expand-file-name "capture/incident-shot.png" sandbox))
       (target (expand-file-name "assets/incident-shot.png" sandbox))
       captured)
  (unwind-protect
      (with-temp-buffer
        (setq default-directory sandbox
              buffer-file-name (expand-file-name "operations.org" sandbox))
        (insert "* Operations\nScreenshot evidence:\n")
        (org-mode)
        (goto-char (point-max))
        (let ((org-download-method 'directory)
              (org-download-image-dir "assets")
              (org-download-heading-lvl nil)
              (org-download-timestamp "")
              (org-download-screenshot-file default-capture)
              (org-download-screenshot-method
               (lambda (filename)
                 (setq captured filename)
                 (neomacs-org-download-test-write filename "PNG:screen-region")))
              (org-download-display-inline-images nil)
              (org-download-annotate-function
               (lambda (_) "#+DOWNLOADED: screenshot fixture\n")))
          (org-download-screenshot "incident-shot.png")
          (list :capture-argument (file-relative-name captured sandbox)
                :capture-removed (not (file-exists-p actual-capture))
                :target (file-relative-name org-download-path-last-file sandbox)
                :target-exists (file-exists-p target)
                :target-bytes (neomacs-org-download-test-read target)
                :buffer (buffer-substring-no-properties (point-min) (point-max)))))
    (when (file-exists-p sandbox) (delete-directory sandbox t))))
"###;
    let expected = expect![[
        r#"OK (:capture-argument "capture/incident-shot.png" :capture-removed t :target "assets/incident-shot.png" :target-exists t :target-bytes "PNG:screen-region" :buffer "* Operations\nScreenshot evidence:\n\n#+DOWNLOADED: screenshot fixture\n[[file:assets/incident-shot.png]]\n")"#
    ]];
    ParityBatchCase::value(
        "custom_screenshot_workflow_captures_imports_and_removes_transient_source",
        elisp_form,
        expected,
    )
}

fn content_detection_resolves_html_aliases_image_headers_and_invalid_pages() -> ParityBatchCase {
    let elisp_form = r###"
(let (responses allocated)
  (setq responses
        (mapcar
         (lambda (contents)
           (let ((buffer (generate-new-buffer " *org-download-response*")))
             (with-current-buffer buffer (insert contents))
             (push buffer allocated)
             buffer))
         '("HTTP/1.1 200 OK\nContent-Type: text/html\n\n<html><img class=\"hero\" src=\"https://cdn.example.test/chart.png?rev=7\"></html>"
           "HTTP/1.1 200 OK\nContent-Type: image/webp\n\nWEBP"
           "HTTP/1.1 200 OK\nContent-Type: text/plain\n\nnot an image")))
  (unwind-protect
      (cl-letf (((symbol-function 'url-retrieve-synchronously)
                 (lambda (&rest _) (pop responses))))
        (let ((html (org-download--parse-link "https://docs.example.test/dashboard"))
              (header (org-download--parse-link "https://api.example.test/render"))
              (invalid
               (condition-case error-data
                   (org-download--parse-link "https://api.example.test/status")
                 (error (list (car error-data) (cadr error-data))))))
          (list :html html :header header :invalid invalid)))
    (dolist (buffer allocated)
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"###;
    let expected = expect![[
        r#"OK (:html ("https://cdn.example.test/chart.png?rev=7" nil) :header ("https://api.example.test/render" "webp") :invalid (search-failed "^%PDF"))"#
    ]];
    ParityBatchCase::value(
        "content_detection_resolves_html_aliases_image_headers_and_invalid_pages",
        elisp_form,
        expected,
    )
}

fn rename_and_bulk_delete_workflows_keep_org_references_and_files_in_sync() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((sandbox (neomacs-org-download-test-directory "org-download-lifecycle"))
       (image-dir (expand-file-name "images" sandbox))
       (original (expand-file-name "incident.png" image-dir))
       (resolved (expand-file-name "incident-resolved.png" image-dir))
       (archived (expand-file-name "incident-archived.png" image-dir)))
  (neomacs-org-download-test-write original "PNG:incident")
  (unwind-protect
      (with-temp-buffer
        (setq default-directory sandbox)
        (insert "* Incident\n"
                "[[file:images/incident.png]]\n"
                "Mirror: [[file:images/incident.png]]\n")
        (org-mode)
        (goto-char (point-min))
        (search-forward "[[file:")
        (goto-char (match-beginning 0))
        (let ((org-download-image-dir "images")
              (org-download-heading-lvl nil)
              (org-download-display-inline-images nil)
              (answers '("incident-resolved" "incident-archived")))
          (cl-letf (((symbol-function 'read-string)
                     (lambda (&rest _) (pop answers))))
            (org-download-rename-at-point)
            (setq org-download-path-last-file resolved)
            (org-download-rename-last-file))
          (let ((renamed
                 (list :buffer (buffer-string)
                       :original (file-exists-p original)
                       :resolved (file-exists-p resolved)
                       :archived (file-exists-p archived)
                       :last (file-relative-name org-download-path-last-file sandbox))))
            (let ((first (expand-file-name "delete-a.png" image-dir))
                  (second (expand-file-name "delete-b.png" image-dir)))
              (neomacs-org-download-test-write first "A")
              (neomacs-org-download-test-write second "B")
              (erase-buffer)
              (insert "#+DOWNLOADED: first\n[[file:images/delete-a.png]]\n"
                      "#+DOWNLOADED: second\n[[file:images/delete-b.png]]\n"
                      "Retained incident note\n")
              (org-download--delete (point-min) (point-max))
              (list :renamed renamed
                    :bulk-delete
                    (list :buffer (buffer-string)
                          :first-exists (file-exists-p first)
                          :second-exists (file-exists-p second)))))))
    (when (file-exists-p sandbox) (delete-directory sandbox t))))
"###;
    let expected = expect![[
        r#"OK (:renamed (:buffer "* Incident\n[[file:images/incident-archived.png]]\nMirror: [[file:images/incident-archived.png]]\n" :original nil :resolved nil :archived t :last "images/incident-archived.png") :bulk-delete (:buffer "\nRetained incident note\n" :first-exists nil :second-exists nil))"#
    ]];
    ParityBatchCase::value(
        "rename_and_bulk_delete_workflows_keep_org_references_and_files_in_sync",
        elisp_form,
        expected,
    )
}

fn drag_and_drop_base64_enable_disable_and_fallback_follow_mode_contracts() -> ParityBatchCase {
    let elisp_form = r###"
(let ((sandbox (neomacs-org-download-test-directory "org-download-dnd")))
  (unwind-protect
      (let* ((dnd-protocol-alist
              '(("^file:///" . dnd-open-local-file)
                ("^https://" . dnd-open-file)))
             enabled disabled fallback org-dispatch)
        (org-download-enable)
        (org-download-enable)
        (setq enabled (copy-tree dnd-protocol-alist))
        (org-download-disable)
        (setq disabled (copy-tree dnd-protocol-alist))
        (with-temp-buffer
          (cl-letf (((symbol-function 'dnd-handle-one-url)
                     (lambda (window action uri)
                       (setq fallback (list window action uri))
                       'private)))
            (org-download-dnd "https://docs.example.test/runbook" 'copy)))
        (with-temp-buffer
          (setq default-directory sandbox)
          (insert "* Evidence\n")
          (org-mode)
          (goto-char (point-max))
          (let ((org-download-image-dir "images")
                (org-download-heading-lvl nil)
                (org-download-timestamp "")
                (org-download-display-inline-images nil)
                (org-download-annotate-function
                 (lambda (_) "#+DOWNLOADED: base64 fixture\n"))
                (uri (concat "data:image/png;base64,"
                             (base64-encode-string "PNG:clipboard-image" t))))
            (org-download-dnd-base64 uri 'copy)
            (cl-letf (((symbol-function 'org-download-image)
                       (lambda (link) (setq org-dispatch link) :stored)))
              (org-download-dnd "file:///srv/incoming/diagram.png" 'move))
            (list :enabled enabled
                  :disabled disabled
                  :fallback fallback
                  :org-dispatch org-dispatch
                  :buffer (buffer-string)
                  :files
                  (sort (directory-files "images" nil
                                         directory-files-no-dot-files-regexp)
                        #'string<)
                  :bytes
                  (neomacs-org-download-test-read
                   (car (directory-files "images" t "\\.png\\'")))))))
    (when (file-exists-p sandbox) (delete-directory sandbox t))))
"###;
    let expected = expect![[
        r#"OK (:enabled (("^\\(https?\\|ftp\\|file\\|nfs\\):" . org-download-dnd) ("^data:" . org-download-dnd-base64) ("^file:///" . dnd-open-local-file) ("^https://" . dnd-open-file)) :disabled (("^\\(https?\\|ftp\\|file\\|nfs\\):" . org-download-dnd) ("^data:" . org-download-dnd-base64) ("^file:///" . dnd-open-local-file) ("^https://" . dnd-open-file)) :fallback (nil copy "https://docs.example.test/runbook") :org-dispatch "file:///srv/incoming/diagram.png" :buffer "* Evidence\n\n#+DOWNLOADED: base64 fixture\n[[file:images/UE5HOmNsaX.png]]\n" :files ("UE5HOmNsaX.png") :bytes "PNG:clipboard-image")"#
    ]];
    ParityBatchCase::value(
        "drag_and_drop_base64_enable_disable_and_fallback_follow_mode_contracts",
        elisp_form,
        expected,
    )
}

#[test]
fn org_download_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(ORG_DOWNLOAD_MELPA_PIN, "org-download.el")
            .expect("prepare revision-pinned Org Download below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "org-download-package-batch",
        "Org Download",
        &[
            package_contract_exposes_commands_storage_policies_and_dnd_handlers(),
            nested_org_headings_drive_deterministic_directories_and_url_filenames(),
            local_image_workflow_copies_bytes_and_inserts_annotated_indented_org_link(),
            org_attachment_workflow_uses_heading_dir_copies_file_and_emits_attachment_link(),
            custom_screenshot_workflow_captures_imports_and_removes_transient_source(),
            content_detection_resolves_html_aliases_image_headers_and_invalid_pages(),
            rename_and_bulk_delete_workflows_keep_org_references_and_files_in_sync(),
            drag_and_drop_base64_enable_disable_and_fallback_follow_mode_contracts(),
        ],
    );
}
