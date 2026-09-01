use expect_test::expect;

use super::ParityBatchCase;

fn completing_a_named_snippet_replaces_context_and_preserves_the_editing_session() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root (neomacs-helm-c-yas-test-root "helm-c-yas-complete"))
       (mode-dir (expand-file-name "emacs-lisp-mode" root)))
  (neomacs-helm-c-yas-test-write-file
   (expand-file-name "deploy" mode-dir)
   (concat
    "# -*- mode: snippet -*-\n"
    "# name: Deploy service\n"
    "# key: dep\n"
    "# expand-env: ((yas-indent-line 'fixed))\n"
    "# --\n"
    "(deploy \"${1:api}\" :region \"${2:us-east-1}\")\n"
    ";; owner=${1:$(upcase yas-text)}$0\n"))
  (neomacs-helm-c-yas-test-write-file
   (expand-file-name "rollback" mode-dir)
   (concat
    "# -*- mode: snippet -*-\n"
    "# name: Deploy rollback\n"
    "# key: rb\n"
    "# --\n"
    "(rollback \"${1:release}\")$0\n"))
  (neomacs-helm-c-yas-test-reset-yas root)
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (emacs-lisp-mode)
      (yas-minor-mode 1)
      (insert "Plan: Dep")
      (let ((helm-yas-display-key-on-candidate t)
            (helm-yas-display-msg-after-complete t)
            (neomacs-helm-c-yas-test-selection "[dep] Deploy service")
            (neomacs-helm-c-yas-test-action "Insert snippet")
            (neomacs-helm-c-yas-test-pattern "service"))
        (cl-letf (((symbol-function 'helm)
                   #'neomacs-helm-c-yas-test-helm))
          (helm-yas-complete))
        (let ((after-insert
               (list
                :buffer (buffer-substring-no-properties
                         (point-min) (point-max))
                :point (point)
                :active (length (yas-active-snippets 'all))
                :message (current-message))))
          (execute-kbd-macro "billing-api")
          (yas-next-field-or-maybe-expand)
          (execute-kbd-macro "eu-west-2")
          (yas-next-field-or-maybe-expand)
          (yas-exit-all-snippets)
          (list
           :session
           (neomacs-helm-c-yas-test-session-summary
            neomacs-helm-c-yas-test-last-session)
           :after-insert after-insert
           :final
           (list
            :buffer (buffer-substring-no-properties
                     (point-min) (point-max))
            :point (point)
            :active (length (yas-active-snippets 'all)))))))))
"##;
    let expect = expect![[
        r#"OK (:session (:source "Yasnippet" :initial-input "Dep" :replacement-span (7 10) :selected-text "" :pattern "service" :candidates ("[dep] Deploy service" "[rb] Deploy rollback") :matches ("[dep] Deploy service") :selected "[dep] Deploy service" :actions ("Insert snippet" "Open snippet file" "Open snippet file other window" "Create new snippet on region" "Reload All Snippts" "Rename snippet file" "Delete snippet file") :action "Insert snippet" :action-invoked t :action-result-truthy t) :after-insert (:buffer "Plan: (deploy \"api\" :region \"us-east-1\")\n      ;; owner=API\n" :point 16 :active 1 :message nil) :final (:buffer "Plan: (deploy \"billing-api\" :region \"eu-west-2\")\n      ;; owner=BILLING-API\n" :point 76 :active 0))"#
    ]];
    ParityBatchCase::value(
        "completing_a_named_snippet_replaces_context_and_preserves_the_editing_session",
        elisp_form,
        expect,
    )
}

fn catalog_options_filter_conditions_duplicates_keys_and_multiword_patterns() -> ParityBatchCase {
    let elisp_form = r##"
(let ((root (neomacs-helm-c-yas-test-root "helm-c-yas-catalog")))
  (make-directory root t)
  (neomacs-helm-c-yas-test-reset-yas root)
  (yas-define-snippets
   'emacs-lisp-mode
   (list
    (list "audit" "(audit-release)" "Release audit"
          '(eq major-mode 'emacs-lisp-mode)
          nil nil nil nil "release-audit")
    (list "prod" "(deploy-production)" "Release deploy production"
          nil nil nil nil nil "release-production")
    (list "stage" "(deploy-staging)" "Release deploy staging"
          nil nil nil nil nil "release-staging")
    (list "hidden" "(deploy-secret)" "Release deploy hidden"
          '(eq major-mode 'python-mode)
          nil nil nil nil "release-hidden")
    (list "dup-a" "(same-release)" "Release duplicate"
          nil nil nil nil nil "release-duplicate-a")
    (list "dup-b" "(same-release)" "Release duplicate"
          nil nil nil nil nil "release-duplicate-b")))
  (with-temp-buffer
    (emacs-lisp-mode)
    (yas-minor-mode 1)
    (insert "Release")
    (cl-letf (((symbol-function 'helm)
               #'neomacs-helm-c-yas-test-helm))
      (let ((helm-yas-not-display-dups t)
            (helm-yas-display-key-on-candidate nil)
            (helm-yas-space-match-any-greedy t)
            (neomacs-helm-c-yas-test-selection nil)
            (neomacs-helm-c-yas-test-action nil)
            (neomacs-helm-c-yas-test-pattern "deploy prod"))
        (helm-yas-complete)
        (let ((deduplicated
               (copy-tree neomacs-helm-c-yas-test-last-session)))
          (let ((helm-yas-not-display-dups nil))
            (helm-yas-complete)
            (let ((duplicates
                   (copy-tree neomacs-helm-c-yas-test-last-session)))
              (let ((helm-yas-display-key-on-candidate t))
                (helm-yas-complete)
                (let ((keyed
                       (copy-tree neomacs-helm-c-yas-test-last-session)))
                  (let ((helm-yas-not-display-dups t)
                        (helm-yas-display-key-on-candidate nil)
                        (helm-yas-space-match-any-greedy nil))
                    (helm-yas-complete)
                    (list
                     :deduplicated
                     (list (plist-get deduplicated :candidates)
                           (plist-get deduplicated :matches))
                     :duplicates
                     (list (plist-get duplicates :candidates)
                           (plist-get duplicates :matches))
                     :keyed
                     (list (plist-get keyed :candidates)
                           (plist-get keyed :matches))
                     :plain-pattern-matches
                     (plist-get neomacs-helm-c-yas-test-last-session
                                :matches))))))))))))
"##;
    let expect = expect![[
        r#"OK (:deduplicated (("Release audit" "Release deploy production" "Release deploy staging" "Release duplicate") ("Release deploy production")) :duplicates (("Release audit" "Release deploy production" "Release deploy staging" "Release duplicate" "Release duplicate") ("Release deploy production")) :keyed (("[audit] Release audit" "[dup-a] Release duplicate" "[dup-a] Release duplicate" "[prod] Release deploy production" "[stage] Release deploy staging") ("[prod] Release deploy production")) :plain-pattern-matches ("Release deploy production"))"#
    ]];
    ParityBatchCase::value(
        "catalog_options_filter_conditions_duplicates_keys_and_multiword_patterns",
        elisp_form,
        expect,
    )
}

fn authoring_from_a_real_region_round_trips_through_a_saved_snippet() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-helm-c-yas-test-root "helm c yas author"))
       (mode-dir (expand-file-name "emacs-lisp-mode" root))
       (other-dir (expand-file-name "fish-mode" root))
       (snippet-file (expand-file-name "releasecheck" mode-dir))
       (source-buffer (generate-new-buffer " *helm-c-yas-author-source*"))
       authored)
  (make-directory mode-dir t)
  (make-directory other-dir t)
  (neomacs-helm-c-yas-test-write-file
   (expand-file-name "fish" other-dir)
   "# name: Fish helper\n# key: fish\n# --\necho fish\n")
  (setq yas-snippet-dirs (list root))
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer source-buffer)
        (emacs-lisp-mode)
        (insert
         "(when ${1:approved}\n"
         "  (deploy \"${2:api-λ}\"))$0\n")
        (helm-yas-create-snippet-on-region
         (point-min) (point-max) "releasecheck")
        ;; `helm-yas-create-new-snippet-file' deliberately restores the
        ;; caller's current buffer after `find-file' displays the new snippet.
        ;; Observe the real visiting buffer rather than assuming it remains
        ;; current after the public command returns.
        (with-current-buffer (get-file-buffer snippet-file)
          (setq authored
                (list
                 :file (file-relative-name buffer-file-name root)
                 :mode major-mode
                 :modified-before-save (buffer-modified-p)
                 :contents-before-save
                 (buffer-substring-no-properties (point-min) (point-max))))
          (save-buffer)
          (setq authored
                (append
                 authored
                 (list
                  :modified-after-save (buffer-modified-p)
                  :disk
                  (neomacs-helm-c-yas-test-file-contents snippet-file)))))
        (neomacs-helm-c-yas-test-kill-file-buffer snippet-file))
    (when (buffer-live-p source-buffer)
      (kill-buffer source-buffer)))
  (neomacs-helm-c-yas-test-reset-yas root)
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (emacs-lisp-mode)
      (yas-minor-mode 1)
      (insert "releasecheck")
      (let ((expanded (yas-expand)))
        (execute-kbd-macro "review-approved-p")
        (yas-next-field-or-maybe-expand)
        (execute-kbd-macro "payments-λ")
        (yas-next-field-or-maybe-expand)
        (yas-exit-all-snippets)
        (list
         :authored authored
         :expanded expanded
         :final-buffer
         (buffer-substring-no-properties (point-min) (point-max))
         :point (point)
         :active (length (yas-active-snippets 'all)))))))
"##;
    let expect = expect![[
        r##"OK (:authored (:file "emacs-lisp-mode/releasecheck" :mode snippet-mode :modified-before-save t :contents-before-save "# -*- mode: snippet -*-\n#name : releasecheck\n#key : releasecheck\n#contributor : Parity Author\n# --\n(when ${1:approved}\n  (deploy \"${2:api-λ}\"))$0\n" :modified-after-save nil :disk "# -*- mode: snippet -*-\n#name : releasecheck\n#key : releasecheck\n#contributor : Parity Author\n# --\n(when ${1:approved}\n  (deploy \"${2:api-\316\273}\"))$0\n") :expanded t :final-buffer "(when review-approved-p\n  (deploy \"payments-λ\"))\n" :point 49 :active 0)"##
    ]];
    ParityBatchCase::value(
        "authoring_from_a_real_region_round_trips_through_a_saved_snippet",
        elisp_form,
        expect,
    )
}

fn rename_and_delete_actions_mutate_real_snippet_files_and_reload_the_catalog() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-helm-c-yas-test-root "helm-c-yas-mutate"))
       (mode-dir (expand-file-name "emacs-lisp-mode" root))
       (deploy (expand-file-name "deploy" mode-dir))
       (renamed (expand-file-name "deploy-production" mode-dir))
       (rollback (expand-file-name "rollback" mode-dir))
       rename-prompt delete-prompt rename-session delete-session)
  (neomacs-helm-c-yas-test-write-file
   deploy
   "# name: Deploy production\n# key: dep\n# --\n(deploy-production)$0\n")
  (neomacs-helm-c-yas-test-write-file
   rollback
   "# name: Rollback release\n# key: rb\n# --\n(rollback-release)$0\n")
  (neomacs-helm-c-yas-test-reset-yas root)
  (with-temp-buffer
    (emacs-lisp-mode)
    (yas-minor-mode 1)
    (let ((helm-yas-display-key-on-candidate nil)
          (neomacs-helm-c-yas-test-pattern ""))
      (cl-letf (((symbol-function 'helm)
                 #'neomacs-helm-c-yas-test-helm)
                ((symbol-function 'read-string)
                 (lambda (prompt &rest _arguments)
                   (setq rename-prompt prompt)
                   "deploy-production")))
        (let ((neomacs-helm-c-yas-test-selection "Deploy production")
              (neomacs-helm-c-yas-test-action "Rename snippet file"))
          (helm-yas-complete)
          (setq rename-session
                (copy-tree neomacs-helm-c-yas-test-last-session))))
      (cl-letf (((symbol-function 'helm)
                 #'neomacs-helm-c-yas-test-helm)
                ((symbol-function 'y-or-n-p)
                 (lambda (prompt)
                   (setq delete-prompt prompt)
                   t)))
        (let ((neomacs-helm-c-yas-test-selection "Rollback release")
              (neomacs-helm-c-yas-test-action "Delete snippet file"))
          (helm-yas-complete)
          (setq delete-session
                (copy-tree neomacs-helm-c-yas-test-last-session))))
      (let* ((catalog (helm-yas-build-cur-snippets-alist))
             (names
              (mapcar
               (lambda (candidate)
                 (substring-no-properties (car candidate)))
               (helm-yas-get-transformed-list catalog ""))))
        (list
         :rename
         (list
          :prompt rename-prompt
          :session
          (neomacs-helm-c-yas-test-session-summary rename-session)
          :old-exists (file-exists-p deploy)
          :new-exists (file-exists-p renamed)
          :new-bytes
          (and (file-exists-p renamed)
               (neomacs-helm-c-yas-test-file-contents renamed)))
         :delete
         (list
          :prompt delete-prompt
          :session
          (neomacs-helm-c-yas-test-session-summary delete-session)
          :exists (file-exists-p rollback))
         :directory (directory-files mode-dir nil "^[^.]" nil)
         :catalog names)))))
"##;
    let expect = expect![[
        r##"OK (:rename (:prompt "rename [deploy] to: " :session (:source "Yasnippet" :initial-input "" :replacement-span (1 1) :selected-text "" :pattern "" :candidates ("Deploy production" "Rollback release") :matches ("Deploy production" "Rollback release") :selected "Deploy production" :actions ("Insert snippet" "Open snippet file" "Open snippet file other window" "Create new snippet on region" "Reload All Snippts" "Rename snippet file" "Delete snippet file") :action "Rename snippet file" :action-invoked t :action-result-truthy nil) :old-exists nil :new-exists t :new-bytes "# name: Deploy production\n# key: dep\n# --\n(deploy-production)$0\n") :delete (:prompt "really delete?" :session (:source "Yasnippet" :initial-input "" :replacement-span (1 1) :selected-text "" :pattern "" :candidates ("Deploy production" "Rollback release") :matches ("Deploy production" "Rollback release") :selected "Rollback release" :actions ("Insert snippet" "Open snippet file" "Open snippet file other window" "Create new snippet on region" "Reload All Snippts" "Rename snippet file" "Delete snippet file") :action "Delete snippet file" :action-invoked t :action-result-truthy nil) :exists nil) :directory ("deploy-production") :catalog ("Deploy production"))"##
    ]];
    ParityBatchCase::value(
        "rename_and_delete_actions_mutate_real_snippet_files_and_reload_the_catalog",
        elisp_form,
        expect,
    )
}

fn visiting_a_loaded_snippet_uses_its_real_file_in_current_and_other_windows() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-helm-c-yas-test-root "helm-c-yas-visit"))
       (mode-dir (expand-file-name "emacs-lisp-mode" root))
       (snippet-file (expand-file-name "open-plan" mode-dir))
       (origin (generate-new-buffer "helm-c-yas-origin"))
       visit-session current-window other-window missing)
  (neomacs-helm-c-yas-test-write-file
   snippet-file
   (concat
    "# -*- mode: snippet -*-\n"
    "# name: Open deployment plan\n"
    "# key: plan\n"
    "# --\n"
    "(find-file \"DEPLOYMENT.md\")$0\n"))
  (neomacs-helm-c-yas-test-reset-yas root)
  (unwind-protect
      (save-window-excursion
        (delete-other-windows)
        (switch-to-buffer origin)
        (emacs-lisp-mode)
        (yas-minor-mode 1)
        (let ((neomacs-helm-c-yas-test-selection nil)
              (neomacs-helm-c-yas-test-action nil)
              (neomacs-helm-c-yas-test-pattern ""))
          (cl-letf (((symbol-function 'helm)
                     #'neomacs-helm-c-yas-test-helm))
            (helm-yas-visit-snippet-file))
          (setq visit-session
                (copy-tree neomacs-helm-c-yas-test-last-session)))
        (let* ((helm-yas-cur-snippets-alist
                (helm-yas-build-cur-snippets-alist))
               (template
                (cdr
                 (assoc
                  "Open deployment plan"
                  (assoc-default
                   'transformed helm-yas-cur-snippets-alist)))))
          (helm-yas-find-file-snippet-by-template template)
          (setq current-window
                (list
                 :file (file-relative-name buffer-file-name root)
                 :buffer (buffer-name)
                 :mode major-mode
                 :point (point)
                 :contents
                 (buffer-substring-no-properties (point-min) (point-max))))
          (switch-to-buffer origin)
          (helm-yas-find-file-snippet-by-template template t)
          (setq other-window
                (list
                 :selected (buffer-name)
                 :file (file-relative-name buffer-file-name root)
                 :windows (length (window-list nil 'nomini))
                 :origin-visible (and (get-buffer-window origin) t)))
          (switch-to-buffer origin)
          (let ((helm-yas-cur-snippets-alist
                 '((template-file-alist))))
            (setq missing
                  (list
                   :result
                   (helm-yas-find-file-snippet-by-template "missing-template")
                   :message (current-message)
                   :buffer (buffer-name))))))
    (neomacs-helm-c-yas-test-kill-file-buffer snippet-file)
    (when (buffer-live-p origin)
      (kill-buffer origin)))
  (list
   :visit-source
   (neomacs-helm-c-yas-test-session-summary visit-session)
   :current-window current-window
   :other-window other-window
   :missing missing))
"##;
    let expect = expect![[
        r##"OK (:visit-source (:source "yasnippet snippet files" :initial-input nil :replacement-span nil :selected-text nil :pattern "" :candidates ("open-plan" "open-plan") :matches ("open-plan") :selected nil :actions ("Find file" "Find file as root" "Find file other window" "Find file other frame" "Open dired in file's directory" "Attach file(s) to mail buffer `C-c C-a'" "Edit marked files `C-x C-q'" "Grep File(s) `C-u recurse'" "Zgrep File(s) `C-u Recurse'" "Pdfgrep File(s)" "Insert as org link" "Checksum File" "Ediff File" "Ediff Merge File" "View file" "Insert file" "Add marked files to file-cache" "Delete file(s)" "Copy file(s) `M-C, C-u to follow'" "Rename file(s) `M-R, C-u to follow'" "Symlink files(s) `M-S, C-u to follow'" "Relsymlink file(s) `C-u to follow'" "Hardlink file(s) `M-H, C-u to follow'" "Open file externally (C-u to choose)" "Open file with default tool" "Find file in hex dump") :action nil :action-invoked nil :action-result-truthy nil) :current-window (:file "emacs-lisp-mode/open-plan" :buffer "open-plan" :mode snippet-mode :point 1 :contents "# -*- mode: snippet -*-\n# name: Open deployment plan\n# key: plan\n# --\n(find-file \"DEPLOYMENT.md\")$0\n") :other-window (:selected "open-plan" :file "emacs-lisp-mode/open-plan" :windows 2 :origin-visible t) :missing (:result "not found snippet file" :message nil :buffer "helm-c-yas-origin"))"##
    ]];
    ParityBatchCase::value(
        "visiting_a_loaded_snippet_uses_its_real_file_in_current_and_other_windows",
        elisp_form,
        expect,
    )
}

fn authoring_failures_preserve_existing_files_regions_and_directory_layout() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-helm-c-yas-test-root "helm-c-yas-failures"))
       (mode-dir (expand-file-name "emacs-lisp-mode" root))
       (existing (expand-file-name "existing" mode-dir))
       (source (generate-new-buffer " *helm-c-yas-failure-source*"))
       decline-prompt declined declined-state existing-error)
  (make-directory root t)
  (setq yas-snippet-dirs (list root))
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer source)
        (emacs-lisp-mode)
        (insert "(deploy-selected-region)")
        (goto-char (point-min))
        (push-mark (point-max) t t)
        (cl-letf (((symbol-function 'yes-or-no-p)
                   (lambda (prompt)
                     (setq decline-prompt prompt)
                     nil)))
          (setq declined
                (neomacs-helm-c-yas-test-error
                 (lambda ()
                   (helm-yas-create-snippet-on-region
                    (region-beginning) (region-end) "declined")))))
        (setq declined-state
              (list
               :prompt decline-prompt
               :error declined
               :mark-active (and mark-active t)
               :deactivate-mark (and deactivate-mark t)
               :buffer (buffer-substring-no-properties (point-min) (point-max))
               :root-layout (directory-files root nil "^[^.]" nil)))
        (make-directory mode-dir t)
        (neomacs-helm-c-yas-test-write-file existing "KEEP-EXISTING\n")
        (setq existing-error
              (neomacs-helm-c-yas-test-error
               (lambda ()
                 (helm-yas-create-snippet-on-region
                  (point-min) (point-max) "existing"))))
        (list
         :declined
         declined-state
         :existing
         (list
          :error existing-error
          :bytes (neomacs-helm-c-yas-test-file-contents existing)
          :visited (and (get-file-buffer existing) t)
          :layout (directory-files mode-dir nil "^[^.]" nil))))
    (when (buffer-live-p source)
      (kill-buffer source))))
"##;
    let expect = expect![[
        r#"OK (:declined (:prompt "[ORACLE-SANDBOX]/helm-c-yas-failures/emacs-lisp-mode/ doesn't exist. Would you like to create this directory?" :error (error "Snippet creation failed") :mark-active t :deactivate-mark t :buffer "(deploy-selected-region)" :root-layout nil) :existing (:error (error "can’t create file [existing] already exists") :bytes "KEEP-EXISTING\n" :visited nil :layout ("existing")))"#
    ]];
    ParityBatchCase::value(
        "authoring_failures_preserve_existing_files_regions_and_directory_layout",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        completing_a_named_snippet_replaces_context_and_preserves_the_editing_session(),
        catalog_options_filter_conditions_duplicates_keys_and_multiword_patterns(),
        authoring_from_a_real_region_round_trips_through_a_saved_snippet(),
        rename_and_delete_actions_mutate_real_snippet_files_and_reload_the_catalog(),
        visiting_a_loaded_snippet_uses_its_real_file_in_current_and_other_windows(),
        authoring_failures_preserve_existing_files_regions_and_directory_layout(),
    ]
}
