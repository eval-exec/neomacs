use expect_test::expect;

use super::ParityBatchCase;

fn choosing_a_capf_candidate_from_the_popup_inserts_it_and_runs_exit_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "choosing_a_capf_candidate_from_the_popup_inserts_it_and_runs_exit_metadata",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (setq-local completion-at-point-functions
                '(neomacs-company-test-environment-capf))
    (setq-local company-backends '(company-capf))
    (setq-local company-frontends
                '(company-pseudo-tooltip-frontend
                  company-echo-metadata-frontend))
    (setq-local company-idle-delay nil)
    (setq-local completion-styles '(basic))
    (local-set-key (kbd "C-c c") #'company-complete)
    (company-mode 1)
    (insert "(setq deployment-environment 'pr")
    (execute-kbd-macro (kbd "C-c c"))
    (let ((opened
           (list
            :buffer (buffer-string)
            :prefix company-prefix
            :candidates (copy-sequence company-candidates)
            :selection company-selection
            :selected (nth company-selection company-candidates)
            :annotation (company-call-backend
                         'annotation
                         (nth company-selection company-candidates))
            :kind (company-call-backend
                   'kind
                   (nth company-selection company-candidates))
            :meta (company-call-backend
                   'meta
                   (nth company-selection company-candidates))
            :tooltip-visible (and (company-tooltip-visible-p) t))))
      (company-select-next)
      (let ((after-next
             (list :selection company-selection
                   :selected (nth company-selection company-candidates)
                   :tooltip-visible (and (company-tooltip-visible-p) t))))
        (company-select-previous)
        (let ((after-previous
               (list :selection company-selection
                     :selected (nth company-selection company-candidates))))
          (company-select-next)
          (company-complete-selection)
          (list
           :opened opened
           :after-next after-next
           :after-previous after-previous
           :final
           (list
            :buffer (buffer-string)
            :point (point)
            :events (nreverse neomacs-company-test-events)
            :active (and company-candidates t)
            :tooltip-visible (and (company-tooltip-visible-p) t))))))))
"##,
        expect![[
            r##"OK (:opened (:buffer "(setq deployment-environment 'pr" :prefix "pr" :candidates (#("preproduction" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) #("preview" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) #("production" 0 2 (face completions-common-part) 2 3 (face completions-first-difference))) :selection 0 :selected #("preproduction" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) :annotation #("  environment:pre" 14 16 (face completions-common-part) 16 17 (face completions-first-difference)) :kind constant :meta #("Deploy using the preproduction environment" 17 19 (face completions-common-part) 19 20 (face completions-first-difference)) :tooltip-visible t) :after-next (:selection 1 :selected #("preview" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) :tooltip-visible t) :after-previous (:selection 0 :selected #("preproduction" 0 2 (face completions-common-part) 2 3 (face completions-first-difference))) :final (:buffer "(setq deployment-environment 'preview" :point 38 :events ((:completed #("preview" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) :status finished)) :active nil :tooltip-visible nil))"##
        ]],
    )
}

fn typing_filters_a_live_session_and_an_impossible_prefix_cancels_cleanly() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_filters_a_live_session_and_an_impossible_prefix_cancels_cleanly",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (setq-local completion-at-point-functions
                '(neomacs-company-test-environment-capf))
    (setq-local company-backends '(company-capf))
    (setq-local company-frontends '(company-pseudo-tooltip-frontend))
    (setq-local company-idle-delay nil)
    (setq-local completion-styles '(basic))
    (add-hook 'company-completion-started-hook
              (lambda (explicit)
                (push (list :started explicit)
                      neomacs-company-test-events)) nil t)
    (add-hook 'company-completion-cancelled-hook
              (lambda (reason)
                (push (list :cancelled reason)
                      neomacs-company-test-events)) nil t)
    (local-set-key (kbd "C-c c") #'company-complete)
    (company-mode 1)
    (insert "(deploy-to 'p")
    (execute-kbd-macro (kbd "C-c c"))
    (let ((opened
           (list :buffer (buffer-string)
                 :prefix company-prefix
                 :candidates (neomacs-company-test-plain-candidates)
                 :tooltip-visible (and (company-tooltip-visible-p) t))))
      ;; The first explicit completion has already expanded the common
      ;; `p' prefix to `pr'.  Type one more character to narrow the live
      ;; session to the two preview environments, then explicitly ask for
      ;; the updated candidates as a user would after ordinary typing ends
      ;; the manual session.
      (execute-kbd-macro "e")
      (execute-kbd-macro (kbd "C-c c"))
      (let ((narrowed
             (list :buffer (buffer-string)
                   :prefix company-prefix
                   :candidates (neomacs-company-test-plain-candidates)
                   :selected (nth company-selection company-candidates)
                   :tooltip-visible (and (company-tooltip-visible-p) t))))
        (execute-kbd-macro "x")
        (list
         :opened opened
         :narrowed narrowed
         :cancelled
         (list :buffer (buffer-string)
               :point (point)
               :active (and company-candidates t)
               :prefix company-prefix
               :tooltip-visible (and (company-tooltip-visible-p) t)
               :events (nreverse neomacs-company-test-events)))))))
"##,
        expect![[
            r##"OK (:opened (:buffer "(deploy-to 'pr" :prefix "pr" :candidates ("preproduction" "preview" "production") :tooltip-visible t) :narrowed (:buffer "(deploy-to 'pre" :prefix "pre" :candidates ("preproduction" "preview") :selected #("preproduction" 0 3 (face completions-common-part) 3 4 (face completions-first-difference)) :tooltip-visible t) :cancelled (:buffer "(deploy-to 'prex" :point 17 :active nil :prefix nil :tooltip-visible nil :events ((:started t) (:cancelled abort) (:started t) (:cancelled abort))))"##
        ]],
    )
}

fn an_async_workspace_backend_finishes_before_the_user_selects_a_remote_action() -> ParityBatchCase
{
    ParityBatchCase::value(
        "an_async_workspace_backend_finishes_before_the_user_selects_a_remote_action",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (setq-local company-backends '(neomacs-company-test-remote-backend))
    (setq-local company-frontends '(company-pseudo-tooltip-frontend))
    (setq-local company-idle-delay nil)
    (local-set-key (kbd "C-c c") #'company-complete)
    (company-mode 1)
    (insert "(workspace-command repo")
    (execute-kbd-macro (kbd "C-c c"))
    (let ((loaded
           (list
            :prefix company-prefix
            :candidates (neomacs-company-test-plain-candidates)
            :selection company-selection
            :annotation (company-call-backend
                         'annotation
                         (nth company-selection company-candidates))
            :kind (company-call-backend
                   'kind
                   (nth company-selection company-candidates))
            :meta (company-call-backend
                   'meta
                   (nth company-selection company-candidates))
            :tooltip-visible (and (company-tooltip-visible-p) t))))
      (company-select-next 2)
      (let ((chosen (nth company-selection company-candidates)))
        (company-complete-selection)
        (list
         :loaded loaded
         :chosen chosen
         :final-buffer (buffer-string)
         :events (nreverse neomacs-company-test-events)
         :active (and company-candidates t)
         :tooltip-visible (and (company-tooltip-visible-p) t))))))
"##,
        expect![[
            r##"OK (:loaded (:prefix "repository-" :candidates ("repository-clone" "repository-find" "repository-open") :selection 0 :annotation "  remote index" :kind function :meta "Workspace action: repository-clone" :tooltip-visible t) :chosen "repository-open" :final-buffer "(workspace-command repository-open" :events ((:remote-completed "repository-open")) :active nil :tooltip-visible nil)"##
        ]],
    )
}

fn completing_a_project_file_descends_into_a_directory_then_selects_the_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_a_project_file_descends_into_a_directory_then_selects_the_file",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "company-file-project"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root))
  (neomacs-company-test-write-file
   (expand-file-name "config/preview.toml" root)
   "endpoint = \"preview.example.test\"\n")
  (neomacs-company-test-write-file
   (expand-file-name "config/production.toml" root)
   "endpoint = \"api.example.test\"\n")
  (neomacs-company-test-write-file
   (expand-file-name "config/private/secrets.toml" root)
   "token = \"fixture\"\n")
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (emacs-lisp-mode)
      (setq default-directory root)
      (setq-local company-backends '(company-files))
      (setq-local company-frontends '(company-pseudo-tooltip-frontend))
      (setq-local company-idle-delay nil)
      (local-set-key (kbd "C-c c") #'company-complete)
      (company-mode 1)
      (insert "(load-file \"./con")
      (execute-kbd-macro (kbd "C-c c"))
      ;; `company-complete' expands the sole `config/' directory and
      ;; immediately restarts completion in that new file-name field.
      (let ((after-directory
             (list :buffer (buffer-string)
                   :prefix company-prefix
                   :candidates (neomacs-company-test-plain-candidates)
                   :active (and company-candidates t))))
        (execute-kbd-macro "pr")
        ;; File-name completion ends the session after the directory
        ;; expansion.  A user explicitly invokes it again after typing the
        ;; next path component.
        (execute-kbd-macro (kbd "C-c c"))
        (let ((filtered
               (list :buffer (buffer-string)
                     :prefix company-prefix
                     :candidates (neomacs-company-test-plain-candidates)
                     :kinds (mapcar
                             (lambda (candidate)
                               (company-call-backend 'kind candidate))
                             company-candidates))))
          (company-select-next 2)
          (let ((chosen (nth company-selection company-candidates)))
            (company-complete-selection)
            (list
             :after-directory after-directory
             :filtered filtered
             :chosen chosen
             :final-buffer (buffer-string)
             :active (and company-candidates t)
             :exists (file-exists-p
                      (expand-file-name "config/production.toml"
                                        root)))))))))
"##,
        expect![[
            r##"OK (:after-directory (:buffer "(load-file \"./config/" :prefix "./config/" :candidates ("preview.toml" "private/" "production.toml") :active t) :filtered (:buffer "(load-file \"./config/pr" :prefix "./config/pr" :candidates ("preview.toml" "private/" "production.toml") :kinds (file folder file)) :chosen "production.toml" :final-buffer "(load-file \"./config/production.toml" :active nil :exists t)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        choosing_a_capf_candidate_from_the_popup_inserts_it_and_runs_exit_metadata(),
        typing_filters_a_live_session_and_an_impossible_prefix_cancels_cleanly(),
        an_async_workspace_backend_finishes_before_the_user_selects_a_remote_action(),
        completing_a_project_file_descends_into_a_directory_then_selects_the_file(),
    ]
}
