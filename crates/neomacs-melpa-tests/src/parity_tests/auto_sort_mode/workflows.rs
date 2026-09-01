use expect_test::expect;

use super::ParityBatchCase;

fn manual_sort_reorders_a_release_manifest_and_preserves_the_editing_location() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "Release 4.2 dependencies\n"
   "<!-- { auto-sort-mode.el start } -->\n"
   "zlib >= 1.3\n"
   "AlphaSDK = 2\n"
   "openssl = 3\n"
   "alpha-tools = 7\n"
   "<!-- { auto-sort-mode.el end } -->\n"
   "Owner: platform Ω\n")
  (goto-char (point-min))
  (search-forward "platform")
  (let ((sort-fold-case nil)
        (before (list (line-number-at-pos) (current-column) (char-after))))
    (auto-sort-between-delimiters)
    (list
     :text (buffer-substring-no-properties (point-min) (point-max))
     :location-before before
     :location-after (list (line-number-at-pos) (current-column) (char-after))
     :modified (buffer-modified-p))))
"####;
    let expect = expect![[
        r####"OK (:text "Release 4.2 dependencies\n<!-- { auto-sort-mode.el start } -->\nAlphaSDK = 2\nalpha-tools = 7\nopenssl = 3\nzlib >= 1.3\n<!-- { auto-sort-mode.el end } -->\nOwner: platform Ω\n" :location-before (8 15 32) :location-after (8 15 32) :modified t)"####
    ]];
    ParityBatchCase::value(
        "manual_sort_reorders_a_release_manifest_and_preserves_the_editing_location",
        elisp_form,
        expect,
    )
}

fn custom_delimiters_sort_only_the_first_managed_configuration_section() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "# generated deployment inventory\n"
   "# BEGIN MANAGED SERVICES\n"
   "worker=jobs\n"
   "API=public\n"
   "proxy=edge\n"
   "# END MANAGED SERVICES\n"
   "\n"
   "# BEGIN MANAGED SERVICES\n"
   "zeta=standby\n"
   "beta=standby\n"
   "# END MANAGED SERVICES\n")
  (let ((auto-sort-mode-start-delimiter "# BEGIN MANAGED SERVICES")
        (auto-sort-mode-end-delimiter "# END MANAGED SERVICES")
        (sort-fold-case t))
    (auto-sort-between-delimiters)
    (list
     :text (buffer-substring-no-properties (point-min) (point-max))
     :first-api-line
     (save-excursion
       (goto-char (point-min))
       (search-forward "API=public")
       (line-number-at-pos))
     :second-zeta-before-beta
     (< (save-excursion
          (goto-char (point-min))
          (search-forward "zeta=standby"))
        (save-excursion
          (goto-char (point-min))
          (search-forward "beta=standby"))))))
"####;
    let expect = expect![[
        r####"OK (:text "# generated deployment inventory\n# BEGIN MANAGED SERVICES\nAPI=public\nproxy=edge\nworker=jobs\n# END MANAGED SERVICES\n\n# BEGIN MANAGED SERVICES\nzeta=standby\nbeta=standby\n# END MANAGED SERVICES\n" :first-api-line 3 :second-zeta-before-beta t)"####
    ]];
    ParityBatchCase::value(
        "custom_delimiters_sort_only_the_first_managed_configuration_section",
        elisp_form,
        expect,
    )
}

fn saving_a_real_file_sorts_before_write_and_exposes_the_expected_hook_timeline() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root (neomacs-auto-sort-test--root "auto-sort-save"))
       (path (expand-file-name "services.conf" root))
       (initial
        (concat
         "team=runtime\n"
         "<!-- { auto-sort-mode.el start } -->\n"
         "worker=3\n"
         "api=1\n"
         "cache=2\n"
         "<!-- { auto-sort-mode.el end } -->\n"
         "owner=platform Ω\n"))
       buffer result)
  (unwind-protect
      (progn
        (neomacs-auto-sort-test--write-file path initial)
        (setq buffer (find-file-noselect path)
              neomacs-auto-sort-test--events nil)
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (add-hook 'before-save-hook 'neomacs-auto-sort-test--before-save nil t)
          (add-hook 'after-save-hook 'neomacs-auto-sort-test--after-save nil t)
          (auto-sort-mode 1)
          (goto-char (point-min))
          (search-forward "platform")
          (replace-match "release-platform" t t)
          (save-buffer)
          (setq result
                (list
                 :buffer (buffer-substring-no-properties (point-min) (point-max))
                 :disk (neomacs-auto-sort-test--file-text path)
                 :events (nreverse neomacs-auto-sort-test--events)
                 :point (list (line-number-at-pos) (current-column) (char-after))
                 :modified (buffer-modified-p)
                 :mode auto-sort-mode
                 :write-hook-count
                 (neomacs-auto-sort-test--hook-count
                  'auto-sort-between-delimiters write-contents-functions)))))
    (neomacs-auto-sort-test--cleanup (list buffer) root))
  result)
"####;
    let expect = expect![[
        r####"OK (:buffer "team=runtime\n<!-- { auto-sort-mode.el start } -->\napi=1\ncache=2\nworker=3\n<!-- { auto-sort-mode.el end } -->\nowner=release-platform Ω\n" :disk "team=runtime\n<!-- { auto-sort-mode.el start } -->\napi=1\ncache=2\nworker=3\n<!-- { auto-sort-mode.el end } -->\nowner=release-platform Ω\n" :events ((:before "team=runtime\n<!-- { auto-sort-mode.el start } -->\nworker=3\napi=1\ncache=2\n<!-- { auto-sort-mode.el end } -->\nowner=release-platform Ω\n" t) (:after "team=runtime\n<!-- { auto-sort-mode.el start } -->\napi=1\ncache=2\nworker=3\n<!-- { auto-sort-mode.el end } -->\nowner=release-platform Ω\n" nil)) :point (7 22 32) :modified nil :mode t :write-hook-count 1)"####
    ]];
    ParityBatchCase::value(
        "saving_a_real_file_sorts_before_write_and_exposes_the_expected_hook_timeline",
        elisp_form,
        expect,
    )
}

fn repeated_activation_is_idempotent_and_disabling_leaves_the_next_save_unsorted() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root (neomacs-auto-sort-test--root "auto-sort-lifecycle"))
       (path (expand-file-name "routes.txt" root))
       (document
        (concat
         "<!-- { auto-sort-mode.el start } -->\n"
         "zeta-route\n"
         "alpha-route\n"
         "<!-- { auto-sort-mode.el end } -->\n"))
       buffer enabled disabled result)
  (unwind-protect
      (progn
        (neomacs-auto-sort-test--write-file path document)
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (auto-sort-mode 1)
          (auto-sort-mode 1)
          (setq enabled
                (list
                 :mode auto-sort-mode
                 :hook-count
                 (neomacs-auto-sort-test--hook-count
                  'auto-sort-between-delimiters write-contents-functions)))
          (auto-sort-mode -1)
          (setq disabled
                (list
                 :mode auto-sort-mode
                 :hook-count
                 (neomacs-auto-sort-test--hook-count
                  'auto-sort-between-delimiters write-contents-functions)))
          (goto-char (point-max))
          (insert "saved-with-mode-disabled Ω\n")
          (save-buffer)
          (setq result
                (list
                 :enabled enabled
                 :disabled disabled
                 :buffer (buffer-substring-no-properties (point-min) (point-max))
                 :disk (neomacs-auto-sort-test--file-text path)
                 :modified (buffer-modified-p)))))
    (neomacs-auto-sort-test--cleanup (list buffer) root))
  result)
"####;
    let expect = expect![[
        r####"OK (:enabled (:mode t :hook-count 1) :disabled (:mode nil :hook-count 0) :buffer "<!-- { auto-sort-mode.el start } -->\nzeta-route\nalpha-route\n<!-- { auto-sort-mode.el end } -->\nsaved-with-mode-disabled Ω\n" :disk "<!-- { auto-sort-mode.el start } -->\nzeta-route\nalpha-route\n<!-- { auto-sort-mode.el end } -->\nsaved-with-mode-disabled Ω\n" :modified nil)"####
    ]];
    ParityBatchCase::value(
        "repeated_activation_is_idempotent_and_disabling_leaves_the_next_save_unsorted",
        elisp_form,
        expect,
    )
}

fn saving_while_narrowed_still_sorts_the_full_document_and_restores_the_restriction()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-auto-sort-test--root "auto-sort-narrowed-save"))
       (path (expand-file-name "deployment.md" root))
       (document
        (concat
         "# Deployment\n"
         "<!-- { auto-sort-mode.el start } -->\n"
         "worker\n"
         "api\n"
         "cache\n"
         "<!-- { auto-sort-mode.el end } -->\n"
         "Notes: pending\n"))
       buffer result)
  (unwind-protect
      (progn
        (neomacs-auto-sort-test--write-file path document)
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (auto-sort-mode 1)
          (goto-char (point-min))
          (search-forward "Notes:")
          (let ((beg (line-beginning-position))
                (end (point-max)))
            (narrow-to-region beg end)
            (goto-char (point-max))
            (insert "owner: release Ω\n")
            (save-buffer)
            (setq result
                  (list
                   :narrowed (buffer-narrowed-p)
                   :visible (buffer-substring-no-properties (point-min) (point-max))
                   :restriction (list (point-min) (point-max))
                   :full-buffer
                   (save-restriction
                     (widen)
                     (buffer-substring-no-properties (point-min) (point-max)))
                   :disk (neomacs-auto-sort-test--file-text path)
                   :modified (buffer-modified-p))))))
    (neomacs-auto-sort-test--cleanup (list buffer) root))
  result)
"####;
    let expect = expect![[
        r####"OK (:narrowed t :visible "Notes: pending\nowner: release Ω\n" :restriction (103 135) :full-buffer "# Deployment\n<!-- { auto-sort-mode.el start } -->\napi\ncache\nworker\n<!-- { auto-sort-mode.el end } -->\nNotes: pending\nowner: release Ω\n" :disk "# Deployment\n<!-- { auto-sort-mode.el start } -->\napi\ncache\nworker\n<!-- { auto-sort-mode.el end } -->\nNotes: pending\nowner: release Ω\n" :modified nil)"####
    ]];
    ParityBatchCase::value(
        "saving_while_narrowed_still_sorts_the_full_document_and_restores_the_restriction",
        elisp_form,
        expect,
    )
}

fn incomplete_managed_sections_are_safe_no_ops_during_normal_editing() -> ParityBatchCase {
    let elisp_form = r####"
(let (without-start without-end)
  (with-temp-buffer
    (insert "Runbook draft\nzeta step\nalpha step\nOwner: ops Ω\n")
    (goto-char (point-min))
    (search-forward "zeta")
    (set-buffer-modified-p nil)
    (setq without-start
          (list
           :return (auto-sort-between-delimiters)
           :text (buffer-substring-no-properties (point-min) (point-max))
           :point (list (line-number-at-pos) (current-column) (char-after))
           :modified (buffer-modified-p))))
  (with-temp-buffer
    (insert
     "Runbook draft\n"
     "<!-- { auto-sort-mode.el start } -->\n"
     "zeta step\n"
     "alpha step\n"
     "Owner: ops Ω\n")
    (goto-char (point-min))
    (search-forward "Owner")
    (set-buffer-modified-p nil)
    (setq without-end
          (list
           :return (auto-sort-between-delimiters)
           :text (buffer-substring-no-properties (point-min) (point-max))
           :point (list (line-number-at-pos) (current-column) (char-after))
           :modified (buffer-modified-p))))
  (list :without-start without-start :without-end without-end))
"####;
    let expect = expect![[
        r####"OK (:without-start (:return nil :text "Runbook draft\nzeta step\nalpha step\nOwner: ops Ω\n" :point (2 4 32) :modified nil) :without-end (:return nil :text "Runbook draft\n<!-- { auto-sort-mode.el start } -->\nzeta step\nalpha step\nOwner: ops Ω\n" :point (5 5 58) :modified nil))"####
    ]];
    ParityBatchCase::value(
        "incomplete_managed_sections_are_safe_no_ops_during_normal_editing",
        elisp_form,
        expect,
    )
}

fn embedded_comment_delimiters_and_case_folding_sort_a_practical_markdown_index() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "# Service index\n"
   "generated: <!-- { auto-sort-mode.el start } --> do not edit\n"
   "Zoo API\n"
   "billing worker\n"
   "Auth gateway\n"
   "footer: <!-- { auto-sort-mode.el end } --> generated\n"
   "Maintainer: docs Ω\n")
  (let ((sort-fold-case t))
    (auto-sort-between-delimiters)
    (list
     :text (buffer-substring-no-properties (point-min) (point-max))
     :order
     (mapcar
      (lambda (name)
        (save-excursion
          (goto-char (point-min))
          (search-forward name)
          (line-number-at-pos)))
      '("Auth gateway" "billing worker" "Zoo API")))))
"####;
    let expect = expect![[
        r####"OK (:text "# Service index\ngenerated: <!-- { auto-sort-mode.el start } --> do not edit\nAuth gateway\nbilling worker\nZoo API\nfooter: <!-- { auto-sort-mode.el end } --> generated\nMaintainer: docs Ω\n" :order (3 4 5))"####
    ]];
    ParityBatchCase::value(
        "embedded_comment_delimiters_and_case_folding_sort_a_practical_markdown_index",
        elisp_form,
        expect,
    )
}

fn sorting_moves_each_propertized_record_as_one_unit() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "<!-- { auto-sort-mode.el start } -->\n")
  (let ((beg (point)))
    (insert "zebra service\n")
    (add-text-properties beg (point) '(neomacs-service-id zeta face warning)))
  (let ((beg (point)))
    (insert "alpha service\n")
    (add-text-properties beg (point) '(neomacs-service-id alpha face success)))
  (let ((beg (point)))
    (insert "middle service\n")
    (add-text-properties beg (point) '(neomacs-service-id middle face shadow)))
  (insert "<!-- { auto-sort-mode.el end } -->\n")
  (auto-sort-between-delimiters)
  (let (records)
    (save-excursion
      (goto-char (point-min))
      (forward-line 1)
      (dotimes (_ 3)
        (setq records
              (cons
               (list
                (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position))
                (get-text-property (point) 'neomacs-service-id)
                (get-text-property (point) 'face))
               records))
        (forward-line 1)))
    (list
     :text (buffer-substring-no-properties (point-min) (point-max))
     :records (nreverse records)
     :start-properties (text-properties-at (point-min))
     :end-properties
     (save-excursion
       (goto-char (point-max))
       (forward-line -1)
       (text-properties-at (point))))))
"####;
    let expect = expect![[
        r####"OK (:text "<!-- { auto-sort-mode.el start } -->\nalpha service\nmiddle service\nzebra service\n<!-- { auto-sort-mode.el end } -->\n" :records (("alpha service" alpha success) ("middle service" middle shadow) ("zebra service" zeta warning)) :start-properties nil :end-properties nil)"####
    ]];
    ParityBatchCase::value(
        "sorting_moves_each_propertized_record_as_one_unit",
        elisp_form,
        expect,
    )
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        manual_sort_reorders_a_release_manifest_and_preserves_the_editing_location(),
        custom_delimiters_sort_only_the_first_managed_configuration_section(),
        saving_a_real_file_sorts_before_write_and_exposes_the_expected_hook_timeline(),
        repeated_activation_is_idempotent_and_disabling_leaves_the_next_save_unsorted(),
        saving_while_narrowed_still_sorts_the_full_document_and_restores_the_restriction(),
        incomplete_managed_sections_are_safe_no_ops_during_normal_editing(),
        embedded_comment_delimiters_and_case_folding_sort_a_practical_markdown_index(),
        sorting_moves_each_propertized_record_as_one_unit(),
    ]
}
